//! Local calldata construction for curated yield exits.

use alloy_primitives::{Address, U256};
use eollo_protocol::capability::{Amount, CapabilityError, CapabilityErrorCode, SCHEMA_VERSION};
use eollo_protocol::yield_capability::{
    Registry, RegistryEntry, YieldBuildStep, YieldWithdrawRequest, YieldWithdrawResponse,
};

use crate::token;
use crate::yield_build::{deposit_asset, hex_data, receipt_asset, word_address, word_u256};

const AAVE_WITHDRAW: &str = "69328dec";
const ERC4626_REDEEM: &str = "ba087652";
const ROCKET_BURN: &str = "42966c68";
const DISCLAIMER: &str =
    "Calldata only. Simulate and inspect before signing; Eollo does not sign or broadcast.";

pub fn withdraw(request: &YieldWithdrawRequest) -> Result<YieldWithdrawResponse, CapabilityError> {
    let registry = Registry::load_default()
        .map_err(|error| CapabilityError::internal(format!("yield registry: {error}")))?;
    let entry = registry
        .entries()
        .iter()
        .find(|entry| entry.id == request.opportunity_id)
        .ok_or_else(|| {
            CapabilityError::invalid_argument(format!(
                "unknown opportunity_id: {}",
                request.opportunity_id
            ))
        })?
        .clone();
    if request.recipient == Address::ZERO {
        return Err(CapabilityError::invalid_argument("recipient is required"));
    }
    let owner = request.owner.unwrap_or(request.recipient);
    if owner == Address::ZERO {
        return Err(CapabilityError::invalid_argument("owner must not be zero"));
    }
    let full_exit = request.amount.eq_ignore_ascii_case("max");
    let amount = if full_exit {
        None
    } else {
        Some(token::parse_amount(
            &request.amount,
            entry.receipt_decimals,
        )?)
    };
    let mut warnings = Vec::new();
    if request.owner.is_some() && !entry.id.starts_with("morpho-") {
        warnings.push(
            "owner is only encoded for Morpho; this market withdraws from the transaction sender"
                .to_string(),
        );
    }
    let step = match entry.id.as_str() {
        "aave-v3-weth" | "sparklend-weth" => {
            let asset = entry
                .deposit_asset_address
                .parse::<Address>()
                .map_err(|_| CapabilityError::internal("invalid deposit asset address"))?;
            let raw = amount.unwrap_or(U256::MAX);
            step(
                "withdraw",
                &entry,
                hex_data(
                    AAVE_WITHDRAW,
                    &[
                        word_address(asset),
                        word_u256(raw),
                        word_address(request.recipient),
                    ],
                ),
                amount,
                format!(
                    "Withdraw {} from {} to {:#x}",
                    if full_exit {
                        "the full position".to_string()
                    } else {
                        format!("{} {}", request.amount, entry.receipt_asset_symbol)
                    },
                    entry.market_name,
                    request.recipient
                ),
            )
        }
        "morpho-steakhouse-eth" | "morpho-gauntlet-weth" => {
            let raw = amount.ok_or_else(|| {
                CapabilityError::invalid_argument(
                    "Morpho withdrawals require an exact amount; max is not supported yet",
                )
            })?;
            step(
                "redeem",
                &entry,
                hex_data(
                    ERC4626_REDEEM,
                    &[
                        word_u256(raw),
                        word_address(request.recipient),
                        word_address(owner),
                    ],
                ),
                Some(raw),
                format!(
                    "Redeem {} {} from {} for {:#x}",
                    request.amount,
                    entry.receipt_asset_symbol,
                    entry.market_name,
                    request.recipient
                ),
            )
        }
        "rocket-pool-reth" => {
            let raw = amount.ok_or_else(|| {
                CapabilityError::invalid_argument(
                    "Rocket Pool burn requires an exact rETH amount; max is not supported yet",
                )
            })?;
            warnings.push(
                "Rocket Pool burn uses the transaction sender and may require a withdrawal delay"
                    .to_string(),
            );
            step(
                "burn",
                &entry,
                hex_data(ROCKET_BURN, &[word_u256(raw)]),
                Some(raw),
                format!("Burn {} rETH to ETH", request.amount),
            )
        }
        other => {
            return Err(CapabilityError::new(
                CapabilityErrorCode::Unsupported,
                format!("yield.withdraw does not construct {other} exit calldata yet"),
                false,
            ));
        }
    };
    Ok(YieldWithdrawResponse {
        schema_version: SCHEMA_VERSION,
        capability: "yield.withdraw".to_string(),
        request_id: crate::client::request_id(),
        chain: "ethereum".to_string(),
        opportunity_id: entry.id.clone(),
        action: entry.action,
        amount: amount.map(|value| Amount::from_raw(value, entry.receipt_decimals)),
        amount_kind: if full_exit { "max" } else { "exact" }.to_string(),
        recipient: request.recipient,
        owner,
        receipt_asset: receipt_asset(&entry),
        underlying_asset: deposit_asset(&entry),
        steps: vec![step],
        approvals: vec![],
        warnings,
        disclaimer: DISCLAIMER.to_string(),
    })
}

fn step(
    kind: &str,
    entry: &RegistryEntry,
    data: String,
    amount: Option<U256>,
    description: String,
) -> YieldBuildStep {
    YieldBuildStep {
        kind: kind.to_string(),
        to: entry.contract,
        data,
        value: "0".to_string(),
        token: Some(entry.receipt_asset_symbol.clone()),
        amount: amount.map(|value| Amount::from_raw(value, entry.receipt_decimals)),
        description,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: &str) -> YieldWithdrawRequest {
        YieldWithdrawRequest {
            opportunity_id: id.to_string(),
            amount: "1".to_string(),
            recipient: Address::with_last_byte(0x42),
            owner: None,
        }
    }

    #[test]
    fn builds_aave_withdraw() {
        let response = withdraw(&request("aave-v3-weth")).unwrap();
        assert_eq!(response.steps[0].kind, "withdraw");
        assert!(response.steps[0].data.starts_with("0x69328dec"));
        assert_eq!(response.owner, response.recipient);
    }

    #[test]
    fn supports_aave_max() {
        let mut request = request("sparklend-weth");
        request.amount = "max".to_string();
        let response = withdraw(&request).unwrap();
        assert_eq!(response.amount_kind, "max");
        assert!(response.amount.is_none());
        assert!(
            response.steps[0]
                .data
                .contains("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
        );
    }

    #[test]
    fn builds_morpho_redeem() {
        let response = withdraw(&request("morpho-steakhouse-eth")).unwrap();
        assert_eq!(response.steps[0].kind, "redeem");
        assert!(response.steps[0].data.starts_with("0xba087652"));
    }

    #[test]
    fn builds_rocket_burn() {
        let response = withdraw(&request("rocket-pool-reth")).unwrap();
        assert_eq!(response.steps[0].kind, "burn");
        assert!(response.steps[0].data.starts_with("0x42966c68"));
    }

    #[test]
    fn rejects_lido_and_max_morpho() {
        assert_eq!(
            withdraw(&request("lido-steth")).unwrap_err().code,
            CapabilityErrorCode::Unsupported
        );
        let mut request = request("morpho-steakhouse-eth");
        request.amount = "max".to_string();
        assert!(withdraw(&request).is_err());
    }
}
