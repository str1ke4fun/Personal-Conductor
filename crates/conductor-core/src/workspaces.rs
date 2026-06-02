use crate::db;
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    Code,
    Document,
    Office,
    Notes,
    Generic,
}

impl WorkspaceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Document => "document",
            Self::Office => "office",
            Self::Notes => "notes",
            Self::Generic => "generic",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "code" => Ok(Self::Code),
            "document" => Ok(Self::Document),
            "office" => Ok(Self::Office),
            "notes" => Ok(Self::Notes),
            "generic" => Ok(Self::Generic),
            other => bail!("unknown workspace kind: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Trusted,
    AskWrite,
    ReadOnly,
    Untrusted,
}

impl TrustLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::AskWrite => "ask_write",
            Self::ReadOnly => "read_only",
            Self::Untrusted => "untrusted",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "trusted" => Ok(Self::Trusted),
            "ask_write" => Ok(Self::AskWrite),
            "read_only" => Ok(Self::ReadOnly),
            "untrusted" => Ok(Self::Untrusted),
            other => bail!("unknown trust level: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Workspace {
    pub id: String,
    pub root: PathBuf,
    pub name: String,
    pub kind: WorkspaceKind,
    pub trust_level: TrustLevel,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_active_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

impl Workspace {
    pub fn new(root: PathBuf, kind: WorkspaceKind) -> Self {
        let now = Utc::now();
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| "unnamed")
            .to_string();
        Self {
            id: format!("ws-{}-{}", now.format("%Y%m%d"), name),
            root,
            name,
            kind,
            trust_level: TrustLevel::AskWrite,
            created_at: now,
            updated_at: now,
            last_active_at: None,
            metadata: serde_json::json!({}),
        }
    }
}

pub async fn create(workspace: Workspace) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        INSERT INTO workspaces (
            id, root, name, kind, trust_level, 
            created_at, updated_at, last_active_at, metadata_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(id) DO UPDATE SET
            root = excluded.root,
            name = excluded.name,
            kind = excluded.kind,
            trust_level = excluded.trust_level,
            updated_at = excluded.updated_at,
            last_active_at = excluded.last_active_at,
            metadata_json = excluded.metadata_json
        "#,
    )
    .bind(&workspace.id)
    .bind(workspace.root.display().to_string())
    .bind(&workspace.name)
    .bind(workspace.kind.as_str())
    .bind(workspace.trust_level.as_str())
    .bind(workspace.created_at.to_rfc3339())
    .bind(workspace.updated_at.to_rfc3339())
    .bind(workspace.last_active_at.map(|dt| dt.to_rfc3339()))
    .bind(serde_json::to_string(&workspace.metadata)?)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn get(id: &str) -> anyhow::Result<Workspace> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, root, name, kind, trust_level, 
               created_at, updated_at, last_active_at, metadata_json
        FROM workspaces
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .with_context(|| format!("workspace not found: {id}"))?;
    Ok(row_to_workspace(row)?)
}

pub async fn get_by_root(root: &Path) -> anyhow::Result<Option<Workspace>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, root, name, kind, trust_level, 
               created_at, updated_at, last_active_at, metadata_json
        FROM workspaces
        WHERE root = ?1
        "#,
    )
    .bind(root.display().to_string())
    .fetch_optional(&pool)
    .await?;
    row.map(row_to_workspace).transpose()
}

/// Find or create a workspace for the given root path.
/// If a workspace already exists for this root, return it.
/// Otherwise, create a new one with `Trusted` trust level.
pub async fn ensure_for_root(root: &Path) -> anyhow::Result<Workspace> {
    if let Some(ws) = get_by_root(root).await? {
        return Ok(ws);
    }

    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "default".to_string());

    let workspace = Workspace {
        id: uuid::Uuid::new_v4().to_string(),
        root: root.to_path_buf(),
        name,
        kind: WorkspaceKind::Code,
        trust_level: TrustLevel::Trusted,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        last_active_at: None,
        metadata: serde_json::json!({}),
    };

    create(workspace.clone()).await?;
    Ok(workspace)
}

