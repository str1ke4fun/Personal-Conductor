use crate::proposals::RiskLevel;

use super::registry::{
    ToolExecutionResult, ToolPermission, ToolProviderKind, ToolRegistry, ToolSpec,
};
use super::shared_runtime;

fn task_output_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" },
            "task_list_id": { "type": "string" },
            "subject": { "type": "string" },
            "description": { "type": "string" },
            "active_form": { "type": ["string", "null"] },
            "owner": { "type": ["string", "null"] },
            "status": {
                "type": "string",
                "enum": ["pending", "in_progress", "completed"]
            },
            "workspace_id": { "type": ["string", "null"] },
            "source": { "type": "string" },
            "kind": { "type": "string" },
            "blocks": { "type": "array", "items": { "type": "string" } },
            "blocked_by": { "type": "array", "items": { "type": "string" } },
            "metadata_json": { "type": ["object", "null"] },
            "created_at": { "type": "string" },
            "updated_at": { "type": "string" }
        }
    })
}

fn execute_tasks_list(
    _spec: &ToolSpec,
    _input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let runtime = shared_runtime();
    let tasks = runtime.block_on(crate::tasks::load())?;
    let pending_tasks: Vec<serde_json::Value> = tasks
        .pending()
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "kind": t.kind,
                "status": t.status.as_str(),
                "created_at": t.created_at.to_rfc3339(),
            })
        })
        .collect();
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "tasks": pending_tasks }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_tasks_show(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let task_id = input
        .get("task_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing task_id"))?;

    let runtime = shared_runtime();
    let tasks = runtime.block_on(crate::tasks::load())?;
    let task = tasks
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| anyhow::anyhow!("task not found: {}", task_id))?;

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "id": task.id,
            "source": task.source,
            "kind": task.kind,
            "status": task.status.as_str(),
            "created_at": task.created_at.to_rfc3339(),
            "summary_ref": task.summary_ref,
            "focus_hint": task.focus_hint,
        }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_task_create(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let create_input: crate::tasklist::TaskCreateInput = serde_json::from_value(input.clone())?;
    let runtime = shared_runtime();
    let task = runtime.block_on(crate::tasklist::create_task(create_input))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(task)?,
        error: None,
        duration_ms: 0,
    })
}

fn execute_task_list(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let filter: crate::tasklist::TaskListFilter = serde_json::from_value(input.clone())?;
    let runtime = shared_runtime();
    let tasks = runtime.block_on(crate::tasklist::list_tasks(filter))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "tasks": tasks }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_task_get(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let task_id = input
        .get("task_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing task_id"))?;
    let task_list_id = input.get("task_list_id").and_then(|v| v.as_str());
    let runtime = shared_runtime();
    let task = runtime.block_on(crate::tasklist::get_task(task_list_id, task_id))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(task)?,
        error: None,
        duration_ms: 0,
    })
}

fn execute_task_update(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let update_input: crate::tasklist::TaskUpdateInput = serde_json::from_value(input.clone())?;
    let runtime = shared_runtime();
    let task = runtime.block_on(crate::tasklist::update_task(update_input))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(task)?,
        error: None,
        duration_ms: 0,
    })
}

fn execute_task_claim(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let task_id = input
        .get("task_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing task_id"))?;
    let owner = input
        .get("owner")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing owner"))?;
    let task_list_id = input.get("task_list_id").and_then(|v| v.as_str());
    let runtime = shared_runtime();
    let task = runtime.block_on(crate::tasklist::claim_task(task_list_id, task_id, owner))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(task)?,
        error: None,
        duration_ms: 0,
    })
}

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.register(
        ToolSpec {
            id: "tasks.list".to_string(),
            name: "列出任务".to_string(),
            description: "列出待处理的任务".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "kind": { "type": "string" },
                                "status": { "type": "string" },
                                "created_at": { "type": "string" }
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
        execute_tasks_list,
    );

    registry.register(
        ToolSpec {
            id: "tasks.show".to_string(),
            name: "显示任务详情".to_string(),
            description: "显示指定任务的详细信息".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" }
                },
                "required": ["task_id"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "source": { "type": "string" },
                    "kind": { "type": "string" },
                    "status": { "type": "string" },
                    "created_at": { "type": "string" },
                    "summary_ref": { "type": ["string", "null"] },
                    "focus_hint": { "type": ["string", "null"] }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_tasks_show,
    );

    registry.register(
        ToolSpec {
            id: "task.create".to_string(),
            name: "创建任务".to_string(),
            description: "创建 TaskList V2 任务".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_list_id": { "type": "string" },
                    "workspace_id": { "type": "string" },
                    "subject": { "type": "string" },
                    "description": { "type": "string" },
                    "active_form": { "type": "string" },
                    "owner": { "type": "string" },
                    "source": { "type": "string" },
                    "kind": { "type": "string" },
                    "metadata": { "type": "object" },
                    "blocks": { "type": "array", "items": { "type": "string" } },
                    "blocked_by": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["subject"]
            }),
            output_schema: task_output_schema(),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_task_create,
    );

    registry.register(
        ToolSpec {
            id: "task.list".to_string(),
            name: "列出任务".to_string(),
            description: "列出 TaskList V2 任务".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_list_id": { "type": "string" },
                    "workspace_id": { "type": "string" },
                    "status": {
                        "type": "string",
                        "enum": ["pending", "in_progress", "completed"]
                    },
                    "owner": { "type": "string" },
                    "include_completed": { "type": "boolean" },
                    "available_only": { "type": "boolean" }
                }
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "items": task_output_schema()
                    }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_task_list,
    );

    registry.register(
        ToolSpec {
            id: "task.get".to_string(),
            name: "读取任务".to_string(),
            description: "读取 TaskList V2 任务详情".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_list_id": { "type": "string" },
                    "task_id": { "type": "string" }
                },
                "required": ["task_id"]
            }),
            output_schema: task_output_schema(),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_task_get,
    );

    registry.register(
        ToolSpec {
            id: "task.update".to_string(),
            name: "更新任务".to_string(),
            description: "更新 TaskList V2 任务字段和依赖关系".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_list_id": { "type": "string" },
                    "task_id": { "type": "string" },
                    "subject": { "type": "string" },
                    "description": { "type": "string" },
                    "active_form": { "type": "string" },
                    "owner": { "type": "string" },
                    "status": {
                        "type": "string",
                        "enum": ["pending", "in_progress", "completed"]
                    },
                    "metadata": { "type": "object" },
                    "add_blocks": { "type": "array", "items": { "type": "string" } },
                    "add_blocked_by": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["task_id"]
            }),
            output_schema: task_output_schema(),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_task_update,
    );

    registry.register(
        ToolSpec {
            id: "task.claim".to_string(),
            name: "领取任务".to_string(),
            description: "用 SQLite 事务原子领取一个未阻塞的 pending 任务".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "task_list_id": { "type": "string" },
                    "task_id": { "type": "string" },
                    "owner": { "type": "string" }
                },
                "required": ["task_id", "owner"]
            }),
            output_schema: task_output_schema(),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_task_claim,
    );
}
