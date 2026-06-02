use crate::db;
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, HashSet};

const DEFAULT_TASK_LIST_ID: &str = "global";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskListScope {
    Global,
    Workspace(String),
    AgentRun(String),
    ChatSession(String),
    Document(String),
}

impl TaskListScope {
    pub fn as_storage_key(&self) -> String {
        match self {
            Self::Global => "global".to_string(),
            Self::Workspace(id) => format!("workspace:{id}"),
            Self::AgentRun(id) => format!("agent:{id}"),
            Self::ChatSession(id) => format!("chat:{id}"),
            Self::Document(id) => format!("document:{id}"),
        }
    }

    pub fn from_storage_key(value: &str) -> Self {
        if value == "global" {
            return Self::Global;
        }
        if let Some(id) = value.strip_prefix("workspace:") {
            return Self::Workspace(id.to_string());
        }
        if let Some(id) = value.strip_prefix("agent:") {
            return Self::AgentRun(id.to_string());
        }
        if let Some(id) = value.strip_prefix("chat:") {
            return Self::ChatSession(id.to_string());
        }
        if let Some(id) = value.strip_prefix("document:") {
            return Self::Document(id.to_string());
        }
        Self::Global
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TaskList {
    pub id: String,
    pub scope: TaskListScope,
    pub title: String,
    pub workspace_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata_json: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskStatus {
    Pending,
    InProgress,
    Completed,
}

impl AgentTaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            other => bail!("unknown agent task status: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentTask {
    pub id: String,
    pub task_list_id: String,
    pub subject: String,
    pub description: String,
    pub active_form: Option<String>,
    pub owner: Option<String>,
    pub status: AgentTaskStatus,
    pub workspace_id: Option<String>,
    pub source: String,
    pub kind: String,
    pub legacy_id: Option<String>,
    pub est_minutes: Option<u32>,
    pub blocks: Vec<String>,
    pub blocked_by: Vec<String>,
    pub metadata_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TaskCreateInput {
    pub task_list_id: Option<String>,
    pub workspace_id: Option<String>,
    pub subject: String,
    #[serde(default)]
    pub description: String,
    pub active_form: Option<String>,
    pub owner: Option<String>,
    pub source: Option<String>,
    pub kind: Option<String>,
    pub legacy_id: Option<String>,
    pub est_minutes: Option<u32>,
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub blocks: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TaskUpdateInput {
    pub task_list_id: Option<String>,
    pub task_id: String,
    pub subject: Option<String>,
    pub description: Option<String>,
    pub active_form: Option<String>,
    pub owner: Option<String>,
    pub status: Option<AgentTaskStatus>,
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub add_blocks: Vec<String>,
    #[serde(default)]
    pub add_blocked_by: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TaskListFilter {
    pub task_list_id: Option<String>,
    pub workspace_id: Option<String>,
    pub status: Option<AgentTaskStatus>,
    pub owner: Option<String>,
    pub include_completed: bool,
    pub available_only: bool,
}

pub async fn ensure_task_list(
    id: Option<&str>,
    workspace_id: Option<&str>,
) -> anyhow::Result<TaskList> {
    let pool = db::pool().await?;
    ensure_task_list_with_pool(&pool, id, workspace_id).await
}

pub async fn create_task(input: TaskCreateInput) -> anyhow::Result<AgentTask> {
    let pool = db::pool().await?;
    let subject = input.subject.trim();
    if subject.is_empty() {
        bail!("task subject cannot be empty");
    }
    let task_list = ensure_task_list_with_pool(
        &pool,
        input.task_list_id.as_deref(),
        input.workspace_id.as_deref(),
    )
    .await?;
    let now = Utc::now();
    let task_id = next_task_id(&pool, &task_list.id).await?;
    let source = input.source.unwrap_or_else(|| "manual".to_string());
    let kind = input.kind.unwrap_or_else(|| "task".to_string());
    let metadata_json = input.metadata.map(|value| value.to_string());

    sqlx::query(
        r#"
        INSERT INTO agent_tasklist_items (
            task_list_id, id, subject, description, active_form, owner,
            status, workspace_id, source, kind, legacy_id, est_minutes,
            created_at, updated_at, metadata_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        "#,
    )
    .bind(&task_list.id)
    .bind(&task_id)
    .bind(subject)
    .bind(input.description)
    .bind(input.active_form)
    .bind(input.owner)
    .bind(AgentTaskStatus::Pending.as_str())
    .bind(input.workspace_id)
    .bind(source)
    .bind(kind)
    .bind(input.legacy_id)
    .bind(input.est_minutes.map(|v| v as i64))
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(metadata_json)
    .execute(&pool)
    .await?;

    insert_dependencies(
        &pool,
        &task_list.id,
        &task_id,
        &input.blocks,
        &input.blocked_by,
    )
    .await?;
    get_task_from_pool(&pool, &task_list.id, &task_id).await
}

pub async fn list_tasks(filter: TaskListFilter) -> anyhow::Result<Vec<AgentTask>> {
    let pool = db::pool().await?;
    let task_list = ensure_task_list_with_pool(
        &pool,
        filter.task_list_id.as_deref(),
        filter.workspace_id.as_deref(),
    )
    .await?;
    let mut tasks = list_tasks_for_list(&pool, &task_list.id).await?;

    if let Some(workspace_id) = filter.workspace_id {
        tasks.retain(|task| task.workspace_id.as_deref() == Some(workspace_id.as_str()));
    }
    let status_by_id = tasks
        .iter()
        .map(|task| (task.id.clone(), task.status.clone()))
        .collect::<HashMap<_, _>>();
    if let Some(status) = filter.status {
        tasks.retain(|task| task.status == status);
    } else if !filter.include_completed {
        tasks.retain(|task| task.status != AgentTaskStatus::Completed);
    }
    if let Some(owner) = filter.owner {
        tasks.retain(|task| task.owner.as_deref() == Some(owner.as_str()));
    }
    if filter.available_only {
        tasks.retain(|task| is_available_with_statuses(task, &status_by_id));
    }

    Ok(tasks)
}

pub async fn list_tasks_by_budget(
    budget_minutes: u32,
    filter: TaskListFilter,
) -> anyhow::Result<Vec<AgentTask>> {
    let pool = db::pool().await?;
    let task_list = ensure_task_list_with_pool(
        &pool,
        filter.task_list_id.as_deref(),
        filter.workspace_id.as_deref(),
    )
    .await?;

    let budget = budget_minutes as i64;
    let rows = sqlx::query(
        r#"
        SELECT task_list_id, id, subject, description, active_form, owner,
               status, workspace_id, source, kind, legacy_id, est_minutes,
               created_at, updated_at, metadata_json
        FROM agent_tasklist_items
        WHERE task_list_id = ?1
          AND status IN ('pending', 'in_progress')
          AND (est_minutes IS NULL OR est_minutes <= ?2)
        ORDER BY est_minutes IS NULL ASC, est_minutes ASC, created_at DESC
        "#,
    )
    .bind(&task_list.id)
    .bind(budget)
    .fetch_all(&pool)
    .await?;

    let mut tasks = rows
        .into_iter()
        .map(row_to_agent_task)
        .collect::<anyhow::Result<Vec<_>>>()?;

    attach_dependencies(&pool, &task_list.id, &mut tasks).await?;

    if let Some(workspace_id) = filter.workspace_id {
        tasks.retain(|task| task.workspace_id.as_deref() == Some(workspace_id.as_str()));
    }
    if let Some(owner) = filter.owner {
        tasks.retain(|task| task.owner.as_deref() == Some(owner.as_str()));
    }

    Ok(tasks)
}

pub async fn get_task(task_list_id: Option<&str>, task_id: &str) -> anyhow::Result<AgentTask> {
    let pool = db::pool().await?;
    let task_list = ensure_task_list_with_pool(&pool, task_list_id, None).await?;
    get_task_from_pool(&pool, &task_list.id, task_id).await
}

pub async fn update_task(input: TaskUpdateInput) -> anyhow::Result<AgentTask> {
    let pool = db::pool().await?;
    let task_list = ensure_task_list_with_pool(&pool, input.task_list_id.as_deref(), None).await?;
    let existing = get_task_from_pool(&pool, &task_list.id, &input.task_id).await?;
    let now = Utc::now();
    let subject = input.subject.unwrap_or(existing.subject);
    let description = input.description.unwrap_or(existing.description);
    let active_form = input.active_form.or(existing.active_form);
    let owner = input.owner.or(existing.owner);
    let status = input.status.unwrap_or(existing.status);
    let metadata_json = match input.metadata {
        Some(value) => Some(value.to_string()),
        None => existing.metadata_json.map(|value| value.to_string()),
    };

    sqlx::query(
        r#"
        UPDATE agent_tasklist_items
        SET subject = ?1,
            description = ?2,
            active_form = ?3,
            owner = ?4,
            status = ?5,
            updated_at = ?6,
            metadata_json = ?7
        WHERE task_list_id = ?8 AND id = ?9
        "#,
    )
    .bind(subject)
    .bind(description)
    .bind(active_form)
    .bind(owner)
    .bind(status.as_str())
    .bind(now.to_rfc3339())
    .bind(metadata_json)
    .bind(&task_list.id)
    .bind(&input.task_id)
    .execute(&pool)
    .await?;

    insert_dependencies(
        &pool,
        &task_list.id,
        &input.task_id,
        &input.add_blocks,
        &input.add_blocked_by,
    )
    .await?;

    get_task_from_pool(&pool, &task_list.id, &input.task_id).await
}

pub async fn claim_task(
    task_list_id: Option<&str>,
    task_id: &str,
    owner: &str,
) -> anyhow::Result<AgentTask> {
    let mut conn = db::pool().await?.acquire().await?;
    let task_list_id = task_list_id_or_default(task_list_id);
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;

    let result = async {
        let task = get_task_on_conn(&mut conn, &task_list_id, task_id).await?;
        if task.status != AgentTaskStatus::Pending {
            bail!("task is not pending: {task_id}");
        }
        if task.owner.is_some() {
            bail!("task already has an owner: {task_id}");
        }

        let all_tasks = list_tasks_for_list_on_conn(&mut conn, &task_list_id).await?;
        let status_by_id = all_tasks
            .iter()
            .map(|task| (task.id.clone(), task.status.clone()))
            .collect::<HashMap<_, _>>();
        if !is_available_with_statuses(&task, &status_by_id) {
            bail!("task is blocked: {task_id}");
        }

        let now = Utc::now();
        let affected = sqlx::query(
            r#"
            UPDATE agent_tasklist_items
            SET owner = ?1, updated_at = ?2
            WHERE task_list_id = ?3 AND id = ?4 AND owner IS NULL AND status = 'pending'
            "#,
        )
        .bind(owner)
        .bind(now.to_rfc3339())
        .bind(&task_list_id)
        .bind(task_id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
        if affected != 1 {
            bail!("failed to claim task: {task_id}");
        }

        get_task_on_conn(&mut conn, &task_list_id, task_id).await
    }
    .await;

    match result {
        Ok(task) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(task)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

pub async fn migrate_legacy_tasks(task_list_id: Option<&str>) -> anyhow::Result<usize> {
    let pool = db::pool().await?;
    let task_list = ensure_task_list_with_pool(&pool, task_list_id, None).await?;
    let legacy = crate::tasks::load().await?;
    let mut migrated = 0;

    for task in legacy.tasks {
        let status = match task.status {
            crate::tasks::TaskStatus::Pending => AgentTaskStatus::Pending,
            crate::tasks::TaskStatus::InProgress => AgentTaskStatus::InProgress,
            crate::tasks::TaskStatus::Passed
            | crate::tasks::TaskStatus::Rejected
            | crate::tasks::TaskStatus::Skipped => AgentTaskStatus::Completed,
        };
        let metadata = serde_json::json!({
            "legacy_task_id": task.id,
            "legacy_status": task.status.as_str(),
            "artifact_file": task.artifact.file.map(|path| path.display().to_string()),
            "artifact_anchor": task.artifact.anchor,
            "summary_ref": task.summary_ref,
            "est_minutes": task.est_minutes,
            "focus_hint": task.focus_hint,
            "session_id": task.session_id,
            "terminal_id": task.terminal_id,
            "cwd": task.cwd.as_ref().map(|path| path.display().to_string()),
            "current_request": task.current_request,
            "last_output_summary": task.last_output_summary,
            "last_event_at": task.last_event_at.map(|value| value.to_rfc3339()),
            "permission_summary": task.permission_summary,
            "review_result": match task.status {
                crate::tasks::TaskStatus::Passed => Some("passed"),
                crate::tasks::TaskStatus::Rejected => Some("rejected"),
                crate::tasks::TaskStatus::Skipped => Some("skipped"),
                _ => None,
            },
            "migrated_from": "tasks",
        });
        let subject = task
            .current_request
            .clone()
            .or_else(|| task.focus_hint.clone())
            .unwrap_or_else(|| task.kind.clone());
        let description = task
            .last_output_summary
            .clone()
            .or_else(|| task.permission_summary.clone())
            .unwrap_or_default();
        let active_form = task.focus_hint.clone();
        let workspace_id = task
            .cwd
            .as_ref()
            .and_then(|cwd| cwd.file_name())
            .and_then(|name| name.to_str())
            .map(|name| format!("workspace:{name}"));
        let now = Utc::now();

        let affected = sqlx::query(
            r#"
            INSERT INTO agent_tasklist_items (
                task_list_id, id, subject, description, active_form, owner,
                status, workspace_id, source, kind, legacy_id, est_minutes,
                created_at, updated_at, metadata_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(task_list_id, id) DO NOTHING
            "#,
        )
        .bind(&task_list.id)
        .bind(&task.id)
        .bind(subject)
        .bind(description)
        .bind(active_form)
        .bind(status.as_str())
        .bind(workspace_id)
        .bind("hook")
        .bind(&task.kind)
        .bind(&task.id)
        .bind(task.est_minutes.map(|v| v as i64))
        .bind(task.created_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(metadata.to_string())
        .execute(&pool)
        .await?
        .rows_affected();
        migrated += affected as usize;
    }

    Ok(migrated)
}

fn task_list_id_or_default(id: Option<&str>) -> String {
    id.map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_TASK_LIST_ID)
        .to_string()
}

async fn ensure_task_list_with_pool(
    pool: &SqlitePool,
    id: Option<&str>,
    workspace_id: Option<&str>,
) -> anyhow::Result<TaskList> {
    ensure_task_list_with_executor(pool, id, workspace_id).await
}

async fn ensure_task_list_with_executor<'e, E>(
    executor: E,
    id: Option<&str>,
    workspace_id: Option<&str>,
) -> anyhow::Result<TaskList>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite> + Copy,
{
    let id = id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_TASK_LIST_ID);
    let scope = workspace_id
        .map(|workspace_id| TaskListScope::Workspace(workspace_id.to_string()))
        .unwrap_or_else(|| TaskListScope::from_storage_key(id));
    let title = if id == DEFAULT_TASK_LIST_ID {
        "Global Inbox".to_string()
    } else {
        id.to_string()
    };
    let now = Utc::now();

    sqlx::query(
        r#"
        INSERT INTO task_lists (id, scope, title, workspace_id, created_at, updated_at, metadata_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)
        ON CONFLICT(id) DO UPDATE SET
            workspace_id = COALESCE(excluded.workspace_id, task_lists.workspace_id),
            updated_at = task_lists.updated_at
        "#,
    )
    .bind(id)
    .bind(scope.as_storage_key())
    .bind(title)
    .bind(workspace_id)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(executor)
    .await?;

    let row = sqlx::query(
        r#"
        SELECT id, scope, title, workspace_id, created_at, updated_at, metadata_json
        FROM task_lists
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(executor)
    .await?;

    row_to_task_list(row)
}

async fn next_task_id(pool: &SqlitePool, task_list_id: &str) -> anyhow::Result<String> {
    let max_num: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT MAX(CAST(id AS INTEGER))
        FROM agent_tasklist_items
        WHERE task_list_id = ?1 AND id GLOB '[0-9]*'
        "#,
    )
    .bind(task_list_id)
    .fetch_one(pool)
    .await?;

    Ok((max_num.unwrap_or(0) + 1).to_string())
}

async fn insert_dependencies(
    pool: &SqlitePool,
    task_list_id: &str,
    task_id: &str,
    blocks: &[String],
    blocked_by: &[String],
) -> anyhow::Result<()> {
    let mut seen = HashSet::new();
    for blocked_task_id in blocks {
        if blocked_task_id == task_id || !seen.insert(("blocks", blocked_task_id.as_str())) {
            continue;
        }
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO task_dependencies (task_list_id, from_task_id, to_task_id)
            VALUES (?1, ?2, ?3)
            "#,
        )
        .bind(task_list_id)
        .bind(task_id)
        .bind(blocked_task_id)
        .execute(pool)
        .await?;
    }

    for blocker_task_id in blocked_by {
        if blocker_task_id == task_id || !seen.insert(("blocked_by", blocker_task_id.as_str())) {
            continue;
        }
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO task_dependencies (task_list_id, from_task_id, to_task_id)
            VALUES (?1, ?2, ?3)
            "#,
        )
        .bind(task_list_id)
        .bind(blocker_task_id)
        .bind(task_id)
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn list_tasks_for_list(
    pool: &SqlitePool,
    task_list_id: &str,
) -> anyhow::Result<Vec<AgentTask>> {
    list_tasks_for_list_with_executor(pool, task_list_id).await
}

async fn list_tasks_for_list_with_executor<'e, E>(
    executor: E,
    task_list_id: &str,
) -> anyhow::Result<Vec<AgentTask>>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite> + Copy,
{
    let rows = sqlx::query(
        r#"
        SELECT task_list_id, id, subject, description, active_form, owner,
               status, workspace_id, source, kind, legacy_id, est_minutes,
               created_at, updated_at, metadata_json
        FROM agent_tasklist_items
        WHERE task_list_id = ?1
        ORDER BY CAST(id AS INTEGER) ASC, created_at ASC
        "#,
    )
    .bind(task_list_id)
    .fetch_all(executor)
    .await?;

    let mut tasks = rows
        .into_iter()
        .map(row_to_agent_task)
        .collect::<anyhow::Result<Vec<_>>>()?;
    attach_dependencies(executor, task_list_id, &mut tasks).await?;
    Ok(tasks)
}

async fn list_tasks_for_list_on_conn(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    task_list_id: &str,
) -> anyhow::Result<Vec<AgentTask>> {
    let rows = sqlx::query(
        r#"
        SELECT task_list_id, id, subject, description, active_form, owner,
               status, workspace_id, source, kind, legacy_id, est_minutes,
               created_at, updated_at, metadata_json
        FROM agent_tasklist_items
        WHERE task_list_id = ?1
        ORDER BY CAST(id AS INTEGER) ASC, created_at ASC
        "#,
    )
    .bind(task_list_id)
    .fetch_all(&mut **conn)
    .await?;

    let mut tasks = rows
        .into_iter()
        .map(row_to_agent_task)
        .collect::<anyhow::Result<Vec<_>>>()?;
    attach_dependencies_on_conn(conn, task_list_id, &mut tasks).await?;
    Ok(tasks)
}

async fn get_task_from_pool(
    pool: &SqlitePool,
    task_list_id: &str,
    task_id: &str,
) -> anyhow::Result<AgentTask> {
    get_task_with_executor(pool, task_list_id, task_id).await
}

async fn get_task_with_executor<'e, E>(
    executor: E,
    task_list_id: &str,
    task_id: &str,
) -> anyhow::Result<AgentTask>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite> + Copy,
{
    let row = sqlx::query(
        r#"
        SELECT task_list_id, id, subject, description, active_form, owner,
               status, workspace_id, source, kind, legacy_id, est_minutes,
               created_at, updated_at, metadata_json
        FROM agent_tasklist_items
        WHERE task_list_id = ?1 AND id = ?2
        "#,
    )
    .bind(task_list_id)
    .bind(task_id)
    .fetch_one(executor)
    .await
    .with_context(|| format!("task not found: {task_id}"))?;
    let mut task = row_to_agent_task(row)?;
    attach_dependencies(executor, task_list_id, std::slice::from_mut(&mut task)).await?;
    Ok(task)
}

async fn get_task_on_conn(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    task_list_id: &str,
    task_id: &str,
) -> anyhow::Result<AgentTask> {
    let row = sqlx::query(
        r#"
        SELECT task_list_id, id, subject, description, active_form, owner,
               status, workspace_id, source, kind, legacy_id, est_minutes,
               created_at, updated_at, metadata_json
        FROM agent_tasklist_items
        WHERE task_list_id = ?1 AND id = ?2
        "#,
    )
    .bind(task_list_id)
    .bind(task_id)
    .fetch_one(&mut **conn)
    .await
    .with_context(|| format!("task not found: {task_id}"))?;
    let mut task = row_to_agent_task(row)?;
    attach_dependencies_on_conn(conn, task_list_id, std::slice::from_mut(&mut task)).await?;
    Ok(task)
}

async fn attach_dependencies<'e, E>(
    executor: E,
    task_list_id: &str,
    tasks: &mut [AgentTask],
) -> anyhow::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite> + Copy,
{
    if tasks.is_empty() {
        return Ok(());
    }

    let rows = sqlx::query(
        r#"
        SELECT from_task_id, to_task_id
        FROM task_dependencies
        WHERE task_list_id = ?1
        "#,
    )
    .bind(task_list_id)
    .fetch_all(executor)
    .await?;

    for row in rows {
        let from_task_id: String = row.try_get("from_task_id")?;
        let to_task_id: String = row.try_get("to_task_id")?;
        for task in tasks.iter_mut() {
            if task.id == from_task_id {
                task.blocks.push(to_task_id.clone());
            }
            if task.id == to_task_id {
                task.blocked_by.push(from_task_id.clone());
            }
        }
    }
    for task in tasks {
        task.blocks.sort();
        task.blocks.dedup();
        task.blocked_by.sort();
        task.blocked_by.dedup();
    }

    Ok(())
}

async fn attach_dependencies_on_conn(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
    task_list_id: &str,
    tasks: &mut [AgentTask],
) -> anyhow::Result<()> {
    if tasks.is_empty() {
        return Ok(());
    }

    let rows = sqlx::query(
        r#"
        SELECT from_task_id, to_task_id
        FROM task_dependencies
        WHERE task_list_id = ?1
        "#,
    )
    .bind(task_list_id)
    .fetch_all(&mut **conn)
    .await?;

    apply_dependency_rows(rows, tasks)
}

fn apply_dependency_rows(
    rows: Vec<sqlx::sqlite::SqliteRow>,
    tasks: &mut [AgentTask],
) -> anyhow::Result<()> {
    for row in rows {
        let from_task_id: String = row.try_get("from_task_id")?;
        let to_task_id: String = row.try_get("to_task_id")?;
        for task in tasks.iter_mut() {
            if task.id == from_task_id {
                task.blocks.push(to_task_id.clone());
            }
            if task.id == to_task_id {
                task.blocked_by.push(from_task_id.clone());
            }
        }
    }
    for task in tasks {
        task.blocks.sort();
        task.blocks.dedup();
        task.blocked_by.sort();
        task.blocked_by.dedup();
    }

    Ok(())
}

fn is_available_with_statuses(
    task: &AgentTask,
    status_by_id: &HashMap<String, AgentTaskStatus>,
) -> bool {
    task.status == AgentTaskStatus::Pending
        && task.owner.is_none()
        && task
            .blocked_by
            .iter()
            .all(|task_id| status_by_id.get(task_id) == Some(&AgentTaskStatus::Completed))
}

fn row_to_task_list(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<TaskList> {
    let metadata_json = row
        .try_get::<Option<String>, _>("metadata_json")?
        .map(|value| serde_json::from_str(&value))
        .transpose()?;
    Ok(TaskList {
        id: row.try_get("id")?,
        scope: TaskListScope::from_storage_key(row.try_get::<String, _>("scope")?.as_str()),
        title: row.try_get("title")?,
        workspace_id: row.try_get("workspace_id")?,
        created_at: parse_utc(row.try_get::<String, _>("created_at")?.as_str())?,
        updated_at: parse_utc(row.try_get::<String, _>("updated_at")?.as_str())?,
        metadata_json,
    })
}

fn row_to_agent_task(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<AgentTask> {
    let metadata_json = row
        .try_get::<Option<String>, _>("metadata_json")?
        .map(|value| serde_json::from_str(&value))
        .transpose()?;
    let est_minutes: Option<i64> = row.try_get("est_minutes")?;
    Ok(AgentTask {
        task_list_id: row.try_get("task_list_id")?,
        id: row.try_get("id")?,
        subject: row.try_get("subject")?,
        description: row.try_get("description")?,
        active_form: row.try_get("active_form")?,
        owner: row.try_get("owner")?,
        status: AgentTaskStatus::from_str(row.try_get::<String, _>("status")?.as_str())?,
        workspace_id: row.try_get("workspace_id")?,
        source: row.try_get("source")?,
        kind: row.try_get("kind")?,
        legacy_id: row.try_get("legacy_id")?,
        est_minutes: est_minutes.map(|v| v as u32),
        blocks: Vec::new(),
        blocked_by: Vec::new(),
        metadata_json,
        created_at: parse_utc(row.try_get::<String, _>("created_at")?.as_str())?,
        updated_at: parse_utc(row.try_get::<String, _>("updated_at")?.as_str())?,
    })
}

fn parse_utc(value: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

/// Update a task's status by its ID across all task lists.
/// Used for agent run writeback when the run's metadata contains a task_id.
pub async fn update_task_status_by_id(
    task_id: &str,
    status: &str,
    output_summary: Option<&str>,
) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let now = Utc::now();

    // Build metadata update with output_summary if provided
    let metadata_update = if let Some(summary) = output_summary {
        serde_json::json!({ "last_run_output": summary }).to_string()
    } else {
        // Keep existing metadata
        let existing: Option<String> =
            sqlx::query_scalar("SELECT metadata_json FROM agent_tasklist_items WHERE id = ?1")
                .bind(task_id)
                .fetch_optional(&pool)
                .await?;
        existing.unwrap_or_else(|| "{}".to_string())
    };

    let result = sqlx::query(
        r#"
        UPDATE agent_tasklist_items
        SET status = ?1,
            updated_at = ?2,
            metadata_json = json_patch(COALESCE(metadata_json, '{}'), ?3)
        WHERE id = ?4
        "#,
    )
    .bind(status)
    .bind(now.to_rfc3339())
    .bind(&metadata_update)
    .bind(task_id)
    .execute(&pool)
    .await?;

    if result.rows_affected() == 0 {
        // Task not found — not an error, the task may have been deleted
        return Ok(());
    }

    Ok(())
}

/// List all pending or in-progress agent tasks across all task lists,
/// regardless of source. Returns items that need user attention.
pub async fn list_pending_review() -> anyhow::Result<Vec<AgentTask>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT task_list_id, id, subject, description, active_form, owner,
               status, workspace_id, source, kind, legacy_id, est_minutes,
               created_at, updated_at, metadata_json
        FROM agent_tasklist_items
        WHERE status IN ('pending', 'in_progress')
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(&pool)
    .await?;

    let mut tasks = rows
        .into_iter()
        .map(row_to_agent_task)
        .collect::<anyhow::Result<Vec<_>>>()?;

    // Attach dependencies for each task list
    let task_list_ids: Vec<String> = tasks
        .iter()
        .map(|t| t.task_list_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    for task_list_id in &task_list_ids {
        let mut list_tasks: Vec<&mut AgentTask> = tasks
            .iter_mut()
            .filter(|t| t.task_list_id == *task_list_id)
            .collect();
        if !list_tasks.is_empty() {
            let dep_rows = sqlx::query(
                r#"
                SELECT from_task_id, to_task_id
                FROM task_dependencies
                WHERE task_list_id = ?1
                "#,
            )
            .bind(task_list_id)
            .fetch_all(&pool)
            .await?;
            let mut task_slices: Vec<AgentTask> = list_tasks.iter().map(|t| (*t).clone()).collect();
            apply_dependency_rows(dep_rows, &mut task_slices)?;
            for (i, task) in task_slices.into_iter().enumerate() {
                list_tasks[i].blocks = task.blocks;
                list_tasks[i].blocked_by = task.blocked_by;
            }
        }
    }

    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;
    use std::path::PathBuf;

    #[tokio::test]
    async fn create_list_get_and_update_task() {
        let _root = TestRoot::new();
        let created = create_task(TaskCreateInput {
            subject: "整理当前方案文档结构".to_string(),
            description: "读取当前文档，找出缺失步骤。".to_string(),
            active_form: Some("整理方案文档".to_string()),
            metadata: Some(serde_json::json!({ "source": "document_assistant" })),
            ..Default::default()
        })
        .await
        .expect("create task");

        assert_eq!(created.id, "1");
        assert_eq!(created.status, AgentTaskStatus::Pending);

        let listed = list_tasks(TaskListFilter::default())
            .await
            .expect("list tasks");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].subject, "整理当前方案文档结构");

        let updated = update_task(TaskUpdateInput {
            task_id: created.id.clone(),
            status: Some(AgentTaskStatus::InProgress),
            owner: Some("pet".to_string()),
            metadata: Some(serde_json::json!({ "last_reason": "用户批准开始处理" })),
            ..Default::default()
        })
        .await
        .expect("update task");
        assert_eq!(updated.status, AgentTaskStatus::InProgress);
        assert_eq!(updated.owner.as_deref(), Some("pet"));

        let got = get_task(None, &created.id).await.expect("get task");
        assert_eq!(
            got.metadata_json.unwrap()["last_reason"],
            "用户批准开始处理"
        );
    }

    #[tokio::test]
    async fn dependencies_control_available_tasks_and_claim() {
        let _root = TestRoot::new();
        let blocker = create_task(TaskCreateInput {
            subject: "先读文档".to_string(),
            ..Default::default()
        })
        .await
        .expect("create blocker");
        let blocked = create_task(TaskCreateInput {
            subject: "再修改文档".to_string(),
            blocked_by: vec![blocker.id.clone()],
            ..Default::default()
        })
        .await
        .expect("create blocked");

        let available = list_tasks(TaskListFilter {
            available_only: true,
            ..Default::default()
        })
        .await
        .expect("list available");
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].id, blocker.id);

        let claim_result = claim_task(None, &blocked.id, "agent-a").await;
        assert!(claim_result.is_err());

        update_task(TaskUpdateInput {
            task_id: blocker.id.clone(),
            status: Some(AgentTaskStatus::Completed),
            ..Default::default()
        })
        .await
        .expect("complete blocker");

        let claimed = claim_task(None, &blocked.id, "agent-a")
            .await
            .expect("claim unblocked task");
        assert_eq!(claimed.owner.as_deref(), Some("agent-a"));
    }

    #[tokio::test]
    async fn migrates_legacy_tasks_idempotently() {
        let _root = TestRoot::new();
        crate::tasks::add(crate::tasks::Task {
            id: "t-legacy-001".to_string(),
            source: "claude".to_string(),
            kind: "review-doc".to_string(),
            artifact: crate::tasks::Artifact {
                file: Some(PathBuf::from("docs/a.md")),
                anchor: Some("section-1".to_string()),
            },
            summary_ref: Some("summaries/a.md".to_string()),
            est_minutes: Some(8),
            focus_hint: Some("check current doc".to_string()),
            status: crate::tasks::TaskStatus::Passed,
            created_at: Utc::now(),
            session_id: Some("s1".to_string()),
            terminal_id: Some("term1".to_string()),
            cwd: Some(PathBuf::from("I:/personal-agent")),
            current_request: Some("review the plan".to_string()),
            last_output_summary: Some("done".to_string()),
            last_event_at: Some(Utc::now()),
            permission_summary: Some("read only".to_string()),
        })
        .await
        .expect("add legacy task");

        assert_eq!(migrate_legacy_tasks(None).await.expect("migrate"), 1);
        assert_eq!(migrate_legacy_tasks(None).await.expect("migrate again"), 0);

        let migrated = get_task(None, "t-legacy-001")
            .await
            .expect("get migrated task");
        assert_eq!(migrated.status, AgentTaskStatus::Completed);
        assert_eq!(migrated.source, "hook");
        assert_eq!(migrated.kind, "review-doc");
        assert_eq!(migrated.legacy_id.as_deref(), Some("t-legacy-001"));
        let metadata = migrated.metadata_json.expect("metadata");
        assert_eq!(metadata["review_result"], "passed");
        assert_eq!(metadata["artifact_file"], "docs/a.md");
    }

    #[tokio::test]
    async fn create_task_with_source_agent() {
        let _root = TestRoot::new();
        let created = create_task(TaskCreateInput {
            subject: "Agent-generated task".to_string(),
            source: Some("agent".to_string()),
            ..Default::default()
        })
        .await
        .expect("create task with source=agent");
        assert_eq!(created.source, "agent");
        assert_eq!(created.status, AgentTaskStatus::Pending);

        let fetched = get_task(None, &created.id).await.expect("get task");
        assert_eq!(fetched.source, "agent");
    }

    #[tokio::test]
    async fn create_task_with_source_hook() {
        let _root = TestRoot::new();
        let created = create_task(TaskCreateInput {
            subject: "Hook-triggered task".to_string(),
            source: Some("hook".to_string()),
            legacy_id: Some("legacy-123".to_string()),
            ..Default::default()
        })
        .await
        .expect("create task with source=hook");
        assert_eq!(created.source, "hook");
        assert_eq!(created.legacy_id.as_deref(), Some("legacy-123"));

        let fetched = get_task(None, &created.id).await.expect("get task");
        assert_eq!(fetched.source, "hook");
        assert_eq!(fetched.legacy_id.as_deref(), Some("legacy-123"));
    }

    #[tokio::test]
    async fn create_task_with_source_proposal() {
        let _root = TestRoot::new();
        let created = create_task(TaskCreateInput {
            subject: "Proposal-backed task".to_string(),
            source: Some("proposal".to_string()),
            ..Default::default()
        })
        .await
        .expect("create task with source=proposal");
        assert_eq!(created.source, "proposal");

        let fetched = get_task(None, &created.id).await.expect("get task");
        assert_eq!(fetched.source, "proposal");
    }

    #[tokio::test]
    async fn list_pending_review_returns_tasks_from_all_sources() {
        let _root = TestRoot::new();

        // Create tasks from different sources
        let t1 = create_task(TaskCreateInput {
            subject: "Manual task".to_string(),
            source: Some("manual".to_string()),
            ..Default::default()
        })
        .await
        .expect("create manual task");

        let _t2 = create_task(TaskCreateInput {
            subject: "Agent task".to_string(),
            source: Some("agent".to_string()),
            ..Default::default()
        })
        .await
        .expect("create agent task");

        let _t3 = create_task(TaskCreateInput {
            subject: "Hook task".to_string(),
            source: Some("hook".to_string()),
            ..Default::default()
        })
        .await
        .expect("create hook task");

        let _t4 = create_task(TaskCreateInput {
            subject: "Proposal task".to_string(),
            source: Some("proposal".to_string()),
            ..Default::default()
        })
        .await
        .expect("create proposal task");

        // Complete one task -- it should not appear in pending_review
        update_task(TaskUpdateInput {
            task_id: t1.id.clone(),
            status: Some(AgentTaskStatus::Completed),
            ..Default::default()
        })
        .await
        .expect("complete manual task");

        let pending = list_pending_review().await.expect("list pending review");
        assert_eq!(pending.len(), 3);
        let sources: Vec<&str> = pending.iter().map(|t| t.source.as_str()).collect();
        assert!(sources.contains(&"agent"));
        assert!(sources.contains(&"hook"));
        assert!(sources.contains(&"proposal"));
        assert!(!sources.contains(&"manual")); // completed
    }

    #[tokio::test]
    async fn legacy_migration_creates_agent_task_with_hook_source() {
        let _root = TestRoot::new();
        crate::tasks::add(crate::tasks::Task {
            id: "t-migrate-001".to_string(),
            source: "codex".to_string(),
            kind: "fix-bug".to_string(),
            artifact: crate::tasks::Artifact {
                file: Some(PathBuf::from("src/main.rs")),
                anchor: Some("fn main".to_string()),
            },
            summary_ref: Some("summaries/fix.md".to_string()),
            est_minutes: Some(15),
            focus_hint: Some("fix the crash".to_string()),
            status: crate::tasks::TaskStatus::Pending,
            created_at: Utc::now(),
            session_id: Some("s2".to_string()),
            terminal_id: None,
            cwd: Some(PathBuf::from("I:/personal-agent")),
            current_request: Some("fix the null pointer".to_string()),
            last_output_summary: None,
            last_event_at: None,
            permission_summary: None,
        })
        .await
        .expect("add legacy task");

        let count = migrate_legacy_tasks(None).await.expect("migrate");
        assert_eq!(count, 1);

        let migrated = get_task(None, "t-migrate-001").await.expect("get migrated");
        assert_eq!(migrated.source, "hook");
        assert_eq!(migrated.kind, "fix-bug");
        assert_eq!(migrated.legacy_id.as_deref(), Some("t-migrate-001"));
        assert_eq!(migrated.status, AgentTaskStatus::Pending);
        assert_eq!(migrated.subject, "fix the null pointer");

        // Idempotent: migrating again should not duplicate
        let count2 = migrate_legacy_tasks(None).await.expect("migrate again");
        assert_eq!(count2, 0);
    }

    #[tokio::test]
    async fn budget_filter_returns_tasks_within_budget() {
        let _root = TestRoot::new();
        create_task(TaskCreateInput {
            subject: "quick task".to_string(),
            est_minutes: Some(10),
            ..Default::default()
        })
        .await
        .expect("create task within budget");

        create_task(TaskCreateInput {
            subject: "medium task".to_string(),
            est_minutes: Some(25),
            ..Default::default()
        })
        .await
        .expect("create task within budget");

        let tasks = list_tasks_by_budget(30, TaskListFilter::default())
            .await
            .expect("list by budget");
        assert_eq!(tasks.len(), 2);
        let subjects: Vec<&str> = tasks.iter().map(|t| t.subject.as_str()).collect();
        assert!(subjects.contains(&"quick task"));
        assert!(subjects.contains(&"medium task"));
    }

    #[tokio::test]
    async fn budget_filter_excludes_tasks_exceeding_budget() {
        let _root = TestRoot::new();
        create_task(TaskCreateInput {
            subject: "quick task".to_string(),
            est_minutes: Some(5),
            ..Default::default()
        })
        .await
        .expect("create quick task");

        create_task(TaskCreateInput {
            subject: "huge task".to_string(),
            est_minutes: Some(120),
            ..Default::default()
        })
        .await
        .expect("create huge task");

        let tasks = list_tasks_by_budget(30, TaskListFilter::default())
            .await
            .expect("list by budget");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].subject, "quick task");
    }

    #[tokio::test]
    async fn budget_filter_includes_tasks_with_null_est_minutes() {
        let _root = TestRoot::new();
        create_task(TaskCreateInput {
            subject: "estimated task".to_string(),
            est_minutes: Some(10),
            ..Default::default()
        })
        .await
        .expect("create estimated task");

        create_task(TaskCreateInput {
            subject: "unestimated task".to_string(),
            ..Default::default()
        })
        .await
        .expect("create unestimated task");

        let tasks = list_tasks_by_budget(30, TaskListFilter::default())
            .await
            .expect("list by budget");
        assert_eq!(tasks.len(), 2);
        let subjects: Vec<&str> = tasks.iter().map(|t| t.subject.as_str()).collect();
        assert!(subjects.contains(&"estimated task"));
        assert!(subjects.contains(&"unestimated task"));
    }

    #[tokio::test]
    async fn budget_filter_sorts_quick_wins_first_then_by_created() {
        let _root = TestRoot::new();
        // Create tasks with different est_minutes; they get sequential IDs so created_at
        // ordering follows creation order within each est_minutes bucket.
        create_task(TaskCreateInput {
            subject: "big task".to_string(),
            est_minutes: Some(60),
            ..Default::default()
        })
        .await
        .expect("create big");

        create_task(TaskCreateInput {
            subject: "small task".to_string(),
            est_minutes: Some(5),
            ..Default::default()
        })
        .await
        .expect("create small");

        create_task(TaskCreateInput {
            subject: "medium task".to_string(),
            est_minutes: Some(20),
            ..Default::default()
        })
        .await
        .expect("create medium");

        create_task(TaskCreateInput {
            subject: "no estimate".to_string(),
            ..Default::default()
        })
        .await
        .expect("create no estimate");

        let tasks = list_tasks_by_budget(120, TaskListFilter::default())
            .await
            .expect("list by budget");
        assert_eq!(tasks.len(), 4);

        // Quick wins (low est_minutes) come first, nulls last
        assert_eq!(tasks[0].subject, "small task");
        assert_eq!(tasks[1].subject, "medium task");
        assert_eq!(tasks[2].subject, "big task");
        assert_eq!(tasks[3].subject, "no estimate");
        assert!(tasks[3].est_minutes.is_none());
    }
}
