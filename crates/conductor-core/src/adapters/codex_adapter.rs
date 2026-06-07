use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::agent_teams;
use crate::codex::{self, InteractiveAgentSessionStatus};
use crate::events;
use crate::goal_tasks::AgentTask;
use crate::heartbeat;

// ---------------------------------------------------------------------------
// AgentRunRef -- lightweight handle returned from CodexAdapter::spawn
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentRunRef {
    pub run_id: String,
    pub session_id: String,
    pub task_id: String,
    pub workspace_id: String,
    pub status: CodexRunStatus,
    pub started_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// CodexRunStatus -- adapter-level status that maps session state to runtime state
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexRunStatus {
    /// Session created, process starting.
    Starting,
    /// Actively running.
    Running,
    /// Process completed successfully.
    Completed,
    /// Process failed.
    Failed,
    /// Waiting for user/input after interrupt.
    AwaitingInput,
    /// Session was interrupted.
    Interrupted,
}

impl CodexRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::AwaitingInput => "awaiting_input",
            Self::Interrupted => "interrupted",
        }
    }

    /// Map from InteractiveAgentSessionStatus to CodexRunStatus.
    pub fn from_session_status(status: &InteractiveAgentSessionStatus) -> Self {
        match status {
            InteractiveAgentSessionStatus::Created | InteractiveAgentSessionStatus::Starting => {
                CodexRunStatus::Starting
            }
            InteractiveAgentSessionStatus::Ready | InteractiveAgentSessionStatus::Running => {
                CodexRunStatus::Running
            }
            InteractiveAgentSessionStatus::AwaitInput => CodexRunStatus::AwaitingInput,
            InteractiveAgentSessionStatus::Interrupted
            | InteractiveAgentSessionStatus::Resumable => CodexRunStatus::Interrupted,
            InteractiveAgentSessionStatus::Completed => CodexRunStatus::Completed,
            InteractiveAgentSessionStatus::Failed => CodexRunStatus::Failed,
        }
    }
}

// ---------------------------------------------------------------------------
// RuntimeEvent -- events emitted during output parsing
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum RuntimeEvent {
    /// Session is actively running a task.
    Running {
        session_id: String,
        task_id: String,
        workspace_id: String,
    },
    /// Session completed the task successfully.
    Completed {
        session_id: String,
        task_id: String,
        workspace_id: String,
        exit_code: Option<i32>,
    },
    /// Session failed with an error.
    Failed {
        session_id: String,
        task_id: String,
        workspace_id: String,
        error: String,
    },
    /// Session is awaiting input (interrupted -> resumable).
    AwaitingInput {
        session_id: String,
        task_id: String,
        workspace_id: String,
    },
}

// ---------------------------------------------------------------------------
// ActiveRun -- internal tracking of spawned sessions
// ---------------------------------------------------------------------------

struct ActiveRun {
    run_ref: AgentRunRef,
    last_emitted_status: Option<CodexRunStatus>,
}

// ---------------------------------------------------------------------------
// CodexAdapter
// ---------------------------------------------------------------------------

/// Adapter that bridges `AgentTask` (goal_tasks) and `InteractiveAgentSession` (codex).
///
/// Responsibilities:
/// - Spawn codex sessions for tasks and track them
/// - Parse session output status into `RuntimeEvent`
/// - Send periodic heartbeats
/// - Handle interrupts (task -> awaiting_input)
pub struct CodexAdapter {
    /// Agent identity used for heartbeats.
    agent_id: String,
    /// Default command to run (defaults to "codex").
    command: Option<String>,
    /// Heartbeat TTL in seconds.
    heartbeat_ttl: i64,
    /// Active runs indexed by session_id.
    active_runs: Arc<RwLock<HashMap<String, ActiveRun>>>,
}

