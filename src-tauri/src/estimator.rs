use std::cmp::Ordering;

use crate::models::ALGORITHM_VERSION;

#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    pub refresh_seconds: u64,
    pub monitoring_gap_minutes: u64,
    pub settlement_seconds: u64,
    pub hard_settlement_seconds: u64,
    pub minimum_decimal_quota_points: f64,
    pub minimum_whole_quota_points: f64,
    pub minimum_eligible_cost_usd: f64,
    pub minimum_events: u64,
    pub low_usage_quarantine_percent: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            refresh_seconds: 10,
            monitoring_gap_minutes: 5,
            settlement_seconds: 60,
            hard_settlement_seconds: 120,
            minimum_decimal_quota_points: 0.5,
            minimum_whole_quota_points: 3.0,
            minimum_eligible_cost_usd: 0.25,
            minimum_events: 2,
            low_usage_quarantine_percent: 3.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MeasurementDecision {
    Valid {
        quote_usd: f64,
        confidence: Confidence,
    },
    Pending(String),
    Rejected(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy)]
pub struct SettlementInput {
    pub cost_delta_usd: f64,
    pub quota_delta_points: f64,
    pub events: u64,
    pub decimal_quota: bool,
    pub sources_unchanged_for_seconds: u64,
    pub monotonic: bool,
    pub complete: bool,
    pub low_usage_percent: f64,
}

pub fn settle_interval(input: SettlementInput, thresholds: Thresholds) -> MeasurementDecision {
    if !input.complete {
        return MeasurementDecision::Pending("incomplete-data boundary".into());
    }
    if !input.monotonic {
        return MeasurementDecision::Rejected("quota or cost movement was not monotonic".into());
    }
    if !input.cost_delta_usd.is_finite() || !input.quota_delta_points.is_finite() {
        return MeasurementDecision::Rejected("non-finite measurement".into());
    }
    if input.cost_delta_usd <= 0.0 {
        return MeasurementDecision::Rejected("zero or negative eligible cost delta".into());
    }
    if input.quota_delta_points <= 0.0 {
        return MeasurementDecision::Rejected("zero or negative quota delta".into());
    }
    if input.low_usage_percent <= thresholds.low_usage_quarantine_percent {
        return MeasurementDecision::Pending("low-usage quarantine".into());
    }
    let minimum_quota = if input.decimal_quota {
        thresholds.minimum_decimal_quota_points
    } else {
        thresholds.minimum_whole_quota_points
    };
    if input.quota_delta_points < minimum_quota {
        return MeasurementDecision::Pending("quota movement below estimator threshold".into());
    }
    if input.cost_delta_usd < thresholds.minimum_eligible_cost_usd {
        return MeasurementDecision::Pending("eligible cost below estimator threshold".into());
    }
    if input.events < thresholds.minimum_events {
        return MeasurementDecision::Pending("not enough eligible events".into());
    }
    if input.sources_unchanged_for_seconds < thresholds.settlement_seconds {
        return MeasurementDecision::Pending("waiting for source settlement".into());
    }
    let quote_usd = input.cost_delta_usd / (input.quota_delta_points / 100.0);
    if !quote_usd.is_finite() || quote_usd <= 0.0 {
        return MeasurementDecision::Rejected("non-finite quote result".into());
    }
    let confidence = if input.quota_delta_points >= 5.0 {
        Confidence::High
    } else {
        Confidence::Medium
    };
    MeasurementDecision::Valid {
        quote_usd,
        confidence,
    }
}

pub fn median_latest_five(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let start = values.len().saturating_sub(5);
    let mut sorted: Vec<f64> = values[start..]
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        Some((sorted[middle - 1] + sorted[middle]) / 2.0)
    } else {
        Some(sorted[middle])
    }
}

pub fn workload_comparable(
    dominant_model_same: bool,
    cache_share_delta_points: f64,
    fast_share_delta_points: f64,
    long_context_share_delta_points: f64,
) -> bool {
    dominant_model_same
        && cache_share_delta_points.abs() <= 15.0
        && fast_share_delta_points.abs() <= 10.0
        && long_context_share_delta_points.abs() <= 10.0
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrendState {
    Stable,
    Watching,
    PossibleReduction,
    LikelyReduction,
    SustainedTrend,
}

pub fn classify_trend(
    comparable_declines: &[f64],
    reset_boundaries_spanned: u64,
    persisted_across_boundaries: bool,
) -> TrendState {
    let declines = comparable_declines
        .iter()
        .filter(|decline| **decline <= -10.0)
        .count();
    let likely = comparable_declines
        .iter()
        .filter(|decline| **decline <= -15.0)
        .count();
    if persisted_across_boundaries && reset_boundaries_spanned >= 2 && likely >= 5 {
        return TrendState::SustainedTrend;
    }
    if reset_boundaries_spanned >= 2 && likely >= 5 {
        return TrendState::LikelyReduction;
    }
    if declines >= 5 && likely >= 5 {
        return TrendState::LikelyReduction;
    }
    if declines >= 3 {
        return TrendState::PossibleReduction;
    }
    if declines >= 1 {
        return TrendState::Watching;
    }
    TrendState::Stable
}

pub fn algorithm_version() -> &'static str {
    ALGORITHM_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_settlement_and_then_quotes() {
        let thresholds = Thresholds::default();
        assert!(matches!(
            settle_interval(
                SettlementInput {
                    cost_delta_usd: 1.0,
                    quota_delta_points: 1.0,
                    events: 2,
                    decimal_quota: true,
                    sources_unchanged_for_seconds: 59,
                    monotonic: true,
                    complete: true,
                    low_usage_percent: 20.0
                },
                thresholds
            ),
            MeasurementDecision::Pending(_)
        ));
        assert!(
            matches!(settle_interval(SettlementInput { cost_delta_usd: 1.0, quota_delta_points: 1.0, events: 2, decimal_quota: true, sources_unchanged_for_seconds: 60, monotonic: true, complete: true, low_usage_percent: 20.0 }, thresholds), MeasurementDecision::Valid { quote_usd, .. } if (quote_usd - 100.0).abs() < f64::EPSILON)
        );
    }

    #[test]
    fn rejects_low_usage_without_fabricating_zero() {
        let decision = settle_interval(
            SettlementInput {
                cost_delta_usd: 4.0,
                quota_delta_points: 10.0,
                events: 10,
                decimal_quota: true,
                sources_unchanged_for_seconds: 120,
                monotonic: true,
                complete: true,
                low_usage_percent: 3.0,
            },
            Thresholds::default(),
        );
        assert_eq!(
            decision,
            MeasurementDecision::Pending("low-usage quarantine".into())
        );
    }

    #[test]
    fn compares_workloads_with_centralized_limits() {
        assert!(workload_comparable(true, 15.0, 10.0, 10.0));
        assert!(!workload_comparable(true, 15.1, 10.0, 10.0));
        assert!(!workload_comparable(false, 0.0, 0.0, 0.0));
    }

    #[test]
    fn median_uses_latest_five_values() {
        assert_eq!(
            median_latest_five(&[10.0, 9.0, 8.0, 7.0, 6.0, 100.0]),
            Some(8.0)
        );
        assert_eq!(median_latest_five(&[7.0, 12.0, 15.0, 98.0]), Some(13.5));
    }
}
