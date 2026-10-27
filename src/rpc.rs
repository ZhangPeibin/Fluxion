use std::time::Duration;

use alloy_primitives::Address;
use serde_json::{Value, json};

use crate::error::QuoteError;
use crate::util::hex_decode;

#[derive(Clone)]
pub struct RpcClient {
    http: reqwest::Client,
    url: String,
}

impl RpcClient {
    pub fn new(url: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            url,
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    async fn post(&self, body: Value) -> Result<Value, QuoteError> {
        let mut last = String::from("unknown rpc error");
        for attempt in 0..3u32 {
            let result = self.http.post(&self.url).json(&body).send().await;
            match result {
                Ok(resp) => match resp.json::<Value>().await {
                    Ok(value) => {
                        if let Some(err) = value.get("error") {
                            let msg = err.to_string();
                            let transient = msg.contains("429")
                                || msg.to_lowercase().contains("rate")
                                || msg.to_lowercase().contains("too many");
                            if transient {
                                last = msg;
                            } else {
                                return Err(QuoteError::RpcError(msg));
                            }
                        } else {
                            return Ok(value);
                        }
                    }
                    Err(e) => last = e.to_string(),
                },
                Err(e) => last = e.to_string(),
            }
            tokio::time::sleep(Duration::from_millis(100 * u64::from(attempt + 1))).await;
        }
        Err(QuoteError::RpcError(last))
    }

    pub async fn eth_call(&self, to: Address, data: &str) -> Result<Vec<u8>, QuoteError> {
        let data = if data.starts_with("0x") {
            data.to_string()
        } else {
            format!("0x{data}")
        };
        let resp = self
            .post(json!({
                "jsonrpc": "2.0",
                "method": "eth_call",
                "params": [{ "to": format!("{to:#x}"), "data": data }, "latest"],
                "id": 1
            }))
            .await?;
        let result = resp
            .get("result")
            .and_then(|r| r.as_str())
            .ok_or_else(|| QuoteError::RpcError("missing result".to_string()))?;
        hex_decode(result).map_err(QuoteError::RpcError)
    }

    pub async fn block_number(&self) -> Result<u64, QuoteError> {
        let resp = self
            .post(json!({
                "jsonrpc": "2.0",
                "method": "eth_blockNumber",
                "params": [],
                "id": 1
            }))
            .await?;
        let hex = resp
            .get("result")
            .and_then(|r| r.as_str())
            .ok_or_else(|| QuoteError::RpcError("missing result".to_string()))?;
        u64::from_str_radix(hex.trim_start_matches("0x"), 16)
            .map_err(|e| QuoteError::RpcError(e.to_string()))
    }
}
