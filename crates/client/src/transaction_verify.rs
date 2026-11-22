//! Read-only verification of a sent transaction against its receipt.
//!
//! The capability returns an explicit status (`success`, `reverted`, `pending`, `not_found`) and
//! decoded receipt effects. When the caller supplies a prior `transaction.inspect` report, the
//! actual effects are compared with the inspected ones. It never signs or broadcasts.

use std::collections::BTreeSet;

use alloy_primitives::{Address, B256, U256};
use serde_json::Value;

use crate::capability::{Amount, CapabilityError};
use crate::chain::Chain;
use crate::transaction::Rpc;
use crate::transaction_inspect::decode_log;
use crate::wallet::WalletConfig;
use fluxion_protocol::transaction::{
    TransactionInspectEffects, TransactionInspectSource, TransactionVerifyExpectation,
    TransactionVerifyMismatch, TransactionVerifyResponse,
};

const MAX_MISMATCHES: usize = 50;
const EFFECT_KEYS: &[&str] = &["native", "tokens", "approvals", "nfts"];

pub struct VerifyRequest {
    pub chain: Chain,
    pub tx_hash: String,
    pub confirmations: u64,
    pub expected: Option<TransactionInspectEffects>,
}

pub async fn verify(
    config: &WalletConfig,
    request: VerifyRequest,
) -> Result<TransactionVerifyResponse, CapabilityError> {
    let tx_hash = normalize_hash(&request.tx_hash)?;
    let rpc = Rpc::new(&config.rpc_url_for(request.chain));
    let (head_number, head_hash, _) = rpc.block(None).await?;
    let tx = rpc.transaction_by_hash(&tx_hash).await?;
    let receipt = rpc.receipt(&tx_hash).await?;
    let sources = vec![TransactionInspectSource {
        name: format!("{}-rpc", request.chain.as_str()),
        scope: "tx+receipt".to_string(),
    }];

    let Some(tx) = tx else {
        return Ok(TransactionVerifyResponse {
            chain: request.chain.as_str().to_string(),
            chain_id: request.chain.chain_id(),
            tx_hash,
            status: "not_found".to_string(),
            block_number: head_number,
            block_hash: head_hash,
            from: None,
            to: None,
            value: None,
            confirmations: 0,
            gas_used: None,
            effective_gas_price: None,
            fee: None,
            logs_count: None,
            effects: None,
            expectation: TransactionVerifyExpectation::not_configured(),
            sources,
            warnings: vec!["transaction was not found on this chain".to_string()],
        });
    };

    let from = tx
        .get("from")
        .and_then(Value::as_str)
        .and_then(parse_address);
    let to = tx.get("to").and_then(Value::as_str).and_then(parse_address);
    let value = tx.get("value").and_then(hex_u256).unwrap_or(U256::ZERO);
    let gas_price = tx.get("gasPrice").and_then(hex_u256);

    let Some(receipt) = receipt else {
        return Ok(TransactionVerifyResponse {
            chain: request.chain.as_str().to_string(),
            chain_id: request.chain.chain_id(),
            tx_hash,
            status: "pending".to_string(),
            block_number: head_number,
            block_hash: head_hash,
            from,
            to,
            value: Some(Amount::from_raw(value, 18)),
            confirmations: 0,
            gas_used: None,
            effective_gas_price: None,
            fee: None,
            logs_count: None,
            effects: None,
            expectation: TransactionVerifyExpectation::not_configured(),
            sources,
            warnings: vec!["transaction is not mined yet".to_string()],
        });
    };

    let block_number = receipt
        .get("blockNumber")
        .and_then(hex_u64)
        .unwrap_or(head_number);
    let block_hash = receipt
        .get("blockHash")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<B256>().ok())
        .unwrap_or(head_hash);
    let confirmations = if head_number >= block_number {
        head_number.saturating_sub(block_number) + 1
    } else {
        0
    };
    let gas_used = receipt.get("gasUsed").and_then(hex_u256);
    let effective_gas_price = receipt
        .get("effectiveGasPrice")
        .and_then(hex_u256)
        .or(gas_price);
    let fee = match (gas_used, effective_gas_price) {
        (Some(gas), Some(price)) => Some(Amount::from_raw(gas.saturating_mul(price), 18)),
        _ => None,
    };

    let mut warnings = Vec::new();
    let mut effects = TransactionInspectEffects::default();
    let logs = receipt
        .get("logs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let execution_status = receipt.get("status").and_then(hex_u64);
    if execution_status == Some(1) {
        for log in &logs {
            decode_log(log, &mut effects, &mut warnings);
        }
        if !value.is_zero() {
            if let Some(from) = from {
                effects.native.push(
                    fluxion_protocol::transaction::TransactionInspectNativeEffect {
                        account: from,
                        delta: format!("-{value}"),
                    },
                );
            }
            let recipient = to.or_else(|| {
                receipt
                    .get("contractAddress")
                    .and_then(Value::as_str)
                    .and_then(parse_address)
            });
            if let Some(recipient) = recipient {
                effects.native.push(
                    fluxion_protocol::transaction::TransactionInspectNativeEffect {
                        account: recipient,
                        delta: value.to_string(),
                    },
                );
            }
        }
    } else if execution_status == Some(0) {
        warnings.push("transaction reverted; no effects were applied".to_string());
    }

    let status = if confirmations < request.confirmations {
        warnings.push(format!(
            "transaction has {confirmations} confirmations; {0} required",
            request.confirmations
        ));
        "pending"
    } else {
        match execution_status {
            Some(1) => "success",
            Some(0) => "reverted",
            _ => {
                warnings.push("receipt does not expose an execution status".to_string());
                "pending"
            }
        }
    }
    .to_string();

    let expectation = match request.expected.as_ref() {
        Some(expected) => compare_effects(expected, &effects),
        None => TransactionVerifyExpectation::not_configured(),
    };

    Ok(TransactionVerifyResponse {
        chain: request.chain.as_str().to_string(),
        chain_id: request.chain.chain_id(),
        tx_hash,
        status,
        block_number,
        block_hash,
        from,
        to,
        value: Some(Amount::from_raw(value, 18)),
        confirmations,
        gas_used: gas_used.map(|value| value.to_string()),
        effective_gas_price: effective_gas_price.map(|value| value.to_string()),
        fee,
        logs_count: Some(logs.len() as u64),
        effects: Some(effects),
        expectation,
        sources,
        warnings,
    })
}

