// model-router-core
//
// Pure shared types for model routing, usable by conductor-core and any
// future MCP server crate without pulling in DB, async runtimes, or Tauri.

pub mod types;

pub use types::{BackendKind, CallerContext, OodaPhase, ResolvedModel, TransportKind, WorkKind};
