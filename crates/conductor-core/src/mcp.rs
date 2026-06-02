use crate::tools::{ToolExecutorFn, ToolSpec};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, RwLock};

// ── JSON-RPC 2.0 Types ──────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum JsonRpcId {
    Num(i64),
    Str(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<JsonRpcId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for JsonRpcError {}

// ── MCP Protocol Types ───────────────────────────────────────────────────────

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InitializeParams {
    pub protocolVersion: String,
    pub capabilities: ClientCapabilities,
    pub clientInfo: ClientInfo,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolsCapability {
    #[serde(default)]
    pub listChanged: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InitializeResult {
    pub protocolVersion: String,
    pub capabilities: ServerCapabilities,
    pub serverInfo: ServerInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolsListResult {
    pub tools: Vec<Tool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolsCallParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolsCallResult {
    pub content: Vec<ToolCallContent>,
    #[serde(default)]
    pub isError: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ToolCallContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    #[allow(non_snake_case)]
    Image { data: String, mimeType: String },
    #[serde(rename = "resource")]
    EmbeddedResource { resource: serde_json::Value },
}

// ── Stdio Transport ─────────────────────────────────────────────────────────

/// Manages a subprocess for MCP stdio transport.
/// Communicates via newline-delimited JSON over stdin/stdout.
struct StdioProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl StdioProcess {
    fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> anyhow::Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args);
        for (key, value) in env {
            cmd.env(key, value);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());

        let mut child = cmd.spawn().map_err(|e| {
            anyhow::anyhow!("failed to spawn MCP stdio process '{}': {}", command, e)
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to capture stdin of MCP stdio process"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("failed to capture stdout of MCP stdio process"))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    /// Send a JSON-RPC message and read the next response line.
    /// Uses newline-delimited JSON framing.
    fn send_and_receive(&mut self, request: &JsonRpcRequest) -> anyhow::Result<JsonRpcResponse> {
        let mut json = serde_json::to_string(request)?;
        json.push('\n');
        self.stdin.write_all(json.as_bytes())?;
        self.stdin.flush()?;

        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .map_err(|e| anyhow::anyhow!("failed to read from MCP stdio stdout: {}", e))?;

        if line.trim().is_empty() {
            anyhow::bail!("MCP stdio process closed stdout (EOF)");
        }

        let response: JsonRpcResponse = serde_json::from_str(line.trim())?;
        Ok(response)
    }

    /// Send a JSON-RPC notification (no response expected).
    fn send_notification(&mut self, notification: &JsonRpcNotification) -> anyhow::Result<()> {
        let mut json = serde_json::to_string(notification)?;
        json.push('\n');
        self.stdin.write_all(json.as_bytes())?;
        self.stdin.flush()?;
        Ok(())
    }
}

impl Drop for StdioProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Thread-safe handle to a stdio subprocess.
#[derive(Clone)]
pub(crate) struct StdioHandle(Arc<Mutex<StdioProcess>>);

// ── Transport Abstraction ────────────────────────────────────────────────────

#[derive(Clone)]
pub enum McpTransport {
    Http {
        endpoint: String,
        api_key: Option<String>,
    },
    Stdio(StdioHandle),
}

// ── MCP Server Config ────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub api_key: Option<String>,
    pub enabled: bool,
    pub risk_level: String,
    /// Transport type: "http" (default) or "stdio"
    #[serde(default = "default_transport")]
    pub transport: String,
    /// For stdio transport: command to run
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// For stdio transport: command arguments
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// For stdio transport: environment variables
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

fn default_transport() -> String {
    "http".to_string()
}

/// Classification state for MCP tools.
/// Newly discovered tools start as `PendingClassification` and are disabled by default.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpToolClassification {
    /// Just discovered, not yet classified by the user. Tool is disabled.
    PendingClassification,
    /// User has explicitly enabled this tool.
    Enabled,
    /// User has explicitly disabled this tool.
    Disabled,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct McpToolMapping {
    pub mcp_tool_name: String,
    pub conductor_tool_id: String,
    pub input_mapping: Option<serde_json::Value>,
    pub output_mapping: Option<serde_json::Value>,
    pub enabled: bool,
    /// Per-tool classification state. Defaults to `PendingClassification` for newly discovered tools.
    #[serde(default = "default_classification")]
    pub classification: McpToolClassification,
    /// Per-tool risk level override. If `None`, uses PendingClassification default (ReadOnly).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_level: Option<String>,
    /// Per-tool permission overrides. If empty, uses PendingClassification default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
}

fn default_classification() -> McpToolClassification {
    McpToolClassification::PendingClassification
}

// ── MCP Client ───────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct McpClient {
    transport: McpTransport,
    request_id: i64,
    initialized: bool,
    server_capabilities: Option<ServerCapabilities>,
}

impl McpClient {
    fn new(transport: McpTransport) -> Self {
        Self {
            transport,
            request_id: 0,
            initialized: false,
            server_capabilities: None,
        }
    }

    fn next_id(&mut self) -> i64 {
        self.request_id += 1;
        self.request_id
    }

    /// Send a JSON-RPC request and receive a response.
    async fn send_request(&self, request: &JsonRpcRequest) -> anyhow::Result<JsonRpcResponse> {
        match &self.transport {
            McpTransport::Http { endpoint, api_key } => {
                let mut builder = reqwest::Client::new().post(endpoint);

                if let Some(key) = api_key {
                    builder = builder.header("Authorization", format!("Bearer {}", key));
                }

                let response = builder.json(request).send().await?;
                let rpc_response: JsonRpcResponse = response.json().await?;
                Ok(rpc_response)
            }
            McpTransport::Stdio(handle) => {
                let request_clone = request.clone();
                let handle_clone = handle.clone();
                tokio::task::spawn_blocking(move || {
                    let mut proc = handle_clone
                        .0
                        .lock()
                        .map_err(|e| anyhow::anyhow!("stdio handle lock poisoned: {}", e))?;
                    proc.send_and_receive(&request_clone)
                })
                .await?
            }
        }
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn send_notification(&self, notification: &JsonRpcNotification) -> anyhow::Result<()> {
        match &self.transport {
            McpTransport::Http { endpoint, api_key } => {
                let mut builder = reqwest::Client::new().post(endpoint);

                if let Some(key) = api_key {
                    builder = builder.header("Authorization", format!("Bearer {}", key));
                }

                builder.json(notification).send().await?;
                Ok(())
            }
            McpTransport::Stdio(handle) => {
                let notification_clone = notification.clone();
                let handle_clone = handle.clone();
                tokio::task::spawn_blocking(move || {
                    let mut proc = handle_clone
                        .0
                        .lock()
                        .map_err(|e| anyhow::anyhow!("stdio handle lock poisoned: {}", e))?;
                    proc.send_notification(&notification_clone)
                })
                .await?
            }
        }
    }

    /// MCP initialize handshake.
    pub async fn initialize(&mut self) -> anyhow::Result<InitializeResult> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: JsonRpcId::Num(self.next_id()),
            method: "initialize".to_string(),
            params: Some(serde_json::to_value(InitializeParams {
                protocolVersion: "2024-11-05".to_string(),
                capabilities: ClientCapabilities {
                    tools: Some(ToolsCapability { listChanged: false }),
                },
                clientInfo: ClientInfo {
                    name: "conductor".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
            })?),
        };

        let response = self.send_request(&request).await?;

        if let Some(err) = response.error {
            anyhow::bail!("MCP initialize failed: {}", err);
        }

        let result: InitializeResult = serde_json::from_value(
            response
                .result
                .ok_or_else(|| anyhow::anyhow!("Missing result in initialize response"))?,
        )?;

        self.initialized = true;
        self.server_capabilities = Some(result.capabilities.clone());

        // Send initialized notification
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "notifications/initialized".to_string(),
            params: None,
        };
        let _ = self.send_notification(&notification).await;

        Ok(result)
    }

    /// List available tools from the MCP server.
    pub async fn list_tools(&mut self) -> anyhow::Result<Vec<Tool>> {
        if !self.initialized {
            self.initialize().await?;
        }

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: JsonRpcId::Num(self.next_id()),
            method: "tools/list".to_string(),
            params: None,
        };

        let response = self.send_request(&request).await?;

        if let Some(err) = response.error {
            anyhow::bail!("MCP tools/list failed: {}", err);
        }

        let result: ToolsListResult = serde_json::from_value(
            response
                .result
                .ok_or_else(|| anyhow::anyhow!("Missing result in tools/list response"))?,
        )?;

        Ok(result.tools)
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: Option<serde_json::Value>,
    ) -> anyhow::Result<ToolsCallResult> {
        if !self.initialized {
            self.initialize().await?;
        }

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: JsonRpcId::Num(self.next_id()),
            method: "tools/call".to_string(),
            params: Some(serde_json::to_value(ToolsCallParams {
                name: name.to_string(),
                arguments,
            })?),
        };

        let response = self.send_request(&request).await?;

        if let Some(err) = response.error {
            anyhow::bail!("MCP tools/call failed: {}", err);
        }

        let result: ToolsCallResult = serde_json::from_value(
            response
                .result
                .ok_or_else(|| anyhow::anyhow!("Missing result in tools/call response"))?,
        )?;

        Ok(result)
    }

    /// Check if the client has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get server capabilities (available after initialize).
    pub fn server_capabilities(&self) -> Option<&ServerCapabilities> {
        self.server_capabilities.as_ref()
    }
}

// ── Backward-compatible types ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub risk_level: String,
    pub requires_approval: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct McpExecutionResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
}

// ── MCP Provider ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct McpProvider {
    config: McpServerConfig,
    tools: HashMap<String, McpToolMapping>,
    client: Option<McpClient>,
}

