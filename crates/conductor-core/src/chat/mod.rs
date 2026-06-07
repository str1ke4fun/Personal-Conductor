// Submodule declarations
pub mod active_run;
mod commands;
mod db;
mod handler;
mod prompt;
#[cfg(feature = "tauri-events")]
mod send_v2;
mod session;
mod tools;
mod turns;
mod types;
mod util;

#[cfg(test)]
mod tests;

// Re-exports for public API compatibility
pub use types::{
    ChatCapability, ChatMessage, ChatMessageV2, ChatReply, ChatRole, ChatTaskMode, CompletionStep,
    ContentBlock, StreamChatTokenEvent, ThinkingUpdateEvent, ToolCallRecord,
    ToolExecutionUpdateEvent,
};

pub use session::{
    append_assistant_message_to_session, append_user_message_to_session, archive_chat_session,
    create_chat_session, ensure_chat_session, find_session_for_goal, get_chat_session_messages,
    get_chat_session_messages_v2, list_chat_sessions, rename_chat_session, set_chat_session_kind,
    update_chat_session_workspace, update_message_content, ChatSession, ChatSessionSummary,
};

pub use handler::send;
#[cfg(feature = "tauri-events")]
pub use send_v2::{
    send_message_v2, send_message_v2_with_session, send_message_v2_with_session_projection,
    send_message_v2_with_session_projection_ctx, ChatExecutionContext,
};

pub use db::{history, history_for_session, record_assistant_message};
pub use turns::{
    append_turn_event_by_request, get_turn_by_goal_cycle_id, list_message_projections_by_request,
    list_message_projections_by_session, list_turn_events_by_request, list_turns_by_goal_cycle_id,
    ChatTurnEventRecord, MessageProjectionRecord,
};
pub use util::truncate_tool_result;
