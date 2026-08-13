import type { HistoryPoint } from '../domain';

export type ComparisonUnavailableReason = 'missing-anchor' | 'calibration' | 'mature-history';

export interface HistoryComparison {
  eligible: boolean;
  reason: 'valid' | ComparisonUnavailableReason;
  selectedValueUsd: number | null;
  anchorValueUsd: number | null;
  deltaValueUsd: number | null;
  deltaPercent: number | null;
}

const MINIMUM_COMPARISON_COVERAGE = 10;

/**
 * The value signal used for the chart and manual comparisons.
 *
 * `estimatedWeeklyValueUsd` is the stabilized cumulative estimate. The raw
 * interval estimate remains available for audit/history views, but must not
 * become a scrubbed range endpoint when the stabilized signal is absent.
 */
export function getChartEstimate(point: HistoryPoint | null | undefined) {
  const value = point?.estimatedWeeklyValueUsd ?? null;
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function hasMatureEndpointEvidence(point: HistoryPoint | null | undefined) {
  const estimate = getChartEstimate(point);
  return Boolean(
    point &&
    estimate !== null &&
    estimate > 0 &&
    Number.isFinite(point.timestamp) &&
    point.isFinalized &&
    !point.isHeartbeat &&
    (point.confidence === 'high' || point.confidence === 'medium') &&
    point.percentageCoverage !== null &&
    point.percentageCoverage !== undefined &&
    Number.isFinite(point.percentageCoverage) &&
    point.percentageCoverage >= MINIMUM_COMPARISON_COVERAGE &&
    point.epoch !== null &&
    point.epoch !== undefined &&
    point.comparisonEligible !== false,
  );
}

export function isComparisonEligiblePoint(point: HistoryPoint | null | undefined) {
  return hasMatureEndpointEvidence(point);
}

function unavailable(
  reason: ComparisonUnavailableReason,
  selectedValueUsd: number | null,
  anchorValueUsd: number | null,
): HistoryComparison {
  return {
    eligible: false,
    reason,
    selectedValueUsd,
    anchorValueUsd,
    deltaValueUsd: null,
    deltaPercent: null,
  };
}

/**
 * Compare an older anchor with a later selected point only when both points
 * are mature evidence from distinct weekly windows. This is intentionally
 * pure so the chart and headline cannot drift into different eligibility
 * rules.
 */
export function compareHistoryPoints(
  selected: HistoryPoint | null | undefined,
  anchor: HistoryPoint | null | undefined,
): HistoryComparison {
  const selectedValueUsd = getChartEstimate(selected);
  const anchorValueUsd = getChartEstimate(anchor);

  if (!selected || !anchor) {
    return unavailable('missing-anchor', selectedValueUsd, anchorValueUsd);
  }

  if (
    !Number.isFinite(selected.timestamp) ||
    !Number.isFinite(anchor.timestamp) ||
    anchor.timestamp >= selected.timestamp
  ) {
    return unavailable('mature-history', selectedValueUsd, anchorValueUsd);
  }

  // An interpolation that crossed a reset or used unsafe brackets must never
  // be rescued by the nearest point's epoch metadata.
  if (
    (selected.isSynthetic && selected.comparisonEligible !== true) ||
    (anchor.isSynthetic && anchor.comparisonEligible !== true)
  ) {
    return unavailable('mature-history', selectedValueUsd, anchorValueUsd);
  }

  if (
    selected.epoch !== null &&
    selected.epoch !== undefined &&
    anchor.epoch !== null &&
    anchor.epoch !== undefined &&
    selected.epoch === anchor.epoch
  ) {
    return unavailable('calibration', selectedValueUsd, anchorValueUsd);
  }

  if (!hasMatureEndpointEvidence(selected) || !hasMatureEndpointEvidence(anchor)) {
    return unavailable('mature-history', selectedValueUsd, anchorValueUsd);
  }

  // The endpoint checks above guarantee finite, positive values.
  const deltaValueUsd = (selectedValueUsd as number) - (anchorValueUsd as number);
  return {
    eligible: true,
    reason: 'valid',
    selectedValueUsd,
    anchorValueUsd,
    deltaValueUsd,
    deltaPercent: (deltaValueUsd / (anchorValueUsd as number)) * 100,
  };
}

export function comparisonUnavailableMessage(reason: ComparisonUnavailableReason) {
  return reason === 'calibration'
    ? 'Comparison unavailable · estimator calibration'
    : 'Comparison unavailable · endpoints need mature history';
}
