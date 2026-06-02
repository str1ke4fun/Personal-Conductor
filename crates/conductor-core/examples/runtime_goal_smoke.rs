use anyhow::Context;
use conductor_core::{
    adapters::claude_p::{ClaudePAdapter, ClaudePConfig},
    agent_teams::AgentTeamLifecycle,
    goal_orchestrator::{GoalOrchestrator, OrchestratorConfig},
    runtime_api::RuntimeApiServer,
};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::var("RUNTIME_GOAL_SMOKE_FAKE_CLAUDE").as_deref() == Ok("1") {
        println!(
            "## Summary\nruntime smoke adapter path\n\n## Changes\n- bound team member run id\n\n## Risks\n- none\n\n## Next Steps\n- review result"
        );
        return Ok(());
    }

    let temp = tempfile::tempdir().context("create temp conductor root")?;
    std::env::set_var("CONDUCTOR_ROOT", temp.path());

    let token = "runtime-goal-smoke-token";
    let mut server = RuntimeApiServer::new("127.0.0.1", 0, token);
    server.start().await?;
    let base_url = format!(
        "http://{}",
        server.local_addr().context("runtime local addr")?
    );
    let client = reqwest::Client::new();

    let created_goal: conductor_core::goals::GoalRun = post_json(
        &client,
        &base_url,
        token,
        "/runtime/goals",
        &json!({
            "workspace_id": "ws-smoke",
            "title": "Runtime Goal Smoke",
            "objective": "prove runtime goal -> agent team lifecycle",
            "priority": "normal",
            "owner": "smoke",
        }),
    )
    .await?;

    let _: conductor_core::goals::GoalRun = post_empty(
        &client,
        &base_url,
        token,
        &format!("/runtime/goals/{}/start", created_goal.id),
    )
    .await?;

    let orchestrator = GoalOrchestrator::new(OrchestratorConfig {
        workspace_id: "ws-smoke".to_string(),
        ..Default::default()
    });

    for _ in 0..4 {
        orchestrator.tick_goal(&created_goal.id).await?;
    }

    let waiting_goal = conductor_core::goals::get_goal(&created_goal.id)
        .await?
        .context("waiting goal")?;
    let waiting_cycle = conductor_core::goals::get_cycle(
        waiting_goal
            .current_cycle_id
            .as_deref()
            .context("waiting cycle id")?,
    )
    .await?
    .context("waiting cycle")?;
    let waiting_team =
        conductor_core::agent_teams::get_team(&format!("team-{}", waiting_cycle.id)).await?;

    ensure(
        waiting_goal.status == "awaiting_plan_approval",
        "goal should wait for plan approval",
    )?;
    ensure(
        waiting_cycle.status == "dispatching",
        "cycle should pause at dispatching until approval",
    )?;
    ensure(
        waiting_team.lifecycle == AgentTeamLifecycle::AwaitingPlanApproval,
        "team should exist as collaboration container before execution",
    )?;

    let _: conductor_core::goals::GoalRun = post_empty(
        &client,
        &base_url,
        token,
        &format!("/runtime/goals/{}/approve-plan", created_goal.id),
    )
    .await?;
    orchestrator.tick_goal(&created_goal.id).await?;

    let running_goal = conductor_core::goals::get_goal(&created_goal.id)
        .await?
        .context("running goal")?;
    let cycle_id = running_goal
        .current_cycle_id
        .clone()
        .context("running cycle id")?;
    let running_cycle = conductor_core::goals::get_cycle(&cycle_id)
        .await?
        .context("running cycle")?;
    let running_team = conductor_core::agent_teams::get_team(&format!("team-{cycle_id}")).await?;
    let cycle_tasks = conductor_core::goal_tasks::list_tasks_by_cycle(&cycle_id).await?;
    let task = cycle_tasks
        .first()
        .cloned()
        .context("task created after approval")?;

    ensure(
        running_goal.status == "running",
        "goal should enter running after approval",
    )?;
    ensure(
        running_cycle.status == "executing",
        "cycle should enter executing after approval",
    )?;
    ensure(
        running_team.lifecycle == AgentTeamLifecycle::Executing,
        "team should enter executing after approval",
    )?;
    ensure(!cycle_tasks.is_empty(), "tasks should exist after approval")?;

    let _: conductor_core::goal_tasks::AgentTask = post_json(
        &client,
        &base_url,
        token,
        "/runtime/tasks/claim",
        &json!({
            "agent_id": "smoke-agent",
            "lease_ttl_seconds": 300,
            "workspace_id": "ws-smoke",
        }),
    )
    .await?;
    let _: conductor_core::goal_tasks::AgentTask = post_empty(
        &client,
        &base_url,
        token,
        &format!("/runtime/tasks/{}/start", task.id),
    )
    .await?;

    let fake_claude = create_fake_claude_binary()?;
    let adapter = ClaudePAdapter::new(ClaudePConfig {
        runtime_api_url: base_url.clone(),
        claude_binary: fake_claude.to_string_lossy().to_string(),
        default_timeout_seconds: 5,
    });
    let run_ref = adapter.spawn(task.clone(), token.to_string()).await?;

    let bound_member =
        wait_for_member_run_id(&format!("team-{cycle_id}"), &task.id, &run_ref.run_id).await?;
    ensure(
        bound_member.run_id.as_deref() == Some(run_ref.run_id.as_str()),
        "executor member should be bound to spawned run_id",
    )?;

    wait_for_task_status(&task.id, "review_ready").await?;

    orchestrator.tick_goal(&created_goal.id).await?;

    let review_goal = conductor_core::goals::get_goal(&created_goal.id)
        .await?
        .context("review goal")?;
    let review_cycle = conductor_core::goals::get_cycle(&cycle_id)
        .await?
        .context("review cycle")?;
    let review_team = conductor_core::agent_teams::get_team(&format!("team-{cycle_id}")).await?;

    ensure(
        review_goal.status == "awaiting_review",
        "goal should wait for review",
    )?;
    ensure(
        review_cycle.status == "reviewing",
        "cycle should wait in reviewing",
    )?;
    ensure(
        review_team.lifecycle == AgentTeamLifecycle::AwaitingReview,
        "team should wait in awaiting_review",
    )?;

    let _: conductor_core::goals::GoalRun = post_json(
        &client,
        &base_url,
        token,
        &format!("/runtime/goals/{}/review-verdict", created_goal.id),
        &json!({ "verdict": "accepted" }),
    )
    .await?;

    let accepted_goal = conductor_core::goals::get_goal(&created_goal.id)
        .await?
        .context("accepted goal")?;
    let accepted_cycle = conductor_core::goals::get_cycle(&cycle_id)
        .await?
        .context("accepted cycle")?;
    let accepted_team = conductor_core::agent_teams::get_team(&format!("team-{cycle_id}")).await?;

    ensure(
        accepted_goal.status == "accepted",
        "goal should be accepted",
    )?;
    ensure(
        accepted_cycle.status == "completed",
        "cycle should be completed",
    )?;
    ensure(
        accepted_team.lifecycle == AgentTeamLifecycle::Accepted,
        "team should be accepted after review",
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "base_url": base_url,
            "goal_id": created_goal.id,
            "cycle_id": cycle_id,
            "task_id": task.id,
            "run_id": run_ref.run_id,
            "statuses": {
                "goal": accepted_goal.status,
                "cycle": accepted_cycle.status,
                "team": accepted_team.lifecycle.as_str(),
            }
        }))?
    );

    server.stop();
    Ok(())
}

