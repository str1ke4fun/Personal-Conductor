// UserPresence — minimal state machine for user availability.
//
// Presence state affects whether the main-thread LLM continuation should fire
// immediately or be deferred (A0-2 / TC-P0-07 DND guard).

use std::sync::OnceLock;
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserPresence {
    /// App not running / network unreachable.
    Offline,
    /// User is actively interacting.
    Active,
    /// No input for a short period (< 5 min).
    Idle,
    /// No input for a longer period (5–30 min).
    Away,
    /// User has explicitly triggered sleep mode.
    Asleep,
    /// Do-not-disturb — suppress LLM continuations.
    Dnd,
}

impl UserPresence {
    /// Returns true when the LLM should NOT fire a proactive continuation.
    pub fn blocks_llm_continuation(&self) -> bool {
        matches!(self, Self::Offline | Self::Asleep | Self::Dnd)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Active => "active",
            Self::Idle => "idle",
            Self::Away => "away",
            Self::Asleep => "asleep",
            Self::Dnd => "dnd",
        }
    }
}

impl Default for UserPresence {
    fn default() -> Self {
        Self::Active
    }
}

// ── Global presence state ─────────────────────────────────────────────────────

static PRESENCE: OnceLock<RwLock<UserPresence>> = OnceLock::new();

fn presence_lock() -> &'static RwLock<UserPresence> {
    PRESENCE.get_or_init(|| RwLock::new(UserPresence::Active))
}

/// Read the current presence state.
pub async fn resolve_presence() -> UserPresence {
    presence_lock().read().await.clone()
}

/// Update the presence state. Returns the previous state.
pub async fn set_presence(new_state: UserPresence) -> UserPresence {
    let mut guard = presence_lock().write().await;
    let prev = guard.clone();
    *guard = new_state.clone();
    crate::events::emit_presence_changed(prev.as_str(), new_state.as_str()).await;
    prev
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn blocks_continuation_for_dnd_asleep_offline() {
        assert!(UserPresence::Dnd.blocks_llm_continuation());
        assert!(UserPresence::Asleep.blocks_llm_continuation());
        assert!(UserPresence::Offline.blocks_llm_continuation());
        assert!(!UserPresence::Active.blocks_llm_continuation());
        assert!(!UserPresence::Idle.blocks_llm_continuation());
        assert!(!UserPresence::Away.blocks_llm_continuation());
    }

    #[tokio::test]
    async fn set_and_resolve_round_trip() {
        set_presence(UserPresence::Idle).await;
        assert_eq!(resolve_presence().await, UserPresence::Idle);
        set_presence(UserPresence::Active).await;
    }
}
