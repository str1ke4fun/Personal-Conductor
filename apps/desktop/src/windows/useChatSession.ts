import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  api,
  appendGoalUserMessage,
  ChatMessage,
  CommandRun,
  GoalSeed,
  parseContentBlocks,
  createGoal,
  updateGoalStatus,
  type StreamChatTokenEvent,
  type ThinkingUpdate,
  type ToolExecutionUpdate,
} from '../ipc/invoke';
import type { ChatSendOptions } from './ChatComposer';
import { normalizeToolId } from './toolIds';

export interface DisplayMessage extends ChatMessage {
  error?: boolean;
}

/** 11 tool card states aligned with backend ToolCall lifecycle. */
export type ToolCardStatus =
  | 'pending'            // created, not yet started
  | 'running'            // executing
  | 'success'            // completed successfully
  | 'error'              // failed with error
  | 'awaiting_approval'  // needs user approval (was approval_required)
  | 'approved'           // user approved, about to execute
  | 'blocked'            // blocked by dependency or precondition
  | 'cancelled'          // cancelled by user or system
  | 'denied'             // user denied the approval
  | 'retryable'          // failed but can be retried
  | 'timeout';           // execution timed out

export interface StreamToolState {
  tool_use_id: string;
  tool_name: string;
  status: ToolCardStatus;
  input?: Record<string, any>;
  result?: any;
  duration_ms?: number;
  proposal_id?: string;  // linked proposal for approval_required flows
  command_run_id?: string;
}

export interface ProjectedRunState {
  requestId: string;
  streamTokens: string[];
  toolStates: Map<string, StreamToolState>;
  thinkingContent: string | null;
  turnStartedAt: number;
  currentPhase: string | null;
  toolRunCount: number;
  finishedAt: number | null;
}

export interface UseChatSessionReturn {
  messages: DisplayMessage[];
  input: string;
  setInput: (v: string) => void;
  sending: boolean;
  streamTokens: string[];
  toolStates: Map<string, StreamToolState>;
  thinkingContent: string | null;
  endRef: React.RefObject<HTMLDivElement | null>;
  sendMessage: (options: ChatSendOptions) => Promise<void>;
  retryMessage: (msg: DisplayMessage) => void;
  clearMessages: () => void;
  /** Cancel the in-flight LLM request and reset sending state. */
  cancelSending: () => void;
  loadError: string | null;
  petMessageSource?: string;
  /** Approve a pending proposal (from approval_required tool state). */
  approveProposal: (proposalId: string) => Promise<void>;
  /** Reject/deny a pending proposal. */
  rejectProposal: (proposalId: string) => Promise<void>;
  /** Turn-level runtime state */
  turnStartedAt: number | null;
  currentPhase: string | null;
  toolRunCount: number;
  activeToolCount: number;
  projectedRuns: ProjectedRunState[];
  createGoalFromSeed: (goalSeed: GoalSeed) => Promise<void>;
  sendPrompt: (content: string, options: ChatSendOptions) => Promise<void>;
}

interface SessionUiState {
  messages: DisplayMessage[];
  sending: boolean;
  streamTokens: string[];
  toolStates: Map<string, StreamToolState>;
  thinkingContent: string | null;
  loadError: string | null;
  activeRequestId: string | null;
  activeStreamMessageId: string | null;
  turnStartedAt: number | null;
  currentPhase: string | null;
  toolRunCount: number;
  projectedRuns: Record<string, ProjectedRunState>;
}

interface ReplyStoredEvent {
  message_id?: string | null;
  session_id?: string | null;
  request_id?: string | null;
}

const SHORT_CHAT_SEND_CLIENT_TIMEOUT_MS = 150_000;
const LONG_CHAT_SEND_CLIENT_TIMEOUT_MS = 30 * 60 * 1000;
const GLOBAL_SESSION_KEY = '__global__';
const OPTIMISTIC_DEDUP_PREFIX_LEN = 40;
const PROJECTED_RUN_SYNTHESIZING_MESSAGE = '正在写回可审阅结果...';

