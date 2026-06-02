use conductor_core::{
    events, expression,
    goal_orchestrator::{GoalOrchestrator, OrchestratorConfig},
    initiative, pacer, paths, smart_monitor,
    tasks::TaskStatus,
};
use conductor_sense::{focus, idle};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

const PINNED_CHITCHAT_TITLE: &str = "\u{95f2}\u{804a}";
const ENABLE_PENDING_PILEUP_NUDGE: bool = false;

pub fn spawn(app: AppHandle) {
    let pacer_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _handle = pacer::spawn_pacer(tx).await;
        refresh_pet_state(&pacer_app).await;

        let config = conductor_core::config::load()
            .await
            .unwrap_or_else(|_| conductor_core::config::CoreConfig::default());

        while let Some(alert) = rx.recv().await {
            // Temporarily suppress legacy task pile-up nudges while keeping
            // the rest of the reminder pipeline available.
            if !ENABLE_PENDING_PILEUP_NUDGE
                && matches!(alert, pacer::PacerAlert::PendingPileUp { .. })
            {
                continue;
            }

            let quiet_override = crate::QUIET_MODE_UNTIL
                .lock()
                .ok()
                .and_then(|until| until.map(|u| u > std::time::Instant::now()))
                .unwrap_or(false);

            // Quiet mode: suppress all alerts
            if quiet_override || !config.reminders.enabled {
                let _ = pacer_app.emit("pet_state", "quiet");
                continue;
            }

            // Smart Monitor: LLM decides what to do
            let decision = smart_monitor::evaluate(&alert, &config).await;

            // Update pet state
            let _ = pacer_app.emit("pet_state", decision.pet_state.to_str());
            emit_pet_expression(&pacer_app).await;

            // If LLM says notify, send the message
            if decision.notify {
                if let Some(ref message) = decision.message {
                    let _ = append_assistant_message_to_pinned_chat(&pacer_app, message).await;

                    // Emit to task panel
                    let urgency_str = match decision.urgency {
                        smart_monitor::Urgency::Low => "low",
                        smart_monitor::Urgency::Medium => "medium",
                        smart_monitor::Urgency::High => "high",
                    };
                    let _ = pacer_app.emit(
                        "taskpanel_banner",
                        serde_json::json!({
                            "banner": message,
                            "urgency": urgency_str,
                        }),
                    );

                    // System notification for medium/high urgency
                    if decision.urgency != smart_monitor::Urgency::Low {
                        #[allow(unused_imports)]
                        use tauri_plugin_notification::NotificationExt;
                        let _ = pacer_app
                            .notification()
                            .builder()
                            .title("清和提醒")
                            .body(message.clone())
                            .show();
                    }
                }
            }
        }
    });

    // Mood decay timer — runs every 60 seconds
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if let Ok(mut mood) = expression::load_mood().await {
                mood.decay();
                let _ = expression::save_mood(&mood).await;
            }
            // Also apply affection daily decay
            let _ = conductor_core::affection::decrease_over_time().await;
        }
    });

    spawn_sense_watchers(app.clone());
    spawn_task_signal_watcher(app.clone());
    spawn_goal_orchestrator_loop(app.clone());
    spawn_agent_runner();
    spawn_exec_signal_watcher(app);
}

async fn emit_pet_expression(app: &AppHandle) {
    let avatar = conductor_core::avatar::get_current_avatar().await;
    let mood = expression::load_mood().await.unwrap_or_default();
    let affection_state = conductor_core::affection::load().await.unwrap_or_default();
    let zone = mood.zone();

    if let Ok(avatar) = avatar {
        let _ = app.emit(
            "pet_expression",
            serde_json::json!({
                "avatar_id": avatar.avatar_id.as_str(),
                "activity_variant": avatar.activity_variant.as_str(),
                "mood_zone": zone.as_str(),
                "relationship_stage": affection_state.stage.as_str(),
                "pet_state": derive_pet_state(&avatar.activity_variant),
            }),
        );
    }
}

async fn emit_reply_stored(
    app: &AppHandle,
    session_id: &str,
    message_id: &str,
    request_id: Option<&str>,
) {
    let mut payload = serde_json::json!({
        "message_id": message_id,
        "session_id": session_id,
    });
    if let Some(request_id) = request_id {
        payload["request_id"] = serde_json::Value::String(request_id.to_string());
    }
    let _ = app.emit("reply_stored", payload);
}

async fn append_assistant_message_and_notify(
    app: &AppHandle,
    session_id: &str,
    content: &str,
) -> anyhow::Result<()> {
    append_assistant_message_and_notify_with_request_id(app, session_id, content, None).await
}

async fn append_assistant_message_and_notify_with_request_id(
    app: &AppHandle,
    session_id: &str,
    content: &str,
    request_id: Option<&str>,
) -> anyhow::Result<()> {
    let message =
        conductor_core::chat::append_assistant_message_to_session(session_id, content).await?;
    emit_reply_stored(app, session_id, &message.id, request_id).await;
    Ok(())
}

async fn append_assistant_message_to_pinned_chat(
    app: &AppHandle,
    content: &str,
) -> anyhow::Result<()> {
    let session = conductor_core::chat::ensure_chat_session(PINNED_CHITCHAT_TITLE, None).await?;
    append_assistant_message_and_notify(app, &session.id, content).await
}

