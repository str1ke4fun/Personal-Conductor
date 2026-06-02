import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  listActiveHeartbeatsMock: vi.fn(),
  listAgentTeamsMock: vi.fn(),
  getAgentTeamSnapshotMock: vi.fn(),
  submitAgentTeamPlanVerdictMock: vi.fn(),
  listenMock: vi.fn(),
}));

mocks.listenMock.mockResolvedValue(() => {});

vi.mock('@tauri-apps/api/event', () => ({
  listen: mocks.listenMock,
}));

vi.mock('../ipc/invoke', () => ({
  listActiveHeartbeats: mocks.listActiveHeartbeatsMock,
  api: {
    listAgentTeams: mocks.listAgentTeamsMock,
    getAgentTeamSnapshot: mocks.getAgentTeamSnapshotMock,
    submitAgentTeamPlanVerdict: mocks.submitAgentTeamPlanVerdictMock,
  },
}));

import { AgentLanes } from './AgentLanes';

describe('AgentLanes', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(Date, 'now').mockReturnValue(new Date('2026-05-31T00:00:10Z').getTime());
    mocks.listActiveHeartbeatsMock.mockResolvedValue([]);
    mocks.submitAgentTeamPlanVerdictMock.mockResolvedValue({});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('shows executor binding details from the team snapshot', async () => {
    mocks.listAgentTeamsMock.mockResolvedValue([
      {
        id: 'team-1',
        name: 'Execution Team',
        workspace_id: 'ws-1',
        status: 'active',
        lifecycle: 'executing',
        write_scope: ['crates/conductor-core/src/agent_teams.rs'],
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:10Z',
        metadata_json: null,
      },
    ]);
    mocks.getAgentTeamSnapshotMock.mockResolvedValue({
      team: {
        id: 'team-1',
        name: 'Execution Team',
        workspace_id: 'ws-1',
        status: 'active',
        lifecycle: 'executing',
        write_scope: ['crates/conductor-core/src/agent_teams.rs'],
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:10Z',
        metadata_json: null,
      },
      members: [
        {
          team_id: 'team-1',
          agent_id: 'coder',
          role: 'executor',
          run_id: 'run-123',
          cwd: 'I:/personal-agent',
          status: 'active',
          subscriptions: [],
          created_at: '2026-05-31T00:00:00Z',
          updated_at: '2026-05-31T00:00:06Z',
          metadata_json: {
            task_id: 'task-7',
            external_session_id: 'session-9',
          },
        },
      ],
      recent_messages: [
        {
          id: 'message-1',
          team_id: 'team-1',
          sender_agent_id: 'reviewer',
          recipient_agent_id: null,
          kind: 'review_verdict_request',
          content: 'Review the latest execution.',
          read_at: null,
          created_at: '2026-05-31T00:00:09Z',
          metadata_json: null,
        },
      ],
    });

    render(<AgentLanes workspaceId="ws-1" />);

    await waitFor(() => {
      expect(screen.getByText('已绑定执行器')).toBeTruthy();
    });

    expect(screen.getByText('Execution Team')).toBeTruthy();
    expect(screen.getByText('执行中')).toBeTruthy();
    expect(screen.getByText('写入范围: crates/conductor-core/src/agent_teams.rs')).toBeTruthy();
    expect(screen.getByText('最近请求:')).toBeTruthy();
    expect(screen.getByText(/Review the latest execution\./)).toBeTruthy();
    expect(screen.getByText(/coder/i).closest('.agent-lane-task')?.textContent).toBe(
      'coder | 已绑定执行器 | heartbeat 4s'
    );
    expect(screen.queryByText(/task-7/)).toBeNull();
    expect(screen.queryByText(/session-9/)).toBeNull();
  });

  it('shows a collaboration container when no executor is bound yet', async () => {
    mocks.listAgentTeamsMock.mockResolvedValue([
      {
        id: 'team-2',
        name: 'Planning Team',
        workspace_id: 'ws-1',
        status: 'active',
        lifecycle: 'awaiting_plan_approval',
        write_scope: ['crates/conductor-core/src/chat/send_v2.rs', 'apps/desktop/src/windows/ChatTimelinePane.tsx'],
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:10Z',
        metadata_json: null,
      },
    ]);
    mocks.getAgentTeamSnapshotMock.mockResolvedValue({
      team: {
        id: 'team-2',
        name: 'Planning Team',
        workspace_id: 'ws-1',
        status: 'active',
        lifecycle: 'awaiting_plan_approval',
        write_scope: ['crates/conductor-core/src/chat/send_v2.rs', 'apps/desktop/src/windows/ChatTimelinePane.tsx'],
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:10Z',
        metadata_json: null,
      },
      members: [
        {
          team_id: 'team-2',
          agent_id: 'planner',
          role: 'planner',
          run_id: null,
          cwd: null,
          status: 'active',
          subscriptions: [],
          created_at: '2026-05-31T00:00:00Z',
          updated_at: '2026-05-31T00:00:08Z',
          metadata_json: {
            note: 'no executor yet',
          },
        },
      ],
      recent_messages: [
        {
          id: 'message-2',
          team_id: 'team-2',
          sender_agent_id: 'planner',
          recipient_agent_id: null,
          kind: 'plan_approval_request',
          content: 'Approve the execution split.',
          read_at: null,
          created_at: '2026-05-31T00:00:09Z',
          metadata_json: null,
        },
      ],
    });

    render(<AgentLanes workspaceId="ws-1" />);

    await waitFor(() => {
      expect(screen.getByText('Planning Team')).toBeTruthy();
    });

    expect(screen.getByText('Planning Team')).toBeTruthy();
    expect(screen.getAllByText('准备执行')).toHaveLength(2);
    expect(
      screen.getByText(
        '写入范围: crates/conductor-core/src/chat/send_v2.rs +1'
      )
    ).toBeTruthy();
    expect(screen.getByText('最近计划:')).toBeTruthy();
    expect(screen.getByText(/Approve the execution split\./)).toBeTruthy();
    expect(screen.getByText('还没有绑定实际执行器。')).toBeTruthy();
    expect(screen.queryByText(/不会再次要求你审批/)).toBeNull();
  });

  it('auto-advances plan verdicts from collaboration containers', async () => {
    mocks.listAgentTeamsMock.mockResolvedValue([
      {
        id: 'team-2',
        name: 'Planning Team',
        workspace_id: 'ws-1',
        status: 'active',
        lifecycle: 'awaiting_plan_approval',
        write_scope: [],
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:10Z',
        metadata_json: null,
      },
    ]);
    mocks.getAgentTeamSnapshotMock.mockResolvedValue({
      team: {
        id: 'team-2',
        name: 'Planning Team',
        workspace_id: 'ws-1',
        status: 'active',
        lifecycle: 'awaiting_plan_approval',
        write_scope: [],
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:10Z',
        metadata_json: null,
      },
      members: [],
      recent_messages: [],
    });

    render(<AgentLanes workspaceId="ws-1" />);

    await waitFor(() => {
      expect(mocks.submitAgentTeamPlanVerdictMock).toHaveBeenCalledWith('team-2', 'approved');
    });

    expect(screen.queryByRole('button')).toBeNull();
  });

  it('keeps review-stage executor teams read-only', async () => {
    mocks.listAgentTeamsMock.mockResolvedValue([
      {
        id: 'team-3',
        name: 'Review Team',
        workspace_id: 'ws-1',
        status: 'active',
        lifecycle: 'awaiting_review',
        write_scope: [],
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:10Z',
        metadata_json: null,
      },
    ]);
    mocks.getAgentTeamSnapshotMock.mockResolvedValue({
      team: {
        id: 'team-3',
        name: 'Review Team',
        workspace_id: 'ws-1',
        status: 'active',
        lifecycle: 'awaiting_review',
        write_scope: [],
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:10Z',
        metadata_json: null,
      },
      members: [
        {
          team_id: 'team-3',
          agent_id: 'coder',
          role: 'executor',
          run_id: 'run-789',
          cwd: 'I:/personal-agent',
          status: 'active',
          subscriptions: [],
          created_at: '2026-05-31T00:00:00Z',
          updated_at: '2026-05-31T00:00:08Z',
          metadata_json: {},
        },
      ],
      recent_messages: [],
    });

    render(<AgentLanes workspaceId="ws-1" />);
    await waitFor(() => {
      expect(screen.getByText('Review Team')).toBeTruthy();
    });
    expect(screen.queryByRole('button')).toBeNull();
  });
});
