//! Agent-facing CLI. Commands are thin clients over a warm local runtime.

use crate::agent_id::{self, AgentIdConfig, AgentIdLookup, AgentIdRequest};
use crate::capability::{CapabilityError, CapabilityErrorCode, CapabilityErrorResponse};
use crate::client::{RuntimeClient, request_id};
use crate::swap::quote::QuoteRequest;
use crate::token;
use crate::transaction::{self, SimulateRequest, TransactionSimulateResponse};
use crate::wallet::{
    self, AllowanceRequest, AssetsRequest, BalanceRequest, CollectiblesRequest,
    WalletAllowancesResponse, WalletBalancesResponse, WalletCollectiblesResponse, WalletConfig,
};
use crate::wallet_send::{self, SendRequest};
use crate::walletconnect::{self, Manager, SignPayload, SignRequest};
use fluxion_protocol::wallet::{WalletConnectResponse, WalletStatusResponse};

pub async fn dispatch(args: Vec<String>) -> (Vec<String>, Option<i32>) {
    let Some(first) = args.first().map(String::as_str) else {
        print_help();
        return (args, Some(0));
    };
    match first {
        "serve" | "--config" => {
            eprintln!(
                "serve is provided by fluxion-remote; run `fluxion-remote serve --config PATH`"
            );
            (args, Some(2))
        }
        "help" | "--help" | "-h" => {
            print_help();
            (args, Some(0))
        }
        "capability" => {
            let code = run_capability(&args[1..]).await;
            (args, Some(code))
        }
        "wallet" => {
            let code = run_wallet(&args[1..]).await;
            (args, Some(code))
        }
        "transaction" => {
            let code = run_transaction(&args[1..]).await;
            (args, Some(code))
        }
        "tokens" => {
            print_tokens();
            (args, Some(0))
        }
        "mcp" => {
            let code = run_mcp(&args[1..]).await;
            (args, Some(code))
        }
        "swap" => {
            let code = match args.get(1).map(String::as_str) {
                Some("quote") => run_quote(&args[2..]).await,
                Some(value) if !value.starts_with('-') => run_alias(&args[1..]).await,
                _ => run_quote(&args[1..]).await,
            };
            (args, Some(code))
        }
        other if !other.starts_with('-') && other.contains('-') => {
            let code = run_alias(&args).await;
            (args, Some(code))
        }
        other if other.starts_with('-') => {
            eprintln!("unknown option: {other}");
            (args, Some(2))
        }
        other => {
            eprintln!("unknown command: {other}");
            print_help();
            (args, Some(2))
        }
    }
}

fn print_help() {
    println!(
        "Fluxion — Web3 agent capabilities\n\n\
Usage:\n  \
fluxion swap ldo-eth 100                Shorthand quote\n  \
fluxion ldo-eth 100                     Top-level shorthand alias\n  \
fluxion capability list                  List capabilities\n  \
fluxion capability describe NAME        Show capability and execution details\n  \
fluxion swap quote --in TOKEN --out TOKEN --amount AMOUNT [options]\n  \
fluxion wallet balance ADDRESS [--tokens LIST]\n  \
fluxion wallet assets ADDRESS [--include-unknown]\n  \
fluxion wallet allowance ADDRESS --spender ADDRESS [--tokens LIST]\n  \
fluxion wallet collectibles ADDRESS [--collections LIST]\n  \
fluxion wallet connect                    Link a wallet with WalletConnect\n  \
fluxion wallet status [--wait N]          Show the linked wallet session\n  \
fluxion wallet disconnect                 Remove the linked wallet session\n  \
fluxion wallet agent-id HANDLE|ADDRESS    Resolve a BSC .ai Agent ID\n  \
fluxion wallet sign --message TEXT        Sign a message with WalletConnect\n  \
fluxion wallet sign --typed-data FILE     Sign EIP-712 typed data\n  \
fluxion wallet send --to ADDR --token T --amount A  Send ETH/ERC20\n  \
fluxion transaction simulate --to ADDRESS --data HEX [--from ADDRESS]\n  \
fluxion mcp [--url URL]                 Expose capabilities over MCP stdio\n  \
fluxion tokens                          List built-in token symbols\n  \
fluxion --help\n\n\
Quote options:\n  \
--json                    Print the swap.quote JSON response\n  \
--url URL                 Remote service override (default FLUXION_REMOTE_URL)\n  \
--no-split                Disable split allocation\n  \
--max-hops N              Maximum hops\n  \
--split-max-branches N    Maximum split branches\n  \
--decimals-in N           Decimals override for token_in\n  \
--decimals-out N          Decimals override for token_out"
    );
}

fn print_tokens() {
    println!("{:<10} {:<44} {:>8}", "SYMBOL", "ADDRESS", "DECIMALS");
    for token in token::list() {
        println!(
            "{:<10} {:<44} {:>8}",
            token.symbol, token.address, token.decimals
        );
    }
}

#[derive(Debug)]
struct QuoteArgs {
    token_in: String,
    token_out: String,
    amount: String,
    json: bool,
    url: Option<String>,
    split: bool,
    max_hops: Option<usize>,
    split_max_branches: Option<usize>,
    decimals_in: Option<u8>,
    decimals_out: Option<u8>,
}

fn parse_quote_args(raw: &[String]) -> Result<QuoteArgs, CapabilityError> {
    let mut token_in = None;
    let mut token_out = None;
    let mut amount = None;
    let mut json = false;
    let mut url = None;
    let mut split = true;
    let mut max_hops = None;
    let mut split_max_branches = None;
    let mut decimals_in = None;
    let mut decimals_out = None;
    let mut index = 0;
    while index < raw.len() {
        let arg = raw[index].as_str();
        match arg {
            "--json" => json = true,
            "--no-split" => split = false,
            "--split" => split = true,
            "--in" => token_in = Some(value(raw, &mut index, "--in")?),
            "--out" => token_out = Some(value(raw, &mut index, "--out")?),
            "--amount" => amount = Some(value(raw, &mut index, "--amount")?),
            "--url" => url = Some(value(raw, &mut index, "--url")?),
            "--max-hops" => {
                max_hops = Some(parse_number(
                    "--max-hops",
                    &value(raw, &mut index, "--max-hops")?,
                )?)
            }
            "--split-max-branches" => {
                split_max_branches = Some(parse_number(
                    "--split-max-branches",
                    &value(raw, &mut index, "--split-max-branches")?,
                )?)
            }
            "--decimals-in" => {
                decimals_in = Some(parse_decimal(
                    "--decimals-in",
                    &value(raw, &mut index, "--decimals-in")?,
                )?)
            }
            "--decimals-out" => {
                decimals_out = Some(parse_decimal(
                    "--decimals-out",
                    &value(raw, &mut index, "--decimals-out")?,
                )?)
            }
            "--help" | "-h" => {
                print_help();
                return Err(CapabilityError::invalid_argument("help requested"));
            }
            other => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown quote option: {other}"
                )));
            }
        }
        index += 1;
    }
    Ok(QuoteArgs {
        token_in: token_in.ok_or_else(|| CapabilityError::invalid_argument("missing --in"))?,
        token_out: token_out.ok_or_else(|| CapabilityError::invalid_argument("missing --out"))?,
        amount: amount.ok_or_else(|| CapabilityError::invalid_argument("missing --amount"))?,
        json,
        url,
        split,
        max_hops,
        split_max_branches,
        decimals_in,
        decimals_out,
    })
}

