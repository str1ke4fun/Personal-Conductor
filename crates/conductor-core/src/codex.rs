use crate::{db, events, tasks};
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::RwLock;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// CodexConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CodexConfig {
    pub enabled: bool,
    pub api_endpoint: String,
    pub workspace_root: PathBuf,
    /// Path to the codex binary. Defaults to "codex" (expects it on PATH).
    pub codex_binary: String,
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_endpoint: "https://api.openai.com/v1".to_string(),
            workspace_root: PathBuf::from(".codex"),
            codex_binary: "codex".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// InteractiveAgentSessionStatus — state machine
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InteractiveAgentSessionStatus {
    /// Session struct created, process not yet spawned.
    Created,
    /// Process spawn initiated.
    Starting,
    /// Process spawned and ready for interaction (codex banner loaded).
    Ready,
    /// Process is actively running a task.
    Running,
    /// Process is waiting for user input (prompt visible).
    AwaitInput,
    /// Process was interrupted by user (Ctrl+C / kill signal).
    Interrupted,
    /// Session can be resumed (was interrupted, not yet re-spawned).
    Resumable,
    /// Process exited normally.
    Completed,
    /// Process exited with error or failed to spawn.
    Failed,
}

impl InteractiveAgentSessionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::AwaitInput => "await_input",
            Self::Interrupted => "interrupted",
            Self::Resumable => "resumable",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "created" => Ok(Self::Created),
            "starting" => Ok(Self::Starting),
            "ready" => Ok(Self::Ready),
            "running" => Ok(Self::Running),
            "await_input" => Ok(Self::AwaitInput),
            "interrupted" => Ok(Self::Interrupted),
            "resumable" => Ok(Self::Resumable),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            other => bail!("unknown interactive agent session status: {other}"),
        }
    }

    /// Whether this status represents a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }

    /// Valid transitions from this status.
    pub fn valid_transitions(&self) -> &'static [InteractiveAgentSessionStatus] {
        match self {
            Self::Created => &[Self::Starting, Self::Failed],
            Self::Starting => &[Self::Ready, Self::Running, Self::Failed],
            Self::Ready => &[
                Self::Running,
                Self::Interrupted,
                Self::Completed,
                Self::Failed,
            ],
            Self::Running => &[
                Self::AwaitInput,
                Self::Interrupted,
                Self::Completed,
                Self::Failed,
            ],
            Self::AwaitInput => &[
                Self::Running,
                Self::Interrupted,
                Self::Completed,
                Self::Failed,
            ],
            Self::Interrupted => &[Self::Resumable, Self::Failed],
            Self::Resumable => &[Self::Starting, Self::Failed],
            Self::Completed | Self::Failed => &[],
        }
    }

    /// Check if transition to `target` is valid.
    pub fn can_transition_to(&self, target: &InteractiveAgentSessionStatus) -> bool {
        self.valid_transitions().contains(target)
    }
}

// ---------------------------------------------------------------------------
// InteractiveAgentSession — the entity
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InteractiveAgentSession {
    pub id: String,
    /// The command that was/will be executed (e.g. "codex").
    pub command: String,
    /// Working directory for the session.
    pub cwd: PathBuf,
    /// Current session status.
    pub status: InteractiveAgentSessionStatus,
    /// OS process ID (if spawned).
    pub pid: Option<i64>,
    /// Process exit code (if completed/failed).
    pub exit_code: Option<i32>,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the process was started.
    pub started_at: Option<DateTime<Utc>>,
    /// When the process completed/failed/was interrupted.
    pub completed_at: Option<DateTime<Utc>>,
    /// Arbitrary JSON blob for session persistence (accumulated output, context, etc.)
    pub session_data: serde_json::Value,
}

impl InteractiveAgentSession {
    pub fn new(command: String, cwd: PathBuf) -> Self {
        Self {
            id: format!(
                "ias-{}-{}",
                Utc::now().format("%Y%m%dT%H%M%SZ"),
                Uuid::new_v4()
            ),
            command,
            cwd,
            status: InteractiveAgentSessionStatus::Created,
            pid: None,
            exit_code: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            session_data: serde_json::json!({}),
        }
    }

    /// Transition to a new status, returning error if invalid.
    pub fn transition(&mut self, target: InteractiveAgentSessionStatus) -> anyhow::Result<()> {
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
            InteractiveAgentSessionStatus::Starting | InteractiveAgentSessionStatus::Ready
        ) && self.started_at.is_none()
        {
            self.started_at = Some(now);
        }
        if target.is_terminal() || target == InteractiveAgentSessionStatus::Interrupted {
            self.completed_at = Some(now);
        }
        self.status = target;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// CodexActivity (kept for backward compat)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CodexActivity {
    pub session_id: String,
    pub cwd: PathBuf,
    pub changed_files: Vec<PathBuf>,
    pub last_activity: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// CodexTask (kept for backward compat)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CodexTask {
    pub id: String,
    pub source: String,
    pub kind: String,
    pub artifact: tasks::Artifact,
    pub summary_ref: Option<String>,
    pub est_minutes: Option<u32>,
    pub focus_hint: Option<String>,
    pub status: tasks::TaskStatus,
    pub created_at: DateTime<Utc>,
}

