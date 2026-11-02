use alloy_primitives::Address;
use axum::Json;
use fluxion::rpc::RpcClient;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn rpc_counter_includes_retries_and_is_shared_with_pinned_clones() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = attempts.clone();
    let app = axum::Router::new().route(
        "/",
        axum::routing::post(move |Json(request): Json<Value>| {
            let counter = counter.clone();
            async move {
                assert_eq!(request["params"][1], "0x7b");
                if counter.fetch_add(1, Ordering::Relaxed) == 0 {
                    Json(json!({"error":{"code":429,"message":"rate limited"}}))
                } else {
                    Json(json!({"result":"0x01"}))
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc = RpcClient::new(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    assert_eq!(rpc.request_count(), 0);
    assert_eq!(
        rpc.clone()
            .at_block(123)
            .eth_call(Address::with_last_byte(1), "12345678")
            .await
            .unwrap(),
        vec![1]
    );
    assert_eq!(rpc.request_count(), 2);
    assert_eq!(attempts.load(Ordering::Relaxed), 2);
    server.abort();
}
