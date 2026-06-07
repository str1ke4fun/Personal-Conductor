// Pet state types — moved from live2d/stateMap.ts during Live2D cleanup.
// These are pure TypeScript types used by PetWindow and usePetVisualState.

export type PetState = 'idle' | 'working' | 'update' | 'quiet' | 'new_task';

export const STATE_LABELS: Record<PetState, string> = {
  idle: '在呢',
  working: '忙碌中',
  update: '有动静了',
  quiet: '安静一会儿',
  new_task: '新任务！',
};
