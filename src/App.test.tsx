import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
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
});