async fn post_empty<T: DeserializeOwned>(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
    path: &str,
) -> anyhow::Result<T> {
    let response = client
        .post(format!("{base_url}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .with_context(|| format!("POST {path}"))?;
    decode_json(response, path).await
}

async fn post_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
    path: &str,
    body: &serde_json::Value,
) -> anyhow::Result<T> {
    let response = client
        .post(format!("{base_url}{path}"))
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .with_context(|| format!("POST {path}"))?;
    decode_json(response, path).await
}

async fn decode_json<T: DeserializeOwned>(
    response: reqwest::Response,
    path: &str,
) -> anyhow::Result<T> {
    let status = response.status();
    if status != StatusCode::OK && status != StatusCode::CREATED {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("{path} failed with {status}: {body}");
    }
    Ok(response.json::<T>().await?)
}

fn ensure(condition: bool, message: &str) -> anyhow::Result<()> {
    if condition {
        Ok(())
    } else {
        anyhow::bail!(message.to_string())
    }
}

async fn wait_for_task_status(task_id: &str, expected: &str) -> anyhow::Result<()> {
    for _ in 0..50 {
        let task = conductor_core::goal_tasks::get_task(task_id)
            .await?
            .context("task while waiting for status")?;
        if task.status == expected {
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }

    let task = conductor_core::goal_tasks::get_task(task_id)
        .await?
        .context("task after waiting for status")?;
    anyhow::bail!(
        "task {task_id} did not reach status {expected}, current status {}",
        task.status
    )
}

async fn wait_for_member_run_id(
    team_id: &str,
    task_id: &str,
    expected_run_id: &str,
) -> anyhow::Result<conductor_core::agent_teams::AgentTeamMember> {
    for _ in 0..20 {
        let members = conductor_core::agent_teams::list_members(team_id).await?;
        if let Some(member) = members.into_iter().find(|member| {
            member
                .metadata_json
                .as_ref()
                .and_then(|value| value.get("task_id"))
                .and_then(|value| value.as_str())
                == Some(task_id)
                && member.run_id.as_deref() == Some(expected_run_id)
        }) {
            return Ok(member);
        }
        sleep(Duration::from_millis(50)).await;
    }

    anyhow::bail!("team member for task {task_id} did not bind run_id {expected_run_id}")
}

fn create_fake_claude_binary() -> anyhow::Result<std::path::PathBuf> {
    std::env::set_var("RUNTIME_GOAL_SMOKE_FAKE_CLAUDE", "1");
    std::env::current_exe().context("resolve current example binary as fake claude")
}
