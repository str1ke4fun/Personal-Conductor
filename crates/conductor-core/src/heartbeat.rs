use crate::db;
use crate::events;
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentHeartbeat {
    pub id: String,
    pub workspace_id: String,
    pub agent_id: String,
    pub process_id: Option<i64>,
    pub task_id: Option<String>,
    pub goal_id: Option<String>,
    pub status: String, // idle|observing|planning|working|awaiting_permission|awaiting_input|reviewing|blocked|stopping
    pub stage_label: Option<String>,
    pub progress_text: Option<String>,
    pub active_tool_count: i64,
    pub last_event_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Create or update a heartbeat for the given (workspace_id, agent_id) pair.
///
/// If a heartbeat already exists, it is updated in place (preserving `created_at`).
/// Otherwise a new row is inserted. `expires_at` is always set to `now + ttl_seconds`.
pub async fn upsert_heartbeat(
    workspace_id: &str,
    agent_id: &str,
    process_id: Option<i64>,
    task_id: Option<&str>,
    goal_id: Option<&str>,
    status: &str,
    stage_label: Option<&str>,
    progress_text: Option<&str>,
    active_tool_count: i64,
    ttl_seconds: i64,
) -> anyhow::Result<AgentHeartbeat> {
    let pool = db::pool().await?;
    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(ttl_seconds);
    let now_str = now.to_rfc3339();
    let expires_str = expires_at.to_rfc3339();

    // Check for existing heartbeat
    let existing =
        sqlx::query("SELECT id FROM agent_heartbeats WHERE workspace_id = ?1 AND agent_id = ?2")
            .bind(workspace_id)
            .bind(agent_id)
            .fetch_optional(&pool)
            .await?;

    if let Some(row) = existing {
        let id: String = row.try_get("id")?;
        sqlx::query(
            r#"
            UPDATE agent_heartbeats
            SET process_id = ?1,
                task_id = ?2,
                goal_id = ?3,
                status = ?4,
                stage_label = ?5,
                progress_text = ?6,
                active_tool_count = ?7,
                expires_at = ?8
            WHERE id = ?9
            "#,
        )
        .bind(process_id)
        .bind(task_id)
        .bind(goal_id)
        .bind(status)
        .bind(stage_label)
        .bind(progress_text)
        .bind(active_tool_count)
        .bind(&expires_str)
        .bind(&id)
        .execute(&pool)
        .await
        .with_context(|| format!("update heartbeat {id}"))?;

        let row = sqlx::query(
            r#"SELECT id, workspace_id, agent_id, process_id, task_id, goal_id, status,
                      stage_label, progress_text, active_tool_count, last_event_id,
                      created_at, expires_at
               FROM agent_heartbeats WHERE id = ?1"#,
        )
        .bind(&id)
        .fetch_one(&pool)
        .await?;

        row_to_heartbeat(row)
    } else {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO agent_heartbeats (
                id, workspace_id, agent_id, process_id, task_id, goal_id,
                status, stage_label, progress_text, active_tool_count,
                created_at, expires_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
        )
        .bind(&id)
        .bind(workspace_id)
        .bind(agent_id)
        .bind(process_id)
        .bind(task_id)
        .bind(goal_id)
        .bind(status)
        .bind(stage_label)
        .bind(progress_text)
        .bind(active_tool_count)
        .bind(&now_str)
        .bind(&expires_str)
        .execute(&pool)
        .await
        .with_context(|| "insert heartbeat")?;

        Ok(AgentHeartbeat {
            id,
            workspace_id: workspace_id.to_string(),
            agent_id: agent_id.to_string(),
            process_id,
            task_id: task_id.map(String::from),
            goal_id: goal_id.map(String::from),
            status: status.to_string(),
            stage_label: stage_label.map(String::from),
            progress_text: progress_text.map(String::from),
            active_tool_count,
            last_event_id: None,
            created_at: now,
            expires_at,
        })
    }
}

/// Get a single heartbeat by (workspace_id, agent_id).
pub async fn get_heartbeat(
    workspace_id: &str,
    agent_id: &str,
) -> anyhow::Result<Option<AgentHeartbeat>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"SELECT id, workspace_id, agent_id, process_id, task_id, goal_id, status,
                  stage_label, progress_text, active_tool_count, last_event_id,
                  created_at, expires_at
           FROM agent_heartbeats
           WHERE workspace_id = ?1 AND agent_id = ?2"#,
    )
    .bind(workspace_id)
    .bind(agent_id)
    .fetch_optional(&pool)
    .await?;

    row.map(row_to_heartbeat).transpose()
}

