//! Pluggable embedding provider abstraction (TASK-110).
//!
//! Provides an async [`EmbeddingProvider`] trait and several implementations:
//! - [`HashFallbackProvider`] — deterministic hash-based pseudo-embedding (always available)
//! - [`BgeSmallZhProvider`] — Chinese BGE-small model via HTTP API
//! - [`BgeSmallEnProvider`] — English BGE-small model via HTTP API
//! - [`CompositeEmbeddingProvider`] — primary + fallback auto-selection
//!
//! The trait is intentionally async so that HTTP-backed providers can be used
//! without blocking the tokio runtime.

use std::sync::Arc;

/// Async embedding provider trait.
///
/// Each implementation produces `Vec<f32>` vectors of a fixed dimension,
/// identified by a human-readable `model_name`.
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for the given text.
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;

    /// The dimensionality of vectors produced by this provider.
    fn dimension(&self) -> usize;

    /// Short identifier stored alongside embeddings in the database.
    fn model_name(&self) -> &str;
}

// ─── HashFallbackProvider ──────────────────────────────────────────────

/// Deterministic hash-based embedding provider.
///
/// Same text always produces the same vector. No semantic understanding —
/// intended as a fallback when real models are unavailable.
pub struct HashFallbackProvider {
    dim: usize,
}

impl HashFallbackProvider {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    /// Synchronous embed used internally by providers that fall back to hash.
    pub fn embed_sync(&self, text: &str) -> Vec<f32> {
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
}

impl Default for HashFallbackProvider {
    fn default() -> Self {
        Self { dim: 384 }
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for HashFallbackProvider {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(self.embed_sync(text))
    }

    fn dimension(&self) -> usize {
        self.dim
    }

    fn model_name(&self) -> &str {
        "hash-fallback"
    }
}

// ─── BgeSmallEnProvider ────────────────────────────────────────────────

/// English BGE-small-en-v1.5 embedding provider via HTTP API.
///
/// Calls a configurable endpoint (e.g. a local embedding server) that accepts
/// `{"input": "<text>"}` and returns `{"data": [{"embedding": [...]}]}`.
///
/// Falls back to [`HashFallbackProvider`] if the API is unreachable.
pub struct BgeSmallEnProvider {
    api_url: String,
    dim: usize,
    hash_fallback: HashFallbackProvider,
}

impl BgeSmallEnProvider {
    pub fn new(api_url: impl Into<String>) -> Self {
        Self {
            api_url: api_url.into(),
            dim: 384,
            hash_fallback: HashFallbackProvider::new(384),
        }
    }

    /// Create with a default local endpoint.
    pub fn default_local() -> Self {
        Self::new("http://127.0.0.1:6333/embed")
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for BgeSmallEnProvider {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        match call_embedding_api(&self.api_url, text).await {
            Ok(vec) if vec.len() == self.dim => Ok(vec),
            Ok(vec) => {
                tracing::warn!(
                    "BgeSmallEnProvider: expected {} dims, got {}; using hash fallback",
                    self.dim,
                    vec.len()
                );
                Ok(self.hash_fallback.embed_sync(text))
            }
            Err(e) => {
                tracing::warn!("BgeSmallEnProvider API error, using hash fallback: {}", e);
                Ok(self.hash_fallback.embed_sync(text))
            }
        }
    }

    fn dimension(&self) -> usize {
        self.dim
    }

    fn model_name(&self) -> &str {
        "bge-small-en-v1.5"
    }
}

// ─── BgeSmallZhProvider ────────────────────────────────────────────────

/// Chinese BGE-small-zh-v1.5 embedding provider via HTTP API.
///
/// Falls back to [`HashFallbackProvider`] if the model is unavailable.
pub struct BgeSmallZhProvider {
    api_url: String,
    dim: usize,
    hash_fallback: HashFallbackProvider,
}

impl BgeSmallZhProvider {
    pub fn new(api_url: impl Into<String>) -> Self {
        Self {
            api_url: api_url.into(),
            dim: 384,
            hash_fallback: HashFallbackProvider::new(384),
        }
    }

