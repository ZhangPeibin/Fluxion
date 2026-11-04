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
use fluxion::rpc::{BlockHeader, RpcClient};

const ONEINCH_URL: &str = "https://api.1inch.com/swap/v6.0/1/quote";
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
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
    amount_out: Option<String>,
    latency_ms: u128,
    hops: Option<usize>,
    error: Option<String>,
    platforms: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    route: Option<Box<Value>>,
}

impl Side {
    fn ok(amount_out: String, latency_ms: u128, hops: Option<usize>) -> Self {
        Self {
            request_id: None,
            amount_out: Some(amount_out),
            latency_ms,
            hops,
            error: None,
            platforms: Vec::new(),
            route: None,
        }
    }

    fn error(latency_ms: u128, error: String) -> Self {
        Self {
            request_id: None,
            amount_out: None,
            latency_ms,
            hops: None,
            error: Some(error),
            platforms: Vec::new(),
            route: None,
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
    freshness: QuoteFreshness,
    control: Option<Side>,
    paraswap: Side,
    oneinch: Option<Side>,
    zerox: Option<Side>,
}

#[derive(Debug, Clone, Serialize)]
struct HeaderSample {
    number: u64,
    hash: String,
    timestamp: u64,
}

impl From<BlockHeader> for HeaderSample {
    fn from(header: BlockHeader) -> Self {
        Self {
            number: header.number,
            hash: header.hash.to_string(),
            timestamp: header.timestamp,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct QuoteFreshness {
    quote_block: Option<u64>,
    quote_block_timestamp: Option<u64>,
    head_before: Option<HeaderSample>,
    head_after: Option<HeaderSample>,
    lag_blocks_before: Option<i64>,
    lag_blocks_after: Option<i64>,
    lag_secs_before: Option<i64>,
    lag_secs_after: Option<i64>,
    unavailable_reason: Option<String>,
}

impl QuoteFreshness {
    #[cfg(test)]
    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            quote_block: None,
            quote_block_timestamp: None,
            head_before: None,
            head_after: None,
            lag_blocks_before: None,
            lag_blocks_after: None,
            lag_secs_before: None,
            lag_secs_after: None,
            unavailable_reason: Some(reason.into()),
        }
    }

    fn from_quote(
        side: &Side,
        before: Result<BlockHeader, String>,
        after: Result<BlockHeader, String>,
    ) -> Self {
        let before_error = before.as_ref().err().cloned();
        let after_error = after.as_ref().err().cloned();
        let head_before = before.ok().map(HeaderSample::from);
        let head_after = after.ok().map(HeaderSample::from);
        let quote = side.route.as_deref();
        let quote_block = quote
            .and_then(|value| value.get("quote_block"))
            .and_then(Value::as_u64);
        let quote_block_timestamp = quote
            .and_then(|value| value.get("state"))
            .and_then(|state| state.get("block_timestamp"))
            .and_then(Value::as_u64);
        let local_snapshot = quote
            .and_then(|value| value.get("state"))
            .and_then(|state| state.get("pricing_mode"))
            .and_then(Value::as_str)
            == Some("local_snapshot");
        let signed_diff = |head: u64, quote: u64| -> i64 {
            if head >= quote {
                i64::try_from(head - quote).unwrap_or(i64::MAX)
            } else {
                -i64::try_from(quote - head).unwrap_or(i64::MAX)
            }
        };
        let block_lag =
            |head: &Option<HeaderSample>| Some(signed_diff(head.as_ref()?.number, quote_block?));
        let time_lag = |head: &Option<HeaderSample>| {
            Some(signed_diff(
                head.as_ref()?.timestamp,
                quote_block_timestamp?,
            ))
        };
        let mut reasons = Vec::new();
        if quote_block.is_none() {
            reasons.push("quote response missing quote_block".to_string());
        }
        if quote_block_timestamp.is_none() {
            reasons.push("quote response missing state.block_timestamp".to_string());
        }
        if !local_snapshot {
            reasons.push("quote response is not marked local_snapshot".to_string());
        }
        if head_before.is_none() {
            reasons.push(format!(
                "pre-quote latest-head RPC failed: {}",
                before_error.as_deref().unwrap_or("unknown error")
            ));
        }
        if head_after.is_none() {
            reasons.push(format!(
                "post-quote latest-head RPC failed: {}",
                after_error.as_deref().unwrap_or("unknown error")
            ));
        }
        Self {
            quote_block,
            quote_block_timestamp,
            lag_blocks_before: block_lag(&head_before),
            lag_blocks_after: block_lag(&head_after),
            lag_secs_before: time_lag(&head_before),
            lag_secs_after: time_lag(&head_after),
            head_before,
            head_after,
            unavailable_reason: (!reasons.is_empty()).then(|| reasons.join("; ")),
        }
    }

    fn measured(&self) -> bool {
        self.unavailable_reason.is_none()
            && self.lag_blocks_before.is_some()
            && self.lag_blocks_after.is_some()
            && self.lag_secs_before.is_some()
            && self.lag_secs_after.is_some()
    }
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
    freshness_measured: usize,
    freshness_unavailable: usize,
    lag_blocks_p50: Option<i64>,
    lag_blocks_p95: Option<i64>,
    lag_blocks_max: Option<i64>,
    lag_secs_p50: Option<i64>,
    lag_secs_p95: Option<i64>,
    lag_secs_max: Option<i64>,
    comparisons: HashMap<String, Comparison>,
    cohorts: HashMap<String, Cohort>,
}

#[derive(Debug, Serialize)]
struct Quality {
    success: usize,
    compared: usize,
    median_delta_bps: Option<f64>,
    mean_delta_bps: Option<f64>,
    within_50_bps: usize,
    latency_p50_ms: Option<u128>,
    latency_p95_ms: Option<u128>,
}

#[derive(Debug, Serialize)]
struct Cohort {
    rows: usize,
    candidate: Quality,
    control: Quality,
    candidate_better: usize,
    equal: usize,
    candidate_worse: usize,
}

fn quality(rows: &[&Row], control: bool) -> Quality {
    let sides = rows
        .iter()
        .filter_map(|row| {
            if control {
                row.control.as_ref().map(|side| (side, &row.paraswap))
            } else {
                Some((&row.fluxion, &row.paraswap))
            }
        })
        .collect::<Vec<_>>();
    let mut deltas = sides
        .iter()
        .filter_map(|(side, external)| delta_bps(side, external))
        .collect::<Vec<_>>();
    let mut latencies = sides
        .iter()
        .map(|(side, _)| side.latency_ms)
        .collect::<Vec<_>>();
    Quality {
        success: sides.iter().filter(|(side, _)| side.is_ok()).count(),
        compared: deltas.len(),
        mean_delta_bps: (!deltas.is_empty())
            .then(|| deltas.iter().sum::<f64>() / deltas.len() as f64),
        within_50_bps: deltas.iter().filter(|delta| delta.abs() <= 50.0).count(),
        median_delta_bps: median(&mut deltas),
        latency_p50_ms: percentile(&mut latencies, 0.5),
        latency_p95_ms: percentile(&mut latencies, 0.95),
    }
}

fn cohorts(rows: &[Row]) -> HashMap<String, Cohort> {
    let steth = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84";
    let shib = "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce";
    let touches = |row: &Row, token: &str| {
        row.token_in.eq_ignore_ascii_case(token) || row.token_out.eq_ignore_ascii_case(token)
    };
    ["all", "steth", "shib", "other"]
        .into_iter()
        .map(|name| {
            let selected = rows
                .iter()
                .filter(|row| match name {
                    "steth" => touches(row, steth),
                    "shib" => touches(row, shib),
                    "other" => !touches(row, steth) && !touches(row, shib),
                    _ => true,
                })
                .collect::<Vec<_>>();
            let mut better = 0;
            let mut equal = 0;
            let mut worse = 0;
            for row in &selected {
                if let (Some(new), Some(old)) = (
                    row.fluxion.amount_out.as_deref(),
                    row.control
                        .as_ref()
                        .and_then(|side| side.amount_out.as_deref()),
                ) {
                    match U256::from_str_radix(new, 10)
                        .unwrap()
                        .cmp(&U256::from_str_radix(old, 10).unwrap())
                    {
                        std::cmp::Ordering::Greater => better += 1,
                        std::cmp::Ordering::Equal => equal += 1,
                        std::cmp::Ordering::Less => worse += 1,
                    }
                }
            }
            (
                name.to_string(),
                Cohort {
                    rows: selected.len(),
                    candidate: quality(&selected, false),
                    control: quality(&selected, true),
                    candidate_better: better,
                    equal,
                    candidate_worse: worse,
                },
            )
        })
        .collect()
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
    fluxion_split: bool,
    zerox_enabled: bool,
    working_tree_dirty: bool,
    service_info: Value,
    pricing_mode: Option<String>,
    freshness_rpc_host: Option<String>,
    corpus_snapshot: Corpus,
    pair_count: usize,
    delay_ms: u64,
    code_fingerprint: String,
    control_service: Option<String>,
    control_info: Option<Value>,
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
    freshness_rpc_url: Option<String>,
    control_service: Option<String>,
    split: bool,
}

fn print_help() {
    println!(
        "Usage: cargo run --bin bench -- [options]\n\
         \n\
         Options:\n\
            --service URL    Fluxion base URL (default http://127.0.0.1:8080)\n\
            --control-service URL  optional feature-disabled control, quoted in the same run\n\
           --corpus PATH    corpus YAML (default bench/corpus.yaml)\n\
           --out PATH       Markdown output path (also writes PATH.json)\n\
            --limit N        only benchmark the first N pairs\n\
            --split          enable Fluxion multi-pool routing\n\
            --delay-ms MS    pause after each external request (default 250)\n\
            --config PATH    local RPC config (default config.yaml)\n\
            --freshness-rpc-url URL  separate Ethereum RPC for head observations\n\
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
        freshness_rpc_url: None,
        control_service: None,
        split: false,
    };

    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--service" => args.service = it.next().expect("--service needs a URL"),
            "--control-service" => {
                args.control_service = Some(it.next().expect("--control-service needs a URL"))
            }
            "--corpus" => args.corpus = it.next().expect("--corpus needs a path"),
            "--out" => args.out = Some(it.next().expect("--out needs a path")),
            "--config" => args.config = it.next().expect("--config needs a path"),
            "--freshness-rpc-url" => {
                args.freshness_rpc_url = Some(it.next().expect("--freshness-rpc-url needs a URL"))
            }
            "--split" => args.split = true,
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
    let mut lag_blocks = rows
        .iter()
        .filter(|row| row.freshness.measured())
        .filter_map(|row| row.freshness.lag_blocks_after)
        .collect::<Vec<_>>();
    let mut lag_secs = rows
        .iter()
        .filter(|row| row.freshness.measured())
        .filter_map(|row| row.freshness.lag_secs_after)
        .collect::<Vec<_>>();

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
        freshness_measured: rows.iter().filter(|row| row.freshness.measured()).count(),
        freshness_unavailable: rows.iter().filter(|row| !row.freshness.measured()).count(),
        lag_blocks_p50: percentile_signed(&mut lag_blocks, 0.50),
        lag_blocks_p95: percentile_signed(&mut lag_blocks, 0.95),
        lag_blocks_max: lag_blocks.iter().copied().max(),
        lag_secs_p50: percentile_signed(&mut lag_secs, 0.50),
        lag_secs_p95: percentile_signed(&mut lag_secs, 0.95),
        lag_secs_max: lag_secs.iter().copied().max(),
        comparisons: HashMap::from([
            (
                "paraswap".into(),
                comparison(rows, |row| Some(&row.paraswap)),
            ),
            ("1inch".into(), comparison(rows, |row| row.oneinch.as_ref())),
            ("0x".into(), comparison(rows, |row| row.zerox.as_ref())),
        ]),
        cohorts: cohorts(rows),
    }
}

fn percentile_signed(values: &mut [i64], fraction: f64) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let rank = (fraction * values.len() as f64).ceil() as usize;
    Some(values[rank.saturating_sub(1).min(values.len() - 1)])
}

fn format_signed(value: Option<i64>) -> String {
    value.map_or_else(|| "-".to_string(), |n| n.to_string())
}

async fn quote_fluxion(
    client: &reqwest::Client,
    service: &str,
    token_in: &str,
    token_out: &str,
    amount_in: &str,
    split: bool,
) -> Side {
    let url = format!("{}/quote", service.trim_end_matches('/'));
    let request_id = format!(
        "bench-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let started = Instant::now();
    let response = client
        .post(&url)
        .header("x-diag-id", &request_id)
        .json(&json!({
            "token_in": token_in,
            "token_out": token_out,
            "amount_in": amount_in,
            "split": split,
        }))
        .send()
        .await;
    let (text, latency) = match response_text(response, started).await {
        Ok(result) => result,
        Err(side) => return *side,
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
    let hops = if split {
        value["allocations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|allocation| allocation["path"].as_array().map(Vec::len))
            .max()
    } else {
        value.get("path").and_then(Value::as_array).map(Vec::len)
    };
    let mut side = Side::ok(amount_out, latency, hops);
    if let Some(path) = value.get("path").and_then(Value::as_array) {
        side.route = Some(Box::new(Value::Array(path.clone())));
        side.platforms = path
            .iter()
            .filter_map(|hop| hop.get("platform")?.as_str().map(str::to_string))
            .collect();
    }
    if split {
        side.platforms = value["allocations"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|allocation| allocation["path"].as_array().into_iter().flatten())
            .filter_map(|hop| hop["platform"].as_str().map(str::to_owned))
            .collect();
    }
    // Keep block/state, allocations and search metrics alongside the exact output.
    side.route = Some(Box::new(value));
    side.request_id = Some(request_id);
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
        Err(side) => return *side,
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
    side.route = route.get("bestRoute").cloned().map(Box::new);
    side.platforms = paraswap_platforms(&route["bestRoute"]);
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
        .query(&[
            ("src", token_in),
            ("dst", token_out),
            ("amount", amount_in),
            ("includeProtocols", "true"),
        ])
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await;
    let (text, latency) = match response_text(response, started).await {
        Ok(result) => result,
        Err(side) => return *side,
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
    side.route = Some(Box::new(value));
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
        Err(side) => return *side,
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

fn paraswap_platforms(value: &Value) -> Vec<String> {
    fn visit(value: &Value, names: &mut Vec<String>) {
        match value {
            Value::Object(object) => {
                if let Some(exchanges) = object.get("swapExchanges").and_then(Value::as_array) {
                    names.extend(
                        exchanges.iter().filter_map(|exchange| {
                            exchange["exchange"].as_str().map(str::to_string)
                        }),
                    );
                }
                for child in object.values() {
                    visit(child, names);
                }
            }
            Value::Array(values) => {
                for child in values {
                    visit(child, names);
                }
            }
            _ => {}
        }
    }
    let mut names = Vec::new();
    visit(value, &mut names);
    names.sort();
    names.dedup();
    names
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
    let _ = writeln!(
        out,
        "| pricing mode | {} |",
        meta.pricing_mode.as_deref().unwrap_or("unknown")
    );
    let _ = writeln!(
        out,
        "| freshness RPC host | {} |",
        meta.freshness_rpc_host.as_deref().unwrap_or("unknown")
    );
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
    if let Some(info) = &meta.control_info {
        let _ = writeln!(
            out,
            "\nFeature-disabled control: {}\n\n```json\n{}\n```",
            meta.control_service.as_deref().unwrap_or("unknown"),
            serde_json::to_string_pretty(info).unwrap()
        );
    }

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

    let _ = writeln!(out, "\n## Per-quote snapshot freshness\n");
    let _ = writeln!(
        out,
        "| pair | quote block | head before | head after | lag blocks before..after | lag seconds before..after | measurement |\n|---|---:|---:|---:|---:|---:|---|"
    );
    for row in rows {
        let pair = format!(
            "{} → {}",
            symbol(corpus, &row.token_in),
            symbol(corpus, &row.token_out)
        );
        let freshness = &row.freshness;
        let header_number = |header: &Option<HeaderSample>| {
            header
                .as_ref()
                .map_or_else(|| "-".to_string(), |h| h.number.to_string())
        };
        let interval = |before: Option<i64>, after: Option<i64>| match (before, after) {
            (Some(before), Some(after)) => format!("{before}..{after}"),
            _ => "-".to_string(),
        };
        let _ = writeln!(
            out,
            "| {pair} | {} | {} | {} | {} | {} | {} |",
            freshness
                .quote_block
                .map_or_else(|| "-".to_string(), |n| n.to_string()),
            header_number(&freshness.head_before),
            header_number(&freshness.head_after),
            interval(freshness.lag_blocks_before, freshness.lag_blocks_after),
            interval(freshness.lag_secs_before, freshness.lag_secs_after),
            freshness
                .unavailable_reason
                .as_deref()
                .unwrap_or("measured"),
        );
    }

    let _ = writeln!(out, "\n## Summary\n");
    let _ = writeln!(out, "| timing | p50 | p95 | max |\n|---|---:|---:|---:|");
    let _ = writeln!(
        out,
        "| snapshot lag blocks | {} | {} | {} |",
        format_signed(summary.lag_blocks_p50),
        format_signed(summary.lag_blocks_p95),
        format_signed(summary.lag_blocks_max)
    );
    let _ = writeln!(
        out,
        "| snapshot lag seconds | {} | {} | {} |",
        format_signed(summary.lag_secs_p50),
        format_signed(summary.lag_secs_p95),
        format_signed(summary.lag_secs_max)
    );
    for (name, p50, p95) in [
        (
            "fluxion latency ms",
            summary.fluxion_latency_p50_ms,
            summary.fluxion_latency_p95_ms,
        ),
        (
            "paraswap latency ms",
            summary.paraswap_latency_p50_ms,
            summary.paraswap_latency_p95_ms,
        ),
        (
            "1inch latency ms",
            summary.oneinch_latency_p50_ms,
            summary.oneinch_latency_p95_ms,
        ),
        (
            "0x latency ms",
            summary.zerox_latency_p50_ms,
            summary.zerox_latency_p95_ms,
        ),
    ] {
        let _ = writeln!(
            out,
            "| {name} | {} | {} | - |",
            format_latency(p50),
            format_latency(p95)
        );
    }
    let _ = writeln!(out);
    if meta.control_service.is_some() {
        let _ = writeln!(
            out,
            "| cohort | rows | control median bps | candidate median bps | control mean bps | candidate mean bps | control within ±50 bps | candidate within ±50 bps | better/equal/worse vs control |\n|---|---|---|---|---|---|---|---|---|"
        );
        for name in ["all", "steth", "shib", "other"] {
            let c = &summary.cohorts[name];
            let _ = writeln!(
                out,
                "| {name} | {} | {} | {} | {} | {} | {} | {} | {}/{}/{} |",
                c.rows,
                format_option(c.control.median_delta_bps),
                format_option(c.candidate.median_delta_bps),
                format_option(c.control.mean_delta_bps),
                format_option(c.candidate.mean_delta_bps),
                c.control.within_50_bps,
                c.candidate.within_50_bps,
                c.candidate_better,
                c.equal,
                c.candidate_worse
            );
        }
        let _ = writeln!(
            out,
            "\n## Paired control quotes\n\n| pair | amount_in | control output | candidate output | candidate vs control bps | control ms | candidate ms |\n|---|---|---|---|---|---|---|"
        );
        for row in rows {
            if let Some(control) = &row.control {
                let _ = writeln!(
                    out,
                    "| {} → {} | {} | {} | {} | {} | {} | {} |",
                    symbol(corpus, &row.token_in),
                    symbol(corpus, &row.token_out),
                    row.amount_in,
                    format_side(Some(control)),
                    format_side(Some(&row.fluxion)),
                    format_option(delta_bps(&row.fluxion, control)),
                    control.latency_ms,
                    row.fluxion.latency_ms
                );
            }
        }
    }
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
        "- freshness: measured {}, unavailable {}; lag blocks p50/p95/max {}/{}/{}; lag seconds p50/p95/max {}/{}/{}",
        summary.freshness_measured,
        summary.freshness_unavailable,
        format_signed(summary.lag_blocks_p50),
        format_signed(summary.lag_blocks_p95),
        format_signed(summary.lag_blocks_max),
        format_signed(summary.lag_secs_p50),
        format_signed(summary.lag_secs_p95),
        format_signed(summary.lag_secs_max),
    );
    let _ = writeln!(
        out,
        "\nFreshness lag is bracketed by latest-chain headers read immediately before and after each Fluxion quote; the after value is the conservative observed bound, not an atomic chain-head sample. Seconds use Ethereum block timestamps. Negative values indicate the quote block/time is ahead of the observed head and should be treated as provider skew. Missing quote metadata or RPC reads are excluded from freshness percentiles and shown per row. This measures snapshot age, not amount_out deviation from a quote recomputed on latest state.\n\nΔ bps = (fluxion - aggregator) / aggregator × 10000; negative means Fluxion returned less. Quotes use live state, not a pinned block. Raw JSON embeds the corpus, routes, and public service configuration. With --control-service, candidate/control order alternates by row; RPC state and discovery may still differ. A paired live control reduces timing differences but is not a pinned-state experiment. Latencies include failed requests; output deltas only use successful pairs. Gas, MEV and execution are excluded; ParaSwap may use more venues and split routes. Amount tiers are fixed raw inputs, not current USD valuations."
    );
    out
}

async fn response_text(
    response: Result<reqwest::Response, reqwest::Error>,
    started: Instant,
) -> Result<(String, u128), Box<Side>> {
    let response = response.map_err(|e| {
        Box::new(Side::error(
            started.elapsed().as_millis(),
            e.without_url().to_string(),
        ))
    })?;
    let status = response.status();
    let text = response.text().await.map_err(|e| {
        Box::new(Side::error(
            started.elapsed().as_millis(),
            e.without_url().to_string(),
        ))
    })?;
    let elapsed = started.elapsed().as_millis();
    if !status.is_success() {
        return Err(Box::new(Side::error(
            elapsed,
            format!("HTTP {status}: {}", snippet(&text)),
        )));
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

    let state_info: Value = client
        .get(format!("{}/state", args.service.trim_end_matches('/')))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let pricing_mode = state_info
        .get("pricing_mode")
        .and_then(Value::as_str)
        .map(str::to_owned);

    let control_info = if let Some(service) = &args.control_service {
        let info: Value = client
            .get(format!("{}/info", service.trim_end_matches('/')))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if info["mode"] != "mainnet" {
            return Err("control service must use real mainnet discovery".into());
        }
        Some(info)
    } else {
        None
    };

    let config = Config::load(&args.config)?;
    let freshness_rpc_url = args
        .freshness_rpc_url
        .clone()
        .unwrap_or_else(|| config.rpc_url.clone());
    let freshness_rpc_host = reqwest::Url::parse(&freshness_rpc_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned));
    let rpc = RpcClient::new(freshness_rpc_url);
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
            let control_first = rows.len() % 2 == 0;
            let mut control = None;
            if control_first && let Some(service) = &args.control_service {
                control = Some(
                    quote_fluxion(&client, service, token_in, token_out, amount_in, args.split)
                        .await,
                );
            }
            let head_before = rpc.block_header(None).await.map_err(|e| e.to_string());
            let fluxion = quote_fluxion(
                &client,
                &args.service,
                token_in,
                token_out,
                amount_in,
                args.split,
            )
            .await;
            let head_after = rpc.block_header(None).await.map_err(|e| e.to_string());
            let freshness = QuoteFreshness::from_quote(&fluxion, head_before, head_after);
            if !control_first && let Some(service) = &args.control_service {
                control = Some(
                    quote_fluxion(&client, service, token_in, token_out, amount_in, args.split)
                        .await,
                );
            }

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
                freshness,
                control,
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
        fluxion_split: args.split,
        zerox_enabled: zerox_key.is_some(),
        working_tree_dirty: std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(true),
        service_info,
        pricing_mode,
        freshness_rpc_host,
        pair_count: pairs.len(),
        delay_ms: args.delay_ms,
        code_fingerprint: code_fingerprint()?,
        corpus_snapshot: corpus,
        control_service: args.control_service,
        control_info,
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
    fn freshness_brackets_quote_with_chain_heads_and_reports_timestamp_lag() {
        let side = Side {
            route: Some(Box::new(json!({
                "quote_block": 100,
                "state": {"pricing_mode":"local_snapshot", "block_timestamp": 1_000}
            }))),
            ..Side::ok("1".into(), 5, Some(1))
        };
        let header = |number, timestamp| BlockHeader {
            number,
            hash: alloy_primitives::B256::ZERO,
            parent_hash: alloy_primitives::B256::ZERO,
            timestamp,
        };
        let freshness =
            QuoteFreshness::from_quote(&side, Ok(header(101, 1_012)), Ok(header(102, 1_024)));

        assert!(freshness.measured());
        assert_eq!(freshness.lag_blocks_before, Some(1));
        assert_eq!(freshness.lag_blocks_after, Some(2));
        assert_eq!(freshness.lag_secs_before, Some(12));
        assert_eq!(freshness.lag_secs_after, Some(24));
    }

    #[test]
    fn freshness_marks_missing_metadata_and_rpc_failures_unavailable() {
        let side = Side::ok("1".into(), 5, Some(1));
        let freshness =
            QuoteFreshness::from_quote(&side, Err("offline".into()), Err("offline".into()));

        assert!(!freshness.measured());
        let reason = freshness.unavailable_reason.unwrap();
        assert!(reason.contains("missing quote_block"));
        assert!(reason.contains("offline"));
    }

    #[test]
    fn signed_percentiles_keep_negative_provider_skew_visible() {
        let mut values = vec![4, -2, 1, 8];
        assert_eq!(percentile_signed(&mut values, 0.50), Some(1));
        assert_eq!(percentile_signed(&mut [], 0.95), None);
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
            freshness: QuoteFreshness::unavailable("fixture"),
            control: None,
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
    fn paired_control_cohorts_measure_target_tokens_separately() {
        let rows = [
            "0xae7ab96520de3a18e5e111b5eaab095312d7fe84",
            "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce",
            "other",
        ]
        .into_iter()
        .map(|token| Row {
            started_unix_ms: 0,
            finished_unix_ms: 1,
            token_in: token.into(),
            token_out: "destination".into(),
            amount_in: "1".into(),
            fluxion: Side::ok("100".into(), 10, Some(2)),
            freshness: QuoteFreshness::unavailable("fixture"),
            control: Some(Side::ok("50".into(), 5, Some(1))),
            paraswap: Side::ok("100".into(), 4, Some(1)),
            oneinch: None,
            zerox: None,
        })
        .collect::<Vec<_>>();
        let summary = summarize(&rows);
        for name in ["steth", "shib", "other"] {
            assert_eq!(summary.cohorts[name].rows, 1);
            assert_eq!(
                summary.cohorts[name].control.median_delta_bps,
                Some(-5000.0)
            );
            assert_eq!(summary.cohorts[name].candidate.median_delta_bps, Some(0.0));
            assert_eq!(summary.cohorts[name].candidate_better, 1);
        }
        assert_eq!(summary.cohorts["all"].candidate_better, 3);
        let route = json!([{"swaps": [{"swapExchanges": [{"exchange": "ShibaSwap", "data": {"exchange": "0x123"}}]}]}]);
        assert_eq!(paraswap_platforms(&route), vec!["ShibaSwap"]);
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
    async fn split_requests_preserve_parallel_allocations_and_quote_block() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let reply = json!({"amount_out":"123", "path":[], "quote_block":42,
            "allocations":[
                {"amount_in":"3", "amount_out":"73", "path":[{"platform":"uniswap-v2"}]},
                {"amount_in":"2", "amount_out":"50", "path":[{"platform":"uniswap-v3"}]}
            ], "search":{"baseline_rpc_requests":1,"additional_rpc_requests":2}});
        let response = reply.clone();
        let app = axum::Router::new().route(
            "/quote",
            axum::routing::post(move |axum::Json(request): axum::Json<Value>| {
                let response = response.clone();
                async move {
                    assert_eq!(request["split"], true);
                    assert_eq!(request["amount_in"], "5");
                    axum::Json(response)
                }
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let side = quote_fluxion(&reqwest::Client::new(), &url, "a", "b", "5", true).await;
        server.abort();
        assert_eq!(side.amount_out.as_deref(), Some("123"));
        assert_eq!(side.platforms, ["uniswap-v2", "uniswap-v3"]);
        assert_eq!(side.hops, Some(1));
        assert_eq!(side.route.as_deref(), Some(&reply));
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
        let side = quote_fluxion(&reqwest::Client::new(), &url, "a", "b", "1", false).await;
        server.abort();
        assert_eq!(side.amount_out.as_deref(), Some("123"));
        assert_eq!(side.platforms, ["uniswap-v2"]);
        assert!(side.latency_ms >= 60);
    }
}
