pub mod bash_provider;
pub mod cmd_provider;
pub mod ps_provider;
pub mod security;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::Mutex as TokioMutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellProvider {
    Cmd,
    Powershell,
    Bash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellRequest {
    pub command: String,
    pub provider: ShellProvider,
    pub working_dir: Option<String>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShellEvent {
    Stdout(String),
    Stderr(String),
    Exit(i32),
    Timeout,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
}

pub struct ShellExecutor;

impl ShellExecutor {
    pub fn new() -> Self {
        Self
    }

    /// Spawn a command tracked by a `CommandRun`.
    ///
    /// Returns the spawned process info (pid) and reader tasks. The caller
    /// is responsible for waiting on the child and updating the `CommandRun`
    /// status when it exits.
    ///
    /// The `command_run` is registered in the live registry on entry and
    /// unregistered when the returned `SpawnedProcess` is awaited to completion.
    pub async fn execute_tracked(
        &self,
        req: ShellRequest,
        command_run: Arc<TokioMutex<crate::command_runs::CommandRun>>,
    ) -> Result<SpawnedProcess> {
        security::validate_command(&req.command)?;
        security::validate_working_dir(&req.command, req.working_dir.as_deref())?;

        let mut cmd: Command = match req.provider {
            ShellProvider::Cmd => {
                cmd_provider::build_command(&req.command, req.working_dir.as_deref())?
            }
            ShellProvider::Powershell => {
                ps_provider::build_command(&req.command, req.working_dir.as_deref())?
            }
            ShellProvider::Bash => {
                bash_provider::build_command(&req.command, req.working_dir.as_deref())?
            }
        };

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("failed to spawn shell process")?;
        let pid = child.id().unwrap_or(0) as i64;

        // Update CommandRun with pid and transition to Streaming
        {
            let mut run = command_run.lock().await;
            run.pid = Some(pid);
            run.transition(crate::command_runs::CommandRunStatus::Starting)?;
            run.transition(crate::command_runs::CommandRunStatus::Streaming)?;
        }

        // Register as live
        crate::command_runs::register_live(Arc::clone(&command_run)).await;

        let stdout_handle = child.stdout.take().expect("stdout was piped");
        let stderr_handle = child.stderr.take().expect("stderr was piped");

        let stdout_buf = Arc::new(Mutex::new(String::new()));
        let stderr_buf = Arc::new(Mutex::new(String::new()));

        let stdout_clone = Arc::clone(&stdout_buf);
        let stderr_clone = Arc::clone(&stderr_buf);
        let cr_stdout = Arc::clone(&command_run);
        let cr_stderr = Arc::clone(&command_run);

        let stdout_task = tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stdout_handle);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(mut buf) = stdout_clone.lock() {
                    buf.push_str(&line);
                    buf.push('\n');
                }
                // Update CommandRun stdout_tail (keep last 8KB)
                if let Ok(mut run) = cr_stdout.try_lock() {
                    run.stdout_tail.push_str(&line);
                    run.stdout_tail.push('\n');
                    // Trim to last 8KB
                    if run.stdout_tail.len() > 8192 {
                        let trim = run.stdout_tail.len() - 8192;
                        run.stdout_tail = run.stdout_tail[trim..].to_string();
                    }
                }
            }
        });

        let stderr_task = tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stderr_handle);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(mut buf) = stderr_clone.lock() {
                    buf.push_str(&line);
                    buf.push('\n');
                }
                // Update CommandRun stderr_tail (keep last 8KB)
                if let Ok(mut run) = cr_stderr.try_lock() {
                    run.stderr_tail.push_str(&line);
                    run.stderr_tail.push('\n');
                    if run.stderr_tail.len() > 8192 {
                        let trim = run.stderr_tail.len() - 8192;
                        run.stderr_tail = run.stderr_tail[trim..].to_string();
                    }
                }
            }
        });

        Ok(SpawnedProcess {
            child,
            command_run,
            stdout_buf,
            stderr_buf,
            stdout_task,
            stderr_task,
            timeout_duration: Duration::from_secs(req.timeout_secs.unwrap_or(120)),
        })
    }

    pub async fn execute(&self, req: ShellRequest) -> Result<ShellResult> {
        security::validate_command(&req.command)?;
        security::validate_working_dir(&req.command, req.working_dir.as_deref())?;

        let mut cmd: Command = match req.provider {
            ShellProvider::Cmd => {
                cmd_provider::build_command(&req.command, req.working_dir.as_deref())?
            }
            ShellProvider::Powershell => {
                ps_provider::build_command(&req.command, req.working_dir.as_deref())?
            }
            ShellProvider::Bash => {
                bash_provider::build_command(&req.command, req.working_dir.as_deref())?
            }
        };

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().context("failed to spawn shell process")?;

        let stdout_handle = child.stdout.take().expect("stdout was piped");
        let stderr_handle = child.stderr.take().expect("stderr was piped");

        let stdout_buf = Arc::new(Mutex::new(String::new()));
        let stderr_buf = Arc::new(Mutex::new(String::new()));

        let stdout_clone = Arc::clone(&stdout_buf);
        let stderr_clone = Arc::clone(&stderr_buf);

        let stdout_task = tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stdout_handle);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(mut buf) = stdout_clone.lock() {
                    buf.push_str(&line);
                    buf.push('\n');
                }
            }
        });

        let stderr_task = tokio::spawn(async move {
            let reader = tokio::io::BufReader::new(stderr_handle);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(mut buf) = stderr_clone.lock() {
                    buf.push_str(&line);
                    buf.push('\n');
                }
            }
        });

        let timeout_duration = Duration::from_secs(req.timeout_secs.unwrap_or(120));

        let timed_out = match tokio::time::timeout(timeout_duration, child.wait()).await {
            Ok(Ok(status)) => {
                // Wait for reader tasks to finish flushing
                let _ = tokio::join!(stdout_task, stderr_task);
                let stdout = stdout_buf.lock().unwrap().clone();
                let stderr = stderr_buf.lock().unwrap().clone();
                return Ok(ShellResult {
                    stdout,
                    stderr,
                    exit_code: status.code().unwrap_or(-1),
                    timed_out: false,
                });
            }
            Ok(Err(e)) => {
                let _ = tokio::join!(stdout_task, stderr_task);
                return Err(e).context("failed to wait for child process");
            }
            Err(_) => {
                // Timeout — kill the child
                let _ = child.kill().await;
                let _ = tokio::join!(stdout_task, stderr_task);
                true
            }
        };

        let stdout = stdout_buf.lock().unwrap().clone();
        let stderr = stderr_buf.lock().unwrap().clone();

        Ok(ShellResult {
            stdout,
            stderr,
            exit_code: -1,
            timed_out,
        })
    }
}

