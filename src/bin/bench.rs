//! Quote benchmark: compares Fluxion's `/quote` with external aggregator APIs.
//!
//! Usage:
//!   cargo run --bin bench -- [--service URL] [--corpus PATH] [--out PATH.md]
//!                              [--limit N] [--delay-ms MS]
//!
//! Aggregators are enabled when their key is present in `.env`:
//!   ONEINCH_API_KEY  -> 1inch v6 quote API
//!   ZEROX_API_KEY    -> 0x allowance-holder quote API
//!
//! ParaSwap is always enabled (keyless). A real mainnet service is required.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use fluxion::config::Config;
use fluxion::rpc::RpcClient;

const ONEINCH_URL: &str = "https://api.1inch.dev/swap/v6.0/1/quote";
const ZEROX_URL: &str = "https://api.0x.org/swap/allowance-holder/price";
const PARASWAP_URL: &str = "https://api.paraswap.io/prices";

#[derive(Debug, Deserialize, Serialize)]
struct Corpus {
    symbols: HashMap<String, String>,
    #[serde(default)]
    decimals: HashMap<String, u8>,
    amounts: HashMap<String, Vec<String>>,
    pairs: Vec<[String; 2]>,
}

#[derive(Debug, Clone, Serialize)]
struct Side {
    amount_out: Option<String>,
    latency_ms: u128,
    hops: Option<usize>,
    error: Option<String>,
    platforms: Vec<String>,
}

impl Side {
    fn ok(amount_out: String, latency_ms: u128, hops: Option<usize>) -> Self {
        Self {
            amount_out: Some(amount_out),
            latency_ms,
            hops,
            error: None,
            platforms: Vec::new(),
        }
    }

    fn error(latency_ms: u128, error: String) -> Self {
        Self {
            amount_out: None,
            latency_ms,
            hops: None,
            error: Some(error),
            platforms: Vec::new(),
        }
    }

    fn out(&self) -> Option<f64> {
        self.amount_out.as_ref()?.parse().ok()
    }

    fn is_ok(&self) -> bool {
        self.amount_out.is_some()
    }
}

#[derive(Debug, Serialize)]
struct Row {
    started_unix_ms: u128,
    finished_unix_ms: u128,
    token_in: String,
    token_out: String,
    amount_in: String,
    fluxion: Side,
    paraswap: Side,
    oneinch: Option<Side>,
    zerox: Option<Side>,
}

#[derive(Debug, Serialize)]
struct Summary {
    rows: usize,
    fluxion_ok: usize,
    paraswap_ok: usize,
    oneinch_ok: usize,
    zerox_ok: usize,
    paraswap_compared: usize,
    paraswap_median_delta_bps: Option<f64>,
    paraswap_mean_delta_bps: Option<f64>,
    paraswap_within_50_bps: usize,
    oneinch_compared: usize,
    oneinch_median_delta_bps: Option<f64>,
    oneinch_mean_delta_bps: Option<f64>,
    oneinch_within_50_bps: usize,
    zerox_compared: usize,
    zerox_median_delta_bps: Option<f64>,
    zerox_mean_delta_bps: Option<f64>,
    zerox_within_50_bps: usize,
    fluxion_latency_p50_ms: Option<u128>,
    fluxion_latency_p95_ms: Option<u128>,
    paraswap_latency_p50_ms: Option<u128>,
    paraswap_latency_p95_ms: Option<u128>,
    oneinch_latency_p50_ms: Option<u128>,
    oneinch_latency_p95_ms: Option<u128>,
    zerox_latency_p50_ms: Option<u128>,
    zerox_latency_p95_ms: Option<u128>,
    comparisons: HashMap<String, Comparison>,
}

#[derive(Debug, Serialize)]
struct Comparison {
    both_ok: usize,
    fluxion_only: usize,
    aggregator_only: usize,
    both_failed: usize,
}

fn comparison<'a>(rows: &'a [Row], pick: impl Fn(&'a Row) -> Option<&'a Side>) -> Comparison {
    let mut counts = Comparison {
        both_ok: 0,
        fluxion_only: 0,
        aggregator_only: 0,
        both_failed: 0,
    };
    for row in rows {
        if let Some(other) = pick(row) {
            match (row.fluxion.is_ok(), other.is_ok()) {
                (true, true) => counts.both_ok += 1,
                (true, false) => counts.fluxion_only += 1,
                (false, true) => counts.aggregator_only += 1,
                (false, false) => counts.both_failed += 1,
            }
        }
    }
    counts
}

