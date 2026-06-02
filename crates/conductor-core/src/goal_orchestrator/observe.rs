// Observe phase: read goal state from DB

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::agent_messages::AgentMessage;
use crate::events::AuditEvent;
use crate::goal_tasks::AgentTask;
use crate::goals::{GoalCycle, GoalRun};
use crate::heartbeat::AgentHeartbeat;
use crate::leases::WorkLease;

/// Snapshot of all relevant state for a single goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserveReport {
    pub goal: GoalRun,
    pub current_cycle: Option<GoalCycle>,
    pub active_tasks: Vec<AgentTask>,
    pub heartbeats: Vec<AgentHeartbeat>,
    pub active_leases: Vec<WorkLease>,
    pub recent_events: Vec<AuditEvent>,
    pub unread_messages: Vec<AgentMessage>,
}

/// Collect all state relevant to `goal_id` from the database.
pub async fn observe(goal_id: &str) -> Result<ObserveReport> {
    let goal = crate::goals::get_goal(goal_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;

    let cycles = crate::goals::list_cycles_by_goal(goal_id).await?;
    let current_cycle = cycles.into_iter().find(|c| {
        !matches!(
            c.status.as_str(),
            "completed" | "failed" | "blocked" | "cancelled"
        )
    });

    let active_tasks = crate::goal_tasks::list_tasks_by_goal(goal_id)
        .await?
        .into_iter()
        .filter(|t| {
            !matches!(
                t.status.as_str(),
                "completed" | "failed" | "cancelled" | "archived"
            )
        })
        .collect();

    let heartbeats = crate::heartbeat::get_active_heartbeats(&goal.workspace_id).await?;

    let active_leases = crate::leases::list_active_leases(&goal.workspace_id).await?;

    // Use workspace_id from goal for event query
    let recent_events = crate::events::query_events_db(&goal.workspace_id, None, Some(50))
        .await
        .unwrap_or_default();

    let unread_messages =
        crate::agent_messages::get_messages(&goal.workspace_id, None, None, Some(50))
            .await
            .unwrap_or_default();

    Ok(ObserveReport {
        goal,
        current_cycle,
        active_tasks,
        heartbeats,
        active_leases,
        recent_events,
        unread_messages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn observe_nonexistent_goal_errors() {
        let _root = TestRoot::new();
        let result = observe("nonexistent-goal").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn observe_existing_goal_returns_report() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-obs",
            "Test observation",
            "observe objective",
            "normal",
            "test-agent",
            None,
            None,
        )
        .await
        .expect("create goal");

        let report = observe(&goal.id).await.expect("observe");
        assert_eq!(report.goal.id, goal.id);
        assert!(report.current_cycle.is_none());
        assert!(report.active_tasks.is_empty());
    }
}
