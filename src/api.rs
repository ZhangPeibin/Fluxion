use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alloy_primitives::{Address, U256};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router as AxumRouter};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::QuoteError;
use crate::graph::Graph;
use crate::local_state::{QuoteState, SnapshotStore, StateError};
use crate::model::{Hop, QuoteRequest, Route};
use crate::router::Router;
use crate::rpc::RpcClient;
use crate::source::QuoteSource;
use crate::split::{Allocation, PlanKind, SearchStats, SplitConfig, SplitOptimizer, SplitResult};

#[derive(Debug, Deserialize)]
pub struct QuoteRequestDto {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: String,
    #[serde(default)]
    pub max_hops: Option<usize>,
    #[serde(default)]
    pub amount_out_min: Option<String>,
    #[serde(default)]
    pub split: bool,
    #[serde(default)]
    pub split_max_branches: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct HopDto {
    pub operation: String,
    pub platform: String,
    pub pool: String,
    pub i: usize,
    pub j: usize,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: String,
    pub amount_out: String,
}

#[derive(Debug, Serialize)]
pub struct QuoteResponseDto {
    pub amount_out: String,
    pub price_impact_bps: Option<i64>,
    pub path: Vec<HopDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_kind: Option<PlanKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocations: Option<Vec<AllocationDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_amount_out: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_block: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<QuoteState>,
}

#[derive(Debug, Serialize)]
pub struct AllocationDto {
    pub amount_in: String,
    pub amount_out: String,
    pub input_share_percent: f64,
    pub path: Vec<HopDto>,
}

fn allocation_to_dto(allocation: &Allocation, total: U256) -> AllocationDto {
    let numeric = |n: U256| n.to_string().parse::<f64>().unwrap();
    AllocationDto {
        amount_in: allocation.amount_in.to_string(),
        amount_out: allocation.amount_out.to_string(),
        input_share_percent: numeric(allocation.amount_in) / numeric(total) * 100.0,
        path: allocation.hops.iter().map(hop_to_dto).collect(),
    }
}

fn split_to_dto(result: SplitResult, total: U256, block: Option<u64>) -> QuoteResponseDto {
    let path = if result.allocations.len() == 1 {
        result.allocations[0].hops.iter().map(hop_to_dto).collect()
    } else {
        vec![]
    };
    QuoteResponseDto {
        amount_out: result.amount_out.to_string(),
        price_impact_bps: if result.kind == PlanKind::Baseline {
            Some(result.baseline.price_impact_bps)
        } else {
            None
        },
        path,
        plan_kind: Some(result.kind),
        allocations: Some(
            result
                .allocations
                .iter()
                .map(|a| allocation_to_dto(a, total))
                .collect(),
        ),
        baseline_amount_out: Some(result.baseline.amount_out.to_string()),
        search: Some(result.stats),
        quote_block: block,
        state: None,
    }
}

/// A directed pool hop in the routing multigraph. Price fields are omitted when
/// no snapshot (or no value for this edge) is available.
#[derive(Debug, Serialize)]
pub struct GraphEdgeDto {
    pub operation: String,
    pub from: String,
    pub to: String,
    pub platform: String,
    pub pool: String,
    pub i: usize,
    pub j: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mid_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserve: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GraphResponseDto {
    pub tokens: Vec<String>,
    pub edges: Vec<GraphEdgeDto>,
}

pub struct RateLimiter {
    max_requests: u64,
    window: Duration,
    state: Mutex<(Instant, u64)>,
}

impl RateLimiter {
    pub fn new(max_requests: u64, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            state: Mutex::new((Instant::now(), 0)),
        }
    }

