use crate::db;
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

// ── GoalRun ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GoalRun {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub objective: String,
    pub status: String,
    pub priority: String,
    pub owner: String,
    pub budget_json: Option<Value>,
    pub policy_json: Option<Value>,
    pub current_cycle_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub metadata_json: Option<Value>,
}

// ── GoalCycle ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GoalCycle {
    pub id: String,
    pub goal_id: String,
    pub cycle_no: i64,
    pub status: String,
    pub observe_snapshot_ref: Option<String>,
    pub orientation_json: Option<Value>,
    pub dispatch_plan_id: Option<String>,
    pub review_summary_ref: Option<String>,
    pub last_graph_hash: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

// ── State machine validators ─────────────────────────────────────────────────

/// Validate a GoalRun status transition.
///
/// Legal transitions:
///   draft -> planning
///   planning -> awaiting_plan_approval | running
///   awaiting_plan_approval -> running | rework_required
///   running -> awaiting_review | blocked | failed | cancelled
///   awaiting_review -> accepted | rework_required
///   rework_required -> planning
///   blocked -> running | failed | cancelled
///   failed -> archived
///   cancelled -> archived
///   accepted -> archived
pub fn validate_goal_transition(from: &str, to: &str) -> anyhow::Result<()> {
    let allowed = match from {
        "draft" => vec!["planning"],
        "planning" => vec!["awaiting_plan_approval", "running"],
        "awaiting_plan_approval" => vec!["running", "rework_required"],
        "running" => vec!["awaiting_review", "blocked", "failed", "cancelled"],
        "awaiting_review" => vec!["accepted", "rework_required"],
        "rework_required" => vec!["planning"],
        "blocked" => vec!["running", "failed", "cancelled"],
        "failed" => vec!["archived"],
        "cancelled" => vec!["archived"],
        "accepted" => vec!["archived"],
        _ => bail!("unknown goal status: {from}"),
    };
    if allowed.contains(&to) {
        Ok(())
    } else {
        bail!("invalid goal transition: {from} -> {to}")
    }
}

/// Validate a GoalCycle phase transition.
///
/// For non-blocked states, each state can advance to the next sequential
/// phase, or fall back to failed/blocked/cancelled. The blocked state can
/// resume to any previous non-terminal state.
pub fn validate_cycle_transition(from: &str, to: &str) -> anyhow::Result<()> {
    let phases = [
        "observing",
        "orienting",
        "deciding",
        "dispatching",
        "executing",
        "reviewing",
        "summarizing",
    ];
    let terminal = ["completed", "failed", "blocked", "cancelled"];

    if from == "blocked" {
        // blocked can resume to any non-terminal phase
        if phases.contains(&to) {
            return Ok(());
        }
        bail!("invalid cycle transition from blocked: blocked -> {to}");
    }

    // Terminal states cannot transition
    if terminal.contains(&from) && from != "blocked" {
        bail!("cannot transition from terminal cycle state: {from}");
    }

    // Check forward transition or fallback to terminal
    if terminal.contains(&to) {
        // Any non-terminal state can fall to failed/blocked/cancelled
        if !phases.contains(&from) {
            bail!("unknown cycle state: {from}");
        }
        return Ok(());
    }

    // Forward transition: from must be exactly one phase before to
    if let Some(from_idx) = phases.iter().position(|&p| p == from) {
        if let Some(to_idx) = phases.iter().position(|&p| p == to) {
            if to_idx == from_idx + 1 {
                return Ok(());
            }
        }
    }

    bail!("invalid cycle transition: {from} -> {to}")
}

// ── GoalRun CRUD ─────────────────────────────────────────────────────────────

