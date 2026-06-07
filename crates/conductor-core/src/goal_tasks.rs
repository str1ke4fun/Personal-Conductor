use crate::db;
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// AgentTask struct
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentTask {
    pub id: String,
    pub workspace_id: String,
    pub goal_id: Option<String>,
    pub cycle_id: Option<String>,
    pub parent_task_id: Option<String>,
    pub title: String,
    pub instruction: String,
    pub status: String,
    pub agent_kind: String,
    pub assigned_agent_id: Option<String>,
    pub claimed_by: Option<String>,
    pub write_scope_json: Vec<String>,
    pub read_scope_json: Vec<String>,
    pub allowed_tools_json: Vec<String>,
    pub dependencies_json: Vec<String>,
    pub acceptance_json: Vec<String>,
    pub result_ref: Option<String>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

/// Validate a task status transition. Returns Ok(()) if legal, Err otherwise.
pub fn validate_task_transition(from: &str, to: &str) -> anyhow::Result<()> {
    let legal: &[(&str, &[&str])] = &[
        ("proposed", &["queued", "claimed", "cancelled"]),
        ("queued", &["claimed", "cancelled"]),
        ("claimed", &["running", "cancelled"]),
        (
            "running",
            &[
                "awaiting_permission",
                "awaiting_input",
                "review_ready",
                "accepted",
                "blocked",
                "failed",
                "cancelled",
            ],
        ),
        ("awaiting_permission", &["running", "cancelled"]),
        ("awaiting_input", &["running", "cancelled"]),
        ("review_ready", &["accepted", "rework_required", "failed"]),
        ("accepted", &[]),
        ("rework_required", &["queued"]),
        ("blocked", &["queued", "running", "failed", "cancelled"]),
        ("failed", &[]),
        ("cancelled", &[]),
    ];

    for (src, targets) in legal {
        if *src == from {
            if targets.contains(&to) {
                return Ok(());
            }
            bail!("invalid task transition: {from} -> {to} (allowed: {targets:?})");
        }
    }
    bail!("unknown task status: {from}");
}

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

/// Create a new agent task. Status starts as "proposed".
#[allow(clippy::too_many_arguments)]
pub async fn create_task(
    workspace_id: &str,
    goal_id: Option<&str>,
    cycle_id: Option<&str>,
    title: &str,
    instruction: &str,
    agent_kind: &str,
    write_scope_json: Vec<String>,
    read_scope_json: Vec<String>,
    allowed_tools_json: Vec<String>,
    dependencies_json: Vec<String>,
    acceptance_json: Vec<String>,
) -> anyhow::Result<AgentTask> {
    let pool = db::pool().await?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    sqlx::query(
        r#"
        INSERT INTO agent_tasks (
            id, workspace_id, goal_id, cycle_id, parent_task_id,
            title, instruction, status, agent_kind,
            assigned_agent_id, claimed_by,
            write_scope_json, read_scope_json, allowed_tools_json,
            dependencies_json, acceptance_json,
            result_ref, error,
            created_at, updated_at, claimed_at, finished_at
        )
        VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, 'proposed', ?7,
                NULL, NULL,
                ?8, ?9, ?10,
                ?11, ?12,
                NULL, NULL,
                ?13, ?13, NULL, NULL)
        "#,
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(goal_id)
    .bind(cycle_id)
    .bind(title)
    .bind(instruction)
    .bind(agent_kind)
    .bind(serde_json::to_string(&write_scope_json)?)
    .bind(serde_json::to_string(&read_scope_json)?)
    .bind(serde_json::to_string(&allowed_tools_json)?)
    .bind(serde_json::to_string(&dependencies_json)?)
    .bind(serde_json::to_string(&acceptance_json)?)
    .bind(now.to_rfc3339())
    .execute(&pool)
    .await?;

    // Emit event
    let _ = crate::events::append(
        "task",
        "task.created",
        &serde_json::json!({
            "task_id": id,
            "workspace_id": workspace_id,
            "goal_id": goal_id,
            "title": title,
            "agent_kind": agent_kind,
        }),
    )
    .await;

    get_task(&id)
        .await?
        .context("task just created but not found")
}

/// Get a single task by ID.
pub async fn get_task(task_id: &str) -> anyhow::Result<Option<AgentTask>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, workspace_id, goal_id, cycle_id, parent_task_id,
               title, instruction, status, agent_kind,
               assigned_agent_id, claimed_by,
               write_scope_json, read_scope_json, allowed_tools_json,
               dependencies_json, acceptance_json,
               result_ref, error,
               created_at, updated_at, claimed_at, finished_at
        FROM agent_tasks
        WHERE id = ?1
        "#,
    )
    .bind(task_id)
    .fetch_optional(&pool)
    .await?;

    row.map(row_to_task).transpose()
}