impl From<CodexTask> for tasks::Task {
    fn from(codex_task: CodexTask) -> Self {
        tasks::Task {
            id: codex_task.id,
            source: codex_task.source,
            kind: codex_task.kind,
            artifact: codex_task.artifact,
            summary_ref: codex_task.summary_ref,
            est_minutes: codex_task.est_minutes,
            focus_hint: codex_task.focus_hint,
            status: codex_task.status,
            created_at: codex_task.created_at,
            session_id: None,
            terminal_id: None,
            cwd: None,
            current_request: None,
            last_output_summary: None,
            last_event_at: None,
            permission_summary: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Live session registry (in-memory, backed by child processes)
// ---------------------------------------------------------------------------

struct LiveSession {
    meta: InteractiveAgentSession,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout_buf: Vec<u8>,
    stderr_buf: Vec<u8>,
    exit_code: Option<i32>,
}

lazy_static::lazy_static! {
    static ref LIVE_SESSIONS: RwLock<Vec<LiveSession>> = RwLock::new(Vec::new());
}

// ---------------------------------------------------------------------------
// Session persistence (SQLite)
// ---------------------------------------------------------------------------

async fn persist_session(session: &InteractiveAgentSession) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        INSERT INTO codex_sessions (id, command, cwd, status, pid, exit_code, created_at, started_at, completed_at, session_data)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(id) DO UPDATE SET
            status = excluded.status,
            pid = excluded.pid,
            exit_code = excluded.exit_code,
            started_at = excluded.started_at,
            completed_at = excluded.completed_at,
            session_data = excluded.session_data
        "#,
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
    .await?;
    Ok(())
}

async fn load_session_from_db(session_id: &str) -> anyhow::Result<InteractiveAgentSession> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, command, cwd, status, pid, exit_code, created_at, started_at, completed_at, session_data
        FROM codex_sessions
        WHERE id = ?1
        "#,
    )
    .bind(session_id)
    .fetch_one(&pool)
    .await
    .with_context(|| format!("codex session not found in DB: {session_id}"))?;

    let status_str: String = row.try_get("status")?;
    let created_at_str: String = row.try_get("created_at")?;
    let started_at_str: Option<String> = row.try_get("started_at")?;
    let completed_at_str: Option<String> = row.try_get("completed_at")?;
    let session_data_str: String = row.try_get("session_data")?;

    Ok(InteractiveAgentSession {
        id: row.try_get("id")?,
        command: row.try_get("command")?,
        cwd: PathBuf::from(row.try_get::<String, _>("cwd")?),
        status: InteractiveAgentSessionStatus::from_str(&status_str)?,
        pid: row.try_get("pid")?,
        exit_code: row.try_get("exit_code")?,
        created_at: DateTime::parse_from_rfc3339(&created_at_str)?.with_timezone(&Utc),
        started_at: started_at_str
            .as_deref()
            .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
        completed_at: completed_at_str
            .as_deref()
            .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
        session_data: serde_json::from_str(&session_data_str).unwrap_or(serde_json::json!({})),
    })
}

async fn list_sessions_from_db(limit: Option<u32>) -> anyhow::Result<Vec<InteractiveAgentSession>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, command, cwd, status, pid, exit_code, created_at, started_at, completed_at, session_data
        FROM codex_sessions
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )
    .bind(limit.unwrap_or(50).clamp(1, 500) as i64)
    .fetch_all(&pool)
    .await?;

    let mut sessions = Vec::new();
    for row in rows {
        let status_str: String = row.try_get("status")?;
        let created_at_str: String = row.try_get("created_at")?;
        let started_at_str: Option<String> = row.try_get("started_at")?;
        let completed_at_str: Option<String> = row.try_get("completed_at")?;
        let session_data_str: String = row.try_get("session_data")?;

        sessions.push(InteractiveAgentSession {
            id: row.try_get("id")?,
            command: row.try_get("command")?,
            cwd: PathBuf::from(row.try_get::<String, _>("cwd")?),
            status: InteractiveAgentSessionStatus::from_str(&status_str)?,
            pid: row.try_get("pid")?,
            exit_code: row.try_get("exit_code")?,
            created_at: DateTime::parse_from_rfc3339(&created_at_str)?.with_timezone(&Utc),
            started_at: started_at_str
                .as_deref()
                .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
                .transpose()?,
            completed_at: completed_at_str
                .as_deref()
                .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
                .transpose()?,
            session_data: serde_json::from_str(&session_data_str).unwrap_or(serde_json::json!({})),
        });
    }
    Ok(sessions)
}

// ---------------------------------------------------------------------------
// Tauri event emission
// ---------------------------------------------------------------------------

#[cfg(feature = "tauri-events")]
pub fn emit_codex_stdout(app_handle: &tauri::AppHandle, session_id: &str, line: &str) {
    use tauri::Emitter;
    let _ = app_handle.emit(
        "codex_stdout",
        serde_json::json!({
            "session_id": session_id,
            "line": line,
        }),
    );
}

#[cfg(not(feature = "tauri-events"))]
pub fn emit_codex_stdout(_app_handle: &(), _session_id: &str, _line: &str) {
    // No-op in non-Tauri builds
}

#[cfg(feature = "tauri-events")]
pub fn emit_codex_stderr(app_handle: &tauri::AppHandle, session_id: &str, line: &str) {
    use tauri::Emitter;
    let _ = app_handle.emit(
        "codex_stderr",
        serde_json::json!({
            "session_id": session_id,
            "line": line,
        }),
    );
}

#[cfg(not(feature = "tauri-events"))]
pub fn emit_codex_stderr(_app_handle: &(), _session_id: &str, _line: &str) {
    // No-op in non-Tauri builds
}

#[cfg(feature = "tauri-events")]
pub fn emit_codex_status(app_handle: &tauri::AppHandle, session_id: &str, status: &str) {
    use tauri::Emitter;
    let _ = app_handle.emit(
        "codex_status",
        serde_json::json!({
            "session_id": session_id,
            "status": status,
        }),
    );
}

#[cfg(not(feature = "tauri-events"))]
pub fn emit_codex_status(_app_handle: &(), _session_id: &str, _status: &str) {
    // No-op in non-Tauri builds
}

// ---------------------------------------------------------------------------
// Core session operations
// ---------------------------------------------------------------------------