    fn allow(&self) -> bool {
        let mut s = self.state.lock().unwrap();
        let now = Instant::now();
        if now.duration_since(s.0) >= self.window {
            *s = (now, 0);
        }
        s.1 += 1;
        s.1 <= self.max_requests
    }
}

pub struct AppState {
    pub graph: Graph,
    pub source: Arc<dyn QuoteSource + Send + Sync>,
    pub router: Router,
    pub limiter: RateLimiter,
    pub rpc: Option<RpcClient>,
    pub request_timeout: Duration,
}

struct SplitService {
    optimizer: Arc<SplitOptimizer>,
    pin_pricing: bool,
    local: Option<Arc<SnapshotStore>>,
}

pub enum ApiError {
    Quote(QuoteError),
    BadRequest(String),
    RateLimited,
    Timeout,
    State(StateError),
}

impl From<StateError> for ApiError {
    fn from(error: StateError) -> Self {
        Self::State(error)
    }
}

impl From<QuoteError> for ApiError {
    fn from(e: QuoteError) -> Self {
        ApiError::Quote(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::State(error) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({"error":error.to_string(),"pricing_mode":"local_snapshot"})),
                )
                    .into_response();
            }
            ApiError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "RateLimited", None),
            ApiError::Timeout => (StatusCode::GATEWAY_TIMEOUT, "Timeout", None),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "InvalidRequest", Some(msg)),
            ApiError::Quote(e) => match e {
                QuoteError::NoPath => (StatusCode::NOT_FOUND, "NoPathFound", None),
                QuoteError::AmountOutBelowMin => {
                    (StatusCode::UNPROCESSABLE_ENTITY, "AmountOutBelowMin", None)
                }
                QuoteError::InsufficientLiquidity => (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "InsufficientLiquidity",
                    None,
                ),
                QuoteError::ZeroAmount => (StatusCode::BAD_REQUEST, "ZeroAmount", None),
                QuoteError::UnknownPool => (StatusCode::NOT_FOUND, "PoolNotFound", None),
                QuoteError::Overflow => (StatusCode::UNPROCESSABLE_ENTITY, "Overflow", None),
                QuoteError::RpcError(_) => (StatusCode::BAD_GATEWAY, "RpcError", None),
            },
        };
        let body = match message {
            Some(m) => json!({ "error": code, "message": m }),
            None => json!({ "error": code }),
        };
        (status, Json(body)).into_response()
    }
}

fn parse_address(s: &str) -> Result<Address, ApiError> {
    s.parse::<Address>()
        .map_err(|_| ApiError::BadRequest(format!("invalid address: {s}")))
}

fn parse_u256(s: &str) -> Result<U256, ApiError> {
    U256::from_str_radix(s, 10).map_err(|_| ApiError::BadRequest(format!("invalid amount: {s}")))
}

fn hop_to_dto(hop: &Hop) -> HopDto {
    HopDto {
        operation: hop.pool.kind.operation(hop.i).to_string(),
        platform: hop.pool.kind.name().to_string(),
        pool: hop.pool.address.to_string(),
        i: hop.i,
        j: hop.j,
        token_in: hop.token_in.to_string(),
        token_out: hop.token_out.to_string(),
        amount_in: hop.amount_in.to_string(),
        amount_out: hop.amount_out.to_string(),
    }
}

fn route_to_dto(route: Route) -> QuoteResponseDto {
    QuoteResponseDto {
        amount_out: route.amount_out.to_string(),
        price_impact_bps: Some(route.price_impact_bps),
        path: route.hops.iter().map(hop_to_dto).collect(),
        plan_kind: None,
        allocations: None,
        baseline_amount_out: None,
        search: None,
        quote_block: None,
        state: None,
    }
}

fn optimizer_for(
    service: &SplitService,
    limit: Option<usize>,
) -> Result<Arc<SplitOptimizer>, ApiError> {
    let optimizer = service.optimizer.clone();
    if let Some(limit) = limit {
        if limit < 2 || limit > optimizer.config().max_branches {
            return Err(ApiError::BadRequest(format!(
                "split_max_branches must be 2..{}",
                optimizer.config().max_branches
            )));
        }
        let mut config = optimizer.config().clone();
        config.max_branches = limit;
        Ok(Arc::new(
            SplitOptimizer::new(config).map_err(ApiError::BadRequest)?,
        ))
    } else {
        Ok(optimizer)
    }
}

