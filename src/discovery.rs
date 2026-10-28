use alloy_primitives::{Address, U256};
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use thiserror::Error;

use crate::config::Config;
use crate::model::{Pool, PoolKind};
use crate::rpc::RpcClient;
use crate::util::{u256_to_usize, u256_word};

/// Built-in major-token allowlist used when the config leaves `token_allowlist`
/// empty. Pools must have BOTH tokens in the allowlist, which keeps the graph
/// small and avoids junk/thin pools poisoning prices.
const DEFAULT_ALLOWLIST: &[&str] = &[
    "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", // WETH
    "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599", // WBTC
    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // USDC
    "0xdac17f958d2ee523a2206206994597c13d831ec7", // USDT
    "0x6b175474e89094c44da98b954eedeac495271d0f", // DAI
    "0xdc035d45d973e3ec169d2276ddab16f1e407384f", // USDS
    "0xae7ab96520de3a18e5e111b5eaab095312d7fe84", // stETH
    "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0", // wstETH
    "0x514910771af9ca656af840dff83e8264ecf986ca", // LINK
    "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984", // UNI
    "0x7fc66500c84a76ad7e9c93437bfc5ac33e2ddae9", // AAVE
    "0x9f8f72aa9304c8b593d555f12ef6589cc3a579a2", // MKR
    "0xd533a949740bb3306d119cc777fa900ba034cd52", // CRV
    "0x5a98fcbea516cf06857215779fd812ca3bef1b32", // LDO
];

fn allowlist(config: &Config) -> Vec<String> {
    if config.token_allowlist.is_empty() {
        DEFAULT_ALLOWLIST.iter().map(|s| s.to_string()).collect()
    } else {
        config
            .token_allowlist
            .iter()
            .map(|s| s.to_lowercase())
            .collect()
    }
}

fn both_allowed(allow: &[String], token0: &str, token1: &str) -> bool {
    allow.iter().any(|a| a.eq_ignore_ascii_case(token0))
        && allow.iter().any(|a| a.eq_ignore_ascii_case(token1))
}

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("subgraph request failed: {0}")]
    Request(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid address: {0}")]
    Address(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct DiscoveredPool {
    address: String,
    token0: String,
    token1: String,
}

impl DiscoveredPool {
    fn to_pool(&self) -> Result<Pool, DiscoveryError> {
        Ok(Pool {
            kind: PoolKind::UniswapV2,
            address: parse_address(&self.address)?,
            coins: vec![parse_address(&self.token0)?, parse_address(&self.token1)?],
        })
    }

    fn from_pool(p: &Pool) -> Self {
        Self {
            address: format!("{:#x}", p.address),
            token0: format!("{:#x}", p.coins[0]),
            token1: format!("{:#x}", p.coins[1]),
        }
    }
}

fn parse_address(s: &str) -> Result<Address, DiscoveryError> {
    s.parse::<Address>()
        .map_err(|_| DiscoveryError::Address(s.to_string()))
}

/// Build the GraphQL `where` filter: liquidity floor plus optional volume and
/// pool-age floors. `volumeUSD` is all-time volume, used as an activity proxy.
fn discovery_where(config: &Config, tvl_field: &str) -> String {
    let mut parts = vec![format!("{tvl_field}_gt: \"{}\"", config.min_liquidity)];
    if config.min_volume_24h > 0 {
        parts.push(format!("volumeUSD_gt: \"{}\"", config.min_volume_24h));
    }
    if config.min_pool_age_days > 0 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let cutoff = now.saturating_sub(config.min_pool_age_days.saturating_mul(86_400));
        parts.push(format!("createdAtTimestamp_lt: \"{cutoff}\""));
    }
    parts.join(", ")
}

/// Discover Uniswap V2 pairs from the subgraph, filtered by a liquidity
/// threshold and an optional token allowlist.
pub async fn discover_v2(
    config: &Config,
    graph_api_key: &str,
) -> Result<Vec<Pool>, DiscoveryError> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://gateway.thegraph.com/api/{graph_api_key}/subgraphs/id/{}",
        config.uniswap_v2_subgraph_id
    );

    let allow = allowlist(config);
    let where_clause = discovery_where(config, "reserveUSD");
    let mut skip = 0usize;
    let mut pools: Vec<Pool> = Vec::new();

    loop {
        let query = format!(
            r#"{{ pairs(first: 200, skip: {skip}, orderBy: reserveUSD, orderDirection: desc, where: {{ {where_clause} }}) {{ id token0 {{ id }} token1 {{ id }} }} }}"#
        );
        let resp: serde_json::Value = client
            .post(&url)
            .json(&json!({ "query": query }))
            .send()
            .await
            .map_err(|e| DiscoveryError::Request(e.to_string()))?
            .json()
            .await
            .map_err(|e| DiscoveryError::Request(e.to_string()))?;

        if let Some(errors) = resp.get("errors") {
            return Err(DiscoveryError::Request(errors.to_string()));
        }

        let pairs = resp["data"]["pairs"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if pairs.is_empty() {
            break;
        }

        for p in &pairs {
            let address = p["id"].as_str().unwrap_or_default().to_string();
            let token0 = p["token0"]["id"].as_str().unwrap_or_default().to_string();
            let token1 = p["token1"]["id"].as_str().unwrap_or_default().to_string();
            if address.is_empty() || token0.is_empty() || token1.is_empty() {
                continue;
            }
            if !both_allowed(&allow, &token0, &token1) {
                continue;
            }
            pools.push(
                DiscoveredPool {
                    address,
                    token0,
                    token1,
                }
                .to_pool()?,
            );
        }

        if pools.len() >= config.max_pools {
            pools.truncate(config.max_pools);
            break;
        }
        if pairs.len() < 200 {
            break;
        }
        skip += 200;
    }

    Ok(pools)
}