#[derive(Debug, Serialize)]
struct Meta {
    timestamp_unix: u64,
    git_sha: Option<String>,
    service: String,
    corpus: String,
    start_block: Option<u64>,
    end_block: Option<u64>,
    paraswap_enabled: bool,
    oneinch_enabled: bool,
    zerox_enabled: bool,
    working_tree_dirty: bool,
    service_info: Value,
    corpus_snapshot: Corpus,
    pair_count: usize,
    delay_ms: u64,
    code_fingerprint: String,
}

#[derive(Debug, Serialize)]
struct Report<'a> {
    meta: &'a Meta,
    summary: &'a Summary,
    rows: &'a [Row],
}

struct Args {
    service: String,
    corpus: String,
    out: Option<String>,
    limit: Option<usize>,
    delay_ms: u64,
    config: String,
}

fn print_help() {
    println!(
        "Usage: cargo run --bin bench -- [options]\n\
         \n\
         Options:\n\
           --service URL    Fluxion base URL (default http://127.0.0.1:8080)\n\
           --corpus PATH    corpus YAML (default bench/corpus.yaml)\n\
           --out PATH       Markdown output path (also writes PATH.json)\n\
           --limit N        only benchmark the first N pairs\n\
            --delay-ms MS    pause after each external request (default 250)\n\
            --config PATH    local RPC config (default config.yaml)\n\
         \n\
         Env (optional, .env works): ONEINCH_API_KEY, ZEROX_API_KEY"
    );
}

fn parse_args() -> Args {
    let mut args = Args {
        service: "http://127.0.0.1:8080".to_string(),
        corpus: "bench/corpus.yaml".to_string(),
        out: None,
        limit: None,
        delay_ms: 250,
        config: "config.yaml".to_string(),
    };

    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--service" => args.service = it.next().expect("--service needs a URL"),
            "--corpus" => args.corpus = it.next().expect("--corpus needs a path"),
            "--out" => args.out = Some(it.next().expect("--out needs a path")),
            "--config" => args.config = it.next().expect("--config needs a path"),
            "--limit" => {
                args.limit = Some(
                    it.next()
                        .expect("--limit needs an integer")
                        .parse()
                        .expect("invalid --limit"),
                )
            }
            "--delay-ms" => {
                args.delay_ms = it
                    .next()
                    .expect("--delay-ms needs an integer")
                    .parse()
                    .expect("invalid --delay-ms");
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}");
                print_help();
                std::process::exit(2);
            }
        }
    }
    args
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn symbol(corpus: &Corpus, address: &str) -> String {
    corpus
        .symbols
        .get(&address.to_lowercase())
        .cloned()
        .unwrap_or_else(|| short(address))
}

fn decimals(corpus: &Corpus, address: &str) -> u8 {
    corpus
        .decimals
        .get(&address.to_lowercase())
        .copied()
        .unwrap_or(18)
}

fn short(address: &str) -> String {
    if address.len() < 12 {
        return address.to_string();
    }
    format!("{}…{}", &address[..6], &address[address.len() - 4..])
}

fn snippet(text: &str) -> String {
    let cleaned = text.replace(['\n', '\r', '|'], " ");
    if cleaned.chars().count() > 160 {
        format!("{}…", cleaned.chars().take(160).collect::<String>())
    } else {
        cleaned
    }
}

fn parse_amount(value: &str) -> Option<String> {
    U256::from_str_radix(value, 10).ok().map(|v| v.to_string())
}

fn delta_bps(fluxion: &Side, other: &Side) -> Option<f64> {
    let fluxion_out = fluxion.out()?;
    let other_out = other.out()?;
    if other_out <= 0.0 {
        return None;
    }
    Some((fluxion_out - other_out) / other_out * 10_000.0)
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[mid - 1] + values[mid]) / 2.0)
    } else {
        Some(values[mid])
    }
}

fn percentile(values: &mut [u128], fraction: f64) -> Option<u128> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let rank = (fraction * values.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(values.len() - 1);
    Some(values[index])
}

fn side_latencies<F>(rows: &[Row], pick: F) -> Vec<u128>
where
    F: Fn(&Row) -> Option<&Side>,
{
    rows.iter()
        .filter_map(pick)
        .map(|side| side.latency_ms)
        .collect()
}