/// List all tasks belonging to a goal.
pub async fn list_tasks_by_goal(goal_id: &str) -> anyhow::Result<Vec<AgentTask>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, workspace_id, goal_id, cycle_id, parent_task_id,
               title, instruction, status, agent_kind,
               assigned_agent_id, claimed_by,
               write_scope_json, read_scope_json, allowed_tools_json,
               dependencies_json, acceptance_json,
               result_ref, error,
               created_at, updated_at, claimed_at, finished_at
        FROM agent_tasks
        WHERE goal_id = ?1
        ORDER BY created_at ASC
        "#,
    )
    .bind(goal_id)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(row_to_task).collect()
}

/// List all tasks belonging to a cycle.
pub async fn list_tasks_by_cycle(cycle_id: &str) -> anyhow::Result<Vec<AgentTask>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, workspace_id, goal_id, cycle_id, parent_task_id,
               title, instruction, status, agent_kind,
               assigned_agent_id, claimed_by,
               write_scope_json, read_scope_json, allowed_tools_json,
               dependencies_json, acceptance_json,
               result_ref, error,
               created_at, updated_at, claimed_at, finished_at
        FROM agent_tasks
        WHERE cycle_id = ?1
        ORDER BY created_at ASC
        "#,
    )
    .bind(cycle_id)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(row_to_task).collect()
}

/// List tasks by workspace and status.
pub async fn list_tasks_by_status(
    workspace_id: &str,
    status: &str,
) -> anyhow::Result<Vec<AgentTask>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, workspace_id, goal_id, cycle_id, parent_task_id,
               title, instruction, status, agent_kind,
               assigned_agent_id, claimed_by,
               write_scope_json, read_scope_json, allowed_tools_json,
               dependencies_json, acceptance_json,
               result_ref, error,
               created_at, updated_at, claimed_at, finished_at
        FROM agent_tasks
        WHERE workspace_id = ?1 AND status = ?2
        ORDER BY created_at ASC
        "#,
    )
    .bind(workspace_id)
    .bind(status)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(row_to_task).collect()
}

/// List all tasks for a workspace, ordered by updated_at DESC.
pub async fn list_tasks(workspace_id: &str) -> anyhow::Result<Vec<AgentTask>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, workspace_id, goal_id, cycle_id, parent_task_id,
               title, instruction, status, agent_kind,
               assigned_agent_id, claimed_by,
               write_scope_json, read_scope_json, allowed_tools_json,
               dependencies_json, acceptance_json,
               result_ref, error,
               created_at, updated_at, claimed_at, finished_at
        FROM agent_tasks
        WHERE workspace_id = ?1
        ORDER BY updated_at DESC
        "#,
    )
    .bind(workspace_id)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(row_to_task).collect()
}

