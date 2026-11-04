//! Remote backend client mapping and auth smoke test.

use std::sync::{Arc, Mutex};

use alloy_primitives::{Address, U256};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use fluxion::capability::TokenRef;
use fluxion::client::RuntimeClient;
use fluxion::swap::quote::QuoteRequest;
use serde_json::{Value, json};

type CapturedRequest = Option<(Option<String>, Value)>;

#[derive(Clone)]
struct Capture(Arc<Mutex<CapturedRequest>>);

async fn quote(
    State(capture): State<Capture>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    let auth = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    *capture.0.lock().unwrap() = Some((auth, body));
    Json(json!({
        "amount_out": "5000000000000000",
        "price_impact_bps": 2,
        "path": [],
        "plan_kind": "baseline",
        "allocations": null,
        "quote_block": 123,
        "state": {
            "pricing_mode": "local_snapshot",
            "block_hash": "0x000000000000000000000000000000000000000000000000000000000000007b",
            "parent_hash": "0x000000000000000000000000000000000000000000000000000000000000007a",
            "block_timestamp": 1,
            "state_age_secs": 2,
            "lag_blocks": 0
        }
    }))
}

#[tokio::test]
async fn remote_client_sends_bearer_and_maps_response() {
    let capture = Capture(Arc::new(Mutex::new(None)));
    let app = Router::new()
        .route("/quote", post(quote))
        .with_state(capture.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client =
        RuntimeClient::with_api_key(format!("http://{address}"), Some("test-key".to_string()));
    let request = QuoteRequest {
        token_in: Address::with_last_byte(1),
        token_out: Address::with_last_byte(2),
        amount_in: U256::from(10),
        max_hops: None,
        amount_out_min: None,
        split: true,
        split_max_branches: None,
    };
    let response = client
        .swap_quote(
            &request,
            TokenRef::new(Some("LDO".into()), Address::with_last_byte(1), 18),
            TokenRef::new(Some("WETH".into()), Address::with_last_byte(2), 18),
            "ethereum",
            "fluxion-remote",
        )
        .await
        .unwrap();
    assert_eq!(response.provenance.provider, "fluxion-remote");
    assert_eq!(response.provenance.quote_block, Some(123));
    assert_eq!(response.amount_out.raw, "5000000000000000");

    let (auth, body) = capture.0.lock().unwrap().clone().unwrap();
    assert_eq!(auth.as_deref(), Some("Bearer test-key"));
    assert_eq!(body["split"], true);
    assert_eq!(body["amount_in"], "10");

    server.abort();
}
