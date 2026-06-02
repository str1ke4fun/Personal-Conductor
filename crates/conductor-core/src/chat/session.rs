use super::db;
use super::types::{ChatMessage, ToolCallRecord};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[cfg(feature = "tauri-events")]
pub(super) async fn resolve_session_workspace(
    session_id: Option<&str>,
) -> anyhow::Result<Option<crate::workspaces::Workspace>> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };
    let pool = crate::db::pool().await?;
    let workspace_id: Option<String> =
        sqlx::query_scalar("SELECT workspace_id FROM chat_sessions WHERE id = ?1")
            .bind(session_id)
            .fetch_optional(&pool)
            .await?
            .flatten();
    let Some(workspace_id) = workspace_id else {
        return Ok(None);
    };
    Ok(Some(crate::workspaces::get(&workspace_id).await?))
}

/// A bounded chat session that tracks workspace and run context.
///
/// Each session accumulates messages and tool records, enabling
/// conversation continuity and future interrupt/resume support.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatSession {
    pub id: String,
    pub workspace_id: Option<String>,
    pub run_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub messages: Vec<ChatMessage>,
    pub tool_records: Vec<ToolCallRecord>,
}

impl ChatSession {
    /// Create a new session, optionally bound to a workspace or run.
    pub fn new(workspace_id: Option<String>, run_id: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            workspace_id,
            run_id,
            created_at: Utc::now(),
            messages: Vec::new(),
            tool_records: Vec::new(),
        }
    }

    /// Send a user message through the session and get a reply.
    ///
    /// Delegates to the existing `send()` function for LLM logic and
    /// records the resulting messages and tool calls in the session state.
    pub async fn send_message(
        &mut self,
        content: String,
    ) -> anyhow::Result<super::types::ChatReply> {
        let reply = super::handler::send(content).await?;

        // Track the user message (deduce from history — the user message is
        // always the second-to-last entry after send() returns).
        if reply.history.len() >= 2 {
            if let Some(user_msg) = reply.history.get(reply.history.len() - 2) {
                if !self.messages.iter().any(|m| m.id == user_msg.id) {
                    self.messages.push(user_msg.clone());
                }
            }
        }

        // Track the assistant reply
        if !self.messages.iter().any(|m| m.id == reply.message.id) {
            self.messages.push(reply.message.clone());
        }

        // Track tool calls from the reply
        if let Some(ref calls) = reply.message.tool_calls {
            self.tool_records.extend(calls.clone());
        }

        Ok(reply)
    }

    /// Return a summary of the session for logging / display.
    pub fn summary(&self) -> String {
        format!(
            "ChatSession(id={}, workspace={:?}, run={:?}, messages={}, tools={})",
            self.id,
            self.workspace_id,
            self.run_id,
            self.messages.len(),
            self.tool_records.len(),
        )
    }
}

/// Summary of a chat session for the sidebar listing.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatSessionSummary {
    pub id: String,
    pub title: String,
    pub workspace_id: Option<String>,
    /// Session kind: "chat" (default, short-task conversation) or "goal"
    /// (long-task / autonomous Goal session).
    pub session_kind: String,
    /// Associated goal ID when session_kind = "goal".
    pub goal_id: Option<String>,
    pub message_count: i64,
    pub last_message_preview: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Whether this session currently has an active LLM run.
    pub working: bool,
    /// Timestamp when the current run started (if working).
    pub working_since: Option<DateTime<Utc>>,
    /// Elapsed milliseconds since the run started (if working).
    pub working_elapsed_ms: Option<u64>,
    /// Current processing phase (e.g. "tool_calling", "planning").
    pub working_stage: Option<String>,
    /// Number of currently executing tools.
    pub active_tool_count: Option<u32>,
    /// Total number of tool runs in this turn.
    pub tool_run_count: Option<u32>,
}

/// Create a new chat session.
pub async fn create_chat_session(
    title: Option<String>,
    workspace_id: Option<String>,
) -> anyhow::Result<ChatSessionSummary> {
    let pool = crate::db::pool().await?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let title = title.unwrap_or_else(|| format!("会话 {}", now.format("%m-%d %H:%M")));

    sqlx::query(
        "INSERT INTO chat_sessions (id, workspace_id, run_id, created_at, title, updated_at) VALUES (?1, ?2, NULL, ?3, ?4, ?3)"
    )
    .bind(&id)
    .bind(&workspace_id)
    .bind(now.to_rfc3339())
    .bind(&title)
    .execute(&pool)
    .await?;

    Ok(ChatSessionSummary {
        id,
        title,
        workspace_id,
        session_kind: "chat".to_string(),
        goal_id: None,
        message_count: 0,
        last_message_preview: None,
        created_at: now,
        updated_at: now,
        working: false,
        working_since: None,
        working_elapsed_ms: None,
        working_stage: None,
        active_tool_count: None,
        tool_run_count: None,
    })
}

