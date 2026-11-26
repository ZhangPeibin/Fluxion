//! Local calldata construction for curated yield deposits.
//!
//! This module never signs, approves or broadcasts. It returns the exact
//! transactions a wallet must sign, plus any required ERC20 approval.

use alloy_primitives::{Address, U256};
use data_encoding::HEXLOWER;
use eollo_protocol::capability::{Amount, CapabilityError, CapabilityErrorCode, SCHEMA_VERSION};
use eollo_protocol::yield_capability::{
    Registry, YieldAction, YieldApproval, YieldAsset, YieldBuildRequest, YieldBuildResponse,
    YieldBuildStep,
};

use crate::token;

const LIDO_SUBMIT: &str = "a1903eab";
const ROCKET_DEPOSIT: &str = "d0e30db0";
const WETH_DEPOSIT: &str = "d0e30db0";
const ERC20_APPROVE: &str = "095ea7b3";
const AAVE_SUPPLY: &str = "617ba037";
const ERC4626_DEPOSIT: &str = "6e553f65";
const VENUS_MINT: &str = "a0712d68";
const VENUS_MINT_NATIVE: &str = "1249c58b";
const DISCLAIMER: &str =
    "Calldata only. Simulate and inspect before signing; Eollo does not sign or broadcast.";

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
    let is_bsc = entry.chain.eq_ignore_ascii_case("bsc");
    let native_symbol = if is_bsc { "BNB" } else { "ETH" };
    let wrapped_symbol = if is_bsc { "WBNB" } else { "WETH" };
    let deposit_is_native = entry
        .deposit_asset_symbol
        .eq_ignore_ascii_case(native_symbol);
    let deposit_is_wrapped = entry
        .deposit_asset_symbol
        .eq_ignore_ascii_case(wrapped_symbol);
    let funding = request
        .funding_asset
        .as_deref()
        .map(str::to_ascii_uppercase)
        .unwrap_or_else(|| {
            if deposit_is_native || deposit_is_wrapped {
                native_symbol.to_string()
            } else {
                entry.deposit_asset_symbol.clone()
            }
        });
    if deposit_is_native && !funding.eq_ignore_ascii_case(native_symbol) {
        return Err(CapabilityError::invalid_argument(format!(
            "{} requires {native_symbol} funding",
            entry.id
        )));
    }
    if deposit_is_wrapped
        && !funding.eq_ignore_ascii_case(native_symbol)
        && !funding.eq_ignore_ascii_case(wrapped_symbol)
    {
        return Err(CapabilityError::invalid_argument(format!(
            "unsupported funding_asset: {funding}; use {native_symbol} or {wrapped_symbol}"
        )));
    }
    if !deposit_is_native
        && !deposit_is_wrapped
        && !funding.eq_ignore_ascii_case(&entry.deposit_asset_symbol)
    {
        return Err(CapabilityError::invalid_argument(format!(
            "{} requires {} funding",
            entry.id, entry.deposit_asset_symbol
        )));
    }
    let mut warnings = Vec::new();
    let (steps, approvals) = match entry.id.as_str() {
        "lido-steth" => (vec![stake_step(&entry, amount, lido_submit())], Vec::new()),
        "rocket-pool-reth" => (
            vec![stake_step(&entry, amount, hex_data(ROCKET_DEPOSIT, &[]))],
            Vec::new(),
        ),
        "aave-v3-weth" | "sparklend-weth" => supply_steps(
            &entry,
            amount,
            request.recipient,
            &funding,
            native_symbol,
            wrapped_symbol,
        )?,
        "morpho-steakhouse-eth" | "morpho-gauntlet-weth" => vault_steps(
            &entry,
            amount,
            request.recipient,
            &funding,
            native_symbol,
            wrapped_symbol,
        )?,
        "venus-vbnb" => (
            vec![venus_native_mint(&entry, amount, native_symbol)],
            Vec::new(),
        ),
        "venus-vusdt" => venus_erc20_mint(&entry, amount)?,
        other => {
            return Err(CapabilityError::new(
                CapabilityErrorCode::Unsupported,
                format!("yield.build does not construct {other} deposit calldata yet"),
                false,
            ));
        }
    };
    if matches!(entry.action, YieldAction::Stake) && deposit_is_native {
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
        chain: entry.chain.clone(),
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
    entry: &eollo_protocol::yield_capability::RegistryEntry,
    amount: U256,
    recipient: Address,
    funding: &str,
    native_symbol: &str,
    wrapped_symbol: &str,
) -> Result<(Vec<YieldBuildStep>, Vec<YieldApproval>), CapabilityError> {
    let asset = deposit_asset_address(entry)?;
    let mut steps = Vec::new();
    if entry
        .deposit_asset_symbol
        .eq_ignore_ascii_case(wrapped_symbol)
        && funding.eq_ignore_ascii_case(native_symbol)
    {
        steps.push(wrap_step(amount, asset, native_symbol, wrapped_symbol));
    }
    steps.push(approve_step(
        entry.contract,
        amount,
        asset,
        &entry.deposit_asset_symbol,
        entry.deposit_decimals,
    ));
    steps.push(YieldBuildStep {
        kind: "supply".to_string(),
        to: entry.contract,
        data: hex_data(
            AAVE_SUPPLY,
            &[
                word_address(asset),
                word_u256(amount),
                word_address(recipient),
                word_u256(U256::ZERO),
            ],
        ),
        value: "0".to_string(),
        token: Some(entry.deposit_asset_symbol.clone()),
        amount: Some(Amount::from_raw(amount, entry.deposit_decimals)),
        description: format!(
            "Supply {} to {} on behalf of {recipient:#x}",
            entry.deposit_asset_symbol, entry.market_name
        ),
    });
    Ok((
        steps,
        vec![YieldApproval {
            token: asset_ref(&entry.deposit_asset_symbol, asset, entry.deposit_decimals),
            spender: entry.contract,
            amount: Amount::from_raw(amount, entry.deposit_decimals),
        }],
    ))
}

