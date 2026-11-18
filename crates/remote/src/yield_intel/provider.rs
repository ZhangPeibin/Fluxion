//! DefiLlama and on-chain data normalization for yield discovery.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alloy_primitives::{Address, B256};
use fluxion_protocol::yield_capability::YieldSource;
use serde::Deserialize;
use serde_json::Value;

use super::YieldError;
use super::registry::RegistryEntry;
use crate::rpc::RpcClient;

const DEFAULT_POOLS_URL: &str = "https://yields.llama.fi/pools";
const DEFAULT_PROTOCOLS_URL: &str = "https://api.llama.fi/protocols";
const DEFAULT_HACKS_URL: &str = "https://api.llama.fi/hacks";

#[derive(Clone)]
pub struct YieldProvider {
    http: reqwest::Client,
    pools_url: String,
    protocols_url: String,
    hacks_url: String,
    pub rpc: RpcClient,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlamaPool {
    pub pool: String,
    pub project: String,
    pub chain: String,
    pub symbol: String,
    #[serde(default, rename = "poolMeta")]
    pub pool_meta: Option<String>,
    #[serde(default, rename = "tvlUsd")]
    pub tvl_usd: Option<f64>,
    #[serde(default)]
    pub apy: Option<f64>,
    #[serde(default, rename = "apyBase")]
    pub apy_base: Option<f64>,
    #[serde(default, rename = "apyReward")]
    pub apy_reward: Option<f64>,
    #[serde(default, rename = "apyMean30d")]
    pub apy_mean_30d: Option<f64>,
    #[serde(default, rename = "apyPct30D")]
    pub apy_pct_30d: Option<f64>,
    #[serde(default)]
    pub outlier: bool,
    #[serde(default, rename = "ilRisk")]
    pub il_risk: Option<String>,
    #[serde(default)]
    pub exposure: Option<String>,
    #[serde(default)]
    pub stablecoin: Option<bool>,
    #[serde(default, rename = "underlyingTokens")]
    pub underlying_tokens: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlamaProtocol {
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tvl: Option<f64>,
    #[serde(default)]
    pub audits: Option<Value>,
    #[serde(default)]
    pub audit_links: Vec<String>,
    #[serde(default, rename = "listedAt")]
    pub listed_at: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlamaHack {
    pub name: String,
    #[serde(default)]
    pub date: u64,
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub classification: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProtocolMeta {
    pub slug: String,
    pub name: String,
    pub category: String,
    pub tvl_usd: Option<u64>,
    pub audits: Option<u32>,
    pub audit_links: Vec<String>,
    pub listed_at: Option<u64>,
    pub incident_count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct OnChainCheck {
    pub contract_has_code: bool,
    pub receipt_has_code: bool,
    pub receipt_symbol: Option<String>,
    pub receipt_decimals: Option<u8>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct ProviderData {
    pub pools: Vec<LlamaPool>,
    pub protocol_meta: HashMap<String, ProtocolMeta>,
    pub sources: Vec<YieldSource>,
    pub warnings: Vec<String>,
}

impl YieldProvider {
    pub fn new(rpc: RpcClient) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("yield provider HTTP client"),
            pools_url: std::env::var("FLUXION_YIELD_POOLS_URL")
                .unwrap_or_else(|_| DEFAULT_POOLS_URL.to_string()),
            protocols_url: std::env::var("FLUXION_YIELD_PROTOCOLS_URL")
                .unwrap_or_else(|_| DEFAULT_PROTOCOLS_URL.to_string()),
            hacks_url: std::env::var("FLUXION_YIELD_HACKS_URL")
                .unwrap_or_else(|_| DEFAULT_HACKS_URL.to_string()),
            rpc,
        }
    }

    pub async fn fetch(&self) -> Result<ProviderData, YieldError> {
        let (pools, protocols, hacks) = tokio::join!(
            self.get::<PoolsResponse>(&self.pools_url),
            self.get::<Vec<LlamaProtocol>>(&self.protocols_url),
            self.get::<Vec<LlamaHack>>(&self.hacks_url),
        );
        let pools = pools?;
        let protocols = protocols?;
        let hacks = hacks?;
        let fetched_at = now();
        let sources = vec![
            YieldSource {
                name: "defillama-yields".to_string(),
                url: self.pools_url.clone(),
                fetched_at,
            },
            YieldSource {
                name: "defillama-protocols".to_string(),
                url: self.protocols_url.clone(),
                fetched_at,
            },
            YieldSource {
                name: "defillama-hacks".to_string(),
                url: self.hacks_url.clone(),
                fetched_at,
            },
        ];
        let (protocol_meta, warnings) = normalize_protocols(protocols, hacks);
        Ok(ProviderData {
            pools: pools.data,
            protocol_meta,
            sources,
            warnings,
        })
    }

    pub async fn verify(&self, entry: &RegistryEntry) -> OnChainCheck {
        let mut check = OnChainCheck::default();
        match self.rpc.code_hash(entry.contract).await {
            Ok(hash) => check.contract_has_code = hash != B256::ZERO,
            Err(error) => check
                .warnings
                .push(format!("contract code check failed: {error}")),
        }
        match self.rpc.code_hash(entry.receipt_asset_address).await {
            Ok(hash) => check.receipt_has_code = hash != B256::ZERO,
            Err(error) => check
                .warnings
                .push(format!("receipt token code check failed: {error}")),
        }
        if check.receipt_has_code {
            match token_symbol(&self.rpc, entry.receipt_asset_address).await {
                Ok(symbol) => {
                    if !symbol.eq_ignore_ascii_case(&entry.receipt_asset_symbol) {
                        check.warnings.push(format!(
                            "receipt symbol mismatch: registry {}, chain {symbol}",
                            entry.receipt_asset_symbol
                        ));
                    }
                    check.receipt_symbol = Some(symbol);
                }
                Err(error) => check
                    .warnings
                    .push(format!("receipt symbol read failed: {error}")),
            }
            match token_decimals(&self.rpc, entry.receipt_asset_address).await {
                Ok(decimals) => {
                    if decimals != entry.receipt_decimals {
                        check.warnings.push(format!(
                            "receipt decimals mismatch: registry {}, chain {decimals}",
                            entry.receipt_decimals
                        ));
                    }
                    check.receipt_decimals = Some(decimals);
                }
                Err(error) => check
                    .warnings
                    .push(format!("receipt decimals read failed: {error}")),
            }
        }
        check
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T, YieldError> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|error| YieldError::Unavailable(format!("{url}: {error}")))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| YieldError::Unavailable(format!("{url}: {error}")))?;
        if !status.is_success() {
            return Err(YieldError::Unavailable(format!(
                "{url} returned HTTP {status}: {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        serde_json::from_str(&text).map_err(|error| {
            YieldError::Unavailable(format!("{url} returned invalid JSON: {error}"))
        })
    }
}

#[derive(Debug, Deserialize)]
struct PoolsResponse {
    data: Vec<LlamaPool>,
}

fn normalize_protocols(
    protocols: Vec<LlamaProtocol>,
    hacks: Vec<LlamaHack>,
) -> (HashMap<String, ProtocolMeta>, Vec<String>) {
    let mut incidents: HashMap<String, u32> = HashMap::new();
    for hack in &hacks {
        *incidents.entry(hack.name.to_lowercase()).or_default() += 1;
    }
    let mut by_slug = HashMap::new();
    let mut warnings = Vec::new();
    for protocol in protocols {
        let slug = protocol
            .slug
            .clone()
            .unwrap_or_else(|| protocol.name.to_lowercase().replace(' ', "-"));
        let name_key = protocol.name.to_lowercase();
        let incident_count = incidents.get(&name_key).copied().unwrap_or(0);
        let meta = ProtocolMeta {
            slug: slug.clone(),
            name: protocol.name.clone(),
            category: protocol.category.unwrap_or_else(|| "unknown".to_string()),
            tvl_usd: protocol.tvl.map(tvl_to_u64),
            audits: parse_audits(protocol.audits.as_ref()),
            audit_links: protocol.audit_links,
            listed_at: protocol.listed_at,
            incident_count,
        };
        by_slug.insert(slug, meta);
    }
    if by_slug.is_empty() {
        warnings.push("DefiLlama protocol metadata was empty".to_string());
    }
    (by_slug, warnings)
}

fn parse_audits(value: Option<&Value>) -> Option<u32> {
    let value = value?;
    if let Some(text) = value.as_str() {
        return text.parse().ok();
    }
    if let Some(number) = value.as_u64() {
        return u32::try_from(number).ok();
    }
    value.as_bool().map(u32::from)
}

pub fn tvl_to_u64(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.round().min(u64::MAX as f64) as u64
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

async fn token_symbol(rpc: &RpcClient, address: Address) -> Result<String, String> {
    let data = rpc
        .eth_call(address, "0x95d89b41")
        .await
        .map_err(|error| error.to_string())?;
    decode_abi_string(&data).ok_or_else(|| "unreadable symbol()".to_string())
}

async fn token_decimals(rpc: &RpcClient, address: Address) -> Result<u8, String> {
    let data = rpc
        .eth_call(address, "0x313ce567")
        .await
        .map_err(|error| error.to_string())?;
    let word = data
        .get(..32)
        .ok_or_else(|| "short decimals()".to_string())?;
    if word[..31].iter().any(|byte| *byte != 0) {
        return Err("decimals() overflow".to_string());
    }
    Ok(word[31])
}

pub fn decode_abi_string(data: &[u8]) -> Option<String> {
    if data.len() >= 64 {
        let offset = usize::try_from(u128::from_be_bytes(data[16..32].try_into().ok()?)).ok()?;
        if offset >= 32 && offset + 32 <= data.len() {
            let length = usize::try_from(u128::from_be_bytes(
                data[offset + 16..offset + 32].try_into().ok()?,
            ))
            .ok()?;
            if length > 0 && data.len() >= offset + 32 + length {
                return String::from_utf8(data[offset + 32..offset + 32 + length].to_vec()).ok();
            }
        }
    }
    let end = data
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(data.len());
    let text = std::str::from_utf8(&data[..end]).ok()?.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_audit_forms() {
        assert_eq!(parse_audits(Some(&Value::String("2".into()))), Some(2));
        assert_eq!(parse_audits(Some(&Value::from(3))), Some(3));
        assert_eq!(parse_audits(Some(&Value::Bool(true))), Some(1));
        assert_eq!(parse_audits(None), None);
    }

    #[test]
    fn decodes_abi_and_bytes32_strings() {
        let mut abi = vec![0u8; 32];
        abi[31] = 32;
        let mut length = vec![0u8; 32];
        length[31] = 3;
        abi.extend_from_slice(&length);
        abi.extend_from_slice(b"ETH");
        assert_eq!(decode_abi_string(&abi).as_deref(), Some("ETH"));

        let mut bytes32 = b"WETH".to_vec();
        bytes32.resize(32, 0);
        assert_eq!(decode_abi_string(&bytes32).as_deref(), Some("WETH"));
    }

    #[test]
    fn tvl_is_rounded_and_bounded() {
        assert_eq!(tvl_to_u64(1.4), 1);
        assert_eq!(tvl_to_u64(f64::NAN), 0);
        assert_eq!(tvl_to_u64(-1.0), 0);
    }
}
