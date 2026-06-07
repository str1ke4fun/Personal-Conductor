import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';

const mocks = vi.hoisted(() => {
  const listeners = new Map<string, (event: { payload: any }) => void>();
  return {
    listeners,
    getChatMessageProjectionsMock: vi.fn(),
    getChatSessionMessagesV2Mock: vi.fn(),
    listChatMessagesMock: vi.fn(),
    getCommandRunMock: vi.fn(),
    approveProposalMock: vi.fn(),
    rejectProposalMock: vi.fn(),
    createGoalMock: vi.fn(),
    updateGoalStatusMock: vi.fn(),
    createChatSessionMock: vi.fn(),
    sendChatMessageV2Mock: vi.fn(),
  };
});

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((eventName: string, callback: (event: { payload: any }) => void) => {
    mocks.listeners.set(eventName, callback);
    return Promise.resolve(() => {
      mocks.listeners.delete(eventName);
    });
  }),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    emit: vi.fn(() => Promise.resolve()),
  }),
}));

vi.mock('../ipc/invoke', () => ({
  api: {
    getChatMessageProjections: mocks.getChatMessageProjectionsMock,
    getChatSessionMessagesV2: mocks.getChatSessionMessagesV2Mock,
    listChatMessages: mocks.listChatMessagesMock,
    getCommandRun: mocks.getCommandRunMock,
    approveProposal: mocks.approveProposalMock,
    rejectProposal: mocks.rejectProposalMock,
    createChatSession: mocks.createChatSessionMock,
    sendChatMessageV2: mocks.sendChatMessageV2Mock,
    setChatSessionKind: vi.fn(() => Promise.resolve()),
  },
  appendGoalUserMessage: vi.fn(() => Promise.resolve()),
  createGoal: mocks.createGoalMock,
  updateGoalStatus: mocks.updateGoalStatusMock,
  parseContentBlocks: (message: { content: string }) => {
    try {
      const parsed = JSON.parse(message.content);
      return Array.isArray(parsed) ? parsed : [{ type: 'text', text: message.content }];
    } catch {
      return [{ type: 'text', text: message.content }];
    }
  },
}));

import { useChatSession } from './useChatSession';

function Harness() {
  const session = useChatSession({ sessionId: 'session-1' });
  const projected = session.projectedRuns[0];
  return (
    <div>
      <div data-testid="message-count">{session.messages.length}</div>
      <div data-testid="projected-count">{session.projectedRuns.length}</div>
      <div data-testid="projected-thinking">{projected?.thinkingContent ?? ''}</div>
      <div data-testid="projected-phase">{projected?.currentPhase ?? ''}</div>
      <div data-testid="projected-text">{projected?.streamTokens.join('') ?? ''}</div>
      <div data-testid="projected-tools">{projected ? projected.toolStates.size : 0}</div>
    </div>
  );
}

function PrimaryRunHarness() {
  const session = useChatSession({ sessionId: 'session-1' });
  return (
    <div>
      <input
        aria-label="chat-input"
        value={session.input}
        onChange={(event) => session.setInput(event.target.value)}
      />
      <button type="button" onClick={() => void session.sendMessage({ taskMode: 'short', capability: 'ask_write', planOnly: false })}>
        Send
      </button>
      <div data-testid="primary-sending">{String(session.sending)}</div>
      <div data-testid="primary-thinking">{session.thinkingContent ?? ''}</div>
      <div data-testid="primary-text">{session.streamTokens.join('')}</div>
      <div data-testid="primary-tools">{session.toolStates.size}</div>
      <div data-testid="primary-projected-count">{session.projectedRuns.length}</div>
      <div data-testid="primary-message-count">{session.messages.length}</div>
    </div>
  );
}

