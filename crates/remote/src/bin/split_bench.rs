//! Fixed-state allocation experiments and same-run external reference diagnostics.
//! This tool never executes transactions.
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use fluxion_remote::config::Config;
use fluxion_remote::graph::Graph;
use fluxion_remote::model::{Hop, QuoteRequest};
use fluxion_remote::reference::{ReferenceMetadata, paraswap};
use fluxion_remote::router::{PriceSnapshot, Router, RouterConfig};
use fluxion_remote::rpc::RpcClient;
use fluxion_remote::split::{PlanKind, SearchStats, SplitConfig, SplitOptimizer, SplitResult};
use fluxion_remote::universe::{discover_pools, pricing_source, warm_coarse_prices};

#[derive(Debug, Deserialize)]
struct Corpus {
    symbols: HashMap<Address, String>,
    amounts: HashMap<Address, Vec<String>>,
    pairs: Vec<[Address; 2]>,
    #[serde(default)]
    decimals: HashMap<Address, u8>,
}

#[derive(Debug)]
struct Args {
    config: String,
    corpus: String,
    out: Option<PathBuf>,
    block: Option<u64>,
    focus: Option<String>,
    limit: Option<usize>,
    row_timeout_ms: u64,
    search: SplitConfig,
    compare_paraswap: bool,
}

fn args() -> Result<Args, Box<dyn std::error::Error>> {
    let mut args = Args {
        config: "bench/configs/threeway.yaml".into(),
        corpus: "bench/corpus.yaml".into(),
        out: None,
        block: None,
        focus: None,
        limit: None,
        row_timeout_ms: 60_000,
        search: SplitConfig::default(),
        compare_paraswap: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        if flag == "--paraswap" {
            args.compare_paraswap = true;
            continue;
        }
        if flag == "--help" || flag == "-h" {
            println!(
                "Usage: cargo run --bin split_bench -- [options]\n\
              --config PATH       config profile (default bench/configs/threeway.yaml)\n\
              --corpus PATH       raw-unit corpus (default bench/corpus.yaml)\n\
              --out PATH.md       report and sibling JSON\n\
              --block NUMBER      pin this block; otherwise choose after discovery\n\
              --focus SYMBOL     select pairs involving a token symbol/address\n\
              --paraswap         same-run single/two/multi/external comparison; per-row pinned state\n\
              --limit N          maximum selected pairs\n\
              --coarse-segments N --refinement-segments N --rounds N\n\
              --max-pools N --max-quotes N --concurrency N --timeout-ms N\n\
              --row-timeout-ms N  whole-row limit (default 60000; failures are recorded)\n\
              --max-branches N --allocation-segments N (defaults 2 / 20)\n\
              Defaults: coarse=10, refine=4, rounds=2, pools=4, quotes=128, concurrency=8, search timeout=8000 ms."
            );
            std::process::exit(0);
        }
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--config" => args.config = value,
            "--corpus" => args.corpus = value,
            "--out" => args.out = Some(value.into()),
            "--block" => args.block = Some(value.parse()?),
            "--focus" => args.focus = Some(value),
            "--limit" => args.limit = Some(value.parse()?),
            "--coarse-segments" => args.search.coarse_segments = value.parse()?,
            "--refinement-segments" => args.search.refinement_segments = value.parse()?,
            "--rounds" => args.search.refinement_rounds = value.parse()?,
            "--max-pools" => args.search.max_pools = value.parse()?,
            "--max-branches" => args.search.max_branches = value.parse()?,
            "--allocation-segments" => args.search.allocation_segments = value.parse()?,
            "--max-quotes" => args.search.max_quote_calls = value.parse()?,
            "--concurrency" => args.search.concurrency = value.parse()?,
            "--timeout-ms" => args.search.timeout_ms = value.parse()?,
            "--row-timeout-ms" => args.row_timeout_ms = value.parse()?,
            _ => return Err(format!("unknown option: {flag}").into()),
        }
    }
    if args.row_timeout_ms == 0 || args.row_timeout_ms > 3_600_000 {
        return Err("row timeout must be 1..3600000 ms".into());
    }
    if args.compare_paraswap && args.search.max_branches < 3 {
        return Err("--paraswap comparison requires --max-branches 3 or more (normally 4)".into());
    }
    if args
        .out
        .as_ref()
        .is_some_and(|path| path.extension().is_none_or(|ext| ext != "md"))
    {
        return Err("--out must end in .md".into());
    }
    Ok(args)
}

fn hops(hops: &[Hop]) -> Vec<Value> {
    hops.iter().map(|hop| json!({"platform":hop.pool.kind.name(), "operation":hop.pool.kind.operation(hop.i),
        "pool":hop.pool.address, "token_in":hop.token_in,"token_out":hop.token_out,
        "i":hop.i,"j":hop.j,"amount_in":hop.amount_in.to_string(),"amount_out":hop.amount_out.to_string()})).collect()
}

