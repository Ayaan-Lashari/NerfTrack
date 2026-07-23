import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
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
});