/// List all non-expired heartbeats for a workspace (expires_at > now).
pub async fn get_active_heartbeats(workspace_id: &str) -> anyhow::Result<Vec<AgentHeartbeat>> {
    let pool = db::pool().await?;
    let now_str = Utc::now().to_rfc3339();

    let rows = sqlx::query(
        r#"SELECT id, workspace_id, agent_id, process_id, task_id, goal_id, status,
                  stage_label, progress_text, active_tool_count, last_event_id,
                  created_at, expires_at
           FROM agent_heartbeats
           WHERE workspace_id = ?1 AND expires_at > ?2
           ORDER BY agent_id ASC"#,
    )
    .bind(workspace_id)
    .bind(&now_str)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(row_to_heartbeat).collect()
}

/// Find all heartbeats that have expired (expires_at < now) AND status is not "idle".
///
/// Marks them as `status = "idle"`, emits `agent.heartbeat_expired` events, and returns them.
pub async fn scan_expired() -> anyhow::Result<Vec<AgentHeartbeat>> {
    let pool = db::pool().await?;
    let now = Utc::now();
    let now_str = now.to_rfc3339();

    let rows = sqlx::query(
        r#"SELECT id, workspace_id, agent_id, process_id, task_id, goal_id, status,
                  stage_label, progress_text, active_tool_count, last_event_id,
                  created_at, expires_at
           FROM agent_heartbeats
           WHERE expires_at < ?1 AND status != 'idle'"#,
    )
    .bind(&now_str)
    .fetch_all(&pool)
    .await?;

    let expired_ids: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("id").ok())
        .collect();

    for id in &expired_ids {
        sqlx::query("UPDATE agent_heartbeats SET status = 'idle' WHERE id = ?1")
            .bind(id)
            .execute(&pool)
            .await?;
    }

    // Emit events and collect results
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let hb = row_to_heartbeat(row)?;
        events::append(
            "agent",
            "heartbeat_expired",
            &json!({
                "heartbeat_id": hb.id,
                "workspace_id": hb.workspace_id,
                "agent_id": hb.agent_id,
                "previous_status": hb.status,
            }),
        )
        .await?;

        result.push(AgentHeartbeat {
            status: "idle".to_string(),
            ..hb
        });
    }

    Ok(result)
}

/// Delete a heartbeat by (workspace_id, agent_id).
pub async fn delete_heartbeat(workspace_id: &str, agent_id: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query("DELETE FROM agent_heartbeats WHERE workspace_id = ?1 AND agent_id = ?2")
        .bind(workspace_id)
        .bind(agent_id)
        .execute(&pool)
        .await
        .with_context(|| "delete heartbeat")?;
    Ok(())
}

