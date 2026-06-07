//! Claude -p Adapter: spawns Claude subprocesses for task execution with runtime integration.
//!
//! Injects `RUNTIME_API_URL`, `RUNTIME_TOKEN`, `WORKSPACE_ID`, `TASK_ID` into the child
//! environment so the spawned agent can call back to the runtime API.
//!
//! On completion the adapter:
//! - Parses stdout into structured sections (summary / changes / risks / next_steps)
//! - Updates `AgentTask.result_ref` + `AgentTask.status`
//! - Emits `task.review_ready` or `task.failed` audit events

use crate::agent_runs::{self, AgentRun, AgentRunStatus};
use crate::agent_teams;
use crate::db;
use crate::events;
use crate::goal_tasks::AgentTask;
use crate::paths::Paths;
use anyhow::Context;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};
use tokio::fs;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Configuration for the Claude -p adapter.
#[derive(Clone, Debug)]
pub struct ClaudePConfig {
    /// URL of the local runtime API server (injected as RUNTIME_API_URL).
    pub runtime_api_url: String,
    /// Path to the claude binary. Defaults to "claude".
    pub claude_binary: String,
    /// Default timeout in seconds for spawned processes.
    pub default_timeout_seconds: u64,
}

impl Default for ClaudePConfig {
    fn default() -> Self {
        Self {
            runtime_api_url: "http://127.0.0.1:9821".to_string(),
            claude_binary: "claude".to_string(),
            default_timeout_seconds: 600,
        }
    }
}

/// Lightweight handle returned immediately after spawning a Claude process.
///
/// The `status` field mirrors the initial `AgentRun` status ("running").
/// For the latest status, query `agent_runs::get(&run_id)`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentRunRef {
    pub run_id: String,
    pub task_id: String,
    pub workspace_id: String,
    pub status: AgentRunStatus,
    pub pid: Option<i64>,
}

/// Structured output parsed from Claude's stdout.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ClaudeOutput {
    pub summary: String,
    pub changes: Vec<String>,
    pub risks: Vec<String>,
    pub next_steps: Vec<String>,
    /// Raw stdout for debugging.
    pub raw_stdout: String,
    /// Raw stderr for debugging.
    pub raw_stderr: String,
}

/// Environment variables injected into the child process.
struct EnvVars {
    runtime_api_url: String,
    runtime_token: String,
    workspace_id: String,
    task_id: String,
}

/// Result of waiting for a child process.
enum SpawnFinish {
    Status(std::process::ExitStatus),
    IoError(String),
    TimedOut,
}

// ---------------------------------------------------------------------------
// ClaudePAdapter
// ---------------------------------------------------------------------------

/// The Claude -p adapter.
///
/// Spawns `claude -p <prompt>` subprocesses for task execution, injecting
/// runtime environment variables so the child agent can call back to the
/// conductor runtime API.
pub struct ClaudePAdapter {
    config: ClaudePConfig,
}

impl ClaudePAdapter {
    /// Create a new adapter with the given configuration.
    pub fn new(config: ClaudePConfig) -> Self {
        Self { config }
    }

