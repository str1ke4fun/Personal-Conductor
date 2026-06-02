use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub llm_tool_call_id: Option<String>,
    pub tool_id: String,
    pub input_json: String,
    pub output_json: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub agent_run_id: Option<String>,
    pub risk_level: Option<String>,
    pub proposal_id: Option<String>,
    pub permission_grant_id: Option<String>,
    pub command_run_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolCallCreate {
    pub id: String,
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub llm_tool_call_id: Option<String>,
    pub tool_id: String,
    pub input_json: String,
    pub agent_run_id: Option<String>,
    pub risk_level: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolCallFilter {
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub llm_tool_call_id: Option<String>,
    pub tool_id: Option<String>,
    pub status: Option<String>,
    pub proposal_id: Option<String>,
    pub command_run_id: Option<String>,
    pub limit: Option<u32>,
}

pub async fn create(input: ToolCallCreate) -> Result<ToolCall> {
    let pool = db::pool().await?;
    let now = Utc::now();
    sqlx::query(
        r#"
        INSERT INTO tool_calls (
            id, session_id, workspace_id, llm_tool_call_id, tool_id, input_json,
            status, started_at, agent_run_id, risk_level
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?8, ?9)
        "#,
    )
    .bind(&input.id)
    .bind(&input.session_id)
    .bind(&input.workspace_id)
    .bind(&input.llm_tool_call_id)
    .bind(&input.tool_id)
    .bind(&input.input_json)
    .bind(now.to_rfc3339())
    .bind(&input.agent_run_id)
    .bind(&input.risk_level)
    .execute(&pool)
    .await?;

    Ok(ToolCall {
        id: input.id,
        session_id: input.session_id,
        workspace_id: input.workspace_id,
        llm_tool_call_id: input.llm_tool_call_id,
        tool_id: input.tool_id,
        input_json: input.input_json,
        output_json: None,
        status: "pending".to_string(),
        error: None,
        started_at: now,
        completed_at: None,
        duration_ms: None,
        agent_run_id: input.agent_run_id,
        risk_level: input.risk_level,
        proposal_id: None,
        permission_grant_id: None,
        command_run_id: None,
    })
}

pub async fn mark_executing(id: &str) -> Result<()> {
    set_status(id, "executing").await
}

pub async fn set_status(id: &str, status: &str) -> Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE tool_calls
        SET status = ?1
        WHERE id = ?2
        "#,
    )
    .bind(status)
    .bind(id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn attach_proposal(
    id: &str,
    proposal_id: &str,
    permission_grant_id: Option<&str>,
) -> Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE tool_calls
        SET proposal_id = ?1, permission_grant_id = COALESCE(?2, permission_grant_id)
        WHERE id = ?3
        "#,
    )
    .bind(proposal_id)
    .bind(permission_grant_id)
    .bind(id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn attach_command_run(id: &str, command_run_id: &str) -> Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE tool_calls
        SET command_run_id = ?1
        WHERE id = ?2
        "#,
    )
    .bind(command_run_id)
    .bind(id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn mark_approval_required(
    id: &str,
    proposal_id: &str,
    permission_grant_id: Option<&str>,
    error: &str,
) -> Result<()> {
    let pool = db::pool().await?;
    let now = Utc::now();
    sqlx::query(
        r#"
        UPDATE tool_calls
        SET status = 'approval_required', proposal_id = ?1,
            permission_grant_id = COALESCE(?2, permission_grant_id),
            error = ?3, completed_at = ?4,
            duration_ms = CAST((julianday(?4) - julianday(started_at)) * 86400000 AS INTEGER)
        WHERE id = ?5
        "#,
    )
    .bind(proposal_id)
    .bind(permission_grant_id)
    .bind(error)
    .bind(now.to_rfc3339())
    .bind(id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn complete(id: &str, output: &str) -> Result<()> {
    let pool = db::pool().await?;
    let now = Utc::now();
    sqlx::query(
        r#"
        UPDATE tool_calls
        SET status = 'succeeded', output_json = ?1, completed_at = ?2,
            duration_ms = CAST((julianday(?2) - julianday(started_at)) * 86400000 AS INTEGER)
        WHERE id = ?3
        "#,
    )
    .bind(output)
    .bind(now.to_rfc3339())
    .bind(id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn fail(id: &str, error: &str) -> Result<()> {
    let pool = db::pool().await?;
    let now = Utc::now();
    sqlx::query(
        r#"
        UPDATE tool_calls
        SET status = 'failed', error = ?1, completed_at = ?2,
            duration_ms = CAST((julianday(?2) - julianday(started_at)) * 86400000 AS INTEGER)
        WHERE id = ?3
        "#,
    )
    .bind(error)
    .bind(now.to_rfc3339())
    .bind(id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn get(id: &str) -> Result<ToolCall> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, session_id, tool_id, input_json, output_json, status, error,
               started_at, completed_at, duration_ms, agent_run_id,
               workspace_id, llm_tool_call_id, risk_level, proposal_id,
               permission_grant_id, command_run_id
        FROM tool_calls WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(&pool)
    .await?;
    row_to_tool_call(row)
}

pub async fn list(filter: ToolCallFilter) -> Result<Vec<ToolCall>> {
    let pool = db::pool().await?;
    let limit = filter.limit.unwrap_or(100) as i64;

    let mut sql = String::from(
        "SELECT id, session_id, tool_id, input_json, output_json, status, error, \
         started_at, completed_at, duration_ms, agent_run_id, workspace_id, \
         llm_tool_call_id, risk_level, proposal_id, permission_grant_id, command_run_id \
         FROM tool_calls WHERE 1=1",
    );
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref sid) = filter.session_id {
        binds.push(sid.clone());
        sql.push_str(&format!(" AND session_id = ?{}", binds.len()));
    }
    if let Some(ref wid) = filter.workspace_id {
        binds.push(wid.clone());
        sql.push_str(&format!(" AND workspace_id = ?{}", binds.len()));
    }
    if let Some(ref llm_id) = filter.llm_tool_call_id {
        binds.push(llm_id.clone());
        sql.push_str(&format!(" AND llm_tool_call_id = ?{}", binds.len()));
    }
    if let Some(ref tid) = filter.tool_id {
        binds.push(tid.clone());
        sql.push_str(&format!(" AND tool_id = ?{}", binds.len()));
    }
    if let Some(ref st) = filter.status {
        binds.push(st.clone());
        sql.push_str(&format!(" AND status = ?{}", binds.len()));
    }
    if let Some(ref proposal_id) = filter.proposal_id {
        binds.push(proposal_id.clone());
        sql.push_str(&format!(" AND proposal_id = ?{}", binds.len()));
    }
    if let Some(ref command_run_id) = filter.command_run_id {
        binds.push(command_run_id.clone());
        sql.push_str(&format!(" AND command_run_id = ?{}", binds.len()));
    }

    let limit_idx = binds.len() + 1;
    sql.push_str(&format!(" ORDER BY started_at DESC LIMIT ?{limit_idx}"));

    let mut query = sqlx::query(&sql);
    for bind in &binds {
        query = query.bind(bind);
    }
    query = query.bind(limit);

    let rows = query.fetch_all(&pool).await?;
    rows.into_iter().map(row_to_tool_call).collect()
}

fn row_to_tool_call(row: sqlx::sqlite::SqliteRow) -> Result<ToolCall> {
    use sqlx::Row;
    let started_at_str: String = row.try_get("started_at")?;
    let completed_at_str: Option<String> = row.try_get("completed_at")?;

    Ok(ToolCall {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        workspace_id: row.try_get("workspace_id")?,
        llm_tool_call_id: row.try_get("llm_tool_call_id")?,
        tool_id: row.try_get("tool_id")?,
        input_json: row.try_get("input_json")?,
        output_json: row.try_get("output_json")?,
        status: row.try_get("status")?,
        error: row.try_get("error")?,
        started_at: chrono::DateTime::parse_from_rfc3339(&started_at_str)?.with_timezone(&Utc),
        completed_at: completed_at_str
            .map(|s| chrono::DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
        duration_ms: row.try_get("duration_ms")?,
        agent_run_id: row.try_get("agent_run_id")?,
        risk_level: row.try_get("risk_level")?,
        proposal_id: row.try_get("proposal_id")?,
        permission_grant_id: row.try_get("permission_grant_id")?,
        command_run_id: row.try_get("command_run_id")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    fn make_tool_call(id: &str, session_id: Option<&str>, tool_id: &str) -> ToolCallCreate {
        ToolCallCreate {
            id: id.into(),
            session_id: session_id.map(str::to_string),
            workspace_id: Some("ws-1".into()),
            llm_tool_call_id: Some(format!("llm-{id}")),
            tool_id: tool_id.into(),
            input_json: "{}".into(),
            agent_run_id: None,
            risk_level: Some("read_only".into()),
        }
    }

    #[tokio::test]
    async fn create_and_get_tool_call() {
        let _root = TestRoot::new();
        let mut input = make_tool_call("tc-001", Some("sess-1"), "file.read");
        input.input_json = r#"{"path":"/tmp/test"}"#.into();
        let tc = create(input).await.expect("create");
        assert_eq!(tc.status, "pending");

        let got = get("tc-001").await.expect("get");
        assert_eq!(got.tool_id, "file.read");
        assert_eq!(got.session_id, Some("sess-1".into()));
        assert_eq!(got.workspace_id, Some("ws-1".into()));
        assert_eq!(got.llm_tool_call_id, Some("llm-tc-001".into()));
        assert_eq!(got.risk_level, Some("read_only".into()));
    }

    #[tokio::test]
    async fn complete_tool_call() {
        let _root = TestRoot::new();
        let mut input = make_tool_call("tc-002", None, "bash.execute");
        input.input_json = r#"{"command":"echo hi"}"#.into();
        create(input).await.expect("create");

        complete("tc-002", r#"{"stdout":"hi"}"#)
            .await
            .expect("complete");

        let got = get("tc-002").await.expect("get");
        assert_eq!(got.status, "succeeded");
        assert!(got.output_json.is_some());
        assert!(got.completed_at.is_some());
        assert!(got.duration_ms.is_some());
    }

    #[tokio::test]
    async fn fail_tool_call() {
        let _root = TestRoot::new();
        let mut input = make_tool_call("tc-003", None, "bash.execute");
        input.input_json = r#"{"command":"false"}"#.into();
        create(input).await.expect("create");

        fail("tc-003", "exit code 1").await.expect("fail");

        let got = get("tc-003").await.expect("get");
        assert_eq!(got.status, "failed");
        assert_eq!(got.error, Some("exit code 1".into()));
    }

    #[tokio::test]
    async fn list_with_filter() {
        let _root = TestRoot::new();
        create(make_tool_call("tc-010", Some("s1"), "file.read"))
            .await
            .expect("create");
        create(make_tool_call("tc-011", Some("s1"), "bash.execute"))
            .await
            .expect("create");
        create(make_tool_call("tc-012", Some("s2"), "file.read"))
            .await
            .expect("create");

        let all = list(ToolCallFilter::default()).await.expect("list all");
        assert_eq!(all.len(), 3);

        let s1 = list(ToolCallFilter {
            session_id: Some("s1".into()),
            ..Default::default()
        })
        .await
        .expect("list s1");
        assert_eq!(s1.len(), 2);

        let fr = list(ToolCallFilter {
            tool_id: Some("file.read".into()),
            ..Default::default()
        })
        .await
        .expect("list file.read");
        assert_eq!(fr.len(), 2);

        let limited = list(ToolCallFilter {
            limit: Some(1),
            ..Default::default()
        })
        .await
        .expect("list limited");
        assert_eq!(limited.len(), 1);
    }

    #[tokio::test]
    async fn links_proposal_and_command_run() {
        let _root = TestRoot::new();
        create(make_tool_call("tc-020", Some("s1"), "bash.execute"))
            .await
            .expect("create");

        mark_executing("tc-020").await.expect("mark executing");
        attach_command_run("tc-020", "cr-020")
            .await
            .expect("attach command run");
        mark_approval_required("tc-020", "p-020", Some("pg-020"), "approval_required")
            .await
            .expect("mark approval");

        let got = get("tc-020").await.expect("get");
        assert_eq!(got.status, "approval_required");
        assert_eq!(got.command_run_id, Some("cr-020".into()));
        assert_eq!(got.proposal_id, Some("p-020".into()));
        assert_eq!(got.permission_grant_id, Some("pg-020".into()));
        assert!(got.completed_at.is_some());

        let by_command = list(ToolCallFilter {
            command_run_id: Some("cr-020".into()),
            ..Default::default()
        })
        .await
        .expect("list by command");
        assert_eq!(by_command.len(), 1);
    }

    #[tokio::test]
    async fn get_nonexistent_fails() {
        let _root = TestRoot::new();
        let result = get("nonexistent").await;
        assert!(result.is_err());
    }
}
