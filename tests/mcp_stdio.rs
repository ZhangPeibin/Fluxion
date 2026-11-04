//! End-to-end MCP stdio smoke test against a mock warm runtime.

use std::process::Stdio;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

async fn mock_runtime() -> (String, tokio::task::JoinHandle<()>) {
    let app = axum::Router::new()
        .route("/ready", axum::routing::get(|| async { "ok" }))
        .route(
            "/quote",
            axum::routing::post(|| async {
                axum::Json(json!({
                    "amount_out": "5000000000000000",
                    "price_impact_bps": 3,
                    "path": [],
                    "plan_kind": "baseline",
                    "allocations": null,
                    "baseline_amount_out": "5000000000000000",
                    "quote_block": 100,
                    "state": {
                        "pricing_mode": "local_snapshot",
                        "block_hash": "0x0000000000000000000000000000000000000000000000000000000000000064",
                        "parent_hash": "0x0000000000000000000000000000000000000000000000000000000000000063",
                        "block_timestamp": 1,
                        "state_age_secs": 1,
                        "lag_blocks": 0
                    }
                }))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), server)
}

async fn request(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    value: Value,
) -> Value {
    let mut line = serde_json::to_string(&value).unwrap();
    line.push('\n');
    stdin.write_all(line.as_bytes()).await.unwrap();
    stdin.flush().await.unwrap();
    let mut response = String::new();
    stdout.read_line(&mut response).await.unwrap();
    serde_json::from_str(&response).unwrap()
}

#[tokio::test]
async fn mcp_stdio_initializes_lists_and_calls_swap_quote() {
    let (url, server) = mock_runtime().await;
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_fluxion"))
        .args(["mcp", "--url", &url, "--no-auto-start"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let initialize = request(
        &mut stdin,
        &mut stdout,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}
        }),
    )
    .await;
    assert_eq!(initialize["result"]["serverInfo"]["name"], "fluxion");

    // Notifications never receive a response.
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .unwrap();
    stdin.flush().await.unwrap();

    let tools = request(
        &mut stdin,
        &mut stdout,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
    )
    .await;
    assert_eq!(tools["result"]["tools"][0]["name"], "swap_quote");

    let called = request(
        &mut stdin,
        &mut stdout,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "swap_quote",
                "arguments": {"token_in": "LDO", "token_out": "ETH", "amount": "100"}
            }
        }),
    )
    .await;
    assert_eq!(called["result"]["isError"], false);
    let text = called["result"]["content"][0]["text"].as_str().unwrap();
    let capability: Value = serde_json::from_str(text).unwrap();
    assert_eq!(capability["capability"], "swap.quote");
    assert_eq!(capability["amount_out"]["raw"], "5000000000000000");
    assert_eq!(capability["provenance"]["provider"], "local-mcp");
    assert_eq!(capability["provenance"]["quote_block"], 100);

    let remote = request(
        &mut stdin,
        &mut stdout,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "swap_quote",
                "arguments": {
                    "token_in": "LDO",
                    "token_out": "ETH",
                    "amount": "100",
                    "provider": "remote"
                }
            }
        }),
    )
    .await;
    assert_eq!(remote["result"]["isError"], true);
    assert!(
        remote["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("UNSUPPORTED")
    );

    child.kill().await.unwrap();
    server.abort();
}