    /// Create a new adapter with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(ClaudePConfig::default())
    }

    /// Spawn a Claude -p process for the given task.
    ///
    /// Injects environment variables: `RUNTIME_API_URL`, `RUNTIME_TOKEN`,
    /// `WORKSPACE_ID`, `TASK_ID`. Returns an [`AgentRunRef`] immediately;
    /// the process runs in the background.
    ///
    /// On completion the adapter automatically:
    /// - Parses stdout into [`ClaudeOutput`]
    /// - Updates `AgentTask.result_ref` + `AgentTask.status` to "review_ready"
    ///   (or "failed" on error)
    /// - Emits `task.review_ready` or `task.failed` events
    pub async fn spawn(
        &self,
        task: AgentTask,
        runtime_token: String,
    ) -> anyhow::Result<AgentRunRef> {
        self.spawn_with_timeout(task, runtime_token, self.config.default_timeout_seconds)
            .await
    }

    /// Spawn with a custom timeout in seconds.
    pub async fn spawn_with_timeout(
        &self,
        task: AgentTask,
        runtime_token: String,
        timeout_seconds: u64,
    ) -> anyhow::Result<AgentRunRef> {
        let prompt = build_task_prompt(&task).await;
        let workspace_id = task.workspace_id.clone();
        let task_id = task.id.clone();

        // -- Create the AgentRun record ---------------------------------------
        let run_id = next_run_id();
        let now = Utc::now();
        let command_json = serde_json::json!({
            "program": self.config.claude_binary,
            "args": ["-p", "<task_prompt>"],
            "timeout_seconds": timeout_seconds,
            "adapter": "claude_p",
        });

        let run = AgentRun {
            id: run_id.clone(),
            agent_id: "claude_p".to_string(),
            role: "task_agent".to_string(),
            workspace_id: Some(workspace_id.clone()),
            cwd: None,
            status: AgentRunStatus::Queued,
            pid: None,
            command_json: Some(command_json),
            input_ref: None,
            output_ref: None,
            error: None,
            started_at: now,
            updated_at: now,
            finished_at: None,
            metadata_json: Some(serde_json::json!({
                "task_id": task_id,
                "workspace_id": workspace_id,
                "adapter": "claude_p",
            })),
        };

        agent_runs::upsert(&run).await?;
        events::emit_agent_run_created(&run_id, "claude_p", "queued").await;
        crate::tasks::touch_signal_file().await;

        // -- Build environment -------------------------------------------------
        let env = build_env(
            &self.config.runtime_api_url,
            &runtime_token,
            &workspace_id,
            &task_id,
        );

        // -- Create stdout/stderr sidecar files --------------------------------
        let runs_dir = Paths::agent_runs_dir();
        fs::create_dir_all(&runs_dir).await?;
        let stdout_file = std::fs::File::create(runs_dir.join(format!("{run_id}-stdout.log")))
            .context("create stdout sidecar")?;
        let stderr_file = std::fs::File::create(runs_dir.join(format!("{run_id}-stderr.log")))
            .context("create stderr sidecar")?;

        // -- Spawn the child process ------------------------------------------
        let mut cmd = std::process::Command::new(&self.config.claude_binary);
        cmd.arg("-p")
            .arg(&prompt)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file))
            .env("RUNTIME_API_URL", &env.runtime_api_url)
            .env("RUNTIME_TOKEN", &env.runtime_token)
            .env("WORKSPACE_ID", &env.workspace_id)
            .env("TASK_ID", &env.task_id);

        let child = cmd.spawn().context("spawn claude -p for task")?;
        let pid = child.id();

        // -- Transition run to Running ----------------------------------------
        let mut running_run = run;
        running_run.status = AgentRunStatus::Running;
        running_run.pid = Some(pid as i64);
        running_run.updated_at = Utc::now();
        agent_runs::upsert(&running_run).await?;
        events::emit_agent_run_phase_changed(&run_id, "queued", "running").await;
        bind_executor_run_to_team_member(&task, &run_id).await?;

        // -- Background monitor thread ----------------------------------------
        let rid = run_id.clone();
        let tid = task_id.clone();
        std::thread::spawn(move || {
            let finish = wait_for_child(child, Duration::from_secs(timeout_seconds));
            if let Ok(rt) = tokio::runtime::Runtime::new() {
                let _ = rt.block_on(finish_run(&rid, &tid, finish));
            }
        });

        Ok(AgentRunRef {
            run_id,
            task_id,
            workspace_id,
            status: AgentRunStatus::Running,
            pid: Some(pid as i64),
        })
    }
}

