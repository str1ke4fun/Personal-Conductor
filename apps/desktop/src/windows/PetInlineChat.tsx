import { getCurrentWindow } from '@tauri-apps/api/window';
import { useState } from 'react';
import { api } from '../ipc/invoke';

interface PetInlineChatProps {
  onMessage: (content: string) => void;
}

export function PetInlineChat({ onMessage }: PetInlineChatProps) {
  const [petInput, setPetInput] = useState('');
  const [petSending, setPetSending] = useState(false);
  const appWindow = getCurrentWindow();

  async function sendPetMessage() {
    const content = petInput.trim();
    if (!content || petSending) return;

    setPetInput('');
    setPetSending(true);
    onMessage(content);
    try {
      const chitchat = await api.ensureChatSession('闲聊');
      const reply = await api.sendChatMessageV2(content, chitchat.id);
      const bubbleText = reply.bubble_summary ?? reply.message.content;
      onMessage(bubbleText);
      appWindow.emit('pet_message', {
        id: reply.message.id,
        content: bubbleText,
        kind: 'assistant',
      }).catch(() => {});
      api.recordActivity().catch(() => {});
    } catch (err) {
      onMessage(err instanceof Error ? err.message : '发送没成功，看看配置有没有问题？');
    } finally {
      setPetSending(false);
    }
  }

  return (
    <form
      className="pet-inline-chat"
      onSubmit={(event) => {
        event.preventDefault();
        void sendPetMessage();
      }}
      onMouseDown={(event) => event.stopPropagation()}
    >
      <input
        value={petInput}
        onChange={(event) => setPetInput(event.target.value)}
        placeholder="说点什么..."
        disabled={petSending}
      />
      <button type="submit" disabled={petSending || !petInput.trim()} title="发送">
        发送
      </button>
    </form>
  );
}
