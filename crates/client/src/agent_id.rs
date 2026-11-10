//! BSC `.ai` Agent ID identity resolution.
//!
//! Primary source is the public SPACE ID Agent ID API. Every result is checked
//! against the BSC registry when possible; the registry is also used when the
//! API is unavailable or lagging behind a recent registration.

use std::env;
use std::time::Duration;

use alloy_primitives::{Address, U256, address, keccak256};
use serde::Deserialize;
use serde_json::Value;

use crate::capability::{CapabilityError, CapabilityErrorCode};
use fluxion_protocol::wallet::{AgentIdQuery, AssetSource, WalletAgentIdResponse};

const DEFAULT_API_URL: &str = "https://agentid.prd.space.id";
const DEFAULT_BSC_RPC_URL: &str = "https://bsc-dataseed.bnbchain.org";
const REGISTRY: Address = address!("60B9262456dE011F7d15a54CdD8c48bAaa8B022F");
const CHAIN_ID: u64 = 56;

#[derive(Debug, Clone)]
pub struct AgentIdConfig {
    pub api_url: String,
    pub bsc_rpc_url: String,
}

impl AgentIdConfig {
    pub fn from_env() -> Self {
        Self {
            api_url: env::var("FLUXION_AGENT_ID_API_URL")
                .unwrap_or_else(|_| DEFAULT_API_URL.to_string()),
            bsc_rpc_url: env::var("FLUXION_BSC_RPC_URL")
                .unwrap_or_else(|_| DEFAULT_BSC_RPC_URL.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum AgentIdLookup {
    Handle(String),
    Address(Address),
}

#[derive(Debug, Clone)]
pub struct AgentIdRequest {
    pub lookup: AgentIdLookup,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiResolution {
    handle: String,
    owner: Address,
    token_id: String,
    tx_hash: String,
    block_number: u64,
    as_of_block: u64,
}

enum ApiResult {
    Found(ApiResolution),
    NotFound { as_of_block: Option<u64> },
    Unavailable(String),
}

#[derive(Debug)]
struct OnchainRecord {
    handle: String,
    owner: Address,
}

pub async fn resolve(
    config: &AgentIdConfig,
    request: AgentIdRequest,
) -> Result<WalletAgentIdResponse, CapabilityError> {
    let query = match &request.lookup {
        AgentIdLookup::Handle(raw) => {
            let handle = normalize_handle(raw)?;
            AgentIdQuery {
                kind: "handle".to_string(),
                value: handle,
            }
        }
        AgentIdLookup::Address(address) => AgentIdQuery {
            kind: "address".to_string(),
            value: format!("{address}"),
        },
    };

    let api = lookup_api(config, &request.lookup).await?;
    let mut sources = Vec::new();
    let mut warnings = Vec::new();

    match api {
        ApiResult::Found(api) => {
            sources.push(api_source());
            let mut response = response_from_api(query, &api);
            let rpc = Rpc::new(&config.bsc_rpc_url);
            match rpc.lookup(&request.lookup, api.as_of_block).await {
                Ok(Some(onchain)) => {
                    sources.push(rpc_source(Some(api.as_of_block), "verify"));
                    response.coverage = "api+onchain".to_string();
                    if onchain.handle != api.handle || onchain.owner != api.owner {
                        warnings.push(
                            "SPACE ID API and BSC registry disagree; using on-chain values"
                                .to_string(),
                        );
                        response.handle = Some(onchain.handle.clone());
                        response.agent_id = Some(format!("{}.ai", onchain.handle));
                        response.owner = Some(onchain.owner);
                        response.token_id = Some(token_id(&onchain.handle));
                    }
                }
                Ok(None) => {
                    warnings.push(
                        "SPACE ID API returned a record, but the BSC registry returned no owner"
                            .to_string(),
                    );
                }
                Err(error) => {
                    warnings.push(format!(
                        "BSC registry verification failed: {}",
                        error.message
                    ));
                }
            }
            response.sources = sources;
            response.warnings = warnings;
            Ok(response)
        }
        ApiResult::NotFound { as_of_block } => {
            sources.push(api_source());
            let rpc = Rpc::new(&config.bsc_rpc_url);
            match rpc.lookup_latest(&request.lookup).await {
                Ok(Some(onchain)) => {
                    sources.push(rpc_source(None, "resolve"));
                    warnings.push("SPACE ID API had no record; resolved on-chain".to_string());
                    Ok(response_from_onchain(
                        query,
                        onchain,
                        as_of_block,
                        "onchain".to_string(),
                        sources,
                        warnings,
                    ))
                }
                Ok(None) => {
                    sources.push(rpc_source(None, "resolve"));
                    Ok(not_found(
                        query,
                        as_of_block,
                        "api+onchain".to_string(),
                        sources,
                        warnings,
                    ))
                }
                Err(error) => {
                    warnings.push(format!("BSC registry fallback failed: {}", error.message));
                    Ok(not_found(
                        query,
                        as_of_block,
                        "api".to_string(),
                        sources,
                        warnings,
                    ))
                }
            }
        }
        ApiResult::Unavailable(message) => {
            warnings.push(format!("SPACE ID API unavailable: {message}"));
            let rpc = Rpc::new(&config.bsc_rpc_url);
            match rpc.lookup_latest(&request.lookup).await {
                Ok(Some(onchain)) => {
                    sources.push(rpc_source(None, "resolve"));
                    Ok(response_from_onchain(
                        query,
                        onchain,
                        None,
                        "onchain".to_string(),
                        sources,
                        warnings,
                    ))
                }
                Ok(None) => Ok(not_found(
                    query,
                    None,
                    "onchain".to_string(),
                    sources,
                    warnings,
                )),
                Err(error) => Err(CapabilityError::new(
                    CapabilityErrorCode::RpcError,
                    format!(
                        "{}; BSC fallback failed: {}",
                        warnings.join("; "),
                        error.message
                    ),
                    true,
                )),
            }
        }
    }
}

fn response_from_api(query: AgentIdQuery, api: &ApiResolution) -> WalletAgentIdResponse {
    let handle = normalize_handle(&api.handle).unwrap_or_else(|_| api.handle.to_ascii_lowercase());
    WalletAgentIdResponse {
        chain: "bsc".to_string(),
        chain_id: CHAIN_ID,
        query,
        found: true,
        agent_id: Some(format!("{handle}.ai")),
        handle: Some(handle),
        owner: Some(api.owner),
        token_id: Some(api.token_id.clone()),
        tx_hash: Some(api.tx_hash.clone()),
        block_number: Some(api.block_number),
        as_of_block: Some(api.as_of_block),
        coverage: "api".to_string(),
        sources: vec![api_source()],
        warnings: Vec::new(),
    }
}

fn response_from_onchain(
    query: AgentIdQuery,
    onchain: OnchainRecord,
    as_of_block: Option<u64>,
    coverage: String,
    sources: Vec<AssetSource>,
    warnings: Vec<String>,
) -> WalletAgentIdResponse {
    WalletAgentIdResponse {
        chain: "bsc".to_string(),
        chain_id: CHAIN_ID,
        query,
        found: true,
        handle: Some(onchain.handle.clone()),
        agent_id: Some(format!("{}.ai", onchain.handle)),
        owner: Some(onchain.owner),
        token_id: Some(token_id(&onchain.handle)),
        tx_hash: None,
        block_number: None,
        as_of_block,
        coverage,
        sources,
        warnings,
    }
}

fn not_found(
    query: AgentIdQuery,
    as_of_block: Option<u64>,
    coverage: String,
    sources: Vec<AssetSource>,
    warnings: Vec<String>,
) -> WalletAgentIdResponse {
    WalletAgentIdResponse {
        chain: "bsc".to_string(),
        chain_id: CHAIN_ID,
        query,
        found: false,
        handle: None,
        agent_id: None,
        owner: None,
        token_id: None,
        tx_hash: None,
        block_number: None,
        as_of_block,
        coverage,
        sources,
        warnings,
    }
}

async fn lookup_api(
    config: &AgentIdConfig,
    lookup: &AgentIdLookup,
) -> Result<ApiResult, CapabilityError> {
    let base = config.api_url.trim_end_matches('/');
    let url = match lookup {
        AgentIdLookup::Handle(handle) => format!("{base}/v1/handles/{}", normalize_handle(handle)?),
        AgentIdLookup::Address(address) => format!("{base}/v1/addresses/{address}"),
    };
    let response = match reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return Ok(ApiResult::Unavailable(format!(
                "SPACE ID Agent ID request failed: {error}"
            )));
        }
    };
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status.is_success() {
        return match serde_json::from_str::<ApiResolution>(&body) {
            Ok(record) => Ok(ApiResult::Found(record)),
            Err(error) => Ok(ApiResult::Unavailable(format!(
                "invalid SPACE ID Agent ID response: {error}"
            ))),
        };
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        let as_of_block = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| value.get("asOfBlock").and_then(Value::as_u64));
        return Ok(ApiResult::NotFound { as_of_block });
    }
    if status == reqwest::StatusCode::BAD_REQUEST {
        return Err(CapabilityError::invalid_argument(format!(
            "SPACE ID Agent ID rejected the query: {body}"
        )));
    }
    Ok(ApiResult::Unavailable(format!("HTTP {status}: {body}")))
}

struct Rpc {
    client: reqwest::Client,
    url: String,
}

impl Rpc {
    fn new(url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.to_string(),
        }
    }

    async fn lookup(
        &self,
        lookup: &AgentIdLookup,
        block: u64,
    ) -> Result<Option<OnchainRecord>, CapabilityError> {
        self.lookup_at(lookup, &format!("0x{block:x}")).await
    }

    async fn lookup_latest(
        &self,
        lookup: &AgentIdLookup,
    ) -> Result<Option<OnchainRecord>, CapabilityError> {
        self.lookup_at(lookup, "latest").await
    }

    async fn lookup_at(
        &self,
        lookup: &AgentIdLookup,
        tag: &str,
    ) -> Result<Option<OnchainRecord>, CapabilityError> {
        match lookup {
            AgentIdLookup::Handle(raw) => {
                let handle = normalize_handle(raw)?;
                let data = encode_call("ownerOfHandle(string)", &[encode_string_argument(&handle)]);
                let result = self.eth_call(&data, tag).await?;
                let owner = decode_address(&result)
                    .ok_or_else(|| rpc("ownerOfHandle returned invalid data"))?;
                if owner == Address::ZERO {
                    Ok(None)
                } else {
                    Ok(Some(OnchainRecord { handle, owner }))
                }
            }
            AgentIdLookup::Address(address) => {
                let data = encode_call("handleOf(address)", &[encode_address_argument(*address)]);
                let result = self.eth_call(&data, tag).await?;
                let handle = decode_string(&result).unwrap_or_default();
                if handle.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(OnchainRecord {
                        handle,
                        owner: *address,
                    }))
                }
            }
        }
    }

    async fn eth_call(&self, data: &[u8], tag: &str) -> Result<Vec<u8>, CapabilityError> {
        let result = self
            .call(
                "eth_call",
                serde_json::json!([
                    {"to": REGISTRY, "data": format!("0x{}", hex(data))},
                    tag
                ]),
            )
            .await?;
        let text = result
            .as_str()
            .ok_or_else(|| rpc("eth_call returned a non-string result"))?;
        unhex(text)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, CapabilityError> {
        let response = self
            .client
            .post(&self.url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params,
            }))
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|error| rpc(format!("BSC RPC request failed: {error}")))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(rpc(format!("BSC RPC returned HTTP {status}: {body}")));
        }
        let value: Value = serde_json::from_str(&body)
            .map_err(|error| rpc(format!("BSC RPC returned invalid JSON: {error}")))?;
        if let Some(error) = value.get("error") {
            return Err(rpc(format!("BSC RPC error: {error}")));
        }
        value
            .get("result")
            .cloned()
            .ok_or_else(|| rpc("BSC RPC response has no result"))
    }
}

