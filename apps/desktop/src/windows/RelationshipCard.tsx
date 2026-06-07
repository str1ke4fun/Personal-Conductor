import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface RelationshipStats {
  days_known: number;
  conversation_count: number;
  task_count: number;
  affection_level: number;
  last_upgrade_date?: string;
}

export const RelationshipCard: React.FC = () => {
  const [stats, setStats] = useState<RelationshipStats | null>(null);

  useEffect(() => {
    invoke<RelationshipStats>('get_relationship_stats')
      .then(setStats)
      .catch(() => {
        // Fallback mock while backend is being wired
        setStats({
          days_known: 0,
          conversation_count: 0,
          task_count: 0,
          affection_level: 0,
        });
      });
  }, []);

  if (!stats) return <div style={{ padding: 16, color: 'var(--text-secondary)', fontSize: 13 }}>加载中…</div>;

  return (
    <div style={{
      padding: '16px 20px',
      display: 'flex',
      flexDirection: 'column',
      gap: 16,
      fontFamily: 'var(--font-ui)',
    }}>
      {/* Header */}
      <div>
        <div style={{ fontSize: 13, color: 'var(--text-secondary)' }}>我们认识了</div>
        <div style={{
          fontSize: 28,
          fontFamily: 'var(--font-display)',
          color: 'var(--text-primary)',
          fontWeight: 600,
          lineHeight: 1.2,
        }}>
          {stats.days_known} <span style={{ fontSize: 16, fontWeight: 400 }}>天</span>
        </div>
      </div>

      {/* Stats grid */}
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
        {[
          { label: '对话次数', value: stats.conversation_count },
          { label: '完成任务', value: stats.task_count },
          { label: '好感度', value: stats.affection_level },
          { label: '关系等级', value: affectionLevel(stats.affection_level) },
        ].map(({ label, value }) => (
          <div key={label} style={{
            background: 'var(--bg-surface)',
            border: '0.5px solid var(--border-hair)',
            borderRadius: 8,
            padding: '10px 12px',
          }}>
            <div style={{ fontSize: 11, color: 'var(--text-secondary)', marginBottom: 4 }}>{label}</div>
            <div style={{ fontSize: 18, color: 'var(--text-primary)', fontWeight: 600 }}>{value}</div>
          </div>
        ))}
      </div>

      {stats.last_upgrade_date && (
        <div style={{ fontSize: 12, color: 'var(--text-secondary)' }}>
          上次升级：{stats.last_upgrade_date}
        </div>
      )}
    </div>
  );
};

function affectionLevel(score: number): string {
  if (score >= 80) return '深交';
  if (score >= 50) return '熟悉';
  if (score >= 20) return '了解';
  return '初识';
}
