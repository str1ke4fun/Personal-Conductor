import { useEffect, useRef, useState } from 'react';
import type { ToolCardStatus } from './useChatSession';
import { normalizeToolId } from './toolIds';

interface ToolUseCardProps {
  toolId: string;         // e.g. "bash.execute", "file.glob"
  input: Record<string, any>;
  status: ToolCardStatus;
  result?: {
    stdout?: string;
    stderr?: string;
    exit_code?: number;
    output?: any;
    error?: string;
  };
  durationMs?: number;
  proposalId?: string;
  onApprove?: (proposalId: string) => void;
  onReject?: (proposalId: string) => void;
}

const TOOL_DISPLAY_NAMES: Record<string, string> = {
  'bash.execute': '执行命令',
  'bash.cancel': '终止命令',
  'file.glob': '搜索文件',
  'file.grep': '搜索内容',
  'file.read': '读取文件',
  'file.write': '写入文件',
  'file.edit': '编辑文件',
  'file.stat': '文件信息',
  'workspace.current': '查看工作区',
  'task.list': '查看任务',
  'task.get': '任务详情',
  'pet.set_avatar': '切换形象',
  'codex.start': '启动 Codex 终端',
  'codex.read_output': '读取终端输出',
  'codex.send_input': '发送终端输入',
  'codex.interrupt': '中断终端',
  'codex.resume': '恢复终端',
  'codex.stop': '停止终端',
};

const TOOL_ICONS: Record<string, string> = {
  'bash': '⚡',
  'file': '📄',
  'task': '📋',
  'pet': '🎭',
  'agent': '🤖',
  'memory': '🧠',
  'office': '📊',
  'codex': '💻',
};

/** Status display config: label, icon, CSS class. */
const STATUS_CONFIG: Record<ToolCardStatus, { label: string; icon: string; className: string }> = {
  pending:           { label: '等待中',     icon: '○',  className: 'pending' },
  running:           { label: '执行中',     icon: '◌',  className: 'running' },
  success:           { label: '已完成',     icon: '✓',  className: 'success' },
  error:             { label: '出错了',     icon: '✗',  className: 'error' },
  awaiting_approval: { label: '需要你批准', icon: '⏸', className: 'awaiting-approval' },
  approved:          { label: '已批准',     icon: '▶',  className: 'approved' },
  blocked:           { label: '被阻塞',     icon: '⊘',  className: 'blocked' },
  cancelled:         { label: '已取消',     icon: '⊘',  className: 'cancelled' },
  denied:            { label: '已拒绝',     icon: '✗',  className: 'denied' },
  retryable:         { label: '可重试',     icon: '↻',  className: 'retryable' },
  timeout:           { label: '已超时',     icon: '⏰',  className: 'timeout' },
};

function getToolIcon(toolId: string): string {
  const prefix = normalizeToolId(toolId).split('.')[0];
  return TOOL_ICONS[prefix] || '🔧';
}

function getToolDisplayName(toolId: string): string {
  const normalized = normalizeToolId(toolId);
  if (TOOL_DISPLAY_NAMES[normalized]) return TOOL_DISPLAY_NAMES[normalized];
  const parts = normalized.split('.');
  const leaf = parts[parts.length - 1] || toolId;
  const readableLeaf = leaf.replace(/[_-]+/g, ' ');
  if (toolId.startsWith('mcp.')) return `MCP 工具：${readableLeaf}`;
  return readableLeaf;
}

function truncate(value: string, max = 120): string {
  return value.length > max ? `${value.slice(0, max)}...` : value;
}

function formatToolSummary(toolId: string, input: Record<string, any>): string {
  const normalized = normalizeToolId(toolId);
  if (normalized === 'bash.execute' && typeof input.command === 'string') {
    return truncate(input.command, 180);
  }
  if (normalized === 'file.grep') {
    const pattern = input.pattern ?? input.query ?? '';
    const path = input.path ?? input.glob ?? input.cwd ?? '';
    return truncate([pattern && `pattern=${pattern}`, path && `path=${path}`].filter(Boolean).join(' '));
  }
  if (normalized.startsWith('file.')) {
    const path = input.path ?? input.file_path ?? input.target ?? '';
    if (path) return truncate(String(path));
  }
  if (normalized === 'workspace.current') {
    return '读取当前工作区路径';
  }
  const firstValue = Object.values(input).find((value) => typeof value === 'string' && value.trim());
  if (typeof firstValue === 'string') return truncate(firstValue);
  return '';
}

/** Whether the status indicates the tool is actively processing or waiting. */
function isActive(status: ToolCardStatus): boolean {
  return status === 'running' || status === 'pending' || status === 'approved' || status === 'awaiting_approval';
}

function formatElapsed(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return min > 0 ? `${min}m ${sec}s` : `${sec}s`;
}

