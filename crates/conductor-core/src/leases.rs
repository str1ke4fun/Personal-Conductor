use crate::db;
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorkLease {
    pub id: String,
    pub workspace_id: String,
    pub holder_id: String,
    pub task_id: Option<String>,
    pub lease_type: String, // task_claim | write_scope | command | review
    pub scope_json: Vec<String>,
    pub status: String, // active | released | expired | revoked
    pub ttl_seconds: i64,
    pub acquired_at: DateTime<Utc>,
    pub renewed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub released_at: Option<DateTime<Utc>>,
}

/// Acquire a new lease. Checks for conflicting active leases before inserting.
pub async fn acquire(
    workspace_id: &str,
    holder_id: &str,
    task_id: Option<&str>,
    lease_type: &str,
    scope_json: Vec<String>,
    ttl_seconds: i64,
) -> anyhow::Result<WorkLease> {
    let pool = db::pool().await?;
    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(ttl_seconds);
    let scope_str = serde_json::to_string(&scope_json)?;

    // Conflict detection
    check_conflicts(&pool, workspace_id, task_id, lease_type, &scope_json).await?;

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO work_leases (
            id, workspace_id, holder_id, task_id, lease_type,
            scope_json, status, ttl_seconds,
            acquired_at, renewed_at, expires_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(holder_id)
    .bind(task_id)
    .bind(lease_type)
    .bind(&scope_str)
    .bind("active")
    .bind(ttl_seconds)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(expires_at.to_rfc3339())
    .execute(&pool)
    .await?;

    Ok(WorkLease {
        id,
        workspace_id: workspace_id.to_string(),
        holder_id: holder_id.to_string(),
        task_id: task_id.map(String::from),
        lease_type: lease_type.to_string(),
        scope_json,
        status: "active".to_string(),
        ttl_seconds,
        acquired_at: now,
        renewed_at: now,
        expires_at,
        released_at: None,
    })
}

/// Renew an active, non-expired lease. Updates renewed_at and expires_at.
pub async fn renew(lease_id: &str) -> anyhow::Result<WorkLease> {
    let pool = db::pool().await?;
    let now = Utc::now();

    let row = sqlx::query(
        "SELECT id, workspace_id, holder_id, task_id, lease_type, scope_json, status, ttl_seconds, acquired_at, renewed_at, expires_at, released_at FROM work_leases WHERE id = ?1",
    )
    .bind(lease_id)
    .fetch_optional(&pool)
    .await
    .with_context(|| format!("lease not found: {lease_id}"))?;

    let Some(row) = row else {
        bail!("lease not found: {lease_id}");
    };

    let status: String = row.try_get("status")?;
    if status != "active" {
        bail!("cannot renew lease {lease_id}: status is {status}");
    }

    let expires_at_str: String = row.try_get("expires_at")?;
    let expires_at = DateTime::parse_from_rfc3339(&expires_at_str)?.with_timezone(&Utc);
    if now > expires_at {
        bail!("cannot renew lease {lease_id}: already expired");
    }

    let ttl: i64 = row.try_get("ttl_seconds")?;
    let new_expires = now + chrono::Duration::seconds(ttl);

    sqlx::query("UPDATE work_leases SET renewed_at = ?1, expires_at = ?2 WHERE id = ?3")
        .bind(now.to_rfc3339())
        .bind(new_expires.to_rfc3339())
        .bind(lease_id)
        .execute(&pool)
        .await?;

    row_to_lease(row).map(|mut lease| {
        lease.renewed_at = now;
        lease.expires_at = new_expires;
        lease
    })
}

/// Release a lease, marking it as released.
pub async fn release(lease_id: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let now = Utc::now();

    let result = sqlx::query(
        "UPDATE work_leases SET status = 'released', released_at = ?1 WHERE id = ?2 AND status = 'active'",
    )
    .bind(now.to_rfc3339())
    .bind(lease_id)
    .execute(&pool)
    .await?;

    if result.rows_affected() == 0 {
        bail!("lease not found or not active: {lease_id}");
    }
    Ok(())
}

/// Scan for active leases that have expired, mark them as expired, and return them.
pub async fn expire_scan() -> anyhow::Result<Vec<WorkLease>> {
    let pool = db::pool().await?;
    let now = Utc::now();
    let now_str = now.to_rfc3339();

    let rows = sqlx::query(
        r#"
        SELECT id, workspace_id, holder_id, task_id, lease_type, scope_json, status, ttl_seconds,
               acquired_at, renewed_at, expires_at, released_at
        FROM work_leases
        WHERE status = 'active' AND expires_at < ?1
        "#,
    )
    .bind(&now_str)
    .fetch_all(&pool)
    .await?;

    let expired_ids: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("id").ok())
        .collect();

    for id in &expired_ids {
        sqlx::query("UPDATE work_leases SET status = 'expired', released_at = ?1 WHERE id = ?2")
            .bind(&now_str)
            .bind(id)
            .execute(&pool)
            .await?;
    }

    // Convert rows, overriding status to "expired" and released_at to now
    rows.into_iter()
        .map(|row| {
            let mut lease = row_to_lease(row)?;
            lease.status = "expired".to_string();
            lease.released_at = Some(now);
            Ok(lease)
        })
        .collect()
}