fn value(raw: &[String], index: &mut usize, name: &str) -> Result<String, CapabilityError> {
    *index += 1;
    raw.get(*index)
        .cloned()
        .ok_or_else(|| CapabilityError::invalid_argument(format!("{name} needs a value")))
}

fn parse_number(name: &str, value: &str) -> Result<usize, CapabilityError> {
    value
        .parse()
        .map_err(|_| CapabilityError::invalid_argument(format!("invalid {name}: {value}")))
}

fn parse_decimal(name: &str, value: &str) -> Result<u8, CapabilityError> {
    value
        .parse()
        .map_err(|_| CapabilityError::invalid_argument(format!("invalid {name}: {value}")))
}

async fn run_alias(args: &[String]) -> i32 {
    let Some(pair) = args.first() else {
        return 2;
    };
    let Some((token_in, token_out)) = pair.split_once('-') else {
        eprintln!("invalid pair: {pair}");
        return 2;
    };
    let mut forwarded = vec![
        "--in".to_string(),
        token_in.to_string(),
        "--out".to_string(),
        token_out.to_string(),
    ];
    let mut rest = &args[1..];
    if let Some(amount) = rest.first().filter(|value| !value.starts_with('-')) {
        forwarded.push("--amount".to_string());
        forwarded.push(amount.clone());
        rest = &rest[1..];
    } else {
        forwarded.push("--amount".to_string());
        forwarded.push("1".to_string());
    }
    forwarded.extend_from_slice(rest);
    run_quote(&forwarded).await
}

async fn run_quote(raw: &[String]) -> i32 {
    let args = match parse_quote_args(raw) {
        Ok(args) => args,
        Err(error) if error.message == "help requested" => return 0,
        Err(error) => {
            let json = raw.iter().any(|arg| arg == "--json");
            report_error(&error, json);
            return exit_code(error.code);
        }
    };
    let token_in = match token::resolve(&args.token_in, args.decimals_in) {
        Ok(token) => token,
        Err(error) => {
            report_error(&error, args.json);
            return exit_code(error.code);
        }
    };
    let token_out = match token::resolve(&args.token_out, args.decimals_out) {
        Ok(token) => token,
        Err(error) => {
            report_error(&error, args.json);
            return exit_code(error.code);
        }
    };
    let amount_in = match token::parse_amount(&args.amount, token_in.decimals) {
        Ok(amount) => amount,
        Err(error) => {
            report_error(&error, args.json);
            return exit_code(error.code);
        }
    };
    let request = QuoteRequest {
        token_in: token_in.address,
        token_out: token_out.address,
        amount_in,
        max_hops: args.max_hops,
        amount_out_min: None,
        split: args.split,
        split_max_branches: args.split_max_branches,
    };
    let backend = match crate::routing::from_env(args.url.as_deref()) {
        Ok(backend) => backend,
        Err(error) => {
            report_error(&error, args.json);
            return exit_code(error.code);
        }
    };
    let client = RuntimeClient::with_api_key(backend.base_url.clone(), backend.api_key.clone());
    match client
        .swap_quote(&request, token_in, token_out, "ethereum", "remote")
        .await
    {
        Ok(response) => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&response).unwrap());
            } else {
                print_human(&response, &args.amount);
            }
            0
        }
        Err(error) => {
            report_error(&error, args.json);
            exit_code(error.code)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WalletMode {
    Balance,
    Assets,
    Allowance,
    Collectibles,
}

#[derive(Debug)]
struct WalletArgs {
    mode: WalletMode,
    address: String,
    spender: Option<String>,
    collections: Vec<String>,
    tokens: Vec<String>,
    json: bool,
    rpc_url: Option<String>,
    block: Option<u64>,
    include_native: bool,
    include_unknown: bool,
    include_empty: bool,
}

async fn run_wallet(raw: &[String]) -> i32 {
    match raw.first().map(String::as_str) {
        Some("connect") => return run_wallet_connect(&raw[1..]).await,
        Some("status") => return run_wallet_status(&raw[1..]).await,
        Some("disconnect") => return run_wallet_disconnect(&raw[1..]).await,
        Some("agent-id") => return run_wallet_agent_id(&raw[1..]).await,
        Some("sign") => return run_wallet_sign(&raw[1..]).await,
        Some("send") => return run_wallet_send(&raw[1..]).await,
        _ => {}
    }
    let args = match parse_wallet_args(raw) {
        Ok(args) => args,
        Err(error) if error.message == "help requested" => {
            println!(
                "Usage:\n  fluxion wallet balance ADDRESS [--tokens ETH,USDC] [--no-native]\n  fluxion wallet assets ADDRESS [--include-unknown] [--no-native]\n  fluxion wallet connect\n  fluxion wallet status\n  fluxion wallet disconnect\n  fluxion wallet agent-id HANDLE|ADDRESS\n  fluxion wallet sign --message TEXT | --typed-data FILE\n  fluxion wallet send --to ADDR --token T --amount A\n\nOptions:\n  --rpc-url URL   local Ethereum RPC (default FLUXION_RPC_URL)\n  --block N       pin a block\n  --json          print the structured response"
            );
            return 0;
        }
        Err(error) => {
            report_error(&error, raw.iter().any(|arg| arg == "--json"));
            return exit_code(error.code);
        }
    };
    let address = match args.address.parse::<alloy_primitives::Address>() {
        Ok(address) => address,
        Err(_) => {
            let error = CapabilityError::invalid_argument(format!(
                "invalid wallet address: {}",
                args.address
            ));
            report_error(&error, args.json);
            return exit_code(error.code);
        }
    };
    let mut config = WalletConfig::from_env();
    if let Some(rpc_url) = args.rpc_url {
        config.rpc_url = rpc_url;
    }
    enum WalletOutput {
        Balances(WalletBalancesResponse),
        Allowances(WalletAllowancesResponse),
        Collectibles(WalletCollectiblesResponse),
    }
    let result = match args.mode {
        WalletMode::Balance => wallet::balance(
            &config,
            BalanceRequest {
                address,
                tokens: args.tokens,
                include_native: args.include_native,
                block: args.block,
            },
        )
        .await
        .map(WalletOutput::Balances),
        WalletMode::Assets => {
            if !args.tokens.is_empty() {
                let error = CapabilityError::invalid_argument(
                    "wallet assets does not accept --tokens; use wallet balance",
                );
                report_error(&error, args.json);
                return exit_code(error.code);
            }
            wallet::assets(
                &config,
                AssetsRequest {
                    address,
                    include_native: args.include_native,
                    include_unknown: args.include_unknown,
                    block: args.block,
                },
            )
            .await
            .map(WalletOutput::Balances)
        }
        WalletMode::Allowance => {
            let Some(spender) = args.spender.as_deref() else {
                let error =
                    CapabilityError::invalid_argument("wallet allowance requires --spender");
                report_error(&error, args.json);
                return exit_code(error.code);
            };
            let spender = match spender.parse::<alloy_primitives::Address>() {
                Ok(spender) => spender,
                Err(_) => {
                    let error = CapabilityError::invalid_argument("invalid --spender address");
                    report_error(&error, args.json);
                    return exit_code(error.code);
                }
            };
            wallet::allowance(
                &config,
                AllowanceRequest {
                    owner: address,
                    spender,
                    tokens: args.tokens,
                    block: args.block,
                },
            )
            .await
            .map(WalletOutput::Allowances)
        }
        WalletMode::Collectibles => {
            let mut collections = Vec::new();
            for collection in args.collections {
                match collection.parse::<alloy_primitives::Address>() {
                    Ok(address) => collections.push(address),
                    Err(_) => {
                        let error = CapabilityError::invalid_argument(format!(
                            "invalid collection address: {collection}"
                        ));
                        report_error(&error, args.json);
                        return exit_code(error.code);
                    }
                }
            }
            wallet::collectibles(
                &config,
                CollectiblesRequest {
                    owner: address,
                    collections,
                    include_empty: args.include_empty,
                    block: args.block,
                },
            )
            .await
            .map(WalletOutput::Collectibles)
        }
    };
    match result {
        Ok(WalletOutput::Balances(response)) => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&response).unwrap());
            } else {
                print_wallet_human(&response);
            }
            0
        }
        Ok(WalletOutput::Allowances(response)) => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&response).unwrap());
            } else {
                print_allowance_human(&response);
            }
            0
        }
        Ok(WalletOutput::Collectibles(response)) => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&response).unwrap());
            } else {
                print_collectibles_human(&response);
            }
            0
        }
        Err(error) => {
            report_error(&error, args.json);
            exit_code(error.code)
        }
    }
}

