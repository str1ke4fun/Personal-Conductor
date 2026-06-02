import React from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { CommandRunCard } from './CommandRunCard';

describe('CommandRunCard', () => {
  it('shows command status and expands to reveal test output', () => {
    render(
      <CommandRunCard
        command="npm test -- --run"
        cwd="I:/personal-agent/apps/desktop"
        status="error"
        stdout="PASS src/windows/ChatTimelinePane.test.tsx"
        stderr="1 failed"
        exitCode={1}
        durationMs={1532}
      />,
    );

    expect(screen.getByText('npm test -- --run')).toBeTruthy();
    expect(screen.getByText('Error')).toBeTruthy();
    expect(screen.getByText('cwd: I:/personal-agent/apps/desktop')).toBeTruthy();

    fireEvent.click(screen.getByText('npm test -- --run'));

    expect(screen.getByText('stdout')).toBeTruthy();
    expect(screen.getByText('stderr')).toBeTruthy();
    expect(screen.getByText('PASS src/windows/ChatTimelinePane.test.tsx')).toBeTruthy();
    expect(screen.getByText('1 failed')).toBeTruthy();
    expect(screen.getByText('exit code: 1')).toBeTruthy();
  });

  it('surfaces a cancel action for running commands', () => {
    const onCancel = vi.fn();
    render(
      <CommandRunCard
        command="cargo test -p conductor-core"
        status="running"
        onCancel={onCancel}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