fn goal_status_projection_text(goal: &conductor_core::goals::GoalRun) -> Option<String> {
    if matches!(
        goal.status.as_str(),
        "awaiting_review" | "accepted" | "rework_required" | "blocked" | "failed"
    ) {
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

#[derive(Debug, Clone)]
struct GoalStatusProjectionPayload {
    content: String,
    persist_as_review_summary: bool,
}

fn goal_task_step_status(task_status: &str) -> String {
    match task_status {
        "accepted" | "review_ready" => "done",
        "cancelled" => "skipped",
        _ => "failed",
    }
    .to_string()
}

fn goal_task_step_detail(task: &conductor_core::goal_tasks::AgentTask) -> Option<String> {
    let mut parts = vec![format!("状态：{}", task.status)];
    if let Some(error) = task
        .error
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(trim_chars(error, 120));
    }
    Some(parts.join(" | "))
}

fn summarize_goal_task_counts(tasks: &[conductor_core::goal_tasks::AgentTask]) -> String {
    let accepted = tasks
        .iter()
        .filter(|task| task.status == "accepted")
        .count();
    let failed = tasks.iter().filter(|task| task.status == "failed").count();
    let blocked = tasks.iter().filter(|task| task.status == "blocked").count();
    let rework = tasks
        .iter()
        .filter(|task| task.status == "rework_required")
        .count();
    let other = tasks
        .len()
        .saturating_sub(accepted + failed + blocked + rework);

    let mut parts = vec![format!("共 {} 个子任务", tasks.len())];
    if accepted > 0 {
        parts.push(format!("accepted {} 个", accepted));
    }
    if failed > 0 {
        parts.push(format!("failed {} 个", failed));
    }
    if blocked > 0 {
        parts.push(format!("blocked {} 个", blocked));
    }
    if rework > 0 {
        parts.push(format!("rework {} 个", rework));
    }
    if other > 0 {
        parts.push(format!("其他 {} 个", other));
    }
    parts.join("，")
}

fn build_goal_cycle_projection_payload(
    goal: &conductor_core::goals::GoalRun,
    cycle: &conductor_core::goals::GoalCycle,
    tasks: &[conductor_core::goal_tasks::AgentTask],
) -> GoalStatusProjectionPayload {
    let steps = tasks
        .iter()
        .take(6)
        .map(|task| conductor_core::chat::CompletionStep {
            label: goal_task_projection_title(&task.title),
            detail: goal_task_step_detail(task),
            status: goal_task_step_status(&task.status),
        })
        .collect::<Vec<_>>();

    let content_blocks = match goal.status.as_str() {
        "awaiting_review" => vec![
            conductor_core::chat::ContentBlock::Completion {
                title: format!("Goal 本轮已生成可审阅结果：{}", goal.title),
                summary: Some(summarize_goal_task_counts(tasks)),
                steps,
                duration_ms: None,
            },
            conductor_core::chat::ContentBlock::Text {
                text: format!("Cycle #{} 结果已写回会话。", cycle.cycle_no),
            },
        ],
        "accepted" => vec![
            conductor_core::chat::ContentBlock::Completion {
                title: format!("Goal 已完成：{}", goal.title),
                summary: Some(summarize_goal_task_counts(tasks)),
                steps,
                duration_ms: None,
            },
            conductor_core::chat::ContentBlock::Text {
                text: format!("Cycle #{} 已收口。", cycle.cycle_no),
            },
        ],
        "rework_required" => vec![
            conductor_core::chat::ContentBlock::Blocked {
                title: format!("Goal 需要继续推进：{}", goal.title),
                reason: summarize_goal_task_counts(tasks),
                action_items: vec![],
            },
            conductor_core::chat::ContentBlock::Text {
                text: format!("Cycle #{} 还需要继续推进。", cycle.cycle_no),
            },
        ],
        "blocked" => vec![
            conductor_core::chat::ContentBlock::Blocked {
                title: format!("Goal 暂时阻塞：{}", goal.title),
                reason: summarize_goal_task_counts(tasks),
                action_items: vec![],
            },
            conductor_core::chat::ContentBlock::Text {
                text: format!("Cycle #{} 暂时阻塞。", cycle.cycle_no),
            },
        ],
        "failed" => vec![
            conductor_core::chat::ContentBlock::Blocked {
                title: format!("Goal 执行失败：{}", goal.title),
                reason: summarize_goal_task_counts(tasks),
                action_items: vec![],
            },
            conductor_core::chat::ContentBlock::Text {
                text: format!(
                    "Cycle #{} 以失败结束。最近一轮结果和错误已保留。",
                    cycle.cycle_no
                ),
            },
        ],
        _ => vec![conductor_core::chat::ContentBlock::Text {
            text: format!("**Goal 进展 | {}**", goal.title),
        }],
    };

    GoalStatusProjectionPayload {
        content: serde_json::to_string(&content_blocks).unwrap_or_else(|_| {
            format!(
                "Goal 状态更新：{}\n\n{}",
                goal.title,
                summarize_goal_task_counts(tasks)
            )
        }),
        persist_as_review_summary: goal.status == "awaiting_review",
    }
}

async fn build_goal_status_projection_payload(
    previous_status: &str,
    goal: &conductor_core::goals::GoalRun,
) -> anyhow::Result<Option<GoalStatusProjectionPayload>> {
    if previous_status == goal.status {
        return Ok(None);
    }

    if let Some(text) = goal_status_projection_text(goal) {
        return Ok(Some(GoalStatusProjectionPayload {
            content: text,
            persist_as_review_summary: false,
        }));
    }

    let Some(cycle_id) = goal.current_cycle_id.as_deref() else {
        return Ok(None);
    };
    let Some(cycle) = conductor_core::goals::get_cycle(cycle_id).await? else {
        return Ok(None);
    };
    let tasks = conductor_core::goal_tasks::list_tasks_by_cycle(cycle_id).await?;
    if tasks.is_empty() {
        return Ok(None);
    }

    Ok(Some(build_goal_cycle_projection_payload(
        goal, &cycle, &tasks,
    )))
}

async fn project_goal_status_change_to_session(
    app: &AppHandle,
    previous_status: &str,
    goal: &conductor_core::goals::GoalRun,
) -> anyhow::Result<()> {
    let Some(payload) = build_goal_status_projection_payload(previous_status, goal).await? else {
        return Ok(());
    };
    let Some(session_id) = conductor_core::chat::find_session_for_goal(&goal.id).await else {
        return Ok(());
    };
    let message =
        conductor_core::chat::append_assistant_message_to_session(&session_id, &payload.content)
            .await?;
    if payload.persist_as_review_summary {
        if let Some(cycle_id) = goal.current_cycle_id.as_deref() {
            let _ = conductor_core::goals::set_cycle_review_summary_ref(
                cycle_id,
                Some(&format!("chat:{}", message.id)),
            )
            .await;
        }
    }
    emit_reply_stored(app, &session_id, &message.id, None).await;
    Ok(())
}

fn derive_pet_state(variant: &conductor_core::avatar::ActivityVariant) -> &'static str {
    use conductor_core::avatar::ActivityVariant;
    match variant {
        ActivityVariant::Idle => "idle",
        ActivityVariant::Thinking
        | ActivityVariant::Reading
        | ActivityVariant::Writing
        | ActivityVariant::ToolCalling => "working",
        ActivityVariant::AgentLeading => "working",
        ActivityVariant::WaitingUser => "idle",
        ActivityVariant::Error => "update",
        ActivityVariant::Done => "update",
    }
}

