use crate::proposals::RiskLevel;
use std::path::PathBuf;

use super::registry::{
    ToolExecutionResult, ToolPermission, ToolProviderKind, ToolRegistry, ToolSpec,
};
use super::shared_runtime;

const DEFAULT_SUBAGENT_TIMEOUT_SECONDS: u64 = 600;
const MIN_SUBAGENT_TIMEOUT_SECONDS: u64 = 300;
const MAX_SUBAGENT_TIMEOUT_SECONDS: u64 = 3600;

fn execute_subagent_claude_p(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing prompt"))?;
    let cwd = input
        .get("cwd")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);
    let timeout_seconds = input
        .get("timeout_seconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_SUBAGENT_TIMEOUT_SECONDS)
        .clamp(MIN_SUBAGENT_TIMEOUT_SECONDS, MAX_SUBAGENT_TIMEOUT_SECONDS);
    let workspace_id = input
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    let tool_call_id = input
        .get("tool_call_id")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    let task_id = input
        .get("task_id")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    let team_id = input
        .get("team_id")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    let agent_member_id = input
        .get("agent_member_id")
        .and_then(|v| v.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);

    let runtime = shared_runtime();
    let run = runtime.block_on(crate::agent_runs::start_claude_run(
        crate::agent_runs::StartAgentRunInput {
            agent_id: input
                .get("agent_id")
                .and_then(|v| v.as_str())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("claude")
                .to_string(),
            role: input
                .get("role")
                .and_then(|v| v.as_str())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("subagent")
                .to_string(),
            workspace_id,
            cwd,
            prompt: prompt.to_string(),
            timeout_seconds,
            metadata: Some(serde_json::json!({
                "source": "subagent.claude_p",
                "session_id": session_id,
                "tool_call_id": tool_call_id,
                "task_id": task_id,
                "team_id": team_id,
                "agent_id": agent_member_id,
            })),
        },
    ))?;

    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({
            "agent_run_id": run.id.clone(),
            "run": run,
            "stdout": "",
            "stderr": "",
            "exit_code": null,
            "duration_ms": 0,
            "log_path": null,
            "timed_out": false,
            "status": "running",
            "message": "started background claude -p run; use agent.read_output with agent_run_id to read results"
        }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_agent_start(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let start_input: crate::agent_runs::StartAgentRunInput = serde_json::from_value(input.clone())?;
    let runtime = shared_runtime();
    let run = runtime.block_on(crate::agent_runs::start_claude_run(start_input))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(run)?,
        error: None,
        duration_ms: 0,
    })
}

fn execute_agent_read_output(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let run_id = input
        .get("run_id")
        .or_else(|| input.get("id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing run_id"))?;
    let max_bytes = input
        .get("max_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(16_384)
        .clamp(1, 1_000_000) as usize;
    let runtime = shared_runtime();
    let output = runtime.block_on(crate::agent_runs::read_output(run_id, max_bytes))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(output)?,
        error: None,
        duration_ms: 0,
    })
}

fn execute_agent_stop(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let run_id = input
        .get("run_id")
        .or_else(|| input.get("id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing run_id"))?;
    let runtime = shared_runtime();
    let run = runtime.block_on(crate::agent_runs::stop(run_id))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(run)?,
        error: None,
        duration_ms: 0,
    })
}

fn execute_agent_team_create(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let create_input: crate::agent_teams::CreateAgentTeamInput =
        serde_json::from_value(input.clone())?;
    let runtime = shared_runtime();
    let team = runtime.block_on(crate::agent_teams::create_team(create_input))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(team)?,
        error: None,
        duration_ms: 0,
    })
}

fn execute_agent_team_list(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let workspace_id = input.get("workspace_id").and_then(|v| v.as_str());
    let include_archived = input
        .get("include_archived")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let runtime = shared_runtime();
    let teams = runtime.block_on(crate::agent_teams::list_teams(
        workspace_id,
        include_archived,
    ))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "teams": teams }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_agent_team_add_member(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let add_input: crate::agent_teams::AddAgentTeamMemberInput =
        serde_json::from_value(input.clone())?;
    let runtime = shared_runtime();
    let member = runtime.block_on(crate::agent_teams::add_member(add_input))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(member)?,
        error: None,
        duration_ms: 0,
    })
}