fn aggregator_deltas<F>(rows: &[Row], pick: F) -> Vec<f64>
where
    F: Fn(&Row) -> Option<&Side>,
{
    rows.iter()
        .filter_map(|row| {
            let other = pick(row)?;
            delta_bps(&row.fluxion, other)
        })
        .collect()
}

fn summarize(rows: &[Row]) -> Summary {
    let mut paraswap_deltas = aggregator_deltas(rows, |row| Some(&row.paraswap));
    let mut oneinch_deltas = aggregator_deltas(rows, |row| row.oneinch.as_ref());
    let mut zerox_deltas = aggregator_deltas(rows, |row| row.zerox.as_ref());
    let mut fluxion_latencies = side_latencies(rows, |row| Some(&row.fluxion));
    let mut paraswap_latencies = side_latencies(rows, |row| Some(&row.paraswap));
    let mut oneinch_latencies = side_latencies(rows, |row| row.oneinch.as_ref());
    let mut zerox_latencies = side_latencies(rows, |row| row.zerox.as_ref());

    let mean = |values: &[f64]| -> Option<f64> {
        if values.is_empty() {
            None
        } else {
            Some(values.iter().sum::<f64>() / values.len() as f64)
        }
    };
    let within_50 = |values: &[f64]| values.iter().filter(|d| d.abs() <= 50.0).count();

    Summary {
        rows: rows.len(),
        fluxion_ok: rows.iter().filter(|row| row.fluxion.is_ok()).count(),
        paraswap_ok: rows.iter().filter(|row| row.paraswap.is_ok()).count(),
        oneinch_ok: rows
            .iter()
            .filter(|row| row.oneinch.as_ref().is_some_and(Side::is_ok))
            .count(),
        zerox_ok: rows
            .iter()
            .filter(|row| row.zerox.as_ref().is_some_and(Side::is_ok))
            .count(),
        paraswap_compared: paraswap_deltas.len(),
        paraswap_median_delta_bps: median(&mut paraswap_deltas),
        paraswap_mean_delta_bps: mean(&paraswap_deltas),
        paraswap_within_50_bps: within_50(&paraswap_deltas),
        oneinch_compared: oneinch_deltas.len(),
        oneinch_median_delta_bps: median(&mut oneinch_deltas),
        oneinch_mean_delta_bps: mean(&oneinch_deltas),
        oneinch_within_50_bps: within_50(&oneinch_deltas),
        zerox_compared: zerox_deltas.len(),
        zerox_median_delta_bps: median(&mut zerox_deltas),
        zerox_mean_delta_bps: mean(&zerox_deltas),
        zerox_within_50_bps: within_50(&zerox_deltas),
        fluxion_latency_p50_ms: percentile(&mut fluxion_latencies, 0.50),
        fluxion_latency_p95_ms: percentile(&mut fluxion_latencies, 0.95),
        paraswap_latency_p50_ms: percentile(&mut paraswap_latencies, 0.50),
        paraswap_latency_p95_ms: percentile(&mut paraswap_latencies, 0.95),
        oneinch_latency_p50_ms: percentile(&mut oneinch_latencies, 0.50),
        oneinch_latency_p95_ms: percentile(&mut oneinch_latencies, 0.95),
        zerox_latency_p50_ms: percentile(&mut zerox_latencies, 0.50),
        zerox_latency_p95_ms: percentile(&mut zerox_latencies, 0.95),
        comparisons: HashMap::from([
            (
                "paraswap".into(),
                comparison(rows, |row| Some(&row.paraswap)),
            ),
            ("1inch".into(), comparison(rows, |row| row.oneinch.as_ref())),
            ("0x".into(), comparison(rows, |row| row.zerox.as_ref())),
        ]),
    }
}

async fn quote_fluxion(
    client: &reqwest::Client,
    service: &str,
    token_in: &str,
    token_out: &str,
    amount_in: &str,
) -> Side {
    let url = format!("{}/quote", service.trim_end_matches('/'));
    let started = Instant::now();
    let response = client
        .post(&url)
        .json(&json!({
            "token_in": token_in,
            "token_out": token_out,
            "amount_in": amount_in,
        }))
        .send()
        .await;
    let (text, latency) = match response_text(response, started).await {
        Ok(result) => result,
        Err(side) => return side,
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Side::error(latency, format!("invalid JSON: {}", snippet(&text)));
    };
    let Some(amount_out) = value.get("amount_out").and_then(Value::as_str) else {
        return Side::error(latency, format!("missing amount_out: {}", snippet(&text)));
    };
    let Some(amount_out) = parse_amount(amount_out) else {
        return Side::error(latency, "amount_out is not a decimal integer".to_string());
    };
    let hops = value.get("path").and_then(Value::as_array).map(Vec::len);
    let mut side = Side::ok(amount_out, latency, hops);
    if let Some(path) = value.get("path").and_then(Value::as_array) {
        side.platforms = path
            .iter()
            .filter_map(|hop| hop.get("platform")?.as_str().map(str::to_string))
            .collect();
    }
    side
}

