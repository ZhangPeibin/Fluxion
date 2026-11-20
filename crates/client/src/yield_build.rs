//! Local calldata construction for curated yield deposits.
//!
//! This module never signs, approves or broadcasts. It returns the exact
//! transactions a wallet must sign, plus any required ERC20 approval.

use alloy_primitives::{Address, U256};
use data_encoding::HEXLOWER;
use fluxion_protocol::capability::{Amount, CapabilityError, CapabilityErrorCode, SCHEMA_VERSION};
use fluxion_protocol::yield_capability::{
    Registry, YieldAction, YieldApproval, YieldAsset, YieldBuildRequest, YieldBuildResponse,
    YieldBuildStep,
};

use crate::token;

const WETH: Address = alloy_primitives::address!("C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");
const LIDO_SUBMIT: &str = "a1903eab";
const ROCKET_DEPOSIT: &str = "d0e30db0";
const WETH_DEPOSIT: &str = "d0e30db0";
const ERC20_APPROVE: &str = "095ea7b3";
const AAVE_SUPPLY: &str = "617ba037";
const ERC4626_DEPOSIT: &str = "6e553f65";
const DISCLAIMER: &str =
    "Calldata only. Simulate and inspect before signing; Fluxion does not sign or broadcast.";

pub fn build(request: &YieldBuildRequest) -> Result<YieldBuildResponse, CapabilityError> {
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
    let amount = token::parse_amount(&request.amount, entry.deposit_decimals)?;
    let funding = request
        .funding_asset
        .as_deref()
        .unwrap_or("ETH")
        .to_ascii_uppercase();
    if entry.deposit_asset_symbol == "ETH" && funding != "ETH" {
        return Err(CapabilityError::invalid_argument(format!(
            "{} requires ETH funding",
            entry.id
        )));
    }
    if entry.deposit_asset_symbol != "ETH" && !matches!(funding.as_str(), "ETH" | "WETH") {
        return Err(CapabilityError::invalid_argument(format!(
            "unsupported funding_asset: {funding}; use ETH or WETH"
        )));
    }
    let mut warnings = Vec::new();
    let (steps, approvals) = match entry.id.as_str() {
        "lido-steth" => (vec![stake_step(&entry, amount, lido_submit())], Vec::new()),
        "rocket-pool-reth" => (
            vec![stake_step(&entry, amount, hex_data(ROCKET_DEPOSIT, &[]))],
            Vec::new(),
        ),
        "aave-v3-weth" | "sparklend-weth" => {
            supply_steps(&entry, amount, request.recipient, &funding)?
        }
        "morpho-steakhouse-eth" | "morpho-gauntlet-weth" => {
            vault_steps(&entry, amount, request.recipient, &funding)?
        }
        other => {
            return Err(CapabilityError::new(
                CapabilityErrorCode::Unsupported,
                format!("yield.build does not construct {other} deposit calldata yet"),
                false,
            ));
        }
    };
    if matches!(entry.action, YieldAction::Stake) && entry.deposit_asset_symbol == "ETH" {
        warnings.push(
            "receipt tokens are minted to the transaction sender; recipient must be the sender"
                .to_string(),
        );
    }
    if entry.id.starts_with("morpho-") {
        warnings.push(
            "vault shares depend on the current exchange rate and available vault liquidity"
                .to_string(),
        );
    }
    Ok(YieldBuildResponse {
        schema_version: SCHEMA_VERSION,
        capability: "yield.build".to_string(),
        request_id: crate::client::request_id(),
        chain: "ethereum".to_string(),
        opportunity_id: entry.id.clone(),
        action: entry.action,
        amount: Amount::from_raw(amount, entry.deposit_decimals),
        recipient: request.recipient,
        deposit_asset: deposit_asset(&entry),
        receipt_asset: receipt_asset(&entry),
        requires_wrap: steps.iter().any(|step| step.kind == "wrap"),
        steps,
        approvals,
        warnings,
        disclaimer: DISCLAIMER.to_string(),
    })
}

