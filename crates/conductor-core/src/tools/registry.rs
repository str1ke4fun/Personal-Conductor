use crate::app_state::AppState;
use crate::proposals::RiskLevel;
use anyhow::bail;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

use super::{shared_runtime, WorkspaceRootGuard};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolProviderKind {
    Internal,
    Cli,
    Subagent,
    Mcp,
}

impl ToolProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Internal => "internal",
            Self::Cli => "cli",
            Self::Subagent => "subagent",
            Self::Mcp => "mcp",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolPermission {
    ReadWorkspace,
    WriteWorkspace,
    ReadExternalPath,
    WriteExternalPath,
    Network,
    SendMessage,
    SystemControl,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolSpec {
    pub id: String,
    pub name: String,
    pub description: String,
    pub provider: ToolProviderKind,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub risk_level: RiskLevel,
    pub permissions: Vec<ToolPermission>,
    pub supports_dry_run: bool,
    pub workspace_required: bool,
}

#[derive(Debug)]
pub struct ToolExecutionResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
    pub duration_ms: u64,
}

pub type ToolExecutorFn =
    fn(&ToolSpec, &serde_json::Value) -> Result<ToolExecutionResult, anyhow::Error>;

impl PartialOrd for RiskLevel {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RiskLevel {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let order = match self {
            RiskLevel::ReadOnly => 0,
            RiskLevel::DraftOnly => 1,
            RiskLevel::WorkspaceWrite => 2,
            RiskLevel::ExternalSideEffect => 3,
            RiskLevel::Destructive => 4,
        };
        let other_order = match other {
            RiskLevel::ReadOnly => 0,
            RiskLevel::DraftOnly => 1,
            RiskLevel::WorkspaceWrite => 2,
            RiskLevel::ExternalSideEffect => 3,
            RiskLevel::Destructive => 4,
        };
        order.cmp(&other_order)
    }
}

pub struct ToolRegistry {
    tools: HashMap<String, (ToolSpec, ToolExecutorFn)>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, spec: ToolSpec, executor: ToolExecutorFn) {
        self.tools.insert(spec.id.clone(), (spec, executor));
    }

    pub fn get(&self, tool_id: &str) -> Option<&(ToolSpec, ToolExecutorFn)> {
        self.tools.get(tool_id)
    }

    pub fn list(&self) -> Vec<&ToolSpec> {
        self.tools.values().map(|(spec, _)| spec).collect()
    }

    pub fn list_by_risk(&self, max_risk: RiskLevel) -> Vec<&ToolSpec> {
        self.tools
            .values()
            .filter(|(spec, _)| spec.risk_level <= max_risk)
            .map(|(spec, _)| spec)
            .collect()
    }
}

pub fn register_tool(spec: ToolSpec, executor: ToolExecutorFn) {
    let mut registry = AppState::global().tool_registry().write().unwrap();
    registry.register(spec, executor);
}

pub fn get_tool(tool_id: &str) -> Option<(ToolSpec, ToolExecutorFn)> {
    let registry = AppState::global().tool_registry().read().unwrap();
    registry.get(tool_id).cloned()
}

pub fn list_tools() -> Vec<ToolSpec> {
    let registry = AppState::global().tool_registry().read().unwrap();
    registry.list().iter().cloned().cloned().collect()
}

pub fn execute_tool(
    tool_id: &str,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let (spec, executor) =
        get_tool(tool_id).ok_or_else(|| anyhow::anyhow!("tool not found: {}", tool_id))?;

    validate_input(&spec, input)?;
    run_tool_executor(&spec, executor, input)
}

pub fn execute_tool_with_workspace(
    tool_id: &str,
    input: &serde_json::Value,
    workspace_id: Option<&str>,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let (spec, executor) =
        get_tool(tool_id).ok_or_else(|| anyhow::anyhow!("tool not found: {}", tool_id))?;

    validate_input(&spec, input)?;
    let effective_spec = effective_spec_for_input(&spec, input)?;
    validate_workspace_context(&effective_spec, workspace_id)?;
    let workspace_root = workspace_id
        .map(|id| {
            shared_runtime()
                .block_on(crate::workspaces::get(id))
                .map(|ws| ws.root)
        })
        .transpose()?;
    let _guard = WorkspaceRootGuard::push(workspace_root);
    run_tool_executor(&effective_spec, executor, input)
}

pub async fn execute_tool_with_workspace_async(
    tool_id: &str,
    input: &serde_json::Value,
    workspace_id: Option<&str>,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let (spec, executor) =
        get_tool(tool_id).ok_or_else(|| anyhow::anyhow!("tool not found: {}", tool_id))?;

    validate_input(&spec, input)?;
    let effective_spec = effective_spec_for_input(&spec, input)?;
    validate_workspace_context_async(&effective_spec, workspace_id).await?;
    let workspace_root = match workspace_id {
        Some(id) => Some(crate::workspaces::get(id).await?.root),
        None => None,
    };

    let spec_for_worker = effective_spec.clone();
    let input_for_worker = input.clone();
    tokio::task::spawn_blocking(move || {
        let _guard = WorkspaceRootGuard::push(workspace_root);
        run_tool_executor(&spec_for_worker, executor, &input_for_worker)
    })
    .await?
}

fn run_tool_executor(
    spec: &ToolSpec,
    executor: ToolExecutorFn,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let start = Instant::now();
    let result = executor(spec, input);
    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(mut res) => {
            res.duration_ms = duration_ms;
            Ok(res)
        }
        Err(err) => Err(err),
    }
}

