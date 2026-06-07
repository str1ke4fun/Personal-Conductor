// D-1: CallerContext and ResolvedModel are mirrored in model-router-core::types.
// GB2: unit tests for ModelResolver::resolve() are at the bottom of this file.
// ModelResolver — unified LLM model selection for all call sites.
//
// Usage:
//   let resolved = ModelResolver::resolve(CallerContext::ChatMainLoop, None).await?;
//   llm::call_with_tools(..., &resolved.model_id, ...).await?;

use crate::agent_backends::BackendKind;
use crate::llm_profiles::TransportKind;
use crate::routing::{OodaPhase, RoutingPolicyFilter, WorkKind};
use anyhow::Result;
use serde::{Deserialize, Serialize};

// ── CallerContext ─────────────────────────────────────────────────────────────

/// The context in which a model call is being made.
/// Used by ModelResolver to pick the right profile and routing policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallerContext {
    /// Main chat loop responding to a user message.
    ChatMainLoop,
    /// Conversation summarizer running in background.
    Summarizer,
    /// Smart monitor checking for anomalies.
    SmartMonitor,
    /// Goal orchestrator — OODA decide phase.
    GoalOrchestrator {
        phase: OodaPhase,
        work_kind: WorkKind,
    },
    /// Subagent spawned from a tool call.
    Subagent { work_kind: WorkKind },
    /// Memory recall / embedding generation.
    MemoryRecall,
}

impl CallerContext {
    pub fn work_kind(&self) -> WorkKind {
        match self {
            Self::ChatMainLoop | Self::Summarizer | Self::SmartMonitor | Self::MemoryRecall => {
                WorkKind::Planning
            }
            Self::GoalOrchestrator { work_kind, .. } => work_kind.clone(),
            Self::Subagent { work_kind } => work_kind.clone(),
        }
    }

    pub fn ooda_phase(&self) -> Option<OodaPhase> {
        match self {
            Self::GoalOrchestrator { phase, .. } => Some(phase.clone()),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ChatMainLoop => "chat_main_loop",
            Self::Summarizer => "summarizer",
            Self::SmartMonitor => "smart_monitor",
            Self::GoalOrchestrator { .. } => "goal_orchestrator",
            Self::Subagent { .. } => "subagent",
            Self::MemoryRecall => "memory_recall",
        }
    }
}

// ── ResolvedModel ─────────────────────────────────────────────────────────────

/// The result of model resolution — a concrete model+backend ready for use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedModel {
    /// The model identifier to pass to the LLM client (e.g. "claude-sonnet-4-5").
    pub model_id: String,
    /// How to invoke the model.
    pub transport: TransportKind,
    /// The LLM profile that was selected, if any.
    pub profile_id: Option<String>,
    /// The routing policy that was applied, if any.
    pub policy_id: Option<String>,
    /// Whether a fallback was used (profile was unavailable or disabled).
    pub fallback_used: bool,
    /// Which backend kind was selected.
    pub backend_kind: BackendKind,
    // ── Profile-sourced request config fields (None = use global config fallback) ──
    pub provider: Option<String>,
    pub api_base_url: Option<String>,
    pub api_key: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
}

// ── ModelResolver ─────────────────────────────────────────────────────────────

pub struct ModelResolver;

impl ModelResolver {
    /// Resolve which model to use for a given caller context.
    ///
    /// Resolution order:
    ///  1. If `hint` is provided, use it directly.
    ///  2. Look up enabled RoutingPolicy matching `ctx.work_kind()` and `ooda_phase()`.
    ///  3. If policy has a profile_id, load the LlmProfile.
    ///  4. Fall back to `config.llm.model`.
    ///
    /// A-5: `emit_model_routed` fires on every return path.
    pub async fn resolve(ctx: CallerContext, hint: Option<&str>) -> Result<ResolvedModel> {
        Self::resolve_with_request(ctx, hint, None).await
    }

    /// Like `resolve`, but also writes a `model.routed` turn event when
    /// `request_id` is `Some`.
    pub async fn resolve_with_request(
        ctx: CallerContext,
        hint: Option<&str>,
        request_id: Option<&str>,
    ) -> Result<ResolvedModel> {
        Self::resolve_with_context(ctx, hint, request_id, None, None).await
    }