    /// Create with a default local endpoint.
    pub fn default_local() -> Self {
        Self::new("http://127.0.0.1:6334/embed")
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for BgeSmallZhProvider {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        match call_embedding_api(&self.api_url, text).await {
            Ok(vec) if vec.len() == self.dim => Ok(vec),
            Ok(vec) => {
                tracing::warn!(
                    "BgeSmallZhProvider: expected {} dims, got {}; using hash fallback",
                    self.dim,
                    vec.len()
                );
                Ok(self.hash_fallback.embed_sync(text))
            }
            Err(e) => {
                tracing::warn!("BgeSmallZhProvider API error, using hash fallback: {}", e);
                Ok(self.hash_fallback.embed_sync(text))
            }
        }
    }

    fn dimension(&self) -> usize {
        self.dim
    }

    fn model_name(&self) -> &str {
        "bge-small-zh-v1.5"
    }
}

// ─── CompositeEmbeddingProvider ────────────────────────────────────────

/// Composite provider that tries a primary provider first,
/// falling back to a secondary provider on failure.
///
/// Typical usage: `BgeSmallZhProvider` primary → `HashFallbackProvider` fallback.
pub struct CompositeEmbeddingProvider {
    primary: Arc<dyn EmbeddingProvider>,
    fallback: Arc<dyn EmbeddingProvider>,
}

impl CompositeEmbeddingProvider {
    pub fn new(primary: Arc<dyn EmbeddingProvider>, fallback: Arc<dyn EmbeddingProvider>) -> Self {
        Self { primary, fallback }
    }

    /// Build a default composite: BgeSmallZh → HashFallback.
    pub fn default_composite() -> Self {
        Self {
            primary: Arc::new(BgeSmallZhProvider::default_local()),
            fallback: Arc::new(HashFallbackProvider::default()),
        }
    }

    /// Build a composite with explicit providers.
    pub fn with_providers(
        primary: impl EmbeddingProvider + 'static,
        fallback: impl EmbeddingProvider + 'static,
    ) -> Self {
        Self {
            primary: Arc::new(primary),
            fallback: Arc::new(fallback),
        }
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for CompositeEmbeddingProvider {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        // Try primary first; on any error or dimension mismatch, use fallback.
        match self.primary.embed(text).await {
            Ok(vec) if vec.len() == self.primary.dimension() => Ok(vec),
            Ok(vec) => {
                tracing::warn!(
                    "CompositeEmbeddingProvider: primary '{}' returned {} dims (expected {}); using fallback '{}'",
                    self.primary.model_name(),
                    vec.len(),
                    self.primary.dimension(),
                    self.fallback.model_name(),
                );
                self.fallback.embed(text).await
            }
            Err(e) => {
                tracing::warn!(
                    "CompositeEmbeddingProvider: primary '{}' failed: {}; using fallback '{}'",
                    self.primary.model_name(),
                    e,
                    self.fallback.model_name(),
                );
                self.fallback.embed(text).await
            }
        }
    }

    fn dimension(&self) -> usize {
        // Report primary dimension; fallback is expected to match.
        self.primary.dimension()
    }

    fn model_name(&self) -> &str {
        // We return the primary's model name since that's what gets stored
        // in the database on successful embed.
        self.primary.model_name()
    }
}

// ─── HTTP helper ───────────────────────────────────────────────────────

/// Call a generic embedding HTTP API.
///
/// Expected request: `POST <url>` with JSON body `{"input": "<text>"}`
/// Expected response: `{"data": [{"embedding": [...]}]}`  (OpenAI-compatible format)
///
/// Returns the embedding vector or an error.
async fn call_embedding_api(url: &str, text: &str) -> anyhow::Result<Vec<f32>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let body = serde_json::json!({ "input": text });

    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "embedding API returned status {}",
            resp.status()
        ));
    }

    let json: serde_json::Value = resp.json().await?;
    let embedding = json
        .get("data")
        .and_then(|d| d.get(0))
        .and_then(|d| d.get("embedding"))
        .and_then(|e| e.as_array())
        .ok_or_else(|| anyhow::anyhow!("unexpected embedding API response format"))?;

    let vec: Vec<f32> = embedding
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();

    Ok(vec)
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hash_fallback_deterministic() {
        let provider = HashFallbackProvider::default();
        let a = provider.embed("hello world").await.unwrap();
        let b = provider.embed("hello world").await.unwrap();
        assert_eq!(a, b, "same text must produce identical embeddings");
    }

    #[tokio::test]
    async fn test_hash_fallback_dimension() {
        let provider = HashFallbackProvider::default();
        assert_eq!(provider.dimension(), 384);
        let vec = provider.embed("test").await.unwrap();
        assert_eq!(vec.len(), 384);
    }

