import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import App, { HomeView } from './App';
import type { HistoryPoint, HistoryResponse } from './domain';
import { demoQuote, demoStatus } from './lib/fixtures';

function historyPoint(overrides: Partial<HistoryPoint> = {}): HistoryPoint {
  return {
    timestamp: 0,
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

function customHistory(points: HistoryPoint[], baseline = 10): HistoryResponse {
  return {
    points,
    statistics: {
      range: '1D',
      baselineEstimatedWeeklyValueUsd: baseline,
      baselineTimestamp: points[0]?.timestamp ?? null,
      currentEstimatedWeeklyValueUsd: points.at(-1)?.estimatedWeeklyValueUsd ?? null,
      deltaValueUsd: null,
      deltaPercent: null,
      pointCount: points.length,
      partial: false,
    },
    bucket: '5m',
  };
}

function renderHomeWithHistory(history: HistoryResponse) {
  render(
    <HomeView
      status={demoStatus}
      quote={demoQuote}
      history={history}
      annotations={[]}
      range="1D"
      reducedMotion={false}
      isRefreshing={false}
      onRefresh={vi.fn()}
      onRangeChange={vi.fn()}
      onResetAnnotations={vi.fn()}
    />,
  );
  const chart = screen.getByRole('img', {
    name: /Estimated weekly API-equivalent value/,
  });
  vi.spyOn(chart, 'getBoundingClientRect').mockReturnValue({
    left: 0,
    top: 0,
    width: 1000,
    height: 308,
    right: 1000,
    bottom: 308,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  });
  return chart;
}

describe('NerfTrack app shell', () => {
  it('renders the dashboard reference surface with a non-zero quote', async () => {
    render(<App />);
    expect(await screen.findByText('Codex Weekly API-equivalent Estimator')).toBeInTheDocument();
    expect(screen.getAllByText('≈$371').length).toBeGreaterThan(0);
    expect(screen.getByText('Weekly Used')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Refresh data' })).toBeInTheDocument();
    expect(screen.getByText(/Live ·/)).toBeInTheDocument();
  });

  it('switches to setup and changes a monitoring control', async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Setup' }));
    expect(screen.getByText('Set up NerfTrack')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Retry detection' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Start monitoring' })).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Reset saved selections' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Need help/i })).not.toBeInTheDocument();
    const refreshSelect = screen.getByLabelText('Refresh interval');
    await user.selectOptions(refreshSelect, '20');
    expect(refreshSelect).toHaveValue('20');
  });

  it('shows an in-app confirmation before resetting local data', async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    await user.click(screen.getByRole('button', { name: 'Reset all data' }));

    expect(screen.getByRole('alertdialog')).toHaveTextContent('Reset all local data?');
    expect(screen.getByRole('button', { name: 'Confirm reset' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
  });

  it('keeps the GitHub update control and starter page accessible from settings', async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(await screen.findByRole('button', { name: 'Up to date' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Settings' }));
    await user.click(screen.getByRole('button', { name: 'Open starter page again' }));

    expect(screen.getByRole('heading', { name: 'Help NerfTrack keep going.' })).toBeInTheDocument();
    expect(screen.getByText('Let the resets continue')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Star NerfTrack on GitHub/ })).not.toBeDisabled();
    expect(screen.queryByRole('button', { name: /skip/i })).not.toBeInTheDocument();
    expect(screen.queryByText(/follow on X/i)).not.toBeInTheDocument();
  });

  it('offers fast checkpoint restore and a separate full log import', async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));

    expect(screen.getByRole('button', { name: 'Restore last checkpoint' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Import all data' })).toBeInTheDocument();
    expect(screen.getByText(/fastest recovery option/i)).toBeInTheDocument();
    expect(screen.getByText(/re-read every available Codex log/i)).toBeInTheDocument();
  });

  it('edits and validates a local custom pricing override', async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    await user.click(screen.getByRole('button', { name: 'Add override' }));
    expect(screen.getByLabelText('Model ID 1')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Save pricing' }));
    expect(screen.getByRole('alert')).toHaveTextContent('Each override needs a model ID.');
    await user.type(screen.getByLabelText('Model ID 1'), 'local-codex');
    await user.click(screen.getByRole('button', { name: 'Save pricing' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('autofills custom pricing drafts from detected unpriced models', async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));

    expect(screen.getByText('local-codex-preview')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Autofill detected model' }));

    expect(screen.getByLabelText('Model ID 1')).toHaveValue('local-codex-preview');
    expect(screen.getByText('All detected models are in this draft')).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Autofill detected model' }),
    ).not.toBeInTheDocument();
  });

  it('navigates to diagnostics without leaking sensitive fields', async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Diagnostics' }));
    expect(screen.getByRole('heading', { name: 'Diagnostics' })).toBeInTheDocument();
    expect(screen.getByText(/Prompts, account identifiers/)).toBeInTheDocument();
  });

  it('shows the dollar and percentage difference across a held drag', async () => {
    render(<App />);
    const chart = await screen.findByRole('img', {
      name: /Estimated weekly API-equivalent value/,
    });
    vi.spyOn(chart, 'getBoundingClientRect').mockReturnValue({
      left: 0,
      top: 0,
      width: 1000,
      height: 308,
      right: 1000,
      bottom: 308,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });

    fireEvent(chart, new MouseEvent('pointerdown', { bubbles: true, clientX: 400 }));
    fireEvent(chart, new MouseEvent('pointermove', { bubbles: true, clientX: 900 }));

    expect(screen.getByText('Selected range').parentElement).toHaveTextContent(
      /[+−]\$\d+\.\d{2} \([+−]\d+\.\d{2}%\)/,
    );
  });

  it('suppresses a same-window calibration percentage and uses neutral styling', () => {
    const chart = renderHomeWithHistory(
      customHistory([
        historyPoint({ estimatedWeeklyValueUsd: 94.35, percentageCoverage: 9 }),
        historyPoint({
          timestamp: 3_600_000,
          estimatedWeeklyValueUsd: 158.04,
          percentageCoverage: 53,
        }),
      ]),
    );

    fireEvent(chart, new MouseEvent('pointerdown', { bubbles: true, clientX: 0 }));
    fireEvent(chart, new MouseEvent('pointermove', { bubbles: true, clientX: 1000 }));

    expect(screen.getByText('Comparison unavailable · estimator calibration')).toBeInTheDocument();
    expect(screen.queryByText(/\([+−]\d+\.\d{2}%\)/)).not.toBeInTheDocument();
    expect(chart.closest('.usage-chart')).toHaveClass('chart-neutral');
    expect(screen.getByText('≈$158')).toBeInTheDocument();
  });

  it('rejects an immature cross-window anchor without falling back to backend baseline', () => {
    const chart = renderHomeWithHistory(
      customHistory([
        historyPoint({
          estimatedWeeklyValueUsd: 72.62,
          confidence: 'medium',
          percentageCoverage: 8,
        }),
        historyPoint({
          timestamp: 3_600_000,
          epoch: 2,
          estimatedWeeklyValueUsd: 160.84,
          percentageCoverage: 51,
        }),
      ]),
    );

    fireEvent(chart, new MouseEvent('pointerdown', { bubbles: true, clientX: 0 }));
    fireEvent(chart, new MouseEvent('pointermove', { bubbles: true, clientX: 1000 }));

    expect(
      screen.getByText('Comparison unavailable · endpoints need mature history'),
    ).toBeInTheDocument();
    expect(screen.queryByText(/\([+−]\d+\.\d{2}%\)/)).not.toBeInTheDocument();
    expect(screen.queryByText('Selected range')).not.toBeInTheDocument();
    expect(chart.closest('.usage-chart')).toHaveClass('chart-neutral');
  });

  it('does not treat a hover without an anchor as a selected-range comparison', () => {
    const chart = renderHomeWithHistory(
      customHistory([
        historyPoint({ estimatedWeeklyValueUsd: 100 }),
        historyPoint({
          timestamp: 3_600_000,
          epoch: 2,
          estimatedWeeklyValueUsd: 200,
        }),
      ]),
    );
    const hoverEvent = new MouseEvent('pointermove', { bubbles: true, clientX: 1000 });
    Object.defineProperty(hoverEvent, 'pointerType', { value: 'mouse' });
    fireEvent(chart, hoverEvent);

    expect(screen.queryByText('Selected range')).not.toBeInTheDocument();
    expect(screen.queryByText(/\([+−]\d+\.\d{2}%\)/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Comparison unavailable/)).not.toBeInTheDocument();
  });

  it('switches cached ranges without remounting the chart or keeping a weekly label', async () => {
    const user = userEvent.setup();
    render(<App />);
    const chart = await screen.findByRole('img', {
      name: /Estimated weekly API-equivalent value/,
    });

    await user.click(screen.getByRole('tab', { name: '1M' }));

    expect(screen.getByText(/^Since /)).toBeInTheDocument();
    expect(screen.getByRole('img', { name: /Estimated weekly API-equivalent value/ })).toBe(chart);
  });
});
