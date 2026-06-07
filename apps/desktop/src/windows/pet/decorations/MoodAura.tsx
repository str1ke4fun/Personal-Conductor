import React from 'react';

export const MoodAura: React.FC<{ mood: string }> = ({ mood }) => (
  <div
    style={{
      position: 'absolute',
      inset: 0,
      borderRadius: '50%',
      filter: `drop-shadow(var(--pet-glow-${mood === 'happy' ? 'happy' : mood === 'shy' ? 'shy' : 'aff'}))`,
      opacity: mood === 'idle' ? 0 : 0.7,
      transition: 'opacity 0.5s',
    }}
  />
);
