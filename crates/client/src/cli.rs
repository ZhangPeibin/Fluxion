//! Agent-facing CLI. Commands are thin clients over a warm local runtime.

use crate::agent_id::{self, AgentIdConfig, AgentIdLookup, AgentIdRequest};
use crate::capability::{
    CapabilityEntry, CapabilityError, CapabilityErrorCode, CapabilityErrorResponse,
    CapabilityRegistry, uses_v2, wrap_error, wrap_success,
};
use crate::chain::{Chain, DEFAULT_CHAIN};
use crate::client::{RuntimeClient, request_id};
use crate::fourmeme::{self, FourMemeConfig, ListRequest};
use crate::fourmeme_ui;
use crate::signature_inspect::{self, SignatureInspectRequest, SignatureInspectResponse};
use crate::swap::quote::QuoteRequest;
use crate::swap_build::{self, BuildRequest};
use crate::token;
use crate::transaction::{self, SimulateRequest, TransactionSimulateResponse};
use crate::transaction_inspect::{self, InspectPolicy, InspectRequest, TransactionInspectResponse};
use crate::transaction_send::{self, SendRequest as TransactionSendRequest};
use crate::transaction_verify;
use crate::wallet::{
    self, AllowanceRequest, AssetsRequest, BalanceRequest, CollectiblesRequest,
    WalletAllowancesResponse, WalletBalancesResponse, WalletCollectiblesResponse, WalletConfig,
};
use crate::wallet_send::{self, SendRequest};
use crate::walletconnect::{self, Manager, SignPayload, SignRequest};
use eollo_protocol::position::{
    PositionAnalyzeRequest, PositionAnalyzeResponse, PositionCostBasis,
};
use eollo_protocol::swap::QuoteResponse;
use eollo_protocol::wallet::{WalletConnectResponse, WalletStatusResponse};
use eollo_protocol::yield_capability::{
    RiskProfile, YieldBuildRequest, YieldBuildResponse, YieldDiscoverRequest,
    YieldDiscoverResponse, YieldHistoryRequest, YieldHistoryResponse, YieldWithdrawRequest,
    YieldWithdrawResponse,
};

pub async fn dispatch(args: Vec<String>) -> (Vec<String>, Option<i32>) {
    let Some(first) = args.first().map(String::as_str) else {
        print_help();
        return (args, Some(0));
    };
    match first {
        "serve" | "--config" => {
            eprintln!("serve is provided by eollo-remote; run `eollo-remote serve --config PATH`");
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
        "signature" => {
            let code = run_signature(&args[1..]).await;
            (args, Some(code))
        }
        "fourmeme" => {
            let code = run_fourmeme(&args[1..]).await;
            (args, Some(code))
        }
        "yield" => {
            let code = run_yield(&args[1..]).await;
            (args, Some(code))
        }
        "position" => {
            let code = run_position(&args[1..]).await;
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
                Some("build") => run_swap_build(&args[2..]).await,
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
        "Eollo — Web3 agent capabilities\n\n\
Usage:\n  \
eollo swap ldo-eth 100                Shorthand quote\n  \
eollo ldo-eth 100                     Top-level shorthand alias\n  \
eollo capability list                  List capabilities\n  \
eollo capability describe NAME        Show capability and execution details\n  \
eollo swap quote --in TOKEN --out TOKEN --amount AMOUNT [--chain bsc|ethereum] [options]\n  \
eollo swap build --quote FILE --recipient ADDRESS [--slippage-bps N]\n  \
eollo wallet balance ADDRESS [--tokens LIST] [--chain bsc|ethereum]\n  \
eollo wallet assets ADDRESS [--include-unknown] [--chain bsc|ethereum]\n  \
eollo wallet allowance ADDRESS --spender ADDRESS [--tokens LIST] [--chain]\n  \
eollo wallet collectibles ADDRESS [--collections LIST] [--chain]\n  \
eollo wallet connect [--chain bsc|ethereum]  Link a wallet with WalletConnect\n  \
eollo wallet status [--wait N]          Show the linked wallet session\n  \
eollo wallet disconnect                 Remove the linked wallet session\n  \
eollo wallet agent-id HANDLE|ADDRESS    Resolve a BSC .ai Agent ID\n  \
eollo wallet sign --message TEXT        Sign a message with WalletConnect\n  \
eollo wallet sign --typed-data FILE [--policy FILE]  Sign EIP-712 typed data\n  \
eollo wallet send --to ADDR --token T --amount A [--policy FILE]  Send ETH/ERC20\n  \
eollo transaction inspect --to ADDRESS --data HEX [--chain bsc|ethereum]\n  \
eollo transaction simulate --to ADDRESS --data HEX [--chain bsc|ethereum]\n  \
eollo transaction send --to ADDRESS --data HEX [--value BNB] [--policy FILE]\n  \
eollo transaction verify --hash HASH [--chain bsc|ethereum] [--confirmations N] [--expect FILE]\n  \
eollo signature inspect --typed-data FILE | --authorization-list FILE | --user-operation FILE [--policy FILE]\n  \
eollo fourmeme agent-tokens [--wallet ADDRESS] [--limit N] [--lookback-blocks N]\n  \
eollo fourmeme token ADDRESS [--wallet ADDRESS] [--json]\n  \
eollo fourmeme ui [--port N] [--lookback-blocks N] [--wallet ADDRESS] [--open]\n  \
eollo yield discover --asset ETH [--amount 10] [--risk conservative|balanced|aggressive]\n  \
eollo yield build --opportunity ID --amount 10 --recipient ADDRESS [--funding-asset ETH|WETH]\n  \
eollo yield history --opportunity ID [--days 30] [--max-points 500]\n  \
eollo yield withdraw --opportunity ID --amount 10|max --recipient ADDRESS [--owner ADDRESS]\n  \
eollo position analyze --owner ADDRESS [--opportunities lido-steth,aave-v3-weth] [--include-zero]\n  \
eollo mcp [--url URL]                 Expose capabilities over MCP stdio\n  \
eollo tokens                          List built-in token symbols\n  \
eollo --help\n\n\
Quote options:\n  \
--json                    Print the swap.quote JSON response\n  \
--chain bsc|ethereum      Chain to quote on (default ethereum)\n  \
--url URL                 Remote service override (default EOLLO_REMOTE_URL or EOLLO_BSC_REMOTE_URL)\n  \
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
    chain: Chain,
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
    let mut chain = Chain::Ethereum;
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
            "--chain" => chain = Chain::parse(&value(raw, &mut index, "--chain")?)?,
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
        chain,
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

#[derive(Debug)]
struct BuildArgs {
    quote: String,
    recipient: String,
    slippage_bps: u16,
    deadline: Option<u64>,
    json: bool,
}

async fn run_swap_build(raw: &[String]) -> i32 {
    if raw.first().map(String::as_str) == Some("--help")
        || raw.first().map(String::as_str) == Some("-h")
    {
        println!(
            "Usage:\n  eollo swap build --quote FILE --recipient ADDRESS [--slippage-bps N] [--deadline UNIX_SECONDS] [--json]"
        );
        return 0;
    }
    let args = match parse_build_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as("swap.build", &error, raw.iter().any(|arg| arg == "--json"));
            return exit_code(error.code);
        }
    };
    let text = match std::fs::read_to_string(&args.quote) {
        Ok(text) => text,
        Err(error) => {
            let error = CapabilityError::invalid_argument(format!(
                "cannot read quote file {}: {error}",
                args.quote
            ));
            report_error_as("swap.build", &error, args.json);
            return exit_code(error.code);
        }
    };
    let quote: QuoteResponse = match serde_json::from_str(&text) {
        Ok(quote) => quote,
        Err(error) => match quote_from_envelope(&text) {
            Ok(quote) => quote,
            Err(envelope_error) => {
                let error = CapabilityError::invalid_argument(format!(
                    "invalid quote file {}: {error}; envelope: {envelope_error}",
                    args.quote
                ));
                report_error_as("swap.build", &error, args.json);
                return exit_code(error.code);
            }
        },
    };
    let recipient = match args.recipient.parse::<alloy_primitives::Address>() {
        Ok(address) => address,
        Err(_) => {
            let error = CapabilityError::invalid_argument("invalid --recipient address");
            report_error_as("swap.build", &error, args.json);
            return exit_code(error.code);
        }
    };
    let deadline = args.deadline.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() + 600)
            .unwrap_or(0)
    });
    match swap_build::build(
        &quote,
        BuildRequest {
            recipient,
            slippage_bps: args.slippage_bps,
            deadline,
        },
    ) {
        Ok(response) => {
            if args.json {
                print_capability_json("swap.build", &response);
            } else {
                println!("router: {}", response.router);
                println!("chain: {} ({})", response.chain, response.chain_id);
                if let Some(block) = response.quote_block {
                    println!("quote block: {block}");
                }
                println!(
                    "amount out min: {} {}",
                    response.amount_out_min.decimal, response.token_out
                );
                println!("deadline: {}", response.deadline);
                println!("data: {}", response.data);
            }
            0
        }
        Err(error) => {
            report_error_as("swap.build", &error, args.json);
            exit_code(error.code)
        }
    }
}

