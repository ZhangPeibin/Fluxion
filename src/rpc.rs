use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

use alloy_primitives::{Address, B256};
use serde::Serialize;
use serde_json::{Value, json};

use crate::error::QuoteError;
use crate::util::hex_decode;

#[derive(Clone)]
pub struct RpcClient {
    http: reqwest::Client,
    url: String,
    block_tag: Value,
    requests: Arc<AtomicU64>,
}

impl RpcClient {
    pub fn new(url: String) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("HTTP client"),
            url,
            block_tag: json!("latest"),
            requests: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Actual HTTP JSON-RPC attempts, including retries, shared by client clones.
    pub fn request_count(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }

    /// Pin read-only calls for reproducible adapter diagnostics. Aggregator
    /// endpoints do not support this, so live benchmark services use latest.
    pub fn at_block(mut self, block: u64) -> Self {
        self.block_tag = json!(format!("0x{block:x}"));
        self
    }

    /// EIP-1898: every read belongs to this exact canonical block, even if a
    /// reorg occurs while a background snapshot is being assembled.
    pub fn at_block_hash(mut self, hash: B256) -> Self {
        self.block_tag = json!({"blockHash":hash,"requireCanonical":true});
        self
    }

    pub async fn block_header(&self, number: Option<u64>) -> Result<BlockHeader, QuoteError> {
        let tag = number.map_or_else(|| "latest".to_string(), |n| format!("0x{n:x}"));
        let value = self.post(json!({"jsonrpc":"2.0","id":1,"method":"eth_getBlockByNumber","params":[tag,false]})).await?;
        let result = &value["result"];
        let hex = |key: &str| -> Result<u64, QuoteError> {
            let text = result[key]
                .as_str()
                .and_then(|s| s.strip_prefix("0x"))
                .ok_or_else(|| QuoteError::RpcError(format!("missing block {key}")))?;
            u64::from_str_radix(text, 16)
                .map_err(|_| QuoteError::RpcError(format!("invalid block {key}")))
        };
        let hash = |key: &str| -> Result<B256, QuoteError> {
            result[key]
                .as_str()
                .ok_or_else(|| QuoteError::RpcError(format!("missing block {key}")))?
                .parse()
                .map_err(|_| QuoteError::RpcError(format!("invalid block {key}")))
        };
        let header = BlockHeader {
            number: hex("number")?,
            hash: hash("hash")?,
            parent_hash: hash("parentHash")?,
            timestamp: hex("timestamp")?,
        };
        if number.is_some_and(|n| n != header.number) {
            return Err(QuoteError::RpcError("block number mismatch".into()));
        }
        Ok(header)
    }

    async fn post(&self, body: Value) -> Result<Value, QuoteError> {
        let mut last = String::from("unknown rpc error");
        for attempt in 0..3u32 {
            self.requests.fetch_add(1, Ordering::Relaxed);
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
                    Err(e) => last = e.without_url().to_string(),
                },
                Err(e) => last = e.without_url().to_string(),
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
                "params": [{ "to": format!("{to:#x}"), "data": data }, self.block_tag],
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

    pub async fn block_hash(&self, block: u64) -> Result<String, QuoteError> {
        let value = self.post(json!({"jsonrpc":"2.0", "id":1, "method":"eth_getBlockByNumber", "params":[format!("0x{block:x}"), false]})).await?;
        value["result"]["hash"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| QuoteError::RpcError("missing block hash".into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockHeader {
    pub number: u64,
    pub hash: B256,
    pub parent_hash: B256,
    pub timestamp: u64,
}
