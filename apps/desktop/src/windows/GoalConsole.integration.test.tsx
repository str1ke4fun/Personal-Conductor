import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  listGoalsMock: vi.fn(),
  getGoalCyclesMock: vi.fn(),
  listGoalTasksMock: vi.fn(),
  listActiveHeartbeatsMock: vi.fn(),
  listenMock: vi.fn(),
}));

mocks.listenMock.mockResolvedValue(() => {});

vi.mock('@tauri-apps/api/event', () => ({
  listen: mocks.listenMock,
}));

vi.mock('../ipc/invoke', () => ({
  listGoals: mocks.listGoalsMock,
  getGoalCycles: mocks.getGoalCyclesMock,
  listGoalTasks: mocks.listGoalTasksMock,
  listActiveHeartbeats: mocks.listActiveHeartbeatsMock,
}));

import GoalConsole from './GoalConsole';

describe('GoalConsole integration', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(Date, 'now').mockReturnValue(new Date('2026-05-31T00:10:00Z').getTime());
    mocks.listGoalsMock.mockResolvedValue([
      {
        id: 'goal-1',
        workspace_id: 'ws-1',
        title: 'Land agent team flow',
        objective: 'Keep goal progress visible without team-by-team review',
        status: 'awaiting_review',
        priority: 'normal',
        owner: 'user',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:09:00Z',
      },
    ]);
    mocks.getGoalCyclesMock.mockResolvedValue([
      {
        id: 'cycle-1',
        goal_id: 'goal-1',
        cycle_no: 1,
        status: 'summarizing',
        started_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:09:00Z',
      },
    ]);
    mocks.listGoalTasksMock.mockResolvedValue([
      {
        id: 'task-review',
        workspace_id: 'ws-1',
        goal_id: 'goal-1',
        cycle_id: 'cycle-1',
        title: 'Review projected build result',
        instruction: 'Check the task output',
        status: 'review_ready',
        agent_kind: 'backend-agent',
        write_scope_json: ['crates/conductor-core/src/agent_teams.rs'],
        read_scope_json: [],
        allowed_tools_json: [],
        dependencies_json: [],
        acceptance_json: ['cargo check passes', 'mailbox request visible'],
        result_ref: 'runs/cargo-check.txt',
        error: undefined,
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:08:00Z',
      },
      {
        id: 'task-blocked',
        workspace_id: 'ws-1',
        goal_id: 'goal-1',
        cycle_id: 'cycle-1',
        title: 'Resolve writeback edge case',
        instruction: 'Fix the writeback path',
        status: 'blocked',
        agent_kind: 'backend-agent',
        write_scope_json: [],
        read_scope_json: [],
        allowed_tools_json: [],
        dependencies_json: [],
        acceptance_json: [],
        error: 'waiting for follow-up guidance',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:09:00Z',
      },
    ]);
    mocks.listActiveHeartbeatsMock.mockResolvedValue([
      {
        id: 'hb-1',
        workspace_id: 'ws-1',
        agent_id: 'executor',
        goal_id: 'goal-1',
        status: 'reviewing',
        stage_label: 'Summarizing',
        progress_text: 'Writing back the result',
        active_tool_count: 0,
        created_at: '2026-05-31T00:07:00Z',
        expires_at: '2026-05-31T00:08:00Z',
      },
      {
        id: 'hb-other',
        workspace_id: 'ws-1',
        agent_id: 'other-goal',
        goal_id: 'goal-2',
        status: 'working',
        stage_label: 'Ignored',
        progress_text: 'Should not render',
        active_tool_count: 0,
        created_at: '2026-05-31T00:07:00Z',
        expires_at: '2026-05-31T00:08:00Z',
      },
    ]);
  });

  it('stays read-only, filters to the active goal, and hides task ids/result refs', async () => {
    render(<GoalConsole workspaceId="ws-1" goalId="goal-1" />);

    await waitFor(() => {
      expect(mocks.listGoalTasksMock).toHaveBeenCalledWith('goal-1');
      expect(mocks.listActiveHeartbeatsMock).toHaveBeenCalledWith('ws-1');
    });

    expect(screen.getByText('Land agent team flow')).toBeTruthy();
    expect(screen.getByText(/executor · Writing back the result/)).toBeTruthy();
    expect(screen.queryByText(/other-goal/)).toBeNull();
    expect(screen.getByText('Review projected build result')).toBeTruthy();
    expect(screen.getByText('Resolve writeback edge case')).toBeTruthy();
    expect(screen.getByText('waiting for follow-up guidance')).toBeTruthy();
    expect(screen.queryByText(/task-review/)).toBeNull();
    expect(screen.queryByText(/runs\/cargo-check\.txt/)).toBeNull();
    expect(screen.queryAllByRole('button')).toHaveLength(0);
  });
});