/// Create a new GoalRun with status "draft".
pub async fn create_goal(
    workspace_id: &str,
    title: &str,
    objective: &str,
    priority: &str,
    owner: &str,
    budget_json: Option<Value>,
    policy_json: Option<Value>,
) -> anyhow::Result<GoalRun> {
    let now = Utc::now();
    let id = format!("goal-{}", Uuid::new_v4());
    let pool = db::pool().await?;

    sqlx::query(
        r#"INSERT INTO goal_runs
          (id, workspace_id, title, objective, status, priority, owner,
           budget_json, policy_json, current_cycle_id, created_at, updated_at,
           finished_at, metadata_json)
          VALUES (?, ?, ?, ?, 'draft', ?, ?, ?, ?, NULL, ?, ?, NULL, NULL)"#,
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(title)
    .bind(objective)
    .bind(priority)
    .bind(owner)
    .bind(budget_json.as_ref().map(|v| v.to_string()))
    .bind(policy_json.as_ref().map(|v| v.to_string()))
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(&pool)
    .await
    .with_context(|| "insert goal_run")?;

    let goal = GoalRun {
        id,
        workspace_id: workspace_id.to_string(),
        title: title.to_string(),
        objective: objective.to_string(),
        status: "draft".to_string(),
        priority: priority.to_string(),
        owner: owner.to_string(),
        budget_json,
        policy_json,
        current_cycle_id: None,
        created_at: now,
        updated_at: now,
        finished_at: None,
        metadata_json: None,
    };

    // Emit event
    let payload = serde_json::json!({
        "goal_id": goal.id,
        "workspace_id": goal.workspace_id,
        "title": goal.title,
        "status": goal.status,
    });
    let _ = crate::events::append("goal", "goal.created", &payload).await;

    Ok(goal)
}

/// Fetch a single GoalRun by id.
pub async fn get_goal(goal_id: &str) -> anyhow::Result<Option<GoalRun>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"SELECT id, workspace_id, title, objective, status, priority, owner,
                  budget_json, policy_json, current_cycle_id,
                  created_at, updated_at, finished_at, metadata_json
           FROM goal_runs WHERE id = ?"#,
    )
    .bind(goal_id)
    .fetch_optional(&pool)
    .await
    .with_context(|| "fetch goal_run")?;

    match row {
        Some(row) => Ok(Some(row_to_goal(&row)?)),
        None => Ok(None),
    }
}

/// List GoalRuns for a workspace, with optional status filter and limit.
pub async fn list_goals(
    workspace_id: &str,
    status_filter: Option<&str>,
    limit: Option<i64>,
) -> anyhow::Result<Vec<GoalRun>> {
    let pool = db::pool().await?;
    let cap = limit.unwrap_or(100);

    let rows = match status_filter {
        Some(status) => {
            sqlx::query(
                r#"SELECT id, workspace_id, title, objective, status, priority, owner,
                          budget_json, policy_json, current_cycle_id,
                          created_at, updated_at, finished_at, metadata_json
                   FROM goal_runs
                   WHERE workspace_id = ? AND status = ?
                   ORDER BY updated_at DESC
                   LIMIT ?"#,
            )
            .bind(workspace_id)
            .bind(status)
            .bind(cap)
            .fetch_all(&pool)
            .await?
        }
        None => {
            sqlx::query(
                r#"SELECT id, workspace_id, title, objective, status, priority, owner,
                          budget_json, policy_json, current_cycle_id,
                          created_at, updated_at, finished_at, metadata_json
                   FROM goal_runs
                   WHERE workspace_id = ?
                   ORDER BY updated_at DESC
                   LIMIT ?"#,
            )
            .bind(workspace_id)
            .bind(cap)
            .fetch_all(&pool)
            .await?
        }
    };

    rows.iter().map(row_to_goal).collect()
}

/// List non-terminal goals across all workspaces.
pub async fn list_active_goals(limit: Option<i64>) -> anyhow::Result<Vec<GoalRun>> {
    let pool = db::pool().await?;
    let cap = limit.unwrap_or(100);

    let rows = sqlx::query(
        r#"SELECT id, workspace_id, title, objective, status, priority, owner,
                  budget_json, policy_json, current_cycle_id,
                  created_at, updated_at, finished_at, metadata_json
           FROM goal_runs
           WHERE status NOT IN ('accepted', 'archived', 'cancelled', 'failed')
           ORDER BY updated_at DESC
           LIMIT ?"#,
    )
    .bind(cap)
    .fetch_all(&pool)
    .await?;

    rows.iter().map(row_to_goal).collect()
}

