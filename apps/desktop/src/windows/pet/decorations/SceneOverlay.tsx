import React from 'react';

type SceneKind = 'morning' | 'afternoon' | 'evening' | 'night' | 'music' | 'work' | 'relax';

interface SceneOverlayProps {
  sceneKind: SceneKind;
}

export const SceneOverlay: React.FC<SceneOverlayProps> = ({ sceneKind }) => (
  <div style={{
    position: 'absolute',
    inset: 0,
    background: `var(--scene-tint-${sceneKind})`,
    pointerEvents: 'none',
    borderRadius: 'inherit',
  }} />
);
