import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  getGoalCycles,
  listActiveHeartbeats,
  listGoalTasks,
  listGoals,
  type AgentHeartbeat,
  type AgentTaskItem,
  type GoalCycle,
  type GoalRun,
} from '../ipc/invoke';

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

const GOAL_STATUS_COLORS: Record<string, string> = {
  draft: '#888',
  planning: '#3b82f6',
  awaiting_plan_approval: '#f59e0b',
  awaiting_review: '#f59e0b',
  running: '#10b981',
  blocked: '#ef4444',
  rework_required: '#f97316',
  accepted: '#22c55e',
  failed: '#ef4444',
  cancelled: '#6b7280',
  archived: '#6b7280',
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

  return (
    <div className="goal-console goal-console-readonly">
      <div className="goal-card expanded">
        <div className="goal-card-header">
          <span className="goal-title">{goal.title}</span>
          <span
            className="goal-status-badge"
            style={{ backgroundColor: GOAL_STATUS_COLORS[goal.status] ?? '#888' }}
          >
            {GOAL_STATUS_LABELS[goal.status] ?? goal.status}
          </span>
        </div>

        <div className="goal-detail">
          <p className="goal-objective">{goal.objective}</p>

          <div className="goal-section">
            <h5>当前状态</h5>
            <div className="task-item">
              <span className="task-title">
                {currentCycle
                  ? `第 ${currentCycle.cycle_no} 轮`
                  : '等待建立执行轮次'}
              </span>
              <span className="task-status">
                {currentCycle
                  ? GOAL_STATUS_LABELS[currentCycle.status] ?? currentCycle.status
                  : GOAL_STATUS_LABELS[goal.status] ?? goal.status}
              </span>
            </div>
            {currentCycle && (
              <div className="review-meta">
                已运行 {formatDuration(currentCycle.started_at, currentCycle.finished_at)}
              </div>
            )}
            <div className="review-meta">
              排队 {taskSummary.queued} · 执行中 {taskSummary.running} · 可审阅{' '}
              {taskSummary.reviewable} · 已收口 {taskSummary.settled}
            </div>
          </div>

          <div className="goal-section">
            <h5>执行进展</h5>
            {liveHeartbeats.length > 0 ? (
              liveHeartbeats.map((heartbeat) => (
                <div key={heartbeat.id} className="task-item">
                  <span className="task-title">
                    {heartbeat.agent_id}
                    {heartbeat.progress_text ? ` · ${heartbeat.progress_text}` : ''}
                  </span>
                  <span className="task-status">
                    {HEARTBEAT_STATUS_LABELS[heartbeat.status] ?? heartbeat.status}
                  </span>
                </div>
              ))
            ) : (
              <div className="review-empty">当前没有后台执行心跳。</div>
            )}
          </div>

          <div className="goal-section">
            <h5>工作项</h5>
            {visibleTasks.length > 0 ? (
              visibleTasks.map((task) => (
                <div key={task.id} className="task-item">
                  <span className="task-title">{task.title}</span>
                  <span
                    className="task-status"
                    style={{ color: GOAL_STATUS_COLORS[task.status] ?? '#888' }}
                  >
                    {TASK_STATUS_LABELS[task.status] ?? task.status}
                  </span>
                  {task.error && <div className="review-error">{task.error}</div>}
                </div>
              ))
            ) : (
              <div className="review-empty">当前还没有可见工作项。</div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default GoalConsole;