pub fn normalize_root(root: &Path) -> anyhow::Result<PathBuf> {
    if root.exists() {
        let canonical = root.canonicalize()?;
        // On Windows, canonicalize() adds \\?\ prefix which displays as garbled text.
        // Strip it to get a normal path like D:\Projects\demo.
        #[cfg(windows)]
        {
            let s = canonical.to_string_lossy();
            if let Some(stripped) = s.strip_prefix(r"\\?\") {
                return Ok(PathBuf::from(stripped));
            }
        }
        return Ok(canonical);
    }
    if !root.is_absolute() {
        bail!("workspace root must be an absolute path");
    }
    Ok(root.to_path_buf())
}

pub async fn create_or_attach(
    root: &Path,
    name: Option<String>,
    kind: Option<WorkspaceKind>,
) -> anyhow::Result<Workspace> {
    let normalized = normalize_root(root)?;
    if !normalized.exists() {
        std::fs::create_dir_all(&normalized)?;
    }

    if let Some(existing) = get_by_root(&normalized).await? {
        return Ok(existing);
    }

    let now = Utc::now();
    let workspace = Workspace {
        id: uuid::Uuid::new_v4().to_string(),
        root: normalized.clone(),
        name: name.unwrap_or_else(|| {
            normalized
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("workspace")
                .to_string()
        }),
        kind: kind.unwrap_or_else(|| infer_kind_from_path(&normalized)),
        trust_level: TrustLevel::Trusted,
        created_at: now,
        updated_at: now,
        last_active_at: Some(now),
        metadata: serde_json::json!({ "source": "chat_workspace_picker" }),
    };

    create(workspace.clone()).await?;
    Ok(workspace)
}

pub async fn list_all() -> anyhow::Result<Vec<Workspace>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, root, name, kind, trust_level, 
               created_at, updated_at, last_active_at, metadata_json
        FROM workspaces
        ORDER BY updated_at DESC
        "#,
    )
    .fetch_all(&pool)
    .await?;
    rows.into_iter().map(row_to_workspace).collect()
}

