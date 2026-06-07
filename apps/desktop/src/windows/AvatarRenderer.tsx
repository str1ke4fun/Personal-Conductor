import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api, type AppSettings, type AvatarId, type AvatarSettings, type MoodZone } from '../ipc/invoke';
import type { PetVisualState } from './usePetVisualState';
import { PetBody } from './pet/PetBody';

const DEFAULT_AVATAR: AvatarSettings = {
  mode: 'video',
  videoSrc: '/avatar/original/video.mp4',
  fit: 'contain',
  loopVideo: true,
  muted: true,
  playbackRate: 1,
};

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

interface AvatarAsset {
  src: string;
  scale: number;
  align?: 'center' | 'bottom';
}

// Mood zones that produce visually distinct images
type VisualMood = 'positive' | 'neutral' | 'shy' | 'negative';

function moodToVisual(mood: MoodZone): VisualMood {
  switch (mood) {
    case 'happy':
      return 'positive';
    case 'shy':
      return 'shy';
    case 'frustrated':
      return 'negative';
    default:
      return 'neutral';
  }
}

// Base manifest (activity-only, no mood) — backward compatible
const AVATAR_MANIFEST: Record<string, AvatarAsset> = {
  // Document Secretary
  'document_secretary:thinking': { src: '/avatar/document_secretary/thinking.png', scale: 1.18, align: 'bottom' },
  'document_secretary:idle': { src: '/avatar/document_secretary/thinking.png', scale: 1.18, align: 'bottom' },
  'document_secretary:reading': { src: '/avatar/document_secretary/tired.png', scale: 1.18, align: 'bottom' },
  'document_secretary:waiting_user': { src: '/avatar/document_secretary/drink.png', scale: 1.18, align: 'bottom' },
  'document_secretary:done': { src: '/avatar/document_secretary/drink.png', scale: 1.18, align: 'bottom' },
  'document_secretary:error': { src: '/avatar/document_secretary/shy.png', scale: 1.18, align: 'bottom' },
  'document_secretary:writing': { src: '/avatar/document_secretary/writing.png', scale: 1.2, align: 'bottom' },
  'document_secretary:tool_calling': { src: '/avatar/document_secretary/writing.png', scale: 1.2, align: 'bottom' },
  'document_secretary:agent_leading': { src: '/avatar/document_secretary/thinking.png', scale: 1.18, align: 'bottom' },

  // Programmer
  'programmer:thinking': { src: '/avatar/programmer/thinking.png', scale: 1.2, align: 'bottom' },
  'programmer:idle': { src: '/avatar/programmer/thinking.png', scale: 1.2, align: 'bottom' },
  'programmer:reading': { src: '/avatar/programmer/thinking.png', scale: 1.2, align: 'bottom' },
  'programmer:waiting_user': { src: '/avatar/programmer/thinking.png', scale: 1.2, align: 'bottom' },
  'programmer:done': { src: '/avatar/programmer/finish.png', scale: 1.2, align: 'bottom' },
  'programmer:error': { src: '/avatar/programmer/error.png', scale: 1.2, align: 'bottom' },
  'programmer:agent_leading': { src: '/avatar/programmer/agent_leader.png', scale: 1.2, align: 'bottom' },
  'programmer:writing': { src: '/avatar/programmer/coding.png', scale: 1.2, align: 'bottom' },
  'programmer:tool_calling': { src: '/avatar/programmer/coding.png', scale: 1.2, align: 'bottom' },
};

// Mood-zone overlay manifest: composite keys for mood-specific images
// Phase 2 extension — add entries here when new mood images are created
const MOOD_MANIFEST: Record<string, AvatarAsset> = {
  // Example entries (uncomment when images exist):
  // 'programmer:positive:idle': { src: '/avatar/programmer/happy_idle.png', scale: 1.2, align: 'bottom' },
};

function normalizeAvatar(settings?: Partial<AvatarSettings>): AvatarSettings {
  return {
    ...DEFAULT_AVATAR,
    ...settings,
    mode: 'video',
    fit: settings?.fit === 'cover' ? 'cover' : 'contain',
    videoSrc: settings?.videoSrc?.trim() || DEFAULT_AVATAR.videoSrc,
    playbackRate: settings?.playbackRate || DEFAULT_AVATAR.playbackRate,
  };
}

/**
 * 3-tier fallback for image resolution:
 * 1. Mood composite key: `${avatarId}:${visualMood}:${activityVariant}`
 * 2. Neutral mood: `${avatarId}:neutral:${activityVariant}` (falls through to base manifest)
 * 3. Legacy key: `${avatarId}:${activityVariant}`
 * 4. Thinking fallback: `${avatarId}:thinking`
 */
