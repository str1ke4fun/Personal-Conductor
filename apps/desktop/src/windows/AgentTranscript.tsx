import React, { useEffect, useState } from 'react';
import { listGoalEvents, type AuditEvent } from '../ipc/invoke';

interface AgentTranscriptProps {
  workspaceId: string;
  goalId?: string;
}

const EVENT_ICONS: Record<string, string> = {
  'agent_run.created': '🚀',
  'agent_run.phase_changed': '🔄',
  'tool_call.proposed': '🔧',
  'tool_call.finished': '✅',
  'tool_call.blocked': '🚫',
  'permission.requested': '🔐',
  'permission.approved': '✅',
  'permission.denied': '❌',
  'permission.revoked': '🔒',
  'act.tasks_created': '📋',
  'recovery.started': '🔄',
  'recovery.completed': '✅',
};

export const AgentTranscript: React.FC<AgentTranscriptProps> = ({ workspaceId, goalId }) => {
  const [events, setEvents] = useState<AuditEvent[]>([]);
  const [filter, setFilter] = useState('');

  useEffect(() => {
    const refresh = () => {
      listGoalEvents(workspaceId, 100).then(setEvents).catch(console.error);
    };
    refresh();
    const interval = window.setInterval(refresh, 5000);
    return () => window.clearInterval(interval);
  }, [workspaceId]);

  const filtered = events.filter(e => {
    if (goalId && e.target !== goalId && e.session_id !== goalId) return false;
    if (filter && !e.event_type.includes(filter) && !e.source.includes(filter)) return false;
    return true;
  });

  const formatTime = (ts: string) => {
    const d = new Date(ts);
    return d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  };

  return (
    <div className="agent-transcript">
      <h4>📜 Agent Transcript</h4>
      <input
        className="transcript-filter"
        placeholder="过滤事件..."
        value={filter}
        onChange={e => setFilter(e.target.value)}
      />
      <div className="transcript-list">
        {filtered.length === 0 ? (
          <div className="transcript-empty">暂无事件</div>
        ) : (
          filtered.map((evt, idx) => (
            <div key={idx} className="transcript-event">
              <span className="transcript-icon">{EVENT_ICONS[evt.event_type] ?? '📌'}</span>
              <span className="transcript-time">{formatTime(evt.timestamp)}</span>
              <span className="transcript-type">{evt.event_type}</span>
              <span className="transcript-source">{evt.source}</span>
              <span className="transcript-detail">
                {typeof evt.detail === 'string' ? evt.detail : JSON.stringify(evt.detail)}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
};

export default AgentTranscript;