/// Accept a raw `QuoteResponse` or the capability envelope that `--json` prints.
fn quote_from_envelope(text: &str) -> Result<QuoteResponse, String> {
    let envelope: serde_json::Value =
        serde_json::from_str(text).map_err(|error| error.to_string())?;
    let data = envelope
        .get("data")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .ok_or_else(|| "missing data object".to_string())?;
    let meta = envelope
        .get("meta")
        .cloned()
        .ok_or_else(|| "missing meta".to_string())?;
    let chain = envelope
        .get("chain")
        .and_then(|chain| chain.get("name"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "missing chain name".to_string())?
        .to_string();
    let block = envelope
        .get("block")
        .and_then(|block| block.get("number"))
        .and_then(serde_json::Value::as_u64);
    let provenance = envelope.get("provenance");
    let pricing_mode = provenance
        .and_then(|value| value.get("pricing_mode"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("local_snapshot");
    let state_age_secs = provenance
        .and_then(|value| value.get("age_secs"))
        .and_then(serde_json::Value::as_u64);
    let mut object = data;
    // `CapabilityMeta` is flattened in `QuoteResponse`, so restore its fields
    // at the top level rather than nesting them under `meta`.
    if let Some(meta) = meta.as_object() {
        for (key, value) in meta {
            object.insert(key.clone(), value.clone());
        }
    }
    object.insert("chain".to_string(), serde_json::Value::String(chain));
    object.insert(
        "provenance".to_string(),
        serde_json::json!({
            "execution": "remote",
            "pricing_mode": pricing_mode,
            "quote_block": block,
            "state_age_secs": state_age_secs,
        }),
    );
    serde_json::from_value(serde_json::Value::Object(object)).map_err(|error| error.to_string())
}

fn parse_build_args(raw: &[String]) -> Result<BuildArgs, CapabilityError> {
    let mut quote = None;
    let mut recipient = None;
    let mut slippage_bps = 50u16;
    let mut deadline = None;
    let mut json = false;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--quote" => {
                index += 1;
                quote = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--quote needs a file"))?
                        .clone(),
                );
            }
            "--recipient" => {
                index += 1;
                recipient = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--recipient needs a value")
                        })?
                        .clone(),
                );
            }
            "--slippage-bps" => {
                index += 1;
                slippage_bps = raw
                    .get(index)
                    .ok_or_else(|| {
                        CapabilityError::invalid_argument("--slippage-bps needs a value")
                    })?
                    .parse()
                    .map_err(|_| CapabilityError::invalid_argument("invalid --slippage-bps"))?;
                if slippage_bps > 10_000 {
                    return Err(CapabilityError::invalid_argument(
                        "--slippage-bps must be between 0 and 10000",
                    ));
                }
            }
            "--deadline" => {
                index += 1;
                deadline = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--deadline needs a value")
                        })?
                        .parse()
                        .map_err(|_| CapabilityError::invalid_argument("invalid --deadline"))?,
                );
            }
            "--json" => json = true,
            value if value.starts_with('-') => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown swap build option: {value}"
                )));
            }
            value => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unexpected swap build argument: {value}"
                )));
            }
        }
        index += 1;
    }
    Ok(BuildArgs {
        quote: quote
            .ok_or_else(|| CapabilityError::invalid_argument("swap build requires --quote"))?,
        recipient: recipient
            .ok_or_else(|| CapabilityError::invalid_argument("swap build requires --recipient"))?,
        slippage_bps,
        deadline,
        json,
    })
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
    let chain_id = args.chain.chain_id();
    let token_in = match token::resolve_for_chain(&args.token_in, args.decimals_in, chain_id) {
        Ok(token) => token,
        Err(error) => {
            report_error(&error, args.json);
            return exit_code(error.code);
        }
    };
    let token_out = match token::resolve_for_chain(&args.token_out, args.decimals_out, chain_id) {
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
    let backend = match crate::routing::from_env_for_chain(args.chain.as_str(), args.url.as_deref())
    {
        Ok(backend) => backend,
        Err(error) => {
            report_error(&error, args.json);
            return exit_code(error.code);
        }
    };
    let client = RuntimeClient::with_api_key(backend.base_url.clone(), backend.api_key.clone());
    match client
        .swap_quote(&request, token_in, token_out, args.chain.as_str(), "remote")
        .await
    {
        Ok(response) => {
            if args.json {
                print_capability_json("swap.quote", &response);
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

fn wallet_capability(mode: WalletMode) -> &'static str {
    match mode {
        WalletMode::Balance => "wallet.balance",
        WalletMode::Assets => "wallet.assets",
        WalletMode::Allowance => "wallet.allowance",
        WalletMode::Collectibles => "wallet.collectibles",
    }
}

#[derive(Debug)]
struct WalletArgs {
    mode: WalletMode,
    chain: Chain,
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
    let requested_capability = match raw.first().map(String::as_str) {
        Some("assets") => "wallet.assets",
        Some("allowance") => "wallet.allowance",
        Some("collectibles") => "wallet.collectibles",
        _ => "wallet.balance",
    };
    let args = match parse_wallet_args(raw) {
        Ok(args) => args,
        Err(error) if error.message == "help requested" => {
            println!(
                "Usage:\n  eollo wallet balance ADDRESS [--tokens BNB,USDT] [--no-native] [--chain bsc|ethereum]\n  eollo wallet assets ADDRESS [--include-unknown] [--no-native] [--chain bsc|ethereum]\n  eollo wallet connect [--chain bsc|ethereum]\n  eollo wallet status\n  eollo wallet disconnect\n  eollo wallet agent-id HANDLE|ADDRESS\n  eollo wallet sign --message TEXT | --typed-data FILE [--policy FILE]\n  eollo wallet send --to ADDR --token T --amount A [--policy FILE]\n\nOptions:\n  --rpc-url URL   override the selected chain RPC\n  --block N       pin a block\n  --json          print the structured response"
            );
            return 0;
        }
        Err(error) => {
            report_error_as(
                requested_capability,
                &error,
                raw.iter().any(|arg| arg == "--json"),
            );
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
            report_error_as(wallet_capability(args.mode), &error, args.json);
            return exit_code(error.code);
        }
    };
    let mut config = WalletConfig::from_env();
    if let Some(rpc_url) = args.rpc_url {
        config.set_rpc_url(rpc_url);
    }
    enum WalletOutput {
        Balances(&'static str, WalletBalancesResponse),
        Allowances(&'static str, WalletAllowancesResponse),
        Collectibles(&'static str, WalletCollectiblesResponse),
    }
    let result = match args.mode {
        WalletMode::Balance => wallet::balance(
            &config,
            BalanceRequest {
                chain: args.chain,
                address,
                tokens: args.tokens,
                include_native: args.include_native,
                block: args.block,
            },
        )
        .await
        .map(|response| WalletOutput::Balances("wallet.balance", response)),
        WalletMode::Assets => {
            if !args.tokens.is_empty() {
                let error = CapabilityError::invalid_argument(
                    "wallet assets does not accept --tokens; use wallet balance",
                );
                report_error_as(wallet_capability(args.mode), &error, args.json);
                return exit_code(error.code);
            }
            wallet::assets(
                &config,
                AssetsRequest {
                    chain: args.chain,
                    address,
                    include_native: args.include_native,
                    include_unknown: args.include_unknown,
                    block: args.block,
                },
            )
            .await
            .map(|response| WalletOutput::Balances("wallet.assets", response))
        }
        WalletMode::Allowance => {
            let Some(spender) = args.spender.as_deref() else {
                let error =
                    CapabilityError::invalid_argument("wallet allowance requires --spender");
                report_error_as(wallet_capability(args.mode), &error, args.json);
                return exit_code(error.code);
            };
            let spender = match spender.parse::<alloy_primitives::Address>() {
                Ok(spender) => spender,
                Err(_) => {
                    let error = CapabilityError::invalid_argument("invalid --spender address");
                    report_error_as(wallet_capability(args.mode), &error, args.json);
                    return exit_code(error.code);
                }
            };
            wallet::allowance(
                &config,
                AllowanceRequest {
                    chain: args.chain,
                    owner: address,
                    spender,
                    tokens: args.tokens,
                    block: args.block,
                },
            )
            .await
            .map(|response| WalletOutput::Allowances("wallet.allowance", response))
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
                        report_error_as(wallet_capability(args.mode), &error, args.json);
                        return exit_code(error.code);
                    }
                }
            }
            wallet::collectibles(
                &config,
                CollectiblesRequest {
                    chain: args.chain,
                    owner: address,
                    collections,
                    include_empty: args.include_empty,
                    block: args.block,
                },
            )
            .await
            .map(|response| WalletOutput::Collectibles("wallet.collectibles", response))
        }
    };
    match result {
        Ok(WalletOutput::Balances(capability, response)) => {
            if args.json {
                print_capability_json(capability, &response);
            } else {
                print_wallet_human(&response);
            }
            0
        }
        Ok(WalletOutput::Allowances(capability, response)) => {
            if args.json {
                print_capability_json(capability, &response);
            } else {
                print_allowance_human(&response);
            }
            0
        }
        Ok(WalletOutput::Collectibles(capability, response)) => {
            if args.json {
                print_capability_json(capability, &response);
            } else {
                print_collectibles_human(&response);
            }
            0
        }
        Err(error) => {
            report_error_as(wallet_capability(args.mode), &error, args.json);
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
    let policy = match args.policy.as_deref() {
        Some(path) => {
            let text = match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) => {
                    let error = CapabilityError::invalid_argument(format!(
                        "cannot read policy file {path}: {error}"
                    ));
                    report_error_as("wallet.send", &error, args.json);
                    return exit_code(error.code);
                }
            };
            match serde_json::from_str::<InspectPolicy>(&text) {
                Ok(policy) => Some(policy),
                Err(error) => {
                    let error = CapabilityError::invalid_argument(format!(
                        "invalid policy file {path}: {error}"
                    ));
                    report_error_as("wallet.send", &error, args.json);
                    return exit_code(error.code);
                }
            }
        }
        None => None,
    };
    let mut config = WalletConfig::from_env();
    if let Some(rpc_url) = args.rpc_url {
        config.set_rpc_url(rpc_url);
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
            policy,
        },
    )
    .await
    {
        Ok(response) if args.json => {
            print_capability_json("wallet.send", &response);
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
    policy: Option<String>,
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
    let mut policy = None;
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
            "--policy" => {
                index += 1;
                policy = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--policy needs a file path")
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
        policy,
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
    let policy = match args.policy.as_deref() {
        Some(path) => {
            let text = match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) => {
                    let error = CapabilityError::invalid_argument(format!(
                        "cannot read policy file {path}: {error}"
                    ));
                    report_error_as("wallet.sign", &error, args.json);
                    return exit_code(error.code);
                }
            };
            match serde_json::from_str::<InspectPolicy>(&text) {
                Ok(policy) => Some(policy),
                Err(error) => {
                    let error = CapabilityError::invalid_argument(format!(
                        "invalid policy file {path}: {error}"
                    ));
                    report_error_as("wallet.sign", &error, args.json);
                    return exit_code(error.code);
                }
            }
        }
        None => None,
    };
    match walletconnect::sign(SignRequest {
        address: args.address,
        payload,
        timeout: std::time::Duration::from_secs(args.timeout_seconds),
        policy,
    })
    .await
    {
        Ok(response) if args.json => {
            print_capability_json("wallet.sign", &response);
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
    policy: Option<String>,
    json: bool,
    timeout_seconds: u64,
}

fn parse_wallet_sign_args(raw: &[String]) -> Result<WalletSignArgs, CapabilityError> {
    let mut message = None;
    let mut typed_data = None;
    let mut address = None;
    let mut policy = None;
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
            "--policy" => {
                index += 1;
                policy = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--policy needs a file path")
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
        policy,
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
            print_capability_json("wallet.agent-id", &response);
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
    let (json, chain_id) = match parse_wallet_connect_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as(
                "wallet.connect",
                &error,
                raw.iter().any(|arg| arg == "--json"),
            );
            return exit_code(error.code);
        }
    };
    let manager = Manager::new();
    let response = match manager.connect(chain_id).await {
        Ok(response) => response,
        Err(error) => {
            report_error_as("wallet.connect", &error, json);
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
            report_error_as("wallet.connect", &error, json);
            return exit_code(error.code);
        }
    };
    if json {
        print_capability_json("wallet.connect", &status);
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
            report_error_as(
                "wallet.status",
                &error,
                raw.iter().any(|arg| arg == "--json"),
            );
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
            print_capability_json("wallet.status", &status);
            0
        }
        Ok(status) => {
            print_wallet_status_human(&status);
            0
        }
        Err(error) => {
            report_error_as("wallet.status", &error, json);
            exit_code(error.code)
        }
    }
}

async fn run_wallet_disconnect(raw: &[String]) -> i32 {
    let json = match parse_wallet_session_args(raw) {
        Ok(json) => json,
        Err(error) => {
            report_error_as(
                "wallet.disconnect",
                &error,
                raw.iter().any(|arg| arg == "--json"),
            );
            return exit_code(error.code);
        }
    };
    match walletconnect::disconnect() {
        Ok(response) if json => {
            print_capability_json("wallet.disconnect", &response);
            0
        }
        Ok(_) => {
            println!("wallet disconnected");
            0
        }
        Err(error) => {
            report_error_as("wallet.disconnect", &error, json);
            exit_code(error.code)
        }
    }
}

fn parse_wallet_connect_args(raw: &[String]) -> Result<(bool, u64), CapabilityError> {
    let mut json = false;
    let mut chain_id = DEFAULT_CHAIN.chain_id();
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--json" => json = true,
            "--chain" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--chain needs a value"))?;
                chain_id = walletconnect::parse_chain_id(value)?;
            }
            value => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown wallet option: {value}"
                )));
            }
        }
        index += 1;
    }
    Ok((json, chain_id))
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
    let mut chain = DEFAULT_CHAIN;
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
            "--chain" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--chain needs a value"))?;
                chain = Chain::parse(value)?;
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
        chain,
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
    chain: Chain,
    from: Option<String>,
    to: String,
    data: String,
    value: String,
    block: Option<u64>,
    rpc_url: Option<String>,
    policy: Option<String>,
    timeout_seconds: u64,
    json: bool,
}

