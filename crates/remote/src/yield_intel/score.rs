//! Safety-tier filtering, ranking and recommendation.

use std::collections::HashSet;

use alloy_primitives::U256;
use eollo_protocol::capability::{Amount, SCHEMA_VERSION};
use eollo_protocol::yield_capability::{
    RiskProfile, SafetyTier, YieldAction, YieldAsset, YieldDiscoverRequest, YieldDiscoverResponse,
    YieldEvidence, YieldExclusion, YieldInfo, YieldMarket, YieldOpportunity, YieldProtocol,
    YieldRecommendation, YieldRiskFactor, YieldSource, YieldWithdrawal,
};

use super::RegistryEntry;
use super::YieldError;
use super::provider::{OnChainCheck, ProtocolMeta, tvl_to_u64};

pub const DISCLAIMER: &str =
    "Safety tiers are Eollo's curated confidence labels, not a guarantee. APY is variable.";
pub const COVERAGE: &str = "curated-universe+defillama+onchain";

#[derive(Debug, Clone)]
pub struct BaseOpportunity {
    pub entry: RegistryEntry,
    pub protocol: YieldProtocol,
    pub market: YieldMarket,
    pub yield_info: YieldInfo,
    pub ranking_apy_base: f64,
    pub ranking_apy_total: f64,
    pub risk_factors: Vec<YieldRiskFactor>,
    pub evidence: Vec<YieldEvidence>,
    pub data_confidence: String,
    pub warnings: Vec<String>,
    pub market_tvl_usd: u64,
}

#[derive(Debug, Clone)]
pub struct YieldSnapshot {
    pub as_of: u64,
    pub opportunities: Vec<BaseOpportunity>,
    pub excluded: Vec<YieldExclusion>,
    pub sources: Vec<YieldSource>,
    pub warnings: Vec<String>,
}

pub struct PoolMatch<'a> {
    pub pool: &'a super::provider::LlamaPool,
    pub explicit: bool,
}

