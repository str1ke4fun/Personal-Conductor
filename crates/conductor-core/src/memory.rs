use crate::db;
use crate::embedding::{EmbeddingProvider, HashFallbackProvider};
use crate::scene::generate_scene_tags;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Global,
    Workspace,
    Document,
    Session,
}

impl MemoryScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Workspace => "workspace",
            Self::Document => "document",
            Self::Session => "session",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "global" => Ok(Self::Global),
            "workspace" => Ok(Self::Workspace),
            "document" => Ok(Self::Document),
            "session" => Ok(Self::Session),
            other => Err(anyhow::anyhow!("unknown memory scope: {}", other)),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    UserConfirmed,
    Inferred,
    Tool,
    Summary,
    PatternAggregation,
}

impl MemorySource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserConfirmed => "user_confirmed",
            Self::Inferred => "inferred",
            Self::Tool => "tool",
            Self::Summary => "summary",
            Self::PatternAggregation => "pattern_aggregation",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "user_confirmed" => Ok(Self::UserConfirmed),
            "inferred" => Ok(Self::Inferred),
            "tool" => Ok(Self::Tool),
            "summary" => Ok(Self::Summary),
            "pattern_aggregation" => Ok(Self::PatternAggregation),
            other => Err(anyhow::anyhow!("unknown memory source: {}", other)),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemorySensitivity {
    Normal,
    Private,
    Secret,
}