fn normalize_handle(value: &str) -> Result<String, CapabilityError> {
    let lower = value.trim().to_ascii_lowercase();
    let handle = lower.strip_suffix(".ai").unwrap_or(&lower);
    if handle.is_empty() || !handle.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(CapabilityError::invalid_argument(
            "Agent ID must be a lowercase alphanumeric handle, e.g. alice.ai",
        ));
    }
    Ok(handle.to_string())
}

fn token_id(handle: &str) -> String {
    U256::from_be_slice(keccak256(handle.as_bytes()).as_slice()).to_string()
}

fn encode_call(signature: &str, arguments: &[Vec<u8>]) -> Vec<u8> {
    let mut out = keccak256(signature.as_bytes())[..4].to_vec();
    for argument in arguments {
        out.extend_from_slice(argument);
    }
    out
}

fn encode_string_argument(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(64 + bytes.len().div_ceil(32) * 32);
    out.extend_from_slice(&word(U256::from(32)));
    out.extend_from_slice(&word(U256::from(bytes.len())));
    out.extend_from_slice(bytes);
    let padding = (32 - bytes.len() % 32) % 32;
    out.extend(std::iter::repeat_n(0u8, padding));
    out
}

fn encode_address_argument(address: Address) -> Vec<u8> {
    let mut out = vec![0u8; 12];
    out.extend_from_slice(address.as_slice());
    out
}