/// Update GoalRun status with transition validation.
pub async fn update_goal_status(goal_id: &str, new_status: &str) -> anyhow::Result<GoalRun> {
    let mut goal = get_goal(goal_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;

    validate_goal_transition(&goal.status, new_status)?;

    let now = Utc::now();
    let finished_at = if matches!(new_status, "accepted" | "archived" | "failed" | "cancelled") {
        Some(now)
    } else {
        None
    };

    let pool = db::pool().await?;
    sqlx::query(
        "UPDATE goal_runs SET status = ?, updated_at = ?, finished_at = COALESCE(?, finished_at) WHERE id = ?",
    )
    .bind(new_status)
    .bind(now.to_rfc3339())
    .bind(finished_at.map(|dt| dt.to_rfc3339()))
    .bind(goal_id)
    .execute(&pool)
    .await
    .with_context(|| "update goal status")?;

    let old_status = goal.status.clone();
    goal.status = new_status.to_string();
    goal.updated_at = now;
    if finished_at.is_some() {
        goal.finished_at = finished_at;
    }

    // Emit event
    let payload = serde_json::json!({
        "goal_id": goal.id,
        "from": old_status,
        "to": new_status,
    });
    let _ = crate::events::append("goal", "goal.status_changed", &payload).await;

    Ok(goal)
}

/// Update the goal title/objective without changing its lifecycle status.
pub async fn update_goal_objective(
    goal_id: &str,
    title: &str,
    objective: &str,
) -> anyhow::Result<GoalRun> {
    let mut goal = get_goal(goal_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;

    let title = title.trim();
    let objective = objective.trim();
    if title.is_empty() {
        bail!("goal title cannot be empty");
    }
    if objective.is_empty() {
        bail!("goal objective cannot be empty");
    }

    let previous_title = goal.title.clone();
    let previous_objective = goal.objective.clone();
    let now = Utc::now();
    let pool = db::pool().await?;
    sqlx::query("UPDATE goal_runs SET title = ?, objective = ?, updated_at = ? WHERE id = ?")
        .bind(title)
        .bind(objective)
        .bind(now.to_rfc3339())
        .bind(goal_id)
        .execute(&pool)
        .await
        .with_context(|| "update goal objective")?;

    goal.title = title.to_string();
    goal.objective = objective.to_string();
    goal.updated_at = now;

    let payload = serde_json::json!({
        "goal_id": goal.id,
        "previous_title": previous_title,
        "title": goal.title,
        "previous_objective": previous_objective,
        "objective": goal.objective,
    });
    let _ = crate::events::append("goal", "goal.objective_changed", &payload).await;

    Ok(goal)
}

/// Delete a GoalRun and cascade-delete its cycles.
pub async fn delete_goal(goal_id: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;

    // Cascade delete cycles first
    sqlx::query("DELETE FROM goal_cycles WHERE goal_id = ?")
        .bind(goal_id)
        .execute(&pool)
        .await
        .with_context(|| "cascade delete goal_cycles")?;

    // Delete the goal
    let result = sqlx::query("DELETE FROM goal_runs WHERE id = ?")
        .bind(goal_id)
        .execute(&pool)
        .await
        .with_context(|| "delete goal_run")?;

    if result.rows_affected() == 0 {
        bail!("goal not found: {goal_id}");
    }

    Ok(())
}

// ── GoalCycle CRUD ───────────────────────────────────────────────────────────

/// Create a new GoalCycle with status "observing".
pub async fn create_cycle(goal_id: &str, cycle_no: i64) -> anyhow::Result<GoalCycle> {
    let now = Utc::now();
    let id = format!("cycle-{}", Uuid::new_v4());
    let pool = db::pool().await?;

    sqlx::query(
        r#"INSERT INTO goal_cycles
          (id, goal_id, cycle_no, status, observe_snapshot_ref, orientation_json,
           dispatch_plan_id, review_summary_ref, started_at, updated_at, finished_at)
          VALUES (?, ?, ?, 'observing', NULL, NULL, NULL, NULL, ?, ?, NULL)"#,
    )
    .bind(&id)
    .bind(goal_id)
    .bind(cycle_no)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(&pool)
    .await
    .with_context(|| "insert goal_cycle")?;

    // Update the goal's current_cycle_id
    sqlx::query("UPDATE goal_runs SET current_cycle_id = ?, updated_at = ? WHERE id = ?")
        .bind(&id)
        .bind(now.to_rfc3339())
        .bind(goal_id)
        .execute(&pool)
        .await
        .with_context(|| "update goal current_cycle_id")?;

    let cycle = GoalCycle {
        id,
        goal_id: goal_id.to_string(),
        cycle_no,
        status: "observing".to_string(),
        observe_snapshot_ref: None,
        orientation_json: None,
        dispatch_plan_id: None,
        review_summary_ref: None,
        last_graph_hash: None,
        started_at: now,
        updated_at: now,
        finished_at: None,
    };

    // Emit event
    let payload = serde_json::json!({
        "cycle_id": cycle.id,
        "goal_id": cycle.goal_id,
        "cycle_no": cycle.cycle_no,
        "status": cycle.status,
    });
    let _ = crate::events::append("goal", "goal_cycle.created", &payload).await;

    Ok(cycle)
}

/// Fetch a single GoalCycle by id.
pub async fn get_cycle(cycle_id: &str) -> anyhow::Result<Option<GoalCycle>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"SELECT id, goal_id, cycle_no, status, observe_snapshot_ref,
                  orientation_json, dispatch_plan_id, review_summary_ref,
                  last_graph_hash, started_at, updated_at, finished_at
           FROM goal_cycles WHERE id = ?"#,
    )
    .bind(cycle_id)
    .fetch_optional(&pool)
    .await
    .with_context(|| "fetch goal_cycle")?;

    match row {
        Some(row) => Ok(Some(row_to_cycle(&row)?)),
        None => Ok(None),
    }
}

