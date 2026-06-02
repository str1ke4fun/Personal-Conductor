import { useEffect, useMemo, useState } from 'react';
import { api, MemoryEntry } from '../ipc/invoke';

interface MemoryPanelProps {
  standalone?: boolean;
}

function formatTime(isoString: string): string {
  const date = new Date(isoString);
  return date.toLocaleString([], {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function truncateValue(value: string, maxLen = 120): string {
  if (value.length <= maxLen) return value;
  return value.slice(0, maxLen) + '...';
}

function getSourceLabel(source: string): string {
  const labels: Record<string, string> = {
    user_confirmed: '用户确认',
    inferred: '推断',
    tool: '工具',
    summary: '摘要',
  };
  return labels[source] || source;
}

function getStatusLabel(status: string): { label: string; className: string } {
  switch (status) {
    case 'active':
      return { label: '活跃', className: 'memory-status-active' };
    case 'candidate':
      return { label: '候选', className: 'memory-status-candidate' };
    case 'archived':
      return { label: '已归档', className: 'memory-status-archived' };
    case 'quarantined':
      return { label: '隔离', className: 'memory-status-quarantined' };
    case 'forgotten':
      return { label: '已遗忘', className: 'memory-status-forgotten' };
    default:
      return { label: status, className: '' };
  }
}

export function MemoryPanel({ standalone = false }: MemoryPanelProps) {
  const [entries, setEntries] = useState<MemoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [filterCategory, setFilterCategory] = useState<string>('');
  const [filterStatus, setFilterStatus] = useState<string>('');
  const [rebuilding, setRebuilding] = useState(false);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  async function refresh() {
    setLoading(true);
    try {
      const category = filterCategory || null;
      const status = filterStatus || null;
      const data = await api.memoryList(category, status);
      setEntries(data);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  }

  async function handleForget(id: string) {
    try {
      await api.memoryForget(id);
      await refresh();
    } catch {
      // ignore
    }
  }

  async function handleArchive(id: string) {
    try {
      await api.memoryUpdateStatus(id, 'archived');
      await refresh();
    } catch {
      // ignore
    }
  }

  async function handleRebuild() {
    setRebuilding(true);
    try {
      await api.memoryRebuildEmbeddings();
    } catch {
      // ignore
    } finally {
      setRebuilding(false);
    }
  }

  function handleFilterChange() {
    void refresh();
  }

  const categories = useMemo(() => {
    const cats = new Set(entries.map((e) => e.category));
    return Array.from(cats).sort();
  }, [entries]);

  const grouped = useMemo(() => {
    const groups = new Map<string, MemoryEntry[]>();
    for (const entry of entries) {
      const list = groups.get(entry.category) || [];
      list.push(entry);
      groups.set(entry.category, list);
    }
    return Array.from(groups.entries()).sort(([a], [b]) => a.localeCompare(b));
  }, [entries]);

  return (
    <div className="memory-panel">
      <div className="memory-header">
        <h2>记忆管理</h2>
        <div className="memory-actions">
          <button
            type="button"
            className="memory-rebuild-btn"
            onClick={() => void handleRebuild()}
            disabled={rebuilding}
          >
            {rebuilding ? '重建中...' : '重建索引'}
          </button>
          <button
            type="button"
            className="refresh-btn-small"
            onClick={() => void refresh()}
            title="刷新"
          >
            🔄
          </button>
        </div>
      </div>

      <div className="memory-filters">
        <select
          value={filterCategory}
          onChange={(e) => {
            setFilterCategory(e.target.value);
            setTimeout(handleFilterChange, 0);
          }}
          className="memory-filter-select"
        >
          <option value="">全部分类</option>
          {categories.map((cat) => (
            <option key={cat} value={cat}>
              {cat}
            </option>
          ))}
        </select>
        <select
          value={filterStatus}
          onChange={(e) => {
            setFilterStatus(e.target.value);
            setTimeout(handleFilterChange, 0);
          }}
          className="memory-filter-select"
        >
          <option value="">全部状态</option>
          <option value="active">活跃</option>
          <option value="candidate">候选</option>
          <option value="archived">已归档</option>
          <option value="quarantined">隔离</option>
        </select>
      </div>

      {loading ? (
        <div className="memory-loading">加载中...</div>
      ) : entries.length === 0 ? (
        <div className="empty-state">
          <p>还没有记忆条目</p>
          <small>助手会在这里积累对你有用的个人信息</small>
        </div>
      ) : (
        <div className="memory-groups">
          {grouped.map(([category, items]) => (
            <section className="memory-group" key={category}>
              <div className="memory-group-header">
                <h3>{category}</h3>
                <span className="memory-group-count">{items.length} 条</span>
              </div>
              <div className="memory-entry-list">
                {items.map((entry) => {
                  const statusInfo = getStatusLabel(entry.status);
                  const isExpanded = expandedId === entry.id;
                  return (
                    <article className="memory-entry-card" key={entry.id}>
                      <div
                        className="memory-entry-main"
                        onClick={() => setExpandedId(isExpanded ? null : entry.id)}
                      >
                        <div className="memory-entry-key">{entry.key}</div>
                        <div className="memory-entry-value">
                          {isExpanded ? entry.value : truncateValue(entry.value)}
                        </div>
                        <div className="memory-entry-meta">
                          <span className={`memory-status ${statusInfo.className}`}>
                            {statusInfo.label}
                          </span>
                          <span className="memory-source">{getSourceLabel(entry.source)}</span>
                          <span className="memory-confidence">
                            {(entry.confidence * 100).toFixed(0)}%
                          </span>
                          <span className="memory-time">{formatTime(entry.updated_at)}</span>
                        </div>
                      </div>
                      {entry.status !== 'forgotten' && (
                        <div className="memory-entry-actions">
                          {entry.status === 'active' && (
                            <button
                              type="button"
                              className="memory-action-btn memory-archive-btn"
                              onClick={() => void handleArchive(entry.id)}
                              title="归档"
                            >
                              归档
                            </button>
                          )}
                          <button
                            type="button"
                            className="memory-action-btn memory-forget-btn"
                            onClick={() => void handleForget(entry.id)}
                            title="遗忘"
                          >
                            遗忘
                          </button>
                        </div>
                      )}
                    </article>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
      )}

      <div className="memory-footer">
        <span className="memory-total">共 {entries.length} 条记忆</span>
      </div>
    </div>
  );
}
