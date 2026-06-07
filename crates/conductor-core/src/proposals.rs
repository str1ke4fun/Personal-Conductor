use crate::db;
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Pending,
    Approved,
    Running,
    Succeeded,
    Failed,
    Rejected,
    Expired,
    Used,
}

impl ProposalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
            Self::Used => "used",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "rejected" => Ok(Self::Rejected),
            "expired" => Ok(Self::Expired),
            "used" => Ok(Self::Used),
            other => bail!("unknown proposal status: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    ReadOnly,
    DraftOnly,
    WorkspaceWrite,
    ExternalSideEffect,
    Destructive,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::DraftOnly => "draft_only",
            Self::WorkspaceWrite => "workspace_write",
            Self::ExternalSideEffect => "external_side_effect",
            Self::Destructive => "destructive",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "read_only" => Ok(Self::ReadOnly),
            "draft_only" => Ok(Self::DraftOnly),
            "workspace_write" => Ok(Self::WorkspaceWrite),
            "external_side_effect" => Ok(Self::ExternalSideEffect),
            "destructive" => Ok(Self::Destructive),
            other => bail!("unknown risk level: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ProposalSource {
    Chat,
    Proactive,
    Task,
    Hook,
}

impl ProposalSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Proactive => "proactive",
            Self::Task => "task",
            Self::Hook => "hook",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "chat" => Ok(Self::Chat),
            "proactive" => Ok(Self::Proactive),
            "task" => Ok(Self::Task),
            "hook" => Ok(Self::Hook),
            other => bail!("unknown proposal source: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Proposal {
    pub id: String,
    pub workspace_id: Option<String>,
    pub for_cwd: PathBuf,
    pub source: ProposalSource,
    pub title: String,
    pub content: String,
    pub reason: String,
    pub tool_id: Option<String>,
    pub tool_input_json: Option<String>,
    pub risk_level: RiskLevel,
    pub dry_run: bool,
    pub status: ProposalStatus,
    pub result_ref: Option<String>,
    pub agent_task_id: Option<String>,
    pub grant_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create(proposal: Proposal) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        INSERT INTO action_proposals (
            id, workspace_id, for_cwd, source, title, content, reason,
            tool_id, tool_input_json, risk_level, dry_run, status,
            result_ref, agent_task_id, grant_id, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        ON CONFLICT(id) DO UPDATE SET
            workspace_id = excluded.workspace_id,
            for_cwd = excluded.for_cwd,
            source = excluded.source,
            title = excluded.title,
            content = excluded.content,
            reason = excluded.reason,
            tool_id = excluded.tool_id,
            tool_input_json = excluded.tool_input_json,
            risk_level = excluded.risk_level,
            dry_run = excluded.dry_run,
            status = excluded.status,
            result_ref = excluded.result_ref,
            agent_task_id = excluded.agent_task_id,
            grant_id = excluded.grant_id,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&proposal.id)
    .bind(&proposal.workspace_id)
    .bind(proposal.for_cwd.display().to_string())
    .bind(proposal.source.as_str())
    .bind(&proposal.title)
    .bind(&proposal.content)
    .bind(&proposal.reason)
    .bind(&proposal.tool_id)
    .bind(&proposal.tool_input_json)
    .bind(proposal.risk_level.as_str())
    .bind(proposal.dry_run)
    .bind(proposal.status.as_str())
    .bind(&proposal.result_ref)
    .bind(&proposal.agent_task_id)
    .bind(&proposal.grant_id)
    .bind(proposal.created_at.to_rfc3339())
    .bind(proposal.updated_at.to_rfc3339())
    .execute(&pool)
    .await?;

    // Emit audit event: permission.requested
    if let Some(ref tool_id) = proposal.tool_id {
        let _ = crate::events::emit_permission_requested(
            &proposal.id,
            tool_id,
            proposal.risk_level.as_str(),
        )
        .await;
    }

    Ok(())
}

pub async fn list_pending() -> anyhow::Result<Vec<Proposal>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, workspace_id, for_cwd, source, title, content, reason,
               tool_id, tool_input_json, risk_level, dry_run, status,
               result_ref, agent_task_id, grant_id, created_at, updated_at
        FROM action_proposals
        WHERE status = 'pending'
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&pool)
    .await?;
    rows.into_iter().map(row_to_proposal).collect()
}

