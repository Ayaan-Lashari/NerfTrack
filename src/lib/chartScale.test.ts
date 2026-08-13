import { describe, expect, it } from 'vitest';
import type { HistoryPoint } from '../domain';
import { generateYAxisTicks, getChartYAxisScale, Y_AXIS_TICK_UNIT_USD } from './chartScale';

function point(overrides: Partial<HistoryPoint> = {}): HistoryPoint {
  return {
    timestamp: 1_000,
    estimatedWeeklyValueUsd: 100,
    rawEstimatedWeeklyValueUsd: 999,
    observedCostUsd: 1,
    weeklyUsedPercent: 20,
    resetAt: null,
    resetReason: null,
    isFinalized: true,
    isHeartbeat: false,
    epoch: 1,
    confidence: 'high',
    percentageCoverage: 20,
    ...overrides,
  };
}

describe('adaptive chart Y-axis scale', () => {
  it('uses a $30 step, $180 upper bound, and exact ticks for a $160 peak', () => {
    const scale = getChartYAxisScale([point({ estimatedWeeklyValueUsd: 160 })]);

    expect(scale).toMatchObject({ lowerBound: 0, step: 30, upperBound: 180, maxEstimate: 160 });
    expect(scale.ticks).toEqual([0, 30, 60, 90, 120, 150, 180]);
  });

  it('uses a $20 step and no extra headroom for a $100 peak', () => {
    const scale = getChartYAxisScale([point({ estimatedWeeklyValueUsd: 100 })]);

    expect(scale).toMatchObject({ lowerBound: 0, step: 20, upperBound: 100 });
    expect(scale.ticks).toEqual([0, 20, 40, 60, 80, 100]);
  });

  it('uses the smaller tied step for a $200 peak', () => {
    const scale = getChartYAxisScale([point({ estimatedWeeklyValueUsd: 200 })]);

    expect(scale).toMatchObject({ lowerBound: 0, step: 30, upperBound: 210 });
    expect(scale.ticks).toEqual([0, 30, 60, 90, 120, 150, 180, 210]);
  });

  it('uses a $100 step for a $700 peak', () => {
    const scale = getChartYAxisScale([point({ estimatedWeeklyValueUsd: 700 })]);

    expect(scale).toMatchObject({ lowerBound: 0, step: 100, upperBound: 700 });
    expect(scale.ticks).toEqual([0, 100, 200, 300, 400, 500, 600, 700]);
  });

  it('rounds a $148 peak to a $150 upper bound', () => {
    const scale = getChartYAxisScale([point({ estimatedWeeklyValueUsd: 148 })]);

    expect(scale).toMatchObject({ lowerBound: 0, step: 30, upperBound: 150 });
    expect(scale.ticks.at(-1)).toBe(150);
  });

  it('keeps every step and tick aligned to whole-dollar tens', () => {
    const scale = getChartYAxisScale([point({ estimatedWeeklyValueUsd: 161 })]);
    const ticks = generateYAxisTicks(scale.upperBound, scale.step);

    expect(scale.step % Y_AXIS_TICK_UNIT_USD).toBe(0);
    expect(ticks.every((tick) => tick % scale.step === 0)).toBe(true);
    expect(ticks).toEqual(scale.ticks);
  });

  it('keeps normal populated charts in the five-to-seven interval target', () => {
    for (const maxEstimate of [100, 161, 200, 700]) {
      const scale = getChartYAxisScale([point({ estimatedWeeklyValueUsd: maxEstimate })]);
      expect(scale.ticks.length - 1).toBeGreaterThanOrEqual(5);
      expect(scale.ticks.length - 1).toBeLessThanOrEqual(7);
    }
  });

  it('keeps the lower bound at zero when there is no valid positive estimate', () => {
    const scale = getChartYAxisScale([
      point({ estimatedWeeklyValueUsd: null }),
      point({ estimatedWeeklyValueUsd: 999, isHeartbeat: true }),
    ]);

    expect(scale.lowerBound).toBe(0);
    expect(scale.step).toBe(Y_AXIS_TICK_UNIT_USD);
    expect(scale.maxEstimate).toBeNull();
  });

  it('ignores unsafe, immature, interpolated, and heartbeat estimates', () => {
    const scale = getChartYAxisScale([
      point({ estimatedWeeklyValueUsd: 148 }),
      point({ estimatedWeeklyValueUsd: 900, isHeartbeat: true }),
      point({ estimatedWeeklyValueUsd: 800, isSynthetic: true, comparisonEligible: true }),
      point({ estimatedWeeklyValueUsd: 700, comparisonEligible: false }),
      point({ estimatedWeeklyValueUsd: 600, confidence: 'low', percentageCoverage: 4 }),
    ]);

    expect(scale.maxEstimate).toBe(148);
    expect(scale.upperBound).toBe(150);
  });
});
