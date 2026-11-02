use alloy_primitives::{Address, U256};
use fluxion::{
    graph::Graph,
    model::{Pool, PoolKind, QuoteRequest},
    router::Router,
    source::{FixedRate, MockV2},
    split::{PlanKind, SplitConfig, SplitOptimizer},
};
use std::collections::HashMap;

fn fixture() -> (Graph, MockV2, QuoteRequest) {
    let mut graph = Graph::new();
    let mut reserves = HashMap::new();
    for id in 10..14 {
        let address = Address::with_last_byte(id);
        graph.add_pool(Pool {
            kind: PoolKind::UniswapV2,
            address,
            coins: vec![Address::with_last_byte(1), Address::with_last_byte(2)],
        });
        reserves.insert(address, [U256::from(1000), U256::from(100_000)]);
    }
    (
        graph,
        MockV2::new(reserves),
        QuoteRequest {
            token_in: Address::with_last_byte(1),
            token_out: Address::with_last_byte(2),
            amount_in: U256::from(1001),
            max_hops: None,
            amount_out_min: None,
        },
    )
}

#[tokio::test]
async fn four_pool_plan_beats_two_and_conserves_integer_remainder() {
    let (graph, source, request) = fixture();
    let router = Router::default();
    let two = SplitOptimizer::new(SplitConfig::default())
        .unwrap()
        .quote(&router, &graph, &source, &request)
        .await
        .unwrap();
    let multi = SplitOptimizer::new(SplitConfig::multi_pool())
        .unwrap()
        .quote(&router, &graph, &source, &request)
        .await
        .unwrap();
    assert_eq!(multi.kind, PlanKind::Split);
    assert_eq!(multi.allocations.len(), 4);
    assert!(multi.amount_out > two.amount_out);
    assert_eq!(
        multi.allocations.iter().map(|a| a.amount_in).sum::<U256>(),
        request.amount_in
    );
    assert_eq!(
        multi.allocations.iter().map(|a| a.amount_out).sum::<U256>(),
        multi.amount_out
    );
    assert!(multi.stats.additional_quote_calls <= 256);
}

#[tokio::test]
async fn multi_budget_exhaustion_retains_successful_two_pool_plan() {
    let (graph, source, request) = fixture();
    let router = Router::default();
    let two = SplitOptimizer::new(SplitConfig::default())
        .unwrap()
        .quote(&router, &graph, &source, &request)
        .await
        .unwrap();
    let config = SplitConfig {
        max_quote_calls: two.stats.additional_quote_calls,
        ..SplitConfig::multi_pool()
    };
    let multi = SplitOptimizer::new(config)
        .unwrap()
        .quote(&router, &graph, &source, &request)
        .await
        .unwrap();
    assert!(multi.amount_out >= two.amount_out);
    assert_eq!(
        multi.allocations.iter().map(|a| a.amount_in).sum::<U256>(),
        request.amount_in
    );
}

#[tokio::test]
async fn multi_ties_and_tiny_or_maximum_inputs_keep_valid_baseline() {
    let (graph, _, mut request) = fixture();
    for amount in [U256::from(1), U256::from(3), U256::MAX] {
        request.amount_in = amount;
        let result = SplitOptimizer::new(SplitConfig::multi_pool())
            .unwrap()
            .quote(
                &Router::default(),
                &graph,
                &FixedRate::new(U256::from(1)),
                &request,
            )
            .await
            .unwrap();
        assert_eq!(result.kind, PlanKind::Baseline);
        assert_eq!(result.amount_out, amount);
    }
}

