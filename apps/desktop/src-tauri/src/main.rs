mod commands;
#[cfg(debug_assertions)]
mod dev_server;
mod error;
mod tray;
mod window_state;
mod worker;

use chrono::Utc;
use conductor_core::{
    connectors::register_builtin_connectors,
    paths::Paths,
    runtime_api::{generate_runtime_token, RuntimeApiServer},
};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Mutex};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_TRANSPARENT,
};

#[tauri::command]
fn set_pet_click_through(window: tauri::WebviewWindow, through: bool) -> Result<(), String> {
    #[cfg(windows)]
    unsafe {
        let hwnd = window.hwnd().map_err(|err| err.to_string())?;
        let mut style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if through {
            style |= (WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0) as isize;
        } else {
            style &= !(WS_EX_TRANSPARENT.0 as isize);
            style |= WS_EX_LAYERED.0 as isize;
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
    }
    #[cfg(not(windows))]
    let _ = (window, through);
    Ok(())
}

#[tauri::command]
fn set_always_on_top(window: tauri::WebviewWindow, always_on_top: bool) -> Result<(), String> {
    window
        .set_always_on_top(always_on_top)
        .map_err(|err| err.to_string())
}

static QUIET_MODE_UNTIL: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

#[derive(Default)]
struct RuntimeApiState(Mutex<Option<RuntimeApiServer>>);

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeApiSnapshot {
    bind: String,
    port: u16,
    base_url: String,
    token: String,
    running: bool,
    updated_at: String,
}

#[tauri::command]
async fn quiet_for_minutes(app: AppHandle, minutes: u32) -> Result<(), String> {
    conductor_core::events::append(
        "desktop",
        "quiet_requested",
        &serde_json::json!({ "minutes": minutes }),
    )
    .await
    .map_err(|err| err.to_string())?;

    let until = std::time::Instant::now() + std::time::Duration::from_secs(minutes as u64 * 60);
    *QUIET_MODE_UNTIL.lock().unwrap() = Some(until);

    let _ = app.emit("pet_state", "quiet");

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(minutes as u64 * 60)).await;
        let mut lock = QUIET_MODE_UNTIL.lock().unwrap();
        if *lock == Some(until) {
            *lock = None;
            let _ = app.emit("pet_state", "idle");
        }
    });

    Ok(())
}

async fn persist_runtime_api_snapshot(snapshot: &RuntimeApiSnapshot) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(conductor_core::paths::state()).await?;
    tokio::fs::write(
        Paths::runtime_api_state_json(),
        serde_json::to_vec_pretty(snapshot)?,
    )
    .await?;
    tokio::fs::write(Paths::runtime_token_txt(), format!("{}\n", snapshot.token)).await?;
    Ok(())
}

async fn initialize_user_state_from_template(app: &AppHandle) -> anyhow::Result<()> {
    let state_dir = conductor_core::paths::state();
    tokio::fs::create_dir_all(&state_dir).await?;

    let resource_dir = match app.path().resource_dir() {
        Ok(path) => path,
        Err(err) => {
            tracing::warn!(error = %err, "failed to resolve bundled resource directory");
            return Ok(());
        }
    };
    let template_dir = resource_dir.join("state-template");
    if !tokio::fs::try_exists(&template_dir).await.unwrap_or(false) {
        tracing::warn!(
            path = %template_dir.display(),
            "bundled state template is not present"
        );
        return Ok(());
    }

    for file_name in [
        "conductor.sqlite",
        "config.json",
        "desktop.json",
        "tasks.json",
        "tasks.md",
    ] {
        copy_template_file_if_missing(&template_dir, &state_dir, file_name).await?;
    }

    tokio::fs::create_dir_all(Paths::summaries_dir()).await?;
    Ok(())
}