fn as_f64(value: U256) -> f64 {
    value.to_string().parse().unwrap()
}
fn gain_bps(base: U256, out: U256) -> Option<f64> {
    if base.is_zero() {
        None
    } else {
        Some(as_f64(out - base) / as_f64(base) * 10000.0)
    }
}

fn delta(base: U256, out: U256) -> Option<f64> {
    if base.is_zero() {
        None
    } else if out >= base {
        Some(as_f64(out - base) / as_f64(base) * 10000.0)
    } else {
        Some(-as_f64(base - out) / as_f64(base) * 10000.0)
    }
}

fn plan_json(result: &SplitResult, amount: U256) -> Value {
    json!({"amount_out":result.amount_out.to_string(),"baseline_amount_out":result.baseline.amount_out.to_string(),"kind":result.kind,"stats":result.stats,
        "allocations":result.allocations.iter().map(|a|json!({"amount_in":a.amount_in.to_string(),"amount_out":a.amount_out.to_string(),"input_share_percent":as_f64(a.amount_in)/as_f64(amount)*100.0,"path":hops(&a.hops)})).collect::<Vec<_>>()})
}

async fn local_quote(
    optimizer: &SplitOptimizer,
    router: &Router,
    graph: &Graph,
    source: &(dyn fluxion_remote::source::QuoteSource + Send + Sync),
    request: &QuoteRequest,
    rpc: &RpcClient,
    timeout_ms: u64,
) -> Result<(SplitResult, u128), String> {
    let started = Instant::now();
    let meter = || rpc.request_count();
    let result = tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        optimizer.quote_measured(router, graph, source, request, Some(&meter)),
    )
    .await
    .map_err(|_| "whole-row quote timeout".to_string())?
    .map_err(|e| e.to_string())?;
    Ok((result, started.elapsed().as_millis()))
}

struct CompareContext<'a> {
    config: &'a Config,
    graph: &'a Graph,
    rpc: &'a RpcClient,
    client: &'a reqwest::Client,
    optimizer: &'a SplitOptimizer,
    two: &'a SplitOptimizer,
    metadata: &'a mut ReferenceMetadata,
    block_override: Option<u64>,
    timeout_ms: u64,
}

