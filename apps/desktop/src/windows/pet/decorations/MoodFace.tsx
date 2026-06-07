import React from 'react';

const MOOD_EMOJI: Record<string, string> = {
  happy: '✨',
  shy: '💕',
  sad: '💧',
  quiet: '💤',
  idle: '',
};

export const MoodFace: React.FC<{ mood: string }> = ({ mood }) => {
  const emoji = MOOD_EMOJI[mood] || '';
  if (!emoji) return null;
  return (
    <div style={{ position: 'absolute', top: '8%', right: '8%', fontSize: 18, userSelect: 'none' }}>
      {emoji}
    </div>
  );
};
