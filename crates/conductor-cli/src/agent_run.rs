use anyhow::Context;
use conductor_core::{
    adapters::claude_p::{AgentRunRef, ClaudePAdapter, ClaudePConfig},
    goal_tasks::AgentTask,
    paths::Paths,
};
use serde::Deserialize;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct AgentRunnerConfig {
    pub runtime_url: Option<String>,
    pub token: Option<String>,
    pub agent_id: String,
    pub workspace_id: Option<String>,
    pub lease_ttl_seconds: i64,
    pub poll_interval_ms: u64,
    pub once: bool,
    pub claude_binary: String,
    pub timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeApiSnapshot {
    base_url: String,
    token: String,
    running: bool,
}

pub async fn run(config: AgentRunnerConfig) -> anyhow::Result<()> {
    let resolved = ResolvedRunnerConfig::resolve(config).await?;
    let client = reqwest::Client::new();

    // ClaudeP and Codex adapters are only used when explicitly requested
    // via task.agent_kind = "claude_p" / "codex".
    let claude_p = ClaudePAdapter::new(ClaudePConfig {
        runtime_api_url: resolved.runtime_url.clone(),
        claude_binary: resolved.claude_binary.clone(),
        default_timeout_seconds: resolved.timeout_seconds,
    });

    loop {
        match claim_next_task(&client, &resolved).await? {
            Some(task) => {
                let started = start_task(&client, &resolved, &task.id).await?;
                let execution = if started.agent_kind == "claude_p" {
                    claude_p
                        .spawn(started.clone(), resolved.token.clone())
                        .await
                        .with_context(|| format!("claude_p spawn for task {}", started.id))
                } else {
                    execute_via_conductor_api(&started, &resolved, &client)
                        .await
                        .with_context(|| format!("conductor api execute for task {}", started.id))
                };

                match execution {
                    Ok(run_ref) => {
                        print_spawned_task(&started, &run_ref)?;
                    }
                    Err(err) => {
                        let failure_summary = format!("{err:#}");
                        if let Err(fail_err) = fail_task_via_runtime_api(
                            &client,
                            &resolved,
                            &started.id,
                            &failure_summary,
                        )
                        .await
                        {
                            eprintln!(
                                "failed to mark task {} as failed after execution error: {fail_err:#}",
                                started.id
                            );
                        }
                        if resolved.once {
                            return Err(err);
                        }
                    }
                }
            }
            None if resolved.once => break,
            None => tokio::time::sleep(Duration::from_millis(resolved.poll_interval_ms)).await,
        }
    }

    Ok(())
}

#[derive(Clone, Debug)]
struct ResolvedRunnerConfig {
    runtime_url: String,
    token: String,
    agent_id: String,
    workspace_id: Option<String>,
    lease_ttl_seconds: i64,
    poll_interval_ms: u64,
    once: bool,
    claude_binary: String,
    timeout_seconds: u64,
}

impl ResolvedRunnerConfig {
    async fn resolve(config: AgentRunnerConfig) -> anyhow::Result<Self> {
        let snapshot = if config.runtime_url.is_some() && config.token.is_some() {
            None
        } else {
            Some(load_runtime_snapshot().await?)
        };

        let runtime_url = config
            .runtime_url
            .or_else(|| snapshot.as_ref().map(|state| state.base_url.clone()))
            .context("missing runtime URL; pass --runtime-url or start the desktop app first")?;
        let token = config
            .token
            .or_else(|| snapshot.as_ref().map(|state| state.token.clone()))
            .context("missing runtime token; pass --token or start the desktop app first")?;

        Ok(Self {
            runtime_url,
            token,
            agent_id: config.agent_id,
            workspace_id: config.workspace_id,
            lease_ttl_seconds: config.lease_ttl_seconds,
            poll_interval_ms: config.poll_interval_ms,
            once: config.once,
            claude_binary: config.claude_binary,
            timeout_seconds: config.timeout_seconds,
        })
    }
}

async fn load_runtime_snapshot() -> anyhow::Result<RuntimeApiSnapshot> {
    let bytes = tokio::fs::read(Paths::runtime_api_state_json())
        .await
        .context("read runtime-api.json")?;
    let snapshot: RuntimeApiSnapshot =
        serde_json::from_slice(&bytes).context("parse runtime-api.json")?;
    if !snapshot.running {
        anyhow::bail!("runtime API is not marked running; start the desktop app first");
    }
    Ok(snapshot)
}

async fn claim_next_task(
    client: &reqwest::Client,
    config: &ResolvedRunnerConfig,
) -> anyhow::Result<Option<AgentTask>> {
    let response = client
        .post(format!("{}/runtime/tasks/claim", config.runtime_url))
        .bearer_auth(&config.token)
        .json(&serde_json::json!({
            "agent_id": config.agent_id,
            "lease_ttl_seconds": config.lease_ttl_seconds,
            "workspace_id": config.workspace_id,
        }))
        .send()
        .await
        .context("claim next task request")?;

    match response.status() {
        reqwest::StatusCode::NOT_FOUND => Ok(None),
        status if status.is_success() => Ok(Some(response.json::<AgentTask>().await?)),
        status => {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("claim next task failed with {status}: {body}");
        }
    }
}

async fn start_task(
    client: &reqwest::Client,
    config: &ResolvedRunnerConfig,
    task_id: &str,
) -> anyhow::Result<AgentTask> {
    let response = client
        .post(format!(
            "{}/runtime/tasks/{task_id}/start",
            config.runtime_url
        ))
        .bearer_auth(&config.token)
        .send()
        .await
        .with_context(|| format!("start task request for {task_id}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("start task failed with {status}: {body}");
    }

    Ok(response.json::<AgentTask>().await?)
}

fn print_spawned_task(task: &AgentTask, run_ref: &AgentRunRef) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "task_id": task.id,
            "title": task.title,
            "workspace_id": task.workspace_id,
            "status": task.status,
            "run_id": run_ref.run_id,
            "pid": run_ref.pid,
        }))?
    );
    Ok(())
}

