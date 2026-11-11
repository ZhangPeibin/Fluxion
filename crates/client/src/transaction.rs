//! Local read-only EVM transaction simulation and shared JSON-RPC helpers.

use alloy_primitives::{Address, B256, U256};
use serde_json::{Value, json};

use crate::capability::{Amount, CapabilityError};
use crate::chain::Chain;
use crate::wallet::WalletConfig;

pub use fluxion_protocol::transaction::TransactionSimulateResponse;

#[derive(Debug, Clone)]
pub struct SimulateRequest {
    pub chain: Chain,
    pub from: Option<Address>,
    pub to: Address,
    pub data: String,
    pub value: U256,
    pub block: Option<u64>,
}

pub async fn simulate(
    config: &WalletConfig,
    request: SimulateRequest,
) -> Result<TransactionSimulateResponse, CapabilityError> {
    let data = normalize_data(&request.data)?;
    let rpc = Rpc::new(&config.rpc_url_for(request.chain));
    let (block_number, block_hash, tag) = rpc.block(request.block).await?;
    let mut tx = json!({
        "to": request.to,
        "data": data,
    });
    if let Some(from) = request.from {
        tx["from"] = json!(from);
    }
    if !request.value.is_zero() {
        tx["value"] = json!(format!("0x{:x}", request.value));
    }
    let call = rpc.call("eth_call", json!([tx, tag])).await;
    match call {
        Ok(result) => {
            let return_data = result.as_str().map(str::to_string).ok_or_else(|| {
                CapabilityError::new(
                    crate::capability::CapabilityErrorCode::RpcError,
                    "eth_call returned a non-string result",
                    true,
                )
            })?;
            let gas_estimate = rpc
                .call("eth_estimateGas", json!([tx, tag]))
                .await
                .ok()
                .and_then(|value| value.as_str().and_then(decode_hex_u256))
                .map(|value| value.to_string());
            Ok(TransactionSimulateResponse {
                chain: request.chain.as_str().to_string(),
                chain_id: request.chain.chain_id(),
                block_number,
                block_hash,
                from: request.from,
                to: request.to,
                value: Amount::from_raw(request.value, 18),
                success: true,
                return_data,
                gas_estimate,
                revert_data: None,
                error: None,
            })
        }
        Err(error) => Ok(TransactionSimulateResponse {
            chain: request.chain.as_str().to_string(),
            chain_id: request.chain.chain_id(),
            block_number,
            block_hash,
            from: request.from,
            to: request.to,
            value: Amount::from_raw(request.value, 18),
            success: false,
            return_data: "0x".to_string(),
            gas_estimate: None,
            revert_data: error.data,
            error: Some(error.message),
        }),
    }
}

pub(crate) fn normalize_data(data: &str) -> Result<String, CapabilityError> {
    let data = data.trim();
    let Some(hex) = data.strip_prefix("0x").or_else(|| data.strip_prefix("0X")) else {
        return Err(CapabilityError::invalid_argument(
            "data must be 0x-prefixed hex",
        ));
    };
    if !hex.len().is_multiple_of(2) || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(CapabilityError::invalid_argument(
            "data must be 0x-prefixed hex",
        ));
    }
    Ok(format!("0x{}", hex.to_ascii_lowercase()))
}

fn decode_hex_u256(value: &str) -> Option<U256> {
    U256::from_str_radix(value.trim_start_matches("0x"), 16).ok()
}

pub(crate) struct Rpc {
    client: reqwest::Client,
    url: String,
}

struct RpcFailure {
    message: String,
    data: Option<String>,
}