pub async fn list_by_status(status: ProposalStatus) -> anyhow::Result<Vec<Proposal>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, workspace_id, for_cwd, source, title, content, reason,
               tool_id, tool_input_json, risk_level, dry_run, status,
               result_ref, agent_task_id, grant_id, created_at, updated_at
        FROM action_proposals
        WHERE status = ?1
        ORDER BY created_at DESC
        "#,
    )
    .bind(status.as_str())
    .fetch_all(&pool)
    .await?;
    rows.into_iter().map(row_to_proposal).collect()
}

pub async fn list_for_cwd(
    cwd: &Path,
    status: Option<ProposalStatus>,
) -> anyhow::Result<Vec<Proposal>> {
    let pool = db::pool().await?;
    let query = if let Some(status) = status {
        sqlx::query(
            r#"
            SELECT id, workspace_id, for_cwd, source, title, content, reason,
                   tool_id, tool_input_json, risk_level, dry_run, status,
                   result_ref, agent_task_id, grant_id, created_at, updated_at
            FROM action_proposals
            WHERE for_cwd LIKE ?1 || '%' AND status = ?2
            ORDER BY created_at DESC
            "#,
        )
        .bind(format!("{}%", cwd.display()))
        .bind(status.as_str())
    } else {
        sqlx::query(
            r#"
            SELECT id, workspace_id, for_cwd, source, title, content, reason,
                   tool_id, tool_input_json, risk_level, dry_run, status,
                   result_ref, agent_task_id, grant_id, created_at, updated_at
            FROM action_proposals
            WHERE for_cwd LIKE ?1 || '%'
            ORDER BY created_at DESC
            "#,
        )
        .bind(format!("{}%", cwd.display()))
    };
    let rows = query.fetch_all(&pool).await?;
    rows.into_iter().map(row_to_proposal).collect()
}

pub async fn get(id: &str) -> anyhow::Result<Proposal> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, workspace_id, for_cwd, source, title, content, reason,
               tool_id, tool_input_json, risk_level, dry_run, status,
               result_ref, agent_task_id, grant_id, created_at, updated_at
        FROM action_proposals
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .with_context(|| format!("proposal not found: {id}"))?;
    row_to_proposal(row)
}

pub async fn find_open_by_tool_request(
    workspace_id: Option<&str>,
    tool_id: &str,
    tool_input_json: &str,
) -> anyhow::Result<Option<Proposal>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, workspace_id, for_cwd, source, title, content, reason,
               tool_id, tool_input_json, risk_level, dry_run, status,
               result_ref, agent_task_id, grant_id, created_at, updated_at
        FROM action_proposals
        WHERE tool_id = ?1
          AND tool_input_json = ?2
          AND (
                (?3 IS NULL AND workspace_id IS NULL)
                OR workspace_id = ?3
              )
          AND status IN ('pending', 'approved', 'running')
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(tool_id)
    .bind(tool_input_json)
    .bind(workspace_id)
    .fetch_optional(&pool)
    .await?;
    row.map(row_to_proposal).transpose()
}

pub async fn approve(id: &str) -> anyhow::Result<()> {
    set_status(id, ProposalStatus::Approved).await?;
    // Best-effort audit event
    if let Ok(p) = get(id).await {
        let tool_id = p.tool_id.as_deref().unwrap_or("unknown");
        crate::events::emit_permission_approved(id, tool_id).await;
    }
    Ok(())
}

pub async fn reject(id: &str) -> anyhow::Result<()> {
    set_status(id, ProposalStatus::Rejected).await?;
    // Best-effort audit event
    if let Ok(p) = get(id).await {
        let tool_id = p.tool_id.as_deref().unwrap_or("unknown");
        crate::events::emit_permission_denied(id, tool_id).await;
    }
    Ok(())
}

pub async fn mark_running(id: &str) -> anyhow::Result<()> {
    set_status(id, ProposalStatus::Running).await
}

pub async fn mark_succeeded(id: &str, result_ref: Option<String>) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE action_proposals
        SET status = 'succeeded', result_ref = ?1, updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(result_ref)
    .bind(Utc::now().to_rfc3339())
    .bind(id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn mark_failed(id: &str) -> anyhow::Result<()> {
    set_status(id, ProposalStatus::Failed).await
}

pub async fn mark_used(id: &str) -> anyhow::Result<()> {
    set_status(id, ProposalStatus::Used).await
}

pub async fn next_id() -> anyhow::Result<String> {
    let pool = db::pool().await?;
    let date = Utc::now().format("%Y%m%d").to_string();
    let prefix = format!("p-{date}-");

    let max_num: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT MAX(CAST(SUBSTR(id, LENGTH(?1) + 1) AS INTEGER))
        FROM action_proposals
        WHERE id LIKE ?1 || '%'
        "#,
    )
    .bind(&prefix)
    .fetch_one(&pool)
    .await?;

    let next = max_num.unwrap_or(0) + 1;
    Ok(format!("{prefix}{next:03}"))
}

