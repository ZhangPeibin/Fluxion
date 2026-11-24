//! Historical APY series for a curated yield opportunity.

use eollo_protocol::capability::SCHEMA_VERSION;
use eollo_protocol::yield_capability::{
    YieldHistoryPoint, YieldHistoryRequest, YieldHistoryResponse, YieldHistorySummary, YieldSource,
};

use super::RegistryEntry;
use super::YieldError;
use super::provider::HistoryPoint;

pub const DISCLAIMER: &str = "Historical APY is not current APY and is not a guarantee.";
pub const COVERAGE: &str = "defillama-chart";

pub fn build_response(
    entry: &RegistryEntry,
    points: Vec<HistoryPoint>,
    request: &YieldHistoryRequest,
    fetched_at: u64,
    now: u64,
) -> Result<YieldHistoryResponse, YieldError> {
    if request.days == 0 || request.days > 365 {
        return Err(YieldError::Invalid(
            "days must be between 1 and 365".to_string(),
        ));
    }
    if request.max_points == 0 || request.max_points > 2000 {
        return Err(YieldError::Invalid(
            "max_points must be between 1 and 2000".to_string(),
        ));
    }
    let cutoff = now.saturating_sub(u64::from(request.days) * 86_400);
    let filtered = points
        .iter()
        .filter(|point| point.timestamp >= cutoff)
        .collect::<Vec<_>>();
    let mut warnings = Vec::new();
    if filtered.is_empty() {
        warnings.push(format!(
            "no data points in the last {} days; use a longer window",
            request.days
        ));
    }
    let as_of = points.last().map_or(0, |point| point.timestamp);
    if as_of > 0 && now.saturating_sub(as_of) > 6 * 3_600 {
        warnings.push(format!(
            "latest history point is {}s old",
            now.saturating_sub(as_of)
        ));
    }
    let mut summary = summarize(&filtered);
    let series = downsample(filtered, request.max_points);
    summary.points = series.len();
    let pool_id = entry.defillama_pool.clone().unwrap_or_default();
    Ok(YieldHistoryResponse {
        schema_version: SCHEMA_VERSION,
        capability: "yield.history".to_string(),
        request_id: request_id(),
        chain: "ethereum".to_string(),
        opportunity_id: entry.id.clone(),
        days: request.days,
        series,
        summary,
        as_of,
        coverage: COVERAGE.to_string(),
        sources: vec![YieldSource {
            name: "defillama-yields-chart".to_string(),
            url: format!("https://yields.llama.fi/chart/{pool_id}"),
            fetched_at,
        }],
        disclaimer: DISCLAIMER.to_string(),
        warnings,
    })
}

fn summarize(points: &[&HistoryPoint]) -> YieldHistorySummary {
    let apys = points
        .iter()
        .filter_map(|point| point.apy.or(point.apy_base))
        .collect::<Vec<_>>();
    let bases = points
        .iter()
        .filter_map(|point| point.apy_base)
        .collect::<Vec<_>>();
    let rewards = points
        .iter()
        .filter_map(|point| point.apy_reward)
        .collect::<Vec<_>>();
    let latest = points.last().and_then(|point| point.apy.or(point.apy_base));
    let first = apys.first().copied();
    let apy_change_bps = match (first, latest) {
        (Some(first), Some(latest)) if first > 0.0 => Some((latest - first) / first * 10_000.0),
        _ => None,
    };
    YieldHistorySummary {
        apy_mean_percent: mean(&apys),
        apy_min_percent: min(&apys),
        apy_max_percent: max(&apys),
        apy_latest_percent: latest,
        apy_change_bps,
        base_apy_mean_percent: mean(&bases),
        reward_apy_mean_percent: mean(&rewards),
        points: points.len(),
    }
}

fn downsample(points: Vec<&HistoryPoint>, max_points: usize) -> Vec<YieldHistoryPoint> {
    if points.is_empty() {
        return Vec::new();
    }
    let step = points.len().div_ceil(max_points).max(1);
    let mut sampled = points
        .iter()
        .enumerate()
        .filter(|(index, _)| index % step == 0)
        .map(|(_, point)| point_response(point))
        .collect::<Vec<_>>();
    if sampled.last().map(|point| point.timestamp) != points.last().map(|point| point.timestamp) {
        sampled.push(point_response(points.last().unwrap()));
    }
    sampled
}

fn point_response(point: &HistoryPoint) -> YieldHistoryPoint {
    YieldHistoryPoint {
        timestamp: point.timestamp,
        apy_percent: point.apy,
        base_apy_percent: point.apy_base,
        reward_apy_percent: point.apy_reward,
        tvl_usd: point.tvl_usd,
    }
}

fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn min(values: &[f64]) -> Option<f64> {
    values.iter().copied().reduce(f64::min)
}

fn max(values: &[f64]) -> Option<f64> {
    values.iter().copied().reduce(f64::max)
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

    fn point(timestamp: u64, apy: f64) -> HistoryPoint {
        HistoryPoint {
            timestamp,
            apy: Some(apy),
            apy_base: Some(apy),
            apy_reward: None,
            tvl_usd: Some(1),
        }
    }

    fn entry() -> RegistryEntry {
        super::super::Registry::load_default()
            .unwrap()
            .entries()
            .iter()
            .find(|entry| entry.id == "lido-steth")
            .unwrap()
            .clone()
    }

    #[test]
    fn summarizes_and_filters_history() {
        let request = YieldHistoryRequest {
            opportunity_id: "lido-steth".to_string(),
            days: 10,
            max_points: 100,
        };
        let response = build_response(
            &entry(),
            vec![point(1_000, 1.0), point(2_000, 2.0), point(3_000, 3.0)],
            &request,
            4_000,
            4_000,
        )
        .unwrap();
        assert_eq!(response.series.len(), 3);
        assert_eq!(response.summary.apy_latest_percent, Some(3.0));
        assert_eq!(response.summary.apy_mean_percent, Some(2.0));
        assert_eq!(response.as_of, 3_000);
    }

    #[test]
    fn downsamples_to_the_requested_cap() {
        let request = YieldHistoryRequest {
            opportunity_id: "lido-steth".to_string(),
            days: 10,
            max_points: 2,
        };
        let points = (0..10)
            .map(|index| point(1_000 + index, index as f64))
            .collect();
        let response = build_response(&entry(), points, &request, 4_000, 4_000).unwrap();
        assert!(response.series.len() <= 3);
        assert_eq!(response.series.last().unwrap().timestamp, 1_009);
    }

    #[test]
    fn rejects_invalid_windows() {
        let request = YieldHistoryRequest {
            opportunity_id: "lido-steth".to_string(),
            days: 0,
            max_points: 10,
        };
        assert!(matches!(
            build_response(&entry(), vec![point(1, 1.0)], &request, 1, 1),
            Err(YieldError::Invalid(_))
        ));
    }
}
