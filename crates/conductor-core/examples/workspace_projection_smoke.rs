use anyhow::Context;
use conductor_core::{
    agent_runs::{self, AgentRun, AgentRunStatus},
    agent_teams::{self, AddAgentTeamMemberInput, AgentTeamLifecycle, CreateAgentTeamInput},
    command_runs::{self, CommandRun, CommandRunStatus},
    goal_tasks, goals, projection,
    tool_calls::{self, ToolCallCreate},
    workspaces,
};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let temp = tempfile::tempdir().context("create temp conductor root")?;
    std::env::set_var("CONDUCTOR_ROOT", temp.path());

    let workspace =
        workspaces::create_or_attach(temp.path(), Some("Projection Smoke".to_string()), None)
            .await
            .context("create workspace")?;

    let goal = goals::create_goal(
        &workspace.id,
        "Projection Smoke Goal",
        "Answer who worked, what ran, and where it landed",
        "normal",
        "smoke",
        None,
        None,
    )
    .await
    .context("create goal")?;
    goals::update_goal_status(&goal.id, "planning")
        .await
        .context("goal planning")?;
    goals::update_goal_status(&goal.id, "awaiting_plan_approval")
        .await
        .context("goal awaiting approval")?;
    goals::update_goal_status(&goal.id, "running")
        .await
        .context("goal running")?;

    let cycle = goals::create_cycle(&goal.id, 1)
        .await
        .context("create cycle")?;
    goals::advance_cycle_phase(&cycle.id, "orienting")
        .await
        .context("cycle orienting")?;
    goals::advance_cycle_phase(&cycle.id, "deciding")
        .await
        .context("cycle deciding")?;
    goals::advance_cycle_phase(&cycle.id, "dispatching")
        .await
        .context("cycle dispatching")?;
    goals::advance_cycle_phase(&cycle.id, "executing")
        .await
        .context("cycle executing")?;

    let task = goal_tasks::create_task(
        &workspace.id,
        Some(&goal.id),
        Some(&cycle.id),
        "Merged projection task",
        "Build a single activity record",
        "claude_p",
        vec![],
        vec![],
        vec![],
        vec![],
        vec!["projection answers who/what/where".to_string()],
    )
    .await
    .context("create task")?;
    goal_tasks::claim_task(&task.id, "agent-projection", 3600)
        .await
        .context("claim task")?;
    goal_tasks::start_task(&task.id)
        .await
        .context("start task")?;

    let team = agent_teams::create_team(CreateAgentTeamInput {
        id: Some(format!("team-{}", cycle.id)),
        name: "Merged Execution Team".to_string(),
        workspace_id: Some(workspace.id.clone()),
        metadata: Some(json!({
            "cycle_id": cycle.id,
        })),
        ..Default::default()
    })
    .await
    .context("create team")?;
    agent_teams::add_member(AddAgentTeamMemberInput {
        team_id: team.id.clone(),
        agent_id: "claude_p:merged".to_string(),
        role: "executor".to_string(),
        run_id: None,
        metadata: Some(json!({
            "task_id": task.id,
        })),
        ..Default::default()
    })
    .await
    .context("add executor")?;
    agent_teams::transition_team_lifecycle(&team.id, AgentTeamLifecycle::Planning)
        .await
        .context("team planning")?;
    agent_teams::transition_team_lifecycle(&team.id, AgentTeamLifecycle::AwaitingPlanApproval)
        .await
        .context("team awaiting approval")?;

    let run = AgentRun {
        id: "ar-projection-smoke".to_string(),
        agent_id: "claude_p".to_string(),
        role: "task_agent".to_string(),
        workspace_id: Some(workspace.id.clone()),
        cwd: Some(workspace.root.clone()),
        status: AgentRunStatus::Running,
        pid: Some(4242),
        command_json: Some(json!({
            "program": "claude",
            "args": ["-p", "projection smoke"],
        })),
        input_ref: None,
        output_ref: Some("runs/ar-projection-smoke-output.json".to_string()),
        error: None,
        started_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        finished_at: None,
        metadata_json: Some(json!({
            "task_id": task.id,
            "prompt": "Projection smoke prompt",
        })),
    };
    agent_runs::upsert(&run).await.context("upsert run")?;
    agent_teams::bind_member_run_to_task(
        &team.id,
        &task.id,
        &run.id,
        Some(json!({
            "agent_run_id": run.id,
            "task_id": task.id,
        })),
    )
    .await
    .context("bind run to team")?;
    agent_teams::handle_plan_approval_response(
        &team.id,
        agent_teams::PlanApprovalVerdict::Approved,
    )
    .await
    .context("approve plan")?;

    let tool_call = tool_calls::create(ToolCallCreate {
        id: "tc-projection-smoke".to_string(),
        session_id: None,
        workspace_id: Some(workspace.id.clone()),
        llm_tool_call_id: Some("llm-projection-smoke".to_string()),
        tool_id: "shell.exec".to_string(),
        input_json: r#"{"command":"cargo check"}"#.to_string(),
        agent_run_id: Some(run.id.clone()),
        risk_level: Some("workspace_write".to_string()),
    })
    .await
    .context("create tool call")?;
    tool_calls::mark_executing(&tool_call.id)
        .await
        .context("mark tool executing")?;

    let mut command_run = CommandRun::new(
        "cargo check".to_string(),
        workspace.root.display().to_string(),
        None,
    );
    command_run.tool_call_id = Some(tool_call.id.clone());
    command_run.agent_run_id = Some(run.id.clone());
    command_run
        .transition(CommandRunStatus::Starting)
        .context("command starting")?;
    command_runs::insert(&command_run)
        .await
        .context("insert command run")?;
    tool_calls::attach_command_run(&tool_call.id, &command_run.id)
        .await
        .context("attach command run")?;

    let projection = projection::list_workspace_activities(&workspace.id, Some(10))
        .await
        .context("build workspace projection")?;
    let merged = projection
        .active
        .iter()
        .find(|item| item.task_id.as_deref() == Some(task.id.as_str()))
        .context("merged goal-task activity")?;

    anyhow::ensure!(
        merged
            .agent_runs
            .iter()
            .any(|agent_run| agent_run.id == run.id && agent_run.agent_id == "claude_p"),
        "merged activity should carry agent_run"
    );
    anyhow::ensure!(
        merged
            .tool_calls
            .iter()
            .any(|tool| tool.id == tool_call.id && tool.tool_id == "shell.exec"),
        "merged activity should carry tool_call"
    );
    anyhow::ensure!(
        merged
            .command_runs
            .iter()
            .any(|command| command.command.contains("cargo check")),
        "merged activity should carry command_run"
    );
    anyhow::ensure!(
        merged.agent_teams.iter().any(|team_ref| {
            team_ref.id == team.id
                && team_ref.name == "Merged Execution Team"
                && team_ref.lifecycle == "executing"
        }),
        "merged activity should carry agent_team"
    );
    anyhow::ensure!(
        merged.artifacts.iter().any(|artifact| {
            artifact.output_ref.as_deref() == Some("runs/ar-projection-smoke-output.json")
        }),
        "merged activity should carry landed artifact"
    );

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "workspace_id": workspace.id,
            "goal_id": goal.id,
            "cycle_id": cycle.id,
            "task_id": task.id,
            "activity_id": merged.activity_id,
            "who": merged.agent_teams,
            "agent_runs": merged.agent_runs,
            "tool_calls": merged.tool_calls,
            "command_runs": merged.command_runs,
            "artifacts": merged.artifacts,
        }))?
    );

    Ok(())
}
