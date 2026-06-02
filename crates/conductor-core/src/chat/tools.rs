use crate::{
    chat::types::ChatTaskMode,
    config::CoreConfig,
    connectors::ConnectorRegistry,
    llm::{ToolDefinition, ToolFunction},
    policy::PolicyEngine,
    proposals::RiskLevel,
    tools::ToolPermission,
};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

#[cfg(feature = "tauri-events")]
use tauri::Emitter;

const TOOL_SEARCH_ID: &str = "tool.search";

pub(super) async fn build_tool_definitions(
    config: &CoreConfig,
    user_prompt: &str,
    task_mode: ChatTaskMode,
    plan_only: bool,
) -> Vec<ToolDefinition> {
    build_tool_definitions_with_allowed_ids(config, user_prompt, task_mode, plan_only, None).await
}

pub(super) async fn build_tool_definitions_with_allowed_ids(
    config: &CoreConfig,
    user_prompt: &str,
    task_mode: ChatTaskMode,
    plan_only: bool,
    allowed_tool_ids_override: Option<&[String]>,
) -> Vec<ToolDefinition> {
    crate::tools::register_builtin_tools();
    if let Some(allowed_tool_ids_override) =
        allowed_tool_ids_override.filter(|tool_ids| !tool_ids.is_empty())
    {
        return tool_definitions_for_ids(config, allowed_tool_ids_override, task_mode, plan_only);
    }

    let allowed_ids = &config.llm.allowed_tool_ids;
    let context_ids = contextual_tool_ids(user_prompt, &config.tool_tiers);
    let all_tools = crate::tools::list_tools();

    // ── New path: match_enabled_skills → collect_capabilities → PolicyEngine.filter_tools ──
    let matched_skills = crate::skills::match_enabled_skills(user_prompt, None)
        .await
        .unwrap_or_default();

    let policy_tool_ids: Vec<String> = if !matched_skills.is_empty() {
        // Default all capabilities to user-authorized (policy 7).
        // User-level authorization is managed separately; here we pass `true` so
        // that the connector-level checks (policies 4-6, 8-9) are exercised.
        let caps = crate::skills::collect_capabilities(&matched_skills);
        let grants: Vec<(String, bool)> = caps.into_iter().map(|c| (c, true)).collect();

        let results = PolicyEngine::filter_tools(&matched_skills, &ConnectorRegistry, &grants)
            .await
            .unwrap_or_default();

        results
            .iter()
            .filter(|r| {
                r.reason.is_none()
                    || r.reason
                        .as_deref()
                        .is_some_and(|reason| reason.starts_with("connector auth status"))
            })
            .flat_map(|r| r.allowed_tools.clone())
            .collect()
    } else {
        vec![]
    };

    // ── Legacy fallback: PolicyEngine returned no tools but allowed_tool_ids is non-empty ──
    if matches!(task_mode, ChatTaskMode::Long) {
        let mut final_ids = long_mode_default_tool_ids(config);
        final_ids.extend(policy_tool_ids);
        final_ids.extend(context_ids);
        final_ids.sort();
        final_ids.dedup();
        return tool_definitions_for_ids(config, &final_ids, task_mode, plan_only);
    }

    if policy_tool_ids.is_empty() && !allowed_ids.is_empty() {
        #[allow(deprecated)]
        let skill_ids = crate::skills::skill_contextual_tools(user_prompt);

        let selected_ids: Vec<String> = all_tools
            .iter()
            .filter(|spec| {
                allowed_ids.contains(&spec.id)
                    || context_ids.contains(&spec.id)
                    || skill_ids.contains(&spec.id)
            })
            .map(|spec| spec.id.clone())
            .collect();
        return tool_definitions_for_ids(config, &selected_ids, task_mode, plan_only);
    }

    // ── New path: merge policy-approved tools with context-tier tools ──
    let mut final_ids = policy_tool_ids;
    final_ids.extend(context_ids);
    final_ids.sort();
    final_ids.dedup();

    tool_definitions_for_ids(config, &final_ids, task_mode, plan_only)
}

pub(super) async fn build_tool_definitions_for_catalog_selection(
    config: &CoreConfig,
    selected_tool_ids: &[String],
    task_mode: ChatTaskMode,
    plan_only: bool,
) -> anyhow::Result<Vec<ToolDefinition>> {
    let mut final_ids = vec![TOOL_SEARCH_ID.to_string()];
    final_ids.extend(PolicyEngine::authorize_tool_ids(selected_tool_ids).await?);
    Ok(tool_definitions_for_ids(
        config, &final_ids, task_mode, plan_only,
    ))
}

