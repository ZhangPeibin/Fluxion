use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alloy_primitives::{Address, U256};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router as AxumRouter};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::QuoteError;
use crate::graph::Graph;
use crate::model::{Hop, QuoteRequest, Route};
use crate::router::Router;
use crate::source::QuoteSource;

#[derive(Debug, Deserialize)]
pub struct QuoteRequestDto {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: String,
    #[serde(default)]
    pub max_hops: Option<usize>,
    #[serde(default)]
    pub amount_out_min: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HopDto {
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
    pub price_impact_bps: i64,
    pub path: Vec<HopDto>,
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
    pub source: Box<dyn QuoteSource + Send + Sync>,
    pub router: Router,
    pub limiter: RateLimiter,
}

pub enum ApiError {
    Quote(QuoteError),
    BadRequest(String),
    RateLimited,
}

impl From<QuoteError> for ApiError {
    fn from(e: QuoteError) -> Self {
        ApiError::Quote(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "RateLimited", None),
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
        price_impact_bps: route.price_impact_bps,
        path: route.hops.iter().map(hop_to_dto).collect(),
    }
}

async fn quote(
    State(state): State<Arc<AppState>>,
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

    let route = state
        .router
        .route(&state.graph, state.source.as_ref(), &request)
        .await?;
    Ok(Json(route_to_dto(route)))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

pub fn app(state: Arc<AppState>) -> AxumRouter {
    AxumRouter::new()
        .route("/quote", post(quote))
        .route("/health", get(health))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Pool, PoolKind};
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
            source: Box::new(source),
            router: Router::default(),
            limiter: RateLimiter::new(1000, Duration::from_secs(1)),
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