function sendClientTimeoutMs(taskMode: ChatSendOptions['taskMode']): number {
  return taskMode === 'long' ? LONG_CHAT_SEND_CLIENT_TIMEOUT_MS : SHORT_CHAT_SEND_CLIENT_TIMEOUT_MS;
}

function mergeHistory(
  current: DisplayMessage[],
  backend: ChatMessage[],
): DisplayMessage[] {
  const backendMap = new Map<string, ChatMessage>();
  for (const msg of backend) {
    backendMap.set(msg.id, msg);
  }

  const backendUserPrefixes = new Set<string>();
  for (const msg of backend) {
    if (msg.role === 'user') {
      backendUserPrefixes.add(msg.content.slice(0, OPTIMISTIC_DEDUP_PREFIX_LEN));
    }
  }

  const merged: DisplayMessage[] = [];
  const seenBackendIds = new Set<string>();

  for (const cur of current) {
    const backendMatch = backendMap.get(cur.id);
    if (backendMatch) {
      seenBackendIds.add(cur.id);
      merged.push({
        ...backendMatch,
        error: cur.error && !backendMatch.content ? true : undefined,
      });
      continue;
    }

    if (cur.id.startsWith('temp-') && cur.role === 'user') {
      const prefix = cur.content.slice(0, OPTIMISTIC_DEDUP_PREFIX_LEN);
      if (backendUserPrefixes.has(prefix)) {
        continue;
      }
      merged.push(cur);
      continue;
    }

    if (cur.error) {
      merged.push(cur);
      continue;
    }
  }

  for (const msg of backend) {
    if (!seenBackendIds.has(msg.id)) {
      merged.push(msg as DisplayMessage);
    }
  }

  return sortMessages(merged);
}

function sortMessages(messages: DisplayMessage[]): DisplayMessage[] {
  return [...messages].sort((a, b) => {
    if (typeof a.seq === 'number' && typeof b.seq === 'number' && a.seq !== b.seq) {
      return a.seq - b.seq;
    }
    const timeDelta = Date.parse(a.created_at) - Date.parse(b.created_at);
    if (timeDelta !== 0) return timeDelta;
    return a.id.localeCompare(b.id);
  });
}

function createEmptySessionState(): SessionUiState {
  return {
    messages: [],
    sending: false,
    streamTokens: [],
    toolStates: new Map(),
    thinkingContent: null,
    loadError: null,
    activeRequestId: null,
    activeStreamMessageId: null,
    turnStartedAt: null,
    currentPhase: null,
    toolRunCount: 0,
    projectedRuns: {},
  };
}

function createProjectedRunState(requestId: string): ProjectedRunState {
  return {
    requestId,
    streamTokens: [],
    toolStates: new Map(),
    thinkingContent: null,
    turnStartedAt: Date.now(),
    currentPhase: null,
    toolRunCount: 0,
    finishedAt: null,
  };
}

function goalSeedUserMessage(goalSeed: GoalSeed): string {
  const objective = goalSeed.objective.trim();
  if (!objective) {
    return goalSeed.title.trim();
  }

  const match = objective.match(
    /^User request:\s*\n?([\s\S]*?)(?:\n{2,}Conversation context:|$)/i,
  );
  const extracted = match?.[1]?.trim();
  return extracted || objective;
}

function markProjectedRunAwaitingReply(
  projectedRuns: Record<string, ProjectedRunState>,
  requestId?: string | null,
): Record<string, ProjectedRunState> {
  if (!requestId) {
    return projectedRuns;
  }
  const existing = projectedRuns[requestId];
  if (!existing) {
    return projectedRuns;
  }
  return {
    ...projectedRuns,
    [requestId]: {
      ...existing,
      thinkingContent: PROJECTED_RUN_SYNTHESIZING_MESSAGE,
      currentPhase: 'synthesizing',
      finishedAt: null,
    },
  };
}