async fn compared(
    row: &mut Row,
    request: &QuoteRequest,
    decimals: [u8; 2],
    context: &mut CompareContext<'_>,
    two_first: bool,
) -> Result<Value, String> {
    let started = Instant::now();
    let external = paraswap(
        context.client,
        request.token_in,
        request.token_out,
        request.amount_in,
        decimals,
    )
    .await;
    let (external, error) = match external {
        Ok(quote) => (Some(quote), None),
        Err(error) => (None, Some(error)),
    };
    let mut data = json!({"paraswap":external,"paraswap_error":error,"paraswap_elapsed_ms":started.elapsed().as_millis()});
    row.comparison = Some(data.clone());
    let head = context
        .rpc
        .block_number()
        .await
        .map_err(|e| e.to_string())?;
    let block = context
        .block_override
        .or_else(|| {
            external
                .as_ref()
                .and_then(|quote| quote.block_number)
                .filter(|b| *b > 0 && *b <= head)
        })
        .unwrap_or(head);
    let hash = context
        .rpc
        .block_hash(block)
        .await
        .map_err(|e| e.to_string())?;
    let rpc = context.rpc.clone().at_block(block);
    let before = rpc.request_count();
    let (source, _) = pricing_source(context.config, rpc.clone());
    warm_coarse_prices(context.graph, &source, &rpc).await;
    let router = Router::new(RouterConfig {
        max_hops: context.config.max_hops,
        top_k: context.config.top_k,
        lambda: context.config.slippage_penalty_lambda,
        reserve_ratio_k: context.config.reserve_ratio_k,
    });
    router.set_snapshot(std::sync::Arc::new(
        PriceSnapshot::build(context.graph, source.as_ref()).await,
    ));
    data["block"] = json!(block);
    data["block_hash"] = json!(hash);
    data["setup_rpc_requests"] = json!(rpc.request_count() - before);
    row.comparison = Some(data.clone());
    let (two, multi) = if two_first {
        let two = local_quote(
            context.two,
            &router,
            context.graph,
            source.as_ref(),
            request,
            &rpc,
            context.timeout_ms,
        )
        .await;
        let multi = local_quote(
            context.optimizer,
            &router,
            context.graph,
            source.as_ref(),
            request,
            &rpc,
            context.timeout_ms,
        )
        .await;
        (two, multi)
    } else {
        let multi = local_quote(
            context.optimizer,
            &router,
            context.graph,
            source.as_ref(),
            request,
            &rpc,
            context.timeout_ms,
        )
        .await;
        let two = local_quote(
            context.two,
            &router,
            context.graph,
            source.as_ref(),
            request,
            &rpc,
            context.timeout_ms,
        )
        .await;
        (two, multi)
    };
    let two_out = two.as_ref().ok().map(|(result, _)| result.amount_out);
    match &two {
        Ok((result, elapsed)) => {
            data["two_pool"] = plan_json(result, request.amount_in);
            data["two_pool"]["elapsed_ms"] = json!(elapsed);
        }
        Err(error) => data["two_pool_error"] = json!(error),
    }
    row.comparison = Some(data.clone());
    let (multi, elapsed) = multi?;
    row.elapsed_ms = elapsed;
    if two
        .as_ref()
        .is_ok_and(|(two, _)| two.baseline.amount_out != multi.baseline.amount_out)
    {
        return Err("paired local baselines differ; comparison invalid".into());
    }
    let external_out = external
        .as_ref()
        .and_then(|quote| U256::from_str_radix(&quote.amount_out, 10).ok());
    data["deltas"] = json!({"single_vs_paraswap_bps":external_out.and_then(|out|delta(out,multi.baseline.amount_out)),
        "two_vs_paraswap_bps":external_out.zip(two_out).and_then(|(reference,local)|delta(reference,local)),
        "multi_vs_paraswap_bps":external_out.and_then(|out|delta(out,multi.amount_out)),
        "multi_vs_two_bps":two_out.and_then(|out|delta(out,multi.amount_out))});
    // The detailed reference replay is intentionally bounded to the target
    // SHIB cases. It can quote BONE diagnostically without changing the graph.
    let shib: Address = "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce"
        .parse()
        .unwrap();
    if (row.token_in == shib || row.token_out == shib)
        && let Some(quote) = &external
    {
        let before = rpc.request_count();
        let full = tokio::time::timeout(
            Duration::from_secs(20),
            context.metadata.replay(
                quote,
                request.token_in,
                request.token_out,
                request.amount_in,
                context.config,
                &rpc,
                source.as_ref(),
            ),
        )
        .await;
        if let Ok(replay) = full {
            let reference = replay
                .amount_out
                .as_ref()
                .and_then(|out| U256::from_str_radix(out, 10).ok());
            data["deltas"]["multi_vs_replayed_reference_bps"] =
                json!(reference.and_then(|reference| delta(reference, multi.amount_out)));
            data["reference_replay"] = serde_json::to_value(replay).unwrap();
        } else {
            data["reference_replay_error"] = json!("reference replay timeout");
        }
        let allowed = tokio::time::timeout(
            Duration::from_secs(20),
            context.metadata.replay_allowed(
                quote,
                request.token_in,
                request.token_out,
                request.amount_in,
                context.config,
                &rpc,
                source.as_ref(),
            ),
        )
        .await;
        if let Ok(replay) = allowed {
            let reference = replay
                .amount_out
                .as_ref()
                .and_then(|out| U256::from_str_radix(out, 10).ok());
            data["deltas"]["multi_vs_allowlisted_reference_bps"] =
                json!(reference.and_then(|reference| delta(reference, multi.amount_out)));
            data["allowlisted_reference_replay"] = serde_json::to_value(replay).unwrap();
        } else {
            data["allowlisted_reference_replay_error"] = json!("reference replay timeout");
        }
        data["replay_rpc_requests"] = json!(rpc.request_count() - before);
    }
    row.set_result(multi, request.amount_in)?;
    data["comparison_elapsed_ms"] = json!(started.elapsed().as_millis());
    Ok(data)
}

#[derive(Debug, Serialize)]
struct Row {
    token_in: Address,
    token_out: Address,
    symbol_in: String,
    symbol_out: String,
    amount_in: String,
    started_unix_ms: u128,
    elapsed_ms: u128,
    rpc_requests: u64,
    baseline_out: Option<String>,
    selected_out: Option<String>,
    kind: Option<PlanKind>,
    gain_bps: Option<f64>,
    stats: Option<SearchStats>,
    baseline_path: Option<Vec<Value>>,
    allocations: Option<Vec<Value>>,
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    comparison: Option<Value>,
}

impl Row {
    fn set_result(&mut self, result: SplitResult, amount: U256) -> Result<(), String> {
        let allocated = result
            .allocations
            .iter()
            .try_fold(U256::ZERO, |sum, a| sum.checked_add(a.amount_in))
            .ok_or("allocation sum overflow")?;
        let output = result
            .allocations
            .iter()
            .try_fold(U256::ZERO, |sum, a| sum.checked_add(a.amount_out))
            .ok_or("output sum overflow")?;
        if allocated != amount
            || output != result.amount_out
            || result.amount_out < result.baseline.amount_out
        {
            return Err(
                "optimizer invariant violated: input/output conservation or baseline guard".into(),
            );
        }
        self.baseline_out = Some(result.baseline.amount_out.to_string());
        self.selected_out = Some(result.amount_out.to_string());
        self.kind = Some(result.kind);
        self.gain_bps = gain_bps(result.baseline.amount_out, result.amount_out);
        self.baseline_path = Some(hops(&result.baseline.hops));
        self.allocations = Some(result.allocations.iter().map(|a| json!({"amount_in":a.amount_in.to_string(),"amount_out":a.amount_out.to_string(),
            "input_share_percent":as_f64(a.amount_in)/as_f64(amount)*100.0,"path":hops(&a.hops)})).collect());
        self.stats = Some(result.stats);
        Ok(())
    }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let n = values.len();
    Some(if n.is_multiple_of(2) {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    } else {
        values[n / 2]
    })
}
fn percentile(mut values: Vec<u128>, p: f64) -> Option<u128> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(values[((values.len() as f64 * p).ceil() as usize).saturating_sub(1)])
}

