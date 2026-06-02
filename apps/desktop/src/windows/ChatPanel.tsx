import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useState } from 'react';
import { api } from '../ipc/invoke';
import { useChatSession } from './useChatSession';
import { ChatTimelinePane } from './ChatTimelinePane';
import { ChatComposer } from './ChatComposer';

interface ChatPanelProps {
  standalone?: boolean;
}

export function ChatPanel({ standalone = false }: ChatPanelProps) {
  const [sessionId, setSessionId] = useState<string | null>(null);
  const session = useChatSession({
    petMessageSource: 'chat_panel',
    acceptPetMessages: true,
    sessionId,
  });

  useEffect(() => {
    api.ensureChatSession('闲聊')
      .then((chatSession) => setSessionId(chatSession.id))
      .catch(() => {});
  }, []);

  return (
    <div className="chat-panel">
      <div className="chat-header">
        <h2>和清和闲聊</h2>
        {standalone && (
          <button
            className="chat-close-btn"
            onClick={() => void getCurrentWindow().hide()}
            title="关闭"
          >
            x
          </button>
        )}
      </div>

      <ChatTimelinePane
        messages={session.messages}
        sending={session.sending}
        streamTokens={session.streamTokens}
        toolStates={session.toolStates}
        thinkingContent={session.thinkingContent}
        projectedRuns={session.projectedRuns}
        endRef={session.endRef}
        onRetry={session.retryMessage}
        onApproveProposal={session.approveProposal}
        onRejectProposal={session.rejectProposal}
        turnStartedAt={session.turnStartedAt}
        currentPhase={session.currentPhase}
        toolRunCount={session.toolRunCount}
        activeToolCount={session.activeToolCount}
      />

      <ChatComposer
        input={session.input}
        setInput={session.setInput}
        sending={session.sending}
        onSend={session.sendMessage}
      />
    </div>
  );
}
