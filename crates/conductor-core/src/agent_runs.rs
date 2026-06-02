use crate::{db, paths::Paths, tasks};
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::{
    path::PathBuf,
    process::{Child, Stdio},
    time::{Duration, Instant},
};
use tokio::{fs, io::AsyncWriteExt, process::Command as TokioCommand};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Stopped,
}

impl AgentRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "stopped" => Ok(Self::Stopped),
            other => bail!("unknown agent run status: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentRun {
    pub id: String,
    pub agent_id: String,
    pub role: String,
    pub workspace_id: Option<String>,
    pub cwd: Option<PathBuf>,
    pub status: AgentRunStatus,
    pub pid: Option<i64>,
    pub command_json: Option<serde_json::Value>,
    pub input_ref: Option<String>,
    pub output_ref: Option<String>,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub metadata_json: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StartAgentRunInput {
    #[serde(default = "default_agent_id")]
    pub agent_id: String,
    #[serde(default = "default_role")]
    pub role: String,
    pub workspace_id: Option<String>,
    pub cwd: Option<PathBuf>,
    pub prompt: String,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AgentRunFilter {
    pub workspace_id: Option<String>,
    pub agent_id: Option<String>,
    pub status: Option<AgentRunStatus>,
    pub limit: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentRunOutput {
    pub run: AgentRun,
    pub stdout: String,
    pub stderr: String,
    pub output_ref: Option<String>,
}

fn default_agent_id() -> String {
    "claude".to_string()
}

fn default_role() -> String {
    "assistant".to_string()
}

fn default_timeout_seconds() -> u64 {
    300
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect()
    }
}

pub async fn start_claude_run(input: StartAgentRunInput) -> anyhow::Result<AgentRun> {
    if input.agent_id.trim().is_empty() {
        bail!("agent_id cannot be empty");
    }
    if input.prompt.trim().is_empty() {
        bail!("prompt cannot be empty");
    }

    let run_id = next_run_id();
    let started_at = Utc::now();
    let input_ref = write_run_input(&run_id, &input).await?;
    let output_ref = output_path_for_run(&run_id).display().to_string();
    let command_json = serde_json::json!({
        "program": "claude",
        "args": ["-p", input.prompt],
        "timeout_seconds": input.timeout_seconds.clamp(1, 86_400),
    });
    let run = AgentRun {
        id: run_id.clone(),
        agent_id: input.agent_id,
        role: input.role,
        workspace_id: input.workspace_id,
        cwd: input.cwd,
        status: AgentRunStatus::Queued,
        pid: None,
        command_json: Some(command_json),
        input_ref: Some(input_ref),
        output_ref: Some(output_ref),
        error: None,
        started_at,
        updated_at: started_at,
        finished_at: None,
        metadata_json: input.metadata,
    };

    upsert(&run).await?;
    crate::events::emit_agent_run_created(&run.id, &run.agent_id, run.status.as_str()).await;
    spawn_claude(run, &input.prompt, input.timeout_seconds.clamp(1, 86_400)).await
}

async fn spawn_claude(
    mut run: AgentRun,
    prompt: &str,
    timeout_seconds: u64,
) -> anyhow::Result<AgentRun> {
    fs::create_dir_all(Paths::agent_runs_dir()).await?;
    let stdout_path = stdout_path_for_run(&run.id);
    let stderr_path = stderr_path_for_run(&run.id);
    let stdout_file = std::fs::File::create(&stdout_path)
        .with_context(|| format!("create {}", stdout_path.display()))?;
    let stderr_file = std::fs::File::create(&stderr_path)
        .with_context(|| format!("create {}", stderr_path.display()))?;

    let mut cmd = std::process::Command::new("claude");
    cmd.arg("-p")
        .arg(prompt)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));
    if let Some(cwd) = &run.cwd {
        cmd.current_dir(cwd);
    }

    let child = cmd.spawn().context("spawn claude -p")?;
    let pid = child.id();

    run.status = AgentRunStatus::Running;
    run.pid = Some(pid as i64);
    run.updated_at = Utc::now();
    upsert(&run).await?;
    crate::events::emit_agent_run_phase_changed(&run.id, "queued", "running").await;

    let run_id = run.id.clone();
    std::thread::spawn(move || {
        let finish = wait_for_child(child, Duration::from_secs(timeout_seconds));
        if let Ok(runtime) = tokio::runtime::Runtime::new() {
            let _ = runtime.block_on(finish_spawned_run(&run_id, finish));
        }
    });

    Ok(run)
}