async fn copy_template_file_if_missing(
    template_dir: &Path,
    state_dir: &Path,
    file_name: &str,
) -> anyhow::Result<()> {
    let destination = state_dir.join(file_name);
    if tokio::fs::try_exists(&destination).await.unwrap_or(false) {
        return Ok(());
    }

    let source = template_dir.join(file_name);
    if !tokio::fs::try_exists(&source).await.unwrap_or(false) {
        tracing::warn!(
            path = %source.display(),
            "state template file is not present"
        );
        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::copy(&source, &destination).await?;
    tracing::info!(
        source = %source.display(),
        destination = %destination.display(),
        "initialized state file from bundled template"
    );
    Ok(())
}

async fn mark_runtime_api_stopped() -> anyhow::Result<()> {
    let state_path = Paths::runtime_api_state_json();
    let token_path = Paths::runtime_token_txt();
    if !tokio::fs::try_exists(&state_path).await.unwrap_or(false) {
        return Ok(());
    }

    let existing = tokio::fs::read(&state_path).await?;
    let mut snapshot: RuntimeApiSnapshot = serde_json::from_slice(&existing)?;
    snapshot.running = false;
    snapshot.updated_at = Utc::now().to_rfc3339();
    tokio::fs::write(state_path, serde_json::to_vec_pretty(&snapshot)?).await?;
    if !tokio::fs::try_exists(&token_path).await.unwrap_or(false) {
        tokio::fs::write(token_path, format!("{}\n", snapshot.token)).await?;
    }
    Ok(())
}

async fn start_runtime_api_server() -> anyhow::Result<(RuntimeApiServer, RuntimeApiSnapshot)> {
    let token = generate_runtime_token();
    let mut server = RuntimeApiServer::new("127.0.0.1", 0, &token);
    server.start().await?;
    let addr = server
        .local_addr()
        .ok_or_else(|| anyhow::anyhow!("runtime API server did not report a local address"))?;

    let snapshot = RuntimeApiSnapshot {
        bind: addr.ip().to_string(),
        port: addr.port(),
        base_url: format!("http://{}", addr),
        token,
        running: true,
        updated_at: Utc::now().to_rfc3339(),
    };

    Ok((server, snapshot))
}

async fn start_runtime_api(app: &AppHandle) -> anyhow::Result<()> {
    let (server, snapshot) = start_runtime_api_server().await?;
    persist_runtime_api_snapshot(&snapshot).await?;

    let state = app.state::<RuntimeApiState>();
    let mut guard = state.0.lock().unwrap();
    *guard = Some(server);
    Ok(())
}

fn stop_runtime_api(app: &AppHandle) {
    if let Some(state) = app.try_state::<RuntimeApiState>() {
        let mut guard = state.0.lock().unwrap();
        if let Some(server) = guard.as_mut() {
            server.stop();
        }
        *guard = None;
    }

    tauri::async_runtime::spawn(async {
        if let Err(err) = mark_runtime_api_stopped().await {
            tracing::warn!(error = %err, "failed to persist stopped runtime API state");
        }
    });
}

fn install_auxiliary_window_close_handlers(app: &AppHandle) {
    for label in ["tasks", "chat", "settings", "workbench"] {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
        let window_handle = window.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window_handle.hide();
            }
        });
    }
}