function removeProjectedRun(
  projectedRuns: Record<string, ProjectedRunState>,
  requestId?: string | null,
): Record<string, ProjectedRunState> {
  if (!requestId || !projectedRuns[requestId]) {
    return projectedRuns;
  }
  const { [requestId]: _removed, ...rest } = projectedRuns;
  return rest;
}

function cleanupFinishedProjectedRuns(
  projectedRuns: Record<string, ProjectedRunState>,
  maxAgeMs = 5_000,
): Record<string, ProjectedRunState> {
  const now = Date.now();
  return Object.fromEntries(
    Object.entries(projectedRuns).filter(([, run]) => {
      return run.finishedAt == null || now - run.finishedAt < maxAgeMs;
    }),
  );
}

function seedToolStatesFromMessages(messages: ChatMessage[]): Map<string, StreamToolState> {
  const seeded = new Map<string, StreamToolState>();
  for (const message of messages) {
    if (message.role !== 'assistant') continue;
    const blocks = parseContentBlocks(message);
    const resultMap = new Map<string, { content: string; is_error: boolean }>();
    for (const block of blocks) {
      if (block.type === 'tool_result') {
        resultMap.set(block.tool_use_id, {
          content: block.content,
          is_error: block.is_error,
        });
      }
    }
    for (const block of blocks) {
      if (block.type !== 'tool_use') continue;
      const result = resultMap.get(block.id);
      let parsedResult: any = result?.content;
      try {
        parsedResult = result ? JSON.parse(result.content) : undefined;
      } catch {
        // keep string
      }
      const commandRunId = isCommandStartPayload(parsedResult) ? parsedResult.command_run_id : undefined;
      seeded.set(block.id, {
        tool_use_id: block.id,
        tool_name: block.name,
        status: commandRunId ? 'running' : (result ? (result.is_error ? 'error' : 'success') : 'pending'),
        input: block.input,
        result: parsedResult,
        command_run_id: commandRunId,
      });
    }
  }
  return seeded;
}

function isCommandStartPayload(output: unknown): output is { command_run_id: string; status?: string } {
  return !!output
    && typeof output === 'object'
    && 'command_run_id' in output
    && typeof (output as { command_run_id?: unknown }).command_run_id === 'string';
}

function mapCommandRunToCardStatus(run: CommandRun): ToolCardStatus {
  switch (run.status) {
    case 'awaiting_permission':
      return 'awaiting_approval';
    case 'prepared':
    case 'starting':
    case 'streaming':
      return 'running';
    case 'exited':
      return run.exit_code === 0 ? 'success' : 'error';
    case 'timed_out':
      return 'timeout';
    case 'killed':
      return 'cancelled';
    default:
      return 'pending';
  }
}