fn summary(rows: &[&Row]) -> Value {
    let gains = rows
        .iter()
        .filter(|row| row.error.is_none())
        .filter_map(|row| row.gain_bps)
        .collect::<Vec<_>>();
    let mut stops = BTreeMap::new();
    for row in rows {
        if let Some(stats) = &row.stats {
            *stops
                .entry(
                    serde_json::to_value(stats.stop_reason)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_string(),
                )
                .or_insert(0usize) += 1;
        }
    }
    let mut value = json!({"rows":rows.len(),"success":rows.iter().filter(|r|r.error.is_none()).count(),
        "split":rows.iter().filter(|r|r.kind==Some(PlanKind::Split)).count(),
        "baseline":rows.iter().filter(|r|r.kind==Some(PlanKind::Baseline)).count(),
        "direct":rows.iter().filter(|r|r.kind==Some(PlanKind::Direct)).count(),
        "median_gain_bps":median(gains.clone()),"mean_gain_bps":(!gains.is_empty()).then(||gains.iter().sum::<f64>()/gains.len() as f64),
        "max_gain_bps":gains.into_iter().max_by(f64::total_cmp),"stop_reasons":stops,
        "baseline_latency_p50_ms":percentile(rows.iter().filter_map(|r|r.stats.as_ref().map(|s|s.baseline_ms)).collect(),0.5),
        "baseline_latency_p95_ms":percentile(rows.iter().filter_map(|r|r.stats.as_ref().map(|s|s.baseline_ms)).collect(),0.95),
        "search_latency_p50_ms":percentile(rows.iter().filter_map(|r|r.stats.as_ref().map(|s|s.search_ms)).collect(),0.5),
        "search_latency_p95_ms":percentile(rows.iter().filter_map(|r|r.stats.as_ref().map(|s|s.search_ms)).collect(),0.95),
        "total_latency_p50_ms":percentile(rows.iter().map(|r|r.elapsed_ms).collect(),0.5),
        "total_latency_p95_ms":percentile(rows.iter().map(|r|r.elapsed_ms).collect(),0.95),
        "additional_quote_calls":rows.iter().filter_map(|r|r.stats.as_ref().map(|s|s.additional_quote_calls)).sum::<usize>(),
        "baseline_rpc_requests":rows.iter().filter_map(|r|r.stats.as_ref().and_then(|s|s.baseline_rpc_requests)).sum::<u64>(),
        "additional_rpc_requests":rows.iter().filter_map(|r|r.stats.as_ref().and_then(|s|s.additional_rpc_requests)).sum::<u64>(),
        "total_row_rpc_requests_including_failures":rows.iter().map(|r|r.rpc_requests).sum::<u64>()});
    if rows.iter().any(|row| row.comparison.is_some()) {
        for key in [
            "single_vs_paraswap_bps",
            "two_vs_paraswap_bps",
            "multi_vs_paraswap_bps",
            "multi_vs_two_bps",
            "multi_vs_replayed_reference_bps",
            "multi_vs_allowlisted_reference_bps",
        ] {
            let values = rows
                .iter()
                .filter(|row| row.error.is_none())
                .filter_map(|row| row.comparison.as_ref()?.get("deltas")?.get(key)?.as_f64())
                .collect::<Vec<_>>();
            value[key] = json!({"compared":values.len(),"median":median(values.clone()),"mean":(!values.is_empty()).then(||values.iter().sum::<f64>()/values.len() as f64),
                "within_50_bps":values.iter().filter(|value|value.abs()<=50.0).count()});
        }
        value["paraswap_failed"] = json!(
            rows.iter()
                .filter(|row| row
                    .comparison
                    .as_ref()
                    .is_some_and(|data| data["paraswap_error"].is_string()))
                .count()
        );
        for (name, latencies) in [
            (
                "paraswap",
                rows.iter()
                    .filter_map(|row| {
                        row.comparison.as_ref()?["paraswap_elapsed_ms"]
                            .as_u64()
                            .map(u128::from)
                    })
                    .collect::<Vec<_>>(),
            ),
            (
                "two_pool",
                rows.iter()
                    .filter_map(|row| {
                        row.comparison.as_ref()?["two_pool"]["elapsed_ms"]
                            .as_u64()
                            .map(u128::from)
                    })
                    .collect::<Vec<_>>(),
            ),
            (
                "candidate",
                rows.iter()
                    .filter(|row| row.error.is_none())
                    .map(|row| row.elapsed_ms)
                    .collect::<Vec<_>>(),
            ),
        ] {
            value[format!("{name}_latency_p50_ms")] = json!(percentile(latencies.clone(), 0.5));
            value[format!("{name}_latency_p95_ms")] = json!(percentile(latencies, 0.95));
        }
        value["reference_replayed"] = json!(
            rows.iter()
                .filter(|row| row
                    .comparison
                    .as_ref()
                    .is_some_and(|c| c["reference_replay"]["status"] == "replayed"))
                .count()
        );
        value["allowlisted_reference_replayed"] = json!(
            rows.iter()
                .filter(|row| row
                    .comparison
                    .as_ref()
                    .is_some_and(|c| c["allowlisted_reference_replay"]["status"] == "replayed"))
                .count()
        );
        value["multi_better_than_two"] = json!(
            rows.iter()
                .filter(|row| row.comparison.as_ref().is_some_and(|c| {
                    c["deltas"]["multi_vs_two_bps"]
                        .as_f64()
                        .is_some_and(|value| value > 0.0)
                }))
                .count()
        );
        value["multi_worse_than_two"] = json!(
            rows.iter()
                .filter(|row| row.comparison.as_ref().is_some_and(|c| {
                    c["deltas"]["multi_vs_two_bps"]
                        .as_f64()
                        .is_some_and(|value| value < 0.0)
                }))
                .count()
        );
    }
    value
}

