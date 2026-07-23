export type Range = '1D' | '1W' | '1M' | '3M' | '6M';

export type AppStatusState =
  | 'connected'
  | 'detecting'
  | 'settling'
  | 'recalibrating'
  | 'unsupported'
  | 'needs_setup'
  | 'error';

export type NavKey = 'home' | 'setup' | 'diagnostics' | 'history' | 'settings';

export interface DiscoveryStatus {
  state: 'auto_detected' | 'selected' | 'missing' | 'unsupported' | 'redacted' | 'not_required';
  redactedLocation: string | null;
  message: string;
}

export interface AppStatus {
  state: AppStatusState;
  label: string;
  detail: string;
  integrationMode: 'cli' | 'gui' | 'unknown';
  accountState: 'authenticated' | 'unsupported' | 'unknown';
  connectionQuality: 'good' | 'degraded' | 'offline' | 'unknown';
  plan: string | null;
  resetAt: number | null;
  lastUpdatedAt: number | null;
  codexHome: DiscoveryStatus;
  codexExecutable: DiscoveryStatus;
  appServer: DiscoveryStatus;
  dataQuality: 'complete' | 'partial' | 'interrupted' | 'unknown';
}

export interface CurrentQuote {
  valueUsd: number | null;
  changeUsd: number | null;
  changePercent: number | null;
  observedCostUsd: number | null;
  weeklyUsedPercent: number | null;
  resetAt: number | null;
  status: 'valid' | 'pending' | 'empty' | 'unsupported' | 'error';
  dominantModel: string | null;
  algorithmVersion: string;
  confidence: 'high' | 'medium' | 'low' | 'none';
  note: string | null;
}

export interface HistoryPoint {
  timestamp: number;
  valueUsd: number | null;
  rawValueUsd: number | null;
  weeklyUsedPercent: number | null;
  isFinalized: boolean;
  isHeartbeat: boolean;
  dominantModel: string | null;
}

export interface RangeStatistics {
  range: Range;
  baselineValueUsd: number | null;
  currentValueUsd: number | null;
  deltaUsd: number | null;
  deltaPercent: number | null;
  pointCount: number;
  partial: boolean;
}

export interface HistoryResponse {
  points: HistoryPoint[];
  statistics: RangeStatistics;
  bucket: 'raw' | '5m' | '30m' | '2h' | '4h';
}

export interface Annotation {
  id: string;
  timestamp: number;
  label: string;
  kind: 'reset' | 'diagnostic' | 'note';
}

export interface DiagnosticsSummary {
  totalEvents: number;
  pricedEvents: number;
  pendingEvents: number;
  rejectedEvents: number;
  unattributedEvents: number;
  partialLineRetries: number;
  monitoringGaps: number;
  hiddenResets: number;
  reasons: Array<{ reason: string; count: number }>;
  modelIds: string[];
  privacy: string;
}

export interface AdvancedSettings {
  refreshIntervalSeconds: number;
  reconciliationIntervalHours: number;
  monitoringGapMinutes: number;
  settlementWindowSeconds: number;
  minimumQuotaMovementPoints: number;
  minimumEligibleCostUsd: number;
  minimumEvents: number;
  lowUsageQuarantinePercent: number;
  reducedMotion: boolean;
}

export interface AppSettings extends AdvancedSettings {
  appearance: 'dark';
  currency: 'USD';
  localOnly: true;
  telemetry: false;
  autoUpdater: false;
}

export interface RedactedSelection {
  selected: boolean;
  status: DiscoveryStatus;
}
