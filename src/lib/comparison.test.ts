import { describe, expect, it } from 'vitest';
import type { HistoryPoint } from '../domain';
import { compareHistoryPoints, getChartEstimate } from './comparison';

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

describe('history comparison eligibility', () => {
  it('treats same-epoch movement as estimator calibration', () => {
    const result = compareHistoryPoints(
      point({
        timestamp: 2_000,
        estimatedWeeklyValueUsd: 158.04,
        percentageCoverage: 53,
      }),
      point({
        estimatedWeeklyValueUsd: 94.35,
        percentageCoverage: 9,
      }),
    );

    expect(result).toMatchObject({
      eligible: false,
      reason: 'calibration',
      deltaValueUsd: null,
      deltaPercent: null,
    });
  });

  it('rejects a low-coverage baseline even when the selected endpoint is high confidence', () => {
    const result = compareHistoryPoints(
      point({
        timestamp: 2_000,
        epoch: 2,
        estimatedWeeklyValueUsd: 160.84,
        percentageCoverage: 51,
      }),
      point({
        estimatedWeeklyValueUsd: 72.62,
        epoch: 1,
        confidence: 'medium',
        percentageCoverage: 8,
      }),
    );

    expect(result).toMatchObject({
      eligible: false,
      reason: 'mature-history',
      deltaValueUsd: null,
      deltaPercent: null,
    });
  });

  it('preserves the signed comparison for two mature cross-window endpoints', () => {
    const result = compareHistoryPoints(
      point({ timestamp: 2_000, epoch: 2, estimatedWeeklyValueUsd: 150 }),
      point({ timestamp: 1_000, epoch: 1, estimatedWeeklyValueUsd: 100 }),
    );

    expect(result).toMatchObject({
      eligible: true,
      reason: 'valid',
      selectedValueUsd: 150,
      anchorValueUsd: 100,
      deltaValueUsd: 50,
      deltaPercent: 50,
    });
  });

  it('does not use a backend baseline when the anchor is null or unusable', () => {
    const selected = point({ timestamp: 2_000, epoch: 2, estimatedWeeklyValueUsd: 160.84 });
    const nullAnchor = compareHistoryPoints(selected, null);
    const invalidAnchor = compareHistoryPoints(
      selected,
      point({ epoch: 1, estimatedWeeklyValueUsd: null, percentageCoverage: 20 }),
    );

    expect(nullAnchor).toMatchObject({
      eligible: false,
      reason: 'missing-anchor',
      anchorValueUsd: null,
      deltaValueUsd: null,
      deltaPercent: null,
    });
    expect(invalidAnchor).toMatchObject({
      eligible: false,
      reason: 'mature-history',
      anchorValueUsd: null,
      deltaValueUsd: null,
      deltaPercent: null,
    });
    expect(
      getChartEstimate(point({ estimatedWeeklyValueUsd: 12, rawEstimatedWeeklyValueUsd: 999 })),
    ).toBe(12);
  });

  it('rejects heartbeat and non-finalized endpoints', () => {
    const selected = point({ timestamp: 2_000, epoch: 2 });
    expect(compareHistoryPoints(selected, point({ epoch: 1, isHeartbeat: true }))).toMatchObject({
      eligible: false,
      deltaPercent: null,
    });
    expect(compareHistoryPoints(selected, point({ epoch: 1, isFinalized: false }))).toMatchObject({
      eligible: false,
      deltaPercent: null,
    });
  });
});