impl McpProvider {
    pub fn new(config: McpServerConfig) -> Self {
        Self {
            config,
            tools: HashMap::new(),
            client: None,
        }
    }

    pub fn add_tool_mapping(&mut self, mapping: McpToolMapping) {
        self.tools.insert(mapping.mcp_tool_name.clone(), mapping);
    }

    pub fn tool_mappings(&self) -> Vec<McpToolMapping> {
        self.tools.values().cloned().collect()
    }

    fn build_transport(&self) -> anyhow::Result<McpTransport> {
        match self.config.transport.as_str() {
            "stdio" => {
                let command =
                    self.config.command.clone().ok_or_else(|| {
                        anyhow::anyhow!("stdio transport requires 'command' field")
                    })?;
                let process = StdioProcess::spawn(&command, &self.config.args, &self.config.env)?;
                Ok(McpTransport::Stdio(StdioHandle(Arc::new(Mutex::new(
                    process,
                )))))
            }
            _ => Ok(McpTransport::Http {
                endpoint: format!("http://{}:{}", self.config.host, self.config.port),
                api_key: self.config.api_key.clone(),
            }),
        }
    }

    pub async fn connect(&mut self) -> anyhow::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let transport = self.build_transport()?;
        let mut client = McpClient::new(transport);

        // Perform MCP initialize handshake
        let init_result = client.initialize().await?;
        tracing::info!(
            "MCP server '{}' connected: {} v{}",
            self.config.id,
            init_result.serverInfo.name,
            init_result.serverInfo.version
        );