fn supply_steps(
    entry: &fluxion_protocol::yield_capability::RegistryEntry,
    amount: U256,
    recipient: Address,
    funding: &str,
) -> Result<(Vec<YieldBuildStep>, Vec<YieldApproval>), CapabilityError> {
    let mut steps = Vec::new();
    match funding {
        "ETH" => steps.push(wrap_step(amount)),
        "WETH" => {}
        other => {
            return Err(CapabilityError::invalid_argument(format!(
                "unsupported funding_asset: {other}; use ETH or WETH"
            )));
        }
    }
    steps.push(approve_step(entry.contract, amount));
    steps.push(YieldBuildStep {
        kind: "supply".to_string(),
        to: entry.contract,
        data: hex_data(
            AAVE_SUPPLY,
            &[
                word_address(WETH),
                word_u256(amount),
                word_address(recipient),
                word_u256(U256::ZERO),
            ],
        ),
        value: "0".to_string(),
        token: Some("WETH".to_string()),
        amount: Some(Amount::from_raw(amount, 18)),
        description: format!(
            "Supply WETH to {} on behalf of {recipient:#x}",
            entry.market_name
        ),
    });
    Ok((
        steps,
        vec![YieldApproval {
            token: weth_asset(),
            spender: entry.contract,
            amount: Amount::from_raw(amount, 18),
        }],
    ))
}

fn vault_steps(
    entry: &fluxion_protocol::yield_capability::RegistryEntry,
    amount: U256,
    recipient: Address,
    funding: &str,
) -> Result<(Vec<YieldBuildStep>, Vec<YieldApproval>), CapabilityError> {
    let (mut steps, approvals) = supply_steps(entry, amount, recipient, funding)?;
    let supply = steps
        .pop()
        .ok_or_else(|| CapabilityError::internal("missing supply step"))?;
    steps.push(YieldBuildStep {
        kind: "deposit".to_string(),
        to: entry.contract,
        data: hex_data(
            ERC4626_DEPOSIT,
            &[word_u256(amount), word_address(recipient)],
        ),
        value: "0".to_string(),
        token: Some("WETH".to_string()),
        amount: Some(Amount::from_raw(amount, 18)),
        description: format!("Deposit WETH into {} for {recipient:#x}", entry.market_name),
    });
    let _ = supply;
    Ok((steps, approvals))
}

fn stake_step(
    entry: &fluxion_protocol::yield_capability::RegistryEntry,
    amount: U256,
    data: String,
) -> YieldBuildStep {
    YieldBuildStep {
        kind: "stake".to_string(),
        to: entry.contract,
        data,
        value: amount.to_string(),
        token: Some("ETH".to_string()),
        amount: Some(Amount::from_raw(amount, 18)),
        description: format!("Stake ETH into {}", entry.market_name),
    }
}

fn wrap_step(amount: U256) -> YieldBuildStep {
    YieldBuildStep {
        kind: "wrap".to_string(),
        to: WETH,
        data: hex_data(WETH_DEPOSIT, &[]),
        value: amount.to_string(),
        token: Some("ETH".to_string()),
        amount: Some(Amount::from_raw(amount, 18)),
        description: "Wrap ETH into WETH".to_string(),
    }
}

fn approve_step(spender: Address, amount: U256) -> YieldBuildStep {
    YieldBuildStep {
        kind: "approve".to_string(),
        to: WETH,
        data: hex_data(ERC20_APPROVE, &[word_address(spender), word_u256(amount)]),
        value: "0".to_string(),
        token: Some("WETH".to_string()),
        amount: Some(Amount::from_raw(amount, 18)),
        description: format!("Approve WETH for {spender:#x}"),
    }
}

fn lido_submit() -> String {
    hex_data(LIDO_SUBMIT, &[word_address(Address::ZERO)])
}

