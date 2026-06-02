import { useState } from 'react';
import type { GoalSeed } from '../../ipc/invoke';

interface CapabilityRequestCardProps {
  reason: string;
  suggestedMode: string;
  goalSeed: GoalSeed;
  canCreateGoal: boolean;
  onCreateGoal?: (goalSeed: GoalSeed) => Promise<void> | void;
}

function formatSuggestedMode(suggestedMode: string): string {
  if (suggestedMode === 'long') {
    return 'Goal 持续执行';
  }
  return suggestedMode;
}

export function CapabilityRequestCard({
  reason,
  suggestedMode,
  goalSeed,
  canCreateGoal,
  onCreateGoal,
}: CapabilityRequestCardProps) {
  const [dismissed, setDismissed] = useState(false);

  if (dismissed) {
    return null;
  }

  return (
    <div className="capability-request-card">
      <div className="capability-request-header">
        <span className="capability-request-badge">建议升级</span>
        <span className="capability-request-mode">
          {formatSuggestedMode(suggestedMode)}
        </span>
      </div>
      <div className="capability-request-title">{goalSeed.title}</div>
      <div className="capability-request-reason">{reason}</div>
      <pre className="capability-request-preview">{goalSeed.objective}</pre>
      <div className="capability-request-actions">
        <button
          type="button"
          className="composer-btn send"
          disabled={!canCreateGoal}
          onClick={() => void onCreateGoal?.(goalSeed)}
        >
          转为 Goal
        </button>
        <button
          type="button"
          className="composer-btn retry"
          onClick={() => setDismissed(true)}
        >
          保持聊天
        </button>
        {!canCreateGoal && (
          <span className="capability-request-hint">需先关联工作区。</span>
        )}
      </div>
    </div>
  );
}
