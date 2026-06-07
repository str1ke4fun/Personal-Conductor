import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  listGoalsMock: vi.fn(),
  getGoalCyclesMock: vi.fn(),
  listGoalTasksMock: vi.fn(),
  listGoalHintsMock: vi.fn(),
  getGoalGraphMock: vi.fn(),
  createGoalHintMock: vi.fn(),
  dismissGoalHintMock: vi.fn(),
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
  listGoalHints: mocks.listGoalHintsMock,
  getGoalGraph: mocks.getGoalGraphMock,
  createGoalHint: mocks.createGoalHintMock,
  dismissGoalHint: mocks.dismissGoalHintMock,
  listActiveHeartbeats: mocks.listActiveHeartbeatsMock,
}));

import GoalConsole from './GoalConsole';

describe('GoalConsole', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listGoalsMock.mockResolvedValue([
      {
        id: 'goal-1',
        workspace_id: 'ws-1',
        title: 'Goal One',
        objective: 'Ship the integration',
        status: 'running',
        priority: 'normal',
        owner: 'user',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:00Z',
      },
      {
        id: 'goal-2',
        workspace_id: 'ws-1',
        title: 'Goal Two',
        objective: 'Should stay hidden',
        status: 'accepted',
        priority: 'normal',
        owner: 'user',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:00Z',
      },
    ]);
    mocks.getGoalCyclesMock.mockResolvedValue([
      {
        id: 'cycle-1',
        goal_id: 'goal-1',
        cycle_no: 2,
        status: 'running',
        started_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:05:00Z',
      },
    ]);
    mocks.listGoalTasksMock.mockResolvedValue([
      {
        id: 'task-1',
        workspace_id: 'ws-1',
        goal_id: 'goal-1',
        cycle_id: 'cycle-1',
        title: 'Implement runtime path',
        instruction: 'Do it',
        status: 'running',
        agent_kind: 'backend-agent',
        write_scope_json: [],
        read_scope_json: [],
        allowed_tools_json: [],
        dependencies_json: [],
        acceptance_json: [],
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:02:00Z',
      },
    ]);
    mocks.listActiveHeartbeatsMock.mockResolvedValue([
      {
        id: 'hb-1',
        workspace_id: 'ws-1',
        agent_id: 'planner',
        goal_id: 'goal-1',
        status: 'working',
        stage_label: 'Planning',
        progress_text: 'Splitting tasks',
        active_tool_count: 1,
        created_at: '2026-05-31T00:01:00Z',
        expires_at: '2026-05-31T00:02:00Z',
      },
    ]);
    mocks.listGoalHintsMock.mockResolvedValue([]);
    mocks.getGoalGraphMock.mockResolvedValue({
      goal_id: 'goal-1',
      facts: [],
      intents: [],
      hints: [],
      recent_events: [],
      facts_count: 0,
      open_intents_count: 0,
      events_count: 0,
      chat_turn_request_id: null,
    });
    mocks.createGoalHintMock.mockResolvedValue(undefined);
    mocks.dismissGoalHintMock.mockResolvedValue(undefined);
  });

  it('renders only the current goal session status and exposes no manual actions', async () => {
    const { container } = render(<GoalConsole workspaceId="ws-1" goalId="goal-1" />);

    await waitFor(() => {
      expect(mocks.listGoalTasksMock).toHaveBeenCalledWith('goal-1');
      expect(mocks.listActiveHeartbeatsMock).toHaveBeenCalledWith('ws-1');
    });

    expect(screen.getByText('Goal One')).toBeTruthy();
    expect(screen.getByText('Ship the integration')).toBeTruthy();
    expect(screen.queryByText('Goal Two')).toBeNull();
    expect(screen.getByText(/planner · Splitting tasks/)).toBeTruthy();
    expect(screen.getByText('Implement runtime path')).toBeTruthy();
    expect(screen.queryByText(/task-1/)).toBeNull();
    expect(container.querySelectorAll('.btn-action, .btn-primary')).toHaveLength(0);
  });

  it('shows a minimal empty state when the session has no goal yet', () => {
    render(<GoalConsole workspaceId="ws-1" goalId={null} />);

    expect(screen.getByText('当前会话还没有关联 Goal。')).toBeTruthy();
  });
});