/// List all GoalCycles for a given goal, ordered by cycle_no descending.
pub async fn list_cycles_by_goal(goal_id: &str) -> anyhow::Result<Vec<GoalCycle>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"SELECT id, goal_id, cycle_no, status, observe_snapshot_ref,
                  orientation_json, dispatch_plan_id, review_summary_ref,
                  last_graph_hash, started_at, updated_at, finished_at
           FROM goal_cycles
           WHERE goal_id = ?
           ORDER BY cycle_no DESC"#,
    )
    .bind(goal_id)
    .fetch_all(&pool)
    .await
    .with_context(|| "list goal_cycles")?;

    rows.iter().map(row_to_cycle).collect()
}

/// Advance a GoalCycle to a new phase with transition validation.
pub async fn advance_cycle_phase(cycle_id: &str, new_status: &str) -> anyhow::Result<GoalCycle> {
    let mut cycle = get_cycle(cycle_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("cycle not found: {cycle_id}"))?;

    validate_cycle_transition(&cycle.status, new_status)?;

    let now = Utc::now();
    let finished_at = if matches!(new_status, "completed" | "failed" | "cancelled") {
        Some(now)
    } else {
        None
    };

    let pool = db::pool().await?;
    sqlx::query(
        "UPDATE goal_cycles SET status = ?, updated_at = ?, finished_at = COALESCE(?, finished_at) WHERE id = ?",
    )
    .bind(new_status)
    .bind(now.to_rfc3339())
    .bind(finished_at.map(|dt| dt.to_rfc3339()))
    .bind(cycle_id)
    .execute(&pool)
    .await
    .with_context(|| "update cycle status")?;

    let old_status = cycle.status.clone();
    cycle.status = new_status.to_string();
    cycle.updated_at = now;
    if finished_at.is_some() {
        cycle.finished_at = finished_at;
    }

    // Emit event
    let payload = serde_json::json!({
        "cycle_id": cycle.id,
        "goal_id": cycle.goal_id,
        "from": old_status,
        "to": new_status,
    });
    let _ = crate::events::append("goal", "goal_cycle.phase_changed", &payload).await;

    Ok(cycle)
}

