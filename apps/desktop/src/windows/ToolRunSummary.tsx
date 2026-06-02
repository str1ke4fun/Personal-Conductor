import { useState } from 'react';
import type { StreamToolState, ToolCardStatus } from './useChatSession';
import { ToolUseCard } from './ToolUseCard';
import { normalizeToolId } from './toolIds';

interface ToolRunSummaryProps {
  toolStates: StreamToolState[];
  mode: 'live' | 'persisted';
  onApprove?: (proposalId: string) => void;
  onReject?: (proposalId: string) => void;
}

interface ToolGroup {
  key: string;
  toolName: string;
  displayName: string;
  items: StreamToolState[];
  count: number;
  latestStatus: ToolCardStatus;
  totalDurationMs: number;
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
  'memory.set': '写入记忆',
  'memory.get': '读取记忆',
  'memory.search': '搜索记忆',
  'agent.start': '启动子代理',
  'codex.start': '启动终端',
};

const PROMINENT_TOOLS = new Set([
  'bash.execute',
  'file.write',
  'file.edit',
  'codex.start',
  'agent.start',
]);

const PROMINENT_STATUSES: ToolCardStatus[] = [
  'awaiting_approval',
  'blocked',
  'denied',
];

const STATUS_ICONS: Record<ToolCardStatus, string> = {
  pending: '⏳',
  running: '⚡',
  success: '✅',
  error: '❌',
  awaiting_approval: '\u{1F512}',
  approved: '✅',
  blocked: '⛔',
  cancelled: '\u{1F6AB}',
  denied: '❌',
  retryable: '\u{1F504}',
  timeout: '⏰',
};

function isProminent(state: StreamToolState): boolean {
  return PROMINENT_TOOLS.has(normalizeToolId(state.tool_name)) || PROMINENT_STATUSES.includes(state.status);
}

function inputSummary(input?: Record<string, any>): string {
  if (!input) return '';
  const values = Object.values(input);
  if (values.length === 0) return '';
  const first = String(values[0]);
  return first.length > 40 ? first.slice(0, 40) + '...' : first;
}

function groupTools(toolStates: StreamToolState[]): ToolGroup[] {
  const groups = new Map<string, ToolGroup>();

  for (const state of toolStates) {
    const normalizedToolName = normalizeToolId(state.tool_name);
    if (isProminent(state)) {
      // Prominent tools get their own group (never aggregated)
      groups.set(state.tool_use_id, {
        key: state.tool_use_id,
        toolName: state.tool_name,
        displayName: TOOL_DISPLAY_NAMES[normalizedToolName] ?? normalizedToolName,
        items: [state],
        count: 1,
        latestStatus: state.status,
        totalDurationMs: state.duration_ms ?? 0,
      });
      continue;
    }

    const groupKey = normalizedToolName;
    const existing = groups.get(groupKey);
    if (existing) {
      existing.items.push(state);
      existing.count++;
      existing.latestStatus = state.status;
      existing.totalDurationMs += state.duration_ms ?? 0;
    } else {
      groups.set(groupKey, {
        key: groupKey,
        toolName: state.tool_name,
        displayName: TOOL_DISPLAY_NAMES[normalizedToolName] ?? normalizedToolName,
        items: [state],
        count: 1,
        latestStatus: state.status,
        totalDurationMs: state.duration_ms ?? 0,
      });
    }
  }

  return Array.from(groups.values());
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60000)}m ${Math.round((ms % 60000) / 1000)}s`;
}

export function ToolRunSummary({ toolStates, mode, onApprove, onReject }: ToolRunSummaryProps) {
  const [expanded, setExpanded] = useState(false);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

  const groups = groupTools(toolStates);
  const visibleGroups = expanded ? groups : groups.slice(-5);
  const hiddenCount = groups.length - visibleGroups.length;

  const toggleGroup = (key: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  if (groups.length === 0) return null;

  return (
    <div className="tool-run-summary">
      <div className="tool-run-summary-bar">
        <span className="tool-run-summary-count">
          {groups.length} {groups.length === 1 ? 'tool' : 'tools'}
        </span>
        {hiddenCount > 0 && (
          <button
            className="tool-run-summary-toggle"
            onClick={() => setExpanded(!expanded)}
          >
            {expanded ? '收起' : `展开全部 (+${hiddenCount})`}
          </button>
        )}
      </div>
      <div className="tool-run-summary-list">
        {visibleGroups.map((group) => (
          <div key={group.key} className="tool-run-row">
            <div
              className="tool-run-row-header"
              onClick={() => group.count > 1 && toggleGroup(group.key)}
              style={{ cursor: group.count > 1 ? 'pointer' : 'default' }}
            >
              <span className="tool-run-status-icon">
                {STATUS_ICONS[group.latestStatus]}
              </span>
              <span className="tool-run-name">{group.displayName}</span>
              {group.count > 1 && (
                <span className="tool-run-count-badge">x{group.count}</span>
              )}
              {group.items[0] && (
                <span className="tool-run-input-hint">
                  {inputSummary(group.items[0].input)}
                </span>
              )}
              {group.totalDurationMs > 0 && (
                <span className="tool-run-duration">
                  {formatDuration(group.totalDurationMs)}
                </span>
              )}
            </div>
            {expandedGroups.has(group.key) && group.items.length > 1 && (
              <div className="tool-run-row-detail">
                {group.items.map((item) => (
                  <ToolUseCard
                    key={item.tool_use_id}
                    toolId={item.tool_name}
                    input={item.input ?? {}}
                    status={item.status}
                    result={item.result}
                    durationMs={item.duration_ms}
                    proposalId={item.proposal_id}
                    onApprove={onApprove}
                    onReject={onReject}
                  />
                ))}
              </div>
            )}
            {group.count === 1 && expandedGroups.has(group.key) && (
              <div className="tool-run-row-detail">
                <ToolUseCard
                  key={group.items[0].tool_use_id}
                  toolId={group.items[0].tool_name}
                  input={group.items[0].input ?? {}}
                  status={group.items[0].status}
                  result={group.items[0].result}
                  durationMs={group.items[0].duration_ms}
                  proposalId={group.items[0].proposal_id}
                  onApprove={onApprove}
                  onReject={onReject}
                />
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
