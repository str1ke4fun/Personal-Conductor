import React from 'react';

export const SleepZ: React.FC<{ visible: boolean }> = ({ visible }) => {
  if (!visible) return null;
  return (
    <div
      style={{
        position: 'absolute',
        top: '10%',
        right: '15%',
        fontSize: 20,
        color: 'var(--state-quiet)',
        opacity: 0.8,
        userSelect: 'none',
        animation: 'pet-breathe 3s ease-in-out infinite',
      }}
    >
      z
    </div>
  );
};
