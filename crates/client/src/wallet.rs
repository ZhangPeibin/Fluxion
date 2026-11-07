//! Local read-only wallet balances and asset discovery.
//!
//! `wallet.balance` is local-only: explicit tokens are resolved and queried
//! against one pinned block. `wallet.assets` may use the 1inch Balance API to
//! discover candidate tokens, but every returned balance is re-read locally.

use std::collections::{HashMap, HashSet};

use alloy_primitives::{Address, B256, U256};
use serde_json::{Value, json};

use crate::capability::{Amount, CapabilityError, TokenRef};
use crate::token;

pub use fluxion_protocol::wallet::{AssetSource, WalletBalancesResponse, WalletTokenBalance};

const DEFAULT_RPC: &str = "https://eth.drpc.org";
const DEFAULT_ONEINCH_BALANCE: &str = "https://api.1inch.com/balance/v1.2";
const NATIVE: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const BALANCE_OF: &str = "70a08231";
const DECIMALS: &str = "313ce567";
const SYMBOL: &str = "95d89b41";

#[derive(Debug, Clone)]
pub struct WalletConfig {
    pub rpc_url: String,
    pub oneinch_api_key: Option<String>,
    pub oneinch_balance_url: String,
}

impl WalletConfig {
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            oneinch_api_key: std::env::var("ONEINCH_API_KEY")
                .ok()
                .filter(|key| !key.trim().is_empty()),
            oneinch_balance_url: std::env::var("FLUXION_ONEINCH_BALANCE_URL")
                .unwrap_or_else(|_| DEFAULT_ONEINCH_BALANCE.to_string()),
        }
    }

    pub fn from_env() -> Self {
        Self::new(
            std::env::var("FLUXION_RPC_URL")
                .or_else(|_| std::env::var("RPC_URL"))
                .unwrap_or_else(|_| DEFAULT_RPC.to_string()),
        )
    }
}

