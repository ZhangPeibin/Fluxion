//! Loading is deployment-profile based, not get_dy-signature based.
use crate::{
    curve_local::{self, Classic3Pool},
    local_state::StateError,
    model::Pool,
    rpc::RpcClient,
    state::multicall,
    util::{selector, u256_word},
};
use alloy_primitives::{Address, U256};
use std::collections::HashMap;

async fn read(
    rpc: &RpcClient,
    calls: &[(Address, String)],
    batch: usize,
) -> Result<Vec<U256>, StateError> {
    let mut result = Vec::with_capacity(calls.len());
    for chunk in calls.chunks(batch) {
        for (ok, data) in multicall(rpc, chunk)
            .await
            .map_err(|_| StateError::SyncFailed)?
        {
            if !ok || data.len() != 32 {
                return Err(StateError::Incomplete);
            }
            result.push(U256::from_be_slice(&data));
        }
    }
    Ok(result)
}

pub async fn load(
    pools: &[&Pool],
    rpc: &RpcClient,
    batch: usize,
) -> Result<HashMap<Address, Classic3Pool>, StateError> {
    let mut calls = Vec::new();
    for pool in pools {
        if !curve_local::supported(pool) {
            return Err(StateError::Unsupported);
        }
        for signature in ["coins(uint256)", "balances(uint256)"] {
            for i in 0..3 {
                calls.push((
                    pool.address,
                    format!("{}{}", selector(signature), u256_word(U256::from(i))),
                ));
            }
        }
        for signature in ["A()", "fee()"] {
            calls.push((pool.address, selector(signature)));
        }
    }
    let values = read(rpc, &calls, batch).await?;
    let mut states = Vec::new();
    let mut probes = Vec::new();
    let mut expected = Vec::new();
    for (pool, words) in pools.iter().zip(values.as_chunks::<8>().0) {
        for (word, coin) in words.iter().take(3).zip(&pool.coins) {
            if *word != U256::from_be_slice(coin.as_slice()) {
                return Err(StateError::Incomplete);
            }
        }
        let state = Classic3Pool::new([words[3], words[4], words[5]], words[6], words[7])
            .map_err(|_| StateError::Incomplete)?;
        if state.is_liquid() {
            for i in 0..3 {
                let unit = if i == 0 {
                    U256::from(10u128.pow(18))
                } else {
                    U256::from(1_000_000)
                };
                for j in 0..3 {
                    if i == j {
                        continue;
                    }
                    for amount in [U256::from(1_000_000), unit + U256::from(1)] {
                        probes.push((
                            pool.address,
                            format!(
                                "{}{}{}{}",
                                selector("get_dy(int128,int128,uint256)"),
                                u256_word(U256::from(i)),
                                u256_word(U256::from(j)),
                                u256_word(amount)
                            ),
                        ));
                        expected.push(
                            state
                                .amount_out(i, j, amount)
                                .map_err(|_| StateError::Incomplete)?,
                        );
                    }
                }
            }
        }
        states.push(state);
    }
    if read(rpc, &probes, batch).await? != expected {
        return Err(StateError::Incomplete);
    }
    Ok(pools
        .iter()
        .zip(states)
        .map(|(p, s)| (p.address, s))
        .collect())
}