pub async fn execute_proposal(id: &str) -> anyhow::Result<crate::tools::ToolExecutionResult> {
    let proposal = get(id).await?;

    if proposal.status != ProposalStatus::Approved {
        bail!("proposal must be approved before execution: {}", id);
    }

    let tool_id = proposal
        .tool_id
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("proposal has no tool_id"))?;

    let input: serde_json::Value = proposal
        .tool_input_json
        .as_ref()
        .map(|json| serde_json::from_str(json))
        .transpose()?
        .unwrap_or(serde_json::json!({}));

    let tool_call_id = uuid::Uuid::new_v4().to_string();
    let input_json = serde_json::to_string(&input)?;
    crate::tool_calls::create(crate::tool_calls::ToolCallCreate {
        id: tool_call_id.clone(),
        session_id: None,
        workspace_id: proposal.workspace_id.clone(),
        turn_id: None,
        llm_tool_call_id: None,
        tool_id: tool_id.clone(),
        input_json,
        agent_run_id: None,
        risk_level: Some(proposal.risk_level.as_str().to_string()),
    })
    .await?;
    crate::tool_calls::attach_proposal(&tool_call_id, id, proposal.grant_id.as_deref()).await?;
    crate::events::emit_tool_call_lifecycle(
        "tool_call.proposed",
        &tool_call_id,
        proposal.workspace_id.as_deref(),
        None,
        tool_id,
        "pending",
        serde_json::json!({
            "proposal_id": id,
            "permission_grant_id": proposal.grant_id.as_deref(),
            "risk_level": proposal.risk_level.as_str(),
            "input": input.clone(),
        }),
    )
    .await;

    let mut exec_input = input.clone();
    if let Some(obj) = exec_input.as_object_mut() {
        obj.insert(
            "tool_call_id".to_string(),
            serde_json::json!(tool_call_id.as_str()),
        );
        obj.insert("proposal_id".to_string(), serde_json::json!(id));
        if let Some(grant_id) = proposal.grant_id.as_deref() {
            obj.insert(
                "permission_grant_id".to_string(),
                serde_json::json!(grant_id),
            );
        }
    }

    mark_running(id).await?;

    let start = Utc::now();
    let tool_run_id = format!("tr-{}-{}", start.format("%Y%m%d"), id);
    crate::tool_calls::mark_executing(&tool_call_id).await?;
    crate::events::emit_tool_call_lifecycle(
        "tool_call.executing",
        &tool_call_id,
        proposal.workspace_id.as_deref(),
        None,
        tool_id,
        "executing",
        serde_json::json!({
            "proposal_id": id,
            "permission_grant_id": proposal.grant_id.as_deref(),
            "risk_level": proposal.risk_level.as_str(),
        }),
    )
    .await;

    let result = match crate::tools::execute_tool_with_workspace_async(
        tool_id,
        &exec_input,
        proposal.workspace_id.as_deref(),
    )
    .await
    {
        Ok(result) => {
            let output_str = serde_json::to_string(&result.output)?;
            let command_run_id = result
                .output
                .get("command_run_id")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            if let Some(ref command_run_id) = command_run_id {
                crate::tool_calls::attach_command_run(&tool_call_id, command_run_id).await?;
            }
            if result.success {
                crate::tool_calls::complete(&tool_call_id, &output_str).await?;
            } else {
                crate::tool_calls::fail(&tool_call_id, &output_str).await?;
            }
            let finished_at = Utc::now();
            log_tool_run(
                &tool_run_id,
                id,
                proposal.workspace_id.as_deref(),
                tool_id,
                &start,
                Some(&finished_at),
                &input,
                &result.output,
                result.error.as_ref().map(|x| x.as_str()),
            )
            .await?;

            if result.success {
                mark_succeeded(id, Some(format!("tool-runs/{}", tool_run_id))).await?;
            } else {
                mark_failed(id).await?;
            }

            crate::events::emit_tool_call_lifecycle(
                "tool_call.finished",
                &tool_call_id,
                proposal.workspace_id.as_deref(),
                None,
                tool_id,
                if result.success {
                    "succeeded"
                } else {
                    "failed"
                },
                serde_json::json!({
                    "proposal_id": id,
                    "permission_grant_id": proposal.grant_id.as_deref(),
                    "risk_level": proposal.risk_level.as_str(),
                    "success": result.success,
                    "duration_ms": result.duration_ms,
                    "command_run_id": command_run_id,
                }),
            )
            .await;

            result
        }
        Err(err) => {
            let error = err.to_string();
            crate::tool_calls::fail(&tool_call_id, &error).await?;
            let finished_at = Utc::now();
            log_tool_run(
                &tool_run_id,
                id,
                proposal.workspace_id.as_deref(),
                tool_id,
                &start,
                Some(&finished_at),
                &input,
                &serde_json::json!({}),
                Some(&error),
            )
            .await?;

            mark_failed(id).await?;
            crate::events::emit_tool_call_lifecycle(
                "tool_call.finished",
                &tool_call_id,
                proposal.workspace_id.as_deref(),
                None,
                tool_id,
                "failed",
                serde_json::json!({
                    "proposal_id": id,
                    "permission_grant_id": proposal.grant_id.as_deref(),
                    "risk_level": proposal.risk_level.as_str(),
                    "success": false,
                    "duration_ms": (finished_at - start).num_milliseconds().max(0),
                    "error": error,
                }),
            )
            .await;
            return Err(err);
        }
    };

    Ok(result)
}

