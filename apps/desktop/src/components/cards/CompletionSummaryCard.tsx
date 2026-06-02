import type { CompletionStep } from '../../ipc/invoke';

interface CompletionSummaryCardProps {
  title: string;
  steps?: CompletionStep[];
  summary?: string;
  durationMs?: number;
}

const STEP_ICONS: Record<'done' | 'skipped' | 'failed', string> = {
  done: '完成',
  skipped: '跳过',
  failed: '异常',
};

function stepIconForStatus(status: CompletionStep['status']): string {
  if (status === 'done' || status === 'skipped' || status === 'failed') {
    return STEP_ICONS[status];
  }
  return '完成';
}

export function CompletionSummaryCard({
  title,
  steps,
  summary,
  durationMs,
}: CompletionSummaryCardProps) {
  const formatDuration = (ms: number) =>
    ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;

  return (
    <div className="completion-card">
      <div className="completion-header">
        <span className="completion-icon">完成</span>
        <span className="completion-title">{title}</span>
        {durationMs !== undefined && (
          <span className="completion-duration">{formatDuration(durationMs)}</span>
        )}
      </div>

      {summary && <div className="completion-summary">{summary}</div>}

      {steps && steps.length > 0 && (
        <ul className="completion-steps">
          {steps.map((step, i) => (
            <li
              key={i}
              className={`completion-step completion-step-${step.status}`}
            >
              <span className="completion-step-icon">{stepIconForStatus(step.status)}</span>
              <span className="completion-step-label">{step.label}</span>
              {step.detail && (
                <span className="completion-step-detail">{step.detail}</span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