pub fn build_snapshot(
    registry: &[RegistryEntry],
    pools: &[super::provider::LlamaPool],
    protocol_meta: &std::collections::HashMap<String, ProtocolMeta>,
    checks: &std::collections::HashMap<String, OnChainCheck>,
    as_of: u64,
    sources: Vec<YieldSource>,
    warnings: Vec<String>,
) -> YieldSnapshot {
    let mut opportunities = Vec::new();
    let mut excluded = Vec::new();
    let mut all_warnings = warnings;
    for entry in registry {
        let Some(found) = find_pool(entry, pools) else {
            excluded.push(YieldExclusion {
                id: entry.id.clone(),
                reason: "NO_LIVE_DATA".to_string(),
            });
            all_warnings.push(format!("no DefiLlama pool matched {}", entry.id));
            continue;
        };
        let pool = found.pool;
        if pool.outlier {
            excluded.push(YieldExclusion {
                id: entry.id.clone(),
                reason: "OUTLIER".to_string(),
            });
            continue;
        }
        let check = checks.get(&entry.id).cloned().unwrap_or_default();
        let meta = protocol_meta
            .get(&entry.protocol_slug)
            .cloned()
            .unwrap_or_else(|| ProtocolMeta {
                slug: entry.protocol_slug.clone(),
                name: entry.protocol_name.clone(),
                category: entry.category.clone(),
                tvl_usd: None,
                audits: None,
                audit_links: Vec::new(),
                listed_at: None,
                incident_count: 0,
            });
        let mut item_warnings = check.warnings.clone();
        all_warnings.extend(item_warnings.iter().cloned());
        if !protocol_meta.contains_key(&entry.protocol_slug) {
            item_warnings.push(format!(
                "DefiLlama protocol metadata missing for {}",
                entry.protocol_slug
            ));
        }

        let apy = pool.apy.or(pool.apy_base).unwrap_or(0.0);
        let base = pool.apy_base;
        let reward = pool.apy_reward;
        let mean30 = pool.apy_mean_30d;
        let ranking_base = mean30.or(base).unwrap_or(0.0);
        let ranking_total = mean30.or(Some(apy)).or(base).unwrap_or(0.0);
        let mut risk_factors = entry
            .risk_notes
            .iter()
            .map(|note| YieldRiskFactor {
                code: note.code.clone(),
                level: note.level.clone(),
                evidence: note.evidence.clone(),
            })
            .collect::<Vec<_>>();
        if reward.is_some_and(|value| value > 0.0) {
            risk_factors.push(YieldRiskFactor {
                code: "REWARD_DEPENDENCY".to_string(),
                level: "medium".to_string(),
                evidence: format!(
                    "Reward APY is {:.2} percentage points of the total.",
                    reward.unwrap_or_default()
                ),
            });
        }
        if meta.incident_count > 0 {
            risk_factors.push(YieldRiskFactor {
                code: "INCIDENT_HISTORY".to_string(),
                level: "medium".to_string(),
                evidence: format!("{} historical incident(s) reported", meta.incident_count),
            });
        }
        if !check.contract_has_code || !check.receipt_has_code {
            risk_factors.push(YieldRiskFactor {
                code: "CONTRACT_VERIFICATION".to_string(),
                level: "high".to_string(),
                evidence: "On-chain code verification failed or is incomplete.".to_string(),
            });
        }

        let explicit = found.explicit;
        let data_confidence = confidence(explicit, &check, &meta, &item_warnings);

        let mut evidence = vec![
            YieldEvidence {
                source: "defillama-yields".to_string(),
                fact: "pool_id".to_string(),
                value: pool.pool.clone(),
            },
            YieldEvidence {
                source: "defillama-yields".to_string(),
                fact: "market_tvl_usd".to_string(),
                value: tvl_to_u64(pool.tvl_usd.unwrap_or_default()).to_string(),
            },
            YieldEvidence {
                source: "defillama-protocols".to_string(),
                fact: "audits".to_string(),
                value: meta
                    .audits
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
            },
            YieldEvidence {
                source: "ethereum-rpc".to_string(),
                fact: "contract_code".to_string(),
                value: check.contract_has_code.to_string(),
            },
            YieldEvidence {
                source: "ethereum-rpc".to_string(),
                fact: "receipt_asset".to_string(),
                value: format!(
                    "{} / {}",
                    check.receipt_symbol.as_deref().unwrap_or("unknown"),
                    check
                        .receipt_decimals
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "?".to_string())
                ),
            },
        ];
        if let Some(listed_at) = meta.listed_at {
            evidence.push(YieldEvidence {
                source: "defillama-protocols".to_string(),
                fact: "listed_at".to_string(),
                value: listed_at.to_string(),
            });
        }

        opportunities.push(BaseOpportunity {
            entry: entry.clone(),
            protocol: YieldProtocol {
                slug: meta.slug,
                name: meta.name,
                category: meta.category,
                tvl_usd: meta.tvl_usd,
                audits: meta.audits,
                audit_links: meta.audit_links,
                listed_at: meta.listed_at,
                incident_count: Some(meta.incident_count),
            },
            market: YieldMarket {
                name: entry.market_name.clone(),
                tvl_usd: tvl_to_u64(pool.tvl_usd.unwrap_or_default()),
                contract: entry.contract,
                deposit_asset: YieldAsset {
                    symbol: entry.deposit_asset_symbol.clone(),
                    address: entry.deposit_asset_address.clone(),
                    decimals: entry.deposit_decimals,
                },
                receipt_asset: YieldAsset {
                    symbol: entry.receipt_asset_symbol.clone(),
                    address: format!("{:#x}", entry.receipt_asset_address),
                    decimals: entry.receipt_decimals,
                },
                requires_wrap: entry.requires_wrap,
                withdrawal: YieldWithdrawal {
                    mode: entry.withdrawal.mode.clone(),
                    lockup_days: entry.withdrawal.lockup_days,
                    notes: entry.withdrawal.notes.clone(),
                },
            },
            yield_info: YieldInfo {
                apy_percent: apy,
                base_apy_percent: base,
                reward_apy_percent: reward,
                apy_mean_30d_percent: mean30,
                apy_pct_30d: pool.apy_pct_30d,
                ranking_apy_percent: ranking_base,
                yield_type: match entry.action {
                    YieldAction::Stake => "staking_apr".to_string(),
                    YieldAction::Lend => "supply_apy".to_string(),
                },
                methodology: "DefiLlama pool metrics; ranking uses 30-day mean of base APY."
                    .to_string(),
            },
            ranking_apy_base: ranking_base,
            ranking_apy_total: ranking_total,
            risk_factors,
            evidence,
            data_confidence,
            warnings: item_warnings,
            market_tvl_usd: tvl_to_u64(pool.tvl_usd.unwrap_or_default()),
        });
    }
    YieldSnapshot {
        as_of,
        opportunities,
        excluded,
        sources,
        warnings: all_warnings,
    }
}

