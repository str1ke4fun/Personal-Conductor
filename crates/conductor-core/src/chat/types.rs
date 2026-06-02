use anyhow::bail;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChatTaskMode {
    #[default]
    Short,
    Long,
}

impl ChatTaskMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::Long => "long",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChatCapability {
    ReadOnly,
    #[default]
    AskWrite,
    Trusted,
}

impl ChatCapability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::AskWrite => "ask_write",
            Self::Trusted => "trusted",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GoalSeed {
    pub title: String,
    pub objective: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CapabilityRequest {
    pub reason: String,
    pub suggested_mode: String,
    pub goal_seed: GoalSeed,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlanStep {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CompletionStep {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub status: String,
}

/// Content block variants aligned with the Anthropic API content-blocks format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
    #[serde(rename = "capability_request")]
    CapabilityRequest {
        #[serde(flatten)]
        request: CapabilityRequest,
    },
    #[serde(rename = "plan")]
    Plan {
        title: String,
        steps: Vec<PlanStep>,
        status: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        write_scope: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        diff_preview: Option<String>,
    },
    #[serde(rename = "completion")]
    Completion {
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        steps: Vec<CompletionStep>,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    #[serde(rename = "blocked")]
    Blocked {
        title: String,
        reason: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        action_items: Vec<String>,
    },
    #[serde(rename = "runtime_projection")]
    RuntimeProjection { request_id: String, label: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
}

impl ChatRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }

    pub(super) fn from_db(value: &str) -> anyhow::Result<Self> {
        match value {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            other => bail!("unknown chat role: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub arguments: String,
    pub result: String,
    pub success: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatMessage {
    pub id: String,
    pub role: ChatRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub seq: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallRecord>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatReply {
    pub message: ChatMessage,
    pub history: Vec<ChatMessage>,
    /// Short 1-2 sentence summary for the pet bubble UI.
    /// Avoids showing raw JSON content blocks or overly long text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bubble_summary: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StreamChatTokenEvent {
    pub session_id: Option<String>,
    pub request_id: String,
    pub token: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ThinkingUpdateEvent {
    pub session_id: Option<String>,
    pub request_id: String,
    pub phase: String,
    pub message: String,
    pub turn: i32,
    pub timestamp: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolExecutionUpdateEvent {
    pub session_id: Option<String>,
    pub request_id: String,
    pub tool_use_id: String,
    pub tool_name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// Content-blocks variant of a chat message, aligned with the Anthropic API format.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatMessageV2 {
    pub id: String,
    pub role: ChatRole,
    pub content_blocks: Vec<ContentBlock>,
    pub created_at: DateTime<Utc>,
    pub seq: i64,
}

impl ChatMessage {
    /// Convert a legacy message to the V2 content-blocks format.
    ///
    /// If `content` is a JSON-encoded `Vec<ContentBlock>` it will be deserialized
    /// into proper blocks; otherwise the entire string is wrapped in a single
    /// [`ContentBlock::Text`].
    pub fn to_v2(&self) -> ChatMessageV2 {
        let content_blocks: Vec<ContentBlock> =
            serde_json::from_str(&self.content).unwrap_or_else(|_| {
                vec![ContentBlock::Text {
                    text: self.content.clone(),
                }]
            });

        ChatMessageV2 {
            id: self.id.clone(),
            role: self.role.clone(),
            content_blocks,
            created_at: self.created_at,
            seq: self.seq,
        }
    }
}

impl ChatMessageV2 {
    /// Convert back to the legacy format for storage.
    ///
    /// Content blocks are JSON-serialized into the `content` string field.
    pub fn to_legacy(&self) -> ChatMessage {
        let content =
            serde_json::to_string(&self.content_blocks).unwrap_or_else(|_| "[]".to_string());

        ChatMessage {
            id: self.id.clone(),
            role: self.role.clone(),
            content,
            created_at: self.created_at,
            seq: self.seq,
            tool_calls: None,
        }
    }
}
