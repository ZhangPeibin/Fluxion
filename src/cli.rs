//! Agent-facing CLI. Commands are thin clients over a warm local runtime.

use std::env;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::capability::{CapabilityError, CapabilityErrorCode};
use crate::client::{RuntimeClient, request_id};
use crate::swap::quote::{QuoteErrorResponse, QuoteRequest};
use crate::token;

const DEFAULT_URL: &str = "http://127.0.0.1:8080";

pub async fn dispatch(args: Vec<String>) -> (Vec<String>, Option<i32>) {
    let Some(first) = args.first().map(String::as_str) else {
        // Preserve the historical no-argument server startup.
        return (args, None);
    };
    match first {
        "serve" => (args[1..].to_vec(), None),
        "--config" => (args, None),
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
        "start" | "up" => {
            let code = run_up(&args[1..]).await;
            (args, Some(code))
        }
        "stop" | "down" => {
            let code = run_down();
            (args, Some(code))
        }
        "status" => {
            let code = run_status(&args[1..]).await;
            (args, Some(code))
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
fluxion start                           Start the self-hosted local runtime\n  \
fluxion stop                            Stop the self-hosted local runtime\n  \
fluxion status                          Show local runtime readiness and state\n  \
fluxion swap ldo-eth 100                Shorthand quote\n  \
fluxion ldo-eth 100                     Top-level shorthand alias\n  \
fluxion capability list                  List capabilities\n  \
fluxion capability describe NAME        Show capability and execution details\n  \
fluxion swap quote --in TOKEN --out TOKEN --amount AMOUNT [options]\n  \
fluxion mcp [--url URL]                 Expose capabilities over MCP stdio\n  \
fluxion serve [--config PATH]           Run the local runtime in the foreground\n  \
fluxion tokens                          List built-in token symbols\n  \
fluxion --help\n\n\
Quote options:\n  \
--json                    Print the swap.quote JSON response\n  \
--url URL                 Runtime URL (default {DEFAULT_URL}, env FLUXION_URL)\n  \
--no-auto-start           Fail instead of starting a local runtime\n  \
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
    url: String,
    split: bool,
    max_hops: Option<usize>,
    split_max_branches: Option<usize>,
    decimals_in: Option<u8>,
    decimals_out: Option<u8>,
    auto_start: bool,
}

fn parse_quote_args(raw: &[String]) -> Result<QuoteArgs, CapabilityError> {
    let mut token_in = None;
    let mut token_out = None;
    let mut amount = None;
    let mut json = false;
    let mut url = env::var("FLUXION_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let mut split = true;
    let mut max_hops = None;
    let mut split_max_branches = None;
    let mut decimals_in = None;
    let mut decimals_out = None;
    let mut auto_start = true;
    let mut index = 0;
    while index < raw.len() {
        let arg = raw[index].as_str();
        match arg {
            "--json" => json = true,
            "--no-auto-start" => auto_start = false,
            "--no-split" => split = false,
            "--split" => split = true,
            "--in" => token_in = Some(value(raw, &mut index, "--in")?),
            "--out" => token_out = Some(value(raw, &mut index, "--out")?),
            "--amount" => amount = Some(value(raw, &mut index, "--amount")?),
            "--url" => url = value(raw, &mut index, "--url")?,
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
        auto_start,
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
    let backend = match crate::routing::from_env("local-cli", &args.url) {
        Ok(backend) => backend,
        Err(error) => {
            report_error(&error, args.json);
            return exit_code(error.code);
        }
    };
    let client = RuntimeClient::with_api_key(backend.base_url.clone(), backend.api_key.clone());
    if backend.is_local()
        && let Err(error) = ensure_runtime(&client, args.auto_start).await
    {
        report_error(&error, args.json);
        return exit_code(error.code);
    }
    match client
        .swap_quote(&request, token_in, token_out, "ethereum", backend.provider)
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
                for provider in &descriptor.providers {
                    if provider.available {
                        println!(
                            "  {:<12} {:<7} {:<10} available",
                            provider.id.as_str(),
                            provider.location,
                            provider.transport
                        );
                    } else {
                        println!(
                            "  {:<12} {:<7} {:<10} unavailable: {}",
                            provider.id.as_str(),
                            provider.location,
                            provider.transport,
                            provider.unavailable_reason.unwrap_or("not available")
                        );
                    }
                }
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

async fn run_up(raw: &[String]) -> i32 {
    let mut url = env::var("FLUXION_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let mut config = runtime_config();
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--config" => {
                index += 1;
                let Some(value) = raw.get(index) else {
                    eprintln!("--config needs a value");
                    return 2;
                };
                config = value.clone();
            }
            "--url" => {
                index += 1;
                let Some(value) = raw.get(index) else {
                    eprintln!("--url needs a value");
                    return 2;
                };
                url = value.clone();
            }
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            other => {
                eprintln!("unknown up option: {other}");
                return 2;
            }
        }
        index += 1;
    }
    let client = RuntimeClient::new(url);
    if client.ready().await {
        println!("runtime already ready at {}", client.base_url());
        return 0;
    }
    if let Err(error) = spawn_runtime(&config) {
        report_error(&error, false);
        return exit_code(error.code);
    }
    match wait_ready(&client).await {
        Ok(()) => {
            println!("runtime ready at {}", client.base_url());
            0
        }
        Err(error) => {
            report_error(&error, false);
            exit_code(error.code)
        }
    }
}

fn run_down() -> i32 {
    let pid_path = pid_file();
    let Ok(text) = fs::read_to_string(&pid_path) else {
        println!("runtime is not running");
        return 0;
    };
    let pid = text.trim();
    if pid.is_empty() {
        println!("runtime is not running");
        return 0;
    }
    let status = Command::new("kill").arg(pid).status();
    let _ = fs::remove_file(&pid_path);
    match status {
        Ok(status) if status.success() => {
            println!("stopped runtime pid {pid}");
            0
        }
        _ => {
            println!("runtime pid {pid} was not running");
            0
        }
    }
}

async fn run_status(raw: &[String]) -> i32 {
    let mut url = env::var("FLUXION_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--url" => {
                index += 1;
                let Some(value) = raw.get(index) else {
                    eprintln!("--url needs a value");
                    return 2;
                };
                url = value.clone();
            }
            "--help" | "-h" => {
                print_help();
                return 0;
            }
            other => {
                eprintln!("unknown status option: {other}");
                return 2;
            }
        }
        index += 1;
    }
    let client = RuntimeClient::new(url);
    if !client.ready().await {
        eprintln!("runtime not ready at {}", client.base_url());
        return 5;
    }
    match client.state().await {
        Some(state) => {
            println!("{}", serde_json::to_string_pretty(&state).unwrap());
            0
        }
        None => {
            eprintln!("runtime ready but /state is unavailable");
            1
        }
    }
}

fn runtime_dir() -> PathBuf {
    let base = env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    base.join(".fluxion")
}

fn pid_file() -> PathBuf {
    runtime_dir().join("runtime.pid")
}

fn log_file() -> PathBuf {
    runtime_dir().join("runtime.log")
}

fn runtime_config() -> String {
    if let Ok(path) = env::var("FLUXION_CONFIG") {
        return path;
    }
    for path in ["configs/runtime.yaml", "config.yaml", "config.example.yaml"] {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
    }
    let dir = runtime_dir();
    let path = dir.join("config.yaml");
    if !path.exists() {
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(&path, include_str!("../configs/runtime.yaml"));
    }
    path.display().to_string()
}

fn is_local_url(url: &str) -> bool {
    let url = url.to_ascii_lowercase();
    url.starts_with("http://127.0.0.1")
        || url.starts_with("http://localhost")
        || url.starts_with("http://[::1]")
}

fn spawn_runtime(config: &str) -> Result<(), CapabilityError> {
    if !std::path::Path::new(config).exists() {
        return Err(CapabilityError::invalid_argument(format!(
            "runtime config not found: {config}; run `fluxion start --config PATH`"
        )));
    }
    let parsed = crate::config::Config::load(config).map_err(|error| {
        CapabilityError::invalid_argument(format!("invalid config {config}: {error}"))
    })?;
    let needs_graph_key = parsed.enabled_platforms.is_empty()
        || parsed
            .enabled_platforms
            .iter()
            .any(|platform| platform == "uniswap-v2" || platform == "uniswap-v3");
    let has_graph_key = env::var("GRAPH_API_KEY")
        .ok()
        .is_some_and(|key| !key.trim().is_empty());
    if needs_graph_key && !has_graph_key {
        return Err(CapabilityError::invalid_argument(
            "GRAPH_API_KEY is required before `fluxion start`; export it or put it in .env",
        ));
    }
    let dir = runtime_dir();
    fs::create_dir_all(&dir)
        .map_err(|error| CapabilityError::internal(format!("cannot create {dir:?}: {error}")))?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file())
        .map_err(|error| CapabilityError::internal(format!("cannot open runtime log: {error}")))?;
    let stderr = log
        .try_clone()
        .map_err(|error| CapabilityError::internal(format!("cannot clone log handle: {error}")))?;
    let exe = env::current_exe().map_err(|error| {
        CapabilityError::internal(format!("cannot locate fluxion binary: {error}"))
    })?;
    let child = Command::new(exe)
        .args(["serve", "--config", config])
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| CapabilityError::internal(format!("cannot start runtime: {error}")))?;
    let _ = fs::write(pid_file(), child.id().to_string());
    eprintln!(
        "warming Fluxion runtime (pid {}, log {})",
        child.id(),
        log_file().display()
    );
    Ok(())
}

async fn wait_ready(client: &RuntimeClient) -> Result<(), CapabilityError> {
    for _ in 0..360 {
        if client.ready().await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(CapabilityError::new(
        CapabilityErrorCode::Timeout,
        "runtime did not become ready within 180s; check ~/.fluxion/runtime.log",
        true,
    ))
}

async fn ensure_runtime(client: &RuntimeClient, auto_start: bool) -> Result<(), CapabilityError> {
    if client.ready().await {
        return Ok(());
    }
    if !auto_start {
        return Err(CapabilityError::new(
            CapabilityErrorCode::RpcError,
            format!("runtime not ready at {}", client.base_url()),
            true,
        ));
    }
    if !is_local_url(client.base_url()) {
        return Err(CapabilityError::new(
            CapabilityErrorCode::RpcError,
            format!(
                "remote runtime not reachable at {}; start it or pass --no-auto-start",
                client.base_url()
            ),
            true,
        ));
    }
    spawn_runtime(&runtime_config())?;
    wait_ready(client).await
}

async fn run_mcp(raw: &[String]) -> i32 {
    let mut url = env::var("FLUXION_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let mut auto_start = true;
    let mut index = 0;
    while index < raw.len() {
        match raw[index].as_str() {
            "--no-auto-start" => auto_start = false,
            "--url" => {
                index += 1;
                let Some(value) = raw.get(index) else {
                    eprintln!("--url needs a value");
                    return 2;
                };
                url = value.clone();
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
    let backend = match crate::routing::from_env("local-mcp", &url) {
        Ok(backend) => backend,
        Err(error) => {
            report_error(&error, false);
            return exit_code(error.code);
        }
    };
    let client = RuntimeClient::with_api_key(backend.base_url.clone(), backend.api_key.clone());
    if backend.is_local()
        && let Err(error) = ensure_runtime(&client, auto_start).await
    {
        report_error(&error, false);
        return exit_code(error.code);
    }
    crate::mcp::run_stdio(client, backend.provider).await
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