    /// Full resolution with optional task linkage for route_decisions persistence. (S2)
    pub async fn resolve_with_context(
        ctx: CallerContext,
        hint: Option<&str>,
        request_id: Option<&str>,
        task_id: Option<&str>,
        workspace_id: Option<&str>,
    ) -> Result<ResolvedModel> {
        let resolved = Self::resolve_inner(&ctx, hint).await;
        crate::events::emit_model_routed(
            ctx.as_str(),
            &resolved.model_id,
            resolved.profile_id.as_deref(),
            resolved.policy_id.as_deref(),
            resolved.fallback_used,
            request_id,
        )
        .await;
        // S2: persist route_decisions row so routing is auditable per turn/task
        if let Err(e) = Self::persist_route_decision(&ctx, &resolved, task_id, workspace_id).await {
            tracing::warn!("persist_route_decision failed: {e:#}");
        }
        Ok(resolved)
    }

    async fn persist_route_decision(
        ctx: &CallerContext,
        resolved: &ResolvedModel,
        task_id: Option<&str>,
        workspace_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let pool = crate::db::pool().await?;
        let id = format!("rd-{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let reason = if resolved.fallback_used {
            format!("fallback: no matching policy for {}", ctx.as_str())
        } else {
            format!("policy matched for {}", ctx.as_str())
        };
        // P0.3: backend_id should be a unique backend instance identifier,
        // not just the backend_kind string. When a profile_id is available,
        // combine it with backend_kind to form a more specific identifier.
        // TODO: introduce per-instance backend IDs once the backend registry
        // supports them.
        let backend_id = match resolved.profile_id.as_deref() {
            Some(pid) => format!("{}:{}", resolved.backend_kind.as_str(), pid),
            None => resolved.backend_kind.as_str().to_string(),
        };
        sqlx::query(
            r#"INSERT INTO route_decisions
               (id, workspace_id, task_id, task_kind, policy_id, backend_id,
                backend_kind, profile_id, reason, fallback_used, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
        )
        .bind(&id)
        .bind(workspace_id.unwrap_or(""))
        .bind(task_id)
        .bind(ctx.work_kind().as_str())
        .bind(resolved.policy_id.as_deref())
        .bind(backend_id)
        .bind(resolved.backend_kind.as_str())
        .bind(resolved.profile_id.as_deref())
        .bind(&reason)
        .bind(resolved.fallback_used)
        .bind(&now)
        .execute(&pool)
        .await?;
        Ok(())
    }

    async fn resolve_inner(ctx: &CallerContext, hint: Option<&str>) -> ResolvedModel {
        // 1. Direct hint override.
        if let Some(model_id) = hint {
            return ResolvedModel {
                model_id: model_id.to_string(),
                transport: TransportKind::HttpApi,
                profile_id: None,
                policy_id: None,
                fallback_used: false,
                backend_kind: BackendKind::ClaudeP,
                provider: None,
                api_base_url: None,
                api_key: None,
                temperature: None,
                max_tokens: None,
            };
        }

        // 2. Try routing policy lookup (phase-aware).
        let policies = crate::routing::list_policies(RoutingPolicyFilter {
            work_kind: Some(ctx.work_kind()),
            caller_phase: ctx.ooda_phase(),
            enabled: Some(true),
            limit: Some(1),
        })
        .await
        .unwrap_or_default();

        if let Some(policy) = policies.first() {
            // 3. Load profile if policy has one.
            if let Some(ref profile_id) = policy.profile_id {
                if let Ok(Some(profile)) = crate::llm_profiles::get_profile(profile_id).await {
                    if profile.enabled {
                        return ResolvedModel {
                            model_id: profile.model_id.clone(),
                            transport: profile.transport.clone(),
                            profile_id: Some(profile_id.clone()),
                            policy_id: Some(policy.id.clone()),
                            fallback_used: false,
                            backend_kind: policy.backend_kind.clone(),
                            provider: Some(profile.provider.clone()),
                            api_base_url: Some(profile.api_base_url.clone()),
                            api_key: profile.api_key_encrypted.clone(),
                            temperature: Some(profile.temperature),
                            max_tokens: Some(profile.max_tokens),
                        };
                    }
                }
            }
            // Policy exists but no usable profile — use backend default model.
            let model_id = Self::default_model_for_backend(&policy.backend_kind).await;
            return ResolvedModel {
                model_id,
                transport: TransportKind::HttpApi,
                profile_id: None,
                policy_id: Some(policy.id.clone()),
                fallback_used: true,
                backend_kind: policy.backend_kind.clone(),
                provider: None,
                api_base_url: None,
                api_key: None,
                temperature: None,
                max_tokens: None,
            };
        }

        // 4. Fall back to config default — no policy matched.
        let model_id = Self::config_default_model().await;
        ResolvedModel {
            model_id,
            transport: TransportKind::HttpApi,
            profile_id: None,
            policy_id: None,
            fallback_used: true,
            backend_kind: BackendKind::ClaudeP,
            provider: None,
            api_base_url: None,
            api_key: None,
            temperature: None,
            max_tokens: None,
        }
    }

    /// Read the configured default model from CoreConfig (field: `llm.model`).
    async fn config_default_model() -> String {
        crate::config::load()
            .await
            .map(|c| c.llm.model)
            .unwrap_or_else(|_| "claude-sonnet-4-5".to_string())
    }

    /// Return a sensible default model string for a given backend kind.
    /// Only used when a policy exists but has no profile attached.
    async fn default_model_for_backend(backend: &BackendKind) -> String {
        match backend {
            // ClaudeP runs the claude CLI, which picks its own model.
            BackendKind::ClaudeP => Self::config_default_model().await,
            // CodexInteractive drives Codex CLI; model name is passed through.
            BackendKind::CodexInteractive => Self::config_default_model().await,
            // AgentTeam and Review delegate to sub-backends; fall back to config.
            BackendKind::AgentTeam | BackendKind::Review => Self::config_default_model().await,
            // McpRouter delegates to the model-router-mcp server; use config default as identifier.
            BackendKind::McpRouter => Self::config_default_model().await,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        routing::{CreateRoutingPolicyInput, OodaPhase, WorkKind},
        test_support::TestRoot,
    };

    // ── 1. hint branch ────────────────────────────────────────────────────────

    /// When a hint is provided, resolve() returns that model directly without
    /// touching the DB.  No TestRoot needed — hint path is pure in-memory.
    #[tokio::test]
    async fn resolve_hint_returns_hint_model() {
        let resolved = ModelResolver::resolve(CallerContext::ChatMainLoop, Some("gpt-4o"))
            .await
            .expect("resolve with hint");

        assert_eq!(resolved.model_id, "gpt-4o");
        assert!(!resolved.fallback_used);
        assert!(resolved.profile_id.is_none());
        assert!(resolved.policy_id.is_none());
    }

    // ── 2. policy + profile branch ────────────────────────────────────────────

    /// When an enabled routing policy exists and it points at an enabled profile,
    /// resolve() returns the profile's model_id with fallback_used = false.
    #[tokio::test]
    async fn resolve_policy_with_profile_returns_profile_model() {
        let _root = TestRoot::new();

        // Create the profile first so validate_profile() passes.
        let profile = crate::llm_profiles::create_profile(
            "Test GPT",
            "openai",
            "gpt-4-turbo",
            "https://api.openai.com/v1",
            None,
            4096,
            0.7,
        )
        .await
        .expect("create profile");

        // Create a planning policy that references the profile.
        let policy = crate::routing::create_policy(CreateRoutingPolicyInput {
            id: None,
            work_kind: WorkKind::Planning,
            caller_phase: None,
            backend_kind: BackendKind::ClaudeP,
            profile_id: Some(profile.id.clone()),
            priority: 10,
            enabled: true,
            reason_template: None,
        })
        .await
        .expect("create policy");

        let resolved = ModelResolver::resolve(CallerContext::ChatMainLoop, None)
            .await
            .expect("resolve via policy+profile");

        assert_eq!(resolved.model_id, "gpt-4-turbo");
        assert!(!resolved.fallback_used);
        assert_eq!(resolved.profile_id.as_deref(), Some(profile.id.as_str()));
        assert_eq!(resolved.policy_id.as_deref(), Some(policy.id.as_str()));
    }

    // ── 3. fallback branch (no policy matches) ────────────────────────────────

    /// When no routing policy exists for the work_kind, resolve() falls back to
    /// the config default model with fallback_used = true.
    #[tokio::test]
    async fn resolve_no_policy_uses_config_default() {
        let _root = TestRoot::new();
        // Empty DB — no policies seeded.

        let resolved = ModelResolver::resolve(CallerContext::ChatMainLoop, None)
            .await
            .expect("resolve fallback");

        // config::load() returns CoreConfig::default() from a fresh temp dir,
        // whose llm.model is "gpt-4.1-mini".
        assert_eq!(resolved.model_id, "gpt-4.1-mini");
        assert!(resolved.fallback_used);
        assert!(resolved.profile_id.is_none());
        assert!(resolved.policy_id.is_none());
    }

    // ── 4. policy-no-profile branch ───────────────────────────────────────────

    /// When a policy exists but has no profile_id, resolve() uses the backend
    /// default model (config default) and sets fallback_used = true.
    #[tokio::test]
    async fn resolve_policy_without_profile_uses_backend_default() {
        let _root = TestRoot::new();

        let policy = crate::routing::create_policy(CreateRoutingPolicyInput {
            id: None,
            work_kind: WorkKind::Planning,
            caller_phase: None,
            backend_kind: BackendKind::ClaudeP,
            profile_id: None, // no profile attached
            priority: 10,
            enabled: true,
            reason_template: None,
        })
        .await
        .expect("create policy without profile");

        let resolved = ModelResolver::resolve(CallerContext::ChatMainLoop, None)
            .await
            .expect("resolve policy-no-profile");

        assert_eq!(resolved.model_id, "gpt-4.1-mini");
        assert!(resolved.fallback_used);
        assert!(resolved.profile_id.is_none());
        // Policy was matched even though no profile was usable.
        assert_eq!(resolved.policy_id.as_deref(), Some(policy.id.as_str()));
    }

    // ── 5. caller_phase filter ────────────────────────────────────────────────

    /// Two policies share the same work_kind but differ in caller_phase:
    ///   - policy_generic: caller_phase = None  (wildcard)
    ///   - policy_reason:  caller_phase = Some(Reason)
    ///
    /// Calling with GoalOrchestrator { phase: Reason } must return the
    /// Reason-specific policy's profile, not the wildcard one.
    #[tokio::test]
    async fn resolve_caller_phase_selects_specific_policy() {
        let _root = TestRoot::new();

        // Generic profile (should NOT be picked for the Reason phase call).
        let profile_generic = crate::llm_profiles::create_profile(
            "Generic",
            "openai",
            "gpt-3.5-turbo",
            "https://api.openai.com/v1",
            None,
            2048,
            0.5,
        )
        .await
        .expect("create generic profile");

        // Reason-phase profile (should be picked).
        let profile_reason = crate::llm_profiles::create_profile(
            "Reason Phase",
            "openai",
            "gpt-4o",
            "https://api.openai.com/v1",
            None,
            8192,
            0.2,
        )
        .await
        .expect("create reason profile");

        // Wildcard policy — matches any phase.
        crate::routing::create_policy(CreateRoutingPolicyInput {
            id: None,
            work_kind: WorkKind::Planning,
            caller_phase: None,
            backend_kind: BackendKind::ClaudeP,
            profile_id: Some(profile_generic.id.clone()),
            priority: 5,
            enabled: true,
            reason_template: None,
        })
        .await
        .expect("create generic policy");

        // Reason-specific policy — higher priority so it wins when phase matches.
        let policy_reason = crate::routing::create_policy(CreateRoutingPolicyInput {
            id: None,
            work_kind: WorkKind::Planning,
            caller_phase: Some(OodaPhase::Reason),
            backend_kind: BackendKind::ClaudeP,
            profile_id: Some(profile_reason.id.clone()),
            priority: 20,
            enabled: true,
            reason_template: None,
        })
        .await
        .expect("create reason policy");

        let ctx = CallerContext::GoalOrchestrator {
            phase: OodaPhase::Reason,
            work_kind: WorkKind::Planning,
        };
        let resolved = ModelResolver::resolve(ctx, None)
            .await
            .expect("resolve with phase");

        assert_eq!(resolved.model_id, "gpt-4o");
        assert!(!resolved.fallback_used);
        assert_eq!(
            resolved.profile_id.as_deref(),
            Some(profile_reason.id.as_str())
        );
        assert_eq!(
            resolved.policy_id.as_deref(),
            Some(policy_reason.id.as_str())
        );
    }
}
