import React, { useEffect, useState } from 'react';
import { getGoalCycles, type GoalCycle } from '../ipc/invoke';

interface CycleIndicatorProps {
  goalId: string;
}

const PHASE_LABELS: Record<string, string> = {
  observing: 'Observe',
  orienting: 'Orient',
  deciding: 'Decide',
  dispatching: 'Dispatch',
  executing: 'Act',
  reviewing: 'Review',
  summarizing: 'Done',
  completed: 'Done',
  failed: 'Failed',
  blocked: 'Blocked',
  cancelled: 'Cancelled',
};

const PHASE_COLOR: Record<string, string> = {
  observing: 'var(--state-quiet)',
  orienting: 'var(--state-quiet)',
  deciding: 'var(--state-warn)',
  dispatching: 'var(--state-warn)',
  executing: 'var(--state-running)',
  reviewing: 'var(--state-done)',
  summarizing: 'var(--state-done)',
  completed: 'var(--state-done)',
  failed: 'var(--state-error)',
  blocked: 'var(--state-error)',
  cancelled: 'var(--state-quiet)',
};

export const CycleIndicator: React.FC<CycleIndicatorProps> = ({ goalId }) => {
  const [latestCycle, setLatestCycle] = useState<GoalCycle | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [allCycles, setAllCycles] = useState<GoalCycle[]>([]);

  useEffect(() => {
    const refresh = () => {
      getGoalCycles(goalId).then((cycles) => {
        setAllCycles(cycles);
        if (cycles.length > 0) setLatestCycle(cycles[0]);
      }).catch(console.error);
    };
    refresh();
    const t = window.setInterval(refresh, 4000);
    return () => window.clearInterval(t);
  }, [goalId]);

  if (!latestCycle) return null;

  const statusColor = PHASE_COLOR[latestCycle.status] ?? 'var(--text-secondary)';
  const label = PHASE_LABELS[latestCycle.status] ?? latestCycle.status;

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: '4px 10px',
        background: 'var(--bg-surface)',
        borderRadius: 6,
        border: '0.5px solid var(--border-hair)',
        cursor: 'pointer',
        userSelect: 'none',
        minHeight: 32,
        transition: 'all 200ms ease-out',
      }}
      onClick={() => setExpanded(e => !e)}
    >
      {/* Status dot */}
      <div style={{
        width: 8, height: 8, borderRadius: '50%',
        background: statusColor,
        flexShrink: 0,
        boxShadow: latestCycle.status === 'executing' ? `0 0 6px ${statusColor}` : 'none',
      }} />

      {/* Cycle label */}
      <span style={{ fontSize: 12, color: 'var(--text-secondary)', fontFamily: 'var(--font-ui)' }}>
        Cycle #{latestCycle.cycle_no}
      </span>
      <span style={{ fontSize: 12, color: statusColor, fontFamily: 'var(--font-ui)', fontWeight: 500 }}>
        {label}
      </span>

      {/* Expand chevron */}
      <span style={{ fontSize: 10, color: 'var(--text-secondary)', marginLeft: 'auto' }}>
        {expanded ? '▲' : '▼'}
      </span>

      {/* Expanded: all cycles timeline */}
      {expanded && (
        <div
          style={{
            position: 'absolute',
            top: '100%',
            left: 0,
            right: 0,
            marginTop: 4,
            background: 'var(--bg-elevated)',
            border: '0.5px solid var(--border-hair)',
            borderRadius: 6,
            padding: '8px 0',
            zIndex: 100,
          }}
          onClick={e => e.stopPropagation()}
        >
          {allCycles.map(cycle => (
            <div key={cycle.id} style={{
              display: 'flex', alignItems: 'center', gap: 8,
              padding: '4px 12px', fontSize: 12,
            }}>
              <div style={{
                width: 6, height: 6, borderRadius: '50%',
                background: PHASE_COLOR[cycle.status] ?? 'var(--text-secondary)',
                flexShrink: 0,
              }} />
              <span style={{ color: 'var(--text-secondary)' }}>#{cycle.cycle_no}</span>
              <span style={{ color: PHASE_COLOR[cycle.status] ?? 'var(--text-secondary)' }}>
                {PHASE_LABELS[cycle.status] ?? cycle.status}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
