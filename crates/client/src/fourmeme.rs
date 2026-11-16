//! Four.meme Binance Agent ID Exclusive token discovery.
//!
//! Discovery scans `TokenCreated` logs from OpenFour Core and filters on the
//! Agent ID preset. The Four.meme API is optional enrichment only. Read-only.

use std::collections::BTreeSet;
use std::time::Duration;

use alloy_primitives::{Address, U256};
use data_encoding::HEXLOWER;
use serde_json::{Value, json};

use crate::capability::{CapabilityError, CapabilityErrorCode};
use crate::transaction::Rpc;
use fluxion_protocol::fourmeme::{FourMemeAgentTokensResponse, FourMemeToken};
use fluxion_protocol::wallet::AssetSource;

const DEFAULT_API_URL: &str = "https://four.meme/meme-api/v1";
const DEFAULT_BSC_RPC_URL: &str = "https://bsc-rpc.publicnode.com";
const OPENFOUR_CORE: Address =
    alloy_primitives::address!("ebe7b6C1089D9F72aD07f34E36d898e44E5e27f3");
const TOKEN_CREATED_TOPIC: &str =
    "0x7dae4dcd1b20eb0445e7cc1d02b3e2e7f32fa63081a0e764732c269f2b8f228f";
const AGENT_PRESET_TOPIC: &str =
    "0x0000000000000000000000000000000000000000000000000000019dfab455f7";
const AGENT_TEMPLATE_ID: &str = "1778027615735";
const AGENT_IDENTIFIER: Address =
    alloy_primitives::address!("09B44A633de9F9EBF6FB9Bdd5b5629d3DD2cef13");
const IS_AGENT_SELECTOR: &str = "0x1ffbb064";
const DEFAULT_LOOKBACK_BLOCKS: u64 = 100_000;
const LOG_CHUNK_BLOCKS: u64 = 5_000;

#[derive(Debug, Clone)]
pub struct FourMemeConfig {
    pub api_url: String,
    pub bsc_rpc_url: String,
}

