import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';
import { api, type ChatSessionSummary, listGoals, type GoalRun } from '../ipc/invoke';

interface ChatSessionSidebarProps {
  activeSessionId: string | null;
  onSelectSession: (sessionId: string | null) => void;
  currentWorkspaceId?: string | null;
  onSessionsLoaded?: (sessions: ChatSessionSummary[]) => void;
}

const PINNED_CHITCHAT_TITLE = '\u95f2\u804a';
const ACTIVE_GOAL_STATUSES = new Set([
  'planning',
  'running',
  'awaiting_review',
]);
const INTERNAL_SESSION_PREFIXES = ['goal-task-exec:'];
const GOAL_STATUS_LABELS: Record<string, string> = {
  planning: 'Planning',
  running: 'Running',
  awaiting_review: 'Needs review',
  blocked: 'Blocked',
  rework_required: 'Needs rework',
  accepted: 'Accepted',
  failed: 'Failed',
  cancelled: 'Cancelled',
};

function formatRelativeTime(isoString: string): string {
  const diffMin = Math.floor((Date.now() - new Date(isoString).getTime()) / 60_000);
  if (diffMin < 1) return 'Just now';
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) return `${diffHour}h ago`;
  const diffDay = Math.floor(diffHour / 24);
  if (diffDay < 7) return `${diffDay}d ago`;
  return new Date(isoString).toLocaleDateString();
}

function formatElapsed(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return `${String(min).padStart(2, '0')}:${String(sec).padStart(2, '0')}`;
}

function sortSessions(sessions: ChatSessionSummary[]): ChatSessionSummary[] {
  return [...sessions].sort((a, b) => {
    if (a.title === PINNED_CHITCHAT_TITLE && b.title !== PINNED_CHITCHAT_TITLE) return -1;
    if (b.title === PINNED_CHITCHAT_TITLE && a.title !== PINNED_CHITCHAT_TITLE) return 1;
    return Date.parse(b.updated_at) - Date.parse(a.updated_at);
  });
}

function isActiveGoal(goal: GoalRun): boolean {
  return ACTIVE_GOAL_STATUSES.has(goal.status);
}

function formatGoalSummary(goals: GoalRun[]): string {
  if (goals.length === 0) {
    return '';
  }
  const [first, ...rest] = goals;
  const statusLabel = GOAL_STATUS_LABELS[first.status] ?? first.status;
  const headline = `${first.title} · ${statusLabel}`;
  return rest.length > 0 ? `${headline} +${rest.length}` : headline;
}

function isUserVisibleSession(session: ChatSessionSummary): boolean {
  return !INTERNAL_SESSION_PREFIXES.some((prefix) => session.title.startsWith(prefix));
}

