// MCP per-tool mapping and classification.
//
// Each MCP tool discovered from a server enters `PendingClassification` state
// and is disabled by default. Tools must be explicitly classified via
// `classify_mcp_tool()` before they can be called.
//
// Per-tool risk level and permissions override the provider defaults:
// - `risk_level`: e.g. "read_only", "workspace_write", "destructive"
// - `permissions`: e.g. ["read_workspace", "write_workspace", "network"]
//
// Classification states:
// - `PendingClassification`: newly discovered, disabled, needs user action
// - `Enabled`: user-approved, callable with configured risk/permissions
// - `Disabled`: user-rejected, cannot be called
//
// See `mcp::classify_mcp_tool()`, `mcp::pending_tools()`, and
// `mcp::McpToolClassification` for the public API.