async fn bind_executor_run_to_team_member(task: &AgentTask, run_id: &str) -> anyhow::Result<()> {
    let Some(cycle_id) = task.cycle_id.as_deref() else {
        return Ok(());
    };

    agent_teams::bind_member_run_to_task(
        &format!("team-{cycle_id}"),
        &task.id,
        run_id,
        Some(serde_json::json!({
            "agent_run_id": run_id,
        })),
    )
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Prompt & env helpers
// ---------------------------------------------------------------------------

/// Build the prompt sent to Claude from the task's fields.
///
/// Instructs Claude to execute the task and produce structured output that
/// [`parse_output`] can extract: `## Summary`, `## Changes`, `## Risks`, `## Next Steps`.
pub async fn build_task_prompt(task: &AgentTask) -> String {
    // Resolve the workspace root path so the LLM knows where to write files.
    let workspace_root = crate::workspaces::get(&task.workspace_id)
        .await
        .ok()
        .map(|ws| ws.root.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("workspace:{}", task.workspace_id));

    format!(
        "You are a task agent. Complete the following task to the best of your ability.\n\n\
         Title: {title}\n\
         Instruction: {instruction}\n\
         Workspace root: {workspace_root}\n\
         Task ID: {task_id}\n\n\
         Important:\n\
         - Use available tools (Bash, Read, Write files) to carry out the task.\n\
         - Write any significant findings or documents to files under: {workspace_root}\n\
         - Do NOT just describe what you would do — actually do it.\n\n\
         After completing the task, output your results in the following format:\n\n\
         ## Summary\n\
         <what you actually did and what you found>\n\n\
         ## Changes\n\
         <list of files created or modified, one per line with - prefix, include full paths>\n\n\
         ## Risks\n\
         <any risks or concerns, one per line with - prefix>\n\n\
         ## Next Steps\n\
         <concrete suggested next steps, one per line with - prefix>",
        title = task.title,
        instruction = task.instruction,
        workspace_root = workspace_root,
        task_id = task.id,
    )
}

/// Build the environment variables injected into the child process.
fn build_env(
    runtime_api_url: &str,
    runtime_token: &str,
    workspace_id: &str,
    task_id: &str,
) -> EnvVars {
    EnvVars {
        runtime_api_url: runtime_api_url.to_string(),
        runtime_token: runtime_token.to_string(),
        workspace_id: workspace_id.to_string(),
        task_id: task_id.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Output parsing
// ---------------------------------------------------------------------------

/// Parse Claude's stdout into structured sections.
///
/// Looks for `## Summary`, `## Changes`, `## Risks`, `## Next Steps` headers.
/// If no structured sections are found, the first 500 characters of stdout
/// are used as the summary fallback.
pub fn parse_output(stdout: &str, stderr: &str) -> ClaudeOutput {
    let mut output = ClaudeOutput {
        raw_stdout: stdout.to_string(),
        raw_stderr: stderr.to_string(),
        ..Default::default()
    };

    output.summary = extract_section(stdout, "Summary");
    output.changes = extract_list_section(stdout, "Changes");
    output.risks = extract_list_section(stdout, "Risks");
    output.next_steps = extract_list_section(stdout, "Next Steps");

    // Fallback: if no structured output, use first 500 chars as summary
    if output.summary.is_empty() && !stdout.is_empty() {
        output.summary = truncate_chars(stdout, 500).trim().to_string();
    }

    output
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect()
    }
}

/// Extract the text under a `## <header>` section until the next `## ` or EOF.
fn extract_section(text: &str, header: &str) -> String {
    // Build a pattern: `## Header\n<content>` until next `## ` or end
    let needle = format!("## {}", header);
    let start = match text.find(&needle) {
        Some(pos) => pos + needle.len(),
        None => return String::new(),
    };
    // Skip to the end of the header line
    let after_header = match text[start..].find('\n') {
        Some(offset) => &text[start + offset + 1..],
        None => return String::new(),
    };
    // Find the end of the section (next ## header or EOF)
    let end = after_header.find("\n## ").unwrap_or(after_header.len());
    after_header[..end].trim().to_string()
}

/// Extract a list section: each line stripped of its list marker (`-`, `*`, or `1.`).
fn extract_list_section(text: &str, header: &str) -> Vec<String> {
    let section = extract_section(text, header);
    if section.is_empty() {
        return Vec::new();
    }
    section
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                trimmed[2..].trim().to_string()
            } else if let Some(pos) = trimmed.find(". ") {
                if trimmed[..pos].chars().all(|c| c.is_ascii_digit()) {
                    trimmed[pos + 2..].trim().to_string()
                } else {
                    trimmed.to_string()
                }
            } else {
                trimmed.to_string()
            }
        })
        .filter(|line| !line.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Process lifecycle
// ---------------------------------------------------------------------------

/// Poll the child process until it exits or the timeout elapses.
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

/// File path helpers consistent with `agent_runs` conventions.
fn stdout_path(run_id: &str) -> PathBuf {
    Paths::agent_runs_dir().join(format!("{run_id}-stdout.log"))
}

fn stderr_path(run_id: &str) -> PathBuf {
    Paths::agent_runs_dir().join(format!("{run_id}-stderr.log"))
}

fn output_path(run_id: &str) -> PathBuf {
    Paths::agent_runs_dir().join(format!("{run_id}-output.json"))
}

/// Read a file, returning an empty string on any error.
async fn read_file_safe(path: &PathBuf) -> String {
    fs::read_to_string(path).await.unwrap_or_default()
}

/// Write a JSON output file with stdout + stderr content.
async fn write_run_output(run_id: &str, stdout: &str, stderr: &str) -> anyhow::Result<String> {
    let dir = Paths::agent_runs_dir();
    fs::create_dir_all(&dir).await?;
    let path = output_path(run_id);
    let content = serde_json::to_string_pretty(&serde_json::json!({
        "stdout": stdout,
        "stderr": stderr,
        "written_at": Utc::now().to_rfc3339(),
    }))?;
    fs::write(&path, content).await?;
    Ok(path.display().to_string())
}

/// Background completion handler: called after the child exits.
///
/// Reads stdout/stderr, parses the output, updates the `AgentRun` record,
/// updates the linked `AgentTask`, and emits audit events.
async fn finish_run(run_id: &str, task_id: &str, finish: SpawnFinish) -> anyhow::Result<()> {
    let mut run = agent_runs::get(run_id).await?;
    if run.status == AgentRunStatus::Stopped {
        return Ok(());
    }

    let now = Utc::now();
    let stdout = read_file_safe(&stdout_path(run_id)).await;
    let stderr = read_file_safe(&stderr_path(run_id)).await;
    let parsed = parse_output(&stdout, &stderr);

    match finish {
        SpawnFinish::Status(status) => {
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
            let timeout_error = "claude timed out";
            run.error = Some(timeout_error.to_string());
            let stderr = if stderr.trim().is_empty() {
                timeout_error.to_string()
            } else {
                format!("{}\n{}", stderr.trim_end(), timeout_error)
            };
            let output_ref = write_run_output(run_id, &stdout, &stderr).await?;
            run.output_ref = Some(output_ref);
        }
    }

    run.updated_at = now;
    run.finished_at = Some(now);
    agent_runs::upsert(&run).await?;

    events::emit_agent_run_phase_changed(run_id, "running", run.status.as_str()).await;

    // Wake the desktop watcher so agent_runs_changed + goals_changed fire.
    crate::tasks::touch_signal_file().await;

    // Best-effort task update — don't fail the run if this fails
    update_task_on_completion(task_id, &run, &parsed).await;

    Ok(())
}

// ---------------------------------------------------------------------------
// Task writeback
// ---------------------------------------------------------------------------

/// Update the linked task after the run completes.
///
/// - On success: set `status = "review_ready"`, `result_ref` to the output ref.
/// - On failure: set `status = "failed"`, `error` to the error message.
///
/// This is best-effort — errors are logged but not propagated.
async fn update_task_on_completion(task_id: &str, run: &AgentRun, output: &ClaudeOutput) {
    let result = match run.status {
        AgentRunStatus::Succeeded => {
            let result_ref = run.output_ref.as_deref().unwrap_or("");
            set_task_review_ready(task_id, result_ref, output).await
        }
        AgentRunStatus::Failed => {
            let error = run.error.as_deref().unwrap_or("unknown error");
            goal_tasks_fail_safe(task_id, error).await
        }
        _ => Ok(()),
    };

    if let Err(e) = result {
        tracing::warn!(
            "failed to update task {task_id} after run {}: {e:#}",
            run.id
        );
    }

    // Project the agent run result back into the linked chat session.
    project_run_to_chat_session(task_id, run, output).await;
}

fn projected_run_summary(run: &AgentRun, output: &ClaudeOutput) -> String {
    if !output.raw_stdout.trim().is_empty() {
        output.raw_stdout.clone()
    } else if output.summary.is_empty() {
        if run.status == AgentRunStatus::Failed {
            run.error.as_deref().unwrap_or("unknown error").to_string()
        } else {
            String::new()
        }
    } else {
        output.summary.clone()
    }
}

fn projected_run_detail_text(task_title: &str, run: &AgentRun, output: &ClaudeOutput) -> String {
    let summary = projected_run_summary(run, output);
    let trimmed_summary = summary.trim();
    let failure_reason = run
        .error
        .as_deref()
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .unwrap_or_else(|| {
            if trimmed_summary.is_empty() {
                "unknown error"
            } else {
                trimmed_summary
            }
        });

    if run.status == AgentRunStatus::Succeeded {
        if trimmed_summary.is_empty() {
            format!("子任务 `{task_title}` 已完成。")
        } else {
            format!("子任务 `{task_title}` 已完成。\n\n{trimmed_summary}")
        }
    } else if trimmed_summary.is_empty() || trimmed_summary == failure_reason {
        format!("子任务 `{task_title}` 未能继续。\n\n{failure_reason}")
    } else {
        format!("子任务 `{task_title}` 未能继续。\n\n{failure_reason}\n\n{trimmed_summary}")
    }
}

fn projected_run_content_blocks(
    task_title: &str,
    run: &AgentRun,
    output: &ClaudeOutput,
) -> Vec<serde_json::Value> {
    let detail_text = projected_run_detail_text(task_title, run, output);

    if run.status == AgentRunStatus::Succeeded {
        vec![
            serde_json::json!({
                "type": "completion",
                "title": format!("子任务已完成：{task_title}"),
                "summary": serde_json::Value::Null,
            }),
            serde_json::json!({
                "type": "text",
                "text": detail_text,
            }),
        ]
    } else {
        let failure_reason = run
            .error
            .as_deref()
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
            .unwrap_or("unknown error");
        let action_items = if !output.next_steps.is_empty() {
            output.next_steps.clone()
        } else {
            output.risks.iter().take(3).cloned().collect::<Vec<_>>()
        };

        vec![
            serde_json::json!({
                "type": "blocked",
                "title": format!("子任务需要处理：{task_title}"),
                "reason": failure_reason,
                "action_items": action_items,
            }),
            serde_json::json!({
                "type": "text",
                "text": detail_text,
            }),
        ]
    }
}

/// Write a brief assistant message into the chat session that owns this goal task,
/// so the user can see sub-agent progress directly in the conversation.
async fn project_run_to_chat_session(task_id: &str, run: &AgentRun, output: &ClaudeOutput) {
    let task = match crate::goal_tasks::get_task(task_id).await {
        Ok(Some(t)) => t,
        _ => return,
    };
    let goal_id = match task.goal_id.as_deref() {
        Some(g) => g.to_string(),
        None => return,
    };

    // Find the chat session linked to this goal.
    let session_id = match crate::chat::find_session_for_goal(&goal_id).await {
        Some(sid) => sid,
        None => return,
    };

    let task_title = task.title.chars().take(48).collect::<String>();
    let content_blocks = projected_run_content_blocks(&task_title, run, output);
    let fallback_text = projected_run_detail_text(&task_title, run, output);
    let content = serde_json::to_string(&content_blocks).unwrap_or(fallback_text);

    if let Ok(pool) = crate::db::pool().await {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let _ = sqlx::query(
            "INSERT INTO chat_messages (id, role, content, created_at, seq, tool_calls, session_id) \
             VALUES (?1, 'assistant', ?2, ?3, ?3, NULL, ?4)",
        )
        .bind(&id)
        .bind(&content)
        .bind(&now)
        .bind(&session_id)
        .execute(&pool)
        .await;
    }
}

/// Transition a task to `review_ready` with the result ref.
async fn set_task_review_ready(
    task_id: &str,
    result_ref: &str,
    output: &ClaudeOutput,
) -> anyhow::Result<()> {
    let task = crate::goal_tasks::get_task(task_id).await?;
    let task = match task {
        Some(t) => t,
        None => return Ok(()), // task deleted — nothing to do
    };

    if task.status != "running" {
        return Ok(()); // task already moved to a different state
    }

    let now = Utc::now();
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE agent_tasks
        SET status = 'review_ready',
            result_ref = ?1,
            updated_at = ?2,
            finished_at = ?2
        WHERE id = ?3 AND status = 'running'
        "#,
    )
    .bind(result_ref)
    .bind(now.to_rfc3339())
    .bind(task_id)
    .execute(&pool)
    .await?;

    let _ = events::append(
        "task",
        "task.review_ready",
        &serde_json::json!({
            "task_id": task_id,
            "result_ref": result_ref,
            "summary": output.summary,
        }),
    )
    .await;

    Ok(())
}

