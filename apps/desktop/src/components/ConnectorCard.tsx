import type { ConnectorSpec } from '../ipc/invoke';

interface ConnectorCardProps {
  connector: ConnectorSpec;
}

const IMPL_LABELS: Record<string, string> = {
  native_rust: 'Rust 原生',
  local_cli: 'CLI',
  mcp_server: 'MCP',
  http_api: 'HTTP API',
};

const AUTH_LABELS: Record<string, { label: string; className: string }> = {
  not_configured: { label: '未配置', className: 'auth-none' },
  authenticated: { label: '已认证', className: 'auth-ok' },
  expired: { label: '已过期', className: 'auth-warn' },
  failed: { label: '认证失败', className: 'auth-error' },
};

const RISK_LABELS: Record<string, string> = {
  low: '低',
  medium: '中',
  high: '高',
};

export function ConnectorCard({ connector }: ConnectorCardProps) {
  const auth = AUTH_LABELS[connector.auth_status] ?? { label: connector.auth_status, className: 'auth-none' };

  return (
    <div className={`connector-card ${connector.enabled ? 'enabled' : 'disabled'}`}>
      <div className="connector-card-header">
        <div className="connector-card-title">
          <strong>{connector.name}</strong>
          <span className={`connector-impl-badge impl-${connector.implementation_type}`}>
            {IMPL_LABELS[connector.implementation_type] ?? connector.implementation_type}
          </span>
        </div>
        <div className="connector-card-status">
          <span className={`connector-auth-badge ${auth.className}`}>{auth.label}</span>
          <span className={`connector-enabled-badge ${connector.enabled ? 'on' : 'off'}`}>
            {connector.enabled ? '启用' : '关闭'}
          </span>
        </div>
      </div>

      <p className="connector-card-desc">{connector.description}</p>

      {connector.capabilities.length > 0 && (
        <div className="connector-capabilities">
          {connector.capabilities.map((cap) => (
            <div key={cap.capability} className="connector-cap-row">
              <span className="connector-cap-name">{cap.capability}</span>
              <span className={`connector-risk-badge risk-${cap.risk_level}`}>
                {RISK_LABELS[cap.risk_level] ?? cap.risk_level}
              </span>
              {cap.requires_confirmation && (
                <span className="connector-confirm-badge">需确认</span>
              )}
              {cap.tools.length > 0 && (
                <span className="connector-cap-tools">
                  {cap.tools.join(', ')}
                </span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
