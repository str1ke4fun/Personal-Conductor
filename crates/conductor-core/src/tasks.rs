use crate::{db, lock::with_lock, paths::Paths};
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::{collections::HashSet, path::PathBuf};
use tokio::{
    fs,
    io::{AsyncWriteExt, BufWriter},
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Artifact {
    pub file: Option<PathBuf>,
    pub anchor: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Passed,
    Rejected,
    Skipped,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Passed => "passed",
            Self::Rejected => "rejected",
            Self::Skipped => "skipped",
        }
    }

    pub fn from_db(value: &str) -> anyhow::Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "passed" => Ok(Self::Passed),
            "rejected" => Ok(Self::Rejected),
            "skipped" => Ok(Self::Skipped),
            other => bail!("unknown task status: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Task {
    pub id: String,
    pub source: String,
    pub kind: String,
    pub artifact: Artifact,
    pub summary_ref: Option<String>,
    pub est_minutes: Option<u32>,
    pub focus_hint: Option<String>,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_output_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_event_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_summary: Option<String>,
}

impl Task {
    pub fn artifact_label(&self) -> String {
        self.artifact
            .file
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .or_else(|| {
                self.artifact
                    .file
                    .as_ref()
                    .map(|path| path.display().to_string())
            })
            .unwrap_or_else(|| self.kind.clone())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TasksFile {
    pub updated_at: DateTime<Utc>,
    pub horizon_minutes: u32,
    pub tasks: Vec<Task>,
}

impl TasksFile {
    pub fn pending(&self) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Pending)
            .collect()
    }

    pub fn in_progress(&self) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|task| task.status == TaskStatus::InProgress)
            .collect()
    }
}

impl Default for TasksFile {
    fn default() -> Self {
        Self {
            updated_at: Utc::now(),
            horizon_minutes: 60,
            tasks: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskActivityStats {
    pub pending_total: usize,
    pub in_progress_total: usize,
    pub active_hook_sessions: usize,
    pub pending_hook_reviews: usize,
    pub pending_other: usize,
}

pub fn activity_stats(tasks: &[Task]) -> TaskActivityStats {
    let pending_total = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Pending)
        .count();
    let in_progress_total = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::InProgress)
        .count();

    let mut active_hook_sessions = HashSet::new();
    for task in tasks.iter().filter(|task| {
        task.status == TaskStatus::InProgress && is_hook_source(task.source.as_str())
    }) {
        active_hook_sessions.insert(
            hook_identity_key(task).unwrap_or_else(|| format!("{}:task:{}", task.source, task.id)),
        );
    }

    let mut pending_hook_review_keys = HashSet::new();
    let mut pending_hook_reviews = 0;
    let mut pending_other = 0;
    for task in tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Pending)
    {
        if is_hook_review_task(task) {
            if let Some(key) = hook_identity_key(task) {
                if pending_hook_review_keys.insert(key) {
                    pending_hook_reviews += 1;
                }
            } else {
                pending_hook_reviews += 1;
            }
        } else {
            pending_other += 1;
        }
    }

    TaskActivityStats {
        pending_total,
        in_progress_total,
        active_hook_sessions: active_hook_sessions.len(),
        pending_hook_reviews,
        pending_other,
    }
}

pub fn is_hook_review_task(task: &Task) -> bool {
    task.status == TaskStatus::Pending
        && is_hook_source(task.source.as_str())
        && (hook_identity_key(task).is_some()
            || task.current_request.is_some()
            || task.last_output_summary.is_some()
            || task.last_event_at.is_some()
            || task.permission_summary.is_some())
}

fn is_hook_source(source: &str) -> bool {
    matches!(source, "claude" | "codex")
}

fn hook_identity_key(task: &Task) -> Option<String> {
    if let Some(session_id) = task
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(format!("{}:session:{session_id}", task.source));
    }

    if let Some(terminal_id) = task
        .terminal_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let cwd = task
            .cwd
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        return Some(format!("{}:terminal:{terminal_id}:{cwd}", task.source));
    }

    None
}