fn main() {
    #[cfg(debug_assertions)]
    let _dev_server = dev_server::ensure_vite_dev_server();

    let app = tauri::Builder::default()
        .manage(RuntimeApiState::default())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            conductor_core::tools::register_builtin_tools();
            tauri::async_runtime::block_on(initialize_user_state_from_template(app.handle()))?;
            tauri::async_runtime::block_on(register_builtin_connectors())?;
            tauri::async_runtime::block_on(start_runtime_api(app.handle()))?;
            tauri::async_runtime::spawn(async {
                tracing::info!("Syncing MCP tools...");
                match conductor_core::mcp::sync_mcp_tools().await {
                    Ok(()) => tracing::info!("MCP tools synced successfully"),
                    Err(e) => tracing::warn!("MCP tools sync failed: {}", e),
                }
            });
            tray::build_tray(app.handle())?;
            worker::spawn(app.handle().clone());
            window_state::apply_pet_window_state(app.handle());
            install_auxiliary_window_close_handlers(app.handle());
            if let Some(pet) = app.get_webview_window("pet") {
                let _ = set_pet_click_through(pet, false);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tasks,
            commands::show_task,
            commands::get_task_activity_stats,
            commands::create_chat_session,
            commands::ensure_chat_session,
            commands::list_chat_sessions,
            commands::get_chat_session_messages,
            commands::get_chat_session_messages_v2,
            commands::get_chat_turn_events,
            commands::get_chat_message_projections,
            commands::rename_chat_session,
            commands::archive_chat_session,
            commands::update_chat_session_workspace,
            commands::set_chat_session_kind,
            commands::list_agent_tasks,
            commands::list_tasks_by_budget,
            commands::migrate_legacy_tasks_to_tasklist,
            commands::list_agent_runs,
            commands::read_agent_run_output,
            commands::stop_agent_run,
            commands::get_tool_call,
            commands::list_tool_calls,
            commands::get_command_run,
            commands::list_command_runs,
            commands::list_agent_teams,
            commands::create_agent_team,
            commands::add_agent_team_member,
            commands::get_agent_team_snapshot,
            commands::submit_agent_team_plan_verdict,
            commands::submit_agent_team_review_verdict,
            commands::send_agent_mailbox_message,
            commands::list_agent_mailbox,
            commands::mark_agent_mailbox_read,
            commands::pass_task,
            commands::skip_task,
            commands::reject_task,
            commands::list_proposals,
            commands::approve_proposal,
            commands::execute_proposal,
            commands::reject_proposal,
            commands::get_settings,
            commands::get_workspace_status,
            commands::list_workspaces,
            commands::attach_workspace,
            commands::update_settings,
            commands::save_settings,
            commands::test_llm_connection,
            commands::chat_history,
            commands::send_chat_message_v2,
            commands::list_chat_messages,
            commands::get_current_avatar,
            commands::set_pet_avatar,
            commands::set_activity_variant,
            commands::set_main_avatar_manual,
            commands::set_sub_avatar_manual,
            commands::toggle_avatar_lock,
            commands::get_foreground_app,
            commands::show_pet_message,
            commands::get_affection,
            commands::add_affection,
            commands::interact_affection,
            commands::decrease_affection_over_time,
            commands::get_expression_state,
            commands::get_mood_state,
            commands::get_emotion_history,
            commands::get_affection_history,
            commands::get_emotion_summary,
            commands::get_relationship_stats,
            commands::memory_set,
            commands::memory_get,
            commands::memory_get_by_category,
            commands::memory_save_preferences,
            commands::memory_load_preferences,
            commands::memory_add_conversation,
            commands::memory_get_recent_conversations,
            commands::memory_search_conversations,
            commands::memory_list,
            commands::memory_update_status,
            commands::memory_forget,
            commands::memory_rebuild_embeddings,
            commands::memory_update,
            commands::memory_delete,
            commands::memory_archive,
            commands::get_music_state,
            commands::check_initiative,
            commands::record_activity,
            commands::list_scenes,
            commands::switch_scene,
            commands::get_current_scene,
            commands::get_current_persona,
            commands::list_personas,
            commands::set_current_persona,
            commands::generate_prompt,
            commands::get_image_prompt,
            commands::onboarding_status,
            commands::list_skills,
            commands::import_skills,
            commands::save_skills,
            commands::import_skill_markdown,
            commands::list_skill_packages,
            commands::update_skill_enabled,
            commands::delete_skill_package,
            commands::list_connectors,
            commands::list_goals,
            commands::create_goal,
            commands::append_goal_user_message,
            commands::update_goal_status,
            commands::update_goal_objective,
            commands::start_goal,
            commands::pause_goal,
            commands::resume_goal,
            commands::cancel_goal,
            commands::approve_goal_plan,
            commands::reject_goal_plan,
            commands::submit_goal_review_verdict,
            commands::get_goal_cycles,
            commands::list_active_heartbeats,
            commands::list_goal_tasks,
            commands::list_goal_events,
            commands::write_workspace_projection,
            commands::list_workspace_activity_projection,
            commands::list_llm_profiles,
            commands::create_llm_profile,
            commands::delete_llm_profile,
            commands::get_runtime_api_info,
            commands::list_goal_hints,
            commands::create_goal_hint,
            commands::dismiss_goal_hint,
            commands::get_goal_graph,
            window_state::load_pet_window_state,
            window_state::save_pet_window_state,
            set_pet_click_through,
            set_always_on_top,
            quiet_for_minutes
        ])
        .build(tauri::generate_context!())
        .expect("error while building conductor desktop");

    app.run(|app_handle, event| {
        if matches!(
            event,
            tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
        ) {
            stop_runtime_api(app_handle);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;
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
    async fn persist_runtime_api_snapshot_writes_state_and_token_files() {
        let _root = TestRoot::new();
        let snapshot = RuntimeApiSnapshot {
            bind: "127.0.0.1".to_string(),
            port: 4011,
            base_url: "http://127.0.0.1:4011".to_string(),
            token: "runtime-token-1".to_string(),
            running: true,
            updated_at: "2026-05-31T00:00:00Z".to_string(),
        };

        persist_runtime_api_snapshot(&snapshot)
            .await
            .expect("persist snapshot");

        let saved_state = tokio::fs::read(Paths::runtime_api_state_json())
            .await
            .expect("read runtime state");
        let parsed: RuntimeApiSnapshot =
            serde_json::from_slice(&saved_state).expect("parse runtime state");
        assert_eq!(parsed.base_url, snapshot.base_url);
        assert!(parsed.running);

        let token = tokio::fs::read_to_string(Paths::runtime_token_txt())
            .await
            .expect("read runtime token");
        assert_eq!(token.trim(), "runtime-token-1");
    }

    #[tokio::test]
    async fn mark_runtime_api_stopped_flips_running_flag() {
        let _root = TestRoot::new();
        persist_runtime_api_snapshot(&RuntimeApiSnapshot {
            bind: "127.0.0.1".to_string(),
            port: 4012,
            base_url: "http://127.0.0.1:4012".to_string(),
            token: "runtime-token-2".to_string(),
            running: true,
            updated_at: "2026-05-31T00:00:00Z".to_string(),
        })
        .await
        .expect("persist snapshot");

        mark_runtime_api_stopped()
            .await
            .expect("mark runtime stopped");

        let saved_state = tokio::fs::read(Paths::runtime_api_state_json())
            .await
            .expect("read runtime state");
        let parsed: RuntimeApiSnapshot =
            serde_json::from_slice(&saved_state).expect("parse runtime state");
        assert!(!parsed.running);

        let token = tokio::fs::read_to_string(Paths::runtime_token_txt())
            .await
            .expect("read runtime token");
        assert_eq!(token.trim(), "runtime-token-2");
    }

    #[tokio::test]
    async fn start_runtime_api_server_serves_health_and_uses_auth_token() {
        let _root = TestRoot::new();
        let (mut server, snapshot) = start_runtime_api_server()
            .await
            .expect("start runtime API server");
        persist_runtime_api_snapshot(&snapshot)
            .await
            .expect("persist runtime snapshot");

        assert_eq!(snapshot.bind, "127.0.0.1");
        assert!(snapshot.running);

        let client = reqwest::Client::new();
        let ok = client
            .get(format!("{}/runtime/health", snapshot.base_url))
            .header("Authorization", format!("Bearer {}", snapshot.token))
            .send()
            .await
            .expect("request runtime health with token");
        assert_eq!(ok.status(), StatusCode::OK);

        let unauthorized = client
            .get(format!("{}/runtime/health", snapshot.base_url))
            .send()
            .await
            .expect("request runtime health without token");
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let saved = tokio::fs::read(Paths::runtime_api_state_json())
            .await
            .expect("read persisted runtime state");
        let parsed: RuntimeApiSnapshot =
            serde_json::from_slice(&saved).expect("parse persisted runtime state");
        assert_eq!(parsed.base_url, snapshot.base_url);
        assert_eq!(parsed.token, snapshot.token);
        assert!(parsed.running);

        server.stop();
    }
}