export function useChatSession(opts?: {
  petMessageSource?: string;
  acceptPetMessages?: boolean;
  sessionId?: string | null;
  workspaceId?: string | null;
  onSessionCreated?: (sessionId: string) => void;
}): UseChatSessionReturn {
  const [sessionStates, setSessionStates] = useState<Record<string, SessionUiState>>({});
  const [input, setInput] = useState('');
  const [ownedSessionId, setOwnedSessionId] = useState<string | null>(opts?.sessionId ?? null);
  const endRef = useRef<HTMLDivElement>(null);
  const sessionIdRef = useRef<string | null | undefined>(opts?.sessionId);

  useEffect(() => {
    if (opts?.sessionId) {
      setOwnedSessionId(opts.sessionId);
      sessionIdRef.current = opts.sessionId;
      return;
    }
    sessionIdRef.current = ownedSessionId;
  }, [opts?.sessionId, ownedSessionId]);

  const activeSessionId = opts?.sessionId ?? ownedSessionId ?? null;
  const sessionKey = activeSessionId ?? GLOBAL_SESSION_KEY;
  const currentState = sessionStates[sessionKey] ?? createEmptySessionState();

  const updateSessionState = (key: string, updater: (state: SessionUiState) => SessionUiState) => {
    setSessionStates((prev) => {
      const current = prev[key] ?? createEmptySessionState();
      return {
        ...prev,
        [key]: updater(current),
      };
    });
  };

  useEffect(() => {
    let cancelled = false;
    const key = sessionKey;
    const loadMessages = async () => {
      updateSessionState(key, (state) => ({ ...state, loadError: null }));
      try {
        const items = activeSessionId
          ? await api.getChatSessionMessages(activeSessionId)
          : await api.listChatMessages();
        if (cancelled) return;
        updateSessionState(key, (state) => ({
          ...state,
          messages: sortMessages(items),
          toolStates: seedToolStatesFromMessages(items),
          projectedRuns: cleanupFinishedProjectedRuns(state.projectedRuns),
          loadError: null,
        }));
      } catch (err) {
        if (cancelled) return;
        updateSessionState(key, (state) => ({
          ...state,
          loadError: err instanceof Error ? err.message : '加载消息失败',
        }));
      }
    };
    void loadMessages();
    return () => {
      cancelled = true;
    };
  }, [activeSessionId, sessionKey]);

  // Re-fetch messages when a goal task completes and injects a result back
  // into this session. Only runs when not actively sending (no race condition).
  useEffect(() => {
    if (!activeSessionId) return;
    const sid = activeSessionId;
    const projectedRunCount = Object.keys(currentState.projectedRuns).length;
    const unlisten = listen('goals_changed', async () => {
      if (currentState.sending || projectedRunCount > 0) return;
      try {
        const items = await api.getChatSessionMessages(sid);
        updateSessionState(sid, (state) => ({
          ...state,
          messages: mergeHistory(state.messages, items),
          projectedRuns: cleanupFinishedProjectedRuns(state.projectedRuns, 0),
        }));
      } catch {
        // ignore — best effort refresh
      }
    });
    return () => { void unlisten.then((d) => d()).catch(() => {}); };
  }, [activeSessionId, currentState.projectedRuns, currentState.sending]);

  useEffect(() => {
    const sid = activeSessionId;
    const key = sid ?? GLOBAL_SESSION_KEY;
    const unlisten = listen<ReplyStoredEvent>('reply_stored', async (event) => {
      if ((event.payload.session_id ?? null) !== sid) return;
      try {
        const items = sid
          ? await api.getChatSessionMessages(sid)
          : await api.listChatMessages();
        updateSessionState(key, (state) => ({
          ...(state.activeRequestId === event.payload.request_id
            ? {
                ...state,
                sending: false,
                streamTokens: [],
                toolStates: new Map(),
                thinkingContent: null,
                activeRequestId: null,
                activeStreamMessageId: null,
                turnStartedAt: null,
                currentPhase: null,
                toolRunCount: 0,
              }
            : state),
          messages: mergeHistory(state.messages, items),
          projectedRuns:
            state.activeRequestId === event.payload.request_id
              ? cleanupFinishedProjectedRuns(state.projectedRuns, 0)
              : event.payload.request_id
                ? removeProjectedRun(state.projectedRuns, event.payload.request_id)
                : cleanupFinishedProjectedRuns(state.projectedRuns, 0),
        }));
      } catch {
        // ignore
      }
    });
    return () => { void unlisten.then((d) => d()).catch(() => {}); };
  }, [activeSessionId]);

  useEffect(() => {
    const unlisten = listen<StreamChatTokenEvent>('stream-chat-token', (event) => {
      const payload = event.payload;
      const key = payload.session_id ?? GLOBAL_SESSION_KEY;
      updateSessionState(key, (state) => {
        if (state.activeRequestId === payload.request_id) {
          return {
            ...state,
            streamTokens: [...state.streamTokens, payload.token],
          };
        }
        const existingRun = state.projectedRuns[payload.request_id] ?? createProjectedRunState(payload.request_id);
        return {
          ...state,
          projectedRuns: {
            ...state.projectedRuns,
            [payload.request_id]: {
              ...existingRun,
              streamTokens: [...existingRun.streamTokens, payload.token],
              finishedAt: null,
            },
          },
        };
      });
    });
    return () => { void unlisten.then((d) => d()).catch(() => {}); };
  }, []);

  useEffect(() => {
    const unlisten = listen<ToolExecutionUpdate>('tool-execution-update', (event) => {
      const update = event.payload;
      const key = update.session_id ?? GLOBAL_SESSION_KEY;
      updateSessionState(key, (state) => {
        const isPrimaryRun = state.activeRequestId === update.request_id;
        const projectedRun = state.projectedRuns[update.request_id] ?? createProjectedRunState(update.request_id);
        const next = new Map(isPrimaryRun ? state.toolStates : projectedRun.toolStates);
        const existing = next.get(update.tool_use_id);

        // Map backend status to frontend ToolCardStatus (11 states)
        let cardStatus: ToolCardStatus;
        switch (update.status) {
          case 'completed':
            cardStatus = 'success';
            break;
          case 'error':
            cardStatus = 'error';
            break;
          case 'started':
            cardStatus = 'running';
            break;
          case 'approval_required':
            cardStatus = 'awaiting_approval';
            break;
          case 'approved':
            cardStatus = 'approved';
            break;
          case 'blocked':
            cardStatus = 'blocked';
            break;
          case 'cancelled':
            cardStatus = 'cancelled';
            break;
          case 'denied':
            cardStatus = 'denied';
            break;
          case 'retryable':
            cardStatus = 'retryable';
            break;
          case 'timeout':
            cardStatus = 'timeout';
            break;
          default:
            cardStatus = 'pending';
        }

        const commandRunId = isCommandStartPayload(update.output)
          ? update.output.command_run_id
          : existing?.command_run_id;
        const normalizedToolName = normalizeToolId(update.tool_name);
        const isBackgroundCommand =
          normalizedToolName === 'bash.execute'
          && update.status === 'completed'
          && isCommandStartPayload(update.output)
          && (update.output.status ?? 'started') === 'started';
        if (isBackgroundCommand) {
          cardStatus = 'running';
        }

        // Extract proposal_id from output if this is an approval_required event
        const proposalId =
          update.status === 'approval_required' && update.output
            ? (update.output as any).proposal_id ?? existing?.proposal_id
            : existing?.proposal_id;

        next.set(update.tool_use_id, {
          tool_use_id: update.tool_use_id,
          tool_name: update.tool_name,
          status: cardStatus,
          input: update.input ?? existing?.input,
          result: update.output ?? existing?.result,
          duration_ms: update.duration_ms ?? existing?.duration_ms,
          proposal_id: proposalId,
          command_run_id: commandRunId,
        });
        if (isPrimaryRun) {
          const newToolRunCount = !existing ? state.toolRunCount + 1 : state.toolRunCount;
          return { ...state, toolStates: next, toolRunCount: newToolRunCount };
        }
        const newToolRunCount = !existing ? projectedRun.toolRunCount + 1 : projectedRun.toolRunCount;
        return {
          ...state,
          projectedRuns: {
            ...state.projectedRuns,
            [update.request_id]: {
              ...projectedRun,
              toolStates: next,
              toolRunCount: newToolRunCount,
              finishedAt: null,
            },
          },
        };
      });
    });
    return () => { void unlisten.then((d) => d()).catch(() => {}); };
  }, []);

  useEffect(() => {
    const unlisten = listen<ThinkingUpdate>('thinking-update', (event) => {
      const update = event.payload;
      const key = update.session_id ?? GLOBAL_SESSION_KEY;
      updateSessionState(key, (state) => {
        if (state.activeRequestId === update.request_id) {
          return {
            ...state,
            thinkingContent: update.phase === 'done' ? null : update.message,
            currentPhase: update.phase === 'done' ? null : update.phase,
          };
        }
        const projectedRun = state.projectedRuns[update.request_id] ?? createProjectedRunState(update.request_id);
        const awaitingReplyRun =
          markProjectedRunAwaitingReply(state.projectedRuns, update.request_id)[update.request_id]
          ?? projectedRun;
        return {
          ...state,
          projectedRuns: {
            ...state.projectedRuns,
            [update.request_id]: {
              ...(update.phase === 'done' ? awaitingReplyRun : projectedRun),
              thinkingContent: update.phase === 'done'
                ? PROJECTED_RUN_SYNTHESIZING_MESSAGE
                : update.message,
              currentPhase: update.phase === 'done' ? 'synthesizing' : update.phase,
              finishedAt: null,
            },
          },
        };
      });
    });
    return () => { void unlisten.then((d) => d()).catch(() => {}); };
  }, []);

  useEffect(() => {
    const directCommandTools = [...currentState.toolStates.values()];
    const projectedCommandTools = Object.values(currentState.projectedRuns).flatMap((run) =>
      [...run.toolStates.values()],
    );
    const commandTools = [...directCommandTools, ...projectedCommandTools].filter(
      (tool) => !!tool.command_run_id && (tool.status === 'running' || tool.status === 'awaiting_approval'),
    );
    if (commandTools.length === 0) {
      return;
    }

    let cancelled = false;
    const syncCommandRuns = async () => {
      try {
        const commandRunIds = [...new Set(commandTools.map((tool) => tool.command_run_id!))];
        const runs = await Promise.all(
          commandRunIds.map((commandRunId) => api.getCommandRun(commandRunId)),
        );
        if (cancelled) return;
        updateSessionState(sessionKey, (state) => {
          const next = new Map(state.toolStates);
          const nextProjectedRuns = { ...state.projectedRuns };
          for (const run of runs) {
            for (const [toolUseId, toolState] of next.entries()) {
              if (toolState.command_run_id !== run.id) continue;
              next.set(toolUseId, {
                ...toolState,
                status: mapCommandRunToCardStatus(run),
                result: {
                  ...(toolState.result ?? {}),
                  command_run_id: run.id,
                  status: run.status,
                  stdout: run.stdout_tail,
                  stderr: run.stderr_tail,
                  exit_code: run.exit_code,
                },
              });
            }
            for (const [requestId, projectedRun] of Object.entries(nextProjectedRuns)) {
              const nextToolStates = new Map(projectedRun.toolStates);
              let changed = false;
              for (const [toolUseId, toolState] of nextToolStates.entries()) {
                if (toolState.command_run_id !== run.id) continue;
                nextToolStates.set(toolUseId, {
                  ...toolState,
                  status: mapCommandRunToCardStatus(run),
                  result: {
                    ...(toolState.result ?? {}),
                    command_run_id: run.id,
                    status: run.status,
                    stdout: run.stdout_tail,
                    stderr: run.stderr_tail,
                    exit_code: run.exit_code,
                  },
                });
                changed = true;
              }
              if (changed) {
                nextProjectedRuns[requestId] = {
                  ...projectedRun,
                  toolStates: nextToolStates,
                };
              }
            }
          }
          return { ...state, toolStates: next, projectedRuns: nextProjectedRuns };
        });
      } catch (err) {
        console.error('Failed to sync command runs:', err);
      }
    };

    void syncCommandRuns();
    const intervalId = window.setInterval(() => {
      void syncCommandRuns();
    }, 1200);

    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, [currentState.projectedRuns, currentState.toolStates, sessionKey]);

  useEffect(() => {
    if (Object.keys(currentState.projectedRuns).length === 0) {
      return;
    }
    const intervalId = window.setInterval(() => {
      updateSessionState(sessionKey, (state) => ({
        ...state,
        projectedRuns: cleanupFinishedProjectedRuns(state.projectedRuns),
      }));
    }, 1000);
    return () => {
      window.clearInterval(intervalId);
    };
  }, [currentState.projectedRuns, sessionKey]);

  useEffect(() => {
    if (opts?.petMessageSource === undefined || !opts?.acceptPetMessages) return;
    const unlisten = listen<string | { id?: string; content: string; source?: string }>('pet_message', (event) => {
      const payload = event.payload;
      if (typeof payload === 'object' && payload.source === (opts.petMessageSource ?? 'chat_panel')) return;
      const content = typeof payload === 'string' ? payload : payload.content;
      if (!content) return;
      const msgId = typeof payload === 'string' ? `pet-${Date.now()}` : payload.id || `pet-${Date.now()}`;
      updateSessionState(sessionKey, (state) => {
        if (
          state.messages.length > 0 &&
          state.messages[state.messages.length - 1].content === content &&
          state.messages[state.messages.length - 1].role === 'assistant'
        ) {
          return state;
        }
        return {
          ...state,
          messages: sortMessages([
            ...state.messages,
            { id: msgId, role: 'assistant', content, created_at: new Date().toISOString() },
          ]),
        };
      });
    });
    return () => { void unlisten.then((d) => d()).catch(() => {}); };
  }, [opts?.acceptPetMessages, opts?.petMessageSource, sessionKey]);

  async function submitMessage(content: string, options: ChatSendOptions, clearInput = false) {
    if (!content.trim() || currentState.sending) return;
    const normalizedContent = content.trim();
    if (clearInput) {
      setInput('');
    }

    const userMsg: DisplayMessage = {
      id: `temp-${Date.now()}`,
      role: 'user',
      content: normalizedContent,
      created_at: new Date().toISOString(),
    };

    let effectiveSessionId = sessionIdRef.current;
    let effectiveKey = sessionKey;

    if (!effectiveSessionId) {
      const session = await api.createChatSession();
      effectiveSessionId = session.id;
      sessionIdRef.current = session.id;
      setOwnedSessionId(session.id);
      effectiveKey = session.id;
      opts?.onSessionCreated?.(session.id);
    }

    const requestId = `req-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

    updateSessionState(effectiveKey, (state) => ({
      ...state,
      messages: [...state.messages, userMsg],
      sending: true,
      streamTokens: [],
      toolStates: new Map(),
      thinkingContent: null,
      loadError: null,
      activeRequestId: requestId,
      activeStreamMessageId: `stream-${requestId}`,
      turnStartedAt: Date.now(),
      currentPhase: null,
      toolRunCount: 0,
    }));

    let timeoutId: number | undefined;
    const clientTimeoutMs = sendClientTimeoutMs(options.taskMode);
    try {
      const timeoutPromise = new Promise<never>((_, reject) => {
        timeoutId = window.setTimeout(
          () => reject(new Error('等待超时了，后端可能暂时卡住')),
          clientTimeoutMs,
        );
      });
      const reply = await Promise.race([
        api.sendChatMessageV2(
          normalizedContent,
          effectiveSessionId,
          options.taskMode,
          options.capability,
          options.planOnly,
          options.approvedWriteScope ?? null,
          requestId,
        ),
        timeoutPromise,
      ]);
      updateSessionState(effectiveKey, (state) => {
        if (state.activeRequestId !== requestId) {
          return state;
        }
        return {
          ...state,
          messages: mergeHistory(state.messages, reply.history),
          sending: false,
          streamTokens: [],
          thinkingContent: null,
          activeRequestId: null,
          activeStreamMessageId: null,
          turnStartedAt: null,
          currentPhase: null,
          toolRunCount: 0,
        };
      });
      void getCurrentWindow().emit('pet_message', {
        id: reply.message.id,
        content: reply.bubble_summary ?? reply.message.content,
        kind: 'assistant',
        source: opts?.petMessageSource ?? 'chat_panel',
      }).catch(() => {});
    } catch (err) {
      updateSessionState(effectiveKey, (state) => ({
        ...state,
        sending: false,
        activeRequestId: null,
        activeStreamMessageId: null,
        turnStartedAt: null,
        currentPhase: null,
        toolRunCount: 0,
        messages: state.messages.map((msg) =>
          msg.id === userMsg.id
            ? {
                ...msg,
                error: true,
                content: `${msg.content}\n\n发送没成功：${err instanceof Error ? err.message : '出了点问题'}`,
              }
            : msg,
        ),
      }));
    } finally {
      if (timeoutId !== undefined) {
        window.clearTimeout(timeoutId);
      }
    }
  }

  async function sendMessage(options: ChatSendOptions) {
    if (!input.trim() || currentState.sending) return;
    await submitMessage(input, options, true);
  }

  async function sendPrompt(content: string, options: ChatSendOptions) {
    await submitMessage(content, options, false);
  }

  function retryMessage(msg: DisplayMessage) {
    updateSessionState(sessionKey, (state) => ({
      ...state,
      messages: state.messages.filter((m) => m.id !== msg.id),
    }));
    setInput(msg.content);
  }

  function cancelSending() {
    updateSessionState(sessionKey, (state) => {
      if (!state.sending) return state;
      // Drop the in-flight stream message and reset to the last clean state.
      const cleanMessages = state.messages.filter(
        (m) => m.id !== state.activeStreamMessageId && !m.id.startsWith('stream-'),
      );
      return {
        ...state,
        messages: cleanMessages,
        sending: false,
        streamTokens: [],
        thinkingContent: null,
        activeRequestId: null,
        activeStreamMessageId: null,
        turnStartedAt: null,
        currentPhase: null,
        toolRunCount: 0,
      };
    });
  }

  function clearMessages() {
    updateSessionState(sessionKey, () => createEmptySessionState());
  }

  async function approveProposal(proposalId: string) {
    try {
      await api.approveProposal(proposalId);
    } catch (err) {
      console.error('Failed to approve proposal:', err);
    }
  }

  async function rejectProposal(proposalId: string) {
    try {
      await api.rejectProposal(proposalId);
    } catch (err) {
      console.error('Failed to reject proposal:', err);
    }
  }

  async function createGoalFromSeed(goalSeed: GoalSeed) {
    const workspaceId = opts?.workspaceId;
    if (!workspaceId) {
      throw new Error('Attach a workspace before upgrading to a goal.');
    }

    const goal = await createGoal(workspaceId, goalSeed.title, goalSeed.objective);

    // Link the current chat session to the new goal so goal progress
    // can be projected back into this conversation.
    if (sessionIdRef.current) {
      await api.setChatSessionKind(sessionIdRef.current, 'goal', goal.id);
      await appendGoalUserMessage(
        sessionIdRef.current,
        goalSeedUserMessage(goalSeed),
      );
    }

    await updateGoalStatus(goal.id, 'planning');
  }

  const memoizedToolStates = useMemo(() => currentState.toolStates, [currentState.toolStates]);
  const memoizedProjectedRuns = useMemo(
    () =>
      Object.values(currentState.projectedRuns)
        .filter((run) => run.finishedAt === null)
        .sort((a, b) => a.turnStartedAt - b.turnStartedAt),
    [currentState.projectedRuns],
  );

  const activeToolCount = useMemo(() => {
    let count = 0;
    currentState.toolStates.forEach((s) => {
      if (s.status === 'running' || s.status === 'awaiting_approval') count++;
    });
    return count;
  }, [currentState.toolStates]);

  return {
    messages: currentState.messages,
    input,
    setInput,
    sending: currentState.sending,
    streamTokens: currentState.streamTokens,
    toolStates: memoizedToolStates,
    thinkingContent: currentState.thinkingContent,
    endRef,
    sendMessage,
    retryMessage,
    clearMessages,
    cancelSending,
    loadError: currentState.loadError,
    approveProposal,
    rejectProposal,
    turnStartedAt: currentState.turnStartedAt,
    currentPhase: currentState.currentPhase,
    toolRunCount: currentState.toolRunCount,
    activeToolCount,
    projectedRuns: memoizedProjectedRuns,
    createGoalFromSeed,
    sendPrompt,
  };
}