/// Get a single lease by ID.
pub async fn get_lease(lease_id: &str) -> anyhow::Result<Option<WorkLease>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        "SELECT id, workspace_id, holder_id, task_id, lease_type, scope_json, status, ttl_seconds, acquired_at, renewed_at, expires_at, released_at FROM work_leases WHERE id = ?1",
    )
    .bind(lease_id)
    .fetch_optional(&pool)
    .await?;
    row.map(row_to_lease).transpose()
}

/// List all active leases for a workspace.
pub async fn list_active_leases(workspace_id: &str) -> anyhow::Result<Vec<WorkLease>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, workspace_id, holder_id, task_id, lease_type, scope_json, status, ttl_seconds,
               acquired_at, renewed_at, expires_at, released_at
        FROM work_leases
        WHERE workspace_id = ?1 AND status = 'active'
        ORDER BY acquired_at ASC
        "#,
    )
    .bind(workspace_id)
    .fetch_all(&pool)
    .await?;
    rows.into_iter().map(row_to_lease).collect()
}

// ── Conflict detection ──────────────────────────────────────────────────

async fn check_conflicts(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
    task_id: Option<&str>,
    lease_type: &str,
    scope_json: &[String],
) -> anyhow::Result<()> {
    match lease_type {
        "task_claim" => {
            if let Some(tid) = task_id {
                let count: i64 = sqlx::query_scalar(
                    r#"
                    SELECT COUNT(*) FROM work_leases
                    WHERE workspace_id = ?1 AND task_id = ?2 AND status = 'active'
                    "#,
                )
                .bind(workspace_id)
                .bind(tid)
                .fetch_one(pool)
                .await?;
                if count > 0 {
                    bail!(
                        "lease conflict: task_id '{}' already has an active lease in workspace {}",
                        tid,
                        workspace_id
                    );
                }
            }
        }
        "write_scope" => {
            // Fetch all active write_scope leases in this workspace
            let rows = sqlx::query(
                r#"
                SELECT scope_json FROM work_leases
                WHERE workspace_id = ?1 AND lease_type = 'write_scope' AND status = 'active'
                "#,
            )
            .bind(workspace_id)
            .fetch_all(pool)
            .await?;

            for row in rows {
                let existing_str: String = row.try_get("scope_json")?;
                let existing: Vec<String> = serde_json::from_str(&existing_str)?;
                if paths_overlap(scope_json, &existing) {
                    bail!(
                        "lease conflict: overlapping write scope paths with existing lease in workspace {}",
                        workspace_id
                    );
                }
            }
        }
        _ => {
            // command, review, etc. — no conflict check
        }
    }
    Ok(())
}

/// Check if two sets of file paths overlap (parent/child relationship).
fn paths_overlap(a: &[String], b: &[String]) -> bool {
    for pa in a {
        for pb in b {
            if path_is_parent(pa, pb) || path_is_parent(pb, pa) || pa == pb {
                return true;
            }
        }
    }
    false
}

/// Returns true if `parent` is a path prefix (directory ancestor) of `child`.
fn path_is_parent(parent: &str, child: &str) -> bool {
    if parent.is_empty() || child.is_empty() {
        return false;
    }
    let p = normalize_slashes(parent);
    let c = normalize_slashes(child);
    if p == c {
        return true;
    }
    // Ensure the parent ends with a separator for prefix matching
    let prefix = if p.ends_with('/') { p } else { format!("{p}/") };
    c.starts_with(&prefix)
}

fn normalize_slashes(s: &str) -> String {
    s.replace('\\', "/")
}

