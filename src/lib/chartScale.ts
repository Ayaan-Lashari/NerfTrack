import type { HistoryPoint } from '../domain';
import { getChartEstimate, isComparisonEligiblePoint } from './comparison';

export const Y_AXIS_TICK_UNIT_USD = 10;
const TARGET_INTERVAL_COUNT = 6;
const MIN_INTERVAL_COUNT = 5;
const MAX_INTERVAL_COUNT = 7;
const EMPTY_CHART_UPPER_BOUND_USD = 50;

export interface ChartYAxisScale {
  lowerBound: 0;
  step: number;
  upperBound: number;
  ticks: number[];
  maxEstimate: number | null;
}

/**
 * Return exact multiples of the selected tick step.
 *
 * The step and upper bound are integer USD amounts, so this produces one
 * shared sequence for both grid geometry and labels without independently
 * rounding either one.
 */
export function generateYAxisTicks(upperBound: number, step: number) {
  const safeStep = Math.max(
    Y_AXIS_TICK_UNIT_USD,
    Math.round(step / Y_AXIS_TICK_UNIT_USD) * Y_AXIS_TICK_UNIT_USD,
  );
  const safeUpperBound = Math.max(safeStep, Math.ceil(upperBound / safeStep) * safeStep);
  const tickCount = Math.round(safeUpperBound / safeStep);
  return Array.from({ length: tickCount + 1 }, (_, index) => index * safeStep);
}

function candidateYAxisSteps(maxEstimate: number) {
  const largestRelevantPower = Math.max(1, Math.floor(Math.log10(maxEstimate)) + 1);
  const steps = new Set<number>();
  for (let power = 1; power <= largestRelevantPower + 1; power += 1) {
    const decade = 10 ** power;
    for (let multiplier = 1; multiplier <= 10; multiplier += 1) {
      steps.add(decade * multiplier);
    }
  }
  return [...steps].sort((left, right) => left - right);
}

function intervalCount(maxEstimate: number, step: number) {
  return Math.max(1, Math.ceil(maxEstimate / step));
}

/**
 * Choose a readable adaptive step. Candidate steps use 1–10 times powers of
 * ten, so every result is a whole-number multiple of $10 without producing
 * awkward values such as $120. Scales in the requested 5–7 interval band are
 * preferred; within that band, six intervals wins and a smaller step breaks a
 * tie. If a discrete $10 step cannot land in the band, the closest fallback
 * uses the same deterministic ordering.
 */
export function chooseYAxisStep(maxEstimate: number | null) {
  if (maxEstimate === null || !Number.isFinite(maxEstimate) || maxEstimate <= 0) {
    return Y_AXIS_TICK_UNIT_USD;
  }

  const candidates = candidateYAxisSteps(maxEstimate);
  const valid = candidates.filter((step) => {
    const intervals = intervalCount(maxEstimate, step);
    return intervals >= MIN_INTERVAL_COUNT && intervals <= MAX_INTERVAL_COUNT;
  });
  const pool = valid.length ? valid : candidates;

  return pool.sort((left, right) => {
    const leftIntervals = intervalCount(maxEstimate, left);
    const rightIntervals = intervalCount(maxEstimate, right);
    const leftDistance = Math.abs(leftIntervals - TARGET_INTERVAL_COUNT);
    const rightDistance = Math.abs(rightIntervals - TARGET_INTERVAL_COUNT);
    return leftDistance - rightDistance || left - right;
  })[0];
}

/**
 * Only mature, stored stabilized estimates can establish the visible axis.
 * Interpolated points, heartbeat carry-forwards, and unsafe comparison points
 * remain available for display but cannot inflate the scale.
 */
function axisEstimate(point: HistoryPoint) {
  const estimate = getChartEstimate(point);
  if (
    estimate === null ||
    estimate <= 0 ||
    point.isSynthetic === true ||
    point.isHeartbeat ||
    !isComparisonEligiblePoint(point)
  ) {
    return null;
  }
  return estimate;
}

export function getChartYAxisScale(points: readonly HistoryPoint[]): ChartYAxisScale {
  const maxEstimate = points.reduce<number | null>((maximum, point) => {
    const estimate = axisEstimate(point);
    if (estimate === null) return maximum;
    return maximum === null ? estimate : Math.max(maximum, estimate);
  }, null);

  if (maxEstimate === null) {
    const step = Y_AXIS_TICK_UNIT_USD;
    return {
      lowerBound: 0,
      step,
      upperBound: EMPTY_CHART_UPPER_BOUND_USD,
      ticks: generateYAxisTicks(EMPTY_CHART_UPPER_BOUND_USD, step),
      maxEstimate: null,
    };
  }

  const step = chooseYAxisStep(maxEstimate);
  const upperBound = Math.ceil(maxEstimate / step) * step;
  return {
    lowerBound: 0,
    step,
    upperBound,
    ticks: generateYAxisTicks(upperBound, step),
    maxEstimate,
  };
}

export function yAxisValueToY(
  value: number,
  scale: ChartYAxisScale,
  plotTop: number,
  plotBottom: number,
) {
  const clampedValue = Math.max(scale.lowerBound, Math.min(scale.upperBound, value));
  const valueRange = Math.max(scale.upperBound - scale.lowerBound, 1);
  return plotBottom - ((clampedValue - scale.lowerBound) / valueRange) * (plotBottom - plotTop);
}

export function formatYAxisTick(value: number) {
  return `$${value}`;
}
