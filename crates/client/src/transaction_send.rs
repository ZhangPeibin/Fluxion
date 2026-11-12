//! Generic calldata send through a local WalletConnect session.
//!
//! Every send is simulated and inspected before the phone request. Policy in
//! enforce mode can stop the request; the wallet remains the signing boundary.

use std::time::Duration;

use alloy_primitives::{Address, U256};

use crate::capability::{Amount, CapabilityError, CapabilityErrorCode};
use crate::chain::Chain;
use crate::transaction::{self, SimulateRequest, normalize_data};
use crate::transaction_inspect::{self, InspectPolicy, InspectRequest};
use crate::wallet::WalletConfig;
use fluxion_protocol::transaction::TransactionSendResponse;

const MAX_DATA_BYTES: usize = 256 * 1024;

pub struct SendRequest {
    pub from: Option<Address>,
    pub to: Address,
    pub data: String,
    pub value: U256,
    pub block: Option<u64>,
    pub timeout: Duration,
    pub policy: Option<InspectPolicy>,
}

pub async fn send(
    config: &WalletConfig,
    request: SendRequest,
) -> Result<TransactionSendResponse, CapabilityError> {
    if request.to == Address::ZERO {
        return Err(CapabilityError::invalid_argument(
            "target cannot be the zero address",
        ));
    }
    let data = normalize_data(&request.data)?;
    if data.len().saturating_sub(2) / 2 > MAX_DATA_BYTES {
        return Err(CapabilityError::invalid_argument(format!(
            "calldata exceeds {MAX_DATA_BYTES} bytes"
        )));
    }

    let session = crate::walletconnect::Session::load()?.ok_or_else(|| {
        CapabilityError::new(
            CapabilityErrorCode::Unsupported,
            "wallet is not connected; run wallet.connect first",
            false,
        )
    })?;
    let from = request
        .from
        .or_else(|| session.accounts.first().copied())
        .ok_or_else(|| {
            CapabilityError::new(
                CapabilityErrorCode::Unsupported,
                "WalletConnect session has no account",
                false,
            )
        })?;
    if !session.accounts.contains(&from) {
        return Err(CapabilityError::invalid_argument(
            "from address is not in the WalletConnect session",
        ));
    }
    let chain = Chain::from_id(session.chain_id)?;
    let chain_config = WalletConfig {
        rpc_url: config.rpc_url_for(chain),
        ..config.clone()
    };

    let simulation = transaction::simulate(
        &chain_config,
        SimulateRequest {
            chain,
            from: Some(from),
            to: request.to,
            data: data.clone(),
            value: request.value,
            block: request.block,
        },
    )
    .await?;
    if !simulation.success {
        let detail = simulation
            .error
            .as_deref()
            .or(simulation.revert_data.as_deref())
            .unwrap_or("transaction simulation reverted");
        return Err(CapabilityError::new(
            CapabilityErrorCode::SimulationFailed,
            detail,
            false,
        ));
    }

    let inspection = transaction_inspect::inspect_with_policy(
        &chain_config,
        InspectRequest {
            chain,
            from: Some(from),
            to: request.to,
            data: data.clone(),
            value: request.value,
            block: request.block,
        },
        request.policy,
    )
    .await?;
    if inspection.policy.blocked {
        return Err(CapabilityError::new(
            CapabilityErrorCode::PolicyBlocked,
            format!(
                "transaction.send blocked by policy: {}",
                inspection.policy.reasons.join("; ")
            ),
            false,
        ));
    }

    let tx_hash = session
        .send_transaction(
            from,
            request.to,
            request.value,
            data.clone(),
            request.timeout,
        )
        .await?;
    Ok(TransactionSendResponse {
        chain: chain.as_str().to_string(),
        chain_id: chain.chain_id(),
        from,
        to: request.to,
        value: Amount::from_raw(request.value, 18),
        data,
        tx_hash,
        simulation,
        inspection,
    })
}
