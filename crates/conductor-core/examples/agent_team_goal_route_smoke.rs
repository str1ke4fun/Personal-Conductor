use conductor_core::{
    agent_teams::{self, PlanApprovalVerdict, ReviewVerdict},
    goal_orchestrator::{GoalOrchestrator, OrchestratorConfig},
    goals,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    std::env::set_var("CONDUCTOR_ROOT", temp.path());

    let goal = goals::create_goal(
        "ws-team-route",
        "Goal-linked team routing",
        "Prove agent team verdicts route through goal execution",
        "normal",
        "test",
        None,
        None,
    )
    .await?;
    goals::update_goal_status(&goal.id, "planning").await?;

    let orchestrator = GoalOrchestrator::new(OrchestratorConfig {
        workspace_id: "ws-team-route".to_string(),
        ..Default::default()
    });

    for _ in 0..4 {
        orchestrator.tick_goal(&goal.id).await?;
    }

    let waiting_goal = goals::get_goal(&goal.id).await?.expect("goal exists");
    let cycle_id = waiting_goal
        .current_cycle_id
        .clone()
        .expect("current cycle id");
    let team_id = format!("team-{cycle_id}");
    let plan_mailbox = agent_teams::list_mailbox(agent_teams::AgentMailboxFilter {
        team_id: team_id.clone(),
        include_read: true,
        ..Default::default()
    })
    .await?;

    let plan_team =
        agent_teams::handle_plan_approval_response(&team_id, PlanApprovalVerdict::Approved).await?;
    let task = conductor_core::goal_tasks::list_tasks_by_cycle(&cycle_id)
        .await?
        .into_iter()
        .next()
        .expect("task exists after approval");
    conductor_core::goal_tasks::claim_task(&task.id, "agent-team-route", 300).await?;
    conductor_core::goal_tasks::start_task(&task.id).await?;
    conductor_core::goal_tasks::complete_task(&task.id, "result/ref").await?;
    orchestrator.tick_goal(&goal.id).await?;

    let review_team = agent_teams::get_team(&team_id).await?;
    let review_mailbox = agent_teams::list_mailbox(agent_teams::AgentMailboxFilter {
        team_id: team_id.clone(),
        include_read: true,
        ..Default::default()
    })
    .await?;
    let accepted_team =
        agent_teams::handle_review_verdict(&team_id, ReviewVerdict::Accepted).await?;
    let accepted_goal = goals::get_goal(&goal.id)
        .await?
        .expect("accepted goal exists");
    let accepted_cycle = goals::get_cycle(&cycle_id)
        .await?
        .expect("accepted cycle exists");

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "goal_id": goal.id,
            "cycle_id": cycle_id,
            "team_id": team_id,
            "statuses": {
                "plan_request_kind": plan_mailbox.first().map(|message| message.kind.as_str()),
                "after_plan_verdict_team": plan_team.lifecycle.as_str(),
                "review_request_kind": review_mailbox.first().map(|message| message.kind.as_str()),
                "before_review_verdict_team": review_team.lifecycle.as_str(),
                "after_review_verdict_team": accepted_team.lifecycle.as_str(),
                "goal": accepted_goal.status,
                "cycle": accepted_cycle.status,
            }
        }))?
    );

    Ok(())
}