async fn run_wallet_send(raw: &[String]) -> i32 {
    let args = match parse_wallet_send_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as("wallet.send", &error, raw.iter().any(|arg| arg == "--json"));
            return exit_code(error.code);
        }
    };
    let to = match args.to.parse::<alloy_primitives::Address>() {
        Ok(address) => address,
        Err(_) => {
            let error = CapabilityError::invalid_argument("invalid --to address");
            report_error_as("wallet.send", &error, args.json);
            return exit_code(error.code);
        }
    };
    let from = match args.from {
        Some(value) => match value.parse::<alloy_primitives::Address>() {
            Ok(address) => Some(address),
            Err(_) => {
                let error = CapabilityError::invalid_argument("invalid --from address");
                report_error_as("wallet.send", &error, args.json);
                return exit_code(error.code);
            }
        },
        None => None,
    };
    let mut config = WalletConfig::from_env();
    if let Some(rpc_url) = args.rpc_url {
        config.rpc_url = rpc_url;
    }
    match wallet_send::send(
        &config,
        SendRequest {
            from,
            to,
            token: args.token,
            amount: args.amount,
            decimals: args.decimals,
            block: args.block,
            timeout: std::time::Duration::from_secs(args.timeout_seconds),
        },
    )
    .await
    {
        Ok(response) if args.json => {
            println!("{}", serde_json::to_string_pretty(&response).unwrap());
            0
        }
        Ok(response) => {
            println!("From: {}", response.from);
            println!("To: {}", response.recipient);
            println!(
                "Amount: {} {}",
                response.amount.decimal,
                token_label(&response.token)
            );
            println!("Tx: {}", response.tx_hash);
            0
        }
        Err(error) => {
            report_error_as("wallet.send", &error, args.json);
            exit_code(error.code)
        }
    }
}

struct WalletSendArgs {
    to: String,
    token: String,
    amount: String,
    from: Option<String>,
    decimals: Option<u8>,
    block: Option<u64>,
    rpc_url: Option<String>,
    json: bool,
    timeout_seconds: u64,
}

fn parse_wallet_send_args(raw: &[String]) -> Result<WalletSendArgs, CapabilityError> {
    let mut to = None;
    let mut token = None;
    let mut amount = None;
    let mut from = None;
    let mut decimals = None;
    let mut block = None;
    let mut rpc_url = None;
    let mut json = false;
    let mut timeout_seconds = 90;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--json" => json = true,
            "--to" => {
                index += 1;
                to = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--to needs a value"))?
                        .clone(),
                );
            }
            "--token" => {
                index += 1;
                token = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--token needs a value"))?
                        .clone(),
                );
            }
            "--amount" => {
                index += 1;
                amount = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--amount needs a value"))?
                        .clone(),
                );
            }
            "--from" => {
                index += 1;
                from = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--from needs a value"))?
                        .clone(),
                );
            }
            "--decimals" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--decimals needs a value"))?;
                decimals = Some(value.parse().map_err(|_| {
                    CapabilityError::invalid_argument(format!("invalid --decimals: {value}"))
                })?);
            }
            "--block" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--block needs a value"))?;
                block = Some(value.parse().map_err(|_| {
                    CapabilityError::invalid_argument(format!("invalid --block: {value}"))
                })?);
            }
            "--rpc-url" => {
                index += 1;
                rpc_url = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--rpc-url needs a value")
                        })?
                        .clone(),
                );
            }
            "--timeout" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--timeout needs a value"))?;
                timeout_seconds = value.parse().map_err(|_| {
                    CapabilityError::invalid_argument(format!("invalid --timeout: {value}"))
                })?;
                if timeout_seconds == 0 || timeout_seconds > 300 {
                    return Err(CapabilityError::invalid_argument(
                        "--timeout must be between 1 and 300 seconds",
                    ));
                }
            }
            arg if arg.starts_with('-') => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown wallet option: {arg}"
                )));
            }
            arg => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unexpected wallet send argument: {arg}"
                )));
            }
        }
        index += 1;
    }
    Ok(WalletSendArgs {
        to: to.ok_or_else(|| CapabilityError::invalid_argument("wallet send requires --to"))?,
        token: token
            .ok_or_else(|| CapabilityError::invalid_argument("wallet send requires --token"))?,
        amount: amount
            .ok_or_else(|| CapabilityError::invalid_argument("wallet send requires --amount"))?,
        from,
        decimals,
        block,
        rpc_url,
        json,
        timeout_seconds,
    })
}

