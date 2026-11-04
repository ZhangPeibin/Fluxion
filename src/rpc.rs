use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

use alloy_primitives::{Address, B256, keccak256};
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

    /// Runtime bytecode fingerprint for deployment inventory at the same block
    /// tag as state reads. This is not a source-verification oracle by itself.
    pub async fn code_hash(&self, address: Address) -> Result<B256, QuoteError> {
        let response = self
            .post(json!({"jsonrpc":"2.0","id":1,"method":"eth_getCode",
            "params":[address,self.block_tag]}))
            .await?;
        let hex = response["result"]
            .as_str()
            .ok_or_else(|| QuoteError::RpcError("missing bytecode".into()))?;
        let bytes = hex_decode(hex).map_err(QuoteError::RpcError)?;
        if bytes.is_empty() {
            return Err(QuoteError::RpcError("empty runtime bytecode".into()));
        }
        Ok(keccak256(bytes))
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

    /// Read canonical logs for an inclusive block range. The caller is
    /// responsible for pinning range boundaries to a verified head and for
    /// discarding results after a reorg.
    pub async fn get_logs(
        &self,
        addresses: &[Address],
        topics: &[B256],
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<RpcLog>, QuoteError> {
        if addresses.is_empty() {
            return Ok(Vec::new());
        }
        let address = addresses
            .iter()
            .map(|address| format!("{address:#x}"))
            .collect::<Vec<_>>();
        let topic_filter = if topics.is_empty() {
            Vec::new()
        } else {
            vec![
                topics
                    .iter()
                    .map(|topic| format!("{topic:#x}"))
                    .collect::<Vec<_>>(),
            ]
        };
        let value = self
            .post(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "eth_getLogs",
                "params": [{
                    "address": address,
                    "fromBlock": format!("0x{from_block:x}"),
                    "toBlock": format!("0x{to_block:x}"),
                    "topics": topic_filter,
                }]
            }))
            .await?;
        let entries = value["result"]
            .as_array()
            .ok_or_else(|| QuoteError::RpcError("missing logs result".into()))?;
        entries.iter().map(RpcLog::from_value).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcLog {
    pub address: Address,
    pub topics: Vec<B256>,
    pub data: Vec<u8>,
    pub block_number: u64,
    pub removed: bool,
}

impl RpcLog {
    fn from_value(value: &Value) -> Result<Self, QuoteError> {
        let address = value["address"]
            .as_str()
            .ok_or_else(|| QuoteError::RpcError("log missing address".into()))?
            .parse()
            .map_err(|_| QuoteError::RpcError("invalid log address".into()))?;
        let topics = value["topics"]
            .as_array()
            .ok_or_else(|| QuoteError::RpcError("log missing topics".into()))?
            .iter()
            .map(|topic| {
                topic
                    .as_str()
                    .ok_or_else(|| QuoteError::RpcError("invalid log topic".into()))?
                    .parse()
                    .map_err(|_| QuoteError::RpcError("invalid log topic".into()))
            })
            .collect::<Result<Vec<B256>, QuoteError>>()?;
        let data = value["data"]
            .as_str()
            .ok_or_else(|| QuoteError::RpcError("log missing data".into()))
            .and_then(|data| hex_decode(data).map_err(QuoteError::RpcError))?;
        let block_number = value["blockNumber"]
            .as_str()
            .and_then(|number| number.strip_prefix("0x"))
            .and_then(|number| u64::from_str_radix(number, 16).ok())
            .ok_or_else(|| QuoteError::RpcError("invalid log block number".into()))?;
        Ok(Self {
            address,
            topics,
            data,
            block_number,
            removed: value["removed"].as_bool().unwrap_or(false),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockHeader {
    pub number: u64,
    pub hash: B256,
    pub parent_hash: B256,
    pub timestamp: u64,
}