pub(crate) fn validate_input(
    spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<(), anyhow::Error> {
    if !input.is_object() {
        bail!("input must be a JSON object");
    }

    let input_obj = input.as_object().unwrap();
    let schema_obj = spec.input_schema.as_object();

    if let Some(schema) = schema_obj {
        if let Some(required) = schema.get("required") {
            if let Some(required_array) = required.as_array() {
                for req in required_array {
                    if let Some(field) = req.as_str() {
                        if !input_obj.contains_key(field) {
                            bail!("missing required field: {}", field);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn effective_spec_for_input(
    spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolSpec, anyhow::Error> {
    if spec.id != "bash.execute" {
        return Ok(spec.clone());
    }

    let mut effective = spec.clone();
    let Some(command) = input.get("command").and_then(|value| value.as_str()) else {
        return Ok(effective);
    };

    effective.risk_level = crate::shell::security::classify_command_risk(command)?;
    if effective.risk_level == RiskLevel::ReadOnly {
        effective.permissions = vec![ToolPermission::ReadWorkspace];
    } else if effective.risk_level == RiskLevel::WorkspaceWrite {
        effective.permissions = vec![
            ToolPermission::ReadWorkspace,
            ToolPermission::WriteWorkspace,
        ];
    }
    Ok(effective)
}

fn validate_workspace_context(
    spec: &ToolSpec,
    workspace_id: Option<&str>,
) -> Result<(), anyhow::Error> {
    if spec.workspace_required && workspace_id.is_none() {
        bail!("tool requires workspace context: {}", spec.id);
    }

    let Some(workspace_id) = workspace_id else {
        return Ok(());
    };

    let runtime = shared_runtime();
    let workspace = runtime.block_on(crate::workspaces::get(workspace_id))?;
    validate_trust_level(spec, workspace_id, &workspace.trust_level)
}

async fn validate_workspace_context_async(
    spec: &ToolSpec,
    workspace_id: Option<&str>,
) -> Result<(), anyhow::Error> {
    if spec.workspace_required && workspace_id.is_none() {
        bail!("tool requires workspace context: {}", spec.id);
    }

    let Some(workspace_id) = workspace_id else {
        return Ok(());
    };

    let workspace = crate::workspaces::get(workspace_id).await?;
    validate_trust_level(spec, workspace_id, &workspace.trust_level)
}

/// Returns `true` when the tool's risk level requires explicit user approval
/// under the given workspace trust level.
///
/// - `Trusted` workspaces: nothing needs approval (all tools pass).
/// - `AskWrite` workspaces: `WorkspaceWrite` and above need approval.
/// - `ReadOnly` / `Untrusted`: handled separately by `validate_trust_level`.
pub fn needs_approval(spec: &ToolSpec, trust_level: &crate::workspaces::TrustLevel) -> bool {
    match trust_level {
        crate::workspaces::TrustLevel::Trusted => false,
        crate::workspaces::TrustLevel::AskWrite => spec.risk_level >= RiskLevel::WorkspaceWrite,
        // ReadOnly and Untrusted are hard-blocked, not "needs approval"
        crate::workspaces::TrustLevel::ReadOnly | crate::workspaces::TrustLevel::Untrusted => false,
    }
}

fn validate_trust_level(
    spec: &ToolSpec,
    workspace_id: &str,
    trust_level: &crate::workspaces::TrustLevel,
) -> Result<(), anyhow::Error> {
    match trust_level {
        crate::workspaces::TrustLevel::Trusted => Ok(()),
        crate::workspaces::TrustLevel::AskWrite => {
            if spec.risk_level >= RiskLevel::WorkspaceWrite {
                bail!(
                    "approval_required: tool '{}' with risk_level '{}' requires approval in AskWrite workspace {}",
                    spec.id,
                    spec.risk_level.as_str(),
                    workspace_id
                );
            }
            Ok(())
        }
        crate::workspaces::TrustLevel::ReadOnly => {
            if spec.permissions.contains(&ToolPermission::WriteWorkspace)
                || spec.risk_level >= RiskLevel::WorkspaceWrite
            {
                bail!("workspace is read_only: {}", workspace_id);
            }
            Ok(())
        }
        crate::workspaces::TrustLevel::Untrusted => {
            if spec.risk_level > RiskLevel::ReadOnly {
                bail!("workspace is untrusted: {}", workspace_id);
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bash_spec() -> ToolSpec {
        ToolSpec {
            id: "bash.execute".to_string(),
            name: "execute".to_string(),
            description: "execute shell command".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["command"],
                "properties": {
                    "command": { "type": "string" }
                }
            }),
            output_schema: serde_json::json!({}),
            risk_level: RiskLevel::ExternalSideEffect,
            permissions: vec![ToolPermission::SystemControl],
            supports_dry_run: false,
            workspace_required: false,
        }
    }

    #[test]
    fn bash_execute_read_only_command_lowers_effective_risk() {
        let effective = effective_spec_for_input(
            &bash_spec(),
            &serde_json::json!({ "command": "findstr /s /n TODO src\\*.rs" }),
        )
        .expect("effective spec");

        assert_eq!(effective.risk_level, RiskLevel::ReadOnly);
        assert_eq!(effective.permissions, vec![ToolPermission::ReadWorkspace]);
    }

    #[test]
    fn bash_execute_write_command_keeps_write_risk() {
        let effective = effective_spec_for_input(
            &bash_spec(),
            &serde_json::json!({ "command": "mkdir tmp-output" }),
        )
        .expect("effective spec");

        assert_eq!(effective.risk_level, RiskLevel::WorkspaceWrite);
        assert_eq!(
            effective.permissions,
            vec![
                ToolPermission::ReadWorkspace,
                ToolPermission::WriteWorkspace
            ]
        );
    }
}