async fn run_wallet_sign(raw: &[String]) -> i32 {
    let args = match parse_wallet_sign_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as("wallet.sign", &error, raw.iter().any(|arg| arg == "--json"));
            return exit_code(error.code);
        }
    };
    let payload = match (args.message, args.typed_data) {
        (Some(message), None) => SignPayload::Message(message),
        (None, Some(typed_data)) => SignPayload::TypedData(typed_data),
        _ => {
            let error = CapabilityError::invalid_argument(
                "wallet sign needs exactly one of --message or --typed-data",
            );
            report_error_as("wallet.sign", &error, args.json);
            return exit_code(error.code);
        }
    };
    match walletconnect::sign(SignRequest {
        address: args.address,
        payload,
        timeout: std::time::Duration::from_secs(args.timeout_seconds),
    })
    .await
    {
        Ok(response) if args.json => {
            println!("{}", serde_json::to_string_pretty(&response).unwrap());
            0
        }
        Ok(response) => {
            println!("Address: {}", response.address);
            println!("Method: {}", response.method);
            println!("Signature: {}", response.signature);
            0
        }
        Err(error) => {
            report_error_as("wallet.sign", &error, args.json);
            exit_code(error.code)
        }
    }
}

struct WalletSignArgs {
    message: Option<String>,
    typed_data: Option<serde_json::Value>,
    address: Option<alloy_primitives::Address>,
    json: bool,
    timeout_seconds: u64,
}

fn parse_wallet_sign_args(raw: &[String]) -> Result<WalletSignArgs, CapabilityError> {
    let mut message = None;
    let mut typed_data = None;
    let mut address = None;
    let mut json = false;
    let mut timeout_seconds = 90;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--json" => json = true,
            "--message" => {
                index += 1;
                message = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--message needs a value")
                        })?
                        .clone(),
                );
            }
            "--typed-data" => {
                index += 1;
                let path = raw.get(index).ok_or_else(|| {
                    CapabilityError::invalid_argument("--typed-data needs a file")
                })?;
                let text = std::fs::read_to_string(path).map_err(|error| {
                    CapabilityError::invalid_argument(format!("cannot read {path}: {error}"))
                })?;
                typed_data = Some(serde_json::from_str(&text).map_err(|error| {
                    CapabilityError::invalid_argument(format!(
                        "invalid typed data in {path}: {error}"
                    ))
                })?);
            }
            "--typed-data-json" => {
                index += 1;
                let text = raw.get(index).ok_or_else(|| {
                    CapabilityError::invalid_argument("--typed-data-json needs a value")
                })?;
                typed_data = Some(serde_json::from_str(text).map_err(|error| {
                    CapabilityError::invalid_argument(format!("invalid typed data: {error}"))
                })?);
            }
            "--address" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--address needs a value"))?;
                address = Some(
                    value
                        .parse()
                        .map_err(|_| CapabilityError::invalid_argument("invalid --address"))?,
                );
            }
            "--timeout" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--timeout needs a value"))?;
                timeout_seconds = value.parse().map_err(|_| {
                    CapabilityError::invalid_argument(format!("invalid --timeout: {value}"))
                })?;
                if timeout_seconds == 0 || timeout_seconds > 300 {
                    return Err(CapabilityError::invalid_argument(
                        "--timeout must be between 1 and 300 seconds",
                    ));
                }
            }
            arg if arg.starts_with('-') => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown wallet option: {arg}"
                )));
            }
            arg => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unexpected wallet sign argument: {arg}"
                )));
            }
        }
        index += 1;
    }
    Ok(WalletSignArgs {
        message,
        typed_data,
        address,
        json,
        timeout_seconds,
    })
}

async fn run_wallet_agent_id(raw: &[String]) -> i32 {
    let args = match parse_wallet_agent_id_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as(
                "wallet.agent-id",
                &error,
                raw.iter().any(|arg| arg == "--json"),
            );
            return exit_code(error.code);
        }
    };
    let lookup = if args.value.starts_with("0x") || args.value.starts_with("0X") {
        match args.value.parse::<alloy_primitives::Address>() {
            Ok(address) => AgentIdLookup::Address(address),
            Err(_) => {
                let error = CapabilityError::invalid_argument("invalid Agent ID address");
                report_error_as("wallet.agent-id", &error, args.json);
                return exit_code(error.code);
            }
        }
    } else {
        AgentIdLookup::Handle(args.value)
    };
    let mut config = AgentIdConfig::from_env();
    if let Some(rpc_url) = args.rpc_url {
        config.bsc_rpc_url = rpc_url;
    }
    match agent_id::resolve(&config, AgentIdRequest { lookup }).await {
        Ok(response) if args.json => {
            println!("{}", serde_json::to_string_pretty(&response).unwrap());
            0
        }
        Ok(response) => {
            if response.found {
                println!("Agent ID: {}", response.agent_id.as_deref().unwrap_or("-"));
                if let Some(owner) = response.owner {
                    println!("Owner: {owner}");
                }
                if let Some(token_id) = &response.token_id {
                    println!("Token ID: {token_id}");
                }
            } else {
                println!("No Agent ID found for {}", response.query.value);
            }
            println!("Coverage: {}", response.coverage);
            for warning in &response.warnings {
                eprintln!("warning: {warning}");
            }
            0
        }
        Err(error) => {
            report_error_as("wallet.agent-id", &error, args.json);
            exit_code(error.code)
        }
    }
}

struct WalletAgentIdArgs {
    value: String,
    json: bool,
    rpc_url: Option<String>,
}

