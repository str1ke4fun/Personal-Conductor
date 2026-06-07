import { useState } from 'react';
import type { ToolCardStatus } from '../../windows/useChatSession';

interface PermissionCardProps {
  toolName: string;
  summary: string;
  detail?: string;
  status: ToolCardStatus;
  proposalId?: string;
  riskLevel?: string;
  onApprove?: (proposalId: string) => void;
  onReject?: (proposalId: string) => void;
  onApproveOnce?: (proposalId: string) => void;
}

export function PermissionCard({
  toolName,
  summary,
  detail,
  status,
  proposalId,
  riskLevel,
  onApprove,
  onReject,
  onApproveOnce,
}: PermissionCardProps) {
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState(false);

  const isPending = status === 'awaiting_approval';
  const isDenied = status === 'denied';
  const isApproved = status === 'approved' || status === 'success';
  const isBlocked = status === 'blocked';

  const riskColor =
    riskLevel === 'high' ? 'high' : riskLevel === 'medium' ? 'medium' : 'low';

  async function handleApprove() {
    if (!proposalId || !onApprove || busy) return;
    setBusy(true);
    try {
      onApprove(proposalId);
    } finally {
      setBusy(false);
    }
  }

  async function handleReject() {
    if (!proposalId || !onReject || busy) return;
    setBusy(true);
    try {
      onReject(proposalId);
    } finally {
      setBusy(false);
    }
  }

  async function handleApproveOnce() {
    if (!proposalId || !onApproveOnce || busy) return;
    setBusy(true);
    try {
      onApproveOnce(proposalId);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className={`permission-card perm-status-${
        isPending
          ? 'awaiting'
          : isDenied
            ? 'denied'
            : isApproved
              ? 'approved'
              : isBlocked
                ? 'blocked'
                : 'default'
      }`}
    >
      <div className="permission-header" onClick={() => setExpanded(!expanded)}>
        <span className="permission-icon">
          {isPending ? '⏸' : isDenied ? '✗' : isApproved ? '✓' : '⊘'}
        </span>
        <span className="permission-tool">{toolName}</span>
        {riskLevel && (
          <span className={`permission-risk risk-${riskColor}`}>{riskLevel}</span>
        )}
        <span className="permission-summary">{summary}</span>
        {isPending && (
          <span className="permission-awaiting-badge">需要批准</span>
        )}
        {isDenied && <span className="permission-denied-badge">已拒绝</span>}
        <span className="permission-expand">{expanded ? '▼' : '▶'}</span>
      </div>

      {isPending && proposalId && (onApprove || onReject) && (
        <div className="permission-actions">
          {onApprove && (
            <button
              type="button"
              className="perm-btn approve"
              onClick={(e) => {
                e.stopPropagation();
                void handleApprove();
              }}
              disabled={busy}
            >
              批准
            </button>
          )}
          {onApproveOnce && (
            <button
              type="button"
              className="perm-btn once"
              onClick={(e) => {
                e.stopPropagation();
                void handleApproveOnce();
              }}
              disabled={busy}
            >
              仅此一次
            </button>
          )}
          {onReject && (
            <button
              type="button"
              className="perm-btn deny"
              onClick={(e) => {
                e.stopPropagation();
                void handleReject();
              }}
              disabled={busy}
            >
              拒绝
            </button>
          )}
        </div>
      )}

      {expanded && detail && (
        <div className="permission-detail">
          <pre>{detail}</pre>
        </div>
      )}
    </div>
  );
}
