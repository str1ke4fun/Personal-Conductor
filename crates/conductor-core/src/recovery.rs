use crate::db;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::info;

// ── Recovery Summary ────────────────────────────────────────────────────────

/// Aggregate result of a `recover_on_startup()` call.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RecoverySummary {
    /// GoalRuns that were transitioned to "degraded".
    pub goals_degraded: Vec<String>,
    /// AgentTasks that were transitioned from "claimed" to "blocked".
    pub tasks_blocked: Vec<String>,
    /// WorkLeases that were marked "expired".
    pub leases_expired: Vec<String>,
    /// AgentHeartbeats that were marked "stale".
    pub heartbeats_stale: Vec<String>,
}

// ── recover_on_startup ──────────────────────────────────────────────────────

/// Scan all in-flight runtime state and reconcile stale/orphaned records after
/// an unclean shutdown.
///
/// 1. **goal_runs** with non-terminal status -> "degraded" (Orchestrator must re-Orient)
/// 2. **agent_tasks** with status "claimed" -> "blocked" (holder process likely exited)
/// 3. **work_leases** with status "active" and `expires_at < now` -> "expired"
/// 4. **agent_heartbeats** with non-idle status and `expires_at < now` -> "stale"
///
/// A `recovery.started` event is emitted at the beginning and a
/// `recovery.completed` event at the end.
pub async fn recover_on_startup() -> anyhow::Result<RecoverySummary> {
    let pool = db::pool().await?;
    let now = Utc::now();
    let now_str = now.to_rfc3339();
    let mut summary = RecoverySummary::default();

    // Emit recovery.started
    let _ = crate::events::append(
        "recovery",
        "recovery.started",
        &serde_json::json!({ "timestamp": &now_str }),
    )
    .await;

    info!("recovery: scanning for stale runtime state...");

    // ── 1. Active goal_runs -> degraded ──────────────────────────────────
    //
    // Terminal statuses that should NOT be touched:
    //   accepted, archived, failed, cancelled, degraded
    let goal_rows = sqlx::query(
        r#"SELECT id, workspace_id, title, status
           FROM goal_runs
           WHERE status NOT IN ('accepted', 'archived', 'failed', 'cancelled', 'degraded')"#,
    )
    .fetch_all(&pool)
    .await?;

    for row in &goal_rows {
        let id: String = row.try_get("id")?;
        let old_status: String = row.try_get("status")?;
        let ws_id: String = row.try_get("workspace_id")?;

        sqlx::query("UPDATE goal_runs SET status = 'degraded', updated_at = ?1 WHERE id = ?2")
            .bind(&now_str)
            .bind(&id)
            .execute(&pool)
            .await?;

        let _ = crate::events::append(
            "recovery",
            "recovery.goal_degraded",
            &serde_json::json!({
                "goal_id": id,
                "workspace_id": ws_id,
                "previous_status": old_status,
            }),
        )
        .await;

        summary.goals_degraded.push(id);
    }

    info!(
        "recovery: degraded {} goal_run(s)",
        summary.goals_degraded.len()
    );

    // ── 2. Claimed agent_tasks -> blocked ────────────────────────────────
    let task_rows = sqlx::query(
        r#"SELECT id, workspace_id, title, claimed_by
           FROM agent_tasks
           WHERE status = 'claimed'"#,
    )
    .fetch_all(&pool)
    .await?;

    for row in &task_rows {
        let id: String = row.try_get("id")?;
        let ws_id: String = row.try_get("workspace_id")?;
        let claimed_by: Option<String> = row.try_get("claimed_by")?;

        sqlx::query(
            r#"UPDATE agent_tasks
               SET status = 'blocked',
                   error = 'recovered: holder process exited during restart',
                   updated_at = ?1
               WHERE id = ?2"#,
        )
        .bind(&now_str)
        .bind(&id)
        .execute(&pool)
        .await?;

        let _ = crate::events::append(
            "recovery",
            "recovery.task_blocked",
            &serde_json::json!({
                "task_id": id,
                "workspace_id": ws_id,
                "claimed_by": claimed_by,
            }),
        )
        .await;

        summary.tasks_blocked.push(id);
    }

    info!(
        "recovery: blocked {} claimed task(s)",
        summary.tasks_blocked.len()
    );

    // ── 3. Active work_leases past expiry -> expired ─────────────────────
    let lease_rows = sqlx::query(
        r#"SELECT id, workspace_id, holder_id
           FROM work_leases
           WHERE status = 'active' AND expires_at < ?1"#,
    )
    .bind(&now_str)
    .fetch_all(&pool)
    .await?;

    for row in &lease_rows {
        let id: String = row.try_get("id")?;
        let ws_id: String = row.try_get("workspace_id")?;
        let holder_id: String = row.try_get("holder_id")?;

        sqlx::query("UPDATE work_leases SET status = 'expired', released_at = ?1 WHERE id = ?2")
            .bind(&now_str)
            .bind(&id)
            .execute(&pool)
            .await?;

        let _ = crate::events::append(
            "recovery",
            "recovery.lease_expired",
            &serde_json::json!({
                "lease_id": id,
                "workspace_id": ws_id,
                "holder_id": holder_id,
            }),
        )
        .await;

        summary.leases_expired.push(id);
    }

    info!(
        "recovery: expired {} work_lease(s)",
        summary.leases_expired.len()
    );

    // ── 4. Expired non-idle heartbeats -> stale ──────────────────────────
    let hb_rows = sqlx::query(
        r#"SELECT id, workspace_id, agent_id, status
           FROM agent_heartbeats
           WHERE status != 'idle' AND status != 'stale' AND expires_at < ?1"#,
    )
    .bind(&now_str)
    .fetch_all(&pool)
    .await?;

    for row in &hb_rows {
        let id: String = row.try_get("id")?;
        let ws_id: String = row.try_get("workspace_id")?;
        let agent_id: String = row.try_get("agent_id")?;
        let old_status: String = row.try_get("status")?;

        sqlx::query("UPDATE agent_heartbeats SET status = 'stale' WHERE id = ?1")
            .bind(&id)
            .execute(&pool)
            .await?;

        let _ = crate::events::append(
            "recovery",
            "recovery.heartbeat_stale",
            &serde_json::json!({
                "heartbeat_id": id,
                "workspace_id": ws_id,
                "agent_id": agent_id,
                "previous_status": old_status,
            }),
        )
        .await;

        summary.heartbeats_stale.push(id);
    }

    info!(
        "recovery: marked {} heartbeat(s) as stale",
        summary.heartbeats_stale.len()
    );

    // Emit recovery.completed
    let _ = crate::events::append(
        "recovery",
        "recovery.completed",
        &serde_json::json!({
            "timestamp": &now_str,
            "goals_degraded": summary.goals_degraded.len(),
            "tasks_blocked": summary.tasks_blocked.len(),
            "leases_expired": summary.leases_expired.len(),
            "heartbeats_stale": summary.heartbeats_stale.len(),
        }),
    )
    .await;

    info!("recovery: complete");

    Ok(summary)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    /// Happy path: create active goals, tasks, leases, and heartbeats, then
    /// run recovery and verify all are properly transitioned.
    #[tokio::test]
    async fn recovery_normal_full() {
        let _root = TestRoot::new();
        let pool = db::pool().await.expect("pool");
        let now = Utc::now();
        let past = (now - chrono::Duration::hours(2)).to_rfc3339();
        let future = (now + chrono::Duration::hours(2)).to_rfc3339();
        let now_str = now.to_rfc3339();

        // Insert a running goal
        sqlx::query(
            r#"INSERT INTO goal_runs
              (id, workspace_id, title, objective, status, priority, owner, created_at, updated_at)
              VALUES ('goal-r1', 'ws-1', 'Recover me', 'test', 'running', 'p1', 'user', ?1, ?1)"#,
        )
        .bind(&now_str)
        .execute(&pool)
        .await
        .unwrap();

        // Insert a planning goal
        sqlx::query(
            r#"INSERT INTO goal_runs
              (id, workspace_id, title, objective, status, priority, owner, created_at, updated_at)
              VALUES ('goal-r2', 'ws-1', 'Also recover', 'test', 'planning', 'p2', 'user', ?1, ?1)"#,
        )
        .bind(&now_str)
        .execute(&pool)
        .await
        .unwrap();

        // Insert a claimed agent task
        sqlx::query(
            r#"INSERT INTO agent_tasks
              (id, workspace_id, title, instruction, status, agent_kind, claimed_by,
               write_scope_json, read_scope_json, allowed_tools_json, dependencies_json, acceptance_json,
               created_at, updated_at, claimed_at)
              VALUES ('task-r1', 'ws-1', 'Orphaned task', 'do stuff', 'claimed', 'claude_p', 'agent-ghost',
               '[]', '[]', '[]', '[]', '[]', ?1, ?1, ?1)"#,
        )
        .bind(&now_str)
        .execute(&pool)
        .await
        .unwrap();

        // Insert an expired active lease
        sqlx::query(
            r#"INSERT INTO work_leases
              (id, workspace_id, holder_id, task_id, lease_type, scope_json, status, ttl_seconds,
               acquired_at, renewed_at, expires_at)
              VALUES ('lease-r1', 'ws-1', 'agent-ghost', 'task-r1', 'task_claim', '[]', 'active', 3600,
               ?1, ?1, ?2)"#,
        )
        .bind(&now_str)
        .bind(&past)
        .execute(&pool)
        .await
        .unwrap();

        // Insert an expired heartbeat with non-idle status
        sqlx::query(
            r#"INSERT INTO agent_heartbeats
              (id, workspace_id, agent_id, status, active_tool_count, created_at, expires_at)
              VALUES ('hb-r1', 'ws-1', 'agent-ghost', 'working', 2, ?1, ?2)"#,
        )
        .bind(&now_str)
        .bind(&past)
        .execute(&pool)
        .await
        .unwrap();

        // Also insert a future lease (should NOT be expired)
        sqlx::query(
            r#"INSERT INTO work_leases
              (id, workspace_id, holder_id, lease_type, scope_json, status, ttl_seconds,
               acquired_at, renewed_at, expires_at)
              VALUES ('lease-future', 'ws-1', 'agent-alive', 'command', '[]', 'active', 3600,
               ?1, ?1, ?2)"#,
        )
        .bind(&now_str)
        .bind(&future)
        .execute(&pool)
        .await
        .unwrap();

        // Run recovery
        let summary = recover_on_startup().await.expect("recovery");

        // Verify goals degraded
        assert_eq!(summary.goals_degraded.len(), 2);
        assert!(summary.goals_degraded.contains(&"goal-r1".to_string()));
        assert!(summary.goals_degraded.contains(&"goal-r2".to_string()));

        // Verify goal statuses in DB
        let g1_status: String =
            sqlx::query_scalar("SELECT status FROM goal_runs WHERE id = 'goal-r1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(g1_status, "degraded");

        // Verify tasks blocked
        assert_eq!(summary.tasks_blocked.len(), 1);
        assert_eq!(summary.tasks_blocked[0], "task-r1");
        let t1_status: String =
            sqlx::query_scalar("SELECT status FROM agent_tasks WHERE id = 'task-r1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(t1_status, "blocked");

        // Verify leases expired
        assert_eq!(summary.leases_expired.len(), 1);
        assert_eq!(summary.leases_expired[0], "lease-r1");
        let l1_status: String =
            sqlx::query_scalar("SELECT status FROM work_leases WHERE id = 'lease-r1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(l1_status, "expired");

        // Future lease should still be active
        let l_future: String =
            sqlx::query_scalar("SELECT status FROM work_leases WHERE id = 'lease-future'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(l_future, "active");

        // Verify heartbeats stale
        assert_eq!(summary.heartbeats_stale.len(), 1);
        assert_eq!(summary.heartbeats_stale[0], "hb-r1");
        let hb1_status: String =
            sqlx::query_scalar("SELECT status FROM agent_heartbeats WHERE id = 'hb-r1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(hb1_status, "stale");
    }

    /// Partial corruption: some tables have data, others are empty. Recovery
    /// should succeed without errors and only report what it found.
    #[tokio::test]
    async fn recovery_partial_corruption() {
        let _root = TestRoot::new();
        let pool = db::pool().await.expect("pool");
        let now = Utc::now();
        let past = (now - chrono::Duration::hours(1)).to_rfc3339();
        let now_str = now.to_rfc3339();

        // Only insert goals -- no tasks, leases, or heartbeats
        sqlx::query(
            r#"INSERT INTO goal_runs
              (id, workspace_id, title, objective, status, priority, owner, created_at, updated_at)
              VALUES ('goal-partial', 'ws-p', 'Partial', 'test', 'awaiting_review', 'p1', 'user', ?1, ?1)"#,
        )
        .bind(&now_str)
        .execute(&pool)
        .await
        .unwrap();

        // Also insert an already-terminal goal (should be skipped)
        sqlx::query(
            r#"INSERT INTO goal_runs
              (id, workspace_id, title, objective, status, priority, owner, created_at, updated_at)
              VALUES ('goal-done', 'ws-p', 'Done', 'test', 'accepted', 'p1', 'user', ?1, ?1)"#,
        )
        .bind(&now_str)
        .execute(&pool)
        .await
        .unwrap();

        // Insert an already-idle heartbeat (should be skipped)
        sqlx::query(
            r#"INSERT INTO agent_heartbeats
              (id, workspace_id, agent_id, status, active_tool_count, created_at, expires_at)
              VALUES ('hb-idle', 'ws-p', 'agent-z', 'idle', 0, ?1, ?2)"#,
        )
        .bind(&now_str)
        .bind(&past)
        .execute(&pool)
        .await
        .unwrap();

        let summary = recover_on_startup().await.expect("recovery");

        // Only the non-terminal goal should be degraded
        assert_eq!(summary.goals_degraded.len(), 1);
        assert_eq!(summary.goals_degraded[0], "goal-partial");

        // Nothing else
        assert!(summary.tasks_blocked.is_empty());
        assert!(summary.leases_expired.is_empty());
        assert!(summary.heartbeats_stale.is_empty());

        // Terminal goal untouched
        let done_status: String =
            sqlx::query_scalar("SELECT status FROM goal_runs WHERE id = 'goal-done'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(done_status, "accepted");

        // Idle heartbeat untouched
        let idle_status: String =
            sqlx::query_scalar("SELECT status FROM agent_heartbeats WHERE id = 'hb-idle'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(idle_status, "idle");
    }

    /// Empty database: recovery should succeed with zero counts and no errors.
    #[tokio::test]
    async fn recovery_empty_database() {
        let _root = TestRoot::new();

        let summary = recover_on_startup().await.expect("recovery");

        assert!(summary.goals_degraded.is_empty());
        assert!(summary.tasks_blocked.is_empty());
        assert!(summary.leases_expired.is_empty());
        assert!(summary.heartbeats_stale.is_empty());
    }
}