enum SpawnFinish {
    Status(std::process::ExitStatus),
    IoError(String),
    TimedOut,
}

fn wait_for_child(mut child: Child, timeout: Duration) -> SpawnFinish {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return SpawnFinish::Status(status),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return SpawnFinish::TimedOut;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(err) => return SpawnFinish::IoError(err.to_string()),
        }
    }
}

async fn finish_spawned_run(run_id: &str, finish: SpawnFinish) -> anyhow::Result<()> {
    let mut run = get(run_id).await?;
    if run.status == AgentRunStatus::Stopped {
        return Ok(());
    }

    let now = Utc::now();
    match finish {
        SpawnFinish::Status(status) => {
            let stdout = read_sidecar(&stdout_path_for_run(run_id)).await?;
            let stderr = read_sidecar(&stderr_path_for_run(run_id)).await?;
            let output_ref = write_run_output(run_id, &stdout, &stderr).await?;
            run.output_ref = Some(output_ref);
            run.status = if status.success() {
                AgentRunStatus::Succeeded
            } else {
                AgentRunStatus::Failed
            };
            if !status.success() {
                run.error = Some(format!("claude exited with {status}"));
            }
        }
        SpawnFinish::IoError(err) => {
            run.status = AgentRunStatus::Failed;
            run.error = Some(err.clone());
            let output_ref = write_run_output(run_id, "", &err).await?;
            run.output_ref = Some(output_ref);
        }
        SpawnFinish::TimedOut => {
            run.status = AgentRunStatus::Failed;
            run.error = Some("claude timed out".to_string());
            let output_ref =
                write_run_output(run_id, "", &run.error.clone().unwrap_or_default()).await?;
            run.output_ref = Some(output_ref);
        }
    }
    run.updated_at = now;
    run.finished_at = Some(now);
    upsert(&run).await?;

    // Writeback: if the run has a linked task_id in metadata, update the task
    if let Some(task_id) = run
        .metadata_json
        .as_ref()
        .and_then(|m| m.get("task_id"))
        .and_then(|v| v.as_str())
    {
        let task_id = task_id.to_string();
        let output_summary = run.error.clone().or_else(|| {
            run.output_ref
                .as_ref()
                .map(|_| format!("Run {} completed with status {:?}", run.id, run.status))
        });
        let new_status = match run.status {
            AgentRunStatus::Succeeded => "completed",
            AgentRunStatus::Failed | AgentRunStatus::Stopped => "pending", // return to pending for review
            _ => return Ok(()),
        };
        // Best-effort task update — don't fail the run if task update fails
        let _ = crate::tasklist::update_task_status_by_id(
            &task_id,
            new_status,
            output_summary.as_deref(),
        )
        .await;
    } else if run.status == AgentRunStatus::Succeeded || run.status == AgentRunStatus::Failed {
        // No linked task — auto-create an AgentTask for review
        let prompt_summary = run
            .metadata_json
            .as_ref()
            .and_then(|m| m.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("Agent run completed");
        let subject = if prompt_summary.chars().count() > 80 {
            format!("{}...", truncate_chars(prompt_summary, 80))
        } else {
            prompt_summary.to_string()
        };
        let description = format!(
            "AgentRun {} ({}) — status: {:?}, output: {}",
            run.id,
            run.agent_id,
            run.status,
            run.output_ref.as_deref().unwrap_or("none")
        );
        let _ = crate::tasklist::create_task(crate::tasklist::TaskCreateInput {
            subject,
            description,
            source: Some("agent_run".to_string()),
            kind: Some("agent-review".to_string()),
            metadata: Some(serde_json::json!({
                "agent_run_id": run.id,
                "agent_id": run.agent_id,
                "output_ref": run.output_ref,
                "status": format!("{:?}", run.status),
            })),
            ..Default::default()
        })
        .await;
    }

    Ok(())
}

pub async fn list(filter: AgentRunFilter) -> anyhow::Result<Vec<AgentRun>> {
    let pool = db::pool().await?;
    let mut rows = sqlx::query(
        r#"
        SELECT id, agent_id, role, workspace_id, cwd, status, pid, command_json,
               input_ref, output_ref, error, started_at, updated_at, finished_at, metadata_json
        FROM agent_runs
        ORDER BY updated_at DESC
        LIMIT ?1
        "#,
    )
    .bind(filter.limit.unwrap_or(20).clamp(1, 200) as i64)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(row_to_agent_run)
    .collect::<anyhow::Result<Vec<_>>>()?;

    if let Some(workspace_id) = filter.workspace_id {
        rows.retain(|run| run.workspace_id.as_deref() == Some(workspace_id.as_str()));
    }
    if let Some(agent_id) = filter.agent_id {
        rows.retain(|run| run.agent_id == agent_id);
    }
    if let Some(status) = filter.status {
        rows.retain(|run| run.status == status);
    }

    Ok(rows)
}

pub async fn get(id: &str) -> anyhow::Result<AgentRun> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, agent_id, role, workspace_id, cwd, status, pid, command_json,
               input_ref, output_ref, error, started_at, updated_at, finished_at, metadata_json
        FROM agent_runs
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .with_context(|| format!("agent run not found: {id}"))?;
    row_to_agent_run(row)
}