async fn run_transaction(raw: &[String]) -> i32 {
    if raw.first().map(String::as_str) == Some("--help")
        || raw.first().map(String::as_str) == Some("-h")
    {
        println!(
            "Usage:\n  eollo transaction inspect --to ADDRESS --data 0x... [--from ADDRESS] [--value BNB] [--chain bsc|ethereum] [--block N] [--policy FILE] [--json]\n  eollo transaction simulate --to ADDRESS --data 0x... [--from ADDRESS] [--value BNB] [--chain bsc|ethereum] [--block N] [--json]\n  eollo transaction send --to ADDRESS --data 0x... [--from ADDRESS] [--value BNB] [--policy FILE] [--timeout N] [--json]\n  eollo transaction verify --hash HASH [--chain bsc|ethereum] [--confirmations N] [--expect FILE] [--rpc-url URL] [--json]"
        );
        return 0;
    }
    let command = match raw.first().map(String::as_str) {
        Some("inspect") => "inspect",
        Some("simulate") => "simulate",
        Some("send") => "send",
        Some("verify") => "verify",
        _ => {
            let error = CapabilityError::invalid_argument(
                "expected: transaction inspect|simulate|send|verify",
            );
            report_error_as(
                "transaction.inspect",
                &error,
                raw.iter().any(|arg| arg == "--json"),
            );
            return exit_code(error.code);
        }
    };
    let capability = match command {
        "send" => "transaction.send",
        "inspect" => "transaction.inspect",
        "simulate" => "transaction.simulate",
        "verify" => "transaction.verify",
        _ => "transaction.inspect",
    };
    if command == "verify" {
        return run_transaction_verify(&raw[1..]).await;
    }
    let args = match parse_transaction_args(&raw[1..]) {
        Ok(args) => args,
        Err(error) => {
            report_error_as(capability, &error, raw.iter().any(|arg| arg == "--json"));
            return exit_code(error.code);
        }
    };
    let to = match args.to.parse::<alloy_primitives::Address>() {
        Ok(address) => address,
        Err(_) => {
            let error = CapabilityError::invalid_argument("invalid --to address");
            report_error_as(capability, &error, args.json);
            return exit_code(error.code);
        }
    };
    let from = match args.from.as_deref() {
        Some(from) => match from.parse::<alloy_primitives::Address>() {
            Ok(address) => Some(address),
            Err(_) => {
                let error = CapabilityError::invalid_argument("invalid --from address");
                report_error_as(capability, &error, args.json);
                return exit_code(error.code);
            }
        },
        None => None,
    };
    let value = match token::parse_amount(&args.value, 18) {
        Ok(value) => value,
        Err(error) => {
            report_error_as(capability, &error, args.json);
            return exit_code(error.code);
        }
    };
    let mut config = WalletConfig::from_env();
    if let Some(rpc_url) = args.rpc_url {
        config.set_rpc_url(rpc_url);
    }

    if command == "inspect" {
        let policy = match args.policy.as_deref() {
            Some(path) => {
                let text = match std::fs::read_to_string(path) {
                    Ok(text) => text,
                    Err(error) => {
                        let error = CapabilityError::invalid_argument(format!(
                            "cannot read policy file {path}: {error}"
                        ));
                        report_error_as(capability, &error, args.json);
                        return exit_code(error.code);
                    }
                };
                match serde_json::from_str::<InspectPolicy>(&text) {
                    Ok(policy) => Some(policy),
                    Err(error) => {
                        let error = CapabilityError::invalid_argument(format!(
                            "invalid policy file {path}: {error}"
                        ));
                        report_error_as(capability, &error, args.json);
                        return exit_code(error.code);
                    }
                }
            }
            None => None,
        };
        return match transaction_inspect::inspect_with_policy(
            &config,
            InspectRequest {
                chain: args.chain,
                from,
                to,
                data: args.data,
                value,
                block: args.block,
            },
            policy,
        )
        .await
        {
            Ok(response) => {
                if args.json {
                    print_capability_json("transaction.inspect", &response);
                } else {
                    print_transaction_inspect_human(&response);
                }
                0
            }
            Err(error) => {
                report_error_as(capability, &error, args.json);
                exit_code(error.code)
            }
        };
    }

    if command == "send" {
        let policy = match args.policy.as_deref() {
            Some(path) => {
                let text = match std::fs::read_to_string(path) {
                    Ok(text) => text,
                    Err(error) => {
                        let error = CapabilityError::invalid_argument(format!(
                            "cannot read policy file {path}: {error}"
                        ));
                        report_error_as(capability, &error, args.json);
                        return exit_code(error.code);
                    }
                };
                match serde_json::from_str::<InspectPolicy>(&text) {
                    Ok(policy) => Some(policy),
                    Err(error) => {
                        let error = CapabilityError::invalid_argument(format!(
                            "invalid policy file {path}: {error}"
                        ));
                        report_error_as(capability, &error, args.json);
                        return exit_code(error.code);
                    }
                }
            }
            None => None,
        };
        return match transaction_send::send(
            &config,
            TransactionSendRequest {
                from,
                to,
                data: args.data,
                value,
                block: args.block,
                timeout: std::time::Duration::from_secs(args.timeout_seconds),
                policy,
            },
        )
        .await
        {
            Ok(response) => {
                if args.json {
                    print_capability_json("transaction.send", &response);
                } else {
                    println!("From: {}", response.from);
                    println!("To: {}", response.to);
                    println!("Tx: {}", response.tx_hash);
                    println!(
                        "Risk: {} · {}",
                        response.inspection.risk, response.inspection.coverage.simulation
                    );
                }
                0
            }
            Err(error) => {
                report_error_as(capability, &error, args.json);
                exit_code(error.code)
            }
        };
    }

    match transaction::simulate(
        &config,
        SimulateRequest {
            chain: args.chain,
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
                print_capability_json("transaction.simulate", &response);
            } else {
                print_transaction_human(&response);
            }
            0
        }
        Err(error) => {
            report_error_as(capability, &error, args.json);
            exit_code(error.code)
        }
    }
}

#[derive(Debug)]
struct FourMemeArgs {
    wallet: Option<String>,
    limit: usize,
    lookback_blocks: u64,
    before_block: Option<u64>,
    json: bool,
}

const FOURMEME_USAGE: &str = "Usage:\n  eollo fourmeme agent-tokens [--wallet ADDRESS] [--limit N] [--lookback-blocks N] [--before-block N] [--json]\n  eollo fourmeme token ADDRESS [--wallet ADDRESS] [--json]\n  eollo fourmeme ui [--port N] [--lookback-blocks N] [--wallet ADDRESS] [--open]";

async fn run_fourmeme(raw: &[String]) -> i32 {
    let Some(command) = raw.first().map(String::as_str) else {
        println!("{FOURMEME_USAGE}");
        return 2;
    };
    if command == "--help" || command == "-h" {
        println!("{FOURMEME_USAGE}");
        return 0;
    }
    if raw[1..].iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{FOURMEME_USAGE}");
        return 0;
    }
    match command {
        "agent-tokens" => run_fourmeme_agent_tokens(&raw[1..]).await,
        "token" => run_fourmeme_token(&raw[1..]).await,
        "ui" => run_fourmeme_ui(&raw[1..]).await,
        other => {
            eprintln!("unknown fourmeme command: {other}");
            println!("{FOURMEME_USAGE}");
            2
        }
    }
}