#[derive(Debug, Clone)]
pub struct BalanceRequest {
    pub address: Address,
    /// Symbols or addresses. Empty means the built-in registry.
    pub tokens: Vec<String>,
    pub include_native: bool,
    pub block: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct AssetsRequest {
    pub address: Address,
    pub include_native: bool,
    pub include_unknown: bool,
    pub block: Option<u64>,
}

pub async fn balance(
    config: &WalletConfig,
    request: BalanceRequest,
) -> Result<WalletBalancesResponse, CapabilityError> {
    let rpc = Rpc::new(&config.rpc_url);
    let (block_number, block_hash, tag) = rpc.block(request.block).await?;
    let mut tokens = if request.tokens.is_empty() {
        registry_tokens()
    } else {
        resolve_tokens(&rpc, &request.tokens, &tag).await?
    };
    dedupe_tokens(&mut tokens);

    let mut sources = vec![rpc_source(block_number)];
    let warnings = Vec::new();
    let (native, mut balances) =
        verify_tokens(&rpc, request.address, request.include_native, tokens, &tag).await?;
    sort_balances(&mut balances);
    Ok(WalletBalancesResponse {
        address: request.address,
        block_number,
        block_hash,
        native,
        tokens: balances,
        unknown_count: 0,
        coverage: "local".to_string(),
        sources: std::mem::take(&mut sources),
        warnings,
    })
}

pub async fn assets(
    config: &WalletConfig,
    request: AssetsRequest,
) -> Result<WalletBalancesResponse, CapabilityError> {
    let rpc = Rpc::new(&config.rpc_url);
    let (block_number, block_hash, tag) = rpc.block(request.block).await?;
    let mut warnings = Vec::new();
    let mut sources = vec![rpc_source(block_number)];

    let discovery = discover_oneinch(config, request.address).await;
    let (mut known, unknown, native_seen, coverage) = match discovery {
        Ok((known, unknown, native_seen)) => {
            sources.push(AssetSource {
                name: "1inch-balance".to_string(),
                scope: "candidate-discovery".to_string(),
                block: None,
            });
            (known, unknown, native_seen, "provider+local")
        }
        Err(error) => {
            warnings.push(format!(
                "asset discovery unavailable; returned built-in registry tokens only: {}",
                error.message
            ));
            (registry_tokens(), HashMap::new(), None, "registry+local")
        }
    };
    dedupe_tokens(&mut known);

    let mut unknown_count = unknown.len();
    if request.include_unknown && !unknown.is_empty() {
        let addresses = unknown.keys().copied().collect::<Vec<_>>();
        let resolved = resolve_unknown_tokens(&rpc, &addresses, &tag).await;
        for (address, token) in resolved {
            match token {
                Ok(token) => known.push(token),
                Err(error) => warnings.push(format!(
                    "skipped unknown token {address}: {}",
                    error.message
                )),
            }
        }
        unknown_count = 0;
    }
    let _ = native_seen; // The provider only identifies candidates; native is re-read locally.
    let (native, mut balances) =
        verify_tokens(&rpc, request.address, request.include_native, known, &tag).await?;
    sort_balances(&mut balances);
    Ok(WalletBalancesResponse {
        address: request.address,
        block_number,
        block_hash,
        native,
        tokens: balances,
        unknown_count,
        coverage: coverage.to_string(),
        sources,
        warnings,
    })
}

fn rpc_source(block: u64) -> AssetSource {
    AssetSource {
        name: "ethereum-rpc".to_string(),
        scope: "verified-balances".to_string(),
        block: Some(block),
    }
}

fn registry_tokens() -> Vec<TokenRef> {
    token::list()
        .iter()
        .filter_map(|token| {
            token.address.parse::<Address>().ok().map(|address| {
                TokenRef::new(Some(token.symbol.to_string()), address, token.decimals)
            })
        })
        .collect()
}

fn known_token(address: Address) -> Option<TokenRef> {
    token::list().iter().find_map(|token| {
        (token.address.parse::<Address>().ok() == Some(address))
            .then(|| TokenRef::new(Some(token.symbol.to_string()), address, token.decimals))
    })
}

fn dedupe_tokens(tokens: &mut Vec<TokenRef>) {
    let mut seen = HashSet::new();
    tokens.retain(|token| seen.insert(token.address));
}

async fn resolve_tokens(
    rpc: &Rpc,
    inputs: &[String],
    tag: &str,
) -> Result<Vec<TokenRef>, CapabilityError> {
    let mut tokens = Vec::new();
    for input in inputs {
        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        if input.eq_ignore_ascii_case("ETH") {
            continue; // Native is controlled by include_native.
        }
        if input.starts_with("0x") || input.starts_with("0X") {
            let address = input.parse::<Address>().map_err(|_| {
                CapabilityError::invalid_argument(format!("invalid token address: {input}"))
            })?;
            if let Some(known) = known_token(address) {
                tokens.push(known);
            } else {
                let (decimals, symbol) = token_metadata(rpc, address, tag).await?;
                tokens.push(TokenRef::new(symbol, address, decimals));
            }
        } else {
            let known = token::resolve(input, None)?;
            tokens.push(known);
        }
    }
    Ok(tokens)
}

async fn resolve_unknown_tokens(
    rpc: &Rpc,
    addresses: &[Address],
    tag: &str,
) -> Vec<(Address, Result<TokenRef, CapabilityError>)> {
    let mut out = Vec::new();
    for &address in addresses {
        let result = token_metadata(rpc, address, tag)
            .await
            .map(|(decimals, symbol)| TokenRef::new(symbol, address, decimals));
        out.push((address, result));
    }
    out
}

async fn token_metadata(
    rpc: &Rpc,
    address: Address,
    tag: &str,
) -> Result<(u8, Option<String>), CapabilityError> {
    let data = rpc.eth_call(address, DECIMALS, tag).await?;
    let decimals = decode_uint(&data).ok_or_else(|| {
        CapabilityError::new(
            crate::capability::CapabilityErrorCode::RpcError,
            format!("cannot read decimals for token {address}"),
            false,
        )
    })?;
    if decimals > U256::from(u8::MAX) {
        return Err(CapabilityError::invalid_argument(format!(
            "token {address} has invalid decimals"
        )));
    }
    let symbol = rpc
        .eth_call(address, SYMBOL, tag)
        .await
        .ok()
        .and_then(|data| decode_string(&data));
    Ok((decimals.to::<u8>(), symbol))
}

async fn verify_tokens(
    rpc: &Rpc,
    address: Address,
    include_native: bool,
    tokens: Vec<TokenRef>,
    tag: &str,
) -> Result<(Option<WalletTokenBalance>, Vec<WalletTokenBalance>), CapabilityError> {
    let mut calls = Vec::with_capacity(tokens.len() + usize::from(include_native));
    if include_native {
        calls.push(("eth_getBalance".to_string(), json!([address, tag])));
    }
    for token in &tokens {
        calls.push((
            "eth_call".to_string(),
            json!([{ "to": token.address, "data": format!("0x{BALANCE_OF}{:0>64}", hex_address(address)) }, tag]),
        ));
    }
    let results = rpc.batch_or_individual(calls).await?;
    let mut index = 0;
    let native = if include_native {
        let result = results.get(index).cloned().transpose()?;
        index += 1;
        result
            .and_then(|value| value.as_str().and_then(decode_hex_u256))
            .map(|raw| WalletTokenBalance {
                token: TokenRef::new(Some("ETH".to_string()), native_address(), 18),
                amount: Amount::from_raw(raw, 18),
                known: true,
            })
    } else {
        None
    };
    let mut balances = Vec::new();
    for token in tokens {
        let result = results.get(index).cloned().transpose()?;
        index += 1;
        let Some(raw) = result.and_then(|value| value.as_str().and_then(decode_hex_u256)) else {
            continue;
        };
        if raw.is_zero() {
            continue;
        }
        balances.push(WalletTokenBalance {
            known: known_token(token.address).is_some(),
            amount: Amount::from_raw(raw, token.decimals),
            token,
        });
    }
    Ok((native, balances))
}

fn sort_balances(balances: &mut [WalletTokenBalance]) {
    balances.sort_by(|a, b| {
        b.known
            .cmp(&a.known)
            .then_with(|| a.token.symbol.cmp(&b.token.symbol))
            .then_with(|| a.token.address.cmp(&b.token.address))
    });
}

fn native_address() -> Address {
    NATIVE.parse().unwrap()
}

fn hex_address(address: Address) -> String {
    let text = format!("{address:#x}");
    text.trim_start_matches("0x").to_string()
}

async fn discover_oneinch(
    config: &WalletConfig,
    address: Address,
) -> Result<(Vec<TokenRef>, HashMap<Address, U256>, Option<U256>), CapabilityError> {
    let key = config.oneinch_api_key.as_deref().ok_or_else(|| {
        CapabilityError::new(
            crate::capability::CapabilityErrorCode::Unsupported,
            "ONEINCH_API_KEY is not configured",
            false,
        )
    })?;
    let url = format!(
        "{}/1/balances/{}",
        config.oneinch_balance_url.trim_end_matches('/'),
        address
    );
    let response = reqwest::Client::new()
        .get(url)
        .bearer_auth(key)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|error| {
            CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                format!("1inch balance request failed: {error}"),
                true,
            )
        })?;
    if !response.status().is_success() {
        return Err(CapabilityError::new(
            crate::capability::CapabilityErrorCode::RpcError,
            format!("1inch balance returned HTTP {}", response.status()),
            true,
        ));
    }
    let values: HashMap<String, String> = response.json().await.map_err(|error| {
        CapabilityError::new(
            crate::capability::CapabilityErrorCode::RpcError,
            format!("1inch balance response was not a balance map: {error}"),
            true,
        )
    })?;
    let mut known = Vec::new();
    let mut unknown = HashMap::new();
    let mut native = None;
    for (raw_address, raw_balance) in values {
        let Ok(balance) = U256::from_str_radix(&raw_balance, 10) else {
            continue;
        };
        if balance.is_zero() {
            continue;
        }
        let Ok(token_address) = raw_address.parse::<Address>() else {
            continue;
        };
        if raw_address.eq_ignore_ascii_case(NATIVE) {
            native = Some(balance);
        } else if let Some(token) = known_token(token_address) {
            known.push(token);
        } else {
            unknown.insert(token_address, balance);
        }
    }
    Ok((known, unknown, native))
}

