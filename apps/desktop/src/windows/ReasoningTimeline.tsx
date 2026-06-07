import React, { useEffect, useState } from 'react';
import { api, type ChatTurnEvent } from '../ipc/invoke';

interface ReasoningTimelineProps {
  requestId: string;
}

const EVENT_LABEL: Record<string, string> = {
  'model.routed': '模型路由',
  'ooda.phase_changed': 'OODA 阶段',
  'tool_call.proposed': '工具调用',
  'tool_call.executing': '执行中',
  'tool_call.finished': '工具完成',
  'subagent.completed': '子任务完成',
  'goal_task.execution_started': '任务开始',
  'goal_task.result_projected': '结果写回',
  'goal_task.writeback_failed': '写回失败',
};

const EVENT_COLOR: Record<string, string> = {
  'model.routed': 'var(--state-quiet)',
  'ooda.phase_changed': 'var(--state-warn)',
  'tool_call.proposed': 'var(--state-running)',
  'tool_call.finished': 'var(--state-done)',
  'subagent.completed': 'var(--state-done)',
  'goal_task.result_projected': 'var(--state-done)',
  'goal_task.writeback_failed': 'var(--state-error)',
};

export const ReasoningTimeline: React.FC<ReasoningTimelineProps> = ({ requestId }) => {
  const [events, setEvents] = useState<ChatTurnEvent[]>([]);

  useEffect(() => {
    if (!requestId) return;
    const refresh = () => {
      api.getChatTurnEvents(requestId)
        .then(setEvents)
        .catch(console.error);
    };
    refresh();
    const t = window.setInterval(refresh, 3000);
    return () => window.clearInterval(t);
  }, [requestId]);

  if (events.length === 0) {
    return (
      <div style={{ padding: '8px 0', color: 'var(--text-secondary)', fontSize: 12, fontStyle: 'italic' }}>
        暂无推理事件
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
      {events.map(event => {
        const label = EVENT_LABEL[event.event_type] ?? event.event_type;
        const color = EVENT_COLOR[event.event_type] ?? 'var(--text-secondary)';
        const time = new Date(event.created_at).toLocaleTimeString('zh-CN', {
          hour: '2-digit', minute: '2-digit', second: '2-digit',
        });

        return (
          <div key={event.id} style={{
            display: 'flex',
            alignItems: 'flex-start',
            gap: 8,
            padding: '3px 0',
            lineHeight: '24px',
            maxWidth: '60ch',
          }}>
            {/* Timeline dot + line */}
            <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 0, flexShrink: 0 }}>
              <div style={{ width: 7, height: 7, borderRadius: '50%', background: color, marginTop: 8 }} />
            </div>

            {/* Content */}
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                <span style={{ fontSize: 11, color: 'var(--text-secondary)', fontFamily: 'var(--font-mono)', flexShrink: 0 }}>
                  {time}
                </span>
                <span style={{ fontSize: 12, color, fontFamily: 'var(--font-ui)', fontWeight: 500 }}>
                  {label}
                </span>
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
};
