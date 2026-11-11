//! ETH/ERC20 transfers through a local WalletConnect session.

use std::time::Duration;

use alloy_primitives::{Address, U256, address};

use crate::capability::{Amount, CapabilityError, CapabilityErrorCode, TokenRef};
use crate::token;
use crate::transaction::{self, SimulateRequest};
use crate::wallet::{WalletConfig, WalletSendResponse};

const NATIVE: Address = address!("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");

#[derive(Debug, Clone)]
pub struct SendRequest {
    pub from: Option<Address>,
    pub to: Address,
    pub token: String,
    pub amount: String,
    pub decimals: Option<u8>,
    pub block: Option<u64>,
    pub timeout: Duration,
}

enum TransferToken {
    Native,
    Erc20(TokenRef),
}

pub async fn send(
    config: &WalletConfig,
    request: SendRequest,
) -> Result<WalletSendResponse, CapabilityError> {
    if request.to == Address::ZERO {
        return Err(CapabilityError::invalid_argument(
            "recipient cannot be the zero address",
        ));
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

    let transfer_token =
        resolve_transfer_token(&request.token, request.decimals, session.chain_id)?;
    let (token_ref, decimals) = match &transfer_token {
        TransferToken::Native => (TokenRef::new(Some("ETH".to_string()), NATIVE, 18), 18u8),
        TransferToken::Erc20(token) => (token.clone(), token.decimals),
    };
    let raw = token::parse_amount(&request.amount, decimals)?;
    if raw.is_zero() {
        return Err(CapabilityError::invalid_argument(
            "amount must be greater than zero",
        ));
    }

    let (tx_to, value, data, contract) = match &transfer_token {
        TransferToken::Native => (request.to, raw, "0x".to_string(), None),
        TransferToken::Erc20(token) => (
            token.address,
            U256::ZERO,
            format!("0x{}", hex(&encode_transfer(request.to, raw))),
            Some(token.address),
        ),
    };

    let chain = crate::chain::Chain::from_id(session.chain_id)?;
    let chain_config = WalletConfig {
        rpc_url: config.rpc_url_for(chain),
        ..config.clone()
    };
    let simulation = transaction::simulate(
        &chain_config,
        SimulateRequest {
            chain,
            from: Some(from),
            to: tx_to,
            data: data.clone(),
            value,
            block: request.block,
        },
    )
    .await?;
    if !simulation.success {
        let detail = simulation
            .error
            .as_deref()
            .or(simulation.revert_data.as_deref())
            .unwrap_or("transfer simulation reverted");
        return Err(CapabilityError::new(
            CapabilityErrorCode::SimulationFailed,
            detail,
            false,
        ));
    }

    let tx_hash = session
        .send_transaction(from, tx_to, value, data, request.timeout)
        .await?;
    Ok(WalletSendResponse {
        chain: chain.as_str().to_string(),
        from,
        recipient: request.to,
        token: token_ref,
        amount: Amount::from_raw(raw, decimals),
        contract,
        chain_id: session.chain_id,
        tx_hash,
        simulation,
    })
}

fn resolve_transfer_token(
    input: &str,
    decimals: Option<u8>,
    chain_id: u64,
) -> Result<TransferToken, CapabilityError> {
    let input = input.trim();
    let native = if chain_id == 56 { "BNB" } else { "ETH" };
    if input.eq_ignore_ascii_case(native) {
        return Ok(TransferToken::Native);
    }
    Ok(TransferToken::Erc20(token::resolve_for_chain(
        input, decimals, chain_id,
    )?))
}

fn encode_transfer(to: Address, amount: U256) -> Vec<u8> {
    let mut data = Vec::with_capacity(4 + 12 + 20 + 32);
    data.extend_from_slice(&[0xa9, 0x05, 0x9c, 0xbb]);
    data.extend_from_slice(&[0u8; 12]);
    data.extend_from_slice(to.as_slice());
    data.extend_from_slice(&amount.to_be_bytes::<32>());
    data
}

fn hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_erc20_transfer_calldata() {
        let to: Address = "0x5ED10018795d667050e996833C60fF02b1388058"
            .parse()
            .unwrap();
        let data = encode_transfer(to, U256::from(1_000_000u64));
        assert_eq!(data.len(), 68);
        assert_eq!(&data[..4], &[0xa9, 0x05, 0x9c, 0xbb]);
        assert_eq!(&data[4..16], &[0u8; 12]);
        assert_eq!(&data[16..36], to.as_slice());
        assert_eq!(U256::from_be_slice(&data[36..]), U256::from(1_000_000u64));
    }

    #[test]
    fn resolves_native_and_erc20_tokens() {
        assert!(matches!(
            resolve_transfer_token("eth", None, 1).unwrap(),
            TransferToken::Native
        ));
        let usdc = resolve_transfer_token("USDC", None, 1).unwrap();
        match usdc {
            TransferToken::Erc20(token) => assert_eq!(token.decimals, 6),
            TransferToken::Native => panic!("USDC must be ERC20"),
        }
    }
}
