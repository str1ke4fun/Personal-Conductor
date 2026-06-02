use crate::db;
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

// ── Enums ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    ClaudeP,
    CodexInteractive,
    AgentTeam,
    Review,
}

impl BackendKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClaudeP => "claude_p",
            Self::CodexInteractive => "codex_interactive",
            Self::AgentTeam => "agent_team",
            Self::Review => "review",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "claude_p" => Ok(Self::ClaudeP),
            "codex_interactive" => Ok(Self::CodexInteractive),
            "agent_team" => Ok(Self::AgentTeam),
            "review" => Ok(Self::Review),
            other => bail!("unknown backend kind: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Unknown,
    Healthy,
    Unhealthy,
    Degraded,
}

impl HealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Healthy => "healthy",
            Self::Unhealthy => "unhealthy",
            Self::Degraded => "degraded",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "unknown" => Ok(Self::Unknown),
            "healthy" => Ok(Self::Healthy),
            "unhealthy" => Ok(Self::Unhealthy),
            "degraded" => Ok(Self::Degraded),
            other => bail!("unknown health status: {other}"),
        }
    }
}

// ── Model ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentBackend {
    pub id: String,
    pub name: String,
    pub kind: BackendKind,
    pub executable_path: Option<String>,
    pub default_env_json: Option<serde_json::Value>,
    pub health_check_url: Option<String>,
    pub enabled: bool,
    pub last_health_check_at: Option<DateTime<Utc>>,
    pub health_status: HealthStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── Input ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateBackendInput {
    pub name: String,
    pub kind: BackendKind,
    pub executable_path: Option<String>,
    pub default_env_json: Option<serde_json::Value>,
    pub health_check_url: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UpdateBackendInput {
    pub name: Option<String>,
    pub kind: Option<BackendKind>,
    pub executable_path: Option<Option<String>>,
    pub default_env_json: Option<Option<serde_json::Value>>,
    pub health_check_url: Option<Option<String>>,
    pub enabled: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BackendFilter {
    pub kind: Option<BackendKind>,
    pub enabled: Option<bool>,
    pub health_status: Option<HealthStatus>,
    pub limit: Option<u32>,
}

// ── CRUD ─────────────────────────────────────────────────────────────────

pub async fn create(input: CreateBackendInput) -> anyhow::Result<AgentBackend> {
    if input.name.trim().is_empty() {
        bail!("backend name cannot be empty");
    }

    let id = format!("be-{}", Uuid::new_v4());
    let now = Utc::now();
    let pool = db::pool().await?;

    sqlx::query(
        r#"
        INSERT INTO agent_backends (
            id, name, kind, executable_path, default_env_json,
            health_check_url, enabled, health_status, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
    )
    .bind(&id)
    .bind(&input.name)
    .bind(input.kind.as_str())
    .bind(&input.executable_path)
    .bind(
        input
            .default_env_json
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
    )
    .bind(&input.health_check_url)
    .bind(input.enabled as i64)
    .bind(HealthStatus::Unknown.as_str())
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(&pool)
    .await?;

    Ok(AgentBackend {
        id,
        name: input.name,
        kind: input.kind,
        executable_path: input.executable_path,
        default_env_json: input.default_env_json,
        health_check_url: input.health_check_url,
        enabled: input.enabled,
        last_health_check_at: None,
        health_status: HealthStatus::Unknown,
        created_at: now,
        updated_at: now,
    })
}

pub async fn get(id: &str) -> anyhow::Result<AgentBackend> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, name, kind, executable_path, default_env_json,
               health_check_url, enabled, last_health_check_at,
               health_status, created_at, updated_at
        FROM agent_backends
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?
    .with_context(|| format!("agent backend not found: {id}"))?;
    row_to_backend(row)
}

pub async fn list(filter: BackendFilter) -> anyhow::Result<Vec<AgentBackend>> {
    let pool = db::pool().await?;
    let limit = filter.limit.unwrap_or(50).clamp(1, 500) as i64;

    let rows = sqlx::query(
        r#"
        SELECT id, name, kind, executable_path, default_env_json,
               health_check_url, enabled, last_health_check_at,
               health_status, created_at, updated_at
        FROM agent_backends
        ORDER BY name ASC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(&pool)
    .await?;

    let mut backends: Vec<AgentBackend> = rows
        .into_iter()
        .map(row_to_backend)
        .collect::<anyhow::Result<Vec<_>>>()?;

    if let Some(kind) = filter.kind {
        backends.retain(|b| b.kind == kind);
    }
    if let Some(enabled) = filter.enabled {
        backends.retain(|b| b.enabled == enabled);
    }
    if let Some(status) = filter.health_status {
        backends.retain(|b| b.health_status == status);
    }

    Ok(backends)
}

pub async fn update(id: &str, input: UpdateBackendInput) -> anyhow::Result<AgentBackend> {
    let mut backend = get(id).await?;
    let now = Utc::now();

    if let Some(name) = input.name {
        if name.trim().is_empty() {
            bail!("backend name cannot be empty");
        }
        backend.name = name;
    }
    if let Some(kind) = input.kind {
        backend.kind = kind;
    }
    if let Some(exec) = input.executable_path {
        backend.executable_path = exec;
    }
    if let Some(env) = input.default_env_json {
        backend.default_env_json = env;
    }
    if let Some(url) = input.health_check_url {
        backend.health_check_url = url;
    }
    if let Some(enabled) = input.enabled {
        backend.enabled = enabled;
    }
    backend.updated_at = now;

    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE agent_backends SET
            name = ?2, kind = ?3, executable_path = ?4, default_env_json = ?5,
            health_check_url = ?6, enabled = ?7, last_health_check_at = ?8,
            health_status = ?9, updated_at = ?10
        WHERE id = ?1
        "#,
    )
    .bind(&backend.id)
    .bind(&backend.name)
    .bind(backend.kind.as_str())
    .bind(&backend.executable_path)
    .bind(
        backend
            .default_env_json
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
    )
    .bind(&backend.health_check_url)
    .bind(backend.enabled as i64)
    .bind(backend.last_health_check_at.map(|dt| dt.to_rfc3339()))
    .bind(backend.health_status.as_str())
    .bind(backend.updated_at.to_rfc3339())
    .execute(&pool)
    .await?;

    Ok(backend)
}

pub async fn delete(id: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let result = sqlx::query("DELETE FROM agent_backends WHERE id = ?1")
        .bind(id)
        .execute(&pool)
        .await?;
    if result.rows_affected() == 0 {
        bail!("agent backend not found: {id}");
    }
    Ok(())
}

// ── Health check ─────────────────────────────────────────────────────────

/// Run a single health check for a backend and persist the result.
pub async fn run_health_check(backend: &mut AgentBackend) -> anyhow::Result<()> {
    let now = Utc::now();
    let new_status = perform_health_check(backend).await;

    backend.health_status = new_status;
    backend.last_health_check_at = Some(now);
    backend.updated_at = now;

    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE agent_backends SET
            health_status = ?2,
            last_health_check_at = ?3,
            updated_at = ?4
        WHERE id = ?1
        "#,
    )
    .bind(&backend.id)
    .bind(backend.health_status.as_str())
    .bind(backend.last_health_check_at.unwrap().to_rfc3339())
    .bind(backend.updated_at.to_rfc3339())
    .execute(&pool)
    .await?;

    Ok(())
}

async fn perform_health_check(backend: &AgentBackend) -> HealthStatus {
    // If backend is disabled, report as unknown
    if !backend.enabled {
        return HealthStatus::Unknown;
    }

    // Strategy 1: If health_check_url is set, HTTP ping
    if let Some(url) = &backend.health_check_url {
        return match http_ping(url).await {
            Ok(true) => HealthStatus::Healthy,
            Ok(false) => HealthStatus::Unhealthy,
            Err(_) => HealthStatus::Degraded,
        };
    }

    // Strategy 2: If executable_path is set, check process existence
    if let Some(exe_path) = &backend.executable_path {
        return if std::path::Path::new(exe_path).exists() {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        };
    }

    // No health check configured
    HealthStatus::Unknown
}

async fn http_ping(url: &str) -> anyhow::Result<bool> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let resp = client.get(url).send().await?;
    Ok(resp.status().is_success())
}

