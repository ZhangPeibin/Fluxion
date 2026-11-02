use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use alloy_primitives::{Address, U256};
use async_trait::async_trait;

use crate::adapters::{V2Reserves, decode_reserves};
use crate::error::QuoteError;
use crate::rpc::RpcClient;
use crate::util::{pad_address, u32_word, u256_to_usize, u256_word};

const MULTICALL3: &str = "0xcA11bde05977b3631167028862bE2a173976CA11";
const AGGREGATE3: &str = "82ad56cb";
pub const GET_RESERVES: &str = "0902f1ac";
const BATCH_SIZE: usize = 400;
const HOT_WINDOW: Duration = Duration::from_secs(300);

#[derive(Clone)]
struct Cached {
    reserves: [U256; 2],
    at: Instant,
    last_used: Option<Instant>,
}

/// Caches V2 reserves with a TTL. Cold pools are lazy-loaded on first access;
/// the hot set is refreshed in a single Multicall3 batch.
pub struct StateCache {
    rpc: RpcClient,
    ttl: Duration,
    cache: Mutex<HashMap<Address, Cached>>,
}

impl StateCache {
    pub fn new(rpc: RpcClient, ttl: Duration) -> Self {
        Self {
            rpc,
            ttl,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Seeds reserves from an external batch (e.g. a Multicall3 prefetch).
    pub fn seed_reserves(&self, pool: Address, reserves: [U256; 2]) {
        self.cache.lock().unwrap().insert(
            pool,
            Cached {
                reserves,
                at: Instant::now(),
                last_used: None,
            },
        );
    }

    pub async fn refresh_hot(&self) -> Result<(), QuoteError> {
        let addresses: Vec<Address> = {
            let cache = self.cache.lock().unwrap();
            cache
                .iter()
                .filter_map(|(pool, c)| {
                    c.last_used
                        .filter(|at| at.elapsed() < HOT_WINDOW)
                        .map(|_| *pool)
                })
                .collect()
        };
        if addresses.is_empty() {
            return Ok(());
        }

        for chunk in addresses.chunks(BATCH_SIZE) {
            let calls = chunk
                .iter()
                .map(|pool| (*pool, GET_RESERVES.to_string()))
                .collect::<Vec<_>>();
            let results = multicall(&self.rpc, &calls).await?;
            let mut cache = self.cache.lock().unwrap();
            for (pool, (success, return_data)) in chunk.iter().zip(results) {
                if success
                    && let Ok(reserves) = decode_reserves(&return_data)
                    && let Some(entry) = cache.get_mut(pool)
                {
                    entry.reserves = reserves;
                    entry.at = Instant::now();
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl V2Reserves for StateCache {
    async fn get_reserves(&self, pool: Address) -> Result<[U256; 2], QuoteError> {
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(c) = cache.get_mut(&pool)
                && c.at.elapsed() < self.ttl
            {
                c.last_used = Some(Instant::now());
                return Ok(c.reserves);
            }
        }

        // Cold pool (or stale): lazy-load a single call, then mark hot.
        let data = self.rpc.eth_call(pool, GET_RESERVES).await?;
        let reserves = decode_reserves(&data)?;
        self.cache.lock().unwrap().insert(
            pool,
            Cached {
                reserves,
                at: Instant::now(),
                last_used: Some(Instant::now()),
            },
        );
        Ok(reserves)
    }
}

fn multicall3() -> Address {
    MULTICALL3.parse().unwrap()
}

/// Encode `aggregate3((address,bool,bytes)[])` for arbitrary calls. Each
/// failure is isolated (`allowFailure = true`) so one bad pool cannot revert a
/// whole prefetch batch.
fn encode_aggregate3(calls: &[(Address, String)]) -> String {
    let k = calls.len();
    let mut parts: Vec<String> = Vec::new();
    parts.push(AGGREGATE3.to_string());
    parts.push(u32_word(0x20)); // offset to the array
    parts.push(u256_word(U256::from(k))); // array length

    // Each tuple is (target, allowFailure, offsetToBytes) + (length + data)
    // words. Offsets are relative to the start of the elements area.
    let mut cursor = 0usize;
    for (_, data) in calls {
        parts.push(u256_word(U256::from(32 * (k + cursor))));
        let data_words = (data.len() / 2).div_ceil(32);
        cursor += 4 + data_words;
    }

    for (target, data) in calls {
        let bytes = data.len() / 2;
        parts.push(pad_address(*target));
        parts.push(u256_word(U256::from(1))); // allowFailure = true
        parts.push(u32_word(0x60)); // offset to callData = 96
        parts.push(u256_word(U256::from(bytes))); // callData length
        let mut padded = data.clone();
        let remainder = data.len() % 64;
        if remainder != 0 {
            padded.push_str(&"0".repeat(64 - remainder));
        }
        parts.push(padded);
    }

    parts.concat()
}

#[cfg(test)]
fn encode_aggregate3_get_reserves(pools: &[Address]) -> String {
    let calls: Vec<(Address, String)> = pools
        .iter()
        .map(|pool| (*pool, GET_RESERVES.to_string()))
        .collect();
    encode_aggregate3(&calls)
}

/// Batch `eth_call`s through Multicall3. Returns `(success, returnData)` per
/// call in input order.
pub async fn multicall(
    rpc: &RpcClient,
    calls: &[(Address, String)],
) -> Result<Vec<(bool, Vec<u8>)>, QuoteError> {
    if calls.is_empty() {
        return Ok(Vec::new());
    }
    let calldata = encode_aggregate3(calls);
    let data = rpc.eth_call(multicall3(), &calldata).await?;
    let results = decode_aggregate3(&data)?;
    if results.len() != calls.len() {
        return Err(QuoteError::RpcError(
            "aggregate3 result count mismatch".to_string(),
        ));
    }
    Ok(results)
}

/// Decode `aggregate3`'s `Result[]` return into `(success, returnData)` pairs.
///
/// `Result = (bool, bytes)` is dynamic, so the array is encoded as a length
/// followed by one offset per result; each result then has its own
/// `(success, offsetToBytes)` head. Offsets are relative to the start of their
/// containing tuple, not to the whole array.
fn decode_aggregate3(data: &[u8]) -> Result<Vec<(bool, Vec<u8>)>, QuoteError> {
    let short = || QuoteError::RpcError("short aggregate3 result".to_string());
    if data.len() < 64 {
        return Err(short());
    }
    let read = |start: usize| -> Result<usize, QuoteError> {
        let end = start.checked_add(32).ok_or_else(short)?;
        let word = data.get(start..end).ok_or_else(short)?;
        Ok(u256_to_usize(U256::from_be_slice(word)))
    };
    let offset = read(0)?;
    let n = read(offset)?;
    let heads_base = offset.checked_add(32).ok_or_else(short)?;
    if n > data.len().saturating_sub(heads_base) / 32 {
        return Err(short());
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let head = heads_base + i * 32;
        if head + 32 > data.len() {
            return Err(short());
        }
        let elem_rel = read(head)?;
        let base = heads_base.checked_add(elem_rel).ok_or_else(short)?;
        if base.checked_add(64).is_none_or(|end| end > data.len()) {
            return Err(short());
        }
        let success = U256::from_be_slice(&data[base..base + 32]) != U256::ZERO;
        let rel_off = u256_to_usize(U256::from_be_slice(&data[base + 32..base + 64]));
        let rd_abs = base.checked_add(rel_off).ok_or_else(short)?;
        let rd_len = read(rd_abs)?;
        let start = rd_abs.checked_add(32).ok_or_else(short)?;
        let end = start.checked_add(rd_len).ok_or_else(short)?;
        out.push((success, data.get(start..end).ok_or_else(short)?.to_vec()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(value: u64) -> [u8; 32] {
        let mut out = [0u8; 32];
        out[24..].copy_from_slice(&value.to_be_bytes());
        out
    }

    #[test]
    fn aggregate3_decodes_nested_result_offsets() {
        let mut data = Vec::new();
        data.extend_from_slice(&word(32)); // offset to array
        data.extend_from_slice(&word(2)); // length
        data.extend_from_slice(&word(64)); // element 0 -> base 128
        data.extend_from_slice(&word(192)); // element 1 -> base 256
        data.extend_from_slice(&word(1)); // success
        data.extend_from_slice(&word(64)); // bytes at base + 64
        data.extend_from_slice(&word(32)); // bytes length
        data.extend_from_slice(&word(7)); // bytes payload
        data.extend_from_slice(&word(1));
        data.extend_from_slice(&word(64));
        data.extend_from_slice(&word(32));
        data.extend_from_slice(&word(9));

        let decoded = decode_aggregate3(&data).unwrap();
        assert_eq!(decoded.len(), 2);
        assert!(decoded[0].0 && decoded[1].0);
        assert_eq!(decoded[0].1, word(7).to_vec());
        assert_eq!(decoded[1].1, word(9).to_vec());
    }

    #[test]
    fn aggregate3_encoding_is_stable() {
        let a = Address::with_last_byte(1);
        let b = Address::with_last_byte(2);
        let calldata = encode_aggregate3_get_reserves(&[a, b]);
        assert!(calldata.starts_with(AGGREGATE3));
        // selector (4 bytes) + 14 words (offset + length + 2 heads + 10 tuple words)
        assert_eq!(calldata.len(), 4 * 2 + 14 * 64);
    }

    #[test]
    fn aggregate3_rejects_overflowing_offsets_and_lengths() {
        let mut data = vec![255; 64];
        assert!(decode_aggregate3(&data).is_err());
        data[..32].copy_from_slice(&word(32));
        assert!(decode_aggregate3(&data).is_err());
    }

    #[tokio::test]
    async fn prefetch_does_not_make_cold_pools_hot() {
        let state = StateCache::new(
            RpcClient::new("http://127.0.0.1:1".into()),
            Duration::from_secs(30),
        );
        let pool = Address::with_last_byte(1);
        state.seed_reserves(pool, [U256::from(100), U256::from(200)]);
        // Succeeds without touching the deliberately unreachable RPC.
        state.refresh_hot().await.unwrap();
        assert_eq!(state.get_reserves(pool).await.unwrap()[0], U256::from(100));
        assert!(state.cache.lock().unwrap()[&pool].last_used.is_some());
    }
}