async fn run_fourmeme_agent_tokens(raw: &[String]) -> i32 {
    let args = match parse_fourmeme_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as(
                "fourmeme.agent-tokens",
                &error,
                raw.iter().any(|arg| arg == "--json"),
            );
            return exit_code(error.code);
        }
    };
    let wallet = match parse_optional_address(args.wallet.as_deref(), "--wallet") {
        Ok(wallet) => wallet,
        Err(error) => {
            report_error_as("fourmeme.agent-tokens", &error, args.json);
            return exit_code(error.code);
        }
    };
    match fourmeme::agent_tokens(
        &FourMemeConfig::from_env(),
        ListRequest {
            wallet,
            limit: args.limit,
            lookback_blocks: args.lookback_blocks,
            before_block: args.before_block,
        },
    )
    .await
    {
        Ok(response) => {
            if args.json {
                print_capability_json("fourmeme.agent-tokens", &response);
            } else {
                println!(
                    "Four.meme · Binance Agent ID Exclusive · {} events · blocks {}-{}",
                    response.scanned, response.from_block, response.to_block
                );
                for token in &response.tokens {
                    println!(
                        "{}  {}  {}  {}{}",
                        token.address,
                        token.symbol,
                        token.phase,
                        token.progress.as_deref().unwrap_or("-"),
                        token
                            .wallet_eligible
                            .map(|value| format!("  eligible={value}"))
                            .unwrap_or_default()
                    );
                }
                for warning in &response.warnings {
                    eprintln!("warning: {warning}");
                }
            }
            0
        }
        Err(error) => {
            report_error_as("fourmeme.agent-tokens", &error, args.json);
            exit_code(error.code)
        }
    }
}

async fn run_fourmeme_token(raw: &[String]) -> i32 {
    let args = match parse_fourmeme_token_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as(
                "fourmeme.token",
                &error,
                raw.iter().any(|arg| arg == "--json"),
            );
            return exit_code(error.code);
        }
    };
    let address = match args.address.parse::<alloy_primitives::Address>() {
        Ok(address) => address,
        Err(_) => {
            let error = CapabilityError::invalid_argument("invalid token address");
            report_error_as("fourmeme.token", &error, args.json);
            return exit_code(error.code);
        }
    };
    let wallet = match parse_optional_address(args.wallet.as_deref(), "--wallet") {
        Ok(wallet) => wallet,
        Err(error) => {
            report_error_as("fourmeme.token", &error, args.json);
            return exit_code(error.code);
        }
    };
    match fourmeme::token(&FourMemeConfig::from_env(), address, wallet).await {
        Ok(detail) => {
            if args.json {
                print_capability_json("fourmeme.token", &detail);
            } else {
                println!("{} ({}) · {}", detail.name, detail.symbol, detail.phase);
                println!("address: {}", detail.address);
                if let Some(name) = detail.template_name.as_deref() {
                    println!("template: {name}");
                }
                if let Some(raised) = detail.raised.as_deref() {
                    println!("raised: {raised} BNB");
                }
                if let Some(cap) = detail.market_cap.as_deref() {
                    println!("market cap: {cap}");
                }
                if let Some(price) = detail.price.as_deref() {
                    println!("price: {price}");
                }
                if let Some(creator) = detail.creator {
                    println!("creator: {creator}");
                }
                if let Some(eligible) = detail.wallet_eligible {
                    println!(
                        "wallet eligible: {eligible}{}",
                        detail
                            .eligibility_reason
                            .as_deref()
                            .map(|reason| format!(" ({reason})"))
                            .unwrap_or_default()
                    );
                }
                println!("four.meme: https://four.meme/token/{}", detail.address);
                println!("bscscan: https://bscscan.com/address/{}", detail.address);
                for warning in &detail.warnings {
                    eprintln!("warning: {warning}");
                }
            }
            0
        }
        Err(error) => {
            report_error_as("fourmeme.token", &error, args.json);
            exit_code(error.code)
        }
    }
}

async fn run_fourmeme_ui(raw: &[String]) -> i32 {
    let args = match parse_fourmeme_ui_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as("fourmeme.ui", &error, false);
            return exit_code(error.code);
        }
    };
    let wallet = match parse_optional_address(args.wallet.as_deref(), "--wallet") {
        Ok(wallet) => wallet,
        Err(error) => {
            report_error_as("fourmeme.ui", &error, false);
            return exit_code(error.code);
        }
    };
    match fourmeme_ui::serve(fourmeme_ui::UiOptions {
        port: args.port,
        lookback_blocks: args.lookback_blocks,
        wallet,
        open: args.open,
    })
    .await
    {
        Ok(()) => 0,
        Err(error) => {
            report_error_as("fourmeme.ui", &error, false);
            exit_code(error.code)
        }
    }
}

fn parse_optional_address(
    value: Option<&str>,
    flag: &str,
) -> Result<Option<alloy_primitives::Address>, CapabilityError> {
    value
        .map(|value| {
            value
                .parse::<alloy_primitives::Address>()
                .map_err(|_| CapabilityError::invalid_argument(format!("invalid {flag} address")))
        })
        .transpose()
}

fn parse_fourmeme_args(raw: &[String]) -> Result<FourMemeArgs, CapabilityError> {
    let mut wallet = None;
    let mut limit = 20usize;
    let mut lookback_blocks = 100_000u64;
    let mut before_block = None;
    let mut json = false;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--wallet" => {
                index += 1;
                wallet = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--wallet needs a value"))?
                        .clone(),
                );
            }
            "--limit" => {
                index += 1;
                limit = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--limit needs a value"))?
                    .parse()
                    .map_err(|_| CapabilityError::invalid_argument("invalid --limit"))?;
                if limit == 0 || limit > 50 {
                    return Err(CapabilityError::invalid_argument(
                        "--limit must be between 1 and 50",
                    ));
                }
            }
            "--lookback-blocks" => {
                index += 1;
                lookback_blocks = raw
                    .get(index)
                    .ok_or_else(|| {
                        CapabilityError::invalid_argument("--lookback-blocks needs a value")
                    })?
                    .parse()
                    .map_err(|_| CapabilityError::invalid_argument("invalid --lookback-blocks"))?;
                if lookback_blocks == 0 || lookback_blocks > 5_000_000 {
                    return Err(CapabilityError::invalid_argument(
                        "--lookback-blocks must be between 1 and 5000000",
                    ));
                }
            }
            "--before-block" => {
                index += 1;
                before_block = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--before-block needs a value")
                        })?
                        .parse()
                        .map_err(|_| CapabilityError::invalid_argument("invalid --before-block"))?,
                );
            }
            "--json" => json = true,
            value => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown fourmeme option: {value}"
                )));
            }
        }
        index += 1;
    }
    Ok(FourMemeArgs {
        wallet,
        limit,
        lookback_blocks,
        before_block,
        json,
    })
}

struct FourMemeTokenArgs {
    address: String,
    wallet: Option<String>,
    json: bool,
}

fn parse_fourmeme_token_args(raw: &[String]) -> Result<FourMemeTokenArgs, CapabilityError> {
    let mut address = None;
    let mut wallet = None;
    let mut json = false;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--wallet" => {
                index += 1;
                wallet = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--wallet needs a value"))?
                        .clone(),
                );
            }
            "--json" => json = true,
            value if !value.starts_with('-') && address.is_none() => {
                address = Some(value.to_string());
            }
            value => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown fourmeme token option: {value}"
                )));
            }
        }
        index += 1;
    }
    Ok(FourMemeTokenArgs {
        address: address
            .ok_or_else(|| CapabilityError::invalid_argument("token address is required"))?,
        wallet,
        json,
    })
}

struct FourMemeUiArgs {
    port: u16,
    lookback_blocks: u64,
    wallet: Option<String>,
    open: bool,
}

fn parse_fourmeme_ui_args(raw: &[String]) -> Result<FourMemeUiArgs, CapabilityError> {
    let mut port = 8787u16;
    let mut lookback_blocks = 100_000u64;
    let mut wallet = None;
    let mut open = false;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--port" => {
                index += 1;
                port = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--port needs a value"))?
                    .parse()
                    .map_err(|_| CapabilityError::invalid_argument("invalid --port"))?;
                if port == 0 {
                    return Err(CapabilityError::invalid_argument(
                        "--port must be between 1 and 65535",
                    ));
                }
            }
            "--lookback-blocks" => {
                index += 1;
                lookback_blocks = raw
                    .get(index)
                    .ok_or_else(|| {
                        CapabilityError::invalid_argument("--lookback-blocks needs a value")
                    })?
                    .parse()
                    .map_err(|_| CapabilityError::invalid_argument("invalid --lookback-blocks"))?;
                if lookback_blocks == 0 || lookback_blocks > 5_000_000 {
                    return Err(CapabilityError::invalid_argument(
                        "--lookback-blocks must be between 1 and 5000000",
                    ));
                }
            }
            "--wallet" => {
                index += 1;
                wallet = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--wallet needs a value"))?
                        .clone(),
                );
            }
            "--open" => open = true,
            value => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown fourmeme ui option: {value}"
                )));
            }
        }
        index += 1;
    }
    Ok(FourMemeUiArgs {
        port,
        lookback_blocks,
        wallet,
        open,
    })
}

struct SignatureArgs {
    chain: Chain,
    from: Option<String>,
    typed_data: Option<String>,
    authorization_list: Option<String>,
    user_operation: Option<String>,
    policy: Option<String>,
    json: bool,
}

