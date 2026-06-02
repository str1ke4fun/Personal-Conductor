use super::types::{ChatMessage, ChatRole, ContentBlock, ToolCallRecord};
use anyhow::bail;
use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

pub async fn history(limit: u32) -> anyhow::Result<Vec<ChatMessage>> {
    let pool = crate::db::pool().await?;
    let limit = i64::from(limit.clamp(1, 500));
    let rows = sqlx::query(
        r#"
        SELECT id, role, content, created_at, tool_calls, seq
        FROM (
            SELECT id, role, content, created_at, tool_calls, COALESCE(seq, rowid) AS seq
            FROM chat_messages
            WHERE session_id IS NULL
            ORDER BY COALESCE(seq, rowid) DESC, created_at DESC, id DESC
            LIMIT ?1
        )
        ORDER BY seq ASC, created_at ASC, id ASC
        "#,
    )
    .bind(limit)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(message_from_row).collect()
}

/// Load message history for a specific session.
pub async fn history_for_session(session_id: &str, limit: u32) -> anyhow::Result<Vec<ChatMessage>> {
    let pool = crate::db::pool().await?;
    let limit = i64::from(limit.clamp(1, 500));
    let rows = sqlx::query(
        r#"
        SELECT id, role, content, created_at, tool_calls, COALESCE(seq, rowid) AS seq
        FROM chat_messages
        WHERE session_id = ?1
        ORDER BY COALESCE(seq, rowid) ASC, created_at ASC, id ASC
        LIMIT ?2
        "#,
    )
    .bind(session_id)
    .bind(limit)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(message_from_row).collect()
}

pub async fn record_assistant_message(content: String) -> anyhow::Result<ChatMessage> {
    let content = content.trim().to_string();
    if content.is_empty() {
        bail!("鍥炲鍐呭涓嶈兘涓虹┖");
    }
    let pool = crate::db::pool().await?;
    insert_message(&pool, ChatRole::Assistant, content, None).await
}

pub(super) async fn insert_message(
    pool: &sqlx::SqlitePool,
    role: ChatRole,
    content: String,
    tool_calls: Option<Vec<ToolCallRecord>>,
) -> anyhow::Result<ChatMessage> {
    insert_message_with_session(pool, role, content, tool_calls, None).await
}

pub(super) async fn insert_message_with_session(
    pool: &sqlx::SqlitePool,
    role: ChatRole,
    content: String,
    tool_calls: Option<Vec<ToolCallRecord>>,
    session_id: Option<&str>,
) -> anyhow::Result<ChatMessage> {
    let tool_calls_json = tool_calls
        .as_ref()
        .and_then(|tc| serde_json::to_string(tc).ok());
    let seq: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) + 1 FROM chat_messages")
        .fetch_one(pool)
        .await?;
    let message = ChatMessage {
        id: Uuid::new_v4().to_string(),
        role,
        content,
        created_at: Utc::now(),
        seq,
        tool_calls,
    };
    sqlx::query(
        r#"
        INSERT INTO chat_messages (id, role, content, created_at, seq, tool_calls, session_id)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(&message.id)
    .bind(message.role.as_str())
    .bind(&message.content)
    .bind(message.created_at.to_rfc3339())
    .bind(message.seq)
    .bind(&tool_calls_json)
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(message)
}

pub async fn store_timeout_reply_with_text(
    session_id: Option<&str>,
    text: &str,
) -> anyhow::Result<ChatMessage> {
    let text = text.trim();
    if text.is_empty() {
        bail!("timeout reply text cannot be empty");
    }
    let blocks = vec![ContentBlock::Text {
        text: text.to_string(),
    }];
    let content = serde_json::to_string(&blocks)?;
    let pool = crate::db::pool().await?;
    insert_message_with_session(&pool, ChatRole::Assistant, content, None, session_id).await
}

pub(super) fn message_from_row(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<ChatMessage> {
    let created_at =
        DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc);
    let tool_calls: Option<Vec<ToolCallRecord>> = row
        .try_get::<Option<String>, _>("tool_calls")?
        .and_then(|json| serde_json::from_str(&json).ok());
    Ok(ChatMessage {
        id: row.try_get("id")?,
        role: ChatRole::from_db(row.try_get::<String, _>("role")?.as_str())?,
        content: row.try_get("content")?,
        created_at,
        seq: row.try_get("seq")?,
        tool_calls,
    })
}