async fn quote(
    State(state): State<Arc<AppState>>,
    Extension(service): Extension<Arc<SplitService>>,
    Json(req): Json<QuoteRequestDto>,
) -> Result<Json<QuoteResponseDto>, ApiError> {
    if !state.limiter.allow() {
        return Err(ApiError::RateLimited);
    }

    let request = QuoteRequest {
        token_in: parse_address(&req.token_in)?,
        token_out: parse_address(&req.token_out)?,
        amount_in: parse_u256(&req.amount_in)?,
        max_hops: req.max_hops,
        amount_out_min: req.amount_out_min.as_deref().map(parse_u256).transpose()?,
    };

    if !req.split && req.split_max_branches.is_some() {
        return Err(ApiError::BadRequest(
            "split_max_branches requires split=true".into(),
        ));
    }
    if let Some(store) = &service.local {
        if request.amount_in.is_zero() {
            return Err(QuoteError::ZeroAmount.into());
        }
        let optimizer = optimizer_for(&service, req.split_max_branches)?;
        let snapshot = store.acquire()?;
        let deadline = tokio::time::Instant::now()
            + state
                .request_timeout
                .saturating_sub(Duration::from_millis(25));
        let calculate = async {
            let mut response = if req.split {
                let mut baseline_request = request.clone();
                baseline_request.amount_out_min = None;
                let result = snapshot
                    .split(&optimizer, &baseline_request, deadline)
                    .await?;
                if request
                    .amount_out_min
                    .is_some_and(|min| result.amount_out < min)
                {
                    return Err(ApiError::Quote(QuoteError::AmountOutBelowMin));
                }
                split_to_dto(result, request.amount_in, Some(snapshot.block().number))
            } else {
                route_to_dto(snapshot.route(&request).await?)
            };
            response.quote_block = Some(snapshot.block().number);
            response.state = Some(store.quote_state(&snapshot)?);
            Ok::<_, ApiError>(Json(response))
        };
        return tokio::time::timeout(state.request_timeout, calculate)
            .await
            .map_err(|_| ApiError::Timeout)?;
    }

    if req.split {
        if request.amount_in.is_zero() {
            return Err(ApiError::Quote(QuoteError::ZeroAmount));
        }
        let optimizer = optimizer_for(&service, req.split_max_branches)?;
        let deadline = tokio::time::Instant::now()
            + state
                .request_timeout
                .saturating_sub(Duration::from_millis(25));
        let mut split_request = request.clone();
        split_request.amount_out_min = None;
        let calculation = async {
            if service.pin_pricing
                && let Some(rpc) = state
                    .rpc
                    .as_ref()
                    .filter(|_| request.token_in != request.token_out)
            {
                let block = rpc.block_number().await?;
                // This assembly only selects adapter/cache implementations;
                // the server's existing filtered graph and Router are retained.
                // Fresh request-local caches cannot carry a different block.
                let (source, _) = crate::universe::pricing_source(
                    &crate::config::Config::default(),
                    rpc.clone().at_block(block),
                );
                optimizer
                    .quote_until(
                        &state.router,
                        &state.graph,
                        source.as_ref(),
                        &split_request,
                        deadline,
                    )
                    .await
                    .map(|result| (result, Some(block)))
            } else {
                optimizer
                    .quote_until(
                        &state.router,
                        &state.graph,
                        state.source.as_ref(),
                        &split_request,
                        deadline,
                    )
                    .await
                    .map(|result| (result, None))
            }
        };
        return match tokio::time::timeout(state.request_timeout, calculation).await {
            Ok(result) => {
                let (result, block) = result?;
                if request
                    .amount_out_min
                    .is_some_and(|min| result.amount_out < min)
                {
                    return Err(ApiError::Quote(QuoteError::AmountOutBelowMin));
                }
                Ok(Json(split_to_dto(result, request.amount_in, block)))
            }
            Err(_) => Err(ApiError::Timeout),
        };
    }
    let route = match tokio::time::timeout(
        state.request_timeout,
        state
            .router
            .route(&state.graph, state.source.as_ref(), &request),
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => return Err(ApiError::Timeout),
    };
    tracing::info!(
        token_in = %request.token_in,
        token_out = %request.token_out,
        amount_out = %route.amount_out,
        hops = route.hops.len(),
        "quote resolved"
    );
    Ok(Json(route_to_dto(route)))
}