fn find_pool<'a>(
    entry: &RegistryEntry,
    pools: &'a [super::provider::LlamaPool],
) -> Option<PoolMatch<'a>> {
    if let Some(id) = &entry.defillama_pool
        && let Some(pool) = pools
            .iter()
            .find(|pool| pool.pool == *id && pool.chain.eq_ignore_ascii_case(&entry.chain))
    {
        return Some(PoolMatch {
            pool,
            explicit: true,
        });
    }
    pools
        .iter()
        .filter(|pool| {
            pool.chain.eq_ignore_ascii_case(&entry.chain)
                && pool.project.eq_ignore_ascii_case(&entry.defillama_project)
                && pool.symbol.eq_ignore_ascii_case(&entry.defillama_symbol)
                && entry
                    .defillama_pool_meta
                    .as_deref()
                    .is_none_or(|meta| pool.pool_meta.as_deref() == Some(meta))
        })
        .max_by(|left, right| {
            left.tvl_usd
                .unwrap_or_default()
                .total_cmp(&right.tvl_usd.unwrap_or_default())
        })
        .map(|pool| PoolMatch {
            pool,
            explicit: false,
        })
}

fn confidence(
    explicit: bool,
    check: &OnChainCheck,
    meta: &ProtocolMeta,
    warnings: &[String],
) -> String {
    if !check.contract_has_code || !check.receipt_has_code {
        return "low".to_string();
    }
    if explicit && meta.audits.is_some_and(|count| count > 0) && warnings.is_empty() {
        "high".to_string()
    } else {
        "medium".to_string()
    }
}

