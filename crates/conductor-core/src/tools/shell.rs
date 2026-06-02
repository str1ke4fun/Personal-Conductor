use crate::command_runs::{CommandRun, CommandRunStatus};
use crate::proposals::RiskLevel;

use super::registry::{
    ToolExecutionResult, ToolPermission, ToolProviderKind, ToolRegistry, ToolSpec,
};
use super::{current_workspace_root, resolve_workspace_path, shared_runtime};

fn execute_bash_tool(
    spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let command = input["command"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing required field: command"))?;

    let provider = match input["provider"].as_str() {
        Some("powershell") => crate::shell::ShellProvider::Powershell,
        Some("bash") => crate::shell::ShellProvider::Bash,
        _ => crate::shell::ShellProvider::Cmd,
    };

    let working_dir = match input["working_dir"].as_str() {
        Some(path) => Some(resolve_workspace_path(path)?.display().to_string()),
        None => Some(current_workspace_root().display().to_string()),
    };
    let timeout_secs = input["timeout_secs"].as_u64();

    let req = crate::shell::ShellRequest {
        command: command.to_string(),
        provider,
        working_dir,
        timeout_secs,
    };

    let runtime = shared_runtime();

    // Create CommandRun entity
    let cwd = req.working_dir.clone().unwrap_or_else(|| ".".to_string());
    let session_id = input["session_id"].as_str().map(|s| s.to_string());
    let mut command_run = CommandRun::new(command.to_string(), cwd, session_id);
    command_run.tool_call_id = input["tool_call_id"].as_str().map(|s| s.to_string());
    command_run.agent_run_id = input["agent_run_id"].as_str().map(|s| s.to_string());
    command_run.permission_grant_id = input["permission_grant_id"].as_str().map(|s| s.to_string());
    command_run.risk_level = Some(spec.risk_level.as_str().to_string());
    command_run.env_delta_json = input
        .get("env_delta")
        .map(serde_json::to_string)
        .transpose()?;

    // If caller wants AwaitingPermission, set it; otherwise go straight to Starting
    if input["await_permission"].as_bool().unwrap_or(false) {
        let _ = command_run.transition(CommandRunStatus::AwaitingPermission);
    }

    let command_run_id = command_run.id.clone();

    // Persist the initial CommandRun
    runtime.block_on(crate::command_runs::insert(&command_run))?;

    // Spawn the tracked execution in the background
    let executor = crate::shell::ShellExecutor::new();
    let cr = std::sync::Arc::new(tokio::sync::Mutex::new(command_run));
    let cr_clone = std::sync::Arc::clone(&cr);

    // Spawn a background task for execution
    tokio::runtime::Handle::current().spawn(async move {
        match executor.execute_tracked(req, cr_clone).await {
            Ok(spawned) => {
                // Wait for completion in background
                if let Err(e) = spawned.wait().await {
                    tracing::error!("command run execution error: {e}");
                }
            }
            Err(e) => {
                // Failed to spawn — update CommandRun
                tracing::error!("command run spawn error: {e}");
                let mut run = cr.lock().await;
                run.stderr_tail = e.to_string();
                let _ = run.transition(CommandRunStatus::Exited);
                run.exit_code = Some(-1);
                let _ = crate::command_runs::update(&run).await;
            }
        }
    });

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "command_run_id": command_run_id,
            "status": "started",
        }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_bash_cancel_tool(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let command_id = input["command_id"]
        .as_str()
        .or_else(|| input["command_run_id"].as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required field: command_id or command_run_id"))?;

    let runtime = shared_runtime();
    let result = runtime.block_on(crate::command_runs::kill(command_id))?;

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "command_run_id": result.id,
            "status": result.status.as_str(),
            "exit_code": result.exit_code,
            "stdout_tail": result.stdout_tail,
            "stderr_tail": result.stderr_tail,
        }),
        error: None,
        duration_ms: 0,
    })
}

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.register(
        ToolSpec {
            id: "bash.execute".to_string(),
            name: "执行命令".to_string(),
            description:
                "在系统 Shell 中执行命令并返回 command_run_id。支持 cmd、PowerShell、bash 三种 shell。命令在后台异步执行，实时输出通过事件推送。"
                    .to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "要执行的 Shell 命令"
                    },
                    "provider": {
                        "type": "string",
                        "enum": ["cmd", "powershell", "bash"],
                        "description": "Shell 类型 (默认 cmd)"
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "工作目录 (默认当前目录)"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "超时秒数 (默认 120)"
                    },
                    "session_id": {
                        "type": "string",
                        "description": "关联的会话 ID (可选)"
                    },
                    "await_permission": {
                        "type": "boolean",
                        "description": "是否等待用户许可后执行 (默认 false)"
                    }
                },
                "required": ["command"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command_run_id": { "type": "string" },
                    "status": { "type": "string" }
                }
            }),
            risk_level: RiskLevel::ExternalSideEffect,
            permissions: vec![ToolPermission::SystemControl],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_bash_tool,
    );

    registry.register(
        ToolSpec {
            id: "bash.cancel".to_string(),
            name: "终止命令".to_string(),
            description: "通过 command_run_id 终止一个正在运行的 Shell 命令。".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command_id": {
                        "type": "string",
                        "description": "要终止的命令 ID"
                    },
                    "command_run_id": {
                        "type": "string",
                        "description": "要终止的命令 ID (别名)"
                    }
                }
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command_run_id": { "type": "string" },
                    "status": { "type": "string" },
                    "exit_code": { "type": "integer" },
                    "stdout_tail": { "type": "string" },
                    "stderr_tail": { "type": "string" }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::SystemControl],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_bash_cancel_tool,
    );
}
