//! Capability envelope v2.
//!
//! Adapters use these helpers to wrap existing capability payloads in the common response shape.
//! The migration is per capability: a capability is wrapped only when its catalog entry declares
//! `envelope: v2`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use crate::capability::{CapabilityError, CapabilityRegistry};

pub const ENVELOPE_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityEnvelope<T> {
    pub meta: EnvelopeMeta,
    pub chain: ChainRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<BlockRef>,
    pub as_of: String,
    pub provenance: EnvelopeProvenance,
    pub coverage: EnvelopeCoverage,
    pub warnings: Vec<Notice>,
    pub unknowns: Vec<Notice>,
    pub data: T,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityErrorEnvelope {
    pub meta: EnvelopeMeta,
    pub chain: ChainRef,
    pub error: CapabilityError,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvelopeMeta {
    pub schema_version: u32,
    pub capability: String,
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChainRef {
    pub name: String,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockRef {
    pub number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvelopeProvenance {
    pub sources: Vec<Value>,
    pub freshness: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvelopeCoverage {
    pub dimensions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

/// Whether a capability has opted into envelope v2 in the catalog.
pub fn uses_v2(capability: &str) -> bool {
    CapabilityRegistry::global()
        .get(capability)
        .map(|entry| entry.envelope == "v2")
        .unwrap_or(false)
}

pub fn wrap_success(capability: &str, value: Value) -> Value {
    let request_id = value
        .get("request_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(new_request_id);
    let as_of = extract_as_of(&value).unwrap_or_else(now_iso8601);
    let chain = extract_chain(capability, &value);
    let block = extract_block(&value);
    let provenance = extract_provenance(&value);
    let coverage = extract_coverage(capability, &value);
    let warnings = extract_notices(value.get("warnings"), "WARNING");
    let unknowns = extract_notices(value.get("unknowns"), "UNKNOWN");

    let mut data = value;
    if let Some(object) = data.as_object_mut() {
        for key in COMMON_KEYS {
            object.remove(*key);
        }
    }

    let envelope = CapabilityEnvelope {
        meta: EnvelopeMeta {
            schema_version: ENVELOPE_SCHEMA_VERSION,
            capability: capability.to_string(),
            request_id,
        },
        chain,
        block,
        as_of,
        provenance,
        coverage,
        warnings,
        unknowns,
        data,
    };
    serde_json::to_value(envelope).expect("capability envelope must serialize")
}

pub fn wrap_error(capability: &str, error: CapabilityError) -> Value {
    let envelope = CapabilityErrorEnvelope {
        meta: EnvelopeMeta {
            schema_version: ENVELOPE_SCHEMA_VERSION,
            capability: capability.to_string(),
            request_id: new_request_id(),
        },
        chain: default_chain_ref(capability),
        error,
    };
    serde_json::to_value(envelope).expect("capability error envelope must serialize")
}

fn default_chain_name(capability: &str) -> String {
    CapabilityRegistry::global()
        .get(capability)
        .and_then(|entry| entry.default_chain.clone())
        .unwrap_or_else(|| "bsc".to_string())
}

fn default_chain_ref(capability: &str) -> ChainRef {
    let name = default_chain_name(capability);
    ChainRef {
        id: chain_id_for(&name),
        name,
    }
}

fn extract_chain(capability: &str, value: &Value) -> ChainRef {
    let chain_id = value.get("chain_id").and_then(Value::as_u64);
    let name = value
        .get("chain")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| chain_id.map(chain_name_for))
        .unwrap_or_else(|| default_chain_name(capability));
    let id = chain_id.unwrap_or_else(|| chain_id_for(&name));
    ChainRef { name, id }
}

fn chain_name_for(id: u64) -> String {
    if id == 1 {
        "ethereum".to_string()
    } else {
        "bsc".to_string()
    }
}

fn chain_id_for(name: &str) -> u64 {
    match name.trim().to_ascii_lowercase().as_str() {
        "ethereum" | "eth" | "mainnet" | "1" => 1,
        _ => 56,
    }
}

fn extract_block(value: &Value) -> Option<BlockRef> {
    let provenance = value.get("provenance");
    let simulation = value.get("simulation");
    let number = value
        .get("block_number")
        .and_then(Value::as_u64)
        .or_else(|| value.get("quote_block").and_then(Value::as_u64))
        .or_else(|| {
            provenance
                .and_then(|item| item.get("quote_block"))
                .and_then(Value::as_u64)
        })
        .or_else(|| {
            simulation
                .and_then(|item| item.get("block_number"))
                .and_then(Value::as_u64)
        })?;
    let hash = value
        .get("block_hash")
        .and_then(Value::as_str)
        .or_else(|| {
            provenance
                .and_then(|item| item.get("block_hash"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            simulation
                .and_then(|item| item.get("block_hash"))
                .and_then(Value::as_str)
        })
        .map(str::to_string);
    Some(BlockRef { number, hash })
}

fn extract_provenance(value: &Value) -> EnvelopeProvenance {
    let provenance = value.get("provenance");
    let sources = value
        .get("sources")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| {
            provenance
                .and_then(|item| item.get("sources"))
                .and_then(Value::as_array)
                .cloned()
        })
        .unwrap_or_default();
    let age_secs = value
        .get("state_age_secs")
        .and_then(Value::as_u64)
        .or_else(|| {
            provenance
                .and_then(|item| item.get("state_age_secs"))
                .and_then(Value::as_u64)
        });
    let freshness = match age_secs {
        Some(age) if age <= 60 => "fresh",
        Some(_) => "stale",
        None => "unknown",
    };
    let pricing_mode = provenance
        .and_then(|item| item.get("pricing_mode"))
        .and_then(Value::as_str)
        .map(str::to_string);
    EnvelopeProvenance {
        sources,
        freshness: freshness.to_string(),
        age_secs,
        pricing_mode,
    }
}

fn extract_coverage(capability: &str, value: &Value) -> EnvelopeCoverage {
    let mut dimensions = BTreeMap::new();
    match value.get("coverage") {
        Some(Value::String(coverage)) => {
            dimensions.insert("data".to_string(), normalize_coverage(coverage));
        }
        Some(Value::Object(coverage)) => {
            if let Some(Value::Object(items)) = coverage.get("dimensions") {
                for (name, status) in items {
                    if let Some(status) = status.as_str() {
                        dimensions.insert(name.clone(), normalize_coverage(status));
                    }
                }
            } else {
                for (name, status) in coverage {
                    if let Some(status) = status.as_str() {
                        dimensions.insert(name.clone(), normalize_coverage(status));
                    }
                }
            }
        }
        _ => {}
    }
    if dimensions.is_empty() {
        match capability {
            "transaction.simulate" => {
                dimensions.insert("simulation".to_string(), "complete".to_string());
            }
            "swap.quote" => {
                dimensions.insert("data".to_string(), "complete".to_string());
                let freshness = value
                    .get("provenance")
                    .and_then(|item| {
                        item.get("state_age_secs")
                            .or_else(|| item.get("lag_blocks"))
                    })
                    .is_some();
                dimensions.insert(
                    "freshness".to_string(),
                    if freshness {
                        "complete"
                    } else {
                        "not_configured"
                    }
                    .to_string(),
                );
            }
            "swap.build" => {
                dimensions.insert("data".to_string(), "complete".to_string());
            }
            "wallet.send" => {
                dimensions.insert("simulation".to_string(), "complete".to_string());
            }
            "wallet.sign" => {
                dimensions.insert("policy".to_string(), "not_run".to_string());
            }
            "transaction.send" => {
                dimensions.insert("simulation".to_string(), "complete".to_string());
                let policy = value
                    .get("inspection")
                    .and_then(|inspection| inspection.get("coverage"))
                    .and_then(|coverage| coverage.get("policy"))
                    .and_then(Value::as_str)
                    .map(normalize_coverage)
                    .unwrap_or_else(|| "not_run".to_string());
                dimensions.insert("policy".to_string(), policy);
            }
            _ => {}
        }
    }
    EnvelopeCoverage { dimensions }
}

fn normalize_coverage(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "complete" | "partial" | "not_run" | "unavailable" | "not_configured" => normalized,
        other if other.contains("partial") => "partial".to_string(),
        other if other.contains("unavailable") => "unavailable".to_string(),
        other if other.contains("not_configured") => "not_configured".to_string(),
        _ => "complete".to_string(),
    }
}

fn extract_notices(value: Option<&Value>, default_code: &str) -> Vec<Notice> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| match item {
                    Value::String(message) => Notice {
                        code: default_code.to_string(),
                        message: message.clone(),
                        source: None,
                        details: None,
                    },
                    Value::Object(object) => Notice {
                        code: object
                            .get("code")
                            .and_then(Value::as_str)
                            .unwrap_or(default_code)
                            .to_string(),
                        message: object
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        source: object
                            .get("source")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        details: object.get("details").cloned(),
                    },
                    other => Notice {
                        code: default_code.to_string(),
                        message: other.to_string(),
                        source: None,
                        details: None,
                    },
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_as_of(value: &Value) -> Option<String> {
    match value.get("as_of")? {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => {
            let seconds = value.as_u64()?;
            time::OffsetDateTime::from_unix_timestamp(seconds as i64)
                .ok()
                .and_then(|time| {
                    time.format(&time::format_description::well_known::Rfc3339)
                        .ok()
                })
        }
        _ => None,
    }
}

fn now_iso8601() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn new_request_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("req_{nanos:x}{counter:x}")
}

const COMMON_KEYS: &[&str] = &[
    "schema_version",
    "capability",
    "request_id",
    "chain",
    "chain_id",
    "block_number",
    "block_hash",
    "quote_block",
    "coverage",
    "warnings",
    "unknowns",
    "sources",
    "provenance",
    "state_age_secs",
    "lag_blocks",
    "as_of",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilityErrorCode, CapabilityErrorResponse};
    use serde_json::json;

    #[test]
    fn migrated_capabilities_use_v2() {
        for capability in [
            "wallet.balance",
            "wallet.assets",
            "wallet.allowance",
            "wallet.collectibles",
            "fourmeme.agent-tokens",
            "fourmeme.token",
            "transaction.simulate",
            "transaction.inspect",
            "signature.inspect",
            "yield.discover",
            "yield.build",
            "yield.history",
            "yield.withdraw",
            "position.analyze",
            "swap.quote",
            "swap.build",
            "wallet.connect",
            "wallet.status",
            "wallet.disconnect",
            "wallet.sign",
            "wallet.send",
            "transaction.send",
        ] {
            assert!(uses_v2(capability), "{capability} should use v2");
        }
        assert!(uses_v2("wallet.agent-id"));
        assert!(!uses_v2("unknown.capability"));
    }

    #[test]
    fn wraps_wallet_balance_payload() {
        let value = json!({
            "chain": "bsc",
            "chain_id": 56,
            "address": "0x0000000000000000000000000000000000000001",
            "block_number": 42,
            "block_hash": "0xabc",
            "native": null,
            "tokens": [],
            "unknown_count": 0,
            "coverage": "local",
            "sources": [{"name": "bsc-rpc", "scope": "balance"}],
            "warnings": ["provider unavailable"]
        });
        let wrapped = wrap_success("wallet.balance", value);
        assert_eq!(wrapped["meta"]["schema_version"], 2);
        assert_eq!(wrapped["meta"]["capability"], "wallet.balance");
        assert_eq!(wrapped["chain"]["name"], "bsc");
        assert_eq!(wrapped["chain"]["id"], 56);
        assert_eq!(wrapped["block"]["number"], 42);
        assert_eq!(wrapped["block"]["hash"], "0xabc");
        assert_eq!(wrapped["coverage"]["dimensions"]["data"], "complete");
        assert_eq!(wrapped["warnings"][0]["code"], "WARNING");
        assert_eq!(wrapped["warnings"][0]["message"], "provider unavailable");
        assert!(wrapped["data"].get("chain").is_none());
        assert!(wrapped["data"].get("coverage").is_none());
        assert!(wrapped["data"].get("address").is_some());
        assert!(wrapped["provenance"]["sources"].is_array());
    }

    #[test]
    fn wraps_swap_quote_payload() {
        let value = json!({
            "schema_version": 1,
            "capability": "swap.quote",
            "request_id": "req_swap",
            "chain": "ethereum",
            "token_in": {"symbol": "LDO", "address": "0x0000000000000000000000000000000000000001", "decimals": 18},
            "token_out": {"symbol": "WETH", "address": "0x0000000000000000000000000000000000000002", "decimals": 18},
            "amount_in": {"raw": "100000000000000000000", "decimal": "100"},
            "amount_out": {"raw": "5000000000000000000", "decimal": "5"},
            "plan_kind": "direct",
            "allocations": [],
            "provenance": {
                "execution": "remote",
                "pricing_mode": "local_snapshot",
                "quote_block": 42,
                "block_hash": "0xabc",
                "state_age_secs": 3,
                "lag_blocks": 0
            },
            "latency_ms": 12
        });
        let wrapped = wrap_success("swap.quote", value);
        assert_eq!(wrapped["meta"]["capability"], "swap.quote");
        assert_eq!(wrapped["chain"]["name"], "ethereum");
        assert_eq!(wrapped["block"]["number"], 42);
        assert_eq!(wrapped["coverage"]["dimensions"]["data"], "complete");
        assert_eq!(wrapped["coverage"]["dimensions"]["freshness"], "complete");
        assert_eq!(wrapped["provenance"]["pricing_mode"], "local_snapshot");
        assert_eq!(wrapped["data"]["plan_kind"], "direct");
        assert!(wrapped["data"].get("provenance").is_none());
    }

    #[test]
    fn wraps_swap_build_payload() {
        let value = json!({
            "chain": "ethereum",
            "chain_id": 1,
            "quote_block": 77,
            "router": "0x0000000000000000000000000000000000000002",
            "value": "0",
            "data": "0x1234",
            "token_in": "0x0000000000000000000000000000000000000001",
            "token_out": "0x0000000000000000000000000000000000000002",
            "amount_in": {"raw": "1", "decimal": "1"},
            "amount_out_min": {"raw": "1", "decimal": "1"},
            "recipient": "0x0000000000000000000000000000000000000003",
            "deadline": 100,
            "path": [],
            "hops": []
        });
        let wrapped = wrap_success("swap.build", value);
        assert_eq!(wrapped["block"]["number"], 77);
        assert_eq!(wrapped["coverage"]["dimensions"]["data"], "complete");
        assert!(wrapped["data"].get("quote_block").is_none());
    }

    #[test]
    fn wallet_connect_derives_chain_from_chain_id() {
        let value = json!({
            "status": "connected",
            "connection_id": "c1",
            "uri": "wc:...",
            "qr_ascii": "",
            "expires_at": 1,
            "accounts": [],
            "chain_id": 1
        });
        let wrapped = wrap_success("wallet.connect", value);
        assert_eq!(wrapped["chain"]["name"], "ethereum");
        assert_eq!(wrapped["chain"]["id"], 1);
    }

    #[test]
    fn wraps_yield_discover_payload() {
        let value = json!({
            "schema_version": 1,
            "capability": "yield.discover",
            "request_id": "req_yield",
            "chain": "ethereum",
            "asset": {"symbol": "ETH", "address": "native", "decimals": 18},
            "risk_profile": "conservative",
            "objective": "safe yield",
            "opportunities": [],
            "excluded": [],
            "as_of": 1700000000,
            "age_secs": 5,
            "stale": false,
            "coverage": "curated-universe+onchain",
            "sources": [{"name": "defillama", "url": "https://example.com", "fetched_at": 1700000000}],
            "warnings": []
        });
        let wrapped = wrap_success("yield.discover", value);
        assert_eq!(wrapped["meta"]["capability"], "yield.discover");
        assert_eq!(wrapped["meta"]["request_id"], "req_yield");
        assert_eq!(wrapped["chain"]["name"], "ethereum");
        assert_eq!(wrapped["coverage"]["dimensions"]["data"], "complete");
        assert_eq!(wrapped["data"]["asset"]["symbol"], "ETH");
        assert!(wrapped["data"].get("chain").is_none());
        assert!(wrapped["data"].get("coverage").is_none());
        assert!(wrapped["data"].get("as_of").is_none());
        assert_eq!(wrapped["provenance"]["sources"][0]["name"], "defillama");
    }

    #[test]
    fn converts_unix_as_of_to_rfc3339() {
        let value = json!({
            "chain": "ethereum",
            "chain_id": 1,
            "as_of": 1700000000,
            "coverage": "curated-universe+onchain",
            "sources": [],
            "warnings": []
        });
        let wrapped = wrap_success("position.analyze", value);
        assert!(
            wrapped["as_of"]
                .as_str()
                .unwrap()
                .starts_with("2023-11-14T")
        );
        assert!(wrapped["data"].get("as_of").is_none());
    }

    #[test]
    fn transaction_inspect_maps_coverage_and_unknowns() {
        let value = json!({
            "chain": "bsc",
            "chain_id": 56,
            "block_number": 11,
            "block_hash": "0xabc",
            "to": "0x0000000000000000000000000000000000000001",
            "value": {"raw": "0", "decimal": "0"},
            "coverage": {
                "decode": "complete",
                "metadata": "partial",
                "simulation": "unavailable",
                "trace": "not_configured",
                "policy": "not_run"
            },
            "unknowns": ["trace unavailable"],
            "warnings": ["source unverified"],
            "sources": []
        });
        let wrapped = wrap_success("transaction.inspect", value);
        assert_eq!(wrapped["coverage"]["dimensions"]["decode"], "complete");
        assert_eq!(wrapped["coverage"]["dimensions"]["metadata"], "partial");
        assert_eq!(
            wrapped["coverage"]["dimensions"]["simulation"],
            "unavailable"
        );
        assert_eq!(wrapped["coverage"]["dimensions"]["trace"], "not_configured");
        assert_eq!(wrapped["coverage"]["dimensions"]["policy"], "not_run");
        assert_eq!(wrapped["unknowns"][0]["code"], "UNKNOWN");
        assert_eq!(wrapped["warnings"][0]["code"], "WARNING");
    }

    #[test]
    fn transaction_simulate_gets_an_explicit_simulation_dimension() {
        let value = json!({
            "chain": "ethereum",
            "chain_id": 1,
            "block_number": 9,
            "block_hash": "0xabc",
            "to": "0x0000000000000000000000000000000000000001",
            "value": {"raw": "0", "decimal": "0"},
            "success": true,
            "return_data": "0x"
        });
        let wrapped = wrap_success("transaction.simulate", value);
        assert_eq!(wrapped["coverage"]["dimensions"]["simulation"], "complete");
        assert!(wrapped["data"].get("block_number").is_none());
    }

    #[test]
    fn wraps_fourmeme_payload() {
        let value = json!({
            "chain": "bsc",
            "mode": "binance_agent_id_exclusive",
            "scanned": 1,
            "lookback_blocks": 100000,
            "from_block": 1,
            "to_block": 100001,
            "tokens": [],
            "coverage": "onchain",
            "sources": [],
            "warnings": []
        });
        let wrapped = wrap_success("fourmeme.agent-tokens", value);
        assert_eq!(wrapped["meta"]["capability"], "fourmeme.agent-tokens");
        assert_eq!(wrapped["chain"]["name"], "bsc");
        assert_eq!(wrapped["chain"]["id"], 56);
        assert_eq!(wrapped["coverage"]["dimensions"]["data"], "complete");
        assert_eq!(wrapped["data"]["mode"], "binance_agent_id_exclusive");
        assert!(wrapped["data"].get("chain").is_none());
        assert!(wrapped["data"].get("coverage").is_none());
    }

    #[test]
    fn wraps_allowance_payload() {
        let value = json!({
            "chain": "ethereum",
            "chain_id": 1,
            "owner": "0x0000000000000000000000000000000000000001",
            "spender": "0x0000000000000000000000000000000000000002",
            "block_number": 7,
            "block_hash": "0xdef",
            "allowances": [],
            "coverage": "local",
            "sources": [],
            "warnings": []
        });
        let wrapped = wrap_success("wallet.allowance", value);
        assert_eq!(wrapped["meta"]["capability"], "wallet.allowance");
        assert_eq!(wrapped["chain"]["name"], "ethereum");
        assert_eq!(wrapped["chain"]["id"], 1);
        assert_eq!(wrapped["coverage"]["dimensions"]["data"], "complete");
        assert!(wrapped["data"].get("owner").is_some());
        assert!(wrapped["data"].get("chain").is_none());
    }

    #[test]
    fn wraps_errors_with_the_same_chain_context() {
        let wrapped = wrap_error(
            "wallet.balance",
            CapabilityError::new(CapabilityErrorCode::RpcError, "rpc failed", true),
        );
        assert_eq!(wrapped["meta"]["schema_version"], 2);
        assert_eq!(wrapped["chain"]["name"], "bsc");
        assert_eq!(wrapped["chain"]["id"], 56);
        assert_eq!(wrapped["error"]["code"], "RPC_ERROR");
        assert_eq!(wrapped["error"]["retryable"], true);
    }

    #[test]
    fn v1_capabilities_keep_their_error_shape() {
        let wrapped = serde_json::to_value(CapabilityErrorResponse::new(
            "wallet.agent-id",
            "req_1",
            CapabilityError::new(CapabilityErrorCode::RpcError, "rpc failed", true),
        ))
        .unwrap();
        assert_eq!(wrapped["schema_version"], 1);
        assert!(wrapped.get("chain").is_none());
    }
}