/// Best-effort fail via the `goal_tasks` module (handles lease release + event).
async fn goal_tasks_fail_safe(task_id: &str, error: &str) -> anyhow::Result<()> {
    // Check task exists and is in "running" state before calling fail_task
    let task = crate::goal_tasks::get_task(task_id).await?;
    let task = match task {
        Some(t) => t,
        None => return Ok(()),
    };
    if task.status != "running" {
        return Ok(());
    }
    let _ = crate::goal_tasks::fail_task(task_id, error).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn next_run_id() -> String {
    format!(
        "ar-{}-{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        Uuid::new_v4()
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Test 1: parse_output extracts all sections ---------------------------

    #[test]
    fn parse_output_extracts_all_sections() {
        let stdout = "\
Some preamble text here.

## Summary
Implemented the new feature with full test coverage.

## Changes
- Added src/feature.rs
- Modified src/lib.rs
- Updated Cargo.toml

## Risks
- May conflict with existing module
- Performance impact unknown

## Next Steps
- Run integration tests
- Deploy to staging
- Monitor for regressions

Some trailing text.";

        let output = parse_output(stdout, "some stderr");

        assert_eq!(
            output.summary,
            "Implemented the new feature with full test coverage."
        );
        // Verify the expected changes are present (parser may include section boundary lines)
        assert!(
            output.changes.contains(&"Added src/feature.rs".to_string()),
            "changes: {:?}",
            output.changes
        );
        assert!(output.changes.contains(&"Modified src/lib.rs".to_string()));
        assert!(output.changes.contains(&"Updated Cargo.toml".to_string()));
        assert!(output.risks.len() >= 2, "risks: {:?}", output.risks);
        assert!(output
            .risks
            .contains(&"May conflict with existing module".to_string()));
        assert!(output
            .risks
            .contains(&"Performance impact unknown".to_string()));
        assert!(
            output.next_steps.len() >= 3,
            "next_steps: {:?}",
            output.next_steps
        );
        assert!(output
            .next_steps
            .contains(&"Run integration tests".to_string()));
        assert!(output.next_steps.contains(&"Deploy to staging".to_string()));
        assert!(output
            .next_steps
            .contains(&"Monitor for regressions".to_string()));
        assert_eq!(output.raw_stdout, stdout);
        assert_eq!(output.raw_stderr, "some stderr");
    }

    // -- Test 2: parse_output handles empty / no sections ---------------------

    #[test]
    fn parse_output_handles_empty_and_unstructured_input() {
        // Empty input
        let output = parse_output("", "");
        assert!(output.summary.is_empty());
        assert!(output.changes.is_empty());
        assert!(output.risks.is_empty());
        assert!(output.next_steps.is_empty());

        // No sections — fallback to first 500 chars
        let plain = "Just some plain text output without any markdown sections.";
        let output = parse_output(plain, "");
        assert_eq!(output.summary, plain);
        assert!(output.changes.is_empty());

        // Partial sections — only Summary present, captures until EOF
        let partial = "## Summary\nOnly summary exists.\n\nNo other sections.";
        let output = parse_output(partial, "");
        assert_eq!(output.summary, "Only summary exists.\n\nNo other sections.");
        assert!(output.changes.is_empty());
        assert!(output.risks.is_empty());
        assert!(output.next_steps.is_empty());
    }

    // -- Test 3: build_task_prompt includes task info -------------------------

    #[tokio::test]
    async fn build_task_prompt_includes_task_info() {
        let task = AgentTask {
            id: "task-abc".to_string(),
            workspace_id: "ws-xyz".to_string(),
            goal_id: Some("goal-1".to_string()),
            cycle_id: Some("cycle-1".to_string()),
            parent_task_id: None,
            title: "Implement feature X".to_string(),
            instruction: "Add the new feature with tests.".to_string(),
            status: "running".to_string(),
            agent_kind: "claude_p".to_string(),
            assigned_agent_id: None,
            claimed_by: None,
            write_scope_json: vec![],
            read_scope_json: vec![],
            allowed_tools_json: vec![],
            dependencies_json: vec![],
            acceptance_json: vec![],
            result_ref: None,
            error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            claimed_at: None,
            finished_at: None,
        };

        let prompt = build_task_prompt(&task).await;

        assert!(prompt.contains("task-abc"), "prompt should contain task_id");
        assert!(
            prompt.contains("ws-xyz") || prompt.contains("workspace:ws-xyz"),
            "prompt should reference workspace"
        );
        assert!(
            prompt.contains("Implement feature X"),
            "prompt should contain title"
        );
        assert!(
            prompt.contains("Add the new feature with tests."),
            "prompt should contain instruction"
        );
        assert!(
            prompt.contains("## Summary"),
            "prompt should instruct output format"
        );
        assert!(
            prompt.contains("## Changes"),
            "prompt should have Changes section"
        );
        assert!(
            prompt.contains("## Risks"),
            "prompt should have Risks section"
        );
        assert!(
            prompt.contains("## Next Steps"),
            "prompt should have Next Steps section"
        );
    }

    // -- Test 4: build_env sets all variables ---------------------------------

    #[test]
    fn build_env_sets_all_variables() {
        let env = build_env(
            "http://localhost:9821",
            "tok-abcdef123456",
            "ws-env-test",
            "task-env-test",
        );

        assert_eq!(env.runtime_api_url, "http://localhost:9821");
        assert_eq!(env.runtime_token, "tok-abcdef123456");
        assert_eq!(env.workspace_id, "ws-env-test");
        assert_eq!(env.task_id, "task-env-test");
    }

    // -- Test 5: AgentRunRef serialization roundtrip --------------------------

    #[test]
    fn agent_run_ref_serialization_roundtrip() {
        let run_ref = AgentRunRef {
            run_id: "ar-test-001".to_string(),
            task_id: "task-test-001".to_string(),
            workspace_id: "ws-serial".to_string(),
            status: AgentRunStatus::Running,
            pid: Some(12345),
        };

        let json = serde_json::to_string(&run_ref).expect("serialize");
        let deserialized: AgentRunRef = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.run_id, "ar-test-001");
        assert_eq!(deserialized.task_id, "task-test-001");
        assert_eq!(deserialized.workspace_id, "ws-serial");
        assert_eq!(deserialized.status, AgentRunStatus::Running);
        assert_eq!(deserialized.pid, Some(12345));
    }

    // -- Test 6: extract_section with asterisk lists --------------------------

    #[test]
    fn parse_output_handles_asterisk_and_numbered_lists() {
        let stdout = "\
## Changes
* First change
* Second change

## Next Steps
1. Do this first
2. Then do this
3. Finally this";

        let output = parse_output(stdout, "");

        assert_eq!(output.changes.len(), 2);
        assert_eq!(output.changes[0], "First change");
        assert_eq!(output.changes[1], "Second change");
        assert_eq!(output.next_steps.len(), 3);
        assert_eq!(output.next_steps[0], "Do this first");
        assert_eq!(output.next_steps[1], "Then do this");
        assert_eq!(output.next_steps[2], "Finally this");
    }

    #[test]
    fn parse_output_fallback_summary_keeps_utf8_boundaries() {
        let stdout = format!("{}尾部", "然".repeat(260));
        let output = parse_output(&stdout, "");

        assert_eq!(output.summary.chars().count(), 500);
        assert!(output.summary.chars().all(|ch| ch == '然'));
    }

    fn sample_agent_run(status: AgentRunStatus) -> AgentRun {
        let now = Utc::now();
        AgentRun {
            id: "run-1".to_string(),
            agent_id: "claude_p".to_string(),
            role: "task_agent".to_string(),
            workspace_id: Some("ws-1".to_string()),
            cwd: None,
            status,
            pid: None,
            command_json: None,
            input_ref: None,
            output_ref: None,
            error: None,
            started_at: now,
            updated_at: now,
            finished_at: Some(now),
            metadata_json: None,
        }
    }

    #[test]
    fn projected_run_content_prefers_full_stdout_without_truncation() {
        let raw_stdout = "A".repeat(640);
        let output = ClaudeOutput {
            summary: "short summary".to_string(),
            raw_stdout: raw_stdout.clone(),
            ..Default::default()
        };

        let blocks = projected_run_content_blocks(
            "Long task title",
            &sample_agent_run(AgentRunStatus::Succeeded),
            &output,
        );

        assert_eq!(blocks[0]["summary"], serde_json::Value::Null);

        let text = blocks[1]["text"]
            .as_str()
            .expect("projection text block should be a string");
        assert!(text.contains(&raw_stdout));
        assert!(!text.contains("short summary"));
    }

    // -- Test 7: ClaudePConfig defaults --------------------------------------

    #[test]
    fn claude_p_config_defaults() {
        let config = ClaudePConfig::default();
        assert_eq!(config.runtime_api_url, "http://127.0.0.1:9821");
        assert_eq!(config.claude_binary, "claude");
        assert_eq!(config.default_timeout_seconds, 600);
    }
}
