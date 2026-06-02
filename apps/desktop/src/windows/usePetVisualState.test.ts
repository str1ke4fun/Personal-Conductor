import { renderHook, waitFor, act } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => {
  const listeners = new Map<string, (event: { payload: unknown }) => void>();
  return {
    listeners,
    listenMock: vi.fn((eventName: string, callback: (event: { payload: unknown }) => void) => {
      listeners.set(eventName, callback);
      return Promise.resolve(() => listeners.delete(eventName));
    }),
    getCurrentAvatarMock: vi.fn(),
    getMoodStateMock: vi.fn(),
  };
});

vi.mock('@tauri-apps/api/event', () => ({
  listen: mocks.listenMock,
}));

vi.mock('../ipc/invoke', () => ({
  api: {
    getCurrentAvatar: mocks.getCurrentAvatarMock,
    getMoodState: mocks.getMoodStateMock,
  },
}));

import { usePetVisualState, type PetVisualState } from './usePetVisualState';

describe('usePetVisualState', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listeners.clear();
    mocks.getCurrentAvatarMock.mockResolvedValue({
      id: 'avatar-1',
      avatarId: 'programmer',
      activityVariant: 'thinking',
      updatedAt: '2026-06-02T00:00:00Z',
      lockedMainAvatar: false,
      lockedActivityVariant: false,
    });
    mocks.getMoodStateMock.mockResolvedValue({
      zone: 'happy',
      label: 'Happy',
      valence: 0.6,
      arousal: 0.4,
    });
  });

  it('hydrates from backend state instead of sticking to the placeholder avatar', async () => {
    const { result } = renderHook(() => usePetVisualState());

    await waitFor(() => {
      expect(result.current.avatarId).toBe('programmer');
    });

    expect(result.current.activityVariant).toBe('thinking');
    expect(result.current.moodZone).toBe('happy');
    expect(result.current.petState).toBe('working');
  });

  it('accepts snake_case pet_avatar_changed payloads from tauri events', async () => {
    const { result } = renderHook(() => usePetVisualState());

    await waitFor(() => {
      expect(result.current.avatarId).toBe('programmer');
    });

    act(() => {
      mocks.listeners.get('pet_avatar_changed')?.({
        payload: {
          avatar_id: 'document_secretary',
          activity_variant: 'writing',
        },
      });
    });

    await waitFor(() => {
      expect(result.current.avatarId).toBe('document_secretary');
    }, { timeout: 2500 });

    expect(result.current.activityVariant).toBe('writing');
    expect(result.current.petState).toBe('working');
  });

  it('keeps the documented PetVisualState shape', () => {
    const state: PetVisualState = {
      avatarId: 'test',
      activityVariant: 'thinking',
      moodZone: 'happy',
      petState: 'working',
    };
    expect(state.avatarId).toBe('test');
    expect(state.activityVariant).toBe('thinking');
    expect(state.moodZone).toBe('happy');
    expect(state.petState).toBe('working');
  });
});
