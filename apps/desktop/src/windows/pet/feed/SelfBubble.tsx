import React from 'react';

interface SelfBubbleProps {
  message: { id: string; content: string; priority: 'low' | 'normal' | 'high' };
  onClose: () => void;
  onPause?: () => void;
  signature?: string;
}

export const SelfBubble: React.FC<SelfBubbleProps> = ({
  message,
  onClose,
  onPause,
  signature = '清和',
}) => (
  <div
    style={{
      position: 'absolute',
      top: 0,
      left: '105%',
      width: 220,
      background: 'var(--bg-elevated)',
      border: '0.5px solid var(--border-hair)',
      borderRadius: 10,
      padding: '10px 14px 8px',
      boxShadow: '0 4px 20px rgba(0,0,0,0.35)',
      zIndex: 200,
    }}
    onMouseEnter={onPause}
    onMouseLeave={onPause}
  >
    {/* Signature */}
    <div style={{
      fontSize: 11,
      color: 'var(--text-secondary)',
      fontFamily: '"Fraunces", Georgia, serif',
      fontStyle: 'italic',
      marginBottom: 6,
    }}>
      {signature}
    </div>

    {/* Content */}
    <div style={{
      fontSize: 13,
      color: 'var(--text-primary)',
      lineHeight: 1.5,
      wordBreak: 'break-word',
    }}>
      {message.content}
    </div>

    {/* Close */}
    <button
      onClick={onClose}
      style={{
        position: 'absolute',
        top: 6,
        right: 8,
        background: 'none',
        border: 'none',
        color: 'var(--text-secondary)',
        cursor: 'pointer',
        fontSize: 14,
        lineHeight: 1,
        padding: 0,
      }}
    >
      ×
    </button>
  </div>
);
