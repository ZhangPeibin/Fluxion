//! Agent-facing CLI. Commands are thin clients over a warm local runtime.

use crate::capability::{CapabilityError, CapabilityErrorCode};
use crate::client::{RuntimeClient, request_id};
use crate::swap::quote::{QuoteErrorResponse, QuoteRequest};
use crate::token;

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
