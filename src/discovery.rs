use alloy_primitives::Address;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::config::Config;
use crate::model::{Pool, PoolKind};

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
            if !config.token_allowlist.is_empty()
                && !config
                    .token_allowlist
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case(&token0) || a.eq_ignore_ascii_case(&token1))
            {
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
