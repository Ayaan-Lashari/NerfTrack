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
      />,
    );
    const chart = screen.getByRole('img', { name: /Estimated weekly API equivalent/ });
    await user.click(chart);
    await user.keyboard('{ArrowLeft}');
    expect(screen.getAllByText(/May/).length).toBeGreaterThan(0);
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
    const chart = screen.getByRole('img', { name: /Estimated weekly API equivalent/ });
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
    expect(onScrub).toHaveBeenCalledTimes(2);
    expect(onScrub.mock.calls[1][0].timestamp).toBeGreaterThan(onScrub.mock.calls[0][0].timestamp);
  });

  it('interpolates between stored vertices instead of snapping to them', () => {
    const onScrub = vi.fn();
    const points = [
      {
        timestamp: 0,
        valueUsd: 0,
        rawValueUsd: 0,
        weeklyUsedPercent: 0,
        isFinalized: true,
        isHeartbeat: false,
        dominantModel: null,
      },
      {
        timestamp: 1000,
        valueUsd: 100,
        rawValueUsd: 100,
        weeklyUsedPercent: 100,
        isFinalized: true,
        isHeartbeat: false,
        dominantModel: null,
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
    const chart = screen.getByRole('img', { name: /Estimated weekly API equivalent/ });
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
    expect(interpolated.timestamp).toBeCloseTo(500);
    expect(interpolated.valueUsd).toBeCloseTo(50);
    expect(interpolated.timestamp).not.toBe(points[0].timestamp);
    expect(interpolated.timestamp).not.toBe(points[1].timestamp);
  });
});