fn vault_steps(
    entry: &eollo_protocol::yield_capability::RegistryEntry,
    amount: U256,
    recipient: Address,
    funding: &str,
    native_symbol: &str,
    wrapped_symbol: &str,
) -> Result<(Vec<YieldBuildStep>, Vec<YieldApproval>), CapabilityError> {
    let asset = deposit_asset_address(entry)?;
    let mut steps = Vec::new();
    if entry
        .deposit_asset_symbol
        .eq_ignore_ascii_case(wrapped_symbol)
        && funding.eq_ignore_ascii_case(native_symbol)
    {
        steps.push(wrap_step(amount, asset, native_symbol, wrapped_symbol));
    }
    steps.push(approve_step(
        entry.contract,
        amount,
        asset,
        &entry.deposit_asset_symbol,
        entry.deposit_decimals,
    ));
    steps.push(YieldBuildStep {
        kind: "deposit".to_string(),
        to: entry.contract,
        data: hex_data(
            ERC4626_DEPOSIT,
            &[word_u256(amount), word_address(recipient)],
        ),
        value: "0".to_string(),
        token: Some(entry.deposit_asset_symbol.clone()),
        amount: Some(Amount::from_raw(amount, entry.deposit_decimals)),
        description: format!(
            "Deposit {} into {} for {recipient:#x}",
            entry.deposit_asset_symbol, entry.market_name
        ),
    });
    Ok((
        steps,
        vec![YieldApproval {
            token: asset_ref(&entry.deposit_asset_symbol, asset, entry.deposit_decimals),
            spender: entry.contract,
            amount: Amount::from_raw(amount, entry.deposit_decimals),
        }],
    ))
}

fn venus_native_mint(
    entry: &eollo_protocol::yield_capability::RegistryEntry,
    amount: U256,
    native_symbol: &str,
) -> YieldBuildStep {
    YieldBuildStep {
        kind: "supply".to_string(),
        to: entry.contract,
        data: hex_data(VENUS_MINT_NATIVE, &[]),
        value: amount.to_string(),
        token: Some(native_symbol.to_string()),
        amount: Some(Amount::from_raw(amount, entry.deposit_decimals)),
        description: format!("Mint {} with {native_symbol}", entry.receipt_asset_symbol),
    }
}