fn spawn_sense_watchers(app: AppHandle) {
    let app_arc = Arc::new(app.clone());
    let initiative_app = Arc::clone(&app_arc);

    tauri::async_runtime::spawn(async move {
        let mut last_initiative_emit: Option<chrono::DateTime<chrono::Utc>> = None;

        loop {
            tokio::time::sleep(Duration::from_secs(5 * 60)).await;

            let config = conductor_core::config::load()
                .await
                .unwrap_or_else(|_| conductor_core::config::CoreConfig::default());
            if !config.proactive.enabled {
                continue;
            }

            let now = chrono::Utc::now();
            let cooldown = chrono::Duration::minutes(config.proactive.cooldown_minutes as i64);
            if last_initiative_emit
                .as_ref()
                .is_some_and(|last| now - *last < cooldown)
            {
                continue;
            }

            let proposals = conductor_core::initiative::check_for_initiatives();
            if proposals.is_empty() {
                continue;
            }

            let best = &proposals[0];

            let llm_config = &config.llm;
            let request_config = conductor_core::llm::LlmRequestConfig::from_config(llm_config);

            let time_of_day = chrono::Local::now().format("%H:%M").to_string();
            let system_prompt = format!(
                "你是{}，一个桌面助手。风格：{}。当前时间{}。根据以下上下文生成一句简短自然的中文问候（不超过30字），不要加引号。",
                config.persona.name, config.persona.style, time_of_day
            );
            let user_prompt = format!("上下文：{}", best.message);

            let content = conductor_core::llm::call(
                &llm_config.model,
                &system_prompt,
                &user_prompt,
                &request_config,
            )
            .await
            .unwrap_or_else(|_| best.message.clone());

            last_initiative_emit = Some(now);
            let _ = append_assistant_message_to_pinned_chat(&initiative_app, &content).await;
            let _ = initiative_app.emit("pet_state", "update");
            let _ = initiative_app.emit(
                "pet_message",
                json!({
                    "id": format!("initiative-{}", now.timestamp_millis()),
                    "kind": "proactive",
                    "content": content,
                    "action": "open_chat",
                }),
            );
        }
    });

    tauri::async_runtime::spawn(async move {
        let (focus_tx, mut focus_rx) = mpsc::unbounded_channel();
        let (idle_tx, mut idle_rx) = mpsc::unbounded_channel();
        let mut last_prompt_by_process: HashMap<String, chrono::DateTime<chrono::Utc>> =
            HashMap::new();

        if let Err(err) = focus::spawn_focus_watcher(focus_tx) {
            tracing::warn!(error = ?err, "failed to start focus watcher");
        }
        let _idle_handle = idle::spawn_idle_watcher(10 * 60, idle_tx);

        loop {
            tokio::select! {
                Some(event) = focus_rx.recv() => {
                    let payload = json!({
                        "title": event.title,
                        "process_name": event.process_name,
                        "process_path": event.process_path,
                    });
                    let _ = events::append("desktop", "focus_changed", &payload).await;
                    // Don't emit "idle" directly on focus change — let the coordinator
                    // (avatar state machine) degrade to idle when appropriate.
                    // Only record context for the initiative engine.

                    // Feed focus events into initiative engine so activity triggers can fire
                    let activity_type = classify_focus_activity(&event.process_name, &event.title);
                    initiative::update_initiative_context(initiative::PartialContext {
                        workspace_id: None,
                        active_tool: Some(event.process_name.clone()),
                        activity: Some(initiative::ActivityRecord {
                            timestamp: std::time::Instant::now(),
                            activity_type,
                            details: json!({ "process": event.process_name, "title": event.title }),
                        }),
                        touch: true,
                        current_task: None,
                    });

                    maybe_emit_proactive_prompt(&app, &event, &mut last_prompt_by_process).await;
                }
                Some(event) = idle_rx.recv() => {
                    let kind = match &event {
                        idle::IdleEvent::IdleStarted { .. } => "idle_started",
                        idle::IdleEvent::IdleEnded { .. } => "idle_ended",
                    };
                    let payload = serde_json::to_value(event).unwrap_or_else(|_| json!({}));
                    let _ = events::append("desktop", kind, &payload).await;
                    // Don't emit "quiet" directly on idle — let the coordinator
                    // decide when to transition based on combined context.
                }
            }
        }
    });
}

fn classify_focus_activity(process_name: &str, title: &str) -> String {
    let lower_process = process_name.to_ascii_lowercase();
    let lower_title = title.to_ascii_lowercase();
    if lower_process.contains("code")
        || lower_process.contains("cursor")
        || lower_process.contains("trae")
        || lower_process.contains("terminal")
        || lower_process.contains("pwsh")
    {
        "code_edit".to_string()
    } else if lower_title.contains(".doc")
        || lower_title.contains(".md")
        || lower_title.contains("文档")
        || lower_title.contains("document")
    {
        "document_edit".to_string()
    } else {
        "desktop_activity".to_string()
    }
}

async fn maybe_emit_proactive_prompt(
    app: &AppHandle,
    event: &focus::FocusEvent,
    last_prompt_by_process: &mut HashMap<String, chrono::DateTime<chrono::Utc>>,
) {
    let config = conductor_core::config::load()
        .await
        .unwrap_or_else(|_| conductor_core::config::CoreConfig::default());
    if !config.proactive.enabled || !config.proactive.focus_detection {
        return;
    }
    if config.proactive.quiet_when_fullscreen && event.is_maximized {
        return;
    }

    let Some(trigger) = config.proactive.tool_triggers.iter().find(|trigger| {
        trigger.enabled
            && event
                .process_name
                .eq_ignore_ascii_case(trigger.process_name.as_str())
    }) else {
        return;
    };

    let now = chrono::Utc::now();
    let cooldown = chrono::Duration::minutes(config.proactive.cooldown_minutes as i64);
    let process_key = trigger.process_name.to_ascii_lowercase();
    if last_prompt_by_process
        .get(&process_key)
        .is_some_and(|last| now - *last < cooldown)
    {
        return;
    }

    last_prompt_by_process.insert(process_key, now);

    let llm_config = &config.llm;
    let request_config = conductor_core::llm::LlmRequestConfig::from_config(llm_config);

    let time_of_day = chrono::Local::now().format("%H:%M").to_string();
    let task_status = match conductor_core::tasks::load().await {
        Ok(file)
            if file
                .tasks
                .iter()
                .any(|t| t.status == TaskStatus::Pending || t.status == TaskStatus::InProgress) =>
        {
            "有进行中的任务"
        }
        _ => "暂无进行中任务",
    };

    let system_prompt = format!(
        "你是{}，一个桌面助手。风格：{}。用户刚切换到{}（窗口标题：{}）。当前时间{}，{}。根据上下文生成一句简短自然的中文问候（不超过30字），不要加引号。",
        config.persona.name,
        config.persona.style,
        event.process_name,
        event.title,
        time_of_day,
        task_status,
    );
    let user_prompt = format!("默认问候：{}", trigger.prompt);

    let content = conductor_core::llm::call(
        &llm_config.model,
        &system_prompt,
        &user_prompt,
        &request_config,
    )
    .await
    .unwrap_or_else(|_| trigger.prompt.clone());

    let _ = append_assistant_message_to_pinned_chat(app, &content).await;
    let _ = app.emit("pet_state", "update");
    let _ = app.emit(
        "pet_message",
        json!({
            "id": format!("focus-{}-{}", event.process_name, now.timestamp_millis()),
            "kind": "proactive",
            "content": content,
            "action": "open_chat",
            "processName": event.process_name,
            "title": event.title,
        }),
    );
}

