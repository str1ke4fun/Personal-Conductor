use crate::{
    agent_runs::{AgentRun, AgentRunStatus},
    paths::Paths,
};
use anyhow::Context;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    process::{Child, ExitStatus, Stdio},
    time::{Duration, Instant},
};
use tokio::{fs, io::AsyncWriteExt, sync::Semaphore};
use uuid::Uuid;

static CLAUDE_SEMAPHORE: Semaphore = Semaphore::const_new(1);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubagentResult {
    pub agent_run_id: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub log_path: Option<std::path::PathBuf>,
    pub timed_out: bool,
}

pub async fn run_claude_p(
    prompt: &str,
    cwd: Option<&Path>,
    timeout: Duration,
) -> anyhow::Result<SubagentResult> {
    let _permit = CLAUDE_SEMAPHORE.acquire().await?;
    let start = Instant::now();
    let mut run = create_agent_run(prompt, cwd, timeout).await?;
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
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }

    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            mark_run_failed(&mut run, format!("spawn claude -p: {err}"), "queued").await?;
            return Err(err).context("spawn claude -p");
        }
    };

    run.status = AgentRunStatus::Running;
    run.pid = Some(child.id() as i64);
    run.updated_at = Utc::now();
    crate::agent_runs::upsert(&run).await?;
    crate::events::emit_agent_run_phase_changed(&run.id, "queued", "running").await;

    let finish = tokio::task::spawn_blocking(move || wait_for_child(child, timeout))
        .await
        .context("join subagent wait task")?;
    let stdout = read_sidecar(&stdout_path).await?;
    let mut stderr = read_sidecar(&stderr_path).await?;

    let (exit_code, timed_out, status_error, transition_from) = match finish {
        SpawnFinish::Status(status) => (
            status.code(),
            false,
            (!status.success()).then(|| format!("claude exited with {status}")),
            "running",
        ),
        SpawnFinish::IoError(err) => {
            if stderr.is_empty() {
                stderr = err.clone();
            }
            (None, false, Some(err), "running")
        }
        SpawnFinish::TimedOut => {
            if !stderr.is_empty() && !stderr.ends_with('\n') {
                stderr.push('\n');
            }
            stderr.push_str("claude -p timed out");
            (
                None,
                true,
                Some("claude -p timed out".to_string()),
                "running",
            )
        }
    };

    let output_ref = write_agent_run_output(&run.id, &stdout, &stderr).await?;
    run.output_ref = Some(output_ref);
    run.status = if exit_code == Some(0) {
        AgentRunStatus::Succeeded
    } else {
        AgentRunStatus::Failed
    };
    run.error = status_error;
    run.updated_at = Utc::now();
    run.finished_at = Some(run.updated_at);
    crate::agent_runs::upsert(&run).await?;
    crate::events::emit_agent_run_phase_changed(&run.id, transition_from, run.status.as_str())
        .await;

    let mut result = SubagentResult {
        agent_run_id: Some(run.id.clone()),
        stdout,
        stderr,
        exit_code,
        duration_ms: start.elapsed().as_millis() as u64,
        log_path: None,
        timed_out,
    };
    result.log_path = Some(write_log(prompt, cwd, &result).await?);
    Ok(result)
}

async fn create_agent_run(
    prompt: &str,
    cwd: Option<&Path>,
    timeout: Duration,
) -> anyhow::Result<AgentRun> {
    let run_id = next_run_id();
    let started_at = Utc::now();
    let input_ref = write_agent_run_input(&run_id, prompt, cwd, timeout).await?;
    let output_ref = output_path_for_run(&run_id).display().to_string();
    let run = AgentRun {
        id: run_id,
        agent_id: "claude".to_string(),
        role: "assistant".to_string(),
        workspace_id: None,
        cwd: cwd.map(|path| path.to_path_buf()),
        status: AgentRunStatus::Queued,
        pid: None,
        command_json: Some(serde_json::json!({
            "program": "claude",
            "args": ["-p", prompt],
            "timeout_seconds": timeout.as_secs().clamp(1, 86_400),
            "source": "subagent.claude_p"
        })),
        input_ref: Some(input_ref),
        output_ref: Some(output_ref),
        error: None,
        started_at,
        updated_at: started_at,
        finished_at: None,
        metadata_json: Some(serde_json::json!({
            "prompt": prompt,
            "source": "subagent.claude_p",
            "observability": "synchronous"
        })),
    };
    crate::agent_runs::upsert(&run).await?;
    crate::events::emit_agent_run_created(&run.id, &run.agent_id, run.status.as_str()).await;
    Ok(run)
}

