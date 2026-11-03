//! Loading is deployment-profile based, not get_dy-signature based.
use crate::{
    curve_local::{self, CurveState},
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
) -> Result<HashMap<Address, CurveState>, StateError> {
    let mut calls = Vec::new();
    let mut layouts = Vec::new();
    for pool in pools {
        let profile = curve_local::profile_for(pool).ok_or(StateError::Unsupported)?;
        let count = pool.coins.len();
        layouts.push((calls.len(), count, profile));
        for signature in ["coins(uint256)", "balances(uint256)"] {
            for i in 0..count {
                calls.push((
                    pool.address,
                    format!("{}{}", selector(signature), u256_word(U256::from(i))),
                ));
            }
        }
        for signature in [profile.a_getter(), "fee()"] {
            calls.push((pool.address, selector(signature)));
        }
    }
    let values = read(rpc, &calls, batch).await?;
    let mut states = Vec::new();
    let mut probes = Vec::new();
    let mut expected = Vec::new();
    for (pool, (offset, count, profile)) in pools.iter().zip(layouts) {
        let words = &values[offset..offset + count * 2 + 2];
        for (word, coin) in words.iter().take(count).zip(&pool.coins) {
            if *word != U256::from_be_slice(coin.as_slice()) {
                return Err(StateError::Incomplete);
            }
        }
        let state = profile
            .build(
                &words[count..count * 2],
                words[count * 2],
                words[count * 2 + 1],
            )
            .map_err(|_| StateError::Incomplete)?;
        if state.is_liquid() {
            for (i, unit) in profile.units().into_iter().enumerate() {
                for j in 0..count {
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
                        expected.push(state.amount_out(i, j, amount));
                    }
                }
            }
        }
        states.push(state);
    }
    for (calls, expected) in probes.chunks(batch).zip(expected.chunks(batch)) {
        let results = multicall(rpc, calls)
            .await
            .map_err(|_| StateError::SyncFailed)?;
        for ((ok, data), expected) in results.into_iter().zip(expected) {
            match expected {
                Ok(value) if ok && data.len() == 32 && U256::from_be_slice(&data) == *value => {}
                Err(_) if !ok => {} // a modeled view revert is not missing metadata
                _ => return Err(StateError::Incomplete),
            }
        }
    }
    Ok(pools
        .iter()
        .zip(states)
        .map(|(p, s)| (p.address, s))
        .collect())
}