/// Extract `effects` from a `transaction.inspect` envelope, a bare `{effects}` object, or an
/// effects object.
pub fn expected_from_value(value: &Value) -> Result<TransactionInspectEffects, CapabilityError> {
    let candidate = value
        .get("data")
        .and_then(|data| data.get("effects"))
        .or_else(|| value.get("effects"))
        .unwrap_or(value);
    let object = candidate.as_object().ok_or_else(|| {
        CapabilityError::invalid_argument("expected report has no effects object")
    })?;
    if !EFFECT_KEYS.iter().any(|key| object.contains_key(*key)) {
        return Err(CapabilityError::invalid_argument(
            "expected report has no effects object",
        ));
    }
    serde_json::from_value(candidate.clone()).map_err(|error| {
        CapabilityError::invalid_argument(format!(
            "expected report has no valid effects object: {error}"
        ))
    })
}

fn compare_effects(
    expected: &TransactionInspectEffects,
    actual: &TransactionInspectEffects,
) -> TransactionVerifyExpectation {
    let mut mismatches = Vec::new();
    compare_set(
        "native",
        canonical_native(expected),
        canonical_native(actual),
        &mut mismatches,
    );
    compare_set(
        "token",
        canonical_tokens(expected),
        canonical_tokens(actual),
        &mut mismatches,
    );
    compare_set(
        "approval",
        canonical_approvals(expected),
        canonical_approvals(actual),
        &mut mismatches,
    );
    compare_set(
        "nft",
        canonical_nfts(expected),
        canonical_nfts(actual),
        &mut mismatches,
    );
    TransactionVerifyExpectation {
        status: if mismatches.is_empty() {
            "match".to_string()
        } else {
            "mismatch".to_string()
        },
        mismatches,
    }
}

