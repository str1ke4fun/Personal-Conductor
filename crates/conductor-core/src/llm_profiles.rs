use crate::db;
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

// ── LlmProfile ────────────────────────────────────────────────────────────────

/// Represents a configured LLM provider profile (model + endpoint + credentials).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LlmProfile {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model_id: String,
    pub api_base_url: String,
    pub api_key_encrypted: Option<String>,
    pub max_tokens: i64,
    pub temperature: f64,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── CRUD ──────────────────────────────────────────────────────────────────────

/// Create a new LlmProfile. Returns the inserted record.
pub async fn create_profile(
    name: &str,
    provider: &str,
    model_id: &str,
    api_base_url: &str,
    api_key_encrypted: Option<&str>,
    max_tokens: i64,
    temperature: f64,
) -> anyhow::Result<LlmProfile> {
    let valid_providers = ["openai", "anthropic", "local"];
    if !valid_providers.contains(&provider) {
        bail!("invalid provider: {provider}. must be one of: openai, anthropic, local");
    }

    let now = Utc::now();
    let id = format!("llmprof-{}", Uuid::new_v4());
    let pool = db::pool().await?;

    sqlx::query(
        r#"INSERT INTO llm_profiles
          (id, name, provider, model_id, api_base_url, api_key_encrypted,
           max_tokens, temperature, enabled, created_at, updated_at)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)"#,
    )
    .bind(&id)
    .bind(name)
    .bind(provider)
    .bind(model_id)
    .bind(api_base_url)
    .bind(api_key_encrypted)
    .bind(max_tokens)
    .bind(temperature)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(&pool)
    .await
    .with_context(|| "insert llm_profile")?;

    Ok(LlmProfile {
        id,
        name: name.to_string(),
        provider: provider.to_string(),
        model_id: model_id.to_string(),
        api_base_url: api_base_url.to_string(),
        api_key_encrypted: api_key_encrypted.map(String::from),
        max_tokens,
        temperature,
        enabled: true,
        created_at: now,
        updated_at: now,
    })
}

/// Fetch a single LlmProfile by id.
pub async fn get_profile(profile_id: &str) -> anyhow::Result<Option<LlmProfile>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"SELECT id, name, provider, model_id, api_base_url, api_key_encrypted,
                  max_tokens, temperature, enabled, created_at, updated_at
           FROM llm_profiles WHERE id = ?"#,
    )
    .bind(profile_id)
    .fetch_optional(&pool)
    .await
    .with_context(|| "fetch llm_profile")?;

    match row {
        Some(row) => Ok(Some(row_to_profile(&row)?)),
        None => Ok(None),
    }
}

/// List all LlmProfile records, optionally filtering by enabled status.
pub async fn list_profiles(enabled_only: bool) -> anyhow::Result<Vec<LlmProfile>> {
    let pool = db::pool().await?;

    let rows = if enabled_only {
        sqlx::query(
            r#"SELECT id, name, provider, model_id, api_base_url, api_key_encrypted,
                      max_tokens, temperature, enabled, created_at, updated_at
               FROM llm_profiles WHERE enabled = 1
               ORDER BY created_at DESC"#,
        )
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query(
            r#"SELECT id, name, provider, model_id, api_base_url, api_key_encrypted,
                      max_tokens, temperature, enabled, created_at, updated_at
               FROM llm_profiles
               ORDER BY created_at DESC"#,
        )
        .fetch_all(&pool)
        .await?
    };

    rows.iter().map(row_to_profile).collect()
}

