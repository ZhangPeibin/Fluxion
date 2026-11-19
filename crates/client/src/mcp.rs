//! Minimal MCP stdio server exposing the `swap_quote` tool.
//!
//! The MCP adapter is the agent-facing transport. It forwards to Fluxion's
//! internal execution backend (hosted service or local runtime), so repeated
//! tool calls do not pay discovery or snapshot warmup.

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::agent_id::{self, AgentIdConfig, AgentIdLookup, AgentIdRequest};
use crate::capability::{CapabilityError, CapabilityErrorResponse};
use crate::chain::{Chain, DEFAULT_CHAIN};
use crate::client::{RuntimeClient, request_id};
use crate::fourmeme::{self, FourMemeConfig, ListRequest as FourMemeListRequest};
use crate::signature_inspect::{self, SignatureInspectRequest};
use crate::swap::quote::QuoteRequest;
use crate::swap_build::{self, BuildRequest};
use crate::token;
use crate::transaction::{self, SimulateRequest};
use crate::transaction_inspect::{self, InspectPolicy, InspectRequest};
use crate::transaction_send::{self, SendRequest as TransactionSendRequest};
use crate::wallet::{
    self, AllowanceRequest, AssetsRequest, BalanceRequest, CollectiblesRequest, WalletConfig,
};
use crate::wallet_send::{self, SendRequest};
use crate::walletconnect::{self, Manager, SignPayload, SignRequest};
use fluxion_protocol::swap::QuoteResponse;
use fluxion_protocol::yield_capability::{
    RiskProfile, YieldBuildRequest, YieldDiscoverRequest, YieldHistoryRequest,
};

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
                "name": "swap_build",
                "description": "Build Uniswap V2 swap calldata from a swap.quote response. Local, read-only; currently supports one uniswap-v2 allocation and never signs or broadcasts.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "quote": {"type": "object", "description": "The swap.quote JSON response"},
                        "recipient": {"type": "string", "description": "Output recipient address"},
                        "slippage_bps": {"type": "integer", "minimum": 0, "maximum": 10000, "default": 50},
                        "deadline": {"type": "integer", "minimum": 0, "description": "Unix deadline; defaults to now + 600"}
                    },
                    "required": ["quote", "recipient"]
                }
            },
            {
                "name": "wallet_balance",
                "description": "Read native ETH and explicit ERC20 balances for an Ethereum address at one pinned block. Local read-only RPC; never signs.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "address": {"type": "string", "description": "Ethereum or BSC address"},
                        "chain": {"type": "string", "enum": ["bsc", "ethereum"], "default": "bsc"},
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
                        "address": {"type": "string", "description": "Ethereum or BSC address"},
                        "chain": {"type": "string", "enum": ["bsc", "ethereum"], "default": "bsc"},
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
                        "address": {"type": "string", "description": "Owner Ethereum or BSC address"},
                        "chain": {"type": "string", "enum": ["bsc", "ethereum"], "default": "bsc"},
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
                        "address": {"type": "string", "description": "Owner Ethereum or BSC address"},
                        "chain": {"type": "string", "enum": ["bsc", "ethereum"], "default": "bsc"},
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
                "description": "Start a WalletConnect v2 pairing. Pass chain ethereum or bsc. The first text content block is a 53-column fenced QR; reproduce it in your reply before polling status.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "chain": {"type": "string", "enum": ["ethereum", "bsc"], "default": "bsc"}
                    }
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
                "name": "wallet_send",
                "description": "Send a native or ERC20/BEP-20 transfer through the connected WalletConnect session. Simulates locally first, then asks the user to confirm in their wallet. Pass to, token, and amount.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "to": {"type": "string", "description": "Recipient address"},
                        "token": {"type": "string", "description": "BNB/ETH, a chain built-in symbol, or a token address"},
                        "amount": {"type": "string", "description": "Human amount, e.g. 0.1"},
                        "from": {"type": "string", "description": "Optional session account address"},
                        "decimals": {"type": "integer", "minimum": 0, "maximum": 38},
                        "block": {"type": "integer", "minimum": 0},
                        "rpc_url": {"type": "string", "description": "Optional local RPC override"},
                        "timeout_seconds": {"type": "integer", "minimum": 1, "maximum": 110, "default": 90},
                        "policy": {
                            "type": "object",
                            "description": "Optional enforcement policy; mode=enforce blocks on high policy findings"
                        }
                    },
                    "required": ["to", "token", "amount"]
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
                        "timeout_seconds": {"type": "integer", "minimum": 1, "maximum": 110, "default": 90},
                        "policy": {
                            "type": "object",
                            "description": "Optional typed-data policy; mode=enforce blocks high-risk findings"
                        }
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
                "description": "Simulate a BSC or Ethereum transaction with eth_call at one pinned block. Read-only: returns success/revert, return data and estimated gas; never signs or broadcasts.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "chain": {"type": "string", "enum": ["bsc", "ethereum"], "default": "bsc"},
                        "from": {"type": "string", "description": "Optional sender address"},
                        "to": {"type": "string", "description": "Target contract or recipient"},
                        "data": {"type": "string", "description": "0x-prefixed calldata"},
                        "value": {"type": "string", "description": "Optional ETH value in human units", "default": "0"},
                        "block": {"type": "integer", "minimum": 0},
                        "rpc_url": {"type": "string", "description": "Optional local RPC override"}
                    },
                    "required": ["to", "data"]
                }
            },
            {
                "name": "transaction_inspect",
                "description": "Inspect unsigned EVM calldata before signing. Decodes the intent, pins a block, and returns findings, coverage and explicit unknowns. Advisory only; never signs or broadcasts and never claims safety.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "chain": {"type": "string", "enum": ["bsc", "ethereum"], "default": "bsc"},
                        "from": {"type": "string", "description": "Optional sender address"},
                        "to": {"type": "string", "description": "Target contract or recipient"},
                        "data": {"type": "string", "description": "0x-prefixed calldata; use 0x for a native transfer"},
                        "value": {"type": "string", "description": "Optional native value in human units", "default": "0"},
                        "block": {"type": "integer", "minimum": 0},
                        "rpc_url": {"type": "string", "description": "Optional local RPC override"},
                        "policy": {
                            "type": "object",
                            "description": "Optional caller policy: mode, chains, max_native_spend, max_token_spend, target_allowlist, method_allowlist, spender_allowlist"
                        }
                    },
                    "required": ["to", "data"]
                }
            },
            {
                "name": "transaction_send",
                "description": "Simulate and inspect arbitrary calldata, then send it through the connected WalletConnect session. Policy in enforce mode can block before the phone request. The wallet remains the signing boundary.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "from": {"type": "string", "description": "Optional session account address"},
                        "to": {"type": "string", "description": "Target contract or recipient"},
                        "data": {"type": "string", "description": "0x-prefixed calldata"},
                        "value": {"type": "string", "description": "Optional native value in human units", "default": "0"},
                        "block": {"type": "integer", "minimum": 0},
                        "rpc_url": {"type": "string", "description": "Optional local RPC override"},
                        "policy": {"type": "object", "description": "Optional policy; enforce can block the send"},
                        "timeout_seconds": {"type": "integer", "minimum": 1, "maximum": 110, "default": 90}
                    },
                    "required": ["to", "data"]
                }
            },
            {
                "name": "signature_inspect",
                "description": "Inspect EIP-712 typed data, EIP-7702 authorization lists or an ERC-4337 user operation before signing. Advisory only; never signs or broadcasts.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "chain": {"type": "string", "enum": ["bsc", "ethereum"], "default": "bsc"},
                        "from": {"type": "string", "description": "Optional signer address"},
                        "typed_data": {"type": "object", "description": "EIP-712 typed data object"},
                        "authorization_list": {"type": "array", "description": "EIP-7702 authorization list"},
                        "user_operation": {"type": "object", "description": "ERC-4337 user operation"}
                    }
                }
            },
            {
                "name": "fourmeme_agent_tokens",
                "description": "List recent Four.meme tokens in Binance Agent ID Exclusive mode. Read-only; filters by template 1778027615735 and can check AgentIdentifier eligibility for a wallet.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "wallet": {"type": "string", "description": "Optional wallet for AgentIdentifier eligibility"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 50, "default": 20},
                        "lookback_blocks": {"type": "integer", "minimum": 1, "maximum": 5000000, "default": 100000}
                    }
                }
            },
            {
                "name": "fourmeme_token",
                "description": "Read Four.meme token detail, including phase, template, market data and optional AgentIdentifier eligibility. Read-only; never buys, sells or signs.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "address": {"type": "string", "description": "Token contract address"},
                        "wallet": {"type": "string", "description": "Optional wallet for AgentIdentifier eligibility"}
                    },
                    "required": ["address"]
                }
            },
            {
                "name": "yield_discover",
                "description": "Find safe, high-APY ETH staking and lending opportunities on Ethereum from a curated universe. Returns safety tiers, APY components, TVL, withdrawal conditions, evidence and one recommendation with tradeoffs. Read-only; never deposits, signs or broadcasts.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "asset": {"type": "string", "description": "Currently ETH", "default": "ETH"},
                        "amount": {"type": "string", "description": "Optional human amount, e.g. 10"},
                        "risk_profile": {"type": "string", "enum": ["conservative", "balanced", "aggressive"], "default": "conservative"},
                        "min_tvl_usd": {"type": "integer", "minimum": 0},
                        "max_lockup_days": {"type": "integer", "minimum": 0},
                        "include_reward_apy": {"type": "boolean", "default": false},
                        "protocols": {"type": "array", "items": {"type": "string"}, "description": "Optional DefiLlama protocol slugs"},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 20, "default": 10}
                    },
                    "required": ["asset"]
                }
            },
            {
                "name": "yield_build",
                "description": "Build local deposit calldata for a curated yield opportunity. Returns the exact transactions and any required WETH approval. Read-only; never signs or broadcasts. Supports Lido, Rocket Pool, Aave V3, SparkLend and curated Morpho vaults.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "opportunity_id": {"type": "string", "description": "ID from yield_discover, e.g. lido-steth"},
                        "amount": {"type": "string", "description": "Human amount, e.g. 10"},
                        "recipient": {"type": "string", "description": "Position/receipt owner; must be the sender for Lido/Rocket Pool"},
                        "funding_asset": {"type": "string", "enum": ["ETH", "WETH"], "default": "ETH", "description": "Use WETH to skip the wrap step"}
                    },
                    "required": ["opportunity_id", "amount", "recipient"]
                }
            },
            {
                "name": "yield_history",
                "description": "Read historical APY, base/reward components and TVL for a curated yield opportunity. Returns a downsampled series plus mean/min/max/change summary. Read-only; does not predict future yield.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "opportunity_id": {"type": "string", "description": "ID from yield_discover, e.g. lido-steth"},
                        "days": {"type": "integer", "minimum": 1, "maximum": 365, "default": 30},
                        "max_points": {"type": "integer", "minimum": 1, "maximum": 2000, "default": 500}
                    },
                    "required": ["opportunity_id"]
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
        "swap_build" => swap_build_tool(&arguments),
        "wallet_balance" => wallet_balance_tool(&arguments).await,
        "wallet_assets" => wallet_assets_tool(&arguments).await,
        "wallet_allowance" => wallet_allowance_tool(&arguments).await,
        "wallet_collectibles" => wallet_collectibles_tool(&arguments).await,
        "wallet_connect" => wallet_connect_tool(&arguments, wallet)
            .await
            .map(|(value, png)| {
                image = png;
                value
            }),
        "wallet_status" => wallet_status_tool(&arguments, wallet).await,
        "wallet_send" => wallet_send_tool(&arguments).await,
        "wallet_sign" => wallet_sign_tool(&arguments).await,
        "wallet_agent_id" => wallet_agent_id_tool(&arguments).await,
        "wallet_disconnect" => wallet_disconnect_tool(wallet).await,
        "transaction_simulate" => transaction_simulate_tool(&arguments).await,
        "transaction_inspect" => transaction_inspect_tool(&arguments).await,
        "transaction_send" => transaction_send_tool(&arguments).await,
        "fourmeme_agent_tokens" => fourmeme_agent_tokens_tool(&arguments).await,
        "fourmeme_token" => fourmeme_token_tool(&arguments).await,
        "yield_discover" => yield_discover_tool(&arguments, backend).await,
        "yield_build" => yield_build_tool(&arguments),
        "yield_history" => yield_history_tool(&arguments, backend).await,
        "signature_inspect" => signature_inspect_tool(&arguments).await,
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
        "swap_build" => "swap.build",
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
        "transaction_inspect" => "transaction.inspect",
        "transaction_send" => "transaction.send",
        "fourmeme_agent_tokens" => "fourmeme.agent-tokens",
        "fourmeme_token" => "fourmeme.token",
        "yield_discover" => "yield.discover",
        "yield_build" => "yield.build",
        "yield_history" => "yield.history",
        "signature_inspect" => "signature.inspect",
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
            chain: chain_arg(arguments)?,
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
            chain: chain_arg(arguments)?,
            address,
            include_native: bool_arg(arguments, "include_native").unwrap_or(true),
            include_unknown: bool_arg(arguments, "include_unknown").unwrap_or(false),
            block: optional_u64(arguments, "block")?,
        },
    )
    .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

