import { useCallback, useEffect, useMemo, useState } from 'react';
import type {
  AdvancedSettings,
  AppSettings,
  AppStatus,
  CurrentQuote,
  HistoryPoint,
  NavKey,
  Range,
} from './domain';
import { DiagnosticsView } from './components/DiagnosticsView';
import { Icon } from './components/Icons';
import { HistoryView } from './components/HistoryView';
import { MetricCard, UsageRing } from './components/MetricCard';
import { SetupView } from './components/SetupView';
import { SettingsView } from './components/SettingsView';
import { SideNav } from './components/SideNav';
import { UsageChart } from './components/UsageChart';
import {
  getAnnotations,
  getCurrentQuote,
  getCurrentStatus,
  getDiagnosticsSummary,
  getHistory,
  getSettings,
  retryDetection,
  resetAnnotations,
  selectCodexExecutable,
  selectCodexHome,
  updateSettings,
} from './lib/commands';
import { demoQuote, demoStatus } from './lib/fixtures';
import type { Annotation, DiagnosticsSummary, HistoryResponse } from './domain';

const ranges: Range[] = ['1D', '1W', '1M', '3M', '6M'];

function formatUsd(value: number | null) {
  return value === null ? 'Not available' : `$${value.toFixed(2)}`;
}

function formatSignedUsd(value: number | null) {
  if (value === null) return '—';
  return `${value < 0 ? '−' : '+'}$${Math.abs(value).toFixed(2)}`;
}

function formatPercent(value: number | null) {
  return value === null ? '—' : `${value < 0 ? '−' : '+'}${Math.abs(value).toFixed(2)}%`;
}

function formatReset(status: AppStatus) {
  if (!status.resetAt) return 'Pending';
  const fallback = status.resetAt === demoStatus.resetAt;
  if (fallback) return '2d 7h';
  const remaining = status.resetAt - Date.now();
  if (remaining <= 0) return 'Reset observed';
  const hours = Math.floor(remaining / 3_600_000);
  return `${Math.floor(hours / 24)}d ${hours % 24}h`;
}

