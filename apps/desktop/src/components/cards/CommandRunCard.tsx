import { useState } from 'react';
import type { ToolCardStatus } from '../../windows/useChatSession';

interface CommandRunCardProps {
  command: string;
  cwd?: string;
  status: ToolCardStatus;
  stdout?: string;
  stderr?: string;
  exitCode?: number;
  durationMs?: number;
  onCancel?: () => void;
}

const STATUS_DISPLAY: Record<
  string,
  { label: string; icon: string; className: string }
> = {
  pending: { label: 'Pending', icon: '[ ]', className: 'pending' },
  running: { label: 'Running', icon: '[>]', className: 'running' },
  success: { label: 'Success', icon: '[OK]', className: 'success' },
  error: { label: 'Error', icon: '[X]', className: 'error' },
  cancelled: { label: 'Cancelled', icon: '[-]', className: 'cancelled' },
  timeout: { label: 'Timed out', icon: '[!]', className: 'timeout' },
};

export function CommandRunCard({
  command,
  cwd,
  status,
  stdout,
  stderr,
  exitCode,
  durationMs,
  onCancel,
}: CommandRunCardProps) {
  const [expanded, setExpanded] = useState(status === 'running');
  const display = STATUS_DISPLAY[status] ?? STATUS_DISPLAY.pending;
  const isRunning = status === 'running' || status === 'pending';
  const hasOutput = Boolean(stdout) || Boolean(stderr);

  const formatDuration = (ms: number) =>
    ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;

  return (
    <div className={`cmd-card cmd-status-${display.className}`}>
      <div className="cmd-header" onClick={() => setExpanded(!expanded)}>
        <span className="cmd-icon">{display.icon}</span>
        <span className="cmd-command">{command}</span>
        <span className="cmd-status-badge">
          {isRunning && <span className="status-spinner" />}
          <span>{display.label}</span>
        </span>
        {durationMs !== undefined && (
          <span className="cmd-duration">{formatDuration(durationMs)}</span>
        )}
        {hasOutput && (
          <span className="cmd-expand-arrow">{expanded ? '[-]' : '[+]'}</span>
        )}
      </div>

      {cwd && <div className="cmd-cwd">cwd: {cwd}</div>}

      {isRunning && onCancel && (
        <div className="cmd-actions">
          <button
            type="button"
            className="cmd-btn cancel"
            onClick={(e) => {
              e.stopPropagation();
              onCancel();
            }}
          >
            Stop
          </button>
        </div>
      )}

      {expanded && hasOutput && (
        <div className="cmd-output">
          {stdout && (
            <div className="cmd-stdout">
              <div className="cmd-output-label">stdout</div>
              <pre>
                {stdout.length > 5000
                  ? `${stdout.slice(0, 5000)}\n... (truncated)`
                  : stdout}
              </pre>
            </div>
          )}
          {stderr && (
            <div className="cmd-stderr">
              <div className="cmd-output-label">stderr</div>
              <pre>{stderr}</pre>
            </div>
          )}
          {exitCode !== undefined && exitCode !== 0 && (
            <div className="cmd-exit-code">exit code: {exitCode}</div>
          )}
        </div>
      )}
    </div>
  );
}