pub(super) fn should_use_progressive_tool_discovery(
    prompt: &str,
    task_mode: ChatTaskMode,
    prebuilt_tool_count: usize,
) -> bool {
    if prebuilt_tool_count <= 1 {
        return false;
    }

    let lower = prompt.to_ascii_lowercase();
    let char_count = lower.chars().count();
    let line_count = lower.lines().filter(|line| !line.trim().is_empty()).count();
    let coordination_hits = [
        " and ",
        " then ",
        " also ",
        " after that ",
        " compare ",
        " integrate ",
        " synchronize ",
        " workflow ",
        "同时",
        "然后",
        "并且",
        "接着",
        "再",
    ]
    .iter()
    .filter(|needle| lower.contains(**needle))
    .count();
    let connector_hits = [
        "lark",
        "calendar",
        "doc",
        "document",
        "email",
        "mail",
        "schedule",
        "tool",
        "connector",
        "api",
    ]
    .iter()
    .filter(|needle| lower.contains(**needle))
    .count();

    char_count >= 160 || line_count >= 3 || coordination_hits >= 2 || connector_hits >= 3
    // Long-task mode already has the full tool set — progressive discovery
    // would replace it with only tool.search, blinding the LLM.
    // && !matches!(task_mode, ChatTaskMode::Long) is implicit: Long callers
    // skip this function entirely (handled below).
}

fn contextual_tool_ids(prompt: &str, tool_tiers: &[crate::config::ToolTierConfig]) -> Vec<String> {
    let lower = prompt.to_lowercase();
    let mut ids = Vec::new();

    for tier in tool_tiers {
        if !tier.enabled {
            continue;
        }
        if tier.keywords.iter().any(|kw| lower.contains(kw.as_str())) {
            ids.extend(tier.tool_ids.iter().cloned());
        }
    }

    ids
}

fn long_mode_default_tool_ids(config: &CoreConfig) -> Vec<String> {
    let mut ids = vec![TOOL_SEARCH_ID.to_string()];
    ids.extend(config.llm.allowed_tool_ids.iter().cloned());
    for tier in config.tool_tiers.iter().filter(|tier| tier.enabled) {
        ids.extend(tier.tool_ids.iter().cloned());
    }
    ids.sort();
    ids.dedup();
    ids
}

pub(super) fn tool_name_for_llm(tool_id: &str) -> String {
    tool_id.replace('.', "__")
}

pub(super) fn tool_id_from_llm_name(name: &str) -> String {
    name.replace("__", ".")
}

fn tool_definitions_for_ids(
    config: &CoreConfig,
    tool_ids: &[String],
    task_mode: ChatTaskMode,
    plan_only: bool,
) -> Vec<ToolDefinition> {
    let wanted: HashSet<&str> = tool_ids.iter().map(String::as_str).collect();
    filter_tool_specs_for_mode(
        crate::tools::list_tools().iter().filter(|spec| {
            if !wanted.contains(spec.id.as_str()) {
                return false;
            }
            if config.pet.avatar_locked
                && matches!(
                    spec.id.as_str(),
                    "pet.set_avatar" | "conductor.pet.set_avatar"
                )
            {
                return false;
            }
            true
        }),
        task_mode,
        plan_only,
    )
    .into_iter()
    .map(|spec| ToolDefinition {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: tool_name_for_llm(&spec.id),
            description: spec.description.clone(),
            parameters: spec.input_schema.clone(),
        },
    })
    .collect()
}

fn filter_tool_specs_for_mode<'a>(
    specs: impl IntoIterator<Item = &'a crate::tools::ToolSpec>,
    task_mode: ChatTaskMode,
    plan_only: bool,
) -> Vec<&'a crate::tools::ToolSpec> {
    specs
        .into_iter()
        .filter(|spec| tool_allowed_in_mode(spec, task_mode, plan_only))
        .collect()
}

