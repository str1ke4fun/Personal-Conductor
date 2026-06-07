// Observe phase: read goal state from DB

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::agent_messages::AgentMessage;
use crate::events::AuditEvent;
use crate::goal_hints::GoalHint;
use crate::goal_tasks::AgentTask;
use crate::goals::{GoalCycle, GoalRun};
use crate::heartbeat::AgentHeartbeat;
use crate::leases::WorkLease;

/// Goal-scoped fact surfaced from memory_entries for Cairn-style graph reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedFact {
    pub id: String,
    pub key: String,
    pub value: String,
    pub category: String,
    pub updated_at: String,
}

/// Snapshot of all relevant state for a single goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserveReport {
    pub goal: GoalRun,
    pub current_cycle: Option<GoalCycle>,
    pub facts: Vec<ObservedFact>,
    pub active_tasks: Vec<AgentTask>,
    pub heartbeats: Vec<AgentHeartbeat>,
    pub active_leases: Vec<WorkLease>,
    pub recent_events: Vec<AuditEvent>,
    pub unread_messages: Vec<AgentMessage>,
    /// Active hints for this goal, most-recent first (up to 10).
    pub recent_hints: Vec<GoalHint>,
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

    let facts = {
        let pool = crate::db::pool().await?;
        sqlx::query(
            r#"
            SELECT id, key, value, category, updated_at
            FROM memory_entries
            WHERE goal_id = ?
            ORDER BY updated_at DESC
            LIMIT 100
            "#,
        )
        .bind(goal_id)
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| {
            use sqlx::Row;
            ObservedFact {
                id: row.try_get("id").unwrap_or_default(),
                key: row.try_get("key").unwrap_or_default(),
                value: row.try_get("value").unwrap_or_default(),
                category: row.try_get("category").unwrap_or_default(),
                updated_at: row.try_get("updated_at").unwrap_or_default(),
            }
        })
        .collect()
    };

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

    let recent_hints = crate::goal_hints::list_active_hints(goal_id, Some(10))
        .await
        .unwrap_or_default();

    Ok(ObserveReport {
        goal,
        current_cycle,
        facts,
        active_tasks,
        heartbeats,
        active_leases,
        recent_events,
        unread_messages,
        recent_hints,
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
        assert!(report.recent_hints.is_empty());
    }

    #[tokio::test]
    async fn observe_surfaces_active_hints() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-obs-hints",
            "Hint Observation",
            "test hints in observe",
            "normal",
            "test-agent",
            None,
            None,
        )
        .await
        .expect("create goal");

        crate::goal_hints::create_hint(&goal.id, None, "user", "focus on auth", None)
            .await
            .expect("create hint");

        let report = observe(&goal.id).await.expect("observe");
        assert_eq!(report.recent_hints.len(), 1);
        assert_eq!(report.recent_hints[0].content, "focus on auth");
    }
}
