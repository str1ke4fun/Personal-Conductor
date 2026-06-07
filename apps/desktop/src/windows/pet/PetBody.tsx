import React from 'react';
import { PetBodyShell } from './PetBodyShell';

interface PetBodyProps {
  imageUrl: string;
  mood?: string;
  alt?: string;
}

export const PetBody: React.FC<PetBodyProps> = ({ imageUrl, mood, alt = '清和' }) => (
  <PetBodyShell mood={mood}>
    <img
      src={imageUrl}
      alt={alt}
      style={{ width: '100%', height: '100%', objectFit: 'contain', display: 'block' }}
    />
    <div className="pet-eye-layer" style={{ position: 'absolute', inset: 0 }} />
  </PetBodyShell>
);