async fn api_quote(body: serde_json::Value) -> (axum::http::StatusCode, serde_json::Value) {
    use tower::ServiceExt;
    let (graph, source, _) = fixture();
    let app = fluxion::api::app(std::sync::Arc::new(fluxion::api::AppState {
        graph,
        source: std::sync::Arc::new(source),
        router: Router::default(),
        limiter: fluxion::api::RateLimiter::new(100, std::time::Duration::from_secs(1)),
        rpc: None,
        request_timeout: std::time::Duration::from_secs(3),
    }));
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/quote")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        response.status(),
        serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1_000_000)
                .await
                .unwrap(),
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn http_opt_in_exposes_allocations_without_faking_sequential_path() {
    use serde_json::json;
    let (_, _, request) = fixture();
    let base = json!({"token_in":request.token_in.to_string(),"token_out":request.token_out.to_string(),"amount_in":"1001"});
    let (status, legacy) = api_quote(base.clone()).await;
    assert!(status.is_success());
    assert_eq!(legacy["path"].as_array().unwrap().len(), 1);
    assert!(legacy.get("allocations").is_none());
    assert!(legacy["price_impact_bps"].is_number());
    let mut input = base.clone();
    input["split"] = json!(true);
    input["amount_out_min"] = json!("70000");
    let (status, multi) = api_quote(input.clone()).await;
    assert!(status.is_success());
    assert_eq!(multi["allocations"].as_array().unwrap().len(), 4);
    assert!(multi["path"].as_array().unwrap().is_empty());
    assert!(multi["price_impact_bps"].is_null());
    assert_eq!(
        multi["allocations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| U256::from_str_radix(a["amount_in"].as_str().unwrap(), 10).unwrap())
            .sum::<U256>(),
        request.amount_in
    );
    input["split_max_branches"] = json!(2);
    assert_eq!(
        api_quote(input).await.0,
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut invalid = base;
    invalid["split_max_branches"] = json!(2);
    assert_eq!(
        api_quote(invalid).await.0,
        axum::http::StatusCode::BAD_REQUEST
    );
}

struct SlowFractions(fluxion::source::MockV2);
#[async_trait::async_trait]
impl fluxion::source::QuoteSource for SlowFractions {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount: U256,
    ) -> Result<U256, fluxion::error::QuoteError> {
        if amount != U256::from(1001) {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        self.0.amount_out(pool, i, j, amount).await
    }
    async fn mid_price(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
    ) -> Result<f64, fluxion::error::QuoteError> {
        self.0.mid_price(pool, i, j).await
    }
    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, fluxion::error::QuoteError> {
        self.0.reserve_in(pool, i).await
    }
}

#[tokio::test]
async fn http_deadline_retains_baseline_instead_of_timing_out_during_extra_search() {
    use serde_json::json;
    use tower::ServiceExt;
    let (graph, source, request) = fixture();
    let app = fluxion::api::app(std::sync::Arc::new(fluxion::api::AppState {
        graph,
        source: std::sync::Arc::new(SlowFractions(source)),
        router: Router::default(),
        rpc: None,
        limiter: fluxion::api::RateLimiter::new(100, std::time::Duration::from_secs(1)),
        request_timeout: std::time::Duration::from_millis(200),
    }));
    let response=app.oneshot(axum::http::Request::builder().method("POST").uri("/quote").header("content-type","application/json").body(axum::body::Body::from(json!({"token_in":request.token_in.to_string(),"token_out":request.token_out.to_string(),"amount_in":"1001","split":true}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let value: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(value["plan_kind"], "baseline");
    assert_eq!(value["search"]["stop_reason"], "time_budget");
}

#[tokio::test]
async fn http_split_prices_all_allocations_at_one_returned_block() {
    use axum::Json;
    use serde_json::{Value, json};
    use tower::ServiceExt;
    let rpc_app = axum::Router::new().route(
        "/",
        axum::routing::post(|Json(request): Json<Value>| async move {
            let result = if request["method"] == "eth_blockNumber" {
                "0x7b".to_string()
            } else {
                assert_eq!(request["method"], "eth_call");
                assert_eq!(request["params"][1], "0x7b");
                assert_eq!(request["params"][0]["data"], "0x0902f1ac");
                format!("0x{:064x}{:064x}{:064x}", 1000, 100_000, 0)
            };
            Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = fluxion::rpc::RpcClient::new(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move {
        axum::serve(listener, rpc_app).await.unwrap();
    });
    let (graph, _, request) = fixture();
    let app = fluxion::api::app(std::sync::Arc::new(fluxion::api::AppState {
        graph,
        source: std::sync::Arc::new(FixedRate::new(U256::from(1))),
        router: Router::default(),
        rpc: Some(rpc),
        limiter: fluxion::api::RateLimiter::new(100, std::time::Duration::from_secs(1)),
        request_timeout: std::time::Duration::from_secs(3),
    }));
    let response=app.oneshot(axum::http::Request::builder().method("POST").uri("/quote").header("content-type","application/json").body(axum::body::Body::from(json!({"token_in":request.token_in.to_string(),"token_out":request.token_out.to_string(),"amount_in":"1001","split":true}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let value: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(value["quote_block"], 123);
    assert_eq!(value["allocations"].as_array().unwrap().len(), 4);
    assert!(
        U256::from_str_radix(value["amount_out"].as_str().unwrap(), 10).unwrap()
            > U256::from(70_000)
    );
    server.abort();
}

#[tokio::test]
async fn mock_split_does_not_use_the_readiness_rpc_for_pricing() {
    use serde_json::json;
    use tower::ServiceExt;
    let (graph, source, request) = fixture();
    let rpc = fluxion::rpc::RpcClient::new("http://127.0.0.1:1".into());
    let app = fluxion::api::app_with_split(
        std::sync::Arc::new(fluxion::api::AppState {
            graph,
            source: std::sync::Arc::new(source),
            router: Router::default(),
            rpc: Some(rpc.clone()),
            limiter: fluxion::api::RateLimiter::new(100, std::time::Duration::from_secs(1)),
            request_timeout: std::time::Duration::from_secs(3),
        }),
        SplitOptimizer::new(SplitConfig::multi_pool()).unwrap(),
        false,
    );
    let response=app.oneshot(axum::http::Request::builder().method("POST").uri("/quote").header("content-type","application/json").body(axum::body::Body::from(json!({"token_in":request.token_in.to_string(),"token_out":request.token_out.to_string(),"amount_in":"1001","split":true}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(rpc.request_count(), 0);
    let value: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(value["allocations"].as_array().unwrap().len(), 4);
    assert!(value.get("quote_block").is_none());
}