fn tool_allowed_in_mode(
    spec: &crate::tools::ToolSpec,
    task_mode: ChatTaskMode,
    plan_only: bool,
) -> bool {
    // All tools are callable in both modes — the distinction is only in whether
    // they are included in the *default* tool_defs set.
    // Agent/subagent tools are excluded from the short-task default list so the
    // LLM doesn't reach for them unprompted, but they are NOT hard-blocked:
    // if the user explicitly asks (keyword match via contextual_tool_ids), they
    // will be added back in `build_tool_definitions`.
    let _ = task_mode; // mode only affects contextual inclusion, not hard gating

    if !plan_only {
        return true;
    }

    if spec.risk_level > RiskLevel::ReadOnly {
        return false;
    }

    !spec.permissions.iter().any(|permission| {
        matches!(
            permission,
            ToolPermission::WriteWorkspace
                | ToolPermission::WriteExternalPath
                | ToolPermission::SystemControl
                | ToolPermission::SendMessage
        )
    })
}

// ── Tauri-specific tool execution ────────────────────────────────────────────

/// When `validate_trust_level` returns an `approval_required:` error for an
/// AskWrite workspace, create a Proposal so the user can approve the tool.
#[cfg(feature = "tauri-events")]
async fn maybe_create_askwrite_proposal(
    tc: &crate::llm::ToolCall,
    args: &serde_json::Value,
    tool_id: &str,
    workspace_id: Option<&str>,
    error: &str,
    app_handle: &tauri::AppHandle,
) -> anyhow::Result<Option<crate::proposals::Proposal>> {
    if !error.starts_with("approval_required:") {
        return Ok(None);
    }

    let Some(workspace_id) = workspace_id else {
        return Ok(None);
    };

    // Check for existing open proposal for the same tool request
    let tool_input_json = serde_json::to_string(args)?;
    if let Some(existing) =
        crate::proposals::find_open_by_tool_request(Some(workspace_id), tool_id, &tool_input_json)
            .await?
    {
        let _ = app_handle.emit(
            "proposal-changed",
            serde_json::json!({
                "id": existing.id,
                "status": existing.status.as_str(),
                "workspace_id": existing.workspace_id,
                "tool_id": existing.tool_id,
            }),
        );
        return Ok(Some(existing));
    }

    let workspace = crate::workspaces::get(workspace_id).await?;
    let title = format!("审批工具调用: {}", tool_id);
    let content = format!(
        "工具 `{}` 在 AskWrite 工作区中请求执行写入操作。审批通过后可继续执行。",
        tc.function.name,
    );
    let reason = format!(
        "当前会话工作区 {} 为 AskWrite 模式，工具 {} 的风险等级需要用户审批。",
        workspace.name, tool_id,
    );

    // Determine risk level from tool spec
    let risk_level = if let Some((spec, _)) = crate::tools::get_tool(tool_id) {
        spec.risk_level
    } else {
        crate::proposals::RiskLevel::WorkspaceWrite
    };

    let proposal = crate::proposals::Proposal {
        id: crate::proposals::next_id().await?,
        workspace_id: Some(workspace.id.clone()),
        for_cwd: workspace.root.clone(),
        source: crate::proposals::ProposalSource::Chat,
        title,
        content,
        reason,
        tool_id: Some(tool_id.to_string()),
        tool_input_json: Some(tool_input_json),
        risk_level,
        dry_run: false,
        status: crate::proposals::ProposalStatus::Pending,
        result_ref: None,
        agent_task_id: None,
        grant_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    crate::proposals::create(proposal.clone()).await?;
    crate::events::emit_tool_call_blocked(tool_id, &proposal.id, "approval_required").await;
    let _ = app_handle.emit(
        "proposal-changed",
        serde_json::json!({
            "id": proposal.id,
            "status": proposal.status.as_str(),
            "workspace_id": proposal.workspace_id,
            "tool_id": proposal.tool_id,
        }),
    );
    let _ = app_handle.emit(
        "tasks_changed",
        serde_json::json!({ "reason": "proposal_created" }),
    );
    Ok(Some(proposal))
}

#[cfg(feature = "tauri-events")]
pub(super) async fn maybe_create_external_access_proposal(
    tc: &crate::llm::ToolCall,
    args: &serde_json::Value,
    tool_id: &str,
    workspace_id: Option<&str>,
    error: &str,
    app_handle: &tauri::AppHandle,
) -> anyhow::Result<Option<crate::proposals::Proposal>> {
    if !error.contains("outside current workspace") {
        return Ok(None);
    }

    let Some(workspace_id) = workspace_id else {
        return Ok(None);
    };

    let workspace = crate::workspaces::get(workspace_id).await?;
    let target = args
        .get("path")
        .and_then(|value| value.as_str())
        .or_else(|| args.get("working_dir").and_then(|value| value.as_str()))
        .unwrap_or("<unknown path>");
    let tool_input_json = serde_json::to_string(args)?;
    if let Some(existing) =
        crate::proposals::find_open_by_tool_request(Some(workspace_id), tool_id, &tool_input_json)
            .await?
    {
        let _ = app_handle.emit(
            "proposal-changed",
            serde_json::json!({
                "id": existing.id,
                "status": existing.status.as_str(),
                "workspace_id": existing.workspace_id,
                "tool_id": existing.tool_id,
            }),
        );
        return Ok(Some(existing));
    }
    let title = match tool_id {
        "file.read" => format!("读取工作区外路径 {target}"),
        "file.write" => format!("写入工作区外路径 {target}"),
        "file.edit" => format!("修改工作区外路径 {target}"),
        "file.glob" => format!("扫描工作区外路径 {target}"),
        "file.grep" => format!("搜索工作区外路径 {target}"),
        "file.stat" => format!("查看工作区外路径 {target}"),
        "bash.execute" => format!("在工作区外目录执行命令 {target}"),
        _ => format!("访问工作区外路径 {target}"),
    };
    let content = format!(
        "工具 `{}` 请求访问工作区外路径 `{}`。审批通过后可继续执行。",
        tc.function.name, target
    );
    let reason = format!(
        "当前会话工作区是 {}，本次请求访问其外部路径 {}。",
        workspace.root.display(),
        target
    );
    let risk_level = match tool_id {
        "file.write" | "file.edit" | "bash.execute" => {
            crate::proposals::RiskLevel::ExternalSideEffect
        }
        _ => crate::proposals::RiskLevel::ReadOnly,
    };
    let proposal = crate::proposals::Proposal {
        id: crate::proposals::next_id().await?,
        workspace_id: Some(workspace.id.clone()),
        for_cwd: workspace.root.clone(),
        source: crate::proposals::ProposalSource::Chat,
        title,
        content,
        reason,
        tool_id: Some(tool_id.to_string()),
        tool_input_json: Some(tool_input_json),
        risk_level,
        dry_run: false,
        status: crate::proposals::ProposalStatus::Pending,
        result_ref: None,
        agent_task_id: None,
        grant_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    crate::proposals::create(proposal.clone()).await?;
    crate::events::emit_tool_call_blocked(tool_id, &proposal.id, "approval_required").await;
    let _ = app_handle.emit(
        "proposal-changed",
        serde_json::json!({
            "id": proposal.id,
            "status": proposal.status.as_str(),
            "workspace_id": proposal.workspace_id,
            "tool_id": proposal.tool_id,
        }),
    );
    let _ = app_handle.emit(
        "tasks_changed",
        serde_json::json!({ "reason": "proposal_created" }),
    );
    Ok(Some(proposal))
}

#[cfg(feature = "tauri-events")]
pub(super) async fn execute_tool_call(
    tc: &crate::llm::ToolCall,
    session_id: Option<&str>,
    workspace_id: Option<&str>,
    approved_write_scope: Option<&[String]>,
    app_handle: &tauri::AppHandle,
) -> (String, bool, u64, &'static str) {
    let args: serde_json::Value =
        serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));
    let tool_id = tool_id_from_llm_name(&tc.function.name);
    let risk_level =
        crate::tools::get_tool(&tool_id).map(|(spec, _)| spec.risk_level.as_str().to_string());

    // Record ToolCall before execution
    let tc_id = uuid::Uuid::new_v4().to_string();
    let _tool_call = crate::tool_calls::create(crate::tool_calls::ToolCallCreate {
        id: tc_id.clone(),
        session_id: session_id.map(str::to_string),
        workspace_id: workspace_id.map(str::to_string),
        llm_tool_call_id: Some(tc.id.clone()),
        tool_id: tool_id.clone(),
        input_json: tc.function.arguments.clone(),
        agent_run_id: None,
        risk_level: risk_level.clone(),
    })
    .await;

    crate::events::emit_tool_call_lifecycle(
        "tool_call.proposed",
        &tc_id,
        workspace_id,
        session_id,
        &tool_id,
        "pending",
        serde_json::json!({
            "llm_tool_call_id": tc.id.as_str(),
            "risk_level": risk_level.as_deref(),
            "input": args,
        }),
    )
    .await;

    let mut exec_args = args.clone();
    if let Some(obj) = exec_args.as_object_mut() {
        obj.insert(
            "tool_call_id".to_string(),
            serde_json::json!(tc_id.as_str()),
        );
        if let Some(session_id) = session_id {
            obj.entry("session_id".to_string())
                .or_insert_with(|| serde_json::json!(session_id));
        }
    }

    let start = std::time::Instant::now();
    let _ = crate::tool_calls::mark_executing(&tc_id).await;
    crate::events::emit_tool_call_lifecycle(
        "tool_call.executing",
        &tc_id,
        workspace_id,
        session_id,
        &tool_id,
        "executing",
        serde_json::json!({
            "llm_tool_call_id": tc.id.as_str(),
            "risk_level": risk_level.as_deref(),
        }),
    )
    .await;
    if let Err(error) =
        validate_approved_write_scope(&tool_id, &exec_args, workspace_id, approved_write_scope)
            .await
    {
        let duration_ms = start.elapsed().as_millis() as u64;
        let _ = crate::tool_calls::fail(&tc_id, &error.to_string()).await;
        crate::events::emit_tool_call_lifecycle(
            "tool_call.blocked",
            &tc_id,
            workspace_id,
            session_id,
            &tool_id,
            "blocked",
            serde_json::json!({
                "llm_tool_call_id": tc.id.as_str(),
                "risk_level": risk_level.as_deref(),
                "reason": error.to_string(),
                "duration_ms": duration_ms,
            }),
        )
        .await;
        return (
            serde_json::json!({ "error": error.to_string() }).to_string(),
            false,
            duration_ms,
            "blocked",
        );
    }
    let result =
        crate::tools::execute_tool_with_workspace_async(&tool_id, &exec_args, workspace_id).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(result) => {
            let output_str =
                serde_json::to_string(&result.output).unwrap_or_else(|_| "{}".to_string());
            let command_run_id = result
                .output
                .get("command_run_id")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            if let Some(ref command_run_id) = command_run_id {
                let _ = crate::tool_calls::attach_command_run(&tc_id, command_run_id).await;
            }
            let _ = if result.success {
                crate::tool_calls::complete(&tc_id, &output_str).await
            } else {
                crate::tool_calls::fail(&tc_id, &output_str).await
            };
            crate::events::emit_tool_call_lifecycle(
                "tool_call.finished",
                &tc_id,
                workspace_id,
                session_id,
                &tool_id,
                if result.success {
                    "succeeded"
                } else {
                    "failed"
                },
                serde_json::json!({
                    "llm_tool_call_id": tc.id.as_str(),
                    "risk_level": risk_level.as_deref(),
                    "success": result.success,
                    "duration_ms": duration_ms,
                    "command_run_id": command_run_id,
                }),
            )
            .await;
            (
                output_str,
                result.success,
                duration_ms,
                if result.success { "completed" } else { "error" },
            )
        }
        Err(e) => {
            let error = e.to_string();

            // Check AskWrite approval flow first (before external access)
            if let Ok(Some(proposal)) = maybe_create_askwrite_proposal(
                tc,
                &args,
                &tool_id,
                workspace_id,
                &error,
                app_handle,
            )
            .await
            {
                let _ = crate::tool_calls::mark_approval_required(
                    &tc_id,
                    &proposal.id,
                    proposal.grant_id.as_deref(),
                    &error,
                )
                .await;
                crate::events::emit_tool_call_lifecycle(
                    "tool_call.blocked",
                    &tc_id,
                    workspace_id,
                    session_id,
                    &tool_id,
                    "approval_required",
                    serde_json::json!({
                        "llm_tool_call_id": tc.id.as_str(),
                        "risk_level": risk_level.as_deref(),
                        "proposal_id": proposal.id.as_str(),
                        "permission_grant_id": proposal.grant_id.as_deref(),
                        "reason": "approval_required",
                        "duration_ms": duration_ms,
                    }),
                )
                .await;
                return (
                    serde_json::json!({
                        "approval_required": true,
                        "proposal_id": proposal.id.as_str(),
                        "message": "工具需要审批，请确认后继续。"
                    })
                    .to_string(),
                    true,
                    duration_ms,
                    "approval_required",
                );
            }

            if let Ok(Some(proposal)) = maybe_create_external_access_proposal(
                tc,
                &args,
                &tool_id,
                workspace_id,
                &error,
                app_handle,
            )
            .await
            {
                let _ = crate::tool_calls::mark_approval_required(
                    &tc_id,
                    &proposal.id,
                    proposal.grant_id.as_deref(),
                    &error,
                )
                .await;
                crate::events::emit_tool_call_lifecycle(
                    "tool_call.blocked",
                    &tc_id,
                    workspace_id,
                    session_id,
                    &tool_id,
                    "approval_required",
                    serde_json::json!({
                        "llm_tool_call_id": tc.id.as_str(),
                        "risk_level": risk_level.as_deref(),
                        "proposal_id": proposal.id.as_str(),
                        "permission_grant_id": proposal.grant_id.as_deref(),
                        "reason": "approval_required",
                        "duration_ms": duration_ms,
                    }),
                )
                .await;
                return (
                    serde_json::json!({
                        "approval_required": true,
                        "proposal_id": proposal.id.as_str(),
                        "message": "已创建审批，等待你确认后再继续这次工作区外访问。"
                    })
                    .to_string(),
                    true,
                    duration_ms,
                    "approval_required",
                );
            }

            let _ = crate::tool_calls::fail(&tc_id, &error).await;
            crate::events::emit_tool_call_lifecycle(
                "tool_call.finished",
                &tc_id,
                workspace_id,
                session_id,
                &tool_id,
                "failed",
                serde_json::json!({
                    "llm_tool_call_id": tc.id.as_str(),
                    "risk_level": risk_level.as_deref(),
                    "success": false,
                    "duration_ms": duration_ms,
                    "error": error,
                }),
            )
            .await;
            (
                serde_json::json!({ "error": error }).to_string(),
                false,
                duration_ms,
                "error",
            )
        }
    }
}