describe('useChatSession projected runs', () => {
  beforeEach(() => {
    mocks.listeners.clear();
    mocks.getChatMessageProjectionsMock.mockReset();
    mocks.getChatSessionMessagesV2Mock.mockReset();
    mocks.listChatMessagesMock.mockReset();
    mocks.getCommandRunMock.mockReset();
    mocks.approveProposalMock.mockReset();
    mocks.rejectProposalMock.mockReset();
    mocks.createGoalMock.mockReset();
    mocks.updateGoalStatusMock.mockReset();
    mocks.createChatSessionMock.mockReset();
    mocks.sendChatMessageV2Mock.mockReset();
    // projections empty → fallback to V2 mock (which tests inject data into)
    mocks.getChatMessageProjectionsMock.mockResolvedValue([]);
    mocks.getChatSessionMessagesV2Mock.mockResolvedValue([]);
  });

  it('projects background thinking, tools, and persisted replies into the active session', async () => {
    render(<Harness />);

    await waitFor(() => {
      expect(mocks.getChatSessionMessagesV2Mock).toHaveBeenCalledWith('session-1');
    });

    await act(async () => {
      mocks.listeners.get('thinking-update')?.({
        payload: {
          session_id: 'session-1',
          request_id: 'bg-1',
          phase: 'planning',
          message: 'Planning background task',
          turn: 0,
          timestamp: '2026-06-01T10:00:00Z',
        },
      });
      mocks.listeners.get('tool-execution-update')?.({
        payload: {
          session_id: 'session-1',
          request_id: 'bg-1',
          tool_use_id: 'tool-1',
          tool_name: 'bash__execute',
          status: 'started',
          input: { command: 'echo hi' },
        },
      });
      mocks.listeners.get('stream-chat-token')?.({
        payload: {
          session_id: 'session-1',
          request_id: 'bg-1',
          token: 'partial output',
        },
      });
      mocks.listeners.get('thinking-update')?.({
        payload: {
          session_id: 'session-1',
          request_id: 'bg-2',
          phase: 'planning',
          message: 'Second background task still running',
          turn: 0,
          timestamp: '2026-06-01T10:01:00Z',
        },
      });
    });

    expect(screen.getByTestId('projected-count').textContent).toBe('2');
    expect(screen.getByTestId('projected-thinking').textContent).toBe('Planning background task');
    expect(screen.getByTestId('projected-text').textContent).toBe('partial output');
    expect(screen.getByTestId('projected-tools').textContent).toBe('1');

    mocks.getChatSessionMessagesV2Mock.mockResolvedValueOnce([
      {
        id: 'assistant-1',
        role: 'assistant',
        content: JSON.stringify([{ type: 'text', text: 'Projected final answer' }]),
        created_at: '2026-06-01T10:02:00Z',
        seq: 2,
      },
    ]);

    await act(async () => {
      mocks.listeners.get('thinking-update')?.({
        payload: {
          session_id: 'session-1',
          request_id: 'bg-1',
          phase: 'done',
          message: 'Done',
          turn: 1,
          timestamp: '2026-06-01T10:02:00Z',
        },
      });
    });

    expect(screen.getByTestId('projected-count').textContent).toBe('2');
    expect(screen.getByTestId('projected-phase').textContent).toBe('synthesizing');
    expect(screen.getByTestId('projected-thinking').textContent).toBe('正在写回可审阅结果...');

    mocks.getChatSessionMessagesV2Mock.mockResolvedValueOnce([
      {
        id: 'assistant-early',
        role: 'assistant',
        content: JSON.stringify([{ type: 'text', text: 'Early goal refresh should not win the race' }]),
        created_at: '2026-06-01T10:01:59Z',
        seq: 1,
      },
    ]);

    await act(async () => {
      mocks.listeners.get('goals_changed')?.({ payload: {} });
    });

    expect(screen.getByTestId('message-count').textContent).toBe('0');
    expect(screen.getByTestId('projected-count').textContent).toBe('2');

    await act(async () => {
      mocks.listeners.get('reply_stored')?.({
        payload: {
          message_id: 'assistant-1',
          session_id: 'session-1',
          request_id: 'bg-1',
        },
      });
    });

    await waitFor(() => {
      expect(screen.getByTestId('message-count').textContent).toBe('1');
      expect(screen.getByTestId('projected-count').textContent).toBe('1');
      expect(screen.getByTestId('projected-thinking').textContent).toBe('Second background task still running');
    });
  });

  it('binds foreground streaming events to the active run and closes it on reply_stored', async () => {
    let resolveReply: ((value: any) => void) | null = null;
    mocks.sendChatMessageV2Mock.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveReply = resolve;
        }),
    );

    render(<PrimaryRunHarness />);

    await waitFor(() => {
      expect(mocks.getChatSessionMessagesV2Mock).toHaveBeenCalledWith('session-1');
    });

    fireEvent.change(screen.getByLabelText('chat-input'), { target: { value: 'inspect the repo' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => {
      expect(mocks.sendChatMessageV2Mock).toHaveBeenCalledTimes(1);
    });

    const requestId = mocks.sendChatMessageV2Mock.mock.calls[0][6];
    expect(typeof requestId).toBe('string');
    expect(requestId).toContain('req-');

    await act(async () => {
      mocks.listeners.get('thinking-update')?.({
        payload: {
          session_id: 'session-1',
          request_id: requestId,
          phase: 'analyzing',
          message: 'Inspecting the repository',
          turn: 0,
          timestamp: '2026-06-02T10:00:00Z',
        },
      });
      mocks.listeners.get('tool-execution-update')?.({
        payload: {
          session_id: 'session-1',
          request_id: requestId,
          tool_use_id: 'tool-foreground-1',
          tool_name: 'bash__execute',
          status: 'started',
          input: { command: 'git status --short' },
        },
      });
      mocks.listeners.get('stream-chat-token')?.({
        payload: {
          session_id: 'session-1',
          request_id: requestId,
          token: 'Working output',
        },
      });
    });

    expect(screen.getByTestId('primary-thinking').textContent).toBe('Inspecting the repository');
    expect(screen.getByTestId('primary-text').textContent).toBe('Working output');
    expect(screen.getByTestId('primary-tools').textContent).toBe('1');
    expect(screen.getByTestId('primary-projected-count').textContent).toBe('0');

    mocks.getChatSessionMessagesV2Mock.mockResolvedValueOnce([
      {
        id: 'user-1',
        role: 'user',
        content: 'inspect the repo',
        created_at: '2026-06-02T10:00:00Z',
        seq: 1,
      },
      {
        id: 'assistant-foreground-1',
        role: 'assistant',
        content: JSON.stringify([{ type: 'text', text: 'Final conclusion' }]),
        created_at: '2026-06-02T10:00:02Z',
        seq: 2,
      },
    ]);

    await act(async () => {
      mocks.listeners.get('reply_stored')?.({
        payload: {
          message_id: 'assistant-foreground-1',
          session_id: 'session-1',
          request_id: requestId,
        },
      });
    });

    await waitFor(() => {
      expect(screen.getByTestId('primary-sending').textContent).toBe('false');
      expect(screen.getByTestId('primary-message-count').textContent).toBe('2');
      expect(screen.getByTestId('primary-projected-count').textContent).toBe('0');
      expect(screen.getByTestId('primary-thinking').textContent).toBe('');
    });

    await act(async () => {
      resolveReply?.({
        message: {
          id: 'assistant-foreground-1',
          role: 'assistant',
          content: JSON.stringify([{ type: 'text', text: 'Final conclusion' }]),
          created_at: '2026-06-02T10:00:02Z',
          seq: 2,
        },
        history: [
          {
            id: 'user-1',
            role: 'user',
            content: 'inspect the repo',
            created_at: '2026-06-02T10:00:00Z',
            seq: 1,
          },
          {
            id: 'assistant-foreground-1',
            role: 'assistant',
            content: JSON.stringify([{ type: 'text', text: 'Final conclusion' }]),
            created_at: '2026-06-02T10:00:02Z',
            seq: 2,
          },
        ],
        bubble_summary: 'Final conclusion',
      });
    });
  });
});
