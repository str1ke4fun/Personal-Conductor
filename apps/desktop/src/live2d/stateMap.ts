import type { MoodZone } from '../ipc/invoke';

export type PetState = 'idle' | 'working' | 'update' | 'quiet' | 'new_task';

export const STATE_TO_EXPR: Partial<Record<PetState, string>> = {
  idle: 'Idle',
  working: 'Happy',
  update: 'Happy',
  quiet: 'Sleep',
  new_task: 'Surprised',
};

export const STATE_TO_MOTION: Record<PetState, { group: string; index: number }> = {
  idle: { group: 'Idle', index: 0 },
  working: { group: 'Idle', index: 0 },
  update: { group: 'Tap@Head', index: 0 },
  quiet: { group: 'FlickDown', index: 0 },
  new_task: { group: 'Tap', index: 0 },
};

export const STATE_LABELS: Record<PetState, string> = {
  idle: '正常运行',
  working: '处理任务中',
  update: '有新进展',
  quiet: '专注模式',
  new_task: '新任务到达',
};

// Mood zone → Live2D expression mapping
export const MOOD_TO_EXPR: Partial<Record<MoodZone, string>> = {
  happy: 'Happy',
  content: 'Idle',
  neutral: 'Idle',
  bored: 'Idle',
  shy: 'Shy',
  sad: 'Sad',
  frustrated: 'Frustrated',
};

// Resolve expression from both pet state and mood zone
// Priority: ActivityVariant > Mood (per conflict resolution design)
export function resolveExpression(state: PetState, moodZone?: MoodZone): string {
  // Quiet (focus mode) overrides everything
  if (state === 'quiet') return 'Sleep';
  // New task overrides mood
  if (state === 'new_task') return 'Surprised';
  // Update is positive
  if (state === 'update') return 'Happy';
  // Working uses mood if available
  if (state === 'working' && moodZone) {
    return MOOD_TO_EXPR[moodZone] || 'Happy';
  }
  // Idle uses mood if available
  if (state === 'idle' && moodZone) {
    return MOOD_TO_EXPR[moodZone] || 'Idle';
  }
  // Fallback to state-based expression
  return STATE_TO_EXPR[state] || 'Idle';
}
