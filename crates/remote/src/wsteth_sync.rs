//! Background-only loading and validation of the supported wrapper model.
use crate::{
    local_state::StateError,
    model::Pool,
    rpc::RpcClient,
    state::multicall,
    util::{selector, u256_word},
    wsteth_local::{SUPPORTED_VERSION, WstEthState},
};
use alloy_primitives::{Address, U256};
use std::collections::HashMap;

async fn read(
    rpc: &RpcClient,
    calls: &[(Address, String)],
    batch: usize,
) -> Result<Vec<U256>, StateError> {
    let mut words = Vec::with_capacity(calls.len());
    for chunk in calls.chunks(batch) {
        for (ok, bytes) in multicall(rpc, chunk)
            .await
            .map_err(|_| StateError::SyncFailed)?
        {
            if !ok || bytes.len() != 32 {
                return Err(StateError::Incomplete);
            }
            words.push(U256::from_be_slice(&bytes));
        }
    }
    Ok(words)
}

pub async fn load(
    pools: &[&Pool],
    rpc: &RpcClient,
    batch: usize,
) -> Result<HashMap<Address, WstEthState>, StateError> {
    let calls = pools
        .iter()
        .flat_map(|p| {
            [
                (p.address, selector("stETH()")),
                (p.coins[0], selector("getContractVersion()")),
            ]
        })
        .collect::<Vec<_>>();
    let identity = read(rpc, &calls, batch).await?;
    for (pool, words) in pools.iter().zip(identity.as_chunks::<2>().0) {
        if words[0] != U256::from_be_slice(pool.coins[0].as_slice()) {
            return Err(StateError::Incomplete);
        }
        if words[1] != U256::from(SUPPORTED_VERSION) {
            return Err(StateError::Unsupported);
        }
    }
    let calls = pools
        .iter()
        .flat_map(|p| {
            [
                "getTotalPooledEther()",
                "getTotalShares()",
                "getExternalEther()",
                "getExternalShares()",
            ]
            .map(|signature| (p.coins[0], selector(signature)))
        })
        .collect::<Vec<_>>();
    let totals = read(rpc, &calls, batch).await?;
    let mut states = Vec::new();
    let mut probes = Vec::new();
    let mut expected = Vec::new();
    let unit = U256::from(10u64).pow(U256::from(18));
    for (pool, words) in pools.iter().zip(totals.as_chunks::<4>().0) {
        let state = WstEthState::new(SUPPORTED_VERSION, words[0], words[1], words[2], words[3])
            .map_err(|_| StateError::Incomplete)?;
        let maximum = U256::from(u128::MAX - 1);
        // Effective numerator/denominator probes detect rounded total-rate
        // substitution that ordinary one-token samples can easily miss.
        for (wrap, amount) in [
            (true, unit),
            (false, unit),
            (true, state.internal_ether().min(maximum)),
            (false, state.internal_shares().min(maximum)),
        ] {
            let signature = if wrap {
                "getWstETHByStETH(uint256)"
            } else {
                "getStETHByWstETH(uint256)"
            };
            probes.push((
                pool.address,
                format!("{}{}", selector(signature), u256_word(amount)),
            ));
            expected.push(
                state
                    .amount_out(amount, wrap)
                    .map_err(|_| StateError::Incomplete)?,
            );
        }
        states.push(state);
    }
    if read(rpc, &probes, batch).await? != expected {
        return Err(StateError::Incomplete);
    }
    Ok(pools
        .iter()
        .zip(states)
        .map(|(pool, state)| (pool.address, state))
        .collect())
}