async fn quote_paraswap(
    client: &reqwest::Client,
    token_in: &str,
    token_out: &str,
    amount_in: &str,
    src_decimals: u8,
    dest_decimals: u8,
) -> Side {
    let src_decimals = src_decimals.to_string();
    let dest_decimals = dest_decimals.to_string();
    let started = Instant::now();
    let response = client
        .get(PARASWAP_URL)
        .query(&[
            ("srcToken", token_in),
            ("destToken", token_out),
            ("amount", amount_in),
            ("srcDecimals", src_decimals.as_str()),
            ("destDecimals", dest_decimals.as_str()),
            ("side", "SELL"),
            ("network", "1"),
        ])
        .send()
        .await;
    let (text, latency) = match response_text(response, started).await {
        Ok(result) => result,
        Err(side) => return side,
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Side::error(latency, format!("invalid JSON: {}", snippet(&text)));
    };
    let Some(route) = value.get("priceRoute") else {
        return Side::error(latency, format!("missing priceRoute: {}", snippet(&text)));
    };
    let Some(amount_out) = route.get("destAmount").and_then(Value::as_str) else {
        return Side::error(latency, format!("missing destAmount: {}", snippet(&text)));
    };
    let Some(amount_out) = parse_amount(amount_out) else {
        return Side::error(latency, "destAmount is not a decimal integer".to_string());
    };
    let hops = route
        .get("bestRoute")
        .and_then(Value::as_array)
        .and_then(|routes| routes.first())
        .and_then(|route| route.get("swaps"))
        .and_then(Value::as_array)
        .map(Vec::len);
    let mut side = Side::ok(amount_out, latency, hops);
    side.platforms = route_names(route, "exchange");
    side
}

async fn quote_oneinch(
    client: &reqwest::Client,
    key: &str,
    token_in: &str,
    token_out: &str,
    amount_in: &str,
) -> Side {
    let started = Instant::now();
    let response = client
        .get(ONEINCH_URL)
        .query(&[("src", token_in), ("dst", token_out), ("amount", amount_in)])
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await;
    let (text, latency) = match response_text(response, started).await {
        Ok(result) => result,
        Err(side) => return side,
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Side::error(latency, format!("invalid JSON: {}", snippet(&text)));
    };
    let Some(amount_out) = value.get("dstAmount").and_then(Value::as_str) else {
        return Side::error(latency, format!("missing dstAmount: {}", snippet(&text)));
    };
    let Some(amount_out) = parse_amount(amount_out) else {
        return Side::error(latency, "dstAmount is not a decimal integer".to_string());
    };
    let hops = value
        .get("protocols")
        .and_then(Value::as_array)
        .and_then(|routes| routes.first())
        .and_then(Value::as_array)
        .map(Vec::len);
    let mut side = Side::ok(amount_out, latency, hops);
    side.platforms = route_names(&value["protocols"], "name");
    side
}

async fn quote_zerox(
    client: &reqwest::Client,
    key: &str,
    token_in: &str,
    token_out: &str,
    amount_in: &str,
) -> Side {
    let started = Instant::now();
    let response = client
        .get(ZEROX_URL)
        .query(&[
            ("sellToken", token_in),
            ("buyToken", token_out),
            ("sellAmount", amount_in),
            ("chainId", "1"),
        ])
        .header("0x-api-key", key)
        .header("0x-version", "v2")
        .send()
        .await;
    let (text, latency) = match response_text(response, started).await {
        Ok(result) => result,
        Err(side) => return side,
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Side::error(latency, format!("invalid JSON: {}", snippet(&text)));
    };
    let Some(amount_out) = value.get("buyAmount").and_then(Value::as_str) else {
        return Side::error(latency, format!("missing buyAmount: {}", snippet(&text)));
    };
    let Some(amount_out) = parse_amount(amount_out) else {
        return Side::error(latency, "buyAmount is not a decimal integer".to_string());
    };
    let hops = value
        .get("route")
        .and_then(|route| route.get("fills"))
        .and_then(Value::as_array)
        .map(Vec::len);
    let mut side = Side::ok(amount_out, latency, hops);
    side.platforms = route_names(&value["route"], "source");
    side
}

