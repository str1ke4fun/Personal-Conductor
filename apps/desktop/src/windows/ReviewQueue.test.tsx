import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  listGoalTasksMock: vi.fn(),
  listenMock: vi.fn(),
}));

mocks.listenMock.mockResolvedValue(() => {});

vi.mock('@tauri-apps/api/event', () => ({
  listen: mocks.listenMock,
}));

vi.mock('../ipc/invoke', () => ({
  listGoalTasks: mocks.listGoalTasksMock,
}));

import ReviewQueue from './ReviewQueue';

describe('ReviewQueue', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listenMock.mockResolvedValue(() => {});
    mocks.listGoalTasksMock.mockResolvedValue([
      {
        id: 'task-review',
        workspace_id: 'ws-1',
        goal_id: 'goal-1',
        cycle_id: 'cycle-1',
        title: 'Run cargo check',
        instruction: 'Verify the backend build',
        status: 'review_ready',
        agent_kind: 'backend-agent',
        write_scope_json: ['crates/conductor-core/src/agent_teams.rs'],
        read_scope_json: [],
        allowed_tools_json: [],
        dependencies_json: [],
        acceptance_json: ['cargo check passes', 'review the changed files'],
        result_ref: 'runs/cargo-check.txt',
        error: undefined,
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:01:00Z',
      },
      {
        id: 'task-complete',
        workspace_id: 'ws-1',
        goal_id: 'goal-1',
        cycle_id: 'cycle-1',
        title: 'Build desktop app',
        instruction: 'Verify the frontend build',
        status: 'accepted',
        agent_kind: 'frontend-agent',
        write_scope_json: ['apps/desktop/src/windows/ReviewQueue.tsx', 'apps/desktop/src/windows/ReviewQueue.test.tsx'],
        read_scope_json: [],
        allowed_tools_json: [],
        dependencies_json: [],
        acceptance_json: ['npm run build passes'],
        result_ref: 'runs/npm-build.txt',
        error: undefined,
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:02:00Z',
      },
      {
        id: 'task-fail',
        workspace_id: 'ws-1',
        goal_id: 'goal-1',
        cycle_id: 'cycle-1',
        title: 'Run failing test',
        instruction: 'Capture the error',
        status: 'failed',
        agent_kind: 'backend-agent',
        write_scope_json: [],
        read_scope_json: [],
        allowed_tools_json: [],
        dependencies_json: [],
        acceptance_json: [],
        result_ref: 'runs/failing-test.txt',
        error: 'assertion failed',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:03:00Z',
      },
    ]);
  });

  it('projects write scope, acceptance, and verification output for goal tasks', async () => {
    render(<ReviewQueue goalId="goal-1" />);

    await waitFor(() => {
      expect(mocks.listGoalTasksMock).toHaveBeenCalledWith('goal-1');
    });

    expect(screen.getByText('待你验收 (1)')).toBeTruthy();
    expect(screen.getByText('已结束 (2)')).toBeTruthy();
    expect(screen.getByText('写入范围: crates/conductor-core/src/agent_teams.rs')).toBeTruthy();
    expect(
      screen.getByText('验收标准: cargo check passes | review the changed files')
    ).toBeTruthy();
    expect(screen.getByText('当前结果: 待验收')).toBeTruthy();
    expect(screen.getByText('结果引用: runs/cargo-check.txt')).toBeTruthy();
    expect(
      screen.getByText(
        '写入范围: apps/desktop/src/windows/ReviewQueue.tsx +1'
      )
    ).toBeTruthy();
    expect(screen.getByText('当前结果: 通过')).toBeTruthy();
    expect(screen.getByText('结果引用: runs/npm-build.txt')).toBeTruthy();
    expect(screen.getByText('当前结果: 失败')).toBeTruthy();
    expect(screen.getByText('错误: assertion failed')).toBeTruthy();
  });
});