#[cfg(feature = "tauri-events")]
async fn validate_approved_write_scope(
    tool_id: &str,
    args: &serde_json::Value,
    workspace_id: Option<&str>,
    approved_write_scope: Option<&[String]>,
) -> anyhow::Result<()> {
    if !matches!(tool_id, "file.write" | "file.edit") {
        return Ok(());
    }

    let Some(scope) = approved_write_scope else {
        return Ok(());
    };
    if scope.is_empty() {
        return Ok(());
    }

    let Some(target) = args
        .get("path")
        .and_then(|value| value.as_str())
        .or_else(|| args.get("file_path").and_then(|value| value.as_str()))
    else {
        return Ok(());
    };
    let Some(workspace_id) = workspace_id else {
        anyhow::bail!("write scope enforcement requires workspace context");
    };
    let workspace = crate::workspaces::get(workspace_id).await?;
    if path_matches_write_scope(&workspace.root, target, scope) {
        return Ok(());
    }

    anyhow::bail!(
        "write blocked: target '{}' is outside approved write scope ({})",
        target,
        scope.join(", ")
    )
}

fn path_matches_write_scope(workspace_root: &Path, target: &str, scope: &[String]) -> bool {
    let normalized_target = normalize_scope_path(workspace_root, target);
    scope.iter().any(|allowed| {
        let allowed_path = normalize_scope_path(workspace_root, allowed);
        normalized_target == allowed_path || normalized_target.starts_with(&allowed_path)
    })
}