/// Delete a task by ID.
pub async fn delete_task(task_id: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let result = sqlx::query("DELETE FROM agent_tasks WHERE id = ?1")
        .bind(task_id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        bail!("task not found: {task_id}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Lifecycle transitions
// ---------------------------------------------------------------------------

/// Claim a task: proposed/queued -> claimed. Acquires a WorkLease.
pub async fn claim_task(
    task_id: &str,
    agent_id: &str,
    lease_ttl_seconds: i64,
) -> anyhow::Result<AgentTask> {
    let task = get_task(task_id)
        .await?
        .with_context(|| format!("task not found: {task_id}"))?;

    validate_task_transition(&task.status, "claimed")?;

    // Acquire lease (conflict check happens inside leases::acquire)
    let lease = crate::leases::acquire(
        &task.workspace_id,
        agent_id,
        Some(task_id),
        "task_claim",
        vec![],
        lease_ttl_seconds,
    )
    .await
    .map_err(|e| anyhow::anyhow!("lease conflict: {e}"))?;

    let now = Utc::now();
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE agent_tasks
        SET status = 'claimed',
            claimed_by = ?1,
            claimed_at = ?2,
            updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(agent_id)
    .bind(now.to_rfc3339())
    .bind(task_id)
    .execute(&pool)
    .await?;

    let _ = crate::events::append(
        "task",
        "task.claimed",
        &serde_json::json!({
            "task_id": task_id,
            "agent_id": agent_id,
            "lease_id": lease.id,
        }),
    )
    .await;

    get_task(task_id)
        .await?
        .context("task just claimed but not found")
}

/// Claim the oldest queued task, optionally filtering by workspace.
pub async fn claim_next_queued_task(
    workspace_id: Option<&str>,
    agent_id: &str,
    lease_ttl_seconds: i64,
) -> anyhow::Result<Option<AgentTask>> {
    let pool = db::pool().await?;
    let row = match workspace_id {
        Some(workspace_id) => {
            sqlx::query(
                r#"
                SELECT id
                FROM agent_tasks
                WHERE workspace_id = ?1 AND status = 'queued'
                ORDER BY created_at ASC
                LIMIT 1
                "#,
            )
            .bind(workspace_id)
            .fetch_optional(&pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"
                SELECT id
                FROM agent_tasks
                WHERE status = 'queued'
                ORDER BY created_at ASC
                LIMIT 1
                "#,
            )
            .fetch_optional(&pool)
            .await?
        }
    };

    let Some(row) = row else {
        return Ok(None);
    };
    let task_id: String = row.try_get("id")?;
    claim_task(&task_id, agent_id, lease_ttl_seconds)
        .await
        .map(Some)
}

/// Start a task: claimed -> running.
pub async fn start_task(task_id: &str) -> anyhow::Result<AgentTask> {
    let task = get_task(task_id)
        .await?
        .with_context(|| format!("task not found: {task_id}"))?;

    validate_task_transition(&task.status, "running")?;

    let now = Utc::now();
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE agent_tasks
        SET status = 'running',
            updated_at = ?1
        WHERE id = ?2
        "#,
    )
    .bind(now.to_rfc3339())
    .bind(task_id)
    .execute(&pool)
    .await?;

    get_task(task_id)
        .await?
        .context("task just started but not found")
}

/// Complete a task: running -> accepted. Releases the WorkLease.
pub async fn complete_task(task_id: &str, result_ref: &str) -> anyhow::Result<AgentTask> {
    let task = get_task(task_id)
        .await?
        .with_context(|| format!("task not found: {task_id}"))?;

    validate_task_transition(&task.status, "accepted")?;

    // Release lease if one exists
    release_task_lease(task_id).await?;

    let now = Utc::now();
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE agent_tasks
        SET status = 'accepted',
            result_ref = ?1,
            finished_at = ?2,
            updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(result_ref)
    .bind(now.to_rfc3339())
    .bind(task_id)
    .execute(&pool)
    .await?;

    let _ = crate::events::append(
        "task",
        "task.completed",
        &serde_json::json!({
            "task_id": task_id,
            "result_ref": result_ref,
        }),
    )
    .await;

    get_task(task_id)
        .await?
        .context("task just completed but not found")
}

/// Fail a task: running -> failed. Releases the WorkLease.
pub async fn fail_task(task_id: &str, error: &str) -> anyhow::Result<AgentTask> {
    let task = get_task(task_id)
        .await?
        .with_context(|| format!("task not found: {task_id}"))?;

    validate_task_transition(&task.status, "failed")?;

    release_task_lease(task_id).await?;

    let now = Utc::now();
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE agent_tasks
        SET status = 'failed',
            error = ?1,
            finished_at = ?2,
            updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(error)
    .bind(now.to_rfc3339())
    .bind(task_id)
    .execute(&pool)
    .await?;

    let _ = crate::events::append(
        "task",
        "task.failed",
        &serde_json::json!({
            "task_id": task_id,
            "error": error,
        }),
    )
    .await;

    get_task(task_id)
        .await?
        .context("task just failed but not found")
}

/// Block a task: running -> blocked.
pub async fn block_task(task_id: &str, reason: &str) -> anyhow::Result<AgentTask> {
    let task = get_task(task_id)
        .await?
        .with_context(|| format!("task not found: {task_id}"))?;

    validate_task_transition(&task.status, "blocked")?;
    release_task_lease(task_id).await?;

    let now = Utc::now();
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE agent_tasks
        SET status = 'blocked',
            error = ?1,
            updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(reason)
    .bind(now.to_rfc3339())
    .bind(task_id)
    .execute(&pool)
    .await?;

    let _ = crate::events::append(
        "task",
        "task.blocked",
        &serde_json::json!({
            "task_id": task_id,
            "reason": reason,
        }),
    )
    .await;

    get_task(task_id)
        .await?
        .context("task just blocked but not found")
}

/// Set task to review_ready with a result reference (for desktop-executed tasks).
pub async fn set_task_result_ref_review_ready(
    task_id: &str,
    result_ref: &str,
) -> anyhow::Result<()> {
    let pool = crate::db::pool().await?;
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE agent_tasks SET status='review_ready', result_ref=?1, updated_at=?2, finished_at=?2 \
         WHERE id=?3 AND status='running'",
    )
    .bind(result_ref)
    .bind(&now)
    .bind(task_id)
    .execute(&pool)
    .await?;
    anyhow::ensure!(
        result.rows_affected() == 1,
        "task {task_id} was not moved to review_ready; expected running task writeback"
    );
    let _ = crate::events::emit_goal_task_result_projected(task_id, result_ref, None, None).await;
    Ok(())
}

/// Set task to blocked with a result reference while preserving the partial output.
pub async fn set_task_result_ref_blocked(
    task_id: &str,
    result_ref: &str,
    reason: &str,
) -> anyhow::Result<()> {
    release_task_lease(task_id).await?;
    let pool = crate::db::pool().await?;
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE agent_tasks SET status='blocked', result_ref=?1, error=?2, updated_at=?3 \
         WHERE id=?4 AND status='running'",
    )
    .bind(result_ref)
    .bind(reason)
    .bind(&now)
    .bind(task_id)
    .execute(&pool)
    .await?;
    anyhow::ensure!(
        result.rows_affected() == 1,
        "task {task_id} was not moved to blocked; expected running task writeback"
    );
    let _ = crate::events::emit_goal_task_writeback_failed(task_id, reason, None).await;
    Ok(())
}