fn route_names(value: &Value, key: &str) -> Vec<String> {
    fn visit(value: &Value, key: &str, names: &mut Vec<String>) {
        match value {
            Value::Object(object) => {
                if let Some(name) = object.get(key).and_then(Value::as_str) {
                    names.push(name.to_string());
                }
                for child in object.values() {
                    visit(child, key, names);
                }
            }
            Value::Array(array) => {
                for child in array {
                    visit(child, key, names);
                }
            }
            _ => {}
        }
    }
    let mut names = Vec::new();
    visit(value, key, &mut names);
    names.sort();
    names.dedup();
    names
}

fn format_option(value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{value:.1}"),
        None => "-".to_string(),
    }
}

fn format_latency(value: Option<u128>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "-".to_string(),
    }
}

fn format_block(value: Option<u64>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "unknown".to_string(),
    }
}

fn format_side(side: Option<&Side>) -> String {
    match side {
        Some(side) => match (&side.amount_out, &side.error) {
            (Some(amount_out), _) => amount_out.clone(),
            (None, Some(error)) => format!("ERR: {error}"),
            (None, None) => "-".to_string(),
        },
        None => "skipped".to_string(),
    }
}

fn format_hops(side: Option<&Side>) -> String {
    side.and_then(|side| side.hops)
        .map(|hops| hops.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn render_markdown(meta: &Meta, rows: &[Row], summary: &Summary, corpus: &Corpus) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Fluxion quote benchmark\n");
    let _ = writeln!(out, "| field | value |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(out, "| timestamp | {} |", meta.timestamp_unix);
    let _ = writeln!(
        out,
        "| git | {} |",
        meta.git_sha.as_deref().unwrap_or("unknown")
    );
    let _ = writeln!(out, "| service | {} |", meta.service);
    let _ = writeln!(out, "| working tree dirty | {} |", meta.working_tree_dirty);
    let _ = writeln!(out, "| code fingerprint | {} |", meta.code_fingerprint);
    let _ = writeln!(out, "| start block | {} |", format_block(meta.start_block));
    let _ = writeln!(out, "| end block | {} |", format_block(meta.end_block));
    let _ = writeln!(out, "| paraswap | enabled (keyless) |");
    let _ = writeln!(
        out,
        "| 1inch | {} |",
        if meta.oneinch_enabled {
            "enabled"
        } else {
            "skipped (no ONEINCH_API_KEY)"
        }
    );
    let _ = writeln!(
        out,
        "| 0x | {} |",
        if meta.zerox_enabled {
            "enabled"
        } else {
            "skipped (no ZEROX_API_KEY)"
        }
    );
    let _ = writeln!(out, "| corpus | {} pairs x amounts |", meta.pair_count);
    let _ = writeln!(
        out,
        "\nService mode, graph size and effective public configuration:\n\n```json\n{}\n```",
        serde_json::to_string_pretty(&meta.service_info).unwrap()
    );

    let _ = writeln!(
        out,
        "\n| pair | amount_in | fluxion_out | fluxion_hops | paraswap_out | Δ paraswap bps | 1inch_out | Δ 1inch bps | 0x_out | Δ 0x bps | fluxion ms | paraswap ms | 1inch ms | 0x ms |"
    );
    let _ = writeln!(
        out,
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    for row in rows {
        let pair = format!(
            "{} → {}",
            symbol(corpus, &row.token_in),
            symbol(corpus, &row.token_out)
        );
        let paraswap_delta = delta_bps(&row.fluxion, &row.paraswap);
        let oneinch_delta = row
            .oneinch
            .as_ref()
            .and_then(|side| delta_bps(&row.fluxion, side));
        let zerox_delta = row
            .zerox
            .as_ref()
            .and_then(|side| delta_bps(&row.fluxion, side));
        let _ = writeln!(
            out,
            "| {pair} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            row.amount_in,
            format_side(Some(&row.fluxion)),
            format_hops(Some(&row.fluxion)),
            format_side(Some(&row.paraswap)),
            format_option(paraswap_delta),
            format_side(row.oneinch.as_ref()),
            format_option(oneinch_delta),
            format_side(row.zerox.as_ref()),
            format_option(zerox_delta),
            format_latency(Some(row.fluxion.latency_ms)),
            format_latency(Some(row.paraswap.latency_ms)),
            format_latency(row.oneinch.as_ref().map(|side| side.latency_ms)),
            format_latency(row.zerox.as_ref().map(|side| side.latency_ms)),
        );
    }

    let _ = writeln!(out, "\n## Summary\n");
    let _ = writeln!(out, "- rows: {}", summary.rows);
    for name in ["paraswap", "1inch", "0x"] {
        let counts = &summary.comparisons[name];
        let _ = writeln!(
            out,
            "- {name}: both success {}, Fluxion only {}, aggregator only {}, both failed {}",
            counts.both_ok, counts.fluxion_only, counts.aggregator_only, counts.both_failed
        );
    }
    let _ = writeln!(
        out,
        "- success: fluxion {}, paraswap {}, 1inch {}, 0x {}",
        summary.fluxion_ok, summary.paraswap_ok, summary.oneinch_ok, summary.zerox_ok
    );
    let _ = writeln!(
        out,
        "- paraswap deltas (n={}): median {} bps, mean {} bps, within ±50 bps {}",
        summary.paraswap_compared,
        format_option(summary.paraswap_median_delta_bps),
        format_option(summary.paraswap_mean_delta_bps),
        summary.paraswap_within_50_bps
    );
    let _ = writeln!(
        out,
        "- 1inch deltas (n={}): median {} bps, mean {} bps, within ±50 bps {}",
        summary.oneinch_compared,
        format_option(summary.oneinch_median_delta_bps),
        format_option(summary.oneinch_mean_delta_bps),
        summary.oneinch_within_50_bps
    );
    let _ = writeln!(
        out,
        "- 0x deltas (n={}): median {} bps, mean {} bps, within ±50 bps {}",
        summary.zerox_compared,
        format_option(summary.zerox_median_delta_bps),
        format_option(summary.zerox_mean_delta_bps),
        summary.zerox_within_50_bps
    );
    let _ = writeln!(
        out,
        "- latency p50/p95 (ms): fluxion {}/{}, paraswap {}/{}, 1inch {}/{}, 0x {}/{}",
        format_latency(summary.fluxion_latency_p50_ms),
        format_latency(summary.fluxion_latency_p95_ms),
        format_latency(summary.paraswap_latency_p50_ms),
        format_latency(summary.paraswap_latency_p95_ms),
        format_latency(summary.oneinch_latency_p50_ms),
        format_latency(summary.oneinch_latency_p95_ms),
        format_latency(summary.zerox_latency_p50_ms),
        format_latency(summary.zerox_latency_p95_ms)
    );
    let _ = writeln!(
        out,
        "\nΔ bps = (fluxion - aggregator) / aggregator × 10000; negative means Fluxion returned less.\n\nQuotes use live state, not a pinned block. Block range and row timestamps describe skew, not deterministic replay. Raw JSON embeds the corpus and public service configuration. Latencies include failed requests; output deltas only use successful pairs. Gas, MEV and execution are excluded; ParaSwap may use more venues and split routes. Amount tiers are fixed raw inputs, not current USD valuations."
    );
    out
}

async fn response_text(
    response: Result<reqwest::Response, reqwest::Error>,
    started: Instant,
) -> Result<(String, u128), Side> {
    let response = response
        .map_err(|e| Side::error(started.elapsed().as_millis(), e.without_url().to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| Side::error(started.elapsed().as_millis(), e.without_url().to_string()))?;
    let elapsed = started.elapsed().as_millis();
    if !status.is_success() {
        return Err(Side::error(
            elapsed,
            format!("HTTP {status}: {}", snippet(&text)),
        ));
    }
    Ok((text, elapsed))
}

fn validate_corpus(corpus: &Corpus) -> Result<(), String> {
    if corpus.pairs.is_empty() {
        return Err("corpus has no pairs".into());
    }
    for [token_in, token_out] in &corpus.pairs {
        for token in [token_in, token_out] {
            token
                .parse::<Address>()
                .map_err(|_| format!("invalid corpus address: {token}"))?;
            if !corpus.decimals.contains_key(&token.to_lowercase()) {
                return Err(format!("missing decimals for {token}"));
            }
        }
        let amounts = corpus
            .amounts
            .get(&token_in.to_lowercase())
            .ok_or_else(|| format!("missing amounts for {token_in}"))?;
        if amounts.is_empty() {
            return Err(format!("empty amount list for {token_in}"));
        }
        for amount in amounts {
            let parsed =
                parse_amount(amount).ok_or_else(|| format!("invalid input amount: {amount}"))?;
            if parsed == "0" {
                return Err("zero input amount".into());
            }
        }
    }
    Ok(())
}

/// Hash actual source contents, including untracked binaries, rather than only HEAD.
fn code_fingerprint() -> Result<String, Box<dyn std::error::Error>> {
    use std::io::Write;
    fn visit(path: &Path, files: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                visit(&path, files)?;
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
        Ok(())
    }
    let mut files = vec!["Cargo.toml".into(), "Cargo.lock".into()];
    visit(Path::new("src"), &mut files)?;
    files.sort();
    let mut bytes = Vec::new();
    for path in files {
        bytes.extend_from_slice(path.to_string_lossy().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&std::fs::read(path)?);
        bytes.push(0);
    }
    let mut child = std::process::Command::new("git")
        .args(["hash-object", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(&bytes)?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err("cannot fingerprint source".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn git_sha() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let args = parse_args();

    let text = std::fs::read_to_string(&args.corpus)?;
    let corpus: Corpus = serde_yaml::from_str(&text)?;
    validate_corpus(&corpus).map_err(std::io::Error::other)?;

    let mut pairs = corpus.pairs.clone();
    if let Some(limit) = args.limit {
        pairs.truncate(limit);
    }

    let oneinch_key = env_nonempty("ONEINCH_API_KEY");
    let zerox_key = env_nonempty("ZEROX_API_KEY");
    if oneinch_key.is_none() {
        eprintln!("ONEINCH_API_KEY not set — 1inch column will be skipped");
    }
    if zerox_key.is_none() {
        eprintln!("ZEROX_API_KEY not set — 0x column will be skipped");
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let service_info: Value = client
        .get(format!("{}/info", args.service.trim_end_matches('/')))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if service_info["mode"] != "mainnet" {
        return Err(
            "refusing to benchmark a mock service; start Fluxion with real discovery".into(),
        );
    }

    let config = Config::load(&args.config)?;
    let rpc = RpcClient::new(config.rpc_url.clone());
    let start_block = rpc.block_number().await.ok();

    eprintln!(
        "benchmarking {} pairs from {} (1inch={}, 0x={})",
        pairs.len(),
        args.corpus,
        oneinch_key.is_some(),
        zerox_key.is_some()
    );

    let mut rows = Vec::new();
    for [token_in, token_out] in &pairs {
        let Some(amounts) = corpus.amounts.get(&token_in.to_lowercase()) else {
            eprintln!("skip {token_in}: no amounts entry");
            continue;
        };
        for amount_in in amounts {
            let started_unix_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
            let label = format!(
                "{}->{} {}",
                symbol(&corpus, token_in),
                symbol(&corpus, token_out),
                amount_in
            );
            let fluxion =
                quote_fluxion(&client, &args.service, token_in, token_out, amount_in).await;

            let paraswap = quote_paraswap(
                &client,
                token_in,
                token_out,
                amount_in,
                decimals(&corpus, token_in),
                decimals(&corpus, token_out),
            )
            .await;
            if args.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(args.delay_ms)).await;
            }

            let oneinch = match &oneinch_key {
                Some(key) => {
                    let side = quote_oneinch(&client, key, token_in, token_out, amount_in).await;
                    if args.delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(args.delay_ms)).await;
                    }
                    Some(side)
                }
                None => None,
            };

            let zerox = match &zerox_key {
                Some(key) => {
                    let side = quote_zerox(&client, key, token_in, token_out, amount_in).await;
                    if args.delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(args.delay_ms)).await;
                    }
                    Some(side)
                }
                None => None,
            };

            eprintln!(
                "{label} fluxion={} paraswap={} oneinch={} zerox={}",
                format_side(Some(&fluxion)),
                format_side(Some(&paraswap)),
                format_side(oneinch.as_ref()),
                format_side(zerox.as_ref())
            );
            rows.push(Row {
                started_unix_ms,
                finished_unix_ms: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
                token_in: token_in.clone(),
                token_out: token_out.clone(),
                amount_in: amount_in.clone(),
                fluxion,
                paraswap,
                oneinch,
                zerox,
            });
        }
    }

    let end_block = rpc.block_number().await.ok();
    let timestamp_unix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let meta = Meta {
        timestamp_unix,
        git_sha: git_sha(),
        service: args.service.clone(),
        corpus: args.corpus.clone(),
        start_block,
        end_block,
        paraswap_enabled: true,
        oneinch_enabled: oneinch_key.is_some(),
        zerox_enabled: zerox_key.is_some(),
        working_tree_dirty: std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(true),
        service_info,
        pair_count: pairs.len(),
        delay_ms: args.delay_ms,
        code_fingerprint: code_fingerprint()?,
        corpus_snapshot: corpus,
    };
    let summary = summarize(&rows);

    let block = start_block
        .map(|block| block.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let out_path = args
        .out
        .unwrap_or_else(|| format!("docs/benchmarks/bench-{timestamp_unix}-{block}.md"));
    let path = Path::new(&out_path);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }

    let markdown = render_markdown(&meta, &rows, &summary, &meta.corpus_snapshot);
    std::fs::write(path, markdown)?;
    let json_path = path.with_extension("json");
    let report = Report {
        meta: &meta,
        summary: &summary,
        rows: &rows,
    };
    std::fs::write(&json_path, serde_json::to_string_pretty(&report)?)?;

    println!("wrote {} and {}", path.display(), json_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_yaml_parses() {
        let corpus: Corpus = serde_yaml::from_str(
            "symbols:\n  \"0xabc\": WETH\namounts:\n  \"0xabc\": [\"1\"]\npairs:\n  - [\"0xabc\", \"0xdef\"]\n",
        )
        .unwrap();
        assert_eq!(corpus.pairs.len(), 1);
        assert_eq!(symbol(&corpus, "0xABC"), "WETH");
    }

    #[test]
    fn delta_is_signed_relative_to_aggregator() {
        let fluxion = Side::ok("100".to_string(), 1, None);
        let other = Side::ok("110".to_string(), 1, None);
        let delta = delta_bps(&fluxion, &other).unwrap();
        assert!((delta + 909.0909).abs() < 0.01);
    }

    #[test]
    fn median_handles_odd_and_even_lengths() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(median(&mut []), None);
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        let mut values = vec![40, 10, 30, 20];
        assert_eq!(percentile(&mut values, 0.50), Some(20));
        assert_eq!(percentile(&mut values, 0.95), Some(40));
    }

    #[test]
    fn comparison_keeps_failures_out_of_output_deltas() {
        let ok = || Side::ok("100".into(), 10, Some(1));
        let failed = || Side::error(30, "unavailable".into());
        let rows = [
            (ok(), ok()),
            (ok(), failed()),
            (failed(), ok()),
            (failed(), failed()),
        ]
        .into_iter()
        .map(|(fluxion, paraswap)| Row {
            started_unix_ms: 0,
            finished_unix_ms: 1,
            token_in: "a".into(),
            token_out: "b".into(),
            amount_in: "1".into(),
            fluxion,
            paraswap,
            oneinch: None,
            zerox: None,
        })
        .collect::<Vec<_>>();
        let summary = summarize(&rows);
        let counts = &summary.comparisons["paraswap"];
        assert_eq!(
            (
                counts.both_ok,
                counts.fluxion_only,
                counts.aggregator_only,
                counts.both_failed
            ),
            (1, 1, 1, 1)
        );
        assert_eq!(summary.paraswap_compared, 1);
        assert_eq!(summary.paraswap_median_delta_bps, Some(0.0));
        assert_eq!(summary.oneinch_latency_p50_ms, None);
    }

    #[test]
    fn shipped_corpus_is_valid_and_missing_decimals_are_rejected() {
        let mut corpus: Corpus =
            serde_yaml::from_str(include_str!("../../bench/corpus.yaml")).unwrap();
        validate_corpus(&corpus).unwrap();
        corpus.decimals.clear();
        assert!(validate_corpus(&corpus).unwrap_err().contains("decimals"));
        assert!(snippet(&"中".repeat(200)).ends_with('…'));
    }

    #[tokio::test]
    async fn latency_includes_receiving_the_response_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().route(
            "/quote",
            axum::routing::post(|| async {
                axum::body::Body::from_stream(futures::stream::once(async {
                    tokio::time::sleep(Duration::from_millis(60)).await;
                    Ok::<_, std::io::Error>(
                        r#"{"amount_out":"123","path":[{"platform":"uniswap-v2"}]}"#,
                    )
                }))
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let side = quote_fluxion(&reqwest::Client::new(), &url, "a", "b", "1").await;
        server.abort();
        assert_eq!(side.amount_out.as_deref(), Some("123"));
        assert_eq!(side.platforms, ["uniswap-v2"]);
        assert!(side.latency_ms >= 60);
    }
}
