import { useState, useCallback, useEffect } from 'react';

export interface BubbleMessage {
  id: string;
  content: string;
  kind: 'self' | 'user' | 'notification';
  priority: 'low' | 'normal' | 'high';
  createdAt: number;
  ttl?: number; // ms, default 8000
}

const PRIORITY_ORDER = { high: 0, normal: 1, low: 2 };

export function useBubbleQueue() {
  const [queue, setQueue] = useState<BubbleMessage[]>([]);
  const [current, setCurrent] = useState<BubbleMessage | null>(null);
  const [paused, setPaused] = useState(false);

  const enqueue = useCallback((msg: BubbleMessage) => {
    setQueue(q => {
      const next = [...q, msg].sort((a, b) =>
        PRIORITY_ORDER[a.priority] - PRIORITY_ORDER[b.priority] ||
        a.createdAt - b.createdAt
      );
      return next;
    });
  }, []);

  const dequeue = useCallback(() => {
    setCurrent(null);
    setQueue(q => q.slice(1));
  }, []);

  // Promote from queue to current when current is empty
  useEffect(() => {
    if (!current && queue.length > 0) {
      setCurrent(queue[0]);
      setQueue(q => q.slice(1));
    }
  }, [current, queue]);

  // Auto-dismiss after ttl
  useEffect(() => {
    if (!current || paused) return;
    const ttl = current.ttl ?? 8000;
    const t = window.setTimeout(() => setCurrent(null), ttl);
    return () => window.clearTimeout(t);
  }, [current, paused]);

  return { queue, enqueue, dequeue, current, paused, setPaused };
}