/// Update mutable fields of an LlmProfile. Returns the updated record.
pub async fn update_profile(
    profile_id: &str,
    name: Option<&str>,
    provider: Option<&str>,
    model_id: Option<&str>,
    api_base_url: Option<&str>,
    api_key_encrypted: Option<Option<&str>>,
    max_tokens: Option<i64>,
    temperature: Option<f64>,
    enabled: Option<bool>,
) -> anyhow::Result<LlmProfile> {
    let mut profile = get_profile(profile_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("llm_profile not found: {profile_id}"))?;

    if let Some(p) = provider {
        let valid_providers = ["openai", "anthropic", "local"];
        if !valid_providers.contains(&p) {
            bail!("invalid provider: {p}. must be one of: openai, anthropic, local");
        }
    }

    let now = Utc::now();
    let pool = db::pool().await?;

    sqlx::query(
        r#"UPDATE llm_profiles SET
           name = COALESCE(?, name),
           provider = COALESCE(?, provider),
           model_id = COALESCE(?, model_id),
           api_base_url = COALESCE(?, api_base_url),
           api_key_encrypted = ?,
           max_tokens = COALESCE(?, max_tokens),
           temperature = COALESCE(?, temperature),
           enabled = COALESCE(?, enabled),
           updated_at = ?
           WHERE id = ?"#,
    )
    .bind(name)
    .bind(provider)
    .bind(model_id)
    .bind(api_base_url)
    .bind(api_key_encrypted.flatten()) // None => leave unchanged, Some(None) => set NULL, Some(Some(v)) => set v
    .bind(max_tokens)
    .bind(temperature)
    .bind(enabled)
    .bind(now.to_rfc3339())
    .bind(profile_id)
    .execute(&pool)
    .await
    .with_context(|| "update llm_profile")?;

    // Reflect changes locally
    if let Some(v) = name {
        profile.name = v.to_string();
    }
    if let Some(v) = provider {
        profile.provider = v.to_string();
    }
    if let Some(v) = model_id {
        profile.model_id = v.to_string();
    }
    if let Some(v) = api_base_url {
        profile.api_base_url = v.to_string();
    }
    if let Some(v) = api_key_encrypted {
        profile.api_key_encrypted = v.map(String::from);
    }
    if let Some(v) = max_tokens {
        profile.max_tokens = v;
    }
    if let Some(v) = temperature {
        profile.temperature = v;
    }
    if let Some(v) = enabled {
        profile.enabled = v;
    }
    profile.updated_at = now;

    Ok(profile)
}

/// Delete an LlmProfile by id. Returns error if not found.
pub async fn delete_profile(profile_id: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let result = sqlx::query("DELETE FROM llm_profiles WHERE id = ?")
        .bind(profile_id)
        .execute(&pool)
        .await
        .with_context(|| "delete llm_profile")?;

    if result.rows_affected() == 0 {
        bail!("llm_profile not found: {profile_id}");
    }

    Ok(())
}

// ── Row mapping helper ────────────────────────────────────────────────────────

