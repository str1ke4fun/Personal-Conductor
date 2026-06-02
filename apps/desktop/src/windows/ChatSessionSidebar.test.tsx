import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  ensureChatSessionMock: vi.fn(),
  listChatSessionsMock: vi.fn(),
  createChatSessionMock: vi.fn(),
  renameChatSessionMock: vi.fn(),
  archiveChatSessionMock: vi.fn(),
  listGoalsMock: vi.fn(),
  listenMock: vi.fn(),
}));

mocks.listenMock.mockResolvedValue(() => {});

vi.mock('@tauri-apps/api/event', () => ({
  listen: mocks.listenMock,
}));

vi.mock('../ipc/invoke', () => ({
  api: {
    ensureChatSession: mocks.ensureChatSessionMock,
    listChatSessions: mocks.listChatSessionsMock,
    createChatSession: mocks.createChatSessionMock,
    renameChatSession: mocks.renameChatSessionMock,
    archiveChatSession: mocks.archiveChatSessionMock,
  },
  listGoals: mocks.listGoalsMock,
}));

import { ChatSessionSidebar } from './ChatSessionSidebar';

describe('ChatSessionSidebar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.ensureChatSessionMock.mockResolvedValue({
      id: 'chat-pinned',
      title: '闲聊',
      workspace_id: 'ws-1',
      message_count: 3,
      last_message_preview: 'Pinned session',
      created_at: '2026-05-31T00:00:00Z',
      updated_at: '2026-05-31T00:00:00Z',
      working: false,
    });
    mocks.listChatSessionsMock.mockResolvedValue([
      {
        id: 'chat-1',
        title: 'Workspace Session',
        workspace_id: 'ws-1',
        message_count: 8,
        last_message_preview: 'Goal work in progress',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:10:00Z',
        working: false,
      },
      {
        id: 'chat-2',
        title: 'Other Workspace Session',
        workspace_id: 'ws-2',
        message_count: 5,
        last_message_preview: 'Other workspace',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:05:00Z',
        working: false,
      },
      {
        id: 'chat-internal',
        title: 'goal-task-exec:task-1',
        workspace_id: 'ws-1',
        message_count: 1,
        last_message_preview: 'internal',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:15:00Z',
        working: true,
      },
    ]);
    mocks.createChatSessionMock.mockResolvedValue({ id: 'chat-new' });
    mocks.renameChatSessionMock.mockResolvedValue({});
    mocks.archiveChatSessionMock.mockResolvedValue({});
    mocks.listGoalsMock.mockResolvedValue([
      {
        id: 'goal-1',
        workspace_id: 'ws-1',
        title: 'Ship runtime loop',
        objective: 'Finish the goal runtime',
        status: 'running',
        priority: 'normal',
        owner: 'user',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:05:00Z',
      },
      {
        id: 'goal-2',
        workspace_id: 'ws-1',
        title: 'Archived goal',
        objective: 'done',
        status: 'accepted',
        priority: 'normal',
        owner: 'user',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:05:00Z',
      },
    ]);
  });

  it('shows active workspace goal summary in the session sidebar', async () => {
    render(
      <ChatSessionSidebar
        activeSessionId="chat-1"
        onSelectSession={vi.fn()}
        currentWorkspaceId="ws-1"
      />
    );

    await waitFor(() => {
      expect(mocks.listGoalsMock).toHaveBeenCalledWith('ws-1');
    });

    expect(await screen.findByText('Goals 1 active')).toBeTruthy();
    expect(screen.getByText('Goal: Ship runtime loop · Running')).toBeTruthy();
  });

  it('does not show workspace goal summary on sessions from another workspace', async () => {
    render(
      <ChatSessionSidebar
        activeSessionId="chat-2"
        onSelectSession={vi.fn()}
        currentWorkspaceId="ws-1"
      />
    );

    await screen.findByText('Other Workspace Session');

    const otherSessionCard = screen.getByText('Other Workspace Session').closest('.session-item');
    expect(otherSessionCard?.textContent).not.toContain('Goal: Ship runtime loop · Running');
  });

  it('hides internal goal execution sessions from the sidebar', async () => {
    render(
      <ChatSessionSidebar
        activeSessionId="chat-1"
        onSelectSession={vi.fn()}
        currentWorkspaceId="ws-1"
      />
    );

    await screen.findByText('Workspace Session');

    expect(screen.queryByText('goal-task-exec:task-1')).toBeNull();
  });
});
