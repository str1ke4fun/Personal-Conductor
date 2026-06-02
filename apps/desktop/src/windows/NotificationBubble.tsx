import { useEffect, useState } from 'react';

export interface Notification {
  id: string;
  content: string;
  urgency: 'low' | 'medium' | 'high';
}

export function NotificationBubble({ notification, onClose }: { notification: Notification; onClose: (id: string) => void }) {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => setVisible(true), 50);
    return () => clearTimeout(timer);
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => {
      setVisible(false);
      setTimeout(() => onClose(notification.id), 300);
    }, 10000);
    return () => clearTimeout(timer);
  }, [notification.id, onClose]);

  const handleClick = () => {
    setVisible(false);
    setTimeout(() => onClose(notification.id), 300);
  };

  const getIcon = () => {
    switch (notification.urgency) {
      case 'high':
        return '⚠️';
      case 'medium':
        return '📢';
      default:
        return '💬';
    }
  };

  return (
    <div
      className={`notification-bubble notification-${notification.urgency} ${visible ? 'notification-visible' : ''}`}
      onClick={handleClick}
    >
      <span className="notification-icon">{getIcon()}</span>
      <span className="notification-content">{notification.content}</span>
      <span className="notification-close">✕</span>
    </div>
  );
}