async fn run_signature(raw: &[String]) -> i32 {
    if raw.first().map(String::as_str) == Some("--help")
        || raw.first().map(String::as_str) == Some("-h")
        || raw.is_empty()
    {
        println!(
            "Usage:\n  eollo signature inspect --typed-data FILE [--chain bsc|ethereum] [--policy FILE] [--json]\n  eollo signature inspect --authorization-list FILE [--chain bsc|ethereum] [--policy FILE] [--json]\n  eollo signature inspect --user-operation FILE [--chain bsc|ethereum] [--policy FILE] [--json]"
        );
        return 0;
    }
    if raw.first().map(String::as_str) != Some("inspect") {
        let error = CapabilityError::invalid_argument("expected: signature inspect");
        report_error_as(
            "signature.inspect",
            &error,
            raw.iter().any(|arg| arg == "--json"),
        );
        return exit_code(error.code);
    }
    let args = match parse_signature_args(&raw[1..]) {
        Ok(args) => args,
        Err(error) => {
            report_error_as("signature.inspect", &error, args_json(raw));
            return exit_code(error.code);
        }
    };
    let selected = [
        args.typed_data.is_some(),
        args.authorization_list.is_some(),
        args.user_operation.is_some(),
    ]
    .into_iter()
    .filter(|selected| *selected)
    .count();
    if selected != 1 {
        let error = CapabilityError::invalid_argument(
            "signature inspect needs exactly one of --typed-data, --authorization-list or --user-operation",
        );
        report_error_as("signature.inspect", &error, args.json);
        return exit_code(error.code);
    }
    let from = match args.from.as_deref() {
        Some(value) => match value.parse::<alloy_primitives::Address>() {
            Ok(address) => Some(address),
            Err(_) => {
                let error = CapabilityError::invalid_argument("invalid --from address");
                report_error_as("signature.inspect", &error, args.json);
                return exit_code(error.code);
            }
        },
        None => None,
    };
    let input = |path: Option<&str>| -> Result<Option<serde_json::Value>, CapabilityError> {
        let Some(path) = path else {
            return Ok(None);
        };
        let text = std::fs::read_to_string(path).map_err(|error| {
            CapabilityError::invalid_argument(format!("cannot read {path}: {error}"))
        })?;
        serde_json::from_str(&text).map(Some).map_err(|error| {
            CapabilityError::invalid_argument(format!("invalid JSON in {path}: {error}"))
        })
    };
    let typed_data = match input(args.typed_data.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            report_error_as("signature.inspect", &error, args.json);
            return exit_code(error.code);
        }
    };
    let authorization_list = match input(args.authorization_list.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            report_error_as("signature.inspect", &error, args.json);
            return exit_code(error.code);
        }
    };
    let user_operation = match input(args.user_operation.as_deref()) {
        Ok(value) => value,
        Err(error) => {
            report_error_as("signature.inspect", &error, args.json);
            return exit_code(error.code);
        }
    };
    let policy = match args.policy.as_deref() {
        Some(path) => {
            let text = match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) => {
                    let error = CapabilityError::invalid_argument(format!(
                        "cannot read policy file {path}: {error}"
                    ));
                    report_error_as("signature.inspect", &error, args.json);
                    return exit_code(error.code);
                }
            };
            match serde_json::from_str::<InspectPolicy>(&text) {
                Ok(policy) => Some(policy),
                Err(error) => {
                    let error = CapabilityError::invalid_argument(format!(
                        "invalid policy file {path}: {error}"
                    ));
                    report_error_as("signature.inspect", &error, args.json);
                    return exit_code(error.code);
                }
            }
        }
        None => None,
    };
    match signature_inspect::inspect(SignatureInspectRequest {
        chain: args.chain,
        from,
        typed_data,
        authorization_list,
        user_operation,
        policy,
    }) {
        Ok(response) => {
            if args.json {
                print_capability_json("signature.inspect", &response);
            } else {
                print_signature_human(&response);
            }
            0
        }
        Err(error) => {
            report_error_as("signature.inspect", &error, args.json);
            exit_code(error.code)
        }
    }
}

fn parse_signature_args(raw: &[String]) -> Result<SignatureArgs, CapabilityError> {
    let mut chain = DEFAULT_CHAIN;
    let mut from = None;
    let mut typed_data = None;
    let mut authorization_list = None;
    let mut user_operation = None;
    let mut policy = None;
    let mut json = false;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--chain" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--chain needs a value"))?;
                chain = Chain::parse(value)?;
            }
            "--from" => {
                index += 1;
                from = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--from needs a value"))?
                        .clone(),
                );
            }
            "--typed-data" => {
                index += 1;
                typed_data = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--typed-data needs a file path")
                        })?
                        .clone(),
                );
            }
            "--authorization-list" => {
                index += 1;
                authorization_list = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument(
                                "--authorization-list needs a file path",
                            )
                        })?
                        .clone(),
                );
            }
            "--user-operation" => {
                index += 1;
                user_operation = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--user-operation needs a file path")
                        })?
                        .clone(),
                );
            }
            "--policy" => {
                index += 1;
                policy = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--policy needs a file path")
                        })?
                        .clone(),
                );
            }
            "--json" => json = true,
            value if value.starts_with('-') => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown signature option: {value}"
                )));
            }
            value => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unexpected signature argument: {value}"
                )));
            }
        }
        index += 1;
    }
    Ok(SignatureArgs {
        chain,
        from,
        typed_data,
        authorization_list,
        user_operation,
        policy,
        json,
    })
}

fn args_json(raw: &[String]) -> bool {
    raw.iter().any(|arg| arg == "--json")
}

fn print_signature_human(response: &SignatureInspectResponse) {
    println!(
        "{} · {} · risk {}",
        response.kind, response.chain, response.risk
    );
    for finding in &response.findings {
        println!(
            "{} [{}] {}",
            finding.severity.to_ascii_uppercase(),
            finding.id,
            finding.title
        );
    }
    for unknown in &response.unknowns {
        eprintln!("unknown: {unknown}");
    }
    for warning in &response.warnings {
        eprintln!("warning: {warning}");
    }
}

fn parse_transaction_args(raw: &[String]) -> Result<TransactionArgs, CapabilityError> {
    let mut chain = DEFAULT_CHAIN;
    let mut from = None;
    let mut to = None;
    let mut data = None;
    let mut value = "0".to_string();
    let mut block = None;
    let mut rpc_url = None;
    let mut policy = None;
    let mut timeout_seconds = 90;
    let mut json = false;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--chain" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--chain needs a value"))?;
                chain = Chain::parse(value)?;
            }
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
            "--policy" => {
                index += 1;
                policy = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--policy needs a file path")
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
        chain,
        from,
        to: to.ok_or_else(|| CapabilityError::invalid_argument("missing --to"))?,
        data: data.ok_or_else(|| CapabilityError::invalid_argument("missing --data"))?,
        value,
        block,
        rpc_url,
        policy,
        timeout_seconds,
        json,
    })
}

struct VerifyArgs {
    chain: Chain,
    hash: String,
    confirmations: u64,
    expect: Option<String>,
    rpc_url: Option<String>,
    json: bool,
}

fn parse_verify_args(raw: &[String]) -> Result<VerifyArgs, CapabilityError> {
    let mut chain = DEFAULT_CHAIN;
    let mut hash = None;
    let mut confirmations = 1u64;
    let mut expect = None;
    let mut rpc_url = None;
    let mut json = false;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--chain" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| CapabilityError::invalid_argument("--chain needs a value"))?;
                chain = Chain::parse(value)?;
            }
            "--hash" => {
                index += 1;
                hash = Some(
                    raw.get(index)
                        .ok_or_else(|| CapabilityError::invalid_argument("--hash needs a value"))?
                        .clone(),
                );
            }
            "--confirmations" => {
                index += 1;
                let value = raw.get(index).ok_or_else(|| {
                    CapabilityError::invalid_argument("--confirmations needs a value")
                })?;
                confirmations = value.parse().map_err(|_| {
                    CapabilityError::invalid_argument(format!("invalid --confirmations: {value}"))
                })?;
            }
            "--expect" => {
                index += 1;
                expect = Some(
                    raw.get(index)
                        .ok_or_else(|| {
                            CapabilityError::invalid_argument("--expect needs a file path")
                        })?
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
            "--json" => json = true,
            value if value.starts_with('-') => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown transaction verify option: {value}"
                )));
            }
            value => {
                if hash.is_some() {
                    return Err(CapabilityError::invalid_argument(
                        "transaction verify accepts one transaction hash",
                    ));
                }
                hash = Some(value.to_string());
            }
        }
        index += 1;
    }
    Ok(VerifyArgs {
        chain,
        hash: hash.ok_or_else(|| CapabilityError::invalid_argument("missing --hash"))?,
        confirmations,
        expect,
        rpc_url,
        json,
    })
}

async fn run_transaction_verify(raw: &[String]) -> i32 {
    let args = match parse_verify_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as(
                "transaction.verify",
                &error,
                raw.iter().any(|arg| arg == "--json"),
            );
            return exit_code(error.code);
        }
    };
    let expected = match args.expect.as_deref() {
        Some(path) => {
            let text = match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) => {
                    let error = CapabilityError::invalid_argument(format!(
                        "cannot read expected report {path}: {error}"
                    ));
                    report_error_as("transaction.verify", &error, args.json);
                    return exit_code(error.code);
                }
            };
            let value: serde_json::Value = match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(error) => {
                    let error = CapabilityError::invalid_argument(format!(
                        "invalid expected report {path}: {error}"
                    ));
                    report_error_as("transaction.verify", &error, args.json);
                    return exit_code(error.code);
                }
            };
            match transaction_verify::expected_from_value(&value) {
                Ok(effects) => Some(effects),
                Err(error) => {
                    report_error_as("transaction.verify", &error, args.json);
                    return exit_code(error.code);
                }
            }
        }
        None => None,
    };
    let mut config = WalletConfig::from_env();
    if let Some(rpc_url) = args.rpc_url {
        config.set_rpc_url(rpc_url);
    }
    match transaction_verify::verify(
        &config,
        transaction_verify::VerifyRequest {
            chain: args.chain,
            tx_hash: args.hash,
            confirmations: args.confirmations,
            expected,
        },
    )
    .await
    {
        Ok(response) => {
            if args.json {
                print_capability_json("transaction.verify", &response);
            } else {
                print_transaction_verify_human(&response);
            }
            0
        }
        Err(error) => {
            report_error_as("transaction.verify", &error, args.json);
            exit_code(error.code)
        }
    }
}

