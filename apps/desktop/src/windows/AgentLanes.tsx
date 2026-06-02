import { listen } from '@tauri-apps/api/event';
import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  api,
  listActiveHeartbeats,
  type AgentHeartbeat,
  type AgentMailboxMessage,
  type AgentTeam,
  type AgentTeamMember,
  type AgentTeamSnapshot,
} from '../ipc/invoke';

interface AgentLanesProps {
  workspaceId: string;
}

const STATUS_ICONS: Record<string, string> = {
  idle: '闲',
  observing: '观',
  planning: '规',
  working: '行',
  awaiting_permission: '等',
  awaiting_input: '问',
  reviewing: '整',
  blocked: '阻',
  stopping: '停',
  stale: '断',
};

const TEAM_LIFECYCLE_LABELS: Record<string, string> = {
  awaiting_plan_approval: '待启动执行',
  executing: '执行中',
  awaiting_review: '待收口',
  accepted: '已完成',
  failed: '已失败',
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
  blocked: '已阻塞',
  stopping: '停止中',
  stale: '心跳过期',
};

const MESSAGE_KIND_LABELS: Record<string, string> = {
  review_verdict_request: '最近请求',
  plan_approval_request: '最近计划',
  plan_proposed: '最近计划',
  progress_update: '最近进展',
};

function readMetadataString(member: AgentTeamMember, key: string): string | null {
  const value = member.metadata_json?.[key];
  return typeof value === 'string' && value.trim() ? value : null;
}

function hasExecutor(member: AgentTeamMember): boolean {
  return Boolean(
    member.run_id ||
      readMetadataString(member, 'task_id') ||
      readMetadataString(member, 'external_session_id') ||
      readMetadataString(member, 'session_id'),
  );
}

function formatExecutorSummary(
  member: AgentTeamMember,
  formatElapsed: (createdAt: string) => string,
): string {
  const segments: string[] = [];

  if (
    member.run_id ||
    readMetadataString(member, 'task_id') ||
    readMetadataString(member, 'external_session_id') ||
    readMetadataString(member, 'session_id')
  ) {
    segments.push('已绑定执行器');
  }

  segments.push(`heartbeat ${formatElapsed(member.updated_at)}`);
  return segments.join(' | ');
}

function getLatestMessage(messages: AgentMailboxMessage[]): string | null {
  const latest = messages[0];
  if (!latest?.content) {
    return null;
  }
  return latest.content.trim() || null;
}

function getLatestMessageKind(messages: AgentMailboxMessage[]): string | null {
  const latest = messages[0];
  if (!latest?.kind) {
    return null;
  }
  return MESSAGE_KIND_LABELS[latest.kind] ?? latest.kind.replace(/_/g, ' ');
}

function formatWriteScope(writeScope: string[]): string | null {
  if (writeScope.length === 0) {
    return null;
  }
  if (writeScope.length === 1) {
    return writeScope[0];
  }
  return `${writeScope[0]} +${writeScope.length - 1}`;
}