export function ChatSessionSidebar({
  activeSessionId,
  onSelectSession,
  currentWorkspaceId,
  onSessionsLoaded,
}: ChatSessionSidebarProps) {
  const [sessions, setSessions] = useState<ChatSessionSummary[]>([]);
  const [activeGoals, setActiveGoals] = useState<GoalRun[]>([]);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState('');
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);

  const hasWorking = sessions.some((session) => session.working);
  const refreshRef = useRef<() => void>(() => {});

  useEffect(() => {
    if (!hasWorking) return;
    const id = window.setInterval(() => refreshRef.current(), 5000);
    return () => window.clearInterval(id);
  }, [hasWorking]);

  const refresh = useCallback(async () => {
    try {
      const [pinned, list, goals] = await Promise.all([
        api.ensureChatSession(PINNED_CHITCHAT_TITLE),
        api.listChatSessions(50),
        currentWorkspaceId ? listGoals(currentWorkspaceId).catch(() => []) : Promise.resolve([]),
      ]);
      const pinnedIds = new Set([pinned.id]);
      const pinnedTitles = new Set([pinned.title]);
      const filtered = list.filter((session) => {
        if (pinnedIds.has(session.id)) return false;
        if (pinnedTitles.has(session.title)) return false;
        if (!isUserVisibleSession(session)) return false;
        return true;
      });
      const merged = sortSessions([pinned, ...filtered]);
      setSessions(merged);
      setActiveGoals(goals.filter(isActiveGoal));
      onSessionsLoaded?.(merged);
    } catch {
      // ignore sidebar refresh failures
    }
  }, [currentWorkspaceId, onSessionsLoaded]);

  useEffect(() => {
    refreshRef.current = () => void refresh();
  }, [refresh]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (activeSessionId !== null) {
      void refresh();
    }
  }, [activeSessionId, refresh]);

  useEffect(() => {
    const unlistenGoals = listen('goals_changed', () => {
      void refresh();
    });
    const unlistenTeams = listen('agent_teams_changed', () => {
      void refresh();
    });

    return () => {
      unlistenGoals.then((dispose) => dispose()).catch(() => {});
      unlistenTeams.then((dispose) => dispose()).catch(() => {});
    };
  }, [refresh]);

  async function handleNewSession() {
    try {
      const session = await api.createChatSession(undefined, currentWorkspaceId ?? undefined);
      await refresh();
      onSelectSession(session.id);
    } catch {
      // ignore create failures
    }
  }

  async function handleRename(sessionId: string) {
    const title = editTitle.trim();
    if (!title) {
      setEditingId(null);
      return;
    }
    try {
      await api.renameChatSession(sessionId, title);
      await refresh();
    } catch {
      // ignore rename failures
    }
    setEditingId(null);
  }

  async function handleArchive(sessionId: string, event: React.MouseEvent) {
    event.stopPropagation();
    const target = sessions.find((session) => session.id === sessionId);
    if (target?.title === PINNED_CHITCHAT_TITLE) return;
    try {
      await api.archiveChatSession(sessionId);
      if (activeSessionId === sessionId) {
        onSelectSession(sessions[0]?.id ?? null);
      }
      await refresh();
    } catch {
      // ignore archive failures
    }
  }

  function startRename(session: ChatSessionSummary, event: React.MouseEvent) {
    event.stopPropagation();
    setEditingId(session.id);
    setEditTitle(session.title);
  }

  const activeGoalSummary = formatGoalSummary(activeGoals);

  return (
    <div className="chat-session-sidebar">
      <div className="session-sidebar-avatar">
        <img
          src="/avatar/programmer/thinking.png"
          alt="Assistant avatar"
          className="session-avatar-image"
        />
        <div>
          <div className="session-avatar-title">Qinghe</div>
          <div className="session-avatar-subtitle">Chat and workbench</div>
        </div>
      </div>

      <div className="session-sidebar-actions">
        <button
          type="button"
          className="session-new-btn"
          onClick={() => void handleNewSession()}
          title="Create session"
        >
          + New session
        </button>
        {currentWorkspaceId && activeGoals.length > 0 ? (
          <div className="session-goal-banner" title={activeGoalSummary}>
            <span className="session-goal-banner-label">Goals {activeGoals.length} active</span>
            <span className="session-goal-banner-detail">{activeGoalSummary}</span>
          </div>
        ) : null}
      </div>

      <div className="session-list">
        {sessions.map((session) => {
          const showWorkspaceGoals =
            Boolean(currentWorkspaceId) &&
            session.workspace_id === currentWorkspaceId &&
            session.title !== PINNED_CHITCHAT_TITLE &&
            activeGoals.length > 0;

          return (
            <div
              key={session.id}
              className={`session-item ${activeSessionId === session.id ? 'active' : ''} ${session.working ? 'working' : ''}`}
              onClick={() => onSelectSession(session.id)}
              role="button"
              tabIndex={0}
              onKeyDown={(event) => {
                if (event.key === 'Enter') onSelectSession(session.id);
              }}
            >
              {editingId === session.id ? (
                <input
                  className="session-rename-input"
                  value={editTitle}
                  onChange={(event) => setEditTitle(event.target.value)}
                  onBlur={() => void handleRename(session.id)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault();
                      void handleRename(session.id);
                    }
                    if (event.key === 'Escape') setEditingId(null);
                  }}
                  autoFocus
                  onClick={(event) => event.stopPropagation()}
                />
              ) : (
                <>
                  <div className="session-item-title">
                    {session.title}
                    {session.title === PINNED_CHITCHAT_TITLE ? (
                      <span className="session-pin-badge">Pinned</span>
                    ) : null}
                  </div>
                  <div className="session-item-preview">
                    {session.last_message_preview ?? 'Empty conversation'}
                  </div>
                  {showWorkspaceGoals ? (
                    <div className="session-goal-summary">Goal: {activeGoalSummary}</div>
                  ) : null}
                  <div className="session-item-meta">
                    {session.working && session.working_since ? (
                      <>
                        <span className="session-working-label">
                          <span className="session-working-dot" />
                          Working {formatElapsed(now - new Date(session.working_since).getTime())}
                        </span>
                        <span className="session-working-detail">
                          {session.working_stage ??
                            (session.tool_run_count != null && session.tool_run_count > 0
                              ? `${session.tool_run_count} tools`
                              : null)}
                        </span>
                      </>
                    ) : (
                      <>
                        <span>{session.message_count} msgs</span>
                        <span>{formatRelativeTime(session.updated_at)}</span>
                      </>
                    )}
                  </div>
                  <div className="session-item-actions">
                    <button
                      type="button"
                      className="session-action-btn"
                      onClick={(event) => startRename(session, event)}
                      title="Rename"
                    >
                      R
                    </button>
                    {session.title !== PINNED_CHITCHAT_TITLE ? (
                      <button
                        type="button"
                        className="session-action-btn"
                        onClick={(event) => void handleArchive(session.id, event)}
                        title="Archive"
                      >
                        X
                      </button>
                    ) : null}
                  </div>
                </>
              )}
            </div>
          );
        })}

        {sessions.length === 0 && (
          <div className="session-empty">No conversations yet. Create one above.</div>
        )}
      </div>
    </div>
  );
}