pub async fn ensure_chat_session(
    title: &str,
    workspace_id: Option<String>,
) -> anyhow::Result<ChatSessionSummary> {
    let sessions = list_chat_sessions(Some(200)).await?;
    if let Some(existing) = sessions.into_iter().find(|session| session.title == title) {
        return Ok(existing);
    }
    create_chat_session(Some(title.to_string()), workspace_id).await
}

/// List all chat sessions with message counts and previews.
///
/// Merges live `ActiveChatRun` state so the frontend can show which
/// sessions are currently working.
pub async fn list_chat_sessions(limit: Option<u32>) -> anyhow::Result<Vec<ChatSessionSummary>> {
    let pool = crate::db::pool().await?;
    let limit = i64::from(limit.unwrap_or(50).clamp(1, 200));

    // Build a lookup of currently active runs grouped by session_id.
    let active_runs: std::collections::HashMap<String, Vec<super::active_run::ActiveChatRun>> =
        super::active_run::list_active_runs().into_iter().fold(
            std::collections::HashMap::new(),
            |mut grouped, run| {
                grouped.entry(run.session_id.clone()).or_default().push(run);
                grouped
            },
        );
    let now = Utc::now();

    let rows = sqlx::query(
        r#"
        SELECT
            s.id,
            s.title,
            s.workspace_id,
            s.session_kind,
            s.goal_id,
            s.created_at,
            COUNT(m.id) as msg_count,
            MAX(m.created_at) as last_msg_at,
            (SELECT content FROM chat_messages WHERE session_id = s.id ORDER BY created_at DESC LIMIT 1) as last_content
        FROM chat_sessions s
        LEFT JOIN chat_messages m ON m.session_id = s.id
        WHERE COALESCE(s.archived, 0) = 0
          AND COALESCE(s.title, '') NOT LIKE 'goal-task-exec:%'
        GROUP BY s.id
        ORDER BY COALESCE(MAX(m.created_at), s.created_at) DESC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(&pool)
    .await?;

    let sessions = rows
        .into_iter()
        .map(|row| {
            let id: String = row.try_get("id").unwrap_or_default();
            let created_at_str: String = row.try_get("created_at").unwrap_or_default();
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let last_msg_at: Option<String> = row.try_get("last_msg_at").ok().flatten();
            let updated_at = last_msg_at
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or(created_at);
            let last_content: Option<String> = row.try_get("last_content").ok().flatten();
            let preview = last_content.and_then(|c| {
                let text = if c.trim().starts_with('[') {
                    serde_json::from_str::<Vec<serde_json::Value>>(&c)
                        .ok()
                        .and_then(|blocks| {
                            blocks.iter().find_map(|b| {
                                if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                                    b.get("text")
                                        .and_then(|t| t.as_str())
                                        .map(|s| s.to_string())
                                } else {
                                    None
                                }
                            })
                        })
                        .unwrap_or_default()
                } else {
                    c
                };
                if text.chars().count() > 60 {
                    Some(text.chars().take(60).collect::<String>() + "…")
                } else if text.is_empty() {
                    None
                } else {
                    Some(text)
                }
            });

            let title: String = row
                .try_get::<String, _>("title")
                .ok()
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| format!("会话 {}", &id[..8.min(id.len())]));

            // Merge active run state if this session is currently working.
            let active = active_runs.get(&id);
            let (
                working,
                working_since,
                working_elapsed_ms,
                working_stage,
                active_tool_count,
                tool_run_count,
            ) = if let Some(runs) = active {
                let earliest_started = runs.iter().map(|run| run.started_at).min().unwrap_or(now);
                let latest_run = runs
                    .iter()
                    .max_by_key(|run| run.started_at)
                    .cloned()
                    .unwrap_or_else(|| runs[0].clone());
                let elapsed = (now - earliest_started).num_milliseconds().max(0) as u64;
                (
                    true,
                    Some(earliest_started),
                    Some(elapsed),
                    latest_run.phase.clone(),
                    Some(runs.iter().map(|run| run.active_tool_count).sum()),
                    Some(runs.iter().map(|run| run.tool_run_count).sum()),
                )
            } else {
                (false, None, None, None, None, None)
            };

            ChatSessionSummary {
                id: id.clone(),
                title,
                workspace_id: row.try_get("workspace_id").ok().flatten(),
                session_kind: row
                    .try_get::<Option<String>, _>("session_kind")
                    .ok()
                    .flatten()
                    .filter(|k| !k.is_empty())
                    .unwrap_or_else(|| "chat".to_string()),
                goal_id: row.try_get::<Option<String>, _>("goal_id").ok().flatten(),
                message_count: row.try_get::<i64, _>("msg_count").unwrap_or(0),
                last_message_preview: preview,
                created_at,
                updated_at,
                working,
                working_since,
                working_elapsed_ms,
                working_stage,
                active_tool_count,
                tool_run_count,
            }
        })
        .collect();

    Ok(sessions)
}

