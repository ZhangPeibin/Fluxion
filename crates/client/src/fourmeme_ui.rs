//! Local read-only browser UI for Four.meme Agent ID Exclusive tokens.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alloy_primitives::Address;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use tokio::sync::Mutex;

use crate::capability::{CapabilityError, CapabilityErrorCode};
use crate::fourmeme::{self, FourMemeConfig, ListRequest};
use fluxion_protocol::fourmeme::{FourMemeAgentTokensResponse, FourMemeTokenDetail};

const INDEX_HTML: &str = include_str!("../assets/fourmeme/index.html");
const APP_JS: &str = include_str!("../assets/fourmeme/app.js");
const STYLES_CSS: &str = include_str!("../assets/fourmeme/styles.css");
const LIST_TTL: Duration = Duration::from_secs(30);
const DETAIL_TTL: Duration = Duration::from_secs(60);
const MAX_LOOKBACK_BLOCKS: u64 = 5_000_000;
const CSP: &str = "default-src 'self'; img-src 'self' https: data:; style-src 'self'; script-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'";

#[derive(Debug, Clone)]
pub struct UiOptions {
    pub port: u16,
    pub lookback_blocks: u64,
    pub wallet: Option<Address>,
    pub open: bool,
}

type DetailCache = HashMap<(Address, Option<Address>), CachedDetail>;

#[derive(Clone)]
struct UiState {
    config: FourMemeConfig,
    defaults: Defaults,
    list_cache: Arc<Mutex<Option<CachedList>>>,
    detail_cache: Arc<Mutex<DetailCache>>,
}

#[derive(Clone, Serialize)]
struct Defaults {
    lookback_blocks: u64,
    wallet: Option<Address>,
}

struct CachedList {
    at: Instant,
    lookback_blocks: u64,
    wallet: Option<Address>,
    response: FourMemeAgentTokensResponse,
}

struct CachedDetail {
    at: Instant,
    detail: FourMemeTokenDetail,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    lookback_blocks: Option<u64>,
    wallet: Option<String>,
    refresh: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenQuery {
    wallet: Option<String>,
    refresh: Option<String>,
}

#[derive(Debug)]
struct ApiError(CapabilityError);

impl From<CapabilityError> for ApiError {
    fn from(error: CapabilityError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0.code {
            CapabilityErrorCode::InvalidArgument => StatusCode::BAD_REQUEST,
            CapabilityErrorCode::Timeout => StatusCode::GATEWAY_TIMEOUT,
            CapabilityErrorCode::RpcError => StatusCode::BAD_GATEWAY,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = json!({
            "error": {
                "code": self.0.code.as_str(),
                "message": self.0.message,
                "retryable": self.0.retryable,
            }
        });
        (status, Json(body)).into_response()
    }
}

pub async fn serve(options: UiOptions) -> Result<(), CapabilityError> {
    validate_lookback(options.lookback_blocks)?;
    let state = UiState {
        config: FourMemeConfig::from_env(),
        defaults: Defaults {
            lookback_blocks: options.lookback_blocks,
            wallet: options.wallet,
        },
        list_cache: Arc::new(Mutex::new(None)),
        detail_cache: Arc::new(Mutex::new(HashMap::new())),
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(script))
        .route("/styles.css", get(styles))
        .route("/api/config", get(config))
        .route("/api/tokens", get(list_tokens))
        .route("/api/token/{address}", get(token_detail))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], options.port)))
        .await
        .map_err(|error| {
            CapabilityError::internal(format!("cannot bind 127.0.0.1:{}: {error}", options.port))
        })?;
    let address = listener.local_addr().map_err(|error| {
        CapabilityError::internal(format!("cannot read local UI address: {error}"))
    })?;
    let url = format!("http://{address}");
    println!("Four.meme token UI: {url}");
    println!("Read-only local page. Press Ctrl-C to stop.");
    if options.open {
        open_browser(&url);
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|error| CapabilityError::internal(format!("local UI stopped: {error}")))
}

