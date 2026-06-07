import React from 'react';

type ActivityVariant = 'idle' | 'thinking' | 'reading' | 'writing' | 'coding' | 'waiting' | string;

const ICONS: Record<string, string> = {
  thinking: '💭',
  reading: '📖',
  writing: '✍️',
  coding: '⚡',
  waiting: '⏳',
};

interface ActionIconProps {
  activityVariant: ActivityVariant;
  position?: 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right';
}

export const ActionIcon: React.FC<ActionIconProps> = ({ activityVariant, position = 'top-right' }) => {
  const icon = ICONS[activityVariant];
  if (!icon) return null;

  const posStyle: React.CSSProperties = {
    position: 'absolute',
    fontSize: 16,
    userSelect: 'none',
    pointerEvents: 'none',
    ...(position === 'top-right' ? { top: '8%', right: '8%' } : {}),
    ...(position === 'top-left' ? { top: '8%', left: '8%' } : {}),
    ...(position === 'bottom-right' ? { bottom: '8%', right: '8%' } : {}),
    ...(position === 'bottom-left' ? { bottom: '8%', left: '8%' } : {}),
  };

  return <div style={posStyle}>{icon}</div>;
};
