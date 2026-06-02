import { useEffect, useState } from 'react';

export interface ChatBubbleProps {
  content: string;
  onClose: () => void;
  onOpenChat?: () => void;
  duration?: number;
}

export function ChatBubble({ content, onClose, onOpenChat, duration = 8000 }: ChatBubbleProps) {
  const [visible, setVisible] = useState(false);
  const [closing, setClosing] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setVisible(true), 100);
    return () => clearTimeout(timer);
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => {
      setClosing(true);
      setTimeout(() => onClose(), 300);
    }, duration);
    return () => clearTimeout(timer);
  }, [duration, onClose]);

  const handleClose = () => {
    onOpenChat?.();
    setClosing(true);
    setTimeout(() => onClose(), 300);
  };

  return (
    <div
      className={`chat-bubble ${visible ? 'chat-visible' : ''} ${closing ? 'chat-closing' : ''}`}
      onClick={handleClose}
    >
      <div className="chat-bubble-content">{content}</div>
      <div className="chat-bubble-arrow"></div>
    </div>
  );
}