fn summaries(rows: &[Row]) -> BTreeMap<&'static str, Value> {
    let shib: Address = "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce"
        .parse()
        .unwrap();
    let steth: Address = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84"
        .parse()
        .unwrap();
    ["all", "shib", "steth", "other"]
        .into_iter()
        .map(|name| {
            let selected = rows
                .iter()
                .filter(|row| {
                    let shib = row.token_in == shib || row.token_out == shib;
                    let steth = row.token_in == steth || row.token_out == steth;
                    match name {
                        "shib" => shib,
                        "steth" => steth,
                        "other" => !shib && !steth,
                        _ => true,
                    }
                })
                .collect::<Vec<_>>();
            (name, summary(&selected))
        })
        .collect()
}

fn save(path: &Path, meta: &Value, rows: &[Row]) -> Result<(), Box<dyn std::error::Error>> {
    let summaries = summaries(rows);
    std::fs::write(
        path.with_extension("json"),
        serde_json::to_string_pretty(&json!({"meta":meta,"summary":summaries,"rows":rows}))?,
    )?;
    let mut md = if meta["paraswap_comparison"] == true {
        String::from("# Single / two-pool / multi-pool / ParaSwap comparison\n\n")
    } else {
        String::from("# Fixed-block pool allocation experiment\n\n")
    };
    let mut readable_meta = meta.clone();
    if let Some(object) = readable_meta.as_object_mut() {
        object.remove("pool_inventory");
        object.remove("corpus_yaml");
        if let Some(blocks) = object.remove("verified_row_blocks") {
            object.insert(
                "verified_row_block_count".into(),
                json!(blocks.as_object().map_or(0, |blocks| blocks.len())),
            );
        }
    }
    writeln!(
        md,
        "```json\n{}\n```\n",
        serde_json::to_string_pretty(&readable_meta)?
    )?;
    writeln!(
        md,
        "Full pool inventory and corpus are in the sibling JSON report.\n"
    )?;
    writeln!(
        md,
        "## Summary\n\n| cohort | rows | success | split | baseline | median gain bps | mean gain bps | max gain bps | baseline p50/p95 ms | extra search p50/p95 ms | extra adapter calls | extra RPC |\n|---|---|---|---|---|---|---|---|---|---|---|---|"
    )?;
    for name in ["all", "shib", "steth", "other"] {
        let s = &summaries[name];
        writeln!(
            md,
            "| {name} | {} | {} | {} | {} | {} | {} | {} | {}/{} | {}/{} | {} | {} |",
            s["rows"],
            s["success"],
            s["split"],
            s["baseline"],
            s["median_gain_bps"],
            s["mean_gain_bps"],
            s["max_gain_bps"],
            s["baseline_latency_p50_ms"],
            s["baseline_latency_p95_ms"],
            s["search_latency_p50_ms"],
            s["search_latency_p95_ms"],
            s["additional_quote_calls"],
            s["additional_rpc_requests"]
        )?;
    }
    writeln!(
        md,
        "\n## Per-row comparison\n\n| pair | amount in | baseline out | selected out | kind | gain bps | search stop | baseline/search ms | extra quotes/RPC | allocations |\n|---|---|---|---|---|---|---|---|---|---|"
    )?;
    for row in rows {
        let stats = row.stats.as_ref();
        let allocations = row
            .allocations
            .as_ref()
            .map(|allocations| {
                allocations
                    .iter()
                    .map(|a| {
                        format!(
                            "{:.3}% {}",
                            a["input_share_percent"].as_f64().unwrap_or(0.0),
                            a["path"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|hop| hop["platform"].as_str().unwrap_or("?"))
                                .collect::<Vec<_>>()
                                .join("→")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" + ")
            })
            .unwrap_or_default();
        let output = row.selected_out.clone().unwrap_or_else(|| {
            row.error
                .as_deref()
                .unwrap_or("incomplete")
                .replace(['\n', '|'], " ")
        });
        writeln!(
            md,
            "| {} → {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            row.symbol_in,
            row.symbol_out,
            row.amount_in,
            row.baseline_out.as_deref().unwrap_or("—"),
            output,
            row.kind
                .map(|kind| serde_json::to_value(kind)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string())
                .unwrap_or_else(|| "error".into()),
            row.gain_bps
                .map(|g| format!("{g:.4}"))
                .unwrap_or_else(|| "—".into()),
            stats
                .map(|s| serde_json::to_value(s.stop_reason)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string())
                .unwrap_or_default(),
            stats
                .map(|s| format!("{}/{}", s.baseline_ms, s.search_ms))
                .unwrap_or_default(),
            stats
                .map(|s| format!(
                    "{}/{}",
                    s.additional_quote_calls,
                    s.additional_rpc_requests.unwrap_or(0)
                ))
                .unwrap_or_default(),
            allocations
        )?;
    }
    if rows.iter().any(|row| row.comparison.is_some()) {
        writeln!(
            md,
            "\n## Same-run comparison\n\nLocal single/two/multi modes use the same pinned block per row. Raw external quotes can contain cached, mixed-block state. SHIB reference allocations are re-quoted separately at that local block; unsupported venues/shared pools produce no fabricated replay total.\n\n| cohort | n | single median/mean vs ParaSwap bps | two median/mean | multi median/mean | multi within ±50 bps | multi median vs two bps |\n|---|---|---|---|---|---|---|"
        )?;
        for name in ["all", "shib", "steth", "other"] {
            let s = &summaries[name];
            writeln!(
                md,
                "| {name} | {} | {}/{} | {}/{} | {}/{} | {} | {} |",
                s["multi_vs_paraswap_bps"]["compared"],
                s["single_vs_paraswap_bps"]["median"],
                s["single_vs_paraswap_bps"]["mean"],
                s["two_vs_paraswap_bps"]["median"],
                s["two_vs_paraswap_bps"]["mean"],
                s["multi_vs_paraswap_bps"]["median"],
                s["multi_vs_paraswap_bps"]["mean"],
                s["multi_vs_paraswap_bps"]["within_50_bps"],
                s["multi_vs_two_bps"]["median"]
            )?;
        }
        writeln!(
            md,
            "\n| cohort | single phase p50/p95 ms | two-pool p50/p95 ms | multi-pool p50/p95 ms | ParaSwap p50/p95 ms |\n|---|---|---|---|---|"
        )?;
        for name in ["all", "shib", "steth", "other"] {
            let s = &summaries[name];
            writeln!(
                md,
                "| {name} | {}/{} | {}/{} | {}/{} | {}/{} |",
                s["baseline_latency_p50_ms"],
                s["baseline_latency_p95_ms"],
                s["two_pool_latency_p50_ms"],
                s["two_pool_latency_p95_ms"],
                s["candidate_latency_p50_ms"],
                s["candidate_latency_p95_ms"],
                s["paraswap_latency_p50_ms"],
                s["paraswap_latency_p95_ms"]
            )?;
        }
        writeln!(
            md,
            "\n| pair | input | row block | single output | two-pool output | multi output | ParaSwap output | multi vs ParaSwap bps | multi vs full replay bps | multi vs allowlisted replay bps |\n|---|---|---|---|---|---|---|---|---|---|"
        )?;
        for row in rows {
            if let Some(c) = &row.comparison {
                writeln!(
                    md,
                    "| {} → {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                    row.symbol_in,
                    row.symbol_out,
                    row.amount_in,
                    c["block"],
                    row.baseline_out.as_deref().unwrap_or("—"),
                    c["two_pool"]["amount_out"].as_str().unwrap_or("—"),
                    row.selected_out.as_deref().unwrap_or("—"),
                    c["paraswap"]["amount_out"].as_str().unwrap_or("—"),
                    c["deltas"]["multi_vs_paraswap_bps"],
                    c["deltas"]["multi_vs_replayed_reference_bps"],
                    c["deltas"]["multi_vs_allowlisted_reference_bps"]
                )?;
            }
        }
    }
    writeln!(
        md,
        "\n## Interpretation\n\nGain is (selected − baseline) / baseline × 10000, using integer adapter outputs for selection. Baseline includes the existing multi-hop/conversion Router. Candidate pools are capped and search is approximate. A budget stop retains the best complete plan found. In fixed mode, all local quotes use the recorded block; in ParaSwap comparison mode, each row pins its local variants to the reference-reported block when usable, otherwise the current head (or explicit --block override). Discovery remains a live filtered inventory. All used block hashes are verified at the end. RPC counts include actual HTTP attempts/retries; adapter calls are separate. Comparison row RPC includes setup, both local optimizers and optional diagnostic replay, not external HTTP. Candidate latency excludes those extra comparison operations; their total is recorded separately. Replay without non-allowlisted exchanges renormalizes remaining input weights and is a diagnostic reference plan, not the aggregator reported quote. Incomplete runs have completed=false; failures and unsupported replays stay visible. Gross outputs exclude gas, MEV and actual execution. Percentages are display-only; raw integer inputs are authoritative."
    )?;
    std::fs::write(path, md)?;
    Ok(())
}

fn fingerprint() -> Result<String, Box<dyn std::error::Error>> {
    fn collect(path: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                collect(&path, files)?;
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
        Ok(())
    }
    let mut files = vec![PathBuf::from("Cargo.toml"), PathBuf::from("Cargo.lock")];
    collect(Path::new("src"), &mut files)?;
    files.sort();
    let mut bytes = Vec::new();
    for file in files {
        bytes.extend_from_slice(file.to_string_lossy().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&std::fs::read(file)?);
        bytes.push(0);
    }
    Ok(format!("{:#x}", alloy_primitives::keccak256(bytes)))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("info")
        .init();
    let args = args()?;
    let optimizer = SplitOptimizer::new(args.search.clone()).map_err(std::io::Error::other)?;
    let config = Config::load(&args.config)?;
    let text = std::fs::read_to_string(&args.corpus)?;
    let corpus: Corpus = serde_yaml::from_str(&text)?;
    let label = |token: &Address| {
        corpus
            .symbols
            .get(token)
            .cloned()
            .unwrap_or_else(|| format!("{token:#x}"))
    };
    let mut pairs = corpus
        .pairs
        .iter()
        .filter(|pair| {
            args.focus.as_ref().is_none_or(|focus| {
                pair.iter().any(|token| {
                    label(token).eq_ignore_ascii_case(focus)
                        || format!("{token:#x}").eq_ignore_ascii_case(focus)
                })
            })
        })
        .copied()
        .collect::<Vec<_>>();
    if let Some(limit) = args.limit {
        pairs.truncate(limit);
    }
    if pairs.is_empty() {
        return Err("no selected corpus pairs".into());
    }
    let mut cases = Vec::new();
    for [a, b] in pairs {
        if args.compare_paraswap
            && (!corpus.decimals.contains_key(&a) || !corpus.decimals.contains_key(&b))
        {
            return Err("explicit corpus decimals required for external quotes".into());
        }
        let amounts = corpus.amounts.get(&a).ok_or("missing input amounts")?;
        if amounts.is_empty() {
            return Err("empty amount list".into());
        }
        for raw in amounts {
            let amount = U256::from_str_radix(raw, 10)?;
            if amount.is_zero() {
                return Err("zero corpus amount".into());
            }
            cases.push((a, b, amount));
        }
    }
    let rpc = RpcClient::new(config.rpc_url.clone());
    let key = std::env::var("GRAPH_API_KEY").ok();
    let pools = discover_pools(&config, &rpc, key.as_deref()).await?;
    let discovery_rpc = rpc.request_count();
    let block = match args.block {
        Some(block) => block,
        None => rpc.block_number().await?,
    };
    let hash = rpc.block_hash(block).await?;
    let rpc = rpc.at_block(block);
    let mut graph = Graph::new();
    for pool in &pools {
        graph.add_pool(pool.clone());
    }
    let (source, _state) = pricing_source(&config, rpc.clone());
    warm_coarse_prices(&graph, &source, &rpc).await;
    let router = Router::new(RouterConfig {
        max_hops: config.max_hops,
        top_k: config.top_k,
        lambda: config.slippage_penalty_lambda,
        reserve_ratio_k: config.reserve_ratio_k,
    });
    router.set_snapshot(std::sync::Arc::new(
        PriceSnapshot::build(&graph, source.as_ref()).await,
    ));
    let setup_rpc = rpc.request_count() - discovery_rpc;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let path = args
        .out
        .unwrap_or_else(|| format!("docs/benchmarks/split-{timestamp}-{block}.md").into());
    if path.extension().is_none_or(|ext| ext != "md") {
        return Err("--out must end in .md".into());
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let git = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()?;
    let dirty = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()?;
    let inventory=pools.iter().map(|pool|json!({"platform":pool.kind.name(),"address":pool.address,"coins":pool.coins,
        "fee":pool.kind.v3_fee(),
        "directions":(0..pool.coins.len()).flat_map(|i|(0..pool.coins.len()).filter(move |&j|pool.kind.allows(i,j)).map(move |j|[i,j])).collect::<Vec<_>>()})).collect::<Vec<_>>();
    let mut two_config = args.search.clone();
    two_config.max_branches = 2;
    let two_optimizer = SplitOptimizer::new(two_config.clone()).map_err(std::io::Error::other)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;
    let mut metadata = ReferenceMetadata::default();
    let mut meta = json!({"timestamp_unix":timestamp,"git_sha":String::from_utf8_lossy(&git.stdout).trim(),"working_tree_dirty":!dirty.stdout.is_empty(),
        "source_fingerprint_keccak256":fingerprint()?,"block":block,"block_hash":hash,"block_hash_unchanged":null,"completed":false,"expected_rows":cases.len(),
        "config":config.public_settings(),"search_config":args.search,"row_timeout_ms":args.row_timeout_ms,"corpus_yaml":text,"focus":args.focus,
        "pool_count":pools.len(),"edge_count":graph.edge_count(),"pool_inventory":inventory,"discovery_rpc_requests":discovery_rpc,"setup_rpc_requests":setup_rpc,
        "paraswap_comparison":args.compare_paraswap,"two_pool_search_config":two_config,
        "pricing_state_mode":if args.compare_paraswap{"per_row_reference_block_if_available_else_head"}else{"one_fixed_block"},
        "reference_replay_scope":"SHIB only; supported interfaces, no shared-pool simulation"});
    let mut rows = Vec::new();
    save(&path, &meta, &rows)?;
    for (a, b, amount) in cases {
        let started = Instant::now();
        let before = rpc.request_count();
        let mut row = Row {
            token_in: a,
            token_out: b,
            symbol_in: label(&a),
            symbol_out: label(&b),
            amount_in: amount.to_string(),
            started_unix_ms: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
            elapsed_ms: 0,
            rpc_requests: 0,
            baseline_out: None,
            selected_out: None,
            kind: None,
            gain_bps: None,
            stats: None,
            baseline_path: None,
            allocations: None,
            error: None,
            comparison: None,
        };
        let request = QuoteRequest {
            token_in: a,
            token_out: b,
            amount_in: amount,
            max_hops: None,
            amount_out_min: None,
        };
        if args.compare_paraswap {
            let mut context = CompareContext {
                config: &config,
                graph: &graph,
                rpc: &rpc,
                client: &client,
                optimizer: &optimizer,
                two: &two_optimizer,
                metadata: &mut metadata,
                block_override: args.block,
                timeout_ms: args.row_timeout_ms,
            };
            match compared(
                &mut row,
                &request,
                [corpus.decimals[&a], corpus.decimals[&b]],
                &mut context,
                rows.len().is_multiple_of(2),
            )
            .await
            {
                Ok(data) => row.comparison = Some(data),
                Err(error) => {
                    row.error = Some(error);
                    row.elapsed_ms = started.elapsed().as_millis();
                }
            }
        } else {
            let meter = || rpc.request_count();
            match tokio::time::timeout(
                Duration::from_millis(args.row_timeout_ms),
                optimizer.quote_measured(&router, &graph, source.as_ref(), &request, Some(&meter)),
            )
            .await
            {
                Ok(Ok(result)) => {
                    if let Err(error) = row.set_result(result, amount) {
                        row.error = Some(error);
                    }
                }
                Ok(Err(error)) => row.error = Some(error.to_string()),
                Err(_) => row.error = Some("whole-row timeout (no result fabricated)".into()),
            }
            row.elapsed_ms = started.elapsed().as_millis();
        }
        row.rpc_requests = rpc.request_count() - before;
        eprintln!(
            "{}→{} {} kind={:?} gain_bps={:?} stop={:?} error={:?}",
            row.symbol_in,
            row.symbol_out,
            row.amount_in,
            row.kind,
            row.gain_bps,
            row.stats.as_ref().map(|s| s.stop_reason),
            row.error
        );
        rows.push(row);
        save(&path, &meta, &rows)?;
    }
    meta["completed"] = json!(true);
    let mut verified = BTreeMap::new();
    let mut unchanged = rpc.block_hash(block).await? == hash;
    for row in &rows {
        if let Some(data) = &row.comparison
            && let (Some(block), Some(hash)) = (data["block"].as_u64(), data["block_hash"].as_str())
        {
            match verified.entry(block) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let same = rpc.block_hash(block).await? == hash;
                    unchanged &= same;
                    entry.insert(json!({"hash":hash,"unchanged":same}));
                }
                std::collections::btree_map::Entry::Occupied(entry) => {
                    unchanged &= entry.get()["hash"] == hash
                }
            }
        }
    }
    meta["verified_row_blocks"] = json!(verified);
    meta["block_hash_unchanged"] = json!(unchanged);
    save(&path, &meta, &rows)?;
    println!(
        "wrote {} and {}",
        path.display(),
        path.with_extension("json").display()
    );
    if meta["block_hash_unchanged"] != true {
        return Err("block hash changed; comparison is invalid".into());
    }
    Ok(())
}
