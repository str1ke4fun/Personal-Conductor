import { useState } from 'react';
import type { PlanStep } from '../../ipc/invoke';

interface PlanCardProps {
  title: string;
  steps: PlanStep[];
  status: 'draft' | 'awaiting_approval' | 'approved' | 'rejected' | 'executing';
  writeScope?: string[];
  diffPreview?: string;
  onApprove?: () => void;
  onReject?: () => void;
}

const STATUS_CONFIG: Record<
  PlanCardProps['status'],
  { label: string; icon: string; className: string }
> = {
  draft: { label: 'Draft', icon: '[D]', className: 'draft' },
  awaiting_approval: {
    label: 'Awaiting approval',
    icon: '[!]',
    className: 'awaiting-approval',
  },
  approved: { label: 'Approved', icon: '[OK]', className: 'approved' },
  rejected: { label: 'Rejected', icon: '[X]', className: 'rejected' },
  executing: { label: 'Executing', icon: '[>]', className: 'executing' },
};

export function PlanCard({
  title,
  steps,
  status,
  writeScope = [],
  diffPreview,
  onApprove,
  onReject,
}: PlanCardProps) {
  const [expanded, setExpanded] = useState(true);
  const [busy, setBusy] = useState(false);
  const config = STATUS_CONFIG[status];

  async function handleApprove() {
    if (!onApprove || busy) return;
    setBusy(true);
    try {
      onApprove();
    } finally {
      setBusy(false);
    }
  }

  async function handleReject() {
    if (!onReject || busy) return;
    setBusy(true);
    try {
      onReject();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={`plan-card plan-status-${config.className}`}>
      <div className="plan-header" onClick={() => setExpanded(!expanded)}>
        <span className="plan-icon">[Plan]</span>
        <span className="plan-title">{title}</span>
        <span className="plan-step-count">{steps.length} steps</span>
        <span className={`plan-status-badge plan-badge-${config.className}`}>
          {config.icon} {config.label}
        </span>
        <span className="plan-expand-arrow">{expanded ? '[-]' : '[+]'}</span>
      </div>

      {status === 'awaiting_approval' && (onApprove || onReject) && (
        <div className="plan-approval-bar">
          <span className="plan-approval-message">Approve this plan?</span>
          <div className="plan-approval-actions">
            {onApprove && (
              <button
                type="button"
                className="plan-btn approve"
                onClick={(e) => {
                  e.stopPropagation();
                  void handleApprove();
                }}
                disabled={busy}
              >
                Approve
              </button>
            )}
            {onReject && (
              <button
                type="button"
                className="plan-btn deny"
                onClick={(e) => {
                  e.stopPropagation();
                  void handleReject();
                }}
                disabled={busy}
              >
                Reject
              </button>
            )}
          </div>
        </div>
      )}

      {expanded && (
        <>
          <ol className="plan-steps">
            {steps.map((step, i) => (
              <li key={i} className="plan-step">
                <span className="plan-step-number">{i + 1}</span>
                <div className="plan-step-content">
                  <span className="plan-step-title">{step.title}</span>
                  {step.detail && (
                    <span className="plan-step-detail">{step.detail}</span>
                  )}
                </div>
              </li>
            ))}
          </ol>
          {writeScope.length > 0 && (
            <div className="plan-scope">
              <div className="plan-section-title">Write scope</div>
              <ul className="plan-scope-list">
                {writeScope.map((item, index) => (
                  <li key={`${item}-${index}`}>{item}</li>
                ))}
              </ul>
            </div>
          )}
          {diffPreview && (
            <div className="plan-diff-preview">
              <div className="plan-section-title">Diff preview</div>
              <pre>{diffPreview}</pre>
            </div>
          )}
        </>
      )}
    </div>
  );
}