async fn log_tool_run(
    id: &str,
    proposal_id: &str,
    workspace_id: Option<&str>,
    tool_id: &str,
    started_at: &DateTime<Utc>,
    finished_at: Option<&DateTime<Utc>>,
    input: &serde_json::Value,
    output: &serde_json::Value,
    error: Option<&str>,
) -> anyhow::Result<()> {
    let pool = db::pool().await?;

    let input_ref = save_tool_run_input(id, input).await?;
    let output_ref = save_tool_run_output(id, output).await?;

    sqlx::query(
        r#"
        INSERT INTO tool_runs (
            id, proposal_id, workspace_id, tool_id, status,
            started_at, finished_at, input_ref, output_ref, error
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
    )
    .bind(id)
    .bind(proposal_id)
    .bind(workspace_id)
    .bind(tool_id)
    .bind("completed")
    .bind(started_at.to_rfc3339())
    .bind(finished_at.map(|dt| dt.to_rfc3339()))
    .bind(input_ref)
    .bind(output_ref)
    .bind(error)
    .execute(&pool)
    .await?;

    Ok(())
}

async fn save_tool_run_input(id: &str, input: &serde_json::Value) -> anyhow::Result<String> {
    let path = crate::paths::Paths::subagent_runs_dir().join(format!("{}_input.json", id));
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, serde_json::to_string_pretty(input)?).await?;
    Ok(path.display().to_string())
}

async fn save_tool_run_output(id: &str, output: &serde_json::Value) -> anyhow::Result<String> {
    let path = crate::paths::Paths::subagent_runs_dir().join(format!("{}_output.json", id));
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, serde_json::to_string_pretty(output)?).await?;
    Ok(path.display().to_string())
}

async fn set_status(id: &str, status: ProposalStatus) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE action_proposals
        SET status = ?1, updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(status.as_str())
    .bind(Utc::now().to_rfc3339())
    .bind(id)
    .execute(&pool)
    .await?;
    Ok(())
}

