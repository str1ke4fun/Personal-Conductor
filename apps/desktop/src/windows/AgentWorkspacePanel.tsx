import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, appendGoalUserMessage, approveGoalPlan, createGoal, getGoalGraph, listGoals, resumeGoal, updateGoalObjective, updateGoalStatus, type ChatSessionSummary, type GoalGraphSnapshot, type GoalRun, type Workspace, type WorkspaceStatus } from '../ipc/invoke';
import { ChatComposer } from './ChatComposer';
import { ChatSessionSidebar } from './ChatSessionSidebar';
import { ChatTimelinePane } from './ChatTimelinePane';
import { CycleIndicator } from './CycleIndicator';
import GoalConsole from './GoalConsole';
import { TaskDrawerPane } from './TaskDrawerPane';
import { useChatSession } from './useChatSession';

type SessionKind = 'chat' | 'goal';
type ThemeMode = 'light' | 'dark' | 'system';

const GOAL_STATUS_LABELS: Record<string, string> = {
  draft: '草稿',
  planning: '规划中',
  awaiting_plan_approval: '待确认',
  running: '执行中',
  awaiting_review: '待收口',
  accepted: '已完成',
  blocked: '已阻塞',
  rework_required: '待补充',
  failed: '已失败',
  cancelled: '已取消',
  archived: '已归档',
};

function buildGoalDraft(userText: string): { title: string; objective: string } {
  const normalized = userText.trim();
  const title = normalized.split('\n')[0].slice(0, 72) || 'Goal';
  return {
    title,
    objective: `User request:\n${normalized}`,
  };
}

const GOAL_TERMINAL_STATUSES = new Set(['accepted', 'cancelled', 'failed', 'archived']);

const GOAL_OBJECTIVE_OVERRIDE_PATTERNS = [
  /^(?:修改|更新|调整|重设)目标[:：\s]+([\s\S]+)/u,
  /^目标(?:改为|改成|更新为|调整为|设为)[:：\s]*([\s\S]+)/u,
  /^goal(?:\s+objective)?\s*(?:to|=|:)\s*([\s\S]+)/i,
];

export function extractGoalObjectiveOverride(input: string): { title: string; objective: string } | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  for (const pattern of GOAL_OBJECTIVE_OVERRIDE_PATTERNS) {
    const match = trimmed.match(pattern);
    const candidate = match?.[1]?.trim();
    if (candidate) {
      return buildGoalDraft(candidate);
    }
  }

  return null;
}

function getInitialTheme(): ThemeMode {
  try {
    const stored = localStorage.getItem('conductor-theme');
    if (stored === 'light' || stored === 'dark' || stored === 'system') return stored;
  } catch {}
  return 'system';
}

function resolveTheme(mode: ThemeMode): 'light' | 'dark' {
  if (mode === 'system') {
    return typeof window.matchMedia === 'function'
      && window.matchMedia('(prefers-color-scheme: dark)').matches
      ? 'dark'
      : 'light';
  }
  return mode;
}

function applyTheme(resolved: 'light' | 'dark') {
  document.documentElement.classList.toggle('dark', resolved === 'dark');
}