/// Resume a blocked task by putting it back into the queue.
pub async fn resume_blocked_task(task_id: &str) -> anyhow::Result<AgentTask> {
    let task = get_task(task_id)
        .await?
        .with_context(|| format!("task not found: {task_id}"))?;

    validate_task_transition(&task.status, "queued")?;

    let now = Utc::now();
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE agent_tasks
        SET status = 'queued',
            error = NULL,
            result_ref = NULL,
            updated_at = ?1
        WHERE id = ?2
        "#,
    )
    .bind(now.to_rfc3339())
    .bind(task_id)
    .execute(&pool)
    .await?;

    get_task(task_id)
        .await?
        .context("task just resumed from blocked but not found")
}

/// Rework a task: rework_required -> queued.
pub async fn rework_task(task_id: &str) -> anyhow::Result<AgentTask> {
    let task = get_task(task_id)
        .await?
        .with_context(|| format!("task not found: {task_id}"))?;

    validate_task_transition(&task.status, "queued")?;

    let now = Utc::now();
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE agent_tasks
        SET status = 'queued',
            updated_at = ?1
        WHERE id = ?2
        "#,
    )
    .bind(now.to_rfc3339())
    .bind(task_id)
    .execute(&pool)
    .await?;

    get_task(task_id)
        .await?
        .context("task just reworked but not found")
}

