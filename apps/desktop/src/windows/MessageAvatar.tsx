import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';

interface AvatarState {
  avatar_id: string;
  activity_variant: string;
}

const AVATAR_THUMBS: Record<string, string> = {
  'programmer:thinking': '/avatar/programmer/thinking.png',
  'programmer:idle': '/avatar/programmer/thinking.png',
  'programmer:writing': '/avatar/programmer/coding.png',
  'programmer:tool_calling': '/avatar/programmer/coding.png',
  'programmer:agent_leading': '/avatar/programmer/agent_leader.png',
  'programmer:done': '/avatar/programmer/finish.png',
  'programmer:error': '/avatar/programmer/error.png',
  'programmer:waiting_user': '/avatar/programmer/thinking.png',
  'programmer:reading': '/avatar/programmer/thinking.png',
  'document_secretary:thinking': '/avatar/document_secretary/thinking.png',
  'document_secretary:idle': '/avatar/document_secretary/thinking.png',
  'document_secretary:writing': '/avatar/document_secretary/writing.png',
  'document_secretary:tool_calling': '/avatar/document_secretary/writing.png',
  'document_secretary:agent_leading': '/avatar/document_secretary/thinking.png',
  'document_secretary:done': '/avatar/document_secretary/drink.png',
  'document_secretary:error': '/avatar/document_secretary/shy.png',
  'document_secretary:waiting_user': '/avatar/document_secretary/drink.png',
  'document_secretary:reading': '/avatar/document_secretary/tired.png',
};

function getAvatarSrc(avatarId: string, variant: string): string {
  const key = `${avatarId}:${variant}`;
  return AVATAR_THUMBS[key] ?? '/avatar/programmer/thinking.png';
}

export function MessageAvatar() {
  const [state, setState] = useState<AvatarState>({ avatar_id: 'programmer', activity_variant: 'idle' });

  useEffect(() => {
    const unlisten = listen<Record<string, string>>('pet_avatar_changed', (event) => {
      const payload = event.payload;
      setState({
        avatar_id: payload.avatar_id ?? state.avatar_id,
        activity_variant: payload.activity_variant ?? state.activity_variant,
      });
    });
    return () => { void unlisten.then((d) => d()).catch(() => {}); };
  }, []);

  const src = getAvatarSrc(state.avatar_id, state.activity_variant);

  return (
    <img
      className="message-avatar"
      src={src}
      alt="avatar"
      draggable={false}
    />
  );
}
