import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  listTasks: vi.fn(),
  listAgentTasks: vi.fn(),
  listAgentRuns: vi.fn(),
  listProposals: vi.fn(),
  listWorkspaceActivityProjection: vi.fn(),
  listenMock: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: mocks.listenMock,
}));

vi.mock('../ipc/invoke', () => ({
  api: {
    listTasks: mocks.listTasks,
    listAgentTasks: mocks.listAgentTasks,
    listAgentRuns: mocks.listAgentRuns,
    listProposals: mocks.listProposals,
  },
  listWorkspaceActivityProjection: mocks.listWorkspaceActivityProjection,
}));

import { TaskDrawerPane } from './TaskDrawerPane';

describe('TaskDrawerPane', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listenMock.mockResolvedValue(() => {});
    mocks.listTasks.mockResolvedValue([]);
    mocks.listAgentTasks.mockResolvedValue([]);
    mocks.listAgentRuns.mockResolvedValue([]);
    mocks.listProposals.mockResolvedValue([]);
    mocks.listWorkspaceActivityProjection.mockResolvedValue({
      workspace_id: 'ws-1',
      active: [
        {
          activity_id: 'goal_task:task-1',
          kind: 'goal_cycle',
          status: 'running',
          title: 'Render projection drawer',
          actor: 'claude_p',
          started_at: '2026-05-31T00:00:00Z',
          updated_at: '2026-05-31T00:01:00Z',
          session_id: null,
          goal_id: 'goal-1',
          task_id: 'task-1',
          assistant_message: 'Working through the goal task',
          tool_calls: [{ id: 'tc-1', tool_id: 'files.read', status: 'completed', command_run_id: 'cr-1', risk_level: 'read_only' }],
          command_runs: [{ id: 'cr-1', command: 'cargo check', status: 'exited', exit_code: 0 }],
          agent_runs: [{ id: 'ar-1', agent_id: 'claude_p', status: 'running', output_ref: 'runs/ar-1-output.json', error: null }],
          agent_teams: [{ id: 'team-1', name: 'Execution Team', lifecycle: 'executing' }],
          artifacts: [
            { label: 'agent_run', file: null, summary_ref: null, output_ref: 'runs/ar-1-output.json', result_ref: null },
            { label: 'command_run', file: null, summary_ref: null, output_ref: null, result_ref: 'cargo check passed' },
          ],
        },
      ],
      records: [
        {
          activity_id: 'chat:session-1',
          kind: 'chat_turn',
          status: 'completed',
          title: 'Projection Session',
          actor: 'assistant',
          started_at: '2026-05-31T00:00:00Z',
          updated_at: '2026-05-31T00:02:00Z',
          session_id: 'session-1',
          goal_id: null,
          task_id: null,
          assistant_message: 'Built workspace projection',
          tool_calls: [{ id: 'tc-2', tool_id: 'files.read', status: 'completed', command_run_id: 'cr-2', risk_level: 'read_only' }],
          command_runs: [{ id: 'cr-2', command: 'cargo check', status: 'exited', exit_code: 0 }],
          agent_runs: [{ id: 'ar-2', agent_id: 'review_agent', status: 'succeeded', output_ref: 'runs/ar-2-output.json', error: null }],
          agent_teams: [{ id: 'team-2', name: 'Review Team', lifecycle: 'accepted' }],
          artifacts: [
            { label: 'command_run', file: null, summary_ref: null, output_ref: null, result_ref: 'cargo check passed' },
            { label: 'legacy_task', file: 'docs/workspace.md', summary_ref: 'summary://workspace-projection', output_ref: null, result_ref: null },
          ],
        },
      ],
    });
  });

  it('groups in-progress work under the busy section and reveals detail on expand', async () => {
    render(<TaskDrawerPane workspaceId="ws-1" />);

    await waitFor(() => {
      expect(mocks.listWorkspaceActivityProjection).toHaveBeenCalledWith('ws-1', 12);
    });

    // Busy section header with a count; the in-progress activity surfaces as a
    // one-line summary (title + actor) without dumping every field.
    expect(screen.getByText(/正在推进/)).toBeTruthy();
    const summary = screen.getByText('Render projection drawer');
    expect(summary).toBeTruthy();
    const wrapper = summary.closest('.drawer-card-wrapper');
    expect(wrapper?.className).toContain('collapsed');

    fireEvent.click(summary);

    expect(wrapper?.className).toContain('expanded');
    expect(await screen.findByText(/工具: files.read/)).toBeTruthy();
    expect(screen.getByText(/命令: cargo check/)).toBeTruthy();
    expect(screen.getByText(/claude_p\(running\)/)).toBeTruthy();
  });

  it('subscribes to goal and agent-team refresh events for projection updates', async () => {
    render(<TaskDrawerPane workspaceId="ws-1" />);

    await waitFor(() => {
      expect(mocks.listWorkspaceActivityProjection).toHaveBeenCalledWith('ws-1', 12);
    });

    expect(mocks.listenMock).toHaveBeenCalledWith('tasks_changed', expect.any(Function));
    expect(mocks.listenMock).toHaveBeenCalledWith('agent_runs_changed', expect.any(Function));
    expect(mocks.listenMock).toHaveBeenCalledWith('proposal-changed', expect.any(Function));
    expect(mocks.listenMock).toHaveBeenCalledWith('goals_changed', expect.any(Function));
    expect(mocks.listenMock).toHaveBeenCalledWith('agent_teams_changed', expect.any(Function));
  });
});