fn venus_erc20_mint(
    entry: &eollo_protocol::yield_capability::RegistryEntry,
    amount: U256,
) -> Result<(Vec<YieldBuildStep>, Vec<YieldApproval>), CapabilityError> {
    let asset = deposit_asset_address(entry)?;
    let steps = vec![
        approve_step(
            entry.contract,
            amount,
            asset,
            &entry.deposit_asset_symbol,
            entry.deposit_decimals,
        ),
        YieldBuildStep {
            kind: "supply".to_string(),
            to: entry.contract,
            data: hex_data(VENUS_MINT, &[word_u256(amount)]),
            value: "0".to_string(),
            token: Some(entry.deposit_asset_symbol.clone()),
            amount: Some(Amount::from_raw(amount, entry.deposit_decimals)),
            description: format!(
                "Mint {} with {}",
                entry.receipt_asset_symbol, entry.deposit_asset_symbol
            ),
        },
    ];
    Ok((
        steps,
        vec![YieldApproval {
            token: asset_ref(&entry.deposit_asset_symbol, asset, entry.deposit_decimals),
            spender: entry.contract,
            amount: Amount::from_raw(amount, entry.deposit_decimals),
        }],
    ))
}

fn deposit_asset_address(
    entry: &eollo_protocol::yield_capability::RegistryEntry,
) -> Result<Address, CapabilityError> {
    entry
        .deposit_asset_address
        .parse::<Address>()
        .map_err(|_| CapabilityError::internal("invalid deposit asset address"))
}

fn asset_ref(symbol: &str, address: Address, decimals: u8) -> YieldAsset {
    YieldAsset {
        symbol: symbol.to_string(),
        address: format!("{address:#x}"),
        decimals,
    }
}

fn stake_step(
    entry: &eollo_protocol::yield_capability::RegistryEntry,
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

fn wrap_step(
    amount: U256,
    wrapped: Address,
    native_symbol: &str,
    wrapped_symbol: &str,
) -> YieldBuildStep {
    YieldBuildStep {
        kind: "wrap".to_string(),
        to: wrapped,
        data: hex_data(WETH_DEPOSIT, &[]),
        value: amount.to_string(),
        token: Some(native_symbol.to_string()),
        amount: Some(Amount::from_raw(amount, 18)),
        description: format!("Wrap {native_symbol} into {wrapped_symbol}"),
    }
}

fn approve_step(
    spender: Address,
    amount: U256,
    token: Address,
    symbol: &str,
    decimals: u8,
) -> YieldBuildStep {
    YieldBuildStep {
        kind: "approve".to_string(),
        to: token,
        data: hex_data(ERC20_APPROVE, &[word_address(spender), word_u256(amount)]),
        value: "0".to_string(),
        token: Some(symbol.to_string()),
        amount: Some(Amount::from_raw(amount, decimals)),
        description: format!("Approve {symbol} for {spender:#x}"),
    }
}

fn lido_submit() -> String {
    hex_data(LIDO_SUBMIT, &[word_address(Address::ZERO)])
}

pub(crate) fn deposit_asset(entry: &eollo_protocol::yield_capability::RegistryEntry) -> YieldAsset {
    YieldAsset {
        symbol: entry.deposit_asset_symbol.clone(),
        address: entry.deposit_asset_address.clone(),
        decimals: entry.deposit_decimals,
    }
}

pub(crate) fn receipt_asset(entry: &eollo_protocol::yield_capability::RegistryEntry) -> YieldAsset {
    YieldAsset {
        symbol: entry.receipt_asset_symbol.clone(),
        address: format!("{:#x}", entry.receipt_asset_address),
        decimals: entry.receipt_decimals,
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

    #[test]
    fn builds_venus_vbnb_native_mint() {
        let response = build(&request("venus-vbnb")).unwrap();
        assert_eq!(response.chain, "bsc");
        assert_eq!(response.steps.len(), 1);
        assert_eq!(response.steps[0].kind, "supply");
        assert_eq!(response.steps[0].data, "0x1249c58b");
        assert_eq!(response.steps[0].value, "10000000000000000000");
        assert!(response.approvals.is_empty());
    }

    #[test]
    fn builds_venus_vusdt_approve_and_mint() {
        let response = build(&request("venus-vusdt")).unwrap();
        let kinds = response
            .steps
            .iter()
            .map(|step| step.kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(kinds, vec!["approve", "supply"]);
        assert_eq!(response.chain, "bsc");
        assert!(response.steps[0].data.starts_with("0x095ea7b3"));
        assert!(response.steps[1].data.starts_with("0xa0712d68"));
        assert_eq!(response.approvals.len(), 1);
    }
}
