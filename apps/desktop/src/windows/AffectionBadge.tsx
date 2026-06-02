import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { PetExpressionPayload, RelationshipStage } from '../ipc/invoke';
import { api } from '../ipc/invoke';

const STAGE_LABELS: Record<RelationshipStage, string> = {
  stranger: '陌生人',
  acquaintance: '初识',
  colleague: '同事',
  friend: '朋友',
  close_friend: '挚友',
};

const STAGE_RANGES: Record<RelationshipStage, [number, number]> = {
  stranger: [0, 19],
  acquaintance: [20, 39],
  colleague: [40, 59],
  friend: [60, 79],
  close_friend: [80, 100],
};

const STAGE_ICONS: Record<RelationshipStage, string> = {
  stranger: '\u{1F464}',
  acquaintance: '\u{1F91D}',
  colleague: '\u{1F4BC}',
  friend: '\u{1F60A}',
  close_friend: '\u{2764}\u{FE0F}',
};

function getStage(value: number): RelationshipStage {
  if (value >= 80) return 'close_friend';
  if (value >= 60) return 'friend';
  if (value >= 40) return 'colleague';
  if (value >= 20) return 'acquaintance';
  return 'stranger';
}

function getProgress(value: number, stage: RelationshipStage): number {
  const [min, max] = STAGE_RANGES[stage];
  return Math.min(100, ((value - min) / (max - min)) * 100);
}

export function AffectionBadge() {
  const [stage, setStage] = useState<RelationshipStage>('colleague');
  const [value, setValue] = useState(50);
  const [animating, setAnimating] = useState(false);
  const prevValueRef = useRef(value);

  // Initial fetch
  useEffect(() => {
    api
      .getAffection()
      .then((v) => {
        setValue(v);
        setStage(getStage(v));
        prevValueRef.current = v;
      })
      .catch(() => {});
  }, []);

  // Listen for expression events
  useEffect(() => {
    const unlisten = listen<PetExpressionPayload>('pet_expression', (event) => {
      const newStage = event.payload.relationship_stage;
      if (newStage) {
        setStage(newStage);
      }
    });
    return () => {
      unlisten.then((d) => d()).catch(() => {});
    };
  }, []);

  // Periodic refresh for affection value
  useEffect(() => {
    const interval = setInterval(() => {
      api
        .getAffection()
        .then((v) => {
          if (v !== prevValueRef.current) {
            setAnimating(true);
            setValue(v);
            setStage(getStage(v));
            prevValueRef.current = v;
            setTimeout(() => setAnimating(false), 600);
          }
        })
        .catch(() => {});
    }, 30_000);
    return () => clearInterval(interval);
  }, []);

  const label = STAGE_LABELS[stage];
  const icon = STAGE_ICONS[stage];
  const progress = getProgress(value, stage);

  return (
    <div
      className={`affection-badge${animating ? ' affection-bump' : ''}`}
      title={`关系: ${label} (${value})`}
    >
      <span className="affection-icon">{icon}</span>
      <span className="affection-label">{label}</span>
      <div className="affection-bar-track">
        <div
          className="affection-bar-fill"
          style={{ width: `${progress}%` }}
        />
      </div>
    </div>
  );
}