pub fn discover(
    snapshot: &YieldSnapshot,
    request: &YieldDiscoverRequest,
    now: u64,
    max_age_secs: u64,
    hard_max_age_secs: u64,
) -> Result<YieldDiscoverResponse, YieldError> {
    validate_request(request)?;
    let age_secs = now.saturating_sub(snapshot.as_of);
    if age_secs > hard_max_age_secs {
        return Err(YieldError::Stale);
    }
    let stale = age_secs > max_age_secs;
    let amount = request
        .amount
        .as_deref()
        .map(|value| parse_units(value, 18))
        .transpose()?;
    let profile = request.risk_profile;
    let default_min_tvl = match profile {
        RiskProfile::Conservative => 250_000_000,
        RiskProfile::Balanced => 100_000_000,
        RiskProfile::Aggressive => 10_000_000,
    };
    let default_lockup = match profile {
        RiskProfile::Conservative => 0,
        RiskProfile::Balanced => 7,
        RiskProfile::Aggressive => 30,
    };
    let min_tvl = request.min_tvl_usd.unwrap_or(default_min_tvl);
    let max_lockup = request.max_lockup_days.unwrap_or(default_lockup);
    let include_reward = request.include_reward_apy.unwrap_or(false);
    let protocols = request
        .protocols
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let mut selected = Vec::new();
    let mut excluded = snapshot.excluded.clone();
    let mut warnings = snapshot.warnings.clone();
    if stale {
        warnings.push(format!(
            "yield snapshot is {age_secs}s old; values may have moved"
        ));
    }
    for item in &snapshot.opportunities {
        if !item.entry.chain.eq_ignore_ascii_case(&request.chain) {
            continue;
        }
        if let Some(reason) = exclusion_reason(
            item,
            profile,
            min_tvl,
            max_lockup,
            include_reward,
            &protocols,
        ) {
            excluded.push(YieldExclusion {
                id: item.entry.id.clone(),
                reason: reason.to_string(),
            });
            continue;
        }
        let ranking = if include_reward {
            item.ranking_apy_total
        } else {
            item.ranking_apy_base
        };
        let mut opportunity = opportunity_response(item, ranking, amount);
        opportunity.sources = snapshot.sources.clone();
        if include_reward
            && item
                .yield_info
                .reward_apy_percent
                .is_some_and(|value| value > 0.0)
        {
            opportunity.warnings.push(
                "ranking includes reward APY; reward emissions can change or stop".to_string(),
            );
        }
        selected.push(opportunity);
    }
    selected.sort_by(|left, right| {
        right
            .yield_info
            .ranking_apy_percent
            .total_cmp(&left.yield_info.ranking_apy_percent)
            .then_with(|| tier_rank(left.safety_tier).cmp(&tier_rank(right.safety_tier)))
            .then_with(|| right.market.tvl_usd.cmp(&left.market.tvl_usd))
            .then_with(|| left.id.cmp(&right.id))
    });
    selected.truncate(request.limit);

    let recommended = selected
        .iter()
        .max_by(|left, right| {
            left.yield_info
                .ranking_apy_percent
                .total_cmp(&right.yield_info.ranking_apy_percent)
                .then_with(|| left.market.tvl_usd.cmp(&right.market.tvl_usd))
        })
        .map(|best| recommendation(best, &selected, include_reward));
    if selected.is_empty() {
        warnings.push("no opportunity satisfies the requested constraints".to_string());
    }
    dedupe_exclusions(&mut excluded);

    Ok(YieldDiscoverResponse {
        schema_version: SCHEMA_VERSION,
        capability: "yield.discover".to_string(),
        request_id: request_id(),
        chain: request.chain.clone(),
        asset: YieldAsset {
            symbol: request.asset.to_ascii_uppercase(),
            address: "native".to_string(),
            decimals: 18,
        },
        amount: amount.map(|value| Amount::from_raw(value, 18)),
        risk_profile: profile,
        objective: "highest_apy_within_eligible_safety_tiers".to_string(),
        recommended,
        opportunities: selected,
        excluded,
        as_of: snapshot.as_of,
        age_secs,
        stale,
        coverage: COVERAGE.to_string(),
        sources: snapshot.sources.clone(),
        disclaimer: DISCLAIMER.to_string(),
        warnings,
    })
}

fn validate_request(request: &YieldDiscoverRequest) -> Result<(), YieldError> {
    let native_asset = match request.chain.to_ascii_lowercase().as_str() {
        "ethereum" => "ETH",
        "bsc" | "bnb" => "BNB",
        other => {
            return Err(YieldError::Invalid(format!(
                "yield.discover does not support chain={other}"
            )));
        }
    };
    if !request.asset.eq_ignore_ascii_case(native_asset) {
        return Err(YieldError::Invalid(format!(
            "yield.discover on chain={} requires asset={native_asset}",
            request.chain
        )));
    }
    if !(1..=20).contains(&request.limit) {
        return Err(YieldError::Invalid(
            "limit must be between 1 and 20".to_string(),
        ));
    }
    Ok(())
}

fn exclusion_reason(
    item: &BaseOpportunity,
    profile: RiskProfile,
    min_tvl: u64,
    max_lockup: u32,
    include_reward: bool,
    protocols: &HashSet<String>,
) -> Option<&'static str> {
    if !tier_allowed(item.entry.safety_tier, profile) {
        return Some("TIER_NOT_ELIGIBLE");
    }
    if !protocols.is_empty() && !protocols.contains(&item.entry.protocol_slug.to_ascii_lowercase())
    {
        return Some("PROTOCOL_NOT_SELECTED");
    }
    if item.market_tvl_usd < min_tvl {
        return Some("TVL_BELOW_MIN");
    }
    if item
        .entry
        .withdrawal
        .lockup_days
        .is_some_and(|days| days > max_lockup)
    {
        return Some("LOCKUP_EXCEEDS_MAX");
    }
    if !include_reward && item.ranking_apy_base <= 0.0 {
        return Some("NO_BASE_APY");
    }
    if item.data_confidence == "low" {
        return Some("LOW_CONFIDENCE");
    }
    None
}

fn tier_allowed(tier: SafetyTier, profile: RiskProfile) -> bool {
    match profile {
        RiskProfile::Conservative => tier == SafetyTier::Core,
        RiskProfile::Balanced => matches!(tier, SafetyTier::Core | SafetyTier::Established),
        RiskProfile::Aggressive => true,
    }
}

