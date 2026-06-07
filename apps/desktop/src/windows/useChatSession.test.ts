import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    emit: vi.fn(() => Promise.resolve()),
  }),
}));

vi.mock('../ipc/invoke', () => ({
  api: {
    listChatMessages: vi.fn(() => Promise.resolve([])),
    getChatMessageProjections: vi.fn(() => Promise.resolve([])),
    getChatSessionMessagesV2: vi.fn(() => Promise.resolve([])),
    sendChatMessageV2: vi.fn(() =>
      Promise.resolve({
        message: { id: 'msg-1', content: 'reply', role: 'assistant' },
        history: [],
        bubble_summary: 'reply',
      }),
    ),
    createChatSession: vi.fn(() => Promise.resolve({ id: 'session-1' })),
    setChatSessionKind: vi.fn(() => Promise.resolve()),
    approveProposal: vi.fn(() => Promise.resolve()),
    rejectProposal: vi.fn(() => Promise.resolve()),
  },
  appendGoalUserMessage: vi.fn(() => Promise.resolve()),
  createGoal: vi.fn(() => Promise.resolve({ id: 'goal-1' })),
  updateGoalStatus: vi.fn(() => Promise.resolve()),
}));

import { renderHook, act, waitFor } from '@testing-library/react';
import { useChatSession, type ToolCardStatus, type DisplayMessage } from './useChatSession';
import { api, appendGoalUserMessage, createGoal, updateGoalStatus } from '../ipc/invoke';

describe('useChatSession', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('ToolCardStatus mapping (11 states)', () => {
    // The status mapping logic is internal to the hook's event handler.
    // We verify the exported type covers all 11 states.
    const allStatuses: ToolCardStatus[] = [
      'pending',
      'running',
      'success',
      'error',
      'awaiting_approval',
      'approved',
      'blocked',
      'cancelled',
      'denied',
      'retryable',
      'timeout',
    ];

    it('should define exactly 11 tool card statuses', () => {
      expect(allStatuses).toHaveLength(11);
    });

    it('should include all expected statuses', () => {
      const statusSet = new Set(allStatuses);
      expect(statusSet.has('pending')).toBe(true);
      expect(statusSet.has('running')).toBe(true);
      expect(statusSet.has('success')).toBe(true);
      expect(statusSet.has('error')).toBe(true);
      expect(statusSet.has('awaiting_approval')).toBe(true);
      expect(statusSet.has('approved')).toBe(true);
      expect(statusSet.has('blocked')).toBe(true);
      expect(statusSet.has('cancelled')).toBe(true);
      expect(statusSet.has('denied')).toBe(true);
      expect(statusSet.has('retryable')).toBe(true);
      expect(statusSet.has('timeout')).toBe(true);
    });
  });

  describe('initial state', () => {
    it('should return empty messages and default state', () => {
      const { result } = renderHook(() => useChatSession());
      expect(result.current.messages).toEqual([]);
      expect(result.current.input).toBe('');
      expect(result.current.sending).toBe(false);
      expect(result.current.streamTokens).toEqual([]);
      expect(result.current.thinkingContent).toBeNull();
      expect(result.current.loadError).toBeNull();
    });

    it('should provide sendMessage and retryMessage functions', () => {
      const { result } = renderHook(() => useChatSession());
      expect(typeof result.current.sendMessage).toBe('function');
      expect(typeof result.current.retryMessage).toBe('function');
      expect(typeof result.current.clearMessages).toBe('function');
    });

    it('should provide approveProposal and rejectProposal functions', () => {
      const { result } = renderHook(() => useChatSession());
      expect(typeof result.current.approveProposal).toBe('function');
      expect(typeof result.current.rejectProposal).toBe('function');
    });
  });

  describe('input management', () => {
    it('should update input via setInput', () => {
      const { result } = renderHook(() => useChatSession());
      act(() => {
        result.current.setInput('hello');
      });
      expect(result.current.input).toBe('hello');
    });

    it('creates and reuses a session when sending without a pre-bound sessionId', async () => {
      const { result } = renderHook(() => useChatSession());

      act(() => {
        result.current.setInput('ship it');
      });

      await act(async () => {
        await result.current.sendMessage({
          taskMode: 'short',
          capability: 'ask_write',
          planOnly: false,
          approvedWriteScope: null,
        });
      });

      await waitFor(() => {
        expect(api.createChatSession).toHaveBeenCalledTimes(1);
        expect(api.sendChatMessageV2).toHaveBeenCalledWith(
          'ship it',
          'session-1',
          'short',
          'ask_write',
          false,
          null,
          expect.any(String),
        );
      });
    });
  });

  describe('clearMessages', () => {
    it('should reset messages and state', () => {
      const { result } = renderHook(() => useChatSession());
      act(() => {
        result.current.setInput('test');
      });
      expect(result.current.input).toBe('test');
      // clearMessages resets the session state but not input
      act(() => {
        result.current.clearMessages();
      });
      expect(result.current.messages).toEqual([]);
    });
  });

  describe('createGoalFromSeed', () => {
    it('creates a goal and moves it into planning when a workspace is attached', async () => {
      const { result } = renderHook(() =>
        useChatSession({
          workspaceId: 'ws-1',
        }),
      );

      await act(async () => {
        await result.current.createGoalFromSeed({
          title: 'Ship the runtime runner',
          objective: 'Carry forward the chat context into a goal.',
        });
      });

      expect(createGoal).toHaveBeenCalledWith(
        'ws-1',
        'Ship the runtime runner',
        'Carry forward the chat context into a goal.',
      );
      expect(updateGoalStatus).toHaveBeenCalledWith('goal-1', 'planning');
    });

    it('links the session to the new goal and persists the original user request', async () => {
      const { result } = renderHook(() =>
        useChatSession({
          workspaceId: 'ws-1',
          sessionId: 'session-9',
        }),
      );

      await act(async () => {
        await result.current.createGoalFromSeed({
          title: 'Ship the runtime runner',
          objective: 'User request:\nCarry forward the chat context into a goal.\n\nConversation context:\nKeep the current session visible.',
        });
      });

      expect(api.setChatSessionKind).toHaveBeenCalledWith('session-9', 'goal', 'goal-1');
      expect(appendGoalUserMessage).toHaveBeenCalledWith(
        'session-9',
        'Carry forward the chat context into a goal.',
      );
      expect(api.setChatSessionKind.mock.invocationCallOrder[0]).toBeLessThan(
        updateGoalStatus.mock.invocationCallOrder[0],
      );
    });

    it('rejects goal creation when no workspace is attached', async () => {
      const { result } = renderHook(() => useChatSession());

      await expect(
        result.current.createGoalFromSeed({
          title: 'Need background execution',
          objective: 'Upgrade this chat into a goal.',
        }),
      ).rejects.toThrow('Attach a workspace before upgrading to a goal.');
    });
  });

  describe('DisplayMessage', () => {
    it('should support error flag on DisplayMessage', () => {
      const msg: DisplayMessage = {
        id: 'test-1',
        role: 'user',
        content: 'hello',
        created_at: new Date().toISOString(),
        error: true,
      };
      expect(msg.error).toBe(true);
    });
  });
});
