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
        let resp: Value = self
            .http
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| QuoteError::RpcError(e.to_string()))?
            .json()
            .await
            .map_err(|e| QuoteError::RpcError(e.to_string()))?;
        if let Some(err) = resp.get("error") {
            return Err(QuoteError::RpcError(err.to_string()));
        }
        Ok(resp)
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