fn execute_agent_team_snapshot(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let team_id = input
        .get("team_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing team_id"))?;
    let message_limit = input
        .get("message_limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .clamp(1, 200) as u32;
    let runtime = shared_runtime();
    let snapshot = runtime.block_on(crate::agent_teams::snapshot(team_id, message_limit))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(snapshot)?,
        error: None,
        duration_ms: 0,
    })
}

fn execute_agent_team_plan_verdict(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let team_id = input
        .get("team_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing team_id"))?;
    let verdict = input
        .get("verdict")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing verdict"))?;
    let runtime = shared_runtime();
    let team = runtime.block_on(crate::agent_teams::handle_plan_approval_response(
        team_id,
        crate::agent_teams::PlanApprovalVerdict::from_str(verdict)?,
    ))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(team)?,
        error: None,
        duration_ms: 0,
    })
}

fn execute_agent_team_review_verdict(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let team_id = input
        .get("team_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing team_id"))?;
    let verdict = input
        .get("verdict")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing verdict"))?;
    let parsed = match verdict.trim().to_ascii_lowercase().as_str() {
        "accepted" | "accept" | "approved" => crate::agent_teams::ReviewVerdict::Accepted,
        "failed" | "fail" | "rejected" | "reject" | "rework_required" => {
            crate::agent_teams::ReviewVerdict::Failed
        }
        other => anyhow::bail!("unknown review verdict: {other}"),
    };
    let runtime = shared_runtime();
    let team = runtime.block_on(crate::agent_teams::handle_review_verdict(team_id, parsed))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(team)?,
        error: None,
        duration_ms: 0,
    })
}

fn execute_agent_mailbox_send(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let send_input: crate::agent_teams::SendAgentMessageInput =
        serde_json::from_value(input.clone())?;
    let runtime = shared_runtime();
    let messages = runtime.block_on(crate::agent_teams::send_message(send_input))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "messages": messages }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_agent_mailbox_list(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let filter: crate::agent_teams::AgentMailboxFilter = serde_json::from_value(input.clone())?;
    let runtime = shared_runtime();
    let messages = runtime.block_on(crate::agent_teams::list_mailbox(filter))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::json!({ "messages": messages }),
        error: None,
        duration_ms: 0,
    })
}

fn execute_agent_mailbox_mark_read(
    _spec: &ToolSpec,
    input: &serde_json::Value,
) -> Result<ToolExecutionResult, anyhow::Error> {
    let message_id = input
        .get("message_id")
        .or_else(|| input.get("id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing message_id"))?;
    let runtime = shared_runtime();
    let message = runtime.block_on(crate::agent_teams::mark_message_read(message_id))?;
    Ok(ToolExecutionResult {
        success: true,
        output: serde_json::to_value(message)?,
        error: None,
        duration_ms: 0,
    })
}