pub(crate) fn deposit_asset(
    entry: &fluxion_protocol::yield_capability::RegistryEntry,
) -> YieldAsset {
    YieldAsset {
        symbol: entry.deposit_asset_symbol.clone(),
        address: entry.deposit_asset_address.clone(),
        decimals: entry.deposit_decimals,
    }
}

pub(crate) fn receipt_asset(
    entry: &fluxion_protocol::yield_capability::RegistryEntry,
) -> YieldAsset {
    YieldAsset {
        symbol: entry.receipt_asset_symbol.clone(),
        address: format!("{:#x}", entry.receipt_asset_address),
        decimals: entry.receipt_decimals,
    }
}

fn weth_asset() -> YieldAsset {
    YieldAsset {
        symbol: "WETH".to_string(),
        address: format!("{WETH:#x}"),
        decimals: 18,
    }
}

pub(crate) fn hex_data(selector: &str, words: &[[u8; 32]]) -> String {
    let mut bytes = HEXLOWER
        .decode(selector.as_bytes())
        .expect("selector is hex");
    for word in words {
        bytes.extend_from_slice(word);
    }
    format!("0x{}", HEXLOWER.encode(&bytes))
}

pub(crate) fn word_address(address: Address) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(address.as_slice());
    word
}

pub(crate) fn word_u256(value: U256) -> [u8; 32] {
    value.to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: &str) -> YieldBuildRequest {
        YieldBuildRequest {
            opportunity_id: id.to_string(),
            amount: "10".to_string(),
            recipient: Address::with_last_byte(0x42),
            funding_asset: None,
        }
    }

    #[test]
    fn builds_lido_submit() {
        let response = build(&request("lido-steth")).unwrap();
        assert_eq!(response.steps.len(), 1);
        assert_eq!(response.steps[0].kind, "stake");
        assert_eq!(
            response.steps[0].to,
            response.receipt_asset.address.parse::<Address>().unwrap()
        );
        assert!(response.steps[0].data.starts_with("0xa1903eab"));
        assert_eq!(response.steps[0].value, "10000000000000000000");
    }

    #[test]
    fn builds_rocket_deposit() {
        let response = build(&request("rocket-pool-reth")).unwrap();
        assert_eq!(response.steps[0].data, "0xd0e30db0");
    }

    #[test]
    fn builds_aave_wrap_approve_supply() {
        let response = build(&request("aave-v3-weth")).unwrap();
        let kinds = response
            .steps
            .iter()
            .map(|step| step.kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(kinds, vec!["wrap", "approve", "supply"]);
        assert!(response.requires_wrap);
        assert_eq!(response.approvals.len(), 1);
        assert!(response.steps[2].data.starts_with("0x617ba037"));
        assert!(
            response.steps[2]
                .data
                .contains("0000000000000000000000000000000000000042")
        );
    }

    #[test]
    fn builds_morpho_erc4626_deposit() {
        let response = build(&request("morpho-steakhouse-eth")).unwrap();
        let kinds = response
            .steps
            .iter()
            .map(|step| step.kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(kinds, vec!["wrap", "approve", "deposit"]);
        assert!(response.steps[2].data.starts_with("0x6e553f65"));
    }

    #[test]
    fn weth_funding_skips_wrap() {
        let mut request = request("aave-v3-weth");
        request.funding_asset = Some("WETH".to_string());
        let response = build(&request).unwrap();
        let kinds = response
            .steps
            .iter()
            .map(|step| step.kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(kinds, vec!["approve", "supply"]);
        assert!(!response.requires_wrap);
    }

    #[test]
    fn rejects_unsupported_market() {
        let error = build(&request("cbeth")).unwrap_err();
        assert_eq!(error.code, CapabilityErrorCode::Unsupported);
    }

    #[test]
    fn rejects_zero_recipient() {
        let mut request = request("lido-steth");
        request.recipient = Address::ZERO;
        assert!(build(&request).is_err());
    }
}
