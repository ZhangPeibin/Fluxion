//! Minimal MCP stdio server exposing the `swap_quote` tool.
//!
//! The MCP adapter is the agent-facing transport. It forwards to Fluxion's
//! internal execution backend (hosted service or local runtime), so repeated
//! tool calls do not pay discovery or snapshot warmup.

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::agent_id::{self, AgentIdConfig, AgentIdLookup, AgentIdRequest};
use crate::capability::{CapabilityError, CapabilityErrorResponse};
use crate::client::{RuntimeClient, request_id};
use crate::swap::quote::QuoteRequest;
use crate::token;
use crate::transaction::{self, SimulateRequest};
use crate::wallet::{
    self, AllowanceRequest, AssetsRequest, BalanceRequest, CollectiblesRequest, WalletConfig,
};
use crate::walletconnect::{self, Manager, SignPayload, SignRequest};

pub async fn run_stdio(backend: Result<RuntimeClient, CapabilityError>) -> i32 {
    let wallet = Manager::new();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some(response) = handle_line(line, &backend, &wallet).await {
                    let mut text = match serde_json::to_string(&response) {
                        Ok(text) => text,
                        Err(error) => {
                            eprintln!("mcp response encode error: {error}");
                            return 1;
                        }
                    };
                    text.push('\n');
                    if out.write_all(text.as_bytes()).await.is_err() || out.flush().await.is_err() {
                        return 1;
                    }
                }
            }
            Ok(None) => return 0,
            Err(error) => {
                eprintln!("mcp stdin error: {error}");
                return 1;
            }
        }
    }
}

async fn handle_line(
    line: &str,
    backend: &Result<RuntimeClient, CapabilityError>,
    wallet: &Manager,
) -> Option<Value> {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": {"code": -32700, "message": format!("parse error: {error}")}
            }));
        }
    };
    handle_request(value, backend, wallet).await
}

