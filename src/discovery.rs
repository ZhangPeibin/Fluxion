use alloy_primitives::Address;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::config::Config;
use crate::model::{Pool, PoolKind};

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
    let mut skip = 0usize;
    let mut pools: Vec<Pool> = Vec::new();

    loop {
        let query = format!(
            r#"{{ pairs(first: 200, skip: {skip}, orderBy: reserveUSD, orderDirection: desc, where: {{ reserveUSD_gt: "{}" }}) {{ id token0 {{ id }} token1 {{ id }} }} }}"#,
            config.min_liquidity
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
    let mut skip = 0usize;
    let mut pools: Vec<Pool> = Vec::new();

    loop {
        let query = format!(
            r#"{{ pools(first: 200, skip: {skip}, orderBy: volumeUSD, orderDirection: desc, where: {{ totalValueLockedUSD_gt: "{}" }}) {{ id token0 {{ id }} token1 {{ id }} feeTier }} }}"#,
            config.min_liquidity
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

const CURVE_3POOL: &str = "0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7";
const DAI: &str = "0x6B175474E89094C44Da98b954EedeAC495271d0F";
const USDC: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
const USDT: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";

/// Known Curve pools. Registry-based discovery (AddressProvider -> registry ->
/// pool_list) is deferred; this returns a fixed, well-known 3pool.
pub fn discover_curve(_config: &Config) -> Vec<Pool> {
    vec![Pool {
        kind: PoolKind::Curve,
        address: CURVE_3POOL.parse().unwrap(),
        coins: vec![
            DAI.parse().unwrap(),
            USDC.parse().unwrap(),
            USDT.parse().unwrap(),
        ],
    }]
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