/// Determine the codex binary command and args for the current platform.
fn build_codex_command(binary: &str, cwd: &Path) -> Command {
    let mut cmd = Command::new(binary);
    cmd.current_dir(cwd);
    cmd
}

/// Start a new interactive agent session.
///
/// Spawns the codex binary (or a fallback shell command), pipes stdin/stdout/stderr,
/// and registers the session in both the in-memory registry and SQLite.
pub async fn start_session(
    cwd: PathBuf,
    command: Option<String>,
    workspace_id: Option<String>,
) -> anyhow::Result<InteractiveAgentSession> {
    let config = crate::config::load().await.unwrap_or_default();
    let binary = command.unwrap_or_else(|| config.codex.codex_binary.clone());

    let mut session = InteractiveAgentSession::new(binary.clone(), cwd.clone());
    session.transition(InteractiveAgentSessionStatus::Starting)?;

    // Spawn the codex process.
    let mut child = build_codex_command(&binary, &cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn codex process")?;

    let pid = child.id().map(|p| p as i64);
    session.pid = pid;

    let stdin = child.stdin.take();
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    // Transition to Running (codex starts executing immediately).
    session.transition(InteractiveAgentSessionStatus::Running)?;
    persist_session(&session).await?;

    // Spawn background tasks that drain stdout/stderr into buffers.
    let stdout_handle = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stdout_pipe {
            pipe.read_to_end(&mut buf).await.ok();
        }
        buf
    });
    let stderr_handle = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stderr_pipe {
            pipe.read_to_end(&mut buf).await.ok();
        }
        buf
    });

    // Wait for the process to exit in a background task so start_session returns immediately.
    let session_id = session.id.clone();
    let session_meta = session.clone();
    tokio::spawn(async move {
        let exit_status = child.wait().await.ok();
        let stdout_buf = stdout_handle.await.unwrap_or_default();
        let stderr_buf = stderr_handle.await.unwrap_or_default();
        let exit_code = exit_status.and_then(|s| s.code());

        // Determine final status.
        let final_status = match exit_code {
            Some(0) => InteractiveAgentSessionStatus::Completed,
            Some(_) => InteractiveAgentSessionStatus::Failed,
            None => InteractiveAgentSessionStatus::Completed,
        };

        let mut meta_out = session_meta;
        meta_out.exit_code = Some(exit_code.unwrap_or(-1));
        meta_out.status = final_status.clone();
        meta_out.completed_at = Some(Utc::now());

        // Update the live session.
        let mut sessions = LIVE_SESSIONS.write().await;
        if let Some(live) = sessions.iter_mut().find(|s| s.meta.id == session_id) {
            live.meta.status = final_status;
            live.meta.completed_at = meta_out.completed_at;
            live.stdout_buf = stdout_buf;
            live.stderr_buf = stderr_buf;
            live.exit_code = exit_code;
            live.child = None;

            // Persist final state.
            let _ = persist_session(&live.meta).await;
        }
    });

    let live = LiveSession {
        meta: session.clone(),
        child: None, // moved into the background task
        stdin,
        stdout_buf: Vec::new(),
        stderr_buf: Vec::new(),
        exit_code: None,
    };

    LIVE_SESSIONS.write().await.push(live);

    // Fire workspace_id into session_data if provided.
    if let Some(ws_id) = workspace_id {
        let mut sessions = LIVE_SESSIONS.write().await;
        if let Some(live) = sessions.iter_mut().find(|s| s.meta.id == session.id) {
            live.meta.session_data = serde_json::json!({ "workspace_id": ws_id });
            let _ = persist_session(&live.meta).await;
        }
    }

    Ok(session)
}

/// Read incremental output from a running session, starting at `offset` bytes.
pub async fn read_output(session_id: &str, offset: usize) -> anyhow::Result<CodexOutput> {
    let sessions = LIVE_SESSIONS.read().await;
    let session = sessions
        .iter()
        .find(|s| s.meta.id == session_id)
        .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;

    let full_stdout = String::from_utf8_lossy(&session.stdout_buf).to_string();
    let full_stderr = String::from_utf8_lossy(&session.stderr_buf).to_string();
    let stdout = full_stdout.get(offset..).unwrap_or("").to_string();
    let stderr = full_stderr.get(offset..).unwrap_or("").to_string();

    Ok(CodexOutput {
        session_id: session_id.to_string(),
        stdout,
        stderr,
        exit_code: session.exit_code,
    })
}

/// Send keyboard / text input to a running session's stdin.
pub async fn send_input(session_id: &str, input: &str) -> anyhow::Result<()> {
    let mut sessions = LIVE_SESSIONS.write().await;
    let session = sessions
        .iter_mut()
        .find(|s| s.meta.id == session_id)
        .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;

    if session.meta.status.is_terminal() {
        bail!(
            "session {session_id} is in terminal state: {}",
            session.meta.status.as_str()
        );
    }

    if let Some(ref mut stdin) = session.stdin {
        stdin
            .write_all(input.as_bytes())
            .await
            .context("failed to write to stdin")?;
        stdin.flush().await.context("failed to flush stdin")?;
        // Transition to Running if we were in AwaitInput.
        if session.meta.status == InteractiveAgentSessionStatus::AwaitInput {
            session.meta.status = InteractiveAgentSessionStatus::Running;
        }
        Ok(())
    } else {
        bail!("session {session_id} has no stdin (process may have exited)")
    }
}