async fn handle_request(
    value: Value,
    backend: &Result<RuntimeClient, CapabilityError>,
    wallet: &Manager,
) -> Option<Value> {
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if method.starts_with("notifications/") {
        return None;
    }
    let id = value.get("id").cloned()?;
    let params = value.get("params").cloned().unwrap_or(Value::Null);
    let result = match method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {
                "name": "fluxion",
                "version": env!("CARGO_PKG_VERSION"),
            }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools_list()),
        "tools/call" => Ok(call_tool(params, backend, wallet).await),
        other => Err((-32601, format!("method not found: {other}"))),
    };
    Some(match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, message)) => {
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
        }
    })
}

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "swap_quote",
                "description": "Get an exact-in swap quote from the warm local Fluxion runtime. \
                                Token inputs may be built-in symbols (LDO, ETH, USDC) or 0x addresses. \
                                Amounts are human units such as 100 or 0.5. \
                                Split results may nest shared nodes under allocations[].sources and \
                                allocations[].children; expand them when reporting complete routes.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "token_in": {"type": "string", "description": "Input token symbol or address"},
                        "token_out": {"type": "string", "description": "Output token symbol or address"},
                        "amount": {"type": "string", "description": "Human amount, e.g. 100 or 0.5"},
                        "decimals_in": {"type": "integer", "minimum": 0, "maximum": 38},
                        "decimals_out": {"type": "integer", "minimum": 0, "maximum": 38},
                        "split": {"type": "boolean", "default": true},
                        "max_hops": {"type": "integer", "minimum": 1},
                        "split_max_branches": {"type": "integer", "minimum": 2, "maximum": 8}
                    },
                    "required": ["token_in", "token_out", "amount"]
                }
            },
            {
                "name": "wallet_balance",
                "description": "Read native ETH and explicit ERC20 balances for an Ethereum address at one pinned block. Local read-only RPC; never signs.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "address": {"type": "string", "description": "Ethereum address"},
                        "tokens": {"type": "array", "items": {"type": "string"}, "description": "Symbols or addresses; empty = built-in registry"},
                        "include_native": {"type": "boolean", "default": true},
                        "block": {"type": "integer", "minimum": 0},
                        "rpc_url": {"type": "string", "description": "Optional local RPC override"}
                    },
                    "required": ["address"]
                }
            },
            {
                "name": "wallet_assets",
                "description": "Discover an Ethereum account's ERC20 positions using the 1inch Balance API, then verify balances locally at one pinned block. Defaults to known-registry tokens plus native ETH.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "address": {"type": "string", "description": "Ethereum address"},
                        "include_native": {"type": "boolean", "default": true},
                        "include_unknown": {"type": "boolean", "default": false},
                        "block": {"type": "integer", "minimum": 0},
                        "rpc_url": {"type": "string", "description": "Optional local RPC override"}
                    },
                    "required": ["address"]
                }
            },
            {
                "name": "wallet_allowance",
                "description": "Read ERC20 allowances for an owner/spender pair at one pinned block. Local read-only RPC; never approves or signs.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "address": {"type": "string", "description": "Owner Ethereum address"},
                        "spender": {"type": "string", "description": "Spender address"},
                        "tokens": {"type": "array", "items": {"type": "string"}, "description": "Symbols or addresses; empty = built-in registry"},
                        "block": {"type": "integer", "minimum": 0},
                        "rpc_url": {"type": "string", "description": "Optional local RPC override"}
                    },
                    "required": ["address", "spender"]
                }
            },
            {
                "name": "wallet_collectibles",
                "description": "Read an Ethereum account's ERC721/ERC1155 holdings. Uses Alchemy NFT discovery when ALCHEMY_API_KEY is set, verifies locally, and falls back to explicit/curated collections.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "address": {"type": "string", "description": "Owner Ethereum address"},
                        "collections": {"type": "array", "items": {"type": "string"}, "description": "Optional collection contract addresses"},
                        "include_empty": {"type": "boolean", "default": false},
                        "block": {"type": "integer", "minimum": 0},
                        "rpc_url": {"type": "string", "description": "Optional local RPC override"}
                    },
                    "required": ["address"]
                }
            },
            {
                "name": "wallet_connect",
                "description": "Start a WalletConnect v2 pairing for an Ethereum wallet. The first text content block is a 53-column fenced QR; reproduce it in your reply before polling status. Also returns a URI and optional PNG path. Read-only connection setup; never asks for private keys.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "wallet_status",
                "description": "Read the locally stored WalletConnect session. Use wait_seconds to long-poll a pending pairing; returns as soon as the status changes or the wait expires.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "wait_seconds": {"type": "integer", "minimum": 0, "maximum": 110, "default": 0}
                    }
                }
            },
            {
                "name": "wallet_sign",
                "description": "Sign a message or EIP-712 typed data with the connected WalletConnect session. The user must confirm on their phone. Pass exactly one of message or typed_data. Never asks for a private key.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "message": {"type": "string", "description": "UTF-8 message for personal_sign"},
                        "typed_data": {"type": "object", "description": "EIP-712 typed data object for eth_signTypedData_v4"},
                        "address": {"type": "string", "description": "Optional session account address"},
                        "timeout_seconds": {"type": "integer", "minimum": 1, "maximum": 110, "default": 90}
                    }
                }
            },
            {
                "name": "wallet_agent_id",
                "description": "Resolve a BSC .ai Agent ID. Pass exactly one of handle (alice or alice.ai) or address. Returns the owner, canonical handle, token id, and whether an Agent ID exists.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "handle": {"type": "string", "description": "Agent ID handle, e.g. alice or alice.ai"},
                        "address": {"type": "string", "description": "BSC wallet address"},
                        "rpc_url": {"type": "string", "description": "Optional BSC RPC override"}
                    }
                }
            },
            {
                "name": "wallet_disconnect",
                "description": "Remove the locally stored WalletConnect session and abort any pending pairing.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "transaction_simulate",
                "description": "Simulate an Ethereum transaction with eth_call at one pinned block. Read-only: returns success/revert, return data and estimated gas; never signs or broadcasts.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "from": {"type": "string", "description": "Optional sender address"},
                        "to": {"type": "string", "description": "Target contract or recipient"},
                        "data": {"type": "string", "description": "0x-prefixed calldata"},
                        "value": {"type": "string", "description": "Optional ETH value in human units", "default": "0"},
                        "block": {"type": "integer", "minimum": 0},
                        "rpc_url": {"type": "string", "description": "Optional local RPC override"}
                    },
                    "required": ["to", "data"]
                }
            }
        ]
    })
}