        self.client = Some(client);
        Ok(())
    }

    pub async fn discover_tools(&mut self) -> anyhow::Result<Vec<ToolSpec>> {
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("MCP client not connected"))?;

        let mcp_tools = client.list_tools().await?;

        let mut specs = Vec::new();
        for tool in mcp_tools {
            // Check if we have a mapping for this tool
            if let Some(mapping) = self.tools.get(&tool.name) {
                // Only register enabled, classified tools
                if !mapping.enabled {
                    continue;
                }

                // Resolve per-tool risk level (override > provider default)
                let risk_level =
                    resolve_tool_risk_level(mapping.risk_level.as_deref(), &self.config.risk_level);

                // Resolve per-tool permissions (override > empty default)
                let permissions = resolve_tool_permissions(&mapping.permissions);

                let spec = ToolSpec {
                    id: mapping.conductor_tool_id.clone(),
                    name: tool.name.clone(),
                    description: tool.description.unwrap_or_default(),
                    provider: crate::tools::ToolProviderKind::Mcp,
                    input_schema: tool.input_schema,
                    output_schema: serde_json::json!({}),
                    risk_level,
                    permissions,
                    supports_dry_run: true,
                    workspace_required: false,
                };
                specs.push(spec);
            } else {
                // New tool discovered -- enter PendingClassification state.
                // Default: disabled, must be explicitly enabled.
                let conductor_id = format!("mcp.{}.{}", self.config.id, tool.name);

                let mapping = McpToolMapping {
                    mcp_tool_name: tool.name.clone(),
                    conductor_tool_id: conductor_id.clone(),
                    input_mapping: None,
                    output_mapping: None,
                    enabled: false,
                    classification: McpToolClassification::PendingClassification,
                    risk_level: None,
                    permissions: vec![],
                };

                self.tools.insert(tool.name.clone(), mapping);

                // Log discovery but do NOT register as executable tool
                tracing::info!(
                    "MCP tool '{}' discovered from provider '{}' -- PendingClassification (disabled)",
                    tool.name,
                    self.config.id
                );
            }
        }

        Ok(specs)
    }

    pub fn create_executor(&self, _mcp_tool_name: &str) -> Option<ToolExecutorFn> {
        Some(execute_mcp_tool)
    }
}

/// Resolve per-tool risk level: tool override > provider default.
fn resolve_tool_risk_level(
    tool_override: Option<&str>,
    provider_default: &str,
) -> crate::proposals::RiskLevel {
    let level_str = tool_override.unwrap_or(provider_default);
    crate::proposals::RiskLevel::from_str(level_str)
        .unwrap_or(crate::proposals::RiskLevel::ReadOnly)
}

/// Resolve per-tool permissions from string names.
fn resolve_tool_permissions(perm_names: &[String]) -> Vec<crate::tools::ToolPermission> {
    perm_names
        .iter()
        .filter_map(|name| match name.as_str() {
            "read_workspace" => Some(crate::tools::ToolPermission::ReadWorkspace),
            "write_workspace" => Some(crate::tools::ToolPermission::WriteWorkspace),
            "read_external_path" => Some(crate::tools::ToolPermission::ReadExternalPath),
            "write_external_path" => Some(crate::tools::ToolPermission::WriteExternalPath),
            "network" => Some(crate::tools::ToolPermission::Network),
            "send_message" => Some(crate::tools::ToolPermission::SendMessage),
            "system_control" => Some(crate::tools::ToolPermission::SystemControl),
            _ => {
                tracing::warn!("unknown permission name: {}", name);
                None
            }
        })
        .collect()
}