fn print_transaction_verify_human(
    response: &eollo_protocol::transaction::TransactionVerifyResponse,
) {
    println!("Tx:       {}", response.tx_hash);
    println!(
        "Status:   {} ({} confirmations)",
        response.status, response.confirmations
    );
    println!(
        "Block:    {} ({})",
        response.block_number, response.block_hash
    );
    if let Some(from) = response.from {
        println!("From:     {from}");
    }
    if let Some(to) = response.to {
        println!("To:       {to}");
    }
    if let Some(value) = &response.value {
        println!("Value:    {}", value.decimal);
    }
    if let Some(gas) = &response.gas_used {
        println!("Gas used: {gas}");
    }
    if let Some(fee) = &response.fee {
        println!("Fee:      {}", fee.decimal);
    }
    if let Some(logs) = response.logs_count {
        println!("Effects:  {logs} logs");
    }
    println!("Expected: {}", response.expectation.status);
    for mismatch in &response.expectation.mismatches {
        println!(
            "  {}: expected {}, actual {}",
            mismatch.kind, mismatch.expected, mismatch.actual
        );
    }
    for warning in &response.warnings {
        println!("Warning:  {warning}");
    }
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

fn print_transaction_inspect_human(response: &TransactionInspectResponse) {
    let label = response
        .intent
        .signature
        .as_deref()
        .or(response.intent.name.as_deref())
        .unwrap_or("unknown");
    println!(
        "{} · block {} · risk {}",
        label, response.block_number, response.risk
    );
    println!(
        "{} -> {} · value {}",
        response
            .from
            .map(|address| address.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        response.to,
        response.value.decimal
    );
    for arg in &response.intent.args {
        println!("{:<12} {}", arg.name, arg.value);
    }
    println!(
        "coverage: decode={} metadata={} simulation={} policy={}",
        response.coverage.decode,
        response.coverage.metadata,
        response.coverage.simulation,
        response.coverage.policy
    );
    for finding in &response.findings {
        println!(
            "{} [{}] {}",
            finding.severity.to_ascii_uppercase(),
            finding.id,
            finding.title
        );
    }
    for unknown in &response.unknowns {
        eprintln!("unknown: {unknown}");
    }
    for warning in &response.warnings {
        eprintln!("warning: {warning}");
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
        let response = if uses_v2(capability) {
            wrap_error(capability, error.clone())
        } else {
            serde_json::to_value(CapabilityErrorResponse::new(
                capability,
                request_id(),
                error.clone(),
            ))
            .unwrap()
        };
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
        CapabilityErrorCode::PolicyBlocked => 9,
        CapabilityErrorCode::Internal => 1,
    }
}

#[derive(Debug)]
struct YieldArgs {
    chain: Chain,
    asset: String,
    amount: Option<String>,
    risk_profile: RiskProfile,
    min_tvl_usd: Option<u64>,
    max_lockup_days: Option<u32>,
    include_reward_apy: bool,
    protocols: Vec<String>,
    limit: usize,
    url: Option<String>,
    json: bool,
}

const YIELD_USAGE: &str = "Usage:\n  eollo yield discover --asset ETH|BNB [--chain bsc|ethereum] [--amount 10]\n    [--risk conservative|balanced|aggressive] [--min-tvl-usd N] [--max-lockup-days N]\n    [--include-reward-apy] [--protocols lido,aave-v3] [--limit N] [--url URL] [--json]\n  eollo yield build --opportunity ID --amount 10 --recipient ADDRESS [--funding-asset ETH|WETH] [--json]\n  eollo yield history --opportunity ID [--days 30] [--max-points 500] [--url URL] [--json]\n  eollo yield withdraw --opportunity ID --amount 10|max --recipient ADDRESS [--owner ADDRESS] [--json]";

async fn run_yield(raw: &[String]) -> i32 {
    let Some(command) = raw.first().map(String::as_str) else {
        println!("{YIELD_USAGE}");
        return 2;
    };
    if command == "--help" || command == "-h" {
        println!("{YIELD_USAGE}");
        return 0;
    }
    if raw[1..].iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{YIELD_USAGE}");
        return 0;
    }
    match command {
        "discover" => run_yield_discover(&raw[1..]).await,
        "build" => run_yield_build(&raw[1..]),
        "history" => run_yield_history(&raw[1..]).await,
        "withdraw" => run_yield_withdraw(&raw[1..]),
        other => {
            eprintln!("unknown yield command: {other}");
            println!("{YIELD_USAGE}");
            2
        }
    }
}

async fn run_yield_discover(raw: &[String]) -> i32 {
    let json = raw.iter().any(|arg| arg == "--json");
    let args = match parse_yield_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as("yield.discover", &error, json);
            return exit_code(error.code);
        }
    };
    let controller =
        match crate::routing::from_env_for_chain(args.chain.as_str(), args.url.as_deref()) {
            Ok(backend) => RuntimeClient::with_api_key(backend.base_url, backend.api_key),
            Err(error) => {
                report_error_as("yield.discover", &error, args.json);
                return exit_code(error.code);
            }
        };
    let request = YieldDiscoverRequest {
        chain: args.chain.as_str().to_string(),
        asset: args.asset,
        amount: args.amount,
        risk_profile: args.risk_profile,
        min_tvl_usd: args.min_tvl_usd,
        max_lockup_days: args.max_lockup_days,
        include_reward_apy: Some(args.include_reward_apy),
        protocols: args.protocols,
        limit: args.limit,
    };
    match controller.yield_discover(&request).await {
        Ok(response) => {
            if args.json {
                print_capability_json("yield.discover", &response);
            } else {
                print_yield_human(&response);
            }
            0
        }
        Err(error) => {
            report_error_as("yield.discover", &error, args.json);
            exit_code(error.code)
        }
    }
}

fn run_yield_build(raw: &[String]) -> i32 {
    let json = raw.iter().any(|arg| arg == "--json");
    let (opportunity_id, amount, recipient, funding_asset) = match parse_yield_build_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as("yield.build", &error, json);
            return exit_code(error.code);
        }
    };
    let request = YieldBuildRequest {
        opportunity_id,
        amount,
        recipient,
        funding_asset,
    };
    match crate::yield_build::build(&request) {
        Ok(response) => {
            if json {
                print_capability_json("yield.build", &response);
            } else {
                print_yield_build_human(&response);
            }
            0
        }
        Err(error) => {
            report_error_as("yield.build", &error, json);
            exit_code(error.code)
        }
    }
}

fn parse_yield_build_args(
    raw: &[String],
) -> Result<(String, String, alloy_primitives::Address, Option<String>), CapabilityError> {
    let mut opportunity_id = None;
    let mut amount = None;
    let mut recipient = None;
    let mut funding_asset = None;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--opportunity" | "--opportunity-id" => {
                opportunity_id = Some(value(raw, &mut index, "--opportunity")?)
            }
            "--amount" => amount = Some(value(raw, &mut index, "--amount")?),
            "--recipient" => {
                let text = value(raw, &mut index, "--recipient")?;
                recipient = Some(text.parse::<alloy_primitives::Address>().map_err(|_| {
                    CapabilityError::invalid_argument("invalid --recipient address")
                })?);
            }
            "--funding-asset" => funding_asset = Some(value(raw, &mut index, "--funding-asset")?),
            "--json" => {}
            other => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown yield build option: {other}"
                )));
            }
        }
        index += 1;
    }
    let Some(opportunity_id) = opportunity_id else {
        return Err(CapabilityError::invalid_argument(
            "--opportunity is required",
        ));
    };
    let Some(amount) = amount else {
        return Err(CapabilityError::invalid_argument("--amount is required"));
    };
    let Some(recipient) = recipient else {
        return Err(CapabilityError::invalid_argument("--recipient is required"));
    };
    Ok((opportunity_id, amount, recipient, funding_asset))
}

async fn run_yield_history(raw: &[String]) -> i32 {
    let json = raw.iter().any(|arg| arg == "--json");
    let (opportunity_id, days, max_points, url, chain) = match parse_yield_history_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as("yield.history", &error, json);
            return exit_code(error.code);
        }
    };
    let controller = match crate::routing::from_env_for_chain(chain.as_str(), url.as_deref()) {
        Ok(backend) => RuntimeClient::with_api_key(backend.base_url, backend.api_key),
        Err(error) => {
            report_error_as("yield.history", &error, json);
            return exit_code(error.code);
        }
    };
    match controller
        .yield_history(&YieldHistoryRequest {
            opportunity_id,
            days,
            max_points,
        })
        .await
    {
        Ok(response) => {
            if json {
                print_capability_json("yield.history", &response);
            } else {
                print_yield_history_human(&response);
            }
            0
        }
        Err(error) => {
            report_error_as("yield.history", &error, json);
            exit_code(error.code)
        }
    }
}

fn parse_yield_history_args(
    raw: &[String],
) -> Result<(String, u32, usize, Option<String>, Chain), CapabilityError> {
    let mut opportunity_id = None;
    let mut days = 30u32;
    let mut max_points = 500usize;
    let mut url = None;
    let mut chain = Chain::Ethereum;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--opportunity" | "--opportunity-id" => {
                opportunity_id = Some(value(raw, &mut index, "--opportunity")?)
            }
            "--days" => {
                days = value(raw, &mut index, "--days")?
                    .parse()
                    .map_err(|_| CapabilityError::invalid_argument("invalid --days"))?;
            }
            "--max-points" => {
                max_points = value(raw, &mut index, "--max-points")?
                    .parse()
                    .map_err(|_| CapabilityError::invalid_argument("invalid --max-points"))?;
            }
            "--chain" => chain = Chain::parse(&value(raw, &mut index, "--chain")?)?,
            "--url" => url = Some(value(raw, &mut index, "--url")?),
            "--json" => {}
            other => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown yield history option: {other}"
                )));
            }
        }
        index += 1;
    }
    let Some(opportunity_id) = opportunity_id else {
        return Err(CapabilityError::invalid_argument(
            "--opportunity is required",
        ));
    };
    Ok((opportunity_id, days, max_points, url, chain))
}

fn print_yield_build_human(response: &YieldBuildResponse) {
    println!(
        "yield.build · {} · {} {}",
        response.opportunity_id, response.amount.decimal, response.deposit_asset.symbol
    );
    println!("recipient: {:#x}", response.recipient);
    for (index, step) in response.steps.iter().enumerate() {
        println!(
            "{}. {} -> {:#x} · value {} · {}",
            index + 1,
            step.kind,
            step.to,
            step.value,
            step.description
        );
        println!("   data: {}", step.data);
    }
    for approval in &response.approvals {
        println!(
            "approval: {} {} -> {:#x}",
            approval.amount.decimal, approval.token.symbol, approval.spender
        );
    }
    for warning in &response.warnings {
        eprintln!("warning: {warning}");
    }
}