export const AgentLanes: React.FC<AgentLanesProps> = ({ workspaceId }) => {
  const [heartbeats, setHeartbeats] = useState<AgentHeartbeat[]>([]);
  const [teamSnapshots, setTeamSnapshots] = useState<AgentTeamSnapshot[]>([]);
  const [now, setNow] = useState(Date.now());
  const intervalRef = useRef<ReturnType<typeof setInterval>>();
  const autoAdvancedTeamIdsRef = useRef<Set<string>>(new Set());

  const refresh = useCallback(async () => {
    try {
      const [heartbeatList, teams] = await Promise.all([
        listActiveHeartbeats(workspaceId),
        api.listAgentTeams(workspaceId),
      ]);
      const snapshots = await Promise.all(
        teams.map((team: AgentTeam) => api.getAgentTeamSnapshot(team.id, 10)),
      );
      setHeartbeats(heartbeatList);
      setTeamSnapshots(snapshots);
    } catch (error) {
      console.error('Failed to refresh agent lanes:', error);
    }
  }, [workspaceId]);

  useEffect(() => {
    void refresh();
    intervalRef.current = setInterval(refresh, 5000);
    return () => clearInterval(intervalRef.current);
  }, [refresh]);

  useEffect(() => {
    const unlistenTeams = listen('agent_teams_changed', () => {
      void refresh();
    });
    const unlistenGoals = listen('goals_changed', () => {
      void refresh();
    });
    const unlistenRuns = listen('agent_runs_changed', () => {
      void refresh();
    });

    return () => {
      unlistenTeams.then((dispose) => dispose()).catch(() => {});
      unlistenGoals.then((dispose) => dispose()).catch(() => {});
      unlistenRuns.then((dispose) => dispose()).catch(() => {});
    };
  }, [refresh]);

  useEffect(() => {
    const tick = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(tick);
  }, []);

  const formatElapsed = (createdAt: string) => {
    const ms = now - new Date(createdAt).getTime();
    const secs = Math.floor(ms / 1000);
    const mins = Math.floor(secs / 60);
    const hrs = Math.floor(mins / 60);
    if (hrs > 0) return `${hrs}h${mins % 60}m`;
    if (mins > 0) return `${mins}m${secs % 60}s`;
    return `${secs}s`;
  };

  const executorSnapshots = teamSnapshots.filter((snapshot) =>
    snapshot.members.some(hasExecutor),
  );
  const collaborationSnapshots = teamSnapshots.filter(
    (snapshot) =>
      snapshot.team.lifecycle === 'awaiting_plan_approval' &&
      !snapshot.members.some(hasExecutor),
  );

  useEffect(() => {
    if (collaborationSnapshots.length === 0) {
      autoAdvancedTeamIdsRef.current.clear();
      return;
    }

    const pending = collaborationSnapshots.filter(
      (snapshot) => !autoAdvancedTeamIdsRef.current.has(snapshot.team.id),
    );
    if (pending.length === 0) return;

    pending.forEach((snapshot) => autoAdvancedTeamIdsRef.current.add(snapshot.team.id));
    void Promise.all(
      pending.map(async (snapshot) => {
        try {
          await api.submitAgentTeamPlanVerdict(snapshot.team.id, 'approved');
        } catch (error) {
          autoAdvancedTeamIdsRef.current.delete(snapshot.team.id);
          console.error(`Failed to auto-advance team plan ${snapshot.team.id}:`, error);
        }
      }),
    ).then(() => refresh());
  }, [collaborationSnapshots, refresh]);

  if (
    heartbeats.length === 0 &&
    executorSnapshots.length === 0 &&
    collaborationSnapshots.length === 0
  ) {
    return (
      <div className="agent-lanes">
        <h4>协作执行</h4>
        <div className="agent-lanes-empty">当前没有后台执行。</div>
      </div>
    );
  }

  return (
    <div className="agent-lanes">
      <h4>协作执行</h4>

      {executorSnapshots.length > 0 && (
        <div className="agent-lanes-team-section">
          <h5>已绑定执行器</h5>
          {executorSnapshots.map((snapshot) => (
            <div key={snapshot.team.id} className="agent-lane team-lane">
              <div className="agent-lane-header">
                <span className="agent-lane-icon">team</span>
                <span className="agent-lane-id">{snapshot.team.name}</span>
                <span className="agent-lane-status">
                  {TEAM_LIFECYCLE_LABELS[snapshot.team.lifecycle] ?? snapshot.team.lifecycle}
                </span>
              </div>
              <div className="agent-lane-body">
                {formatWriteScope(snapshot.team.write_scope) && (
                  <div className="agent-lane-progress">
                    写入范围: {formatWriteScope(snapshot.team.write_scope)}
                  </div>
                )}
                {getLatestMessage(snapshot.recent_messages) && (
                  <div className="agent-lane-progress">
                    {getLatestMessageKind(snapshot.recent_messages) && (
                      <strong>{getLatestMessageKind(snapshot.recent_messages)}: </strong>
                    )}
                    {getLatestMessage(snapshot.recent_messages)}
                  </div>
                )}
                {snapshot.members.filter(hasExecutor).map((member) => (
                  <div
                    key={`${snapshot.team.id}-${member.agent_id}`}
                    className="agent-lane-task"
                  >
                    <strong>{member.agent_id}</strong>
                    {` | ${formatExecutorSummary(member, formatElapsed)}`}
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}

      {collaborationSnapshots.length > 0 && (
        <div className="agent-lanes-team-section">
          <h5>准备执行</h5>
          {collaborationSnapshots.map((snapshot) => (
            <div key={snapshot.team.id} className="agent-lane team-lane">
              <div className="agent-lane-header">
                <span className="agent-lane-icon">team</span>
                <span className="agent-lane-id">{snapshot.team.name}</span>
                <span className="agent-lane-status">准备执行</span>
              </div>
              <div className="agent-lane-body">
                {formatWriteScope(snapshot.team.write_scope) && (
                  <div className="agent-lane-progress">
                    写入范围: {formatWriteScope(snapshot.team.write_scope)}
                  </div>
                )}
                {getLatestMessage(snapshot.recent_messages) && (
                  <div className="agent-lane-progress">
                    {getLatestMessageKind(snapshot.recent_messages) && (
                      <strong>{getLatestMessageKind(snapshot.recent_messages)}: </strong>
                    )}
                    {getLatestMessage(snapshot.recent_messages)}
                  </div>
                )}
                <div className="agent-lane-progress">还没有绑定实际执行器。</div>
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="agent-lanes-grid">
        {heartbeats.map((hb) => (
          <div key={hb.id} className={`agent-lane ${hb.status}`}>
            <div className="agent-lane-header">
              <span className="agent-lane-icon">
                {STATUS_ICONS[hb.status] ?? 'unknown'}
              </span>
              <span className="agent-lane-id">{hb.agent_id}</span>
              <span className="agent-lane-status">
                {HEARTBEAT_STATUS_LABELS[hb.status] ?? hb.status}
              </span>
            </div>
            <div className="agent-lane-body">
              {hb.stage_label && <div className="agent-lane-stage">{hb.stage_label}</div>}
              {hb.progress_text && (
                <div className="agent-lane-progress">{hb.progress_text}</div>
              )}
              <div className="agent-lane-meta">
                <span className="agent-lane-elapsed">
                  已运行 {formatElapsed(hb.created_at)}
                </span>
                {hb.active_tool_count > 0 && (
                  <span className="agent-lane-tools">
                    {hb.active_tool_count} 个工具运行中
                  </span>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

export default AgentLanes;