pub async fn load() -> anyhow::Result<TasksFile> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, source, kind, artifact_file, artifact_anchor, summary_ref,
               est_minutes, focus_hint, status, created_at, session_id,
               terminal_id, cwd, current_request, last_output_summary,
               last_event_at, permission_summary
        FROM tasks
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(&pool)
    .await?;

    let mut tasks = Vec::with_capacity(rows.len());
    let mut updated_at = Utc::now();
    for row in rows {
        let created_at = parse_utc(row.try_get::<String, _>("created_at")?.as_str())?;
        updated_at = updated_at.max(created_at);
        tasks.push(Task {
            id: row.try_get("id")?,
            source: row.try_get("source")?,
            kind: row.try_get("kind")?,
            artifact: Artifact {
                file: row
                    .try_get::<Option<String>, _>("artifact_file")?
                    .map(PathBuf::from),
                anchor: row.try_get("artifact_anchor")?,
            },
            summary_ref: row.try_get("summary_ref")?,
            est_minutes: row
                .try_get::<Option<i64>, _>("est_minutes")?
                .map(|value| value as u32),
            focus_hint: row.try_get("focus_hint")?,
            status: TaskStatus::from_db(row.try_get::<String, _>("status")?.as_str())?,
            created_at,
            session_id: row.try_get("session_id")?,
            terminal_id: row.try_get("terminal_id")?,
            cwd: row.try_get::<Option<String>, _>("cwd")?.map(PathBuf::from),
            current_request: row.try_get("current_request")?,
            last_output_summary: row.try_get("last_output_summary")?,
            last_event_at: row
                .try_get::<Option<String>, _>("last_event_at")?
                .as_deref()
                .map(parse_utc)
                .transpose()?,
            permission_summary: row.try_get("permission_summary")?,
        });
    }

    Ok(TasksFile {
        updated_at,
        horizon_minutes: 60,
        tasks,
    })
}

