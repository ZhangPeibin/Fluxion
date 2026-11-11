//! Deterministic pre-sign transaction inspection.
//!
//! The first slice decodes calldata and pins a block. It deliberately does not
//! claim safety: metadata, simulation and policy are reported as not run.

use alloy_primitives::{Address, U256};

use crate::capability::{Amount, CapabilityError};
use crate::chain::Chain;
use crate::transaction::{Rpc, normalize_data};
use crate::wallet::WalletConfig;

pub use fluxion_protocol::transaction::{
    TransactionInspectArg, TransactionInspectCoverage, TransactionInspectEffects,
    TransactionInspectIntent, TransactionInspectResponse, TransactionInspectSource,
    TransactionInspectTarget,
};

#[derive(Debug, Clone)]
pub struct InspectRequest {
    pub chain: Chain,
    pub from: Option<Address>,
    pub to: Address,
    pub data: String,
    pub value: U256,
    pub block: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
enum ArgType {
    Address,
    Uint,
    Uint8,
    Bool,
    Bytes32,
}

#[derive(Debug, Clone, Copy)]
struct ArgSpec {
    name: &'static str,
    kind: &'static str,
    arg_type: ArgType,
}

const fn arg(name: &'static str, kind: &'static str, arg_type: ArgType) -> ArgSpec {
    ArgSpec {
        name,
        kind,
        arg_type,
    }
}

const TRANSFER: [ArgSpec; 2] = [
    arg("to", "address", ArgType::Address),
    arg("amount", "uint256", ArgType::Uint),
];
const TRANSFER_FROM: [ArgSpec; 3] = [
    arg("from", "address", ArgType::Address),
    arg("to", "address", ArgType::Address),
    arg("amount", "uint256", ArgType::Uint),
];
const APPROVE: [ArgSpec; 2] = [
    arg("spender", "address", ArgType::Address),
    arg("amount", "uint256", ArgType::Uint),
];
const ALLOWANCE_CHANGE: [ArgSpec; 2] = [
    arg("spender", "address", ArgType::Address),
    arg("addedValue", "uint256", ArgType::Uint),
];
const SET_APPROVAL_FOR_ALL: [ArgSpec; 2] = [
    arg("operator", "address", ArgType::Address),
    arg("approved", "bool", ArgType::Bool),
];
const SAFE_TRANSFER_FROM: [ArgSpec; 3] = [
    arg("from", "address", ArgType::Address),
    arg("to", "address", ArgType::Address),
    arg("tokenId", "uint256", ArgType::Uint),
];
const WITHDRAW: [ArgSpec; 1] = [arg("wad", "uint256", ArgType::Uint)];
const PERMIT: [ArgSpec; 7] = [
    arg("owner", "address", ArgType::Address),
    arg("spender", "address", ArgType::Address),
    arg("value", "uint256", ArgType::Uint),
    arg("deadline", "uint256", ArgType::Uint),
    arg("v", "uint8", ArgType::Uint8),
    arg("r", "bytes32", ArgType::Bytes32),
    arg("s", "bytes32", ArgType::Bytes32),
];

struct DecodedIntent {
    intent: TransactionInspectIntent,
    decode_coverage: &'static str,
    unknowns: Vec<String>,
    warnings: Vec<String>,
}

pub async fn inspect(
    config: &WalletConfig,
    request: InspectRequest,
) -> Result<TransactionInspectResponse, CapabilityError> {
    let data = normalize_data(&request.data)?;
    let rpc = Rpc::new(&config.rpc_url_for(request.chain));
    let (block_number, block_hash, _) = rpc.block(request.block).await?;
    let decoded = decode_intent(&data)?;
    let mut unknowns = decoded.unknowns;
    unknowns.push("asset effects were not simulated".to_string());
    unknowns.push("target metadata was not inspected".to_string());
    unknowns.push("no policy was configured".to_string());

    Ok(TransactionInspectResponse {
        chain: request.chain.as_str().to_string(),
        chain_id: request.chain.chain_id(),
        block_number,
        block_hash,
        from: request.from,
        to: request.to,
        value: Amount::from_raw(request.value, 18),
        intent: decoded.intent,
        target: TransactionInspectTarget {
            address: request.to,
            kind: "unknown".to_string(),
        },
        effects: TransactionInspectEffects::default(),
        findings: Vec::new(),
        risk: "unknown".to_string(),
        coverage: TransactionInspectCoverage {
            decode: decoded.decode_coverage.to_string(),
            metadata: "not_run".to_string(),
            simulation: "not_run".to_string(),
            policy: "not_configured".to_string(),
        },
        unknowns,
        sources: vec![
            TransactionInspectSource {
                name: "calldata-decoder".to_string(),
                scope: "intent".to_string(),
            },
            TransactionInspectSource {
                name: format!("{}-rpc", request.chain.as_str()),
                scope: "block-pinning".to_string(),
            },
        ],
        warnings: decoded.warnings,
    })
}

fn decode_intent(data: &str) -> Result<DecodedIntent, CapabilityError> {
    let bytes = hex_to_bytes(data)?;
    if bytes.is_empty() {
        return Ok(DecodedIntent {
            intent: TransactionInspectIntent {
                selector: "0x".to_string(),
                signature: None,
                name: Some("native_transfer".to_string()),
                args: Vec::new(),
            },
            decode_coverage: "complete",
            unknowns: Vec::new(),
            warnings: Vec::new(),
        });
    }
    if bytes.len() < 4 {
        return Err(CapabilityError::invalid_argument(
            "calldata must be empty or contain a 4-byte selector",
        ));
    }

    let selector = format!("0x{}", data_encoding::HEXLOWER.encode(&bytes[..4]));
    let payload = &bytes[4..];
    let (signature, name, specs) = match selector.as_str() {
        "0xa9059cbb" => ("transfer(address,uint256)", "transfer", &TRANSFER[..]),
        "0x23b872dd" => (
            "transferFrom(address,address,uint256)",
            "transferFrom",
            &TRANSFER_FROM[..],
        ),
        "0x095ea7b3" => ("approve(address,uint256)", "approve", &APPROVE[..]),
        "0x39509351" => (
            "increaseAllowance(address,uint256)",
            "increaseAllowance",
            &ALLOWANCE_CHANGE[..],
        ),
        "0xa457c2d7" => (
            "decreaseAllowance(address,uint256)",
            "decreaseAllowance",
            &ALLOWANCE_CHANGE[..],
        ),
        "0xa22cb465" => (
            "setApprovalForAll(address,bool)",
            "setApprovalForAll",
            &SET_APPROVAL_FOR_ALL[..],
        ),
        "0x42842e0e" => (
            "safeTransferFrom(address,address,uint256)",
            "safeTransferFrom",
            &SAFE_TRANSFER_FROM[..],
        ),
        "0xd0e30db0" => ("deposit()", "deposit", &[][..]),
        "0x2e1a7d4d" => ("withdraw(uint256)", "withdraw", &WITHDRAW[..]),
        "0xd505accf" => (
            "permit(address,address,uint256,uint256,uint8,bytes32,bytes32)",
            "permit",
            &PERMIT[..],
        ),
        "0xb88d4fde" => {
            return Ok(dynamic_intent(
                &selector,
                "safeTransferFrom(address,address,uint256,bytes)",
                "safeTransferFrom",
                "dynamic arguments are not decoded in this slice",
            ));
        }
        "0xac9650d8" => {
            return Ok(dynamic_intent(
                &selector,
                "multicall(bytes[])",
                "multicall",
                "multicall arguments are not decoded in this slice",
            ));
        }
        _ => {
            return Ok(DecodedIntent {
                intent: TransactionInspectIntent {
                    selector: selector.clone(),
                    signature: None,
                    name: None,
                    args: Vec::new(),
                },
                decode_coverage: "partial",
                unknowns: vec![format!(
                    "selector {selector} is not in the built-in ABI registry"
                )],
                warnings: Vec::new(),
            });
        }
    };

    match decode_args(payload, specs) {
        Ok(args) => Ok(DecodedIntent {
            intent: TransactionInspectIntent {
                selector,
                signature: Some(signature.to_string()),
                name: Some(name.to_string()),
                args,
            },
            decode_coverage: "complete",
            unknowns: Vec::new(),
            warnings: Vec::new(),
        }),
        Err(error) => Ok(DecodedIntent {
            intent: TransactionInspectIntent {
                selector: selector.clone(),
                signature: Some(signature.to_string()),
                name: Some(name.to_string()),
                args: Vec::new(),
            },
            decode_coverage: "partial",
            unknowns: vec![format!(
                "arguments for selector {selector} could not be decoded"
            )],
            warnings: vec![error],
        }),
    }
}

fn dynamic_intent(selector: &str, signature: &str, name: &str, warning: &str) -> DecodedIntent {
    DecodedIntent {
        intent: TransactionInspectIntent {
            selector: selector.to_string(),
            signature: Some(signature.to_string()),
            name: Some(name.to_string()),
            args: Vec::new(),
        },
        decode_coverage: "partial",
        unknowns: vec![format!(
            "arguments for selector {selector} were not decoded"
        )],
        warnings: vec![warning.to_string()],
    }
}

fn decode_args(payload: &[u8], specs: &[ArgSpec]) -> Result<Vec<TransactionInspectArg>, String> {
    let expected = specs.len() * 32;
    if payload.len() < expected {
        return Err(format!(
            "expected {expected} argument bytes, found {}",
            payload.len()
        ));
    }
    if payload.len() > expected {
        return Err(format!(
            "expected {expected} argument bytes, found {} trailing bytes",
            payload.len() - expected
        ));
    }

    specs
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            let word = &payload[index * 32..(index + 1) * 32];
            let value = match spec.arg_type {
                ArgType::Address => decode_address(word)?,
                ArgType::Uint => U256::from_be_slice(word).to_string(),
                ArgType::Uint8 => decode_uint8(word)?,
                ArgType::Bool => decode_bool(word)?,
                ArgType::Bytes32 => format!("0x{}", data_encoding::HEXLOWER.encode(word)),
            };
            Ok(TransactionInspectArg {
                name: spec.name.to_string(),
                kind: spec.kind.to_string(),
                value,
            })
        })
        .collect()
}