fn parse_wallet_agent_id_args(raw: &[String]) -> Result<WalletAgentIdArgs, CapabilityError> {
    let mut value = None;
    let mut json = false;
    let mut rpc_url = None;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--json" => json = true,
            "--rpc-url" => {
                index += 1;
                rpc_url = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--rpc-url needs a value")
                        })?
                        .clone(),
                );
            }
            arg if arg.starts_with('-') => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown wallet option: {arg}"
                )));
            }
            arg => {
                if value.is_some() {
                    return Err(CapabilityError::invalid_argument(
                        "wallet agent-id accepts one handle or address",
                    ));
                }
                value = Some(arg.to_string());
            }
        }
        index += 1;
    }
    Ok(WalletAgentIdArgs {
        value: value.ok_or_else(|| {
            CapabilityError::invalid_argument("wallet agent-id needs a handle or address")
        })?,
        json,
        rpc_url,
    })
}

async fn run_wallet_connect(raw: &[String]) -> i32 {
    let json = match parse_wallet_session_args(raw) {
        Ok(json) => json,
        Err(error) => {
            report_error(&error, raw.iter().any(|arg| arg == "--json"));
            return exit_code(error.code);
        }
    };
    let manager = Manager::new();
    let response = match manager.connect().await {
        Ok(response) => response,
        Err(error) => {
            report_error(&error, json);
            return exit_code(error.code);
        }
    };

    if !json && response.status == "pending" {
        println!("Scan with MetaMask or another WalletConnect wallet:\n");
        println!("{}", response.qr_ascii);
        println!();
        println!("URI: {}", response.uri);
        println!();
        println!("waiting for the wallet to approve...");
    }

    let status = match wait_for_wallet(&manager, response).await {
        Ok(status) => status,
        Err(error) => {
            report_error(&error, json);
            return exit_code(error.code);
        }
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&status).unwrap());
    } else {
        print_wallet_status_human(&status);
    }
    0
}

async fn wait_for_wallet(
    manager: &Manager,
    response: WalletConnectResponse,
) -> Result<WalletStatusResponse, CapabilityError> {
    if response.status == "connected" {
        return walletconnect::status();
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
    loop {
        let status = manager.status().await?;
        if status.status != "pending" {
            return Ok(status);
        }
        if std::time::Instant::now() >= deadline {
            return Err(CapabilityError::new(
                CapabilityErrorCode::Timeout,
                "WalletConnect pairing timed out",
                true,
            ));
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn run_wallet_status(raw: &[String]) -> i32 {
    let (json, wait_seconds) = match parse_wallet_status_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error(&error, raw.iter().any(|arg| arg == "--json"));
            return exit_code(error.code);
        }
    };
    let result = if wait_seconds == 0 {
        walletconnect::status()
    } else {
        walletconnect::wait_for_session(std::time::Duration::from_secs(wait_seconds)).await
    };
    match result {
        Ok(status) if json => {
            println!("{}", serde_json::to_string_pretty(&status).unwrap());
            0
        }
        Ok(status) => {
            print_wallet_status_human(&status);
            0
        }
        Err(error) => {
            report_error(&error, json);
            exit_code(error.code)
        }
    }
}

async fn run_wallet_disconnect(raw: &[String]) -> i32 {
    let json = match parse_wallet_session_args(raw) {
        Ok(json) => json,
        Err(error) => {
            report_error(&error, raw.iter().any(|arg| arg == "--json"));
            return exit_code(error.code);
        }
    };
    match walletconnect::disconnect() {
        Ok(response) if json => {
            println!("{}", serde_json::to_string_pretty(&response).unwrap());
            0
        }
        Ok(_) => {
            println!("wallet disconnected");
            0
        }
        Err(error) => {
            report_error(&error, json);
            exit_code(error.code)
        }
    }
}

fn parse_wallet_status_args(raw: &[String]) -> Result<(bool, u64), CapabilityError> {
    let mut json = false;
    let mut wait_seconds = 0;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--json" => json = true,
            "--wait" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--wait needs a value"))?;
                wait_seconds = value.parse().map_err(|_| {
                    CapabilityError::invalid_argument(format!("invalid --wait: {value}"))
                })?;
                if wait_seconds > 300 {
                    return Err(CapabilityError::invalid_argument(
                        "--wait must be 300 seconds or less",
                    ));
                }
            }
            value => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown wallet option: {value}"
                )));
            }
        }
        index += 1;
    }
    Ok((json, wait_seconds))
}

fn parse_wallet_session_args(raw: &[String]) -> Result<bool, CapabilityError> {
    let mut json = false;
    for arg in raw {
        match arg.as_str() {
            "--json" => json = true,
            value => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown wallet option: {value}"
                )));
            }
        }
    }
    Ok(json)
}

fn print_wallet_status_human(status: &WalletStatusResponse) {
    println!("status: {}", status.status);
    if let Some(chain_id) = status.chain_id {
        println!("chain: eip155:{chain_id}");
    }
    if let Some(expires_at) = status.expires_at {
        println!("expires_at: {expires_at}");
    }
    if let Some(wallet) = &status.wallet {
        println!("wallet: {} {}", wallet.name, wallet.url);
    }
    for account in &status.accounts {
        println!("account: {account}");
    }
    if let Some(error) = &status.error {
        eprintln!("error: {error}");
    }
}

