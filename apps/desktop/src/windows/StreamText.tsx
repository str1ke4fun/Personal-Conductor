interface StreamTextProps {
  text: string;
  isStreaming: boolean;
}

export function StreamText({ text, isStreaming }: StreamTextProps) {
  if (!text && !isStreaming) return null;

  return (
    <div className="stream-text">
      <span className="stream-text-content">{text}</span>
      {isStreaming && <span className="cursor-blink">|</span>}
    </div>
  );
}
