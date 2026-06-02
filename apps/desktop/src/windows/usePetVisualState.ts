import { listen } from '@tauri-apps/api/event';
import { useEffect, useRef, useState } from 'react';
import { api, type AvatarState, type MoodZone, type PetExpressionPayload } from '../ipc/invoke';
import type { PetState } from '../live2d/stateMap';

type ActivityVariant =
  | 'idle'
  | 'thinking'
  | 'reading'
  | 'writing'
  | 'tool_calling'
  | 'agent_leading'
  | 'waiting_user'
  | 'error'
  | 'done';

export interface PetVisualState {
  avatarId: string;
  activityVariant: ActivityVariant;
  moodZone: MoodZone | undefined;
  petState: PetState;
}

type AvatarEventPayload = Partial<AvatarState> & {
  avatar_id?: string;
  activity_variant?: string;
};

const INITIAL: PetVisualState = {
  avatarId: 'original',
  activityVariant: 'idle',
  moodZone: undefined,
  petState: 'idle',
};

// Priority: higher index = higher priority, overrides lower
const PRIORITY_ORDER: readonly string[] = [
  'idle',
  'waiting_user',
  'done',
  'working',
  'thinking',
  'tool_calling',
  'writing',
  'update',
  'error',
];

function priorityOf(state: PetState): number {
  const idx = PRIORITY_ORDER.indexOf(state);
  return idx >= 0 ? idx : 0;
}

// Map activityVariant to PetState for comparison
function variantToPetState(variant: ActivityVariant): PetState {
  switch (variant) {
    case 'idle':
      return 'idle';
    case 'thinking':
    case 'reading':
      return 'working';
    case 'writing':
    case 'tool_calling':
    case 'agent_leading':
      return 'working';
    case 'waiting_user':
      return 'idle';
    case 'done':
      return 'update';
    case 'error':
      return 'update';
  }
}

function readAvatarId(payload: AvatarEventPayload): string | undefined {
  return payload.avatarId || payload.avatar_id;
}

function readActivityVariant(payload: AvatarEventPayload): ActivityVariant | undefined {
  const variant = payload.activityVariant || payload.activity_variant;
  return variant ? (variant as ActivityVariant) : undefined;
}

// Min display durations per activity (ms) — prevents flickering
const MIN_DISPLAY_MS: Record<string, number> = {
  tool_calling: 1200,
  writing: 1500,
  thinking: 1200,
  reading: 1200,
  agent_leading: 1200,
  done: 1800,
  update: 1800,
  error: 1800,
  idle: 2000,
  waiting_user: 1800,
};

function keysEqual(a: PetVisualState, b: PetVisualState): boolean {
  return (
    a.avatarId === b.avatarId &&
    a.activityVariant === b.activityVariant &&
    a.moodZone === b.moodZone &&
    a.petState === b.petState
  );
}

export function usePetVisualState(): PetVisualState {
  const [state, setState] = useState<PetVisualState>(INITIAL);
  const stateRef = useRef<PetVisualState>(INITIAL);
  const pendingRef = useRef<PetVisualState | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const displayedAtRef = useRef<number>(Date.now());

  // Schedule a state transition with min-display-time enforcement
  function scheduleTransition(next: PetVisualState) {
    const current = stateRef.current;
    if (keysEqual(current, next)) return;

    const currentPriority = priorityOf(current.petState);
    const nextPriority = priorityOf(next.petState);

    // Higher priority can always override immediately (error > tool_calling > thinking > etc.)
    // But same or lower priority must respect min display time
    if (nextPriority > currentPriority) {
      // Higher priority: apply immediately
      applyState(next);
      return;
    }

    // Same or lower priority: enforce min display time for current state
    const elapsed = Date.now() - displayedAtRef.current;
    const minMs = MIN_DISPLAY_MS[current.activityVariant] ?? 1200;
    const remaining = minMs - elapsed;

    if (remaining <= 0) {
      // Min time already elapsed, apply now
      applyState(next);
    } else {
      // Queue and wait
      pendingRef.current = next;
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        timerRef.current = null;
        const pending = pendingRef.current;
        pendingRef.current = null;
        if (pending) {
          applyState(pending);
        }
      }, remaining);
    }
  }

  function applyState(next: PetVisualState) {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    pendingRef.current = null;
    stateRef.current = next;
    displayedAtRef.current = Date.now();
    setState(next);
  }

  useEffect(() => {
    let cancelled = false;

    void Promise.all([
      api.getCurrentAvatar().catch(() => null),
      api.getMoodState().catch(() => null),
    ]).then(([avatar, mood]) => {
      if (cancelled) return;
      const nextVariant =
        (avatar?.activityVariant as ActivityVariant | undefined) ??
        stateRef.current.activityVariant;
      scheduleTransition({
        avatarId: avatar?.avatarId ?? stateRef.current.avatarId,
        activityVariant: nextVariant,
        moodZone: mood?.zone ?? stateRef.current.moodZone,
        petState: variantToPetState(nextVariant),
      });
    });

    // Source 1: pet_state (PetState string)
    const unlistenPetState = listen<PetState>('pet_state', (event) => {
      const petState = event.payload;
      const current = stateRef.current;
      scheduleTransition({
        ...current,
        petState,
        // Derive activityVariant from petState if it's a high-level change
        activityVariant: petState === 'quiet' ? 'idle' : current.activityVariant,
      });
    });

    // Source 2: pet_avatar_changed (AvatarState with avatarId + activityVariant)
    const unlistenAvatarChanged = listen<AvatarEventPayload>('pet_avatar_changed', (event) => {
      const payload = event.payload;
      const activityVariant = readActivityVariant(payload) || stateRef.current.activityVariant;
      const petState = variantToPetState(activityVariant);
      const current = stateRef.current;
      scheduleTransition({
        ...current,
        avatarId: readAvatarId(payload) || current.avatarId,
        activityVariant,
        petState: current.petState === 'quiet' ? 'quiet' : petState,
      });
    });

    // Source 3: pet_expression (composite, most authoritative)
    const unlistenExpression = listen<PetExpressionPayload>('pet_expression', (event) => {
      const payload = event.payload;
      const activityVariant = (payload.activity_variant as ActivityVariant) || 'idle';
      const petState = (payload.pet_state as PetState) || variantToPetState(activityVariant);
      scheduleTransition({
        avatarId: payload.avatar_id || stateRef.current.avatarId,
        activityVariant,
        moodZone: payload.mood_zone,
        petState,
      });
    });

    return () => {
      cancelled = true;
      unlistenPetState.then((d) => d()).catch(() => {});
      unlistenAvatarChanged.then((d) => d()).catch(() => {});
      unlistenExpression.then((d) => d()).catch(() => {});
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  return state;
}
