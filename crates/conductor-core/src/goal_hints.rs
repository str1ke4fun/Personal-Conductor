// goal_hints — lightweight per-goal user hints that steer the OODA loop.
//
// A hint is a short text note (kind: "user" | "system" | "review") attached to
// a goal (and optionally a cycle). The observe phase surfaces the most recent
// active hints so orient/decide can act on them.

use crate::db;
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── GoalHint ─────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GoalHint {
    pub id: String,
    pub goal_id: String,
    pub cycle_id: Option<String>,
    /// "user" | "system" | "review"
    pub kind: String,
    pub content: String,
    /// "active" | "dismissed" | "expired"
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

// ── CRUD ─────────────────────────────────────────────────────────────────────

/// Create and persist a new hint for `goal_id`.
pub async fn create_hint(
    goal_id: &str,
    cycle_id: Option<&str>,
    kind: &str,
    content: &str,
    expires_at: Option<DateTime<Utc>>,
) -> anyhow::Result<GoalHint> {
    let now = Utc::now();
    let id = format!("hint-{}", Uuid::new_v4());
    let pool = db::pool().await?;

    sqlx::query(
        r#"INSERT INTO goal_hints
          (id, goal_id, cycle_id, kind, content, status, created_at, updated_at, expires_at)
          VALUES (?, ?, ?, ?, ?, 'active', ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(goal_id)
    .bind(cycle_id)
    .bind(kind)
    .bind(content)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(expires_at.map(|dt| dt.to_rfc3339()))
    .execute(&pool)
    .await
    .with_context(|| "insert goal_hint")?;

    Ok(GoalHint {
        id,
        goal_id: goal_id.to_string(),
        cycle_id: cycle_id.map(str::to_string),
        kind: kind.to_string(),
        content: content.to_string(),
        status: "active".to_string(),
        created_at: now,
        updated_at: now,
        expires_at,
    })
}

/// Fetch a single hint by id.
pub async fn get_hint(hint_id: &str) -> anyhow::Result<Option<GoalHint>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"SELECT id, goal_id, cycle_id, kind, content, status,
                  created_at, updated_at, expires_at
           FROM goal_hints WHERE id = ?"#,
    )
    .bind(hint_id)
    .fetch_optional(&pool)
    .await
    .with_context(|| "fetch goal_hint")?;

    match row {
        Some(row) => Ok(Some(row_to_hint(&row)?)),
        None => Ok(None),
    }
}

/// List active hints for a goal, most-recent first. Respects `expires_at`.
pub async fn list_active_hints(goal_id: &str, limit: Option<i64>) -> anyhow::Result<Vec<GoalHint>> {
    let pool = db::pool().await?;
    let cap = limit.unwrap_or(20);
    let now = Utc::now().to_rfc3339();

    let rows = sqlx::query(
        r#"SELECT id, goal_id, cycle_id, kind, content, status,
                  created_at, updated_at, expires_at
           FROM goal_hints
           WHERE goal_id = ?
             AND status = 'active'
             AND (expires_at IS NULL OR expires_at > ?)
           ORDER BY created_at DESC
           LIMIT ?"#,
    )
    .bind(goal_id)
    .bind(&now)
    .bind(cap)
    .fetch_all(&pool)
    .await
    .with_context(|| "list active goal_hints")?;

    rows.iter().map(row_to_hint).collect()
}

/// List all hints for a goal (any status), most-recent first.
pub async fn list_hints_by_goal(
    goal_id: &str,
    limit: Option<i64>,
) -> anyhow::Result<Vec<GoalHint>> {
    let pool = db::pool().await?;
    let cap = limit.unwrap_or(50);

    let rows = sqlx::query(
        r#"SELECT id, goal_id, cycle_id, kind, content, status,
                  created_at, updated_at, expires_at
           FROM goal_hints
           WHERE goal_id = ?
           ORDER BY created_at DESC
           LIMIT ?"#,
    )
    .bind(goal_id)
    .bind(cap)
    .fetch_all(&pool)
    .await
    .with_context(|| "list goal_hints by goal")?;

    rows.iter().map(row_to_hint).collect()
}

/// Dismiss a hint (status → "dismissed"). Idempotent.
pub async fn dismiss_hint(hint_id: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let now = Utc::now();
    sqlx::query("UPDATE goal_hints SET status = 'dismissed', updated_at = ? WHERE id = ?")
        .bind(now.to_rfc3339())
        .bind(hint_id)
        .execute(&pool)
        .await
        .with_context(|| "dismiss goal_hint")?;
    Ok(())
}

// ── Row mapping ───────────────────────────────────────────────────────────────

fn row_to_hint(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<GoalHint> {
    use sqlx::Row;

    let created_at = parse_utc(&row.try_get::<String, _>("created_at")?)?;
    let updated_at = parse_utc(&row.try_get::<String, _>("updated_at")?)?;
    let expires_at: Option<String> = row.try_get("expires_at")?;
    let expires_at = expires_at.as_deref().map(parse_utc).transpose()?;

    Ok(GoalHint {
        id: row.try_get("id")?,
        goal_id: row.try_get("goal_id")?,
        cycle_id: row.try_get("cycle_id")?,
        kind: row.try_get("kind")?,
        content: row.try_get("content")?,
        status: row.try_get("status")?,
        created_at,
        updated_at,
        expires_at,
    })
}

fn parse_utc(value: &str) -> anyhow::Result<DateTime<Utc>> {
    use anyhow::Context;
    Ok(chrono::DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("parse RFC3339 datetime: {value}"))?
        .with_timezone(&Utc))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn create_and_get_hint() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-hints",
            "Hint Test",
            "test hints",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");

        let hint = create_hint(&goal.id, None, "user", "please focus on auth first", None)
            .await
            .expect("create hint");

        assert!(!hint.id.is_empty());
        assert_eq!(hint.goal_id, goal.id);
        assert_eq!(hint.kind, "user");
        assert_eq!(hint.status, "active");
        assert_eq!(hint.content, "please focus on auth first");

        let fetched = get_hint(&hint.id).await.expect("get").expect("some");
        assert_eq!(fetched.id, hint.id);
        assert_eq!(fetched.content, hint.content);
    }

    #[tokio::test]
    async fn list_active_hints_excludes_dismissed() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-hints2",
            "Hint Test 2",
            "test hint filtering",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");

        let h1 = create_hint(&goal.id, None, "user", "hint one", None)
            .await
            .expect("h1");
        let _h2 = create_hint(&goal.id, None, "system", "hint two", None)
            .await
            .expect("h2");

        dismiss_hint(&h1.id).await.expect("dismiss");

        let active = list_active_hints(&goal.id, None)
            .await
            .expect("list active");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].content, "hint two");
    }

    #[tokio::test]
    async fn list_hints_by_goal_returns_all() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-hints3",
            "Hint Test 3",
            "all hints",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");

        for i in 0..3 {
            create_hint(&goal.id, None, "user", &format!("hint {i}"), None)
                .await
                .expect("create");
        }

        let all = list_hints_by_goal(&goal.id, None).await.expect("list all");
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn dismiss_hint_idempotent() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-hints4",
            "Hint Test 4",
            "dismiss test",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");

        let hint = create_hint(&goal.id, None, "user", "dismiss me", None)
            .await
            .expect("create");

        dismiss_hint(&hint.id).await.expect("dismiss first");
        dismiss_hint(&hint.id)
            .await
            .expect("dismiss second — idempotent");

        let fetched = get_hint(&hint.id).await.expect("get").expect("some");
        assert_eq!(fetched.status, "dismissed");
    }
}