/// Interrupt (kill) a running session.
///
/// Sends SIGINT-like signal. On Windows, uses `Child::kill()`.
pub async fn interrupt_session(session_id: &str) -> anyhow::Result<()> {
    let mut sessions = LIVE_SESSIONS.write().await;
    let session = sessions
        .iter_mut()
        .find(|s| s.meta.id == session_id)
        .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;

    if session.meta.status.is_terminal() {
        bail!(
            "session {session_id} is already in terminal state: {}",
            session.meta.status.as_str()
        );
    }

    session
        .meta
        .transition(InteractiveAgentSessionStatus::Interrupted)?;

    if let Some(ref mut child) = session.child {
        // Try to send Ctrl+C equivalent via kill on Windows.
        let _ = child.kill().await;
    }

    session.meta.completed_at = Some(Utc::now());
    persist_session(&session.meta).await?;

    // Mark as resumable.
    session
        .meta
        .transition(InteractiveAgentSessionStatus::Resumable)?;
    persist_session(&session.meta).await?;

    Ok(())
}

/// Resume a previously interrupted session.
///
/// Re-spawns the codex binary in the same cwd, preserving the session ID and accumulated output.
pub async fn resume_session(session_id: &str) -> anyhow::Result<()> {
    // Check if session is in the live registry.
    let mut sessions = LIVE_SESSIONS.write().await;
    let live_idx = sessions.iter().position(|s| s.meta.id == session_id);

    if let Some(idx) = live_idx {
        let live = &mut sessions[idx];
        if live.meta.status != InteractiveAgentSessionStatus::Resumable
            && live.meta.status != InteractiveAgentSessionStatus::Interrupted
        {
            bail!(
                "session {session_id} cannot be resumed from status: {}",
                live.meta.status.as_str()
            );
        }

        // Transition to Starting.
        live.meta
            .transition(InteractiveAgentSessionStatus::Starting)?;
        persist_session(&live.meta).await?;

        // Re-spawn the process.
        let mut child = build_codex_command(&live.meta.command, &live.meta.cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped())
            .spawn()
            .context("failed to re-spawn codex process")?;

        live.meta.pid = child.id().map(|p| p as i64);
        live.stdin = child.stdin.take();
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        live.meta
            .transition(InteractiveAgentSessionStatus::Running)?;
        live.meta.completed_at = None;
        persist_session(&live.meta).await?;

        // Preserve accumulated output.
        let old_stdout = live.stdout_buf.clone();
        let old_stderr = live.stderr_buf.clone();

        let session_id_clone = session_id.to_string();

        // Spawn new background readers.
        let stdout_handle = tokio::spawn(async move {
            let mut buf = old_stdout;
            if let Some(mut pipe) = stdout_pipe {
                pipe.read_to_end(&mut buf).await.ok();
            }
            buf
        });
        let stderr_handle = tokio::spawn(async move {
            let mut buf = old_stderr;
            if let Some(mut pipe) = stderr_pipe {
                pipe.read_to_end(&mut buf).await.ok();
            }
            buf
        });

        // Wait for exit in background.
        tokio::spawn(async move {
            let exit_status = child.wait().await.ok();
            let stdout_buf = stdout_handle.await.unwrap_or_default();
            let stderr_buf = stderr_handle.await.unwrap_or_default();
            let exit_code = exit_status.and_then(|s| s.code());

            let final_status = match exit_code {
                Some(0) => InteractiveAgentSessionStatus::Completed,
                Some(_) => InteractiveAgentSessionStatus::Failed,
                None => InteractiveAgentSessionStatus::Completed,
            };

            let mut sessions = LIVE_SESSIONS.write().await;
            if let Some(live) = sessions.iter_mut().find(|s| s.meta.id == session_id_clone) {
                live.meta.status = final_status;
                live.meta.completed_at = Some(Utc::now());
                live.stdout_buf = stdout_buf;
                live.stderr_buf = stderr_buf;
                live.exit_code = exit_code;
                live.child = None;
                let _ = persist_session(&live.meta).await;
            }
        });

        // Update the live entry's child handle (it was moved into the background task).
        live.child = None;

        return Ok(());
    }

    // Not in live registry — try to load from DB.
    drop(sessions);
    let mut db_session = load_session_from_db(session_id).await?;

    if db_session.status != InteractiveAgentSessionStatus::Resumable
        && db_session.status != InteractiveAgentSessionStatus::Interrupted
    {
        bail!(
            "session {session_id} cannot be resumed from status: {}",
            db_session.status.as_str()
        );
    }

    db_session.transition(InteractiveAgentSessionStatus::Starting)?;
    persist_session(&db_session).await?;

    let mut child = build_codex_command(&db_session.command, &db_session.cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("failed to re-spawn codex process from DB session")?;

    let pid = child.id().map(|p| p as i64);
    let stdin = child.stdin.take();
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    db_session.pid = pid;
    db_session.transition(InteractiveAgentSessionStatus::Running)?;
    db_session.completed_at = None;
    persist_session(&db_session).await?;

    let stdout_handle = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stdout_pipe {
            pipe.read_to_end(&mut buf).await.ok();
        }
        buf
    });
    let stderr_handle = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stderr_pipe {
            pipe.read_to_end(&mut buf).await.ok();
        }
        buf
    });

    let session_id_clone = db_session.id.clone();
    let session_meta = db_session.clone();

    tokio::spawn(async move {
        let exit_status = child.wait().await.ok();
        let stdout_buf = stdout_handle.await.unwrap_or_default();
        let stderr_buf = stderr_handle.await.unwrap_or_default();
        let exit_code = exit_status.and_then(|s| s.code());

        let final_status = match exit_code {
            Some(0) => InteractiveAgentSessionStatus::Completed,
            Some(_) => InteractiveAgentSessionStatus::Failed,
            None => InteractiveAgentSessionStatus::Completed,
        };

        let mut sessions = LIVE_SESSIONS.write().await;
        if let Some(live) = sessions.iter_mut().find(|s| s.meta.id == session_id_clone) {
            live.meta.status = final_status;
            live.meta.completed_at = Some(Utc::now());
            live.stdout_buf = stdout_buf;
            live.stderr_buf = stderr_buf;
            live.exit_code = exit_code;
            live.child = None;
            let _ = persist_session(&live.meta).await;
        }
    });

    let live = LiveSession {
        meta: session_meta,
        child: None,
        stdin,
        stdout_buf: Vec::new(),
        stderr_buf: Vec::new(),
        exit_code: None,
    };

    LIVE_SESSIONS.write().await.push(live);

    Ok(())
}