async fn call_tool(
    params: Value,
    backend: &Result<RuntimeClient, CapabilityError>,
    wallet: &Manager,
) -> Value {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    let capability = capability_for_tool(name);
    let mut image = None;
    let result = match name {
        "swap_quote" => quote_tool(&arguments, backend).await.and_then(|response| {
            serde_json::to_value(response)
                .map_err(|error| CapabilityError::internal(error.to_string()))
        }),
        "wallet_balance" => wallet_balance_tool(&arguments).await,
        "wallet_assets" => wallet_assets_tool(&arguments).await,
        "wallet_allowance" => wallet_allowance_tool(&arguments).await,
        "wallet_collectibles" => wallet_collectibles_tool(&arguments).await,
        "wallet_connect" => wallet_connect_tool(wallet).await.map(|(value, png)| {
            image = png;
            value
        }),
        "wallet_status" => wallet_status_tool(&arguments, wallet).await,
        "wallet_sign" => wallet_sign_tool(&arguments).await,
        "wallet_agent_id" => wallet_agent_id_tool(&arguments).await,
        "wallet_disconnect" => wallet_disconnect_tool(wallet).await,
        "transaction_simulate" => transaction_simulate_tool(&arguments).await,
        other => Err(CapabilityError::invalid_argument(format!(
            "unknown tool: {other}"
        ))),
    };
    match result {
        Ok(value) => tool_result(value, image),
        Err(error) => tool_error(capability, error),
    }
}

fn capability_for_tool(name: &str) -> &'static str {
    match name {
        "swap_quote" => "swap.quote",
        "wallet_balance" => "wallet.balance",
        "wallet_assets" => "wallet.assets",
        "wallet_allowance" => "wallet.allowance",
        "wallet_collectibles" => "wallet.collectibles",
        "wallet_connect" => "wallet.connect",
        "wallet_status" => "wallet.status",
        "wallet_sign" => "wallet.sign",
        "wallet_agent_id" => "wallet.agent-id",
        "wallet_disconnect" => "wallet.disconnect",
        "transaction_simulate" => "transaction.simulate",
        _ => "unknown",
    }
}

async fn wallet_balance_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let address = string_arg(arguments, "address")?
        .parse::<alloy_primitives::Address>()
        .map_err(|_| CapabilityError::invalid_argument("invalid wallet address"))?;
    let tokens = token_list_arg(arguments)?;
    let config = wallet_config(arguments);
    let response = wallet::balance(
        &config,
        BalanceRequest {
            address,
            tokens,
            include_native: bool_arg(arguments, "include_native").unwrap_or(true),
            block: optional_u64(arguments, "block")?,
        },
    )
    .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn wallet_assets_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let address = string_arg(arguments, "address")?
        .parse::<alloy_primitives::Address>()
        .map_err(|_| CapabilityError::invalid_argument("invalid wallet address"))?;
    let config = wallet_config(arguments);
    let response = wallet::assets(
        &config,
        AssetsRequest {
            address,
            include_native: bool_arg(arguments, "include_native").unwrap_or(true),
            include_unknown: bool_arg(arguments, "include_unknown").unwrap_or(false),
            block: optional_u64(arguments, "block")?,
        },
    )
    .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

fn wallet_config(arguments: &Value) -> WalletConfig {
    let mut config = WalletConfig::from_env();
    if let Some(rpc_url) = arguments.get("rpc_url").and_then(Value::as_str) {
        config.rpc_url = rpc_url.to_string();
    }
    config
}