fn row_to_proposal(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<Proposal> {
    Ok(Proposal {
        id: row.try_get("id")?,
        workspace_id: row.try_get("workspace_id")?,
        for_cwd: PathBuf::from(row.try_get::<String, _>("for_cwd")?),
        source: ProposalSource::from_str(row.try_get::<String, _>("source")?.as_str())?,
        title: row.try_get("title")?,
        content: row.try_get("content")?,
        reason: row.try_get("reason")?,
        tool_id: row.try_get("tool_id")?,
        tool_input_json: row.try_get("tool_input_json")?,
        risk_level: RiskLevel::from_str(row.try_get::<String, _>("risk_level")?.as_str())?,
        dry_run: row.try_get("dry_run")?,
        status: ProposalStatus::from_str(row.try_get::<String, _>("status")?.as_str())?,
        result_ref: row.try_get("result_ref")?,
        agent_task_id: row.try_get("agent_task_id")?,
        grant_id: row.try_get("grant_id")?,
        created_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("updated_at")?.as_str())?
            .with_timezone(&Utc),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;
    use crate::tools::register_builtin_tools;
    use crate::workspaces::{self, TrustLevel, Workspace, WorkspaceKind};

    async fn create_workspace(id: &str, root: &str) {
        let now = Utc::now();
        workspaces::create(Workspace {
            id: id.to_string(),
            root: PathBuf::from(root),
            name: id.to_string(),
            kind: WorkspaceKind::Code,
            trust_level: TrustLevel::Trusted,
            created_at: now,
            updated_at: now,
            last_active_at: None,
            metadata: serde_json::json!({}),
        })
        .await
        .expect("create workspace");
    }

    #[tokio::test]
    async fn action_proposal_lifecycle() {
        let _root = TestRoot::new();
        let cwd = PathBuf::from("I:/work/project-a");
        let now = Utc::now();
        let proposal = Proposal {
            id: "p-20260518-001".into(),
            workspace_id: Some("ws-test".into()),
            for_cwd: cwd.clone(),
            source: ProposalSource::Chat,
            title: "Test Proposal".into(),
            content: "Do the next thing".into(),
            reason: "test".into(),
            tool_id: Some("demo.echo".into()),
            tool_input_json: Some(r#"{"message": "hello"}"#.into()),
            risk_level: RiskLevel::ReadOnly,
            dry_run: true,
            status: ProposalStatus::Pending,
            result_ref: None,
            agent_task_id: None,
            grant_id: None,
            created_at: now,
            updated_at: now,
        };
        create(proposal).await.expect("create");
        assert_eq!(list_pending().await.expect("list").len(), 1);
        approve("p-20260518-001").await.expect("approve");
        let approved = list_for_cwd(&cwd, Some(ProposalStatus::Approved))
            .await
            .expect("approved");
        assert_eq!(approved.len(), 1);
        assert_eq!(approved[0].tool_id, Some("demo.echo".into()));
        mark_running("p-20260518-001").await.expect("running");
        mark_succeeded("p-20260518-001", Some("result.txt".into()))
            .await
            .expect("succeeded");
        let retrieved = get("p-20260518-001").await.expect("get");
        assert_eq!(retrieved.status, ProposalStatus::Succeeded);
        assert_eq!(retrieved.result_ref, Some("result.txt".into()));
    }

    #[tokio::test]
    async fn execute_proposal_complete_flow() {
        let _root = TestRoot::new();
        register_builtin_tools();

        let cwd = PathBuf::from("I:/work/project-b");
        create_workspace("ws-test-2", "I:/work/project-b").await;
        let now = Utc::now();
        let proposal = Proposal {
            id: "p-20260518-002".into(),
            workspace_id: Some("ws-test-2".into()),
            for_cwd: cwd,
            source: ProposalSource::Chat,
            title: "Execute Echo".into(),
            content: "Execute echo tool".into(),
            reason: "test execution".into(),
            tool_id: Some("demo.echo".into()),
            tool_input_json: Some(r#"{"message": "test message"}"#.into()),
            risk_level: RiskLevel::ReadOnly,
            dry_run: true,
            status: ProposalStatus::Approved,
            result_ref: None,
            agent_task_id: None,
            grant_id: None,
            created_at: now,
            updated_at: now,
        };
        create(proposal).await.expect("create");

        let result = execute_proposal("p-20260518-002")
            .await
            .expect("execute proposal");
        assert!(result.success);
        assert_eq!(result.output["echo"], "test message");

        let retrieved = get("p-20260518-002").await.expect("get");
        assert_eq!(retrieved.status, ProposalStatus::Succeeded);
        assert!(retrieved.result_ref.is_some());
    }

    #[tokio::test]
    async fn execute_proposal_not_approved() {
        let _root = TestRoot::new();
        register_builtin_tools();

        let cwd = PathBuf::from("I:/work/project-c");
        let now = Utc::now();
        let proposal = Proposal {
            id: "p-20260518-003".into(),
            workspace_id: None,
            for_cwd: cwd,
            source: ProposalSource::Chat,
            title: "Pending Proposal".into(),
            content: "Cannot execute".into(),
            reason: "test pending".into(),
            tool_id: Some("demo.echo".into()),
            tool_input_json: Some(r#"{"message": "hello"}"#.into()),
            risk_level: RiskLevel::ReadOnly,
            dry_run: true,
            status: ProposalStatus::Pending,
            result_ref: None,
            agent_task_id: None,
            grant_id: None,
            created_at: now,
            updated_at: now,
        };
        create(proposal).await.expect("create");

        let result = execute_proposal("p-20260518-003").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be approved"));
    }
}