impl Rpc {
    pub(crate) fn new(url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.to_string(),
        }
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcFailure> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|error| RpcFailure {
                message: format!("local RPC request failed: {error}"),
                data: None,
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(RpcFailure {
                message: format!("local RPC returned HTTP {status}: {body}"),
                data: None,
            });
        }
        let value: Value = response.json().await.map_err(|error| RpcFailure {
            message: format!("local RPC returned invalid JSON: {error}"),
            data: None,
        })?;
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("JSON-RPC error")
                .to_string();
            return Err(RpcFailure {
                message,
                data: error_data(error),
            });
        }
        value.get("result").cloned().ok_or_else(|| RpcFailure {
            message: "JSON-RPC response has no result".to_string(),
            data: None,
        })
    }

    pub(crate) async fn block(
        &self,
        requested: Option<u64>,
    ) -> Result<(u64, B256, String), CapabilityError> {
        let number = match requested {
            Some(number) => number,
            None => {
                let value = self
                    .call("eth_blockNumber", json!([]))
                    .await
                    .map_err(rpc_failure)?;
                value
                    .as_str()
                    .and_then(decode_hex_u256)
                    .and_then(|value| u64::try_from(value).ok())
                    .ok_or_else(|| {
                        CapabilityError::new(
                            crate::capability::CapabilityErrorCode::RpcError,
                            "eth_blockNumber returned an invalid block",
                            true,
                        )
                    })?
            }
        };
        let tag = format!("0x{number:x}");
        let value = self
            .call("eth_getBlockByNumber", json!([tag, false]))
            .await
            .map_err(rpc_failure)?;
        let hash = value
            .get("hash")
            .and_then(Value::as_str)
            .and_then(|hash| hash.parse::<B256>().ok())
            .ok_or_else(|| {
                CapabilityError::new(
                    crate::capability::CapabilityErrorCode::RpcError,
                    "cannot read block hash",
                    true,
                )
            })?;
        Ok((number, hash, tag))
    }
}

fn error_data(error: &Value) -> Option<String> {
    error.get("data").and_then(|data| {
        data.as_str()
            .map(str::to_string)
            .or_else(|| data.get("data").and_then(Value::as_str).map(str::to_string))
    })
}

fn rpc_failure(error: RpcFailure) -> CapabilityError {
    CapabilityError::new(
        crate::capability::CapabilityErrorCode::RpcError,
        error.message,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::extract::Json;
    use axum::routing::post;

    fn word(value: U256) -> String {
        format!("0x{value:064x}")
    }

    async fn success_rpc(Json(body): Json<Value>) -> Json<Value> {
        let method = body["method"].as_str().unwrap_or_default();
        let result = match method {
            "eth_blockNumber" => json!("0x64"),
            "eth_getBlockByNumber" => json!({
                "number": "0x64",
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000064"
            }),
            "eth_call" => json!(word(U256::from(42u64))),
            "eth_estimateGas" => json!("0x5208"),
            _ => json!("0x0"),
        };
        Json(json!({"jsonrpc": "2.0", "id": 1, "result": result}))
    }

    async fn revert_rpc(Json(body): Json<Value>) -> Json<Value> {
        if body["method"] == "eth_call" {
            return Json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "error": {
                    "code": 3,
                    "message": "execution reverted: Nope",
                    "data": "0x08c379a0"
                }
            }));
        }
        success_rpc(Json(body)).await
    }

    fn config(base: &str) -> WalletConfig {
        WalletConfig {
            rpc_url: format!("{base}/rpc"),
            bsc_rpc_url: format!("{base}/rpc"),
            oneinch_api_key: None,
            oneinch_balance_url: "http://unused".to_string(),
            alchemy_api_key: None,
            alchemy_nft_url: "http://unused".to_string(),
        }
    }

    #[tokio::test]
    async fn successful_call_returns_data_and_gas() {
        let app = Router::new().route("/rpc", post(success_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let response = simulate(
            &config(&format!("http://{address}")),
            SimulateRequest {
                chain: Chain::Ethereum,
                from: None,
                to: "0x0000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
                data: "0x1234".to_string(),
                value: U256::ZERO,
                block: None,
            },
        )
        .await
        .unwrap();
        assert!(response.success);
        assert_eq!(response.return_data, word(U256::from(42u64)));
        assert_eq!(response.gas_estimate.as_deref(), Some("21000"));
        assert_eq!(response.block_number, 100);
    }

    #[tokio::test]
    async fn revert_is_reported_without_transport_error() {
        let app = Router::new().route("/rpc", post(revert_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let response = simulate(
            &config(&format!("http://{address}")),
            SimulateRequest {
                chain: Chain::Ethereum,
                from: None,
                to: "0x0000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
                data: "0x1234".to_string(),
                value: U256::ZERO,
                block: Some(100),
            },
        )
        .await
        .unwrap();
        assert!(!response.success);
        assert_eq!(response.revert_data.as_deref(), Some("0x08c379a0"));
        assert!(response.error.as_deref().unwrap().contains("Nope"));
    }

    #[test]
    fn rejects_non_hex_data() {
        assert!(normalize_data("nope").is_err());
        assert!(normalize_data("0x123").is_err());
        assert_eq!(normalize_data("0xAB").unwrap(), "0xab");
    }
}