// Execute a backend-agent task by calling the desktop runtime execute endpoint.
async fn execute_via_conductor_api(
    task: &AgentTask,
    config: &ResolvedRunnerConfig,
    client: &reqwest::Client,
) -> anyhow::Result<AgentRunRef> {
    let response = client
        .post(format!(
            "{}/runtime/tasks/{}/execute",
            config.runtime_url, task.id
        ))
        .bearer_auth(&config.token)
        .send()
        .await
        .with_context(|| format!("execute task request for {}", task.id))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("execute task failed with {status}: {body}");
    }

    Ok(AgentRunRef {
        run_id: format!("desktop-exec-{}", task.id),
        task_id: task.id.clone(),
        workspace_id: task.workspace_id.clone(),
        status: conductor_core::agent_runs::AgentRunStatus::Running,
        pid: None,
    })
}

async fn fail_task_via_runtime_api(
    client: &reqwest::Client,
    config: &ResolvedRunnerConfig,
    task_id: &str,
    error: &str,
) -> anyhow::Result<AgentTask> {
    let response = client
        .post(format!(
            "{}/runtime/tasks/{task_id}/fail",
            config.runtime_url
        ))
        .bearer_auth(&config.token)
        .json(&serde_json::json!({
            "error": error,
        }))
        .send()
        .await
        .with_context(|| format!("fail task request for {task_id}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("fail task failed with {status}: {body}");
    }

    Ok(response.json::<AgentTask>().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use conductor_core::{
        goal_tasks,
        paths::Paths,
        runtime_api::{generate_runtime_token, RuntimeApiServer},
    };
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

    async fn write_runtime_snapshot(base_url: &str, token: &str, running: bool) {
        tokio::fs::create_dir_all(conductor_core::paths::state())
            .await
            .expect("create state dir");
        tokio::fs::write(
            Paths::runtime_api_state_json(),
            serde_json::to_vec_pretty(&serde_json::json!({
                "bind": "127.0.0.1",
                "port": 1234,
                "baseUrl": base_url,
                "token": token,
                "running": running,
                "updatedAt": "2026-05-31T00:00:00Z"
            }))
            .expect("serialize snapshot"),
        )
        .await
        .expect("write runtime snapshot");
    }

    #[tokio::test]
    async fn resolve_runner_config_uses_runtime_snapshot_when_flags_missing() {
        let _root = TestRoot::new();
        write_runtime_snapshot("http://127.0.0.1:9999", "snapshot-token", true).await;

        let resolved = ResolvedRunnerConfig::resolve(AgentRunnerConfig {
            runtime_url: None,
            token: None,
            agent_id: "runner-a".to_string(),
            workspace_id: Some("ws-1".to_string()),
            lease_ttl_seconds: 120,
            poll_interval_ms: 500,
            once: true,
            claude_binary: "claude".to_string(),
            timeout_seconds: 60,
        })
        .await
        .expect("resolve from snapshot");

        assert_eq!(resolved.runtime_url, "http://127.0.0.1:9999");
        assert_eq!(resolved.token, "snapshot-token");
        assert_eq!(resolved.agent_id, "runner-a");
        assert_eq!(resolved.workspace_id.as_deref(), Some("ws-1"));
        assert!(resolved.once);
    }

    #[tokio::test]
    async fn load_runtime_snapshot_rejects_stopped_runtime() {
        let _root = TestRoot::new();
        write_runtime_snapshot("http://127.0.0.1:9999", "snapshot-token", false).await;

        let err = load_runtime_snapshot()
            .await
            .expect_err("stopped snapshot should fail");
        assert!(err.to_string().contains("not marked running"));
    }

    #[tokio::test]
    async fn claim_and_start_task_round_trip_through_runtime_api() {
        let _root = TestRoot::new();
        let token = generate_runtime_token();
        let mut server = RuntimeApiServer::new("127.0.0.1", 0, &token);
        server.start().await.expect("start runtime api");
        let base_url = format!(
            "http://{}",
            server.local_addr().expect("runtime local addr")
        );

        let task = goal_tasks::create_task(
            "ws-runner",
            None,
            None,
            "Runner task",
            "Execute from runtime runner",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create task");
        let queued = goal_tasks::rework_task(&task.id).await.expect("queue task");

        let client = reqwest::Client::new();
        let config = ResolvedRunnerConfig {
            runtime_url: base_url,
            token,
            agent_id: "runner-test".to_string(),
            workspace_id: Some("ws-runner".to_string()),
            lease_ttl_seconds: 300,
            poll_interval_ms: 50,
            once: true,
            claude_binary: "claude".to_string(),
            timeout_seconds: 60,
        };

        let claimed = claim_next_task(&client, &config)
            .await
            .expect("claim request")
            .expect("queued task should be claimed");
        assert_eq!(claimed.id, queued.id);
        assert_eq!(claimed.status, "claimed");
        assert_eq!(claimed.claimed_by.as_deref(), Some("runner-test"));

        let started = start_task(&client, &config, &claimed.id)
            .await
            .expect("start request");
        assert_eq!(started.id, claimed.id);
        assert_eq!(started.status, "running");

        server.stop();
    }

    #[tokio::test]
    async fn fail_task_round_trip_through_runtime_api() {
        let _root = TestRoot::new();
        let token = generate_runtime_token();
        let mut server = RuntimeApiServer::new("127.0.0.1", 0, &token);
        server.start().await.expect("start runtime api");
        let base_url = format!(
            "http://{}",
            server.local_addr().expect("runtime local addr")
        );

        let task = goal_tasks::create_task(
            "ws-runner-fail",
            None,
            None,
            "Runner fail task",
            "Execute from runtime runner",
            "backend-agent",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create task");
        let queued = goal_tasks::rework_task(&task.id).await.expect("queue task");

        let client = reqwest::Client::new();
        let config = ResolvedRunnerConfig {
            runtime_url: base_url,
            token,
            agent_id: "runner-test".to_string(),
            workspace_id: Some("ws-runner-fail".to_string()),
            lease_ttl_seconds: 300,
            poll_interval_ms: 50,
            once: true,
            claude_binary: "claude".to_string(),
            timeout_seconds: 60,
        };

        let claimed = claim_next_task(&client, &config)
            .await
            .expect("claim request")
            .expect("queued task should be claimed");
        let started = start_task(&client, &config, &claimed.id)
            .await
            .expect("start request");
        assert_eq!(started.status, "running");

        let failed = fail_task_via_runtime_api(&client, &config, &queued.id, "boom")
            .await
            .expect("fail request");
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.error.as_deref(), Some("boom"));

        server.stop();
    }
}
