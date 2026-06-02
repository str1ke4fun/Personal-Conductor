import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  chatInput: '',
  setInputMock: vi.fn(),
  sendMessageMock: vi.fn(),
  sendPromptMock: vi.fn(),
  createGoalFromSeedMock: vi.fn(),
  listWorkspacesMock: vi.fn(),
  listChatSessionsMock: vi.fn(),
  createChatSessionMock: vi.fn(),
  setChatSessionKindMock: vi.fn(),
  getWorkspaceStatusMock: vi.fn(),
  updateChatSessionWorkspaceMock: vi.fn(),
  attachWorkspaceMock: vi.fn(),
  listGoalsMock: vi.fn(),
  createGoalMock: vi.fn(),
  updateGoalStatusMock: vi.fn(),
  updateGoalObjectiveMock: vi.fn(),
  resumeGoalMock: vi.fn(),
  approveGoalPlanMock: vi.fn(),
  appendGoalUserMessageMock: vi.fn(),
  closeWindowMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: mocks.closeWindowMock,
  }),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

vi.mock('./useChatSession', () => ({
  useChatSession: () => ({
    messages: [],
    input: mocks.chatInput,
    setInput: mocks.setInputMock,
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
    sendMessage: mocks.sendMessageMock,
    sendPrompt: mocks.sendPromptMock,
    createGoalFromSeed: mocks.createGoalFromSeedMock,
    cancelSending: vi.fn(),
  }),
}));

vi.mock('./ChatSessionSidebar', () => ({
  ChatSessionSidebar: () => <div data-testid="chat-session-sidebar">sidebar</div>,
}));

vi.mock('./ChatComposer', () => ({
  ChatComposer: ({
    input,
    onSend,
  }: {
    input: string;
    onSend: (options: { taskMode: 'short'; capability: 'ask_write'; planOnly: false }) => Promise<void> | void;
  }) => (
    <div data-testid="chat-composer">
      <span data-testid="chat-composer-input">{input}</span>
      <button
        type="button"
        onClick={() => void onSend({ taskMode: 'short', capability: 'ask_write', planOnly: false })}
      >
        Trigger send
      </button>
    </div>
  ),
}));

vi.mock('./TaskDrawerPane', () => ({
  TaskDrawerPane: () => <div data-testid="task-drawer-pane">drawer</div>,
}));

vi.mock('./GoalConsole', () => ({
  default: () => <div data-testid="goal-console">goals</div>,
}));

vi.mock('./ChatTimelinePane', () => ({
  ChatTimelinePane: ({
    onApprovePlan,
    onRejectPlan,
  }: {
    onApprovePlan?: (plan: {
      title: string;
      steps: Array<{ title: string; detail?: string }>;
      writeScope?: string[];
      diffPreview?: string;
    }) => Promise<void> | void;
    onRejectPlan?: (plan: {
      title: string;
      steps: Array<{ title: string; detail?: string }>;
      writeScope?: string[];
      diffPreview?: string;
    }) => Promise<void> | void;
  }) => (
    <div>
      <button
        type="button"
        onClick={() =>
          void onApprovePlan?.({
            title: 'Runtime plan',
            steps: [{ title: 'Run tests' }],
            writeScope: ['crates/conductor-core/src/runtime_api.rs'],
            diffPreview: 'diff preview',
          })
        }
      >
        Trigger approve plan
      </button>
      <button
        type="button"
        onClick={() =>
          void onRejectPlan?.({
            title: 'Runtime plan',
            steps: [{ title: 'Revise scope' }],
            writeScope: ['apps/desktop/src/windows/GoalConsole.tsx'],
            diffPreview: 'diff preview',
          })
        }
      >
        Trigger reject plan
      </button>
    </div>
  ),
}));

vi.mock('../ipc/invoke', () => ({
  api: {
    listWorkspaces: mocks.listWorkspacesMock,
    listChatSessions: mocks.listChatSessionsMock,
    createChatSession: mocks.createChatSessionMock,
    setChatSessionKind: mocks.setChatSessionKindMock,
    getWorkspaceStatus: mocks.getWorkspaceStatusMock,
    updateChatSessionWorkspace: mocks.updateChatSessionWorkspaceMock,
    attachWorkspace: mocks.attachWorkspaceMock,
  },
  listGoals: mocks.listGoalsMock,
  createGoal: mocks.createGoalMock,
  updateGoalStatus: mocks.updateGoalStatusMock,
  updateGoalObjective: mocks.updateGoalObjectiveMock,
  resumeGoal: mocks.resumeGoalMock,
  approveGoalPlan: mocks.approveGoalPlanMock,
  appendGoalUserMessage: mocks.appendGoalUserMessageMock,
}));

import { AgentWorkspacePanel } from './AgentWorkspacePanel';

