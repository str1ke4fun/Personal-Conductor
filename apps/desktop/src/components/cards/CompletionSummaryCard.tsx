import type { CompletionStep } from '../../ipc/invoke';

interface CompletionSummaryCardProps {
  title: string;
  steps?: CompletionStep[];
  summary?: string;
  durationMs?: number;
}

type StepStatus = 'done' | 'skipped' | 'failed';

const STEP_GLYPH: Record<StepStatus, string> = {
  done: '✓',
  skipped: '–',
  failed: '✕',
};

function normalizeStatus(status: CompletionStep['status']): StepStatus {
  return status === 'skipped' || status === 'failed' ? status : 'done';
}

export function CompletionSummaryCard({
  title,
  steps,
  summary,
  durationMs,
}: CompletionSummaryCardProps) {
  const formatDuration = (ms: number) =>
    ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;

  const hasFailure = steps?.some((s) => s.status === 'failed') ?? false;
  const cardTone = hasFailure ? 'has-failure' : 'all-clear';

  return (
    <div className={`completion-card completion-card--${cardTone}`}>
      <div className="completion-header">
        <span className="completion-badge" aria-hidden>
          {hasFailure ? '!' : '✓'}
        </span>
        <span className="completion-title">{title}</span>
        {durationMs !== undefined && (
          <span className="completion-duration">{formatDuration(durationMs)}</span>
        )}
      </div>

      {summary && <div className="completion-summary">{summary}</div>}

      {steps && steps.length > 0 && (
        <ul className="completion-steps">
          {steps.map((step, i) => {
            const status = normalizeStatus(step.status);
            return (
              <li key={i} className={`completion-step completion-step-${status}`}>
                <span className="completion-step-icon" aria-hidden>
                  {STEP_GLYPH[status]}
                </span>
                <span className="completion-step-label">{step.label}</span>
                {step.detail && (
                  <span className="completion-step-detail">{step.detail}</span>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