pub async fn update_last_active(id: &str, timestamp: DateTime<Utc>) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE workspaces
        SET last_active_at = ?1, updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(timestamp.to_rfc3339())
    .bind(timestamp.to_rfc3339())
    .bind(id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn infer_from_cwd(cwd: &Path) -> anyhow::Result<Option<Workspace>> {
    if let Some(existing) = get_by_root(cwd).await? {
        return Ok(Some(existing));
    }

    let kind = infer_kind_from_path(cwd);
    let workspace = Workspace::new(cwd.to_path_buf(), kind);
    create(workspace.clone()).await?;
    Ok(Some(workspace))
}

pub async fn infer_from_process(
    process_name: &str,
    window_title: &str,
) -> anyhow::Result<Option<Workspace>> {
    let kind = infer_kind_from_process(process_name, window_title);

    if let Some(path) = extract_path_from_title(window_title) {
        if let Ok(Some(workspace)) = get_by_root(&path).await {
            return Ok(Some(workspace));
        }
        let workspace = Workspace::new(path, kind);
        create(workspace.clone()).await?;
        return Ok(Some(workspace));
    }

    Ok(None)
}

fn infer_kind_from_path(path: &Path) -> WorkspaceKind {
    let path_str = path.display().to_string().to_lowercase();
    if path_str.contains("code") || path_str.contains("src") || path_str.ends_with(".git") {
        WorkspaceKind::Code
    } else if path_str.contains("doc")
        || path_str.contains("document")
        || path_str.contains("markdown")
    {
        WorkspaceKind::Document
    } else if path_str.contains("ppt") || path_str.contains("excel") || path_str.contains("word") {
        WorkspaceKind::Office
    } else if path_str.contains("note") || path_str.contains("notes") {
        WorkspaceKind::Notes
    } else {
        WorkspaceKind::Generic
    }
}

fn infer_kind_from_process(process_name: &str, window_title: &str) -> WorkspaceKind {
    let process = process_name.to_lowercase();
    let title = window_title.to_lowercase();

    if process.contains("code") || process.contains("cursor") || process.contains("terminal") {
        WorkspaceKind::Code
    } else if process.contains("winword") || process.contains("wps") || title.contains(".docx") {
        WorkspaceKind::Document
    } else if process.contains("powerpnt") || title.contains(".pptx") {
        WorkspaceKind::Office
    } else if process.contains("notepad") || title.contains(".md") {
        WorkspaceKind::Notes
    } else {
        WorkspaceKind::Generic
    }
}

fn extract_path_from_title(title: &str) -> Option<PathBuf> {
    let path_patterns = [
        r"C:\\[^\\]+\\[^\\]+",
        r"I:\\[^\\]+\\[^\\]+",
        r"/[^/]+/[^/]+",
    ];

    for pattern in path_patterns {
        if let Some(captures) = regex::Regex::new(pattern).ok()?.find(title) {
            return Some(PathBuf::from(captures.as_str()));
        }
    }
    None
}

fn row_to_workspace(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<Workspace> {
    let raw_root: String = row.try_get("root")?;
    // Strip \\?\ prefix on Windows for paths stored by canonicalize()
    #[cfg(windows)]
    let raw_root = raw_root
        .strip_prefix(r"\\?\")
        .map(String::from)
        .unwrap_or(raw_root);
    Ok(Workspace {
        id: row.try_get("id")?,
        root: PathBuf::from(raw_root),
        name: row.try_get("name")?,
        kind: WorkspaceKind::from_str(row.try_get::<String, _>("kind")?.as_str())?,
        trust_level: TrustLevel::from_str(row.try_get::<String, _>("trust_level")?.as_str())?,
        created_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("updated_at")?.as_str())?
            .with_timezone(&Utc),
        last_active_at: row
            .try_get::<Option<String>, _>("last_active_at")?
            .as_deref()
            .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
        metadata: serde_json::from_str(row.try_get::<String, _>("metadata_json")?.as_str())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn create_and_get_workspace() {
        let _root = TestRoot::new();

        let workspace = Workspace::new(PathBuf::from("I:/work/project-a"), WorkspaceKind::Code);
        create(workspace.clone()).await.expect("create workspace");

        let retrieved = get(&workspace.id).await.expect("get workspace");
        assert_eq!(retrieved.id, workspace.id);
        assert_eq!(retrieved.name, "project-a");
        assert_eq!(retrieved.kind, WorkspaceKind::Code);
    }

    #[tokio::test]
    async fn infer_from_cwd_creates_workspace() {
        let _root = TestRoot::new();

        let cwd = PathBuf::from("I:/code/my-project");
        let workspace = infer_from_cwd(&cwd).await.expect("infer workspace");

        assert!(workspace.is_some());
        let ws = workspace.unwrap();
        assert_eq!(ws.kind, WorkspaceKind::Code);
        assert_eq!(ws.name, "my-project");
    }

    #[test]
    fn test_infer_kind_from_process() {
        assert_eq!(
            infer_kind_from_process("Code.exe", "main.rs - VS Code"),
            WorkspaceKind::Code
        );
        assert_eq!(
            infer_kind_from_process("WINWORD.EXE", "方案.docx"),
            WorkspaceKind::Document
        );
        assert_eq!(
            infer_kind_from_process("POWERPNT.EXE", "演示文稿.pptx"),
            WorkspaceKind::Office
        );
        assert_eq!(
            infer_kind_from_process("notepad.exe", "notes.md"),
            WorkspaceKind::Notes
        );
        assert_eq!(
            infer_kind_from_process("chrome.exe", "Google"),
            WorkspaceKind::Generic
        );
    }
}