impl Default for ShellExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle to a spawned tracked process.
///
/// Call `wait()` to wait for the process to exit and get the final `ShellResult`.
/// The `CommandRun` is automatically updated and persisted on completion.
pub struct SpawnedProcess {
    child: tokio::process::Child,
    command_run: Arc<TokioMutex<crate::command_runs::CommandRun>>,
    stdout_buf: Arc<Mutex<String>>,
    stderr_buf: Arc<Mutex<String>>,
    stdout_task: tokio::task::JoinHandle<()>,
    stderr_task: tokio::task::JoinHandle<()>,
    timeout_duration: Duration,
}

impl SpawnedProcess {
    /// Wait for the process to exit (with timeout), update the `CommandRun`,
    /// persist it, and return the `ShellResult`.
    pub async fn wait(mut self) -> Result<ShellResult> {
        let run_id = self.command_run.lock().await.id.clone();

        // Destructure to avoid partial move issues with tokio::join!
        let Self {
            ref mut child,
            ref command_run,
            ref stdout_buf,
            ref stderr_buf,
            stdout_task,
            stderr_task,
            timeout_duration,
        } = self;

        let timed_out = match tokio::time::timeout(timeout_duration, child.wait()).await {
            Ok(Ok(status)) => {
                let _ = tokio::join!(stdout_task, stderr_task);
                let stdout = stdout_buf.lock().unwrap().clone();
                let stderr = stderr_buf.lock().unwrap().clone();

                // Update CommandRun
                {
                    let mut run = command_run.lock().await;
                    run.exit_code = status.code();
                    run.stdout_tail = tail_str(&stdout, 8192);
                    run.stderr_tail = tail_str(&stderr, 8192);
                    let _ = run.transition(crate::command_runs::CommandRunStatus::Exited);
                }
                persist_and_unregister(command_run, &run_id).await;

                return Ok(ShellResult {
                    stdout,
                    stderr,
                    exit_code: status.code().unwrap_or(-1),
                    timed_out: false,
                });
            }
            Ok(Err(e)) => {
                let _ = tokio::join!(stdout_task, stderr_task);
                return Err(e).context("failed to wait for child process");
            }
            Err(_) => {
                let _ = child.kill().await;
                let _ = tokio::join!(stdout_task, stderr_task);

                // Update CommandRun as TimedOut
                {
                    let mut run = command_run.lock().await;
                    run.exit_code = Some(-1);
                    let stdout = stdout_buf.lock().unwrap().clone();
                    let stderr = stderr_buf.lock().unwrap().clone();
                    run.stdout_tail = tail_str(&stdout, 8192);
                    run.stderr_tail = tail_str(&stderr, 8192);
                    let _ = run.transition(crate::command_runs::CommandRunStatus::TimedOut);
                }
                persist_and_unregister(command_run, &run_id).await;

                true
            }
        };

        let stdout = stdout_buf.lock().unwrap().clone();
        let stderr = stderr_buf.lock().unwrap().clone();

        Ok(ShellResult {
            stdout,
            stderr,
            exit_code: -1,
            timed_out,
        })
    }
}

async fn persist_and_unregister(
    command_run: &Arc<TokioMutex<crate::command_runs::CommandRun>>,
    run_id: &str,
) {
    let run = command_run.lock().await.clone();
    let _ = crate::command_runs::update(&run).await;
    crate::command_runs::unregister_live(run_id).await;
}

/// Take the last `max_len` characters of a string, preserving UTF-8 boundaries.
fn tail_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let mut start = s.len() - max_len;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    s[start..].to_string()
}