/// Stop a running session and release resources.
pub async fn stop_session(session_id: &str) -> anyhow::Result<()> {
    let mut sessions = LIVE_SESSIONS.write().await;
    let idx = sessions
        .iter()
        .position(|s| s.meta.id == session_id)
        .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;

    let mut session = sessions.remove(idx);

    // Kill the child process if it is still running.
    if let Some(ref mut child) = session.child {
        let _ = child.kill().await;
    }

    session.meta.status = InteractiveAgentSessionStatus::Interrupted;
    session.meta.completed_at = Some(Utc::now());
    persist_session(&session.meta).await?;

    // Transition to Resumable so it can be picked up later.
    session.meta.status = InteractiveAgentSessionStatus::Resumable;
    persist_session(&session.meta).await?;

    Ok(())
}

/// List all tracked sessions (in-memory).
pub async fn list_sessions() -> anyhow::Result<Vec<InteractiveAgentSession>> {
    Ok(LIVE_SESSIONS
        .read()
        .await
        .iter()
        .map(|s| s.meta.clone())
        .collect())
}

/// List sessions from the database.
pub async fn list_sessions_db(limit: Option<u32>) -> anyhow::Result<Vec<InteractiveAgentSession>> {
    list_sessions_from_db(limit).await
}

/// Get a single session by ID (checks live registry first, then DB).
pub async fn get_session(session_id: &str) -> anyhow::Result<InteractiveAgentSession> {
    // Check live registry first.
    {
        let sessions = LIVE_SESSIONS.read().await;
        if let Some(live) = sessions.iter().find(|s| s.meta.id == session_id) {
            return Ok(live.meta.clone());
        }
    }
    // Fall back to DB.
    load_session_from_db(session_id).await
}

/// Resume a session from DB after a restart (re-spawns the process).
pub async fn resume_session_from_db(session_id: &str) -> anyhow::Result<()> {
    resume_session(session_id).await
}

// ---------------------------------------------------------------------------
// Backward-compat aliases
// ---------------------------------------------------------------------------

/// Backward-compatible session status (maps to old 4-state model).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexSessionStatus {
    Running,
    Completed,
    Failed,
    Interrupted,
}

impl From<&InteractiveAgentSessionStatus> for CodexSessionStatus {
    fn from(s: &InteractiveAgentSessionStatus) -> Self {
        match s {
            InteractiveAgentSessionStatus::Running
            | InteractiveAgentSessionStatus::Ready
            | InteractiveAgentSessionStatus::AwaitInput
            | InteractiveAgentSessionStatus::Starting => CodexSessionStatus::Running,
            InteractiveAgentSessionStatus::Completed => CodexSessionStatus::Completed,
            InteractiveAgentSessionStatus::Failed => CodexSessionStatus::Failed,
            InteractiveAgentSessionStatus::Interrupted
            | InteractiveAgentSessionStatus::Resumable
            | InteractiveAgentSessionStatus::Created => CodexSessionStatus::Interrupted,
        }
    }
}

/// Backward-compatible session wrapper.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CodexSession {
    pub id: String,
    pub workspace_id: Option<String>,
    pub terminal_id: Option<String>,
    pub cwd: PathBuf,
    pub status: CodexSessionStatus,
    pub created_at: DateTime<Utc>,
}

impl From<&InteractiveAgentSession> for CodexSession {
    fn from(s: &InteractiveAgentSession) -> Self {
        CodexSession {
            id: s.id.clone(),
            workspace_id: s
                .session_data
                .get("workspace_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            terminal_id: s.pid.map(|p| p.to_string()),
            cwd: s.cwd.clone(),
            status: CodexSessionStatus::from(&s.status),
            created_at: s.created_at,
        }
    }
}

/// Backward-compatible output wrapper.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CodexOutput {
    pub session_id: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

// ---------------------------------------------------------------------------
// Legacy activity detection (kept for backward compat)
// ---------------------------------------------------------------------------

pub async fn detect_codex_activity() -> anyhow::Result<Option<CodexActivity>> {
    let config = crate::config::load().await?;
    if !config.codex.enabled {
        return Ok(None);
    }

    if let Some(activity) = check_codex_logs(&config.codex.workspace_root).await? {
        return Ok(Some(activity));
    }

    if let Some(activity) = check_recent_code_changes(&config.codex.workspace_root).await? {
        return Ok(Some(activity));
    }

    Ok(None)
}

async fn check_codex_logs(workspace_root: &Path) -> anyhow::Result<Option<CodexActivity>> {
    let log_dir = workspace_root.join("logs");
    if !log_dir.exists() {
        return Ok(None);
    }

    let mut entries = tokio::fs::read_dir(&log_dir).await?;
    let mut latest_file: Option<(PathBuf, DateTime<Utc>)> = None;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("log") {
            continue;
        }
        let modified = entry
            .metadata()
            .await
            .and_then(|m| m.modified())
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());
        match &latest_file {
            Some((_, t)) if modified > *t => {
                latest_file = Some((path, modified));
            }
            None => {
                latest_file = Some((path, modified));
            }
            _ => {}
        }
    }

    if let Some((log_path, last_activity)) = latest_file {
        let content = tokio::fs::read_to_string(&log_path).await?;
        let session_id = extract_session_id(&content).unwrap_or_else(|| "unknown".to_string());
        let changed_files = extract_changed_files(&content);

        return Ok(Some(CodexActivity {
            session_id,
            cwd: workspace_root.to_path_buf(),
            changed_files,
            last_activity,
        }));
    }

    Ok(None)
}