/// Execute a tool via MCP JSON-RPC `tools/call`.
/// Enforces per-tool classification: PendingClassification tools are rejected.
fn execute_mcp_tool(
    spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<crate::tools::ToolExecutionResult, anyhow::Error> {
    let registry = MCP_REGISTRY.read().unwrap();
    let Some((provider, mapping)) = registry.values().find_map(|provider| {
        provider
            .tools
            .values()
            .find(|mapping| mapping.enabled && mapping.conductor_tool_id == spec.id)
            .map(|mapping| (provider.clone(), mapping.clone()))
    }) else {
        return Err(anyhow::anyhow!(
            "MCP mapping not found for tool: {}",
            spec.id
        ));
    };

    // Gate: reject PendingClassification tools
    if mapping.classification == McpToolClassification::PendingClassification {
        return Err(anyhow::anyhow!(
            "MCP tool '{}' is PendingClassification and cannot be called directly. \
             Explicitly enable it first.",
            spec.id
        ));
    }

    let Some(mut client) = provider.client.clone() else {
        return Err(anyhow::anyhow!(
            "MCP client not connected for provider: {}",
            provider.config.id
        ));
    };

    let tool_name = mapping.mcp_tool_name.clone();
    let provider_id = provider.config.id.clone();
    let input_clone = input.clone();

    let runtime = crate::tools::shared_runtime();
    runtime.block_on(async move {
        let result = client.call_tool(&tool_name, Some(input_clone)).await?;

        // Extract text content from the result
        let output_text = result
            .content
            .iter()
            .filter_map(|c| match c {
                ToolCallContent::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        let output_value = if output_text.is_empty() {
            serde_json::json!(null)
        } else {
            // Try to parse as JSON, fall back to string
            serde_json::from_str(&output_text).unwrap_or_else(|_| serde_json::json!(output_text))
        };

        let success = !result.isError;

        // Emit audit event for MCP tool call
        let audit_event = crate::events::AuditEvent {
            timestamp: chrono::Utc::now(),
            source: "mcp".to_string(),
            event_type: "mcp_tool_call".to_string(),
            actor: "system".to_string(),
            target: format!("{}.{}", provider_id, tool_name),
            detail: serde_json::json!({
                "tool_id": spec.id,
                "mcp_tool_name": tool_name,
                "provider_id": provider_id,
                "success": success,
                "is_error": result.isError,
            }),
            session_id: None,
        };
        let _ = crate::events::append_event(&audit_event).await;

        Ok(crate::tools::ToolExecutionResult {
            success,
            output: output_value,
            error: if result.isError {
                Some(output_text)
            } else {
                None
            },
            duration_ms: 0,
        })
    })
}

// ── Global Registry ──────────────────────────────────────────────────────────

pub fn executor_target_for_tool(tool_id: &str) -> Option<(String, String)> {
    let registry = MCP_REGISTRY.read().unwrap();
    registry.values().find_map(|provider| {
        provider.tools.values().find_map(|mapping| {
            if mapping.enabled && mapping.conductor_tool_id == tool_id {
                Some((provider.config.id.clone(), mapping.mcp_tool_name.clone()))
            } else {
                None
            }
        })
    })
}

pub fn register_builtin_mcp_mappings() {
    let config = McpServerConfig {
        id: "conductor".to_string(),
        name: "Conductor Local Tools".to_string(),
        host: "127.0.0.1".to_string(),
        port: 0,
        api_key: None,
        enabled: false,
        risk_level: "read_only".to_string(),
        transport: "http".to_string(),
        command: None,
        args: vec![],
        env: HashMap::new(),
    };
    register_mcp_provider(config);
    let _ = add_tool_mapping(
        "conductor",
        McpToolMapping {
            mcp_tool_name: "conductor.pet.set_avatar".to_string(),
            conductor_tool_id: "conductor.pet.set_avatar".to_string(),
            input_mapping: None,
            output_mapping: None,
            enabled: true,
            classification: McpToolClassification::Enabled,
            risk_level: None,
            permissions: vec![],
        },
    );
}

lazy_static::lazy_static! {
    static ref MCP_REGISTRY: RwLock<HashMap<String, McpProvider>> = RwLock::new(HashMap::new());
}

pub fn register_mcp_provider(config: McpServerConfig) {
    let mut registry = MCP_REGISTRY.write().unwrap();
    registry.insert(config.id.clone(), McpProvider::new(config));
}

pub fn add_tool_mapping(provider_id: &str, mapping: McpToolMapping) -> anyhow::Result<()> {
    let mut registry = MCP_REGISTRY.write().unwrap();
    let provider = registry
        .get_mut(provider_id)
        .ok_or_else(|| anyhow::anyhow!("MCP provider not found: {}", provider_id))?;
    provider.add_tool_mapping(mapping);
    Ok(())
}

pub fn get_mcp_provider(id: &str) -> Option<McpProvider> {
    let registry = MCP_REGISTRY.read().unwrap();
    registry.get(id).cloned()
}

pub fn list_mcp_providers() -> Vec<McpServerConfig> {
    let registry = MCP_REGISTRY.read().unwrap();
    registry
        .values()
        .map(|provider| provider.config.clone())
        .collect()
}

pub fn list_tool_mappings(provider_id: &str) -> anyhow::Result<Vec<McpToolMapping>> {
    let registry = MCP_REGISTRY.read().unwrap();
    let provider = registry
        .get(provider_id)
        .ok_or_else(|| anyhow::anyhow!("MCP provider not found: {}", provider_id))?;
    Ok(provider.tool_mappings())
}

/// Classify an MCP tool: set its risk level, permissions, and classification state.
/// This transitions a tool from PendingClassification to Enabled or Disabled.
pub fn classify_mcp_tool(
    provider_id: &str,
    mcp_tool_name: &str,
    classification: McpToolClassification,
    risk_level: Option<String>,
    permissions: Vec<String>,
) -> anyhow::Result<()> {
    let mut registry = MCP_REGISTRY.write().unwrap();
    let provider = registry
        .get_mut(provider_id)
        .ok_or_else(|| anyhow::anyhow!("MCP provider not found: {}", provider_id))?;

    let mapping = provider
        .tools
        .get_mut(mcp_tool_name)
        .ok_or_else(|| anyhow::anyhow!("MCP tool not found: {}", mcp_tool_name))?;

    let enabled = classification == McpToolClassification::Enabled;
    mapping.classification = classification;
    mapping.risk_level = risk_level;
    mapping.permissions = permissions;
    mapping.enabled = enabled;

    Ok(())
}

/// Get all tools in PendingClassification state for a provider.
pub fn pending_tools(provider_id: &str) -> anyhow::Result<Vec<McpToolMapping>> {
    let registry = MCP_REGISTRY.read().unwrap();
    let provider = registry
        .get(provider_id)
        .ok_or_else(|| anyhow::anyhow!("MCP provider not found: {}", provider_id))?;

    Ok(provider
        .tools
        .values()
        .filter(|m| m.classification == McpToolClassification::PendingClassification)
        .cloned()
        .collect())
}

/// Sync all enabled MCP providers: connect, discover tools, and register them.
pub async fn sync_mcp_tools() -> anyhow::Result<()> {
    let mut providers = {
        let registry = MCP_REGISTRY.read().unwrap();
        registry.values().cloned().collect::<Vec<_>>()
    };

    for provider in providers.iter_mut() {
        if !provider.config.enabled {
            continue;
        }

        if let Err(e) = provider.connect().await {
            tracing::warn!(
                "Failed to connect MCP provider '{}': {}",
                provider.config.id,
                e
            );
            continue;
        }

        match provider.discover_tools().await {
            Ok(tools) => {
                for tool in tools {
                    if let Some(executor) = provider.create_executor(&tool.name) {
                        crate::tools::register_tool(tool, executor);
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to discover tools from MCP provider '{}': {}",
                    provider.config.id,
                    e
                );
            }
        }
    }

    let mut registry = MCP_REGISTRY.write().unwrap();
    for provider in providers {
        registry.insert(provider.config.id.clone(), provider);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static MCP_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn isolated_registry() -> MutexGuard<'static, ()> {
        let guard = MCP_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("mcp test lock poisoned");
        MCP_REGISTRY.write().unwrap().clear();
        guard
    }

    /// Helper: create a test McpToolMapping with new fields.
    fn test_mapping(mcp_name: &str, conductor_id: &str) -> McpToolMapping {
        McpToolMapping {
            mcp_tool_name: mcp_name.to_string(),
            conductor_tool_id: conductor_id.to_string(),
            input_mapping: None,
            output_mapping: None,
            enabled: true,
            classification: McpToolClassification::Enabled,
            risk_level: None,
            permissions: vec![],
        }
    }

    /// Helper: create a test McpServerConfig for HTTP.
    fn test_config(id: &str, port: u16) -> McpServerConfig {
        McpServerConfig {
            id: id.to_string(),
            name: format!("Test MCP {}", id),
            host: "localhost".to_string(),
            port,
            api_key: None,
            enabled: false,
            risk_level: "read_only".to_string(),
            transport: "http".to_string(),
            command: None,
            args: vec![],
            env: HashMap::new(),
        }
    }

    // ── Existing tests (updated for new McpToolMapping fields) ─────────────

    #[test]
    fn test_mcp_config() {
        let config = McpServerConfig {
            id: "test-mcp".to_string(),
            name: "Test MCP".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            api_key: None,
            enabled: true,
            risk_level: "read_only".to_string(),
            transport: "http".to_string(),
            command: None,
            args: vec![],
            env: HashMap::new(),
        };

        assert_eq!(config.id, "test-mcp");
        assert_eq!(config.host, "localhost");
        assert!(config.enabled);
    }

    #[test]
    fn test_tool_mapping() {
        let mapping = test_mapping("test.tool", "conductor.test");

        assert_eq!(mapping.mcp_tool_name, "test.tool");
        assert_eq!(mapping.conductor_tool_id, "conductor.test");
        assert!(mapping.enabled);
        assert_eq!(mapping.classification, McpToolClassification::Enabled);
    }

    #[test]
    fn provider_mapping_registry_roundtrip() {
        let _guard = isolated_registry();
        let config = test_config("roundtrip", 8081);
        register_mcp_provider(config.clone());
        add_tool_mapping(
            &config.id,
            test_mapping("remote.pet.set_avatar", "conductor.pet.set_avatar"),
        )
        .expect("add mapping");

        let providers = list_mcp_providers();
        assert!(providers.iter().any(|provider| provider.id == config.id));
        let mappings = list_tool_mappings(&config.id).expect("list mappings");
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].conductor_tool_id, "conductor.pet.set_avatar");
    }

    #[test]
    fn resolves_executor_target_by_conductor_tool_id() {
        let _guard = isolated_registry();
        let config = test_config("target-provider", 8082);
        register_mcp_provider(config.clone());
        add_tool_mapping(
            &config.id,
            test_mapping("remote.avatar", "conductor.pet.set_avatar"),
        )
        .expect("add mapping");

        let target = executor_target_for_tool("conductor.pet.set_avatar").expect("executor target");
        assert_eq!(target, (config.id, "remote.avatar".to_string()));
    }

    #[test]
    fn test_json_rpc_request_serialization() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: JsonRpcId::Num(1),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": { "listChanged": false } },
                "clientInfo": { "name": "test", "version": "0.1.0" }
            })),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"initialize\""));
        assert!(json.contains("\"id\":1"));
    }

    #[test]
    fn test_json_rpc_response_deserialization() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "test-server", "version": "1.0.0" }
            }
        }"#;

        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(response.error.is_none());
        assert!(response.result.is_some());

        let result: InitializeResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.protocolVersion, "2024-11-05");
        assert_eq!(result.serverInfo.name, "test-server");
    }

    #[test]
    fn test_tools_list_result_deserialization() {
        let json = r#"{
            "tools": [
                {
                    "name": "get_weather",
                    "description": "Get current weather",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "location": { "type": "string" }
                        },
                        "required": ["location"]
                    }
                }
            ]
        }"#;

        let result: ToolsListResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.tools.len(), 1);
        assert_eq!(result.tools[0].name, "get_weather");
    }

    #[test]
    fn test_tools_call_result_deserialization() {
        let json = r#"{
            "content": [
                { "type": "text", "text": "Sunny, 25°C" }
            ],
            "isError": false
        }"#;

        let result: ToolsCallResult = serde_json::from_str(json).unwrap();
        assert!(!result.isError);
        assert_eq!(result.content.len(), 1);
        match &result.content[0] {
            ToolCallContent::Text { text } => assert_eq!(text, "Sunny, 25°C"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_stdio_config() {
        let config = McpServerConfig {
            id: "stdio-server".to_string(),
            name: "Stdio MCP".to_string(),
            host: "".to_string(),
            port: 0,
            api_key: None,
            enabled: true,
            risk_level: "read_only".to_string(),
            transport: "stdio".to_string(),
            command: Some("my-mcp-server".to_string()),
            args: vec!["--verbose".to_string()],
            env: HashMap::from([("DEBUG".to_string(), "1".to_string())]),
        };

        assert_eq!(config.transport, "stdio");
        assert_eq!(config.command.as_deref(), Some("my-mcp-server"));
        assert_eq!(config.args, vec!["--verbose"]);
    }

    #[test]
    fn test_backward_compat_config_deserialization() {
        // Old config without transport field should default to "http"
        let json = r#"{
            "id": "old-server",
            "name": "Old Server",
            "host": "localhost",
            "port": 8080,
            "enabled": true,
            "risk_level": "read_only"
        }"#;

        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.transport, "http");
        assert!(config.command.is_none());
        assert!(config.args.is_empty());
    }

    // ── NEW TESTS: per-tool classification, risk mapping, stdio, audit ─────

    #[test]
    fn new_tool_defaults_to_pending_classification() {
        let _guard = isolated_registry();
        let config = test_config("classify-test", 8090);
        register_mcp_provider(config.clone());

        // Simulate adding a tool with default PendingClassification
        let mapping = McpToolMapping {
            mcp_tool_name: "read_file".to_string(),
            conductor_tool_id: "mcp.classify-test.read_file".to_string(),
            input_mapping: None,
            output_mapping: None,
            enabled: false,
            classification: McpToolClassification::PendingClassification,
            risk_level: None,
            permissions: vec![],
        };
        add_tool_mapping(&config.id, mapping).expect("add mapping");

        let mappings = list_tool_mappings(&config.id).expect("list mappings");
        assert_eq!(mappings.len(), 1);
        assert_eq!(
            mappings[0].classification,
            McpToolClassification::PendingClassification
        );
        assert!(!mappings[0].enabled);
    }

    #[test]
    fn per_tool_risk_override_takes_precedence() {
        let _guard = isolated_registry();
        let config = test_config("risk-override", 8091);
        register_mcp_provider(config.clone());

        // Provider default: read_only. Tool override: destructive.
        add_tool_mapping(
            &config.id,
            McpToolMapping {
                mcp_tool_name: "delete_repo".to_string(),
                conductor_tool_id: "mcp.risk-override.delete_repo".to_string(),
                input_mapping: None,
                output_mapping: None,
                enabled: true,
                classification: McpToolClassification::Enabled,
                risk_level: Some("destructive".to_string()),
                permissions: vec!["write_workspace".to_string()],
            },
        )
        .expect("add mapping");

        // Verify per-tool risk is stored
        let mappings = list_tool_mappings(&config.id).expect("list mappings");
        assert_eq!(mappings[0].risk_level.as_deref(), Some("destructive"));

        // Verify resolve_tool_risk_level picks tool override
        let risk = resolve_tool_risk_level(mappings[0].risk_level.as_deref(), &config.risk_level);
        assert_eq!(risk, crate::proposals::RiskLevel::Destructive);
    }

    #[test]
    fn per_tool_risk_falls_back_to_provider_default() {
        // When tool has no risk_level override, it should use provider default
        let risk = resolve_tool_risk_level(None, "workspace_write");
        assert_eq!(risk, crate::proposals::RiskLevel::WorkspaceWrite);
    }

    #[test]
    fn classify_tool_transitions_from_pending_to_enabled() {
        let _guard = isolated_registry();
        let config = test_config("classify-apply", 8092);
        register_mcp_provider(config.clone());

        // Add tool as PendingClassification
        add_tool_mapping(
            &config.id,
            McpToolMapping {
                mcp_tool_name: "write_file".to_string(),
                conductor_tool_id: "mcp.classify-apply.write_file".to_string(),
                input_mapping: None,
                output_mapping: None,
                enabled: false,
                classification: McpToolClassification::PendingClassification,
                risk_level: None,
                permissions: vec![],
            },
        )
        .expect("add mapping");

        // Classify it as Enabled with risk override
        classify_mcp_tool(
            &config.id,
            "write_file",
            McpToolClassification::Enabled,
            Some("workspace_write".to_string()),
            vec!["write_workspace".to_string()],
        )
        .expect("classify");

        let mappings = list_tool_mappings(&config.id).expect("list mappings");
        assert_eq!(mappings[0].classification, McpToolClassification::Enabled);
        assert!(mappings[0].enabled);
        assert_eq!(mappings[0].risk_level.as_deref(), Some("workspace_write"));
        assert_eq!(mappings[0].permissions, vec!["write_workspace"]);
    }

    #[test]
    fn same_provider_read_write_delete_get_separate_authorization() {
        let _guard = isolated_registry();
        let config = test_config("fs-provider", 8093);
        register_mcp_provider(config.clone());

        // Three tools from same provider with different risk levels
        for (name, risk, perms) in [
            ("read_file", "read_only", vec![]),
            (
                "write_file",
                "workspace_write",
                vec!["write_workspace".to_string()],
            ),
            (
                "delete_file",
                "destructive",
                vec!["write_workspace".to_string(), "system_control".to_string()],
            ),
        ] {
            add_tool_mapping(
                &config.id,
                McpToolMapping {
                    mcp_tool_name: name.to_string(),
                    conductor_tool_id: format!("mcp.fs-provider.{}", name),
                    input_mapping: None,
                    output_mapping: None,
                    enabled: true,
                    classification: McpToolClassification::Enabled,
                    risk_level: Some(risk.to_string()),
                    permissions: perms,
                },
            )
            .expect("add mapping");
        }

        let mappings = list_tool_mappings(&config.id).expect("list mappings");
        assert_eq!(mappings.len(), 3);

        // Verify each tool has its own risk level
        let read = mappings
            .iter()
            .find(|m| m.mcp_tool_name == "read_file")
            .unwrap();
        let write = mappings
            .iter()
            .find(|m| m.mcp_tool_name == "write_file")
            .unwrap();
        let delete = mappings
            .iter()
            .find(|m| m.mcp_tool_name == "delete_file")
            .unwrap();

        assert_eq!(read.risk_level.as_deref(), Some("read_only"));
        assert_eq!(write.risk_level.as_deref(), Some("workspace_write"));
        assert_eq!(delete.risk_level.as_deref(), Some("destructive"));

        // Verify resolved risk levels differ
        let read_risk = resolve_tool_risk_level(read.risk_level.as_deref(), &config.risk_level);
        let write_risk = resolve_tool_risk_level(write.risk_level.as_deref(), &config.risk_level);
        let delete_risk = resolve_tool_risk_level(delete.risk_level.as_deref(), &config.risk_level);

        assert!(read_risk < write_risk);
        assert!(write_risk < delete_risk);

        // Verify permissions differ
        assert!(resolve_tool_permissions(&read.permissions).is_empty());
        assert!(resolve_tool_permissions(&write.permissions)
            .contains(&crate::tools::ToolPermission::WriteWorkspace));
        assert!(resolve_tool_permissions(&delete.permissions)
            .contains(&crate::tools::ToolPermission::SystemControl));
    }

    #[test]
    fn pending_tools_lists_only_unclassified() {
        let _guard = isolated_registry();
        let config = test_config("pending-test", 8094);
        register_mcp_provider(config.clone());

        // Add one enabled, one pending
        add_tool_mapping(
            &config.id,
            test_mapping("enabled_tool", "mcp.pending-test.enabled_tool"),
        )
        .expect("add mapping");

        add_tool_mapping(
            &config.id,
            McpToolMapping {
                mcp_tool_name: "new_tool".to_string(),
                conductor_tool_id: "mcp.pending-test.new_tool".to_string(),
                input_mapping: None,
                output_mapping: None,
                enabled: false,
                classification: McpToolClassification::PendingClassification,
                risk_level: None,
                permissions: vec![],
            },
        )
        .expect("add mapping");

        let pending = pending_tools(&config.id).expect("pending tools");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].mcp_tool_name, "new_tool");
    }

    #[test]
    fn tool_mapping_serialization_roundtrip_with_new_fields() {
        let mapping = McpToolMapping {
            mcp_tool_name: "test.tool".to_string(),
            conductor_tool_id: "conductor.test".to_string(),
            input_mapping: None,
            output_mapping: None,
            enabled: true,
            classification: McpToolClassification::Enabled,
            risk_level: Some("workspace_write".to_string()),
            permissions: vec!["write_workspace".to_string(), "network".to_string()],
        };

        let json = serde_json::to_string(&mapping).unwrap();
        let deserialized: McpToolMapping = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.mcp_tool_name, "test.tool");
        assert_eq!(deserialized.classification, McpToolClassification::Enabled);
        assert_eq!(deserialized.risk_level.as_deref(), Some("workspace_write"));
        assert_eq!(deserialized.permissions, vec!["write_workspace", "network"]);
    }

    #[test]
    fn tool_mapping_backward_compat_deserialization() {
        // Old mapping JSON without new fields should deserialize with defaults
        let json = r#"{
            "mcp_tool_name": "old.tool",
            "conductor_tool_id": "conductor.old",
            "enabled": true
        }"#;

        let mapping: McpToolMapping = serde_json::from_str(json).unwrap();
        assert_eq!(
            mapping.classification,
            McpToolClassification::PendingClassification
        );
        assert!(mapping.risk_level.is_none());
        assert!(mapping.permissions.is_empty());
    }

    #[test]
    fn classification_serialization_variants() {
        let pending = serde_json::to_string(&McpToolClassification::PendingClassification).unwrap();
        let enabled = serde_json::to_string(&McpToolClassification::Enabled).unwrap();
        let disabled = serde_json::to_string(&McpToolClassification::Disabled).unwrap();

        assert_eq!(pending, "\"pending_classification\"");
        assert_eq!(enabled, "\"enabled\"");
        assert_eq!(disabled, "\"disabled\"");
    }

    #[test]
    fn stdio_process_framing_multiple_messages() {
        // Verify that StdioProcess correctly handles newline-delimited JSON framing.
        // This tests the parsing logic without spawning an actual process.
        let line1 = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"test","version":"1.0"}}}"#;
        let line2 = r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[]}}"#;
        let combined = format!("{}\n{}\n", line1, line2);

        let mut reader = std::io::BufReader::new(combined.as_bytes());
        let mut buf = String::new();

        // Read first message
        reader.read_line(&mut buf).unwrap();
        let resp1: JsonRpcResponse = serde_json::from_str(buf.trim()).unwrap();
        assert_eq!(resp1.id, Some(JsonRpcId::Num(1)));

        // Read second message
        buf.clear();
        reader.read_line(&mut buf).unwrap();
        let resp2: JsonRpcResponse = serde_json::from_str(buf.trim()).unwrap();
        assert_eq!(resp2.id, Some(JsonRpcId::Num(2)));
    }

    #[test]
    fn resolve_tool_permissions_maps_known_names() {
        let names = vec![
            "read_workspace".to_string(),
            "write_workspace".to_string(),
            "network".to_string(),
            "system_control".to_string(),
        ];
        let perms = resolve_tool_permissions(&names);
        assert_eq!(perms.len(), 4);
        assert!(perms.contains(&crate::tools::ToolPermission::ReadWorkspace));
        assert!(perms.contains(&crate::tools::ToolPermission::WriteWorkspace));
        assert!(perms.contains(&crate::tools::ToolPermission::Network));
        assert!(perms.contains(&crate::tools::ToolPermission::SystemControl));
    }

    #[test]
    fn resolve_tool_permissions_skips_unknown_names() {
        let names = vec![
            "read_workspace".to_string(),
            "totally_unknown_perm".to_string(),
        ];
        let perms = resolve_tool_permissions(&names);
        assert_eq!(perms.len(), 1);
        assert!(perms.contains(&crate::tools::ToolPermission::ReadWorkspace));
    }
}