fn row_to_lease(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<WorkLease> {
    let scope_str: String = row.try_get("scope_json")?;
    let scope_json: Vec<String> = serde_json::from_str(&scope_str)?;

    let acquired_at =
        DateTime::parse_from_rfc3339(row.try_get::<String, _>("acquired_at")?.as_str())?
            .with_timezone(&Utc);
    let renewed_at =
        DateTime::parse_from_rfc3339(row.try_get::<String, _>("renewed_at")?.as_str())?
            .with_timezone(&Utc);
    let expires_at =
        DateTime::parse_from_rfc3339(row.try_get::<String, _>("expires_at")?.as_str())?
            .with_timezone(&Utc);
    let released_at = row
        .try_get::<Option<String>, _>("released_at")?
        .as_deref()
        .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
        .transpose()?;

    Ok(WorkLease {
        id: row.try_get("id")?,
        workspace_id: row.try_get("workspace_id")?,
        holder_id: row.try_get("holder_id")?,
        task_id: row.try_get("task_id")?,
        lease_type: row.try_get("lease_type")?,
        scope_json,
        status: row.try_get("status")?,
        ttl_seconds: row.try_get("ttl_seconds")?,
        acquired_at,
        renewed_at,
        expires_at,
        released_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn acquire_basic() {
        let _root = TestRoot::new();

        let lease = acquire(
            "ws-1",
            "agent-a",
            Some("task-1"),
            "task_claim",
            vec![],
            3600,
        )
        .await
        .expect("acquire");

        assert_eq!(lease.status, "active");
        assert_eq!(lease.workspace_id, "ws-1");
        assert_eq!(lease.holder_id, "agent-a");
        assert_eq!(lease.task_id.as_deref(), Some("task-1"));
        assert_eq!(lease.lease_type, "task_claim");
        assert_eq!(lease.ttl_seconds, 3600);
        assert!(lease.released_at.is_none());
    }

    #[tokio::test]
    async fn acquire_task_claim_conflict() {
        let _root = TestRoot::new();

        acquire(
            "ws-1",
            "agent-a",
            Some("task-1"),
            "task_claim",
            vec![],
            3600,
        )
        .await
        .expect("first acquire");

        let result = acquire(
            "ws-1",
            "agent-b",
            Some("task-1"),
            "task_claim",
            vec![],
            3600,
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("lease conflict"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn acquire_write_scope_conflict() {
        let _root = TestRoot::new();

        acquire(
            "ws-1",
            "agent-a",
            None,
            "write_scope",
            vec!["src/".to_string()],
            3600,
        )
        .await
        .expect("first acquire");

        let result = acquire(
            "ws-1",
            "agent-b",
            None,
            "write_scope",
            vec!["src/main.rs".to_string()],
            3600,
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("lease conflict"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn renew_active() {
        let _root = TestRoot::new();

        let lease = acquire("ws-1", "agent-a", None, "command", vec![], 60)
            .await
            .expect("acquire");

        let renewed = renew(&lease.id).await.expect("renew");
        assert_eq!(renewed.status, "active");
        assert!(renewed.renewed_at >= lease.renewed_at);
        assert!(renewed.expires_at > lease.expires_at);
    }

    #[tokio::test]
    async fn renew_expired_fail() {
        let _root = TestRoot::new();

        // Acquire with TTL of 1 second, then manually expire it
        let lease = acquire("ws-1", "agent-a", None, "command", vec![], 1)
            .await
            .expect("acquire");

        // Force-expire by setting expires_at in the past
        let pool = db::pool().await.unwrap();
        sqlx::query("UPDATE work_leases SET expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1")
            .bind(&lease.id)
            .execute(&pool)
            .await
            .unwrap();

        let result = renew(&lease.id).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already expired"));
    }

    #[tokio::test]
    async fn release_lease() {
        let _root = TestRoot::new();

        let lease = acquire("ws-1", "agent-a", None, "review", vec![], 3600)
            .await
            .expect("acquire");

        release(&lease.id).await.expect("release");

        let loaded = get_lease(&lease.id).await.expect("get").expect("some");
        assert_eq!(loaded.status, "released");
        assert!(loaded.released_at.is_some());
    }

    #[tokio::test]
    async fn expire_scan_marks_expired() {
        let _root = TestRoot::new();

        let lease = acquire("ws-1", "agent-a", None, "command", vec![], 1)
            .await
            .expect("acquire");

        // Force-expire by setting expires_at in the past
        let pool = db::pool().await.unwrap();
        sqlx::query("UPDATE work_leases SET expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1")
            .bind(&lease.id)
            .execute(&pool)
            .await
            .unwrap();

        let expired = expire_scan().await.expect("expire_scan");
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].id, lease.id);
        assert_eq!(expired[0].status, "expired");

        // Verify it no longer shows in active list
        let active = list_active_leases("ws-1").await.expect("list");
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn list_active_filters() {
        let _root = TestRoot::new();

        let lease1 = acquire("ws-1", "agent-a", None, "command", vec![], 3600)
            .await
            .expect("acquire 1");

        let lease2 = acquire("ws-1", "agent-b", None, "review", vec![], 3600)
            .await
            .expect("acquire 2");

        // Release one lease
        release(&lease1.id).await.expect("release");

        let active = list_active_leases("ws-1").await.expect("list");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, lease2.id);

        // Different workspace should be empty
        let other_ws = list_active_leases("ws-2").await.expect("list other");
        assert!(other_ws.is_empty());
    }
}