pub fn save_cache(path: &str, pools: &[Pool]) -> Result<(), DiscoveryError> {
    let discovered: Vec<DiscoveredPool> = pools.iter().map(DiscoveredPool::from_pool).collect();
    let json = serde_json::to_string_pretty(&discovered)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn load_cache(path: &str) -> Result<Vec<Pool>, DiscoveryError> {
    let text = std::fs::read_to_string(path)?;
    let discovered: Vec<DiscoveredPool> = serde_json::from_str(&text)?;
    discovered.iter().map(DiscoveredPool::to_pool).collect()
}

/// Discover Uniswap V3 pools (per fee tier) from the subgraph.
pub async fn discover_v3(
    config: &Config,
    graph_api_key: &str,
) -> Result<Vec<Pool>, DiscoveryError> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://gateway.thegraph.com/api/{graph_api_key}/subgraphs/id/{}",
        config.uniswap_v3_subgraph_id
    );

    let allow = allowlist(config);
    let where_clause = discovery_where(config, "totalValueLockedUSD");
    let mut skip = 0usize;
    let mut pools: Vec<Pool> = Vec::new();

    loop {
        let query = format!(
            r#"{{ pools(first: 200, skip: {skip}, orderBy: volumeUSD, orderDirection: desc, where: {{ {where_clause} }}) {{ id token0 {{ id }} token1 {{ id }} feeTier }} }}"#
        );
        let resp: serde_json::Value = client
            .post(&url)
            .json(&json!({ "query": query }))
            .send()
            .await
            .map_err(|e| DiscoveryError::Request(e.to_string()))?
            .json()
            .await
            .map_err(|e| DiscoveryError::Request(e.to_string()))?;

        if let Some(errors) = resp.get("errors") {
            return Err(DiscoveryError::Request(errors.to_string()));
        }

        let arr = resp["data"]["pools"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if arr.is_empty() {
            break;
        }

        for p in &arr {
            let address = p["id"].as_str().unwrap_or_default().to_string();
            let token0 = p["token0"]["id"].as_str().unwrap_or_default().to_string();
            let token1 = p["token1"]["id"].as_str().unwrap_or_default().to_string();
            let fee: u32 = p["feeTier"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if address.is_empty() || token0.is_empty() || token1.is_empty() || fee == 0 {
                continue;
            }
            if !both_allowed(&allow, &token0, &token1) {
                continue;
            }
            pools.push(Pool {
                kind: PoolKind::UniswapV3 { fee },
                address: parse_address(&address)?,
                coins: vec![parse_address(&token0)?, parse_address(&token1)?],
            });
        }

        if pools.len() >= config.max_pools {
            pools.truncate(config.max_pools);
            break;
        }
        if arr.len() < 200 {
            break;
        }
        skip += 200;
    }

    Ok(pools)
}

const CURVE_GET_ADDRESS: &str = "493f4f74";
const CURVE_POOL_COUNT: &str = "956aae3a";
const CURVE_POOL_LIST: &str = "3a1d5d8e";
const CURVE_COINS: &str = "c6610657";
const CURVE_GET_DY_INT128: &str = "5e0d443f";
const CURVE_GET_DY_U256: &str = "556d6e9f";
const CURVE_REGISTRY_BATCH: usize = 16;

fn addr_from_word(data: &[u8]) -> Result<Address, DiscoveryError> {
    if data.len() < 20 {
        return Err(DiscoveryError::Address("short word".to_string()));
    }
    Ok(Address::from_slice(&data[data.len() - 20..]))
}

/// Resolve a registry address from the AddressProvider by id (`None` if zero).
async fn curve_registry(
    provider: Address,
    id: u64,
    rpc: &RpcClient,
) -> Result<Option<Address>, DiscoveryError> {
    let data = format!("{CURVE_GET_ADDRESS}{}", u256_word(U256::from(id)));
    let out = rpc
        .eth_call(provider, &data)
        .await
        .map_err(|e| DiscoveryError::Request(e.to_string()))?;
    let addr = addr_from_word(&out)?;
    Ok((addr != Address::ZERO).then_some(addr))
}

/// Enumerate one registry: `pool_count` / `pool_list(i)` -> `coins` -> `get_dy` probe.
async fn discover_registry(
    registry: Address,
    max_pools: usize,
    rpc: &RpcClient,
    known: &HashMap<Address, Pool>,
) -> Vec<(Address, Vec<Address>)> {
    let count_data = match rpc.eth_call(registry, CURVE_POOL_COUNT).await {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    if count_data.len() < 32 {
        return Vec::new();
    }
    let count = u256_to_usize(U256::from_be_slice(&count_data[0..32])).min(max_pools);

    stream::iter(0..count)
        .map(|i| {
            let rpc = rpc.clone();
            async move {
                let data = format!("{CURVE_POOL_LIST}{}", u256_word(U256::from(i)));
                let pool = addr_from_word(&rpc.eth_call(registry, &data).await.ok()?).ok()?;
                if known.contains_key(&pool) {
                    return None;
                }
                let mut coins = Vec::new();
                for j in 0..4usize {
                    let cd = format!("{CURVE_COINS}{}", u256_word(U256::from(j)));
                    match rpc.eth_call(pool, &cd).await {
                        Ok(d) if d.len() >= 32 => match addr_from_word(&d) {
                            Ok(coin) => coins.push(coin),
                            Err(_) => break,
                        },
                        _ => break,
                    }
                }
                if coins.len() < 2 {
                    return None;
                }
                // Accept get_dy(int128,...) or get_dy(uint256,...).
                let mut quoteable = false;
                for selector in [CURVE_GET_DY_INT128, CURVE_GET_DY_U256] {
                    let probe = format!(
                        "{selector}{}{}{}",
                        u256_word(U256::ZERO),
                        u256_word(U256::from(1u64)),
                        u256_word(U256::from(1_000_000u64))
                    );
                    if rpc.eth_call(pool, &probe).await.is_ok() {
                        quoteable = true;
                        break;
                    }
                }
                if !quoteable {
                    return None;
                }
                Some((pool, coins))
            }
        })
        .buffer_unordered(CURVE_REGISTRY_BATCH)
        .filter_map(|x| async move { x })
        .collect()
        .await
}

/// Discover Curve pools across the main stableswap, crypto, and MetaRegistry
/// (AddressProvider ids 0/5/7), dedup by address, keeping only pools whose coins
/// are all allowlisted.
pub async fn discover_curve(config: &Config, rpc: &RpcClient) -> Result<Vec<Pool>, DiscoveryError> {
    let allow = allowlist(config);
    let provider: Address = config
        .curve_address_provider
        .parse()
        .map_err(|_| DiscoveryError::Address(config.curve_address_provider.clone()))?;

    let mut by_addr: HashMap<Address, Pool> = HashMap::new();
    for &id in &config.curve_registry_ids {
        let Ok(Some(registry)) = curve_registry(provider, id, rpc).await else {
            continue;
        };
        for (address, coins) in discover_registry(registry, config.max_pools, rpc, &by_addr).await {
            let all_allowed = coins.iter().all(|coin| {
                let s = format!("{coin:#x}");
                allow.iter().any(|a| a.eq_ignore_ascii_case(&s))
            });
            if all_allowed {
                by_addr.entry(address).or_insert(Pool {
                    kind: PoolKind::Curve,
                    address,
                    coins,
                });
            }
        }
    }
    Ok(by_addr.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pools() -> Vec<Pool> {
        vec![
            Pool {
                kind: PoolKind::UniswapV2,
                address: Address::with_last_byte(10),
                coins: vec![Address::with_last_byte(1), Address::with_last_byte(2)],
            },
            Pool {
                kind: PoolKind::UniswapV2,
                address: Address::with_last_byte(11),
                coins: vec![Address::with_last_byte(2), Address::with_last_byte(3)],
            },
        ]
    }

    #[test]
    fn both_allowed_requires_both_tokens() {
        let allow = vec!["0xaaa".to_string(), "0xbbb".to_string()];
        assert!(both_allowed(&allow, "0xAAA", "0xbbb"));
        assert!(!both_allowed(&allow, "0xaaa", "0xccc"));
    }

    #[test]
    fn discovery_where_includes_optional_filters() {
        let cfg = Config {
            min_liquidity: 1000,
            min_volume_24h: 5,
            min_pool_age_days: 3,
            ..Default::default()
        };
        let w = discovery_where(&cfg, "reserveUSD");
        assert!(w.contains("reserveUSD_gt"));
        assert!(w.contains("volumeUSD_gt"));
        assert!(w.contains("createdAtTimestamp_lt"));
    }

    #[test]
    fn cache_round_trips() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("fluxion-pools-{}.json", std::process::id()));
        let path = path.to_str().unwrap();
        save_cache(path, &pools()).unwrap();
        let loaded = load_cache(path).unwrap();
        assert_eq!(loaded, pools());
        let _ = std::fs::remove_file(path);
    }
}
