use crate::db;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentMessage {
    pub id: String,
    pub workspace_id: String,
    pub goal_id: Option<String>,
    pub cycle_id: Option<String>,
    pub task_id: Option<String>,
    pub sender_id: String,
    pub recipient_id: Option<String>,
    pub topic: String,
    pub kind: String,
    pub content: String,
    pub payload_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

/// Create and insert a new agent message.
pub async fn post_message(
    workspace_id: &str,
    goal_id: Option<&str>,
    cycle_id: Option<&str>,
    task_id: Option<&str>,
    sender_id: &str,
    recipient_id: Option<&str>,
    topic: &str,
    kind: &str,
    content: &str,
    payload_json: Option<serde_json::Value>,
) -> Result<AgentMessage> {
    let pool = db::pool().await?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let now_str = now.to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO agent_messages (
            id, workspace_id, goal_id, cycle_id, task_id,
            sender_id, recipient_id, topic, kind, content,
            payload_json, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
    )
    .bind(&id)
    .bind(workspace_id)
    .bind(goal_id)
    .bind(cycle_id)
    .bind(task_id)
    .bind(sender_id)
    .bind(recipient_id)
    .bind(topic)
    .bind(kind)
    .bind(content)
    .bind(
        payload_json
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
    )
    .bind(&now_str)
    .execute(&pool)
    .await
    .with_context(|| "insert agent_message")?;

    Ok(AgentMessage {
        id,
        workspace_id: workspace_id.to_string(),
        goal_id: goal_id.map(String::from),
        cycle_id: cycle_id.map(String::from),
        task_id: task_id.map(String::from),
        sender_id: sender_id.to_string(),
        recipient_id: recipient_id.map(String::from),
        topic: topic.to_string(),
        kind: kind.to_string(),
        content: content.to_string(),
        payload_json,
        created_at: now,
        read_at: None,
    })
}

/// Retrieve messages for a workspace with optional topic and time filters.
pub async fn get_messages(
    workspace_id: &str,
    topic: Option<&str>,
    since: Option<&str>,
    limit: Option<u32>,
) -> Result<Vec<AgentMessage>> {
    let pool = db::pool().await?;
    let effective_limit = limit.unwrap_or(50).clamp(1, 500) as i64;

    let rows = sqlx::query(
        "SELECT id, workspace_id, goal_id, cycle_id, task_id, \
         sender_id, recipient_id, topic, kind, content, \
         payload_json, created_at, read_at \
         FROM agent_messages WHERE workspace_id = ?1 \
         ORDER BY created_at DESC LIMIT ?2",
    )
    .bind(workspace_id)
    .bind(effective_limit)
    .fetch_all(&pool)
    .await?;

    let mut messages: Vec<AgentMessage> = rows
        .into_iter()
        .map(row_to_agent_message)
        .collect::<Result<Vec<_>>>()?;

    if let Some(t) = topic {
        messages.retain(|m| m.topic == t);
    }
    if let Some(since_val) = since {
        let since_dt = DateTime::parse_from_rfc3339(since_val)
            .with_context(|| format!("invalid since timestamp: {since_val}"))?
            .with_timezone(&Utc);
        messages.retain(|m| m.created_at > since_dt);
    }

    Ok(messages)
}

/// Get a single message by id.
pub async fn get_message(message_id: &str) -> Result<Option<AgentMessage>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        "SELECT id, workspace_id, goal_id, cycle_id, task_id, \
         sender_id, recipient_id, topic, kind, content, \
         payload_json, created_at, read_at \
         FROM agent_messages WHERE id = ?1",
    )
    .bind(message_id)
    .fetch_optional(&pool)
    .await?;

    match row {
        Some(r) => Ok(Some(row_to_agent_message(r)?)),
        None => Ok(None),
    }
}

/// Mark a message as read by setting read_at to now.
pub async fn mark_read(message_id: &str) -> Result<()> {
    let pool = db::pool().await?;
    let now_str = Utc::now().to_rfc3339();

    let result = sqlx::query("UPDATE agent_messages SET read_at = ?1 WHERE id = ?2")
        .bind(&now_str)
        .bind(message_id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        anyhow::bail!("message not found: {message_id}");
    }

    Ok(())
}

/// Count unread messages for a specific recipient in a workspace.
pub async fn unread_count(workspace_id: &str, recipient_id: &str) -> Result<i64> {
    let pool = db::pool().await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_messages \
         WHERE workspace_id = ?1 AND recipient_id = ?2 AND read_at IS NULL",
    )
    .bind(workspace_id)
    .bind(recipient_id)
    .fetch_one(&pool)
    .await?;

    Ok(count)
}