fn parse_wallet_args(raw: &[String]) -> Result<WalletArgs, CapabilityError> {
    let mode = match raw.first().map(String::as_str) {
        Some("balance") => WalletMode::Balance,
        Some("assets") => WalletMode::Assets,
        Some("allowance") => WalletMode::Allowance,
        Some("collectibles") => WalletMode::Collectibles,
        Some("--help" | "-h") => return Err(CapabilityError::invalid_argument("help requested")),
        Some(other) => {
            return Err(CapabilityError::invalid_argument(format!(
                "unknown wallet command: {other}"
            )));
        }
        None => return Err(CapabilityError::invalid_argument("missing wallet command")),
    };
    let mut address = None;
    let mut spender = None;
    let mut collections = Vec::new();
    let mut tokens = Vec::new();
    let mut include_empty = false;
    let mut json = false;
    let mut rpc_url = None;
    let mut block = None;
    let mut include_native = true;
    let mut include_unknown = false;
    let mut index = 1;
    while index < raw.len() {
        match raw[index].as_str() {
            "--address" => {
                index += 1;
                address = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--address needs a value")
                        })?
                        .clone(),
                );
            }
            "--collections" => {
                index += 1;
                let value = raw.get(index).ok_or_else(|| {
                    CapabilityError::invalid_argument("--collections needs a value")
                })?;
                collections.extend(
                    value
                        .split(',')
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                );
            }
            "--include-empty" => include_empty = true,
            "--spender" => {
                index += 1;
                spender = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--spender needs a value")
                        })?
                        .clone(),
                );
            }
            "--tokens" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--tokens needs a value"))?;
                tokens.extend(
                    value
                        .split(',')
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                );
            }
            "--token" => {
                index += 1;
                tokens.push(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--token needs a value"))?
                        .clone(),
                );
            }
            "--rpc-url" => {
                index += 1;
                rpc_url = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--rpc-url needs a value")
                        })?
                        .clone(),
                );
            }
            "--block" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--block needs a value"))?;
                block = Some(value.parse().map_err(|_| {
                    CapabilityError::invalid_argument(format!("invalid --block: {value}"))
                })?);
            }
            "--json" => json = true,
            "--no-native" => include_native = false,
            "--include-unknown" => include_unknown = true,
            value if value.starts_with('-') => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown wallet option: {value}"
                )));
            }
            value => {
                if address.is_some() {
                    return Err(CapabilityError::invalid_argument(
                        "wallet accepts one address",
                    ));
                }
                address = Some(value.to_string());
            }
        }
        index += 1;
    }
    let address =
        address.ok_or_else(|| CapabilityError::invalid_argument("missing wallet address"))?;
    if mode != WalletMode::Allowance && spender.is_some() {
        return Err(CapabilityError::invalid_argument(
            "--spender is only valid for wallet allowance",
        ));
    }
    if mode != WalletMode::Collectibles && !collections.is_empty() {
        return Err(CapabilityError::invalid_argument(
            "--collections is only valid for wallet collectibles",
        ));
    }
    Ok(WalletArgs {
        mode,
        address,
        spender,
        collections,
        include_empty,
        tokens,
        json,
        rpc_url,
        block,
        include_native,
        include_unknown,
    })
}

fn print_wallet_human(response: &WalletBalancesResponse) {
    println!(
        "{} · block {} · coverage {}",
        response.address, response.block_number, response.coverage
    );
    if let Some(native) = &response.native {
        println!(
            "{:<10} {:>24}  {}",
            token_label(&native.token),
            native.amount.decimal,
            native.token.address
        );
    }
    for token in &response.tokens {
        println!(
            "{:<10} {:>24}  {}",
            token_label(&token.token),
            token.amount.decimal,
            token.token.address
        );
    }
    if response.unknown_count > 0 {
        println!("unknown tokens hidden: {}", response.unknown_count);
    }
    for warning in &response.warnings {
        eprintln!("warning: {warning}");
    }
}

#[derive(Debug)]
struct TransactionArgs {
    from: Option<String>,
    to: String,
    data: String,
    value: String,
    block: Option<u64>,
    rpc_url: Option<String>,
    json: bool,
}

async fn run_transaction(raw: &[String]) -> i32 {
    if raw.first().map(String::as_str) == Some("--help")
        || raw.first().map(String::as_str) == Some("-h")
    {
        println!(
            "Usage:\n  fluxion transaction simulate --to ADDRESS --data 0x... [--from ADDRESS] [--value ETH] [--block N] [--json]"
        );
        return 0;
    }
    if raw.first().map(String::as_str) != Some("simulate") {
        let error = CapabilityError::invalid_argument("expected: transaction simulate");
        report_error(&error, raw.iter().any(|arg| arg == "--json"));
        return exit_code(error.code);
    }
    let args = match parse_transaction_args(&raw[1..]) {
        Ok(args) => args,
        Err(error) => {
            report_error(&error, raw.iter().any(|arg| arg == "--json"));
            return exit_code(error.code);
        }
    };
    let to = match args.to.parse::<alloy_primitives::Address>() {
        Ok(address) => address,
        Err(_) => {
            let error = CapabilityError::invalid_argument("invalid --to address");
            report_error(&error, args.json);
            return exit_code(error.code);
        }
    };
    let from = match args.from.as_deref() {
        Some(from) => match from.parse::<alloy_primitives::Address>() {
            Ok(address) => Some(address),
            Err(_) => {
                let error = CapabilityError::invalid_argument("invalid --from address");
                report_error(&error, args.json);
                return exit_code(error.code);
            }
        },
        None => None,
    };
    let value = match token::parse_amount(&args.value, 18) {
        Ok(value) => value,
        Err(error) => {
            report_error(&error, args.json);
            return exit_code(error.code);
        }
    };
    let mut config = WalletConfig::from_env();
    if let Some(rpc_url) = args.rpc_url {
        config.rpc_url = rpc_url;
    }
    match transaction::simulate(
        &config,
        SimulateRequest {
            from,
            to,
            data: args.data,
            value,
            block: args.block,
        },
    )
    .await
    {
        Ok(response) => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&response).unwrap());
            } else {
                print_transaction_human(&response);
            }
            0
        }
        Err(error) => {
            report_error(&error, args.json);
            exit_code(error.code)
        }
    }
}

fn parse_transaction_args(raw: &[String]) -> Result<TransactionArgs, CapabilityError> {
    let mut from = None;
    let mut to = None;
    let mut data = None;
    let mut value = "0".to_string();
    let mut block = None;
    let mut rpc_url = None;
    let mut json = false;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--from" => {
                index += 1;
                from = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--from needs a value"))?
                        .clone(),
                );
            }
            "--to" => {
                index += 1;
                to = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--to needs a value"))?
                        .clone(),
                );
            }
            "--data" => {
                index += 1;
                data = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--data needs a value"))?
                        .clone(),
                );
            }
            "--value" => {
                index += 1;
                value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--value needs a value"))?
                    .clone();
            }
            "--block" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--block needs a value"))?;
                block = Some(value.parse().map_err(|_| {
                    CapabilityError::invalid_argument(format!("invalid --block: {value}"))
                })?);
            }
            "--rpc-url" => {
                index += 1;
                rpc_url = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--rpc-url needs a value")
                        })?
                        .clone(),
                );
            }
            "--json" => json = true,
            value if value.starts_with('-') => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown transaction option: {value}"
                )));
            }
            value => {
                if to.is_some() {
                    return Err(CapabilityError::invalid_argument(
                        "transaction accepts one positional address",
                    ));
                }
                to = Some(value.to_string());
            }
        }
        index += 1;
    }
    Ok(TransactionArgs {
        from,
        to: to.ok_or_else(|| CapabilityError::invalid_argument("missing --to"))?,
        data: data.ok_or_else(|| CapabilityError::invalid_argument("missing --data"))?,
        value,
        block,
        rpc_url,
        json,
    })
}