fn chain_arg(arguments: &Value) -> Result<Chain, CapabilityError> {
    arguments
        .get("chain")
        .and_then(Value::as_str)
        .map(Chain::parse)
        .transpose()
        .map(|chain| chain.unwrap_or(DEFAULT_CHAIN))
}

fn wallet_config(arguments: &Value) -> WalletConfig {
    let mut config = WalletConfig::from_env();
    if let Some(rpc_url) = arguments.get("rpc_url").and_then(Value::as_str) {
        config.set_rpc_url(rpc_url);
    }
    config
}

fn swap_build_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let quote: QuoteResponse = serde_json::from_value(
        arguments
            .get("quote")
            .cloned()
            .ok_or_else(|| CapabilityError::invalid_argument("swap_build requires quote"))?,
    )
    .map_err(|error| CapabilityError::invalid_argument(format!("invalid quote: {error}")))?;
    let recipient = string_arg(arguments, "recipient")?
        .parse::<alloy_primitives::Address>()
        .map_err(|_| CapabilityError::invalid_argument("invalid recipient address"))?;
    let slippage_bps = arguments
        .get("slippage_bps")
        .and_then(Value::as_u64)
        .unwrap_or(50)
        .try_into()
        .map_err(|_| CapabilityError::invalid_argument("invalid slippage_bps"))?;
    let deadline = arguments
        .get("deadline")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs() + 600)
                .unwrap_or(0)
        });
    let response = swap_build::build(
        &quote,
        BuildRequest {
            recipient,
            slippage_bps,
            deadline,
        },
    )?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
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
            chain: chain_arg(arguments)?,
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
            chain: chain_arg(arguments)?,
            owner,
            spender,
            tokens: token_list_arg(arguments)?,
            block: optional_u64(arguments, "block")?,
        },
    )
    .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn wallet_connect_tool(
    arguments: &Value,
    wallet: &Manager,
) -> Result<(Value, Option<String>), CapabilityError> {
    let chain_id = arguments
        .get("chain")
        .and_then(Value::as_str)
        .map(crate::walletconnect::parse_chain_id)
        .transpose()?
        .unwrap_or(DEFAULT_CHAIN.chain_id());
    let response = wallet.connect(chain_id).await?;
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

async fn wallet_send_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let to = string_arg(arguments, "to")?
        .parse::<alloy_primitives::Address>()
        .map_err(|_| CapabilityError::invalid_argument("invalid recipient address"))?;
    let from = arguments
        .get("from")
        .and_then(Value::as_str)
        .map(|value| {
            value
                .parse::<alloy_primitives::Address>()
                .map_err(|_| CapabilityError::invalid_argument("invalid from address"))
        })
        .transpose()?;
    let timeout = optional_u64(arguments, "timeout_seconds")?.unwrap_or(90);
    if timeout == 0 || timeout > 110 {
        return Err(CapabilityError::invalid_argument(
            "timeout_seconds must be between 1 and 110",
        ));
    }
    let policy = arguments
        .get("policy")
        .filter(|value| !value.is_null())
        .cloned()
        .map(|value| {
            serde_json::from_value::<InspectPolicy>(value).map_err(|error| {
                CapabilityError::invalid_argument(format!("invalid policy: {error}"))
            })
        })
        .transpose()?;
    let mut config = wallet_config(arguments);
    if let Some(rpc_url) = arguments.get("rpc_url").and_then(Value::as_str) {
        config.set_rpc_url(rpc_url);
    }
    let response = wallet_send::send(
        &config,
        SendRequest {
            from,
            to,
            token: string_arg(arguments, "token")?,
            amount: string_arg(arguments, "amount")?,
            decimals: optional_u8(arguments, "decimals")?,
            block: optional_u64(arguments, "block")?,
            timeout: std::time::Duration::from_secs(timeout),
            policy,
        },
    )
    .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
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
    let policy = arguments
        .get("policy")
        .filter(|value| !value.is_null())
        .cloned()
        .map(|value| {
            serde_json::from_value::<InspectPolicy>(value).map_err(|error| {
                CapabilityError::invalid_argument(format!("invalid policy: {error}"))
            })
        })
        .transpose()?;
    let response = walletconnect::sign(SignRequest {
        address,
        payload,
        timeout: std::time::Duration::from_secs(timeout),
        policy,
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
            chain: chain_arg(arguments)?,
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

async fn transaction_inspect_tool(arguments: &Value) -> Result<Value, CapabilityError> {
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
    let policy = arguments
        .get("policy")
        .filter(|value| !value.is_null())
        .cloned()
        .map(|value| {
            serde_json::from_value::<InspectPolicy>(value).map_err(|error| {
                CapabilityError::invalid_argument(format!("invalid policy: {error}"))
            })
        })
        .transpose()?;
    let config = wallet_config(arguments);
    let response = transaction_inspect::inspect_with_policy(
        &config,
        InspectRequest {
            chain: chain_arg(arguments)?,
            from,
            to,
            data,
            value,
            block: optional_u64(arguments, "block")?,
        },
        policy,
    )
    .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn fourmeme_agent_tokens_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let wallet = arguments
        .get("wallet")
        .and_then(Value::as_str)
        .map(|value| {
            value
                .parse::<alloy_primitives::Address>()
                .map_err(|_| CapabilityError::invalid_argument("invalid wallet address"))
        })
        .transpose()?;
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(20)
        .try_into()
        .map_err(|_| CapabilityError::invalid_argument("invalid limit"))?;
    if !(1..=50).contains(&limit) {
        return Err(CapabilityError::invalid_argument(
            "limit must be between 1 and 50",
        ));
    }
    let lookback_blocks = arguments
        .get("lookback_blocks")
        .and_then(Value::as_u64)
        .unwrap_or(100_000);
    if lookback_blocks == 0 || lookback_blocks > 5_000_000 {
        return Err(CapabilityError::invalid_argument(
            "lookback_blocks must be between 1 and 5000000",
        ));
    }
    let response = fourmeme::agent_tokens(
        &FourMemeConfig::from_env(),
        FourMemeListRequest {
            wallet,
            limit,
            lookback_blocks,
        },
    )
    .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn fourmeme_token_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let address = arguments
        .get("address")
        .and_then(Value::as_str)
        .ok_or_else(|| CapabilityError::invalid_argument("address is required"))?
        .parse::<alloy_primitives::Address>()
        .map_err(|_| CapabilityError::invalid_argument("invalid token address"))?;
    let wallet = arguments
        .get("wallet")
        .and_then(Value::as_str)
        .map(|value| {
            value
                .parse::<alloy_primitives::Address>()
                .map_err(|_| CapabilityError::invalid_argument("invalid wallet address"))
        })
        .transpose()?;
    let detail = fourmeme::token(&FourMemeConfig::from_env(), address, wallet).await?;
    serde_json::to_value(detail).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn yield_discover_tool(
    arguments: &Value,
    backend: &Result<RuntimeClient, CapabilityError>,
) -> Result<Value, CapabilityError> {
    let backend = backend.as_ref().map_err(Clone::clone)?;
    let asset = arguments
        .get("asset")
        .and_then(Value::as_str)
        .unwrap_or("ETH")
        .to_string();
    let amount = arguments
        .get("amount")
        .and_then(Value::as_str)
        .map(str::to_string);
    let risk = arguments
        .get("risk_profile")
        .and_then(Value::as_str)
        .unwrap_or("conservative");
    let risk_profile = match risk {
        "conservative" => RiskProfile::Conservative,
        "balanced" => RiskProfile::Balanced,
        "aggressive" => RiskProfile::Aggressive,
        other => {
            return Err(CapabilityError::invalid_argument(format!(
                "invalid risk_profile: {other}"
            )));
        }
    };
    let min_tvl_usd = arguments.get("min_tvl_usd").and_then(Value::as_u64);
    let max_lockup_days = arguments
        .get("max_lockup_days")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    let include_reward_apy = arguments.get("include_reward_apy").and_then(Value::as_bool);
    let protocols = arguments
        .get("protocols")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .try_into()
        .map_err(|_| CapabilityError::invalid_argument("invalid limit"))?;
    if !(1..=20).contains(&limit) {
        return Err(CapabilityError::invalid_argument(
            "limit must be between 1 and 20",
        ));
    }
    let response = backend
        .yield_discover(&YieldDiscoverRequest {
            chain: "ethereum".to_string(),
            asset,
            amount,
            risk_profile,
            min_tvl_usd,
            max_lockup_days,
            include_reward_apy,
            protocols,
            limit,
        })
        .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

fn yield_build_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let opportunity_id = string_arg(arguments, "opportunity_id")?.to_string();
    let amount = string_arg(arguments, "amount")?.to_string();
    let recipient = string_arg(arguments, "recipient")?
        .parse::<alloy_primitives::Address>()
        .map_err(|_| CapabilityError::invalid_argument("invalid recipient address"))?;
    let funding_asset = arguments
        .get("funding_asset")
        .and_then(Value::as_str)
        .map(str::to_string);
    let response = crate::yield_build::build(&YieldBuildRequest {
        opportunity_id,
        amount,
        recipient,
        funding_asset,
    })?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn yield_history_tool(
    arguments: &Value,
    backend: &Result<RuntimeClient, CapabilityError>,
) -> Result<Value, CapabilityError> {
    let backend = backend.as_ref().map_err(Clone::clone)?;
    let opportunity_id = string_arg(arguments, "opportunity_id")?.to_string();
    let days = arguments
        .get("days")
        .and_then(Value::as_u64)
        .unwrap_or(30)
        .try_into()
        .map_err(|_| CapabilityError::invalid_argument("invalid days"))?;
    let max_points = arguments
        .get("max_points")
        .and_then(Value::as_u64)
        .unwrap_or(500)
        .try_into()
        .map_err(|_| CapabilityError::invalid_argument("invalid max_points"))?;
    let response = backend
        .yield_history(&YieldHistoryRequest {
            opportunity_id,
            days,
            max_points,
        })
        .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn transaction_send_tool(arguments: &Value) -> Result<Value, CapabilityError> {
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
    let value = arguments
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or("0");
    let value = token::parse_amount(value, 18)?;
    let timeout = optional_u64(arguments, "timeout_seconds")?.unwrap_or(90);
    if timeout == 0 || timeout > 110 {
        return Err(CapabilityError::invalid_argument(
            "timeout_seconds must be between 1 and 110",
        ));
    }
    let policy = arguments
        .get("policy")
        .filter(|value| !value.is_null())
        .cloned()
        .map(|value| {
            serde_json::from_value::<InspectPolicy>(value).map_err(|error| {
                CapabilityError::invalid_argument(format!("invalid policy: {error}"))
            })
        })
        .transpose()?;
    let config = wallet_config(arguments);
    let response = transaction_send::send(
        &config,
        TransactionSendRequest {
            from,
            to,
            data: string_arg(arguments, "data")?,
            value,
            block: optional_u64(arguments, "block")?,
            timeout: std::time::Duration::from_secs(timeout),
            policy,
        },
    )
    .await?;
    serde_json::to_value(response).map_err(|error| CapabilityError::internal(error.to_string()))
}

async fn signature_inspect_tool(arguments: &Value) -> Result<Value, CapabilityError> {
    let from = arguments
        .get("from")
        .and_then(Value::as_str)
        .map(|value| {
            value
                .parse::<alloy_primitives::Address>()
                .map_err(|_| CapabilityError::invalid_argument("invalid --from address"))
        })
        .transpose()?;
    let typed_data = arguments
        .get("typed_data")
        .filter(|value| !value.is_null())
        .cloned();
    let authorization_list = arguments
        .get("authorization_list")
        .filter(|value| !value.is_null())
        .cloned();
    let user_operation = arguments
        .get("user_operation")
        .filter(|value| !value.is_null())
        .cloned();
    let response = signature_inspect::inspect(SignatureInspectRequest {
        chain: chain_arg(arguments)?,
        from,
        typed_data,
        authorization_list,
        user_operation,
        policy: None,
    })?;
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
                "swap_build",
                "wallet_balance",
                "wallet_assets",
                "wallet_allowance",
                "wallet_collectibles",
                "wallet_connect",
                "wallet_status",
                "wallet_send",
                "wallet_sign",
                "wallet_agent_id",
                "wallet_disconnect",
                "transaction_simulate",
                "transaction_inspect",
                "transaction_send",
                "signature_inspect",
                "fourmeme_agent_tokens",
                "fourmeme_token",
                "yield_discover",
                "yield_build",
                "yield_history"
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