/// Update the cycle's review summary reference without changing lifecycle phase.
pub async fn set_cycle_review_summary_ref(
    cycle_id: &str,
    review_summary_ref: Option<&str>,
) -> anyhow::Result<GoalCycle> {
    let mut cycle = get_cycle(cycle_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("cycle not found: {cycle_id}"))?;

    let now = Utc::now();
    let pool = db::pool().await?;
    sqlx::query("UPDATE goal_cycles SET review_summary_ref = ?, updated_at = ? WHERE id = ?")
        .bind(review_summary_ref)
        .bind(now.to_rfc3339())
        .bind(cycle_id)
        .execute(&pool)
        .await
        .with_context(|| "update cycle review_summary_ref")?;

    cycle.review_summary_ref = review_summary_ref.map(str::to_string);
    cycle.updated_at = now;

    let payload = serde_json::json!({
        "cycle_id": cycle.id,
        "goal_id": cycle.goal_id,
        "review_summary_ref": cycle.review_summary_ref,
    });
    let _ = crate::events::append("goal", "goal_cycle.review_summary_updated", &payload).await;

    Ok(cycle)
}

/// Read back the last stored graph hash for a cycle (None if never set).
pub async fn get_cycle_hash(cycle_id: &str) -> anyhow::Result<Option<String>> {
    let pool = db::pool().await?;
    let hash: Option<String> =
        sqlx::query_scalar("SELECT last_graph_hash FROM goal_cycles WHERE id = ?")
            .bind(cycle_id)
            .fetch_optional(&pool)
            .await
            .with_context(|| "get_cycle_hash")?
            .flatten();
    Ok(hash)
}

/// Persist a new graph hash for a cycle without changing its lifecycle phase.
pub async fn set_cycle_hash(cycle_id: &str, hash: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let now = Utc::now();
    sqlx::query("UPDATE goal_cycles SET last_graph_hash = ?, updated_at = ? WHERE id = ?")
        .bind(hash)
        .bind(now.to_rfc3339())
        .bind(cycle_id)
        .execute(&pool)
        .await
        .with_context(|| "set_cycle_hash")?;
    Ok(())
}

/// Persist a DispatchPlan to the dispatch_plans table and link it to the cycle. (S1)
pub async fn create_dispatch_plan(
    goal_id: &str,
    cycle_id: &str,
    tasks_json: &str,
    summary: &str,
) -> anyhow::Result<String> {
    let pool = db::pool().await?;
    let id = format!("dp-{}", uuid::Uuid::new_v4());
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"INSERT INTO dispatch_plans
           (id, goal_id, cycle_id, status, summary, tasks_json, created_at)
           VALUES (?1, ?2, ?3, 'proposed', ?4, ?5, ?6)"#,
    )
    .bind(&id)
    .bind(goal_id)
    .bind(cycle_id)
    .bind(summary)
    .bind(tasks_json)
    .bind(&now)
    .execute(&pool)
    .await
    .with_context(|| "create_dispatch_plan")?;

    sqlx::query("UPDATE goal_cycles SET dispatch_plan_id = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(&id)
        .bind(&now)
        .bind(cycle_id)
        .execute(&pool)
        .await
        .with_context(|| "set_cycle_dispatch_plan_id")?;

    Ok(id)
}

// ── Row mapping helpers ──────────────────────────────────────────────────────