fn decode_address(word: &[u8]) -> Result<String, String> {
    if word[..12].iter().any(|byte| *byte != 0) {
        return Err("address argument has nonzero padding".to_string());
    }
    Ok(Address::from_slice(&word[12..]).to_string())
}

fn decode_uint8(word: &[u8]) -> Result<String, String> {
    if word[..31].iter().any(|byte| *byte != 0) {
        return Err("uint8 argument is out of range".to_string());
    }
    Ok(word[31].to_string())
}

fn decode_bool(word: &[u8]) -> Result<String, String> {
    if word[..31].iter().any(|byte| *byte != 0) {
        return Err("bool argument is out of range".to_string());
    }
    match word[31] {
        0 => Ok("false".to_string()),
        1 => Ok("true".to_string()),
        _ => Err("bool argument must be 0 or 1".to_string()),
    }
}

fn hex_to_bytes(data: &str) -> Result<Vec<u8>, CapabilityError> {
    let hex = data
        .strip_prefix("0x")
        .or_else(|| data.strip_prefix("0X"))
        .unwrap_or(data);
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let chars = hex.as_bytes();
    for pair in chars.as_chunks::<2>().0 {
        let high = hex_nibble(pair[0]).ok_or_else(invalid_hex)?;
        let low = hex_nibble(pair[1]).ok_or_else(invalid_hex)?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn invalid_hex() -> CapabilityError {
    CapabilityError::invalid_argument("data must be 0x-prefixed hex")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::extract::Json;
    use axum::routing::post;
    use serde_json::{Value, json};

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

    async fn rpc(Json(body): Json<Value>) -> Json<Value> {
        let result = match body["method"].as_str().unwrap_or_default() {
            "eth_blockNumber" => json!("0x64"),
            "eth_getBlockByNumber" => json!({
                "number": "0x64",
                "hash": "0x0000000000000000000000000000000000000000000000000000000000000064"
            }),
            _ => json!("0x0"),
        };
        Json(json!({"jsonrpc": "2.0", "id": 1, "result": result}))
    }

    async fn base_url() -> String {
        let app = Router::new().route("/rpc", post(rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    fn request(data: &str) -> InspectRequest {
        InspectRequest {
            chain: Chain::Bsc,
            from: None,
            to: "0x0000000000000000000000000000000000000001"
                .parse()
                .unwrap(),
            data: data.to_string(),
            value: U256::ZERO,
            block: None,
        }
    }

    #[tokio::test]
    async fn native_transfer_is_unknown_not_safe() {
        let base = base_url().await;
        let response = inspect(&config(&base), request("0x")).await.unwrap();
        assert_eq!(response.intent.selector, "0x");
        assert_eq!(response.intent.name.as_deref(), Some("native_transfer"));
        assert_eq!(response.risk, "unknown");
        assert_eq!(response.coverage.decode, "complete");
        assert_eq!(response.block_number, 100);
        assert!(response.findings.is_empty());
        assert!(
            response
                .unknowns
                .iter()
                .any(|item| item.contains("not simulated"))
        );
    }

    #[tokio::test]
    async fn decodes_erc20_transfer_args() {
        let base = base_url().await;
        let data = "0xa9059cbb00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003";
        let response = inspect(&config(&base), request(data)).await.unwrap();
        assert_eq!(
            response.intent.signature.as_deref(),
            Some("transfer(address,uint256)")
        );
        assert_eq!(response.intent.args[0].name, "to");
        assert_eq!(
            response.intent.args[0].value,
            "0x0000000000000000000000000000000000000002"
        );
        assert_eq!(response.intent.args[1].value, "3");
        assert_eq!(response.coverage.decode, "complete");
    }

    #[tokio::test]
    async fn unknown_selector_stays_unknown() {
        let base = base_url().await;
        let response = inspect(&config(&base), request("0xdeadbeef"))
            .await
            .unwrap();
        assert!(response.intent.signature.is_none());
        assert!(response.intent.name.is_none());
        assert_eq!(response.risk, "unknown");
        assert_eq!(response.coverage.decode, "partial");
    }

    #[tokio::test]
    async fn rejects_short_calldata() {
        let base = base_url().await;
        let error = inspect(&config(&base), request("0x123456"))
            .await
            .unwrap_err();
        assert_eq!(
            error.code,
            crate::capability::CapabilityErrorCode::InvalidArgument
        );
    }

    #[tokio::test]
    async fn malformed_arguments_are_partial() {
        let base = base_url().await;
        let response = inspect(&config(&base), request("0xa9059cbb00"))
            .await
            .unwrap();
        assert_eq!(response.coverage.decode, "partial");
        assert!(response.intent.args.is_empty());
        assert!(response.warnings[0].contains("expected 64"));
    }
}
