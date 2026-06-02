use crate::proposals::RiskLevel;
use std::path::PathBuf;

use super::registry::{
    ToolExecutionResult, ToolPermission, ToolProviderKind, ToolRegistry, ToolSpec,
};
use super::shared_runtime;

fn execute_codex_start(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let cwd = input
        .get("cwd")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing cwd"))?;
    let workspace_id = input
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let command = input
        .get("command")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let runtime = shared_runtime();
    let result = runtime.block_on(crate::codex::start_session(
        PathBuf::from(cwd),
        command,
        workspace_id,
    ));
    match result {
        Ok(session) => Ok(ToolExecutionResult {
            success: true,
            output: serde_json::to_value(&session)?,
            error: None,
            duration_ms: 0,
        }),
        Err(e) => Ok(ToolExecutionResult {
            success: false,
            output: serde_json::json!({ "error": e.to_string() }),
            error: Some(e.to_string()),
            duration_ms: 0,
        }),
    }
}

fn execute_codex_read_output(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing session_id"))?;
    let offset = input.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let runtime = shared_runtime();
    let result = runtime.block_on(crate::codex::read_output(session_id, offset));
    match result {
        Ok(output) => Ok(ToolExecutionResult {
            success: true,
            output: serde_json::to_value(output)?,
            error: None,
            duration_ms: 0,
        }),
        Err(e) => Ok(ToolExecutionResult {
            success: false,
            output: serde_json::json!({ "error": e.to_string() }),
            error: Some(e.to_string()),
            duration_ms: 0,
        }),
    }
}

fn execute_codex_send_input(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing session_id"))?;
    let text = input
        .get("input")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing input"))?;
    let runtime = shared_runtime();
    let result = runtime.block_on(crate::codex::send_input(session_id, text));
    match result {
        Ok(()) => Ok(ToolExecutionResult {
            success: true,
            output: serde_json::json!({ "success": true }),
            error: None,
            duration_ms: 0,
        }),
        Err(e) => Ok(ToolExecutionResult {
            success: false,
            output: serde_json::json!({ "error": e.to_string() }),
            error: Some(e.to_string()),
            duration_ms: 0,
        }),
    }
}

fn execute_codex_interrupt(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing session_id"))?;
    let runtime = shared_runtime();
    let result = runtime.block_on(crate::codex::interrupt_session(session_id));
    match result {
        Ok(()) => Ok(ToolExecutionResult {
            success: true,
            output: serde_json::json!({ "success": true }),
            error: None,
            duration_ms: 0,
        }),
        Err(e) => Ok(ToolExecutionResult {
            success: false,
            output: serde_json::json!({ "error": e.to_string() }),
            error: Some(e.to_string()),
            duration_ms: 0,
        }),
    }
}

fn execute_codex_resume(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing session_id"))?;
    let runtime = shared_runtime();
    let result = runtime.block_on(crate::codex::resume_session(session_id));
    match result {
        Ok(()) => Ok(ToolExecutionResult {
            success: true,
            output: serde_json::json!({ "success": true }),
            error: None,
            duration_ms: 0,
        }),
        Err(e) => Ok(ToolExecutionResult {
            success: false,
            output: serde_json::json!({ "error": e.to_string() }),
            error: Some(e.to_string()),
            duration_ms: 0,
        }),
    }
}

fn execute_codex_stop(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing session_id"))?;
    let runtime = shared_runtime();
    let result = runtime.block_on(crate::codex::stop_session(session_id));
    match result {
        Ok(()) => Ok(ToolExecutionResult {
            success: true,
            output: serde_json::json!({ "success": true }),
            error: None,
            duration_ms: 0,
        }),
        Err(e) => Ok(ToolExecutionResult {
            success: false,
            output: serde_json::json!({ "error": e.to_string() }),
            error: Some(e.to_string()),
            duration_ms: 0,
        }),
    }
}

fn execute_codex_get_session(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing session_id"))?;
    let runtime = shared_runtime();
    let result = runtime.block_on(crate::codex::get_session(session_id));
    match result {
        Ok(session) => Ok(ToolExecutionResult {
            success: true,
            output: serde_json::to_value(&session)?,
            error: None,
            duration_ms: 0,
        }),
        Err(e) => Ok(ToolExecutionResult {
            success: false,
            output: serde_json::json!({ "error": e.to_string() }),
            error: Some(e.to_string()),
            duration_ms: 0,
        }),
    }
}

