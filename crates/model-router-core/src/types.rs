// model-router-core::types
//
// Pure (no-DB, no-async, no-Tauri) shared type definitions for model routing.
// These are mirrored from conductor-core and serve as the future source of
// truth shared between conductor-core and any MCP server crate.

use serde::{Deserialize, Serialize};

// ── BackendKind ───────────────────────────────────────────────────────────────
// Mirrored from crates/conductor-core/src/agent_backends.rs

/// The kind of agent backend that executes work.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    ClaudeP,
    CodexInteractive,
    AgentTeam,
    Review,
    /// Routes via the model-router-mcp server (D-3).
    McpRouter,
}

impl BackendKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClaudeP => "claude_p",
            Self::CodexInteractive => "codex_interactive",
            Self::AgentTeam => "agent_team",
            Self::Review => "review",
            Self::McpRouter => "mcp_router",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "claude_p" => Ok(Self::ClaudeP),
            "codex_interactive" => Ok(Self::CodexInteractive),
            "agent_team" => Ok(Self::AgentTeam),
            "review" => Ok(Self::Review),
            other => anyhow::bail!("unknown backend kind: {other}"),
        }
    }
}

// ── TransportKind ─────────────────────────────────────────────────────────────
// Mirrored from crates/conductor-core/src/llm_profiles.rs

/// How the model is invoked — HTTP API, CLI subprocess, or MCP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    /// Standard HTTP/HTTPS REST API (OpenAI-compatible or Anthropic API).
    HttpApi,
    /// Claude CLI subprocess (`claude -p`).
    ClaudeCli,
    /// Codex CLI subprocess.
    CodexCli,
    /// MCP server (model-router-mcp or any MCP tool server).
    McpServer,
}

impl TransportKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HttpApi => "http_api",
            Self::ClaudeCli => "claude_cli",
            Self::CodexCli => "codex_cli",
            Self::McpServer => "mcp_server",
        }
    }

    /// Infer transport from legacy `provider` string for backward compatibility.
    pub fn from_provider(provider: &str) -> Self {
        match provider {
            "claude_cli" => Self::ClaudeCli,
            "codex_cli" => Self::CodexCli,
            "mcp_router" | "mcp_server" => Self::McpServer,
            _ => Self::HttpApi,
        }
    }

    /// Parse from the stored TEXT value.
    pub fn from_str_or_default(s: &str) -> Self {
        match s {
            "claude_cli" => Self::ClaudeCli,
            "codex_cli" => Self::CodexCli,
            "mcp_server" => Self::McpServer,
            _ => Self::HttpApi,
        }
    }
}

impl Default for TransportKind {
    fn default() -> Self {
        Self::HttpApi
    }
}

// ── WorkKind ──────────────────────────────────────────────────────────────────
// Mirrored from crates/conductor-core/src/routing.rs

/// The kind of work being dispatched — used for backend selection and routing.
/// Renamed from `TaskKind`; use `WorkKind` in new code.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkKind {
    Planning,
    Coding,
    Review,
    Testing,
    Document,
    ExternalAction,
}

impl WorkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planning => "planning",
            Self::Coding => "coding",
            Self::Review => "review",
            Self::Testing => "testing",
            Self::Document => "document",
            Self::ExternalAction => "external_action",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "planning" => Ok(Self::Planning),
            "coding" => Ok(Self::Coding),
            "review" => Ok(Self::Review),
            "testing" => Ok(Self::Testing),
            "document" => Ok(Self::Document),
            "external_action" => Ok(Self::ExternalAction),
            other => anyhow::bail!("unknown work kind: {other}"),
        }
    }

    pub fn default_backend_kind(&self) -> BackendKind {
        match self {
            Self::Coding | Self::Testing => BackendKind::CodexInteractive,
            Self::Review => BackendKind::Review,
            Self::Document | Self::Planning => BackendKind::ClaudeP,
            Self::ExternalAction => BackendKind::AgentTeam,
        }
    }
}

// ── OodaPhase ─────────────────────────────────────────────────────────────────
// Mirrored from crates/conductor-core/src/routing.rs

/// Phase within the OODA loop — used to route LLM calls to different models
/// depending on which cognitive stage is active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OodaPhase {
    /// Bootstrap: first attempt to solve the goal directly.
    Bootstrap,
    /// Reason: read the whole graph, judge if goal is met, generate new intents.
    Reason,
    /// Explore: claim one open intent and execute it.
    Explore,
    /// Review: evaluate task output against acceptance criteria.
    Review,
}

impl OodaPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Reason => "reason",
            Self::Explore => "explore",
            Self::Review => "review",
        }
    }
}

// ── CallerContext ─────────────────────────────────────────────────────────────
// Mirrored from crates/conductor-core/src/model_resolver.rs

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
// Mirrored from crates/conductor-core/src/model_resolver.rs

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
