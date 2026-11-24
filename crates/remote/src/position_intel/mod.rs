//! Read-only position analysis across curated yield markets.

use std::collections::HashMap;

use alloy_primitives::{Address, U256};
use eollo_protocol::capability::Amount;
use eollo_protocol::position::{
    PositionAnalyzeRequest, PositionAnalyzeResponse, PositionEntry, PositionTotals, SignedAmount,
};
use eollo_protocol::wallet::AssetSource;
use eollo_protocol::yield_capability::{Registry, RegistryEntry};
use thiserror::Error;

use crate::rpc::RpcClient;
use crate::yield_intel::score::YieldSnapshot;

const BALANCE_OF: &str = "70a08231";
const CONVERT_TO_ASSETS: &str = "07a2d13a";
const RETH_RATE: &str = "e6aa216c";
const CBETH_RATE: &str = "3ba0b9a9";
const WEETH_RATE: &str = "679aefce";
const ONE: U256 = U256::from_limbs([1_000_000_000_000_000_000, 0, 0, 0]);
const DISCLAIMER: &str = "Positions are read at a pinned block. Values are estimates; PnL requires a supplied cost basis.";

#[derive(Debug, Error)]
pub enum PositionError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("position data unavailable: {0}")]
    Unavailable(String),
    #[error("position data is stale")]
    Stale,
    #[error("{0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy)]