/// Find the chat session ID linked to a goal (if any).
pub async fn find_session_for_goal(goal_id: &str) -> Option<String> {
    let pool = crate::db::pool().await.ok()?;
    sqlx::query_scalar::<_, String>("SELECT id FROM chat_sessions WHERE goal_id = ?1 LIMIT 1")
        .bind(goal_id)
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten()
}

/// Append an assistant message to a session using the normal sequencer.
pub async fn append_assistant_message_to_session(
    session_id: &str,
    content: &str,
) -> anyhow::Result<ChatMessage> {
    let pool = crate::db::pool().await?;
    db::insert_message_with_session(
        &pool,
        super::types::ChatRole::Assistant,
        content.to_string(),
        None,
        Some(session_id),
    )
    .await
}

pub async fn update_message_content(message_id: &str, content: &str) -> anyhow::Result<()> {
    let pool = crate::db::pool().await?;
    let updated_at = Utc::now().to_rfc3339();
    let result =
        sqlx::query("UPDATE chat_messages SET content = ?1, created_at = ?2 WHERE id = ?3")
            .bind(content)
            .bind(updated_at)
            .bind(message_id)
            .execute(&pool)
            .await?;
    if result.rows_affected() == 0 {
        anyhow::bail!("chat message not found: {message_id}");
    }
    Ok(())
}

/// Append a user message to a session using the normal sequencer.
///
/// Used by the goal-first-send path: when the user's first message in a
/// goal session becomes the goal objective, we still want that message to
/// appear in the conversation timeline — but without triggering a
/// foreground LLM turn (the orchestrator drives execution separately).
pub async fn append_user_message_to_session(
    session_id: &str,
    content: &str,
) -> anyhow::Result<ChatMessage> {
    let pool = crate::db::pool().await?;
    db::insert_message_with_session(
        &pool,
        super::types::ChatRole::User,
        content.to_string(),
        None,
        Some(session_id),
    )
    .await
}

/// Fetch a single chat session by ID (returns None if not found).
pub async fn get_chat_session(session_id: &str) -> anyhow::Result<Option<ChatSessionSummary>> {
    let pool = crate::db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT s.id, s.title, s.workspace_id, s.session_kind, s.goal_id, s.created_at,
               COUNT(m.id) as msg_count,
               MAX(m.created_at) as last_msg_at,
               NULL as last_content
        FROM chat_sessions s
        LEFT JOIN chat_messages m ON m.session_id = s.id
        WHERE s.id = ?1
        GROUP BY s.id
        "#,
    )
    .bind(session_id)
    .fetch_optional(&pool)
    .await?;

    let Some(row) = row else { return Ok(None) };

    let id: String = row.try_get("id").unwrap_or_default();
    let created_at_str: String = row.try_get("created_at").unwrap_or_default();
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(Some(ChatSessionSummary {
        id,
        title: row.try_get("title").unwrap_or_default(),
        workspace_id: row.try_get("workspace_id").ok().flatten(),
        session_kind: row
            .try_get::<Option<String>, _>("session_kind")
            .ok()
            .flatten()
            .unwrap_or_else(|| "chat".to_string()),
        goal_id: row.try_get::<Option<String>, _>("goal_id").ok().flatten(),
        message_count: row.try_get::<i64, _>("msg_count").unwrap_or(0),
        last_message_preview: None,
        created_at,
        updated_at: created_at,
        working: false,
        working_since: None,
        working_elapsed_ms: None,
        working_stage: None,
        active_tool_count: None,
        tool_run_count: None,
    }))
}

/// Get messages for a specific chat session.
pub async fn get_chat_session_messages(
    session_id: &str,
    limit: Option<u32>,
) -> anyhow::Result<Vec<ChatMessage>> {
    let pool = crate::db::pool().await?;
    let limit = i64::from(limit.unwrap_or(200).clamp(1, 500));

    let rows = sqlx::query(
        r#"
        SELECT id, role, content, created_at, tool_calls, seq
        FROM chat_messages
        WHERE session_id = ?1
        ORDER BY seq ASC, created_at ASC, id ASC
        LIMIT ?2
        "#,
    )
    .bind(session_id)
    .bind(limit)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(db::message_from_row).collect()
}