fn execute_codex_list_sessions(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let limit = input
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|l| l as u32);
    let runtime = shared_runtime();
    let result = runtime.block_on(crate::codex::list_sessions_db(limit));
    match result {
        Ok(sessions) => Ok(ToolExecutionResult {
            success: true,
            output: serde_json::to_value(&sessions)?,
            error: None,
            duration_ms: 0,
        }),
        Err(e) => Ok(ToolExecutionResult {
            success: false,
            output: serde_json::json!({ "error": e.to_string() }),
            error: Some(e.to_string()),
            duration_ms: 0,
        }),
    }
}

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.register(
        ToolSpec {
            id: "codex.start".to_string(),
            name: "启动 Codex 会话".to_string(),
            description: "启动一个新的 Codex 交互式代理会话，在指定工作目录中运行。返回会话 ID 供后续操作使用。"
                .to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "cwd": { "type": "string", "description": "会话工作目录" },
                    "command": { "type": "string", "description": "要运行的命令（默认使用 codex 二进制）" },
                    "workspace_id": { "type": "string", "description": "关联的工作区 ID（可选）" }
                },
                "required": ["cwd"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "status": { "type": "string" },
                    "cwd": { "type": "string" }
                }
            }),
            risk_level: RiskLevel::ExternalSideEffect,
            permissions: vec![
                ToolPermission::ReadWorkspace,
                ToolPermission::WriteWorkspace,
            ],
            supports_dry_run: false,
            workspace_required: true,
        },
        execute_codex_start,
    );

    registry.register(
        ToolSpec {
            id: "codex.read_output".to_string(),
            name: "读取 Codex 会话输出".to_string(),
            description: "读取指定 Codex 会话的增量输出，从给定偏移量开始。用于跟踪会话的 stdout/stderr 内容。"
                .to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "会话 ID" },
                    "offset": { "type": "integer", "minimum": 0, "description": "从第几个字节开始读取（默认 0）" }
                },
                "required": ["session_id"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "stdout": { "type": "string" },
                    "stderr": { "type": "string" },
                    "exit_code": { "type": ["integer", "null"] }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: false,
            workspace_required: true,
        },
        execute_codex_read_output,
    );

    registry.register(
        ToolSpec {
            id: "codex.send_input".to_string(),
            name: "向 Codex 会话发送输入".to_string(),
            description:
                "向指定 Codex 会话的 stdin 发送键盘/文本输入。用于交互式会话中提供回答或命令。"
                    .to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "会话 ID" },
                    "input": { "type": "string", "description": "要发送的文本内容" }
                },
                "required": ["session_id", "input"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "success": { "type": "boolean" }
                }
            }),
            risk_level: RiskLevel::ExternalSideEffect,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: true,
        },
        execute_codex_send_input,
    );

    registry.register(
        ToolSpec {
            id: "codex.interrupt".to_string(),
            name: "中断 Codex 会话".to_string(),
            description: "向指定 Codex 会话发送中断信号（Ctrl-C / SIGINT），暂停当前执行。会话进入可恢复状态。"
                .to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "会话 ID" }
                },
                "required": ["session_id"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "success": { "type": "boolean" }
                }
            }),
            risk_level: RiskLevel::ExternalSideEffect,
            permissions: vec![ToolPermission::SystemControl],
            supports_dry_run: false,
            workspace_required: true,
        },
        execute_codex_interrupt,
    );

    registry.register(
        ToolSpec {
            id: "codex.resume".to_string(),
            name: "恢复 Codex 会话".to_string(),
            description:
                "恢复一个之前被中断的 Codex 会话，重新启动进程继续执行。支持从内存和数据库中恢复。"
                    .to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "会话 ID" }
                },
                "required": ["session_id"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "success": { "type": "boolean" }
                }
            }),
            risk_level: RiskLevel::ExternalSideEffect,
            permissions: vec![ToolPermission::SystemControl],
            supports_dry_run: false,
            workspace_required: true,
        },
        execute_codex_resume,
    );

    registry.register(
        ToolSpec {
            id: "codex.stop".to_string(),
            name: "停止 Codex 会话".to_string(),
            description: "停止一个正在运行的 Codex 会话并释放资源。会话进入可恢复状态，可以通过 resume 恢复。"
                .to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "会话 ID" }
                },
                "required": ["session_id"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "success": { "type": "boolean" }
                }
            }),
            risk_level: RiskLevel::ExternalSideEffect,
            permissions: vec![
                ToolPermission::SystemControl,
                ToolPermission::WriteWorkspace,
            ],
            supports_dry_run: false,
            workspace_required: true,
        },
        execute_codex_stop,
    );

    registry.register(
        ToolSpec {
            id: "codex.get_session".to_string(),
            name: "获取 Codex 会话详情".to_string(),
            description: "获取指定 Codex 会话的当前状态和详情。从内存或数据库中查找。".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "会话 ID" }
                },
                "required": ["session_id"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "status": { "type": "string" },
                    "command": { "type": "string" },
                    "cwd": { "type": "string" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: false,
            workspace_required: true,
        },
        execute_codex_get_session,
    );

    registry.register(
        ToolSpec {
            id: "codex.list_sessions".to_string(),
            name: "列出 Codex 会话".to_string(),
            description: "列出所有 Codex 会话（从数据库），按创建时间倒序。"
                .to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "description": "返回数量限制（默认 50）" }
                }
            }),
            output_schema: serde_json::json!({
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "status": { "type": "string" },
                        "command": { "type": "string" }
                    }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: false,
            workspace_required: true,
        },
        execute_codex_list_sessions,
    );
}