async fn graph(
    State(state): State<Arc<AppState>>,
    Extension(service): Extension<Arc<SplitService>>,
) -> Result<Json<GraphResponseDto>, ApiError> {
    let local = service
        .local
        .as_ref()
        .map(|store| store.acquire())
        .transpose()?;
    let snapshot = local
        .as_ref()
        .map(|snapshot| snapshot.prices())
        .or_else(|| state.router.snapshot());
    let graph = local
        .as_ref()
        .map_or(&state.graph, |snapshot| snapshot.graph());
    let tokens = graph
        .tokens()
        .iter()
        .map(|token| token.to_string())
        .collect();
    let mut edges = Vec::with_capacity(graph.edge_count());
    for idx in 0..graph.edge_count() {
        let edge = graph.edge(idx);
        let pool = graph.pool(edge.pool_idx);
        let price = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.edge_prices().get(idx));
        let mid_price = price
            .map(|p| p.mid_price)
            .filter(|mid_price| mid_price.is_finite() && *mid_price > 0.0);
        let reserve = price
            .map(|p| p.reserve_in)
            .filter(|reserve| !reserve.is_zero() && !pool.kind.is_conversion());
        edges.push(GraphEdgeDto {
            operation: pool.kind.operation(edge.i).to_string(),
            from: pool.coins[edge.i].to_string(),
            to: pool.coins[edge.j].to_string(),
            platform: pool.kind.name().to_string(),
            pool: pool.address.to_string(),
            i: edge.i,
            j: edge.j,
            mid_price,
            reserve: reserve.map(|reserve| reserve.to_string()),
        });
    }
    Ok(Json(GraphResponseDto { tokens, edges }))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn ready(
    State(state): State<Arc<AppState>>,
    Extension(service): Extension<Arc<SplitService>>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Some(store) = &service.local {
        let status = store.status();
        return (
            if status.ready {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            },
            Json(json!(status)),
        );
    }
    match &state.rpc {
        Some(rpc) => {
            let check = tokio::time::timeout(Duration::from_secs(3), rpc.block_number()).await;
            match check {
                Ok(Ok(_)) => (StatusCode::OK, Json(json!({ "status": "ready" }))),
                _ => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "status": "not ready" })),
                ),
            }
        }
        None => (StatusCode::OK, Json(json!({ "status": "ready" }))),
    }
}

pub fn app(state: Arc<AppState>) -> AxumRouter {
    app_with_split(
        state,
        SplitOptimizer::new(SplitConfig::multi_pool()).expect("valid default split configuration"),
        true,
    )
}

pub fn app_with_split(
    state: Arc<AppState>,
    optimizer: SplitOptimizer,
    pin_pricing: bool,
) -> AxumRouter {
    build_app(state, optimizer, pin_pricing, None)
}

pub fn app_with_local_state(
    state: Arc<AppState>,
    optimizer: SplitOptimizer,
    store: Arc<SnapshotStore>,
) -> AxumRouter {
    build_app(state, optimizer, false, Some(store))
}

async fn state_status(Extension(service): Extension<Arc<SplitService>>) -> Json<serde_json::Value> {
    Json(service.local.as_ref().map_or_else(
        || json!({"pricing_mode":"rpc"}),
        |store| json!(store.status()),
    ))
}