function formatResetDate(timestamp: number | null) {
  if (!timestamp) return 'Awaiting quota window';
  return new Date(timestamp).toLocaleString('en-US', {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

function HeaderIcon() {
  return (
    <div className="hero-icon">
      <Icon name="terminal" size={33} strokeWidth={1.6} />
    </div>
  );
}

function RangeSelector({ range, onChange }: { range: Range; onChange: (range: Range) => void }) {
  return (
    <div className="range-control" role="tablist" aria-label="History range">
      {ranges.map((item) => (
        <button
          key={item}
          className={range === item ? 'selected' : ''}
          onClick={() => onChange(item)}
          role="tab"
          aria-selected={range === item}
        >
          {item}
        </button>
      ))}
    </div>
  );
}

function HomeView({
  status,
  quote,
  history,
  annotations,
  range,
  reducedMotion,
  onRangeChange,
  onResetAnnotations,
}: {
  status: AppStatus;
  quote: CurrentQuote | null;
  history: HistoryResponse;
  annotations: Annotation[];
  range: Range;
  reducedMotion: boolean;
  onRangeChange: (range: Range) => void;
  onResetAnnotations: () => void;
}) {
  const [scrubbed, setScrubbed] = useState<HistoryPoint | null>(null);
  const displayValue = scrubbed?.valueUsd ?? quote?.valueUsd ?? null;
  const displayChange = scrubbed ? null : (quote?.changeUsd ?? null);
  const displayPercent = scrubbed ? null : (quote?.changePercent ?? null);
  const isEmpty = !quote || quote.status === 'empty';

  return (
    <section className="home-page page-shell">
      <header className="hero-heading">
        <div className="hero-title-wrap">
          <HeaderIcon />
          <div>
            <h1>Codex Weekly API Equivalent</h1>
            <p>Estimated value of a full weekly Codex allowance</p>
          </div>
        </div>
        <div className="hero-controls">
          <RangeSelector range={range} onChange={onRangeChange} />
          <button
            className="more-button"
            aria-label="More chart options"
            onClick={onResetAnnotations}
          >
            <Icon name="more" size={23} />
          </button>
        </div>
      </header>
      <div className="quote-heading">
        <strong className={isEmpty ? 'empty-value' : ''}>{formatUsd(displayValue)}</strong>
        {!isEmpty && (
          <p className={displayChange !== null && displayChange < 0 ? 'negative' : 'positive'}>
            {formatSignedUsd(displayChange)}{' '}
            {displayPercent !== null ? `(${formatPercent(displayPercent)})` : ''}{' '}
            <span>Past Week</span>
          </p>
        )}
        {isEmpty && (
          <p className="muted-state">
            A quote appears after a complete, settled cost and quota interval.
          </p>
        )}
      </div>
      <div className="chart-panel">
        <UsageChart
          points={history.points}
          annotations={annotations}
          range={range}
          reducedMotion={reducedMotion}
          onScrub={setScrubbed}
        />
        <div className="chart-actions">
          <span>
            {history.statistics.partial
              ? 'Partial range · older observations are usage history'
              : 'Complete range'}
          </span>
          <button onClick={onResetAnnotations}>
            <Icon name="refresh" size={14} />
            Reset annotations
          </button>
        </div>
      </div>
      <div className="metric-grid">
        <MetricCard
          icon="chart"
          iconTone="green"
          label="Weekly Used"
          value={
            quote?.weeklyUsedPercent === null || quote?.weeklyUsedPercent === undefined
              ? '—'
              : `${Math.round(quote.weeklyUsedPercent)}%`
          }
          detail="of allowance"
        >
          <UsageRing value={quote?.weeklyUsedPercent ?? null} />
        </MetricCard>
        <MetricCard
          icon="clock"
          iconTone="blue"
          label="Resets In"
          value={formatReset(status)}
          detail={formatResetDate(status.resetAt)}
        />
        <MetricCard
          icon="heart"
          iconTone="purple"
          label="Observed Local Cost"
          value={formatUsd(quote?.observedCostUsd ?? null)}
          detail="This Week"
        />
        <MetricCard
          icon="shield"
          iconTone="lime"
          label="Status"
          value={
            quote?.status === 'valid'
              ? 'Valid'
              : quote?.status === 'pending'
                ? 'Settling'
                : 'Unavailable'
          }
          detail={status.detail}
        />
      </div>
      <footer className="app-footer">
        <span>
          <Icon name="info" size={16} />
          Values are estimates based on observed local usage and may differ from actual API pricing.
        </span>
        <span className="refresh-status">
          <i />
          Auto-refresh: 10s
        </span>
      </footer>
    </section>
  );
}

export default function App() {
  const [active, setActive] = useState<NavKey>('home');
  const [range, setRange] = useState<Range>('1W');
  const [status, setStatus] = useState<AppStatus>(demoStatus);
  const [quote, setQuote] = useState<CurrentQuote | null>(demoQuote);
  const [history, setHistory] = useState<HistoryResponse | null>(null);
  const [annotations, setAnnotations] = useState<Annotation[]>([]);
  const [diagnostics, setDiagnostics] = useState<DiagnosticsSummary | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [nextQuote, nextStatus, nextHistory, nextAnnotations, nextDiagnostics, nextSettings] =
        await Promise.all([
          getCurrentQuote(),
          getCurrentStatus(),
          getHistory(range),
          getAnnotations(),
          getDiagnosticsSummary(),
          getSettings(),
        ]);
      setQuote(nextQuote);
      setStatus(nextStatus);
      setHistory(nextHistory);
      setAnnotations(nextAnnotations);
      setDiagnostics(nextDiagnostics);
      setSettings(nextSettings);
      setLoadError(false);
    } catch {
      setQuote(null);
      setHistory(null);
      setAnnotations([]);
      setDiagnostics(null);
      setStatus((current) => ({
        ...current,
        state: 'error',
        label: 'Unavailable',
        detail: 'Local state error',
        connectionQuality: 'offline',
        dataQuality: 'interrupted',
      }));
      setLoadError(true);
    } finally {
      setIsLoading(false);
    }
  }, [range]);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 10_000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const handleRangeChange = (nextRange: Range) => {
    setRange(nextRange);
  };

  const handleSettingChange = async (key: keyof AppSettings, value: number | boolean) => {
    if (!settings) return;
    const nextSettings = { ...settings, [key]: value };
    setSettings(nextSettings);
    if (key in nextSettings) {
      const advanced: AdvancedSettings = {
        refreshIntervalSeconds: nextSettings.refreshIntervalSeconds,
        reconciliationIntervalHours: nextSettings.reconciliationIntervalHours,
        monitoringGapMinutes: nextSettings.monitoringGapMinutes,
        settlementWindowSeconds: nextSettings.settlementWindowSeconds,
        minimumQuotaMovementPoints: nextSettings.minimumQuotaMovementPoints,
        minimumEligibleCostUsd: nextSettings.minimumEligibleCostUsd,
        minimumEvents: nextSettings.minimumEvents,
        lowUsageQuarantinePercent: nextSettings.lowUsageQuarantinePercent,
        reducedMotion: nextSettings.reducedMotion,
      };
      try {
        await updateSettings(advanced);
      } catch {
        setSettings(settings);
        setLoadError(true);
      }
    }
  };

  const runDetection = async () => {
    setStatus((current) => ({
      ...current,
      state: 'detecting',
      label: 'Detecting',
      detail: 'Local Mode',
    }));
    try {
      const next = await retryDetection();
      setStatus(next);
      await refresh();
    } catch {
      setLoadError(true);
    }
  };

  const handleChooseHome = async () => {
    try {
      const selection = await selectCodexHome();
      if (selection.selected) {
        setStatus((current) => ({ ...current, codexHome: selection.status }));
        setStatus(await retryDetection());
        await refresh();
      }
    } catch {
      setLoadError(true);
    }
  };

  const handleChooseExecutable = async () => {
    try {
      const selection = await selectCodexExecutable();
      if (selection.selected) {
        setStatus((current) => ({ ...current, codexExecutable: selection.status }));
        setStatus(await retryDetection());
      }
    } catch {
      setLoadError(true);
    }
  };

  const displayHistory = useMemo(
    () =>
      history ?? {
        points: [],
        statistics: {
          range,
          baselineValueUsd: null,
          currentValueUsd: null,
          deltaUsd: null,
          deltaPercent: null,
          pointCount: 0,
          partial: true,
        },
        bucket: 'raw' as const,
      },
    [history, range],
  );
  const displaySettings = settings ?? {
    refreshIntervalSeconds: 10,
    reconciliationIntervalHours: 1,
    monitoringGapMinutes: 5,
    settlementWindowSeconds: 60,
    minimumQuotaMovementPoints: 3,
    minimumEligibleCostUsd: 0.25,
    minimumEvents: 2,
    lowUsageQuarantinePercent: 3,
    reducedMotion: false,
    appearance: 'dark' as const,
    currency: 'USD' as const,
    localOnly: true as const,
    telemetry: false as const,
    autoUpdater: false as const,
  };

  const renderPage = () => {
    if (isLoading && !history)
      return (
        <div className="loading-state">
          <span className="loading-spinner" />
          Loading local state…
        </div>
      );
    switch (active) {
      case 'setup':
        return (
          <SetupView
            status={status}
            settings={displaySettings}
            onChooseHome={handleChooseHome}
            onChooseExecutable={handleChooseExecutable}
            onRetry={runDetection}
            onStart={runDetection}
            onSettingChange={handleSettingChange}
          />
        );
      case 'diagnostics':
        return (
          <DiagnosticsView
            diagnostics={
              diagnostics ?? {
                totalEvents: 0,
                pricedEvents: 0,
                pendingEvents: 0,
                rejectedEvents: 0,
                unattributedEvents: 0,
                partialLineRetries: 0,
                monitoringGaps: 0,
                hiddenResets: 0,
                reasons: [],
                modelIds: [],
                privacy: 'Waiting for local data.',
              }
            }
          />
        );
      case 'history':
        return (
          <HistoryView history={displayHistory} range={range} onRangeChange={handleRangeChange} />
        );
      case 'settings':
        return <SettingsView settings={displaySettings} onChange={handleSettingChange} />;
      default:
        return (
          <HomeView
            status={status}
            quote={quote}
            history={displayHistory}
            annotations={annotations}
            range={range}
            reducedMotion={displaySettings.reducedMotion}
            onRangeChange={handleRangeChange}
            onResetAnnotations={async () => {
              try {
                await resetAnnotations();
                setAnnotations([]);
              } catch {
                setLoadError(true);
              }
            }}
          />
        );
    }
  };

  return (
    <div className="app-window">
      <SideNav active={active} status={status} onNavigate={setActive} />
      <main className="app-content">
        {loadError && (
          <div className="global-error" role="alert">
            Local state is unavailable. Check the Diagnostics and Setup pages, then retry detection.
          </div>
        )}
        {renderPage()}
      </main>
    </div>
  );
}