async fn refresh_pet_state(app: &AppHandle) {
    static LAST_STATE: std::sync::OnceLock<std::sync::Mutex<Option<String>>> =
        std::sync::OnceLock::new();

    let state = match conductor_core::tasks::load().await {
        Ok(file)
            if file.tasks.iter().any(|task| {
                task.status == TaskStatus::Pending || task.status == TaskStatus::InProgress
            }) =>
        {
            "working"
        }
        _ => "idle",
    };

    let guard = LAST_STATE.get_or_init(|| std::sync::Mutex::new(None));
    let mut last = guard.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_deref() == Some(state) {
        return;
    }
    *last = Some(state.to_string());
    drop(last);

    let _ = app.emit("pet_state", state);
}

fn spawn_task_signal_watcher(app: AppHandle) {
    let state_dir = paths::state();
    std::thread::spawn(move || {
        if let Err(err) = run_watcher(&app, &state_dir) {
            tracing::warn!(error = ?err, "file watcher failed, falling back to polling");
            run_polling_fallback(&app);
        }
    });
}

fn run_watcher(app: &AppHandle, state_dir: &std::path::Path) -> notify::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(state_dir, RecursiveMode::NonRecursive)?;
    tracing::debug!("task signal watcher started on {}", state_dir.display());

    loop {
        match rx.recv() {
            Ok(Ok(_event)) => {
                std::thread::sleep(Duration::from_millis(100));
                while rx.try_recv().is_ok() {}
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = app.emit("tasks_changed", ());
                    let _ = app.emit("agent_runs_changed", ());
                    let _ = app.emit("goals_changed", ());
                    refresh_pet_state(&app).await;
                });
            }
            Ok(Err(err)) => {
                tracing::debug!(error = ?err, "watcher event error");
            }
            Err(_) => {
                break;
            }
        }
    }

    Ok(())
}

fn run_polling_fallback(app: &AppHandle) {
    let signal_path = paths::Paths::task_signal();
    let mut last_mtime = std::fs::metadata(&signal_path)
        .ok()
        .and_then(|m| m.modified().ok());

    loop {
        std::thread::sleep(Duration::from_secs(1));
        let current_mtime = std::fs::metadata(&signal_path)
            .ok()
            .and_then(|m| m.modified().ok());
        if current_mtime != last_mtime {
            last_mtime = current_mtime;
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = app.emit("tasks_changed", ());
                let _ = app.emit("agent_runs_changed", ());
                let _ = app.emit("agent_teams_changed", ());
                let _ = app.emit("goals_changed", ());
                refresh_pet_state(&app).await;
            });
        }
    }
}

fn spawn_goal_orchestrator_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Desktop runner auto-approves plans — the user confirmed intent by
        // switching to goal mode and sending a message. Manual approval gate
        // is only needed for write-heavy or multi-workspace goals (future).
        let orchestrator = GoalOrchestrator::new(OrchestratorConfig {
            require_plan_approval: false,
            ..OrchestratorConfig::default()
        });

        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if let Err(err) = advance_active_goals_once_with_app(Some(&app), &orchestrator).await {
                tracing::warn!(error = ?err, "failed to advance active goals");
            }
        }
    });
}

#[cfg(test)]
async fn advance_active_goals_once(orchestrator: &GoalOrchestrator) -> anyhow::Result<Vec<String>> {
    advance_active_goals_once_with_app(None, orchestrator).await
}

async fn advance_active_goals_once_with_app(
    app: Option<&AppHandle>,
    orchestrator: &GoalOrchestrator,
) -> anyhow::Result<Vec<String>> {
    let goals = conductor_core::goals::list_active_goals(Some(50)).await?;
    let mut advanced = Vec::new();

    for goal in goals {
        if matches!(
            goal.status.as_str(),
            "planning"
                | "awaiting_plan_approval"
                | "running"
                | "awaiting_review"
                | "rework_required"
        ) {
            if let Err(err) = orchestrator.tick_goal(&goal.id).await {
                tracing::warn!(goal_id = %goal.id, error = ?err, "failed to advance goal");
            } else {
                if let (Some(app), Some(updated_goal)) =
                    (app, conductor_core::goals::get_goal(&goal.id).await?)
                {
                    if let Err(err) =
                        project_goal_status_change_to_session(app, &goal.status, &updated_goal)
                            .await
                    {
                        tracing::warn!(
                            goal_id = %goal.id,
                            error = ?err,
                            "failed to project goal status change to session"
                        );
                    }
                    if updated_goal.status != goal.status
                        || updated_goal.updated_at != goal.updated_at
                        || updated_goal.current_cycle_id != goal.current_cycle_id
                    {
                        let _ = app.emit("goals_changed", ());
                        let _ = app.emit("agent_teams_changed", ());
                    }
                }
                advanced.push(goal.id);
            }
        }
    }

    Ok(advanced)
}