pub async fn load_legacy_json() -> anyhow::Result<TasksFile> {
    match fs::read_to_string(Paths::tasks_json()).await {
        Ok(content) if content.trim().is_empty() => Ok(TasksFile::default()),
        Ok(content) => serde_json::from_str(&content)
            .with_context(|| format!("parse tasks file {}", Paths::tasks_json().display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(TasksFile::default()),
        Err(err) => {
            Err(err).with_context(|| format!("read tasks file {}", Paths::tasks_json().display()))
        }
    }
}

pub async fn add(task: Task) -> anyhow::Result<()> {
    let lock_path = Paths::conductor_sqlite();
    with_lock(&lock_path, || async move {
        let pool = db::pool().await?;
        upsert_task(&pool, &task, Utc::now()).await?;
        render_markdown().await?;
        touch_signal_file().await;
        Ok(())
    })
    .await
}

pub async fn update<F>(id: &str, f: F) -> anyhow::Result<()>
where
    F: FnOnce(&mut Task),
{
    let id = id.to_string();
    let lock_path = Paths::conductor_sqlite();
    with_lock(&lock_path, || async move {
        let pool = db::pool().await?;
        let mut file = load().await?;
        let Some(task) = file.tasks.iter_mut().find(|task| task.id == id) else {
            bail!("task not found: {id}");
        };
        f(task);
        upsert_task(&pool, task, Utc::now()).await?;
        render_markdown().await?;
        touch_signal_file().await;
        Ok(())
    })
    .await
}

pub async fn render_markdown() -> anyhow::Result<()> {
    let file = load().await?;
    render_markdown_from(&file).await
}

pub async fn next_id() -> anyhow::Result<String> {
    let lock_path = Paths::conductor_sqlite();
    with_lock(&lock_path, || async move {
        let file = load().await?;
        let date = Utc::now().format("%Y%m%d").to_string();
        let prefix = format!("t-{date}-");
        let next = file
            .tasks
            .iter()
            .filter_map(|task| task.id.strip_prefix(&prefix))
            .filter_map(|suffix| suffix.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        Ok(format!("{prefix}{next:03}"))
    })
    .await
}

pub async fn migrate_legacy_json() -> anyhow::Result<usize> {
    let legacy_path = Paths::tasks_json();
    if !fs::try_exists(&legacy_path).await? {
        render_markdown().await?;
        return Ok(0);
    }
    let legacy = load_legacy_json().await?;
    let count = legacy.tasks.len();
    for task in legacy.tasks {
        add(task).await?;
    }
    let bak = legacy_path.with_extension("json.bak");
    fs::rename(&legacy_path, &bak)
        .await
        .with_context(|| format!("rename {} to {}", legacy_path.display(), bak.display()))?;
    render_markdown().await?;
    Ok(count)
}

/// One-time migration: create AgentTasks in the agent_tasklist_items table for each
/// legacy task stored in the tasks table. Each migrated task gets
/// source="hook" and legacy_id set to the original task id.
/// Returns the number of tasks migrated (idempotent via ON CONFLICT).
pub async fn migrate_to_agent_tasks(task_list_id: Option<&str>) -> anyhow::Result<usize> {
    crate::tasklist::migrate_legacy_tasks(task_list_id).await
}

async fn upsert_task(
    pool: &sqlx::SqlitePool,
    task: &Task,
    updated_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO tasks (
            id, source, kind, artifact_file, artifact_anchor, summary_ref,
            est_minutes, focus_hint, status, created_at, updated_at,
            session_id, terminal_id, cwd, current_request, last_output_summary,
            last_event_at, permission_summary
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
        ON CONFLICT(id) DO UPDATE SET
            source = excluded.source,
            kind = excluded.kind,
            artifact_file = excluded.artifact_file,
            artifact_anchor = excluded.artifact_anchor,
            summary_ref = excluded.summary_ref,
            est_minutes = excluded.est_minutes,
            focus_hint = excluded.focus_hint,
            status = excluded.status,
            updated_at = excluded.updated_at,
            session_id = excluded.session_id,
            terminal_id = excluded.terminal_id,
            cwd = excluded.cwd,
            current_request = excluded.current_request,
            last_output_summary = excluded.last_output_summary,
            last_event_at = excluded.last_event_at,
            permission_summary = excluded.permission_summary
        "#,
    )
    .bind(&task.id)
    .bind(&task.source)
    .bind(&task.kind)
    .bind(
        task.artifact
            .file
            .as_ref()
            .map(|path| path.display().to_string()),
    )
    .bind(&task.artifact.anchor)
    .bind(&task.summary_ref)
    .bind(task.est_minutes.map(|value| value as i64))
    .bind(&task.focus_hint)
    .bind(task.status.as_str())
    .bind(task.created_at.to_rfc3339())
    .bind(updated_at.to_rfc3339())
    .bind(&task.session_id)
    .bind(&task.terminal_id)
    .bind(task.cwd.as_ref().map(|path| path.display().to_string()))
    .bind(&task.current_request)
    .bind(&task.last_output_summary)
    .bind(task.last_event_at.map(|value| value.to_rfc3339()))
    .bind(&task.permission_summary)
    .execute(pool)
    .await?;
    Ok(())
}

async fn render_markdown_from(file: &TasksFile) -> anyhow::Result<()> {
    let path = Paths::tasks_md();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let mut markdown = format!(
        "# Current Review Queue - {} - {} minute window\n\n",
        file.updated_at.to_rfc3339(),
        file.horizon_minutes
    );
    for task in &file.tasks {
        let checkbox = if matches!(
            task.status,
            TaskStatus::Passed | TaskStatus::Rejected | TaskStatus::Skipped
        ) {
            "x"
        } else {
            " "
        };
        let est = task
            .est_minutes
            .map(|minutes| minutes.to_string())
            .unwrap_or_else(|| "?".to_string());
        let artifact = task
            .artifact
            .file
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "(no file)".to_string());
        markdown.push_str(&format!(
            "- [{checkbox}] [{est}min] {} - {}\n",
            task.kind, artifact
        ));
        if let Some(hint) = &task.focus_hint {
            markdown.push_str(&format!("      -> {hint}\n"));
        }
        if let Some(request) = &task.current_request {
            markdown.push_str(&format!("      -> request: {request}\n"));
        }
        if let Some(output) = &task.last_output_summary {
            markdown.push_str(&format!("      -> done: {output}\n"));
        }
        if let Some(cwd) = &task.cwd {
            markdown.push_str(&format!("      -> cwd: {}\n", cwd.display()));
        }
        if let Some(session_id) = &task.session_id {
            markdown.push_str(&format!("      -> session: {session_id}\n"));
        }
        if let Some(terminal_id) = &task.terminal_id {
            markdown.push_str(&format!("      -> terminal: {terminal_id}\n"));
        }
        if let Some(summary_ref) = &task.summary_ref {
            markdown.push_str(&format!("      -> summary: {summary_ref}\n"));
        }
    }
    let mut writer = BufWriter::new(fs::File::create(&path).await?);
    writer.write_all(markdown.as_bytes()).await?;
    writer.flush().await?;
    writer.get_ref().sync_all().await?;
    Ok(())
}

pub async fn touch_signal_file() {
    let path = Paths::task_signal();
    if let Err(err) = async {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, Utc::now().to_rfc3339()).await
    }
    .await
    {
        tracing::debug!("failed to write task signal file: {err}");
    }
}

fn parse_utc(value: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    fn task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            source: "claude".to_string(),
            kind: "review-doc".to_string(),
            artifact: Artifact {
                file: Some(PathBuf::from(format!("docs/{id}.md"))),
                anchor: None,
            },
            summary_ref: Some(format!("summaries/{id}.md")),
            est_minutes: Some(5),
            focus_hint: Some("check wording".to_string()),
            status: TaskStatus::Pending,
            created_at: Utc::now(),
            session_id: None,
            terminal_id: None,
            cwd: None,
            current_request: None,
            last_output_summary: None,
            last_event_at: None,
            permission_summary: None,
        }
    }

    #[tokio::test]
    async fn add_load_and_render_three_tasks() {
        let _root = TestRoot::new();

        add(task("t-20260518-001")).await.expect("add task 1");
        add(task("t-20260518-002")).await.expect("add task 2");
        add(task("t-20260518-003")).await.expect("add task 3");

        let loaded = load().await.expect("load tasks");
        assert_eq!(loaded.tasks.len(), 3);

        let markdown = fs::read_to_string(Paths::tasks_md())
            .await
            .expect("read tasks markdown");
        assert_eq!(markdown.matches("- [ ] [5min] review-doc").count(), 3);
        assert!(markdown.contains("-> summary: summaries/t-20260518-001.md"));
    }

    #[tokio::test]
    async fn update_passed_task_renders_checked_markdown() {
        let _root = TestRoot::new();
        let id = "t-20260518-001";
        add(task(id)).await.expect("add task");

        update(id, |task| task.status = TaskStatus::Passed)
            .await
            .expect("update task");

        let loaded = load().await.expect("load tasks");
        assert_eq!(loaded.tasks[0].status, TaskStatus::Passed);
        let markdown = fs::read_to_string(Paths::tasks_md())
            .await
            .expect("read tasks markdown");
        assert!(markdown.contains("- [x] [5min] review-doc - docs/t-20260518-001.md"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_adds_are_preserved() {
        let _root = TestRoot::new();
        let mut handles = Vec::new();
        for i in 0..100 {
            handles.push(tokio::spawn(async move {
                add(task(&format!("t-20260518-{i:03}"))).await
            }));
        }
        for handle in handles {
            handle.await.expect("join").expect("add");
        }
        assert_eq!(load().await.expect("load").tasks.len(), 100);
    }

    #[tokio::test]
    async fn signal_file_written_after_add_and_update() {
        let _root = TestRoot::new();
        let id = "t-20260518-001";

        assert!(!fs::try_exists(Paths::task_signal()).await.unwrap_or(false));

        add(task(id)).await.expect("add task");
        let content_after_add = fs::read_to_string(Paths::task_signal())
            .await
            .expect("read signal file after add");
        assert!(
            DateTime::parse_from_rfc3339(&content_after_add).is_ok(),
            "signal file should contain valid RFC3339 timestamp after add"
        );

        update(id, |task| task.status = TaskStatus::Passed)
            .await
            .expect("update task");
        let content_after_update = fs::read_to_string(Paths::task_signal())
            .await
            .expect("read signal file after update");
        assert!(
            DateTime::parse_from_rfc3339(&content_after_update).is_ok(),
            "signal file should contain valid RFC3339 timestamp after update"
        );
    }
}