fn token_list_arg(arguments: &Value) -> Result<Vec<String>, CapabilityError> {
    match arguments.get("tokens") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::String(value)) => Ok(value
            .split(',')
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| CapabilityError::invalid_argument("invalid token in tokens"))
            })
            .collect(),
        Some(_) => Err(CapabilityError::invalid_argument(
            "tokens must be an array or comma-separated string",
        )),
    }
}

fn bool_arg(arguments: &Value, name: &str) -> Option<bool> {
    arguments.get(name).and_then(Value::as_bool)
}

fn optional_u64(arguments: &Value, name: &str) -> Result<Option<u64>, CapabilityError> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| CapabilityError::invalid_argument(format!("invalid {name}"))),
    }
}

async fn wallet_collectibles_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let owner = string_arg(arguments, "address")?
        .parse::<alloy_primitives::Address>()
        .map_err(|_| CapabilityError::invalid_argument("invalid wallet address"))?;
    let collections = match arguments.get("collections") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| CapabilityError::invalid_argument("invalid collection address"))?
                    .parse::<alloy_primitives::Address>()
                    .map_err(|_| CapabilityError::invalid_argument("invalid collection address"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => {
            return Err(CapabilityError::invalid_argument(
                "collections must be an array",
            ));
        }
    };
    let config = wallet_config(arguments);
    let response = wallet::collectibles(
        &config,
        CollectiblesRequest {
            owner,
            collections,
            include_empty: bool_arg(arguments, "include_empty").unwrap_or(false),
            block: optional_u64(arguments, "block")?,
        },
    )
    .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn wallet_allowance_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let owner = string_arg(arguments, "address")?
        .parse::<alloy_primitives::Address>()
        .map_err(|_| CapabilityError::invalid_argument("invalid wallet address"))?;
    let spender = string_arg(arguments, "spender")?
        .parse::<alloy_primitives::Address>()
        .map_err(|_| CapabilityError::invalid_argument("invalid spender address"))?;
    let config = wallet_config(arguments);
    let response = wallet::allowance(
        &config,
        AllowanceRequest {
            owner,
            spender,
            tokens: token_list_arg(arguments)?,
            block: optional_u64(arguments, "block")?,
        },
    )
    .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn wallet_connect_tool(wallet: &Manager) -> Result<(Value, Option<String>), CapabilityError> {
    let response = wallet.connect().await?;
    let image = response.qr_png_base64.clone();
    let value = serde_json::to_value(&response)
        .map_err(|error| CapabilityError::internal(error.to_string()))?;
    Ok((value, image))
}

async fn wallet_status_tool(arguments: &Value, wallet: &Manager) -> Result<Value, CapabilityError> {
    let wait = optional_u64(arguments, "wait_seconds")?.unwrap_or(0);
    if wait > 110 {
        return Err(CapabilityError::invalid_argument(
            "wait_seconds must be between 0 and 110",
        ));
    }
    serde_json::to_value(
        wallet
            .wait_status(std::time::Duration::from_secs(wait))
            .await?,
    )
    .map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn wallet_sign_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let message = arguments
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string);
    let typed_data = match arguments.get("typed_data") {
        None | Some(Value::Null) => None,
        Some(Value::Object(_)) => arguments.get("typed_data").cloned(),
        Some(Value::String(text)) => Some(serde_json::from_str(text).map_err(|error| {
            CapabilityError::invalid_argument(format!("invalid typed_data: {error}"))
        })?),
        Some(_) => {
            return Err(CapabilityError::invalid_argument(
                "typed_data must be a JSON object",
            ));
        }
    };
    let payload = match (message, typed_data) {
        (Some(message), None) => SignPayload::Message(message),
        (None, Some(typed_data)) => SignPayload::TypedData(typed_data),
        _ => {
            return Err(CapabilityError::invalid_argument(
                "wallet_sign needs exactly one of message or typed_data",
            ));
        }
    };
    let address = arguments
        .get("address")
        .and_then(Value::as_str)
        .map(|value| {
            value
                .parse::<alloy_primitives::Address>()
                .map_err(|_| CapabilityError::invalid_argument("invalid wallet address"))
        })
        .transpose()?;
    let timeout = optional_u64(arguments, "timeout_seconds")?.unwrap_or(90);
    if timeout == 0 || timeout > 110 {
        return Err(CapabilityError::invalid_argument(
            "timeout_seconds must be between 1 and 110",
        ));
    }
    let response = walletconnect::sign(SignRequest {
        address,
        payload,
        timeout: std::time::Duration::from_secs(timeout),
    })
    .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn wallet_agent_id_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let handle = arguments
        .get("handle")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let address = arguments
        .get("address")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let lookup = match (handle, address) {
        (Some(handle), None) => AgentIdLookup::Handle(handle.to_string()),
        (None, Some(address)) => AgentIdLookup::Address(
            address
                .parse()
                .map_err(|_| CapabilityError::invalid_argument("invalid Agent ID address"))?,
        ),
        _ => {
            return Err(CapabilityError::invalid_argument(
                "wallet_agent_id needs exactly one of handle or address",
            ));
        }
    };
    let mut config = AgentIdConfig::from_env();
    if let Some(rpc_url) = arguments.get("rpc_url").and_then(Value::as_str) {
        config.bsc_rpc_url = rpc_url.to_string();
    }
    let response = agent_id::resolve(&config, AgentIdRequest { lookup }).await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn wallet_disconnect_tool(wallet: &Manager) -> Result<Value, CapabilityError> {
    serde_json::to_value(wallet.disconnect().await?)
        .map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn transaction_simulate_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let to = string_arg(arguments, "to")?
        .parse::<alloy_primitives::Address>()
        .map_err(|_| CapabilityError::invalid_argument("invalid --to address"))?;
    let from = arguments
        .get("from")
        .and_then(Value::as_str)
        .map(|value| {
            value
                .parse::<alloy_primitives::Address>()
                .map_err(|_| CapabilityError::invalid_argument("invalid --from address"))
        })
        .transpose()?;
    let data = string_arg(arguments, "data")?;
    let value = arguments
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or("0");
    let value = token::parse_amount(value, 18)?;
    let config = wallet_config(arguments);
    let response = transaction::simulate(
        &config,
        SimulateRequest {
            from,
            to,
            data,
            value,
            block: optional_u64(arguments, "block")?,
        },
    )
    .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn quote_tool(
    arguments: &Value,
    backend: &Result<RuntimeClient, CapabilityError>,
) -> Result<Value, CapabilityError> {
    let client = backend.as_ref().map_err(|error| error.clone())?;
    let token_in = string_arg(arguments, "token_in")?;
    let token_out = string_arg(arguments, "token_out")?;
    let amount = string_arg(arguments, "amount")?;
    let token_in = token::resolve(&token_in, optional_u8(arguments, "decimals_in")?)?;
    let token_out = token::resolve(&token_out, optional_u8(arguments, "decimals_out")?)?;
    let amount_in = token::parse_amount(&amount, token_in.decimals)?;
    let request = QuoteRequest {
        token_in: token_in.address,
        token_out: token_out.address,
        amount_in,
        max_hops: optional_usize(arguments, "max_hops")?,
        amount_out_min: None,
        split: arguments
            .get("split")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        split_max_branches: optional_usize(arguments, "split_max_branches")?,
    };
    let response = client
        .swap_quote(&request, token_in, token_out, "ethereum", "remote")
        .await?;
    Ok(serde_json::to_value(response).unwrap_or(Value::Null))
}

fn string_arg(arguments: &Value, name: &str) -> Result<String, CapabilityError> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| CapabilityError::invalid_argument(format!("missing argument: {name}")))
}