export function AgentWorkspacePanel() {
  const [taskDrawerOpen, setTaskDrawerOpen] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [pendingBadgeCount, setPendingBadgeCount] = useState(0);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [sessions, setSessions] = useState<ChatSessionSummary[]>([]);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [workspaceStatus, setWorkspaceStatus] = useState<WorkspaceStatus | null>(null);
  const [attaching, setAttaching] = useState(false);
  const [activeGoal, setActiveGoal] = useState<GoalRun | null>(null);
  const [goalGraph, setGoalGraph] = useState<GoalGraphSnapshot | null>(null);
  const [themeMode, setThemeMode] = useState<ThemeMode>(getInitialTheme);
  const autoApprovedGoalIdRef = useRef<string | null>(null);
  // Updated on every render so handleSend always sees the latest chat state
  // without requiring chat as a useCallback dependency.
  const chatRef = useRef<typeof chat | null>(null);
  // Tracks optimistic session_kind changes that are in-flight to the backend.
  // Prevents stale sidebar refreshes (started before the mode switch) from
  // clobbering the optimistic update and causing a double-mount of GoalConsole.
  const pendingKindUpdates = useRef<Map<string, string>>(new Map());
  const appWindow = getCurrentWindow();

  useEffect(() => {
    const resolved = resolveTheme(themeMode);
    applyTheme(resolved);
    localStorage.setItem('conductor-theme', themeMode);
  }, [themeMode]);

  useEffect(() => {
    if (themeMode !== 'system' || typeof window.matchMedia !== 'function') return;
    const mql = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = () => applyTheme(resolveTheme('system'));
    mql.addEventListener('change', handler);
    return () => mql.removeEventListener('change', handler);
  }, [themeMode]);

  const activeSession = useMemo(
    () => sessions.find((session) => session.id === activeSessionId) ?? null,
    [activeSessionId, sessions],
  );

  const sessionKind: SessionKind = activeSession?.session_kind === 'goal' ? 'goal' : 'chat';

  const activeWorkspaceId = activeSession?.workspace_id ?? null;

  const chat = useChatSession({
    petMessageSource: 'workbench',
    acceptPetMessages: false,
    sessionId: activeSessionId,
    workspaceId: activeWorkspaceId,
    onSessionCreated: setActiveSessionId,
  });
  chatRef.current = chat;

  const recentWorkspaces = useMemo(
    () =>
      [...workspaces]
        .sort((a, b) => {
          const aTime = a.last_active_at ? new Date(a.last_active_at).getTime() : 0;
          const bTime = b.last_active_at ? new Date(b.last_active_at).getTime() : 0;
          return bTime - aTime;
        })
        .slice(0, 5),
    [workspaces],
  );

  const allOtherWorkspaces = useMemo(
    () => workspaces.filter((workspace) => !recentWorkspaces.some((recent) => recent.id === workspace.id)),
    [recentWorkspaces, workspaces],
  );

  const activeWorkspace = useMemo(
    () => workspaces.find((workspace) => workspace.id === activeWorkspaceId) ?? null,
    [activeWorkspaceId, workspaces],
  );

  const refreshWorkspaces = useCallback(async () => {
    try {
      setWorkspaces(await api.listWorkspaces());
    } catch {
      // ignore
    }
  }, []);

  const refreshSessions = useCallback(async () => {
    try {
      const updated = await api.listChatSessions(20);
      // Apply the same pending-kind guard as handleSessionsLoaded so that an
      // in-flight mode switch isn't clobbered even by our own refresh.
      const pending = pendingKindUpdates.current;
      setSessions(
        pending.size === 0
          ? updated
          : updated.map((s) => {
              const pk = pending.get(s.id);
              return pk ? { ...s, session_kind: pk as 'chat' | 'goal' } : s;
            }),
      );
      return updated;
    } catch {
      return null;
    }
  }, []);

  // Smart merge: applies fresh session data but preserves any session_kind values
  // that are currently being optimistically updated (mode switch in flight).
  const handleSessionsLoaded = useCallback((freshSessions: ChatSessionSummary[]) => {
    const pending = pendingKindUpdates.current;
    if (pending.size === 0) {
      setSessions(freshSessions);
      return;
    }
    setSessions(
      freshSessions.map((s) => {
        const pendingKind = pending.get(s.id);
        return pendingKind ? { ...s, session_kind: pendingKind as 'chat' | 'goal' } : s;
      }),
    );
  }, []);

  const createGoalFromInput = useCallback(
    async (userText: string) => {
      if (!activeWorkspaceId) {
        return null;
      }
      const { title, objective } = buildGoalDraft(userText);
      const goal = await createGoal(activeWorkspaceId, title, objective);
      const plannedGoal = await updateGoalStatus(goal.id, 'planning');
      if (activeSessionId) {
        await api.setChatSessionKind(activeSessionId, 'goal', goal.id);
        await appendGoalUserMessage(activeSessionId, userText);
      }
      setActiveGoal(plannedGoal);
      await refreshSessions();
      return plannedGoal;
    },
    [activeSessionId, activeWorkspaceId, refreshSessions],
  );

  useEffect(() => {
    void refreshWorkspaces();
  }, [refreshWorkspaces]);

  useEffect(() => {
    void (async () => {
      try {
        const list = await refreshSessions();
        if (!list) return;
        if (list.length > 0) {
          setActiveSessionId(list[0].id);
          return;
        }

        const session = await api.createChatSession();
        setSessions([session]);
        setActiveSessionId(session.id);
      } catch {
        // ignore
      }
    })();
  }, [refreshSessions]);

  // Sync activeGoal whenever the session's goal_id changes.
  useEffect(() => {
    const goalId = activeSession?.goal_id;
    if (!goalId || !activeWorkspaceId) {
      setActiveGoal(null);
      return;
    }
    listGoals(activeWorkspaceId).then((goals) => {
      setActiveGoal(goals.find((g) => g.id === goalId) ?? null);
    }).catch(() => setActiveGoal(null));
  }, [activeSession?.goal_id, activeWorkspaceId]);

  // Poll activeGoal status while it is in a non-terminal state.
  useEffect(() => {
    const goalId = activeSession?.goal_id;
    if (!goalId || !activeWorkspaceId) return;
    if (activeGoal && GOAL_TERMINAL_STATUSES.has(activeGoal.status)) return;
    const id = window.setInterval(() => {
      listGoals(activeWorkspaceId)
        .then((goals) => setActiveGoal(goals.find((g) => g.id === goalId) ?? null))
        .catch(() => {});
    }, 3000);
    return () => window.clearInterval(id);
  }, [activeSession?.goal_id, activeWorkspaceId, activeGoal?.status]);

  // Fetch and poll the goal reasoning graph for the status bar summary.
  useEffect(() => {
    const goalId = activeSession?.goal_id;
    if (!goalId || sessionKind !== 'goal') {
      setGoalGraph(null);
      return;
    }
    const refresh = () => {
      getGoalGraph(goalId).then(setGoalGraph).catch(() => {});
    };
    refresh();
    const id = window.setInterval(refresh, 8000);
    return () => window.clearInterval(id);
  }, [activeSession?.goal_id, sessionKind]);

  useEffect(() => {
    const goalId = activeGoal?.id;
    const goalStatus = activeGoal?.status;
    if (sessionKind !== 'goal' || !activeWorkspaceId || !goalId || goalStatus !== 'awaiting_plan_approval') {
      if (!goalId || goalStatus !== 'awaiting_plan_approval') {
        autoApprovedGoalIdRef.current = null;
      }
      return;
    }

    if (autoApprovedGoalIdRef.current === goalId) return;
    autoApprovedGoalIdRef.current = goalId;

    approveGoalPlan(goalId)
      .then((goal) => setActiveGoal(goal))
      .catch((error) => {
        autoApprovedGoalIdRef.current = null;
        console.error('Failed to auto-approve legacy goal plan:', error);
      });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionKind, activeWorkspaceId, activeGoal?.id, activeGoal?.status]);

  useEffect(() => {
    if (!activeWorkspaceId) {
      setWorkspaceStatus(null);
      return;
    }

    api.getWorkspaceStatus(activeWorkspaceId)
      .then(setWorkspaceStatus)
      .catch(() => setWorkspaceStatus(null));
  }, [activeWorkspaceId]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.key === 'b') {
        event.preventDefault();
        setSidebarCollapsed((collapsed) => !collapsed);
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, []);

  const toggleDrawer = useCallback(() => {
    setTaskDrawerOpen((open) => !open);
  }, []);

  const toggleSidebar = useCallback(() => {
    setSidebarCollapsed((collapsed) => !collapsed);
  }, []);

  const handleSelectSession = useCallback((sessionId: string | null) => {
    setActiveSessionId(sessionId);
  }, []);

  const shortPath = useCallback((root: string) => {
    const parts = root.replace(/\\/g, '/').split('/').filter(Boolean);
    if (parts.length <= 1) return root;
    if (parts.length === 2) return parts.join('/');
    return `.../${parts.slice(-2).join('/')}`;
  }, []);

  async function handleWorkspaceSelect(workspaceId: string) {
    if (!activeSessionId) return;

    const normalized = workspaceId || null;
    await api.updateChatSessionWorkspace(activeSessionId, normalized ?? undefined);
    await refreshWorkspaces();
    await refreshSessions();
  }

  async function handleFolderPicker() {
    if (!activeSessionId || attaching) return;

    try {
      setAttaching(true);
      const selected = await open({
        directory: true,
        title: 'Select workspace folder',
      });
      if (!selected) return;

      const root = typeof selected === 'string' ? selected : selected;
      const workspace = await api.attachWorkspace(root);
      await api.updateChatSessionWorkspace(activeSessionId, workspace.id);
      await refreshWorkspaces();
      await refreshSessions();
    } catch (error) {
      console.error('Failed to attach workspace:', error);
    } finally {
      setAttaching(false);
    }
  }

  async function handleApprovePlan(plan: {
    title: string;
    steps: Array<{ title: string; detail?: string }>;
    writeScope?: string[];
    diffPreview?: string;
  }) {
    const scopeLines = (plan.writeScope ?? []).map((item) => `- ${item}`).join('\n');
    await chat.sendPrompt(
      [
        `Plan approved: ${plan.title}`,
        'Execute the implementation now.',
        'Keep changes strictly within this write scope:',
        scopeLines || '- Follow the approved scope only',
        'Run the relevant verification commands and report pass/fail with key output.',
      ].join('\n'),
      {
        taskMode: 'short',
        capability: 'ask_write',
        planOnly: false,
        approvedWriteScope: plan.writeScope ?? [],
      },
    );
  }

  async function handleRejectPlan(plan: {
    title: string;
    steps: Array<{ title: string; detail?: string }>;
    writeScope?: string[];
    diffPreview?: string;
  }) {
    await chat.sendPrompt(
      [
        `Revise the plan: ${plan.title}`,
        'Keep plan-only mode.',
        'Tighten the write scope, clarify the execution steps, and refresh the diff preview.',
      ].join('\n'),
      {
        taskMode: 'short',
        capability: 'ask_write',
        planOnly: true,
      },
    );
  }

  // Switch the active session between chat and goal mode. This is a
  // session-level property persisted in the backend (see set_chat_session_kind).
  const handleSwitchKind = useCallback(
    async (kind: SessionKind) => {
      if (!activeSessionId || sessionKind === kind) return;
      const sid = activeSessionId;
      const existingGoalId = activeSession?.goal_id ?? null;
      // Guard: prevent stale sidebar refreshes (in-flight before this switch)
      // from clobbering the optimistic update with old session_kind data.
      pendingKindUpdates.current.set(sid, kind);
      setSessions((current) =>
        current.map((session) =>
          session.id === sid ? { ...session, session_kind: kind } : session,
        ),
      );

      let apiSucceeded = false;
      try {
        await api.setChatSessionKind(sid, kind, existingGoalId);
        apiSucceeded = true;
      } catch (error) {
        console.error('Failed to switch session kind:', error);
      }

      if (!apiSucceeded) {
        // API failed: clear the guard before refreshing so the refresh shows
        // the actual (reverted) backend state.
        pendingKindUpdates.current.delete(sid);
      }

      await refreshSessions();

      if (apiSucceeded) {
        // Success: clear the guard after refresh (the guard kept GoalConsole
        // alive during the refresh if backend data lacked session_kind yet).
        pendingKindUpdates.current.delete(sid);
      }
    },
    [activeSessionId, sessionKind, activeSession, refreshSessions],
  );

  // In goal mode, the first message from the user becomes the goal objective.
  // The goal orchestrator then drives execution and projects progress back into
  // this session — so we must NOT also fire a foreground chat turn here, or the
  // timeline shows two concurrent `Working` blocks (see Bug说明 20260601).
  // Follow-up input can resume or retarget the active goal before we fall back
  // to a foreground long-task turn.
  const handleSend = useCallback(
    async (options: Parameters<ReturnType<typeof useChatSession>['sendMessage']>[0]) => {
      // Always read from the ref so we get the latest input/functions without
      // adding the entire chat object to the dependency array.
      const c = chatRef.current!;
      const trimmedInput = c.input.trim();

      if (sessionKind === 'goal' && !activeSession?.goal_id && activeWorkspaceId && trimmedInput) {
        try {
          await createGoalFromInput(trimmedInput);
        } catch (err) {
          console.error('Failed to create goal from first message:', err);
        }
        // Clear the composer and stop here — the orchestrator owns execution now.
        c.setInput('');
        return;
      }

      if (sessionKind === 'goal' && activeSession?.goal_id && activeSessionId && trimmedInput) {
        const goalId = activeSession.goal_id;
        if (activeGoal && GOAL_TERMINAL_STATUSES.has(activeGoal.status)) {
          try {
            await createGoalFromInput(trimmedInput);
          } catch (err) {
            console.error('Failed to create follow-up goal from terminal session:', err);
          }
          c.setInput('');
          return;
        }
        const objectiveOverride = extractGoalObjectiveOverride(trimmedInput);
        if (objectiveOverride) {
          try {
            await appendGoalUserMessage(activeSessionId, trimmedInput);
            let updatedGoal = await updateGoalObjective(
              goalId,
              objectiveOverride.title,
              objectiveOverride.objective,
            );
            if (activeGoal?.status === 'blocked') {
              try {
                updatedGoal = await resumeGoal(goalId);
              } catch (resumeErr) {
                console.error('Failed to resume blocked goal after objective update:', resumeErr);
              }
            }
            setActiveGoal(updatedGoal);
            await refreshSessions();
          } catch (err) {
            console.error('Failed to update goal objective:', err);
          }
          c.setInput('');
          return;
        }

        try {
          await appendGoalUserMessage(activeSessionId, trimmedInput);
          if (activeGoal?.status === 'blocked') {
            const resumedGoal = await resumeGoal(goalId);
            setActiveGoal(resumedGoal);
          } else if (activeGoal?.status === 'rework_required') {
            const replannedGoal = await updateGoalStatus(goalId, 'planning');
            setActiveGoal(replannedGoal);
          }
          await refreshSessions();
        } catch (err) {
          console.error('Failed to append goal follow-up input:', err);
        }
        if (!activeGoal || !GOAL_TERMINAL_STATUSES.has(activeGoal.status)) {
          c.setInput('');
          return;
        }
      }

      // Goal mode always sends as long-task so the backend uses the correct tool set.
      const goalOptions = sessionKind === 'goal'
        ? { ...options, taskMode: 'long' as const }
        : options;
      return c.sendMessage(goalOptions);
    },
    [sessionKind, activeSession?.goal_id, activeWorkspaceId, activeSessionId, activeGoal, createGoalFromInput, refreshSessions],
  );

  return (
    <div className="workbench-root">
      <aside className={`workbench-sidebar ${sidebarCollapsed ? 'collapsed' : ''}`}>
        <div className="workbench-sidebar-header">
          {!sidebarCollapsed && <h3>会话</h3>}
          <button
            type="button"
            className="workbench-sidebar-toggle"
            onClick={toggleSidebar}
            title={sidebarCollapsed ? '展开侧边栏 (Ctrl+B)' : '收起侧边栏 (Ctrl+B)'}
          >
            {sidebarCollapsed ? '›' : '‹'}
          </button>
        </div>
        {!sidebarCollapsed && (
          <div className="workbench-sidebar-content">
            <ChatSessionSidebar
              activeSessionId={activeSessionId}
              onSelectSession={handleSelectSession}
              currentWorkspaceId={activeWorkspaceId}
              onSessionsLoaded={handleSessionsLoaded}
            />
          </div>
        )}
      </aside>

      <div className={`workbench-main ${sessionKind === 'goal' ? 'goal-mode' : ''}`}>
      <main className="workbench-timeline">
        <div className="workbench-timeline-header">
          <div className="workbench-timeline-identity">
            <div className="workbench-timeline-title">
              <h3>{activeSession?.title ?? '对话'}</h3>
            </div>
            <div className="workbench-workspace-row">
              <select
                className="workbench-select workbench-workspace-select"
                value={activeWorkspaceId ?? ''}
                onChange={(event) => void handleWorkspaceSelect(event.target.value)}
                disabled={!activeSessionId || chat.sending}
                title={chat.sending ? '对话进行中，无法切换工作区' : (workspaceStatus?.root ?? '未关联工作区')}
              >
                <option value="">未关联工作区</option>
                {recentWorkspaces.length > 0 && (
                  <optgroup label="最近">
                    {recentWorkspaces.map((workspace) => (
                      <option key={workspace.id} value={workspace.id} title={workspace.root}>
                        {workspace.name} · {shortPath(workspace.root)}
                      </option>
                    ))}
                  </optgroup>
                )}
                {allOtherWorkspaces.length > 0 && (
                  <optgroup label="全部工作区">
                    {allOtherWorkspaces.map((workspace) => (
                      <option key={workspace.id} value={workspace.id} title={workspace.root}>
                        {workspace.name} · {shortPath(workspace.root)}
                      </option>
                    ))}
                  </optgroup>
                )}
              </select>
              <button
                type="button"
                className="workbench-btn workbench-folder-btn"
                onClick={() => void handleFolderPicker()}
                disabled={!activeSessionId || attaching}
                title="选择工作区文件夹"
              >
                {attaching ? '选择中…' : '选择文件夹'}
              </button>
            </div>
          </div>

          <div
            className="workbench-mode-switch"
            role="tablist"
            aria-label="会话模式"
          >
            <button
              type="button"
              role="tab"
              aria-selected={sessionKind === 'chat'}
              className={`workbench-mode-switch-btn ${sessionKind === 'chat' ? 'active' : ''}`}
              onClick={() => void handleSwitchKind('chat')}
              disabled={!activeSessionId}
              title="聊天：普通对话"
            >
              聊天
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={sessionKind === 'goal'}
              className={`workbench-mode-switch-btn ${sessionKind === 'goal' ? 'active' : ''}`}
              onClick={() => void handleSwitchKind('goal')}
              disabled={!activeSessionId}
              title="目标：长任务 / 自治目标"
            >
              目标
            </button>
          </div>

          <div className="workbench-timeline-actions">
            <button
              type="button"
              className="workbench-theme-toggle"
              onClick={() => {
                setThemeMode((prev) => {
                  if (prev === 'light') return 'dark';
                  if (prev === 'dark') return 'system';
                  return 'light';
                });
              }}
              title={`主题：${themeMode === 'light' ? '亮色' : themeMode === 'dark' ? '暗色' : '跟随系统'}`}
            >
              {themeMode === 'light' ? '☀️' : themeMode === 'dark' ? '🌙' : '💻'}
            </button>
            <button
              type="button"
              className="workbench-btn"
              onClick={toggleDrawer}
              title={taskDrawerOpen ? '收起任务栏' : '展开任务栏'}
            >
              活动{pendingBadgeCount > 0 ? ` ·${pendingBadgeCount}` : ''}
            </button>
            <button
              type="button"
              className="workbench-btn"
              onClick={() => void appWindow.hide()}
              title="关闭工作台"
            >
              ✕
            </button>
          </div>
        </div>

        {sessionKind === 'goal' && activeWorkspaceId && (
          <div className="goal-status-bar">
            <span className="goal-status-bar-label">Goal 模式</span>
            {activeGoal && (
              <>
                <span className="goal-status-bar-title">{activeGoal.title.slice(0, 56)}</span>
                <span className={`goal-status-bar-status goal-status-${activeGoal.status}`}>
                  {GOAL_STATUS_LABELS[activeGoal.status] ?? activeGoal.status}
                </span>
                {activeGoal?.status === 'awaiting_plan_approval' && (
                  <button
                    className="goal-status-bar-btn"
                    onClick={() => {
                      approveGoalPlan(activeGoal?.id ?? '')
                        .then(() => listGoals(activeWorkspaceId ?? ''))
                        .then((goals) => setActiveGoal(goals.find((g) => g.id === (activeGoal?.id ?? '')) ?? null))
                        .catch(console.error);
                    }}
                  >
                    继续执行
                  </button>
                )}
              </>
            )}
            {activeSession?.goal_id && (
              <div style={{ position: 'relative', flexShrink: 0 }}>
                <CycleIndicator goalId={activeSession.goal_id} />
              </div>
            )}
            {goalGraph && (
              <div className="goal-status-bar-graph">
                <span className="goal-graph-chip">事实 {goalGraph.facts_count}</span>
                <span className="goal-graph-chip">意图 {goalGraph.open_intents_count}</span>
                {goalGraph.hints.length > 0 && (
                  <span className="goal-graph-chip goal-graph-chip--hint">提示 {goalGraph.hints.length}</span>
                )}
                {goalGraph.graph_hash && (
                  <span className="goal-graph-chip goal-graph-chip--hash" title={goalGraph.graph_hash}>
                    #{goalGraph.graph_hash.slice(0, 6)}
                  </span>
                )}
              </div>
            )}
          </div>
        )}

        <ChatTimelinePane
        messages={chat.messages}
        sending={chat.sending}
        streamTokens={chat.streamTokens}
        toolStates={chat.toolStates}
        thinkingContent={chat.thinkingContent}
        projectedRuns={chat.projectedRuns}
        sessionKind={sessionKind}
        endRef={chat.endRef}
          onRetry={chat.retryMessage}
          onApproveProposal={chat.approveProposal}
          onRejectProposal={chat.rejectProposal}
          onApprovePlan={handleApprovePlan}
          onRejectPlan={handleRejectPlan}
          onCreateGoal={async (goalSeed) => {
            // createGoalFromSeed already sets session_kind='goal' and goal_id on the backend.
            await chat.createGoalFromSeed(goalSeed);
            // Re-fetch so local state reflects the new goal_id and session_kind.
            const updated = await api.listChatSessions(20);
            setSessions(updated);
          }}
          canCreateGoal={Boolean(activeWorkspaceId)}
          turnStartedAt={chat.turnStartedAt}
          currentPhase={chat.currentPhase}
          toolRunCount={chat.toolRunCount}
          activeToolCount={chat.activeToolCount}
        />

        <ChatComposer
          input={chat.input}
          setInput={chat.setInput}
          sending={chat.sending}
          onSend={handleSend}
          onStop={chat.cancelSending}
          sessionKind={sessionKind}
          workspaceName={activeWorkspace?.name ?? null}
        />
      </main>

      {sessionKind === 'goal' && activeWorkspaceId && (
        <aside className="workbench-goal-panel">
          <GoalConsole
            workspaceId={activeWorkspaceId}
            goalId={activeSession?.goal_id}
          />
        </aside>
      )}
      </div>

      <aside className={`workbench-drawer ${taskDrawerOpen ? 'open' : 'closed'}`}>
        <div className="workbench-drawer-header">
          {taskDrawerOpen && (
            <h3 className="workbench-drawer-title">
              活动
            </h3>
          )}
          <button
            type="button"
            className="workbench-drawer-toggle"
            onClick={toggleDrawer}
            title={taskDrawerOpen ? '收起' : '展开'}
          >
            {taskDrawerOpen ? '›' : '‹'}
          </button>
        </div>
        {taskDrawerOpen && (
          <div className="workbench-drawer-content">
            <TaskDrawerPane workspaceId={activeWorkspaceId} onPendingCountChange={setPendingBadgeCount} />
          </div>
        )}
      </aside>
    </div>
  );
}