impl FourMemeConfig {
    pub fn from_env() -> Self {
        Self {
            api_url: std::env::var("FOURMEME_API_URL")
                .unwrap_or_else(|_| DEFAULT_API_URL.to_string()),
            bsc_rpc_url: std::env::var("FOURMEME_BSC_RPC_URL")
                .unwrap_or_else(|_| DEFAULT_BSC_RPC_URL.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ListRequest {
    pub wallet: Option<Address>,
    pub limit: usize,
    pub lookback_blocks: u64,
}

impl Default for ListRequest {
    fn default() -> Self {
        Self {
            wallet: None,
            limit: 20,
            lookback_blocks: DEFAULT_LOOKBACK_BLOCKS,
        }
    }
}

#[derive(Debug, Clone)]
struct CreatedToken {
    address: Address,
    name: String,
    symbol: String,
    block: u64,
}

pub async fn agent_tokens(
    config: &FourMemeConfig,
    request: ListRequest,
) -> Result<FourMemeAgentTokensResponse, CapabilityError> {
    let wallet = request.wallet;
    let rpc = Rpc::new(&config.bsc_rpc_url);
    let (latest, _, _) = rpc.block(None).await?;
    let from_block = latest.saturating_sub(request.lookback_blocks.saturating_sub(1));
    let mut warnings = Vec::new();
    let mut created = Vec::new();
    let mut seen = BTreeSet::new();
    let mut chunks = 0usize;
    let mut failed_chunks = 0usize;
    let mut end = latest;

    loop {
        let start = end.saturating_sub(LOG_CHUNK_BLOCKS - 1).max(from_block);
        chunks += 1;
        let filter = json!({
            "fromBlock": format!("0x{start:x}"),
            "toBlock": format!("0x{end:x}"),
            "address": OPENFOUR_CORE,
            "topics": [TOKEN_CREATED_TOPIC, AGENT_PRESET_TOPIC]
        });
        match rpc.get_logs(filter).await {
            Ok(logs) => {
                for log in logs {
                    let Some(token) = decode_created(&log) else {
                        warnings.push("ignored an unreadable TokenCreated log".to_string());
                        continue;
                    };
                    if seen.insert(token.address) {
                        created.push(token);
                    }
                }
            }
            Err(error) => {
                failed_chunks += 1;
                warnings.push(format!(
                    "BSC log range {start}-{end} unavailable: {}",
                    error.message
                ));
            }
        }
        if start == from_block {
            break;
        }
        end = start.saturating_sub(1);
    }

    if failed_chunks == chunks {
        return Err(api_error(format!(
            "BSC log discovery failed; set FOURMEME_BSC_RPC_URL to an RPC that supports eth_getLogs: {}",
            warnings.join("; ")
        )));
    }

    created.sort_by(|left, right| {
        right
            .block
            .cmp(&left.block)
            .then_with(|| right.address.cmp(&left.address))
    });
    let scanned = created.len();
    created.truncate(request.limit);

    let mut tokens = Vec::with_capacity(created.len());
    let mut details_used = false;
    let mut details_disabled = false;
    let mut onchain_used = false;
    let mut wallet_agent: Option<Option<bool>> = None;

    for created in created {
        let detail = if details_disabled {
            None
        } else {
            match get_detail(config, created.address).await {
                Ok(detail) => {
                    details_used = true;
                    if template_id(&detail).as_deref() != Some(AGENT_TEMPLATE_ID) {
                        warnings.push(format!(
                            "{} detail does not match Agent preset; using log metadata",
                            created.address
                        ));
                    }
                    Some(detail)
                }
                Err(error) if error.message.contains("Too many requests") => {
                    details_disabled = true;
                    warnings.push(format!(
                        "Four.meme detail API rate-limited; remaining tokens use log metadata: {}",
                        error.message
                    ));
                    None
                }
                Err(error) => {
                    warnings.push(format!(
                        "token detail unavailable for {}: {}",
                        created.address, error.message
                    ));
                    None
                }
            }
        };
        let phase = detail
            .as_ref()
            .map(token_phase)
            .unwrap_or("unknown")
            .to_string();
        let (wallet_eligible, eligibility_reason) = if let Some(wallet) = wallet {
            if phase == "graduated" {
                (Some(true), Some("graduated_open_to_all".to_string()))
            } else if phase == "bonding" {
                onchain_used = true;
                if wallet_agent.is_none() {
                    wallet_agent = Some(match is_agent_wallet(&rpc, wallet).await {
                        Ok(value) => Some(value),
                        Err(error) => {
                            warnings
                                .push(format!("AgentIdentifier check failed: {}", error.message));
                            None
                        }
                    });
                }
                match wallet_agent.flatten() {
                    Some(true) => (Some(true), Some("agent_wallet".to_string())),
                    Some(false) => (Some(false), Some("not_agent_wallet".to_string())),
                    None => (None, Some("agent_identifier_unavailable".to_string())),
                }
            } else {
                (None, Some("token_phase_unknown".to_string()))
            }
        } else {
            (None, None)
        };
        let name = detail
            .as_ref()
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(&created.name)
            .to_string();
        let symbol = detail
            .as_ref()
            .and_then(|value| value.get("shortName"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(&created.symbol)
            .to_string();
        tokens.push(FourMemeToken {
            address: created.address,
            name,
            symbol,
            template_id: AGENT_TEMPLATE_ID.to_string(),
            phase,
            progress: detail
                .as_ref()
                .and_then(|value| value.get("progress"))
                .and_then(value_string),
            raised: detail
                .as_ref()
                .and_then(|value| value.get("raisedAmount"))
                .and_then(value_string),
            agent_id_exclusive: true,
            wallet_eligible,
            eligibility_reason,
        });
    }

    let mut coverage = vec!["onchain"];
    if details_used {
        coverage.push("api");
    }
    if onchain_used {
        coverage.push("agent-id");
    }
    let mut sources = vec![AssetSource {
        name: "bsc-logs".to_string(),
        scope: format!("token-created:{AGENT_TEMPLATE_ID}"),
        block: Some(latest),
    }];
    if details_used {
        sources.push(AssetSource {
            name: "four.meme-api".to_string(),
            scope: "token-detail".to_string(),
            block: None,
        });
    }
    if onchain_used {
        sources.push(AssetSource {
            name: "bsc-rpc".to_string(),
            scope: "agent-identifier".to_string(),
            block: Some(latest),
        });
    }

    Ok(FourMemeAgentTokensResponse {
        chain: "bsc".to_string(),
        mode: "binance_agent_id_exclusive".to_string(),
        scanned,
        lookback_blocks: request.lookback_blocks,
        from_block,
        to_block: latest,
        tokens,
        coverage: coverage.join("+"),
        sources,
        warnings,
    })
}

fn decode_created(log: &Value) -> Option<CreatedToken> {
    let address = topic_address(log.get("topics")?.as_array()?.get(3)?.as_str()?)?;
    let data = decode_hex(log.get("data")?.as_str()?)?;
    let name = word_usize(&data, 1)
        .and_then(|offset| string_at(&data, offset))
        .unwrap_or_default();
    let symbol = word_usize(&data, 2)
        .and_then(|offset| string_at(&data, offset))
        .unwrap_or_default();
    let block = log
        .get("blockNumber")
        .and_then(Value::as_str)
        .and_then(|value| u64::from_str_radix(value.trim_start_matches("0x"), 16).ok())
        .unwrap_or_default();
    Some(CreatedToken {
        address,
        name,
        symbol,
        block,
    })
}

fn topic_address(topic: &str) -> Option<Address> {
    let bytes = decode_hex(topic)?;
    Some(Address::from_slice(bytes.get(12..32)?))
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    HEXLOWER.decode(value.strip_prefix("0x")?.as_bytes()).ok()
}

fn word(data: &[u8], index: usize) -> Option<&[u8]> {
    data.get(index * 32..index * 32 + 32)
}

fn word_usize(data: &[u8], index: usize) -> Option<usize> {
    usize::try_from(U256::from_be_slice(word(data, index)?)).ok()
}

fn string_at(data: &[u8], offset: usize) -> Option<String> {
    let length = word_usize(data, offset / 32)?;
    let start = offset.checked_add(32)?;
    String::from_utf8(data.get(start..start.checked_add(length)?)?.to_vec()).ok()
}

async fn get_detail(config: &FourMemeConfig, address: Address) -> Result<Value, CapabilityError> {
    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "{}/private/token/get/v2",
            config.api_url.trim_end_matches('/')
        ))
        .query(&[("address", address.to_string())])
        .header("accept", "application/json")
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|error| api_error(format!("Four.meme token detail failed: {error}")))?
        .json::<Value>()
        .await
        .map_err(|error| {
            api_error(format!(
                "Four.meme token detail returned invalid JSON: {error}"
            ))
        })?;
    if response.get("code").and_then(Value::as_u64) != Some(0) {
        return Err(api_error(format!(
            "Four.meme token detail error: {}",
            response
                .get("msg")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        )));
    }
    response
        .get("data")
        .cloned()
        .ok_or_else(|| api_error("Four.meme token detail has no data"))
}

async fn is_agent_wallet(rpc: &Rpc, wallet: Address) -> Result<bool, CapabilityError> {
    let data = format!("{IS_AGENT_SELECTOR}{}", address_word(wallet));
    let result = rpc.call_hex(AGENT_IDENTIFIER, &data, "latest").await?;
    let value = result
        .strip_prefix("0x")
        .and_then(|value| U256::from_str_radix(value, 16).ok())
        .ok_or_else(|| api_error("AgentIdentifier returned an invalid bool"))?;
    Ok(!value.is_zero())
}

fn address_word(address: Address) -> String {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(address.as_slice());
    HEXLOWER.encode(&word)
}

fn template_id(detail: &Value) -> Option<String> {
    let value = detail.get("tokenTemplateVo")?.get("id")?;
    value
        .as_u64()
        .map(|id| id.to_string())
        .or_else(|| value.as_str().map(str::to_string))
}

fn token_phase(detail: &Value) -> &'static str {
    match detail.get("status").and_then(Value::as_str) {
        Some("PUBLISH" | "PUBLISHED") => "bonding",
        Some("TRADE") => "graduated",
        _ => "unknown",
    }
}

fn value_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_number().map(|number| number.to_string()))
}

