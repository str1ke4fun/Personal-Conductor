import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  listWorkspacesMock: vi.fn(),
  listChatSessionsMock: vi.fn(),
  createChatSessionMock: vi.fn(),
  getWorkspaceStatusMock: vi.fn(),
  updateChatSessionWorkspaceMock: vi.fn(),
  attachWorkspaceMock: vi.fn(),
  ensureChatSessionMock: vi.fn(),
  renameChatSessionMock: vi.fn(),
  archiveChatSessionMock: vi.fn(),
  setChatSessionKindMock: vi.fn(),
  listGoalsMock: vi.fn(),
  createGoalMock: vi.fn(),
  updateGoalStatusMock: vi.fn(),
  updateGoalObjectiveMock: vi.fn(),
  appendGoalUserMessageMock: vi.fn(),
  getGoalCyclesMock: vi.fn(),
  listGoalTasksMock: vi.fn(),
  startGoalMock: vi.fn(),
  approveGoalPlanMock: vi.fn(),
  rejectGoalPlanMock: vi.fn(),
  pauseGoalMock: vi.fn(),
  resumeGoalMock: vi.fn(),
  cancelGoalMock: vi.fn(),
  submitGoalReviewVerdictMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: vi.fn(),
    hide: vi.fn(),
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

vi.mock('./useChatSession', () => ({
  useChatSession: () => ({
    messages: [],
    input: '',
    setInput: vi.fn(),
    sending: false,
    streamTokens: [],
    toolStates: new Map(),
    thinkingContent: null,
    projectedRuns: [],
    endRef: { current: null },
    retryMessage: vi.fn(),
    approveProposal: vi.fn(),
    rejectProposal: vi.fn(),
    turnStartedAt: null,
    currentPhase: null,
    toolRunCount: 0,
    activeToolCount: 0,
    sendMessage: vi.fn(),
    sendPrompt: vi.fn(),
    createGoalFromSeed: vi.fn(),
    cancelSending: vi.fn(),
  }),
}));

vi.mock('./ChatComposer', () => ({
  ChatComposer: () => <div data-testid="chat-composer">composer</div>,
}));

vi.mock('./ChatTimelinePane', () => ({
  ChatTimelinePane: () => <div data-testid="chat-timeline">timeline</div>,
}));

vi.mock('./TaskDrawerPane', () => ({
  TaskDrawerPane: () => <div data-testid="task-drawer-pane">tasks</div>,
}));

vi.mock('./GoalConsole', () => ({
  default: () => <div data-testid="goal-console">goal console</div>,
}));

vi.mock('./AgentLanes', () => ({
  default: () => <div data-testid="agent-lanes">lanes</div>,
}));

vi.mock('./ReviewQueue', () => ({
  default: () => <div data-testid="review-queue">review</div>,
}));

vi.mock('./AgentTranscript', () => ({
  default: () => <div data-testid="agent-transcript">transcript</div>,
}));

vi.mock('../ipc/invoke', () => ({
  api: {
    listWorkspaces: mocks.listWorkspacesMock,
    listChatSessions: mocks.listChatSessionsMock,
    createChatSession: mocks.createChatSessionMock,
    getWorkspaceStatus: mocks.getWorkspaceStatusMock,
    updateChatSessionWorkspace: mocks.updateChatSessionWorkspaceMock,
    attachWorkspace: mocks.attachWorkspaceMock,
    ensureChatSession: mocks.ensureChatSessionMock,
    renameChatSession: mocks.renameChatSessionMock,
    archiveChatSession: mocks.archiveChatSessionMock,
    setChatSessionKind: mocks.setChatSessionKindMock,
  },
  listGoals: mocks.listGoalsMock,
  createGoal: mocks.createGoalMock,
  updateGoalStatus: mocks.updateGoalStatusMock,
  updateGoalObjective: mocks.updateGoalObjectiveMock,
  appendGoalUserMessage: mocks.appendGoalUserMessageMock,
  getGoalCycles: mocks.getGoalCyclesMock,
  listGoalTasks: mocks.listGoalTasksMock,
  startGoal: mocks.startGoalMock,
  approveGoalPlan: mocks.approveGoalPlanMock,
  rejectGoalPlan: mocks.rejectGoalPlanMock,
  pauseGoal: mocks.pauseGoalMock,
  resumeGoal: mocks.resumeGoalMock,
  cancelGoal: mocks.cancelGoalMock,
  submitGoalReviewVerdict: mocks.submitGoalReviewVerdictMock,
}));