/// Rename a chat session.
pub async fn rename_chat_session(session_id: &str, title: &str) -> anyhow::Result<()> {
    let pool = crate::db::pool().await?;
    // Add title column if it doesn't exist
    let _ = sqlx::query("ALTER TABLE chat_sessions ADD COLUMN title TEXT")
        .execute(&pool)
        .await;

    sqlx::query("UPDATE chat_sessions SET title = ?1 WHERE id = ?2")
        .bind(title)
        .bind(session_id)
        .execute(&pool)
        .await?;

    Ok(())
}

pub async fn update_chat_session_workspace(
    session_id: &str,
    workspace_id: Option<&str>,
) -> anyhow::Result<()> {
    let pool = crate::db::pool().await?;
    sqlx::query("UPDATE chat_sessions SET workspace_id = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(workspace_id)
        .bind(Utc::now().to_rfc3339())
        .bind(session_id)
        .execute(&pool)
        .await?;
    Ok(())
}

/// Update a session's kind ("chat" or "goal") and optionally link it to a goal.
pub async fn set_chat_session_kind(
    session_id: &str,
    kind: &str,
    goal_id: Option<&str>,
) -> anyhow::Result<()> {
    let normalized = if kind == "goal" { "goal" } else { "chat" };
    let pool = crate::db::pool().await?;
    sqlx::query(
        "UPDATE chat_sessions SET session_kind = ?1, goal_id = ?2, updated_at = ?3 WHERE id = ?4",
    )
    .bind(normalized)
    .bind(goal_id)
    .bind(Utc::now().to_rfc3339())
    .bind(session_id)
    .execute(&pool)
    .await?;
    Ok(())
}

/// Archive a chat session (soft delete).
pub async fn archive_chat_session(session_id: &str) -> anyhow::Result<()> {
    let pool = crate::db::pool().await?;
    // Add archived column if it doesn't exist
    let _ = sqlx::query("ALTER TABLE chat_sessions ADD COLUMN archived INTEGER DEFAULT 0")
        .execute(&pool)
        .await;

    sqlx::query("UPDATE chat_sessions SET archived = 1 WHERE id = ?1")
        .bind(session_id)
        .execute(&pool)
        .await?;

    Ok(())
}

/// Auto-title a session based on the first user message.
/// Only updates if the title still looks like the default "会话 ..." format.
#[cfg(feature = "tauri-events")]
pub(super) async fn auto_title_session(session_id: &str, first_message: &str) {
    let pool = match crate::db::pool().await {
        Ok(p) => p,
        Err(_) => return,
    };
    // Check current title
    let current_title: Option<String> =
        sqlx::query_scalar("SELECT title FROM chat_sessions WHERE id = ?1")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .ok()
            .flatten();

    // Only auto-title if title is None or starts with "会话 "
    let should_update = match &current_title {
        None => true,
        Some(t) => t.starts_with("会话 ") || t.is_empty(),
    };
    if !should_update {
        return;
    }

    // Generate title from first message (max 30 chars)
    let title: String = first_message
        .chars()
        .take(30)
        .collect::<String>()
        .trim()
        .to_string();
    if title.is_empty() {
        return;
    }
    let title = if first_message.chars().count() > 30 {
        format!("{}…", title)
    } else {
        title
    };

    let _ = sqlx::query("UPDATE chat_sessions SET title = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(&title)
        .bind(Utc::now().to_rfc3339())
        .bind(session_id)
        .execute(&pool)
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{chat::db, chat::types::ChatRole, test_support::TestRoot};

    #[tokio::test]
    async fn list_chat_sessions_hides_internal_goal_exec_sessions() {
        let _root = TestRoot::new();

        let visible = create_chat_session(Some("Visible".to_string()), None)
            .await
            .expect("create visible session");
        let hidden = create_chat_session(Some("goal-task-exec:test".to_string()), None)
            .await
            .expect("create internal session");

        let sessions = list_chat_sessions(Some(20)).await.expect("list sessions");

        assert!(sessions.iter().any(|session| session.id == visible.id));
        assert!(
            sessions.iter().all(|session| session.id != hidden.id),
            "internal goal execution sessions must stay out of the user sidebar",
        );
    }

    #[tokio::test]
    async fn append_assistant_message_to_session_keeps_monotonic_seq() {
        let _root = TestRoot::new();
        let session = create_chat_session(Some("Projection".to_string()), None)
            .await
            .expect("create session");
        let pool = crate::db::pool().await.expect("db pool");

        db::insert_message_with_session(
            &pool,
            ChatRole::User,
            "hello".to_string(),
            None,
            Some(&session.id),
        )
        .await
        .expect("insert user message");

        let assistant = append_assistant_message_to_session(&session.id, "projected")
            .await
            .expect("append projected assistant");

        let history = get_chat_session_messages(&session.id, Some(10))
            .await
            .expect("load history");

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, ChatRole::User);
        assert_eq!(history[1].id, assistant.id);
        assert_eq!(history[1].seq, history[0].seq + 1);
    }
}
