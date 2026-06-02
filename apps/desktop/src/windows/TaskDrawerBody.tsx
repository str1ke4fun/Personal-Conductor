import { type ReactNode, useCallback, useEffect, useMemo, useState } from 'react';
import {
  type ActivityProjectionItem,
  type AgentRun,
  type AgentTask,
  type Proposal,
  type Task,
  type WorkspaceActivityProjection,
} from '../ipc/invoke';

function formatTime(isoString: string): string {
  return new Date(isoString).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatElapsed(isoString: string): string {
  const totalSeconds = Math.floor(
    Math.max(0, Date.now() - new Date(isoString).getTime()) / 1000,
  );
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
}

function compact(value: string, max = 120): string {
  return value.length > max ? `${value.slice(0, max)}...` : value;
}

function isHookTask(task: Task): boolean {
  return task.source === 'claude' || task.source === 'codex';
}

function getString(value: unknown): string | null {
  return typeof value === 'string' && value.trim().length > 0 ? value : null;
}

function getAgentRunSummary(run: AgentRun): string {
  const meta =
    getString(run.metadata_json?.prompt_summary) ?? getString(run.metadata_json?.prompt);
  if (meta) return compact(meta);
  const args = run.command_json?.args;
  if (Array.isArray(args)) {
    const prompt = args.find((arg, index) => index > 0 && typeof arg === 'string');
    if (typeof prompt === 'string') return compact(prompt);
  }
  return run.role || run.agent_id;
}

function getTaskHeadline(task: Task): string {
  return compact(
    task.current_request ||
      task.last_output_summary ||
      task.permission_summary ||
      task.focus_hint ||
      task.kind,
  );
}

function projectionArtifact(activity: ActivityProjectionItem): string | null {
  const ordered = [
    ...activity.artifacts.filter((artifact) => artifact.label !== 'command_run'),
    ...activity.artifacts.filter((artifact) => artifact.label === 'command_run'),
  ];
  const parts = ordered.flatMap((artifact) => [
    artifact.file,
    artifact.summary_ref,
    artifact.output_ref,
    artifact.result_ref,
  ]);
  return parts.find((part): part is string => typeof part === 'string' && part.trim().length > 0) ?? null;
}

export interface DrawerItem {
  key: string;
  actor: string;
  title: string;
  time: string;
  detail: ReactNode;
}

interface BodyProps {
  tasks: Task[];
  agentTasks: AgentTask[];
  agentRuns: AgentRun[];
  proposals: Proposal[];
  projection: WorkspaceActivityProjection | null;
  onRefresh: () => Promise<void> | void;
  onPendingCountChange?: (count: number) => void;
  renderCard: (item: DrawerItem, tone: 'busy' | 'pending') => ReactNode;
}

export function buildBusyItems(
  props: Omit<BodyProps, 'renderCard' | 'onRefresh' | 'onPendingCountChange'>,
): DrawerItem[] {
  const items: DrawerItem[] = [];

  props.projection?.active.forEach((activity) => {
    const artifact = projectionArtifact(activity);
    items.push({
      key: `act:${activity.activity_id}`,
      actor: activity.actor,
      title: compact(activity.title, 60),
      time: formatTime(activity.updated_at),
      detail: (
        <>
          {artifact && (
            <div className="drawer-detail-row">
              结果: {artifact}
            </div>
          )}
          {activity.assistant_message && (
            <div className="drawer-detail-row">
              {compact(activity.assistant_message, 200)}
            </div>
          )}
          {activity.tool_calls.length > 0 && (
            <div className="drawer-detail-row">
              工具: {activity.tool_calls.map((tool) => tool.tool_id).join('、')}
            </div>
          )}
          {activity.command_runs.length > 0 && (
            <div className="drawer-detail-row">
              命令: {activity.command_runs.map((run) => run.command).join(' | ')}
            </div>
          )}
          {activity.agent_runs.length > 0 && (
            <div className="drawer-detail-row">
              Agent:{' '}
              {activity.agent_runs
                .map((run) => `${run.agent_id}(${run.status})`)
                .join(' | ')}
            </div>
          )}
        </>
      ),
    });
  });

  props.agentRuns
    .filter((run) => run.status === 'running' || run.status === 'queued')
    .forEach((run) => {
      items.push({
        key: `run:${run.id}`,
        actor: run.status === 'running' ? '后台 Agent' : '排队执行',
        title: getAgentRunSummary(run),
        time: formatElapsed(run.started_at),
        detail: (
          <>
            <div className="drawer-detail-row">
              {run.agent_id}
              {run.pid ? ` | PID ${run.pid}` : ''}
            </div>
            {run.output_ref && (
              <div className="drawer-detail-row">结果: {run.output_ref}</div>
            )}
          </>
        ),
      });
    });

  props.tasks
    .filter((task) => task.status === 'in_progress' && isHookTask(task))
    .forEach((task) => {
      items.push({
        key: `hook:${task.id}`,
        actor: `外部会话 ${task.source}`,
        title: getTaskHeadline(task),
        time: formatTime(task.last_event_at ?? task.created_at),
        detail: (
          <div className="drawer-detail-row">
            {task.cwd ?? '未知目录'} | {task.session_id ?? task.terminal_id ?? '未知会话'}
          </div>
        ),
      });
    });

  props.agentTasks
    .filter((task) => task.status === 'in_progress')
    .forEach((task) => {
      items.push({
        key: `atask:${task.id}`,
        actor: '子任务',
        title: compact(task.subject, 60),
        time: formatTime(task.updated_at),
        detail: (
          <div className="drawer-detail-row">
            {task.kind} | {task.source}
          </div>
        ),
      });
    });

  return items;
}

export function buildPendingItems(
  props: Omit<BodyProps, 'renderCard' | 'onRefresh' | 'onPendingCountChange'>,
): DrawerItem[] {
  const items: DrawerItem[] = [];

  props.proposals
    .filter((proposal) => proposal.status === 'pending')
    .forEach((proposal) => {
      items.push({
        key: `prop:${proposal.id}`,
        actor: '待确认',
        title: compact(proposal.title || '操作请求', 60),
        time: formatTime(proposal.created_at),
        detail: (
          <div className="drawer-detail-row">风险等级: {proposal.risk_level}</div>
        ),
      });
    });

  props.tasks
    .filter((task) => task.status === 'pending')
    .forEach((task) => {
      items.push({
        key: `ptask:${task.id}`,
        actor: '待处理',
        title: getTaskHeadline(task),
        time: formatTime(task.last_event_at ?? task.created_at),
        detail: (
          <>
            <div className="drawer-detail-row">来源: {task.source}</div>
            {task.artifact.file && (
              <div className="drawer-detail-row">文件: {task.artifact.file}</div>
            )}
          </>
        ),
      });
    });

  return items;
}

export function TaskDrawerBody(props: BodyProps) {
  const {
    tasks,
    agentTasks,
    agentRuns,
    proposals,
    projection,
    onRefresh,
    onPendingCountChange,
    renderCard,
  } = props;
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(new Set());

  const toggleExpanded = useCallback((key: string) => {
    setExpandedKeys((previous) => {
      const next = new Set(previous);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  }, []);

  const busy = useMemo(
    () => buildBusyItems({ tasks, agentTasks, agentRuns, proposals, projection }),
    [tasks, agentTasks, agentRuns, proposals, projection],
  );
  const pending = useMemo(
    () => buildPendingItems({ tasks, agentTasks, agentRuns, proposals, projection }),
    [tasks, agentTasks, agentRuns, proposals, projection],
  );

  useEffect(() => {
    onPendingCountChange?.(pending.length);
  }, [pending.length, onPendingCountChange]);

  if (busy.length === 0 && pending.length === 0) {
    return (
      <div className="task-drawer">
        <div className="task-drawer-summary">
          <button className="refresh-btn-small" onClick={() => void onRefresh()} title="刷新">
            ↻
          </button>
        </div>
        <div className="empty-state-mini">当前没有后台工作。</div>
      </div>
    );
  }

  return (
    <div className="task-drawer">
      <div className="task-drawer-summary">
        <span className="task-drawer-stat active">{busy.length} 进行中</span>
        <span className="task-drawer-stat pending">{pending.length} 待处理</span>
        <button className="refresh-btn-small" onClick={() => void onRefresh()} title="刷新">
          ↻
        </button>
      </div>

      {busy.length > 0 && (
        <section className="task-drawer-section">
          <h4 className="task-drawer-section-title">正在推进 ({busy.length})</h4>
          <div className="task-drawer-cards">
            {busy.map((item) => (
              <div
                key={item.key}
                className={`drawer-card-wrapper ${expandedKeys.has(item.key) ? 'expanded' : 'collapsed'}`}
                onClick={() => toggleExpanded(item.key)}
                role="button"
                tabIndex={0}
                aria-expanded={expandedKeys.has(item.key)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    toggleExpanded(item.key);
                  }
                }}
              >
                {renderCard(item, 'busy')}
              </div>
            ))}
          </div>
        </section>
      )}

      {pending.length > 0 && (
        <section className="task-drawer-section">
          <h4 className="task-drawer-section-title">待处理 ({pending.length})</h4>
          <div className="task-drawer-cards">
            {pending.map((item) => (
              <div
                key={item.key}
                className={`drawer-card-wrapper ${expandedKeys.has(item.key) ? 'expanded' : 'collapsed'}`}
                onClick={() => toggleExpanded(item.key)}
                role="button"
                tabIndex={0}
                aria-expanded={expandedKeys.has(item.key)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    toggleExpanded(item.key);
                  }
                }}
              >
                {renderCard(item, 'pending')}
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
