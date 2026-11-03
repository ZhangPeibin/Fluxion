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
        let start = calls.len();
        for signature in ["coins(uint256)", "balances(uint256)"] {
            for i in 0..3 {
                calls.push((
                    pool.address,
                    format!("{}{}", selector(signature), u256_word(U256::from(i))),
                ));
            }
        }
        for (signature, index) in profile.extra_fields() {
            let mut data = selector(signature);
            if let Some(index) = index {
                data += &u256_word(U256::from(index));
            }
            calls.push((pool.address, data));
        }
        layouts.push((start, calls.len(), profile));
    }
    let values = read(rpc, &calls, batch).await?;
    let mut states = Vec::new();
    let mut probes = Vec::new();
    let mut expected = Vec::new();
    for (pool, (start, end, profile)) in pools.iter().zip(layouts) {
        let words = &values[start..end];
        for (word, coin) in words.iter().take(3).zip(&pool.coins) {
            if *word != U256::from_be_slice(coin.as_slice()) {
                return Err(StateError::Incomplete);
            }
        }
        let state = profile
            .build(&words[3..])
            .map_err(|_| StateError::Incomplete)?;
        if state.is_liquid() {
            for (i, unit) in profile.units().into_iter().enumerate() {
                for j in 0..3 {
                    if i == j {
                        continue;
                    }
                    for amount in [U256::from(1_000_000), unit + U256::from(1)] {
                        probes.push((
                            pool.address,
                            format!(
                                "{}{}{}{}",
                                selector(profile.get_dy_signature()),
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
        for ((ok, data), expected) in multicall(rpc, calls)
            .await
            .map_err(|_| StateError::SyncFailed)?
            .into_iter()
            .zip(expected)
        {
            match expected {
                Ok(value) if ok && data.len() == 32 && U256::from_be_slice(&data) == *value => {}
                Err(_) if !ok => {}
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