import { AgentWorkspacePanel } from './AgentWorkspacePanel';

describe('AgentWorkspacePanel goal mount', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listWorkspacesMock.mockResolvedValue([
      {
        id: 'ws-1',
        name: 'Personal Agent',
        root: 'I:/personal-agent',
        last_active_at: '2026-05-31T00:00:00Z',
      },
    ]);
    mocks.listChatSessionsMock.mockResolvedValue([
      {
        id: 'session-1',
        title: 'Workspace Session',
        workspace_id: 'ws-1',
        message_count: 2,
        last_message_preview: 'Working on goals',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:05:00Z',
        working: false,
      },
    ]);
    mocks.createChatSessionMock.mockResolvedValue({ id: 'session-new' });
    mocks.getWorkspaceStatusMock.mockResolvedValue({
      workspace_id: 'ws-1',
      root: 'I:/personal-agent',
      exists: true,
      git_branch: null,
      dirty: false,
    });
    mocks.updateChatSessionWorkspaceMock.mockResolvedValue({});
    mocks.attachWorkspaceMock.mockResolvedValue({ id: 'ws-1' });
    mocks.ensureChatSessionMock.mockResolvedValue({
      id: 'chat-pinned',
      title: '闲聊',
      workspace_id: 'ws-1',
      message_count: 1,
      last_message_preview: 'Pinned',
      created_at: '2026-05-31T00:00:00Z',
      updated_at: '2026-05-31T00:00:00Z',
      working: false,
    });
    mocks.renameChatSessionMock.mockResolvedValue({});
    mocks.archiveChatSessionMock.mockResolvedValue({});
    mocks.setChatSessionKindMock.mockResolvedValue(undefined);
    mocks.listGoalsMock.mockResolvedValue([
      {
        id: 'goal-1',
        workspace_id: 'ws-1',
        title: 'Ship runtime loop',
        objective: 'Finish the goal runtime chain',
        status: 'running',
        priority: 'normal',
        owner: 'user',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:05:00Z',
      },
    ]);
    mocks.createGoalMock.mockResolvedValue({});
    mocks.updateGoalStatusMock.mockResolvedValue({});
    mocks.updateGoalObjectiveMock.mockResolvedValue({});
    mocks.appendGoalUserMessageMock.mockResolvedValue(undefined);
    mocks.getGoalCyclesMock.mockResolvedValue([]);
    mocks.listGoalTasksMock.mockResolvedValue([]);
    mocks.startGoalMock.mockResolvedValue({});
    mocks.approveGoalPlanMock.mockResolvedValue({});
    mocks.rejectGoalPlanMock.mockResolvedValue({});
    mocks.pauseGoalMock.mockResolvedValue({});
    mocks.resumeGoalMock.mockResolvedValue({});
    mocks.cancelGoalMock.mockResolvedValue({});
    mocks.submitGoalReviewVerdictMock.mockResolvedValue({});
  });

  it('keeps chat as the primary view in goal mode and mounts GoalConsole beside the timeline', async () => {
    render(<AgentWorkspacePanel />);

    await waitFor(() => {
      expect(mocks.listGoalsMock).toHaveBeenCalledWith('ws-1');
    });

    expect(await screen.findByText('Goals 1 active')).toBeTruthy();

    fireEvent.click(screen.getAllByRole('tab')[1]);

    await waitFor(() => {
      expect(mocks.setChatSessionKindMock).toHaveBeenCalledWith('session-1', 'goal', null);
    });

    expect(screen.getByText('Goal 模式')).toBeTruthy();
    expect(screen.getByTestId('chat-timeline')).toBeTruthy();
    expect(screen.getByTestId('chat-composer')).toBeTruthy();
    expect(await screen.findByTestId('goal-console')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: '活动' }));

    expect(await screen.findByTestId('task-drawer-pane')).toBeTruthy();
  });

});