fn word(value: U256) -> [u8; 32] {
    value.to_be_bytes()
}

fn decode_address(data: &[u8]) -> Option<Address> {
    let word = data.get(..32)?;
    Some(Address::from_slice(&word[12..32]))
}

fn decode_string(data: &[u8]) -> Option<String> {
    let offset = U256::from_be_slice(data.get(..32)?).to::<usize>();
    let length = U256::from_be_slice(data.get(offset..offset + 32)?).to::<usize>();
    let start = offset.checked_add(32)?;
    let bytes = data.get(start..start + length)?;
    String::from_utf8(bytes.to_vec()).ok()
}

fn api_source() -> AssetSource {
    AssetSource {
        name: "space-id-agent-id-api".to_string(),
        scope: "agent-id".to_string(),
        block: None,
    }
}

fn rpc_source(block: Option<u64>, scope: &str) -> AssetSource {
    AssetSource {
        name: "bsc-rpc".to_string(),
        scope: scope.to_string(),
        block,
    }
}

fn hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(bytes)
}

fn unhex(value: &str) -> Result<Vec<u8>, CapabilityError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    data_encoding::HEXLOWER_PERMISSIVE
        .decode(value.as_bytes())
        .map_err(|_| rpc("invalid hex from BSC RPC"))
}

