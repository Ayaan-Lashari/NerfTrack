import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { UsageChart } from './UsageChart';
import { demoAnnotations, getDemoHistory } from '../lib/fixtures';

describe('UsageChart', () => {
  it('supports keyboard nearest-point scrubbing', async () => {
    const user = userEvent.setup();
    render(
      <UsageChart
        points={getDemoHistory('1W').points}
        annotations={demoAnnotations}
        range="1W"
        reducedMotion={false}
        changeValueUsd={-1}
      />,
    );
    const chart = screen.getByRole('img', { name: /Estimated weekly API-equivalent value/ });
    expect(chart.closest('.usage-chart')).toHaveClass('chart-negative');
    await user.click(chart);
    await user.keyboard('{ArrowLeft}');
    expect(document.querySelector('.scrub-readout')).toHaveTextContent(/Observed:/);
  });

  it('scrubs continuously while dragging', () => {
    const onScrub = vi.fn();
    render(
      <UsageChart
        points={getDemoHistory('1W').points}
        annotations={[]}
        range="1W"
        reducedMotion={false}
        onScrub={onScrub}
      />,
    );
    const chart = screen.getByRole('img', { name: /Estimated weekly API-equivalent value/ });
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
    fireEvent(chart, new MouseEvent('pointerdown', { bubbles: true, clientX: 100 }));
    expect(chart).toHaveAttribute('aria-grabbed', 'true');
    fireEvent(chart, new MouseEvent('pointermove', { bubbles: true, clientX: 900 }));
    fireEvent(chart, new MouseEvent('pointerup', { bubbles: true, clientX: 900 }));
    expect(chart).toHaveAttribute('aria-grabbed', 'false');
    expect(chart.querySelector('.chart-anchor-marker')).toBeInTheDocument();
    expect(chart.querySelector('.chart-crosshair')).toBeInTheDocument();
    expect(chart.closest('.usage-chart')).toHaveClass('chart-negative');
    expect(onScrub).toHaveBeenCalledTimes(2);
    expect(onScrub.mock.calls[1][0].timestamp).toBeGreaterThan(onScrub.mock.calls[0][0].timestamp);
    expect(onScrub.mock.calls[1][1]).toEqual(onScrub.mock.calls[0][0]);
  });

  it('interpolates between stored vertices instead of snapping to them', () => {
    const onScrub = vi.fn();
    const points = [
      {
        timestamp: 0,
        estimatedWeeklyValueUsd: 0,
        rawEstimatedWeeklyValueUsd: 0,
        observedCostUsd: 0,
        weeklyUsedPercent: 0,
        resetAt: null,
        resetReason: null,
        isFinalized: true,
        isHeartbeat: false,
        epoch: 1,
        confidence: 'high' as const,
        percentageCoverage: 20,
      },
      {
        timestamp: 3_600_000,
        estimatedWeeklyValueUsd: 100,
        rawEstimatedWeeklyValueUsd: 100,
        observedCostUsd: 10,
        weeklyUsedPercent: 100,
        resetAt: null,
        resetReason: null,
        isFinalized: true,
        isHeartbeat: false,
        epoch: 1,
        confidence: 'high' as const,
        percentageCoverage: 40,
      },
    ];
    render(
      <UsageChart
        points={points}
        annotations={[]}
        range="1D"
        reducedMotion={false}
        onScrub={onScrub}
      />,
    );
    const chart = screen.getByRole('img', { name: /Estimated weekly API-equivalent value/ });
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

    const hoverEvent = new MouseEvent('pointermove', { bubbles: true, clientX: 472 });
    Object.defineProperty(hoverEvent, 'pointerType', { value: 'mouse' });
    fireEvent(chart, hoverEvent);

    const interpolated = onScrub.mock.calls.at(-1)?.[0];
    expect(interpolated.timestamp).toBeCloseTo(1_800_000, -4);
    expect(interpolated.estimatedWeeklyValueUsd).toBeCloseTo(50);
    expect(interpolated.timestamp).not.toBe(points[0].timestamp);
    expect(interpolated.timestamp).not.toBe(points[1].timestamp);

    fireEvent(chart, new MouseEvent('pointerdown', { bubbles: true, clientX: 200 }));
    fireEvent(chart, new MouseEvent('pointermove', { bubbles: true, clientX: 800 }));
    expect(chart.closest('.usage-chart')).toHaveClass('chart-positive');
  });

  it('breaks the rendered path between weekly quota epochs', () => {
    const points = getDemoHistory('1W')
      .points.slice(0, 4)
      .map((point, index) => ({ ...point, epoch: index < 2 ? 1 : 2 }));
    render(<UsageChart points={points} annotations={[]} range="1W" reducedMotion={false} />);

    const path = document.querySelector('.chart-line');
    expect(path?.getAttribute('d')?.match(/M /g)).toHaveLength(2);
  });

  it('compresses substantial inactivity into an unconnected break labeled on hover', () => {
    const points = getDemoHistory('1D')
      .points.slice(0, 2)
      .map((point, index) => ({
        ...point,
        timestamp: index === 0 ? 0 : 12 * 60 * 60 * 1_000,
        epoch: index + 1,
      }));
    render(<UsageChart points={points} annotations={[]} range="1D" reducedMotion={false} />);

    const chart = screen.getByRole('img', { name: /Estimated weekly API-equivalent value/ });
    expect(chart.querySelector('.chart-no-usage-line')).not.toBeInTheDocument();
    expect(chart.querySelector('.chart-inactivity-gap')).toBeInTheDocument();
    expect(chart.querySelector('.chart-line')?.getAttribute('d')?.match(/M /g)).toHaveLength(2);
    const pointXs = [
      ...(chart.querySelector('.chart-line')?.getAttribute('d') ?? '').matchAll(/M ([\d.]+)/g),
    ].map((match) => Number(match[1]));
    expect(pointXs[1] - pointXs[0]).toBeGreaterThan(400);

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
    const hoverEvent = new MouseEvent('pointermove', { bubbles: true, clientX: 472 });
    Object.defineProperty(hoverEvent, 'pointerType', { value: 'mouse' });
    fireEvent(chart, hoverEvent);

    expect(screen.getByRole('status')).toHaveTextContent('No activity · 12h');
    expect(document.querySelector('.scrub-readout')).not.toBeInTheDocument();
  });

  it('stretches all available history from the first to last chart edge', () => {
    const points = getDemoHistory('1D')
      .points.slice(0, 2)
      .map((point, index) => ({
        ...point,
        timestamp: 1_000 + index * 3_600_000,
      }));
    render(<UsageChart points={points} annotations={[]} range="6M" reducedMotion={false} />);

    const labels = [...document.querySelectorAll('.chart-x-label')].map(
      (label) => label.textContent,
    );
    expect(labels[0]).toBe(labels[1]);
    expect(labels.at(-1)).toBe(labels[0]);
    const path = document.querySelector('.chart-line')?.getAttribute('d') ?? '';
    expect(path).toMatch(/^M 0\.00 /);
    expect(path).toContain('L 944.00 ');
  });

  it('uses a pending live endpoint for the time axis without plotting it', () => {
    const points = [
      {
        timestamp: 0,
        estimatedWeeklyValueUsd: 100,
        rawEstimatedWeeklyValueUsd: null,
        observedCostUsd: 1,
        weeklyUsedPercent: 20,
        resetAt: null,
        resetReason: null,
        isFinalized: true,
        isHeartbeat: false,
        epoch: 1,
        confidence: 'high' as const,
        percentageCoverage: 20,
      },
      {
        timestamp: 12 * 60 * 60 * 1_000,
        estimatedWeeklyValueUsd: null,
        rawEstimatedWeeklyValueUsd: null,
        observedCostUsd: null,
        weeklyUsedPercent: 3,
        resetAt: 24 * 60 * 60 * 1_000,
        resetReason: 'reported_reset_changed',
        isFinalized: false,
        isHeartbeat: true,
        epoch: 2,
        confidence: 'none' as const,
        percentageCoverage: null,
      },
    ];
    render(<UsageChart points={points} annotations={[]} range="1D" reducedMotion={false} />);

    const expectedEndLabel = new Date(points[1].timestamp).toLocaleTimeString('en-US', {
      hour: 'numeric',
      minute: '2-digit',
    });
    const labels = [...document.querySelectorAll('.chart-x-label')].map(
      (label) => label.textContent,
    );
    const path = document.querySelector('.chart-line')?.getAttribute('d') ?? '';

    // The null-valued calibration endpoint controls the 1D domain, so the
    // final tick is the latest quota observation rather than a 24-hour fallback.
    expect(labels.at(-1)).toBe(expectedEndLabel);
    // It remains excluded from the plotted estimate series.
    expect(path).toMatch(/^M 0\.00 /);
    expect(path).not.toContain('L 944.00 ');
  });

  it('renders reset annotations as compact icons with hover text', () => {
    const points = getDemoHistory('1W').points;
    const timestamp = points[Math.floor(points.length / 2)].timestamp;
    render(
      <UsageChart
        points={points}
        annotations={[
          { id: 'reset-1', timestamp, label: 'Weekly window · reset changed', kind: 'reset' },
        ]}
        range="1W"
        reducedMotion={false}
      />,
    );

    const marker = screen.getByRole('img', { name: /Reset changed/ });
    expect(marker.querySelector('rect')).not.toBeInTheDocument();
    fireEvent.pointerEnter(marker);
    expect(screen.getByRole('tooltip')).toHaveTextContent('Reset changed');
  });

  it('plots raw estimates and excludes comparison baselines from axis bounds', () => {
    const points = getDemoHistory('1D')
      .points.slice(0, 2)
      .map((point, index) => ({
        ...point,
        estimatedWeeklyValueUsd: 50,
        rawEstimatedWeeklyValueUsd: index === 0 ? 10 : 90,
      }));
    render(
      <UsageChart
        points={points}
        annotations={[]}
        range="1D"
        reducedMotion={false}
        baselineEstimatedWeeklyValueUsd={-1_000}
      />,
    );

    const path = document.querySelector('.chart-line')?.getAttribute('d') ?? '';
    const yCoordinates = [...path.matchAll(/[ML] [\d.]+ ([\d.]+)/g)].map((match) => match[1]);
    expect(new Set(yCoordinates).size).toBe(2);
    expect(screen.queryByText('$-1000')).not.toBeInTheDocument();
  });

  it('renders fixture API-equivalent values with a manual reset boundary', () => {
    render(
      <UsageChart
        points={getDemoHistory('1W').points}
        annotations={demoAnnotations}
        range="1W"
        reducedMotion={false}
      />,
    );
    expect(screen.getByText('Estimated weekly API-equivalent value')).toBeInTheDocument();
    expect(screen.getByText('USD · local token-derived estimate')).toBeInTheDocument();
    expect(document.querySelectorAll('.chart-inactivity-gap')).toHaveLength(2);
    expect(document.querySelector('.chart-line')?.getAttribute('d')?.match(/M /g)?.length).toBe(4);
  });
});
