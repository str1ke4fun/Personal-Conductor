import React, { useEffect, useState } from 'react';

interface HeartFloatProps {
  trigger: number;   // increment to trigger one burst
  duration?: number; // ms, default 1500
}

export const HeartFloat: React.FC<HeartFloatProps> = ({ trigger, duration = 1500 }) => {
  const [particles, setParticles] = useState<{ id: number; x: number }[]>([]);

  useEffect(() => {
    if (trigger === 0) return;
    const id = Date.now();
    const x = 30 + Math.random() * 40; // % from left
    setParticles(p => [...p, { id, x }]);
    setTimeout(() => setParticles(p => p.filter(h => h.id !== id)), duration);
  }, [trigger, duration]);

  return (
    <>
      {particles.map(h => (
        <div key={h.id} style={{
          position: 'absolute',
          bottom: '20%',
          left: `${h.x}%`,
          fontSize: 16,
          pointerEvents: 'none',
          animation: `pet-breathe ${duration}ms ease-out forwards`,
          opacity: 0,
          transform: 'translateY(-40px)',
          animationFillMode: 'forwards',
          userSelect: 'none',
        }}>
          💕
        </div>
      ))}
    </>
  );
};
