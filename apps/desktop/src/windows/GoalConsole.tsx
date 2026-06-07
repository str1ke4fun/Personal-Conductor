import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  createGoalHint,
  dismissGoalHint,
  getGoalCycles,
  getGoalGraph,
  listGoalHints,
  listActiveHeartbeats,
  listGoalTasks,
  listGoals,
  type AgentHeartbeat,
  type AgentTaskItem,
  type GoalCycle,
  type GoalGraphSnapshot,
  type GoalGraphTurn,
  type GoalHint,
  type GoalRun,
} from '../ipc/invoke';
import { ReasoningTimeline } from './ReasoningTimeline';
import { ReasoningGraphCanvas } from './ReasoningGraphCanvas';
import {
  PanelSection,
  StatStrip,
  ProgressMeter,
  StatusPill,
  LiveDot,
  EmptyHint,
  toneForStatus,
} from './PanelKit';

interface GoalConsoleProps {
  workspaceId: string;
  goalId?: string | null;
}

const GOAL_STATUS_LABELS: Record<string, string> = {
  draft: '草稿',
  planning: '分析中',
  awaiting_plan_approval: '准备执行',
  awaiting_review: '待收口',
  running: '执行中',
  blocked: '阻塞',
  rework_required: '待补充',
  accepted: '已完成',
  failed: '失败',
  cancelled: '已取消',
  archived: '已归档',
};

const TASK_STATUS_LABELS: Record<string, string> = {
  proposed: '已提出',
  queued: '排队中',
  claimed: '已认领',
  running: '执行中',
  review_ready: '可审阅',
  accepted: '已通过',
  rework_required: '待补充',
  blocked: '阻塞',
  failed: '失败',
  cancelled: '已取消',
};

const HEARTBEAT_STATUS_LABELS: Record<string, string> = {
  idle: '空闲',
  observing: '观察中',
  planning: '规划中',
  working: '执行中',
  awaiting_permission: '待确认',
  awaiting_input: '等输入',
  reviewing: '整理中',
  blocked: '阻塞',
  stopping: '停止中',
  stale: '心跳过期',
};

// Condensed OODA phase tracker shown inline in the status tab.
const PHASE_ORDER = ['observing', 'orienting', 'deciding', 'dispatching', 'executing', 'reviewing', 'summarizing'];
const PHASE_SHORT: Record<string, string> = {
  observing: '观察',
  orienting: '定向',
  deciding: '决策',
  dispatching: '派发',
  executing: '执行',
  reviewing: '复盘',
  summarizing: '收口',
};

function formatDuration(start: string, end?: string) {
  const ms = new Date(end ?? Date.now()).getTime() - new Date(start).getTime();
  const minutes = Math.floor(ms / 60000);
  if (minutes < 1) {
    return '<1m';
  }
  if (minutes < 60) {
    return `${minutes}m`;
  }
  return `${Math.floor(minutes / 60)}h${minutes % 60}m`;
}

function isCurrentTaskStatus(status: string): boolean {
  return !['accepted', 'failed', 'cancelled'].includes(status);
}

function sortCycles(cycles: GoalCycle[]): GoalCycle[] {
  return [...cycles].sort((a, b) => b.cycle_no - a.cycle_no);
}

