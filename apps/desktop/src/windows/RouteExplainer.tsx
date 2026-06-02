import React from 'react';

interface RouteExplainerProps {
  decision?: {
    task_kind: string;
    backend: string;
    reason: string;
    fallback_used?: boolean;
    routing_policy_id?: string;
  };
}

export const RouteExplainer: React.FC<RouteExplainerProps> = ({ decision }) => {
  if (!decision) {
    return (
      <div className="route-explainer">
        <h4>🧭 Route Decision</h4>
        <div className="route-empty">暂无路由决策</div>
      </div>
    );
  }

  return (
    <div className="route-explainer">
      <h4>🧭 为什么派给它？</h4>
      <div className="route-card">
        <div className="route-row">
          <span className="route-label">任务类型</span>
          <span className="route-value">{decision.task_kind}</span>
        </div>
        <div className="route-row">
          <span className="route-label">分配后端</span>
          <span className="route-value route-backend">{decision.backend}</span>
        </div>
        <div className="route-row">
          <span className="route-label">决策原因</span>
          <span className="route-value">{decision.reason}</span>
        </div>
        {decision.fallback_used && (
          <div className="route-row route-fallback">
            <span className="route-label">⚠️</span>
            <span className="route-value">使用了 fallback 后端</span>
          </div>
        )}
      </div>
    </div>
  );
};

export default RouteExplainer;