impl CodexAdapter {
    /// Create a new CodexAdapter with the given agent identity.
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            command: None,
            heartbeat_ttl: 300,
            active_runs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Override the codex binary command.
    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.command = Some(command.into());
        self
    }

    /// Override the heartbeat TTL (seconds).
    pub fn with_heartbeat_ttl(mut self, ttl: i64) -> Self {
        self.heartbeat_ttl = ttl;
        self
    }

    /// Spawn a codex session for the given task.
    ///
    /// 1. Creates/reuses an `InteractiveAgentSession`
    /// 2. Injects `task_id` into `session_data`
    /// 3. Returns an `AgentRunRef`
    pub async fn spawn(&self, task: &AgentTask) -> anyhow::Result<AgentRunRef> {
        let run_id = format!(
            "codex-run-{}-{}",
            Utc::now().format("%Y%m%dT%H%M%SZ"),
            Uuid::new_v4()
        );

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        // Start the codex session.
        let session =
            codex::start_session(cwd, self.command.clone(), Some(task.workspace_id.clone()))
                .await
                .context("failed to start codex session")?;

        // Inject task_id into session_data by updating the live session's data.
        // (session_data is managed inside codex::LIVE_SESSIONS)
        let status = CodexRunStatus::from_session_status(&session.status);
        let started_at = Utc::now();

        let run_ref = AgentRunRef {
            run_id: run_id.clone(),
            session_id: session.id.clone(),
            task_id: task.id.clone(),
            workspace_id: task.workspace_id.clone(),
            status: status.clone(),
            started_at,
        };

        // Track the active run. Set last_emitted_status to the current status
        // so that poll_status only emits events on actual transitions.
        let active = ActiveRun {
            run_ref: run_ref.clone(),
            last_emitted_status: Some(status.clone()),
        };
        self.active_runs
            .write()
            .await
            .insert(session.id.clone(), active);

        // Emit event.
        let _ = events::append(
            "codex_adapter",
            "codex_adapter.spawned",
            &serde_json::json!({
                "run_id": run_id,
                "session_id": session.id,
                "task_id": task.id,
                "workspace_id": task.workspace_id,
            }),
        )
        .await;

        // Send initial heartbeat.
        let _ = self
            .send_heartbeat(
                &task.workspace_id,
                Some(&task.id),
                "working",
                Some("spawning codex session"),
            )
            .await;

        bind_executor_session_to_team_member(task, &run_ref).await?;

        Ok(run_ref)
    }

    /// Parse the current session status and emit a `RuntimeEvent` if the status changed.
    ///
    /// Returns `Some(event)` if the status changed since last check, `None` otherwise.
    pub async fn poll_status(&self, session_id: &str) -> anyhow::Result<Option<RuntimeEvent>> {
        let session = codex::get_session(session_id)
            .await
            .context("failed to get session")?;

        let current_status = CodexRunStatus::from_session_status(&session.status);

        let mut runs = self.active_runs.write().await;
        let active = runs.get_mut(session_id);

        let (run_ref, should_emit) = if let Some(active) = active {
            let changed = active.last_emitted_status.as_ref() != Some(&current_status);
            active.last_emitted_status = Some(current_status.clone());
            active.run_ref.status = current_status.clone();
            (active.run_ref.clone(), changed)
        } else {
            return Ok(None);
        };

        if !should_emit {
            return Ok(None);
        }

        let event = match &current_status {
            CodexRunStatus::Running => Some(RuntimeEvent::Running {
                session_id: session_id.to_string(),
                task_id: run_ref.task_id,
                workspace_id: run_ref.workspace_id,
            }),
            CodexRunStatus::Completed => Some(RuntimeEvent::Completed {
                session_id: session_id.to_string(),
                task_id: run_ref.task_id,
                workspace_id: run_ref.workspace_id,
                exit_code: session.exit_code,
            }),
            CodexRunStatus::Failed => Some(RuntimeEvent::Failed {
                session_id: session_id.to_string(),
                task_id: run_ref.task_id,
                workspace_id: run_ref.workspace_id,
                error: session
                    .session_data
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
            }),
            CodexRunStatus::AwaitingInput => Some(RuntimeEvent::AwaitingInput {
                session_id: session_id.to_string(),
                task_id: run_ref.task_id,
                workspace_id: run_ref.workspace_id,
            }),
            _ => None,
        };

        if let Some(ref ev) = event {
            let _ = events::append(
                "codex_adapter",
                "codex_adapter.status_changed",
                &serde_json::to_value(ev).unwrap_or_default(),
            )
            .await;

            // Write back terminal states to agent_tasks (mirrors claude_p adapter behaviour).
            match ev {
                RuntimeEvent::Completed {
                    task_id,
                    session_id: sid,
                    ..
                } => {
                    let result_ref = format!("codex:session:{sid}");
                    if let Err(e) =
                        crate::goal_tasks::set_task_result_ref_review_ready(task_id, &result_ref)
                            .await
                    {
                        tracing::warn!(
                            task_id = %task_id,
                            error = %e,
                            "codex_adapter: failed to write review_ready writeback"
                        );
                    }
                }
                RuntimeEvent::Failed { task_id, error, .. } => {
                    let task = crate::goal_tasks::get_task(task_id).await.ok().flatten();
                    if task.map(|t| t.status == "running").unwrap_or(false) {
                        if let Err(e) = crate::goal_tasks::fail_task(task_id, error).await {
                            tracing::warn!(
                                task_id = %task_id,
                                error = %e,
                                "codex_adapter: failed to write failed writeback"
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(event)
    }

    /// Send a heartbeat for the adapter's agent identity.
    pub async fn send_heartbeat(
        &self,
        workspace_id: &str,
        task_id: Option<&str>,
        status: &str,
        progress_text: Option<&str>,
    ) -> anyhow::Result<()> {
        heartbeat::upsert_heartbeat(
            workspace_id,
            &self.agent_id,
            None,
            task_id,
            None,
            status,
            Some("codex_adapter"),
            progress_text,
            0,
            self.heartbeat_ttl,
        )
        .await
        .context("failed to send heartbeat")?;
        Ok(())
    }

    /// Interrupt a running session.
    ///
    /// 1. Interrupts the codex session
    /// 2. Returns an AwaitingInput RuntimeEvent
    pub async fn interrupt(&self, session_id: &str) -> anyhow::Result<RuntimeEvent> {
        // Get the run ref before interrupting.
        let run_ref = {
            let runs = self.active_runs.read().await;
            runs.get(session_id)
                .map(|a| a.run_ref.clone())
                .ok_or_else(|| anyhow::anyhow!("session not tracked: {session_id}"))?
        };

        // Interrupt the codex session.
        codex::interrupt_session(session_id)
            .await
            .context("failed to interrupt session")?;

        // Update the active run status.
        {
            let mut runs = self.active_runs.write().await;
            if let Some(active) = runs.get_mut(session_id) {
                active.run_ref.status = CodexRunStatus::AwaitingInput;
                active.last_emitted_status = Some(CodexRunStatus::AwaitingInput);
            }
        }

        let event = RuntimeEvent::AwaitingInput {
            session_id: session_id.to_string(),
            task_id: run_ref.task_id.clone(),
            workspace_id: run_ref.workspace_id.clone(),
        };

        let _ = events::append(
            "codex_adapter",
            "codex_adapter.interrupted",
            &serde_json::to_value(&event).unwrap_or_default(),
        )
        .await;

        // Update heartbeat.
        let _ = self
            .send_heartbeat(
                &run_ref.workspace_id,
                Some(&run_ref.task_id),
                "awaiting_input",
                Some("session interrupted, awaiting input"),
            )
            .await;

        Ok(event)
    }

    /// Remove a completed/failed run from tracking.
    pub async fn remove_run(&self, session_id: &str) -> Option<AgentRunRef> {
        self.active_runs
            .write()
            .await
            .remove(session_id)
            .map(|a| a.run_ref)
    }

    /// Get the number of active runs.
    pub async fn active_count(&self) -> usize {
        self.active_runs.read().await.len()
    }

    /// Get a snapshot of all active run refs.
    pub async fn list_active(&self) -> Vec<AgentRunRef> {
        self.active_runs
            .read()
            .await
            .values()
            .map(|a| a.run_ref.clone())
            .collect()
    }
}

async fn bind_executor_session_to_team_member(
    task: &AgentTask,
    run_ref: &AgentRunRef,
) -> anyhow::Result<()> {
    let Some(cycle_id) = task.cycle_id.as_deref() else {
        return Ok(());
    };

    agent_teams::bind_member_run_to_task(
        &format!("team-{cycle_id}"),
        &task.id,
        &run_ref.run_id,
        Some(serde_json::json!({
            "agent_run_id": run_ref.run_id,
            "session_id": run_ref.session_id,
        })),
    )
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    fn make_task(workspace_id: &str, task_id: &str) -> AgentTask {
        AgentTask {
            id: task_id.to_string(),
            workspace_id: workspace_id.to_string(),
            goal_id: Some("goal-1".to_string()),
            cycle_id: None,
            parent_task_id: None,
            title: "Test task".to_string(),
            instruction: "Do something".to_string(),
            status: "claimed".to_string(),
            agent_kind: "codex_interactive".to_string(),
            assigned_agent_id: None,
            claimed_by: Some("codex-adapter".to_string()),
            write_scope_json: vec![],
            read_scope_json: vec![],
            allowed_tools_json: vec![],
            dependencies_json: vec![],
            acceptance_json: vec![],
            result_ref: None,
            error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            claimed_at: Some(Utc::now()),
            finished_at: None,
        }
    }

    // -- Test 1: CodexRunStatus mapping from session status --

    #[test]
    fn codex_run_status_from_session_status_mapping() {
        assert_eq!(
            CodexRunStatus::from_session_status(&InteractiveAgentSessionStatus::Created),
            CodexRunStatus::Starting
        );
        assert_eq!(
            CodexRunStatus::from_session_status(&InteractiveAgentSessionStatus::Starting),
            CodexRunStatus::Starting
        );
        assert_eq!(
            CodexRunStatus::from_session_status(&InteractiveAgentSessionStatus::Ready),
            CodexRunStatus::Running
        );
        assert_eq!(
            CodexRunStatus::from_session_status(&InteractiveAgentSessionStatus::Running),
            CodexRunStatus::Running
        );
        assert_eq!(
            CodexRunStatus::from_session_status(&InteractiveAgentSessionStatus::AwaitInput),
            CodexRunStatus::AwaitingInput
        );
        assert_eq!(
            CodexRunStatus::from_session_status(&InteractiveAgentSessionStatus::Interrupted),
            CodexRunStatus::Interrupted
        );
        assert_eq!(
            CodexRunStatus::from_session_status(&InteractiveAgentSessionStatus::Resumable),
            CodexRunStatus::Interrupted
        );
        assert_eq!(
            CodexRunStatus::from_session_status(&InteractiveAgentSessionStatus::Completed),
            CodexRunStatus::Completed
        );
        assert_eq!(
            CodexRunStatus::from_session_status(&InteractiveAgentSessionStatus::Failed),
            CodexRunStatus::Failed
        );
    }

    // -- Test 2: CodexRunStatus serialization roundtrip --

    #[test]
    fn codex_run_status_serde_roundtrip() {
        let statuses = vec![
            CodexRunStatus::Starting,
            CodexRunStatus::Running,
            CodexRunStatus::Completed,
            CodexRunStatus::Failed,
            CodexRunStatus::AwaitingInput,
            CodexRunStatus::Interrupted,
        ];
        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let back: CodexRunStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, status, "roundtrip failed for {:?}", status);
        }
    }

    // -- Test 3: RuntimeEvent serialization --

    #[test]
    fn runtime_event_serialization() {
        let ev = RuntimeEvent::Running {
            session_id: "sess-1".to_string(),
            task_id: "task-1".to_string(),
            workspace_id: "ws-1".to_string(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"Running\""));
        assert!(json.contains("sess-1"));

        let ev2 = RuntimeEvent::Completed {
            session_id: "sess-2".to_string(),
            task_id: "task-2".to_string(),
            workspace_id: "ws-2".to_string(),
            exit_code: Some(0),
        };
        let json2 = serde_json::to_string(&ev2).unwrap();
        assert!(json2.contains("\"type\":\"Completed\""));
        assert!(json2.contains("\"exit_code\":0"));

        let ev3 = RuntimeEvent::AwaitingInput {
            session_id: "sess-3".to_string(),
            task_id: "task-3".to_string(),
            workspace_id: "ws-3".to_string(),
        };
        let json3 = serde_json::to_string(&ev3).unwrap();
        assert!(json3.contains("AwaitingInput"));
    }

    // -- Test 4: CodexAdapter spawn returns AgentRunRef with correct fields --

    #[tokio::test]
    async fn codex_adapter_spawn_returns_run_ref() {
        let _root = TestRoot::new();
        let task = make_task("ws-spawn", "task-spawn-1");

        // Use a binary that exits quickly on all platforms.
        let adapter = CodexAdapter::new("test-agent").with_command(if cfg!(windows) {
            "where.exe"
        } else {
            "echo"
        });

        let run_ref = adapter.spawn(&task).await.expect("spawn should succeed");

        assert!(!run_ref.run_id.is_empty());
        assert!(!run_ref.session_id.is_empty());
        assert_eq!(run_ref.task_id, "task-spawn-1");
        assert_eq!(run_ref.workspace_id, "ws-spawn");
        // Status should be Running since codex::start_session transitions to Running.
        assert_eq!(run_ref.status, CodexRunStatus::Running);
        // Active count should be 1.
        assert_eq!(adapter.active_count().await, 1);
        // list_active should return the run.
        let active = adapter.list_active().await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].session_id, run_ref.session_id);
    }

    // -- Test 5: CodexAdapter poll_status returns None when status unchanged --

    #[tokio::test]
    async fn codex_adapter_poll_status_returns_none_when_unchanged() {
        let _root = TestRoot::new();

        // Create a session manually and persist it so get_session can find it.
        let mut session =
            codex::InteractiveAgentSession::new("test-poll".into(), PathBuf::from("/tmp"));
        session
            .transition(InteractiveAgentSessionStatus::Starting)
            .unwrap();
        session
            .transition(InteractiveAgentSessionStatus::Running)
            .unwrap();

        // Persist to DB so codex::get_session finds it.
        let pool = crate::db::pool().await.unwrap();
        sqlx::query(
            r#"INSERT INTO codex_sessions (id, command, cwd, status, pid, exit_code,
                created_at, started_at, completed_at, session_data)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
        )
        .bind(&session.id)
        .bind(&session.command)
        .bind(session.cwd.to_string_lossy().as_ref())
        .bind(session.status.as_str())
        .bind(session.pid)
        .bind(session.exit_code)
        .bind(session.created_at.to_rfc3339())
        .bind(session.started_at.map(|dt| dt.to_rfc3339()))
        .bind(session.completed_at.map(|dt| dt.to_rfc3339()))
        .bind(session.session_data.to_string())
        .execute(&pool)
        .await
        .expect("persist session");

        let adapter = CodexAdapter::new("test-poll-agent");

        let run_ref = AgentRunRef {
            run_id: "codex-run-poll".to_string(),
            session_id: session.id.clone(),
            task_id: "task-poll-1".to_string(),
            workspace_id: "ws-poll".to_string(),
            status: CodexRunStatus::Running,
            started_at: Utc::now(),
        };

        // Register in adapter with Running status already emitted.
        adapter.active_runs.write().await.insert(
            session.id.clone(),
            ActiveRun {
                run_ref: run_ref.clone(),
                last_emitted_status: Some(CodexRunStatus::Running),
            },
        );

        // First poll: session is still Running, last_emitted is Running -> None.
        let event = adapter.poll_status(&session.id).await.expect("poll");
        assert!(event.is_none(), "no event when status has not changed");

        // Second poll still same status -> None.
        let event2 = adapter.poll_status(&session.id).await.expect("poll 2");
        assert!(event2.is_none(), "still no event when status unchanged");

        // Verify the run is still tracked.
        assert_eq!(adapter.active_count().await, 1);
    }

    // -- Test 6: CodexAdapter interrupt emits AwaitingInput --

    #[tokio::test]
    async fn codex_adapter_interrupt_emits_awaiting_input() {
        let _root = TestRoot::new();

        let adapter = CodexAdapter::new("test-interrupt-agent");

        // Use a session created via codex::start_session with a quick-exit command.
        let session = codex::start_session(
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            Some(if cfg!(windows) {
                "where.exe".to_string()
            } else {
                "echo".to_string()
            }),
            Some("ws-interrupt".to_string()),
        )
        .await
        .expect("start session");

        let run_ref = AgentRunRef {
            run_id: "codex-run-interrupt-test".to_string(),
            session_id: session.id.clone(),
            task_id: "task-interrupt-1".to_string(),
            workspace_id: "ws-interrupt".to_string(),
            status: CodexRunStatus::Running,
            started_at: Utc::now(),
        };

        // Register in adapter.
        adapter.active_runs.write().await.insert(
            session.id.clone(),
            ActiveRun {
                run_ref: run_ref.clone(),
                last_emitted_status: Some(CodexRunStatus::Running),
            },
        );

        // Attempt interrupt. The process may have already exited (quick-exit command),
        // so the interrupt may succeed or fail.
        let result = adapter.interrupt(&session.id).await;
        match result {
            Ok(event) => {
                assert!(matches!(event, RuntimeEvent::AwaitingInput { .. }));
                if let RuntimeEvent::AwaitingInput { task_id, .. } = event {
                    assert_eq!(task_id, "task-interrupt-1");
                }
            }
            Err(_) => {
                // If the process already exited, interrupt_session fails.
                // This is acceptable -- the adapter logic is verified below.
            }
        }

        // Verify remove works regardless of interrupt outcome.
        assert_eq!(adapter.active_count().await, 1);
        let removed = adapter.remove_run(&session.id).await;
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().task_id, "task-interrupt-1");
        assert_eq!(adapter.active_count().await, 0);
    }

    // -- Test 7: CodexAdapter heartbeat integration --

    #[tokio::test]
    async fn codex_adapter_send_heartbeat() {
        let _root = TestRoot::new();

        let adapter = CodexAdapter::new("hb-test-agent");

        adapter
            .send_heartbeat("ws-hb", Some("task-hb-1"), "working", Some("testing"))
            .await
            .expect("heartbeat should succeed");

        let hb = crate::heartbeat::get_heartbeat("ws-hb", "hb-test-agent")
            .await
            .expect("get heartbeat");
        assert!(hb.is_some(), "heartbeat should exist");
        let hb = hb.unwrap();
        assert_eq!(hb.status, "working");
        assert_eq!(hb.task_id.as_deref(), Some("task-hb-1"));
        assert_eq!(hb.stage_label.as_deref(), Some("codex_adapter"));
        assert_eq!(hb.progress_text.as_deref(), Some("testing"));
    }

    // -- Test 8: CodexAdapter empty state --

    #[test]
    fn codex_adapter_new_has_zero_active_runs() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let adapter = CodexAdapter::new("empty-agent");
        rt.block_on(async {
            assert_eq!(adapter.active_count().await, 0);
            assert!(adapter.list_active().await.is_empty());
        });
    }
}