async fn mark_run_failed(
    run: &mut AgentRun,
    error: String,
    previous_status: &str,
) -> anyhow::Result<()> {
    run.status = AgentRunStatus::Failed;
    run.error = Some(error.clone());
    run.updated_at = Utc::now();
    run.finished_at = Some(run.updated_at);
    let output_ref = write_agent_run_output(&run.id, "", &error).await?;
    run.output_ref = Some(output_ref);
    crate::agent_runs::upsert(run).await?;
    crate::events::emit_agent_run_phase_changed(&run.id, previous_status, run.status.as_str())
        .await;
    Ok(())
}

fn next_run_id() -> String {
    format!(
        "ar-{}-{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        Uuid::new_v4()
    )
}

async fn write_agent_run_input(
    run_id: &str,
    prompt: &str,
    cwd: Option<&Path>,
    timeout: Duration,
) -> anyhow::Result<String> {
    let dir = Paths::agent_runs_dir();
    fs::create_dir_all(&dir).await?;
    let path = dir.join(format!("{run_id}-input.json"));
    fs::write(
        &path,
        serde_json::to_string_pretty(&serde_json::json!({
            "agent_id": "claude",
            "role": "assistant",
            "cwd": cwd.map(|path| path.display().to_string()),
            "prompt": prompt,
            "timeout_seconds": timeout.as_secs().clamp(1, 86_400),
            "source": "subagent.claude_p"
        }))?,
    )
    .await?;
    Ok(path.display().to_string())
}

fn output_path_for_run(run_id: &str) -> std::path::PathBuf {
    Paths::agent_runs_dir().join(format!("{run_id}-output.json"))
}

fn stdout_path_for_run(run_id: &str) -> std::path::PathBuf {
    Paths::agent_runs_dir().join(format!("{run_id}-stdout.log"))
}

fn stderr_path_for_run(run_id: &str) -> std::path::PathBuf {
    Paths::agent_runs_dir().join(format!("{run_id}-stderr.log"))
}

async fn read_sidecar(path: &Path) -> anyhow::Result<String> {
    match fs::read_to_string(path).await {
        Ok(content) => Ok(content),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err.into()),
    }
}