fn tier_rank(tier: SafetyTier) -> u8 {
    match tier {
        SafetyTier::Core => 0,
        SafetyTier::Established => 1,
        SafetyTier::Experimental => 2,
    }
}

fn opportunity_response(
    item: &BaseOpportunity,
    ranking_apy: f64,
    amount: Option<U256>,
) -> YieldOpportunity {
    let mut yield_info = item.yield_info.clone();
    yield_info.ranking_apy_percent = ranking_apy;
    YieldOpportunity {
        id: item.entry.id.clone(),
        safety_tier: item.entry.safety_tier,
        protocol: item.protocol.clone(),
        action: item.entry.action,
        market: item.market.clone(),
        yield_info,
        risk_factors: item.risk_factors.clone(),
        evidence: item.evidence.clone(),
        data_confidence: item.data_confidence.clone(),
        estimated_annual_yield: amount.and_then(|amount| annual_yield(amount, ranking_apy)),
        sources: Vec::new(),
        warnings: item.warnings.clone(),
    }
}

fn recommendation(
    best: &YieldOpportunity,
    selected: &[YieldOpportunity],
    include_reward: bool,
) -> YieldRecommendation {
    let mut tradeoffs = Vec::new();
    if best.safety_tier != SafetyTier::Core {
        if let Some(core) = selected
            .iter()
            .find(|item| item.safety_tier == SafetyTier::Core)
        {
            tradeoffs.push(format!(
                "{} is {} tier, not core; core best is {} at {:.2}% APY",
                best.id,
                best.safety_tier.as_str(),
                core.id,
                core.yield_info.ranking_apy_percent
            ));
        } else {
            tradeoffs.push(format!(
                "{} is {} tier, not core",
                best.id,
                best.safety_tier.as_str()
            ));
        }
    }
    if let Some(reward) = best.yield_info.reward_apy_percent
        && reward > 0.0
    {
        tradeoffs.push(format!(
            "reward APY is {reward:.2} percentage points{}",
            if include_reward {
                " and is included in ranking"
            } else {
                " and is excluded from ranking"
            }
        ));
    }
    if let Some(days) = best.market.withdrawal.lockup_days
        && days > 0
    {
        tradeoffs.push(format!("withdrawal lockup is up to {days} day(s)"));
    }
    if let Some(deposit) = best
        .yield_info
        .base_apy_percent
        .filter(|_| best.yield_info.ranking_apy_percent > 0.0)
    {
        tradeoffs.push(format!(
            "base APY is {deposit:.2}%; ranking uses {:.2}%",
            best.yield_info.ranking_apy_percent
        ));
    }
    if tradeoffs.is_empty() {
        tradeoffs
            .push("No higher-yield eligible opportunity cleared the same safety tier.".to_string());
    }
    YieldRecommendation {
        opportunity_id: best.id.clone(),
        reason: format!(
            "Highest ranking APY among eligible opportunities ({} tier).",
            best.safety_tier.as_str()
        ),
        tradeoffs,
    }
}

fn dedupe_exclusions(excluded: &mut Vec<YieldExclusion>) {
    let mut seen = HashSet::new();
    excluded.retain(|item| seen.insert(item.id.clone()));
}

fn annual_yield(amount: U256, apy_percent: f64) -> Option<Amount> {
    if !apy_percent.is_finite() || apy_percent <= 0.0 {
        return None;
    }
    let bps = (apy_percent * 100.0).round();
    if bps > u64::MAX as f64 {
        return None;
    }
    let bps = U256::from(bps as u64);
    let raw = amount
        .checked_mul(bps)
        .unwrap_or(U256::MAX)
        .checked_div(U256::from(10_000u64))
        .unwrap_or_default();
    Some(Amount::from_raw(raw, 18))
}

