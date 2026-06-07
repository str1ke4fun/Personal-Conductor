import React from 'react';
import { PetBody } from './PetBody';

interface PetSceneProps {
  imageUrl: string;
  mood?: string;
  sceneKind?: string;
  decorations?: React.ReactNode;
}

export const PetScene: React.FC<PetSceneProps> = ({ imageUrl, mood, sceneKind, decorations }) => (
  <div style={{ position: 'relative', width: '100%', height: '100%' }}>
    {sceneKind && (
      <div
        style={{
          position: 'absolute',
          inset: 0,
          background: `var(--scene-tint-${sceneKind})`,
          pointerEvents: 'none',
          zIndex: 0,
        }}
      />
    )}
    <div style={{ position: 'relative', zIndex: 1, width: '100%', height: '100%' }}>
      <PetBody imageUrl={imageUrl} mood={mood} />
    </div>
    {decorations && (
      <div style={{ position: 'absolute', inset: 0, pointerEvents: 'none', zIndex: 2 }}>
        {decorations}
      </div>
    )}
  </div>
);
