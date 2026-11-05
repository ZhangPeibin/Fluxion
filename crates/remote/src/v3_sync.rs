//! Background-only ABI/state loading. Math in v3_local has no RPC dependency.
use crate::{
    local_state::StateError,
    model::{Pool, PoolKind},
    rpc::RpcClient,
    state::multicall,
    v3_local::{MAX_TICK, MIN_TICK, V3State, V3Structure, initialized_bits},
};
use alloy_primitives::{Address, B256, U256, keccak256};
use futures::{StreamExt, TryStreamExt, stream};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

const MINT_SIGNATURE: &[u8] = b"Mint(address,address,int24,int24,uint128,uint256,uint256)";
const BURN_SIGNATURE: &[u8] = b"Burn(address,int24,int24,uint128,uint256,uint256)";
const INITIALIZE_SIGNATURE: &[u8] = b"Initialize(uint160,int24)";

fn mint_topic() -> B256 {
    static TOPIC: OnceLock<B256> = OnceLock::new();
    *TOPIC.get_or_init(|| keccak256(MINT_SIGNATURE))
}

fn burn_topic() -> B256 {
    static TOPIC: OnceLock<B256> = OnceLock::new();
    *TOPIC.get_or_init(|| keccak256(BURN_SIGNATURE))
}

fn initialize_topic() -> B256 {
    static TOPIC: OnceLock<B256> = OnceLock::new();
    *TOPIC.get_or_init(|| keccak256(INITIALIZE_SIGNATURE))
}

/// Structural V3 pool events. Swaps are not included on purpose: the current
/// price and active liquidity are re-read from `slot0`/`liquidity` each block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V3Event {
    Mint { tick_lower: i32, tick_upper: i32 },
    Burn { tick_lower: i32, tick_upper: i32 },
    Initialize { tick: i32 },
}

/// Decode only the canonical, non-removed events this project can maintain.
pub fn decode_event(log: &crate::rpc::RpcLog) -> Option<V3Event> {
    if log.removed {
        return None;
    }
    let topic = *log.topics.first()?;
    if topic == mint_topic() || topic == burn_topic() {
        let tick_lower = int24(log.topics.get(2)?.as_slice()).ok()?;
        let tick_upper = int24(log.topics.get(3)?.as_slice()).ok()?;
        return Some(if topic == mint_topic() {
            V3Event::Mint {
                tick_lower,
                tick_upper,
            }
        } else {
            V3Event::Burn {
                tick_lower,
                tick_upper,
            }
        });
    }
    if topic == initialize_topic() {
        return Some(V3Event::Initialize {
            tick: int24(log.data.get(32..64)?).ok()?,
        });
    }
    None
}

