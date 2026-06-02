use crate::db;
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// CommandRunStatus — state machine
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CommandRunStatus {
    /// Just created, not yet submitted for execution.
    Prepared,
    /// Waiting for user permission to execute.
    AwaitingPermission,
    /// Process spawned, about to start streaming.
    Starting,
    /// Process running, stdout/stderr being streamed.
    Streaming,
    /// Process exited normally (may have non-zero exit code).
    Exited,
    /// Process killed due to timeout.
    TimedOut,
    /// Process killed by user or system.
    Killed,
}

impl CommandRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::AwaitingPermission => "awaiting_permission",
            Self::Starting => "starting",
            Self::Streaming => "streaming",
            Self::Exited => "exited",
            Self::TimedOut => "timed_out",
            Self::Killed => "killed",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "prepared" => Ok(Self::Prepared),
            "awaiting_permission" => Ok(Self::AwaitingPermission),
            "starting" => Ok(Self::Starting),
            "streaming" => Ok(Self::Streaming),
            "exited" => Ok(Self::Exited),
            "timed_out" => Ok(Self::TimedOut),
            "killed" => Ok(Self::Killed),
            other => bail!("unknown command run status: {other}"),
        }
    }

    /// Whether this status represents a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Exited | Self::TimedOut | Self::Killed)
    }

    /// Valid transitions from this status.
    pub fn valid_transitions(&self) -> &'static [CommandRunStatus] {
        match self {
            Self::Prepared => &[Self::AwaitingPermission, Self::Starting, Self::Killed],
            Self::AwaitingPermission => &[Self::Starting, Self::Killed],
            Self::Starting => &[Self::Streaming, Self::Exited, Self::TimedOut, Self::Killed],
            Self::Streaming => &[Self::Exited, Self::TimedOut, Self::Killed],
            Self::Exited | Self::TimedOut | Self::Killed => &[],
        }
    }

    /// Check if transition to `target` is valid.
    pub fn can_transition_to(&self, target: &CommandRunStatus) -> bool {
        self.valid_transitions().contains(target)
    }
}

