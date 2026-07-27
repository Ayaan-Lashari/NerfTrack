import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import App from './App';

describe('Nerfify app shell', () => {
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
    expect(screen.getByText('Set up Nerfify')).toBeInTheDocument();
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

    fireEvent(chart, new MouseEvent('pointerdown', { bubbles: true, clientX: 100 }));
    fireEvent(chart, new MouseEvent('pointermove', { bubbles: true, clientX: 900 }));

    expect(screen.getByText('Selected range').parentElement).toHaveTextContent(
      /[+−]\$\d+\.\d{2} \([+−]\d+\.\d{2}%\)/,
    );
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
