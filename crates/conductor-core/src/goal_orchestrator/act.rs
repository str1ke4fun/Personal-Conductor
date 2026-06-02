// Act phase: create AgentTasks from the DispatchPlan and dispatch to adapters
//
// IMPORTANT: Act does NOT execute tools directly. It only creates tasks,
// leases, messages, and events.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::adapters::agent_team_adapter::AgentTeamAdapter;
use crate::agent_teams::{self, AgentTeamLifecycle, CreateAgentTeamInput};
use crate::goal_tasks::AgentTask;

use super::decide::DispatchPlan;

/// Result of executing the Act phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActResult {
    pub tasks_created: Vec<AgentTask>,
    pub tasks_dispatched: usize,
}

/// Execute the Act phase: create AgentTasks from the plan and mark them for dispatch.
///
/// Guard: This function only creates Task/Lease/Message/Event entities.
/// It never directly executes tools or writes files.
pub async fn act(
    goal_id: &str,
    cycle_id: &str,
    workspace_id: &str,
    plan: &DispatchPlan,
) -> Result<ActResult> {
    let mut tasks_created = Vec::new();

    for planned in &plan.tasks {
        let task = crate::goal_tasks::create_task(
            workspace_id,
            Some(goal_id),
            Some(cycle_id),
            &planned.title,
            &planned.instruction,
            &planned.agent_kind,
            planned.write_scope.clone(),
            planned.read_scope.clone(),
            planned.allowed_tools.clone(),
            planned.dependencies.clone(),
            planned.acceptance.clone(),
        )
        .await?;

        // Transition from proposed -> queued so tasks are ready for dispatch
        let task = crate::goal_tasks::rework_task(&task.id)
            .await
            .unwrap_or(task);

        tasks_created.push(task);
    }

    let dispatched = tasks_created.len();

    if !tasks_created.is_empty() {
        let team_id = format!("team-{cycle_id}");
        let existing_team = agent_teams::get_team(&team_id).await.ok();
        let (team, created) = match existing_team {
            Some(team) => (team, false),
            None => (
                agent_teams::create_team(CreateAgentTeamInput {
                    id: Some(team_id.clone()),
                    name: format!("Goal {goal_id} / Cycle {}", cycle_id),
                    workspace_id: Some(workspace_id.to_string()),
                    write_scope: plan.write_scope.clone(),
                    metadata: Some(serde_json::json!({
                        "goal_id": goal_id,
                        "cycle_id": cycle_id,
                    })),
                })
                .await?,
                true,
            ),
        };

        if created {
            AgentTeamAdapter::bind_to_goal(&team.id, goal_id, cycle_id).await?;
        }

        for task in &tasks_created {
            let agent_id = format!("{}:{}", task.agent_kind, task.id);
            agent_teams::add_member(agent_teams::AddAgentTeamMemberInput {
                team_id: team.id.clone(),
                agent_id,
                role: "executor".to_string(),
                run_id: None,
                cwd: None,
                subscriptions: vec![],
                metadata: Some(serde_json::json!({
                    "task_id": task.id,
                    "goal_id": goal_id,
                    "cycle_id": cycle_id,
                })),
            })
            .await?;
        }

        let mut previous = agent_teams::get_team(&team.id).await?.lifecycle;
        let transitions: Vec<AgentTeamLifecycle> = match previous {
            AgentTeamLifecycle::Draft => {
                vec![AgentTeamLifecycle::Planning, AgentTeamLifecycle::Executing]
            }
            AgentTeamLifecycle::Planning => vec![AgentTeamLifecycle::Executing],
            AgentTeamLifecycle::AwaitingPlanApproval => vec![AgentTeamLifecycle::Executing],
            AgentTeamLifecycle::Executing => vec![],
            _ => vec![],
        };

        for next in transitions {
            let transitioned =
                agent_teams::transition_team_lifecycle(&team.id, next.clone()).await?;
            AgentTeamAdapter::on_lifecycle_change(&team.id, &previous, &next, goal_id, cycle_id)
                .await?;
            previous = transitioned.lifecycle;
        }
    }

    // Emit audit event
    let _ = crate::events::append_event(&crate::events::AuditEvent {
        timestamp: chrono::Utc::now(),
        source: "goal_orchestrator".to_string(),
        event_type: "act.tasks_created".to_string(),
        actor: "orchestrator".to_string(),
        target: goal_id.to_string(),
        detail: serde_json::json!({"tasks_created": dispatched}),
        session_id: Some(cycle_id.to_string()),
    });

    Ok(ActResult {
        tasks_created,
        tasks_dispatched: dispatched,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn act_creates_tasks_from_plan() {
        let _root = TestRoot::new();

        // Create a goal first
        let goal = crate::goals::create_goal(
            "ws-act",
            "Test Act",
            "test objective",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");

        let cycle = crate::goals::create_cycle(&goal.id, 1)
            .await
            .expect("create cycle");

        let plan = DispatchPlan {
            tasks: vec![
                super::super::decide::PlannedTask {
                    title: "task-1".to_string(),
                    instruction: "do thing 1".to_string(),
                    agent_kind: "backend-agent".to_string(),
                    write_scope: vec![],
                    read_scope: vec![],
                    allowed_tools: vec![],
                    dependencies: vec![],
                    acceptance: vec!["done".to_string()],
                },
                super::super::decide::PlannedTask {
                    title: "task-2".to_string(),
                    instruction: "do thing 2".to_string(),
                    agent_kind: "test-agent".to_string(),
                    write_scope: vec![],
                    read_scope: vec![],
                    allowed_tools: vec![],
                    dependencies: vec![],
                    acceptance: vec!["done".to_string()],
                },
            ],
            write_scope: vec![],
            acceptance: vec![],
            budget_remaining: super::super::decide::Budget::default(),
            approved: true,
        };

        let result = act(&goal.id, &cycle.id, "ws-act", &plan)
            .await
            .expect("act");

        assert_eq!(result.tasks_created.len(), 2);
        assert_eq!(result.tasks_dispatched, 2);

        let team = crate::agent_teams::get_team(&format!("team-{}", cycle.id))
            .await
            .expect("team created");
        assert_eq!(team.lifecycle.as_str(), "executing");

        let members = crate::agent_teams::list_members(&team.id)
            .await
            .expect("members");
        assert_eq!(members.len(), 2);
        assert!(members.iter().all(|member| {
            member
                .metadata_json
                .as_ref()
                .and_then(|value| value.get("task_id"))
                .is_some()
        }));
    }

    #[tokio::test]
    async fn act_empty_plan_creates_nothing() {
        let _root = TestRoot::new();

        let goal = crate::goals::create_goal(
            "ws-act2",
            "Test Act Empty",
            "test",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");

        let cycle = crate::goals::create_cycle(&goal.id, 1)
            .await
            .expect("create cycle");

        let plan = DispatchPlan {
            tasks: vec![],
            write_scope: vec![],
            acceptance: vec![],
            budget_remaining: super::super::decide::Budget::default(),
            approved: false,
        };

        let result = act(&goal.id, &cycle.id, "ws-act2", &plan)
            .await
            .expect("act");

        assert_eq!(result.tasks_created.len(), 0);
    }
}
