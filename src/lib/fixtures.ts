import type {
  AdvancedSettings,
  Annotation,
  AppSettings,
  AppStatus,
  CurrentQuote,
  DiagnosticsSummary,
  HistoryResponse,
  Range,
} from '../domain';

const minute = 60_000;
const hour = 60 * minute;
const day = 24 * hour;

export const demoNow = Date.UTC(2026, 4, 12, 15, 0, 0);

const rangeDuration: Record<Range, number> = {
  '1D': day,
  '1W': 7 * day,
  '1M': 30 * day,
  '3M': 90 * day,
  '6M': 180 * day,
};

const pathForRange: Record<Range, string> = {
  '1D': '5m',
  '1W': '30m',
  '1M': '2h',
  '3M': '4h',
  '6M': '4h',
};

export const demoStatus: AppStatus = {
  state: 'connected',
  label: 'Connected',
  detail: 'Local Mode',
  integrationMode: 'cli',
  accountState: 'authenticated',
  connectionQuality: 'good',
  plan: 'ChatGPT Plus',
  resetAt: Date.UTC(2026, 4, 15),
  lastUpdatedAt: demoNow,
  codexHome: {
    state: 'auto_detected',
    redactedLocation: '~/Library/Application Support/Codex',
    message: 'Auto-detected',
  },
  codexExecutable: {
    state: 'auto_detected',
    redactedLocation: '/usr/local/bin/codex',
    message: 'Auto-detected',
  },
  appServer: {
    state: 'auto_detected',
    redactedLocation: 'Local stdio App Server',
    message: 'Connected',
  },
  dataQuality: 'complete',
};

export const demoQuote: CurrentQuote = {
  valueUsd: 371.28,
  changeUsd: -18.42,
  changePercent: -4.73,
  observedCostUsd: 77.49,
  weeklyUsedPercent: 34,
  resetAt: demoStatus.resetAt,
  status: 'valid',
  dominantModel: 'gpt-5-codex',
  algorithmVersion: 'nerfify-estimator-v1',
  confidence: 'high',
  note: 'Values are estimates based on observed local usage and may differ from actual API pricing.',
};

export const demoAnnotations: Annotation[] = [
  { id: 'weekly-reset', timestamp: Date.UTC(2026, 4, 7, 0), label: 'Weekly reset', kind: 'reset' },
];

export const demoDiagnostics: DiagnosticsSummary = {
  totalEvents: 846,
  pricedEvents: 812,
  pendingEvents: 22,
  rejectedEvents: 12,
  unattributedEvents: 0,
  partialLineRetries: 4,
  monitoringGaps: 0,
  hiddenResets: 0,
  reasons: [
    { reason: 'Missing pricing snapshot', count: 22 },
    { reason: 'Unknown provider evidence', count: 8 },
    { reason: 'Unsupported record shape', count: 4 },
  ],
  modelIds: ['gpt-5-codex', 'gpt-5-codex-mini'],
  privacy: 'Prompts, account identifiers, and full local paths are never stored or returned.',
};

export const defaultAdvancedSettings: AdvancedSettings = {
  refreshIntervalSeconds: 10,
  reconciliationIntervalHours: 1,
  monitoringGapMinutes: 5,
  settlementWindowSeconds: 60,
  minimumQuotaMovementPoints: 3,
  minimumEligibleCostUsd: 0.25,
  minimumEvents: 2,
  lowUsageQuarantinePercent: 3,
  reducedMotion: false,
};

export const demoSettings: AppSettings = {
  ...defaultAdvancedSettings,
  appearance: 'dark',
  currency: 'USD',
  localOnly: true,
  telemetry: false,
  autoUpdater: false,
};

export function getDemoHistory(range: Range): HistoryResponse {
  const total =
    range === '1D' ? 96 : range === '1W' ? 168 : range === '1M' ? 180 : range === '3M' ? 270 : 360;
  const duration = rangeDuration[range];
  const step = duration / Math.max(total - 1, 1);
  const points = Array.from({ length: total }, (_, index) => {
    const progress = index / Math.max(total - 1, 1);
    const wave = Math.sin(index * 0.36) * 1.8 + Math.sin(index * 0.11) * 2.7;
    const noise = ((index * 17) % 11) * 0.08;
    const trend = progress * -29;
    const resetBump = range === '1W' && index > total * 0.45 && index < total * 0.48 ? 3.2 : 0;
    const value = 401 + trend + wave + noise + resetBump;
    return {
      timestamp: demoNow - duration + index * step,
      valueUsd: Number(value.toFixed(2)),
      rawValueUsd: Number((value - 0.7).toFixed(2)),
      weeklyUsedPercent: Math.max(4, Number((11 + progress * 23).toFixed(1))),
      isFinalized: index < total - 2,
      isHeartbeat: index % 10 === 0,
      dominantModel: index % 6 === 0 ? 'gpt-5-codex-mini' : 'gpt-5-codex',
    };
  });
  const finalValue = demoQuote.valueUsd ?? 0;
  points[points.length - 1].valueUsd = finalValue;
  points[points.length - 1].rawValueUsd = finalValue;
  return {
    points,
    statistics: {
      range,
      baselineValueUsd: points[0].valueUsd,
      currentValueUsd: demoQuote.valueUsd,
      deltaUsd: demoQuote.changeUsd,
      deltaPercent: demoQuote.changePercent,
      pointCount: points.length,
      partial: range !== '1D',
    },
    bucket: pathForRange[range] as HistoryResponse['bucket'],
  };
}
