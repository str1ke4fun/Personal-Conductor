import { listen } from '@tauri-apps/api/event';
import { useEffect, useMemo, useRef, useState } from 'react';
import { AgentTask, type AgentRun, api, Proposal, Task } from '../ipc/invoke';

interface Banner {
  banner: string;
  urgency: 'low' | 'medium' | 'high';
}

function formatTime(isoString: string): string {
  const date = new Date(isoString);
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatTaskType(type: string): string {
  const types: Record<string, string> = {
    'review-doc': '文档看看',
    'review-code': '代码审阅',
    'summary': '做个摘要',
    'chat': '对话记录',
  };
  return types[type] || type;
}

function getStatusInfo(status: string): { label: string; icon: string; className: string } {
  switch (status) {
    case 'pending':
      return { label: '等你看看', icon: '📝', className: 'status-pending' };
    case 'in_progress':
      return { label: '在处理', icon: '⚡', className: 'status-in-progress' };
    case 'passed':
      return { label: '过了', icon: '✅', className: 'status-passed' };
    case 'rejected':
      return { label: '不合适', icon: '❌', className: 'status-rejected' };
    case 'skipped':
      return { label: '跳过了', icon: '⏭️', className: 'status-skipped' };
    default:
      return { label: status, icon: '📌', className: 'status-default' };
  }
}

function getAgentTaskStatusInfo(status: AgentTask['status']): { label: string; className: string } {
  switch (status) {
    case 'pending':
      return { label: '排队中', className: 'status-pending' };
    case 'in_progress':
      return { label: '在处理', className: 'status-in-progress' };
    case 'completed':
      return { label: '搞定了', className: 'status-passed' };
    default:
      return { label: status, className: 'status-default' };
  }
}

function getProposalStatusLabel(status: Proposal['status']): string {
  const labels: Record<Proposal['status'], string> = {
    pending: '等你定夺',
    approved: '已同意',
    running: '执行中',
    succeeded: '搞定了',
    failed: '没成功',
    rejected: '不合适',
    expired: '过期了',
    used: '用过了',
  };
  return labels[status];
}

interface TaskPanelContentProps {
  standalone?: boolean;
}

export function TaskPanelContent({ standalone = false }: TaskPanelContentProps) {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [agentTasks, setAgentTasks] = useState<AgentTask[]>([]);
  const [proposals, setProposals] = useState<Proposal[]>([]);
  const [banner, setBanner] = useState<Banner | null>(null);
  const [proposalsExpanded, setProposalsExpanded] = useState(false);
  const [busyProposalId, setBusyProposalId] = useState<string | null>(null);
  const [migratingTasks, setMigratingTasks] = useState(false);
  const [budgetMinutes, setBudgetMinutes] = useState<number | ''>('');
  const [budgetTasks, setBudgetTasks] = useState<AgentTask[] | null>(null);
  const [loadingBudget, setLoadingBudget] = useState(false);
  const [agentRuns, setAgentRuns] = useState<AgentRun[]>([]);
  const [stoppingRunId, setStoppingRunId] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    refresh();
    refreshAgentRuns();

    // Poll remains a fallback; task and agent-run events should refresh faster.
    timerRef.current = setInterval(() => { void refreshAgentRuns(); }, 5_000);

    const unlistenBanner = listen<Banner>('taskpanel_banner', (event) => {
      setBanner(event.payload);
      void refresh();
      setTimeout(() => setBanner(null), 10000);
    });
    const unlistenTasks = listen('tasks_changed', () => {
      void refresh();
      void refreshAgentRuns();
    });
    const unlistenAgentRuns = listen('agent_runs_changed', () => {
      void refreshAgentRuns();
    });
    const unlistenProposals = listen('proposal-changed', () => {
      void refresh();
    });

    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
      unlistenBanner.then((dispose) => dispose()).catch(() => {});
      unlistenTasks.then((dispose) => dispose()).catch(() => {});
      unlistenAgentRuns.then((dispose) => dispose()).catch(() => {});
      unlistenProposals.then((dispose) => dispose()).catch(() => {});
    };
  }, []);

  async function refreshAgentRuns() {
    try {
      const runs = await api.listAgentRuns(null, false);
      setAgentRuns(runs.filter((r) => r.status === 'running' || r.status === 'queued'));
    } catch {
      // ignore
    }
  }

  async function handleStopAgentRun(runId: string) {
    setStoppingRunId(runId);
    try {
      await api.stopAgentRun(runId);
      await refreshAgentRuns();
    } finally {
      setStoppingRunId(null);
    }
  }

  function formatElapsed(startedAt: string): string {
    const ms = Date.now() - new Date(startedAt).getTime();
    const totalSec = Math.floor(ms / 1000);
    const min = Math.floor(totalSec / 60);
    const sec = totalSec % 60;
    return min > 0 ? `${min}m ${sec}s` : `${sec}s`;
  }

  async function refresh() {
    const proposalStatuses: Proposal['status'][] = ['pending', 'approved', 'running', 'succeeded', 'failed'];
    const [allTasks, allAgentTasks, proposalGroups] = await Promise.all([
      api.listTasks(false),
      api.listAgentTasks(true),
      Promise.all(proposalStatuses.map((status) => api.listProposals(status))),
    ]);
    const proposalsById = new Map<string, Proposal>();
    proposalGroups.flat().forEach((proposal) => proposalsById.set(proposal.id, proposal));
    setTasks(allTasks.sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()));
    setAgentTasks(
      allAgentTasks.sort(
        (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
      ),
    );
    setProposals(
      Array.from(proposalsById.values()).sort(
        (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
      ),
    );
  }

  async function approveProposal(id: string) {
    setBusyProposalId(id);
    try {
      await api.approveProposal(id);
      await refresh();
    } finally {
      setBusyProposalId(null);
    }
  }

  async function executeProposal(id: string) {
    setBusyProposalId(id);
    try {
      await api.executeProposal(id);
      await refresh();
    } finally {
      setBusyProposalId(null);
    }
  }

  async function rejectProposal(id: string) {
    setBusyProposalId(id);
    try {
      await api.rejectProposal(id);
      await refresh();
    } finally {
      setBusyProposalId(null);
    }
  }

  async function migrateLegacyTasks() {
    setMigratingTasks(true);
    try {
      await api.migrateLegacyTasksToTasklist();
      await refresh();
    } finally {
      setMigratingTasks(false);
    }
  }

  async function queryBudget() {
    if (budgetMinutes === '' || budgetMinutes <= 0) {
      setBudgetTasks(null);
      return;
    }
    setLoadingBudget(true);
    try {
      const tasks = await api.listTasksByBudget(budgetMinutes);
      setBudgetTasks(tasks);
    } catch {
      setBudgetTasks(null);
    } finally {
      setLoadingBudget(false);
    }
  }

  const activeAgentTasks = useMemo(
    () => agentTasks.filter((task) => task.status === 'pending' || task.status === 'in_progress'),
    [agentTasks],
  );
  const activeHookTasks = useMemo(
    () => tasks.filter((task) => task.status === 'in_progress' && (task.source === 'claude' || task.source === 'codex')),
    [tasks],
  );
  const pendingHookReviews = useMemo(
    () => tasks.filter((task) => task.status === 'pending' && (task.source === 'claude' || task.source === 'codex')),
    [tasks],
  );

  return (
    <div className="task-content">
      <div className="task-header">
        <h2>今天</h2>
        <span className="task-count">
          {activeAgentTasks.length + activeHookTasks.length + agentRuns.length} 件事在忙
        </span>
        <button
          className="refresh-btn-small"
          onClick={() => {
            void refresh();
            void refreshAgentRuns();
          }}
          title="刷新"
        >
          🔄
        </button>
      </div>

      {agentRuns.length > 0 && (
        <section className="agent-task-section agent-runs-section">
          <div className="agent-task-header">
            <h3>并行任务</h3>
            <span className="task-count">{agentRuns.length} 个运行中</span>
          </div>
          <div className="agent-task-list">
            {agentRuns.map((run) => (
              <article className="task-card status-in-progress" key={run.id}>
                <div className="task-header">
                  <span className="task-time">{formatElapsed(run.started_at)}</span>
                  <span className="task-status status-in-progress">
                    {run.status === 'running' ? '运行中' : '排队中'}
                  </span>
                </div>
                <div className="task-kind">
                  {run.role || run.agent_id}
                </div>
                {run.metadata_json?.prompt_summary != null && (
                  <div className="task-request">{String(run.metadata_json.prompt_summary)}</div>
                )}
                {run.output_ref && <div className="task-done">输出：{run.output_ref}</div>}
                <div className="task-meta">
                  <span>{run.agent_id}</span>
                  {run.pid && <span>PID {run.pid}</span>}
                  {run.cwd && <span>{run.cwd}</span>}
                </div>
                <div className="task-actions">
                  <button
                    type="button"
                    className="task-stop-btn"
                    onClick={() => void handleStopAgentRun(run.id)}
                    disabled={stoppingRunId === run.id}
                  >
                    {stoppingRunId === run.id ? '停止中...' : '停止'}
                  </button>
                </div>
              </article>
            ))}
          </div>
        </section>
      )}

      {activeHookTasks.length > 0 && (
        <section className="agent-task-section">
          <div className="agent-task-header">
            <h3>外部会话</h3>
            <span className="task-count">{activeHookTasks.length} 个进行中</span>
          </div>
          <div className="agent-task-list">
            {activeHookTasks.map((task) => (
              <article className="task-card status-in-progress" key={`hook:${task.id}`}>
                <div className="task-header">
                  <span className="task-time">{formatTime(task.last_event_at ?? task.created_at)}</span>
                  <span className="task-status status-in-progress">{task.source} 正在工作</span>
                </div>
                <div className="task-kind">
                  {task.current_request || task.permission_summary || task.focus_hint || task.kind}
                </div>
                {task.permission_summary && (
                  <div className="task-permission">最新动作：{task.permission_summary}</div>
                )}
                <div className="task-session">
                  <span>{task.terminal_id ?? '未记录终端'}</span>
                  <span>{task.session_id ?? '未记录会话'}</span>
                  <span>{task.cwd ?? '未记录工作区'}</span>
                </div>
              </article>
            ))}
          </div>
        </section>
      )}

      <section className="agent-task-section">
        <div className="agent-task-header">
          <h3>进行中</h3>
          <button type="button" onClick={() => void migrateLegacyTasks()} disabled={migratingTasks}>
            整理旧记录
          </button>
        </div>
        {agentTasks.length === 0 ? (
          <div className="empty-state">
            <p>还没有任务</p>
            <small>有新安排了会在这儿出现</small>
          </div>
        ) : (
          <div className="agent-task-list">
            {agentTasks.map((task) => {
              const status = getAgentTaskStatusInfo(task.status);
              return (
                <article className={`task-card ${status.className}`} key={`${task.task_list_id}:${task.id}`}>
                  <div className="task-header">
                    <span className="task-time">{formatTime(task.updated_at)}</span>
                    <span className={`task-status ${status.className}`}>{status.label}</span>
                  </div>
                  <div className="task-kind">
                    {task.subject}
                    <span style={{ fontSize: '0.7em', opacity: 0.6, marginLeft: 6 }}>
                      {task.task_list_id}
                    </span>
                  </div>
                  {task.description && <div className="task-request">{task.description}</div>}
                  <div className="task-meta">
                    <span>{task.kind}</span>
                    <span>{task.source}</span>
                    {task.owner && <span>{task.owner}</span>}
                    {task.blocked_by.length > 0 && <span>阻塞：{task.blocked_by.join(', ')}</span>}
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </section>

      <section className="agent-task-section budget-section">
        <div className="agent-task-header">
          <h3>时间预算</h3>
        </div>
        <div className="budget-filter">
          <input
            type="number"
            min={1}
            max={480}
            placeholder="分钟"
            value={budgetMinutes}
            onChange={(e) => {
              const val = e.target.value;
              setBudgetMinutes(val === '' ? '' : Number(val));
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void queryBudget();
            }}
            className="budget-input"
          />
          <button
            type="button"
            onClick={() => void queryBudget()}
            disabled={loadingBudget || budgetMinutes === '' || budgetMinutes <= 0}
          >
            {loadingBudget ? '查询中...' : '查找任务'}
          </button>
          {budgetTasks !== null && (
            <button
              type="button"
              className="budget-clear"
              onClick={() => {
                setBudgetTasks(null);
                setBudgetMinutes('');
              }}
            >
              清除
            </button>
          )}
        </div>
        {budgetTasks !== null && (
          <div className="budget-results">
            {budgetTasks.length === 0 ? (
              <div className="empty-state">
                <p>没有在 {budgetMinutes} 分钟内能完成的任务</p>
              </div>
            ) : (
              <div className="agent-task-list">
                {budgetTasks.map((task) => {
                  const status = getAgentTaskStatusInfo(task.status);
                  return (
                    <article className={`task-card ${status.className}`} key={`budget:${task.task_list_id}:${task.id}`}>
                      <div className="task-header">
                        <span className={`task-status ${status.className}`}>{status.label}</span>
                        {task.est_minutes && (
                          <span className="task-time-est">约 {task.est_minutes} 分钟</span>
                        )}
                      </div>
                      <div className="task-kind">{task.subject}</div>
                      {task.description && <div className="task-request">{task.description}</div>}
                      <div className="task-meta">
                        <span>{task.kind}</span>
                        <span>{task.source}</span>
                      </div>
                    </article>
                  );
                })}
              </div>
            )}
          </div>
        )}
      </section>

      {banner && (
        <div className={`task-banner task-banner-${banner.urgency}`}>
          <span className="banner-icon">
            {banner.urgency === 'high' ? '⚠️' : banner.urgency === 'medium' ? '📢' : 'ℹ️'}
          </span>
          <span className="banner-text">{banner.banner}</span>
          <button className="banner-close" onClick={() => setBanner(null)}>
            ✕
          </button>
        </div>
      )}

      <div className="proposals-section">
        <button
          type="button"
          className="proposals-toggle"
          onClick={() => setProposalsExpanded(!proposalsExpanded)}
        >
          <span>建议 ({proposals.length})</span>
          <span className="proposals-toggle-arrow">{proposalsExpanded ? '▼' : '▶'}</span>
        </button>
        {proposalsExpanded && (
          <div className="proposals-content">
            {proposals.length === 0 ? (
              <div className="empty-state">
                <p>暂无建议</p>
              </div>
            ) : (
              proposals.map((proposal) => (
                <article className="proposal-card" key={proposal.id}>
                  <div className="proposal-header">
                    <span className="proposal-time">{formatTime(proposal.created_at)}</span>
                    <span className={`proposal-status proposal-status-${proposal.status}`}>
                      {getProposalStatusLabel(proposal.status)}
                    </span>
                  </div>
                  <div className="proposal-title-text">{proposal.title || proposal.content}</div>
                  {proposal.title && proposal.title !== proposal.content && (
                    <div className="proposal-content-text">{proposal.content}</div>
                  )}
                  <div className="proposal-reason">{proposal.reason}</div>
                  <div className="proposal-meta">
                    <span>{proposal.risk_level}</span>
                    <span>{proposal.tool_id ?? '—'}</span>
                    <span>{proposal.dry_run ? 'dry run' : 'execute'}</span>
                  </div>
                  {(proposal.status === 'pending' || proposal.status === 'approved') && (
                    <div className="proposal-actions">
                      {proposal.status === 'pending' && (
                        <button
                          type="button"
                          onClick={() => void approveProposal(proposal.id)}
                          disabled={busyProposalId === proposal.id}
                        >
                          同意
                        </button>
                      )}
                      {proposal.status === 'approved' && proposal.tool_id && (
                        <button
                          type="button"
                          onClick={() => void executeProposal(proposal.id)}
                          disabled={busyProposalId === proposal.id}
                        >
                          去做
                        </button>
                      )}
                      <button
                        type="button"
                        onClick={() => void rejectProposal(proposal.id)}
                        disabled={busyProposalId === proposal.id}
                      >
                        算了
                      </button>
                      <button type="button" disabled title={proposal.reason}>
                        看看有何不同
                      </button>
                    </div>
                  )}
                  {proposal.result_ref && <div className="proposal-result">结果 — {proposal.result_ref}</div>}
                </article>
              ))
            )}
          </div>
        )}
      </div>

      <div className="task-list">
        <div className="agent-task-header">
          <h3>待审输出</h3>
          <span>{pendingHookReviews.length} 条</span>
        </div>
        {pendingHookReviews.length === 0 ? (
          <div className="empty-state">
            <p>暂无待审输出</p>
            <small>外部会话完成后会在这里留下摘要、文件和会话信息</small>
          </div>
        ) : (
          pendingHookReviews.map((task) => {
            const status = getStatusInfo(task.status);
            return (
              <article className={`task-card ${status.className}`} key={task.id}>
                <div className="task-header">
                  <span className="task-time">{formatTime(task.created_at)}</span>
                  <span className={`task-status ${status.className}`}>
                    {status.icon} {status.label}
                  </span>
                </div>
                  <div className="task-kind">
                    {formatTaskType(task.kind)}
                    <span style={{ fontSize: '0.7em', opacity: 0.6, marginLeft: 6, textTransform: 'capitalize' }}>{task.source}</span>
                  </div>
                {task.current_request && (
                  <div className="task-request">要做的事 — {task.current_request}</div>
                )}
                {task.last_output_summary && (
                  <div className="task-done">做了 — {task.last_output_summary}</div>
                )}
                {task.permission_summary && (
                  <div className="task-permission">需要权限 — {task.permission_summary}</div>
                )}
                {task.focus_hint && <div className="task-hint">{task.focus_hint}</div>}
                <div className="task-meta">
                  <span className="task-file">{task.artifact.file ?? '—'}</span>
                  {task.est_minutes && <span className="task-time-est">约 {task.est_minutes} 分钟</span>}
                </div>
                <div className="task-session">
                  <span>{task.terminal_id ?? '未记录'}</span>
                  <span>{task.session_id ?? '未记录'}</span>
                  <span>{task.cwd ?? '未记录'}</span>
                </div>
              </article>
            );
          })
        )}
      </div>
    </div>
  );
}