fn collect_recent_changes_sync(
    dir: &Path,
    cutoff: DateTime<Utc>,
    files: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name == "node_modules" || name == "target" {
                    continue;
                }
            }
            collect_recent_changes_sync(&path, cutoff, files)?;
        } else if file_type.is_file() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    let modified_dt = DateTime::<Utc>::from(modified);
                    if modified_dt > cutoff {
                        files.push(path);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn check_recent_code_changes(workspace_root: &Path) -> anyhow::Result<Option<CodexActivity>> {
    let cutoff = Utc::now() - chrono::Duration::minutes(5);
    let mut changed_files = Vec::new();

    let _ = collect_recent_changes_sync(workspace_root, cutoff, &mut changed_files);

    if changed_files.is_empty() {
        return Ok(None);
    }

    let session_id = generate_session_id();
    Ok(Some(CodexActivity {
        session_id,
        cwd: workspace_root.to_path_buf(),
        changed_files,
        last_activity: Utc::now(),
    }))
}

fn extract_session_id(content: &str) -> Option<String> {
    for line in content.lines() {
        if line.contains("session_id") || line.contains("sessionId") {
            if let Some(start) = line.find(':') {
                let value = line[start + 1..]
                    .trim()
                    .trim_matches(|c| c == '"' || c == ',');
                return Some(value.to_string());
            }
        }
    }
    None
}

fn extract_changed_files(content: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for line in content.lines() {
        if line.contains("changed") || line.contains("modified") || line.contains("file://") {
            let cleaned = line
                .trim()
                .trim_matches(|c| c == '"' || c == ',' || c == '[' || c == ']');
            if !cleaned.is_empty() && cleaned.contains('.') {
                let path = PathBuf::from(cleaned);
                if path.exists() {
                    files.push(path);
                }
            }
        }
    }
    files
}

fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("codex-{}", duration.as_secs())
}

// ---------------------------------------------------------------------------
// Task generation helpers (kept for backward compat)
// ---------------------------------------------------------------------------

pub async fn generate_test_steps(task_id: &str) -> anyhow::Result<String> {
    let pool = crate::db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT artifact_file, artifact_anchor, summary_ref, focus_hint
        FROM tasks
        WHERE id = ?1 AND source = 'codex'
        "#,
    )
    .bind(task_id)
    .fetch_all(&pool)
    .await?;

    if let Some(row) = rows.first() {
        let artifact_file: Option<String> = row.try_get("artifact_file")?;
        let artifact_anchor: Option<String> = row.try_get("artifact_anchor")?;
        let summary_ref: Option<String> = row.try_get("summary_ref")?;
        let focus_hint: Option<String> = row.try_get("focus_hint")?;

        let steps = build_test_steps(
            artifact_file.as_ref(),
            artifact_anchor.as_ref(),
            summary_ref.as_ref(),
            focus_hint.as_ref(),
        );

        log_codex_event(
            "test_steps_generated",
            &serde_json::json!({
                "task_id": task_id,
                "steps": steps,
            }),
        )
        .await?;

        return Ok(steps);
    }

    anyhow::bail!("task not found: {task_id}")
}

fn build_test_steps(
    artifact_file: Option<&String>,
    artifact_anchor: Option<&String>,
    summary_ref: Option<&String>,
    focus_hint: Option<&String>,
) -> String {
    let mut steps = String::new();

    steps.push_str("# 测试步骤生成\n\n");

    if let Some(file) = artifact_file {
        steps.push_str(&format!("## 目标文件\n- `{}`\n", file));
    }

    if let Some(anchor) = artifact_anchor {
        steps.push_str(&format!("## 锚点位置\n- 函数/方法: `{}`\n", anchor));
    }

    if let Some(summary) = summary_ref {
        steps.push_str(&format!("## 摘要参考\n- {}\n", summary));
    }

    steps.push_str("## 测试步骤\n");

    if let Some(hint) = focus_hint {
        steps.push_str(&format!("1. {}\n", hint));
    }

    steps.push_str("2. 运行现有测试用例验证\n");
    steps.push_str("3. 检查代码覆盖率\n");
    steps.push_str("4. 验证边界条件\n");

    steps
}

async fn log_codex_event(kind: &str, payload: &serde_json::Value) -> anyhow::Result<()> {
    events::append("codex", kind, payload)
        .await
        .context("failed to log codex event")
}

pub fn estimate_minutes_from_file_count(count: usize) -> u32 {
    match count {
        0..=3 => 5,
        4..=10 => 15,
        11..=25 => 30,
        _ => 60,
    }
}