pub async fn read_output(id: &str, max_bytes: usize) -> anyhow::Result<AgentRunOutput> {
    let run = get(id).await?;
    let output_ref = run
        .output_ref
        .clone()
        .unwrap_or_else(|| output_path_for_run(id).display().to_string());

    let path = PathBuf::from(&output_ref);
    let (mut stdout, mut stderr) = if path.exists() {
        let content = fs::read_to_string(&path).await?;
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
            (
                value
                    .get("stdout")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string(),
                value
                    .get("stderr")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string(),
            )
        } else {
            (content, String::new())
        }
    } else {
        (
            read_sidecar(&stdout_path_for_run(id))
                .await
                .unwrap_or_default(),
            read_sidecar(&stderr_path_for_run(id))
                .await
                .unwrap_or_default(),
        )
    };

    let max_bytes = max_bytes.clamp(1, 1_000_000);
    stdout = tail_bytes(&stdout, max_bytes);
    stderr = tail_bytes(&stderr, max_bytes);

    Ok(AgentRunOutput {
        run,
        stdout,
        stderr,
        output_ref: Some(output_ref),
    })
}

pub async fn stop(id: &str) -> anyhow::Result<AgentRun> {
    let mut run = get(id).await?;
    if matches!(
        run.status,
        AgentRunStatus::Succeeded | AgentRunStatus::Failed | AgentRunStatus::Stopped
    ) {
        return Ok(run);
    }

    let mut stop_error = None;
    if let Some(pid) = run.pid {
        if let Err(err) = terminate_pid(pid).await {
            stop_error = Some(err.to_string());
        }
    }

    let now = Utc::now();
    run.status = AgentRunStatus::Stopped;
    run.updated_at = now;
    run.finished_at = Some(now);
    if let Some(err) = stop_error {
        run.error = Some(err);
    }
    upsert(&run).await?;
    Ok(run)
}

async fn terminate_pid(pid: i64) -> anyhow::Result<()> {
    if pid <= 0 {
        bail!("invalid pid: {pid}");
    }

    #[cfg(windows)]
    {
        let status = TokioCommand::new("taskkill")
            .arg("/PID")
            .arg(pid.to_string())
            .arg("/T")
            .arg("/F")
            .status()
            .await
            .context("run taskkill")?;
        if !status.success() {
            bail!("taskkill failed for pid {pid}: {status}");
        }
    }

    #[cfg(not(windows))]
    {
        let status = TokioCommand::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .await
            .context("run kill")?;
        if !status.success() {
            bail!("kill failed for pid {pid}: {status}");
        }
    }

    Ok(())
}

