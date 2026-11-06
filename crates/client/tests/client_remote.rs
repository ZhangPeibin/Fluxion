//! Remote backend client mapping and auth smoke test.

use std::sync::{Arc, Mutex};

use alloy_primitives::{Address, U256};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use fluxion_client::capability::TokenRef;
use fluxion_client::client::RuntimeClient;
use fluxion_client::swap::quote::QuoteRequest;
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
            "remote",
        )
        .await
        .unwrap();
    assert_eq!(response.provenance.execution, "remote");
    assert_eq!(response.provenance.quote_block, Some(123));
    assert_eq!(response.amount_out.raw, "5000000000000000");

    let (auth, body) = capture.0.lock().unwrap().clone().unwrap();
    assert_eq!(auth.as_deref(), Some("Bearer test-key"));
    assert_eq!(body["split"], true);
    assert_eq!(body["amount_in"], "10");

    server.abort();
}

async fn shared_prefix_quote(Json(_body): Json<Value>) -> Json<Value> {
    Json(json!({
        "amount_out": "1805222176213714092",
        "price_impact_bps": null,
        "path": [],
        "plan_kind": "split",
        "allocations": [{
            "amount_in": "50000000000000000000000",
            "amount_out": "1805222176213714092",
            "input_share_percent": 5.0,
            "path": [{
                "operation": "swap",
                "platform": "uniswap-v3",
                "pool": "0xBde1C0D9e1EaF3e53d8CE62De6FD0CF7F7C63bAb",
                "i": 0,
                "j": 1,
                "token_in": "0x0000000000000000000000000000000000000001",
                "token_out": "0x0000000000000000000000000000000000000003",
                "amount_in": "50000000000000000000000",
                "amount_out": "31105779369920438825"
            }],
            "children": [{
                "amount_in": "15552889684960219412",
                "amount_out": "902316368274594981",
                "input_share_percent": 50.0,
                "path": [{
                    "operation": "swap",
                    "platform": "uniswap-v3",
                    "pool": "0x5aB53EE1d50eeF2C1DD3d5402789cd27bB52c1bB",
                    "i": 0,
                    "j": 1,
                    "token_in": "0x0000000000000000000000000000000000000003",
                    "token_out": "0x0000000000000000000000000000000000000002",
                    "amount_in": "15552889684960219412",
                    "amount_out": "902316368274594981"
                }]
            }, {
                "amount_in": "15552889684960219413",
                "amount_out": "902905807939119111",
                "input_share_percent": 50.0,
                "path": [{
                    "operation": "swap",
                    "platform": "uniswap-v3",
                    "pool": "0x4674abc5796e1334B5075326b39B748bee9EaA34",
                    "i": 0,
                    "j": 1,
                    "token_in": "0x0000000000000000000000000000000000000003",
                    "token_out": "0x0000000000000000000000000000000000000002",
                    "amount_in": "15552889684960219413",
                    "amount_out": "902905807939119111"
                }]
            }]
        }],
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
async fn remote_client_maps_nested_shared_prefix_allocations() {
    let app = Router::new().route("/quote", post(shared_prefix_quote));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = RuntimeClient::with_api_key(format!("http://{address}"), None);
    let response = client
        .swap_quote(
            &QuoteRequest {
                token_in: Address::with_last_byte(1),
                token_out: Address::with_last_byte(2),
                amount_in: U256::from(1_000_000),
                max_hops: None,
                amount_out_min: None,
                split: true,
                split_max_branches: None,
            },
            TokenRef::new(Some("LDO".into()), Address::with_last_byte(1), 18),
            TokenRef::new(Some("WETH".into()), Address::with_last_byte(2), 18),
            "ethereum",
            "remote",
        )
        .await
        .unwrap();

    assert_eq!(response.allocations.len(), 1);
    let root = &response.allocations[0];
    assert_eq!(root.hops.len(), 1);
    assert_eq!(root.children.len(), 2);
    assert_eq!(
        root.children
            .iter()
            .map(|child| child.amount_out.raw.as_str())
            .collect::<Vec<_>>(),
        vec!["902316368274594981", "902905807939119111"]
    );
    assert!(
        root.children
            .iter()
            .all(|child| child.hops.len() == 1 && child.hops[0].token_out.ends_with("0002"))
    );

    server.abort();
}
