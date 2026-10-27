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
const GET_RESERVES: &str = "0902f1ac";

#[derive(Clone)]
struct Cached {
    reserves: [U256; 2],
    at: Instant,
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

    pub async fn refresh_hot(&self) -> Result<(), QuoteError> {
        let addresses: Vec<Address> = {
            let cache = self.cache.lock().unwrap();
            cache.keys().copied().collect()
        };
        if addresses.is_empty() {
            return Ok(());
        }

        let calldata = encode_aggregate3_get_reserves(&addresses);
        let data = self.rpc.eth_call(multicall3(), &calldata).await?;
        let results = decode_aggregate3(&data)?;

        let mut cache = self.cache.lock().unwrap();
        for (pool, (success, return_data)) in addresses.iter().zip(results) {
            if success && let Ok(reserves) = decode_reserves(&return_data) {
                cache.insert(
                    *pool,
                    Cached {
                        reserves,
                        at: Instant::now(),
                    },
                );
            }
        }
        Ok(())
    }
}

#[async_trait]
impl V2Reserves for StateCache {
    async fn get_reserves(&self, pool: Address) -> Result<[U256; 2], QuoteError> {
        {
            let cache = self.cache.lock().unwrap();
            if let Some(c) = cache.get(&pool)
                && c.at.elapsed() < self.ttl
            {
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
            },
        );
        Ok(reserves)
    }
}

fn multicall3() -> Address {
    MULTICALL3.parse().unwrap()
}

/// Encode `aggregate3((address,bool,bytes)[])` for N `getReserves` calls.
fn encode_aggregate3_get_reserves(pools: &[Address]) -> String {
    let k = pools.len();
    let mut parts: Vec<String> = Vec::new();
    parts.push(AGGREGATE3.to_string());

    // One dynamic argument: offset to the array.
    parts.push(u32_word(0x20));
    // Array data: length.
    parts.push(u256_word(U256::from(k)));

    // Outer head: k offsets (one per element).
    for i in 0..k {
        parts.push(u256_word(U256::from(32 * (k + 5 * i))));
    }

    // Outer tail: k `Call3` tuples (address, bool, bytes).
    for pool in pools {
        parts.push(pad_address(*pool)); // target
        parts.push("0".repeat(64)); // allowFailure = false
        parts.push(u32_word(0x60)); // offset to callData = 96
        parts.push(u256_word(U256::from(4))); // callData length
        parts.push(format!("{GET_RESERVES}{}", "0".repeat(56))); // padded to 32 bytes
    }

    parts.concat()
}

/// Decode `aggregate3`'s `Result[]` return into `(success, returnData)` pairs.
fn decode_aggregate3(data: &[u8]) -> Result<Vec<(bool, Vec<u8>)>, QuoteError> {
    if data.len() < 64 {
        return Err(QuoteError::RpcError("short aggregate3 result".to_string()));
    }
    let offset = u256_to_usize(U256::from_be_slice(&data[0..32]));
    let n = u256_to_usize(U256::from_be_slice(&data[offset..offset + 32]));
    let heads_base = offset + 32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let head = heads_base + i * 64;
        let success = U256::from_be_slice(&data[head..head + 32]) != U256::ZERO;
        let rel_off = u256_to_usize(U256::from_be_slice(&data[head + 32..head + 64]));
        let rd_abs = heads_base + rel_off;
        let rd_len = u256_to_usize(U256::from_be_slice(&data[rd_abs..rd_abs + 32]));
        let rd = data[rd_abs + 32..rd_abs + 32 + rd_len].to_vec();
        out.push((success, rd));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate3_encoding_is_stable() {
        let a = Address::with_last_byte(1);
        let b = Address::with_last_byte(2);
        let calldata = encode_aggregate3_get_reserves(&[a, b]);
        assert!(calldata.starts_with(AGGREGATE3));
        // selector (4 bytes) + 14 words (offset + length + 2 heads + 10 tuple words)
        assert_eq!(calldata.len(), 4 * 2 + 14 * 64);
    }
}
