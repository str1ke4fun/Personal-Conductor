import React, { useEffect, useState } from 'react';
import { getGoalCycles, type GoalCycle } from '../ipc/invoke';

interface OodaTimelineProps {
  goalId: string;
}

const PHASE_LABELS: Record<string, string> = {
  observing: 'Observe',
  orienting: 'Orient',
  deciding: 'Decide',
  dispatching: 'Dispatch',
  executing: 'Act',
  reviewing: 'Review',
  summarizing: 'Summarize',
  completed: '✅ Done',
  failed: '❌ Failed',
  blocked: '🚫 Blocked',
  cancelled: '⏹ Cancelled',
};

const PHASE_ORDER = ['observing', 'orienting', 'deciding', 'dispatching', 'executing', 'reviewing', 'summarizing'];

export const OodaTimeline: React.FC<OodaTimelineProps> = ({ goalId }) => {
  const [cycles, setCycles] = useState<GoalCycle[]>([]);

  useEffect(() => {
    const refresh = () => {
      getGoalCycles(goalId).then(setCycles).catch(console.error);
    };
    refresh();
    const interval = window.setInterval(refresh, 5000);
    return () => window.clearInterval(interval);
  }, [goalId]);

  const getPhaseIndex = (status: string) => {
    const idx = PHASE_ORDER.indexOf(status);
    return idx >= 0 ? idx : PHASE_ORDER.length;
  };

  return (
    <div className="ooda-timeline">
      <h4>🔄 OODA Timeline</h4>
      {cycles.length === 0 ? (
        <div className="ooda-empty">暂无 Cycle</div>
      ) : (
        <div className="ooda-cycles">
          {cycles.map(cycle => (
            <div key={cycle.id} className="ooda-cycle">
              <div className="ooda-cycle-header">
                <span className="ooda-cycle-no">Cycle #{cycle.cycle_no}</span>
                <span className={`ooda-cycle-status ${cycle.status}`}>
                  {PHASE_LABELS[cycle.status] ?? cycle.status}
                </span>
              </div>
              <div className="ooda-phases">
                {PHASE_ORDER.map((phase, idx) => {
                  const currentIdx = getPhaseIndex(cycle.status);
                  const isActive = idx === currentIdx;
                  const isDone = idx < currentIdx;
                  return (
                    <div
                      key={phase}
                      className={`ooda-phase ${isActive ? 'active' : ''} ${isDone ? 'done' : ''}`}
                    >
                      <div className="ooda-phase-dot" />
                      <span className="ooda-phase-label">{PHASE_LABELS[phase]}</span>
                    </div>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

export default OodaTimeline;