/** Live elapsed timer for long-running tools like agent.start. */
function LiveTimer({ status, durationMs }: { status: ToolCardStatus; durationMs?: number }) {
  const [elapsed, setElapsed] = useState(0);
  const startRef = useRef(Date.now());

  useEffect(() => {
    if (!isActive(status)) return;
    const interval = setInterval(() => {
      setElapsed(Date.now() - startRef.current);
    }, 1000);
    return () => clearInterval(interval);
  }, [status]);

  if (status === 'running' || status === 'pending') {
    return <span className="tool-duration tool-duration-live">已运行 {formatElapsed(elapsed)}</span>;
  }
  if (durationMs !== undefined) {
    const label = status === 'success' ? '耗时' : status === 'error' ? '耗时' : '用时';
    const statusLabel = status === 'success' ? 'succeeded' : status === 'error' ? 'failed' : '';
    return (
      <span className="tool-duration">
        {label} {durationMs < 1000 ? `${durationMs}ms` : formatElapsed(durationMs)}
        {statusLabel && ` · ${statusLabel}`}
      </span>
    );
  }
  return null;
}

export function ToolUseCard({
  toolId,
  input,
  status,
  result,
  durationMs,
  proposalId,
  onApprove,
  onReject,
}: ToolUseCardProps) {
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState(false);
  const normalizedToolId = normalizeToolId(toolId);
  const icon = getToolIcon(toolId);
  const summary = formatToolSummary(toolId, input);
  const config = STATUS_CONFIG[status];
  const verb = isActive(status) ? 'Running' : 'Ran';

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

  return (
    <div className={`tool-use-card tool-status-${config.className}`}>
      <div className="tool-header" onClick={() => setExpanded(!expanded)}>
        <span className="tool-icon">{icon}</span>
        <span className="tool-name">{verb} {getToolDisplayName(toolId)}</span>
        {summary && (
          <span className="tool-command-preview">{summary}</span>
        )}
        <span className="tool-status">
          {status === 'running' && <span className="status-spinner" />}
          {status === 'success' && <span className="status-check">&#10003;</span>}
          {status === 'error' && <span className="status-error">&#10007;</span>}
          {status === 'awaiting_approval' && <span className="status-awaiting">⏸</span>}
          {status === 'blocked' && <span className="status-blocked">⊘</span>}
          {status === 'cancelled' && <span className="status-cancelled">⊘</span>}
          {status === 'denied' && <span className="status-denied">&#10007;</span>}
          {status === 'retryable' && <span className="status-retryable">↻</span>}
          {status === 'timeout' && <span className="status-timeout">⏰</span>}
          {status === 'approved' && <span className="status-approved">▶</span>}
          {status === 'pending' && <span className="status-pending-icon">○</span>}
        </span>
        <span className="tool-status-label">{config.label}</span>
        {normalizedToolId === 'agent.start' ? (
          <LiveTimer status={status} durationMs={durationMs} />
        ) : durationMs !== undefined ? (
          <span className="tool-duration">{durationMs < 1000 ? `${durationMs}ms` : `${(durationMs / 1000).toFixed(1)}s`}</span>
        ) : null}
        <span className="tool-expand-arrow">{expanded ? '▼' : '▶'}</span>
      </div>

      {/* Approval actions inline for awaiting_approval status */}
      {status === 'awaiting_approval' && proposalId && (onApprove || onReject) && (
        <div className="tool-approval-bar">
          <span className="tool-approval-message">这个操作需要你的批准才能继续</span>
          <div className="tool-approval-actions">
            {onApprove && (
              <button
                type="button"
                className="tool-approval-btn approve"
                onClick={(e) => { e.stopPropagation(); void handleApprove(); }}
                disabled={busy}
              >
                批准
              </button>
            )}
            {onReject && (
              <button
                type="button"
                className="tool-approval-btn deny"
                onClick={(e) => { e.stopPropagation(); void handleReject(); }}
                disabled={busy}
              >
                拒绝
              </button>
            )}
          </div>
        </div>
      )}

      {/* Retry hint for retryable status */}
      {status === 'retryable' && (
        <div className="tool-retry-hint">
          <span>操作失败，可以重试</span>
        </div>
      )}

      {expanded && (
        <>
          <div className="tool-input">
            <pre>{JSON.stringify(input, null, 2)}</pre>
          </div>
          {result && (
            <div className="tool-result">
              {result.stdout && (
                <div className="tool-stdout">
                  <div className="tool-output-label">stdout</div>
                  <pre>{result.stdout.length > 5000 ? result.stdout.slice(0, 5000) + '\n... (truncated)' : result.stdout}</pre>
                </div>
              )}
              {result.stderr && (
                <div className="tool-stderr">
                  <div className="tool-output-label">stderr</div>
                  <pre>{result.stderr}</pre>
                </div>
              )}
              {result.exit_code !== undefined && result.exit_code !== 0 && (
                <div className="tool-exit-code">exit code: {result.exit_code}</div>
              )}
              {result.error && (
                <div className="tool-error">{result.error}</div>
              )}
              {result.output !== undefined && (
                <div className="tool-stdout">
                  <div className="tool-output-label">output</div>
                  <pre>{typeof result.output === 'string' ? result.output : JSON.stringify(result.output, null, 2)}</pre>
                </div>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}