/// Topics used to poll structural changes for a set of V3 pools.
pub fn structural_topics() -> [B256; 3] {
    [mint_topic(), burn_topic(), initialize_topic()]
}

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
    let phase = Instant::now();
    tracing::debug!(phase = "headers_rpc", "diag_v3_begin");
    let values = read(rpc, &calls, batch).await?;
    tracing::debug!(
        phase = "headers_rpc",
        elapsed_ms = phase.elapsed().as_millis(),
        "diag_v3_end"
    );
    let phase = Instant::now();
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
    tracing::debug!(
        phase = "headers_decode",
        elapsed_ms = phase.elapsed().as_millis(),
        "diag_v3_end"
    );
    let phase = Instant::now();
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
    tracing::debug!(
        phase = "bitmap_calls_build",
        calls = calls.len(),
        elapsed_ms = phase.elapsed().as_millis(),
        "diag_v3_end"
    );
    let phase = Instant::now();
    tracing::debug!(phase = "bitmap_rpc", "diag_v3_begin");
    let values = read(rpc, &calls, batch).await?;
    tracing::debug!(
        phase = "bitmap_rpc",
        elapsed_ms = phase.elapsed().as_millis(),
        "diag_v3_end"
    );
    let phase = Instant::now();
    for ((index, word), value) in positions.into_iter().zip(values) {
        headers[index].bitmaps.insert(word, unsigned(&value)?);
    }
    tracing::debug!(
        phase = "bitmap_decode",
        elapsed_ms = phase.elapsed().as_millis(),
        "diag_v3_end"
    );
    let phase = Instant::now();
    tracing::debug!(phase = "expand_tick_bits", "diag_v3_begin");
    let mut ticks = Vec::new();
    let mut calls = Vec::new();
    for (index, (pool, h)) in pools.iter().zip(&headers).enumerate() {
        for (&word, &bitmap) in &h.bitmaps {
            for bit in initialized_bits(bitmap) {
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
    tracing::debug!(
        phase = "expand_tick_bits",
        ticks = ticks.len(),
        elapsed_ms = phase.elapsed().as_millis(),
        "diag_v3_end"
    );
    let phase = Instant::now();
    tracing::debug!(phase = "ticks_rpc", "diag_v3_begin");
    let values = read(rpc, &calls, batch).await?;
    tracing::debug!(
        phase = "ticks_rpc",
        elapsed_ms = phase.elapsed().as_millis(),
        "diag_v3_end"
    );
    let phase = Instant::now();
    for ((index, tick), data) in ticks.into_iter().zip(values) {
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
    tracing::debug!(
        phase = "ticks_decode",
        elapsed_ms = phase.elapsed().as_millis(),
        "diag_v3_end"
    );
    let phase = Instant::now();
    tracing::debug!(phase = "validate_states", "diag_v3_begin");
    let states = pools
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
        .collect();
    tracing::debug!(
        phase = "validate_states",
        elapsed_ms = phase.elapsed().as_millis(),
        "diag_v3_end"
    );
    states
}

/// Re-read the fields that change every block for all V3 pools. The immutable
/// structure (bitmaps and initialized ticks) is carried over by `Arc`.
pub async fn refresh_market(
    pools: &[(&Pool, Arc<V3Structure>)],
    rpc: &RpcClient,
    batch: usize,
) -> Result<HashMap<Address, V3State>, StateError> {
    let mut calls = Vec::with_capacity(pools.len() * 2);
    for (pool, _) in pools {
        calls.push((pool.address, "3850c7bd".to_string()));
        calls.push((pool.address, "1a686502".to_string()));
    }
    let values = read(rpc, &calls, batch).await?;
    let mut states = HashMap::with_capacity(pools.len());
    for ((pool, structure), words) in pools.iter().zip(values.as_chunks::<2>().0) {
        let slot0 = &words[0];
        let liquidity = unsigned(&words[1])?;
        if slot0.len() != 224 || liquidity.bit_len() > 128 {
            return Err(StateError::Incomplete);
        }
        let sqrt = unsigned(&slot0[..32])?;
        let tick = int24(&slot0[32..64])?;
        if sqrt.bit_len() > 160 {
            return Err(StateError::Incomplete);
        }
        let state =
            V3State::with_shared_structure(sqrt, tick, liquidity.to::<u128>(), structure.clone())
                .map_err(|_| StateError::Incomplete)?;
        states.insert(pool.address, state);
    }
    Ok(states)
}

/// Read the current `liquidityGross`/`liquidityNet` for ticks touched by
/// events. `None` means the tick is no longer initialized.
pub async fn read_tick_updates(
    requests: &[(Address, Vec<i32>)],
    rpc: &RpcClient,
    batch: usize,
) -> Result<HashMap<Address, Vec<(i32, Option<i128>)>>, StateError> {
    let mut calls = Vec::new();
    let mut layout = Vec::new();
    for (pool, ticks) in requests {
        let mut unique = ticks.clone();
        unique.sort_unstable();
        unique.dedup();
        let start = calls.len();
        for tick in &unique {
            calls.push((*pool, format!("f30dba93{}", signed_word(*tick))));
        }
        layout.push((*pool, unique, start, calls.len()));
    }
    let values = read(rpc, &calls, batch).await?;
    let mut updates = HashMap::new();
    for (pool, ticks, start, end) in layout {
        let mut entries = Vec::with_capacity(ticks.len());
        for (tick, data) in ticks.into_iter().zip(&values[start..end]) {
            if data.len() != 256 {
                return Err(StateError::Incomplete);
            }
            let gross = unsigned(&data[..32])?;
            let net = i128::from_be_bytes(data[48..64].try_into().unwrap());
            let sign = if net < 0 { 255 } else { 0 };
            let initialized = unsigned(&data[224..])?;
            if gross.bit_len() > 128
                || net.unsigned_abs() > gross.to::<u128>()
                || data[32..48].iter().any(|b| *b != sign)
                || initialized != U256::from(u8::from(!gross.is_zero()))
            {
                return Err(StateError::Incomplete);
            }
            entries.push((tick, (!gross.is_zero()).then_some(net)));
        }
        updates.insert(pool, entries);
    }
    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::RpcLog;

    fn signed_word_bytes(value: i32) -> B256 {
        let mut bytes = [if value < 0 { 0xff } else { 0 }; 32];
        bytes[28..32].copy_from_slice(&value.to_be_bytes());
        B256::from(bytes)
    }

    fn log(topics: Vec<B256>, data: Vec<u8>) -> RpcLog {
        RpcLog {
            address: Address::with_last_byte(9),
            topics,
            data,
            block_number: 1,
            removed: false,
        }
    }

    #[test]
    fn decodes_mint_burn_and_initialize_ticks() {
        let [mint, burn, initialize] = structural_topics();
        let minted = log(
            vec![
                mint,
                B256::ZERO,
                signed_word_bytes(-120),
                signed_word_bytes(240),
            ],
            Vec::new(),
        );
        assert_eq!(
            decode_event(&minted),
            Some(V3Event::Mint {
                tick_lower: -120,
                tick_upper: 240
            })
        );
        let burned = log(
            vec![
                burn,
                B256::ZERO,
                signed_word_bytes(-60),
                signed_word_bytes(60),
            ],
            Vec::new(),
        );
        assert_eq!(
            decode_event(&burned),
            Some(V3Event::Burn {
                tick_lower: -60,
                tick_upper: 60
            })
        );
        let mut data = vec![0u8; 64];
        data[32..64].copy_from_slice(signed_word_bytes(-1).as_slice());
        assert_eq!(
            decode_event(&log(vec![initialize], data)),
            Some(V3Event::Initialize { tick: -1 })
        );
    }

    #[test]
    fn ignores_removed_and_unrelated_logs() {
        let [mint, _, _] = structural_topics();
        let mut removed = log(
            vec![
                mint,
                B256::ZERO,
                signed_word_bytes(0),
                signed_word_bytes(60),
            ],
            Vec::new(),
        );
        removed.removed = true;
        assert_eq!(decode_event(&removed), None);
        assert_eq!(decode_event(&log(vec![B256::ZERO], Vec::new())), None);
    }
}