/// Spawn the external `conductor agent run` loop as a child process.
///
/// The runner reads the Runtime API snapshot that `start_runtime_api` writes
/// and polls for queued goal tasks to claim and execute via ClaudeP.
/// It is restarted automatically if it exits unexpectedly.
fn spawn_agent_runner() {
    tauri::async_runtime::spawn(async move {
        let conductor_bin = locate_conductor_binary();

        loop {
            let snapshot_path = conductor_core::paths::Paths::runtime_api_state_json();

            // Wait until the runtime snapshot exists (written by start_runtime_api).
            let mut waited = 0u64;
            while !snapshot_path.exists() && waited < 30 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                waited += 1;
            }

            if !snapshot_path.exists() {
                tracing::warn!("runtime API snapshot not found after 30s; skipping agent runner");
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }

            tracing::info!(bin = %conductor_bin, "starting agent runner");

            let status = tokio::process::Command::new(&conductor_bin)
                .args([
                    "agent",
                    "run",
                    "--agent-id",
                    "desktop-runner",
                    "--poll-interval-ms",
                    "2000",
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;

            match status {
                Ok(s) if s.success() => {
                    tracing::info!("agent runner exited cleanly");
                }
                Ok(s) => {
                    tracing::warn!(code = ?s.code(), "agent runner exited with error; restarting in 5s");
                }
                Err(err) => {
                    tracing::warn!(error = ?err, "failed to start agent runner; retrying in 30s");
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    continue;
                }
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

/// Locate the `conductor` binary adjacent to the running desktop executable.
fn locate_conductor_binary() -> String {
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe.with_file_name(if cfg!(windows) {
            "conductor.exe"
        } else {
            "conductor"
        });
        if candidate.exists() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    // Fallback: assume it's on PATH.
    if cfg!(windows) {
        "conductor.exe"
    } else {
        "conductor"
    }
    .to_string()
}

/// Watch the exec-signals directory for tasks that need to be executed
/// via conductor's built-in chat API (full tool set, not claude -p subprocess).
fn spawn_exec_signal_watcher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let signal_dir = conductor_core::paths::Paths::task_execution_signal("_placeholder")
            .parent()
            .unwrap()
            .to_path_buf();

        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let Ok(mut entries) = tokio::fs::read_dir(&signal_dir).await else {
                continue;
            };

            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("exec") {
                    continue;
                }

                let Ok(task_id) = tokio::fs::read_to_string(&path).await else {
                    continue;
                };
                let task_id = task_id.trim().to_string();

                // Remove the signal file before executing to prevent double-execution.
                let _ = tokio::fs::remove_file(&path).await;

                let app = app.clone();
                tokio::spawn(async move {
                    if let Err(e) = execute_goal_task_via_chat(&app, &task_id).await {
                        tracing::warn!(task_id = %task_id, error = ?e, "execute_goal_task_via_chat failed");
                        // Mark task as failed so goal can handle it.
                        let _ =
                            conductor_core::goal_tasks::fail_task(&task_id, &e.to_string()).await;
                        conductor_core::tasks::touch_signal_file().await;
                        let _ = app.emit("tasks_changed", ());
                        let _ = app.emit("agent_runs_changed", ());
                        let _ = app.emit("agent_teams_changed", ());
                        let _ = app.emit("goals_changed", ());
                    }
                });
            }
        }
    });
}

fn goal_task_projection_title(task_title: &str) -> String {
    task_title.chars().take(60).collect::<String>()
}

fn goal_task_status_projection_ascii(task_title: &str, label: &str, detail: &str) -> String {
    let blocks = vec![conductor_core::chat::ContentBlock::Text {
        text: format!(
            "Goal task update\nTask: {}\nStatus: {}\n\n{}",
            goal_task_projection_title(task_title),
            label,
            detail,
        ),
    }];
    serde_json::to_string(&blocks).unwrap_or_else(|_| {
        format!(
            "Goal task update\nTask: {}\nStatus: {}\n\n{}",
            goal_task_projection_title(task_title),
            label,
            detail,
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GoalTaskBlockedState {
    reason: String,
    action_items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GoalTaskWritebackState {
    ReviewReady,
    Blocked(GoalTaskBlockedState),
}

fn trim_chars(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(max_chars).collect();
    format!("{truncated}…")
}

fn summarize_goal_chat_message(message: &conductor_core::chat::ChatMessage) -> Option<String> {
    let text = message
        .to_v2()
        .content_blocks
        .into_iter()
        .filter_map(|block| match block {
            conductor_core::chat::ContentBlock::Text { text } => Some(text),
            conductor_core::chat::ContentBlock::CapabilityRequest { request } => {
                Some(request.reason)
            }
            conductor_core::chat::ContentBlock::Plan { title, .. } => {
                Some(format!("Plan: {title}"))
            }
            conductor_core::chat::ContentBlock::Completion { title, summary, .. } => Some(
                summary
                    .filter(|summary| !summary.trim().is_empty())
                    .unwrap_or(title),
            ),
            conductor_core::chat::ContentBlock::Blocked {
                title,
                reason,
                action_items,
            } => {
                let mut parts = vec![title, reason];
                if !action_items.is_empty() {
                    parts.push(format!("Next: {}", action_items.join("; ")));
                }
                Some(parts.join("\n"))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    if text.is_empty() {
        None
    } else {
        Some(trim_chars(&text, 280))
    }
}

fn summarize_recent_goal_chat(messages: &[conductor_core::chat::ChatMessage]) -> Vec<String> {
    messages
        .iter()
        .rev()
        .filter_map(|message| {
            summarize_goal_chat_message(message).map(|summary| {
                let role = match message.role {
                    conductor_core::chat::ChatRole::User => "User",
                    conductor_core::chat::ChatRole::Assistant => "Assistant",
                };
                format!("{role}: {summary}")
            })
        })
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

async fn build_goal_task_execution_input(
    task: &conductor_core::goal_tasks::AgentTask,
    goal_session_id: Option<&str>,
) -> String {
    let goal = match task.goal_id.as_deref() {
        Some(goal_id) => conductor_core::goals::get_goal(goal_id)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let cycle = match task.cycle_id.as_deref() {
        Some(cycle_id) => conductor_core::goals::get_cycle(cycle_id)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let recent_messages = match goal_session_id {
        Some(session_id) => conductor_core::chat::get_chat_session_messages(session_id, Some(12))
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let recent_chat = summarize_recent_goal_chat(&recent_messages);

    let mut sections = vec![
        "You are executing a Goal task inside a long-running workflow.".to_string(),
        "Treat the Goal objective as the active objective until the user explicitly replaces it."
            .to_string(),
    ];

    if let Some(goal) = goal.as_ref() {
        sections.push(format!("Goal title: {}", goal.title));
        sections.push(format!("Goal objective:\n{}", goal.objective.trim()));
        sections.push(format!("Goal status: {}", goal.status));
    }

    if let Some(cycle) = cycle.as_ref() {
        sections.push(format!(
            "Current cycle: #{} ({})",
            cycle.cycle_no, cycle.status
        ));
    }

    sections.push(format!("Task title: {}", task.title.trim()));
    sections.push(format!("Task instruction:\n{}", task.instruction.trim()));

    if !task.acceptance_json.is_empty() {
        sections.push(format!(
            "Acceptance criteria:\n- {}",
            task.acceptance_json.join("\n- ")
        ));
    }
    if !task.read_scope_json.is_empty() {
        sections.push(format!("Read scope: {}", task.read_scope_json.join(", ")));
    }
    if !task.write_scope_json.is_empty() {
        sections.push(format!("Write scope: {}", task.write_scope_json.join(", ")));
    }
    if !task.allowed_tools_json.is_empty() {
        sections.push(format!(
            "Explicit allowed tools: {}",
            task.allowed_tools_json.join(", ")
        ));
    }
    if !recent_chat.is_empty() {
        sections.push(format!(
            "Recent visible Goal chat context:\n- {}",
            recent_chat.join("\n- ")
        ));
    }

    sections.push(
        "Execution requirements:\n- Produce a reviewable result in this turn.\n- If a tool call is blocked or needs approval, explain the blocker clearly and what must happen next.\n- End with a concise conclusion, artifacts or file changes, and the next recommended step."
            .to_string(),
    );

    sections.join("\n\n")
}

fn normalize_goal_task_block_reason(reason: &str) -> String {
    let trimmed = reason.trim();
    let trimmed = trimmed
        .strip_prefix("approval_required:")
        .map(str::trim)
        .unwrap_or(trimmed);
    if trimmed.is_empty() {
        "当前子任务遇到阻塞，暂时无法继续。".to_string()
    } else {
        trimmed.to_string()
    }
}

fn derive_goal_task_writeback_state(
    reply: &conductor_core::chat::ChatMessage,
    tool_calls: &[conductor_core::tool_calls::ToolCall],
) -> GoalTaskWritebackState {
    for block in reply.to_v2().content_blocks {
        if let conductor_core::chat::ContentBlock::Blocked {
            reason,
            action_items,
            ..
        } = block
        {
            return GoalTaskWritebackState::Blocked(GoalTaskBlockedState {
                reason: normalize_goal_task_block_reason(&reason),
                action_items,
            });
        }
    }

    if let Some(tool_call) = tool_calls
        .iter()
        .find(|tool_call| tool_call.status == "approval_required")
    {
        return GoalTaskWritebackState::Blocked(GoalTaskBlockedState {
            reason: normalize_goal_task_block_reason(
                tool_call.error.as_deref().unwrap_or("工具调用需要审批。"),
            ),
            action_items: vec!["处理审批请求。".to_string()],
        });
    }

    if let Some(tool_call) = tool_calls
        .iter()
        .find(|tool_call| tool_call.status == "blocked")
    {
        return GoalTaskWritebackState::Blocked(GoalTaskBlockedState {
            reason: normalize_goal_task_block_reason(
                tool_call.error.as_deref().unwrap_or("工具调用被阻塞。"),
            ),
            action_items: vec!["补充缺失条件或调整范围。".to_string()],
        });
    }

    GoalTaskWritebackState::ReviewReady
}

fn prepend_goal_task_blocked_projection(
    task_title: &str,
    blocked: &GoalTaskBlockedState,
    content: &str,
) -> String {
    let mut blocks = serde_json::from_str::<Vec<conductor_core::chat::ContentBlock>>(content)
        .unwrap_or_else(|_| {
            vec![conductor_core::chat::ContentBlock::Text {
                text: content.to_string(),
            }]
        });

    if blocks
        .iter()
        .any(|block| matches!(block, conductor_core::chat::ContentBlock::Blocked { .. }))
    {
        return content.to_string();
    }

    blocks.insert(
        0,
        conductor_core::chat::ContentBlock::Blocked {
            title: format!("子任务需要处理：{}", goal_task_projection_title(task_title)),
            reason: blocked.reason.clone(),
            action_items: blocked.action_items.clone(),
        },
    );

    serde_json::to_string(&blocks).unwrap_or_else(|_| content.to_string())
}

fn goal_task_projection_placeholder_content(request_id: &str, task_title: &str) -> String {
    serde_json::to_string(&vec![
        conductor_core::chat::ContentBlock::RuntimeProjection {
            request_id: request_id.to_string(),
            label: goal_task_projection_title(task_title),
        },
    ])
    .unwrap_or_else(|_| format!("Goal execution started for {}.", task_title))
}

async fn emit_goal_task_projection_started(
    app: &AppHandle,
    session_id: &str,
    request_id: &str,
    task_title: &str,
) -> Option<String> {
    let placeholder = conductor_core::chat::append_assistant_message_to_session(
        session_id,
        &goal_task_projection_placeholder_content(request_id, task_title),
    )
    .await
    .ok();
    if let Some(message) = placeholder.as_ref() {
        emit_reply_stored(app, session_id, &message.id, Some(request_id)).await;
    }
    let _ = app.emit(
        "thinking-update",
        conductor_core::chat::ThinkingUpdateEvent {
            session_id: Some(session_id.to_string()),
            request_id: request_id.to_string(),
            phase: "planning".to_string(),
            message: format!(
                "Background goal execution started for {}.",
                goal_task_projection_title(task_title)
            ),
            turn: 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );
    placeholder.map(|message| message.id)
}

/// Execute a goal task through conductor's built-in chat API.
///
/// Runs in a DEDICATED session (not the user's chat session) to avoid
/// interfering with the conversation. The result is projected back as
/// an assistant message in the linked goal session.
async fn execute_goal_task_via_chat(app: &AppHandle, task_id: &str) -> anyhow::Result<()> {
    use conductor_core::{
        chat::{ChatCapability, ChatTaskMode},
        goal_tasks,
    };

    let task = goal_tasks::get_task(task_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("task not found: {task_id}"))?;
    let goal_session_id = if let Some(goal_id) = &task.goal_id {
        conductor_core::chat::find_session_for_goal(goal_id).await
    } else {
        None
    };
    let projection_request_id = format!(
        "goal-task-{task_id}-{}",
        chrono::Utc::now().timestamp_millis()
    );
    let mut projection_message_id: Option<String> = None;

    if let Some(goal_session_id) = goal_session_id.as_deref() {
        projection_message_id = emit_goal_task_projection_started(
            app,
            goal_session_id,
            &projection_request_id,
            &task.title,
        )
        .await;
    }

    // Create a throwaway session for this task execution.
    // We must NOT use the user's goal session — that would inject a user
    // message mid-conversation and race with the chat LLM loop.
    // Mark it as 'goal' kind so send_v2 skips the capability_request upgrade gate.
    let execution_input = build_goal_task_execution_input(&task, goal_session_id.as_deref()).await;
    let exec_session = conductor_core::chat::create_chat_session(
        Some(format!("goal-task-exec:{task_id}")),
        Some(task.workspace_id.clone()),
    )
    .await?;
    conductor_core::chat::set_chat_session_kind(&exec_session.id, "goal", None).await?;

    let reply_result = conductor_core::chat::send_message_v2_with_session_projection(
        execution_input,
        app,
        Some(exec_session.id.clone()),
        goal_session_id.clone(),
        ChatTaskMode::Long,
        ChatCapability::AskWrite,
        false,
        Some(task.write_scope_json.clone()),
        Some(task.allowed_tools_json.clone()),
        Some(projection_request_id.clone()),
    )
    .await;

    if let Err(err) = conductor_core::chat::archive_chat_session(&exec_session.id).await {
        tracing::warn!(
            session_id = %exec_session.id,
            error = ?err,
            "failed to archive goal execution session"
        );
    }

    let reply = match reply_result {
        Ok(reply) => reply,
        Err(err) => {
            if let Some(goal_session_id) = goal_session_id.as_deref() {
                let failure_projection = goal_task_status_projection_ascii(
                    &task.title,
                    "Failed",
                    &format!("Background execution failed: {err:#}"),
                );
                if let Some(message_id) = projection_message_id.as_deref() {
                    let _ = conductor_core::chat::update_message_content(
                        message_id,
                        &failure_projection,
                    )
                    .await;
                    emit_reply_stored(
                        app,
                        goal_session_id,
                        message_id,
                        Some(&projection_request_id),
                    )
                    .await;
                } else {
                    let _ = append_assistant_message_and_notify_with_request_id(
                        app,
                        goal_session_id,
                        &failure_projection,
                        Some(&projection_request_id),
                    )
                    .await;
                }
            }
            return Err(err);
        }
    };

    // First commit the task result against the execution-session reply so
    // task state cannot lag behind a user-visible projected message.
    let tool_calls = conductor_core::tool_calls::list(conductor_core::tool_calls::ToolCallFilter {
        session_id: Some(exec_session.id.clone()),
        limit: Some(200),
        ..Default::default()
    })
    .await
    .unwrap_or_default();
    let writeback_result: anyhow::Result<()> = async {
        let writeback_state = derive_goal_task_writeback_state(&reply.message, &tool_calls);
        let result_ref = format!("chat:{}", reply.message.id);
        let projected_content = match &writeback_state {
            GoalTaskWritebackState::ReviewReady => {
                conductor_core::goal_tasks::set_task_result_ref_review_ready(task_id, &result_ref)
                    .await?;
                reply.message.content.clone()
            }
            GoalTaskWritebackState::Blocked(blocked) => {
                conductor_core::goal_tasks::set_task_result_ref_blocked(
                    task_id,
                    &result_ref,
                    &blocked.reason,
                )
                .await?;
                prepend_goal_task_blocked_projection(&task.title, blocked, &reply.message.content)
            }
        };

        if let Some(goal_session_id) = goal_session_id.as_deref() {
            let projected_id = if let Some(message_id) = projection_message_id.as_deref() {
                conductor_core::chat::update_message_content(message_id, &projected_content)
                    .await?;
                message_id.to_string()
            } else {
                conductor_core::chat::append_assistant_message_to_session(
                    goal_session_id,
                    &projected_content,
                )
                .await?
                .id
            };
            emit_reply_stored(
                app,
                goal_session_id,
                &projected_id,
                Some(&projection_request_id),
            )
            .await;
        }

        Ok(())
    }
    .await;

    if let Err(err) = writeback_result {
        if let Some(goal_session_id) = goal_session_id.as_deref() {
            let failure_projection = goal_task_status_projection_ascii(
                &task.title,
                "Failed",
                &format!("Failed to write back the projected result: {err:#}"),
            );
            if let Some(message_id) = projection_message_id.as_deref() {
                let _ =
                    conductor_core::chat::update_message_content(message_id, &failure_projection)
                        .await;
                emit_reply_stored(
                    app,
                    goal_session_id,
                    message_id,
                    Some(&projection_request_id),
                )
                .await;
            } else {
                let _ = append_assistant_message_and_notify_with_request_id(
                    app,
                    goal_session_id,
                    &failure_projection,
                    Some(&projection_request_id),
                )
                .await;
            }
        }
        return Err(err);
    }

    conductor_core::tasks::touch_signal_file().await;
    let _ = app.emit("tasks_changed", ());
    let _ = app.emit("agent_runs_changed", ());
    let _ = app.emit("agent_teams_changed", ());
    let _ = app.emit("goals_changed", ());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        advance_active_goals_once, build_goal_cycle_projection_payload,
        derive_goal_task_writeback_state, goal_task_status_projection_ascii,
        prepend_goal_task_blocked_projection, GoalTaskBlockedState, GoalTaskWritebackState,
    };
    use chrono::Utc;
    use conductor_core::{
        chat::{ChatMessage, ChatRole, ContentBlock},
        goal_orchestrator::{GoalOrchestrator, OrchestratorConfig},
        goal_tasks, goals,
        tool_calls::ToolCall,
    };

    fn make_goal(status: &str) -> goals::GoalRun {
        let now = Utc::now();
        goals::GoalRun {
            id: "goal-1".to_string(),
            workspace_id: "ws-1".to_string(),
            title: "Improve chat and goal chain".to_string(),
            objective: "Produce a reliable reviewable result.".to_string(),
            status: status.to_string(),
            priority: "normal".to_string(),
            owner: "test".to_string(),
            budget_json: None,
            policy_json: None,
            current_cycle_id: Some("cycle-1".to_string()),
            created_at: now,
            updated_at: now,
            finished_at: None,
            metadata_json: None,
        }
    }

    fn make_cycle(status: &str) -> goals::GoalCycle {
        let now = Utc::now();
        goals::GoalCycle {
            id: "cycle-1".to_string(),
            goal_id: "goal-1".to_string(),
            cycle_no: 2,
            status: status.to_string(),
            observe_snapshot_ref: None,
            orientation_json: None,
            dispatch_plan_id: None,
            review_summary_ref: None,
            started_at: now,
            updated_at: now,
            finished_at: None,
        }
    }

    fn make_task(title: &str, status: &str) -> goal_tasks::AgentTask {
        let now = Utc::now();
        goal_tasks::AgentTask {
            id: format!("task-{title}"),
            workspace_id: "ws-1".to_string(),
            goal_id: Some("goal-1".to_string()),
            cycle_id: Some("cycle-1".to_string()),
            parent_task_id: None,
            title: title.to_string(),
            instruction: "do work".to_string(),
            status: status.to_string(),
            agent_kind: "backend-agent".to_string(),
            assigned_agent_id: None,
            claimed_by: None,
            write_scope_json: vec![],
            read_scope_json: vec![],
            allowed_tools_json: vec![],
            dependencies_json: vec![],
            acceptance_json: vec![],
            result_ref: Some(format!("chat:{title}")),
            error: None,
            created_at: now,
            updated_at: now,
            claimed_at: None,
            finished_at: None,
        }
    }

    #[tokio::test]
    async fn desktop_worker_advances_started_goals_without_manual_tick_calls() {
        let temp = tempfile::tempdir().expect("temp root");
        std::env::set_var("CONDUCTOR_ROOT", temp.path());

        let goal = goals::create_goal(
            "ws-worker",
            "Worker Loop Goal",
            "Prove desktop worker drives the goal loop",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");

        let orchestrator = GoalOrchestrator::new(OrchestratorConfig {
            workspace_id: "ws-worker".to_string(),
            ..Default::default()
        });
        orchestrator.start(&goal.id).await.expect("start goal");

        let mut goal_was_advanced = false;
        for _ in 0..4 {
            let advanced = advance_active_goals_once(&orchestrator)
                .await
                .expect("worker goal advance");
            goal_was_advanced |= advanced.iter().any(|goal_id| goal_id == &goal.id);
        }

        assert!(
            goal_was_advanced,
            "worker loop should advance the active goal at least once"
        );

        let current_goal = goals::get_goal(&goal.id)
            .await
            .expect("get goal")
            .expect("goal exists");
        let cycle_id = current_goal
            .current_cycle_id
            .clone()
            .expect("current cycle id");
        let cycle = goals::get_cycle(&cycle_id)
            .await
            .expect("get cycle")
            .expect("cycle exists");
        let tasks = goal_tasks::list_tasks_by_cycle(&cycle_id)
            .await
            .expect("list cycle tasks");

        assert_eq!(current_goal.status, "running");
        assert_eq!(cycle.status, "executing");
        if tasks.is_empty() {
            assert!(
                conductor_core::agent_teams::get_team(&format!("team-{cycle_id}"))
                    .await
                    .is_err(),
                "execution team should only exist once tasks were dispatched"
            );
        } else {
            conductor_core::agent_teams::get_team(&format!("team-{cycle_id}"))
                .await
                .expect("team exists when execution tasks were dispatched");
        }
    }

    #[test]
    fn goal_task_status_projection_is_structured_text() {
        let projection = goal_task_status_projection_ascii(
            "execute_goal",
            "Started",
            "Background execution is running.",
        );
        let blocks: Vec<ContentBlock> =
            serde_json::from_str(&projection).expect("status projection content blocks");

        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            &blocks[0],
            ContentBlock::Text { text } if text.contains("Background execution is running.")
        ));
    }

    #[test]
    fn goal_task_writeback_blocks_when_tool_needs_approval() {
        let reply = ChatMessage {
            id: "msg-1".to_string(),
            role: ChatRole::Assistant,
            content: serde_json::to_string(&vec![ContentBlock::Completion {
                title: "已生成可审阅的阶段结果".to_string(),
                summary: Some("需要审批后才能继续。".to_string()),
                steps: vec![],
                duration_ms: None,
            }])
            .expect("serialize"),
            created_at: Utc::now(),
            seq: 1,
            tool_calls: None,
        };
        let tool_calls = vec![ToolCall {
            id: "tc-1".to_string(),
            session_id: Some("session-1".to_string()),
            workspace_id: Some("ws-1".to_string()),
            llm_tool_call_id: Some("llm-1".to_string()),
            tool_id: "file.write".to_string(),
            input_json: "{}".to_string(),
            output_json: None,
            status: "approval_required".to_string(),
            error: Some("approval_required: file.write needs approval".to_string()),
            started_at: Utc::now(),
            completed_at: None,
            duration_ms: None,
            agent_run_id: None,
            risk_level: None,
            proposal_id: None,
            permission_grant_id: None,
            command_run_id: None,
        }];

        let state = derive_goal_task_writeback_state(&reply, &tool_calls);
        assert!(matches!(
            state,
            GoalTaskWritebackState::Blocked(GoalTaskBlockedState { .. })
        ));
    }

    #[test]
    fn blocked_projection_prepends_blocked_card() {
        let content = serde_json::to_string(&vec![ContentBlock::Text {
            text: "阶段结论".to_string(),
        }])
        .expect("serialize");
        let projection = prepend_goal_task_blocked_projection(
            "execute_goal",
            &GoalTaskBlockedState {
                reason: "需要审批".to_string(),
                action_items: vec!["处理审批后继续".to_string()],
            },
            &content,
        );

        let blocks: Vec<ContentBlock> =
            serde_json::from_str(&projection).expect("blocked projection blocks");
        assert!(matches!(
            &blocks[0],
            ContentBlock::Blocked { reason, .. } if reason == "需要审批"
        ));
    }

    #[test]
    fn goal_cycle_projection_builds_reviewable_completion_summary() {
        let payload = build_goal_cycle_projection_payload(
            &make_goal("awaiting_review"),
            &make_cycle("reviewing"),
            &[
                make_task("inspect repo", "accepted"),
                make_task("run checks", "accepted"),
            ],
        );
        let blocks: Vec<ContentBlock> =
            serde_json::from_str(&payload.content).expect("goal review payload");

        assert!(payload.persist_as_review_summary);
        assert!(matches!(
            &blocks[0],
            ContentBlock::Completion { title, .. } if title.contains("可审阅结果")
        ));
    }

    #[test]
    fn goal_cycle_projection_builds_final_completion_for_accepted_goal() {
        let payload = build_goal_cycle_projection_payload(
            &make_goal("accepted"),
            &make_cycle("completed"),
            &[
                make_task("inspect repo", "accepted"),
                make_task("run checks", "accepted"),
            ],
        );
        let blocks: Vec<ContentBlock> =
            serde_json::from_str(&payload.content).expect("goal accepted payload");

        assert!(!payload.persist_as_review_summary);
        assert!(matches!(
            &blocks[0],
            ContentBlock::Completion { title, .. } if title.contains("Goal 已完成")
        ));
    }
}