fn rpc(message: impl Into<String>) -> CapabilityError {
    CapabilityError::new(CapabilityErrorCode::RpcError, message, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_agent_id_handles() {
        assert_eq!(normalize_handle("Alice.AI").unwrap(), "alice");
        assert_eq!(normalize_handle(" alice.ai ").unwrap(), "alice");
        assert!(normalize_handle("alice!").is_err());
        assert!(normalize_handle(".ai").is_err());
    }

    #[test]
    fn encodes_a_dynamic_string_argument() {
        let encoded = encode_string_argument("alice");
        assert_eq!(encoded.len(), 96);
        assert_eq!(U256::from_be_slice(&encoded[..32]), U256::from(32));
        assert_eq!(U256::from_be_slice(&encoded[32..64]), U256::from(5));
        assert_eq!(&encoded[64..69], b"alice");
    }

    #[test]
    fn decodes_a_dynamic_string() {
        let mut data = Vec::new();
        data.extend_from_slice(&word(U256::from(32)));
        data.extend_from_slice(&word(U256::from(5)));
        data.extend_from_slice(b"alice");
        data.extend_from_slice(&[0u8; 27]);
        assert_eq!(decode_string(&data).as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn resolves_handle_with_api_then_verifies_onchain() {
        use axum::Router;
        use axum::extract::Json;
        use axum::routing::{get, post};

        async fn api() -> Json<serde_json::Value> {
            Json(serde_json::json!({
                "handle": "alice",
                "owner": "0xa506ac7069937571dc56c5f8fda53c4a729456c6",
                "tokenId": "70564938991660933374592024341600875602376452319261984317470407481576058979585",
                "txHash": "0x8299fc2d71127c4a307bae1d87eab0abb887db11f5f2215f40b391e4ba7da124",
                "blockNumber": 128745991,
                "asOfBlock": 128800000
            }))
        }

        async fn rpc(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
            let result = if body["method"] == "eth_call" {
                let mut word = [0u8; 32];
                word[12..].copy_from_slice(
                    "0xa506ac7069937571dc56c5f8fda53c4a729456c6"
                        .parse::<Address>()
                        .unwrap()
                        .as_slice(),
                );
                format!("0x{}", hex(&word))
            } else {
                "0x0".to_string()
            };
            Json(serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": result}))
        }

        let app = Router::new()
            .route("/v1/handles/alice", get(api))
            .route("/rpc", post(rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let config = AgentIdConfig {
            api_url: format!("http://{address}"),
            bsc_rpc_url: format!("http://{address}/rpc"),
        };
        let response = resolve(
            &config,
            AgentIdRequest {
                lookup: AgentIdLookup::Handle("alice.ai".to_string()),
            },
        )
        .await
        .unwrap();
        assert!(response.found);
        assert_eq!(response.agent_id.as_deref(), Some("alice.ai"));
        assert_eq!(response.coverage, "api+onchain");
        assert_eq!(response.sources.len(), 2);
        assert!(response.warnings.is_empty());
    }

    #[tokio::test]
    #[ignore = "uses live SPACE ID and BSC RPC"]
    async fn resolves_alice_on_mainnet() {
        let config = AgentIdConfig::from_env();
        let response = resolve(
            &config,
            AgentIdRequest {
                lookup: AgentIdLookup::Handle("alice.ai".to_string()),
            },
        )
        .await
        .unwrap();
        assert!(response.found);
        assert_eq!(
            response.owner,
            Some(
                "0xa506ac7069937571dc56c5f8fda53c4a729456c6"
                    .parse()
                    .unwrap()
            )
        );
    }

    #[test]
    fn derives_the_registry_token_id() {
        assert_eq!(
            token_id("alice"),
            "70564938991660933374592024341600875602376452319261984317470407481576058979585"
        );
    }
}
