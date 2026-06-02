import { useCallback, useEffect, useRef } from 'react';

export type CapabilityMode = 'read_only' | 'ask_write' | 'trusted';
export type TaskMode = 'short' | 'long';

export interface ChatSendOptions {
  taskMode: TaskMode;
  capability: CapabilityMode;
  planOnly: boolean;
  approvedWriteScope?: string[];
}

interface ChatComposerProps {
  input: string;
  setInput: (v: string) => void;
  sending: boolean;
  onSend: (options: ChatSendOptions) => Promise<void>;
  onStop?: () => void;
  onRetry?: () => void;
  sessionKind?: 'chat' | 'goal';
  workspaceName?: string | null;
}

// Chat sessions run with safe, zero-config defaults. Advanced controls
// stay in Goal mode; plain chat should still read like plain chat.
const CHAT_DEFAULTS: ChatSendOptions = {
  taskMode: 'short',
  capability: 'ask_write',
  planOnly: false,
};

export function ChatComposer({
  input,
  setInput,
  sending,
  onSend,
  onStop,
  onRetry,
  sessionKind = 'chat',
  workspaceName = null,
}: ChatComposerProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const adjustHeight = useCallback(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
  }, []);

  useEffect(() => {
    adjustHeight();
  }, [adjustHeight, input]);

  function submit() {
    if (!sending && input.trim()) {
      void onSend({ ...CHAT_DEFAULTS });
    }
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  }

  const isGoalSession = sessionKind === 'goal';
  const modeLabel = isGoalSession ? 'Goal' : 'Chat';
  const modeTitle = isGoalSession ? '推进目标' : '和清和说话';
  const modeHint = isGoalSession
    ? '目标模式 · 长任务，补充约束、优先级，或说「继续」'
    : '清和在旁边，有什么放着说，需要改文件时她会先问你';
  const placeholder = isGoalSession
    ? '补充约束或优先级，直接说「继续」也行…'
    : '说吧，我在听…';

  return (
    <div className="console-composer">
      <div className="composer-shell">
        <div className="composer-header">
          <div className="composer-brand">
            <span className={`composer-brand-mark ${isGoalSession ? 'goal' : 'chat'}`}>{modeLabel}</span>
            <div className="composer-brand-copy">
              <strong>{modeTitle}</strong>
              <span>
                {modeHint}
                {workspaceName ? ` · 工作区 ${workspaceName}` : ''}
              </span>
            </div>
          </div>
          <span className="composer-shortcut">Enter 发送 · Shift+Enter 换行</span>
        </div>

        <div className="composer-input-row">
          <textarea
            ref={textareaRef}
            className="composer-textarea"
            value={input}
            onChange={(event) => setInput(event.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            disabled={sending}
            rows={1}
          />
          <div className="composer-actions">
            {sending ? (
              onStop ? (
                <button type="button" className="composer-btn stop" onClick={onStop} title="停止">
                  停止
                </button>
              ) : null
            ) : input.trim() ? (
              <button
                type="button"
                className="composer-btn send"
                onClick={submit}
                disabled={!input.trim()}
              >
                {isGoalSession ? '继续执行' : '发送'}
              </button>
            ) : onRetry ? (
              <button
                type="button"
                className="composer-btn retry"
                onClick={onRetry}
                title="重试上一条"
              >
                重试
              </button>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