fn row_to_agent_message(row: sqlx::sqlite::SqliteRow) -> Result<AgentMessage> {
    let payload_json: Option<serde_json::Value> = row
        .try_get::<Option<String>, _>("payload_json")?
        .map(|s| serde_json::from_str(&s))
        .transpose()?;

    let created_at =
        DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc);

    let read_at = row
        .try_get::<Option<String>, _>("read_at")?
        .map(|s| DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)))
        .transpose()?;

    Ok(AgentMessage {
        id: row.try_get("id")?,
        workspace_id: row.try_get("workspace_id")?,
        goal_id: row.try_get("goal_id")?,
        cycle_id: row.try_get("cycle_id")?,
        task_id: row.try_get("task_id")?,
        sender_id: row.try_get("sender_id")?,
        recipient_id: row.try_get("recipient_id")?,
        topic: row.try_get("topic")?,
        kind: row.try_get("kind")?,
        content: row.try_get("content")?,
        payload_json,
        created_at,
        read_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn post_and_get() {
        let _root = TestRoot::new();

        let msg = post_message(
            "ws-1",
            Some("goal-1"),
            Some("cycle-1"),
            Some("task-1"),
            "agent-alpha",
            Some("agent-beta"),
            "status-update",
            "message",
            "Hello from alpha",
            Some(serde_json::json!({"priority": "high"})),
        )
        .await
        .expect("post_message");

        assert!(!msg.id.is_empty());
        assert_eq!(msg.workspace_id, "ws-1");
        assert_eq!(msg.sender_id, "agent-alpha");
        assert_eq!(msg.topic, "status-update");
        assert!(msg.read_at.is_none());

        let loaded = get_message(&msg.id)
            .await
            .expect("get_message")
            .expect("message should exist");
        assert_eq!(loaded.content, "Hello from alpha");
        assert_eq!(loaded.goal_id.as_deref(), Some("goal-1"));
        assert_eq!(loaded.cycle_id.as_deref(), Some("cycle-1"));
        assert_eq!(loaded.task_id.as_deref(), Some("task-1"));
        assert_eq!(loaded.recipient_id.as_deref(), Some("agent-beta"));
        assert!(loaded.payload_json.is_some());
    }

    #[tokio::test]
    async fn filter_by_topic() {
        let _root = TestRoot::new();

        post_message(
            "ws-2",
            None,
            None,
            None,
            "a1",
            None,
            "logs",
            "message",
            "log entry",
            None,
        )
        .await
        .expect("post log");
        post_message(
            "ws-2", None, None, None, "a1", None, "alerts", "message", "alert!", None,
        )
        .await
        .expect("post alert");
        post_message(
            "ws-2",
            None,
            None,
            None,
            "a2",
            None,
            "logs",
            "message",
            "another log",
            None,
        )
        .await
        .expect("post log 2");

        let logs = get_messages("ws-2", Some("logs"), None, None)
            .await
            .expect("get logs");
        assert_eq!(logs.len(), 2);
        assert!(logs.iter().all(|m| m.topic == "logs"));

        let alerts = get_messages("ws-2", Some("alerts"), None, None)
            .await
            .expect("get alerts");
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].content, "alert!");
    }

    #[tokio::test]
    async fn mark_read_sets_timestamp() {
        let _root = TestRoot::new();

        let msg = post_message(
            "ws-3",
            None,
            None,
            None,
            "sender",
            Some("recipient"),
            "inbox",
            "message",
            "unread msg",
            None,
        )
        .await
        .expect("post");

        let before = get_message(&msg.id).await.expect("get").expect("exists");
        assert!(before.read_at.is_none());

        mark_read(&msg.id).await.expect("mark_read");

        let after = get_message(&msg.id)
            .await
            .expect("get after")
            .expect("exists");
        assert!(after.read_at.is_some());
    }

    #[tokio::test]
    async fn unread_count_counts_unread_for_recipient() {
        let _root = TestRoot::new();

        // Three messages for "worker", one is broadcast (no recipient), one is for "other"
        post_message(
            "ws-4",
            None,
            None,
            None,
            "boss",
            Some("worker"),
            "tasks",
            "message",
            "task 1",
            None,
        )
        .await
        .expect("post 1");
        post_message(
            "ws-4",
            None,
            None,
            None,
            "boss",
            Some("worker"),
            "tasks",
            "message",
            "task 2",
            None,
        )
        .await
        .expect("post 2");
        post_message(
            "ws-4",
            None,
            None,
            None,
            "boss",
            Some("worker"),
            "tasks",
            "message",
            "task 3",
            None,
        )
        .await
        .expect("post 3");
        post_message(
            "ws-4",
            None,
            None,
            None,
            "boss",
            None,
            "broadcast",
            "message",
            "broadcast",
            None,
        )
        .await
        .expect("post broadcast");
        post_message(
            "ws-4",
            None,
            None,
            None,
            "boss",
            Some("other"),
            "tasks",
            "message",
            "other task",
            None,
        )
        .await
        .expect("post other");

        // All 3 directed at "worker" should be unread
        let count = unread_count("ws-4", "worker").await.expect("unread_count");
        assert_eq!(count, 3);

        // Mark one as read
        let msgs = get_messages("ws-4", Some("tasks"), None, None)
            .await
            .expect("get");
        let worker_msg = msgs
            .iter()
            .find(|m| m.recipient_id.as_deref() == Some("worker"))
            .unwrap();
        mark_read(&worker_msg.id).await.expect("mark_read");

        let count_after = unread_count("ws-4", "worker")
            .await
            .expect("unread_count after");
        assert_eq!(count_after, 2);

        // "other" should have 1 unread
        let other_count = unread_count("ws-4", "other").await.expect("unread other");
        assert_eq!(other_count, 1);
    }
}