// ---------------------------------------------------------------------------
// CommandRun — the entity
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CommandRun {
    pub id: String,
    pub session_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub agent_run_id: Option<String>,
    pub permission_grant_id: Option<String>,
    pub risk_level: Option<String>,
    pub env_delta_json: Option<String>,
    pub command: String,
    pub cwd: String,
    pub status: CommandRunStatus,
    pub exit_code: Option<i32>,
    pub stdout_tail: String,
    pub stderr_tail: String,
    pub pid: Option<i64>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl CommandRun {
    pub fn new(command: String, cwd: String, session_id: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: next_id(),
            session_id,
            tool_call_id: None,
            agent_run_id: None,
            permission_grant_id: None,
            risk_level: None,
            env_delta_json: None,
            command,
            cwd,
            status: CommandRunStatus::Prepared,
            exit_code: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            pid: None,
            started_at: None,
            completed_at: None,
            created_at: now,
        }
    }

    /// Transition to a new status, returning error if invalid.
    pub fn transition(&mut self, target: CommandRunStatus) -> anyhow::Result<()> {
        if !self.status.can_transition_to(&target) {
            bail!(
                "invalid transition: {} -> {}",
                self.status.as_str(),
                target.as_str()
            );
        }
        let now = Utc::now();
        if matches!(
            target,
            CommandRunStatus::Starting | CommandRunStatus::Streaming
        ) && self.started_at.is_none()
        {
            self.started_at = Some(now);
        }
        if target.is_terminal() {
            self.completed_at = Some(now);
        }
        self.status = target;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// In-memory registry for live (non-terminal) command runs
// ---------------------------------------------------------------------------

lazy_static! {
    static ref LIVE_RUNS: Arc<RwLock<HashMap<String, Arc<Mutex<CommandRun>>>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

// ---------------------------------------------------------------------------
// CRUD — database persistence
// ---------------------------------------------------------------------------

pub async fn insert(run: &CommandRun) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        INSERT INTO command_runs (
            id, session_id, tool_call_id, agent_run_id, permission_grant_id,
            risk_level, env_delta_json, command, cwd, status, exit_code,
            stdout_tail, stderr_tail, pid, started_at, completed_at, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        "#,
    )
    .bind(&run.id)
    .bind(&run.session_id)
    .bind(&run.tool_call_id)
    .bind(&run.agent_run_id)
    .bind(&run.permission_grant_id)
    .bind(&run.risk_level)
    .bind(&run.env_delta_json)
    .bind(&run.command)
    .bind(&run.cwd)
    .bind(run.status.as_str())
    .bind(run.exit_code)
    .bind(&run.stdout_tail)
    .bind(&run.stderr_tail)
    .bind(run.pid)
    .bind(run.started_at.map(|dt| dt.to_rfc3339()))
    .bind(run.completed_at.map(|dt| dt.to_rfc3339()))
    .bind(run.created_at.to_rfc3339())
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn get(id: &str) -> anyhow::Result<CommandRun> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, session_id, command, cwd, status, exit_code,
               stdout_tail, stderr_tail, pid, started_at, completed_at, created_at,
               tool_call_id, agent_run_id, permission_grant_id, risk_level, env_delta_json
        FROM command_runs
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .with_context(|| format!("command run not found: {id}"))?;
    row_to_command_run(row)
}

pub async fn update(run: &CommandRun) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE command_runs SET
            session_id = ?2, tool_call_id = ?3, agent_run_id = ?4,
            permission_grant_id = ?5, risk_level = ?6, env_delta_json = ?7,
            command = ?8, cwd = ?9, status = ?10, exit_code = ?11,
            stdout_tail = ?12, stderr_tail = ?13, pid = ?14, started_at = ?15,
            completed_at = ?16
        WHERE id = ?1
        "#,
    )
    .bind(&run.id)
    .bind(&run.session_id)
    .bind(&run.tool_call_id)
    .bind(&run.agent_run_id)
    .bind(&run.permission_grant_id)
    .bind(&run.risk_level)
    .bind(&run.env_delta_json)
    .bind(&run.command)
    .bind(&run.cwd)
    .bind(run.status.as_str())
    .bind(run.exit_code)
    .bind(&run.stdout_tail)
    .bind(&run.stderr_tail)
    .bind(run.pid)
    .bind(run.started_at.map(|dt| dt.to_rfc3339()))
    .bind(run.completed_at.map(|dt| dt.to_rfc3339()))
    .execute(&pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct CommandRunFilter {
    pub session_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub agent_run_id: Option<String>,
    pub status: Option<String>,
    pub active_only: bool,
    pub limit: Option<u32>,
}

pub async fn list(limit: Option<u32>) -> anyhow::Result<Vec<CommandRun>> {
    list_filtered(CommandRunFilter {
        limit,
        ..Default::default()
    })
    .await
}

pub async fn list_filtered(filter: CommandRunFilter) -> anyhow::Result<Vec<CommandRun>> {
    let pool = db::pool().await?;
    let mut sql = String::from(
        "SELECT id, session_id, command, cwd, status, exit_code, stdout_tail, stderr_tail, \
         pid, started_at, completed_at, created_at, tool_call_id, agent_run_id, \
         permission_grant_id, risk_level, env_delta_json FROM command_runs WHERE 1=1",
    );
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref session_id) = filter.session_id {
        binds.push(session_id.clone());
        sql.push_str(&format!(" AND session_id = ?{}", binds.len()));
    }
    if let Some(ref tool_call_id) = filter.tool_call_id {
        binds.push(tool_call_id.clone());
        sql.push_str(&format!(" AND tool_call_id = ?{}", binds.len()));
    }
    if let Some(ref agent_run_id) = filter.agent_run_id {
        binds.push(agent_run_id.clone());
        sql.push_str(&format!(" AND agent_run_id = ?{}", binds.len()));
    }
    if let Some(ref status) = filter.status {
        binds.push(status.clone());
        sql.push_str(&format!(" AND status = ?{}", binds.len()));
    }
    if filter.active_only {
        sql.push_str(" AND status NOT IN ('exited', 'timed_out', 'killed')");
    }

    let limit_idx = binds.len() + 1;
    sql.push_str(&format!(" ORDER BY created_at DESC LIMIT ?{limit_idx}"));

    let mut query = sqlx::query(&sql);
    for bind in &binds {
        query = query.bind(bind);
    }
    query = query.bind(filter.limit.unwrap_or(50).clamp(1, 500) as i64);

    let rows = query.fetch_all(&pool).await?;
    rows.into_iter().map(row_to_command_run).collect()
}

pub async fn list_active() -> anyhow::Result<Vec<CommandRun>> {
    list_filtered(CommandRunFilter {
        active_only: true,
        limit: Some(500),
        ..Default::default()
    })
    .await
}

// ---------------------------------------------------------------------------
// Live run management — in-memory tracking for streaming
// ---------------------------------------------------------------------------

/// Register a run in the live registry (called when execution starts).
pub async fn register_live(run: Arc<Mutex<CommandRun>>) {
    let id = run.lock().await.id.clone();
    LIVE_RUNS.write().await.insert(id, run);
}

/// Remove a run from the live registry (called when execution ends).
pub async fn unregister_live(id: &str) {
    LIVE_RUNS.write().await.remove(id);
}

/// Get a live run handle by id.
pub async fn get_live(id: &str) -> Option<Arc<Mutex<CommandRun>>> {
    LIVE_RUNS.read().await.get(id).cloned()
}

// ---------------------------------------------------------------------------
// Kill / cancel
// ---------------------------------------------------------------------------

/// Kill a running command by its command_run_id.
/// Sets status to Killed and persists the final state.
pub async fn kill(id: &str) -> anyhow::Result<CommandRun> {
    // Try to get from live registry first
    if let Some(live) = get_live(id).await {
        let mut run = live.lock().await;
        if run.status.is_terminal() {
            bail!(
                "command run {} already in terminal state: {}",
                id,
                run.status.as_str()
            );
        }
        run.transition(CommandRunStatus::Killed)?;
        // Persist
        update(&run).await?;
        unregister_live(id).await;
        return Ok(run.clone());
    }

    // Fall back to DB
    let mut run = get(id).await?;
    if run.status.is_terminal() {
        bail!(
            "command run {} already in terminal state: {}",
            id,
            run.status.as_str()
        );
    }
    run.transition(CommandRunStatus::Killed)?;
    update(&run).await?;
    Ok(run)
}

// ---------------------------------------------------------------------------
// Tauri event emission
// ---------------------------------------------------------------------------

/// Emit a command_run.stdout event.
/// When `tauri-events` feature is enabled, emits via `app_handle.emit()`.
/// Otherwise a no-op (tests still work).
#[cfg(feature = "tauri-events")]
pub fn emit_stdout_event(app_handle: &tauri::AppHandle, run_id: &str, line: &str) {
    use tauri::Emitter;
    let _ = app_handle.emit(
        "command_run::stdout",
        serde_json::json!({
            "run_id": run_id,
            "line": line,
        }),
    );
}

#[cfg(not(feature = "tauri-events"))]
pub fn emit_stdout_event(_app_handle: &(), _run_id: &str, _line: &str) {
    // No-op in non-Tauri builds
}

/// Emit a command_run.stderr event.
#[cfg(feature = "tauri-events")]
pub fn emit_stderr_event(app_handle: &tauri::AppHandle, run_id: &str, line: &str) {
    use tauri::Emitter;
    let _ = app_handle.emit(
        "command_run::stderr",
        serde_json::json!({
            "run_id": run_id,
            "line": line,
        }),
    );
}

#[cfg(not(feature = "tauri-events"))]
pub fn emit_stderr_event(_app_handle: &(), _run_id: &str, _line: &str) {
    // No-op in non-Tauri builds
}

/// Emit a command_run.finished event.
#[cfg(feature = "tauri-events")]
pub fn emit_finished_event(
    app_handle: &tauri::AppHandle,
    run_id: &str,
    exit_code: Option<i32>,
    status: &str,
) {
    use tauri::Emitter;
    let _ = app_handle.emit(
        "command_run::finished",
        serde_json::json!({
            "run_id": run_id,
            "exit_code": exit_code,
            "status": status,
        }),
    );
}

#[cfg(not(feature = "tauri-events"))]
pub fn emit_finished_event(
    _app_handle: &(),
    _run_id: &str,
    _exit_code: Option<i32>,
    _status: &str,
) {
    // No-op in non-Tauri builds
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn next_id() -> String {
    format!(
        "cr-{}-{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        Uuid::new_v4()
    )
}

fn row_to_command_run(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<CommandRun> {
    Ok(CommandRun {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        tool_call_id: row.try_get("tool_call_id")?,
        agent_run_id: row.try_get("agent_run_id")?,
        permission_grant_id: row.try_get("permission_grant_id")?,
        risk_level: row.try_get("risk_level")?,
        env_delta_json: row.try_get("env_delta_json")?,
        command: row.try_get("command")?,
        cwd: row.try_get("cwd")?,
        status: CommandRunStatus::from_str(row.try_get::<String, _>("status")?.as_str())?,
        exit_code: row.try_get("exit_code")?,
        stdout_tail: row.try_get("stdout_tail")?,
        stderr_tail: row.try_get("stderr_tail")?,
        pid: row.try_get("pid")?,
        started_at: row
            .try_get::<Option<String>, _>("started_at")?
            .as_deref()
            .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
        completed_at: row
            .try_get::<Option<String>, _>("completed_at")?
            .as_deref()
            .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
        created_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[test]
    fn status_as_str_roundtrip() {
        for status in &[
            CommandRunStatus::Prepared,
            CommandRunStatus::AwaitingPermission,
            CommandRunStatus::Starting,
            CommandRunStatus::Streaming,
            CommandRunStatus::Exited,
            CommandRunStatus::TimedOut,
            CommandRunStatus::Killed,
        ] {
            let s = status.as_str();
            let back = CommandRunStatus::from_str(s).unwrap();
            assert_eq!(&back, status);
        }
    }

    #[test]
    fn invalid_status_str_returns_error() {
        assert!(CommandRunStatus::from_str("invalid").is_err());
        assert!(CommandRunStatus::from_str("").is_err());
    }

    #[test]
    fn terminal_statuses() {
        assert!(!CommandRunStatus::Prepared.is_terminal());
        assert!(!CommandRunStatus::AwaitingPermission.is_terminal());
        assert!(!CommandRunStatus::Starting.is_terminal());
        assert!(!CommandRunStatus::Streaming.is_terminal());
        assert!(CommandRunStatus::Exited.is_terminal());
        assert!(CommandRunStatus::TimedOut.is_terminal());
        assert!(CommandRunStatus::Killed.is_terminal());
    }

    #[test]
    fn valid_transitions_from_prepared() {
        let s = CommandRunStatus::Prepared;
        assert!(s.can_transition_to(&CommandRunStatus::AwaitingPermission));
        assert!(s.can_transition_to(&CommandRunStatus::Starting));
        assert!(s.can_transition_to(&CommandRunStatus::Killed));
        assert!(!s.can_transition_to(&CommandRunStatus::Exited));
        assert!(!s.can_transition_to(&CommandRunStatus::Streaming));
    }

    #[test]
    fn valid_transitions_from_streaming() {
        let s = CommandRunStatus::Streaming;
        assert!(s.can_transition_to(&CommandRunStatus::Exited));
        assert!(s.can_transition_to(&CommandRunStatus::TimedOut));
        assert!(s.can_transition_to(&CommandRunStatus::Killed));
        assert!(!s.can_transition_to(&CommandRunStatus::Starting));
        assert!(!s.can_transition_to(&CommandRunStatus::Prepared));
    }

    #[test]
    fn terminal_has_no_transitions() {
        for status in &[
            CommandRunStatus::Exited,
            CommandRunStatus::TimedOut,
            CommandRunStatus::Killed,
        ] {
            assert!(
                status.valid_transitions().is_empty(),
                "{:?} should have no valid transitions",
                status
            );
        }
    }

    #[test]
    fn command_run_new_sets_fields() {
        let run = CommandRun::new(
            "echo hello".to_string(),
            "/workspace".to_string(),
            Some("sess-1".to_string()),
        );
        assert!(!run.id.is_empty());
        assert!(run.id.starts_with("cr-"));
        assert_eq!(run.command, "echo hello");
        assert_eq!(run.cwd, "/workspace");
        assert_eq!(run.session_id.as_deref(), Some("sess-1"));
        assert!(run.tool_call_id.is_none());
        assert!(run.agent_run_id.is_none());
        assert!(run.permission_grant_id.is_none());
        assert!(run.risk_level.is_none());
        assert!(run.env_delta_json.is_none());
        assert_eq!(run.status, CommandRunStatus::Prepared);
        assert!(run.exit_code.is_none());
        assert!(run.stdout_tail.is_empty());
        assert!(run.stderr_tail.is_empty());
        assert!(run.pid.is_none());
        assert!(run.started_at.is_none());
        assert!(run.completed_at.is_none());
    }

    #[test]
    fn command_run_transition_sets_timestamps() {
        let mut run = CommandRun::new("echo test".to_string(), "/tmp".to_string(), None);
        assert!(run.started_at.is_none());

        run.transition(CommandRunStatus::Starting).unwrap();
        assert!(run.started_at.is_some());
        assert!(run.completed_at.is_none());

        run.transition(CommandRunStatus::Streaming).unwrap();
        assert!(run.completed_at.is_none());

        run.transition(CommandRunStatus::Exited).unwrap();
        assert!(run.completed_at.is_some());
        assert_eq!(run.status, CommandRunStatus::Exited);
    }

    #[test]
    fn command_run_invalid_transition_errors() {
        let mut run = CommandRun::new("echo test".to_string(), "/tmp".to_string(), None);
        // Prepared -> Exited is invalid
        assert!(run.transition(CommandRunStatus::Exited).is_err());
        // Prepared -> Streaming is invalid
        assert!(run.transition(CommandRunStatus::Streaming).is_err());
    }

    #[tokio::test]
    async fn insert_and_get_command_run() {
        let _root = TestRoot::new();
        let run = CommandRun::new(
            "git status".to_string(),
            "/workspace".to_string(),
            Some("sess-1".to_string()),
        );
        insert(&run).await.expect("insert");

        let loaded = get(&run.id).await.expect("get");
        assert_eq!(loaded.id, run.id);
        assert_eq!(loaded.command, "git status");
        assert_eq!(loaded.cwd, "/workspace");
        assert_eq!(loaded.session_id.as_deref(), Some("sess-1"));
        assert_eq!(loaded.status, CommandRunStatus::Prepared);
    }

    #[tokio::test]
    async fn insert_and_filter_command_run_links() {
        let _root = TestRoot::new();
        let mut run = CommandRun::new(
            "echo linked".to_string(),
            "/workspace".to_string(),
            Some("sess-linked".to_string()),
        );
        run.tool_call_id = Some("tc-linked".to_string());
        run.agent_run_id = Some("ar-linked".to_string());
        run.permission_grant_id = Some("pg-linked".to_string());
        run.risk_level = Some("external_side_effect".to_string());
        run.env_delta_json = Some(r#"{"PATH":"<redacted>"}"#.to_string());
        insert(&run).await.expect("insert");

        let loaded = get(&run.id).await.expect("get");
        assert_eq!(loaded.tool_call_id.as_deref(), Some("tc-linked"));
        assert_eq!(loaded.agent_run_id.as_deref(), Some("ar-linked"));
        assert_eq!(loaded.permission_grant_id.as_deref(), Some("pg-linked"));
        assert_eq!(loaded.risk_level.as_deref(), Some("external_side_effect"));

        let filtered = list_filtered(CommandRunFilter {
            tool_call_id: Some("tc-linked".to_string()),
            ..Default::default()
        })
        .await
        .expect("list filtered");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, run.id);
    }

    #[tokio::test]
    async fn update_command_run_persists_changes() {
        let _root = TestRoot::new();
        let mut run = CommandRun::new("echo hello".to_string(), "/tmp".to_string(), None);
        insert(&run).await.expect("insert");

        run.transition(CommandRunStatus::Starting).unwrap();
        run.pid = Some(12345);
        run.stdout_tail = "hello\n".to_string();
        update(&run).await.expect("update");

        let loaded = get(&run.id).await.expect("get");
        assert_eq!(loaded.status, CommandRunStatus::Starting);
        assert_eq!(loaded.pid, Some(12345));
        assert_eq!(loaded.stdout_tail, "hello\n");
        assert!(loaded.started_at.is_some());
    }

    #[tokio::test]
    async fn list_active_excludes_terminal() {
        let _root = TestRoot::new();

        // Create an active run
        let mut active_run = CommandRun::new("ls".to_string(), "/tmp".to_string(), None);
        active_run.transition(CommandRunStatus::Starting).unwrap();
        active_run.transition(CommandRunStatus::Streaming).unwrap();
        insert(&active_run).await.expect("insert active");

        // Create a terminal run
        let mut terminal_run = CommandRun::new("pwd".to_string(), "/tmp".to_string(), None);
        terminal_run.transition(CommandRunStatus::Starting).unwrap();
        terminal_run.transition(CommandRunStatus::Exited).unwrap();
        terminal_run.exit_code = Some(0);
        insert(&terminal_run).await.expect("insert terminal");

        let active = list_active().await.expect("list_active");
        assert!(active.iter().all(|r| !r.status.is_terminal()));
        assert!(active.iter().any(|r| r.id == active_run.id));
        assert!(!active.iter().any(|r| r.id == terminal_run.id));
    }

    #[tokio::test]
    async fn list_with_limit() {
        let _root = TestRoot::new();
        for i in 0..5 {
            let run = CommandRun::new(format!("echo {i}"), "/tmp".to_string(), None);
            insert(&run).await.expect("insert");
        }
        let all = list(None).await.expect("list all");
        assert!(all.len() >= 5);

        let limited = list(Some(3)).await.expect("list limited");
        assert_eq!(limited.len(), 3);
    }

    #[tokio::test]
    async fn kill_sets_status_to_killed() {
        let _root = TestRoot::new();
        let mut run = CommandRun::new("sleep 100".to_string(), "/tmp".to_string(), None);
        run.transition(CommandRunStatus::Starting).unwrap();
        run.pid = Some(99999);
        insert(&run).await.expect("insert");

        let killed = kill(&run.id).await.expect("kill");
        assert_eq!(killed.status, CommandRunStatus::Killed);
        assert!(killed.completed_at.is_some());

        // Verify persisted
        let loaded = get(&run.id).await.expect("get after kill");
        assert_eq!(loaded.status, CommandRunStatus::Killed);
    }

    #[tokio::test]
    async fn kill_terminal_run_errors() {
        let _root = TestRoot::new();
        let mut run = CommandRun::new("echo done".to_string(), "/tmp".to_string(), None);
        run.transition(CommandRunStatus::Starting).unwrap();
        run.transition(CommandRunStatus::Exited).unwrap();
        run.exit_code = Some(0);
        insert(&run).await.expect("insert");

        let result = kill(&run.id).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("terminal state"));
    }

    #[tokio::test]
    async fn get_nonexistent_returns_error() {
        let _root = TestRoot::new();
        let result = get("cr-nonexistent").await;
        assert!(result.is_err());
    }

    #[test]
    fn serialization_roundtrip() {
        let run = CommandRun::new(
            "cargo test".to_string(),
            "I:/personal-agent".to_string(),
            Some("sess-abc".to_string()),
        );
        let json = serde_json::to_string(&run).unwrap();
        let back: CommandRun = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, run.id);
        assert_eq!(back.command, run.command);
        assert_eq!(back.cwd, run.cwd);
        assert_eq!(back.status, run.status);
    }
}
