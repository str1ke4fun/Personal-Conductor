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

fn execute_memory_set(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let key = input
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing key"))?;
    let value = input
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing value"))?;
    let category = input
        .get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("general");

    let runtime = shared_runtime();
    let entry = runtime.block_on(crate::memory::set(key, value, category))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "id": entry.id, "key": key }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_memory_delete(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let key = input
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing key"))?;

    let runtime = shared_runtime();
    let deleted = runtime.block_on(crate::memory::forget(key))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "key": key, "deleted": deleted }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_memory_search(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing query"))?;
    let limit = input
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n.clamp(1, 20) as usize)
        .unwrap_or(5);

    let runtime = shared_runtime();
    let results = runtime.block_on(crate::memory::search_memory(query, None, limit))?;
    let items: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "memory_id": r.chunk.memory_id,
                "category":  r.chunk.category,
                "content":   r.chunk.content,
                "score": r.score,
            })
        })
        .collect();
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "results": items }),
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

    registry.register(
        ToolSpec {
            id: "memory.set".to_string(),
            name: "保存记忆".to_string(),
            description: "保存或更新一条记忆（key/value 形式，category 可选）".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key":      { "type": "string", "description": "记忆的唯一键" },
                    "value":    { "type": "string", "description": "要保存的内容" },
                    "category": { "type": "string", "description": "分类，如 preference/fact/task，默认 general" }
                },
                "required": ["key", "value"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id":  { "type": "string" },
                    "key": { "type": "string" }
                }
            }),
            risk_level: RiskLevel::DraftOnly,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_memory_set,
    );

    registry.register(
        ToolSpec {
            id: "memory.delete".to_string(),
            name: "删除记忆".to_string(),
            description: "将指定键的记忆标记为已遗忘（软删除）".to_string(),
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
                    "key":     { "type": "string" },
                    "deleted": { "type": "boolean" }
                }
            }),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_memory_delete,
    );

    registry.register(
        ToolSpec {
            id: "memory.search".to_string(),
            name: "搜索记忆".to_string(),
            description: "通过关键词或语义搜索记忆，返回最相关的条目".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "搜索词或自然语言描述" },
                    "limit": { "type": "integer", "description": "最多返回条数，默认 5，最大 20" }
                },
                "required": ["query"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "results": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "memory_id": { "type": "string" },
                                "category":  { "type": "string" },
                                "content":   { "type": "string" },
                                "score":     { "type": "number" }
                            }
                        }
                    }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_memory_search,
    );
}