fn compare_set(
    kind: &str,
    expected: BTreeSet<String>,
    actual: BTreeSet<String>,
    mismatches: &mut Vec<TransactionVerifyMismatch>,
) {
    for value in expected.difference(&actual) {
        if mismatches.len() >= MAX_MISMATCHES {
            return;
        }
        mismatches.push(TransactionVerifyMismatch {
            kind: kind.to_string(),
            expected: value.clone(),
            actual: "missing".to_string(),
        });
    }
    for value in actual.difference(&expected) {
        if mismatches.len() >= MAX_MISMATCHES {
            return;
        }
        mismatches.push(TransactionVerifyMismatch {
            kind: kind.to_string(),
            expected: "missing".to_string(),
            actual: value.clone(),
        });
    }
}

fn canonical_native(effects: &TransactionInspectEffects) -> BTreeSet<String> {
    effects
        .native
        .iter()
        .map(|effect| {
            format!(
                "account={} delta={}",
                effect.account,
                normalize_delta(&effect.delta)
            )
        })
        .collect()
}

fn canonical_tokens(effects: &TransactionInspectEffects) -> BTreeSet<String> {
    effects
        .tokens
        .iter()
        .map(|effect| {
            format!(
                "token={} account={} delta={}",
                effect.token,
                effect.account,
                normalize_delta(&effect.delta)
            )
        })
        .collect()
}

fn canonical_approvals(effects: &TransactionInspectEffects) -> BTreeSet<String> {
    effects
        .approvals
        .iter()
        .map(|effect| {
            format!(
                "token={} owner={} spender={} after={}",
                effect.token, effect.owner, effect.spender, effect.after
            )
        })
        .collect()
}

fn canonical_nfts(effects: &TransactionInspectEffects) -> BTreeSet<String> {
    effects
        .nfts
        .iter()
        .map(|effect| {
            format!(
                "collection={} token_id={} from={} to={}",
                effect.collection, effect.token_id, effect.from, effect.to
            )
        })
        .collect()
}

fn normalize_delta(delta: &str) -> String {
    let trimmed = delta.trim();
    let (sign, digits) = match trimmed.strip_prefix('-') {
        Some(digits) => ("-", digits),
        None => ("", trimmed),
    };
    let digits = digits.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let sign = if digits == "0" { "" } else { sign };
    format!("{sign}{digits}")
}

fn normalize_hash(input: &str) -> Result<String, CapabilityError> {
    let hash = input.trim();
    let hex = hash
        .strip_prefix("0x")
        .or_else(|| hash.strip_prefix("0X"))
        .ok_or_else(|| {
            CapabilityError::invalid_argument("tx_hash must be 0x-prefixed 32-byte hex")
        })?;
    if hex.len() != 64 || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(CapabilityError::invalid_argument(
            "tx_hash must be 0x-prefixed 32-byte hex",
        ));
    }
    Ok(format!("0x{}", hex.to_ascii_lowercase()))
}

fn parse_address(value: &str) -> Option<Address> {
    value.parse().ok()
}

fn hex_u64(value: &Value) -> Option<u64> {
    value
        .as_str()
        .and_then(|value| {
            value
                .strip_prefix("0x")
                .or_else(|| value.strip_prefix("0X"))
        })
        .and_then(|value| u64::from_str_radix(value, 16).ok())
}