/// Accept a task after review: review_ready -> accepted.
pub async fn accept_review_ready_task(task_id: &str) -> anyhow::Result<AgentTask> {
    let task = get_task(task_id)
        .await?
        .with_context(|| format!("task not found: {task_id}"))?;

    validate_task_transition(&task.status, "accepted")?;
    release_task_lease(task_id).await?;

    let now = Utc::now();
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE agent_tasks
        SET status = 'accepted',
            finished_at = COALESCE(finished_at, ?1),
            updated_at = ?1
        WHERE id = ?2
        "#,
    )
    .bind(now.to_rfc3339())
    .bind(task_id)
    .execute(&pool)
    .await?;

    let _ = crate::events::append(
        "task",
        "task.accepted",
        &serde_json::json!({
            "task_id": task_id,
        }),
    )
    .await;

    get_task(task_id)
        .await?
        .context("task just accepted but not found")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Find and release the active task_claim lease for a given task_id.
async fn release_task_lease(task_id: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let lease_id: Option<String> = sqlx::query_scalar(
        r#"
        SELECT id FROM work_leases
        WHERE task_id = ?1 AND lease_type = 'task_claim' AND status = 'active'
        LIMIT 1
        "#,
    )
    .bind(task_id)
    .fetch_optional(&pool)
    .await?;

    if let Some(lid) = lease_id {
        crate::leases::release(&lid).await?;
    }
    Ok(())
}

