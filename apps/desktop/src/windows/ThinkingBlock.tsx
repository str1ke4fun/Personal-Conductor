import { useState } from 'react';

interface ThinkingBlockProps {
  thinking: string;
}

export function ThinkingBlock({ thinking }: ThinkingBlockProps) {
  const [visible, setVisible] = useState(false);

  if (!thinking) return null;

  return (
    <div className="thinking-block">
      <button className="thinking-toggle" onClick={() => setVisible(!visible)}>
        {visible ? '收起思考' : '查看思考过程'}
      </button>
      {visible && <div className="thinking-content">{thinking}</div>}
    </div>
  );
}