fn print_yield_history_human(response: &YieldHistoryResponse) {
    println!(
        "yield.history · {} · last {} days",
        response.opportunity_id, response.days
    );
    let summary = &response.summary;
    println!(
        "latest {:.2}% · mean {:.2}% · min {:.2}% · max {:.2}% · {} points",
        summary.apy_latest_percent.unwrap_or_default(),
        summary.apy_mean_percent.unwrap_or_default(),
        summary.apy_min_percent.unwrap_or_default(),
        summary.apy_max_percent.unwrap_or_default(),
        summary.points
    );
    for point in response.series.iter().rev().take(5).rev() {
        println!(
            "{}  {:.2}%",
            point.timestamp,
            point
                .apy_percent
                .or(point.base_apy_percent)
                .unwrap_or_default()
        );
    }
    for warning in &response.warnings {
        eprintln!("warning: {warning}");
    }
}

fn run_yield_withdraw(raw: &[String]) -> i32 {
    let json = raw.iter().any(|arg| arg == "--json");
    let args = match parse_yield_withdraw_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as("yield.withdraw", &error, json);
            return exit_code(error.code);
        }
    };
    match crate::yield_withdraw::withdraw(&args) {
        Ok(response) => {
            if json {
                print_capability_json("yield.withdraw", &response);
            } else {
                print_yield_withdraw_human(&response);
            }
            0
        }
        Err(error) => {
            report_error_as("yield.withdraw", &error, json);
            exit_code(error.code)
        }
    }
}

fn parse_yield_withdraw_args(raw: &[String]) -> Result<YieldWithdrawRequest, CapabilityError> {
    let mut opportunity_id = None;
    let mut amount = None;
    let mut recipient = None;
    let mut owner = None;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--opportunity" | "--opportunity-id" => {
                opportunity_id = Some(value(raw, &mut index, "--opportunity")?)
            }
            "--amount" => amount = Some(value(raw, &mut index, "--amount")?),
            "--recipient" => {
                let text = value(raw, &mut index, "--recipient")?;
                recipient = Some(text.parse::<alloy_primitives::Address>().map_err(|_| {
                    CapabilityError::invalid_argument("invalid --recipient address")
                })?);
            }
            "--owner" => {
                let text = value(raw, &mut index, "--owner")?;
                owner =
                    Some(text.parse::<alloy_primitives::Address>().map_err(|_| {
                        CapabilityError::invalid_argument("invalid --owner address")
                    })?);
            }
            "--json" => {}
            other => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown yield withdraw option: {other}"
                )));
            }
        }
        index += 1;
    }
    Ok(YieldWithdrawRequest {
        opportunity_id: opportunity_id
            .ok_or_else(|| CapabilityError::invalid_argument("--opportunity is required"))?,
        amount: amount.ok_or_else(|| CapabilityError::invalid_argument("--amount is required"))?,
        recipient: recipient
            .ok_or_else(|| CapabilityError::invalid_argument("--recipient is required"))?,
        owner,
    })
}

fn print_yield_withdraw_human(response: &YieldWithdrawResponse) {
    println!(
        "yield.withdraw · {} · {} {}",
        response.opportunity_id, response.amount_kind, response.receipt_asset.symbol
    );
    println!(
        "recipient: {:#x} · owner: {:#x}",
        response.recipient, response.owner
    );
    for (index, step) in response.steps.iter().enumerate() {
        println!(
            "{}. {} -> {:#x} · {}",
            index + 1,
            step.kind,
            step.to,
            step.description
        );
        println!("   data: {}", step.data);
    }
    for warning in &response.warnings {
        eprintln!("warning: {warning}");
    }
}

fn parse_yield_args(raw: &[String]) -> Result<YieldArgs, CapabilityError> {
    let mut chain = Chain::Ethereum;
    let mut asset = None;
    let mut amount = None;
    let mut risk_profile = RiskProfile::Conservative;
    let mut min_tvl_usd = None;
    let mut max_lockup_days = None;
    let mut include_reward_apy = false;
    let mut protocols = Vec::new();
    let mut limit = 10usize;
    let mut url = None;
    let mut json = false;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--chain" => chain = Chain::parse(&value(raw, &mut index, "--chain")?)?,
            "--asset" => asset = Some(value(raw, &mut index, "--asset")?),
            "--amount" => amount = Some(value(raw, &mut index, "--amount")?),
            "--risk" | "--risk-profile" => {
                let text = value(raw, &mut index, "--risk")?;
                risk_profile = parse_risk_profile(&text)?;
            }
            "--min-tvl-usd" => {
                min_tvl_usd = Some(
                    value(raw, &mut index, "--min-tvl-usd")?
                        .parse()
                        .map_err(|_| CapabilityError::invalid_argument("invalid --min-tvl-usd"))?,
                );
            }
            "--max-lockup-days" => {
                max_lockup_days = Some(
                    value(raw, &mut index, "--max-lockup-days")?
                        .parse()
                        .map_err(|_| {
                            CapabilityError::invalid_argument("invalid --max-lockup-days")
                        })?,
                );
            }
            "--include-reward-apy" => include_reward_apy = true,
            "--protocols" => {
                protocols = value(raw, &mut index, "--protocols")?
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            "--limit" => {
                limit = value(raw, &mut index, "--limit")?
                    .parse()
                    .map_err(|_| CapabilityError::invalid_argument("invalid --limit"))?;
                if !(1..=20).contains(&limit) {
                    return Err(CapabilityError::invalid_argument(
                        "limit must be between 1 and 20",
                    ));
                }
            }
            "--url" => url = Some(value(raw, &mut index, "--url")?),
            "--json" => json = true,
            other => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown yield option: {other}"
                )));
            }
        }
        index += 1;
    }
    let asset = asset.unwrap_or_else(|| chain.native_symbol().to_string());
    Ok(YieldArgs {
        chain,
        asset,
        amount,
        risk_profile,
        min_tvl_usd,
        max_lockup_days,
        include_reward_apy,
        protocols,
        limit,
        url,
        json,
    })
}

fn parse_risk_profile(value: &str) -> Result<RiskProfile, CapabilityError> {
    match value {
        "conservative" => Ok(RiskProfile::Conservative),
        "balanced" => Ok(RiskProfile::Balanced),
        "aggressive" => Ok(RiskProfile::Aggressive),
        other => Err(CapabilityError::invalid_argument(format!(
            "invalid risk profile: {other}; use conservative, balanced or aggressive"
        ))),
    }
}

fn print_yield_human(response: &YieldDiscoverResponse) {
    println!(
        "Yield discovery · {} · {:?}",
        response.chain, response.risk_profile
    );
    println!(
        "as of {} · age {}s{}{}",
        response.as_of,
        response.age_secs,
        if response.stale { " · STALE" } else { "" },
        response
            .amount
            .as_ref()
            .map(|amount| format!(" · {} ETH", amount.decimal))
            .unwrap_or_default()
    );
    if let Some(best) = &response.recommended {
        println!("recommended: {}", best.opportunity_id);
        println!("  {}", best.reason);
        for tradeoff in &best.tradeoffs {
            println!("  - {tradeoff}");
        }
    }
    for item in &response.opportunities {
        println!(
            "[{}] {} · {:.2}% APY · TVL ${} · {}{}",
            item.safety_tier.as_str(),
            item.id,
            item.yield_info.ranking_apy_percent,
            item.market.tvl_usd,
            item.market.receipt_asset.symbol,
            item.estimated_annual_yield
                .as_ref()
                .map(|amount| format!(
                    " · ~{} {}/yr",
                    amount.decimal, item.market.receipt_asset.symbol
                ))
                .unwrap_or_default()
        );
    }
    for warning in &response.warnings {
        eprintln!("warning: {warning}");
    }
}

const POSITION_USAGE: &str = "Usage:\n  eollo position analyze --owner ADDRESS [--opportunities lido-steth,aave-v3-weth]\n    [--include-zero] [--cost-basis lido-steth=10,aave-v3-weth=5] [--url URL] [--json]";

async fn run_position(raw: &[String]) -> i32 {
    let Some(command) = raw.first().map(String::as_str) else {
        println!("{POSITION_USAGE}");
        return 2;
    };
    if command == "--help" || command == "-h" {
        println!("{POSITION_USAGE}");
        return 0;
    }
    if raw[1..].iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{POSITION_USAGE}");
        return 0;
    }
    match command {
        "analyze" => run_position_analyze(&raw[1..]).await,
        other => {
            eprintln!("unknown position command: {other}");
            println!("{POSITION_USAGE}");
            2
        }
    }
}

async fn run_position_analyze(raw: &[String]) -> i32 {
    let json = raw.iter().any(|arg| arg == "--json");
    let args = match parse_position_args(raw) {
        Ok(args) => args,
        Err(error) => {
            report_error_as("position.analyze", &error, json);
            return exit_code(error.code);
        }
    };
    let controller = match crate::routing::from_env_for_chain(&args.chain, args.url.as_deref()) {
        Ok(backend) => RuntimeClient::with_api_key(backend.base_url, backend.api_key),
        Err(error) => {
            report_error_as("position.analyze", &error, json);
            return exit_code(error.code);
        }
    };
    match controller
        .position_analyze(&PositionAnalyzeRequest {
            owner: args.owner,
            chain: args.chain,
            opportunity_ids: args.opportunities,
            include_zero: args.include_zero,
            cost_basis: args.cost_basis,
        })
        .await
    {
        Ok(response) => {
            if json {
                print_capability_json("position.analyze", &response);
            } else {
                print_position_human(&response);
            }
            0
        }
        Err(error) => {
            report_error_as("position.analyze", &error, json);
            exit_code(error.code)
        }
    }
}

struct PositionArgs {
    owner: alloy_primitives::Address,
    chain: String,
    opportunities: Vec<String>,
    include_zero: bool,
    cost_basis: Vec<PositionCostBasis>,
    url: Option<String>,
}