fn validate_lookback(lookback_blocks: u64) -> Result<(), CapabilityError> {
    if lookback_blocks == 0 || lookback_blocks > MAX_LOOKBACK_BLOCKS {
        return Err(CapabilityError::invalid_argument(
            "lookback_blocks must be between 1 and 5000000",
        ));
    }
    Ok(())
}

async fn index() -> Response {
    let mut response = text_response(INDEX_HTML, "text/html; charset=utf-8");
    let headers = response.headers_mut();
    headers.insert(
        "content-security-policy",
        header::HeaderValue::from_static(CSP),
    );
    headers.insert(
        "x-content-type-options",
        header::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "referrer-policy",
        header::HeaderValue::from_static("no-referrer"),
    );
    response
}

async fn script() -> Response {
    text_response(APP_JS, "application/javascript; charset=utf-8")
}

async fn styles() -> Response {
    text_response(STYLES_CSS, "text/css; charset=utf-8")
}

fn text_response(body: &'static str, content_type: &'static str) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .header("x-content-type-options", "nosniff")
        .body(Body::from(body))
        .expect("static response is valid")
}

async fn config(State(state): State<UiState>) -> Json<Defaults> {
    Json(state.defaults)
}

async fn list_tokens(
    State(state): State<UiState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<FourMemeAgentTokensResponse>, ApiError> {
    let lookback_blocks = query
        .lookback_blocks
        .unwrap_or(state.defaults.lookback_blocks);
    validate_lookback(lookback_blocks)?;
    let wallet = parse_wallet(query.wallet.as_deref(), state.defaults.wallet)?;
    let refresh = query
        .refresh
        .as_deref()
        .is_some_and(|value| value == "1" || value == "true");

    if !refresh {
        let cache = state.list_cache.lock().await;
        if let Some(cached) = cache.as_ref()
            && cached.at.elapsed() < LIST_TTL
            && cached.lookback_blocks == lookback_blocks
            && cached.wallet == wallet
        {
            return Ok(Json(cached.response.clone()));
        }
    }

    let response = fourmeme::agent_tokens(
        &state.config,
        ListRequest {
            wallet,
            limit: 50,
            lookback_blocks,
        },
    )
    .await?;
    *state.list_cache.lock().await = Some(CachedList {
        at: Instant::now(),
        lookback_blocks,
        wallet,
        response: response.clone(),
    });
    Ok(Json(response))
}

async fn token_detail(
    State(state): State<UiState>,
    Path(address): Path<String>,
    Query(query): Query<TokenQuery>,
) -> Result<Json<FourMemeTokenDetail>, ApiError> {
    let address = address
        .parse::<Address>()
        .map_err(|_| CapabilityError::invalid_argument("invalid token address"))?;
    let wallet = parse_wallet(query.wallet.as_deref(), state.defaults.wallet)?;
    let refresh = query
        .refresh
        .as_deref()
        .is_some_and(|value| value == "1" || value == "true");
    let key = (address, wallet);

    if !refresh {
        let cache = state.detail_cache.lock().await;
        if let Some(cached) = cache.get(&key)
            && cached.at.elapsed() < DETAIL_TTL
        {
            return Ok(Json(cached.detail.clone()));
        }
    }

    let detail = fourmeme::token(&state.config, address, wallet).await?;
    state.detail_cache.lock().await.insert(
        key,
        CachedDetail {
            at: Instant::now(),
            detail: detail.clone(),
        },
    );
    Ok(Json(detail))
}

fn parse_wallet(
    value: Option<&str>,
    fallback: Option<Address>,
) -> Result<Option<Address>, ApiError> {
    match value {
        Some("") => Ok(None),
        Some(value) => value
            .parse::<Address>()
            .map(Some)
            .map_err(|_| ApiError(CapabilityError::invalid_argument("invalid wallet address"))),
        None => Ok(fallback),
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let command = "open";
    #[cfg(target_os = "windows")]
    let command = "explorer";
    #[cfg(all(unix, not(target_os = "macos")))]
    let command = "xdg-open";

    if let Err(error) = std::process::Command::new(command).arg(url).spawn() {
        eprintln!("warning: could not open browser: {error}");
    }
}