fn row_to_task(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<AgentTask> {
    let parse_json_vec = |col: &str| -> anyhow::Result<Vec<String>> {
        let s: String = row.try_get(col)?;
        Ok(serde_json::from_str(&s).unwrap_or_default())
    };

    let parse_utc_opt = |col: &str| -> anyhow::Result<Option<DateTime<Utc>>> {
        let s: Option<String> = row.try_get(col)?;
        match s {
            Some(val) => Ok(Some(
                DateTime::parse_from_rfc3339(&val)?.with_timezone(&Utc),
            )),
            None => Ok(None),
        }
    };

    let parse_utc = |col: &str| -> anyhow::Result<DateTime<Utc>> {
        let s: String = row.try_get(col)?;
        Ok(DateTime::parse_from_rfc3339(&s)?.with_timezone(&Utc))
    };

    Ok(AgentTask {
        id: row.try_get("id")?,
        workspace_id: row.try_get("workspace_id")?,
        goal_id: row.try_get("goal_id")?,
        cycle_id: row.try_get("cycle_id")?,
        parent_task_id: row.try_get("parent_task_id")?,
        title: row.try_get("title")?,
        instruction: row.try_get("instruction")?,
        status: row.try_get("status")?,
        agent_kind: row.try_get("agent_kind")?,
        assigned_agent_id: row.try_get("assigned_agent_id")?,
        claimed_by: row.try_get("claimed_by")?,
        write_scope_json: parse_json_vec("write_scope_json")?,
        read_scope_json: parse_json_vec("read_scope_json")?,
        allowed_tools_json: parse_json_vec("allowed_tools_json")?,
        dependencies_json: parse_json_vec("dependencies_json")?,
        acceptance_json: parse_json_vec("acceptance_json")?,
        result_ref: row.try_get("result_ref")?,
        error: row.try_get("error")?,
        created_at: parse_utc("created_at")?,
        updated_at: parse_utc("updated_at")?,
        claimed_at: parse_utc_opt("claimed_at")?,
        finished_at: parse_utc_opt("finished_at")?,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn create_task_basic() {
        let _root = TestRoot::new();

        let task = create_task(
            "ws-1",
            Some("goal-1"),
            Some("cycle-1"),
            "Write tests",
            "Create unit tests for the module",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec!["tests pass".to_string()],
        )
        .await
        .expect("create_task");

        assert!(!task.id.is_empty());
        assert_eq!(task.workspace_id, "ws-1");
        assert_eq!(task.goal_id.as_deref(), Some("goal-1"));
        assert_eq!(task.cycle_id.as_deref(), Some("cycle-1"));
        assert_eq!(task.title, "Write tests");
        assert_eq!(task.status, "proposed");
        assert_eq!(task.agent_kind, "claude_p");
        assert_eq!(task.acceptance_json, vec!["tests pass"]);
    }

    #[tokio::test]
    async fn claim_task_creates_lease() {
        let _root = TestRoot::new();

        let task = create_task(
            "ws-claim",
            None,
            None,
            "Claimable task",
            "Do something",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        let claimed = claim_task(&task.id, "agent-alpha", 3600)
            .await
            .expect("claim");

        assert_eq!(claimed.status, "claimed");
        assert_eq!(claimed.claimed_by.as_deref(), Some("agent-alpha"));
        assert!(claimed.claimed_at.is_some());

        // Verify lease exists
        let active_leases = crate::leases::list_active_leases("ws-claim")
            .await
            .expect("list leases");
        assert_eq!(active_leases.len(), 1);
        assert_eq!(active_leases[0].task_id.as_deref(), Some(task.id.as_str()));
        assert_eq!(active_leases[0].holder_id, "agent-alpha");
    }

    #[tokio::test]
    async fn claim_task_conflict_returns_error() {
        let _root = TestRoot::new();

        let task = create_task(
            "ws-conflict",
            None,
            None,
            "Contested task",
            "Only one can claim",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        // First claim succeeds
        claim_task(&task.id, "agent-a", 3600)
            .await
            .expect("first claim");

        // Second claim should fail (state machine rejects claimed->claimed,
        // and if it somehow got through, lease conflict would catch it)
        let result = claim_task(&task.id, "agent-b", 3600).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("lease conflict")
                || err.contains("conflict")
                || err.contains("invalid task transition"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn start_task_from_claimed() {
        let _root = TestRoot::new();

        let task = create_task(
            "ws-start",
            None,
            None,
            "Startable task",
            "Run it",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        claim_task(&task.id, "agent-a", 3600).await.expect("claim");

        let running = start_task(&task.id).await.expect("start");
        assert_eq!(running.status, "running");
    }

    #[tokio::test]
    async fn complete_task_releases_lease() {
        let _root = TestRoot::new();

        let task = create_task(
            "ws-complete",
            None,
            None,
            "Completable task",
            "Finish it",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        claim_task(&task.id, "agent-a", 3600).await.expect("claim");
        start_task(&task.id).await.expect("start");

        let accepted = complete_task(&task.id, "output/ref-1")
            .await
            .expect("complete");

        assert_eq!(accepted.status, "accepted");
        assert_eq!(accepted.result_ref.as_deref(), Some("output/ref-1"));
        assert!(accepted.finished_at.is_some());

        // Lease should be released
        let active_leases = crate::leases::list_active_leases("ws-complete")
            .await
            .expect("list leases");
        assert!(
            active_leases.is_empty(),
            "lease should be released after completion"
        );
    }

    #[tokio::test]
    async fn fail_task_releases_lease() {
        let _root = TestRoot::new();

        let task = create_task(
            "ws-fail",
            None,
            None,
            "Failable task",
            "This will fail",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        claim_task(&task.id, "agent-a", 3600).await.expect("claim");
        start_task(&task.id).await.expect("start");

        let failed = fail_task(&task.id, "timeout exceeded").await.expect("fail");

        assert_eq!(failed.status, "failed");
        assert_eq!(failed.error.as_deref(), Some("timeout exceeded"));
        assert!(failed.finished_at.is_some());

        // Lease should be released
        let active_leases = crate::leases::list_active_leases("ws-fail")
            .await
            .expect("list leases");
        assert!(
            active_leases.is_empty(),
            "lease should be released after failure"
        );
    }

    #[tokio::test]
    async fn block_task_from_running() {
        let _root = TestRoot::new();

        let task = create_task(
            "ws-block",
            None,
            None,
            "Blockable task",
            "Might get blocked",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        claim_task(&task.id, "agent-a", 3600).await.expect("claim");
        start_task(&task.id).await.expect("start");

        let blocked = block_task(&task.id, "waiting for approval")
            .await
            .expect("block");

        assert_eq!(blocked.status, "blocked");
        assert_eq!(blocked.error.as_deref(), Some("waiting for approval"));
    }

    #[tokio::test]
    async fn review_ready_writeback_requires_running_status() {
        let _root = TestRoot::new();

        let task = create_task(
            "ws-review-ready",
            None,
            None,
            "Review ready guard",
            "Only running tasks should accept review-ready writeback",
            "backend-agent",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        let err = set_task_result_ref_review_ready(&task.id, "chat:message-1")
            .await
            .expect_err("non-running task should reject review-ready writeback");
        assert!(err.to_string().contains("not moved to review_ready"));
    }

    #[tokio::test]
    async fn blocked_writeback_persists_result_ref_and_reason() {
        let _root = TestRoot::new();

        let task = create_task(
            "ws-blocked-writeback",
            None,
            None,
            "Blocked writeback",
            "Should preserve the partial result when blocked",
            "backend-agent",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        claim_task(&task.id, "agent-a", 3600).await.expect("claim");
        start_task(&task.id).await.expect("start");

        set_task_result_ref_blocked(&task.id, "chat:message-2", "waiting for approval")
            .await
            .expect("blocked writeback");

        let task = get_task(&task.id)
            .await
            .expect("get task")
            .expect("task exists");
        assert_eq!(task.status, "blocked");
        assert_eq!(task.result_ref.as_deref(), Some("chat:message-2"));
        assert_eq!(task.error.as_deref(), Some("waiting for approval"));
    }

    #[tokio::test]
    async fn resume_blocked_task_requeues_and_clears_error() {
        let _root = TestRoot::new();

        let task = create_task(
            "ws-resume-blocked",
            None,
            None,
            "Resume blocked task",
            "queue it again",
            "backend-agent",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        claim_task(&task.id, "agent-a", 3600).await.expect("claim");
        start_task(&task.id).await.expect("start");
        set_task_result_ref_blocked(&task.id, "chat:message-3", "waiting for approval")
            .await
            .expect("blocked writeback");

        let resumed = resume_blocked_task(&task.id)
            .await
            .expect("resume blocked task");
        assert_eq!(resumed.status, "queued");
        assert!(resumed.error.is_none());
        assert!(resumed.result_ref.is_none());
    }

    #[tokio::test]
    async fn invalid_transition_blocked() {
        let _root = TestRoot::new();

        // proposed -> running is not allowed (must go through claimed first)
        assert!(validate_task_transition("proposed", "running").is_err());

        // accepted is terminal
        assert!(validate_task_transition("accepted", "running").is_err());
        assert!(validate_task_transition("accepted", "failed").is_err());

        // failed is terminal
        assert!(validate_task_transition("failed", "running").is_err());

        // cancelled is terminal
        assert!(validate_task_transition("cancelled", "running").is_err());

        // Valid transitions should succeed
        assert!(validate_task_transition("proposed", "claimed").is_ok());
        assert!(validate_task_transition("proposed", "queued").is_ok());
        assert!(validate_task_transition("claimed", "running").is_ok());
        assert!(validate_task_transition("running", "accepted").is_ok());
        assert!(validate_task_transition("running", "failed").is_ok());
        assert!(validate_task_transition("running", "blocked").is_ok());
        assert!(validate_task_transition("blocked", "running").is_ok());
        assert!(validate_task_transition("rework_required", "queued").is_ok());

        // Test the actual API rejects invalid transitions
        let task = create_task(
            "ws-invalid",
            None,
            None,
            "Test invalid transition",
            "desc",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        // Can't start a proposed task directly
        let result = start_task(&task.id).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid task transition"));
    }

    #[tokio::test]
    async fn rework_to_queued() {
        let _root = TestRoot::new();

        // Create a task and manually set it to rework_required
        let task = create_task(
            "ws-rework",
            None,
            None,
            "Reworkable task",
            "Needs revision",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        // Move through the lifecycle to review_ready -> rework_required
        claim_task(&task.id, "agent-a", 3600).await.expect("claim");
        start_task(&task.id).await.expect("start");

        // Manually set to review_ready (via direct DB for test setup)
        let pool = db::pool().await.unwrap();
        sqlx::query("UPDATE agent_tasks SET status = 'review_ready' WHERE id = ?1")
            .bind(&task.id)
            .execute(&pool)
            .await
            .unwrap();

        // Transition to rework_required
        let now = Utc::now();
        sqlx::query(
            "UPDATE agent_tasks SET status = 'rework_required', updated_at = ?1 WHERE id = ?2",
        )
        .bind(now.to_rfc3339())
        .bind(&task.id)
        .execute(&pool)
        .await
        .unwrap();

        let reworked = rework_task(&task.id).await.expect("rework");
        assert_eq!(reworked.status, "queued");
    }

    #[tokio::test]
    async fn list_tasks_by_goal_and_status() {
        let _root = TestRoot::new();

        // Create tasks with different goals and statuses
        let t1 = create_task(
            "ws-list",
            Some("goal-list-1"),
            None,
            "Task A",
            "desc",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create t1");

        let _t2 = create_task(
            "ws-list",
            Some("goal-list-1"),
            None,
            "Task B",
            "desc",
            "codex_interactive",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create t2");

        let _t3 = create_task(
            "ws-list",
            Some("goal-list-2"),
            None,
            "Task C",
            "desc",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create t3");

        // List by goal
        let goal1_tasks = list_tasks_by_goal("goal-list-1")
            .await
            .expect("list by goal");
        assert_eq!(goal1_tasks.len(), 2);
        assert!(goal1_tasks
            .iter()
            .all(|t| t.goal_id.as_deref() == Some("goal-list-1")));

        // List by status
        let proposed = list_tasks_by_status("ws-list", "proposed")
            .await
            .expect("list by status");
        assert_eq!(proposed.len(), 3); // all three are proposed

        // Transition one task, then list again
        claim_task(&t1.id, "agent-a", 3600).await.expect("claim");
        let claimed = list_tasks_by_status("ws-list", "claimed")
            .await
            .expect("list claimed");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].id, t1.id);

        let proposed_after = list_tasks_by_status("ws-list", "proposed")
            .await
            .expect("list proposed after");
        assert_eq!(proposed_after.len(), 2);
    }

    #[tokio::test]
    async fn claim_next_queued_task_picks_oldest_queued() {
        let _root = TestRoot::new();

        let first = create_task(
            "ws-next",
            None,
            None,
            "First queued task",
            "first",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create first");
        let second = create_task(
            "ws-next",
            None,
            None,
            "Second queued task",
            "second",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create second");

        let first = rework_task(&first.id).await.expect("queue first");
        let second = rework_task(&second.id).await.expect("queue second");

        let claimed = claim_next_queued_task(Some("ws-next"), "agent-next", 300)
            .await
            .expect("claim next")
            .expect("queued task");

        assert_eq!(claimed.id, first.id);
        assert_eq!(claimed.status, "claimed");

        let remaining = list_tasks_by_status("ws-next", "queued")
            .await
            .expect("remaining queued");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, second.id);
    }
}