fn api_error(message: impl Into<String>) -> CapabilityError {
    CapabilityError::new(CapabilityErrorCode::RpcError, message, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(value: u64) -> String {
        format!("{value:064x}")
    }

    fn string_word(value: &str) -> String {
        let mut bytes = value.as_bytes().to_vec();
        let length = bytes.len();
        while !bytes.len().is_multiple_of(32) {
            bytes.push(0);
        }
        format!("{}{}", word(length as u64), HEXLOWER.encode(&bytes))
    }

    #[test]
    fn decodes_token_created_log() {
        let data = format!(
            "0x{}{}{}{}{}",
            word(1),
            word(0x60),
            word(0xa0),
            string_word("4AG"),
            string_word("4AGENT")
        );
        let log = json!({
            "topics": [
                TOKEN_CREATED_TOPIC,
                AGENT_PRESET_TOPIC,
                format!("0x{}", word(0x1234)),
                format!("0x{}{}", "0".repeat(24), "0000000000000000000000000000000000000010")
            ],
            "data": data,
            "blockNumber": "0x1234"
        });
        let token = decode_created(&log).unwrap();
        assert_eq!(token.address, Address::with_last_byte(0x10));
        assert_eq!(token.name, "4AG");
        assert_eq!(token.symbol, "4AGENT");
        assert_eq!(token.block, 0x1234);
    }

    #[test]
    fn parses_template_ids_and_phases() {
        assert_eq!(
            template_id(&json!({"tokenTemplateVo": {"id": 1778027615735u64}})).as_deref(),
            Some(AGENT_TEMPLATE_ID)
        );
        assert_eq!(
            template_id(&json!({"tokenTemplateVo": {"id": "1778027615735"}})).as_deref(),
            Some(AGENT_TEMPLATE_ID)
        );
        assert!(template_id(&json!({"tokenTemplateVo": {}})).is_none());
        assert_eq!(token_phase(&json!({"status": "PUBLISH"})), "bonding");
        assert_eq!(token_phase(&json!({"status": "PUBLISHED"})), "bonding");
        assert_eq!(token_phase(&json!({"status": "TRADE"})), "graduated");
        assert_eq!(token_phase(&json!({"status": "OTHER"})), "unknown");
    }
}