function sortTasks(tasks: AgentTaskItem[]): AgentTaskItem[] {
  return [...tasks].sort(
    (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
  );
}

function sortHeartbeats(heartbeats: AgentHeartbeat[]): AgentHeartbeat[] {
  return [...heartbeats].sort(
    (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
  );
}

export const GoalConsole: React.FC<GoalConsoleProps> = ({ workspaceId, goalId }) => {
  const [goal, setGoal] = useState<GoalRun | null>(null);
  const [cycles, setCycles] = useState<GoalCycle[]>([]);
  const [tasks, setTasks] = useState<AgentTaskItem[]>([]);
  const [heartbeats, setHeartbeats] = useState<AgentHeartbeat[]>([]);
  const [activeTab, setActiveTab] = useState<'status' | 'reasoning'>('status');

  // Reasoning tab state
  const [hints, setHints] = useState<GoalHint[]>([]);
  const [graph, setGraph] = useState<GoalGraphSnapshot | null>(null);
  const [hintInput, setHintInput] = useState('');
  const [hintSubmitting, setHintSubmitting] = useState(false);
  const [reasoningError, setReasoningError] = useState<string | null>(null);
  const hintInputRef = useRef<HTMLInputElement>(null);

  const refresh = useCallback(async () => {
    if (!goalId) {
      setGoal(null);
      setCycles([]);
      setTasks([]);
      setHeartbeats([]);
      return;
    }

    try {
      const [goals, nextCycles, nextTasks, workspaceHeartbeats] = await Promise.all([
        listGoals(workspaceId),
        getGoalCycles(goalId).catch(() => []),
        listGoalTasks(goalId).catch(() => []),
        listActiveHeartbeats(workspaceId).catch(() => []),
      ]);
      const nextGoal = goals.find((item) => item.id === goalId) ?? null;
      const taskIds = new Set(nextTasks.map((task) => task.id));
      const nextHeartbeats = workspaceHeartbeats.filter(
        (heartbeat) =>
          heartbeat.goal_id === goalId ||
          (heartbeat.task_id ? taskIds.has(heartbeat.task_id) : false),
      );

      setGoal(nextGoal);
      setCycles(nextCycles);
      setTasks(nextTasks);
      setHeartbeats(nextHeartbeats);
    } catch (error) {
      console.error('Failed to refresh goal session status:', error);
      setGoal(null);
      setCycles([]);
      setTasks([]);
      setHeartbeats([]);
    }
  }, [goalId, workspaceId]);

  const refreshReasoning = useCallback(async () => {
    if (!goalId) {
      setHints([]);
      setGraph(null);
      return;
    }
    const nextHints = await listGoalHints(goalId).catch(() => [] as GoalHint[]);
    setHints(nextHints);
    try {
      const nextGraph = await getGoalGraph(goalId);
      setGraph(nextGraph);
      setReasoningError(null);
    } catch (err) {
      const msg =
        err instanceof Error ? err.message :
        typeof err === 'string' ? err :
        JSON.stringify(err);
      setReasoningError(msg);
    }
  }, [goalId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const interval = window.setInterval(() => {
      void refresh();
    }, 5000);
    return () => window.clearInterval(interval);
  }, [refresh]);

  useEffect(() => {
    const events = ['goals_changed', 'agent_runs_changed', 'agent_teams_changed'];
    const handles = events.map((eventName) => listen(eventName, () => void refresh()));
    return () => {
      handles.forEach((handle) => {
        handle.then((dispose) => dispose()).catch(() => {});
      });
    };
  }, [refresh]);

  // Reasoning tab: initial load + polling when tab is active
  useEffect(() => {
    void refreshReasoning();
  }, [refreshReasoning]);

  useEffect(() => {
    if (activeTab !== 'reasoning') return;
    const interval = window.setInterval(() => {
      void refreshReasoning();
    }, 8000);
    return () => window.clearInterval(interval);
  }, [activeTab, refreshReasoning]);

  const currentCycle = useMemo(() => sortCycles(cycles)[0] ?? null, [cycles]);
  const visibleTasks = useMemo(() => {
    const live = sortTasks(tasks).filter((task) => isCurrentTaskStatus(task.status));
    if (live.length > 0) {
      return live;
    }
    return sortTasks(tasks).slice(0, 3);
  }, [tasks]);
  const liveHeartbeats = useMemo(() => sortHeartbeats(heartbeats), [heartbeats]);

  const taskSummary = useMemo(() => {
    const queued = tasks.filter((task) =>
      ['proposed', 'queued', 'claimed'].includes(task.status),
    ).length;
    const running = tasks.filter((task) => task.status === 'running').length;
    const reviewable = tasks.filter((task) =>
      ['review_ready', 'rework_required', 'blocked'].includes(task.status),
    ).length;
    const settled = tasks.filter((task) =>
      ['accepted', 'failed', 'cancelled'].includes(task.status),
    ).length;
    return { queued, running, reviewable, settled };
  }, [tasks]);
  const reasoningTurns = useMemo<GoalGraphTurn[]>(() => {
    if (!graph) return [];
    if (graph.chat_turns?.length) return graph.chat_turns;
    const requestIds =
      graph.chat_turn_request_ids?.length
        ? graph.chat_turn_request_ids
        : graph.chat_turn_request_id
          ? [graph.chat_turn_request_id]
          : [];
    return requestIds.map((requestId) => ({
      request_id: requestId,
      agent_task_id: null,
      status: 'unknown',
      started_at: '',
    }));
  }, [graph]);

  if (!goalId) {
    return (
      <div className="goal-console goal-console-readonly">
        <div className="goal-empty">当前会话还没有关联 Goal。</div>
      </div>
    );
  }

  if (!goal) {
    return (
      <div className="goal-console goal-console-readonly">
        <div className="goal-empty">当前 Goal 状态暂时不可用。</div>
      </div>
    );
  }

  const handleAddHint = async () => {
    const content = hintInput.trim();
    if (!content || hintSubmitting) return;
    setHintSubmitting(true);
    setReasoningError(null);
    try {
      await createGoalHint(goalId, content);
      setHintInput('');
      await refreshReasoning();
      hintInputRef.current?.focus();
    } catch (err) {
      setReasoningError(String(err));
    } finally {
      setHintSubmitting(false);
    }
  };

  const handleDismissHint = async (hintId: string) => {
    try {
      await dismissGoalHint(goalId, hintId);
      await refreshReasoning();
    } catch (err) {
      setReasoningError(String(err));
    }
  };

  const phaseIndex = currentCycle ? PHASE_ORDER.indexOf(currentCycle.status) : -1;

  return (
    <div className="goal-console goal-console-readonly deck-console">
      <div className="deck-console-header">
        <span className="deck-console-title">{goal.title}</span>
        <StatusPill
          tone={toneForStatus(goal.status)}
          pulse={goal.status === 'running'}
        >
          {GOAL_STATUS_LABELS[goal.status] ?? goal.status}
        </StatusPill>
      </div>

      {/* Tab bar */}
      <div className="deck-tabs">
        <button
          className={`deck-tab goal-tab ${activeTab === 'status' ? 'active' : ''}`}
          onClick={() => setActiveTab('status')}
        >
          状态
        </button>
        <button
          className={`deck-tab goal-tab ${activeTab === 'reasoning' ? 'active' : ''}`}
          onClick={() => { setActiveTab('reasoning'); void refreshReasoning(); }}
        >
          推理
        </button>
      </div>

      <div className="deck-tabpanel goal-detail">
        {activeTab === 'status' && (
          <>
            <p className="deck-console-objective goal-objective">{goal.objective}</p>

            <PanelSection
              title={currentCycle ? `第 ${currentCycle.cycle_no} 轮` : '当前状态'}
              tone={toneForStatus(currentCycle?.status ?? goal.status)}
              action={
                <StatusPill tone={toneForStatus(currentCycle?.status ?? goal.status)} pulse={(currentCycle?.status ?? goal.status) === 'running'}>
                  {currentCycle
                    ? GOAL_STATUS_LABELS[currentCycle.status] ?? currentCycle.status
                    : GOAL_STATUS_LABELS[goal.status] ?? goal.status}
                </StatusPill>
              }
            >
              {phaseIndex >= 0 && (
                <div className="deck-phase-track">
                  {PHASE_ORDER.map((phase, idx) => (
                    <div
                      key={phase}
                      className={`deck-phase ${idx === phaseIndex ? 'is-active' : ''} ${idx < phaseIndex ? 'is-done' : ''}`}
                    >
                      <span className="deck-phase-dot" />
                      <span className="deck-phase-label">{PHASE_SHORT[phase]}</span>
                    </div>
                  ))}
                </div>
              )}
              <StatStrip
                items={[
                  { label: '排队', value: taskSummary.queued, tone: 'warn' },
                  { label: '执行中', value: taskSummary.running, tone: 'running' },
                  { label: '可审阅', value: taskSummary.reviewable, tone: 'done' },
                  { label: '已收口', value: taskSummary.settled, tone: 'quiet' },
                ]}
              />
              <ProgressMeter
                segments={[
                  { tone: 'done', value: taskSummary.settled, label: '已收口' },
                  { tone: 'running', value: taskSummary.running, label: '执行中' },
                  { tone: 'done', value: taskSummary.reviewable, label: '可审阅' },
                  { tone: 'warn', value: taskSummary.queued, label: '排队' },
                ]}
              />
              {currentCycle && (
                <div className="deck-meta-line">
                  已运行 {formatDuration(currentCycle.started_at, currentCycle.finished_at)}
                </div>
              )}
            </PanelSection>

            <PanelSection title="执行进展" count={liveHeartbeats.length || undefined}>
              {liveHeartbeats.length > 0 ? (
                liveHeartbeats.map((heartbeat) => (
                  <div key={heartbeat.id} className="deck-row">
                    <LiveDot tone={toneForStatus(heartbeat.status)} pulse={heartbeat.status === 'working'} />
                    <div className="deck-row-main">
                      <span className="deck-row-title">
                        {heartbeat.agent_id}
                        {heartbeat.progress_text ? ` · ${heartbeat.progress_text}` : ''}
                      </span>
                    </div>
                    <span className="deck-row-time">
                      {HEARTBEAT_STATUS_LABELS[heartbeat.status] ?? heartbeat.status}
                    </span>
                  </div>
                ))
              ) : (
                <EmptyHint>当前没有后台执行心跳。</EmptyHint>
              )}
            </PanelSection>

            <PanelSection title="工作项" count={visibleTasks.length || undefined}>
              {visibleTasks.length > 0 ? (
                visibleTasks.map((task) => (
                  <div key={task.id} className="deck-row">
                    <LiveDot tone={toneForStatus(task.status)} pulse={task.status === 'running'} />
                    <div className="deck-row-main">
                      <span className="deck-row-title">{task.title}</span>
                      {task.error && <span className="deck-row-err">{task.error}</span>}
                    </div>
                    <StatusPill tone={toneForStatus(task.status)}>
                      {TASK_STATUS_LABELS[task.status] ?? task.status}
                    </StatusPill>
                  </div>
                ))
              ) : (
                <EmptyHint>当前还没有可见工作项。</EmptyHint>
              )}
            </PanelSection>
          </>
        )}

          {activeTab === 'reasoning' && (
            <>
              {/* Reasoning graph constellation */}
              <PanelSection
                title="推理图谱"
                action={
                  graph && (
                    <span className="deck-meta-line" style={{ margin: 0 }}>
                      事实 {graph.facts_count} · 意图 {graph.open_intents_count}
                    </span>
                  )
                }
              >
                {graph ? (
                  <ReasoningGraphCanvas snapshot={graph} />
                ) : reasoningError ? (
                  <EmptyHint>图快照加载失败：{reasoningError}</EmptyHint>
                ) : (
                  <EmptyHint>图快照不可用（Goal 尚未开始推理）。</EmptyHint>
                )}
              </PanelSection>

              {/* Hint composer */}
              <PanelSection title="注入提示">
                <div className="deck-hint-row hint-input-row">
                  <input
                    ref={hintInputRef}
                    className="deck-hint-input hint-input"
                    type="text"
                    placeholder="向 Agent 注入推理提示…"
                    value={hintInput}
                    onChange={(e) => setHintInput(e.target.value)}
                    onKeyDown={(e) => { if (e.key === 'Enter') void handleAddHint(); }}
                    disabled={hintSubmitting}
                  />
                  <button
                    className="deck-hint-send hint-add-btn"
                    onClick={() => void handleAddHint()}
                    disabled={hintSubmitting || !hintInput.trim()}
                  >
                    {hintSubmitting ? '…' : '添加'}
                  </button>
                </div>
                {reasoningError && <div className="deck-error">{reasoningError}</div>}
                {hints.filter((h) => h.active).length > 0 && (
                  <div className="deck-hint-chips hint-chips">
                    {hints
                      .filter((h) => h.active)
                      .map((hint) => (
                        <div key={hint.id} className="deck-hint-chip hint-chip">
                          <LiveDot tone="warn" size={6} />
                          <span className="deck-hint-chip-text hint-chip-content">{hint.content}</span>
                          <button
                            className="deck-hint-chip-x hint-chip-dismiss"
                            title="取消提示"
                            onClick={() => void handleDismissHint(hint.id)}
                          >
                            ×
                          </button>
                        </div>
                      ))}
                  </div>
                )}
              </PanelSection>

              {/* Per-turn reasoning events */}
              {reasoningTurns.length > 0 && (
                <PanelSection title="推理事件流" count={reasoningTurns.length > 1 ? reasoningTurns.length : undefined}>
                  {reasoningTurns.map((turn, index) => (
                    <div key={turn.request_id} className="reasoning-turn">
                      {reasoningTurns.length > 1 && (
                        <div className="deck-meta-line">
                          Turn {index + 1}
                          {turn.agent_task_id ? ` · ${turn.agent_task_id}` : ''}
                          {turn.status ? ` · ${turn.status}` : ''}
                        </div>
                      )}
                      <ReasoningTimeline requestId={turn.request_id} />
                    </div>
                  ))}
                </PanelSection>
              )}
            </>
          )}
      </div>
    </div>
  );
};

export default GoalConsole;