    #[tokio::test]
    async fn test_hash_fallback_custom_dimension() {
        let provider = HashFallbackProvider::new(128);
        assert_eq!(provider.dimension(), 128);
        let vec = provider.embed("test").await.unwrap();
        assert_eq!(vec.len(), 128);
    }

    #[tokio::test]
    async fn test_hash_fallback_different_inputs_differ() {
        let provider = HashFallbackProvider::default();
        let a = provider.embed("hello").await.unwrap();
        let b = provider.embed("world").await.unwrap();
        assert_ne!(a, b, "different text must produce different embeddings");
    }

    #[tokio::test]
    async fn test_hash_fallback_model_name() {
        let provider = HashFallbackProvider::default();
        assert_eq!(provider.model_name(), "hash-fallback");
    }

    #[tokio::test]
    async fn test_bge_small_en_model_name_and_dimension() {
        let provider = BgeSmallEnProvider::default_local();
        assert_eq!(provider.model_name(), "bge-small-en-v1.5");
        assert_eq!(provider.dimension(), 384);
    }

    #[tokio::test]
    async fn test_bge_small_en_falls_back_to_hash() {
        // API at 127.0.0.1:6333 is not running; should fallback gracefully.
        let provider = BgeSmallEnProvider::new("http://127.0.0.1:19999/embed");
        let vec = provider.embed("test fallback").await.unwrap();
        assert_eq!(vec.len(), 384, "fallback should produce correct dimension");

        // Verify it's deterministic (hash fallback behavior)
        let vec2 = provider.embed("test fallback").await.unwrap();
        assert_eq!(vec, vec2, "fallback should be deterministic");
    }

    #[tokio::test]
    async fn test_bge_small_zh_model_name_and_dimension() {
        let provider = BgeSmallZhProvider::default_local();
        assert_eq!(provider.model_name(), "bge-small-zh-v1.5");
        assert_eq!(provider.dimension(), 384);
    }

    #[tokio::test]
    async fn test_bge_small_zh_falls_back_to_hash() {
        let provider = BgeSmallZhProvider::new("http://127.0.0.1:19999/embed");
        let vec = provider.embed("测试中文回退").await.unwrap();
        assert_eq!(vec.len(), 384);
    }

    #[tokio::test]
    async fn test_composite_uses_primary() {
        // Both primary and fallback are hash-based; primary should be used.
        let primary = HashFallbackProvider::new(384);
        let fallback = HashFallbackProvider::new(384);
        let composite = CompositeEmbeddingProvider::with_providers(primary, fallback);

        let vec = composite.embed("composite test").await.unwrap();
        assert_eq!(vec.len(), 384);
        assert_eq!(composite.model_name(), "hash-fallback");
    }

    #[tokio::test]
    async fn test_composite_fallback_on_primary_failure() {
        // Primary points to unreachable URL; should fall back.
        let primary = BgeSmallEnProvider::new("http://127.0.0.1:19999/embed");
        let fallback = HashFallbackProvider::new(384);
        let composite = CompositeEmbeddingProvider::with_providers(primary, fallback);

        let vec = composite.embed("fallback test").await.unwrap();
        assert_eq!(
            vec.len(),
            384,
            "should produce correct dimension via fallback"
        );

        // Should match direct hash fallback
        let hash = HashFallbackProvider::new(384);
        let hash_vec = hash.embed("fallback test").await.unwrap();
        assert_eq!(vec, hash_vec, "composite fallback should match direct hash");
    }

    #[tokio::test]
    async fn test_composite_dimension_reports_primary() {
        let primary = HashFallbackProvider::new(256);
        let fallback = HashFallbackProvider::new(384);
        let composite = CompositeEmbeddingProvider::with_providers(primary, fallback);

        // dimension() reports primary's dimension
        assert_eq!(composite.dimension(), 256);

        // But embed falls back to fallback (different dim) — in practice
        // primary and fallback should have the same dim.
        // This test just verifies the primary is tried first.
        let vec = composite.embed("dim test").await.unwrap();
        assert_eq!(
            vec.len(),
            256,
            "primary was reachable, should use primary dim"
        );
    }

    #[tokio::test]
    async fn test_composite_model_name_reports_primary() {
        let primary = BgeSmallZhProvider::new("http://127.0.0.1:19999/embed");
        let fallback = HashFallbackProvider::default();
        let composite = CompositeEmbeddingProvider::with_providers(primary, fallback);

        // model_name reports primary's name even though primary will fail
        // and we'll use fallback for actual embedding
        assert_eq!(composite.model_name(), "bge-small-zh-v1.5");
    }
}
