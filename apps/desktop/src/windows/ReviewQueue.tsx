import { listen } from '@tauri-apps/api/event';
import React, { useCallback, useEffect, useState } from 'react';
import { listGoalTasks, type AgentTaskItem } from '../ipc/invoke';

interface ReviewQueueProps {
  goalId: string;
}

const STATUS_LABELS: Record<string, string> = {
  proposed: '已提案',
  queued: '排队中',
  claimed: '已认领',
  running: '执行中',
  review_ready: '待验收',
  accepted: '已通过',
  rework_required: '需返工',
  blocked: '已阻塞',
  failed: '已失败',
  cancelled: '已取消',
};

function formatScope(scope: string[]): string {
  if (scope.length === 0) {
    return '未声明写入范围';
  }
  if (scope.length === 1) {
    return scope[0];
  }
  return `${scope[0]} +${scope.length - 1}`;
}

function outcomeLabel(task: AgentTaskItem): string {
  switch (task.status) {
    case 'accepted':
      return '通过';
    case 'failed':
    case 'blocked':
    case 'rework_required':
      return '失败';
    case 'review_ready':
      return '待验收';
    case 'running':
      return '执行中';
    default:
      return '排队中';
  }
}

function renderTaskDetails(task: AgentTaskItem) {
  return (
    <>
      <div className="review-meta">执行者: {task.agent_kind}</div>
      <div className="review-meta">写入范围: {formatScope(task.write_scope_json)}</div>
      {task.acceptance_json.length > 0 && (
        <div className="review-meta">验收标准: {task.acceptance_json.join(' | ')}</div>
      )}
      <div className="review-meta">当前结果: {outcomeLabel(task)}</div>
      {task.result_ref && <div className="review-meta">结果引用: {task.result_ref}</div>}
      {task.error && <div className="review-error">错误: {task.error}</div>}
    </>
  );
}

export const ReviewQueue: React.FC<ReviewQueueProps> = ({ goalId }) => {
  const [tasks, setTasks] = useState<AgentTaskItem[]>([]);

  const refresh = useCallback(() => {
    listGoalTasks(goalId).then(setTasks).catch(console.error);
  }, [goalId]);

  useEffect(() => {
    refresh();
    const interval = window.setInterval(refresh, 5000);
    return () => window.clearInterval(interval);
  }, [refresh]);

  useEffect(() => {
    const unlistenGoals = listen('goals_changed', refresh);
    const unlistenTeams = listen('agent_teams_changed', refresh);
    return () => {
      unlistenGoals.then((dispose) => dispose()).catch(() => {});
      unlistenTeams.then((dispose) => dispose()).catch(() => {});
    };
  }, [refresh]);

  const activeTasks = tasks.filter((task) => task.status === 'running');
  const reviewTasks = tasks.filter((task) =>
    ['review_ready', 'rework_required', 'blocked'].includes(task.status),
  );
  const pendingTasks = tasks.filter((task) =>
    ['proposed', 'queued', 'claimed'].includes(task.status),
  );
  const completedTasks = tasks.filter((task) =>
    ['accepted', 'failed', 'cancelled'].includes(task.status),
  );

  if (tasks.length === 0) {
    return (
      <div className="review-queue">
        <h4>执行与验收</h4>
        <div className="review-empty">这个 Goal 还没有拆出可跟踪任务。</div>
      </div>
    );
  }

  return (
    <div className="review-queue">
      <h4>执行与验收</h4>

      {activeTasks.length > 0 && (
        <div className="review-section">
          <h5>执行中 ({activeTasks.length})</h5>
          {activeTasks.map((task) => (
            <div key={task.id} className={`review-card ${task.status}`}>
              <div className="review-card-header">
                <span className="review-title">{task.title}</span>
                <span className="review-status">{STATUS_LABELS[task.status] ?? task.status}</span>
              </div>
              <div className="review-card-body">{renderTaskDetails(task)}</div>
            </div>
          ))}
        </div>
      )}

      {reviewTasks.length > 0 && (
        <div className="review-section">
          <h5>待你验收 ({reviewTasks.length})</h5>
          {reviewTasks.map((task) => (
            <div key={task.id} className={`review-card ${task.status}`}>
              <div className="review-card-header">
                <span className="review-title">{task.title}</span>
                <span className="review-status">{STATUS_LABELS[task.status] ?? task.status}</span>
              </div>
              <div className="review-card-body">{renderTaskDetails(task)}</div>
            </div>
          ))}
        </div>
      )}

      {pendingTasks.length > 0 && (
        <div className="review-section">
          <h5>排队执行 ({pendingTasks.length})</h5>
          {pendingTasks.map((task) => (
            <div key={task.id} className="review-card pending">
              <div className="review-card-header">
                <span className="review-title">{task.title}</span>
                <span className="review-status">{STATUS_LABELS[task.status] ?? task.status}</span>
              </div>
              <div className="review-card-body">{renderTaskDetails(task)}</div>
            </div>
          ))}
        </div>
      )}

      {completedTasks.length > 0 && (
        <div className="review-section">
          <h5>已结束 ({completedTasks.length})</h5>
          {completedTasks.map((task) => (
            <div key={task.id} className={`review-card ${task.status}`}>
              <div className="review-card-header">
                <span className="review-title">{task.title}</span>
                <span className="review-status">{STATUS_LABELS[task.status] ?? task.status}</span>
              </div>
              <div className="review-card-body">{renderTaskDetails(task)}</div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

export default ReviewQueue;