async fn write_agent_run_output(
    run_id: &str,
    stdout: &str,
    stderr: &str,
) -> anyhow::Result<String> {
    let path = output_path_for_run(run_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let mut file = fs::File::create(&path).await?;
    file.write_all(
        serde_json::to_string_pretty(&serde_json::json!({
            "stdout": stdout,
            "stderr": stderr
        }))?
        .as_bytes(),
    )
    .await?;
    file.flush().await?;
    Ok(path.display().to_string())
}

enum SpawnFinish {
    Status(ExitStatus),
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

async fn write_log(
    prompt: &str,
    cwd: Option<&Path>,
    result: &SubagentResult,
) -> anyhow::Result<std::path::PathBuf> {
    fs::create_dir_all(Paths::subagent_runs_dir()).await?;
    let path = Paths::subagent_runs_dir().join(format!(
        "{}-claude-p.log",
        Utc::now().format("%Y%m%dT%H%M%SZ")
    ));
    let mut file = fs::File::create(&path).await?;
    file.write_all(format!("cwd: {:?}\n", cwd).as_bytes())
        .await?;
    file.write_all(format!("prompt:\n{prompt}\n\n").as_bytes())
        .await?;
    file.write_all(format!("exit_code: {:?}\n", result.exit_code).as_bytes())
        .await?;
    file.write_all(format!("duration_ms: {}\n\n", result.duration_ms).as_bytes())
        .await?;
    file.write_all(b"stdout:\n").await?;
    file.write_all(result.stdout.as_bytes()).await?;
    file.write_all(b"\n\nstderr:\n").await?;
    file.write_all(result.stderr.as_bytes()).await?;
    file.flush().await?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;
    use std::process::Command;

    #[tokio::test]
    async fn missing_binary_returns_error() {
        let _root = TestRoot::new();
        let result = run_claude_p("test prompt", None, Duration::from_secs(5)).await;
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("spawn claude -p")
                || err_msg.contains("No such file")
                || err_msg.contains("not found"),
            "unexpected error: {err_msg}"
        );
    }

    #[tokio::test]
    async fn missing_binary_with_cwd() {
        let _root = TestRoot::new();
        let cwd = _root.path().to_path_buf();
        let result = run_claude_p("test", Some(&cwd), Duration::from_secs(5)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fails_fast_not_after_timeout() {
        let _root = TestRoot::new();
        let start = Instant::now();
        let _ = run_claude_p("test", None, Duration::from_secs(30)).await;
        let elapsed = start.elapsed();
        // Should fail in < 5s, not wait for the 30s timeout
        assert!(
            elapsed < Duration::from_secs(5),
            "took too long: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn concurrent_calls_release_semaphore() {
        let _root = TestRoot::new();
        let h1 = tokio::spawn(async { run_claude_p("t1", None, Duration::from_secs(5)).await });
        let h2 = tokio::spawn(async { run_claude_p("t2", None, Duration::from_secs(5)).await });
        let h3 = tokio::spawn(async { run_claude_p("t3", None, Duration::from_secs(5)).await });

        let (r1, r2, r3) = tokio::join!(h1, h2, h3);
        // All should complete (none should hang waiting for semaphore)
        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert!(r3.is_ok());
    }

    #[tokio::test]
    async fn result_serialization_roundtrip() {
        let result = SubagentResult {
            agent_run_id: Some("ar-test".to_string()),
            stdout: "hello".to_string(),
            stderr: "world".to_string(),
            exit_code: Some(0),
            duration_ms: 1234,
            log_path: None,
            timed_out: false,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let deserialized: SubagentResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.agent_run_id.as_deref(), Some("ar-test"));
        assert_eq!(deserialized.stdout, "hello");
        assert_eq!(deserialized.stderr, "world");
        assert_eq!(deserialized.exit_code, Some(0));
        assert_eq!(deserialized.duration_ms, 1234);
        assert!(deserialized.log_path.is_none());
        assert!(!deserialized.timed_out);
    }

    #[tokio::test]
    async fn result_serialization_with_none_fields() {
        let result = SubagentResult {
            agent_run_id: None,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            duration_ms: 0,
            log_path: None,
            timed_out: true,
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let deserialized: SubagentResult = serde_json::from_str(&json).expect("deserialize");
        assert!(deserialized.exit_code.is_none());
        assert!(deserialized.timed_out);
    }

    #[tokio::test]
    async fn write_log_creates_file() {
        let _root = TestRoot::new();
        let result = SubagentResult {
            agent_run_id: Some("ar-log".to_string()),
            stdout: "output".to_string(),
            stderr: "errors".to_string(),
            exit_code: Some(0),
            duration_ms: 500,
            log_path: None,
            timed_out: false,
        };
        let path = write_log("test prompt", None, &result)
            .await
            .expect("write_log");
        assert!(path.exists());

        let content = tokio::fs::read_to_string(&path).await.expect("read");
        assert!(content.contains("test prompt"));
        assert!(content.contains("exit_code: Some(0)"));
        assert!(content.contains("duration_ms: 500"));
        assert!(content.contains("output"));
    }

    #[tokio::test]
    async fn missing_binary_creates_failed_agent_run() {
        let _root = TestRoot::new();
        let _ = run_claude_p("test prompt", None, Duration::from_secs(1)).await;

        let runs = crate::agent_runs::list(crate::agent_runs::AgentRunFilter {
            agent_id: Some("claude".to_string()),
            limit: Some(10),
            ..Default::default()
        })
        .await
        .expect("list runs");
        let run = runs
            .iter()
            .find(|run| {
                run.metadata_json
                    .as_ref()
                    .and_then(|value| value.get("source"))
                    .and_then(|value| value.as_str())
                    == Some("subagent.claude_p")
            })
            .expect("subagent run created");
        assert_eq!(run.status, AgentRunStatus::Failed);
        assert!(run.finished_at.is_some());
    }

    #[test]
    fn timed_out_child_is_killed() {
        let mut cmd = sleep_command();
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        let child = cmd.spawn().expect("spawn sleeper");
        let pid = child.id();

        let finish = wait_for_child(child, Duration::from_millis(200));
        assert!(matches!(finish, SpawnFinish::TimedOut));
        std::thread::sleep(Duration::from_millis(200));
        assert!(!process_exists(pid), "timed out child should be gone");
    }

    fn sleep_command() -> Command {
        #[cfg(windows)]
        {
            let mut cmd = Command::new("powershell");
            cmd.args(["-Command", "Start-Sleep -Seconds 5"]);
            cmd
        }

        #[cfg(not(windows))]
        {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", "sleep 5"]);
            cmd
        }
    }

    fn process_exists(pid: u32) -> bool {
        #[cfg(windows)]
        {
            let output = Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}")])
                .output()
                .expect("tasklist");
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains(&pid.to_string())
        }

        #[cfg(not(windows))]
        {
            Command::new("kill")
                .args(["-0", &pid.to_string()])
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
        }
    }
}