fn row_to_goal(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<GoalRun> {
    let created_at = parse_utc(&row.try_get::<String, _>("created_at")?)?;
    let updated_at = parse_utc(&row.try_get::<String, _>("updated_at")?)?;
    let finished_at: Option<String> = row.try_get("finished_at")?;
    let finished_at = finished_at.as_deref().map(parse_utc).transpose()?;

    let budget_json: Option<String> = row.try_get("budget_json")?;
    let policy_json: Option<String> = row.try_get("policy_json")?;
    let metadata_json: Option<String> = row.try_get("metadata_json")?;

    Ok(GoalRun {
        id: row.try_get("id")?,
        workspace_id: row.try_get("workspace_id")?,
        title: row.try_get("title")?,
        objective: row.try_get("objective")?,
        status: row.try_get("status")?,
        priority: row.try_get("priority")?,
        owner: row.try_get("owner")?,
        budget_json: budget_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?,
        policy_json: policy_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?,
        current_cycle_id: row.try_get("current_cycle_id")?,
        created_at,
        updated_at,
        finished_at,
        metadata_json: metadata_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?,
    })
}

fn row_to_cycle(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<GoalCycle> {
    let started_at = parse_utc(&row.try_get::<String, _>("started_at")?)?;
    let updated_at = parse_utc(&row.try_get::<String, _>("updated_at")?)?;
    let finished_at: Option<String> = row.try_get("finished_at")?;
    let finished_at = finished_at.as_deref().map(parse_utc).transpose()?;

    let orientation_json: Option<String> = row.try_get("orientation_json")?;

    Ok(GoalCycle {
        id: row.try_get("id")?,
        goal_id: row.try_get("goal_id")?,
        cycle_no: row.try_get("cycle_no")?,
        status: row.try_get("status")?,
        observe_snapshot_ref: row.try_get("observe_snapshot_ref")?,
        orientation_json: orientation_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?,
        dispatch_plan_id: row.try_get("dispatch_plan_id")?,
        review_summary_ref: row.try_get("review_summary_ref")?,
        last_graph_hash: row.try_get("last_graph_hash")?,
        started_at,
        updated_at,
        finished_at,
    })
}

fn parse_utc(value: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("parse RFC3339 datetime: {value}"))?
        .with_timezone(&Utc))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn create_goal_basic() {
        let _root = TestRoot::new();

        let goal = create_goal(
            "ws-test",
            "Ship v1.0",
            "Release the first production version",
            "p1",
            "user",
            None,
            None,
        )
        .await
        .expect("create_goal");

        assert!(!goal.id.is_empty());
        assert_eq!(goal.workspace_id, "ws-test");
        assert_eq!(goal.title, "Ship v1.0");
        assert_eq!(goal.objective, "Release the first production version");
        assert_eq!(goal.status, "draft");
        assert_eq!(goal.priority, "p1");
        assert_eq!(goal.owner, "user");
        assert!(goal.current_cycle_id.is_none());
        assert!(goal.finished_at.is_none());
    }

    #[tokio::test]
    async fn goal_status_draft_to_planning() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        let updated = update_goal_status(&goal.id, "planning")
            .await
            .expect("transition to planning");
        assert_eq!(updated.status, "planning");
    }

    #[tokio::test]
    async fn goal_status_planning_to_running_directly() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        update_goal_status(&goal.id, "planning")
            .await
            .expect("to planning");
        let updated = update_goal_status(&goal.id, "running")
            .await
            .expect("transition directly to running");
        assert_eq!(updated.status, "running");
    }

    #[tokio::test]
    async fn goal_status_invalid_transition_blocked() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        // draft -> running is invalid
        let result = update_goal_status(&goal.id, "running").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid goal transition"));
    }

    #[tokio::test]
    async fn goal_status_running_to_awaiting_review() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        // draft -> planning -> awaiting_plan_approval -> running -> awaiting_review
        update_goal_status(&goal.id, "planning")
            .await
            .expect("to planning");
        update_goal_status(&goal.id, "awaiting_plan_approval")
            .await
            .expect("to awaiting_plan_approval");
        let running = update_goal_status(&goal.id, "running")
            .await
            .expect("to running");
        assert_eq!(running.status, "running");

        let review = update_goal_status(&goal.id, "awaiting_review")
            .await
            .expect("to awaiting_review");
        assert_eq!(review.status, "awaiting_review");
    }

    #[tokio::test]
    async fn update_goal_objective_preserves_status() {
        let _root = TestRoot::new();

        let goal = create_goal(
            "ws-1",
            "Initial title",
            "Initial objective",
            "p2",
            "user",
            None,
            None,
        )
        .await
        .expect("create");
        let goal = update_goal_status(&goal.id, "planning")
            .await
            .expect("to planning");

        let updated = update_goal_objective(
            &goal.id,
            "Updated runtime objective",
            "User request:\nFinish the runtime chain before UI polish",
        )
        .await
        .expect("update objective");

        assert_eq!(updated.status, "planning");
        assert_eq!(updated.title, "Updated runtime objective");
        assert_eq!(
            updated.objective,
            "User request:\nFinish the runtime chain before UI polish"
        );
        assert!(updated.updated_at >= goal.updated_at);
    }

    #[tokio::test]
    async fn goal_status_accepted_to_archived() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        // Walk the full happy path to accepted
        update_goal_status(&goal.id, "planning").await.unwrap();
        update_goal_status(&goal.id, "awaiting_plan_approval")
            .await
            .unwrap();
        update_goal_status(&goal.id, "running").await.unwrap();
        update_goal_status(&goal.id, "awaiting_review")
            .await
            .unwrap();
        let accepted = update_goal_status(&goal.id, "accepted").await.unwrap();
        assert_eq!(accepted.status, "accepted");
        assert!(accepted.finished_at.is_some());

        let archived = update_goal_status(&goal.id, "archived").await.unwrap();
        assert_eq!(archived.status, "archived");
    }

    #[tokio::test]
    async fn delete_goal_cascades_cycles() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        create_cycle(&goal.id, 1).await.expect("cycle 1");
        create_cycle(&goal.id, 2).await.expect("cycle 2");

        let cycles = list_cycles_by_goal(&goal.id).await.expect("list cycles");
        assert_eq!(cycles.len(), 2);

        delete_goal(&goal.id).await.expect("delete goal");

        // Goal should be gone
        assert!(get_goal(&goal.id).await.expect("get").is_none());

        // Cycles should also be gone
        let remaining = list_cycles_by_goal(&goal.id).await.expect("list");
        assert_eq!(remaining.len(), 0);
    }

    #[tokio::test]
    async fn create_cycle_basic() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        let cycle = create_cycle(&goal.id, 1).await.expect("create_cycle");

        assert!(!cycle.id.is_empty());
        assert_eq!(cycle.goal_id, goal.id);
        assert_eq!(cycle.cycle_no, 1);
        assert_eq!(cycle.status, "observing");
        assert!(cycle.finished_at.is_none());

        // Verify goal's current_cycle_id was updated
        let updated_goal = get_goal(&goal.id).await.expect("get").unwrap();
        assert_eq!(
            updated_goal.current_cycle_id.as_deref(),
            Some(cycle.id.as_str())
        );
    }

    #[tokio::test]
    async fn cycle_advance_observing_to_orienting() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        let cycle = create_cycle(&goal.id, 1).await.expect("create");

        let advanced = advance_cycle_phase(&cycle.id, "orienting")
            .await
            .expect("advance to orienting");
        assert_eq!(advanced.status, "orienting");
    }

    #[tokio::test]
    async fn cycle_advance_invalid_blocked() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        let cycle = create_cycle(&goal.id, 1).await.expect("create");

        // observing -> deciding (skip orienting) is invalid
        let result = advance_cycle_phase(&cycle.id, "deciding").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid cycle transition"));
    }

    #[tokio::test]
    async fn list_cycles_by_goal_ordered() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        create_cycle(&goal.id, 1).await.expect("cycle 1");
        create_cycle(&goal.id, 2).await.expect("cycle 2");
        create_cycle(&goal.id, 3).await.expect("cycle 3");

        let cycles = list_cycles_by_goal(&goal.id).await.expect("list");
        assert_eq!(cycles.len(), 3);
        // Ordered by cycle_no DESC
        assert_eq!(cycles[0].cycle_no, 3);
        assert_eq!(cycles[1].cycle_no, 2);
        assert_eq!(cycles[2].cycle_no, 1);
    }

    #[tokio::test]
    async fn list_goals_with_status_filter() {
        let _root = TestRoot::new();

        let g1 = create_goal("ws-1", "Goal 1", "Obj", "p1", "user", None, None)
            .await
            .expect("create");
        create_goal("ws-1", "Goal 2", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        // Move g1 to planning
        update_goal_status(&g1.id, "planning")
            .await
            .expect("update");

        // Filter by draft
        let drafts = list_goals("ws-1", Some("draft"), None)
            .await
            .expect("list drafts");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].title, "Goal 2");

        // Filter by planning
        let planning = list_goals("ws-1", Some("planning"), None)
            .await
            .expect("list planning");
        assert_eq!(planning.len(), 1);
        assert_eq!(planning[0].title, "Goal 1");

        // No filter returns all
        let all = list_goals("ws-1", None, None).await.expect("list all");
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn goal_rework_required_to_planning() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        // Path to rework_required via awaiting_plan_approval
        update_goal_status(&goal.id, "planning").await.unwrap();
        update_goal_status(&goal.id, "awaiting_plan_approval")
            .await
            .unwrap();
        let rework = update_goal_status(&goal.id, "rework_required")
            .await
            .expect("to rework_required");
        assert_eq!(rework.status, "rework_required");

        // rework_required -> planning
        let back = update_goal_status(&goal.id, "planning")
            .await
            .expect("back to planning");
        assert_eq!(back.status, "planning");
    }

    // ── Additional cycle transition tests ────────────────────────────────────

    #[tokio::test]
    async fn cycle_full_happy_path() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        let cycle = create_cycle(&goal.id, 1).await.expect("create");

        let phases = [
            "orienting",
            "deciding",
            "dispatching",
            "executing",
            "reviewing",
            "summarizing",
            "completed",
        ];

        let mut current = cycle;
        for phase in &phases {
            current = advance_cycle_phase(&current.id, phase)
                .await
                .unwrap_or_else(|e| panic!("advance to {phase}: {e}"));
            assert_eq!(current.status, *phase);
        }
        assert!(current.finished_at.is_some());
    }

    #[tokio::test]
    async fn cycle_blocked_resumes_to_previous_phase() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");

        let cycle = create_cycle(&goal.id, 1).await.expect("create");

        // observing -> orienting -> blocked
        advance_cycle_phase(&cycle.id, "orienting").await.unwrap();
        let blocked = advance_cycle_phase(&cycle.id, "blocked").await.unwrap();
        assert_eq!(blocked.status, "blocked");

        // blocked -> observing (resume)
        let resumed = advance_cycle_phase(&blocked.id, "observing").await.unwrap();
        assert_eq!(resumed.status, "observing");
    }

    #[tokio::test]
    async fn set_cycle_review_summary_ref_persists_value() {
        let _root = TestRoot::new();

        let goal = create_goal("ws-1", "Test", "Obj", "p2", "user", None, None)
            .await
            .expect("create");
        let cycle = create_cycle(&goal.id, 1).await.expect("create");

        let updated = set_cycle_review_summary_ref(&cycle.id, Some("chat:message-1"))
            .await
            .expect("update review summary");
        assert_eq!(
            updated.review_summary_ref.as_deref(),
            Some("chat:message-1")
        );

        let reloaded = get_cycle(&cycle.id).await.expect("reload").expect("cycle");
        assert_eq!(
            reloaded.review_summary_ref.as_deref(),
            Some("chat:message-1")
        );
    }
}