fn hex_u256(value: &Value) -> Option<U256> {
    let value = value.as_str()?;
    U256::from_str_radix(value.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::extract::Json;
    use axum::routing::post;
    use serde_json::json;

    const HEAD_HASH: &str = "0x0000000000000000000000000000000000000000000000000000000000000064";
    const BLOCK_HASH: &str = "0x0000000000000000000000000000000000000000000000000000000000000062";
    const TX_HASH: &str = "0x00000000000000000000000000000000000000000000000000000000000000aa";
    const FROM: &str = "0x0000000000000000000000000000000000000001";
    const TO: &str = "0x0000000000000000000000000000000000000002";
    const TOKEN: &str = "0x0000000000000000000000000000000000000003";
    const TRANSFER_TOPIC: &str =
        "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

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

    fn sample_tx() -> Value {
        json!({
            "hash": TX_HASH,
            "from": FROM,
            "to": TO,
            "value": "0x16345785d8a0000",
            "gasPrice": "0x2e90edd000"
        })
    }

    fn sample_receipt(status: &str, logs: Value) -> Value {
        json!({
            "transactionHash": TX_HASH,
            "blockNumber": "0x62",
            "blockHash": BLOCK_HASH,
            "status": status,
            "gasUsed": "0x5208",
            "effectiveGasPrice": "0x2e90edd000",
            "logs": logs
        })
    }

    fn transfer_log() -> Value {
        json!({
            "address": TOKEN,
            "topics": [
                TRANSFER_TOPIC,
                "0x0000000000000000000000000000000000000000000000000000000000000001",
                "0x0000000000000000000000000000000000000000000000000000000000000002"
            ],
            "data": "0x00000000000000000000000000000000000000000000000000000000000003e8"
        })
    }

    fn response(body: &Value, tx: Value, receipt: Value) -> Json<Value> {
        let method = body["method"].as_str().unwrap_or_default();
        let result = match method {
            "eth_blockNumber" => json!("0x64"),
            "eth_getBlockByNumber" => json!({"number": "0x64", "hash": HEAD_HASH}),
            "eth_getTransactionByHash" => tx,
            "eth_getTransactionReceipt" => receipt,
            _ => json!(null),
        };
        Json(json!({"jsonrpc": "2.0", "id": 1, "result": result}))
    }

    async fn success_rpc(Json(body): Json<Value>) -> Json<Value> {
        response(&body, sample_tx(), sample_receipt("0x1", json!([])))
    }

    async fn transfer_rpc(Json(body): Json<Value>) -> Json<Value> {
        response(
            &body,
            sample_tx(),
            sample_receipt("0x1", json!([transfer_log()])),
        )
    }

    async fn reverted_rpc(Json(body): Json<Value>) -> Json<Value> {
        response(&body, sample_tx(), sample_receipt("0x0", json!([])))
    }

    async fn pending_rpc(Json(body): Json<Value>) -> Json<Value> {
        response(&body, sample_tx(), Value::Null)
    }

    async fn not_found_rpc(Json(body): Json<Value>) -> Json<Value> {
        response(&body, Value::Null, Value::Null)
    }

    async fn failing_rpc() -> Json<Value> {
        Json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {"code": -32000, "message": "rpc unavailable"}
        }))
    }

    fn native_effects() -> TransactionInspectEffects {
        TransactionInspectEffects {
            native: vec![
                fluxion_protocol::transaction::TransactionInspectNativeEffect {
                    account: FROM.parse().unwrap(),
                    delta: "-100000000000000000".to_string(),
                },
                fluxion_protocol::transaction::TransactionInspectNativeEffect {
                    account: TO.parse().unwrap(),
                    delta: "100000000000000000".to_string(),
                },
            ],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn success_matches_expected_native_effects() {
        let app = Router::new().route("/rpc", post(success_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let response = verify(
            &config(&format!("http://{address}")),
            VerifyRequest {
                chain: Chain::Ethereum,
                tx_hash: TX_HASH.to_string(),
                confirmations: 1,
                expected: Some(native_effects()),
            },
        )
        .await
        .unwrap();
        assert_eq!(response.status, "success");
        assert_eq!(response.confirmations, 3);
        assert_eq!(response.expectation.status, "match");
        assert_eq!(response.fee.as_ref().unwrap().raw, "4200000000000000");
        assert_eq!(response.logs_count, Some(0));
    }

    #[tokio::test]
    async fn mismatch_lists_actual_transfer_effect() {
        let app = Router::new().route("/rpc", post(transfer_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let response = verify(
            &config(&format!("http://{address}")),
            VerifyRequest {
                chain: Chain::Ethereum,
                tx_hash: TX_HASH.to_string(),
                confirmations: 1,
                expected: Some(native_effects()),
            },
        )
        .await
        .unwrap();
        assert_eq!(response.expectation.status, "mismatch");
        assert!(
            response
                .expectation
                .mismatches
                .iter()
                .any(|mismatch| mismatch.kind == "token" && mismatch.expected == "missing")
        );
    }

    #[tokio::test]
    async fn reverted_receipt_is_reported() {
        let app = Router::new().route("/rpc", post(reverted_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let response = verify(
            &config(&format!("http://{address}")),
            VerifyRequest {
                chain: Chain::Ethereum,
                tx_hash: TX_HASH.to_string(),
                confirmations: 1,
                expected: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.status, "reverted");
        assert_eq!(response.expectation.status, "not_configured");
        let effects = response.effects.as_ref().unwrap();
        assert!(effects.native.is_empty());
        assert!(
            response
                .warnings
                .iter()
                .any(|warning| warning.contains("reverted"))
        );
    }

    #[tokio::test]
    async fn missing_receipt_is_pending() {
        let app = Router::new().route("/rpc", post(pending_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let response = verify(
            &config(&format!("http://{address}")),
            VerifyRequest {
                chain: Chain::Ethereum,
                tx_hash: TX_HASH.to_string(),
                confirmations: 1,
                expected: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.status, "pending");
        assert_eq!(response.confirmations, 0);
        assert!(response.effects.is_none());
    }

    #[tokio::test]
    async fn confirmations_below_threshold_stay_pending() {
        let app = Router::new().route("/rpc", post(success_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let response = verify(
            &config(&format!("http://{address}")),
            VerifyRequest {
                chain: Chain::Ethereum,
                tx_hash: TX_HASH.to_string(),
                confirmations: 5,
                expected: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.status, "pending");
        assert_eq!(response.confirmations, 3);
    }

    #[tokio::test]
    async fn missing_transaction_is_not_found() {
        let app = Router::new().route("/rpc", post(not_found_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let response = verify(
            &config(&format!("http://{address}")),
            VerifyRequest {
                chain: Chain::Ethereum,
                tx_hash: TX_HASH.to_string(),
                confirmations: 1,
                expected: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.status, "not_found");
        assert_eq!(response.block_number, 100);
    }

    #[tokio::test]
    async fn rpc_failure_is_an_error() {
        let app = Router::new().route("/rpc", post(failing_rpc));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let error = verify(
            &config(&format!("http://{address}")),
            VerifyRequest {
                chain: Chain::Ethereum,
                tx_hash: TX_HASH.to_string(),
                confirmations: 1,
                expected: None,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, crate::capability::CapabilityErrorCode::RpcError);
    }

    #[test]
    fn rejects_invalid_hash_without_rpc() {
        assert!(normalize_hash("0x1234").is_err());
        assert!(normalize_hash("not-a-hash").is_err());
        assert!(normalize_hash(TX_HASH).is_ok());
    }

    #[test]
    fn extracts_effects_from_envelope_or_bare_object() {
        let envelope = json!({
            "data": {"effects": {"tokens": []}},
            "meta": {"capability": "transaction.inspect"}
        });
        assert!(expected_from_value(&envelope).unwrap().tokens.is_empty());

        let bare = json!({"native": []});
        assert!(expected_from_value(&bare).unwrap().native.is_empty());

        let unrelated = json!({"amount_out": "1"});
        assert!(expected_from_value(&unrelated).is_err());
    }
}