fn parse_position_args(raw: &[String]) -> Result<PositionArgs, CapabilityError> {
    let mut owner = None;
    let mut chain = "ethereum".to_string();
    let mut opportunities = Vec::new();
    let mut include_zero = false;
    let mut cost_basis = Vec::new();
    let mut url = None;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--owner" => {
                let text = value(raw, &mut index, "--owner")?;
                owner =
                    Some(text.parse::<alloy_primitives::Address>().map_err(|_| {
                        CapabilityError::invalid_argument("invalid --owner address")
                    })?);
            }
            "--chain" => chain = value(raw, &mut index, "--chain")?,
            "--opportunities" | "--opportunity-ids" => {
                opportunities = value(raw, &mut index, "--opportunities")?
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            "--include-zero" => include_zero = true,
            "--cost-basis" => {
                for item in value(raw, &mut index, "--cost-basis")?
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                {
                    let (id, amount) = item.split_once('=').ok_or_else(|| {
                        CapabilityError::invalid_argument(
                            "--cost-basis entries must look like id=amount",
                        )
                    })?;
                    cost_basis.push(PositionCostBasis {
                        opportunity_id: id.trim().to_string(),
                        amount: amount.trim().to_string(),
                    });
                }
            }
            "--url" => url = Some(value(raw, &mut index, "--url")?),
            "--json" => {}
            other => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown position option: {other}"
                )));
            }
        }
        index += 1;
    }
    let owner = owner.ok_or_else(|| CapabilityError::invalid_argument("--owner is required"))?;
    Ok(PositionArgs {
        owner,
        chain,
        opportunities,
        include_zero,
        cost_basis,
        url,
    })
}

fn print_position_human(response: &PositionAnalyzeResponse) {
    println!(
        "Positions · {:#x} · block {} · {} active",
        response.owner, response.block_number, response.totals.active_count
    );
    for position in &response.positions {
        println!(
            "[{}] {} · {} {} · {} ETH · {:.2}% APY{}",
            position.safety_tier.as_str(),
            position.opportunity_id,
            position.receipt_balance.decimal,
            position.receipt_asset.symbol,
            position
                .value_eth
                .as_ref()
                .map(|value| value.decimal.clone())
                .unwrap_or_else(|| "?".to_string()),
            position.apy.ranking_apy_percent,
            position
                .estimated_annual_yield_eth
                .as_ref()
                .map(|value| format!(" · ~{} ETH/yr", value.decimal))
                .unwrap_or_default()
        );
    }
    if let Some(value) = &response.totals.value_eth {
        println!("total value: {} ETH", value.decimal);
    }
    if let Some(pnl) = &response.totals.pnl_eth {
        println!("total PnL: {} ETH", pnl.decimal);
    }
    for warning in &response.warnings {
        eprintln!("warning: {warning}");
    }
}

fn capability_json_value<T: serde::Serialize>(capability: &str, response: &T) -> serde_json::Value {
    let value = serde_json::to_value(response).unwrap();
    if uses_v2(capability) {
        wrap_success(capability, value)
    } else {
        value
    }
}

fn capability_list_json() -> serde_json::Value {
    serde_json::json!({
        "schema_version": crate::capability::SCHEMA_VERSION,
        "capabilities": CapabilityRegistry::global().iter().collect::<Vec<_>>(),
    })
}

fn print_capability_json<T: serde::Serialize>(capability: &str, response: &T) {
    let value = capability_json_value(capability, response);
    println!("{}", serde_json::to_string_pretty(&value).unwrap());
}

fn print_capability_entry(entry: &CapabilityEntry) {
    println!("{} — {}", entry.id, entry.summary);
    println!("  status: {}", entry.status);
    println!("  execution: {}", entry.execution);
    println!("  transports: {}", entry.transports.join(", "));
    println!("  chains: {}", entry.chains.join(", "));
    if let Some(default) = &entry.default_chain {
        println!("  default chain: {default}");
    }
    println!("  side effects: {}", entry.side_effects);
    if !entry.requires.is_empty() {
        println!("  requires: {}", entry.requires.join(", "));
    }
    println!("  schema: {}", entry.schema);
    println!("  skill: {}", entry.skill);
    println!("  docs: {}", entry.docs);
}

async fn run_capability(raw: &[String]) -> i32 {
    match raw.first().map(String::as_str) {
        Some("list") => {
            let json = raw.iter().any(|arg| arg == "--json");
            let registry = CapabilityRegistry::global();
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&capability_list_json()).unwrap()
                );
            } else {
                for entry in registry.iter() {
                    println!("{} [{}] {}", entry.id, entry.status, entry.chains.join(","));
                }
            }
            0
        }
        Some("describe") => {
            let Some(name) = raw.get(1).filter(|value| !value.starts_with('-')) else {
                eprintln!("usage: eollo capability describe NAME [--json]");
                return 2;
            };
            let json = raw.iter().any(|arg| arg == "--json");
            let Some(entry) = CapabilityRegistry::global().get(name) else {
                let error = CapabilityError::new(
                    CapabilityErrorCode::Unsupported,
                    format!("unknown capability: {name}"),
                    false,
                );
                report_error(&error, json);
                return exit_code(error.code);
            };
            if json {
                println!("{}", serde_json::to_string_pretty(entry).unwrap());
            } else {
                print_capability_entry(entry);
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
    fn parses_wallet_connect_chain() {
        assert_eq!(
            parse_wallet_connect_args(&[
                "--chain".to_string(),
                "bsc".to_string(),
                "--json".to_string()
            ])
            .unwrap(),
            (true, 56)
        );
        assert_eq!(parse_wallet_connect_args(&[]).unwrap().1, 56);
        assert_eq!(
            parse_wallet_connect_args(&["--chain".to_string(), "ethereum".to_string()])
                .unwrap()
                .1,
            1
        );
        assert!(parse_wallet_connect_args(&["--chain".to_string(), "nope".to_string()]).is_err());
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
            "--policy".to_string(),
            "policy.json".to_string(),
            "--json".to_string(),
        ])
        .unwrap();
        assert_eq!(args.message.as_deref(), Some("hello"));
        assert_eq!(args.policy.as_deref(), Some("policy.json"));
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
            "--policy".to_string(),
            "policy.json".to_string(),
        ])
        .unwrap();
        assert_eq!(args.token, "ETH");
        assert_eq!(args.policy.as_deref(), Some("policy.json"));
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
        assert_eq!(assets.chain, Chain::Bsc);
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
        assert_eq!(balance.chain, Chain::Bsc);
        assert_eq!(balance.tokens, vec!["USDC", "LDO"]);

        let ethereum = parse_wallet_args(&[
            "balance".to_string(),
            "0x0000000000000000000000000000000000000001".to_string(),
            "--chain".to_string(),
            "ethereum".to_string(),
        ])
        .unwrap();
        assert_eq!(ethereum.chain, Chain::Ethereum);
    }

    #[test]
    fn parses_signature_arguments() {
        let parsed = parse_signature_args(&[
            "--typed-data".to_string(),
            "permit.json".to_string(),
            "--chain".to_string(),
            "ethereum".to_string(),
            "--policy".to_string(),
            "policy.json".to_string(),
            "--json".to_string(),
        ])
        .unwrap();
        assert_eq!(parsed.chain, Chain::Ethereum);
        assert_eq!(parsed.typed_data.as_deref(), Some("permit.json"));
        assert_eq!(parsed.policy.as_deref(), Some("policy.json"));
        assert!(parsed.json);
        assert!(
            parse_signature_args(&[
                "--typed-data".to_string(),
                "a".to_string(),
                "extra".to_string()
            ])
            .is_err()
        );
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
            "--policy".to_string(),
            "policy.json".to_string(),
        ])
        .unwrap();
        assert_eq!(parsed.value, "0.5");
        assert_eq!(parsed.policy.as_deref(), Some("policy.json"));
        assert_eq!(parsed.chain, Chain::Bsc);
        assert_eq!(parsed.block, Some(100));
        assert!(parse_transaction_args(&["--data".to_string(), "0x".to_string()]).is_err());
    }

    #[test]
    fn parses_transaction_verify_arguments() {
        let hash = "0x00000000000000000000000000000000000000000000000000000000000000aa";
        let parsed = parse_verify_args(&[
            "--hash".to_string(),
            hash.to_string(),
            "--chain".to_string(),
            "ethereum".to_string(),
            "--confirmations".to_string(),
            "3".to_string(),
            "--expect".to_string(),
            "inspect.json".to_string(),
        ])
        .unwrap();
        assert_eq!(parsed.hash, hash);
        assert_eq!(parsed.chain, Chain::Ethereum);
        assert_eq!(parsed.confirmations, 3);
        assert_eq!(parsed.expect.as_deref(), Some("inspect.json"));
        assert!(parse_verify_args(&[]).is_err());
        assert!(
            parse_verify_args(&[
                "--hash".to_string(),
                hash.to_string(),
                "--chain".to_string(),
                "solana".to_string(),
            ])
            .is_err()
        );
    }

    #[test]
    fn capability_list_covers_every_cli_capability() {
        let listed = capability_list_json();
        let listed = listed["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        let expected = CapabilityRegistry::global()
            .iter()
            .filter(|entry| entry.transports.iter().any(|transport| transport == "cli"))
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(listed, expected);
    }

    #[test]
    fn wraps_v2_capability_json_at_the_cli_boundary() {
        let value = serde_json::json!({
            "chain": "bsc",
            "chain_id": 56,
            "owner": "0x0000000000000000000000000000000000000001",
            "spender": "0x0000000000000000000000000000000000000002",
            "block_number": 1,
            "block_hash": "0xabc",
            "allowances": [],
            "coverage": "local",
            "sources": [],
            "warnings": []
        });
        let wrapped = capability_json_value("wallet.allowance", &value);
        assert_eq!(wrapped["meta"]["schema_version"], 2);
        assert_eq!(wrapped["meta"]["capability"], "wallet.allowance");
        assert_eq!(wrapped["chain"]["name"], "bsc");
        assert!(wrapped["data"].get("owner").is_some());
        assert!(wrapped["data"].get("chain").is_none());
    }

    #[test]
    fn leaves_unknown_capability_json_unchanged_at_the_cli_boundary() {
        let value = serde_json::json!({"chain": "bsc", "found": true});
        let output = capability_json_value("unknown.capability", &value);
        assert_eq!(output, value);
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