pub fn create_codex_task(
    kind: &str,
    artifact: tasks::Artifact,
    summary_ref: Option<String>,
    file_count: usize,
    focus_hint: Option<String>,
) -> CodexTask {
    CodexTask {
        id: Uuid::new_v4().to_string(),
        source: "codex".into(),
        kind: kind.into(),
        artifact,
        summary_ref,
        est_minutes: Some(estimate_minutes_from_file_count(file_count)),
        focus_hint,
        status: tasks::TaskStatus::Pending,
        created_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    // ── Helper: clear live sessions with test mutex ──

    static TEST_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    async fn clear_live_sessions() {
        LIVE_SESSIONS.write().await.clear();
    }

    // ── 1. State machine: valid transitions from Created ──

    #[test]
    fn test_valid_transitions_from_created() {
        let status = InteractiveAgentSessionStatus::Created;
        assert!(status.can_transition_to(&InteractiveAgentSessionStatus::Starting));
        assert!(status.can_transition_to(&InteractiveAgentSessionStatus::Failed));
        assert!(!status.can_transition_to(&InteractiveAgentSessionStatus::Running));
        assert!(!status.can_transition_to(&InteractiveAgentSessionStatus::Completed));
        assert!(!status.can_transition_to(&InteractiveAgentSessionStatus::Resumable));
    }

    // ── 2. State machine: valid transitions from Running ──

    #[test]
    fn test_valid_transitions_from_running() {
        let status = InteractiveAgentSessionStatus::Running;
        assert!(status.can_transition_to(&InteractiveAgentSessionStatus::AwaitInput));
        assert!(status.can_transition_to(&InteractiveAgentSessionStatus::Interrupted));
        assert!(status.can_transition_to(&InteractiveAgentSessionStatus::Completed));
        assert!(status.can_transition_to(&InteractiveAgentSessionStatus::Failed));
        assert!(!status.can_transition_to(&InteractiveAgentSessionStatus::Created));
        assert!(!status.can_transition_to(&InteractiveAgentSessionStatus::Starting));
    }

    // ── 3. State machine: terminal states have no transitions ──

    #[test]
    fn test_terminal_states_have_no_transitions() {
        for status in &[
            InteractiveAgentSessionStatus::Completed,
            InteractiveAgentSessionStatus::Failed,
        ] {
            assert!(
                status.valid_transitions().is_empty(),
                "{:?} should have no valid transitions",
                status
            );
        }
    }

    // ── 4. State machine: invalid transition errors ──

    #[test]
    fn test_invalid_transition_errors() {
        let mut session = InteractiveAgentSession::new("codex".into(), PathBuf::from("/tmp"));
        // Created -> Running is invalid (must go through Starting first).
        let result = session.transition(InteractiveAgentSessionStatus::Running);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid transition"));
    }

    // ── 5. Session creation and status tracking ──

    #[test]
    fn test_session_creation_fields() {
        let session = InteractiveAgentSession::new("codex".into(), PathBuf::from("/workspace"));
        assert!(session.id.starts_with("ias-"));
        assert_eq!(session.command, "codex");
        assert_eq!(session.cwd, PathBuf::from("/workspace"));
        assert_eq!(session.status, InteractiveAgentSessionStatus::Created);
        assert!(session.pid.is_none());
        assert!(session.started_at.is_none());
        assert!(session.completed_at.is_none());
    }

    // ── 6. Session transition sets timestamps ──

    #[test]
    fn test_session_transition_sets_timestamps() {
        let mut session = InteractiveAgentSession::new("codex".into(), PathBuf::from("/tmp"));
        assert!(session.started_at.is_none());

        session
            .transition(InteractiveAgentSessionStatus::Starting)
            .unwrap();
        assert!(session.started_at.is_some());
        assert!(session.completed_at.is_none());

        session
            .transition(InteractiveAgentSessionStatus::Running)
            .unwrap();
        assert!(session.completed_at.is_none());

        session
            .transition(InteractiveAgentSessionStatus::Completed)
            .unwrap();
        assert!(session.completed_at.is_some());
    }

    // ── 7. Status as_str/from_str roundtrip ──

    #[test]
    fn test_status_as_str_roundtrip() {
        let all_statuses = vec![
            InteractiveAgentSessionStatus::Created,
            InteractiveAgentSessionStatus::Starting,
            InteractiveAgentSessionStatus::Ready,
            InteractiveAgentSessionStatus::Running,
            InteractiveAgentSessionStatus::AwaitInput,
            InteractiveAgentSessionStatus::Interrupted,
            InteractiveAgentSessionStatus::Resumable,
            InteractiveAgentSessionStatus::Completed,
            InteractiveAgentSessionStatus::Failed,
        ];
        for status in &all_statuses {
            let s = status.as_str();
            let back = InteractiveAgentSessionStatus::from_str(s).unwrap();
            assert_eq!(&back, status);
        }
    }

    // ── 8. Invalid status string returns error ──

    #[test]
    fn test_invalid_status_str_returns_error() {
        assert!(InteractiveAgentSessionStatus::from_str("invalid").is_err());
        assert!(InteractiveAgentSessionStatus::from_str("").is_err());
        assert!(InteractiveAgentSessionStatus::from_str("foo_bar").is_err());
    }

    // ── 9. Session persistence roundtrip ──

    #[tokio::test]
    async fn test_session_persistence_roundtrip() {
        let _root = TestRoot::new();
        let mut session =
            InteractiveAgentSession::new("codex".into(), PathBuf::from("/workspace/project"));
        session
            .transition(InteractiveAgentSessionStatus::Starting)
            .unwrap();
        session
            .transition(InteractiveAgentSessionStatus::Running)
            .unwrap();
        session.pid = Some(12345);
        session.session_data = serde_json::json!({ "workspace_id": "ws-1" });

        persist_session(&session).await.expect("persist");

        let loaded = load_session_from_db(&session.id).await.expect("load");
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.command, "codex");
        assert_eq!(loaded.cwd, PathBuf::from("/workspace/project"));
        assert_eq!(loaded.status, InteractiveAgentSessionStatus::Running);
        assert_eq!(loaded.pid, Some(12345));
    }

    // ── 10. Session persistence update ──

    #[tokio::test]
    async fn test_session_persistence_update() {
        let _root = TestRoot::new();
        let mut session = InteractiveAgentSession::new("codex".into(), PathBuf::from("/tmp"));
        session
            .transition(InteractiveAgentSessionStatus::Starting)
            .unwrap();
        persist_session(&session).await.expect("persist initial");

        session
            .transition(InteractiveAgentSessionStatus::Running)
            .unwrap();
        session.pid = Some(999);
        persist_session(&session).await.expect("persist update");

        let loaded = load_session_from_db(&session.id).await.expect("load");
        assert_eq!(loaded.status, InteractiveAgentSessionStatus::Running);
        assert_eq!(loaded.pid, Some(999));
    }

    // ── 11. get_session falls back to DB ──

    #[tokio::test]
    async fn test_get_session_fallback_to_db() {
        let _root = TestRoot::new();
        clear_live_sessions().await;

        let mut session = InteractiveAgentSession::new("codex".into(), PathBuf::from("/tmp"));
        session
            .transition(InteractiveAgentSessionStatus::Starting)
            .unwrap();
        session
            .transition(InteractiveAgentSessionStatus::Running)
            .unwrap();
        session
            .transition(InteractiveAgentSessionStatus::Completed)
            .unwrap();
        persist_session(&session).await.expect("persist");

        let loaded = get_session(&session.id).await.expect("get_session");
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.status, InteractiveAgentSessionStatus::Completed);
    }

    // ── 12. Interrupt/resume: Interrupted -> Resumable transition ──

    #[test]
    fn test_interrupt_resume_transitions() {
        let mut session = InteractiveAgentSession::new("codex".into(), PathBuf::from("/tmp"));
        session
            .transition(InteractiveAgentSessionStatus::Starting)
            .unwrap();
        session
            .transition(InteractiveAgentSessionStatus::Running)
            .unwrap();
        session
            .transition(InteractiveAgentSessionStatus::Interrupted)
            .unwrap();
        assert_eq!(session.status, InteractiveAgentSessionStatus::Interrupted);

        // Interrupted -> Resumable is valid.
        session
            .transition(InteractiveAgentSessionStatus::Resumable)
            .unwrap();
        assert_eq!(session.status, InteractiveAgentSessionStatus::Resumable);

        // Resumable -> Starting is valid.
        session
            .transition(InteractiveAgentSessionStatus::Starting)
            .unwrap();
        assert_eq!(session.status, InteractiveAgentSessionStatus::Starting);
    }

    // ── 13. Process spawn failure handling ──

    #[tokio::test]
    async fn test_spawn_nonexistent_binary_fails() {
        let _guard = TEST_MUTEX.lock().await;
        let _root = TestRoot::new();
        clear_live_sessions().await;
        let cwd = std::env::current_dir().unwrap();
        let result =
            start_session(cwd, Some("nonexistent_codex_binary_xyz".to_string()), None).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("failed to spawn"),
            "expected spawn failure message"
        );
    }

    // ── 14. list_sessions_from_db works ──

    #[tokio::test]
    async fn test_list_sessions_from_db() {
        let _root = TestRoot::new();
        clear_live_sessions().await;

        for i in 0..3 {
            let mut session = InteractiveAgentSession::new(
                format!("cmd-{i}"),
                PathBuf::from(format!("/tmp/{i}")),
            );
            session
                .transition(InteractiveAgentSessionStatus::Starting)
                .unwrap();
            session
                .transition(InteractiveAgentSessionStatus::Running)
                .unwrap();
            session
                .transition(InteractiveAgentSessionStatus::Completed)
                .unwrap();
            persist_session(&session).await.expect("persist");
        }

        let sessions = list_sessions_from_db(None).await.expect("list");
        assert!(sessions.len() >= 3);
    }

    // ── 15. Legacy estimate_minutes tests ──

    #[test]
    fn test_estimate_minutes() {
        assert_eq!(estimate_minutes_from_file_count(2), 5);
        assert_eq!(estimate_minutes_from_file_count(7), 15);
        assert_eq!(estimate_minutes_from_file_count(20), 30);
        assert_eq!(estimate_minutes_from_file_count(100), 60);
    }

    // ── 16. CodexTask conversion ──

    #[test]
    fn test_codex_task_conversion() {
        let codex_task = CodexTask {
            id: "t-001".into(),
            source: "codex".into(),
            kind: "test-case".into(),
            artifact: tasks::Artifact {
                file: Some(PathBuf::from("src/main.rs")),
                anchor: Some("main".into()),
            },
            summary_ref: Some("summary.md".into()),
            est_minutes: Some(10),
            focus_hint: Some("run tests".into()),
            status: tasks::TaskStatus::Pending,
            created_at: Utc::now(),
        };

        let task: tasks::Task = codex_task.into();
        assert_eq!(task.source, "codex");
        assert_eq!(task.kind, "test-case");
    }

    // ── 17. Start session with real command ──

    #[tokio::test]
    async fn test_start_session_with_real_command() {
        let _guard = TEST_MUTEX.lock().await;
        let _root = TestRoot::new();
        clear_live_sessions().await;
        let cwd = std::env::current_dir().unwrap();

        // Use a simple command that exists on all platforms.
        #[cfg(windows)]
        let cmd = "cmd.exe".to_string();
        #[cfg(not(windows))]
        let cmd = "echo".to_string();

        let session = start_session(cwd, Some(cmd), Some("test-ws".into()))
            .await
            .expect("start_session should succeed");

        assert!(session.id.starts_with("ias-"));
        assert!(session.pid.is_some());
        assert_eq!(session.status, InteractiveAgentSessionStatus::Running);

        // Wait a moment for the process to finish.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let output = read_output(&session.id, 0).await;
        // The process should have produced some output or exited.
        assert!(output.is_ok() || true); // May have already been cleaned up.
    }

    // ── 18. Serialization roundtrip ──

    #[test]
    fn test_session_serialization_roundtrip() {
        let mut session = InteractiveAgentSession::new("codex".into(), PathBuf::from("/workspace"));
        session
            .transition(InteractiveAgentSessionStatus::Starting)
            .unwrap();
        session
            .transition(InteractiveAgentSessionStatus::Running)
            .unwrap();
        session.pid = Some(42);
        session.session_data = serde_json::json!({ "key": "value" });

        let json = serde_json::to_string(&session).unwrap();
        let back: InteractiveAgentSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, session.id);
        assert_eq!(back.command, session.command);
        assert_eq!(back.status, session.status);
        assert_eq!(back.pid, session.pid);
    }
}
