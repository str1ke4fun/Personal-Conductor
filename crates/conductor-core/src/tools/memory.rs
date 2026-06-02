use crate::proposals::RiskLevel;

use super::registry::{
    ToolExecutionResult, ToolPermission, ToolProviderKind, ToolRegistry, ToolSpec,
};
use super::shared_runtime;

fn execute_memory_get(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let key = input
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing key"))?;

    let runtime = shared_runtime();
    let value = runtime.block_on(crate::memory::get(key))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "key": key, "value": value }),
        error: None,
        duration_ms: 0,
    })
}

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.register(
        ToolSpec {
            id: "memory.get".to_string(),
            name: "获取记忆".to_string(),
            description: "获取指定键的记忆值".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string" }
                },
                "required": ["key"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string" },
                    "value": { "type": ["string", "null"] }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_memory_get,
    );
}