fn decode_hex_u256(value: &str) -> Option<U256> {
    U256::from_str_radix(value.trim_start_matches("0x"), 16).ok()
}

fn decode_uint(bytes: &[u8]) -> Option<U256> {
    (bytes.len() >= 32).then(|| U256::from_be_slice(&bytes[..32]))
}

fn decode_string(bytes: &[u8]) -> Option<String> {
    if bytes.len() == 32 {
        let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(32);
        return String::from_utf8(bytes[..end].to_vec()).ok();
    }
    if bytes.len() < 64 {
        return None;
    }
    let offset = U256::from_be_slice(&bytes[..32]).to::<usize>();
    let length = U256::from_be_slice(bytes.get(offset..offset + 32)?).to::<usize>();
    let data = bytes.get(offset + 32..offset + 32 + length)?;
    String::from_utf8(data.to_vec()).ok()
}

struct Rpc {
    client: reqwest::Client,
    url: String,
}

impl Rpc {
    fn new(url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.to_string(),
        }
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, CapabilityError> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(rpc_error)?;
        let value: Value = response.json().await.map_err(rpc_error)?;
        if let Some(error) = value.get("error") {
            return Err(CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                error.to_string(),
                true,
            ));
        }
        value.get("result").cloned().ok_or_else(|| {
            CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                "JSON-RPC response has no result",
                true,
            )
        })
    }

    async fn batch(
        &self,
        calls: Vec<(String, Value)>,
    ) -> Result<Vec<Result<Value, CapabilityError>>, CapabilityError> {
        if calls.is_empty() {
            return Ok(Vec::new());
        }
        let requests = calls
            .into_iter()
            .enumerate()
            .map(|(idx, (method, params))| {
                json!({
                    "jsonrpc": "2.0",
                    "id": idx,
                    "method": method,
                    "params": params,
                })
            })
            .collect::<Vec<_>>();
        let response = self
            .client
            .post(&self.url)
            .json(&requests)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(rpc_error)?;
        let value: Value = response.json().await.map_err(rpc_error)?;
        let items = value.as_array().ok_or_else(|| {
            CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                "JSON-RPC batch response was not an array",
                true,
            )
        })?;
        let mut results = vec![None; requests.len()];
        for item in items {
            let Some(index) = item.get("id").and_then(Value::as_u64).map(|id| id as usize) else {
                continue;
            };
            if index >= results.len() {
                continue;
            }
            let result = if let Some(error) = item.get("error") {
                Err(CapabilityError::new(
                    crate::capability::CapabilityErrorCode::RpcError,
                    error.to_string(),
                    true,
                ))
            } else {
                item.get("result").cloned().ok_or_else(|| {
                    CapabilityError::new(
                        crate::capability::CapabilityErrorCode::RpcError,
                        "JSON-RPC batch item has no result",
                        true,
                    )
                })
            };
            results[index] = Some(result);
        }
        Ok(results
            .into_iter()
            .map(|result| {
                result.unwrap_or_else(|| {
                    Err(CapabilityError::new(
                        crate::capability::CapabilityErrorCode::RpcError,
                        "JSON-RPC batch response missing item",
                        true,
                    ))
                })
            })
            .collect())
    }

    async fn batch_or_individual(
        &self,
        calls: Vec<(String, Value)>,
    ) -> Result<Vec<Result<Value, CapabilityError>>, CapabilityError> {
        match self.batch(calls.clone()).await {
            Ok(results) if results.iter().all(Result::is_ok) => Ok(results),
            _ => {
                let mut results = Vec::with_capacity(calls.len());
                for (method, params) in calls {
                    results.push(self.call(&method, params).await);
                }
                Ok(results)
            }
        }
    }

    async fn eth_call(
        &self,
        to: Address,
        selector: &str,
        tag: &str,
    ) -> Result<Vec<u8>, CapabilityError> {
        let value = self
            .call(
                "eth_call",
                json!([{ "to": to, "data": format!("0x{selector}") }, tag]),
            )
            .await?;
        let Some(text) = value.as_str() else {
            return Err(CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                "eth_call returned a non-string result",
                true,
            ));
        };
        let text = text.trim_start_matches("0x");
        if !text.len().is_multiple_of(2) {
            return Err(CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                "eth_call returned invalid hex",
                true,
            ));
        }
        let mut bytes = Vec::with_capacity(text.len() / 2);
        for index in (0..text.len()).step_by(2) {
            let byte = u8::from_str_radix(&text[index..index + 2], 16).map_err(|_| {
                CapabilityError::new(
                    crate::capability::CapabilityErrorCode::RpcError,
                    "eth_call returned invalid hex",
                    true,
                )
            })?;
            bytes.push(byte);
        }
        Ok(bytes)
    }

    async fn block(&self, requested: Option<u64>) -> Result<(u64, B256, String), CapabilityError> {
        let number = match requested {
            Some(number) => number,
            None => {
                let value = self.call("eth_blockNumber", json!([])).await?;
                value
                    .as_str()
                    .and_then(decode_hex_u256)
                    .and_then(|value| u64::try_from(value).ok())
                    .ok_or_else(|| {
                        CapabilityError::new(
                            crate::capability::CapabilityErrorCode::RpcError,
                            "eth_blockNumber returned an invalid block",
                            true,
                        )
                    })?
            }
        };
        let tag = format!("0x{number:x}");
        let value = self
            .call("eth_getBlockByNumber", json!([tag, false]))
            .await?;
        let hash = value
            .get("hash")
            .and_then(Value::as_str)
            .and_then(|hash| hash.parse::<B256>().ok())
            .ok_or_else(|| {
                CapabilityError::new(
                    crate::capability::CapabilityErrorCode::RpcError,
                    "cannot read block hash",
                    true,
                )
            })?;
        Ok((number, hash, tag))
    }
}

