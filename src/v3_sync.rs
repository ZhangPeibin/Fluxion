//! Background-only ABI/state loading. Math in v3_local has no RPC dependency.
use crate::{
    local_state::StateError,
    model::{Pool, PoolKind},
    rpc::RpcClient,
    state::multicall,
    v3_local::{MAX_TICK, MIN_TICK, V3State},
};
use alloy_primitives::{Address, U256};
use futures::{StreamExt, TryStreamExt, stream};
use std::collections::{BTreeMap, HashMap};

fn unsigned(data: &[u8]) -> Result<U256, StateError> {
    if data.len() != 32 {
        return Err(StateError::Incomplete);
    }
    Ok(U256::from_be_slice(data))
}

fn int24(data: &[u8]) -> Result<i32, StateError> {
    if data.len() != 32 {
        return Err(StateError::Incomplete);
    }
    let raw = (i32::from(data[29]) << 16) | (i32::from(data[30]) << 8) | i32::from(data[31]);
    let negative = raw & 0x800000 != 0;
    if data[..29]
        .iter()
        .any(|b| *b != if negative { 255 } else { 0 })
    {
        return Err(StateError::Incomplete);
    }
    Ok(if negative { raw | !0xffffff } else { raw })
}

fn signed_word(n: i32) -> String {
    let value = if n >= 0 {
        U256::from(n as u32)
    } else {
        U256::MAX - U256::from(n.unsigned_abs() - 1)
    };
    format!("{value:064x}")
}

async fn read(
    rpc: &RpcClient,
    calls: &[(Address, String)],
    batch: usize,
) -> Result<Vec<Vec<u8>>, StateError> {
    let mut values = Vec::with_capacity(calls.len());
    // All batches use the same pinned client. Bound network fan-out while
    // preserving call order even when responses arrive out of order.
    let mut requests = Vec::new();
    for chunk in calls.chunks(batch) {
        let chunk = chunk.to_vec();
        let rpc = rpc.clone();
        requests.push(async move { multicall(&rpc, &chunk).await });
    }
    let batches = stream::iter(requests)
        .buffered(8)
        .try_collect::<Vec<_>>()
        .await
        .map_err(|_| StateError::SyncFailed)?;
    for results in batches {
        for (ok, data) in results {
            if !ok {
                return Err(StateError::Incomplete);
            }
            values.push(data);
        }
    }
    Ok(values)
}

struct Header {
    sqrt: U256,
    tick: i32,
    liquidity: u128,
    fee: u32,
    spacing: i32,
    bitmaps: BTreeMap<i16, U256>,
    ticks: BTreeMap<i32, i128>,
}

pub async fn load(
    pools: &[&Pool],
    rpc: &RpcClient,
    batch: usize,
    radius: i16,
    max_ticks: usize,
    full_range: bool,
) -> Result<HashMap<Address, V3State>, StateError> {
    let selectors = [
        "3850c7bd", "1a686502", "d0c93a7c", "ddca3f43", "0dfe1681", "d21220a7", "c45a0155",
    ];
    let calls = pools
        .iter()
        .flat_map(|p| selectors.iter().map(move |s| (p.address, s.to_string())))
        .collect::<Vec<_>>();
    let values = read(rpc, &calls, batch).await?;
    let factory: Address = "0x1f98431c8ad98523631ae4a59f267346ea31f984"
        .parse()
        .unwrap();
    let mut headers = Vec::new();
    for (pool, words) in pools.iter().zip(values.as_chunks::<7>().0) {
        if words[0].len() != 224 {
            return Err(StateError::Incomplete);
        }
        let sqrt = unsigned(&words[0][..32])?;
        let tick = int24(&words[0][32..64])?;
        let liquidity = unsigned(&words[1])?;
        let spacing = int24(&words[2])?;
        let fee = unsigned(&words[3])?;
        if liquidity.bit_len() > 128
            || sqrt.bit_len() > 160
            || fee.bit_len() > 24
            || spacing <= 0
            || spacing > 32767
            || pool.kind
                != (PoolKind::UniswapV3 {
                    fee: fee.to::<u32>(),
                })
        {
            return Err(StateError::Incomplete);
        }
        for (idx, expected) in [(4, pool.coins[0]), (5, pool.coins[1]), (6, factory)] {
            let actual = unsigned(&words[idx])?;
            if actual.bit_len() > 160 || Address::from_slice(&words[idx][12..]) != expected {
                return Err(StateError::Incomplete);
            }
        }
        if !(MIN_TICK..MAX_TICK).contains(&tick) {
            return Err(StateError::Incomplete);
        }
        headers.push(Header {
            sqrt,
            tick,
            liquidity: liquidity.to::<u128>(),
            fee: fee.to::<u32>(),
            spacing,
            bitmaps: BTreeMap::new(),
            ticks: BTreeMap::new(),
        });
    }
    let mut positions = Vec::new();
    let mut calls = Vec::new();
    for (index, (pool, h)) in pools.iter().zip(&headers).enumerate() {
        let center = h.tick.div_euclid(h.spacing).div_euclid(256);
        let minimum = MIN_TICK.div_euclid(h.spacing).div_euclid(256);
        let maximum = MAX_TICK.div_euclid(h.spacing).div_euclid(256);
        let (low, high) = if full_range {
            (minimum, maximum)
        } else {
            (
                (center - i32::from(radius)).max(minimum),
                (center + i32::from(radius)).min(maximum),
            )
        };
        for word in low..=high {
            positions.push((index, word as i16));
            calls.push((pool.address, format!("5339c296{}", signed_word(word))));
        }
    }
    for ((index, word), value) in positions.into_iter().zip(read(rpc, &calls, batch).await?) {
        headers[index].bitmaps.insert(word, unsigned(&value)?);
    }
    let mut ticks = Vec::new();
    let mut calls = Vec::new();
    for (index, (pool, h)) in pools.iter().zip(&headers).enumerate() {
        for (&word, &bitmap) in &h.bitmaps {
            for bit in 0..256 {
                if (bitmap >> bit) & U256::from(1) == U256::ZERO {
                    continue;
                }
                let tick = (i32::from(word) * 256 + bit) * h.spacing;
                if !(MIN_TICK..=MAX_TICK).contains(&tick) {
                    return Err(StateError::Incomplete);
                }
                if ticks.len() >= max_ticks {
                    return Err(StateError::Incomplete);
                }
                ticks.push((index, tick));
                calls.push((pool.address, format!("f30dba93{}", signed_word(tick))));
            }
        }
    }
    for ((index, tick), data) in ticks.into_iter().zip(read(rpc, &calls, batch).await?) {
        if data.len() != 256 {
            return Err(StateError::Incomplete);
        }
        let gross = unsigned(&data[..32])?;
        let net = i128::from_be_bytes(data[48..64].try_into().unwrap());
        let sign = if net < 0 { 255 } else { 0 };
        if gross.is_zero()
            || gross.bit_len() > 128
            || net.unsigned_abs() > gross.to::<u128>()
            || data[32..48].iter().any(|b| *b != sign)
            || unsigned(&data[224..])? != U256::from(1)
        {
            return Err(StateError::Incomplete);
        }
        headers[index].ticks.insert(tick, net);
    }
    pools
        .iter()
        .zip(headers)
        .map(|(pool, h)| {
            V3State::new(
                h.sqrt,
                h.tick,
                h.liquidity,
                h.fee,
                h.spacing,
                h.bitmaps,
                h.ticks,
            )
            .map(|state| (pool.address, state))
            .map_err(|_| StateError::Incomplete)
        })
        .collect()
}