fn print_transaction_human(response: &TransactionSimulateResponse) {
    println!(
        "{} · block {}{}",
        if response.success {
            "success"
        } else {
            "reverted"
        },
        response.block_number,
        response
            .gas_estimate
            .as_ref()
            .map(|gas| format!(" · gas {gas}"))
            .unwrap_or_default()
    );
    if let Some(error) = &response.error {
        eprintln!("error: {error}");
    }
    if response.return_data != "0x" {
        println!("return: {}", response.return_data);
    }
}

fn print_allowance_human(response: &WalletAllowancesResponse) {
    println!(
        "{} -> {} · block {} · coverage {}",
        response.owner, response.spender, response.block_number, response.coverage
    );
    for entry in &response.allowances {
        println!(
            "{:<10} {:>24}  {}",
            token_label(&entry.token),
            entry.amount.decimal,
            entry.token.address
        );
    }
    for warning in &response.warnings {
        eprintln!("warning: {warning}");
    }
}

fn print_collectibles_human(response: &WalletCollectiblesResponse) {
    println!(
        "{} · block {} · coverage {}",
        response.owner, response.block_number, response.coverage
    );
    for collection in &response.collections {
        println!(
            "{} {} ({}) · {} items{}",
            collection
                .symbol
                .as_deref()
                .or(collection.name.as_deref())
                .unwrap_or("collection"),
            collection.collection,
            collection.standard,
            collection.item_count,
            if collection.enumerable {
                ""
            } else {
                " · count only"
            }
        );
        for item in collection.items.iter().take(20) {
            println!("  #{} × {}", item.token_id, item.balance);
        }
        if collection.items.len() > 20 {
            println!("  ... {} more", collection.items.len() - 20);
        }
    }
    for warning in &response.warnings {
        eprintln!("warning: {warning}");
    }
}

fn token_label(token: &crate::capability::TokenRef) -> String {
    token
        .symbol
        .clone()
        .unwrap_or_else(|| token.address.to_string())
}

fn print_human(response: &crate::swap::quote::QuoteResponse, amount: &str) {
    println!(
        "{} → {}",
        token_label(&response.token_in),
        token_label(&response.token_out)
    );
    println!(
        "{} {} → {} {}",
        amount,
        token_label(&response.token_in),
        response.amount_out.decimal,
        token_label(&response.token_out)
    );
    let block = response
        .provenance
        .quote_block
        .map_or_else(|| "-".to_string(), |block| block.to_string());
    let lag = response
        .provenance
        .lag_blocks
        .map_or_else(|| "-".to_string(), |lag| lag.to_string());
    println!(
        "plan: {} · block {} · lag {} · {} ms",
        response.plan_kind.as_str(),
        block,
        lag,
        response.latency_ms
    );
}

fn report_error(error: &CapabilityError, json: bool) {
    report_error_as("swap.quote", error, json);
}

fn report_error_as(capability: &str, error: &CapabilityError, json: bool) {
    if json {
        let response = CapabilityErrorResponse::new(capability, request_id(), error.clone());
        println!("{}", serde_json::to_string_pretty(&response).unwrap());
    } else {
        eprintln!("{}: {}", error.code_string(), error.message);
    }
}

fn exit_code(code: CapabilityErrorCode) -> i32 {
    match code {
        CapabilityErrorCode::InvalidArgument | CapabilityErrorCode::AmountOutBelowMin => 2,
        CapabilityErrorCode::NoPath | CapabilityErrorCode::InsufficientLiquidity => 3,
        CapabilityErrorCode::RpcError | CapabilityErrorCode::Unsupported => 4,
        CapabilityErrorCode::StaleState | CapabilityErrorCode::StateIncomplete => 5,
        CapabilityErrorCode::Timeout => 6,
        CapabilityErrorCode::UserRejected => 7,
        CapabilityErrorCode::SimulationFailed => 8,
        CapabilityErrorCode::Internal => 1,
    }
}

async fn run_capability(raw: &[String]) -> i32 {
    match raw.first().map(String::as_str) {
        Some("list") => {
            let json = raw.iter().any(|arg| arg == "--json");
            if json {
                let value = serde_json::json!({
                    "schema_version": crate::capability::SCHEMA_VERSION,
                    "capabilities": crate::capability::names().collect::<Vec<_>>(),
                });
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
            } else {
                for name in crate::capability::names() {
                    println!("{name}");
                }
            }
            0
        }
        Some("describe") => {
            let Some(name) = raw.get(1).filter(|value| !value.starts_with('-')) else {
                eprintln!("usage: fluxion capability describe NAME [--json]");
                return 2;
            };
            let json = raw.iter().any(|arg| arg == "--json");
            let descriptor = match name.as_str() {
                "swap.quote" => crate::swap::describe(),
                "wallet.balance" => fluxion_protocol::wallet::describe_balance(),
                "wallet.assets" => fluxion_protocol::wallet::describe_assets(),
                "wallet.allowance" => fluxion_protocol::wallet::describe_allowance(),
                "wallet.collectibles" => fluxion_protocol::wallet::describe_collectibles(),
                "wallet.connect" => fluxion_protocol::wallet::describe_connect(),
                "wallet.status" => fluxion_protocol::wallet::describe_status(),
                "wallet.disconnect" => fluxion_protocol::wallet::describe_disconnect(),
                "wallet.agent-id" => fluxion_protocol::wallet::describe_agent_id(),
                "wallet.sign" => fluxion_protocol::wallet::describe_sign(),
                "wallet.send" => fluxion_protocol::wallet::describe_send(),
                "transaction.simulate" => fluxion_protocol::transaction::describe(),
                other => {
                    let error = CapabilityError::new(
                        CapabilityErrorCode::Unsupported,
                        format!("unknown capability: {other}"),
                        false,
                    );
                    report_error(&error, json);
                    return exit_code(error.code);
                }
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&descriptor).unwrap());
            } else {
                println!(
                    "{} (schema v{})",
                    descriptor.name, descriptor.schema_version
                );
                println!("  execution: {}", descriptor.execution);
                println!("  transports: {}", descriptor.transports.join(", "));
            }
            0
        }
        Some("--help") | Some("-h") | None => {
            print_help();
            0
        }
        Some(other) => {
            eprintln!("unknown capability command: {other}");
            2
        }
    }
}

