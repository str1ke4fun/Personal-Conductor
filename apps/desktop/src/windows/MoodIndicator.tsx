import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { MoodZone, PetExpressionPayload } from '../ipc/invoke';

const MOOD_LABELS: Record<MoodZone, string> = {
  happy: '开心',
  content: '满足',
  neutral: '平静',
  bored: '无聊',
  shy: '害羞',
  sad: '低落',
  frustrated: '懊恼',
};

const MOOD_COLORS: Record<MoodZone, string> = {
  happy: '#22c55e',
  content: '#10b981',
  neutral: '#6b7280',
  bored: '#a78bfa',
  shy: '#f472b6',
  sad: '#60a5fa',
  frustrated: '#ef4444',
};

const MOOD_EMOJI: Record<MoodZone, string> = {
  happy: '\u{1F604}',
  content: '\u{1F60C}',
  neutral: '\u{1F610}',
  bored: '\u{1F634}',
  shy: '\u{1F633}',
  sad: '\u{1F622}',
  frustrated: '\u{1F624}',
};

interface MoodIndicatorProps {
  moodZone?: MoodZone;
}

export function MoodIndicator({ moodZone: externalMoodZone }: MoodIndicatorProps) {
  const [moodZone, setMoodZone] = useState<MoodZone | undefined>(externalMoodZone);

  // Sync with external prop
  useEffect(() => {
    if (externalMoodZone) {
      setMoodZone(externalMoodZone);
    }
  }, [externalMoodZone]);

  // Also listen for expression events directly
  useEffect(() => {
    const unlisten = listen<PetExpressionPayload>('pet_expression', (event) => {
      if (event.payload.mood_zone) {
        setMoodZone(event.payload.mood_zone);
      }
    });
    return () => {
      unlisten.then((d) => d()).catch(() => {});
    };
  }, []);

  if (!moodZone) return null;

  const color = MOOD_COLORS[moodZone];
  const label = MOOD_LABELS[moodZone];
  const emoji = MOOD_EMOJI[moodZone];

  return (
    <div
      className="mood-indicator"
      style={{ '--mood-color': color } as React.CSSProperties}
      title={`心情: ${label}`}
    >
      <span className="mood-emoji">{emoji}</span>
      <span className="mood-label">{label}</span>
    </div>
  );
}