fn row_to_heartbeat(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<AgentHeartbeat> {
    let created_at =
        DateTime::parse_from_rfc3339(&row.try_get::<String, _>("created_at")?)?.with_timezone(&Utc);
    let expires_at =
        DateTime::parse_from_rfc3339(&row.try_get::<String, _>("expires_at")?)?.with_timezone(&Utc);

    Ok(AgentHeartbeat {
        id: row.try_get("id")?,
        workspace_id: row.try_get("workspace_id")?,
        agent_id: row.try_get("agent_id")?,
        process_id: row.try_get("process_id")?,
        task_id: row.try_get("task_id")?,
        goal_id: row.try_get("goal_id")?,
        status: row.try_get("status")?,
        stage_label: row.try_get("stage_label")?,
        progress_text: row.try_get("progress_text")?,
        active_tool_count: row.try_get("active_tool_count")?,
        last_event_id: row.try_get("last_event_id")?,
        created_at,
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn upsert_creates_new() {
        let _root = TestRoot::new();

        let hb = upsert_heartbeat(
            "ws-1",
            "agent-alpha",
            Some(1234),
            Some("task-1"),
            Some("goal-1"),
            "working",
            Some("coding"),
            Some("writing tests"),
            2,
            300,
        )
        .await
        .expect("upsert");

        assert_eq!(hb.workspace_id, "ws-1");
        assert_eq!(hb.agent_id, "agent-alpha");
        assert_eq!(hb.process_id, Some(1234));
        assert_eq!(hb.task_id.as_deref(), Some("task-1"));
        assert_eq!(hb.goal_id.as_deref(), Some("goal-1"));
        assert_eq!(hb.status, "working");
        assert_eq!(hb.stage_label.as_deref(), Some("coding"));
        assert_eq!(hb.progress_text.as_deref(), Some("writing tests"));
        assert_eq!(hb.active_tool_count, 2);
        assert_eq!(hb.last_event_id, None);
    }

    #[tokio::test]
    async fn upsert_updates_existing() {
        let _root = TestRoot::new();

        let hb1 = upsert_heartbeat(
            "ws-1",
            "agent-beta",
            Some(100),
            None,
            None,
            "observing",
            None,
            None,
            0,
            60,
        )
        .await
        .expect("first upsert");

        let hb2 = upsert_heartbeat(
            "ws-1",
            "agent-beta",
            Some(200),
            Some("task-9"),
            None,
            "working",
            Some("analyzing"),
            Some("reading files"),
            3,
            120,
        )
        .await
        .expect("second upsert");

        // Same id (updated in place), different fields
        assert_eq!(hb2.id, hb1.id);
        assert_eq!(hb2.created_at, hb1.created_at);
        assert_eq!(hb2.process_id, Some(200));
        assert_eq!(hb2.task_id.as_deref(), Some("task-9"));
        assert_eq!(hb2.status, "working");
        assert_eq!(hb2.active_tool_count, 3);
    }

    #[tokio::test]
    async fn get_active_filters_expired() {
        let _root = TestRoot::new();

        // Insert a fresh heartbeat (active)
        upsert_heartbeat(
            "ws-2",
            "agent-active",
            None,
            None,
            None,
            "working",
            None,
            None,
            1,
            300,
        )
        .await
        .expect("active hb");

        // Insert a heartbeat and force-expire it
        upsert_heartbeat(
            "ws-2",
            "agent-stale",
            None,
            None,
            None,
            "working",
            None,
            None,
            0,
            300,
        )
        .await
        .expect("stale hb");

        let pool = db::pool().await.unwrap();
        sqlx::query(
            "UPDATE agent_heartbeats SET expires_at = '2000-01-01T00:00:00Z' WHERE agent_id = 'agent-stale'",
        )
        .execute(&pool)
        .await
        .unwrap();

        let active = get_active_heartbeats("ws-2").await.expect("get active");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].agent_id, "agent-active");
    }

    #[tokio::test]
    async fn scan_expired_finds_and_marks() {
        let _root = TestRoot::new();

        // Create a heartbeat with non-idle status, then force-expire it
        upsert_heartbeat(
            "ws-3",
            "agent-dying",
            Some(999),
            Some("task-x"),
            None,
            "working",
            None,
            None,
            1,
            300,
        )
        .await
        .expect("upsert");

        let pool = db::pool().await.unwrap();
        sqlx::query(
            "UPDATE agent_heartbeats SET expires_at = '2000-01-01T00:00:00Z' WHERE agent_id = 'agent-dying'",
        )
        .execute(&pool)
        .await
        .unwrap();

        let expired = scan_expired().await.expect("scan");
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].agent_id, "agent-dying");
        assert_eq!(expired[0].status, "idle");

        // Verify the DB row is now idle
        let hb = get_heartbeat("ws-3", "agent-dying")
            .await
            .expect("get")
            .expect("some");
        assert_eq!(hb.status, "idle");
    }

    #[tokio::test]
    async fn scan_expired_skips_already_idle() {
        let _root = TestRoot::new();

        // Create an idle heartbeat and force-expire it
        upsert_heartbeat(
            "ws-4",
            "agent-idle",
            None,
            None,
            None,
            "idle",
            None,
            None,
            0,
            300,
        )
        .await
        .expect("upsert");

        let pool = db::pool().await.unwrap();
        sqlx::query(
            "UPDATE agent_heartbeats SET expires_at = '2000-01-01T00:00:00Z' WHERE agent_id = 'agent-idle'",
        )
        .execute(&pool)
        .await
        .unwrap();

        let expired = scan_expired().await.expect("scan");
        assert!(expired.is_empty(), "idle heartbeats should be skipped");
    }

    #[tokio::test]
    async fn delete_removes_heartbeat() {
        let _root = TestRoot::new();

        upsert_heartbeat(
            "ws-5",
            "agent-deleteme",
            None,
            None,
            None,
            "idle",
            None,
            None,
            0,
            300,
        )
        .await
        .expect("upsert");

        // Confirm it exists
        let hb = get_heartbeat("ws-5", "agent-deleteme").await.expect("get");
        assert!(hb.is_some());

        // Delete
        delete_heartbeat("ws-5", "agent-deleteme")
            .await
            .expect("delete");

        // Confirm gone
        let hb = get_heartbeat("ws-5", "agent-deleteme").await.expect("get");
        assert!(hb.is_none());
    }

    #[tokio::test]
    async fn get_heartbeat_not_found() {
        let _root = TestRoot::new();

        let hb = get_heartbeat("ws-nonexistent", "agent-ghost")
            .await
            .expect("get");
        assert!(hb.is_none());
    }
}