describe('AgentWorkspacePanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.chatInput = '';
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
        title: 'Session One',
        workspace_id: 'ws-1',
        message_count: 1,
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:00Z',
        working: false,
      },
    ]);
    mocks.createChatSessionMock.mockResolvedValue({ id: 'session-new' });
    mocks.setChatSessionKindMock.mockResolvedValue(undefined);
    mocks.getWorkspaceStatusMock.mockResolvedValue({
      workspace_id: 'ws-1',
      root: 'I:/personal-agent',
      exists: true,
      git_branch: null,
      dirty: false,
    });
    mocks.sendMessageMock.mockResolvedValue(undefined);
    mocks.resumeGoalMock.mockResolvedValue(undefined);
    mocks.appendGoalUserMessageMock.mockResolvedValue(undefined);
    mocks.updateGoalObjectiveMock.mockResolvedValue(undefined);
  });

  it('sends approved plans back into chat execution with write scope and verification instructions', async () => {
    render(<AgentWorkspacePanel />);

    await waitFor(() => {
      expect(mocks.listChatSessionsMock).toHaveBeenCalled();
    });

    fireEvent.click(screen.getByRole('button', { name: 'Trigger approve plan' }));

    await waitFor(() => {
      expect(mocks.sendPromptMock).toHaveBeenCalledTimes(1);
    });

    const [content, options] = mocks.sendPromptMock.mock.calls[0];
    expect(content).toContain('Plan approved: Runtime plan');
    expect(content).toContain('Keep changes strictly within this write scope:');
    expect(content).toContain('crates/conductor-core/src/runtime_api.rs');
    expect(content).toContain('Run the relevant verification commands and report pass/fail with key output.');
    expect(options).toMatchObject({
      taskMode: 'short',
      capability: 'ask_write',
      planOnly: false,
      approvedWriteScope: ['crates/conductor-core/src/runtime_api.rs'],
    });
  });

  it('sends rejected plans back into plan-only revision mode', async () => {
    render(<AgentWorkspacePanel />);

    await waitFor(() => {
      expect(mocks.listChatSessionsMock).toHaveBeenCalled();
    });

    fireEvent.click(screen.getByRole('button', { name: 'Trigger reject plan' }));

    await waitFor(() => {
      expect(mocks.sendPromptMock).toHaveBeenCalledTimes(1);
    });

    const [content, options] = mocks.sendPromptMock.mock.calls[0];
    expect(content).toContain('Revise the plan: Runtime plan');
    expect(content).toContain('Keep plan-only mode.');
    expect(options).toMatchObject({
      taskMode: 'short',
      capability: 'ask_write',
      planOnly: true,
    });
  });

  it('auto-approves legacy goal sessions without showing a manual status-bar action', async () => {
    const awaitingPlanGoal = {
      id: 'goal-1',
      workspace_id: 'ws-1',
      title: 'Ship runtime loop',
      objective: 'Finish the goal runtime chain',
      status: 'awaiting_plan_approval',
      priority: 'normal',
      owner: 'user',
      created_at: '2026-05-31T00:00:00Z',
      updated_at: '2026-05-31T00:05:00Z',
    };
    mocks.listChatSessionsMock.mockResolvedValue([
      {
        id: 'session-1',
        title: 'Session One',
        workspace_id: 'ws-1',
        session_kind: 'goal',
        goal_id: 'goal-1',
        message_count: 1,
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:00Z',
        working: false,
      },
    ]);
    mocks.listGoalsMock.mockResolvedValue([awaitingPlanGoal]);
    mocks.approveGoalPlanMock.mockResolvedValue({
      ...awaitingPlanGoal,
      status: 'running',
      updated_at: '2026-05-31T00:06:00Z',
    });

    const { container } = render(<AgentWorkspacePanel />);

    await waitFor(() => {
      expect(mocks.approveGoalPlanMock).toHaveBeenCalledWith('goal-1');
    });

    expect(mocks.approveGoalPlanMock).toHaveBeenCalledTimes(1);
    expect(container.querySelector('.goal-status-bar-btn')).toBeNull();
  });

  it('resumes blocked goals from follow-up input instead of starting a foreground long task', async () => {
    mocks.chatInput = 'The approval is granted. Continue with the goal.';
    mocks.listChatSessionsMock.mockResolvedValue([
      {
        id: 'session-1',
        title: 'Session One',
        workspace_id: 'ws-1',
        session_kind: 'goal',
        goal_id: 'goal-1',
        message_count: 1,
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:00Z',
        working: false,
      },
    ]);
    mocks.listGoalsMock.mockResolvedValue([
      {
        id: 'goal-1',
        workspace_id: 'ws-1',
        title: 'Ship runtime loop',
        objective: 'Finish the goal runtime chain',
        status: 'blocked',
        priority: 'normal',
        owner: 'user',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:05:00Z',
      },
    ]);
    mocks.resumeGoalMock.mockResolvedValue({
      id: 'goal-1',
      workspace_id: 'ws-1',
      title: 'Ship runtime loop',
      objective: 'Finish the goal runtime chain',
      status: 'running',
      priority: 'normal',
      owner: 'user',
      created_at: '2026-05-31T00:00:00Z',
      updated_at: '2026-05-31T00:06:00Z',
    });

    render(<AgentWorkspacePanel />);

    await waitFor(() => {
      expect(mocks.listGoalsMock).toHaveBeenCalledWith('ws-1');
    });

    fireEvent.click(screen.getByRole('button', { name: 'Trigger send' }));

    await waitFor(() => {
      expect(mocks.appendGoalUserMessageMock).toHaveBeenCalledWith(
        'session-1',
        'The approval is granted. Continue with the goal.',
      );
    });

    expect(mocks.resumeGoalMock).toHaveBeenCalledWith('goal-1');
    expect(mocks.sendMessageMock).not.toHaveBeenCalled();
    expect(mocks.setInputMock).toHaveBeenCalledWith('');
  });

  it('treats explicit goal objective changes as goal updates instead of foreground chat turns', async () => {
    mocks.chatInput = 'goal: tighten the runtime review loop';
    mocks.listChatSessionsMock.mockResolvedValue([
      {
        id: 'session-1',
        title: 'Session One',
        workspace_id: 'ws-1',
        session_kind: 'goal',
        goal_id: 'goal-1',
        message_count: 1,
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:00Z',
        working: false,
      },
    ]);
    mocks.listGoalsMock.mockResolvedValue([
      {
        id: 'goal-1',
        workspace_id: 'ws-1',
        title: 'Ship runtime loop',
        objective: 'Finish the goal runtime chain',
        status: 'blocked',
        priority: 'normal',
        owner: 'user',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:05:00Z',
      },
    ]);
    mocks.updateGoalObjectiveMock.mockResolvedValue({
      id: 'goal-1',
      workspace_id: 'ws-1',
      title: 'tighten the runtime review loop',
      objective: 'User request:\ntighten the runtime review loop',
      status: 'blocked',
      priority: 'normal',
      owner: 'user',
      created_at: '2026-05-31T00:00:00Z',
      updated_at: '2026-05-31T00:05:30Z',
    });
    mocks.resumeGoalMock.mockResolvedValue({
      id: 'goal-1',
      workspace_id: 'ws-1',
      title: 'tighten the runtime review loop',
      objective: 'User request:\ntighten the runtime review loop',
      status: 'running',
      priority: 'normal',
      owner: 'user',
      created_at: '2026-05-31T00:00:00Z',
      updated_at: '2026-05-31T00:06:00Z',
    });

    render(<AgentWorkspacePanel />);

    await waitFor(() => {
      expect(mocks.listGoalsMock).toHaveBeenCalledWith('ws-1');
    });

    fireEvent.click(screen.getByRole('button', { name: 'Trigger send' }));

    await waitFor(() => {
      expect(mocks.updateGoalObjectiveMock).toHaveBeenCalledWith(
        'goal-1',
        'tighten the runtime review loop',
        'User request:\ntighten the runtime review loop',
      );
    });

    expect(mocks.appendGoalUserMessageMock).toHaveBeenCalledWith(
      'session-1',
      'goal: tighten the runtime review loop',
    );
    expect(mocks.resumeGoalMock).toHaveBeenCalledWith('goal-1');
    expect(mocks.sendMessageMock).not.toHaveBeenCalled();
    expect(mocks.setInputMock).toHaveBeenCalledWith('');
  });

  it('records running goal follow-up input in the goal timeline instead of spawning a foreground long task', async () => {
    mocks.chatInput = 'Please also verify the final review summary output.';
    mocks.listChatSessionsMock.mockResolvedValue([
      {
        id: 'session-1',
        title: 'Session One',
        workspace_id: 'ws-1',
        session_kind: 'goal',
        goal_id: 'goal-1',
        message_count: 1,
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:00Z',
        working: false,
      },
    ]);
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

    render(<AgentWorkspacePanel />);

    await waitFor(() => {
      expect(mocks.listGoalsMock).toHaveBeenCalledWith('ws-1');
    });

    fireEvent.click(screen.getByRole('button', { name: 'Trigger send' }));

    await waitFor(() => {
      expect(mocks.appendGoalUserMessageMock).toHaveBeenCalledWith(
        'session-1',
        'Please also verify the final review summary output.',
      );
    });

    expect(mocks.resumeGoalMock).not.toHaveBeenCalled();
    expect(mocks.updateGoalObjectiveMock).not.toHaveBeenCalled();
    expect(mocks.sendMessageMock).not.toHaveBeenCalled();
    expect(mocks.setInputMock).toHaveBeenCalledWith('');
  });

  it('moves rework-required goals back to planning when the user adds new guidance', async () => {
    mocks.chatInput = 'Please revise the plan around the runtime review queue.';
    mocks.listChatSessionsMock.mockResolvedValue([
      {
        id: 'session-1',
        title: 'Session One',
        workspace_id: 'ws-1',
        session_kind: 'goal',
        goal_id: 'goal-1',
        message_count: 1,
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:00Z',
        working: false,
      },
    ]);
    mocks.listGoalsMock.mockResolvedValue([
      {
        id: 'goal-1',
        workspace_id: 'ws-1',
        title: 'Ship runtime loop',
        objective: 'Finish the goal runtime chain',
        status: 'rework_required',
        priority: 'normal',
        owner: 'user',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:05:00Z',
      },
    ]);
    mocks.updateGoalStatusMock.mockResolvedValue({
      id: 'goal-1',
      workspace_id: 'ws-1',
      title: 'Ship runtime loop',
      objective: 'Finish the goal runtime chain',
      status: 'planning',
      priority: 'normal',
      owner: 'user',
      created_at: '2026-05-31T00:00:00Z',
      updated_at: '2026-05-31T00:06:00Z',
    });

    render(<AgentWorkspacePanel />);

    await waitFor(() => {
      expect(mocks.listGoalsMock).toHaveBeenCalledWith('ws-1');
    });

    fireEvent.click(screen.getByRole('button', { name: 'Trigger send' }));

    await waitFor(() => {
      expect(mocks.updateGoalStatusMock).toHaveBeenCalledWith('goal-1', 'planning');
    });

    expect(mocks.appendGoalUserMessageMock).toHaveBeenCalledWith(
      'session-1',
      'Please revise the plan around the runtime review queue.',
    );
    expect(mocks.sendMessageMock).not.toHaveBeenCalled();
    expect(mocks.setInputMock).toHaveBeenCalledWith('');
  });

  it('starts a new goal when the active goal session is already terminal', async () => {
    mocks.chatInput = 'Take the next pass on the runtime review UX.';
    mocks.listChatSessionsMock.mockResolvedValue([
      {
        id: 'session-1',
        title: 'Session One',
        workspace_id: 'ws-1',
        session_kind: 'goal',
        goal_id: 'goal-1',
        message_count: 1,
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:00:00Z',
        working: false,
      },
    ]);
    mocks.listGoalsMock.mockResolvedValue([
      {
        id: 'goal-1',
        workspace_id: 'ws-1',
        title: 'Ship runtime loop',
        objective: 'Finish the goal runtime chain',
        status: 'accepted',
        priority: 'normal',
        owner: 'user',
        created_at: '2026-05-31T00:00:00Z',
        updated_at: '2026-05-31T00:05:00Z',
      },
    ]);
    mocks.createGoalMock.mockResolvedValue({
      id: 'goal-2',
      workspace_id: 'ws-1',
      title: 'Take the next pass on the runtime review UX.',
      objective: 'User request:\nTake the next pass on the runtime review UX.',
      status: 'draft',
    });
    mocks.updateGoalStatusMock.mockResolvedValue({
      id: 'goal-2',
      workspace_id: 'ws-1',
      title: 'Take the next pass on the runtime review UX.',
      objective: 'User request:\nTake the next pass on the runtime review UX.',
      status: 'planning',
      priority: 'normal',
      owner: 'user',
      created_at: '2026-05-31T00:06:00Z',
      updated_at: '2026-05-31T00:06:00Z',
    });

    render(<AgentWorkspacePanel />);

    await waitFor(() => {
      expect(mocks.listGoalsMock).toHaveBeenCalledWith('ws-1');
    });

    fireEvent.click(screen.getByRole('button', { name: 'Trigger send' }));

    await waitFor(() => {
      expect(mocks.createGoalMock).toHaveBeenCalledWith(
        'ws-1',
        'Take the next pass on the runtime review UX.',
        'User request:\nTake the next pass on the runtime review UX.',
      );
    });

    expect(mocks.updateGoalStatusMock).toHaveBeenCalledWith('goal-2', 'planning');
    expect(mocks.setChatSessionKindMock).toHaveBeenCalledWith('session-1', 'goal', 'goal-2');
    expect(mocks.appendGoalUserMessageMock).toHaveBeenCalledWith(
      'session-1',
      'Take the next pass on the runtime review UX.',
    );
    expect(mocks.sendMessageMock).not.toHaveBeenCalled();
    expect(mocks.resumeGoalMock).not.toHaveBeenCalled();
    expect(mocks.updateGoalObjectiveMock).not.toHaveBeenCalled();
    expect(mocks.setInputMock).toHaveBeenCalledWith('');
  });
});