fn normalize_scope_path(workspace_root: &Path, candidate: &str) -> PathBuf {
    let joined = {
        let path = Path::new(candidate);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            workspace_root.join(path)
        }
    };

    joined
        .components()
        .fold(PathBuf::new(), |mut acc, component| {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    acc.pop();
                }
                other => acc.push(other.as_os_str()),
            }
            acc
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CoreConfig;
    #[cfg(feature = "tauri-events")]
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn build_tool_definitions_short_mode_does_not_hard_block_agent_tools() {
        // Short mode no longer hard-blocks agent tools — they are excluded from
        // the *default* set but can be included when the user explicitly asks.
        // This test verifies the function doesn't panic and returns a result.
        let config = CoreConfig::default();
        let defs = build_tool_definitions(
            &config,
            "please start an agent team",
            ChatTaskMode::Short,
            false,
        )
        .await;
        // Result must be a valid (possibly empty) list — no panic.
        let _ = defs.len();
    }

    #[tokio::test]
    async fn build_tool_definitions_long_mode_keeps_agent_tools() {
        let config = CoreConfig::default();
        let defs = build_tool_definitions(
            &config,
            "read the repository, investigate the failure, and propose the next implementation step",
            ChatTaskMode::Long,
            false,
        )
        .await;
        let names: Vec<&str> = defs.iter().map(|def| def.function.name.as_str()).collect();

        assert!(names.contains(&"agent__start"));
        assert!(names.iter().any(|name| name.starts_with("agent__team__")));
        assert!(names
            .iter()
            .any(|name| name.starts_with("agent__mailbox__")));
    }

    #[tokio::test]
    async fn explicit_allowed_tool_ids_override_long_mode_defaults() {
        let config = CoreConfig::default();
        let allowed = vec!["file.read".to_string(), "bash.execute".to_string()];
        let defs = build_tool_definitions_with_allowed_ids(
            &config,
            "use only the explicitly allowed tools",
            ChatTaskMode::Long,
            false,
            Some(&allowed),
        )
        .await;
        let names: Vec<&str> = defs.iter().map(|def| def.function.name.as_str()).collect();

        assert!(names.contains(&"file__read"));
        assert!(names.contains(&"bash__execute"));
        assert!(!names.contains(&"agent__start"));
        assert!(!names.iter().any(|name| name.starts_with("agent__team__")));
    }

    #[test]
    fn progressive_discovery_skips_simple_prompts() {
        assert!(!should_use_progressive_tool_discovery(
            "read the current workspace",
            ChatTaskMode::Short,
            4,
        ));
    }

    #[test]
    fn progressive_discovery_enables_for_complex_connector_prompt() {
        assert!(should_use_progressive_tool_discovery(
            "Check my Lark calendar, then search the doc space, and finally compare the results before suggesting next steps.",
            ChatTaskMode::Short,
            6,
        ));
    }

    #[tokio::test]
    async fn catalog_selection_keeps_tool_search_and_selected_tool() {
        let config = CoreConfig::default();
        let defs = build_tool_definitions_for_catalog_selection(
            &config,
            &["tool.search".to_string(), "demo.echo".to_string()],
            ChatTaskMode::Short,
            false,
        )
        .await
        .unwrap();
        let names: Vec<&str> = defs.iter().map(|def| def.function.name.as_str()).collect();

        assert!(names.contains(&"tool__search"));
        assert!(names.contains(&"demo__echo"));
    }

    #[tokio::test]
    async fn plan_only_filters_write_and_shell_tools() {
        let config = CoreConfig::default();
        let defs = build_tool_definitions(
            &config,
            "edit files and run tests",
            ChatTaskMode::Short,
            true,
        )
        .await;
        let names: Vec<&str> = defs.iter().map(|def| def.function.name.as_str()).collect();

        assert!(!names.contains(&"file__write"));
        assert!(!names.contains(&"file__edit"));
        assert!(!names.contains(&"bash__execute"));
    }

    #[test]
    fn approved_write_scope_allows_only_matching_paths() {
        let root = std::path::Path::new("I:\\personal-agent");
        let scope = vec![
            "crates/conductor-core/src/chat/send_v2.rs".to_string(),
            "apps/desktop/src/windows".to_string(),
        ];

        assert!(path_matches_write_scope(
            root,
            "crates/conductor-core/src/chat/send_v2.rs",
            &scope,
        ));
        assert!(path_matches_write_scope(
            root,
            "apps/desktop/src/windows/ChatTimelinePane.tsx",
            &scope,
        ));
        assert!(!path_matches_write_scope(
            root,
            "crates/conductor-core/src/chat/tools.rs",
            &scope,
        ));
    }

    #[cfg(feature = "tauri-events")]
    #[tokio::test]
    async fn approved_write_scope_blocks_unapproved_file_write() {
        let root = TestRoot::new();
        let workspace =
            crate::workspaces::create_or_attach(root.path(), Some("Scope Test".to_string()), None)
                .await
                .expect("workspace");

        let err = validate_approved_write_scope(
            "file.write",
            &serde_json::json!({
                "path": root.path().join("crates/conductor-core/src/chat/tools.rs").display().to_string()
            }),
            Some(&workspace.id),
            Some(&["crates/conductor-core/src/chat/send_v2.rs".to_string()]),
        )
        .await
        .expect_err("write should be blocked");

        assert!(err.to_string().contains("outside approved write scope"));
    }
}