fn rpc_error(error: reqwest::Error) -> CapabilityError {
    CapabilityError::new(
        crate::capability::CapabilityErrorCode::RpcError,
        format!("local RPC request failed: {error}"),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_erc20_string_and_bytes32_symbols() {
        let mut abi = vec![0u8; 64];
        abi[31] = 32;
        abi[63] = 4;
        abi.extend_from_slice(b"USDC");
        assert_eq!(decode_string(&abi).as_deref(), Some("USDC"));

        let mut bytes32 = vec![0u8; 32];
        bytes32[..3].copy_from_slice(b"DAI");
        assert_eq!(decode_string(&bytes32).as_deref(), Some("DAI"));
    }

    #[test]
    fn parses_decimal_balance_map_without_float() {
        assert_eq!(
            decode_hex_u256("0xde0b6b3a7640000"),
            Some(U256::from(1_000_000_000_000_000_000u128))
        );
        assert_eq!(decode_hex_u256("0x0"), Some(U256::ZERO));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use axum::extract::Json;
    use axum::routing::{get, post};
    use axum::{Router, http::StatusCode};
    use serde_json::json;

    const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    const LDO: &str = "0x5a98fcbea516cf06857215779fd812ca3bef1b32";
    const UNKNOWN: &str = "0x0000000000000000000000000000000000000003";
    const NATIVE: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

    fn word(value: U256) -> String {
        format!("0x{value:064x}")
    }

    fn abi_string(value: &str) -> String {
        let mut bytes = Vec::new();
        let mut word = [0u8; 32];
        word[31] = 32;
        bytes.extend_from_slice(&word);
        let mut length = [0u8; 32];
        let len = value.len();
        length[24..].copy_from_slice(&(len as u64).to_be_bytes());
        bytes.extend_from_slice(&length);
        let mut data = vec![0u8; 32];
        data[..len].copy_from_slice(value.as_bytes());
        bytes.extend_from_slice(&data);
        format!(
            "0x{}",
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }

    async fn rpc(Json(body): Json<Value>) -> Json<Value> {
        let is_batch = body.is_array();
        let requests = body.as_array().cloned().unwrap_or_else(|| vec![body]);
        let items = requests
            .iter()
            .map(|request| {
                let id = request.get("id").cloned().unwrap_or(Value::Null);
                let method = request["method"].as_str().unwrap_or_default();
                let result = match method {
                    "eth_blockNumber" => json!("0x64"),
                    "eth_getBlockByNumber" => json!({
                        "number": "0x64",
                        "hash": "0x0000000000000000000000000000000000000000000000000000000000000064"
                    }),
                    "eth_getBalance" => json!(word(U256::from(1_000_000_000_000_000_000u128))),
                    "eth_call" => {
                        let call = &request["params"][0];
                        let to = call["to"].as_str().unwrap_or_default().to_ascii_lowercase();
                        let data = call["data"].as_str().unwrap_or_default();
                        if data.starts_with("0x313ce567") {
                            if to == UNKNOWN.to_ascii_lowercase() {
                                json!(word(U256::from(18u64)))
                            } else if to == USDC.to_ascii_lowercase() {
                                json!(word(U256::from(6u64)))
                            } else {
                                json!(word(U256::from(18u64)))
                            }
                        } else if data.starts_with("0x95d89b41") {
                            if to == UNKNOWN.to_ascii_lowercase() {
                                json!(abi_string("TEST"))
                            } else if to == USDC.to_ascii_lowercase() {
                                json!(abi_string("USDC"))
                            } else {
                                json!(abi_string("TOKEN"))
                            }
                        } else if data.starts_with("0x70a08231") {
                            let amount = if to == USDC.to_ascii_lowercase() {
                                U256::from(123_456_789u64)
                            } else if to == LDO.to_ascii_lowercase() {
                                U256::from(5_000_000_000_000_000_000u128)
                            } else if to == UNKNOWN.to_ascii_lowercase() {
                                U256::from(7_000_000_000_000_000_000u128)
                            } else {
                                U256::ZERO
                            };
                            json!(word(amount))
                        } else {
                            json!("0x")
                        }
                    }
                    _ => json!("0x0"),
                };
                json!({"jsonrpc": "2.0", "id": id, "result": result})
            })
            .collect::<Vec<_>>();
        if is_batch {
            Json(Value::Array(items))
        } else {
            Json(items.into_iter().next().unwrap_or(Value::Null))
        }
    }

    async fn oneinch() -> Json<Value> {
        let mut map = serde_json::Map::new();
        map.insert(USDC.to_string(), json!("123456789"));
        map.insert(LDO.to_string(), json!("5000000000000000000"));
        map.insert(UNKNOWN.to_string(), json!("7000000000000000000"));
        map.insert(
            "0x0000000000000000000000000000000000000004".to_string(),
            json!("0"),
        );
        map.insert(NATIVE.to_string(), json!("1000000000000000000"));
        Json(Value::Object(map))
    }

    async fn unavailable() -> (StatusCode, Json<Value>) {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "unavailable"})),
        )
    }

    async fn test_server(provider_ok: bool) -> String {
        let app = Router::new().route("/rpc", post(rpc)).route(
            "/balance/v1.2/1/balances/{address}",
            if provider_ok {
                get(oneinch)
            } else {
                get(|| async { unavailable().await })
            },
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    fn config(base: &str) -> WalletConfig {
        WalletConfig {
            rpc_url: format!("{base}/rpc"),
            oneinch_api_key: Some("test".to_string()),
            oneinch_balance_url: format!("{base}/balance/v1.2"),
        }
    }

    #[tokio::test]
    async fn balance_reads_native_known_and_unknown_tokens() {
        let base = test_server(true).await;
        let response = balance(
            &config(&base),
            BalanceRequest {
                address: "0x0000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
                tokens: vec![USDC.to_string(), UNKNOWN.to_string()],
                include_native: true,
                block: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.coverage, "local");
        assert_eq!(response.native.as_ref().unwrap().amount.decimal, "1");
        assert_eq!(response.tokens.len(), 2);
        let usdc = response
            .tokens
            .iter()
            .find(|token| token.token.address.to_string().eq_ignore_ascii_case(USDC))
            .unwrap();
        assert_eq!(usdc.amount.decimal, "123.456789");
        assert!(usdc.known);
        let unknown = response
            .tokens
            .iter()
            .find(|token| {
                token
                    .token
                    .address
                    .to_string()
                    .eq_ignore_ascii_case(UNKNOWN)
            })
            .unwrap();
        assert_eq!(unknown.token.symbol.as_deref(), Some("TEST"));
        assert_eq!(unknown.amount.decimal, "7");
        assert!(!unknown.known);
    }

    #[tokio::test]
    async fn assets_uses_provider_then_verifies_locally() {
        let base = test_server(true).await;
        let response = assets(
            &config(&base),
            AssetsRequest {
                address: "0x0000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
                include_native: true,
                include_unknown: false,
                block: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            response.coverage, "provider+local",
            "{:?}",
            response.warnings
        );
        assert_eq!(response.unknown_count, 1);
        assert_eq!(response.native.as_ref().unwrap().amount.decimal, "1");
        assert_eq!(response.tokens.len(), 2);
        assert!(
            response
                .tokens
                .iter()
                .all(|token| token.token.address.to_string() != UNKNOWN)
        );
    }

    #[tokio::test]
    async fn assets_can_expand_unknown_tokens_and_falls_back() {
        let base = test_server(true).await;
        let response = assets(
            &config(&base),
            AssetsRequest {
                address: "0x0000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
                include_native: false,
                include_unknown: true,
                block: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.unknown_count, 0);
        assert!(response.tokens.iter().any(|token| {
            token
                .token
                .address
                .to_string()
                .eq_ignore_ascii_case(UNKNOWN)
        }));

        let fallback_base = test_server(false).await;
        // The RPC server is still available; only the provider route fails.
        let fallback = assets(
            &config(&fallback_base),
            AssetsRequest {
                address: "0x0000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
                include_native: false,
                include_unknown: false,
                block: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(fallback.coverage, "registry+local");
        assert!(!fallback.warnings.is_empty());
    }
}