fn parse_units(input: &str, decimals: u8) -> Result<U256, YieldError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(YieldError::Invalid("amount is empty".to_string()));
    }
    let (integer, fraction) = input.split_once('.').unwrap_or((input, ""));
    if integer.is_empty() && fraction.is_empty() {
        return Err(YieldError::Invalid("invalid amount".to_string()));
    }
    if !integer.chars().all(|ch| ch.is_ascii_digit())
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
    {
        return Err(YieldError::Invalid(format!("invalid amount: {input}")));
    }
    let decimals = usize::from(decimals);
    if fraction.len() > decimals {
        return Err(YieldError::Invalid(format!(
            "amount has more than {decimals} decimal places"
        )));
    }
    let mut raw = String::with_capacity(integer.len() + decimals);
    raw.push_str(if integer.is_empty() { "0" } else { integer });
    raw.push_str(fraction);
    raw.push_str(&"0".repeat(decimals - fraction.len()));
    raw.parse()
        .map_err(|_| YieldError::Invalid(format!("amount is too large: {input}")))
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
    fn parses_human_amounts() {
        assert_eq!(
            parse_units("10", 18).unwrap(),
            U256::from(10u64) * U256::from(10u64).pow(U256::from(18u64))
        );
        assert_eq!(
            parse_units("0.5", 18).unwrap(),
            U256::from(5u64) * U256::from(10u64).pow(U256::from(17u64))
        );
        assert!(parse_units("-1", 18).is_err());
        assert!(parse_units("1.0000000000000000001", 18).is_err());
    }

    fn base_from_entry(entry: RegistryEntry, ranking: f64, tvl: u64) -> BaseOpportunity {
        BaseOpportunity {
            protocol: YieldProtocol {
                slug: entry.protocol_slug.clone(),
                name: entry.protocol_name.clone(),
                category: entry.category.clone(),
                tvl_usd: Some(tvl),
                audits: Some(2),
                audit_links: vec![],
                listed_at: Some(1),
                incident_count: Some(0),
            },
            market: YieldMarket {
                name: entry.market_name.clone(),
                tvl_usd: tvl,
                contract: entry.contract,
                deposit_asset: YieldAsset {
                    symbol: entry.deposit_asset_symbol.clone(),
                    address: entry.deposit_asset_address.clone(),
                    decimals: entry.deposit_decimals,
                },
                receipt_asset: YieldAsset {
                    symbol: entry.receipt_asset_symbol.clone(),
                    address: format!("{:#x}", entry.receipt_asset_address),
                    decimals: entry.receipt_decimals,
                },
                requires_wrap: entry.requires_wrap,
                withdrawal: YieldWithdrawal {
                    mode: entry.withdrawal.mode.clone(),
                    lockup_days: entry.withdrawal.lockup_days,
                    notes: entry.withdrawal.notes.clone(),
                },
            },
            yield_info: YieldInfo {
                apy_percent: ranking,
                base_apy_percent: Some(ranking),
                reward_apy_percent: None,
                apy_mean_30d_percent: Some(ranking),
                apy_pct_30d: None,
                ranking_apy_percent: ranking,
                yield_type: "staking_apr".to_string(),
                methodology: "test".to_string(),
            },
            ranking_apy_base: ranking,
            ranking_apy_total: ranking,
            risk_factors: vec![],
            evidence: vec![],
            data_confidence: "high".to_string(),
            warnings: vec![],
            market_tvl_usd: tvl,
            entry,
        }
    }

    fn snapshot() -> YieldSnapshot {
        let registry = eollo_protocol::yield_capability::Registry::load_default().unwrap();
        let mut opportunities = Vec::new();
        for entry in registry.entries() {
            let apy = match entry.id.as_str() {
                "lido-steth" => 2.69,
                "rocket-pool-reth" => 1.96,
                "aave-v3-weth" => 1.39,
                "sparklend-weth" => 1.60,
                "etherfi-weeth" => 2.57,
                "kelp-rseth" => 2.76,
                "cbeth" => 2.57,
                "renzo-ezeth" => 1.94,
                "morpho-steakhouse-eth" => 1.98,
                "morpho-gauntlet-weth" => 2.10,
                _ => 0.5,
            };
            let tvl = match entry.id.as_str() {
                "lido-steth" => 17_000_000_000,
                "rocket-pool-reth" => 869_704_903,
                "aave-v3-weth" => 559_214_208,
                "sparklend-weth" => 110_014_192,
                "etherfi-weeth" => 3_926_252_487,
                "kelp-rseth" => 648_457_534,
                "cbeth" => 360_421_380,
                "renzo-ezeth" => 76_345_462,
                "morpho-steakhouse-eth" => 21_333_321,
                "morpho-gauntlet-weth" => 18_266_440,
                _ => 1_000_000_000,
            };
            opportunities.push(base_from_entry(entry.clone(), apy, tvl));
        }
        YieldSnapshot {
            as_of: 1_000,
            opportunities,
            excluded: vec![],
            sources: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn lending_receipt_symbol_does_not_lower_confidence() {
        let check = OnChainCheck {
            contract_has_code: true,
            receipt_has_code: true,
            receipt_symbol: Some("aEthWETH".to_string()),
            receipt_decimals: Some(18),
            warnings: vec![],
        };
        let meta = ProtocolMeta {
            slug: "aave-v3".to_string(),
            name: "Aave V3".to_string(),
            category: "Lending".to_string(),
            tvl_usd: Some(1),
            audits: Some(2),
            audit_links: vec![],
            listed_at: Some(1),
            incident_count: 0,
        };
        assert_eq!(confidence(true, &check, &meta, &[]), "high");
    }

    #[test]
    fn conservative_only_returns_core() {
        let response = discover(
            &snapshot(),
            &YieldDiscoverRequest {
                asset: "ETH".into(),
                ..test_request()
            },
            1_010,
            900,
            3600,
        )
        .unwrap();
        assert!(
            response
                .opportunities
                .iter()
                .all(|item| item.safety_tier == SafetyTier::Core)
        );
        assert_eq!(response.recommended.unwrap().opportunity_id, "lido-steth");
    }

    #[test]
    fn aggressive_includes_experimental_markets() {
        let response = discover(
            &snapshot(),
            &YieldDiscoverRequest {
                asset: "ETH".into(),
                risk_profile: RiskProfile::Aggressive,
                limit: 20,
                ..test_request()
            },
            1_010,
            900,
            3600,
        )
        .unwrap();
        assert!(
            response
                .opportunities
                .iter()
                .any(|item| item.safety_tier == SafetyTier::Experimental)
        );
        assert!(
            response
                .opportunities
                .iter()
                .any(|item| item.id == "morpho-steakhouse-eth")
        );
    }

    #[test]
    fn balanced_prefers_higher_yield_across_eligible_tiers() {
        let response = discover(
            &snapshot(),
            &YieldDiscoverRequest {
                asset: "ETH".into(),
                risk_profile: RiskProfile::Balanced,
                ..test_request()
            },
            1_010,
            900,
            3600,
        )
        .unwrap();
        assert!(
            response
                .opportunities
                .iter()
                .any(|item| item.safety_tier == SafetyTier::Established)
        );
        let recommended = response.recommended.unwrap();
        assert_eq!(recommended.opportunity_id, "kelp-rseth");
        assert!(
            recommended
                .tradeoffs
                .iter()
                .any(|line| line.contains("core best"))
        );
    }

    #[test]
    fn stale_snapshot_is_explicit() {
        let response = discover(
            &snapshot(),
            &YieldDiscoverRequest {
                asset: "ETH".into(),
                ..test_request()
            },
            2_000,
            900,
            3_600,
        )
        .unwrap();
        assert!(response.stale);
    }

    #[test]
    fn hard_stale_snapshot_fails() {
        let error = discover(
            &snapshot(),
            &YieldDiscoverRequest {
                asset: "ETH".into(),
                ..test_request()
            },
            5_000,
            900,
            3_600,
        )
        .unwrap_err();
        assert!(matches!(error, YieldError::Stale));
    }

    fn test_request() -> YieldDiscoverRequest {
        YieldDiscoverRequest {
            chain: "ethereum".into(),
            asset: "ETH".into(),
            amount: Some("10".into()),
            risk_profile: RiskProfile::Conservative,
            min_tvl_usd: None,
            max_lockup_days: None,
            include_reward_apy: None,
            protocols: vec![],
            limit: 10,
        }
    }

    #[test]
    fn profit_is_scaled_by_bps() {
        let one_eth = U256::from(10u64).pow(U256::from(18u64));
        let profit = annual_yield(one_eth, 2.5).unwrap();
        assert_eq!(profit.decimal, "0.025");
    }
}
