use conductor_core::{
    affection, agent_runs, agent_teams, avatar,
    chat::{self, ChatCapability, ChatMessage, ChatReply, ChatTaskMode},
    command_runs,
    config::{self, CoreConfig},
    connectors, events, expression, goal_tasks, goals, heartbeat, initiative, memory, music,
    persona, projection,
    proposals::{self, Proposal, ProposalStatus},
    scene, skills, tasklist,
    tasks::{self, Task, TaskStatus},
    tool_calls,
    workspaces::{self, WorkspaceKind},
};
use serde::{Deserialize, Serialize};
use tauri::{command, AppHandle, Emitter};

use crate::error::AppError;

#[derive(Serialize)]
pub struct TaskWithSummary {
    task: Task,
    summary: Option<String>,
}

async fn advance_goal_after_plan_approval(goal_id: &str) -> Result<goals::GoalRun, AppError> {
    let goal = goals::get_goal(goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found: {goal_id}")))?;
    let orchestrator = conductor_core::goal_orchestrator::GoalOrchestrator::new(
        conductor_core::goal_orchestrator::OrchestratorConfig {
            workspace_id: goal.workspace_id.clone(),
            ..Default::default()
        },
    );
    conductor_core::goal_orchestrator::approve_goal_plan(goal_id).await?;
    orchestrator.tick_goal(goal_id).await?;
    goals::get_goal(goal_id).await?.ok_or_else(|| {
        AppError::NotFound(format!(
            "goal not found after plan approval advancement: {goal_id}"
        ))
    })
}

fn goal_status_projection_text(previous_status: &str, goal: &goals::GoalRun) -> Option<String> {
    if previous_status == goal.status {
        return None;
    }

    let detail = match goal.status.as_str() {
        "planning" => "正在整理上下文并拆出下一步。",
        "running" => "当前 Goal 正在执行，最新进展会继续写回这条会话。",
        "awaiting_review" => "这一轮已产出结果。",
        "accepted" => "当前 Goal 已完成。",
        "rework_required" => "当前 Goal 还需要继续推进。",
        "blocked" => "当前 Goal 暂时阻塞。",
        "failed" => "当前 Goal 执行失败。",
        "cancelled" => "当前 Goal 已取消。",
        _ => return None,
    };

    Some(format!("**Goal 进展 | {}**\n\n{}", goal.title, detail))
}

async fn project_goal_status_change_to_session(
    app: &AppHandle,
    previous_status: &str,
    goal: &goals::GoalRun,
) -> Result<(), AppError> {
    let Some(content) = goal_status_projection_text(previous_status, goal) else {
        return Ok(());
    };
    let Some(session_id) = chat::find_session_for_goal(&goal.id).await else {
        return Ok(());
    };
    let message = chat::append_assistant_message_to_session(&session_id, &content).await?;
    let _ = app.emit(
        "reply_stored",
        serde_json::json!({
            "message_id": message.id,
            "session_id": session_id,
        }),
    );
    Ok(())
}

#[tauri::command]
pub async fn list_tasks(only_pending: bool) -> Result<Vec<Task>, AppError> {
    let mut file = tasks::load().await?;
    file.tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(file
        .tasks
        .into_iter()
        .filter(|task| !only_pending || task.status == TaskStatus::Pending)
        .take(20)
        .collect())
}

#[tauri::command]
pub async fn get_task_activity_stats() -> Result<tasks::TaskActivityStats, AppError> {
    let file = tasks::load().await?;
    Ok(tasks::activity_stats(&file.tasks))
}

#[tauri::command]
pub async fn show_task(id: String) -> Result<TaskWithSummary, AppError> {
    let file = tasks::load().await?;
    let task = file
        .tasks
        .into_iter()
        .find(|task| task.id == id)
        .ok_or_else(|| AppError::NotFound(format!("task not found: {id}")))?;
    let summary = match &task.summary_ref {
        Some(path) => tokio::fs::read_to_string(conductor_core::paths::state().join(path))
            .await
            .ok(),
        None => None,
    };
    Ok(TaskWithSummary { task, summary })
}

#[tauri::command]
pub async fn list_agent_tasks(
    include_completed: bool,
) -> Result<Vec<tasklist::AgentTask>, AppError> {
    Ok(tasklist::list_tasks(tasklist::TaskListFilter {
        include_completed,
        ..Default::default()
    })
    .await?)
}

#[tauri::command]
pub async fn list_tasks_by_budget(
    budget_minutes: u32,
) -> Result<Vec<tasklist::AgentTask>, AppError> {
    Ok(tasklist::list_tasks_by_budget(budget_minutes, tasklist::TaskListFilter::default()).await?)
}

#[tauri::command]
pub async fn create_chat_session(
    title: Option<String>,
    workspace_id: Option<String>,
) -> Result<chat::ChatSessionSummary, AppError> {
    Ok(chat::create_chat_session(title, workspace_id).await?)
}

#[tauri::command]
pub async fn ensure_chat_session(
    title: String,
    workspace_id: Option<String>,
) -> Result<chat::ChatSessionSummary, AppError> {
    Ok(chat::ensure_chat_session(&title, workspace_id).await?)
}

#[tauri::command]
pub async fn list_chat_sessions(
    limit: Option<u32>,
) -> Result<Vec<chat::ChatSessionSummary>, AppError> {
    Ok(chat::list_chat_sessions(limit).await?)
}

#[tauri::command]
pub async fn get_chat_session_messages(
    session_id: String,
    limit: Option<u32>,
) -> Result<Vec<ChatMessage>, AppError> {
    Ok(chat::get_chat_session_messages(&session_id, limit).await?)
}

#[tauri::command]
pub async fn rename_chat_session(session_id: String, title: String) -> Result<(), AppError> {
    Ok(chat::rename_chat_session(&session_id, &title).await?)
}

#[tauri::command]
pub async fn archive_chat_session(session_id: String) -> Result<(), AppError> {
    Ok(chat::archive_chat_session(&session_id).await?)
}

#[tauri::command]
pub async fn update_chat_session_workspace(
    session_id: String,
    workspace_id: Option<String>,
) -> Result<(), AppError> {
    Ok(chat::update_chat_session_workspace(&session_id, workspace_id.as_deref()).await?)
}

#[tauri::command]
pub async fn set_chat_session_kind(
    session_id: String,
    kind: String,
    goal_id: Option<String>,
) -> Result<(), AppError> {
    Ok(chat::set_chat_session_kind(&session_id, &kind, goal_id.as_deref()).await?)
}

#[tauri::command]
pub async fn migrate_legacy_tasks_to_tasklist(app: AppHandle) -> Result<usize, AppError> {
    let count = tasklist::migrate_legacy_tasks(None).await?;
    let _ = app.emit("tasks_changed", ());
    Ok(count)
}

#[tauri::command]
pub async fn list_agent_runs(
    workspace_id: Option<String>,
    include_finished: bool,
) -> Result<Vec<agent_runs::AgentRun>, AppError> {
    let mut runs = agent_runs::list(agent_runs::AgentRunFilter {
        workspace_id,
        ..Default::default()
    })
    .await?;
    if !include_finished {
        runs.retain(|run| {
            matches!(
                run.status,
                agent_runs::AgentRunStatus::Queued | agent_runs::AgentRunStatus::Running
            )
        });
    }
    Ok(runs)
}

#[tauri::command]
pub async fn read_agent_run_output(
    run_id: String,
    max_bytes: Option<usize>,
) -> Result<agent_runs::AgentRunOutput, AppError> {
    Ok(agent_runs::read_output(&run_id, max_bytes.unwrap_or(16_384)).await?)
}

#[tauri::command]
pub async fn stop_agent_run(
    app: AppHandle,
    run_id: String,
) -> Result<agent_runs::AgentRun, AppError> {
    let run = agent_runs::stop(&run_id).await?;
    let _ = app.emit("agent_runs_changed", ());
    Ok(run)
}

#[tauri::command]
pub async fn get_tool_call(id: String) -> Result<tool_calls::ToolCall, AppError> {
    Ok(tool_calls::get(&id).await?)
}

#[tauri::command]
pub async fn list_tool_calls(
    session_id: Option<String>,
    workspace_id: Option<String>,
    llm_tool_call_id: Option<String>,
    tool_id: Option<String>,
    status: Option<String>,
    proposal_id: Option<String>,
    command_run_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<tool_calls::ToolCall>, AppError> {
    Ok(tool_calls::list(tool_calls::ToolCallFilter {
        session_id,
        workspace_id,
        llm_tool_call_id,
        tool_id,
        status,
        proposal_id,
        command_run_id,
        limit,
    })
    .await?)
}

#[tauri::command]
pub async fn get_command_run(id: String) -> Result<command_runs::CommandRun, AppError> {
    Ok(command_runs::get(&id).await?)
}

#[tauri::command]
pub async fn list_command_runs(
    session_id: Option<String>,
    tool_call_id: Option<String>,
    agent_run_id: Option<String>,
    status: Option<String>,
    active_only: bool,
    limit: Option<u32>,
) -> Result<Vec<command_runs::CommandRun>, AppError> {
    Ok(command_runs::list_filtered(command_runs::CommandRunFilter {
        session_id,
        tool_call_id,
        agent_run_id,
        status,
        active_only,
        limit,
    })
    .await?)
}

#[tauri::command]
pub async fn list_agent_teams(
    workspace_id: Option<String>,
    include_archived: bool,
) -> Result<Vec<agent_teams::AgentTeam>, AppError> {
    Ok(agent_teams::list_teams(workspace_id.as_deref(), include_archived).await?)
}

#[tauri::command]
pub async fn create_agent_team(
    app: AppHandle,
    name: String,
    workspace_id: Option<String>,
) -> Result<agent_teams::AgentTeam, AppError> {
    let team = agent_teams::create_team(agent_teams::CreateAgentTeamInput {
        id: None,
        name,
        workspace_id,
        write_scope: vec![],
        metadata: None,
    })
    .await?;
    let _ = app.emit("agent_teams_changed", ());
    Ok(team)
}

#[tauri::command]
pub async fn add_agent_team_member(
    app: AppHandle,
    team_id: String,
    agent_id: String,
    role: String,
    run_id: Option<String>,
) -> Result<agent_teams::AgentTeamMember, AppError> {
    let member = agent_teams::add_member(agent_teams::AddAgentTeamMemberInput {
        team_id,
        agent_id,
        role,
        run_id,
        cwd: None,
        subscriptions: Vec::new(),
        metadata: None,
    })
    .await?;
    let _ = app.emit("agent_teams_changed", ());
    Ok(member)
}

#[tauri::command]
pub async fn get_agent_team_snapshot(
    team_id: String,
    message_limit: Option<u32>,
) -> Result<agent_teams::AgentTeamSnapshot, AppError> {
    Ok(agent_teams::snapshot(&team_id, message_limit.unwrap_or(20)).await?)
}

#[tauri::command]
pub async fn submit_agent_team_plan_verdict(
    app: AppHandle,
    team_id: String,
    verdict: String,
) -> Result<agent_teams::AgentTeam, AppError> {
    let verdict = agent_teams::PlanApprovalVerdict::from_str(&verdict)?;
    let team = agent_teams::handle_plan_approval_response(&team_id, verdict).await?;
    let _ = app.emit("agent_teams_changed", ());
    let _ = app.emit("goals_changed", ());
    Ok(team)
}

#[tauri::command]
pub async fn submit_agent_team_review_verdict(
    app: AppHandle,
    team_id: String,
    verdict: String,
) -> Result<agent_teams::AgentTeam, AppError> {
    let verdict = match verdict.trim().to_ascii_lowercase().as_str() {
        "accepted" | "accept" | "approved" => agent_teams::ReviewVerdict::Accepted,
        "failed" | "fail" | "rejected" | "reject" | "rework_required" => {
            agent_teams::ReviewVerdict::Failed
        }
        other => {
            return Err(AppError::Validation(format!(
                "unknown agent team review verdict: {other}"
            )))
        }
    };
    let team = agent_teams::handle_review_verdict(&team_id, verdict).await?;
    let _ = app.emit("agent_teams_changed", ());
    let _ = app.emit("goals_changed", ());
    Ok(team)
}

#[tauri::command]
pub async fn send_agent_mailbox_message(
    app: AppHandle,
    team_id: String,
    sender_agent_id: String,
    recipient_agent_id: Option<String>,
    content: String,
) -> Result<Vec<agent_teams::AgentMailboxMessage>, AppError> {
    let messages = agent_teams::send_message(agent_teams::SendAgentMessageInput {
        team_id,
        sender_agent_id,
        recipient_agent_id,
        broadcast: false,
        kind: None,
        content,
        metadata: None,
    })
    .await?;
    let _ = app.emit("agent_mailbox_changed", ());
    Ok(messages)
}

#[tauri::command]
pub async fn list_agent_mailbox(
    team_id: String,
    recipient_agent_id: Option<String>,
    include_read: bool,
) -> Result<Vec<agent_teams::AgentMailboxMessage>, AppError> {
    Ok(agent_teams::list_mailbox(agent_teams::AgentMailboxFilter {
        team_id,
        recipient_agent_id,
        include_read,
        limit: Some(100),
    })
    .await?)
}

#[tauri::command]
pub async fn mark_agent_mailbox_read(
    app: AppHandle,
    message_id: String,
) -> Result<agent_teams::AgentMailboxMessage, AppError> {
    let message = agent_teams::mark_message_read(&message_id).await?;
    let _ = app.emit("agent_mailbox_changed", ());
    Ok(message)
}

#[tauri::command]
pub async fn pass_task(app: AppHandle, id: String) -> Result<(), AppError> {
    set_status(app, id, TaskStatus::Passed).await
}

#[tauri::command]
pub async fn skip_task(app: AppHandle, id: String) -> Result<(), AppError> {
    set_status(app, id, TaskStatus::Skipped).await
}

#[tauri::command]
pub async fn reject_task(app: AppHandle, id: String) -> Result<(), AppError> {
    set_status(app, id, TaskStatus::Rejected).await
}

#[tauri::command]
pub async fn get_settings() -> Result<CoreConfig, AppError> {
    Ok(config::load().await?)
}

#[derive(serde::Serialize)]
pub struct WorkspaceStatus {
    pub root: String,
    pub exists: bool,
    pub writable: bool,
}

#[tauri::command]
pub async fn get_workspace_status(
    workspace_id: Option<String>,
) -> Result<WorkspaceStatus, AppError> {
    let root = if let Some(workspace_id) = workspace_id {
        workspaces::get(&workspace_id).await?.root
    } else {
        conductor_core::paths::root()
    };
    let exists = root.exists();
    let writable = if exists {
        std::fs::metadata(&root)
            .map(|m| !m.permissions().readonly())
            .unwrap_or(false)
    } else {
        false
    };
    Ok(WorkspaceStatus {
        root: root.display().to_string(),
        exists,
        writable,
    })
}

#[tauri::command]
pub async fn list_workspaces() -> Result<Vec<workspaces::Workspace>, AppError> {
    Ok(workspaces::list_all().await?)
}

#[tauri::command]
pub async fn attach_workspace(
    root: String,
    name: Option<String>,
    kind: Option<String>,
) -> Result<workspaces::Workspace, AppError> {
    let kind = match kind.as_deref() {
        Some(raw) => Some(WorkspaceKind::from_str(raw)?),
        None => None,
    };
    Ok(workspaces::create_or_attach(std::path::Path::new(&root), name, kind).await?)
}

#[tauri::command]
pub async fn update_settings(config: CoreConfig) -> Result<CoreConfig, AppError> {
    Ok(config::save(&merge_existing_secret(config).await).await?)
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: CoreConfig) -> Result<CoreConfig, AppError> {
    let saved = config::save(&merge_existing_secret(settings).await).await?;
    let _ = app.emit("settings_changed", &saved);
    Ok(saved)
}

async fn merge_existing_secret(mut settings: CoreConfig) -> CoreConfig {
    let incoming_key = settings
        .llm
        .api_key
        .as_ref()
        .is_some_and(|key| !key.trim().is_empty());
    if incoming_key {
        return settings;
    }

    if settings.llm.api_key_set {
        if let Ok(existing) = config::load().await {
            if existing
                .llm
                .api_key
                .as_ref()
                .is_some_and(|key| !key.trim().is_empty())
            {
                settings.llm.api_key = existing.llm.api_key;
            }
        }
    }

    settings
}

#[tauri::command]
pub async fn test_llm_connection(settings: serde_json::Value) -> Result<String, AppError> {
    use conductor_core::llm::{call, LlmRequestConfig};

    let mut llm_config: conductor_core::config::LlmConfig = serde_json::from_value(settings)
        .map_err(|err| AppError::Validation(format!("Invalid settings: {}", err)))?;
    if llm_config
        .api_key
        .as_ref()
        .is_none_or(|key| key.trim().is_empty())
        && llm_config.api_key_set
    {
        if let Ok(existing) = config::load().await {
            llm_config.api_key = existing.llm.api_key;
        }
    }

    let provider = llm_config.provider.trim().to_ascii_lowercase();
    let base_url = llm_config.base_url.trim().to_ascii_lowercase();
    let api_key = llm_config
        .api_key
        .clone()
        .filter(|k| !k.trim().is_empty())
        .or_else(|| {
            if provider == "anthropic_compatible" || provider == "anthropic" || provider == "claude"
            {
                std::env::var("ANTHROPIC_API_KEY")
                    .ok()
                    .filter(|k| !k.is_empty())
            } else if base_url.contains("openai.com") {
                std::env::var("OPENAI_API_KEY")
                    .ok()
                    .filter(|k| !k.is_empty())
            } else {
                None
            }
        })
        .or_else(|| std::env::var("LLM_API_KEY").ok().filter(|k| !k.is_empty()));

    let request_config = LlmRequestConfig {
        provider: &llm_config.provider,
        model: &llm_config.model,
        base_url: &llm_config.base_url,
        api_key: api_key.as_deref(),
        temperature: 0.0,
        tool_choice: None,
    };

    if api_key.is_none() && (base_url.contains("openai.com") || base_url.contains("anthropic.com"))
    {
        return Err(AppError::Validation(
            "No API key configured. Please set your API key first.".to_string(),
        ));
    }

    let result = call(
        &llm_config.model,
        "You are a connection test bot. Respond with exactly: OK",
        "Test connection",
        &request_config,
    )
    .await;

    match result {
        Ok(response) if response.trim() == "OK" => Ok(format!(
            "Successfully connected to {} using model {}",
            llm_config.provider, llm_config.model
        )),
        Ok(response) => Ok(format!(
            "Connected successfully (response: {})",
            response.trim()
        )),
        Err(err) => Err(AppError::Internal(format!("Connection failed: {}", err))),
    }
}

#[tauri::command]
pub async fn send_chat_message_v2(
    app_handle: AppHandle,
    message: String,
    session_id: Option<String>,
    task_mode: Option<ChatTaskMode>,
    capability: Option<ChatCapability>,
    plan_only: Option<bool>,
    approved_write_scope: Option<Vec<String>>,
    request_id: Option<String>,
) -> Result<ChatReply, AppError> {
    chat::send_message_v2_with_session(
        message,
        &app_handle,
        session_id.clone(),
        task_mode.unwrap_or_default(),
        capability.unwrap_or_default(),
        plan_only.unwrap_or(false),
        approved_write_scope,
        None,
        request_id.clone(),
    )
    .await
    .map_err(Into::into)
}

#[tauri::command]
pub async fn chat_history(limit: Option<u32>) -> Result<Vec<ChatMessage>, AppError> {
    let config = config::load().await?;
    Ok(chat::history(limit.unwrap_or(config.chat_history_limit)).await?)
}

#[tauri::command]
pub async fn list_chat_messages() -> Result<Vec<ChatMessage>, AppError> {
    chat_history(None).await
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForegroundAppDto {
    pub title: String,
    pub process_name: String,
    pub process_path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarStateDto {
    pub id: String,
    pub avatar_id: String,
    pub activity_variant: String,
    pub updated_at: String,
    pub locked_main_avatar: bool,
    pub locked_activity_variant: bool,
}

impl From<avatar::AvatarState> for AvatarStateDto {
    fn from(state: avatar::AvatarState) -> Self {
        Self {
            id: state.id,
            avatar_id: state.avatar_id.as_str().to_string(),
            activity_variant: state.activity_variant.as_str().to_string(),
            updated_at: state.updated_at.to_rfc3339(),
            locked_main_avatar: state.locked_main_avatar,
            locked_activity_variant: state.locked_activity_variant,
        }
    }
}

#[tauri::command]
pub async fn get_current_avatar() -> Result<AvatarStateDto, AppError> {
    Ok(avatar::get_current_avatar()
        .await
        .map(AvatarStateDto::from)?)
}

#[tauri::command]
pub async fn set_pet_avatar(app: AppHandle, avatar_id: String) -> Result<AvatarStateDto, AppError> {
    let avatar_id = avatar::AvatarId::from_str(&avatar_id)?;
    let state = avatar::set_avatar(avatar_id)
        .await
        .map(AvatarStateDto::from)?;
    let _ = app.emit("pet_avatar_changed", &state);
    Ok(state)
}

#[tauri::command]
pub async fn set_activity_variant(
    app: AppHandle,
    variant: String,
) -> Result<AvatarStateDto, AppError> {
    let variant = avatar::ActivityVariant::from_str(&variant)?;
    let state = avatar::set_activity_variant(variant)
        .await
        .map(AvatarStateDto::from)?;
    let _ = app.emit("pet_avatar_changed", &state);
    Ok(state)
}

#[tauri::command]
pub async fn set_main_avatar_manual(
    app: AppHandle,
    avatar_id: String,
) -> Result<AvatarStateDto, AppError> {
    let avatar_id = avatar::AvatarId::from_str(&avatar_id)?;
    let state = avatar::set_main_avatar_manual(avatar_id)
        .await
        .map(AvatarStateDto::from)?;
    let _ = app.emit("pet_avatar_changed", &state);
    Ok(state)
}

#[tauri::command]
pub async fn set_sub_avatar_manual(
    app: AppHandle,
    variant: String,
) -> Result<AvatarStateDto, AppError> {
    let variant = avatar::ActivityVariant::from_str(&variant)?;
    let state = avatar::set_sub_avatar_manual(variant)
        .await
        .map(AvatarStateDto::from)?;
    let _ = app.emit("pet_avatar_changed", &state);
    Ok(state)
}

#[tauri::command]
pub async fn toggle_avatar_lock(
    app: AppHandle,
    lock_type: String,
    locked: bool,
) -> Result<AvatarStateDto, AppError> {
    let state = match lock_type.as_str() {
        "main" => avatar::toggle_lock_main_avatar(locked).await,
        "sub" => avatar::toggle_lock_activity_variant(locked).await,
        other => {
            return Err(AppError::Validation(format!(
                "unknown lock_type: {}",
                other
            )))
        }
    }
    .map(AvatarStateDto::from)?;
    let _ = app.emit("pet_avatar_changed", &state);
    Ok(state)
}

#[tauri::command]
pub async fn get_foreground_app() -> Result<ForegroundAppDto, AppError> {
    Ok(
        conductor_sense::window_title::foreground_window_info().map(|info| ForegroundAppDto {
            title: info.title,
            process_name: info.process_name,
            process_path: info.process_path.map(|path| path.display().to_string()),
        })?,
    )
}

#[tauri::command]
pub async fn show_pet_message(app: AppHandle, content: String) -> Result<(), AppError> {
    let content = content.trim();
    if content.is_empty() {
        return Ok(());
    }
    if let Ok(session) = chat::ensure_chat_session("\u{95f2}\u{804a}", None).await {
        if let Ok(message) = chat::append_assistant_message_to_session(&session.id, content).await {
            let _ = app.emit(
                "reply_stored",
                serde_json::json!({
                    "message_id": message.id,
                    "session_id": session.id,
                }),
            );
        }
    }
    app.emit(
        "pet_message",
        serde_json::json!({
            "id": format!("pet-msg-{}", chrono::Utc::now().timestamp_millis()),
            "content": content,
            "kind": "assistant"
        }),
    )?;
    Ok(())
}

async fn set_status(app: AppHandle, id: String, status: TaskStatus) -> Result<(), AppError> {
    tasks::update(&id, |task| task.status = status).await?;

    let _ = app.emit("tasks_changed", ());

    let active_count = tasks::load()
        .await
        .map(|file| {
            file.tasks
                .iter()
                .filter(|task| {
                    task.status == TaskStatus::Pending || task.status == TaskStatus::InProgress
                })
                .count()
        })
        .unwrap_or(0);

    let state = if active_count > 0 { "working" } else { "idle" };

    let _ = app.emit("pet_state", state);
    Ok(())
}

#[command]
pub async fn list_proposals(status: Option<String>) -> Result<Vec<ProposalDto>, AppError> {
    let status = status.and_then(|s| match s.as_str() {
        "pending" => Some(ProposalStatus::Pending),
        "approved" => Some(ProposalStatus::Approved),
        "running" => Some(ProposalStatus::Running),
        "succeeded" => Some(ProposalStatus::Succeeded),
        "failed" => Some(ProposalStatus::Failed),
        "rejected" => Some(ProposalStatus::Rejected),
        "expired" => Some(ProposalStatus::Expired),
        "used" => Some(ProposalStatus::Used),
        _ => None,
    });

    let proposals = match status {
        Some(s) => proposals::list_by_status(s).await?,
        None => proposals::list_pending().await?,
    };

    Ok(proposals.into_iter().map(ProposalDto::from).collect())
}

#[command]
pub async fn approve_proposal(app: AppHandle, id: String) -> Result<(), AppError> {
    proposals::approve(&id).await?;
    let _ = app.emit(
        "proposal-changed",
        serde_json::json!({ "id": id, "status": "approved" }),
    );
    refresh_pet_state(&app).await;
    Ok(())
}

#[command]
pub async fn execute_proposal(app: AppHandle, id: String) -> Result<serde_json::Value, AppError> {
    conductor_core::tools::register_builtin_tools();
    let proposal = proposals::get(&id).await?;
    let result = proposals::execute_proposal(&id).await?;
    if matches!(
        proposal.tool_id.as_deref(),
        Some("pet.set_avatar" | "conductor.pet.set_avatar")
    ) {
        if let Ok(state) = avatar::get_current_avatar().await.map(AvatarStateDto::from) {
            let _ = app.emit("pet_avatar_changed", &state);
        }
    }
    let _ = app.emit(
        "proposal-changed",
        serde_json::json!({
            "id": id,
            "status": if result.success { "succeeded" } else { "failed" }
        }),
    );
    refresh_pet_state(&app).await;
    Ok(serde_json::json!({
        "success": result.success,
        "output": result.output,
        "error": result.error,
        "duration_ms": result.duration_ms
    }))
}

#[command]
pub async fn reject_proposal(app: AppHandle, id: String) -> Result<(), AppError> {
    proposals::reject(&id).await?;
    let _ = app.emit(
        "proposal-changed",
        serde_json::json!({ "id": id, "status": "rejected" }),
    );
    refresh_pet_state(&app).await;
    Ok(())
}

async fn refresh_pet_state(app: &AppHandle) {
    if let Ok(file) = tasks::load().await {
        let has_active = file.tasks.iter().any(|task| {
            task.status == TaskStatus::Pending || task.status == TaskStatus::InProgress
        });
        let state = if has_active { "working" } else { "idle" };
        let _ = app.emit("pet_state", state);
    }
}

#[derive(Serialize)]
pub struct ProposalDto {
    pub id: String,
    pub workspace_id: Option<String>,
    pub for_cwd: String,
    pub source: String,
    pub title: String,
    pub content: String,
    pub reason: String,
    pub tool_id: Option<String>,
    pub tool_input_json: Option<String>,
    pub risk_level: String,
    pub dry_run: bool,
    pub status: String,
    pub result_ref: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Proposal> for ProposalDto {
    fn from(p: Proposal) -> Self {
        Self {
            id: p.id,
            workspace_id: p.workspace_id,
            for_cwd: p.for_cwd.to_string_lossy().to_string(),
            source: p.source.as_str().to_string(),
            title: p.title,
            content: p.content,
            reason: p.reason,
            tool_id: p.tool_id,
            tool_input_json: p.tool_input_json,
            risk_level: p.risk_level.as_str().to_string(),
            dry_run: p.dry_run,
            status: match p.status {
                ProposalStatus::Pending => "pending",
                ProposalStatus::Approved => "approved",
                ProposalStatus::Running => "running",
                ProposalStatus::Succeeded => "succeeded",
                ProposalStatus::Failed => "failed",
                ProposalStatus::Rejected => "rejected",
                ProposalStatus::Expired => "expired",
                ProposalStatus::Used => "used",
            }
            .to_string(),
            result_ref: p.result_ref,
            created_at: p.created_at.to_rfc3339(),
            updated_at: p.updated_at.to_rfc3339(),
        }
    }
}

#[tauri::command]
pub async fn get_affection() -> Result<u32, AppError> {
    Ok(affection::load().await.map(|state| state.value)?)
}

#[tauri::command]
pub async fn add_affection(value: i32) -> Result<u32, AppError> {
    Ok(affection::add(value).await.map(|state| state.value)?)
}

#[tauri::command]
pub async fn interact_affection() -> Result<u32, AppError> {
    Ok(affection::interact().await.map(|state| state.value)?)
}

#[tauri::command]
pub async fn decrease_affection_over_time() -> Result<u32, AppError> {
    Ok(affection::decrease_over_time()
        .await
        .map(|state| state.value)?)
}

#[tauri::command]
pub async fn get_expression_state() -> Result<serde_json::Value, AppError> {
    let mood = expression::load_mood().await?;
    let affection_state = affection::load().await?;
    let zone = mood.zone();

    Ok(serde_json::json!({
        "mood_zone": zone.as_str(),
        "mood_label": zone.label_zh(),
        "valence": mood.valence,
        "arousal": mood.arousal,
        "relationship_stage": affection_state.stage.as_str(),
        "relationship_label": affection_state.stage.label_zh(),
        "affection_value": affection_state.value,
    }))
}

#[tauri::command]
pub async fn get_mood_state() -> Result<serde_json::Value, AppError> {
    let mood = expression::load_mood().await?;
    let zone = mood.zone();
    Ok(serde_json::json!({
        "zone": zone.as_str(),
        "label": zone.label_zh(),
        "valence": mood.valence,
        "arousal": mood.arousal,
    }))
}

// ── Emotion/Affection visualization DTOs ────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmotionHistoryPoint {
    pub valence: f32,
    pub arousal: f32,
    pub zone: String,
    pub zone_label: String,
    pub updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffectionHistoryPoint {
    pub value: u32,
    pub stage: String,
    pub stage_label: String,
    pub recorded_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmotionSummary {
    pub mood_zone: String,
    pub mood_label: String,
    pub valence: f32,
    pub arousal: f32,
    pub relationship_stage: String,
    pub relationship_label: String,
    pub affection_value: u32,
    pub consecutive_days: u32,
    pub interaction_count: u32,
}

#[tauri::command]
pub async fn get_emotion_history() -> Result<Vec<EmotionHistoryPoint>, AppError> {
    let history = expression::load_mood_history().await?;
    if history.is_empty() {
        // Return current mood as a single-point array
        let mood = expression::load_mood().await?;
        let zone = mood.zone();
        return Ok(vec![EmotionHistoryPoint {
            valence: mood.valence,
            arousal: mood.arousal,
            zone: zone.as_str().to_string(),
            zone_label: zone.label_zh().to_string(),
            updated_at: mood.updated_at.to_rfc3339(),
        }]);
    }
    Ok(history
        .into_iter()
        .map(|mood| {
            let zone = mood.zone();
            EmotionHistoryPoint {
                valence: mood.valence,
                arousal: mood.arousal,
                zone: zone.as_str().to_string(),
                zone_label: zone.label_zh().to_string(),
                updated_at: mood.updated_at.to_rfc3339(),
            }
        })
        .collect())
}

#[tauri::command]
pub async fn get_affection_history() -> Result<Vec<AffectionHistoryPoint>, AppError> {
    // No history tracking for affection yet — return current value as single-point array
    let state = affection::load().await?;
    let now = chrono::Utc::now().to_rfc3339();
    Ok(vec![AffectionHistoryPoint {
        value: state.value,
        stage: state.stage.as_str().to_string(),
        stage_label: state.stage.label_zh().to_string(),
        recorded_at: now,
    }])
}

#[tauri::command]
pub async fn get_emotion_summary() -> Result<EmotionSummary, AppError> {
    let mood = expression::load_mood().await?;
    let affection_state = affection::load().await?;
    let zone = mood.zone();

    Ok(EmotionSummary {
        mood_zone: zone.as_str().to_string(),
        mood_label: zone.label_zh().to_string(),
        valence: mood.valence,
        arousal: mood.arousal,
        relationship_stage: affection_state.stage.as_str().to_string(),
        relationship_label: affection_state.stage.label_zh().to_string(),
        affection_value: affection_state.value,
        consecutive_days: affection_state.consecutive_days,
        interaction_count: affection_state.interaction_count,
    })
}

#[tauri::command]
pub async fn memory_set(key: String, value: String, category: String) -> Result<(), AppError> {
    Ok(memory::set(&key, &value, &category).await.map(|_| ())?)
}

#[tauri::command]
pub async fn memory_get(key: String) -> Result<Option<String>, AppError> {
    Ok(memory::get(&key).await?)
}

#[tauri::command]
pub async fn memory_get_by_category(
    category: String,
) -> Result<Vec<memory::MemoryEntry>, AppError> {
    Ok(memory::get_by_category(&category).await?)
}

#[tauri::command]
pub async fn memory_save_preferences(prefs: serde_json::Value) -> Result<(), AppError> {
    let prefs: memory::UserPreferences = serde_json::from_value(prefs)
        .map_err(|err| AppError::Validation(format!("Invalid preferences: {}", err)))?;
    Ok(memory::save_preferences(&prefs).await?)
}

#[tauri::command]
pub async fn memory_load_preferences() -> Result<memory::UserPreferences, AppError> {
    Ok(memory::load_preferences().await?)
}

#[tauri::command]
pub async fn memory_add_conversation(
    summary: String,
    keywords: Vec<String>,
) -> Result<(), AppError> {
    Ok(memory::add_conversation_summary(&summary, &keywords)
        .await
        .map(|_| ())?)
}

#[tauri::command]
pub async fn memory_get_recent_conversations(
    limit: usize,
) -> Result<Vec<memory::ConversationSummary>, AppError> {
    Ok(memory::get_recent_conversations(limit).await?)
}

#[tauri::command]
pub async fn memory_search_conversations(
    query: String,
) -> Result<Vec<memory::ConversationSummary>, AppError> {
    Ok(memory::search_conversations(&query).await?)
}

#[tauri::command]
pub async fn get_music_state() -> Result<music::MusicInfo, AppError> {
    Ok(music::poll_music_state().await?)
}

#[tauri::command]
pub async fn check_initiative() -> Result<Option<String>, AppError> {
    let proposals = initiative::check_for_initiatives();
    Ok(proposals
        .into_iter()
        .next()
        .map(|proposal| proposal.message))
}

#[tauri::command]
pub async fn record_activity() -> Result<(), AppError> {
    initiative::update_initiative_context(initiative::PartialContext {
        workspace_id: None,
        active_tool: None,
        activity: Some(initiative::ActivityRecord {
            timestamp: std::time::Instant::now(),
            activity_type: "desktop_activity".to_string(),
            details: serde_json::json!({}),
        }),
        touch: true,
        current_task: None,
    });
    Ok(())
}

#[tauri::command]
pub async fn list_scenes() -> Result<Vec<scene::Scene>, AppError> {
    let manager = scene::load_manager().await?;
    Ok(manager.list_scenes().into_iter().cloned().collect())
}

#[tauri::command]
pub async fn switch_scene(scene_id: String) -> Result<bool, AppError> {
    let mut manager = scene::load_manager().await?;
    let switched = manager.switch_scene(&scene_id);
    if switched {
        scene::save_state(manager.get_state()).await?;
    }
    Ok(switched)
}

#[tauri::command]
pub async fn get_current_scene() -> Result<Option<scene::Scene>, AppError> {
    let manager = scene::load_manager().await?;
    Ok(manager.get_current_scene().cloned())
}

#[tauri::command]
pub async fn get_current_persona() -> Result<Option<persona::Persona>, AppError> {
    let manager = persona::load_manager().await?;
    Ok(manager.get_current_persona().cloned())
}

#[tauri::command]
pub async fn list_personas() -> Result<Vec<persona::Persona>, AppError> {
    let manager = persona::load_manager().await?;
    Ok(manager.list_personas().into_iter().cloned().collect())
}

#[tauri::command]
pub async fn set_current_persona(id: String) -> Result<bool, AppError> {
    let mut manager = persona::load_manager().await?;
    let updated = manager.set_current_persona(&id);
    if updated {
        persona::save_state(&manager.to_state()).await?;
    }
    Ok(updated)
}

#[tauri::command]
pub async fn generate_prompt(
    template_id: String,
    variables: serde_json::Value,
) -> Result<Option<String>, AppError> {
    let manager = persona::load_manager().await?;
    let vars: std::collections::HashMap<String, String> = serde_json::from_value(variables)
        .map_err(|err| AppError::Validation(format!("Invalid variables: {}", err)))?;
    Ok(manager.generate_prompt(&template_id, &vars))
}

#[tauri::command]
pub async fn get_image_prompt(prompt_id: String) -> Result<Option<persona::ImagePrompt>, AppError> {
    let manager = persona::load_manager().await?;
    Ok(manager.get_image_prompt(&prompt_id).cloned())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingStatus {
    pub completed_steps: Vec<String>,
    pub next_step: Option<String>,
    pub next_step_description: Option<String>,
    pub is_complete: bool,
}

#[tauri::command]
pub async fn onboarding_status() -> Result<OnboardingStatus, AppError> {
    let mut completed_steps = Vec::new();

    // "welcome" — always completed (app is running)
    completed_steps.push("welcome".to_string());

    // "llm_config" — check if LLM API key is configured
    let llm_configured = match config::load().await {
        Ok(cfg) => {
            cfg.llm.api_key_set
                || cfg
                    .llm
                    .api_key
                    .as_ref()
                    .is_some_and(|k| !k.trim().is_empty())
        }
        Err(_) => false,
    };
    if llm_configured {
        completed_steps.push("llm_config".to_string());
    }

    // "first_chat" — check if any chat sessions exist
    let has_chat = chat::list_chat_sessions(Some(1))
        .await
        .map(|sessions| !sessions.is_empty())
        .unwrap_or(false);
    if has_chat {
        completed_steps.push("first_chat".to_string());
    }

    // "first_task" — check if any tasks exist
    let has_tasks = tasks::load()
        .await
        .map(|file| !file.tasks.is_empty())
        .unwrap_or(false);
    if has_tasks {
        completed_steps.push("first_task".to_string());
    }

    // "workspace" — check if any workspace is attached
    let has_workspace = workspaces::list_all()
        .await
        .map(|ws| !ws.is_empty())
        .unwrap_or(false);
    if has_workspace {
        completed_steps.push("workspace".to_string());
    }

    let all_steps = [
        "welcome",
        "llm_config",
        "first_chat",
        "first_task",
        "workspace",
    ];
    let next_step = all_steps
        .iter()
        .find(|step| !completed_steps.contains(&step.to_string()))
        .map(|s| s.to_string());

    let next_step_description = next_step.as_deref().map(|step| match step {
        "llm_config" => "配置 AI 模型 API Key，让助手能够与你对话",
        "first_chat" => "发送你的第一条消息，开始和助手聊天",
        "first_task" => "创建你的第一个任务，让助手帮你管理待办",
        "workspace" => "关联一个工作区目录，助手可以读写你的项目文件",
        _ => "",
    });

    let is_complete = next_step.is_none();
    Ok(OnboardingStatus {
        completed_steps,
        next_step,
        next_step_description: next_step_description.map(String::from),
        is_complete,
    })
}

#[tauri::command]
#[allow(deprecated)]
pub async fn list_skills() -> Result<Vec<skills::SkillSpec>, AppError> {
    Ok(skills::list_skills().await?)
}

#[tauri::command]
#[allow(deprecated)]
pub async fn import_skills(json: String) -> Result<Vec<skills::SkillSpec>, AppError> {
    Ok(skills::import_skills_from_json(&json).await?)
}

#[tauri::command]
#[allow(deprecated)]
pub async fn save_skills(skills_list: Vec<skills::SkillSpec>) -> Result<(), AppError> {
    Ok(skills::save_skills(&skills_list).await?)
}

// ── Memory management commands ────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntryDto {
    pub id: String,
    pub key: String,
    pub value: String,
    pub category: String,
    pub scope: String,
    pub source: String,
    pub confidence: f64,
    pub sensitivity: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<memory::MemoryEntry> for MemoryEntryDto {
    fn from(e: memory::MemoryEntry) -> Self {
        Self {
            id: e.id,
            key: e.key,
            value: e.value,
            category: e.category,
            scope: e.scope.as_str().to_string(),
            source: e.source.as_str().to_string(),
            confidence: e.confidence,
            sensitivity: e.sensitivity.as_str().to_string(),
            status: e.status,
            created_at: e.created_at.to_rfc3339(),
            updated_at: e.updated_at.to_rfc3339(),
        }
    }
}

#[tauri::command]
pub async fn memory_list(
    category: Option<String>,
    status: Option<String>,
) -> Result<Vec<MemoryEntryDto>, AppError> {
    let entries = memory::list_all(category.as_deref(), status.as_deref()).await?;
    Ok(entries.into_iter().map(MemoryEntryDto::from).collect())
}

#[tauri::command]
pub async fn memory_update_status(id: String, status: String) -> Result<bool, AppError> {
    let valid = [
        "active",
        "archived",
        "forgotten",
        "candidate",
        "quarantined",
    ];
    if !valid.contains(&status.as_str()) {
        return Err(AppError::Validation(format!(
            "invalid status '{}'; must be one of: {:?}",
            status, valid
        )));
    }
    Ok(memory::update_status_by_id(&id, &status).await?)
}

#[tauri::command]
pub async fn memory_forget(id: String) -> Result<bool, AppError> {
    Ok(memory::forget_by_id(&id).await?)
}

#[tauri::command]
pub async fn memory_rebuild_embeddings() -> Result<u64, AppError> {
    Ok(memory::rebuild_embeddings().await?)
}

// ── Skill Package commands (TASK-071) ────────────────────────────────────

#[tauri::command]
pub async fn import_skill_markdown(content: String) -> Result<skills::SkillPackage, AppError> {
    Ok(skills::import_skill_markdown(&content).await?)
}

#[tauri::command]
pub async fn list_skill_packages() -> Result<Vec<skills::SkillPackage>, AppError> {
    Ok(skills::list_skill_packages().await?)
}

#[tauri::command]
pub async fn update_skill_enabled(id: String, enabled: bool) -> Result<bool, AppError> {
    Ok(skills::update_skill_enabled(&id, enabled).await?)
}

#[tauri::command]
pub async fn delete_skill_package(id: String) -> Result<bool, AppError> {
    Ok(skills::delete_skill_package(&id).await?)
}

// ── Connector commands (TASK-071) ────────────────────────────────────────

#[tauri::command]
pub async fn list_connectors() -> Result<Vec<connectors::ConnectorSpec>, AppError> {
    Ok(connectors::ConnectorRegistry::list().await?)
}

// ── Goal commands (TASK-097) ─────────────────────────────────────────────

#[tauri::command]
pub async fn list_goals(
    workspace_id: String,
    status: Option<String>,
) -> Result<Vec<goals::GoalRun>, AppError> {
    Ok(goals::list_goals(&workspace_id, status.as_deref(), None).await?)
}

#[tauri::command]
pub async fn create_goal(
    app: AppHandle,
    workspace_id: String,
    title: String,
    objective: String,
    priority: Option<String>,
    owner: Option<String>,
) -> Result<goals::GoalRun, AppError> {
    let goal = goals::create_goal(
        &workspace_id,
        &title,
        &objective,
        priority.as_deref().unwrap_or("normal"),
        owner.as_deref().unwrap_or("user"),
        None,
        None,
    )
    .await?;
    let _ = app.emit("goals_changed", ());
    Ok(goal)
}

/// Persist a user message into a session's timeline WITHOUT triggering an
/// LLM turn. Used by the goal-first-send path so the user's objective text
/// shows up in the conversation while the orchestrator drives execution.
#[tauri::command]
pub async fn append_goal_user_message(
    app: AppHandle,
    session_id: String,
    content: String,
) -> Result<(), AppError> {
    let content = content.trim();
    if content.is_empty() {
        return Ok(());
    }
    let message = chat::append_user_message_to_session(&session_id, content).await?;
    let _ = app.emit(
        "reply_stored",
        serde_json::json!({
            "message_id": message.id,
            "session_id": session_id,
        }),
    );
    Ok(())
}

#[tauri::command]
pub async fn update_goal_status(
    app: AppHandle,
    goal_id: String,
    status: String,
) -> Result<goals::GoalRun, AppError> {
    let previous = goals::get_goal(&goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found: {goal_id}")))?;
    let goal = goals::update_goal_status(&goal_id, &status).await?;
    project_goal_status_change_to_session(&app, &previous.status, &goal).await?;
    let _ = app.emit("goals_changed", ());
    Ok(goal)
}

#[tauri::command]
pub async fn update_goal_objective(
    app: AppHandle,
    goal_id: String,
    title: String,
    objective: String,
) -> Result<goals::GoalRun, AppError> {
    let goal = goals::update_goal_objective(&goal_id, &title, &objective).await?;
    let _ = app.emit("goals_changed", ());
    Ok(goal)
}

#[tauri::command]
pub async fn start_goal(app: AppHandle, goal_id: String) -> Result<goals::GoalRun, AppError> {
    let goal = goals::get_goal(&goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found: {goal_id}")))?;
    let orchestrator = conductor_core::goal_orchestrator::GoalOrchestrator::new(
        conductor_core::goal_orchestrator::OrchestratorConfig {
            workspace_id: goal.workspace_id.clone(),
            ..Default::default()
        },
    );
    orchestrator.start(&goal_id).await?;
    let updated = goals::get_goal(&goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found after start: {goal_id}")))?;
    project_goal_status_change_to_session(&app, &goal.status, &updated).await?;
    let _ = app.emit("goals_changed", ());
    let _ = app.emit("agent_teams_changed", ());
    Ok(updated)
}

#[tauri::command]
pub async fn pause_goal(app: AppHandle, goal_id: String) -> Result<goals::GoalRun, AppError> {
    let goal = goals::get_goal(&goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found: {goal_id}")))?;
    let orchestrator = conductor_core::goal_orchestrator::GoalOrchestrator::new(
        conductor_core::goal_orchestrator::OrchestratorConfig {
            workspace_id: goal.workspace_id.clone(),
            ..Default::default()
        },
    );
    orchestrator.pause(&goal_id).await?;
    let updated = goals::get_goal(&goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found after pause: {goal_id}")))?;
    project_goal_status_change_to_session(&app, &goal.status, &updated).await?;
    let _ = app.emit("goals_changed", ());
    Ok(updated)
}

#[tauri::command]
pub async fn resume_goal(app: AppHandle, goal_id: String) -> Result<goals::GoalRun, AppError> {
    let goal = goals::get_goal(&goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found: {goal_id}")))?;
    let orchestrator = conductor_core::goal_orchestrator::GoalOrchestrator::new(
        conductor_core::goal_orchestrator::OrchestratorConfig {
            workspace_id: goal.workspace_id.clone(),
            ..Default::default()
        },
    );
    orchestrator.resume(&goal_id).await?;
    let updated = goals::get_goal(&goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found after resume: {goal_id}")))?;
    project_goal_status_change_to_session(&app, &goal.status, &updated).await?;
    let _ = app.emit("goals_changed", ());
    Ok(updated)
}

#[tauri::command]
pub async fn cancel_goal(app: AppHandle, goal_id: String) -> Result<goals::GoalRun, AppError> {
    let goal = goals::get_goal(&goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found: {goal_id}")))?;
    let orchestrator = conductor_core::goal_orchestrator::GoalOrchestrator::new(
        conductor_core::goal_orchestrator::OrchestratorConfig {
            workspace_id: goal.workspace_id.clone(),
            ..Default::default()
        },
    );
    orchestrator.cancel(&goal_id).await?;
    let updated = goals::get_goal(&goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found after cancel: {goal_id}")))?;
    project_goal_status_change_to_session(&app, &goal.status, &updated).await?;
    let _ = app.emit("goals_changed", ());
    Ok(updated)
}

#[tauri::command]
pub async fn approve_goal_plan(
    app: AppHandle,
    goal_id: String,
) -> Result<goals::GoalRun, AppError> {
    let previous = goals::get_goal(&goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found: {goal_id}")))?;
    let updated = advance_goal_after_plan_approval(&goal_id).await?;
    project_goal_status_change_to_session(&app, &previous.status, &updated).await?;
    let _ = app.emit("goals_changed", ());
    let _ = app.emit("agent_teams_changed", ());
    Ok(updated)
}

#[tauri::command]
pub async fn reject_goal_plan(app: AppHandle, goal_id: String) -> Result<goals::GoalRun, AppError> {
    let previous = goals::get_goal(&goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found: {goal_id}")))?;
    conductor_core::goal_orchestrator::reject_goal_plan(&goal_id).await?;
    let updated = goals::get_goal(&goal_id).await?.ok_or_else(|| {
        AppError::NotFound(format!("goal not found after plan rejection: {goal_id}"))
    })?;
    project_goal_status_change_to_session(&app, &previous.status, &updated).await?;
    let _ = app.emit("goals_changed", ());
    let _ = app.emit("agent_teams_changed", ());
    Ok(updated)
}

#[tauri::command]
pub async fn submit_goal_review_verdict(
    app: AppHandle,
    goal_id: String,
    accepted: bool,
) -> Result<goals::GoalRun, AppError> {
    let previous = goals::get_goal(&goal_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("goal not found: {goal_id}")))?;
    conductor_core::goal_orchestrator::apply_goal_review_verdict(&goal_id, accepted).await?;
    let updated = goals::get_goal(&goal_id).await?.ok_or_else(|| {
        AppError::NotFound(format!("goal not found after review verdict: {goal_id}"))
    })?;
    project_goal_status_change_to_session(&app, &previous.status, &updated).await?;
    let _ = app.emit("goals_changed", ());
    let _ = app.emit("agent_teams_changed", ());
    Ok(updated)
}

#[tauri::command]
pub async fn get_goal_cycles(goal_id: String) -> Result<Vec<goals::GoalCycle>, AppError> {
    Ok(goals::list_cycles_by_goal(&goal_id).await?)
}

// ── Agent heartbeat commands (TASK-098) ──────────────────────────────────

#[tauri::command]
pub async fn list_active_heartbeats(
    workspace_id: String,
) -> Result<Vec<heartbeat::AgentHeartbeat>, AppError> {
    Ok(heartbeat::get_active_heartbeats(&workspace_id).await?)
}

// ── Goal tasks commands (TASK-100) ───────────────────────────────────────

#[tauri::command]
pub async fn list_goal_tasks(goal_id: String) -> Result<Vec<goal_tasks::AgentTask>, AppError> {
    Ok(goal_tasks::list_tasks_by_goal(&goal_id).await?)
}

// ── Event transcript commands (TASK-101) ─────────────────────────────────

#[tauri::command]
pub async fn list_goal_events(
    workspace_id: String,
    limit: Option<u32>,
) -> Result<Vec<events::AuditEvent>, AppError> {
    Ok(events::query_events_db(&workspace_id, None, limit.or(Some(50))).await?)
}

#[tauri::command]
pub async fn write_workspace_projection(workspace_id: String) -> Result<String, AppError> {
    let path = projection::ProjectionWriter::new(&workspace_id)
        .write_to_file()
        .await?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub async fn list_workspace_activity_projection(
    workspace_id: String,
    limit: Option<u32>,
) -> Result<projection::WorkspaceActivityProjection, AppError> {
    Ok(projection::list_workspace_activities(&workspace_id, limit).await?)
}

#[cfg(test)]
mod tests {
    use super::advance_goal_after_plan_approval;
    use conductor_core::{agent_teams, goal_orchestrator::GoalOrchestrator, goal_tasks, goals};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct TestRoot {
        _guard: MutexGuard<'static, ()>,
        _temp: tempfile::TempDir,
        previous: Option<std::ffi::OsString>,
    }

    impl TestRoot {
        fn new() -> Self {
            let guard = ENV_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .expect("test env lock poisoned");
            let previous = std::env::var_os("CONDUCTOR_ROOT");
            let temp = tempfile::tempdir().expect("create temp conductor root");
            std::env::set_var("CONDUCTOR_ROOT", temp.path());
            Self {
                _guard: guard,
                _temp: temp,
                previous,
            }
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var("CONDUCTOR_ROOT", previous);
            } else {
                std::env::remove_var("CONDUCTOR_ROOT");
            }
        }
    }

    #[tokio::test]
    async fn approve_goal_plan_helper_advances_goal_into_execution() {
        let _root = TestRoot::new();
        let goal = goals::create_goal(
            "ws-command-approval",
            "Approve from command",
            "advance immediately after plan approval",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");
        goals::update_goal_status(&goal.id, "planning")
            .await
            .expect("set planning");

        let orchestrator =
            GoalOrchestrator::new(conductor_core::goal_orchestrator::OrchestratorConfig {
                workspace_id: "ws-command-approval".to_string(),
                ..Default::default()
            });
        for _ in 0..4 {
            orchestrator
                .tick_goal(&goal.id)
                .await
                .expect("tick to approval");
        }

        let approved = advance_goal_after_plan_approval(&goal.id)
            .await
            .expect("approve and advance");
        let cycle_id = approved
            .current_cycle_id
            .clone()
            .expect("approved goal cycle id");
        let cycle = goals::get_cycle(&cycle_id)
            .await
            .expect("load cycle")
            .expect("cycle exists");
        let team = agent_teams::get_team(&format!("team-{cycle_id}"))
            .await
            .expect("execution team exists");
        let tasks = goal_tasks::list_tasks_by_cycle(&cycle_id)
            .await
            .expect("list tasks");

        assert_eq!(approved.status, "running");
        assert_eq!(cycle.status, "executing");
        assert_eq!(team.lifecycle.as_str(), "executing");
        assert!(!tasks.is_empty());
        assert!(tasks.iter().all(|task| task.status == "queued"));
    }
}
