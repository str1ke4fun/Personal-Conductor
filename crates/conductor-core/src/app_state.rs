//! Unified application state, replacing scattered `lazy_static!` globals.
//!
//! Currently consolidates `ToolRegistry`. Other globals (MCP, Initiative, Codex,
//! CommandRuns) are left in-place with TODO comments for future migration.

use std::sync::{OnceLock, RwLock};

use crate::tools::ToolRegistry;

/// Process-wide shared state, initialized once via [`AppState::global`].
pub struct AppState {
    tool_registry: RwLock<ToolRegistry>,
    // TODO: Move MCP_REGISTRY from mcp.rs into AppState
    // TODO: Move INITIATIVE_ENGINE from initiative.rs into AppState
    // TODO: Move LIVE_SESSIONS from codex.rs into AppState
    // TODO: Move LIVE_RUNS from command_runs.rs into AppState
}

static APP_STATE: OnceLock<AppState> = OnceLock::new();

impl AppState {
    /// Create a fresh `AppState` with default (empty) inner state.
    fn new() -> Self {
        Self {
            tool_registry: RwLock::new(ToolRegistry::new()),
        }
    }

    /// Return the process-wide `AppState`, initializing it on first call.
    pub fn global() -> &'static AppState {
        APP_STATE.get_or_init(Self::new)
    }

    /// Access the tool registry behind its `RwLock`.
    pub fn tool_registry(&self) -> &RwLock<ToolRegistry> {
        &self.tool_registry
    }
}