function resolveImageAsset(
  avatarId: Exclude<AvatarId, 'original'>,
  activityVariant: string,
  moodZone?: MoodZone,
): AvatarAsset {
  if (moodZone) {
    const visual = moodToVisual(moodZone);

    // 1. Try mood composite key
    const moodKey = `${avatarId}:${visual}:${activityVariant}`;
    const moodHit = MOOD_MANIFEST[moodKey];
    if (moodHit) return moodHit;

    // 2. If not neutral, try neutral mood
    if (visual !== 'neutral') {
      const neutralKey = `${avatarId}:neutral:${activityVariant}`;
      const neutralHit = MOOD_MANIFEST[neutralKey];
      if (neutralHit) return neutralHit;
    }
  }

  // 3. Legacy key (base manifest)
  const legacyKey = `${avatarId}:${activityVariant}`;
  const legacyHit = AVATAR_MANIFEST[legacyKey];
  if (legacyHit) return legacyHit;

  // 4. Thinking fallback
  const thinkingKey = `${avatarId}:thinking`;
  const thinkingHit = AVATAR_MANIFEST[thinkingKey];
  if (thinkingHit) return thinkingHit;

  return { src: DEFAULT_AVATAR.videoSrc, scale: 1 };
}

// ---------------------------------------------------------------------------
// useResolvedAvatarSrc — returns just the image src for the current visual state.
// Returns null when the avatar is the 'original' (video) variant.
// ---------------------------------------------------------------------------
export function useResolvedAvatarSrc(visualState: PetVisualState): string | null {
  const [avatarId, setAvatarId] = useState<AvatarId>('original');
  const [activityVariant, setActivityVariant] = useState<ActivityVariant>('idle');
  const [moodZone, setMoodZone] = useState<MoodZone | undefined>(undefined);
  const [lockedMainAvatar, setLockedMainAvatar] = useState(false);
  const [lockedActivityVariant, setLockedActivityVariant] = useState(false);
  const [imageFallback, setImageFallback] = useState<'mapped' | 'thinking'>('mapped');

  useEffect(() => {
    api.getCurrentAvatar()
      .then((state) => {
        setAvatarId(state.avatarId);
        setActivityVariant((state.activityVariant as ActivityVariant) || 'idle');
        setLockedMainAvatar(state.lockedMainAvatar);
        setLockedActivityVariant(state.lockedActivityVariant);
      })
      .catch(() => {});

    const unlistenAvatar = listen('pet_avatar_changed', () => {
      api.getCurrentAvatar().then((state) => {
        setLockedMainAvatar(state.lockedMainAvatar);
        setLockedActivityVariant(state.lockedActivityVariant);
      }).catch(() => {});
    });

    return () => {
      unlistenAvatar.then((dispose) => dispose()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    const isPlaceholder =
      visualState.avatarId === 'original' &&
      visualState.activityVariant === 'idle' &&
      visualState.moodZone === undefined &&
      visualState.petState === 'idle';
    if (isPlaceholder && avatarId !== 'original') return;

    setImageFallback('mapped');
    if (!lockedMainAvatar) setAvatarId(visualState.avatarId as AvatarId);
    if (!lockedActivityVariant) setActivityVariant(visualState.activityVariant);
    setMoodZone(visualState.moodZone);
  }, [visualState, avatarId, lockedMainAvatar, lockedActivityVariant]);

  if (avatarId === 'original') return null;

  const asset = imageFallback === 'mapped'
    ? resolveImageAsset(avatarId, activityVariant, moodZone)
    : (AVATAR_MANIFEST[`${avatarId}:thinking`] ?? { src: DEFAULT_AVATAR.videoSrc, scale: 1 });

  return asset.src;
}

// ---------------------------------------------------------------------------
// AvatarRenderer — full component kept for backward compat.
// Original avatar → video element. Image avatars → PetBody.
// ---------------------------------------------------------------------------
interface AvatarRendererProps {
  visualState?: PetVisualState;
}

export function AvatarRenderer({ visualState }: AvatarRendererProps) {
  const [avatar, setAvatar] = useState<AvatarSettings>(DEFAULT_AVATAR);
  const [avatarId, setAvatarId] = useState<AvatarId>('original');
  const [activityVariant, setActivityVariant] = useState<ActivityVariant>('idle');
  const [moodZone, setMoodZone] = useState<MoodZone | undefined>(undefined);
  const [videoFailed, setVideoFailed] = useState(false);
  const [imageFallback, setImageFallback] = useState<'mapped' | 'thinking' | 'original'>('mapped');
  const [lockedMainAvatar, setLockedMainAvatar] = useState(false);
  const [lockedActivityVariant, setLockedActivityVariant] = useState(false);
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    Promise.all([api.getSettings(), api.getCurrentAvatar()])
      .then(([settings, state]) => {
        setAvatar(normalizeAvatar(settings.pet.avatar));
        setAvatarId(state.avatarId);
        setActivityVariant((state.activityVariant as ActivityVariant) || 'idle');
        setLockedMainAvatar(state.lockedMainAvatar);
        setLockedActivityVariant(state.lockedActivityVariant);
      })
      .catch(() => {
        api
          .getSettings()
          .then((settings) => setAvatar(normalizeAvatar(settings.pet.avatar)))
          .catch(() => {});
      });

    const unlistenSettings = listen<AppSettings>('settings_changed', (event) => {
      setVideoFailed(false);
      setAvatar(normalizeAvatar(event.payload.pet.avatar));
    });

    const unlistenAvatar = listen('pet_avatar_changed', () => {
      api.getCurrentAvatar().then((state) => {
        setLockedMainAvatar(state.lockedMainAvatar);
        setLockedActivityVariant(state.lockedActivityVariant);
      }).catch(() => {});
    });

    return () => {
      unlistenSettings.then((dispose) => dispose()).catch(() => {});
      unlistenAvatar.then((dispose) => dispose()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (!visualState) return;
    const isPlaceholderVisualState =
      visualState.avatarId === 'original' &&
      visualState.activityVariant === 'idle' &&
      visualState.moodZone === undefined &&
      visualState.petState === 'idle';
    if (isPlaceholderVisualState && avatarId !== 'original') return;

    setVideoFailed(false);
    setImageFallback('mapped');
    if (!lockedMainAvatar) setAvatarId(visualState.avatarId as AvatarId);
    if (!lockedActivityVariant) setActivityVariant(visualState.activityVariant);
    setMoodZone(visualState.moodZone);
  }, [visualState, avatarId, lockedMainAvatar, lockedActivityVariant]);

  useEffect(() => {
    if (videoRef.current) {
      videoRef.current.playbackRate = avatar.playbackRate;
    }
  }, [avatar.playbackRate, avatar.videoSrc]);

  // Original avatar — video element
  if (avatarId === 'original') {
    return (
      <div className="avatar-video-frame">
        <video
          ref={videoRef}
          key={`${avatar.videoSrc}-${avatar.fit}-${avatar.playbackRate}`}
          className={`avatar-video avatar-fit-${avatar.fit}`}
          src={avatar.videoSrc}
          style={{ '--avatar-scale': '1.08' } as CSSProperties}
          autoPlay
          loop={avatar.loopVideo}
          muted={avatar.muted}
          playsInline
          onError={() => setVideoFailed(true)}
        />
        {videoFailed && (
          <div className="avatar-video-error">
            视频加载不出来，去设置里看看路径？
          </div>
        )}
      </div>
    );
  }

  // Image avatar — resolve asset then delegate to PetBody
  if (imageFallback !== 'original') {
    let asset: AvatarAsset;
    if (imageFallback === 'mapped') {
      asset = resolveImageAsset(avatarId, activityVariant, moodZone);
    } else {
      const thinkingKey = `${avatarId}:thinking`;
      asset = AVATAR_MANIFEST[thinkingKey] ?? { src: DEFAULT_AVATAR.videoSrc, scale: 1 };
    }

    const isVideoFallback = asset.src === DEFAULT_AVATAR.videoSrc;

    if (isVideoFallback && imageFallback === 'thinking') {
      return (
        <div className="avatar-video-frame">
          <video
            ref={videoRef}
            key={`${avatar.videoSrc}-${avatar.fit}-${avatar.playbackRate}`}
            className={`avatar-video avatar-fit-${avatar.fit}`}
            src={avatar.videoSrc}
            autoPlay
            loop={avatar.loopVideo}
            muted={avatar.muted}
            playsInline
            onError={() => setVideoFailed(true)}
          />
        </div>
      );
    }

    return (
      <div
        className={`avatar-video-frame avatar-align-${asset.align || 'center'}`}
        style={{ '--avatar-scale': String(asset.scale) } as CSSProperties}
      >
        <PetBody
          imageUrl={asset.src}
          mood={moodZone}
          alt=""
        />
      </div>
    );
  }

  // Final fallback — original video
  return (
    <div className="avatar-video-frame">
      <video
        ref={videoRef}
        key={`${avatar.videoSrc}-${avatar.fit}-${avatar.playbackRate}`}
        className={`avatar-video avatar-fit-${avatar.fit}`}
        src={avatar.videoSrc}
        autoPlay
        loop={avatar.loopVideo}
        muted={avatar.muted}
        playsInline
        onError={() => setVideoFailed(true)}
      />
      {videoFailed && (
        <div className="avatar-video-error">
          视频加载不出来，去设置里看看路径？
        </div>
      )}
    </div>
  );
}
