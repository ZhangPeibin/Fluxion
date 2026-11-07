//! Agent-facing CLI. Commands are thin clients over a warm local runtime.

use crate::capability::{CapabilityError, CapabilityErrorCode};
use crate::client::{RuntimeClient, request_id};
use crate::swap::quote::{QuoteErrorResponse, QuoteRequest};
use crate::token;
use crate::wallet::{self, AssetsRequest, BalanceRequest, WalletBalancesResponse, WalletConfig};

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
}

#[derive(Debug)]
struct WalletArgs {
    mode: WalletMode,
    address: String,
    tokens: Vec<String>,
    json: bool,
    rpc_url: Option<String>,
    block: Option<u64>,
    include_native: bool,
    include_unknown: bool,
}

async fn run_wallet(raw: &[String]) -> i32 {
    let args = match parse_wallet_args(raw) {
        Ok(args) => args,
        Err(error) if error.message == "help requested" => {
            println!(
                "Usage:\n  fluxion wallet balance ADDRESS [--tokens ETH,USDC] [--no-native]\n  fluxion wallet assets ADDRESS [--include-unknown] [--no-native]\n\nOptions:\n  --rpc-url URL   local Ethereum RPC (default FLUXION_RPC_URL)\n  --block N       pin a block\n  --json          print the structured response"
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
    let result = match args.mode {
        WalletMode::Balance => {
            wallet::balance(
                &config,
                BalanceRequest {
                    address,
                    tokens: args.tokens,
                    include_native: args.include_native,
                    block: args.block,
                },
            )
            .await
        }
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
        }
    };
    match result {
        Ok(response) => {
            if args.json {
                println!("{}", serde_json::to_string_pretty(&response).unwrap());
            } else {
                print_wallet_human(&response);
            }
            0
        }
        Err(error) => {
            report_error(&error, args.json);
            exit_code(error.code)
        }
    }
}

fn parse_wallet_args(raw: &[String]) -> Result<WalletArgs, CapabilityError> {
    let mode = match raw.first().map(String::as_str) {
        Some("balance") => WalletMode::Balance,
        Some("assets") => WalletMode::Assets,
        Some("--help" | "-h") => return Err(CapabilityError::invalid_argument("help requested")),
        Some(other) => {
            return Err(CapabilityError::invalid_argument(format!(
                "unknown wallet command: {other}"
            )));
        }
        None => return Err(CapabilityError::invalid_argument("missing wallet command")),
    };
    let mut address = None;
    let mut tokens = Vec::new();
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
    Ok(WalletArgs {
        mode,
        address,
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
    if json {
        let response = QuoteErrorResponse::new(request_id(), error.clone());
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
