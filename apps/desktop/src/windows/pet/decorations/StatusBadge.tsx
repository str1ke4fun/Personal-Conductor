import React from 'react';

export const StatusBadge: React.FC<{ label: string }> = ({ label }) => (
  <div
    style={{
      position: 'absolute',
      bottom: '8%',
      left: '50%',
      transform: 'translateX(-50%)',
      background: 'rgba(14,15,18,0.75)',
      color: 'var(--text-secondary)',
      fontSize: 11,
      padding: '2px 8px',
      borderRadius: 8,
      fontFamily: 'var(--font-ui)',
      whiteSpace: 'nowrap',
    }}
  >
    {label}
  </div>
);
