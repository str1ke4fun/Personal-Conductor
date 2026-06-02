interface BlockedCardProps {
  title: string;
  reason: string;
  actionItems?: string[];
}

export function BlockedCard({ title, reason, actionItems }: BlockedCardProps) {
  return (
    <div className="blocked-card">
      <div className="blocked-header">
        <span className="blocked-icon">阻塞</span>
        <span className="blocked-title">{title}</span>
      </div>
      <div className="blocked-reason">{reason}</div>
      {actionItems && actionItems.length > 0 && (
        <div className="blocked-actions-list">
          <span className="blocked-actions-label">需要你处理：</span>
          <ul>
            {actionItems.map((item, i) => (
              <li key={i}>{item}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
