import React from 'react';

/**
 * Shared "deck" UI primitives for the right-hand panels.
 *
 * Both the goal console (goal mode) and the activity drawer (chat mode) compose
 * these so the two modes read as one product surface: the same status pills,
 * the same glowing live dot, the same section chrome and stat strip.
 */

export type SignalTone =
  | 'running'
  | 'done'
  | 'warn'
  | 'error'
  | 'quiet'
  | 'info';

/** Map any backend status string onto one of our six signal tones. */
export function toneForStatus(status: string | undefined): SignalTone {
  switch (status) {
    case 'running':
    case 'in_progress':
    case 'working':
    case 'executing':
      return 'running';
    case 'accepted':
    case 'completed':
    case 'passed':
    case 'succeeded':
    case 'review_ready':
    case 'reviewing':
      return 'done';
    case 'awaiting_plan_approval':
    case 'awaiting_review':
    case 'rework_required':
    case 'awaiting_permission':
    case 'awaiting_input':
    case 'pending':
    case 'proposed':
    case 'queued':
    case 'claimed':
      return 'warn';
    case 'blocked':
    case 'failed':
    case 'cancelled':
    case 'rejected':
    case 'stale':
      return 'error';
    case 'planning':
    case 'observing':
    case 'orienting':
    case 'deciding':
    case 'dispatching':
      return 'info';
    default:
      return 'quiet';
  }
}

interface StatusPillProps {
  tone: SignalTone;
  children: React.ReactNode;
  pulse?: boolean;
  title?: string;
}

export const StatusPill: React.FC<StatusPillProps> = ({ tone, children, pulse, title }) => (
  <span className={`deck-pill deck-pill--${tone} ${pulse ? 'deck-pill--pulse' : ''}`} title={title}>
    <span className="deck-pill-dot" />
    {children}
  </span>
);

interface LiveDotProps {
  tone: SignalTone;
  pulse?: boolean;
  size?: number;
}

export const LiveDot: React.FC<LiveDotProps> = ({ tone, pulse, size = 8 }) => (
  <span
    className={`deck-dot deck-dot--${tone} ${pulse ? 'deck-dot--pulse' : ''}`}
    style={{ width: size, height: size }}
  />
);

interface PanelSectionProps {
  title: React.ReactNode;
  count?: number | string;
  action?: React.ReactNode;
  children: React.ReactNode;
  /** subtle accent stripe color tone for the section header */
  tone?: SignalTone;
}

export const PanelSection: React.FC<PanelSectionProps> = ({
  title,
  count,
  action,
  children,
  tone,
}) => (
  <section className={`deck-section ${tone ? `deck-section--${tone}` : ''}`}>
    <header className="deck-section-head">
      <span className="deck-section-title">{title}</span>
      {count !== undefined && <span className="deck-section-count">{count}</span>}
      {action && <span className="deck-section-action">{action}</span>}
    </header>
    <div className="deck-section-body">{children}</div>
  </section>
);

export interface StatStripItem {
  label: string;
  value: number | string;
  tone?: SignalTone;
}

export const StatStrip: React.FC<{ items: StatStripItem[] }> = ({ items }) => (
  <div className="deck-statstrip">
    {items.map((item) => (
      <div key={item.label} className={`deck-stat ${item.tone ? `deck-stat--${item.tone}` : ''}`}>
        <span className="deck-stat-value">{item.value}</span>
        <span className="deck-stat-label">{item.label}</span>
      </div>
    ))}
  </div>
);

interface ProgressMeterProps {
  /** ordered segments rendered left-to-right; widths are proportional to value */
  segments: Array<{ tone: SignalTone; value: number; label?: string }>;
  total?: number;
}

export const ProgressMeter: React.FC<ProgressMeterProps> = ({ segments, total }) => {
  const sum = total ?? segments.reduce((acc, s) => acc + s.value, 0);
  if (sum <= 0) {
    return <div className="deck-meter deck-meter--empty" />;
  }
  return (
    <div className="deck-meter" role="img" aria-label="进度">
      {segments
        .filter((s) => s.value > 0)
        .map((s, i) => (
          <span
            key={`${s.tone}-${i}`}
            className={`deck-meter-seg deck-meter-seg--${s.tone}`}
            style={{ flexGrow: s.value }}
            title={s.label ? `${s.label}: ${s.value}` : String(s.value)}
          />
        ))}
    </div>
  );
};

export const EmptyHint: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className="deck-empty">{children}</div>
);
