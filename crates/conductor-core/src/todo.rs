use crate::db;
use anyhow::Context;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub chatsession_id: String,
    pub content: serde_json::Value,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn create(chatsession_id: &str, content: serde_json::Value) -> anyhow::Result<TodoItem> {
    let pool = db::pool().await?;
    let now = Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO todos (id, chatsession_id, content, status, created_at, updated_at)
        VALUES (?1, ?2, ?3, 'pending', ?4, ?4)
        "#,
    )
    .bind(&id)
    .bind(chatsession_id)
    .bind(serde_json::to_string(&content)?)
    .bind(&now)
    .execute(&pool)
    .await?;

    Ok(TodoItem {
        id,
        chatsession_id: chatsession_id.to_string(),
        content,
        status: "pending".to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub async fn list_by_session(chatsession_id: &str) -> anyhow::Result<Vec<TodoItem>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, chatsession_id, content, status, created_at, updated_at
        FROM todos
        WHERE chatsession_id = ?1
        ORDER BY created_at ASC
        "#,
    )
    .bind(chatsession_id)
    .fetch_all(&pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(TodoItem {
                id: row.try_get("id")?,
                chatsession_id: row.try_get("chatsession_id")?,
                content: serde_json::from_str(row.try_get::<String, _>("content")?.as_str())?,
                status: row.try_get("status")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect()
}

pub async fn update(id: &str, content: serde_json::Value, status: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let now = Utc::now().to_rfc3339();

    let result = sqlx::query(
        r#"
        UPDATE todos SET content = ?1, status = ?2, updated_at = ?3
        WHERE id = ?4
        "#,
    )
    .bind(serde_json::to_string(&content)?)
    .bind(status)
    .bind(&now)
    .bind(id)
    .execute(&pool)
    .await?;

    if result.rows_affected() == 0 {
        anyhow::bail!("todo not found: {id}");
    }
    Ok(())
}

pub async fn delete(id: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let result = sqlx::query("DELETE FROM todos WHERE id = ?1")
        .bind(id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        anyhow::bail!("todo not found: {id}");
    }
    Ok(())
}

pub async fn clear_session(chatsession_id: &str) -> anyhow::Result<u64> {
    let pool = db::pool().await?;
    let result = sqlx::query("DELETE FROM todos WHERE chatsession_id = ?1")
        .bind(chatsession_id)
        .execute(&pool)
        .await?;

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn create_and_list() {
        let _root = TestRoot::new();
        let item = create("session-1", serde_json::json!({"text": "buy milk"}))
            .await
            .expect("create");
        assert_eq!(item.chatsession_id, "session-1");
        assert_eq!(item.status, "pending");

        let items = list_by_session("session-1").await.expect("list");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, item.id);
    }

    #[tokio::test]
    async fn session_isolation() {
        let _root = TestRoot::new();
        create("s1", serde_json::json!("a"))
            .await
            .expect("create a");
        create("s2", serde_json::json!("b"))
            .await
            .expect("create b");

        let s1 = list_by_session("s1").await.expect("list s1");
        let s2 = list_by_session("s2").await.expect("list s2");
        assert_eq!(s1.len(), 1);
        assert_eq!(s2.len(), 1);
        assert_ne!(s1[0].id, s2[0].id);
    }

    #[tokio::test]
    async fn update_item() {
        let _root = TestRoot::new();
        let item = create("s1", serde_json::json!("old"))
            .await
            .expect("create");
        update(&item.id, serde_json::json!("new"), "done")
            .await
            .expect("update");

        let items = list_by_session("s1").await.expect("list");
        assert_eq!(items[0].content, serde_json::json!("new"));
        assert_eq!(items[0].status, "done");
    }

    #[tokio::test]
    async fn delete_item() {
        let _root = TestRoot::new();
        let item = create("s1", serde_json::json!("x")).await.expect("create");
        delete(&item.id).await.expect("delete");

        let items = list_by_session("s1").await.expect("list");
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn delete_nonexistent_fails() {
        let _root = TestRoot::new();
        let result = delete("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_nonexistent_fails() {
        let _root = TestRoot::new();
        let result = update("nonexistent", serde_json::json!("x"), "done").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_clear_session() {
        let _root = TestRoot::new();
        create("s1", serde_json::json!("a"))
            .await
            .expect("create a");
        create("s1", serde_json::json!("b"))
            .await
            .expect("create b");
        create("s2", serde_json::json!("c"))
            .await
            .expect("create c");

        let removed = super::clear_session("s1").await.expect("clear");
        assert_eq!(removed, 2);

        let s1 = list_by_session("s1").await.expect("list s1");
        let s2 = list_by_session("s2").await.expect("list s2");
        assert!(s1.is_empty());
        assert_eq!(s2.len(), 1);
    }

    #[tokio::test]
    async fn empty_list_for_unknown_session() {
        let _root = TestRoot::new();
        let items = list_by_session("nonexistent").await.expect("list");
        assert!(items.is_empty());
    }
}