fn optional_u8(arguments: &Value, name: &str) -> Result<Option<u8>, CapabilityError> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| CapabilityError::invalid_argument(format!("invalid {name}"))),
    }
}

fn optional_usize(arguments: &Value, name: &str) -> Result<Option<usize>, CapabilityError> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| CapabilityError::invalid_argument(format!("invalid {name}"))),
    }
}

fn tool_result(value: Value, image: Option<String>) -> Value {
    let mut content = Vec::new();
    if let Some(qr) = value
        .get("qr_ascii")
        .and_then(Value::as_str)
        .filter(|qr| !qr.trim().is_empty())
    {
        content.push(json!({
            "type": "text",
            "text": format!(
                "Scan this QR code with MetaMask or another WalletConnect wallet:\n\n```text\n{qr}```"
            ),
        }));
    }
    content.push(json!({
        "type": "text",
        "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
    }));
    if let Some(data) = image {
        content.push(json!({
            "type": "image",
            "data": data,
            "mimeType": "image/png",
        }));
    }
    json!({
        "content": content,
        "isError": false,
    })
}

fn tool_error(capability: &str, error: CapabilityError) -> Value {
    let value = serde_json::to_value(CapabilityErrorResponse::new(
        capability,
        request_id(),
        error,
    ))
    .unwrap_or(Value::Null);
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
        }],
        "isError": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityErrorCode;

    #[test]
    fn exposes_swap_quote_tool_without_execution_selection() {
        let tools = tools_list();
        assert_eq!(tools["tools"][0]["name"], "swap_quote");
        assert_eq!(tools["tools"][0]["inputSchema"]["required"][0], "token_in");
        assert!(
            tools["tools"][0]["inputSchema"]["properties"]
                .get("provider")
                .is_none()
        );
        assert!(
            tools["tools"][0]["description"]
                .as_str()
                .unwrap()
                .contains("children")
        );
        let names = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "swap_quote",
                "wallet_balance",
                "wallet_assets",
                "wallet_allowance",
                "wallet_collectibles",
                "wallet_connect",
                "wallet_status",
                "wallet_sign",
                "wallet_agent_id",
                "wallet_disconnect",
                "transaction_simulate"
            ]
        );
    }

    #[test]
    fn rejects_unknown_tool() {
        let result = futures::executor::block_on(call_tool(
            json!({"name": "nope", "arguments": {}}),
            &Ok(RuntimeClient::new("http://127.0.0.1:1")),
            &Manager::new(),
        ));
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn wallet_tools_validate_arguments_without_remote_backend() {
        let result = futures::executor::block_on(call_tool(
            json!({"name": "wallet_balance", "arguments": {"address": "nope"}}),
            &Err(CapabilityError::invalid_argument("remote unavailable")),
            &Manager::new(),
        ));
        assert_eq!(result["isError"], true);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("INVALID_ARGUMENT")
        );
    }

    #[test]
    fn tool_result_can_include_a_qr_image() {
        let value = tool_result(json!({"status": "pending"}), Some("png".to_string()));
        assert_eq!(value["content"][0]["type"], "text");
        assert_eq!(value["content"][1]["type"], "image");
        assert_eq!(value["content"][1]["mimeType"], "image/png");
        assert_eq!(value["content"][1]["data"], "png");
    }

    #[test]
    fn tool_result_puts_the_raw_qr_before_json() {
        let value = tool_result(
            json!({"status": "pending", "qr_ascii": "line one\nline two\n"}),
            None,
        );
        let first = value["content"][0]["text"].as_str().unwrap();
        assert!(first.contains("```text\nline one\nline two\n```"));
        assert!(
            value["content"][1]["text"]
                .as_str()
                .unwrap()
                .contains("qr_ascii")
        );
    }

    #[test]
    fn error_codes_remain_structured() {
        let value = tool_error(
            "swap.quote",
            CapabilityError::new(CapabilityErrorCode::NoPath, "no path", false),
        );
        assert_eq!(value["isError"], true);
        assert!(
            value["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("NO_PATH")
        );
    }
}
