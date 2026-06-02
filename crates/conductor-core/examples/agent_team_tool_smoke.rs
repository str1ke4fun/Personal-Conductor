use anyhow::Context;
use conductor_core::agent_teams::{self, AgentTeamLifecycle};
use conductor_core::tools::{execute_tool, register_builtin_tools};
use serde_json::json;

fn main() -> anyhow::Result<()> {
    let temp = tempfile::tempdir().context("create temp conductor root")?;
    std::env::set_var("CONDUCTOR_ROOT", temp.path());

    register_builtin_tools();
    let runtime = tokio::runtime::Runtime::new().context("create tokio runtime")?;

    execute_tool(
        "agent.team.create",
        &json!({
            "id": "team-tool-smoke",
            "name": "Tool Smoke Team"
        }),
    )
    .context("create tool team")?;

    execute_tool(
        "agent.team.add_member",
        &json!({
            "team_id": "team-tool-smoke",
            "agent_id": "executor",
            "role": "executor",
            "run_id": "ar-tool-smoke",
            "metadata": {
                "task_id": "task-tool-smoke",
                "external_session_id": "session-tool-smoke"
            }
        }),
    )
    .context("add tool executor")?;

    runtime
        .block_on(agent_teams::transition_team_lifecycle(
            "team-tool-smoke",
            AgentTeamLifecycle::Planning,
        ))
        .context("transition team to planning")?;
    runtime
        .block_on(agent_teams::transition_team_lifecycle(
            "team-tool-smoke",
            AgentTeamLifecycle::AwaitingPlanApproval,
        ))
        .context("transition team to awaiting plan approval")?;

    let planned = execute_tool(
        "agent.team.plan_verdict",
        &json!({
            "team_id": "team-tool-smoke",
            "verdict": "approved"
        }),
    )
    .context("submit plan verdict tool")?;

    runtime
        .block_on(agent_teams::transition_team_lifecycle(
            "team-tool-smoke",
            AgentTeamLifecycle::AwaitingReview,
        ))
        .context("transition team to awaiting review")?;

    let reviewed = execute_tool(
        "agent.team.review_verdict",
        &json!({
            "team_id": "team-tool-smoke",
            "verdict": "accepted"
        }),
    )
    .context("submit review verdict tool")?;

    let snapshot = execute_tool(
        "agent.team.snapshot",
        &json!({
            "team_id": "team-tool-smoke",
            "message_limit": 10
        }),
    )
    .context("snapshot tool team")?;

    anyhow::ensure!(planned.success, "plan verdict should succeed");
    anyhow::ensure!(reviewed.success, "review verdict should succeed");
    anyhow::ensure!(
        snapshot.output["team"]["lifecycle"] == "accepted",
        "expected accepted lifecycle, got {}",
        snapshot.output["team"]["lifecycle"]
    );

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "team_id": snapshot.output["team"]["id"],
            "team_lifecycle": snapshot.output["team"]["lifecycle"],
            "planned_lifecycle": planned.output["lifecycle"],
            "reviewed_lifecycle": reviewed.output["lifecycle"],
            "member_run_id": snapshot.output["members"][0]["run_id"],
            "member_task_id": snapshot.output["members"][0]["metadata_json"]["task_id"],
        }))?
    );

    Ok(())
}