impl MemorySensitivity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Private => "private",
            Self::Secret => "secret",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "normal" => Ok(Self::Normal),
            "private" => Ok(Self::Private),
            "secret" => Ok(Self::Secret),
            other => Err(anyhow::anyhow!("unknown memory sensitivity: {}", other)),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryEntry {
    pub id: String,
    pub key: String,
    pub value: String,
    pub category: String,
    pub scope: MemoryScope,
    pub workspace_id: Option<String>,
    pub path_prefix: Option<String>,
    pub source: MemorySource,
    pub confidence: f64,
    pub sensitivity: MemorySensitivity,
    pub status: String,
    /// Scene tags associated with this entry (aggregated from its chunks).
    #[serde(default)]
    pub scene_tags: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Number of times this entry has been reinforced via `reinforce_pattern`.
    pub interaction_count: i64,
    /// Timestamp of the last reinforcement. Used for confidence decay calculations.
    pub last_reinforced_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UserPreferences {
    pub favorite_topics: Vec<String>,
    pub preferred_time: String,
    pub chat_style: String,
    pub avatar_settings: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConversationSummary {
    pub id: String,
    pub summary: String,
    pub keywords: Vec<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryChunk {
    pub id: String,
    pub memory_id: String,
    pub workspace_id: Option<String>,
    pub scope: MemoryScope,
    pub category: String,
    pub content: String,
    pub summary: Option<String>,
    pub source: MemorySource,
    pub sensitivity: MemorySensitivity,
    pub confidence: f64,
    pub scene_tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryEmbedding {
    pub chunk_id: String,
    pub model: String,
    pub dims: usize,
    pub vector: Vec<f32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemorySearchResult {
    pub chunk: MemoryChunk,
    pub score: f64,
    pub score_details: ScoreDetails,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScoreDetails {
    pub vector_score: f64,
    pub keyword_score: f64,
    pub recency_score: f64,
    pub authority_score: f64,
}

/// Aggregated interaction pattern derived from memory chunk history.
///
/// Each pattern represents a recurring (category, scene_tag) pair observed
/// across recent memory entries, along with its frequency and confidence.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InteractionPattern {
    /// The memory category this pattern belongs to (e.g. "preference", "coding_style").
    pub category: String,
    /// A human-readable description of the pattern (e.g. "preference: coding_style, frequently accessed in evening/weekday").
    pub key_pattern: String,
    /// How many times this (category, scene_tag) combination was observed.
    pub frequency: usize,
    /// The scene tags associated with this pattern (e.g. ["evening", "weekday"]).
    pub scene_tags: Vec<String>,
    /// Confidence score in [0.0, 1.0] based on frequency relative to total observations.
    pub confidence: f64,
}

/// Result of multi-path recall for prompt injection.
///
/// Combines memory entries and conversation summaries matched against a context
/// string, deduplicated across both search paths.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RecallResult {
    /// Memory entries matched by semantic/vector search.
    pub entries: Vec<MemoryEntry>,
    /// Conversation summaries matched by either semantic or keyword search.
    pub summaries: Vec<ConversationSummary>,
    /// Approximate number of chunks searched across all sources.
    pub total_chunks_searched: usize,
}

/// Explicit recall context for prompt construction and future scoped retrieval.
///
/// All fields are enforced where the underlying memory rows carry the relevant
/// scope metadata. Global/workspace memories without an explicit session or goal
/// remain eligible so stable project facts still recall across turns.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RecallContext {
    pub query: String,
    pub workspace_id: Option<String>,
    pub path_prefix: Option<String>,
    pub session_id: Option<String>,
    pub goal_id: Option<String>,
    pub limit: usize,
}

/// Filter criteria for `search_memory_filtered`.
///
/// Default (via `Default::default()`) excludes quarantined, forgotten, and archived
/// entries, does not limit by sensitivity level or category.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchFilter {
    /// Exclude entries with status "quarantined". Default: true.
    pub exclude_quarantined: bool,
    /// Exclude entries with status "forgotten". Default: true.
    pub exclude_forgotten: bool,
    /// Exclude entries with status "archived". Default: true.
    pub exclude_archived: bool,
    /// Maximum sensitivity level to include. `None` means allow all levels.
    /// If set, entries with a sensitivity level higher than the given level are excluded.
    pub max_sensitivity: Option<MemorySensitivity>,
    /// Filter by exact category match. `None` means all categories.
    pub category: Option<String>,
    /// Limit workspace-scoped recall to entries whose stored path prefix is an
    /// ancestor of the current context path. `None` means no path filtering.
    pub path_prefix: Option<String>,
    /// Exclude memories explicitly sourced from a different chat session.
    /// Memories without source session metadata remain eligible.
    pub session_id: Option<String>,
    /// Exclude memories explicitly sourced from a different goal.
    /// Memories without goal metadata remain eligible.
    pub goal_id: Option<String>,
}

impl Default for SearchFilter {
    fn default() -> Self {
        Self {
            exclude_quarantined: true,
            exclude_forgotten: true,
            exclude_archived: true,
            max_sensitivity: None,
            category: None,
            path_prefix: None,
            session_id: None,
            goal_id: None,
        }
    }
}

fn normalize_path_prefix(path_prefix: &str) -> Option<String> {
    let trimmed = path_prefix.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut normalized = trimmed.replace('\\', "/");
    while normalized.len() > 1 && normalized.ends_with('/') && !normalized.ends_with(":/") {
        normalized.pop();
    }

    Some(normalized)
}

impl MemorySensitivity {
    /// Returns a numeric ordering for sensitivity comparison.
    /// Normal (0) < Private (1) < Secret (2).
    pub fn level(&self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Private => 1,
            Self::Secret => 2,
        }
    }
}

// ── Embedding Model Abstraction (TASK-052) ──────────────────────────────

/// Trait for pluggable embedding models.
/// Implement this to add new embedding backends (e.g. Chinese-optimized models).
pub trait EmbeddingModel: Send + Sync {
    /// Generate an embedding vector for the given text.
    fn embed(&self, text: &str) -> Vec<f32>;
    /// The dimensionality of the embedding vectors produced by this model.
    fn dimension(&self) -> usize;
    /// A short identifier stored alongside embeddings in the database.
    fn model_name(&self) -> &str;
}

/// Deterministic hash-based embedding model for testing and as a fallback.
/// Same text always produces the same vector; no semantic understanding.
pub struct HashEmbeddingModel {
    dim: usize,
}

impl HashEmbeddingModel {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

impl Default for HashEmbeddingModel {
    fn default() -> Self {
        Self { dim: 384 }
    }
}

impl EmbeddingModel for HashEmbeddingModel {
    fn embed(&self, text: &str) -> Vec<f32> {
        let mut embedding = vec![0.0f32; self.dim];
        let hash = text
            .as_bytes()
            .iter()
            .fold(0u64, |acc, &b| acc.wrapping_mul(131).wrapping_add(b as u64));
        let mut rng = std::num::Wrapping(hash);
        for i in 0..self.dim {
            rng = rng * std::num::Wrapping(1103515245) + std::num::Wrapping(12345);
            embedding[i] = ((rng.0 >> 16) & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0;
        }
        embedding
    }

    fn dimension(&self) -> usize {
        self.dim
    }

    fn model_name(&self) -> &str {
        "hash-embedding"
    }
}

/// FastEmbed-backed embedding model using BGE-small-zh-v1.5 (Chinese-optimised).
/// Falls back to `HashEmbeddingModel` if the model fails to load or embed.
pub struct FastEmbedModel {
    inner: Option<fastembed::TextEmbedding>,
    dim: usize,
}

impl FastEmbedModel {
    pub fn new() -> Self {
        let mut opts = fastembed::InitOptions::default();
        opts.model_name = fastembed::EmbeddingModel::BGESmallZHV15;
        opts.show_download_progress = true;
        match fastembed::TextEmbedding::try_new(opts) {
            Ok(model) => Self {
                inner: Some(model),
                dim: 512,
            },
            Err(e) => {
                tracing::warn!(
                    "fastembed BGESmallZHV15 init failed, falling back to hash embedding: {}",
                    e
                );
                Self {
                    inner: None,
                    dim: 512,
                }
            }
        }
    }
}

impl EmbeddingModel for FastEmbedModel {
    fn embed(&self, text: &str) -> Vec<f32> {
        if let Some(ref model) = self.inner {
            match model.embed(vec![text], None) {
                Ok(embeddings) => {
                    if let Some(embedding) = embeddings.into_iter().next() {
                        return embedding;
                    }
                }
                Err(e) => {
                    tracing::warn!("fastembed embed failed: {}", e);
                }
            }
        }
        HashEmbeddingModel::new(self.dim).embed(text)
    }

    fn dimension(&self) -> usize {
        self.dim
    }

    fn model_name(&self) -> &str {
        "bge-small-zh-v1.5"
    }
}

/// Chinese-optimized embedding model backed by BGE-small-zh-v1.5 via FastEmbed.
/// Delegates to `FastEmbedModel` which already uses the Chinese model.
pub struct ChineseEmbeddingModel {
    inner: FastEmbedModel,
}

impl ChineseEmbeddingModel {
    pub fn new(dim: usize) -> Self {
        let _ = dim; // dim is fixed by the underlying model
        Self {
            inner: FastEmbedModel::new(),
        }
    }
}

impl Default for ChineseEmbeddingModel {
    fn default() -> Self {
        Self::new(512)
    }
}

impl EmbeddingModel for ChineseEmbeddingModel {
    fn embed(&self, text: &str) -> Vec<f32> {
        self.inner.embed(text)
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
}

/// Global embedding model, initialized lazily with `FastEmbedModel` by default.
/// Swap at runtime via `set_embedding_model()`.
static EMBEDDING_MODEL: OnceLock<std::sync::Mutex<Box<dyn EmbeddingModel>>> = OnceLock::new();

fn embedding_model() -> &'static std::sync::Mutex<Box<dyn EmbeddingModel>> {
    EMBEDDING_MODEL.get_or_init(|| std::sync::Mutex::new(Box::new(FastEmbedModel::new())))
}

/// Replace the active embedding model at runtime.
/// Subsequent embedding calls use the new model.
pub fn set_embedding_model(model: Box<dyn EmbeddingModel>) {
    let mut guard = embedding_model().lock().unwrap();
    *guard = model;
}

/// Return the current model's name (for diagnostics / DB metadata).
pub fn current_model_name() -> String {
    embedding_model().lock().unwrap().model_name().to_string()
}

// ── Async EmbeddingProvider (TASK-110) ──────────────────────────────────

/// Global async embedding provider, initialized lazily with `HashFallbackProvider`.
/// Swap at runtime via `set_embedding_provider()`.
static EMBEDDING_PROVIDER: OnceLock<std::sync::Mutex<Arc<dyn EmbeddingProvider>>> = OnceLock::new();

fn embedding_provider_lock() -> &'static std::sync::Mutex<Arc<dyn EmbeddingProvider>> {
    EMBEDDING_PROVIDER
        .get_or_init(|| std::sync::Mutex::new(Arc::new(HashFallbackProvider::default())))
}

/// Get the active async embedding provider.
pub fn active_embedding_provider() -> Arc<dyn EmbeddingProvider> {
    embedding_provider_lock().lock().unwrap().clone()
}

/// Replace the active async embedding provider at runtime.
pub fn set_embedding_provider(provider: Arc<dyn EmbeddingProvider>) {
    let mut guard = embedding_provider_lock().lock().unwrap();
    *guard = provider;
}

/// Return the active async provider's model name (for diagnostics / DB metadata).
pub fn current_provider_name() -> String {
    active_embedding_provider().model_name().to_string()
}

/// Write gate: maps a source string to (MemorySource, default_status, default_confidence).
/// - "user"    → UserConfirmed, active, 1.0  (direct store)
/// - "tool"    → Tool,           candidate, 0.7  (needs classification)
/// - "inferred"→ Inferred,       candidate, 0.5  (needs classification)
/// - "pattern_aggregation" → PatternAggregation, active, 0.8 (system-aggregated patterns)
fn write_gate(source: &str) -> anyhow::Result<(MemorySource, &'static str, f64)> {
    match source {
        "user" => Ok((MemorySource::UserConfirmed, "active", 1.0)),
        "tool" => Ok((MemorySource::Tool, "candidate", 0.7)),
        "inferred" => Ok((MemorySource::Inferred, "candidate", 0.5)),
        "pattern_aggregation" => Ok((MemorySource::PatternAggregation, "active", 0.8)),
        other => Err(anyhow::anyhow!("unknown write source: {}", other)),
    }
}

/// Store or update a memory entry. Defaults to source="user" (active, confidence=1.0).
/// Call `set_with_source()` directly for tool/inferred origins.
pub async fn set(key: &str, value: &str, category: &str) -> anyhow::Result<MemoryEntry> {
    set_with_source(key, value, category, "user").await
}

/// Store or update a memory entry with explicit source control.
///
/// Write gate rules:
/// - source="user"     → status="active",    confidence=1.0 (direct store)
/// - source="tool"     → status="candidate", confidence=0.7 (needs classification)
/// - source="inferred" → status="candidate", confidence=0.5 (needs classification)
pub async fn set_with_source(
    key: &str,
    value: &str,
    category: &str,
    source: &str,
) -> anyhow::Result<MemoryEntry> {
    set_with_scope(key, value, category, MemoryScope::Global, None, source).await
}

/// Store or update a memory entry in an explicit scope.
///
/// Scoped writes only deduplicate within the same
/// `(key, scope, workspace_id, path_prefix)` tuple, so the same logical key can
/// safely exist in multiple workspaces and subtrees.
pub async fn set_with_scope(
    key: &str,
    value: &str,
    category: &str,
    scope: MemoryScope,
    workspace_id: Option<&str>,
    source: &str,
) -> anyhow::Result<MemoryEntry> {
    set_with_scope_and_path(key, value, category, scope, workspace_id, None, source).await
}

/// Store or update a memory entry in an explicit scope and optional path subtree.
pub async fn set_with_scope_and_path(
    key: &str,
    value: &str,
    category: &str,
    scope: MemoryScope,
    workspace_id: Option<&str>,
    path_prefix: Option<&str>,
    source: &str,
) -> anyhow::Result<MemoryEntry> {
    let pool = db::pool().await?;
    let now = Utc::now();
    let (mem_source, default_status, default_confidence) = write_gate(source)?;
    let normalized_path_prefix = path_prefix.and_then(normalize_path_prefix);

    let existing = sqlx::query(
        r#"
        SELECT id, key, value, category, scope, workspace_id, path_prefix, source, confidence,
               sensitivity, status, expires_at, last_used_at, created_at, updated_at
        FROM memory_entries
        WHERE key = ?1
          AND scope = ?2
          AND ((?3 IS NULL AND workspace_id IS NULL) OR workspace_id = ?3)
          AND ((?4 IS NULL AND path_prefix IS NULL) OR path_prefix = ?4)
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(key)
    .bind(scope.as_str())
    .bind(workspace_id)
    .bind(normalized_path_prefix.as_deref())
    .fetch_optional(&pool)
    .await?;

    if let Some(row) = existing {
        let mut entry = memory_from_row(row)?;
        entry.value = value.to_string();
        entry.category = category.to_string();
        entry.source = mem_source.clone();
        entry.confidence = default_confidence;
        entry.status = default_status.to_string();
        entry.path_prefix = normalized_path_prefix.clone();
        entry.updated_at = now;

        sqlx::query(
            r#"
            UPDATE memory_entries
            SET value = ?1, category = ?2, source = ?3, confidence = ?4,
                status = ?5, path_prefix = ?6, updated_at = ?7
            WHERE id = ?8
            "#,
        )
        .bind(&entry.value)
        .bind(&entry.category)
        .bind(entry.source.as_str())
        .bind(entry.confidence)
        .bind(&entry.status)
        .bind(&entry.path_prefix)
        .bind(entry.updated_at.to_rfc3339())
        .bind(&entry.id)
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            DELETE FROM memory_entries
            WHERE key = ?1
              AND scope = ?2
              AND id != ?3
              AND ((?4 IS NULL AND workspace_id IS NULL) OR workspace_id = ?4)
              AND ((?5 IS NULL AND path_prefix IS NULL) OR path_prefix = ?5)
            "#,
        )
        .bind(&entry.key)
        .bind(entry.scope.as_str())
        .bind(&entry.id)
        .bind(entry.workspace_id.as_deref())
        .bind(entry.path_prefix.as_deref())
        .execute(&pool)
        .await?;

        // Write-through: index into chunks/embeddings for search
        if let Err(e) = index_memory_entry(&entry).await {
            tracing::warn!("index_memory_entry failed for key={}: {}", entry.key, e);
        }

        return Ok(entry);
    }

    let entry = MemoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        key: key.to_string(),
        value: value.to_string(),
        category: category.to_string(),
        scope,
        workspace_id: workspace_id.map(str::to_string),
        path_prefix: normalized_path_prefix.clone(),
        source: mem_source,
        confidence: default_confidence,
        sensitivity: MemorySensitivity::Normal,
        status: default_status.to_string(),
        scene_tags: Vec::new(),
        expires_at: None,
        last_used_at: None,
        created_at: now,
        updated_at: now,
        interaction_count: 0,
        last_reinforced_at: None,
    };

    sqlx::query(
        r#"
        INSERT INTO memory_entries (id, key, value, category, scope, workspace_id, path_prefix, source,
                                   confidence, sensitivity, status, expires_at, last_used_at,
                                   created_at, updated_at, interaction_count, last_reinforced_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        "#,
    )
    .bind(&entry.id)
    .bind(&entry.key)
    .bind(&entry.value)
    .bind(&entry.category)
    .bind(entry.scope.as_str())
    .bind(&entry.workspace_id)
    .bind(&entry.path_prefix)
    .bind(entry.source.as_str())
    .bind(entry.confidence)
    .bind(entry.sensitivity.as_str())
    .bind(&entry.status)
    .bind(entry.expires_at.map(|dt| dt.to_rfc3339()))
    .bind(entry.last_used_at.map(|dt| dt.to_rfc3339()))
    .bind(entry.created_at.to_rfc3339())
    .bind(entry.updated_at.to_rfc3339())
    .bind(entry.interaction_count)
    .bind(entry.last_reinforced_at.map(|dt| dt.to_rfc3339()))
    .execute(&pool)
    .await?;

    // Write-through: index into chunks/embeddings for search
    if let Err(e) = index_memory_entry(&entry).await {
        tracing::warn!("index_memory_entry failed for key={}: {}", entry.key, e);
    }

    Ok(entry)
}

/// Convenience wrapper for workspace-scoped writes.
pub async fn set_for_workspace(
    key: &str,
    value: &str,
    category: &str,
    workspace_id: &str,
    source: &str,
) -> anyhow::Result<MemoryEntry> {
    set_with_scope(
        key,
        value,
        category,
        MemoryScope::Workspace,
        Some(workspace_id),
        source,
    )
    .await
}

/// Promote or change the status of a memory entry by key.
/// Only operates on entries whose current status is "candidate".
/// Valid target statuses: "active", "archived", "forgotten".
pub async fn classify(key: &str, new_status: &str) -> anyhow::Result<bool> {
    let valid_targets = ["active", "archived", "forgotten"];
    if !valid_targets.contains(&new_status) {
        return Err(anyhow::anyhow!(
            "invalid target status '{}'; must be one of: {:?}",
            new_status,
            valid_targets
        ));
    }
    let pool = db::pool().await?;
    let now = Utc::now();
    let result = sqlx::query(
        r#"
        UPDATE memory_entries
        SET status = ?1, updated_at = ?2
        WHERE key = ?3 AND status = 'candidate'
        "#,
    )
    .bind(new_status)
    .bind(now.to_rfc3339())
    .bind(key)
    .execute(&pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get(key: &str) -> anyhow::Result<Option<String>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT value
        FROM memory_entries
        WHERE key = ?1 AND status = 'active'
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(key)
    .fetch_all(&pool)
    .await?;

    if rows.is_empty() {
        Ok(None)
    } else {
        Ok(Some(rows[0].try_get("value")?))
    }
}

pub async fn get_by_category(category: &str) -> anyhow::Result<Vec<MemoryEntry>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, key, value, category, scope, workspace_id, path_prefix, source, confidence,
               sensitivity, status, expires_at, last_used_at, created_at, updated_at
        FROM memory_entries
        WHERE category = ?1 AND status = 'active'
        ORDER BY updated_at DESC
        "#,
    )
    .bind(category)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(memory_from_row).collect()
}

pub async fn archive(key: &str) -> anyhow::Result<bool> {
    let pool = db::pool().await?;
    let now = Utc::now();

    let entry_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM memory_entries WHERE key = ?1 AND status = 'active'")
            .bind(key)
            .fetch_optional(&pool)
            .await?;

    let result = sqlx::query(
        r#"
        UPDATE memory_entries
        SET status = 'archived', updated_at = ?1
        WHERE key = ?2 AND status = 'active'
        "#,
    )
    .bind(now.to_rfc3339())
    .bind(key)
    .execute(&pool)
    .await?;

    if result.rows_affected() > 0 {
        if let Some(ref id) = entry_id {
            let _ = cleanup_entry_chunks(id).await;
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

pub async fn forget(key: &str) -> anyhow::Result<bool> {
    let pool = db::pool().await?;
    let now = Utc::now();

    let entry_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM memory_entries WHERE key = ?1 AND status != 'forgotten'",
    )
    .bind(key)
    .fetch_optional(&pool)
    .await?;

    let result = sqlx::query(
        r#"
        UPDATE memory_entries
        SET status = 'forgotten', updated_at = ?1
        WHERE key = ?2 AND status != 'forgotten'
        "#,
    )
    .bind(now.to_rfc3339())
    .bind(key)
    .execute(&pool)
    .await?;

    if result.rows_affected() > 0 {
        if let Some(ref id) = entry_id {
            let _ = cleanup_entry_chunks(id).await;
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Quarantine a memory entry. Marks active or candidate entries as quarantined.
/// Quarantined entries are excluded from get(), get_by_category(), and search().
pub async fn quarantine(key: &str) -> anyhow::Result<bool> {
    let pool = db::pool().await?;
    let now = Utc::now();

    let entry_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM memory_entries WHERE key = ?1 AND status IN ('active', 'candidate')",
    )
    .bind(key)
    .fetch_optional(&pool)
    .await?;

    let result = sqlx::query(
        r#"
        UPDATE memory_entries
        SET status = 'quarantined', updated_at = ?1
        WHERE key = ?2 AND status IN ('active', 'candidate')
        "#,
    )
    .bind(now.to_rfc3339())
    .bind(key)
    .execute(&pool)
    .await?;

    if result.rows_affected() > 0 {
        if let Some(ref id) = entry_id {
            let _ = cleanup_entry_chunks(id).await;
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Restore a quarantined entry back to candidate status.
/// The entry must be re-classified (via classify()) before it becomes active again.
pub async fn restore_from_quarantine(key: &str) -> anyhow::Result<bool> {
    let pool = db::pool().await?;
    let now = Utc::now();
    let result = sqlx::query(
        r#"
        UPDATE memory_entries
        SET status = 'candidate', updated_at = ?1
        WHERE key = ?2 AND status = 'quarantined'
        "#,
    )
    .bind(now.to_rfc3339())
    .bind(key)
    .execute(&pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// List all memory entries with optional category and status filters.
/// If status is None, returns all non-forgotten entries.
pub async fn list_all(
    category: Option<&str>,
    status: Option<&str>,
) -> anyhow::Result<Vec<MemoryEntry>> {
    let pool = db::pool().await?;

    let (query_str, binds): (&str, Vec<String>) = match (category, status) {
        (Some(cat), Some(st)) => (
            "SELECT id, key, value, category, scope, workspace_id, path_prefix, source, confidence,              sensitivity, status, expires_at, last_used_at, created_at, updated_at              FROM memory_entries WHERE category = ?1 AND status = ?2 ORDER BY updated_at DESC",
            vec![cat.to_string(), st.to_string()],
        ),
        (Some(cat), None) => (
            "SELECT id, key, value, category, scope, workspace_id, path_prefix, source, confidence,              sensitivity, status, expires_at, last_used_at, created_at, updated_at              FROM memory_entries WHERE category = ?1 AND status != 'forgotten' ORDER BY updated_at DESC",
            vec![cat.to_string()],
        ),
        (None, Some(st)) => (
            "SELECT id, key, value, category, scope, workspace_id, path_prefix, source, confidence,              sensitivity, status, expires_at, last_used_at, created_at, updated_at              FROM memory_entries WHERE status = ?1 ORDER BY updated_at DESC",
            vec![st.to_string()],
        ),
        (None, None) => (
            "SELECT id, key, value, category, scope, workspace_id, path_prefix, source, confidence,              sensitivity, status, expires_at, last_used_at, created_at, updated_at              FROM memory_entries WHERE status != 'forgotten' ORDER BY updated_at DESC",
            vec![],
        ),
    };

    let mut q = sqlx::query(query_str);
    for bind in &binds {
        q = q.bind(bind);
    }
    let rows = q.fetch_all(&pool).await?;
    rows.into_iter().map(memory_from_row).collect()
}

/// Update the status of a memory entry by its ID.
pub async fn update_status_by_id(id: &str, new_status: &str) -> anyhow::Result<bool> {
    let pool = db::pool().await?;
    let now = Utc::now();
    let result = sqlx::query(
        r#"
        UPDATE memory_entries
        SET status = ?1, updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(new_status)
    .bind(now.to_rfc3339())
    .bind(id)
    .execute(&pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Forget a memory entry by its ID.
pub async fn forget_by_id(id: &str) -> anyhow::Result<bool> {
    update_status_by_id(id, "forgotten").await
}

/// Update the `value` field of a memory entry by its ID.
pub async fn update_value_by_id(id: &str, value: &str) -> anyhow::Result<bool> {
    let pool = db::pool().await?;
    let now = Utc::now();
    let result = sqlx::query("UPDATE memory_entries SET value = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(value)
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(&pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Hard-delete a memory entry and its associated chunks and embeddings by ID.
pub async fn delete_by_id(id: &str) -> anyhow::Result<bool> {
    let pool = db::pool().await?;
    sqlx::query(
        "DELETE FROM memory_embeddings WHERE chunk_id IN (SELECT id FROM memory_chunks WHERE memory_id = ?1)",
    )
    .bind(id)
    .execute(&pool)
    .await?;
    sqlx::query("DELETE FROM memory_chunks WHERE memory_id = ?1")
        .bind(id)
        .execute(&pool)
        .await?;
    let result = sqlx::query("DELETE FROM memory_entries WHERE id = ?1")
        .bind(id)
        .execute(&pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Rebuild all memory chunks and embeddings from active memory entries.
///
/// Uses the global sync `EmbeddingModel` (backward compatible).
pub async fn rebuild_embeddings() -> anyhow::Result<u64> {
    let pool = db::pool().await?;

    sqlx::query("DELETE FROM memory_embeddings")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM memory_chunks")
        .execute(&pool)
        .await?;

    let entries = list_all(None, Some("active")).await?;
    let count = entries.len() as u64;

    for entry in &entries {
        if let Err(e) = index_memory_entry(entry).await {
            tracing::warn!(
                "rebuild: index_memory_entry failed for key={}: {}",
                entry.key,
                e
            );
        }
    }

    Ok(count)
}

/// Rebuild all embeddings using the current model.
/// Preferred over `rebuild_embeddings` for clarity; functionally identical.
pub async fn rebuild_all_embeddings() -> anyhow::Result<u64> {
    rebuild_embeddings().await
}

/// Rebuild stats returned by [`rebuild_embeddings_with_provider`].
#[derive(Debug)]
pub struct RebuildStats {
    pub total: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub model_name: String,
    pub dims: usize,
}

/// Rebuild all embeddings using an explicit async [`EmbeddingProvider`].
///
/// This is the TASK-110 replacement for `rebuild_embeddings()`. It:
/// - Deletes all existing embeddings and chunks
/// - Re-indexes every active memory entry using the given provider
/// - Records `model_name` and `dims` in `memory_embeddings`
/// - Returns [`RebuildStats`] with success/failure counts
pub async fn rebuild_embeddings_with_provider(
    provider: &dyn EmbeddingProvider,
) -> anyhow::Result<RebuildStats> {
    let pool = db::pool().await?;

    sqlx::query("DELETE FROM memory_embeddings")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM memory_chunks")
        .execute(&pool)
        .await?;

    let entries = list_all(None, Some("active")).await?;
    let total = entries.len() as u64;
    let mut succeeded = 0u64;
    let mut failed = 0u64;

    for entry in &entries {
        match index_memory_entry_with_provider(entry, provider).await {
            Ok(()) => succeeded += 1,
            Err(e) => {
                failed += 1;
                tracing::warn!(
                    "rebuild_with_provider: index failed for key={}: {}",
                    entry.key,
                    e
                );
            }
        }
    }

    Ok(RebuildStats {
        total,
        succeeded,
        failed,
        model_name: provider.model_name().to_string(),
        dims: provider.dimension(),
    })
}

/// Index a memory entry using an explicit async provider instead of the global sync model.
async fn index_memory_entry_with_provider(
    entry: &MemoryEntry,
    provider: &dyn EmbeddingProvider,
) -> anyhow::Result<()> {
    let chunk_id = format!("entry-{}", entry.id);
    let content = format!(
        "类别: {}\n键: {}\n内容: {}",
        entry.category, entry.key, entry.value
    );

    let chunk = MemoryChunk {
        id: chunk_id,
        memory_id: entry.id.clone(),
        workspace_id: entry.workspace_id.clone(),
        scope: entry.scope.clone(),
        category: entry.category.clone(),
        content: content.clone(),
        summary: None,
        source: entry.source.clone(),
        sensitivity: entry.sensitivity.clone(),
        confidence: entry.confidence,
        scene_tags: generate_scene_tags(),
        created_at: entry.created_at,
        updated_at: entry.updated_at,
        expires_at: entry.expires_at,
        last_used_at: entry.last_used_at,
    };

    create_memory_chunk(chunk.clone(), Some("memory_entries"), Some(&entry.id)).await?;

    let vector = provider.embed(&content).await?;
    let embedding = MemoryEmbedding {
        chunk_id: chunk.id,
        model: provider.model_name().to_string(),
        dims: vector.len(),
        vector,
        created_at: Utc::now(),
    };
    create_memory_embedding(embedding).await?;

    Ok(())
}

pub async fn purge_forgotten() -> anyhow::Result<u64> {
    let pool = db::pool().await?;
    let result = sqlx::query(
        r#"
        DELETE FROM memory_entries
        WHERE status = 'forgotten'
        "#,
    )
    .execute(&pool)
    .await?;
    Ok(result.rows_affected())
}

/// Aggregate interaction patterns from recent memory chunks.
///
/// Queries memory chunks created in the last `lookback_days` days (default 30),
/// groups them by (category, scene_tag), counts frequencies, and returns
/// patterns with a confidence score proportional to their relative frequency.
///
/// Patterns with `frequency >= 2` are stored back as a memory entry with
/// source="pattern_aggregation" so they are searchable and persist across sessions.
pub async fn aggregate_interaction_patterns(
    lookback_days: u32,
) -> anyhow::Result<Vec<InteractionPattern>> {
    let pool = db::pool().await?;
    let cutoff = (Utc::now() - chrono::Duration::days(lookback_days as i64)).to_rfc3339();

    // Fetch recent non-forgotten chunks with scene_tags
    let rows = sqlx::query(
        r#"
        SELECT mc.category, mc.scene_tags, mc.content
        FROM memory_chunks mc
        LEFT JOIN memory_entries ment ON mc.memory_id = ment.id
        WHERE mc.created_at >= ?1
          AND (ment.id IS NULL OR ment.status NOT IN ('forgotten', 'quarantined'))
        "#,
    )
    .bind(&cutoff)
    .fetch_all(&pool)
    .await?;

    // Count (category, scene_tag) pairs
    let mut counts: HashMap<(String, String), (usize, Vec<String>)> = HashMap::new();
    let mut total = 0usize;

    for row in &rows {
        let category: String = row.try_get("category")?;
        let scene_tags_json: Option<String> = row.try_get("scene_tags")?;
        let _content: String = row.try_get("content")?;

        let tags: Vec<String> = scene_tags_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        if tags.is_empty() {
            continue;
        }

        for tag in &tags {
            let key = (category.clone(), tag.clone());
            let entry = counts.entry(key).or_insert((0, tags.clone()));
            entry.0 += 1;
            total += 1;
        }
    }

    // Build patterns
    let mut patterns: Vec<InteractionPattern> = counts
        .into_iter()
        .map(|((category, tag), (freq, all_tags))| {
            let confidence = if total > 0 {
                freq as f64 / total as f64
            } else {
                0.0
            };
            let key_pattern = format!(
                "{}: {}, frequently accessed in {}",
                category,
                tag,
                all_tags.join("/")
            );
            InteractionPattern {
                category,
                key_pattern,
                frequency: freq,
                scene_tags: all_tags,
                confidence,
            }
        })
        .filter(|p| p.frequency >= 2)
        .collect();

    // Sort by frequency descending
    patterns.sort_by(|a, b| b.frequency.cmp(&a.frequency));

    // Store top patterns as memory entries
    for pattern in &patterns {
        let key = format!("pattern:{}", pattern.key_pattern);
        let value = serde_json::to_string(pattern)?;
        let _ = set_with_source(&key, &value, &pattern.category, "pattern_aggregation").await;
    }

    Ok(patterns)
}

// ── Confidence / reinforcement constants (TASK-112) ─────────────────────

/// Confidence assigned on first observation of a pattern.
const INITIAL_CONFIDENCE: f64 = 0.5;
/// Confidence increment per reinforcement round.
const REINFORCE_STEP: f64 = 0.1;
/// Hard cap for confidence.
const MAX_CONFIDENCE: f64 = 0.9;
/// Threshold at which a candidate is promoted to stable.
const PROMOTE_THRESHOLD: f64 = 0.7;
/// Threshold at which a stable entry is demoted back to candidate.
const DEMOTE_THRESHOLD: f64 = 0.3;
/// Confidence penalty applied when an entry has not been reinforced for this many days.
const DECAY_AFTER_DAYS: i64 = 7;
/// Amount subtracted per decay period.
const DECAY_STEP: f64 = 0.05;

/// Reinforce (or create) an interaction pattern for a workspace.
///
/// - **First observation**: creates a memory entry with `status="candidate"`,
///   `confidence=0.5`, `interaction_count=1`.
/// - **Subsequent reinforcements** (status=candidate or stable): bumps
///   confidence by +0.1 (capped at 0.9) and increments `interaction_count`.
/// - **Promotion**: when confidence reaches >= 0.7 the entry is promoted
///   from `candidate` to `stable`.
pub async fn reinforce_pattern(
    workspace_id: &str,
    pattern_key: &str,
    pattern_kind: &str,
    evidence: &str,
) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let now = Utc::now();

    // Look up existing entry scoped to this workspace
    let existing = sqlx::query(
        r#"
        SELECT id, key, value, category, scope, workspace_id, source, confidence,
               sensitivity, status, expires_at, last_used_at, created_at, updated_at,
               interaction_count, last_reinforced_at
        FROM memory_entries
        WHERE key = ?1 AND workspace_id = ?2 AND scope = 'workspace'
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(pattern_key)
    .bind(workspace_id)
    .fetch_optional(&pool)
    .await?;

    if let Some(row) = existing {
        // ── Update existing entry ───────────────────────────────────────
        let mut entry = memory_from_row(row)?;
        let old_status = entry.status.clone();

        // Reinforce confidence (applicable to candidate and stable)
        if old_status == "candidate" || old_status == "stable" {
            entry.confidence = (entry.confidence + REINFORCE_STEP).min(MAX_CONFIDENCE);
        }

        entry.interaction_count += 1;
        entry.last_reinforced_at = Some(now);
        entry.value = evidence.to_string();
        entry.updated_at = now;

        // Promotion: candidate → stable when confidence crosses threshold
        if old_status == "candidate" && entry.confidence >= PROMOTE_THRESHOLD {
            entry.status = "stable".to_string();
        }

        sqlx::query(
            r#"
            UPDATE memory_entries
            SET value = ?1, confidence = ?2, status = ?3, interaction_count = ?4,
                last_reinforced_at = ?5, updated_at = ?6
            WHERE id = ?7
            "#,
        )
        .bind(&entry.value)
        .bind(entry.confidence)
        .bind(&entry.status)
        .bind(entry.interaction_count)
        .bind(entry.last_reinforced_at.map(|dt| dt.to_rfc3339()))
        .bind(entry.updated_at.to_rfc3339())
        .bind(&entry.id)
        .execute(&pool)
        .await?;

        // Re-index for search
        if let Err(e) = index_memory_entry(&entry).await {
            tracing::warn!(
                "index_memory_entry failed during reinforce for key={}: {}",
                entry.key,
                e
            );
        }
    } else {
        // ── Create new candidate ────────────────────────────────────────
        let entry = MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            key: pattern_key.to_string(),
            value: evidence.to_string(),
            category: pattern_kind.to_string(),
            scope: MemoryScope::Workspace,
            workspace_id: Some(workspace_id.to_string()),
            path_prefix: None,
            source: MemorySource::PatternAggregation,
            confidence: INITIAL_CONFIDENCE,
            sensitivity: MemorySensitivity::Normal,
            status: "candidate".to_string(),
            scene_tags: Vec::new(),
            expires_at: None,
            last_used_at: None,
            created_at: now,
            updated_at: now,
            interaction_count: 1,
            last_reinforced_at: Some(now),
        };

        sqlx::query(
            r#"
            INSERT INTO memory_entries (id, key, value, category, scope, workspace_id, path_prefix, source,
                                       confidence, sensitivity, status, expires_at, last_used_at,
                                       created_at, updated_at, interaction_count, last_reinforced_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            "#,
        )
        .bind(&entry.id)
        .bind(&entry.key)
        .bind(&entry.value)
        .bind(&entry.category)
        .bind(entry.scope.as_str())
        .bind(&entry.workspace_id)
        .bind(&entry.path_prefix)
        .bind(entry.source.as_str())
        .bind(entry.confidence)
        .bind(entry.sensitivity.as_str())
        .bind(&entry.status)
        .bind(entry.expires_at.map(|dt| dt.to_rfc3339()))
        .bind(entry.last_used_at.map(|dt| dt.to_rfc3339()))
        .bind(entry.created_at.to_rfc3339())
        .bind(entry.updated_at.to_rfc3339())
        .bind(entry.interaction_count)
        .bind(entry.last_reinforced_at.map(|dt| dt.to_rfc3339()))
        .execute(&pool)
        .await?;

        if let Err(e) = index_memory_entry(&entry).await {
            tracing::warn!(
                "index_memory_entry failed during reinforce create for key={}: {}",
                entry.key,
                e
            );
        }
    }

    Ok(())
}

/// Apply time-based confidence decay to pattern entries.
///
/// For every candidate/stable entry whose `last_reinforced_at` is older than
/// 7 days, confidence is reduced by 0.05. If a stable entry's confidence
/// drops below 0.3 it is demoted back to candidate.
///
/// Returns the number of entries affected.
pub async fn apply_confidence_decay() -> anyhow::Result<u64> {
    let pool = db::pool().await?;
    let now = Utc::now();
    let cutoff = (now - chrono::Duration::days(DECAY_AFTER_DAYS)).to_rfc3339();

    let rows = sqlx::query(
        r#"
        SELECT id, key, value, category, scope, workspace_id, source, confidence,
               sensitivity, status, expires_at, last_used_at, created_at, updated_at,
               interaction_count, last_reinforced_at
        FROM memory_entries
        WHERE status IN ('candidate', 'stable')
          AND last_reinforced_at IS NOT NULL
          AND last_reinforced_at < ?1
        "#,
    )
    .bind(&cutoff)
    .fetch_all(&pool)
    .await?;

    let mut affected = 0u64;
    for row in rows {
        let mut entry = memory_from_row(row)?;
        entry.confidence = (entry.confidence - DECAY_STEP).max(0.0);
        entry.updated_at = now;

        // Demotion: stable → candidate when confidence drops below threshold
        if entry.status == "stable" && entry.confidence < DEMOTE_THRESHOLD {
            entry.status = "candidate".to_string();
        }

        sqlx::query(
            r#"
            UPDATE memory_entries
            SET confidence = ?1, status = ?2, updated_at = ?3
            WHERE id = ?4
            "#,
        )
        .bind(entry.confidence)
        .bind(&entry.status)
        .bind(entry.updated_at.to_rfc3339())
        .bind(&entry.id)
        .execute(&pool)
        .await?;

        affected += 1;
    }

    Ok(affected)
}

pub async fn save_preferences(prefs: &UserPreferences) -> anyhow::Result<()> {
    let json = serde_json::to_string(prefs)?;
    set("user_preferences", &json, "preferences").await?;
    Ok(())
}

pub async fn load_preferences() -> anyhow::Result<UserPreferences> {
    match get("user_preferences").await? {
        Some(json) => Ok(serde_json::from_str(&json)?),
        None => Ok(UserPreferences::default()),
    }
}

pub async fn add_conversation_summary(
    summary: &str,
    keywords: &[String],
) -> anyhow::Result<ConversationSummary> {
    let pool = db::pool().await?;
    let now = Utc::now();
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO conversation_summaries (id, summary, keywords, timestamp)
        VALUES (?1, ?2, ?3, ?4)
        "#,
    )
    .bind(&id)
    .bind(summary)
    .bind(serde_json::to_string(keywords)?)
    .bind(now.to_rfc3339())
    .execute(&pool)
    .await?;

    let cs = ConversationSummary {
        id,
        summary: summary.to_string(),
        keywords: keywords.to_vec(),
        timestamp: now,
    };

    // Write-through: index into chunks/embeddings for search
    if let Err(e) = index_conversation_summary(&cs).await {
        tracing::warn!("index_conversation_summary failed for id={}: {}", cs.id, e);
    }

    Ok(cs)
}

pub async fn get_recent_conversations(limit: usize) -> anyhow::Result<Vec<ConversationSummary>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, summary, keywords, timestamp
        FROM conversation_summaries
        ORDER BY timestamp DESC
        LIMIT ?1
        "#,
    )
    .bind(limit as i64)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(conversation_from_row).collect()
}

pub async fn search_conversations(query: &str) -> anyhow::Result<Vec<ConversationSummary>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, summary, keywords, timestamp
        FROM conversation_summaries
        WHERE summary LIKE ?1 OR keywords LIKE ?1
        ORDER BY timestamp DESC
        "#,
    )
    .bind(format!("%{}%", query))
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(conversation_from_row).collect()
}

/// Multi-path recall for prompt injection.
///
/// Gathers relevant memories from two sources:
/// 1. **Semantic search** via `search_memory_filtered` — finds memory entry chunks
///    and conversation summary chunks ranked by vector similarity + keyword match.
/// 2. **Keyword search** via `search_conversations` — finds conversation summaries
///    that directly match the context text.
///
/// Results from both paths are deduplicated: entries by `MemoryEntry.id`,
/// summaries by `ConversationSummary.id`.
pub async fn recall_for_prompt(
    context: &str,
    workspace_id: Option<&str>,
    limit: usize,
) -> anyhow::Result<RecallResult> {
    let recall_context = RecallContext {
        query: context.to_string(),
        workspace_id: workspace_id.map(str::to_string),
        path_prefix: None,
        session_id: None,
        goal_id: None,
        limit,
    };

    recall_for_prompt_with_context(&recall_context).await
}

/// Multi-path recall for prompt injection using an explicit retrieval context.
pub async fn recall_for_prompt_with_context(
    context: &RecallContext,
) -> anyhow::Result<RecallResult> {
    let pool = db::pool().await?;
    let limit = if context.limit == 0 { 5 } else { context.limit };

    // Path 1: Semantic search over all indexed chunks (entries + summaries)
    let search_results = search_memory_filtered(
        &context.query,
        context.workspace_id.as_deref(),
        limit,
        SearchFilter {
            path_prefix: context
                .path_prefix
                .as_deref()
                .and_then(normalize_path_prefix),
            session_id: context
                .session_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            goal_id: context
                .goal_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            ..SearchFilter::default()
        },
    )
    .await?;

    let mut seen_entry_ids = std::collections::HashSet::new();
    let mut seen_summary_ids = std::collections::HashSet::new();
    let mut entries = Vec::new();
    let mut summaries = Vec::new();

    for result in &search_results {
        if result.chunk.id.starts_with("summary-") {
            let sid = &result.chunk.memory_id;
            if seen_summary_ids.insert(sid.clone()) {
                if let Some(cs) = fetch_summary_by_id(&pool, sid).await? {
                    summaries.push(cs);
                }
            }
        } else {
            let eid = &result.chunk.memory_id;
            if seen_entry_ids.insert(eid.clone()) {
                if let Some(entry) = fetch_entry_by_id(&pool, eid).await? {
                    entries.push(entry);
                }
            }
        }
    }

    // Path 2: Keyword search over conversation_summaries
    let keyword_summaries = search_conversations(&context.query).await?;
    for cs in keyword_summaries {
        if seen_summary_ids.insert(cs.id.clone()) {
            summaries.push(cs);
        }
    }

    // Count total chunks for diagnostics
    let total_chunks_searched = seen_entry_ids.len() + seen_summary_ids.len();

    Ok(RecallResult {
        entries,
        summaries,
        total_chunks_searched,
    })
}

/// Fetch a single `MemoryEntry` by its primary key.
/// Scene tags are aggregated from the entry's chunks (memory_chunks table).
async fn fetch_entry_by_id(
    pool: &sqlx::SqlitePool,
    id: &str,
) -> anyhow::Result<Option<MemoryEntry>> {
    let row = sqlx::query(
        r#"
        SELECT id, key, value, category, scope, workspace_id, path_prefix, source, confidence,
               sensitivity, status, expires_at, last_used_at, created_at, updated_at
        FROM memory_entries
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    match row.map(memory_from_row).transpose()? {
        Some(mut entry) => {
            // Enrich with scene_tags from chunks
            let tag_rows = sqlx::query("SELECT scene_tags FROM memory_chunks WHERE memory_id = ?1")
                .bind(id)
                .fetch_all(pool)
                .await?;

            let mut tags = std::collections::HashSet::new();
            for tr in &tag_rows {
                if let Ok(json_str) = tr.try_get::<String, _>("scene_tags") {
                    if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&json_str) {
                        tags.extend(parsed);
                    }
                }
            }
            let mut scene_tags: Vec<String> = tags.into_iter().collect();
            scene_tags.sort();
            entry.scene_tags = scene_tags;

            Ok(Some(entry))
        }
        None => Ok(None),
    }
}

/// Fetch a single `ConversationSummary` by its primary key.
async fn fetch_summary_by_id(
    pool: &sqlx::SqlitePool,
    id: &str,
) -> anyhow::Result<Option<ConversationSummary>> {
    let row = sqlx::query(
        r#"
        SELECT id, summary, keywords, timestamp
        FROM conversation_summaries
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    row.map(conversation_from_row).transpose()
}

fn memory_from_row(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<MemoryEntry> {
    Ok(MemoryEntry {
        id: row.try_get("id")?,
        key: row.try_get("key")?,
        value: row.try_get("value")?,
        category: row.try_get("category")?,
        scope: MemoryScope::from_str(row.try_get::<String, _>("scope")?.as_str())?,
        workspace_id: row.try_get("workspace_id")?,
        path_prefix: row.try_get("path_prefix").ok().flatten(),
        source: MemorySource::from_str(row.try_get::<String, _>("source")?.as_str())?,
        confidence: row.try_get("confidence")?,
        sensitivity: MemorySensitivity::from_str(
            row.try_get::<String, _>("sensitivity")?.as_str(),
        )?,
        status: row
            .try_get::<String, _>("status")
            .unwrap_or_else(|_| "active".to_string()),
        scene_tags: Vec::new(), // enriched separately from chunks
        expires_at: row
            .try_get::<Option<String>, _>("expires_at")?
            .as_deref()
            .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
        last_used_at: row
            .try_get::<Option<String>, _>("last_used_at")?
            .as_deref()
            .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
        created_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("updated_at")?.as_str())?
            .with_timezone(&Utc),
        interaction_count: row.try_get::<i64, _>("interaction_count").unwrap_or(0),
        last_reinforced_at: row
            .try_get::<Option<String>, _>("last_reinforced_at")
            .ok()
            .flatten()
            .and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            }),
    })
}

pub async fn create_memory_chunk(
    chunk: MemoryChunk,
    origin_table: Option<&str>,
    origin_id: Option<&str>,
) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let scene_tags_json = serde_json::to_string(&chunk.scene_tags)?;
    sqlx::query(
        r#"
        INSERT INTO memory_chunks (
            id, memory_id, workspace_id, scope, category, content, summary, source,
            sensitivity, confidence, scene_tags, created_at, updated_at, expires_at, last_used_at,
            origin_table, origin_id
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        ON CONFLICT(id) DO UPDATE SET
            content = excluded.content,
            summary = excluded.summary,
            source = excluded.source,
            sensitivity = excluded.sensitivity,
            confidence = excluded.confidence,
            scene_tags = excluded.scene_tags,
            updated_at = excluded.updated_at,
            origin_table = excluded.origin_table,
            origin_id = excluded.origin_id
        "#,
    )
    .bind(&chunk.id)
    .bind(&chunk.memory_id)
    .bind(&chunk.workspace_id)
    .bind(chunk.scope.as_str())
    .bind(&chunk.category)
    .bind(&chunk.content)
    .bind(&chunk.summary)
    .bind(chunk.source.as_str())
    .bind(chunk.sensitivity.as_str())
    .bind(chunk.confidence)
    .bind(&scene_tags_json)
    .bind(chunk.created_at.to_rfc3339())
    .bind(chunk.updated_at.to_rfc3339())
    .bind(chunk.expires_at.map(|dt| dt.to_rfc3339()))
    .bind(chunk.last_used_at.map(|dt| dt.to_rfc3339()))
    .bind(origin_table)
    .bind(origin_id)
    .execute(&pool)
    .await?;
    Ok(())
}

/// Create a `MemoryChunk` for a given memory entry, automatically attaching
/// scene context tags (time of day, weekday/weekend) at creation time.
///
/// This is the recommended way to create chunks so that scene tags are always present.
pub fn new_chunk_with_scene_tags(
    memory_id: &str,
    workspace_id: Option<&str>,
    scope: MemoryScope,
    category: &str,
    content: &str,
    summary: Option<&str>,
    source: MemorySource,
    sensitivity: MemorySensitivity,
    confidence: f64,
) -> MemoryChunk {
    let now = Utc::now();
    MemoryChunk {
        id: uuid::Uuid::new_v4().to_string(),
        memory_id: memory_id.to_string(),
        workspace_id: workspace_id.map(|s| s.to_string()),
        scope,
        category: category.to_string(),
        content: content.to_string(),
        summary: summary.map(|s| s.to_string()),
        source,
        sensitivity,
        confidence,
        scene_tags: generate_scene_tags(),
        created_at: now,
        updated_at: now,
        expires_at: None,
        last_used_at: None,
    }
}

pub async fn create_memory_embedding(embedding: MemoryEmbedding) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let vector_bytes: Vec<u8> = embedding
        .vector
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let vector_blob = vector_bytes.as_slice();
    sqlx::query(
        r#"
        INSERT INTO memory_embeddings (chunk_id, model, dims, vector, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(chunk_id) DO UPDATE SET
            model = excluded.model,
            dims = excluded.dims,
            vector = excluded.vector,
            created_at = excluded.created_at
        "#,
    )
    .bind(&embedding.chunk_id)
    .bind(&embedding.model)
    .bind(embedding.dims as i64)
    .bind(vector_blob)
    .bind(embedding.created_at.to_rfc3339())
    .execute(&pool)
    .await?;
    Ok(())
}

/// Index a MemoryEntry into memory_chunks + memory_embeddings so search_memory can find it.
/// Uses a deterministic chunk ID based on entry.id for idempotent upserts.
pub async fn index_memory_entry(entry: &MemoryEntry) -> anyhow::Result<()> {
    let chunk_id = format!("entry-{}", entry.id);
    let content = format!(
        "类别: {}\n键: {}\n内容: {}",
        entry.category, entry.key, entry.value
    );

    let chunk = MemoryChunk {
        id: chunk_id,
        memory_id: entry.id.clone(),
        workspace_id: entry.workspace_id.clone(),
        scope: entry.scope.clone(),
        category: entry.category.clone(),
        content: content.clone(),
        summary: None,
        source: entry.source.clone(),
        sensitivity: entry.sensitivity.clone(),
        confidence: entry.confidence,
        scene_tags: generate_scene_tags(),
        created_at: entry.created_at,
        updated_at: entry.updated_at,
        expires_at: entry.expires_at,
        last_used_at: entry.last_used_at,
    };

    create_memory_chunk(chunk.clone(), Some("memory_entries"), Some(&entry.id)).await?;

    let model_name = current_model_name();
    let vector = generate_embedding(&content).await?;
    let embedding = MemoryEmbedding {
        chunk_id: chunk.id,
        model: model_name,
        dims: vector.len(),
        vector,
        created_at: Utc::now(),
    };
    create_memory_embedding(embedding).await?;

    Ok(())
}

/// Index a ConversationSummary into memory_chunks + memory_embeddings so search_memory can find it.
/// Uses a deterministic chunk ID based on summary.id for idempotent upserts.
pub async fn index_conversation_summary(summary: &ConversationSummary) -> anyhow::Result<()> {
    let chunk_id = format!("summary-{}", summary.id);
    let keywords_str = summary.keywords.join(", ");
    let content = format!("对话摘要: {}\n关键词: {}", summary.summary, keywords_str);

    let chunk = MemoryChunk {
        id: chunk_id,
        memory_id: summary.id.clone(),
        workspace_id: None,
        scope: MemoryScope::Global,
        category: "conversation_summary".to_string(),
        content: content.clone(),
        summary: Some(summary.summary.clone()),
        source: MemorySource::Summary,
        sensitivity: MemorySensitivity::Normal,
        confidence: 0.9,
        scene_tags: generate_scene_tags(),
        created_at: summary.timestamp,
        updated_at: summary.timestamp,
        expires_at: None,
        last_used_at: None,
    };

    create_memory_chunk(
        chunk.clone(),
        Some("conversation_summaries"),
        Some(&summary.id),
    )
    .await?;

    let model_name = current_model_name();
    let vector = generate_embedding(&content).await?;
    let embedding = MemoryEmbedding {
        chunk_id: chunk.id,
        model: model_name,
        dims: vector.len(),
        vector,
        created_at: Utc::now(),
    };
    create_memory_embedding(embedding).await?;

    Ok(())
}

/// Re-index an existing memory_chunks row by regenerating its embedding.
/// Useful after provider changes or embedding dimension updates. (文档 B §十 阶段 1)
pub async fn reindex_memory_chunk(chunk_id: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let row = sqlx::query("SELECT content FROM memory_chunks WHERE id = ?1")
        .bind(chunk_id)
        .fetch_optional(&pool)
        .await?;
    let Some(row) = row else {
        anyhow::bail!("memory_chunk not found: {chunk_id}");
    };
    let content: String = sqlx::Row::try_get(&row, "content")?;

    // Delete existing embedding for this chunk
    sqlx::query("DELETE FROM memory_embeddings WHERE chunk_id = ?1")
        .bind(chunk_id)
        .execute(&pool)
        .await?;

    // Regenerate and store new embedding
    let model_name = current_model_name();
    let vector = generate_embedding(&content).await?;
    let embedding = MemoryEmbedding {
        chunk_id: chunk_id.to_string(),
        model: model_name,
        dims: vector.len(),
        vector,
        created_at: Utc::now(),
    };
    create_memory_embedding(embedding).await?;

    Ok(())
}

/// Remove all chunks (and their embeddings) that originated from a given memory_entries row.
/// Called by archive/forget/quarantine so the entry disappears from search results.
async fn cleanup_entry_chunks(entry_id: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;

    sqlx::query(
        r#"
        DELETE FROM memory_embeddings
        WHERE chunk_id IN (
            SELECT id FROM memory_chunks WHERE memory_id = ?1
        )
        "#,
    )
    .bind(entry_id)
    .execute(&pool)
    .await?;

    sqlx::query("DELETE FROM memory_chunks WHERE memory_id = ?1")
        .bind(entry_id)
        .execute(&pool)
        .await?;

    Ok(())
}

/// Search memory with default filter (excludes quarantined, forgotten, archived;
/// allows all sensitivity levels and categories).
pub async fn search_memory(
    query: &str,
    workspace_id: Option<&str>,
    limit: usize,
) -> anyhow::Result<Vec<MemorySearchResult>> {
    search_memory_filtered(query, workspace_id, limit, SearchFilter::default()).await
}

/// Search memory with explicit filter control.
///
/// The `filter` parameter controls which entries are excluded from results
/// based on status (quarantined/forgotten/archived), sensitivity level, and category.
pub async fn search_memory_filtered(
    query: &str,
    workspace_id: Option<&str>,
    limit: usize,
    filter: SearchFilter,
) -> anyhow::Result<Vec<MemorySearchResult>> {
    let pool = db::pool().await?;
    let query_embedding = generate_embedding(query).await?;

    // Build the status exclusion list dynamically
    let mut excluded_statuses: Vec<&str> = Vec::new();
    if filter.exclude_quarantined {
        excluded_statuses.push("quarantined");
    }
    if filter.exclude_forgotten {
        excluded_statuses.push("forgotten");
    }
    if filter.exclude_archived {
        excluded_statuses.push("archived");
    }

    // Determine max sensitivity level to allow (default: secret = allow all)
    let max_sens_level = filter
        .max_sensitivity
        .as_ref()
        .map(|s| s.level())
        .unwrap_or(MemorySensitivity::Secret.level());

    let now = Utc::now().to_rfc3339();
    let normalized_path_prefix = filter
        .path_prefix
        .as_deref()
        .and_then(normalize_path_prefix);
    let normalized_session_id = filter
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let normalized_goal_id = filter
        .goal_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    // Build query dynamically since sqlx compile-time macros can't handle
    // a variable number of IN(...) placeholders.
    let mut sql = String::from(
        "SELECT mc.*, me.vector \
         FROM memory_chunks mc \
         LEFT JOIN memory_embeddings me ON mc.id = me.chunk_id \
         LEFT JOIN memory_entries ment ON mc.memory_id = ment.id \
         WHERE (mc.expires_at IS NULL OR mc.expires_at > $1)",
    );

    let mut bind_idx: u32 = 2;

    // Workspace/scope filter
    if workspace_id.is_some() {
        sql.push_str(&format!(
            " AND (mc.scope = 'global' OR mc.workspace_id = ${})",
            bind_idx
        ));
        bind_idx += 1;
    } else {
        sql.push_str(" AND mc.scope = 'global'");
    }

    if normalized_path_prefix.is_some() {
        sql.push_str(&format!(
            " AND (ment.id IS NULL OR ment.path_prefix IS NULL OR ${0} = REPLACE(ment.path_prefix, '\\', '/') OR ${1} LIKE REPLACE(ment.path_prefix, '\\', '/') || '/%')",
            bind_idx,
            bind_idx + 1
        ));
        bind_idx += 2;
    }

    if normalized_session_id.is_some() {
        sql.push_str(&format!(
            " AND (ment.id IS NULL OR ment.source_session_id IS NULL OR ment.source_session_id = ${})",
            bind_idx
        ));
        bind_idx += 1;
    }

    if normalized_goal_id.is_some() {
        sql.push_str(&format!(
            " AND (ment.id IS NULL OR ment.goal_id IS NULL OR ment.goal_id = ${})",
            bind_idx
        ));
        bind_idx += 1;
    }

    // Status exclusion filter
    if !excluded_statuses.is_empty() {
        let placeholders: Vec<String> = excluded_statuses
            .iter()
            .map(|_| {
                let p = format!("${}", bind_idx);
                bind_idx += 1;
                p
            })
            .collect();
        sql.push_str(&format!(
            " AND (ment.id IS NULL OR ment.status NOT IN ({}))",
            placeholders.join(", ")
        ));
    }

    // Sensitivity filter: exclude chunks whose sensitivity level exceeds the max
    if max_sens_level < MemorySensitivity::Secret.level() {
        let disallowed: Vec<&str> = match max_sens_level {
            0 => vec!["private", "secret"],
            1 => vec!["secret"],
            _ => vec![],
        };
        if !disallowed.is_empty() {
            let placeholders: Vec<String> = disallowed
                .iter()
                .map(|_| {
                    let p = format!("${}", bind_idx);
                    bind_idx += 1;
                    p
                })
                .collect();
            sql.push_str(&format!(
                " AND mc.sensitivity NOT IN ({})",
                placeholders.join(", ")
            ));
        }
    }

    // Category filter
    if filter.category.is_some() {
        sql.push_str(&format!(" AND mc.category = ${}", bind_idx));
        bind_idx += 1;
    }

    sql.push_str(&format!(" ORDER BY mc.updated_at DESC LIMIT ${}", bind_idx));

    // Now build the actual query with binds in the same order
    let mut q = sqlx::query(&sql);
    q = q.bind(&now);

    if let Some(wid) = workspace_id {
        q = q.bind(wid);
    }

    if let Some(path_prefix) = normalized_path_prefix.as_deref() {
        q = q.bind(path_prefix);
        q = q.bind(path_prefix);
    }

    if let Some(session_id) = normalized_session_id {
        q = q.bind(session_id);
    }

    if let Some(goal_id) = normalized_goal_id {
        q = q.bind(goal_id);
    }

    for status in &excluded_statuses {
        q = q.bind(*status);
    }

    // Sensitivity binds
    if max_sens_level < MemorySensitivity::Secret.level() {
        let disallowed: Vec<&str> = match max_sens_level {
            0 => vec!["private", "secret"],
            1 => vec!["secret"],
            _ => vec![],
        };
        for sens in &disallowed {
            q = q.bind(*sens);
        }
    }

    if let Some(ref cat) = filter.category {
        q = q.bind(cat.as_str());
    }

    q = q.bind(limit as i64 * 10);

    let rows = q.fetch_all(&pool).await?;

    let mut results = Vec::new();
    for row in rows {
        let vector: Option<Vec<f32>> = match row.try_get::<&[u8], _>("vector") {
            Ok(blob) => Some(
                blob.chunks(4)
                    .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                    .collect(),
            ),
            Err(_) => None,
        };

        let chunk = chunk_from_row(row)?;
        let vector_score = if let Some(vec) = vector {
            cosine_similarity(&query_embedding, &vec)
        } else {
            0.0
        };

        let keyword_score = keyword_match(query, &chunk.content);
        let recency_score = recency_decay(&chunk.updated_at);
        let authority_score = source_weight(&chunk.source, chunk.confidence);

        let final_score = 0.55 * vector_score
            + 0.20 * keyword_score
            + 0.15 * recency_score
            + 0.10 * authority_score;

        results.push(MemorySearchResult {
            chunk,
            score: final_score,
            score_details: ScoreDetails {
                vector_score,
                keyword_score,
                recency_score,
                authority_score,
            },
        });
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(results.into_iter().take(limit).collect())
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot_product: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as f64) * (*y as f64))
        .sum();
    let norm_a: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}

fn keyword_match(query: &str, content: &str) -> f64 {
    let query_words: Vec<&str> = query.split_whitespace().collect();
    if query_words.is_empty() {
        return 0.0;
    }

    let matches = query_words
        .iter()
        .filter(|word| content.to_lowercase().contains(&word.to_lowercase()))
        .count();

    matches as f64 / query_words.len() as f64
}

fn recency_decay(updated_at: &DateTime<Utc>) -> f64 {
    let now = Utc::now();
    let hours_since_update = (now - *updated_at).num_hours() as f64;

    if hours_since_update < 1.0 {
        1.0
    } else if hours_since_update < 24.0 {
        0.75
    } else if hours_since_update < 72.0 {
        0.5
    } else {
        0.25
    }
}

fn source_weight(source: &MemorySource, confidence: f64) -> f64 {
    let source_multiplier = match source {
        MemorySource::UserConfirmed => 1.0,
        MemorySource::Summary => 0.9,
        MemorySource::Tool => 0.7,
        MemorySource::Inferred => 0.5,
        MemorySource::PatternAggregation => 0.6,
    };
    source_multiplier * confidence
}

async fn generate_embedding(text: &str) -> anyhow::Result<Vec<f32>> {
    let guard = embedding_model().lock().unwrap();
    Ok(guard.embed(text))
}

fn chunk_from_row(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<MemoryChunk> {
    let scene_tags: Vec<String> = row
        .try_get::<Option<String>, _>("scene_tags")?
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    Ok(MemoryChunk {
        id: row.try_get("id")?,
        memory_id: row.try_get("memory_id")?,
        workspace_id: row.try_get("workspace_id")?,
        scope: MemoryScope::from_str(row.try_get::<String, _>("scope")?.as_str())?,
        category: row.try_get("category")?,
        content: row.try_get("content")?,
        summary: row.try_get("summary")?,
        source: MemorySource::from_str(row.try_get::<String, _>("source")?.as_str())?,
        sensitivity: MemorySensitivity::from_str(
            row.try_get::<String, _>("sensitivity")?.as_str(),
        )?,
        confidence: row.try_get("confidence")?,
        scene_tags,
        created_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("updated_at")?.as_str())?
            .with_timezone(&Utc),
        expires_at: row
            .try_get::<Option<String>, _>("expires_at")?
            .as_deref()
            .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
        last_used_at: row
            .try_get::<Option<String>, _>("last_used_at")?
            .as_deref()
            .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
    })
}

fn conversation_from_row(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<ConversationSummary> {
    Ok(ConversationSummary {
        id: row.try_get("id")?,
        summary: row.try_get("summary")?,
        keywords: serde_json::from_str(row.try_get::<String, _>("keywords")?.as_str())?,
        timestamp: DateTime::parse_from_rfc3339(row.try_get::<String, _>("timestamp")?.as_str())?
            .with_timezone(&Utc),
    })
}

pub async fn init_db() -> anyhow::Result<()> {
    let pool = db::pool().await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS memory_entries (
            id TEXT PRIMARY KEY,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            category TEXT NOT NULL,
            scope TEXT NOT NULL DEFAULT 'global',
            workspace_id TEXT,
            path_prefix TEXT,
            source_session_id TEXT,
            source_turn_id TEXT,
            source_message_id TEXT,
            source_projection_id TEXT,
            source_tool_call_id TEXT,
            goal_id TEXT,
            source TEXT NOT NULL DEFAULT 'user_confirmed',
            confidence REAL NOT NULL DEFAULT 1.0,
            sensitivity TEXT NOT NULL DEFAULT 'normal',
            expires_at TEXT,
            last_used_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;
    for ddl in [
        "ALTER TABLE memory_entries ADD COLUMN source_session_id TEXT",
        "ALTER TABLE memory_entries ADD COLUMN source_turn_id TEXT",
        "ALTER TABLE memory_entries ADD COLUMN source_message_id TEXT",
        "ALTER TABLE memory_entries ADD COLUMN source_projection_id TEXT",
        "ALTER TABLE memory_entries ADD COLUMN source_tool_call_id TEXT",
        "ALTER TABLE memory_entries ADD COLUMN goal_id TEXT",
    ] {
        let _ = sqlx::query(ddl).execute(&pool).await;
    }

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_key ON memory_entries(key);")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_category ON memory_entries(category);")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_scope ON memory_entries(scope);")
        .execute(&pool)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_memory_workspace_id ON memory_entries(workspace_id);",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_memory_path_prefix ON memory_entries(path_prefix);",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_memory_source_session ON memory_entries(source_session_id);",
    )
    .execute(&pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_goal ON memory_entries(goal_id);")
        .execute(&pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS conversation_summaries (
            id TEXT PRIMARY KEY,
            summary TEXT NOT NULL,
            keywords TEXT NOT NULL,
            timestamp TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_conversation_timestamp ON conversation_summaries(timestamp);",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS memory_chunks (
            id TEXT PRIMARY KEY,
            memory_id TEXT NOT NULL,
            workspace_id TEXT,
            scope TEXT NOT NULL,
            category TEXT NOT NULL,
            content TEXT NOT NULL,
            summary TEXT,
            source TEXT NOT NULL,
            sensitivity TEXT NOT NULL,
            confidence REAL NOT NULL,
            scene_tags TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            expires_at TEXT,
            last_used_at TEXT
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Migrate: add scene_tags column if table was created before this column existed.
    // SQLite silently ignores "IF NOT COLUMN" isn't native, so we catch the error.
    let _ =
        sqlx::query("ALTER TABLE memory_chunks ADD COLUMN scene_tags TEXT NOT NULL DEFAULT '[]'")
            .execute(&pool)
            .await;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunk_memory_id ON memory_chunks(memory_id);")
        .execute(&pool)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_chunk_workspace_id ON memory_chunks(workspace_id);",
    )
    .execute(&pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunk_scope ON memory_chunks(scope);")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunk_category ON memory_chunks(category);")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunk_updated_at ON memory_chunks(updated_at);")
        .execute(&pool)
        .await?;

    // Migration: add origin_table/origin_id columns for write-through indexing
    let rows = sqlx::query("PRAGMA table_info(memory_chunks);")
        .fetch_all(&pool)
        .await?;
    let has_origin_table = rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .any(|name| name == "origin_table");

    if !has_origin_table {
        sqlx::query("ALTER TABLE memory_chunks ADD COLUMN origin_table TEXT;")
            .execute(&pool)
            .await?;
        sqlx::query("ALTER TABLE memory_chunks ADD COLUMN origin_id TEXT;")
            .execute(&pool)
            .await?;
    }

    // Migration: add interaction_count / last_reinforced_at for pattern reinforcement (TASK-112)
    let _ = sqlx::query(
        "ALTER TABLE memory_entries ADD COLUMN interaction_count INTEGER NOT NULL DEFAULT 0",
    )
    .execute(&pool)
    .await;
    let _ = sqlx::query("ALTER TABLE memory_entries ADD COLUMN last_reinforced_at TEXT")
        .execute(&pool)
        .await;
    let _ = sqlx::query("ALTER TABLE memory_entries ADD COLUMN path_prefix TEXT")
        .execute(&pool)
        .await;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_chunk_origin ON memory_chunks(origin_table, origin_id);",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS memory_embeddings (
            chunk_id TEXT PRIMARY KEY,
            model TEXT NOT NULL,
            dims INTEGER NOT NULL,
            vector BLOB NOT NULL,
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_embedding_model ON memory_embeddings(model);")
        .execute(&pool)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn test_set_and_get() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("test_key", "test_value", "test_category")
            .await
            .expect("set");
        let value = get("test_key").await.expect("get");

        assert_eq!(value, Some("test_value".to_string()));
    }

    #[tokio::test]
    async fn test_set_updates_existing_key() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let first = set("test_key", "first", "test_category")
            .await
            .expect("set");
        let second = set("test_key", "second", "other_category")
            .await
            .expect("update");

        assert_eq!(first.id, second.id);
        assert_eq!(
            get("test_key").await.expect("get"),
            Some("second".to_string())
        );
        assert!(get_by_category("test_category")
            .await
            .expect("old category")
            .is_empty());
        assert_eq!(
            get_by_category("other_category")
                .await
                .expect("new category")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let value = get("nonexistent").await.expect("get");
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_save_and_load_preferences() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let prefs = UserPreferences {
            favorite_topics: vec!["coding".to_string(), "music".to_string()],
            preferred_time: "morning".to_string(),
            chat_style: "friendly".to_string(),
            avatar_settings: HashMap::new(),
        };

        save_preferences(&prefs).await.expect("save");
        let loaded = load_preferences().await.expect("load");

        assert_eq!(loaded.favorite_topics, prefs.favorite_topics);
        assert_eq!(loaded.preferred_time, prefs.preferred_time);
        assert_eq!(loaded.chat_style, prefs.chat_style);
    }

    #[tokio::test]
    async fn test_conversation_summary() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        add_conversation_summary(
            "Test conversation about coding",
            &["coding".to_string(), "rust".to_string()],
        )
        .await
        .expect("add summary");

        let summaries = get_recent_conversations(10).await.expect("get summaries");
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].summary.contains("coding"));
    }

    #[tokio::test]
    async fn test_memory_chunk_creation() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let chunk = MemoryChunk {
            id: uuid::Uuid::new_v4().to_string(),
            memory_id: "memory_123".to_string(),
            workspace_id: Some("ws_123".to_string()),
            scope: MemoryScope::Workspace,
            category: "test_category".to_string(),
            content: "This is a test memory chunk content.".to_string(),
            summary: Some("Test summary".to_string()),
            source: MemorySource::UserConfirmed,
            sensitivity: MemorySensitivity::Normal,
            confidence: 1.0,
            scene_tags: vec!["morning".to_string(), "weekday".to_string()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            last_used_at: None,
        };

        create_memory_chunk(chunk, None, None)
            .await
            .expect("create memory chunk");
    }

    #[tokio::test]
    async fn test_memory_embedding_creation() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let chunk = MemoryChunk {
            id: uuid::Uuid::new_v4().to_string(),
            memory_id: "memory_456".to_string(),
            workspace_id: None,
            scope: MemoryScope::Global,
            category: "test_category".to_string(),
            content: "This is another test chunk.".to_string(),
            summary: None,
            source: MemorySource::Summary,
            sensitivity: MemorySensitivity::Normal,
            confidence: 0.9,
            scene_tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            last_used_at: None,
        };

        create_memory_chunk(chunk.clone(), None, None)
            .await
            .expect("create memory chunk");

        let embedding = MemoryEmbedding {
            chunk_id: chunk.id.clone(),
            model: "test-model".to_string(),
            dims: 384,
            vector: vec![0.0f32; 384],
            created_at: Utc::now(),
        };

        create_memory_embedding(embedding)
            .await
            .expect("create embedding");
    }

    #[tokio::test]
    async fn test_memory_search() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let now = Utc::now();

        let chunk1 = MemoryChunk {
            id: uuid::Uuid::new_v4().to_string(),
            memory_id: "memory_search_test".to_string(),
            workspace_id: None,
            scope: MemoryScope::Global,
            category: "coding".to_string(),
            content: "Rust programming language is great for system programming.".to_string(),
            summary: Some("Rust is good".to_string()),
            source: MemorySource::UserConfirmed,
            sensitivity: MemorySensitivity::Normal,
            confidence: 1.0,
            scene_tags: vec!["afternoon".to_string(), "weekday".to_string()],
            created_at: now,
            updated_at: now,
            expires_at: None,
            last_used_at: None,
        };

        let chunk2 = MemoryChunk {
            id: uuid::Uuid::new_v4().to_string(),
            memory_id: "memory_search_test".to_string(),
            workspace_id: None,
            scope: MemoryScope::Global,
            category: "music".to_string(),
            content: "Music is a great way to relax and enjoy life.".to_string(),
            summary: None,
            source: MemorySource::Inferred,
            sensitivity: MemorySensitivity::Normal,
            confidence: 0.8,
            scene_tags: vec!["evening".to_string(), "weekend".to_string()],
            created_at: now,
            updated_at: now,
            expires_at: None,
            last_used_at: None,
        };

        create_memory_chunk(chunk1, None, None)
            .await
            .expect("create chunk 1");
        create_memory_chunk(chunk2, None, None)
            .await
            .expect("create chunk 2");

        let results = search_memory("rust programming", None, 5)
            .await
            .expect("search memory");

        assert!(!results.is_empty());
        assert!(results[0].chunk.content.contains("Rust"));
    }

    #[tokio::test]
    async fn test_memory_scope_enum() {
        assert_eq!(MemoryScope::Global.as_str(), "global");
        assert_eq!(MemoryScope::Workspace.as_str(), "workspace");
        assert_eq!(
            MemoryScope::from_str("global").unwrap(),
            MemoryScope::Global
        );
        assert_eq!(
            MemoryScope::from_str("workspace").unwrap(),
            MemoryScope::Workspace
        );
    }

    #[tokio::test]
    async fn test_memory_source_enum() {
        assert_eq!(MemorySource::UserConfirmed.as_str(), "user_confirmed");
        assert_eq!(MemorySource::Inferred.as_str(), "inferred");
        assert_eq!(
            MemorySource::from_str("user_confirmed").unwrap(),
            MemorySource::UserConfirmed
        );
    }

    #[tokio::test]
    async fn test_new_entry_has_active_status() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let entry = set("status_key", "value", "cat").await.expect("set");
        assert_eq!(entry.status, "active");
    }

    #[tokio::test]
    async fn test_archive_entry() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("archive_key", "value", "cat").await.expect("set");

        let changed = archive("archive_key").await.expect("archive");
        assert!(changed);

        // Archived entries are no longer returned by get
        let value = get("archive_key").await.expect("get");
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_forget_entry() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("forget_key", "value", "cat").await.expect("set");

        let changed = forget("forget_key").await.expect("forget");
        assert!(changed);

        // Forgotten entries are no longer returned by get
        let value = get("forget_key").await.expect("get");
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_get_by_category_excludes_non_active() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("active_cat", "v1", "status_test").await.expect("set");
        set("archived_cat", "v2", "status_test").await.expect("set");
        archive("archived_cat").await.expect("archive");

        let entries = get_by_category("status_test")
            .await
            .expect("get_by_category");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "active_cat");
    }

    #[tokio::test]
    async fn test_purge_forgotten() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("purge_a", "v1", "cat").await.expect("set");
        set("purge_b", "v2", "cat").await.expect("set");
        forget("purge_a").await.expect("forget");
        forget("purge_b").await.expect("forget");

        let deleted = purge_forgotten().await.expect("purge");
        assert_eq!(deleted, 2);

        // Both should be gone
        let value_a = get("purge_a").await.expect("get a");
        let value_b = get("purge_b").await.expect("get b");
        assert_eq!(value_a, None);
        assert_eq!(value_b, None);
    }

    #[tokio::test]
    async fn test_archive_nonexistent_returns_false() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let changed = archive("does_not_exist").await.expect("archive");
        assert!(!changed);
    }

    #[tokio::test]
    async fn test_forget_nonexistent_returns_false() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let changed = forget("does_not_exist").await.expect("forget");
        assert!(!changed);
    }

    // ── Write gate tests (TASK-014 rework) ──────────────────────────────

    #[tokio::test]
    async fn test_set_user_source_goes_active() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let entry = set_with_source("wg_user", "v", "cat", "user")
            .await
            .expect("set");
        assert_eq!(entry.status, "active");
        assert_eq!(entry.source, MemorySource::UserConfirmed);
        assert!((entry.confidence - 1.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_set_tool_source_goes_candidate() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let entry = set_with_source("wg_tool", "v", "cat", "tool")
            .await
            .expect("set");
        assert_eq!(entry.status, "candidate");
        assert_eq!(entry.source, MemorySource::Tool);
        assert!((entry.confidence - 0.7).abs() < f64::EPSILON);

        // Candidate should NOT appear in get()
        let val = get("wg_tool").await.expect("get");
        assert_eq!(val, None);

        // Candidate should NOT appear in get_by_category()
        let entries = get_by_category("cat").await.expect("get_by_category");
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_set_inferred_source_goes_candidate() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let entry = set_with_source("wg_inf", "v", "cat", "inferred")
            .await
            .expect("set");
        assert_eq!(entry.status, "candidate");
        assert_eq!(entry.source, MemorySource::Inferred);
        assert!((entry.confidence - 0.5).abs() < f64::EPSILON);

        // Candidate should NOT appear in get()
        let val = get("wg_inf").await.expect("get");
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_classify_candidate_to_active() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set_with_source("cl_tool", "v", "cat", "tool")
            .await
            .expect("set");

        // Before classify: not visible
        assert_eq!(get("cl_tool").await.expect("get"), None);

        // Classify to active
        let changed = classify("cl_tool", "active").await.expect("classify");
        assert!(changed);

        // After classify: visible
        let val = get("cl_tool").await.expect("get");
        assert_eq!(val, Some("v".to_string()));
    }

    #[tokio::test]
    async fn test_classify_non_candidate_returns_false() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // user-source entry goes directly to active, not candidate
        set("cl_active", "v", "cat").await.expect("set");

        // classify only works on status='candidate'
        let changed = classify("cl_active", "archived").await.expect("classify");
        assert!(!changed);
    }

    #[tokio::test]
    async fn test_classify_candidate_to_archived() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set_with_source("cl_arch", "v", "cat", "inferred")
            .await
            .expect("set");

        let changed = classify("cl_arch", "archived").await.expect("classify");
        assert!(changed);

        // Archived: not visible via get()
        assert_eq!(get("cl_arch").await.expect("get"), None);

        // But can be found via direct query (status='archived')
        let pool = db::pool().await.expect("pool");
        let row = sqlx::query("SELECT status FROM memory_entries WHERE key = ?1")
            .bind("cl_arch")
            .fetch_one(&pool)
            .await
            .expect("fetch");
        let status: String = row.try_get("status").expect("status");
        assert_eq!(status, "archived");
    }

    #[tokio::test]
    async fn test_update_with_source_changes_status() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // First write as user → active
        let e1 = set_with_source("wg_upd", "v1", "cat", "user")
            .await
            .expect("set");
        assert_eq!(e1.status, "active");

        // Second write as tool → candidate (downgrades)
        let e2 = set_with_source("wg_upd", "v2", "cat", "tool")
            .await
            .expect("set");
        assert_eq!(e2.id, e1.id); // same row
        assert_eq!(e2.status, "candidate");
        assert_eq!(e2.source, MemorySource::Tool);
    }

    // ── Quarantine tests (TASK-027) ──────────────────────────────────────────

    #[tokio::test]
    async fn test_quarantine_active_entry() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("q_active", "v", "cat").await.expect("set");
        assert_eq!(get("q_active").await.expect("get"), Some("v".to_string()));

        let changed = quarantine("q_active").await.expect("quarantine");
        assert!(changed);

        // Quarantined entries are not returned by get()
        assert_eq!(get("q_active").await.expect("get"), None);

        // Verify status in DB
        let pool = db::pool().await.expect("pool");
        let row = sqlx::query("SELECT status FROM memory_entries WHERE key = ?1")
            .bind("q_active")
            .fetch_one(&pool)
            .await
            .expect("fetch");
        let status: String = row.try_get("status").expect("status");
        assert_eq!(status, "quarantined");
    }

    #[tokio::test]
    async fn test_quarantine_candidate_entry() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set_with_source("q_cand", "v", "cat", "tool")
            .await
            .expect("set");

        let changed = quarantine("q_cand").await.expect("quarantine");
        assert!(changed);

        // Verify status in DB
        let pool = db::pool().await.expect("pool");
        let row = sqlx::query("SELECT status FROM memory_entries WHERE key = ?1")
            .bind("q_cand")
            .fetch_one(&pool)
            .await
            .expect("fetch");
        let status: String = row.try_get("status").expect("status");
        assert_eq!(status, "quarantined");
    }

    #[tokio::test]
    async fn test_restore_from_quarantine() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("q_restore", "v", "cat").await.expect("set");
        quarantine("q_restore").await.expect("quarantine");

        // Restore → candidate
        let changed = restore_from_quarantine("q_restore").await.expect("restore");
        assert!(changed);

        // Still not visible (candidate, not active)
        assert_eq!(get("q_restore").await.expect("get"), None);

        // Classify to active
        let changed = classify("q_restore", "active").await.expect("classify");
        assert!(changed);

        // Now visible again
        assert_eq!(get("q_restore").await.expect("get"), Some("v".to_string()));
    }

    #[tokio::test]
    async fn test_quarantine_nonexistent_returns_false() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let changed = quarantine("does_not_exist").await.expect("quarantine");
        assert!(!changed);
    }

    #[tokio::test]
    async fn test_quarantine_excluded_from_get_by_category() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("q_cat_a", "v1", "q_cat").await.expect("set");
        set("q_cat_b", "v2", "q_cat").await.expect("set");
        quarantine("q_cat_a").await.expect("quarantine");

        let entries = get_by_category("q_cat").await.expect("get_by_category");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "q_cat_b");
    }

    // ── Scene tags tests (TASK-050) ────────────────────────────────────────

    #[tokio::test]
    async fn test_new_chunk_with_scene_tags_populates_tags() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let chunk = new_chunk_with_scene_tags(
            "mem_scene_1",
            None,
            MemoryScope::Global,
            "test",
            "content here",
            None,
            MemorySource::UserConfirmed,
            MemorySensitivity::Normal,
            1.0,
        );

        assert_eq!(chunk.scene_tags.len(), 2);
        // Tags should contain one time-of-day and one day-type
        let time_tags = ["morning", "afternoon", "evening", "night"];
        assert!(chunk
            .scene_tags
            .iter()
            .any(|t| time_tags.contains(&t.as_str())));
        let day_tags = ["weekday", "weekend"];
        assert!(chunk
            .scene_tags
            .iter()
            .any(|t| day_tags.contains(&t.as_str())));
    }

    #[tokio::test]
    async fn test_scene_tags_round_trip_through_db() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let chunk = new_chunk_with_scene_tags(
            "mem_scene_rt",
            None,
            MemoryScope::Global,
            "test",
            "round trip content",
            Some("summary"),
            MemorySource::Tool,
            MemorySensitivity::Normal,
            0.8,
        );
        let original_tags = chunk.scene_tags.clone();

        create_memory_chunk(chunk, None, None)
            .await
            .expect("create chunk");

        // Read back via search and verify tags survived the round trip
        let results = search_memory("round trip content", None, 5)
            .await
            .expect("search");
        assert!(!results.is_empty());
        assert_eq!(results[0].chunk.scene_tags, original_tags);
    }

    #[tokio::test]
    async fn test_scene_tags_empty_for_legacy_chunks() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // Simulate a chunk created without scene_tags (empty vec)
        let chunk = MemoryChunk {
            id: uuid::Uuid::new_v4().to_string(),
            memory_id: "mem_legacy".to_string(),
            workspace_id: None,
            scope: MemoryScope::Global,
            category: "test".to_string(),
            content: "legacy content".to_string(),
            summary: None,
            source: MemorySource::UserConfirmed,
            sensitivity: MemorySensitivity::Normal,
            confidence: 1.0,
            scene_tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            last_used_at: None,
        };

        create_memory_chunk(chunk, None, None)
            .await
            .expect("create chunk");

        let results = search_memory("legacy content", None, 5)
            .await
            .expect("search");
        assert!(!results.is_empty());
        assert!(results[0].chunk.scene_tags.is_empty());
    }

    // -- TASK-044: Write-through indexing tests --

    /// memory_set then search_memory can recall the entry via keyword match.
    #[tokio::test]
    async fn test_set_then_search_recalls() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("color_pref", "blue is my favorite color", "preference")
            .await
            .expect("set");

        let results = search_memory("favorite color", None, 5)
            .await
            .expect("search");

        assert!(!results.is_empty(), "search should find the indexed entry");
        assert!(
            results.iter().any(|r| r.chunk.content.contains("blue")),
            "search results should contain the entry content about blue"
        );
    }

    /// Updating an entry re-indexes the chunk with new content.
    #[tokio::test]
    async fn test_update_entry_syncs_chunk() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("lang_pref", "likes python", "preference")
            .await
            .expect("set initial");

        // Update the same key
        set("lang_pref", "likes rust", "preference")
            .await
            .expect("update");

        let results = search_memory("rust language", None, 5)
            .await
            .expect("search");

        assert!(
            results.iter().any(|r| r.chunk.content.contains("rust")),
            "updated chunk should contain new value"
        );
        // Old content should not be the only result (the chunk was updated in place)
        let old_results = search_memory("python language", None, 5)
            .await
            .expect("search old");
        assert!(
            !old_results
                .iter()
                .any(|r| r.chunk.content.contains("likes python")),
            "old content should be gone after update"
        );
    }

    /// Archive removes the entry from search results.
    #[tokio::test]
    async fn test_archive_not_retrievable() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("arch_search", "mountain hiking adventure", "hobby")
            .await
            .expect("set");

        // Verify it's searchable first
        let results = search_memory("mountain hiking", None, 5)
            .await
            .expect("search before archive");
        assert!(results.iter().any(|r| r.chunk.content.contains("mountain")));

        archive("arch_search").await.expect("archive");

        let results = search_memory("mountain hiking", None, 5)
            .await
            .expect("search after archive");
        assert!(
            !results.iter().any(|r| r.chunk.content.contains("mountain")),
            "archived entry should not appear in search"
        );
    }

    /// Forget removes the entry from search results.
    #[tokio::test]
    async fn test_forget_not_retrievable() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("fgt_search", "secret recipe for cookies", "food")
            .await
            .expect("set");

        // Verify searchable
        let results = search_memory("secret recipe", None, 5)
            .await
            .expect("search before forget");
        assert!(results.iter().any(|r| r.chunk.content.contains("recipe")));

        forget("fgt_search").await.expect("forget");

        let results = search_memory("secret recipe", None, 5)
            .await
            .expect("search after forget");
        assert!(
            !results.iter().any(|r| r.chunk.content.contains("recipe")),
            "forgotten entry should not appear in search"
        );
    }

    /// Quarantine removes the entry from search results.
    #[tokio::test]
    async fn test_quarantine_not_retrievable() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("q_search", "dangerous experiment notes", "science")
            .await
            .expect("set");

        // Verify searchable
        let results = search_memory("dangerous experiment", None, 5)
            .await
            .expect("search before quarantine");
        assert!(results
            .iter()
            .any(|r| r.chunk.content.contains("dangerous")));

        quarantine("q_search").await.expect("quarantine");

        let results = search_memory("dangerous experiment", None, 5)
            .await
            .expect("search after quarantine");
        assert!(
            !results
                .iter()
                .any(|r| r.chunk.content.contains("dangerous")),
            "quarantined entry should not appear in search"
        );
    }

    /// Candidate entries (tool/inferred source) should still be indexed and searchable,
    /// since classification is a separate concern from indexing.
    #[tokio::test]
    async fn test_candidate_entries_are_indexed() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set_with_source("cand_search", "machine learning models", "tech", "tool")
            .await
            .expect("set");

        // Candidate entries are indexed for search
        let results = search_memory("machine learning", None, 5)
            .await
            .expect("search");
        assert!(
            results
                .iter()
                .any(|r| r.chunk.content.contains("machine learning")),
            "candidate entry should be indexed and searchable"
        );
    }

    // -- TASK-045: ConversationSummary write-through indexing tests --

    /// add_conversation_summary then search_memory can recall summary content.
    #[tokio::test]
    async fn test_summary_searchable_after_creation() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        add_conversation_summary(
            "We discussed the new authentication module design",
            &["auth".to_string(), "design".to_string()],
        )
        .await
        .expect("add summary");

        let results = search_memory("authentication module", None, 5)
            .await
            .expect("search");

        assert!(
            !results.is_empty(),
            "search should find the indexed conversation summary"
        );
        assert!(
            results
                .iter()
                .any(|r| r.chunk.content.contains("authentication")),
            "search results should contain summary content about authentication"
        );
    }

    /// Summary chunk has correct origin_table and origin_id.
    #[tokio::test]
    async fn test_summary_chunk_has_correct_origin() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let summary =
            add_conversation_summary("Meeting about deployment pipeline", &["deploy".to_string()])
                .await
                .expect("add summary");

        let pool = db::pool().await.expect("pool");
        let chunk_id = format!("summary-{}", summary.id);
        let row = sqlx::query("SELECT origin_table, origin_id FROM memory_chunks WHERE id = ?1")
            .bind(&chunk_id)
            .fetch_one(&pool)
            .await
            .expect("fetch chunk");

        let origin_table: Option<String> = row.try_get("origin_table").expect("origin_table");
        let origin_id: Option<String> = row.try_get("origin_id").expect("origin_id");

        assert_eq!(origin_table.as_deref(), Some("conversation_summaries"));
        assert_eq!(origin_id.as_deref(), Some(summary.id.as_str()));
    }

    // -- TASK-051: Interaction pattern aggregation tests --

    #[tokio::test]
    async fn test_aggregate_patterns_empty_when_no_chunks() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let patterns = aggregate_interaction_patterns(30).await.expect("aggregate");
        assert!(patterns.is_empty(), "no chunks means no patterns");
    }

    #[tokio::test]
    async fn test_aggregate_patterns_groups_by_category_and_tag() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // Create 3 chunks in "coding" category with "evening" tag, 1 in "music" with "morning"
        for i in 0..3 {
            let chunk = MemoryChunk {
                id: format!("pat-chunk-{}", i),
                memory_id: format!("pat-mem-{}", i),
                workspace_id: None,
                scope: MemoryScope::Global,
                category: "coding".to_string(),
                content: format!("coding content {}", i),
                summary: None,
                source: MemorySource::UserConfirmed,
                sensitivity: MemorySensitivity::Normal,
                confidence: 1.0,
                scene_tags: vec!["evening".to_string(), "weekday".to_string()],
                created_at: Utc::now(),
                updated_at: Utc::now(),
                expires_at: None,
                last_used_at: None,
            };
            create_memory_chunk(chunk, None, None)
                .await
                .expect("create chunk");
        }

        let chunk_music = MemoryChunk {
            id: "pat-chunk-music".to_string(),
            memory_id: "pat-mem-music".to_string(),
            workspace_id: None,
            scope: MemoryScope::Global,
            category: "music".to_string(),
            content: "music content".to_string(),
            summary: None,
            source: MemorySource::UserConfirmed,
            sensitivity: MemorySensitivity::Normal,
            confidence: 1.0,
            scene_tags: vec!["morning".to_string(), "weekend".to_string()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            last_used_at: None,
        };
        create_memory_chunk(chunk_music, None, None)
            .await
            .expect("create music chunk");

        let patterns = aggregate_interaction_patterns(30).await.expect("aggregate");

        // coding+evening should appear (frequency 3), coding+weekday (freq 3),
        // music+morning (freq 1 filtered out), music+weekend (freq 1 filtered out)
        let coding_evening = patterns
            .iter()
            .find(|p| p.category == "coding" && p.scene_tags.contains(&"evening".to_string()));
        assert!(
            coding_evening.is_some(),
            "should find coding+evening pattern"
        );
        let pat = coding_evening.unwrap();
        assert!(pat.frequency >= 3);
        assert!(pat.key_pattern.contains("coding"));
        assert!(pat.key_pattern.contains("evening"));

        // music patterns should be filtered out (frequency < 2)
        let music_patterns: Vec<_> = patterns.iter().filter(|p| p.category == "music").collect();
        assert!(
            music_patterns.is_empty(),
            "single-occurrence patterns should be filtered"
        );
    }

    #[tokio::test]
    async fn test_aggregate_patterns_stored_as_memory_entry() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // Create enough chunks to trigger pattern storage
        for i in 0..3 {
            let chunk = MemoryChunk {
                id: format!("store-chunk-{}", i),
                memory_id: format!("store-mem-{}", i),
                workspace_id: None,
                scope: MemoryScope::Global,
                category: "preference".to_string(),
                content: format!("preference content {}", i),
                summary: None,
                source: MemorySource::UserConfirmed,
                sensitivity: MemorySensitivity::Normal,
                confidence: 1.0,
                scene_tags: vec!["morning".to_string(), "weekday".to_string()],
                created_at: Utc::now(),
                updated_at: Utc::now(),
                expires_at: None,
                last_used_at: None,
            };
            create_memory_chunk(chunk, None, None)
                .await
                .expect("create chunk");
        }

        let patterns = aggregate_interaction_patterns(30).await.expect("aggregate");
        assert!(!patterns.is_empty());

        // Verify at least one pattern was stored as a memory entry
        let pool = db::pool().await.expect("pool");
        let row = sqlx::query(
            "SELECT source, category FROM memory_entries WHERE source = 'pattern_aggregation' LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .expect("find stored pattern");

        let source: String = row.try_get("source").expect("source");
        assert_eq!(source, "pattern_aggregation");
    }

    #[tokio::test]
    async fn test_aggregate_patterns_confidence_is_proportional() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // Create 4 "coding"+evening and 2 "coding"+morning
        for i in 0..4 {
            let chunk = MemoryChunk {
                id: format!("conf-evening-{}", i),
                memory_id: format!("conf-mem-e-{}", i),
                workspace_id: None,
                scope: MemoryScope::Global,
                category: "coding".to_string(),
                content: format!("coding evening {}", i),
                summary: None,
                source: MemorySource::UserConfirmed,
                sensitivity: MemorySensitivity::Normal,
                confidence: 1.0,
                scene_tags: vec!["evening".to_string(), "weekday".to_string()],
                created_at: Utc::now(),
                updated_at: Utc::now(),
                expires_at: None,
                last_used_at: None,
            };
            create_memory_chunk(chunk, None, None)
                .await
                .expect("create evening chunk");
        }

        for i in 0..2 {
            let chunk = MemoryChunk {
                id: format!("conf-morning-{}", i),
                memory_id: format!("conf-mem-m-{}", i),
                workspace_id: None,
                scope: MemoryScope::Global,
                category: "coding".to_string(),
                content: format!("coding morning {}", i),
                summary: None,
                source: MemorySource::UserConfirmed,
                sensitivity: MemorySensitivity::Normal,
                confidence: 1.0,
                scene_tags: vec!["morning".to_string(), "weekday".to_string()],
                created_at: Utc::now(),
                updated_at: Utc::now(),
                expires_at: None,
                last_used_at: None,
            };
            create_memory_chunk(chunk, None, None)
                .await
                .expect("create morning chunk");
        }

        let patterns = aggregate_interaction_patterns(30).await.expect("aggregate");

        // evening pattern should have higher confidence than morning
        let evening = patterns
            .iter()
            .find(|p| p.category == "coding" && p.scene_tags.contains(&"evening".to_string()));
        let morning = patterns
            .iter()
            .find(|p| p.category == "coding" && p.scene_tags.contains(&"morning".to_string()));

        assert!(evening.is_some(), "should find evening pattern");
        assert!(morning.is_some(), "should find morning pattern");
        assert!(
            evening.unwrap().confidence > morning.unwrap().confidence,
            "higher frequency should yield higher confidence"
        );
    }

    #[tokio::test]
    async fn test_pattern_aggregation_source_round_trip() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // Verify PatternAggregation source round-trips through DB
        let entry = set_with_source(
            "pat_rt_key",
            "pattern_value",
            "pat_cat",
            "pattern_aggregation",
        )
        .await
        .expect("set");

        assert_eq!(entry.source, MemorySource::PatternAggregation);
        assert_eq!(entry.status, "active");
        assert!((entry.confidence - 0.8).abs() < f64::EPSILON);

        // Re-read from DB to verify round trip
        let pool = db::pool().await.expect("pool");
        let row =
            sqlx::query("SELECT source, status, confidence FROM memory_entries WHERE key = ?1")
                .bind("pat_rt_key")
                .fetch_one(&pool)
                .await
                .expect("fetch");

        let db_source: String = row.try_get("source").expect("source");
        let db_status: String = row.try_get("status").expect("status");
        let db_confidence: f64 = row.try_get("confidence").expect("confidence");

        assert_eq!(db_source, "pattern_aggregation");
        assert_eq!(db_status, "active");
        assert!((db_confidence - 0.8).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_interaction_pattern_equality() {
        let p1 = InteractionPattern {
            category: "coding".to_string(),
            key_pattern: "coding: evening, frequently accessed in evening/weekday".to_string(),
            frequency: 5,
            scene_tags: vec!["evening".to_string(), "weekday".to_string()],
            confidence: 0.4,
        };
        let p2 = p1.clone();
        assert_eq!(p1, p2);
    }

    #[tokio::test]
    async fn test_pattern_aggregation_serializes_to_json() {
        let pattern = InteractionPattern {
            category: "preference".to_string(),
            key_pattern: "preference: morning, frequently accessed in morning/weekday".to_string(),
            frequency: 3,
            scene_tags: vec!["morning".to_string(), "weekday".to_string()],
            confidence: 0.25,
        };

        let json = serde_json::to_string(&pattern).expect("serialize");
        assert!(json.contains("\"category\":\"preference\""));
        assert!(json.contains("\"frequency\":3"));

        let deserialized: InteractionPattern = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized, pattern);
    }

    // ── Embedding model abstraction tests (TASK-052) ────────────────────

    #[test]
    fn test_hash_embedding_deterministic() {
        let model = HashEmbeddingModel::default();
        let a = model.embed("hello world");
        let b = model.embed("hello world");
        assert_eq!(a, b);
        assert_eq!(a.len(), 384);
    }

    #[test]
    fn test_hash_embedding_different_inputs_differ() {
        let model = HashEmbeddingModel::default();
        let a = model.embed("hello");
        let b = model.embed("world");
        assert_ne!(a, b);
    }

    #[test]
    fn test_hash_embedding_custom_dimension() {
        let model = HashEmbeddingModel::new(128);
        assert_eq!(model.dimension(), 128);
        assert_eq!(model.embed("test").len(), 128);
    }

    #[test]
    fn test_hash_embedding_model_name() {
        let model = HashEmbeddingModel::default();
        assert_eq!(model.model_name(), "hash-embedding");
    }

    #[test]
    fn test_chinese_embedding_model_name() {
        let model = ChineseEmbeddingModel::default();
        assert_eq!(model.model_name(), "chinese-embedding-placeholder");
        assert_eq!(model.dimension(), 384);
    }

    #[test]
    fn test_embedding_model_trait_object() {
        let models: Vec<Box<dyn EmbeddingModel>> = vec![
            Box::new(HashEmbeddingModel::default()),
            Box::new(ChineseEmbeddingModel::default()),
        ];
        for model in &models {
            let v = model.embed("test text");
            assert_eq!(v.len(), model.dimension());
        }
    }

    #[tokio::test]
    async fn test_rebuild_all_embeddings() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // Use hash model for deterministic testing
        set_embedding_model(Box::new(HashEmbeddingModel::default()));

        set("rebuild_a", "alpha content", "cat")
            .await
            .expect("set a");
        set("rebuild_b", "beta content", "cat")
            .await
            .expect("set b");

        let count = rebuild_all_embeddings().await.expect("rebuild");
        assert_eq!(count, 2);

        // Verify search still works after rebuild
        let results = search_memory("alpha content", None, 5)
            .await
            .expect("search");
        assert!(
            results.iter().any(|r| r.chunk.content.contains("alpha")),
            "rebuilt index should be searchable"
        );
    }

    #[tokio::test]
    async fn test_set_embedding_model_swaps_model() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // Start with hash model
        set_embedding_model(Box::new(HashEmbeddingModel::default()));
        assert_eq!(current_model_name(), "hash-embedding");

        // Swap to Chinese placeholder
        set_embedding_model(Box::new(ChineseEmbeddingModel::default()));
        assert_eq!(current_model_name(), "chinese-embedding-placeholder");
    }

    #[tokio::test]
    async fn test_embedding_model_name_stored_in_db() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set_embedding_model(Box::new(HashEmbeddingModel::default()));

        set("model_check", "some value", "cat").await.expect("set");

        // Verify the embedding was stored with the correct model name
        let pool = db::pool().await.expect("pool");
        let row = sqlx::query(
            "SELECT me.model FROM memory_embeddings me \
             JOIN memory_chunks mc ON me.chunk_id = mc.id \
             WHERE mc.memory_id = (SELECT id FROM memory_entries WHERE key = 'model_check')",
        )
        .fetch_one(&pool)
        .await
        .expect("fetch embedding");

        let model: String = row.try_get("model").expect("model");
        assert_eq!(model, "hash-embedding");
    }

    // -- TASK-046: SearchFilter tests --

    #[tokio::test]
    async fn test_search_filter_default_matches_search_memory() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("sf_default", "quantum computing research", "science")
            .await
            .expect("set");

        let results_old = search_memory("quantum computing", None, 5)
            .await
            .expect("search old");
        let results_new =
            search_memory_filtered("quantum computing", None, 5, SearchFilter::default())
                .await
                .expect("search filtered");

        assert_eq!(results_old.len(), results_new.len());
        if !results_old.is_empty() {
            assert_eq!(results_old[0].chunk.id, results_new[0].chunk.id);
        }
    }

    #[tokio::test]
    async fn test_search_filter_exclude_quarantined_false_includes_quarantined() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("sf_quar", "secret algorithm notes", "tech")
            .await
            .expect("set");

        // Manually set status to quarantined WITHOUT calling quarantine() which
        // would delete the chunks. This simulates the filter's SQL-level behavior.
        let pool = db::pool().await.expect("pool");
        sqlx::query("UPDATE memory_entries SET status = 'quarantined' WHERE key = ?1")
            .bind("sf_quar")
            .execute(&pool)
            .await
            .expect("update status");

        // Default filter excludes quarantined
        let results_default = search_memory("secret algorithm", None, 5)
            .await
            .expect("search default");
        assert!(
            !results_default
                .iter()
                .any(|r| r.chunk.content.contains("algorithm")),
            "quarantined entry should be excluded by default"
        );

        // Filter with exclude_quarantined=false should include it
        let filter = SearchFilter {
            exclude_quarantined: false,
            exclude_forgotten: true,
            exclude_archived: true,
            max_sensitivity: None,
            category: None,
            path_prefix: None,
            session_id: None,
            goal_id: None,
        };
        let results_inclusive = search_memory_filtered("secret algorithm", None, 5, filter)
            .await
            .expect("search inclusive");
        assert!(
            results_inclusive
                .iter()
                .any(|r| r.chunk.content.contains("algorithm")),
            "quarantined entry should appear when exclude_quarantined=false"
        );
    }

    #[tokio::test]
    async fn test_search_filter_category_filter() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("sf_cat_a", "rust programming", "coding")
            .await
            .expect("set");
        set("sf_cat_b", "jazz music playlist", "music")
            .await
            .expect("set");

        // Filter by category "coding"
        let filter = SearchFilter {
            category: Some("coding".to_string()),
            ..SearchFilter::default()
        };
        let results = search_memory_filtered("rust jazz", None, 5, filter)
            .await
            .expect("search filtered by category");

        for r in &results {
            assert_eq!(r.chunk.category, "coding");
        }
        assert!(
            results.iter().any(|r| r.chunk.content.contains("rust")),
            "coding category result should be present"
        );
        assert!(
            !results.iter().any(|r| r.chunk.content.contains("jazz")),
            "music category result should be filtered out"
        );
    }

    #[tokio::test]
    async fn test_search_filter_sensitivity_normal_only() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // Create a normal-sensitivity chunk directly
        let normal_chunk = MemoryChunk {
            id: "sf-normal-chunk".to_string(),
            memory_id: "sf-normal-mem".to_string(),
            workspace_id: None,
            scope: MemoryScope::Global,
            category: "test".to_string(),
            content: "normal sensitivity data about cats".to_string(),
            summary: None,
            source: MemorySource::UserConfirmed,
            sensitivity: MemorySensitivity::Normal,
            confidence: 1.0,
            scene_tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            last_used_at: None,
        };
        create_memory_chunk(normal_chunk, None, None)
            .await
            .expect("create normal chunk");

        // Create a private-sensitivity chunk
        let private_chunk = MemoryChunk {
            id: "sf-private-chunk".to_string(),
            memory_id: "sf-private-mem".to_string(),
            workspace_id: None,
            scope: MemoryScope::Global,
            category: "test".to_string(),
            content: "private sensitivity data about dogs".to_string(),
            summary: None,
            source: MemorySource::UserConfirmed,
            sensitivity: MemorySensitivity::Private,
            confidence: 1.0,
            scene_tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            last_used_at: None,
        };
        create_memory_chunk(private_chunk, None, None)
            .await
            .expect("create private chunk");

        // Filter: max_sensitivity = Normal (should exclude private and secret)
        let filter = SearchFilter {
            max_sensitivity: Some(MemorySensitivity::Normal),
            ..SearchFilter::default()
        };
        let results = search_memory_filtered("sensitivity data", None, 5, filter)
            .await
            .expect("search with sensitivity filter");

        assert!(
            results.iter().any(|r| r.chunk.content.contains("cats")),
            "normal sensitivity chunk should be included"
        );
        assert!(
            !results.iter().any(|r| r.chunk.content.contains("dogs")),
            "private sensitivity chunk should be excluded when max_sensitivity=Normal"
        );
    }

    #[tokio::test]
    async fn test_search_filter_sensitivity_private_allows_normal_and_private() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        let normal_chunk = MemoryChunk {
            id: "sf2-normal-chunk".to_string(),
            memory_id: "sf2-normal-mem".to_string(),
            workspace_id: None,
            scope: MemoryScope::Global,
            category: "test".to_string(),
            content: "birds are wonderful creatures".to_string(),
            summary: None,
            source: MemorySource::UserConfirmed,
            sensitivity: MemorySensitivity::Normal,
            confidence: 1.0,
            scene_tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            last_used_at: None,
        };
        create_memory_chunk(normal_chunk, None, None)
            .await
            .expect("create normal chunk");

        let private_chunk = MemoryChunk {
            id: "sf2-private-chunk".to_string(),
            memory_id: "sf2-private-mem".to_string(),
            workspace_id: None,
            scope: MemoryScope::Global,
            category: "test".to_string(),
            content: "fish are quiet pets".to_string(),
            summary: None,
            source: MemorySource::UserConfirmed,
            sensitivity: MemorySensitivity::Private,
            confidence: 1.0,
            scene_tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            last_used_at: None,
        };
        create_memory_chunk(private_chunk, None, None)
            .await
            .expect("create private chunk");

        // Filter: max_sensitivity = Private (should include normal and private, exclude secret)
        let filter = SearchFilter {
            max_sensitivity: Some(MemorySensitivity::Private),
            ..SearchFilter::default()
        };
        let results = search_memory_filtered("creatures pets", None, 5, filter)
            .await
            .expect("search with private sensitivity filter");

        assert!(
            results.iter().any(|r| r.chunk.content.contains("birds")),
            "normal sensitivity chunk should be included"
        );
        assert!(
            results.iter().any(|r| r.chunk.content.contains("fish")),
            "private sensitivity chunk should be included when max_sensitivity=Private"
        );
    }

    #[tokio::test]
    async fn test_search_filter_exclude_forgotten_false() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("sf_fgt", "forgotten memory about space", "science")
            .await
            .expect("set");

        // Manually set status to forgotten WITHOUT calling forget() which
        // would delete the chunks. This tests the filter's SQL-level behavior.
        let pool = db::pool().await.expect("pool");
        sqlx::query("UPDATE memory_entries SET status = 'forgotten' WHERE key = ?1")
            .bind("sf_fgt")
            .execute(&pool)
            .await
            .expect("update status");

        // Default excludes forgotten
        let results_default = search_memory("forgotten memory space", None, 5)
            .await
            .expect("search default");
        assert!(
            !results_default
                .iter()
                .any(|r| r.chunk.content.contains("space")),
            "forgotten entry should be excluded by default"
        );

        // Filter with exclude_forgotten=false
        let filter = SearchFilter {
            exclude_forgotten: false,
            ..SearchFilter::default()
        };
        let results = search_memory_filtered("forgotten memory space", None, 5, filter)
            .await
            .expect("search with forgotten included");
        assert!(
            results.iter().any(|r| r.chunk.content.contains("space")),
            "forgotten entry should appear when exclude_forgotten=false"
        );
    }

    #[tokio::test]
    async fn test_sensitivity_level_ordering() {
        assert!(MemorySensitivity::Normal.level() < MemorySensitivity::Private.level());
        assert!(MemorySensitivity::Private.level() < MemorySensitivity::Secret.level());
        assert_eq!(MemorySensitivity::Normal.level(), 0);
        assert_eq!(MemorySensitivity::Private.level(), 1);
        assert_eq!(MemorySensitivity::Secret.level(), 2);
    }

    // ── TASK-048: recall_for_prompt tests ──────────────────────────────────

    #[tokio::test]
    async fn test_recall_for_prompt_returns_entries() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        set_embedding_model(Box::new(HashEmbeddingModel::default()));

        set("recall_key", "favorite language is rust", "preference")
            .await
            .expect("set");

        let result = recall_for_prompt("favorite language", None, 5)
            .await
            .expect("recall");

        assert!(
            !result.entries.is_empty(),
            "recall_for_prompt should return memory entries"
        );
        assert!(
            result.entries.iter().any(|e| e.value.contains("rust")),
            "entries should contain the stored value"
        );
    }

    #[tokio::test]
    async fn test_recall_for_prompt_returns_summaries() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        set_embedding_model(Box::new(HashEmbeddingModel::default()));

        add_conversation_summary(
            "We discussed deployment strategies for kubernetes",
            &["deploy".to_string(), "kubernetes".to_string()],
        )
        .await
        .expect("add summary");

        let result = recall_for_prompt("deployment strategies", None, 5)
            .await
            .expect("recall");

        assert!(
            !result.summaries.is_empty(),
            "recall_for_prompt should return conversation summaries"
        );
        assert!(
            result
                .summaries
                .iter()
                .any(|s| s.summary.contains("deployment")),
            "summaries should contain the stored summary"
        );
    }

    #[tokio::test]
    async fn test_recall_for_prompt_returns_both_entries_and_summaries() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        set_embedding_model(Box::new(HashEmbeddingModel::default()));

        set("recall_both", "rust async programming tips", "coding")
            .await
            .expect("set");

        add_conversation_summary(
            "Discussion about async rust patterns and tokio",
            &["async".to_string(), "rust".to_string()],
        )
        .await
        .expect("add summary");

        let result = recall_for_prompt("rust async", None, 5)
            .await
            .expect("recall");

        assert!(!result.entries.is_empty(), "should return memory entries");
        assert!(
            !result.summaries.is_empty(),
            "should return conversation summaries"
        );
    }

    #[tokio::test]
    async fn test_recall_for_prompt_deduplicates_summaries() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        set_embedding_model(Box::new(HashEmbeddingModel::default()));

        add_conversation_summary(
            "Machine learning model training best practices",
            &["ml".to_string(), "training".to_string()],
        )
        .await
        .expect("add summary");

        let result = recall_for_prompt("machine learning training", None, 5)
            .await
            .expect("recall");

        // Verify no duplicate summary IDs
        let mut ids: Vec<&str> = result.summaries.iter().map(|s| s.id.as_str()).collect();
        let before = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(before, ids.len(), "summaries should be deduplicated");
    }

    #[tokio::test]
    async fn test_recall_for_prompt_empty_context() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        set_embedding_model(Box::new(HashEmbeddingModel::default()));

        set("recall_empty", "some value", "cat").await.expect("set");

        let result = recall_for_prompt("", None, 5)
            .await
            .expect("recall with empty context");

        // Should not panic; results may be empty or contain matches
        // Just verifying the function completes successfully
        let _ = result.entries;
        let _ = result.summaries;
    }

    #[tokio::test]
    async fn test_recall_for_prompt_total_chunks_searched() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        set_embedding_model(Box::new(HashEmbeddingModel::default()));

        set("recall_chunks_a", "alpha beta gamma", "cat")
            .await
            .expect("set");

        add_conversation_summary("delta epsilon zeta", &["delta".to_string()])
            .await
            .expect("add summary");

        let result = recall_for_prompt("alpha delta", None, 10)
            .await
            .expect("recall");

        assert!(
            result.total_chunks_searched > 0,
            "total_chunks_searched should be > 0 when results exist"
        );
    }

    #[tokio::test]
    async fn test_recall_for_prompt_with_context_honors_workspace_scope() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        set_embedding_model(Box::new(HashEmbeddingModel::default()));

        set_for_workspace(
            "auth_module",
            "workspace alpha auth guidance",
            "project",
            "ws-alpha",
            "user",
        )
        .await
        .expect("set alpha workspace memory");
        set_for_workspace(
            "auth_module",
            "workspace beta auth guidance",
            "project",
            "ws-beta",
            "user",
        )
        .await
        .expect("set beta workspace memory");

        let alpha_context = RecallContext {
            query: "auth guidance".to_string(),
            workspace_id: Some("ws-alpha".to_string()),
            path_prefix: Some("crates/conductor-core/src/chat".to_string()),
            session_id: None,
            goal_id: None,
            limit: 10,
        };
        let alpha_result = recall_for_prompt_with_context(&alpha_context)
            .await
            .expect("alpha recall");

        assert!(
            alpha_result.entries.iter().any(|entry| {
                entry.workspace_id.as_deref() == Some("ws-alpha")
                    && entry.value.contains("workspace alpha")
            }),
            "alpha recall should include alpha-scoped memory"
        );
        assert!(
            !alpha_result.entries.iter().any(|entry| {
                entry.workspace_id.as_deref() == Some("ws-beta")
                    && entry.value.contains("workspace beta")
            }),
            "alpha recall should exclude beta-scoped memory"
        );
    }

    #[tokio::test]
    async fn test_recall_for_prompt_with_context_honors_session_and_goal_scope() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        set_embedding_model(Box::new(HashEmbeddingModel::default()));

        let matching = set(
            "routing_scope_matching",
            "perimeter orchid guidance from matching session",
            "project",
        )
        .await
        .expect("set matching memory");
        let other = set(
            "routing_scope_other",
            "perimeter orchid guidance from other session",
            "project",
        )
        .await
        .expect("set other memory");
        let global = set(
            "routing_scope_global",
            "perimeter orchid guidance without session metadata",
            "project",
        )
        .await
        .expect("set global memory");

        let pool = db::pool().await.expect("pool");
        sqlx::query(
            r#"
            UPDATE memory_entries
            SET source_session_id = ?1, goal_id = ?2
            WHERE id = ?3
            "#,
        )
        .bind("session-a")
        .bind("goal-a")
        .bind(&matching.id)
        .execute(&pool)
        .await
        .expect("tag matching memory");
        sqlx::query(
            r#"
            UPDATE memory_entries
            SET source_session_id = ?1, goal_id = ?2
            WHERE id = ?3
            "#,
        )
        .bind("session-b")
        .bind("goal-b")
        .bind(&other.id)
        .execute(&pool)
        .await
        .expect("tag other memory");

        let result = recall_for_prompt_with_context(&RecallContext {
            query: "perimeter orchid guidance".to_string(),
            workspace_id: None,
            path_prefix: None,
            session_id: Some("session-a".to_string()),
            goal_id: Some("goal-a".to_string()),
            limit: 10,
        })
        .await
        .expect("recall with session and goal");

        assert!(
            result.entries.iter().any(|entry| entry.id == matching.id),
            "recall should include memory explicitly tagged to the active session and goal"
        );
        assert!(
            result.entries.iter().any(|entry| entry.id == global.id),
            "recall should keep untagged global memory eligible"
        );
        assert!(
            !result.entries.iter().any(|entry| entry.id == other.id),
            "recall should exclude memory explicitly tagged to another session and goal"
        );
    }

    #[tokio::test]
    async fn test_recall_for_prompt_with_context_honors_path_prefix_scope() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        set_embedding_model(Box::new(HashEmbeddingModel::default()));

        set_for_workspace(
            "repo_auth",
            "workspace auth guidance",
            "project",
            "ws-alpha",
            "user",
        )
        .await
        .expect("set workspace memory");
        set_with_scope_and_path(
            "chat_auth",
            "chat subtree auth guidance",
            "project",
            MemoryScope::Workspace,
            Some("ws-alpha"),
            Some("crates/conductor-core/src/chat"),
            "user",
        )
        .await
        .expect("set chat path memory");
        set_with_scope_and_path(
            "memory_auth",
            "memory subtree auth guidance",
            "project",
            MemoryScope::Workspace,
            Some("ws-alpha"),
            Some("crates/conductor-core/src/memory"),
            "user",
        )
        .await
        .expect("set memory path memory");

        let chat_context = RecallContext {
            query: "auth guidance".to_string(),
            workspace_id: Some("ws-alpha".to_string()),
            path_prefix: Some("crates/conductor-core/src/chat/tools".to_string()),
            session_id: None,
            goal_id: None,
            limit: 10,
        };
        let chat_result = recall_for_prompt_with_context(&chat_context)
            .await
            .expect("chat recall");

        assert!(
            chat_result
                .entries
                .iter()
                .any(|entry| entry.key == "repo_auth" && entry.path_prefix.is_none()),
            "chat recall should include workspace-wide memory"
        );
        assert!(
            chat_result.entries.iter().any(|entry| {
                entry.key == "chat_auth"
                    && entry.path_prefix.as_deref() == Some("crates/conductor-core/src/chat")
            }),
            "chat recall should include ancestor path memory"
        );
        assert!(
            !chat_result
                .entries
                .iter()
                .any(|entry| entry.key == "memory_auth"),
            "chat recall should exclude sibling path memory"
        );
    }

    #[tokio::test]
    async fn test_recall_for_prompt_excludes_forgotten() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        set_embedding_model(Box::new(HashEmbeddingModel::default()));

        set("recall_fgt", "secret project alpha", "project")
            .await
            .expect("set");

        forget("recall_fgt").await.expect("forget");

        let result = recall_for_prompt("secret project", None, 5)
            .await
            .expect("recall");

        assert!(
            !result.entries.iter().any(|e| e.key == "recall_fgt"),
            "forgotten entries should not appear in recall results"
        );
    }

    // ── TASK-112: reinforce_pattern + confidence promotion tests ────────

    /// Helper: read a memory_entries row directly from the DB by key + workspace_id.
    async fn read_pattern_entry(key: &str, workspace_id: &str) -> Option<MemoryEntry> {
        let pool = db::pool().await.expect("pool");
        let row = sqlx::query(
            r#"
            SELECT id, key, value, category, scope, workspace_id, source, confidence,
                   sensitivity, status, expires_at, last_used_at, created_at, updated_at,
                   interaction_count, last_reinforced_at
            FROM memory_entries
            WHERE key = ?1 AND workspace_id = ?2 AND scope = 'workspace'
            LIMIT 1
            "#,
        )
        .bind(key)
        .bind(workspace_id)
        .fetch_optional(&pool)
        .await
        .expect("fetch");
        row.map(|r| memory_from_row(r).expect("parse"))
    }

    #[tokio::test]
    async fn test_reinforce_creates_candidate_with_initial_confidence() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        reinforce_pattern("ws_test", "pat:evening", "coding", "user codes in evening")
            .await
            .expect("reinforce");

        let entry = read_pattern_entry("pat:evening", "ws_test")
            .await
            .expect("entry should exist");
        assert_eq!(entry.status, "candidate");
        assert!((entry.confidence - 0.5).abs() < f64::EPSILON);
        assert_eq!(entry.interaction_count, 1);
        assert!(entry.last_reinforced_at.is_some());
    }

    #[tokio::test]
    async fn test_reinforce_increments_confidence_on_subsequent_calls() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        reinforce_pattern("ws_inc", "pat:morning", "preference", "morning user")
            .await
            .expect("first reinforce");
        reinforce_pattern("ws_inc", "pat:morning", "preference", "still morning")
            .await
            .expect("second reinforce");
        reinforce_pattern("ws_inc", "pat:morning", "preference", "always morning")
            .await
            .expect("third reinforce");

        let entry = read_pattern_entry("pat:morning", "ws_inc")
            .await
            .expect("entry should exist");
        // After 3 calls: create at 0.5, bump to 0.6, bump to 0.7 -> promoted to stable
        assert_eq!(entry.status, "stable");
        assert!((entry.confidence - 0.7).abs() < f64::EPSILON);
        assert_eq!(entry.interaction_count, 3);
    }

    #[tokio::test]
    async fn test_reinforce_promotes_candidate_to_stable_at_threshold() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // 0.5 -> 0.6 -> 0.7 (promotes to stable)
        reinforce_pattern("ws_promo", "pat:rust", "coding", "e1")
            .await
            .expect("r1");
        reinforce_pattern("ws_promo", "pat:rust", "coding", "e2")
            .await
            .expect("r2");
        reinforce_pattern("ws_promo", "pat:rust", "coding", "e3")
            .await
            .expect("r3");

        let entry = read_pattern_entry("pat:rust", "ws_promo")
            .await
            .expect("entry");
        assert_eq!(
            entry.status, "stable",
            "should be promoted to stable at confidence >= 0.7"
        );
        assert!((entry.confidence - 0.7).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_reinforce_confidence_cap_at_09() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // Reinforce many times: 0.5 + 5*0.1 = 1.0, but capped at 0.9
        for i in 0..8 {
            reinforce_pattern("ws_cap", "pat:music", "hobby", &format!("e{}", i))
                .await
                .expect("reinforce");
        }

        let entry = read_pattern_entry("pat:music", "ws_cap")
            .await
            .expect("entry");
        assert!(
            entry.confidence <= 0.9,
            "confidence should be capped at 0.9, got {}",
            entry.confidence
        );
        assert_eq!(entry.status, "stable");
        assert_eq!(entry.interaction_count, 8);
    }

    #[tokio::test]
    async fn test_confidence_decay_reduces_old_entries() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        let pool = db::pool().await.expect("pool");

        // Create a pattern entry with last_reinforced_at 10 days ago
        let ten_days_ago = (Utc::now() - chrono::Duration::days(10)).to_rfc3339();
        let now_str = Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO memory_entries (id, key, value, category, scope, workspace_id, source,
                                       confidence, sensitivity, status, expires_at, last_used_at,
                                       created_at, updated_at, interaction_count, last_reinforced_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
        )
        .bind(&id)
        .bind("pat:decay_test")
        .bind("old evidence")
        .bind("coding")
        .bind("workspace")
        .bind("ws_decay")
        .bind("pattern_aggregation")
        .bind(0.8_f64)
        .bind("normal")
        .bind("stable")
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(&now_str)
        .bind(&now_str)
        .bind(5_i64)
        .bind(&ten_days_ago)
        .execute(&pool)
        .await
        .expect("insert");

        let affected = apply_confidence_decay().await.expect("decay");
        assert_eq!(affected, 1, "one entry should have been decayed");

        let entry = read_pattern_entry("pat:decay_test", "ws_decay")
            .await
            .expect("entry");
        assert!(
            (entry.confidence - 0.75).abs() < f64::EPSILON,
            "confidence should drop from 0.8 to 0.75, got {}",
            entry.confidence
        );
        // Still stable because 0.75 >= 0.3
        assert_eq!(entry.status, "stable");
    }

    #[tokio::test]
    async fn test_confidence_decay_demotes_stable_below_threshold() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");
        let pool = db::pool().await.expect("pool");

        // Create a stable entry with low confidence (0.3) and old last_reinforced_at
        let ten_days_ago = (Utc::now() - chrono::Duration::days(10)).to_rfc3339();
        let now_str = Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO memory_entries (id, key, value, category, scope, workspace_id, source,
                                       confidence, sensitivity, status, expires_at, last_used_at,
                                       created_at, updated_at, interaction_count, last_reinforced_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
        )
        .bind(&id)
        .bind("pat:demote_test")
        .bind("weak evidence")
        .bind("coding")
        .bind("workspace")
        .bind("ws_demote")
        .bind("pattern_aggregation")
        .bind(0.30_f64)
        .bind("normal")
        .bind("stable")
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(&now_str)
        .bind(&now_str)
        .bind(2_i64)
        .bind(&ten_days_ago)
        .execute(&pool)
        .await
        .expect("insert");

        let affected = apply_confidence_decay().await.expect("decay");
        assert_eq!(affected, 1);

        let entry = read_pattern_entry("pat:demote_test", "ws_demote")
            .await
            .expect("entry");
        assert!(
            (entry.confidence - 0.25).abs() < f64::EPSILON,
            "confidence should drop from 0.30 to 0.25, got {}",
            entry.confidence
        );
        assert_eq!(
            entry.status, "candidate",
            "stable entry with confidence < 0.3 should be demoted to candidate"
        );
    }

    #[tokio::test]
    async fn test_confidence_decay_skips_recent_entries() {
        let _root = TestRoot::new();
        init_db().await.expect("init db");

        // Create a pattern reinforced just now — should NOT be decayed
        reinforce_pattern("ws_fresh", "pat:fresh", "test", "recent evidence")
            .await
            .expect("reinforce");

        let affected = apply_confidence_decay().await.expect("decay");
        assert_eq!(affected, 0, "recent entries should not be decayed");

        let entry = read_pattern_entry("pat:fresh", "ws_fresh")
            .await
            .expect("entry");
        assert!((entry.confidence - 0.5).abs() < f64::EPSILON);
    }

    // ── TASK-110: EmbeddingProvider trait tests ─────────────────────────

    #[tokio::test]
    async fn test_set_and_get_embedding_provider() {
        use crate::embedding::HashFallbackProvider;
        use std::sync::Arc;

        let provider = Arc::new(HashFallbackProvider::new(256));
        set_embedding_provider(provider);

        let active = active_embedding_provider();
        assert_eq!(active.dimension(), 256);
        assert_eq!(active.model_name(), "hash-fallback");
    }

    #[tokio::test]
    async fn test_current_provider_name() {
        use crate::embedding::HashFallbackProvider;
        use std::sync::Arc;

        let provider = Arc::new(HashFallbackProvider::new(384));
        set_embedding_provider(provider);

        assert_eq!(current_provider_name(), "hash-fallback");
    }

    #[tokio::test]
    async fn test_rebuild_embeddings_with_provider() {
        use crate::embedding::HashFallbackProvider;

        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("prov_a", "alpha content", "cat").await.expect("set a");
        set("prov_b", "beta content", "cat").await.expect("set b");

        let provider = HashFallbackProvider::default();
        let stats = rebuild_embeddings_with_provider(&provider)
            .await
            .expect("rebuild with provider");

        assert_eq!(stats.total, 2);
        assert_eq!(stats.succeeded, 2);
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.model_name, "hash-fallback");
        assert_eq!(stats.dims, 384);

        // Verify search works after provider rebuild
        let results = search_memory("alpha content", None, 5)
            .await
            .expect("search");
        assert!(
            results.iter().any(|r| r.chunk.content.contains("alpha")),
            "rebuilt index should be searchable"
        );
    }

    #[tokio::test]
    async fn test_rebuild_embeddings_with_provider_records_model_in_db() {
        use crate::embedding::HashFallbackProvider;

        let _root = TestRoot::new();
        init_db().await.expect("init db");

        set("prov_model", "model check value", "cat")
            .await
            .expect("set");

        let provider = HashFallbackProvider::new(384);
        let stats = rebuild_embeddings_with_provider(&provider)
            .await
            .expect("rebuild");

        assert_eq!(stats.model_name, "hash-fallback");

        // Verify model name stored in memory_embeddings
        let pool = db::pool().await.expect("pool");
        let row = sqlx::query(
            "SELECT me.model FROM memory_embeddings me \
             JOIN memory_chunks mc ON me.chunk_id = mc.id \
             WHERE mc.memory_id = (SELECT id FROM memory_entries WHERE key = 'prov_model')",
        )
        .fetch_one(&pool)
        .await
        .expect("fetch embedding");

        let model: String = row.try_get("model").expect("model");
        assert_eq!(model, "hash-fallback");
    }

    #[tokio::test]
    async fn test_active_provider_embed_works() {
        use crate::embedding::HashFallbackProvider;
        use std::sync::Arc;

        let provider = Arc::new(HashFallbackProvider::default());
        set_embedding_provider(provider);

        let active = active_embedding_provider();
        let vec = active.embed("test text").await.expect("embed");
        assert_eq!(vec.len(), 384);
    }
}
