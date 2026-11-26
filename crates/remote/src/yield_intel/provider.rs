//! DefiLlama and on-chain data normalization for yield discovery.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alloy_primitives::{Address, B256};
use eollo_protocol::yield_capability::YieldSource;
use serde::Deserialize;
use serde_json::Value;

use super::RegistryEntry;
use super::YieldError;
use crate::rpc::RpcClient;

const DEFAULT_POOLS_URL: &str = "https://yields.llama.fi/pools";
const DEFAULT_PROTOCOLS_URL: &str = "https://api.llama.fi/protocols";
const DEFAULT_HACKS_URL: &str = "https://api.llama.fi/hacks";
const DEFAULT_CHART_URL: &str = "https://yields.llama.fi/chart";

#[derive(Clone)]
pub struct YieldProvider {
    http: reqwest::Client,
    pools_url: String,
    protocols_url: String,
    hacks_url: String,
    chart_url: String,
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
    #[serde(default, deserialize_with = "vec_or_null")]
    pub audit_links: Vec<String>,
    #[serde(default, rename = "listedAt")]
    pub listed_at: Option<u64>,
}

fn vec_or_null<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<String>>::deserialize(deserializer)?.unwrap_or_default())
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

#[derive(Debug, Clone)]
pub struct HistoryPoint {
    pub timestamp: u64,
    pub apy: Option<f64>,
    pub apy_base: Option<f64>,
    pub apy_reward: Option<f64>,
    pub tvl_usd: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ChartResponse {
    data: Vec<RawHistoryPoint>,
}

#[derive(Debug, Deserialize)]
struct RawHistoryPoint {
    timestamp: String,
    #[serde(default, rename = "tvlUsd")]
    tvl_usd: Option<f64>,
    #[serde(default)]
    apy: Option<f64>,
    #[serde(default, rename = "apyBase")]
    apy_base: Option<f64>,
    #[serde(default, rename = "apyReward")]
    apy_reward: Option<f64>,
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
                .timeout(Duration::from_secs(180))
                .build()
                .expect("yield provider HTTP client"),
            pools_url: std::env::var("EOLLO_YIELD_POOLS_URL")
                .unwrap_or_else(|_| DEFAULT_POOLS_URL.to_string()),
            protocols_url: std::env::var("EOLLO_YIELD_PROTOCOLS_URL")
                .unwrap_or_else(|_| DEFAULT_PROTOCOLS_URL.to_string()),
            hacks_url: std::env::var("EOLLO_YIELD_HACKS_URL")
                .unwrap_or_else(|_| DEFAULT_HACKS_URL.to_string()),
            chart_url: std::env::var("EOLLO_YIELD_CHART_URL")
                .unwrap_or_else(|_| DEFAULT_CHART_URL.to_string()),
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

    pub async fn fetch_history(&self, pool_id: &str) -> Result<Vec<HistoryPoint>, YieldError> {
        let url = format!("{}/{}", self.chart_url.trim_end_matches('/'), pool_id);
        let chart: ChartResponse = self.get(&url).await?;
        let mut points = chart
            .data
            .into_iter()
            .filter_map(|point| {
                let timestamp = parse_iso8601(&point.timestamp)?;
                Some(HistoryPoint {
                    timestamp,
                    apy: point.apy,
                    apy_base: point.apy_base,
                    apy_reward: point.apy_reward,
                    tvl_usd: point.tvl_usd.map(tvl_to_u64),
                })
            })
            .collect::<Vec<_>>();
        points.sort_by_key(|point| point.timestamp);
        if points.is_empty() {
            return Err(YieldError::Unavailable(
                "DefiLlama chart returned no readable points".to_string(),
            ));
        }
        Ok(points)
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

pub fn parse_iso8601(value: &str) -> Option<u64> {
    let bytes = value.as_bytes();
    if bytes.len() < 19
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || bytes.get(10) != Some(&b'T')
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return None;
    }
    let year = value.get(0..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<i64>().ok()?;
    let day = value.get(8..10)?.parse::<i64>().ok()?;
    let hour = value.get(11..13)?.parse::<i64>().ok()?;
    let minute = value.get(14..16)?.parse::<i64>().ok()?;
    let second = value.get(17..19)?.parse::<i64>().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    let seconds = days
        .checked_mul(86_400)?
        .checked_add(hour * 3_600 + minute * 60 + second)?;
    u64::try_from(seconds).ok()
}

fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month_index = (month + 9) % 12;
    let doy = (153 * month_index + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days)
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
    fn parses_iso8601_timestamps() {
        assert_eq!(parse_iso8601("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            parse_iso8601("2026-11-19T02:01:44.223Z"),
            Some(1_795_053_704)
        );
        assert_eq!(parse_iso8601("not-a-date"), None);
    }

    #[test]
    fn tvl_is_rounded_and_bounded() {
        assert_eq!(tvl_to_u64(1.4), 1);
        assert_eq!(tvl_to_u64(f64::NAN), 0);
        assert_eq!(tvl_to_u64(-1.0), 0);
    }
}
