import { useState } from 'react';
import type { SkillPackage } from '../ipc/invoke';

interface SkillCardProps {
  skill: SkillPackage;
  onToggle: (id: string, enabled: boolean) => void;
  onDelete: (id: string) => void;
}

const SOURCE_LABELS: Record<string, string> = {
  builtin: '内置',
  user_import: '导入',
  marketplace: '商店',
  dev_local: '本地',
};

export function SkillCard({ skill, onToggle, onDelete }: SkillCardProps) {
  const [expanded, setExpanded] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const activationItems = [
    ...skill.activation.keywords.map((k) => `关键词: ${k}`),
    ...skill.activation.apps.map((a) => `应用: ${a}`),
    ...skill.activation.url_patterns.map((u) => `URL: ${u}`),
    ...skill.activation.file_patterns.map((f) => `文件: ${f}`),
  ];

  function handleDelete() {
    if (!confirmDelete) {
      setConfirmDelete(true);
      setTimeout(() => setConfirmDelete(false), 3000);
      return;
    }
    onDelete(skill.id);
  }

  return (
    <div className={`skill-card ${skill.enabled ? 'enabled' : 'disabled'}`}>
      <div className="skill-card-header">
        <div className="skill-card-title">
          <strong>{skill.name}</strong>
          <span className="skill-card-version">v{skill.version}</span>
          <span className={`skill-source-badge source-${skill.source}`}>
            {SOURCE_LABELS[skill.source] ?? skill.source}
          </span>
        </div>
        <label className="skill-toggle">
          <input
            type="checkbox"
            checked={skill.enabled}
            onChange={(e) => onToggle(skill.id, e.target.checked)}
          />
          <span className="skill-toggle-label">{skill.enabled ? '启用' : '关闭'}</span>
        </label>
      </div>

      <p className="skill-card-desc">{skill.description}</p>
      {skill.author && <small className="skill-card-author">作者: {skill.author}</small>}

      {skill.capabilities.length > 0 && (
        <div className="skill-capabilities">
          {skill.capabilities.map((cap) => (
            <span key={cap} className="skill-cap-badge">{cap}</span>
          ))}
        </div>
      )}

      {activationItems.length > 0 && (
        <div className="skill-activation">
          <small className="skill-activation-title">触发条件</small>
          <ul className="skill-activation-list">
            {activationItems.map((item, i) => (
              <li key={i}><small>{item}</small></li>
            ))}
          </ul>
        </div>
      )}

      <div className="skill-card-actions">
        {skill.body && (
          <button
            type="button"
            className="skill-action-btn"
            onClick={() => setExpanded(!expanded)}
          >
            {expanded ? '收起内容' : '查看内容'}
          </button>
        )}
        <button
          type="button"
          className={`skill-action-btn danger${confirmDelete ? ' confirming' : ''}`}
          onClick={handleDelete}
        >
          {confirmDelete ? '确认删除' : '删除'}
        </button>
      </div>

      {expanded && skill.body && (
        <pre className="skill-card-body">{skill.body}</pre>
      )}
    </div>
  );
}
