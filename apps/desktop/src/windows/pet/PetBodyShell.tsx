import React from 'react';

interface PetBodyShellProps {
  mood?: string;
  children: React.ReactNode;
}

export const PetBodyShell: React.FC<PetBodyShellProps> = ({ mood = 'idle', children }) => (
  <div className="pet-body" data-mood={mood} style={{ position: 'relative', width: '100%', height: '100%' }}>
    {children}
  </div>
);