fn build_app(
    state: Arc<AppState>,
    optimizer: SplitOptimizer,
    pin_pricing: bool,
    local: Option<Arc<SnapshotStore>>,
) -> AxumRouter {
    AxumRouter::new()
        .route("/quote", post(quote))
        .route("/graph", get(graph))
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/state", get(state_status))
        .layer(Extension(Arc::new(SplitService {
            optimizer: Arc::new(optimizer),
            pin_pricing,
            local,
        })))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Pool, PoolKind};
    use crate::router::PriceSnapshot;
    use crate::source::MockV2;
    use alloy_primitives::Address;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use std::collections::HashMap;
    use tower::ServiceExt;

    fn mock_state() -> Arc<AppState> {
        let eth = Address::with_last_byte(1);
        let usdc = Address::with_last_byte(2);
        let mut g = Graph::new();
        let a = Address::with_last_byte(10);
        let b = Address::with_last_byte(11);
        g.add_pool(Pool {
            kind: PoolKind::UniswapV2,
            address: a,
            coins: vec![eth, usdc],
        });
        g.add_pool(Pool {
            kind: PoolKind::UniswapV2,
            address: b,
            coins: vec![eth, usdc],
        });
        let mut reserves = HashMap::new();
        reserves.insert(a, [U256::from(1000u64), U256::from(5_000_000u64)]);
        reserves.insert(b, [U256::from(500u64), U256::from(2_000_000u64)]);
        let source = MockV2::new(reserves);
        Arc::new(AppState {
            graph: g,
            source: Arc::new(source),
            router: Router::default(),
            limiter: RateLimiter::new(1000, Duration::from_secs(1)),
            rpc: None,
            request_timeout: Duration::from_secs(5),
        })
    }

    fn post_body(body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/quote")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn quote_endpoint_returns_best_route() {
        let app = app(mock_state());
        let eth = Address::with_last_byte(1).to_string();
        let usdc = Address::with_last_byte(2).to_string();
        let resp = app
            .oneshot(post_body(json!({
                "token_in": eth,
                "token_out": usdc,
                "amount_in": "100"
            })))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let out: u128 = v["amount_out"].as_str().unwrap().parse().unwrap();
        assert!(out > 400_000);
        assert_eq!(v["path"].as_array().unwrap().len(), 1);
        assert_eq!(
            v["path"][0]["pool"],
            Address::with_last_byte(10).to_string()
        );
        assert!(v["price_impact_bps"].is_i64());
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = app(mock_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_returns_ok_without_rpc() {
        let app = app(mock_state());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn graph_endpoint_returns_edges_without_snapshot() {
        let app = app(mock_state());
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/graph")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let tokens: Vec<&str> = v["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        assert_eq!(tokens.len(), 2);
        assert!(tokens.contains(&Address::with_last_byte(1).to_string().as_str()));
        assert!(tokens.contains(&Address::with_last_byte(2).to_string().as_str()));

        let edges = v["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 4);
        for edge in edges {
            assert_eq!(edge["platform"], "uniswap-v2");
            assert!(edge["pool"].is_string());
            assert!(edge["from"].is_string());
            assert!(edge["to"].is_string());
            assert!(edge["i"].is_u64());
            assert!(edge["j"].is_u64());
            assert!(edge.get("mid_price").is_none());
            assert!(edge.get("reserve").is_none());
        }

        // Read-only: a quote still works after inspecting the graph.
        let resp = app
            .oneshot(post_body(json!({
                "token_in": Address::with_last_byte(1).to_string(),
                "token_out": Address::with_last_byte(2).to_string(),
                "amount_in": "100"
            })))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn graph_endpoint_includes_prices_from_snapshot() {
        let state = mock_state();
        let snapshot = PriceSnapshot::build(&state.graph, state.source.as_ref()).await;
        state.router.set_snapshot(Arc::new(snapshot));
        let app = app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/graph")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let edges = v["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 4);
        for edge in edges {
            let mid_price = edge["mid_price"].as_f64().unwrap();
            assert!(mid_price.is_finite() && mid_price > 0.0);
            let reserve: u128 = edge["reserve"].as_str().unwrap().parse().unwrap();
            assert!(reserve > 0);
        }
    }

    #[tokio::test]
    async fn missing_pair_returns_404() {
        let app = app(mock_state());
        let eth = Address::with_last_byte(1).to_string();
        let other = Address::with_last_byte(99).to_string();
        let resp = app
            .oneshot(post_body(json!({
                "token_in": eth,
                "token_out": other,
                "amount_in": "100"
            })))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn invalid_amount_returns_400() {
        let app = app(mock_state());
        let eth = Address::with_last_byte(1).to_string();
        let usdc = Address::with_last_byte(2).to_string();
        let resp = app
            .oneshot(post_body(json!({
                "token_in": eth,
                "token_out": usdc,
                "amount_in": "not-a-number"
            })))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