/// Spawn a background task that periodically checks health of all enabled backends.
/// Returns the JoinHandle so the caller can manage the task lifetime.
pub fn spawn_health_check_loop(interval: std::time::Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            if let Err(err) = check_all_backends().await {
                tracing::warn!("health check loop error: {err}");
            }
        }
    })
}

async fn check_all_backends() -> anyhow::Result<()> {
    let filter = BackendFilter {
        enabled: Some(true),
        ..Default::default()
    };
    let backends = list(filter).await?;
    for mut backend in backends {
        if let Err(err) = run_health_check(&mut backend).await {
            tracing::warn!("health check failed for backend {}: {err}", backend.id);
        }
    }
    Ok(())
}

// ── Row mapping ──────────────────────────────────────────────────────────

fn row_to_backend(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<AgentBackend> {
    let default_env_json: Option<serde_json::Value> = row
        .try_get::<Option<String>, _>("default_env_json")?
        .map(|v| serde_json::from_str(&v))
        .transpose()?;

    Ok(AgentBackend {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        kind: BackendKind::from_str(row.try_get::<String, _>("kind")?.as_str())?,
        executable_path: row.try_get("executable_path")?,
        default_env_json,
        health_check_url: row.try_get("health_check_url")?,
        enabled: row.try_get::<i64, _>("enabled")? != 0,
        last_health_check_at: row
            .try_get::<Option<String>, _>("last_health_check_at")?
            .map(|v| DateTime::parse_from_rfc3339(&v).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
        health_status: HealthStatus::from_str(row.try_get::<String, _>("health_status")?.as_str())?,
        created_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("updated_at")?.as_str())?
            .with_timezone(&Utc),
    })
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn crud_create_get_list_delete() {
        let _root = TestRoot::new();

        // Create
        let backend = create(CreateBackendInput {
            name: "My Claude".to_string(),
            kind: BackendKind::ClaudeP,
            executable_path: Some("/usr/bin/claude".to_string()),
            default_env_json: Some(serde_json::json!({"LANG": "en"})),
            health_check_url: Some("http://localhost:8080/health".to_string()),
            enabled: true,
        })
        .await
        .expect("create");

        assert_eq!(backend.name, "My Claude");
        assert_eq!(backend.kind, BackendKind::ClaudeP);
        assert!(backend.enabled);
        assert_eq!(backend.health_status, HealthStatus::Unknown);

        // Get
        let loaded = get(&backend.id).await.expect("get");
        assert_eq!(loaded.id, backend.id);
        assert_eq!(loaded.executable_path.as_deref(), Some("/usr/bin/claude"));
        assert!(loaded.default_env_json.is_some());

        // List
        let all = list(BackendFilter::default()).await.expect("list");
        assert_eq!(all.len(), 1);

        // List filtered by kind
        let filtered = list(BackendFilter {
            kind: Some(BackendKind::CodexInteractive),
            ..Default::default()
        })
        .await
        .expect("list filtered");
        assert_eq!(filtered.len(), 0);

        // Delete
        delete(&backend.id).await.expect("delete");

        // Verify deleted
        let err = get(&backend.id).await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn update_backend_fields() {
        let _root = TestRoot::new();

        let backend = create(CreateBackendInput {
            name: "Original".to_string(),
            kind: BackendKind::Review,
            executable_path: None,
            default_env_json: None,
            health_check_url: None,
            enabled: true,
        })
        .await
        .expect("create");

        let updated = update(
            &backend.id,
            UpdateBackendInput {
                name: Some("Renamed".to_string()),
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .expect("update");

        assert_eq!(updated.name, "Renamed");
        assert!(!updated.enabled);
        assert_eq!(updated.kind, BackendKind::Review); // unchanged

        let loaded = get(&backend.id).await.expect("get");
        assert_eq!(loaded.name, "Renamed");
        assert!(!loaded.enabled);
    }

    #[tokio::test]
    async fn health_check_with_executable_path() {
        let _root = TestRoot::new();

        // Create backend with a path that exists (current exe)
        let exe = std::env::current_exe()
            .expect("current exe")
            .display()
            .to_string();

        let mut backend = create(CreateBackendInput {
            name: "Exe Check".to_string(),
            kind: BackendKind::ClaudeP,
            executable_path: Some(exe),
            default_env_json: None,
            health_check_url: None,
            enabled: true,
        })
        .await
        .expect("create");

        run_health_check(&mut backend).await.expect("health check");
        assert_eq!(backend.health_status, HealthStatus::Healthy);
        assert!(backend.last_health_check_at.is_some());

        // Reload from DB to verify persistence
        let loaded = get(&backend.id).await.expect("get");
        assert_eq!(loaded.health_status, HealthStatus::Healthy);
        assert!(loaded.last_health_check_at.is_some());
    }

    #[tokio::test]
    async fn health_check_disabled_backend_returns_unknown() {
        let _root = TestRoot::new();

        let mut backend = create(CreateBackendInput {
            name: "Disabled".to_string(),
            kind: BackendKind::AgentTeam,
            executable_path: None,
            default_env_json: None,
            health_check_url: None,
            enabled: false,
        })
        .await
        .expect("create");

        run_health_check(&mut backend).await.expect("health check");
        assert_eq!(backend.health_status, HealthStatus::Unknown);
    }

    #[tokio::test]
    async fn health_check_nonexistent_executable_is_unhealthy() {
        let _root = TestRoot::new();

        let mut backend = create(CreateBackendInput {
            name: "Bad Path".to_string(),
            kind: BackendKind::CodexInteractive,
            executable_path: Some("/nonexistent/path/to/exe".to_string()),
            default_env_json: None,
            health_check_url: None,
            enabled: true,
        })
        .await
        .expect("create");

        run_health_check(&mut backend).await.expect("health check");
        assert_eq!(backend.health_status, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn create_rejects_empty_name() {
        let _root = TestRoot::new();

        let err = create(CreateBackendInput {
            name: "".to_string(),
            kind: BackendKind::Review,
            executable_path: None,
            default_env_json: None,
            health_check_url: None,
            enabled: true,
        })
        .await
        .unwrap_err();

        assert!(err.to_string().contains("name cannot be empty"));
    }

    #[tokio::test]
    async fn list_filters_by_enabled() {
        let _root = TestRoot::new();

        create(CreateBackendInput {
            name: "A".to_string(),
            kind: BackendKind::ClaudeP,
            executable_path: None,
            default_env_json: None,
            health_check_url: None,
            enabled: true,
        })
        .await
        .expect("create A");

        create(CreateBackendInput {
            name: "B".to_string(),
            kind: BackendKind::ClaudeP,
            executable_path: None,
            default_env_json: None,
            health_check_url: None,
            enabled: false,
        })
        .await
        .expect("create B");

        let enabled_only = list(BackendFilter {
            enabled: Some(true),
            ..Default::default()
        })
        .await
        .expect("list enabled");
        assert_eq!(enabled_only.len(), 1);
        assert_eq!(enabled_only[0].name, "A");
    }
}