enum Conversion {
    OneToOne,
    Rate(&'static str),
    ConvertToAssets,
}

pub async fn analyze(
    registry: &Registry,
    snapshot: &YieldSnapshot,
    rpc: &RpcClient,
    request: &PositionAnalyzeRequest,
) -> Result<PositionAnalyzeResponse, PositionError> {
    if request.owner == Address::ZERO {
        return Err(PositionError::Invalid(
            "owner must not be the zero address".to_string(),
        ));
    }
    if !request.chain.eq_ignore_ascii_case("ethereum") {
        return Err(PositionError::Invalid(
            "position.analyze currently supports chain=ethereum only".to_string(),
        ));
    }
    let selected = select_entries(registry, request)?;
    let costs = parse_costs(request)?;
    let header = rpc
        .block_header(None)
        .await
        .map_err(|error| PositionError::Unavailable(error.to_string()))?;
    let rpc = rpc.clone().at_block(header.number);
    let mut positions = Vec::new();
    let mut warnings = Vec::new();
    for entry in selected {
        let Some(opportunity) = snapshot
            .opportunities
            .iter()
            .find(|opportunity| opportunity.entry.id == entry.id)
        else {
            warnings.push(format!(
                "{} is not in the current yield snapshot; position skipped",
                entry.id
            ));
            continue;
        };
        let balance = read_balance(&rpc, entry.receipt_asset_address, request.owner).await?;
        let mut item_warnings = Vec::new();
        let value = convert_value(&rpc, &entry, balance, &mut item_warnings).await?;
        let cost = costs.get(&entry.id).copied();
        let (pnl_eth, pnl_percent) = match (value, cost) {
            (Some(value), Some(cost)) if cost > U256::ZERO => (
                signed_difference(value, cost),
                percent_difference(value, cost),
            ),
            (Some(_), Some(_)) => {
                item_warnings.push("cost basis is zero; PnL percent unavailable".to_string());
                (None, None)
            }
            _ => (None, None),
        };
        let estimated = value.and_then(|value| annual_yield(value, opportunity.ranking_apy_base));
        let status = if balance.is_zero() { "empty" } else { "active" };
        positions.push(PositionEntry {
            opportunity_id: entry.id.clone(),
            safety_tier: opportunity.entry.safety_tier,
            protocol: opportunity.protocol.clone(),
            action: opportunity.entry.action,
            receipt_asset: opportunity.market.receipt_asset.clone(),
            receipt_balance: Amount::from_raw(balance, entry.receipt_decimals),
            underlying_asset: opportunity.market.deposit_asset.clone(),
            value_eth: value.map(|value| Amount::from_raw(value, 18)),
            cost_basis_eth: cost.map(|cost| Amount::from_raw(cost, 18)),
            pnl_eth,
            pnl_percent,
            apy: opportunity.yield_info.clone(),
            estimated_annual_yield_eth: estimated,
            withdrawal: opportunity.market.withdrawal.clone(),
            risk_factors: opportunity.risk_factors.clone(),
            data_confidence: opportunity.data_confidence.clone(),
            status: status.to_string(),
            warnings: item_warnings,
        });
    }
    if !request.include_zero {
        positions.retain(|position| position.receipt_balance.raw != "0");
    }
    let totals = totals(&positions);
    let as_of = header.timestamp;
    if as_of.saturating_sub(snapshot.as_of) > 900 {
        warnings.push(format!(
            "yield APY snapshot is {}s older than the position block",
            as_of.saturating_sub(snapshot.as_of)
        ));
    }
    Ok(PositionAnalyzeResponse {
        schema_version: eollo_protocol::capability::SCHEMA_VERSION,
        capability: "position.analyze".to_string(),
        request_id: request_id(),
        chain: "ethereum".to_string(),
        owner: request.owner,
        block_number: header.number,
        block_hash: header.hash,
        as_of,
        yield_data_as_of: snapshot.as_of,
        positions,
        totals,
        coverage: "curated-universe+onchain+yield-snapshot".to_string(),
        sources: vec![
            AssetSource {
                name: "ethereum-rpc".to_string(),
                scope: "receipt-balances".to_string(),
                block: Some(header.number),
            },
            AssetSource {
                name: "yield.discover".to_string(),
                scope: "apy+risk".to_string(),
                block: None,
            },
        ],
        disclaimer: DISCLAIMER.to_string(),
        warnings,
    })
}

fn select_entries(
    registry: &Registry,
    request: &PositionAnalyzeRequest,
) -> Result<Vec<RegistryEntry>, PositionError> {
    if request.opportunity_ids.is_empty() {
        return Ok(registry.entries().to_vec());
    }
    let mut entries = Vec::new();
    for id in &request.opportunity_ids {
        let entry = registry
            .entries()
            .iter()
            .find(|entry| entry.id == *id)
            .ok_or_else(|| PositionError::Invalid(format!("unknown opportunity_id: {id}")))?;
        entries.push(entry.clone());
    }
    Ok(entries)
}

fn parse_costs(request: &PositionAnalyzeRequest) -> Result<HashMap<String, U256>, PositionError> {
    let mut costs = HashMap::new();
    for item in &request.cost_basis {
        let amount = parse_units(&item.amount, 18)?;
        if costs.insert(item.opportunity_id.clone(), amount).is_some() {
            return Err(PositionError::Invalid(format!(
                "duplicate cost basis for {}",
                item.opportunity_id
            )));
        }
    }
    Ok(costs)
}

async fn read_balance(
    rpc: &RpcClient,
    token: Address,
    owner: Address,
) -> Result<U256, PositionError> {
    let data = format!("0x{BALANCE_OF}{}", hex(&word_address(owner)));
    call_u256(rpc, token, &data).await
}

async fn convert_value(
    rpc: &RpcClient,
    entry: &RegistryEntry,
    balance: U256,
    warnings: &mut Vec<String>,
) -> Result<Option<U256>, PositionError> {
    let Some(conversion) = conversion(&entry.id) else {
        if !balance.is_zero() {
            warnings.push(format!(
                "ETH conversion is not implemented for {}; receipt balance is reported without value",
                entry.id
            ));
        }
        return Ok(None);
    };
    if balance.is_zero() {
        return Ok(Some(U256::ZERO));
    }
    let value = match conversion {
        Conversion::OneToOne => {
            if entry.receipt_decimals != 18 {
                warnings.push(format!(
                    "{} receipt decimals are {}, value assumes 18",
                    entry.id, entry.receipt_decimals
                ));
                return Ok(None);
            }
            balance
        }
        Conversion::Rate(selector) => {
            let rate =
                call_u256(rpc, entry.receipt_asset_address, &format!("0x{selector}")).await?;
            balance.checked_mul(rate).unwrap_or(U256::MAX) / ONE
        }
        Conversion::ConvertToAssets => {
            let data = format!("0x{CONVERT_TO_ASSETS}{}", hex(&word_u256(balance)));
            call_u256(rpc, entry.receipt_asset_address, &data).await?
        }
    };
    Ok(Some(value))
}

fn conversion(id: &str) -> Option<Conversion> {
    match id {
        "lido-steth" | "aave-v3-weth" | "sparklend-weth" => Some(Conversion::OneToOne),
        "rocket-pool-reth" => Some(Conversion::Rate(RETH_RATE)),
        "cbeth" => Some(Conversion::Rate(CBETH_RATE)),
        "etherfi-weeth" => Some(Conversion::Rate(WEETH_RATE)),
        "morpho-steakhouse-eth" | "morpho-gauntlet-weth" => Some(Conversion::ConvertToAssets),
        "kelp-rseth" | "renzo-ezeth" => None,
        _ => None,
    }
}

async fn call_u256(rpc: &RpcClient, to: Address, data: &str) -> Result<U256, PositionError> {
    let bytes = rpc
        .eth_call(to, data)
        .await
        .map_err(|error| PositionError::Unavailable(error.to_string()))?;
    let word = bytes
        .get(..32)
        .ok_or_else(|| PositionError::Unavailable("short eth_call result".to_string()))?;
    Ok(U256::from_be_slice(word))
}

fn totals(positions: &[PositionEntry]) -> PositionTotals {
    let mut value = U256::ZERO;
    let mut estimated = U256::ZERO;
    let mut cost = U256::ZERO;
    let mut pnl = 0i128;
    let mut value_known = false;
    let mut estimated_known = false;
    let mut all_costs = true;
    for position in positions {
        if let Some(amount) = &position.value_eth {
            value = value.saturating_add(parse_raw(&amount.raw));
            value_known = true;
        }
        if let Some(amount) = &position.estimated_annual_yield_eth {
            estimated = estimated.saturating_add(parse_raw(&amount.raw));
            estimated_known = true;
        }
        match (&position.cost_basis_eth, &position.pnl_eth) {
            (Some(basis), Some(delta)) => {
                cost = cost.saturating_add(parse_raw(&basis.raw));
                pnl += delta.raw.parse::<i128>().unwrap_or(0);
            }
            _ => all_costs = false,
        }
    }
    PositionTotals {
        position_count: positions.len(),
        active_count: positions
            .iter()
            .filter(|position| position.status == "active")
            .count(),
        value_eth: value_known.then(|| Amount::from_raw(value, 18)),
        estimated_annual_yield_eth: estimated_known.then(|| Amount::from_raw(estimated, 18)),
        cost_basis_eth: all_costs.then(|| Amount::from_raw(cost, 18)),
        pnl_eth: all_costs.then(|| signed_amount(pnl, 18)),
    }
}

fn parse_raw(value: &str) -> U256 {
    value.parse().unwrap_or_default()
}

fn annual_yield(value: U256, apy_percent: f64) -> Option<Amount> {
    if !apy_percent.is_finite() || apy_percent <= 0.0 {
        return Some(Amount::from_raw(U256::ZERO, 18));
    }
    let bps = (apy_percent * 100.0).round();
    if bps > u64::MAX as f64 {
        return None;
    }
    let bps = U256::from(bps as u64);
    let raw = value
        .checked_mul(bps)
        .unwrap_or(U256::MAX)
        .checked_div(U256::from(10_000u64))
        .unwrap_or_default();
    Some(Amount::from_raw(raw, 18))
}

fn signed_difference(value: U256, cost: U256) -> Option<SignedAmount> {
    let value = i128::try_from(value).ok()?;
    let cost = i128::try_from(cost).ok()?;
    Some(signed_amount(value - cost, 18))
}

fn percent_difference(value: U256, cost: U256) -> Option<f64> {
    let value = value.to_string().parse::<f64>().ok()?;
    let cost = cost.to_string().parse::<f64>().ok()?;
    if cost <= 0.0 {
        return None;
    }
    Some((value - cost) / cost * 100.0)
}

fn signed_amount(value: i128, decimals: u8) -> SignedAmount {
    let negative = value < 0;
    let absolute = value.unsigned_abs();
    let text = absolute.to_string();
    let decimals = usize::from(decimals);
    let decimal = if decimals == 0 {
        text
    } else if text.len() <= decimals {
        format!(
            "0.{}{}",
            "0".repeat(decimals - text.len()),
            text.trim_end_matches('0')
        )
    } else {
        let (integer, fraction) = text.split_at(text.len() - decimals);
        let fraction = fraction.trim_end_matches('0');
        if fraction.is_empty() {
            integer.to_string()
        } else {
            format!("{integer}.{fraction}")
        }
    };
    let decimal = decimal.trim_end_matches('.').to_string();
    let raw = if negative {
        format!("-{absolute}")
    } else {
        absolute.to_string()
    };
    let decimal = if negative {
        format!("-{decimal}")
    } else {
        decimal
    };
    SignedAmount { raw, decimal }
}

fn parse_units(input: &str, decimals: u8) -> Result<U256, PositionError> {
    let input = input.trim();
    let (integer, fraction) = input.split_once('.').unwrap_or((input, ""));
    if input.is_empty()
        || (integer.is_empty() && fraction.is_empty())
        || !integer.chars().all(|ch| ch.is_ascii_digit())
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
    {
        return Err(PositionError::Invalid(format!("invalid amount: {input}")));
    }
    let decimals = usize::from(decimals);
    if fraction.len() > decimals {
        return Err(PositionError::Invalid(format!(
            "amount has more than {decimals} decimal places"
        )));
    }
    let mut raw = String::with_capacity(integer.len() + decimals);
    raw.push_str(if integer.is_empty() { "0" } else { integer });
    raw.push_str(fraction);
    raw.push_str(&"0".repeat(decimals - fraction.len()));
    raw.parse()
        .map_err(|_| PositionError::Invalid(format!("amount is too large: {input}")))
}

fn word_address(address: Address) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(address.as_slice());
    word
}

fn word_u256(value: U256) -> [u8; 32] {
    value.to_be_bytes()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn request_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("eollo-{}-{nanos}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_decimal_cost_basis() {
        assert_eq!(
            parse_units("1.5", 18).unwrap(),
            U256::from(15u64) * U256::from(10u64).pow(U256::from(17u64))
        );
        assert!(parse_units("-1", 18).is_err());
    }

    #[test]
    fn formats_signed_amounts() {
        let amount = signed_amount(-1_500_000_000_000_000_000, 18);
        assert_eq!(amount.raw, "-1500000000000000000");
        assert_eq!(amount.decimal, "-1.5");
    }

    #[test]
    fn maps_supported_conversions() {
        assert!(matches!(
            conversion("lido-steth"),
            Some(Conversion::OneToOne)
        ));
        assert!(matches!(
            conversion("rocket-pool-reth"),
            Some(Conversion::Rate(_))
        ));
        assert!(matches!(
            conversion("morpho-steakhouse-eth"),
            Some(Conversion::ConvertToAssets)
        ));
        assert!(conversion("kelp-rseth").is_none());
    }
}
