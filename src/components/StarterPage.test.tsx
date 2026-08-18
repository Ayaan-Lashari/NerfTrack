import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { StarterPage } from './StarterPage';

describe('StarterPage', () => {
  it('lets people continue without opening GitHub', async () => {
    const user = userEvent.setup();
    const onComplete = vi.fn().mockResolvedValue(undefined);

    render(<StarterPage version="1.1.1" onComplete={onComplete} />);

    const skipButton = screen.getByRole('button', { name: 'Continue without starring' });
    expect(skipButton).toBeEnabled();
    await user.click(skipButton);

    expect(onComplete).toHaveBeenCalledOnce();
  });
});
