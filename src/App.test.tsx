import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import App from './App';

describe('Nerfify app shell', () => {
  it('renders the dashboard reference surface with a non-zero quote', async () => {
    render(<App />);
    expect(await screen.findByText('Codex Weekly API Equivalent')).toBeInTheDocument();
    expect(screen.getByText('$371.28')).toBeInTheDocument();
    expect(screen.getByText('Weekly Used')).toBeInTheDocument();
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
      name: /Estimated weekly API equivalent/,
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
});