pub(super) fn register(registry: &mut ToolRegistry) {
    registry.register(
        ToolSpec {
            id: "subagent.claude_p".to_string(),
            name: "运行 Claude 子 Agent".to_string(),
            description: "通过 claude -p 在指定工作目录启动后台子 Agent，立即返回 agent_run_id；用 agent.read_output 读取结果".to_string(),
            provider: ToolProviderKind::Subagent,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string" },
                    "cwd": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "role": { "type": "string" },
                    "workspace_id": { "type": "string" },
                    "timeout_seconds": { "type": "integer", "minimum": 300, "maximum": 3600, "default": 600 },
                    "task_id": { "type": "string", "description": "goal_tasks.id to write result_ref back on completion" },
                    "team_id": { "type": "string", "description": "agent_teams.id for team lifecycle tracking" },
                    "agent_member_id": { "type": "string", "description": "agent_teams member agent_id for status updates" }
                },
                "required": ["prompt"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent_run_id": { "type": ["string", "null"] },
                    "run": { "type": "object" },
                    "status": { "type": "string" },
                    "stdout": { "type": "string" },
                    "stderr": { "type": "string" },
                    "exit_code": { "type": ["integer", "null"] },
                    "duration_ms": { "type": "integer" },
                    "log_path": { "type": ["string", "null"] },
                    "timed_out": { "type": "boolean" },
                    "message": { "type": "string" }
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
        execute_subagent_claude_p,
    );

    registry.register(
        ToolSpec {
            id: "agent.start".to_string(),
            name: "启动后台 Agent".to_string(),
            description: "启动一个可追踪的后台 claude -p Agent 运行，并记录 agent_runs 状态"
                .to_string(),
            provider: ToolProviderKind::Subagent,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "role": { "type": "string" },
                    "workspace_id": { "type": "string" },
                    "cwd": { "type": "string" },
                    "prompt": { "type": "string" },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 86400 },
                    "metadata": { "type": "object" }
                },
                "required": ["prompt"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "role": { "type": "string" },
                    "workspace_id": { "type": ["string", "null"] },
                    "cwd": { "type": ["string", "null"] },
                    "status": { "type": "string" },
                    "pid": { "type": ["integer", "null"] },
                    "input_ref": { "type": ["string", "null"] },
                    "output_ref": { "type": ["string", "null"] },
                    "error": { "type": ["string", "null"] }
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
        execute_agent_start,
    );

    registry.register(
        ToolSpec {
            id: "agent.read_output".to_string(),
            name: "读取 Agent 输出".to_string(),
            description: "读取 agent_runs 中指定后台 Agent 的输出摘要或尾部内容".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" },
                    "max_bytes": { "type": "integer", "minimum": 1, "maximum": 1000000 }
                },
                "required": ["run_id"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "run": { "type": "object" },
                    "stdout": { "type": "string" },
                    "stderr": { "type": "string" },
                    "output_ref": { "type": ["string", "null"] }
                }
            }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_agent_read_output,
    );

    registry.register(
        ToolSpec {
            id: "agent.stop".to_string(),
            name: "停止后台 Agent".to_string(),
            description: "停止 agent_runs 中指定后台 Agent；有 pid 时会尝试终止对应进程树"
                .to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" }
                },
                "required": ["run_id"]
            }),
            output_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "status": { "type": "string" },
                    "pid": { "type": ["integer", "null"] },
                    "error": { "type": ["string", "null"] }
                }
            }),
            risk_level: RiskLevel::ExternalSideEffect,
            permissions: vec![ToolPermission::SystemControl],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_agent_stop,
    );

    registry.register(
        ToolSpec {
            id: "agent.team.create".to_string(),
            name: "创建 Agent Team".to_string(),
            description: "创建一个可追踪的后台 Agent Team 容器".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "workspace_id": { "type": "string" },
                    "metadata": { "type": "object" }
                },
                "required": ["name"]
            }),
            output_schema: serde_json::json!({ "type": "object" }),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_agent_team_create,
    );

    registry.register(
        ToolSpec {
            id: "agent.team.list".to_string(),
            name: "列出 Agent Team".to_string(),
            description: "列出当前持久化的 Agent Team".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "workspace_id": { "type": "string" },
                    "include_archived": { "type": "boolean" }
                }
            }),
            output_schema: serde_json::json!({ "type": "object" }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_agent_team_list,
    );

    registry.register(
        ToolSpec {
            id: "agent.team.add_member".to_string(),
            name: "添加 Agent Team 成员".to_string(),
            description: "把一个后台 agent/run 加入指定 Agent Team".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "team_id": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "role": { "type": "string" },
                    "run_id": { "type": "string" },
                    "cwd": { "type": "string" },
                    "subscriptions": { "type": "array", "items": { "type": "string" } },
                    "metadata": { "type": "object" }
                },
                "required": ["team_id", "agent_id"]
            }),
            output_schema: serde_json::json!({ "type": "object" }),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_agent_team_add_member,
    );

    registry.register(
        ToolSpec {
            id: "agent.team.snapshot".to_string(),
            name: "读取 Agent Team 快照".to_string(),
            description: "读取 team、成员和近期 mailbox 消息".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "team_id": { "type": "string" },
                    "message_limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                },
                "required": ["team_id"]
            }),
            output_schema: serde_json::json!({ "type": "object" }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_agent_team_snapshot,
    );

    registry.register(
        ToolSpec {
            id: "agent.team.plan_verdict".to_string(),
            name: "提交 Agent Team 计划裁决".to_string(),
            description: "对 awaiting_plan_approval 的 Agent Team 提交 approved/rejected 裁决"
                .to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "team_id": { "type": "string" },
                    "verdict": {
                        "type": "string",
                        "enum": ["approved", "rejected"]
                    }
                },
                "required": ["team_id", "verdict"]
            }),
            output_schema: serde_json::json!({ "type": "object" }),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_agent_team_plan_verdict,
    );

    registry.register(
        ToolSpec {
            id: "agent.team.review_verdict".to_string(),
            name: "提交 Agent Team Review 裁决".to_string(),
            description: "对 awaiting_review 的 Agent Team 提交 accepted/failed 裁决".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "team_id": { "type": "string" },
                    "verdict": {
                        "type": "string",
                        "enum": ["accepted", "failed"]
                    }
                },
                "required": ["team_id", "verdict"]
            }),
            output_schema: serde_json::json!({ "type": "object" }),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_agent_team_review_verdict,
    );

    registry.register(
        ToolSpec {
            id: "agent.mailbox.send".to_string(),
            name: "发送 Agent Mailbox 消息".to_string(),
            description: "向指定 team member 或广播收件箱发送消息".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "team_id": { "type": "string" },
                    "sender_agent_id": { "type": "string" },
                    "recipient_agent_id": { "type": "string" },
                    "broadcast": { "type": "boolean" },
                    "kind": {
                        "type": "string",
                        "enum": [
                            "message",
                            "broadcast",
                            "shutdown_request",
                            "shutdown_response",
                            "plan_approval_request",
                            "plan_approval_response",
                            "review_verdict_request"
                        ]
                    },
                    "content": { "type": "string" },
                    "metadata": { "type": "object" }
                },
                "required": ["team_id", "content"]
            }),
            output_schema: serde_json::json!({ "type": "object" }),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_agent_mailbox_send,
    );

    registry.register(
        ToolSpec {
            id: "agent.mailbox.list".to_string(),
            name: "读取 Agent Mailbox".to_string(),
            description: "读取指定 Agent Team 的 mailbox 消息".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "team_id": { "type": "string" },
                    "recipient_agent_id": { "type": "string" },
                    "include_read": { "type": "boolean" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "required": ["team_id"]
            }),
            output_schema: serde_json::json!({ "type": "object" }),
            risk_level: RiskLevel::ReadOnly,
            permissions: vec![ToolPermission::ReadWorkspace],
            supports_dry_run: true,
            workspace_required: false,
        },
        execute_agent_mailbox_list,
    );

    registry.register(
        ToolSpec {
            id: "agent.mailbox.mark_read".to_string(),
            name: "标记 Agent Mailbox 已读".to_string(),
            description: "将指定 mailbox 消息标记为已读".to_string(),
            provider: ToolProviderKind::Internal,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message_id": { "type": "string" }
                },
                "required": ["message_id"]
            }),
            output_schema: serde_json::json!({ "type": "object" }),
            risk_level: RiskLevel::WorkspaceWrite,
            permissions: vec![ToolPermission::WriteWorkspace],
            supports_dry_run: false,
            workspace_required: false,
        },
        execute_agent_mailbox_mark_read,
    );
}