pub async fn upsert(run: &AgentRun) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            id, agent_id, role, workspace_id, cwd, status, pid, command_json,
            input_ref, output_ref, error, started_at, updated_at, finished_at, metadata_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        ON CONFLICT(id) DO UPDATE SET
            agent_id = excluded.agent_id,
            role = excluded.role,
            workspace_id = excluded.workspace_id,
            cwd = excluded.cwd,
            status = excluded.status,
            pid = excluded.pid,
            command_json = excluded.command_json,
            input_ref = excluded.input_ref,
            output_ref = excluded.output_ref,
            error = excluded.error,
            updated_at = excluded.updated_at,
            finished_at = excluded.finished_at,
            metadata_json = excluded.metadata_json
        "#,
    )
    .bind(&run.id)
    .bind(&run.agent_id)
    .bind(&run.role)
    .bind(&run.workspace_id)
    .bind(run.cwd.as_ref().map(|path| path.display().to_string()))
    .bind(run.status.as_str())
    .bind(run.pid)
    .bind(
        run.command_json
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
    )
    .bind(&run.input_ref)
    .bind(&run.output_ref)
    .bind(&run.error)
    .bind(run.started_at.to_rfc3339())
    .bind(run.updated_at.to_rfc3339())
    .bind(run.finished_at.map(|value| value.to_rfc3339()))
    .bind(
        run.metadata_json
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
    )
    .execute(&pool)
    .await?;
    tasks::touch_signal_file().await;
    Ok(())
}

async fn write_run_input(run_id: &str, input: &StartAgentRunInput) -> anyhow::Result<String> {
    let dir = Paths::agent_runs_dir();
    fs::create_dir_all(&dir).await?;
    let path = dir.join(format!("{run_id}-input.json"));
    fs::write(&path, serde_json::to_string_pretty(input)?).await?;
    Ok(path.display().to_string())
}

fn output_path_for_run(run_id: &str) -> PathBuf {
    Paths::agent_runs_dir().join(format!("{run_id}-output.json"))
}

fn stdout_path_for_run(run_id: &str) -> PathBuf {
    Paths::agent_runs_dir().join(format!("{run_id}-stdout.log"))
}

fn stderr_path_for_run(run_id: &str) -> PathBuf {
    Paths::agent_runs_dir().join(format!("{run_id}-stderr.log"))
}

async fn read_sidecar(path: &PathBuf) -> anyhow::Result<String> {
    if path.exists() {
        Ok(fs::read_to_string(path).await?)
    } else {
        Ok(String::new())
    }
}