async fn run_mcp(raw: &[String]) -> i32 {
    let mut url = None;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--url" => {
                index += 1;
                let Some(value) = raw.get(index) else {
                    eprintln!("--url needs a value");
                    return 2;
                };
                url = Some(value.clone());
            }
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            other => {
                eprintln!("unknown mcp option: {other}");
                return 2;
            }
        }
        index += 1;
    }
    let backend = crate::routing::from_env(url.as_deref()).map(|backend| {
        RuntimeClient::with_api_key(backend.base_url.clone(), backend.api_key.clone())
    });
    crate::mcp::run_stdio(backend).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_alias_shape() {
        let args = [
            "ldo-eth".to_string(),
            "100".to_string(),
            "--json".to_string(),
        ];
        assert_eq!(args[0], "ldo-eth");
        let (token_in, token_out) = args[0].split_once('-').unwrap();
        assert_eq!((token_in, token_out), ("ldo", "eth"));
    }

    #[test]
    fn parses_wallet_session_arguments() {
        assert!(parse_wallet_session_args(&["--json".to_string()]).unwrap());
        assert!(!parse_wallet_session_args(&[]).unwrap());
        assert!(parse_wallet_session_args(&["--wat".to_string()]).is_err());
        assert_eq!(
            parse_wallet_status_args(&[
                "--json".to_string(),
                "--wait".to_string(),
                "90".to_string()
            ])
            .unwrap(),
            (true, 90)
        );
        assert!(parse_wallet_status_args(&["--wait".to_string()]).is_err());
        assert!(parse_wallet_status_args(&["--wait".to_string(), "301".to_string()]).is_err());
    }

    #[test]
    fn parses_wallet_agent_id_arguments() {
        let args = parse_wallet_agent_id_args(&[
            "alice.ai".to_string(),
            "--json".to_string(),
            "--rpc-url".to_string(),
            "http://127.0.0.1:8545".to_string(),
        ])
        .unwrap();
        assert_eq!(args.value, "alice.ai");
        assert!(args.json);
        assert_eq!(args.rpc_url.as_deref(), Some("http://127.0.0.1:8545"));
        assert!(parse_wallet_agent_id_args(&[]).is_err());
        assert!(parse_wallet_agent_id_args(&["a".to_string(), "b".to_string()]).is_err());
    }

    #[test]
    fn parses_wallet_sign_arguments() {
        let args = parse_wallet_sign_args(&[
            "--message".to_string(),
            "hello".to_string(),
            "--timeout".to_string(),
            "42".to_string(),
            "--json".to_string(),
        ])
        .unwrap();
        assert_eq!(args.message.as_deref(), Some("hello"));
        assert!(args.typed_data.is_none());
        assert!(args.json);
        assert_eq!(args.timeout_seconds, 42);
        assert!(
            parse_wallet_sign_args(&[
                "--message".to_string(),
                "hello".to_string(),
                "--typed-data-json".to_string(),
                "{}".to_string()
            ])
            .is_ok()
        );
        assert!(parse_wallet_sign_args(&[]).is_ok());
        assert!(parse_wallet_sign_args(&["--timeout".to_string(), "0".to_string()]).is_err());
    }

    #[test]
    fn parses_wallet_send_arguments() {
        let args = parse_wallet_send_args(&[
            "--to".to_string(),
            "0x0000000000000000000000000000000000000001".to_string(),
            "--token".to_string(),
            "ETH".to_string(),
            "--amount".to_string(),
            "0.1".to_string(),
            "--timeout".to_string(),
            "42".to_string(),
        ])
        .unwrap();
        assert_eq!(args.token, "ETH");
        assert_eq!(args.amount, "0.1");
        assert_eq!(args.timeout_seconds, 42);
        assert!(parse_wallet_send_args(&[]).is_err());
    }

    #[test]
    fn parses_wallet_arguments() {
        let assets = parse_wallet_args(&[
            "assets".to_string(),
            "0x0000000000000000000000000000000000000001".to_string(),
            "--include-unknown".to_string(),
            "--no-native".to_string(),
            "--block".to_string(),
            "123".to_string(),
            "--json".to_string(),
        ])
        .unwrap();
        assert_eq!(assets.mode, WalletMode::Assets);
        assert_eq!(assets.block, Some(123));
        assert!(assets.include_unknown);
        assert!(!assets.include_native);
        assert!(assets.json);

        let collectibles = parse_wallet_args(&[
            "collectibles".to_string(),
            "0x0000000000000000000000000000000000000001".to_string(),
            "--collections".to_string(),
            "0x0000000000000000000000000000000000000010".to_string(),
            "--include-empty".to_string(),
        ])
        .unwrap();
        assert_eq!(collectibles.mode, WalletMode::Collectibles);
        assert_eq!(collectibles.collections.len(), 1);
        assert!(collectibles.include_empty);

        let allowance = parse_wallet_args(&[
            "allowance".to_string(),
            "0x0000000000000000000000000000000000000001".to_string(),
            "--spender".to_string(),
            "0x0000000000000000000000000000000000000002".to_string(),
            "--tokens".to_string(),
            "USDC".to_string(),
        ])
        .unwrap();
        assert_eq!(allowance.mode, WalletMode::Allowance);
        assert_eq!(
            allowance.spender.as_deref(),
            Some("0x0000000000000000000000000000000000000002")
        );

        let balance = parse_wallet_args(&[
            "balance".to_string(),
            "0x0000000000000000000000000000000000000001".to_string(),
            "--tokens".to_string(),
            "USDC,LDO".to_string(),
        ])
        .unwrap();
        assert_eq!(balance.mode, WalletMode::Balance);
        assert_eq!(balance.tokens, vec!["USDC", "LDO"]);
    }

    #[test]
    fn parses_transaction_arguments() {
        let parsed = parse_transaction_args(&[
            "--to".to_string(),
            "0x0000000000000000000000000000000000000001".to_string(),
            "--data".to_string(),
            "0x1234".to_string(),
            "--value".to_string(),
            "0.5".to_string(),
            "--block".to_string(),
            "100".to_string(),
        ])
        .unwrap();
        assert_eq!(parsed.value, "0.5");
        assert_eq!(parsed.block, Some(100));
        assert!(parse_transaction_args(&["--data".to_string(), "0x".to_string()]).is_err());
    }

    #[test]
    fn parses_quote_arguments() {
        let args = [
            "--in".to_string(),
            "LDO".to_string(),
            "--out".to_string(),
            "ETH".to_string(),
            "--amount".to_string(),
            "100".to_string(),
            "--json".to_string(),
            "--no-split".to_string(),
        ];
        let parsed = parse_quote_args(&args).unwrap();
        assert_eq!(parsed.token_in, "LDO");
        assert_eq!(parsed.amount, "100");
        assert!(parsed.json);
        assert!(!parsed.split);
    }
}