fn row_to_profile(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<LlmProfile> {
    let created_at = parse_utc(&row.try_get::<String, _>("created_at")?)?;
    let updated_at = parse_utc(&row.try_get::<String, _>("updated_at")?)?;
    let enabled_raw: i64 = row.try_get("enabled")?;

    Ok(LlmProfile {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        provider: row.try_get("provider")?,
        model_id: row.try_get("model_id")?,
        api_base_url: row.try_get("api_base_url")?,
        api_key_encrypted: row.try_get("api_key_encrypted")?,
        max_tokens: row.try_get("max_tokens")?,
        temperature: row.try_get("temperature")?,
        enabled: enabled_raw != 0,
        created_at,
        updated_at,
    })
}

fn parse_utc(value: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("parse RFC3339 datetime: {value}"))?
        .with_timezone(&Utc))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn create_and_get_profile() {
        let _root = TestRoot::new();

        let profile = create_profile(
            "GPT-4 Turbo",
            "openai",
            "gpt-4-turbo",
            "https://api.openai.com/v1",
            Some("sk-encrypted-abc"),
            4096,
            0.7,
        )
        .await
        .expect("create_profile");

        assert!(profile.id.starts_with("llmprof-"));
        assert_eq!(profile.name, "GPT-4 Turbo");
        assert_eq!(profile.provider, "openai");
        assert_eq!(profile.model_id, "gpt-4-turbo");
        assert_eq!(profile.api_base_url, "https://api.openai.com/v1");
        assert_eq!(
            profile.api_key_encrypted.as_deref(),
            Some("sk-encrypted-abc")
        );
        assert_eq!(profile.max_tokens, 4096);
        assert!((profile.temperature - 0.7).abs() < f64::EPSILON);
        assert!(profile.enabled);
        assert!(profile.created_at <= Utc::now());

        // Fetch back
        let loaded = get_profile(&profile.id)
            .await
            .expect("get_profile")
            .expect("should exist");
        assert_eq!(loaded.id, profile.id);
        assert_eq!(loaded.name, "GPT-4 Turbo");
        assert_eq!(loaded.max_tokens, 4096);
    }

    #[tokio::test]
    async fn list_profiles_filters_by_enabled() {
        let _root = TestRoot::new();

        create_profile(
            "A",
            "openai",
            "gpt-4",
            "https://api.openai.com/v1",
            None,
            2048,
            0.5,
        )
        .await
        .expect("create A");
        let b = create_profile(
            "B",
            "anthropic",
            "claude-3",
            "https://api.anthropic.com",
            None,
            4096,
            0.8,
        )
        .await
        .expect("create B");

        // Disable B
        update_profile(&b.id, None, None, None, None, None, None, None, Some(false))
            .await
            .expect("disable B");

        let all = list_profiles(false).await.expect("list all");
        assert_eq!(all.len(), 2);

        let enabled_only = list_profiles(true).await.expect("list enabled");
        assert_eq!(enabled_only.len(), 1);
        assert_eq!(enabled_only[0].name, "A");
    }

    #[tokio::test]
    async fn update_profile_fields() {
        let _root = TestRoot::new();

        let profile = create_profile(
            "Old Name",
            "openai",
            "gpt-3.5-turbo",
            "https://api.openai.com/v1",
            None,
            1024,
            0.5,
        )
        .await
        .expect("create");

        let updated = update_profile(
            &profile.id,
            Some("New Name"),
            Some("anthropic"),
            Some("claude-3-opus"),
            Some("https://api.anthropic.com"),
            Some(Some("sk-new-key")),
            Some(8192),
            Some(0.9),
            None,
        )
        .await
        .expect("update");

        assert_eq!(updated.name, "New Name");
        assert_eq!(updated.provider, "anthropic");
        assert_eq!(updated.model_id, "claude-3-opus");
        assert_eq!(updated.api_base_url, "https://api.anthropic.com");
        assert_eq!(updated.api_key_encrypted.as_deref(), Some("sk-new-key"));
        assert_eq!(updated.max_tokens, 8192);
        assert!((updated.temperature - 0.9).abs() < f64::EPSILON);
        assert!(updated.updated_at > profile.created_at);

        // Verify persisted
        let loaded = get_profile(&profile.id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(loaded.name, "New Name");
        assert_eq!(loaded.provider, "anthropic");
    }

    #[tokio::test]
    async fn delete_profile_removes_record() {
        let _root = TestRoot::new();

        let profile = create_profile(
            "Disposable",
            "local",
            "llama-3",
            "http://localhost:8080",
            None,
            2048,
            1.0,
        )
        .await
        .expect("create");

        delete_profile(&profile.id).await.expect("delete");

        let result = get_profile(&profile.id).await.expect("get");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_returns_error() {
        let _root = TestRoot::new();

        let result = delete_profile("llmprof-nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn invalid_provider_rejected() {
        let _root = TestRoot::new();

        let result = create_profile(
            "Bad",
            "cohere",
            "command-r",
            "https://api.cohere.com",
            None,
            2048,
            0.5,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid provider"));
    }

    #[tokio::test]
    async fn create_profile_with_no_api_key() {
        let _root = TestRoot::new();

        let profile = create_profile(
            "Local Model",
            "local",
            "llama-3-8b",
            "http://localhost:11434",
            None,
            2048,
            1.0,
        )
        .await
        .expect("create");

        assert!(profile.api_key_encrypted.is_none());
        assert_eq!(profile.provider, "local");

        let loaded = get_profile(&profile.id)
            .await
            .expect("get")
            .expect("exists");
        assert!(loaded.api_key_encrypted.is_none());
    }
}