async fn write_run_output(run_id: &str, stdout: &str, stderr: &str) -> anyhow::Result<String> {
    let path = output_path_for_run(run_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let mut file = fs::File::create(&path).await?;
    file.write_all(
        serde_json::to_string_pretty(&serde_json::json!({
            "stdout": stdout,
            "stderr": stderr,
            "written_at": Utc::now().to_rfc3339(),
        }))?
        .as_bytes(),
    )
    .await?;
    file.flush().await?;
    Ok(path.display().to_string())
}

fn row_to_agent_run(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<AgentRun> {
    let command_json = row
        .try_get::<Option<String>, _>("command_json")?
        .map(|value| serde_json::from_str(&value))
        .transpose()?;
    let metadata_json = row
        .try_get::<Option<String>, _>("metadata_json")?
        .map(|value| serde_json::from_str(&value))
        .transpose()?;

    Ok(AgentRun {
        id: row.try_get("id")?,
        agent_id: row.try_get("agent_id")?,
        role: row.try_get("role")?,
        workspace_id: row.try_get("workspace_id")?,
        cwd: row.try_get::<Option<String>, _>("cwd")?.map(PathBuf::from),
        status: AgentRunStatus::from_str(row.try_get::<String, _>("status")?.as_str())?,
        pid: row.try_get("pid")?,
        command_json,
        input_ref: row.try_get("input_ref")?,
        output_ref: row.try_get("output_ref")?,
        error: row.try_get("error")?,
        started_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("started_at")?.as_str())?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("updated_at")?.as_str())?
            .with_timezone(&Utc),
        finished_at: row
            .try_get::<Option<String>, _>("finished_at")?
            .map(|value| DateTime::parse_from_rfc3339(&value).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
        metadata_json,
    })
}

fn next_run_id() -> String {
    format!(
        "ar-{}-{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        Uuid::new_v4()
    )
}

fn tail_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }

    let mut start = value.len() - max_bytes;
    while !value.is_char_boundary(start) {
        start += 1;
    }
    value[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn agent_run_round_trip_and_output_read() {
        let _root = TestRoot::new();
        let now = Utc::now();
        let run = AgentRun {
            id: "ar-test".to_string(),
            agent_id: "claude".to_string(),
            role: "assistant".to_string(),
            workspace_id: Some("ws-test".to_string()),
            cwd: Some(PathBuf::from("I:/personal-agent")),
            status: AgentRunStatus::Running,
            pid: None,
            command_json: Some(serde_json::json!({ "program": "claude" })),
            input_ref: None,
            output_ref: Some(
                write_run_output("ar-test", "hello", "warn")
                    .await
                    .expect("output"),
            ),
            error: None,
            started_at: now,
            updated_at: now,
            finished_at: None,
            metadata_json: Some(serde_json::json!({ "x": 1 })),
        };

        upsert(&run).await.expect("upsert");
        let loaded = get("ar-test").await.expect("get");

        assert_eq!(loaded.id, "ar-test");
        assert_eq!(loaded.workspace_id.as_deref(), Some("ws-test"));
        assert_eq!(loaded.status, AgentRunStatus::Running);

        let output = read_output("ar-test", 1024).await.expect("read output");
        assert_eq!(output.stdout, "hello");
        assert_eq!(output.stderr, "warn");
    }

    #[tokio::test]
    async fn stop_without_pid_marks_run_stopped() {
        let _root = TestRoot::new();
        let now = Utc::now();
        upsert(&AgentRun {
            id: "ar-stop".to_string(),
            agent_id: "claude".to_string(),
            role: "assistant".to_string(),
            workspace_id: None,
            cwd: None,
            status: AgentRunStatus::Running,
            pid: None,
            command_json: None,
            input_ref: None,
            output_ref: None,
            error: None,
            started_at: now,
            updated_at: now,
            finished_at: None,
            metadata_json: None,
        })
        .await
        .expect("upsert");

        let stopped = stop("ar-stop").await.expect("stop");

        assert_eq!(stopped.status, AgentRunStatus::Stopped);
        assert!(stopped.finished_at.is_some());
    }

    #[tokio::test]
    async fn read_output_falls_back_to_sidecar_logs() {
        let _root = TestRoot::new();
        fs::create_dir_all(Paths::agent_runs_dir())
            .await
            .expect("agent runs dir");
        fs::write(stdout_path_for_run("ar-sidecar"), "live stdout")
            .await
            .expect("stdout sidecar");
        fs::write(stderr_path_for_run("ar-sidecar"), "live stderr")
            .await
            .expect("stderr sidecar");

        let now = Utc::now();
        upsert(&AgentRun {
            id: "ar-sidecar".to_string(),
            agent_id: "claude".to_string(),
            role: "assistant".to_string(),
            workspace_id: None,
            cwd: None,
            status: AgentRunStatus::Running,
            pid: None,
            command_json: None,
            input_ref: None,
            output_ref: None,
            error: None,
            started_at: now,
            updated_at: now,
            finished_at: None,
            metadata_json: None,
        })
        .await
        .expect("upsert");

        let output = read_output("ar-sidecar", 1024).await.expect("read output");

        assert_eq!(output.stdout, "live stdout");
        assert_eq!(output.stderr, "live stderr");
    }

    #[test]
    fn tail_bytes_keeps_utf8_boundary() {
        assert_eq!(tail_bytes("abc你好", 7), "c你好");
    }
}
