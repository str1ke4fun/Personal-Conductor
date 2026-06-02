use serde_json;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Activity level based on recent user activity count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActivityLevel {
    Low,
    Normal,
    High,
}

/// Count recent activities within a time window.
fn count_recent_activities(context: &InitiativeContext, window_secs: u64) -> usize {
    let cutoff = Instant::now() - Duration::from_secs(window_secs);
    context
        .user_activity
        .iter()
        .filter(|a| a.timestamp > cutoff)
        .count()
}

/// Classify activity level: 0-2 = Low, 3-9 = Normal, 10+ = High.
fn compute_activity_level(context: &InitiativeContext, window_secs: u64) -> ActivityLevel {
    let count = count_recent_activities(context, window_secs);
    if count >= 10 {
        ActivityLevel::High
    } else if count >= 3 {
        ActivityLevel::Normal
    } else {
        ActivityLevel::Low
    }
}

/// Get activity-aware message prefix.
fn activity_prefix(level: &ActivityLevel) -> &'static str {
    match level {
        ActivityLevel::Low => "好久没动静了，",
        ActivityLevel::Normal => "",
        ActivityLevel::High => "看你挺忙的，",
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InitiativeTrigger {
    DocumentWriting,
    Coding,
    MultiAgentBacklog,
    TimeBased,
    EventBased,
}

#[derive(Clone, Debug)]
pub struct InitiativeContext {
    pub workspace_id: Option<String>,
    pub active_tool: Option<String>,
    pub user_activity: Vec<ActivityRecord>,
    pub last_interaction_time: Instant,
    pub current_task: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ActivityRecord {
    pub timestamp: Instant,
    pub activity_type: String,
    pub details: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct InitiativeProposal {
    pub id: String,
    pub trigger: InitiativeTrigger,
    pub confidence: f64,
    pub suggested_tools: Vec<String>,
    pub message: String,
    pub context: InitiativeContext,
    pub created_at: Instant,
}

pub struct InitiativeEngine {
    triggers: HashMap<InitiativeTrigger, TriggerHandler>,
    context: RwLock<InitiativeContext>,
    last_check: RwLock<Instant>,
}

type TriggerHandler = fn(&InitiativeContext) -> Option<InitiativeProposal>;

impl InitiativeEngine {
    pub fn new() -> Self {
        let mut triggers: HashMap<InitiativeTrigger, TriggerHandler> = HashMap::new();
        triggers.insert(
            InitiativeTrigger::DocumentWriting,
            handle_document_writing as TriggerHandler,
        );
        triggers.insert(InitiativeTrigger::Coding, handle_coding as TriggerHandler);
        triggers.insert(
            InitiativeTrigger::MultiAgentBacklog,
            handle_multi_agent_backlog as TriggerHandler,
        );
        triggers.insert(
            InitiativeTrigger::TimeBased,
            handle_time_based as TriggerHandler,
        );
        triggers.insert(
            InitiativeTrigger::EventBased,
            handle_event_based as TriggerHandler,
        );

        Self {
            triggers,
            context: RwLock::new(InitiativeContext {
                workspace_id: None,
                active_tool: None,
                user_activity: Vec::new(),
                last_interaction_time: Instant::now(),
                current_task: None,
            }),
            last_check: RwLock::new(Instant::now()),
        }
    }

    pub fn update_context(&self, updates: PartialContext) {
        let mut context = self.context.write().unwrap();

        if let Some(workspace_id) = updates.workspace_id {
            context.workspace_id = Some(workspace_id);
        }
        if let Some(active_tool) = updates.active_tool {
            context.active_tool = Some(active_tool);
        }
        if let Some(activity) = updates.activity {
            context.user_activity.push(activity);
            if context.user_activity.len() > 100 {
                context.user_activity.remove(0);
            }
        }
        if updates.touch {
            context.last_interaction_time = Instant::now();
        }
        if let Some(task) = updates.current_task {
            context.current_task = Some(task);
        }
    }

    pub fn check_initiatives(&self) -> Vec<InitiativeProposal> {
        let now = Instant::now();
        let mut last_check = self.last_check.write().unwrap();

        if now.duration_since(*last_check) < Duration::from_secs(30) {
            return Vec::new();
        }
        *last_check = now;

        let context = self.context.read().unwrap();
        let mut proposals = Vec::new();

        for (trigger, handler) in &self.triggers {
            if let Some(proposal) = handler(&context) {
                proposals.push(proposal);
            }
        }

        proposals.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        proposals.truncate(5);

        proposals
    }
}

pub struct PartialContext {
    pub workspace_id: Option<String>,
    pub active_tool: Option<String>,
    pub activity: Option<ActivityRecord>,
    pub touch: bool,
    pub current_task: Option<String>,
}

fn handle_document_writing(context: &InitiativeContext) -> Option<InitiativeProposal> {
    let recent_writing = context
        .user_activity
        .iter()
        .filter(|a| {
            a.activity_type == "document_edit"
                && a.timestamp > Instant::now() - Duration::from_secs(5 * 60)
        })
        .count();

    if recent_writing >= 3 {
        let level = compute_activity_level(context, 5 * 60);
        let prefix = activity_prefix(&level);
        Some(InitiativeProposal {
            id: format!("doc-writing-{}", uuid::Uuid::new_v4()),
            trigger: InitiativeTrigger::DocumentWriting,
            confidence: 0.85,
            suggested_tools: vec![
                "office.inspect_document".to_string(),
                "office.export_text".to_string(),
                "memory.get".to_string(),
            ],
            message: format!("{}文档写得怎么样了？需要帮忙检查结构或整理内容吗？", prefix),
            context: context.clone(),
            created_at: Instant::now(),
        })
    } else {
        None
    }
}

fn handle_coding(context: &InitiativeContext) -> Option<InitiativeProposal> {
    let recent_coding = context
        .user_activity
        .iter()
        .filter(|a| {
            a.activity_type == "code_edit"
                && a.timestamp > Instant::now() - Duration::from_secs(5 * 60)
        })
        .count();

    if recent_coding >= 5 {
        let level = compute_activity_level(context, 5 * 60);
        let prefix = activity_prefix(&level);
        Some(InitiativeProposal {
            id: format!("coding-{}", uuid::Uuid::new_v4()),
            trigger: InitiativeTrigger::Coding,
            confidence: 0.75,
            suggested_tools: vec!["memory.get".to_string()],
            message: format!("{}代码写得顺利吗？需要帮忙查资料或看报错吗？", prefix),
            context: context.clone(),
            created_at: Instant::now(),
        })
    } else {
        None
    }
}

fn handle_multi_agent_backlog(context: &InitiativeContext) -> Option<InitiativeProposal> {
    let pending_proposals = get_pending_proposals_count();

    if pending_proposals >= 3 {
        Some(InitiativeProposal {
            id: format!("backlog-{}", uuid::Uuid::new_v4()),
            trigger: InitiativeTrigger::MultiAgentBacklog,
            confidence: 0.9,
            suggested_tools: vec!["tasks.list".to_string()],
            message: format!("还有 {} 个任务待处理，要看看吗？", pending_proposals),
            context: context.clone(),
            created_at: Instant::now(),
        })
    } else {
        None
    }
}

fn handle_time_based(context: &InitiativeContext) -> Option<InitiativeProposal> {
    let idle_time = Instant::now().duration_since(context.last_interaction_time);

    if idle_time >= Duration::from_secs(10 * 60) {
        let level = compute_activity_level(context, 30 * 60);
        let prefix = activity_prefix(&level);
        Some(InitiativeProposal {
            id: format!("time-based-{}", uuid::Uuid::new_v4()),
            trigger: InitiativeTrigger::TimeBased,
            confidence: 0.6,
            suggested_tools: vec!["tasks.list".to_string(), "events.recent".to_string()],
            message: format!("{}休息好了吗？要不要看看待办？", prefix),
            context: context.clone(),
            created_at: Instant::now(),
        })
    } else {
        None
    }
}

fn handle_event_based(context: &InitiativeContext) -> Option<InitiativeProposal> {
    // Fire when there are multiple recent tool failures or error-class activities
    let recent_errors = context
        .user_activity
        .iter()
        .filter(|a| {
            (a.activity_type == "tool_error" || a.activity_type == "error")
                && a.timestamp > Instant::now() - Duration::from_secs(10 * 60)
        })
        .count();

    if recent_errors >= 3 {
        Some(InitiativeProposal {
            id: format!("event-based-{}", uuid::Uuid::new_v4()),
            trigger: InitiativeTrigger::EventBased,
            confidence: 0.8,
            suggested_tools: vec!["memory.get".to_string()],
            message: format!("最近出了 {} 次错，要帮忙看看吗？", recent_errors),
            context: context.clone(),
            created_at: Instant::now(),
        })
    } else {
        None
    }
}

fn get_pending_proposals_count() -> usize {
    // Try to get pending proposals count from the proposals module
    // Since this is called from sync context, use tokio runtime handle if available
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        match handle.block_on(crate::proposals::list_pending()) {
            Ok(proposals) => return proposals.len(),
            Err(_) => return 0,
        }
    }
    0
}

lazy_static::lazy_static! {
    static ref INITIATIVE_ENGINE: RwLock<InitiativeEngine> = RwLock::new(InitiativeEngine::new());
}

pub fn update_initiative_context(updates: PartialContext) {
    let engine = INITIATIVE_ENGINE.read().unwrap();
    engine.update_context(updates);
}

pub fn check_for_initiatives() -> Vec<InitiativeProposal> {
    let engine = INITIATIVE_ENGINE.read().unwrap();
    engine.check_initiatives()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_writing_trigger() {
        let context = InitiativeContext {
            workspace_id: Some("test-workspace".to_string()),
            active_tool: None,
            user_activity: vec![
                ActivityRecord {
                    timestamp: Instant::now() - Duration::from_secs(60),
                    activity_type: "document_edit".to_string(),
                    details: serde_json::json!({ "path": "test.docx" }),
                },
                ActivityRecord {
                    timestamp: Instant::now() - Duration::from_secs(120),
                    activity_type: "document_edit".to_string(),
                    details: serde_json::json!({ "path": "test.docx" }),
                },
                ActivityRecord {
                    timestamp: Instant::now() - Duration::from_secs(180),
                    activity_type: "document_edit".to_string(),
                    details: serde_json::json!({ "path": "test.docx" }),
                },
            ],
            last_interaction_time: Instant::now(),
            current_task: None,
        };

        let proposal = handle_document_writing(&context);
        assert!(proposal.is_some());
        assert_eq!(proposal.unwrap().confidence, 0.85);
    }

    #[test]
    fn test_coding_trigger() {
        let context = InitiativeContext {
            workspace_id: Some("test-workspace".to_string()),
            active_tool: None,
            user_activity: vec![
                ActivityRecord {
                    timestamp: Instant::now() - Duration::from_secs(30),
                    activity_type: "code_edit".to_string(),
                    details: serde_json::json!({ "path": "main.rs" }),
                };
                5
            ],
            last_interaction_time: Instant::now(),
            current_task: None,
        };

        let proposal = handle_coding(&context);
        assert!(proposal.is_some());
        assert_eq!(proposal.unwrap().confidence, 0.75);
    }

    #[test]
    fn test_time_based_trigger() {
        // Use checked_sub to avoid panic if system uptime < 15 minutes
        let last_interaction = Instant::now()
            .checked_sub(Duration::from_secs(15 * 60))
            .unwrap_or_else(Instant::now);
        let context = InitiativeContext {
            workspace_id: None,
            active_tool: None,
            user_activity: Vec::new(),
            last_interaction_time: last_interaction,
            current_task: None,
        };

        let proposal = handle_time_based(&context);
        assert!(proposal.is_some());
        assert_eq!(proposal.unwrap().confidence, 0.6);
    }

    // ── Activity level tests (TASK-031) ──────────────────────────────────────

    #[test]
    fn test_activity_level_low_with_no_activities() {
        let context = InitiativeContext {
            workspace_id: None,
            active_tool: None,
            user_activity: Vec::new(),
            last_interaction_time: Instant::now(),
            current_task: None,
        };
        assert_eq!(compute_activity_level(&context, 300), ActivityLevel::Low);
    }

    #[test]
    fn test_activity_level_low_with_few_activities() {
        let context = InitiativeContext {
            workspace_id: None,
            active_tool: None,
            user_activity: vec![
                ActivityRecord {
                    timestamp: Instant::now() - Duration::from_secs(30),
                    activity_type: "code_edit".to_string(),
                    details: serde_json::json!({}),
                };
                2
            ],
            last_interaction_time: Instant::now(),
            current_task: None,
        };
        assert_eq!(compute_activity_level(&context, 300), ActivityLevel::Low);
    }

    #[test]
    fn test_activity_level_normal() {
        let context = InitiativeContext {
            workspace_id: None,
            active_tool: None,
            user_activity: vec![
                ActivityRecord {
                    timestamp: Instant::now() - Duration::from_secs(30),
                    activity_type: "code_edit".to_string(),
                    details: serde_json::json!({}),
                };
                5
            ],
            last_interaction_time: Instant::now(),
            current_task: None,
        };
        assert_eq!(compute_activity_level(&context, 300), ActivityLevel::Normal);
    }

    #[test]
    fn test_activity_level_high() {
        let context = InitiativeContext {
            workspace_id: None,
            active_tool: None,
            user_activity: vec![
                ActivityRecord {
                    timestamp: Instant::now() - Duration::from_secs(30),
                    activity_type: "tool_call".to_string(),
                    details: serde_json::json!({}),
                };
                15
            ],
            last_interaction_time: Instant::now(),
            current_task: None,
        };
        assert_eq!(compute_activity_level(&context, 300), ActivityLevel::High);
    }

    #[test]
    fn test_activity_context_messages() {
        assert_eq!(activity_prefix(&ActivityLevel::Low), "好久没动静了，");
        assert_eq!(activity_prefix(&ActivityLevel::Normal), "");
        assert_eq!(activity_prefix(&ActivityLevel::High), "看你挺忙的，");
    }

    #[test]
    fn test_time_based_trigger_adapts_to_activity_level() {
        // High activity + idle → message includes "看你挺忙的"
        let last_interaction = Instant::now()
            .checked_sub(Duration::from_secs(15 * 60))
            .unwrap_or_else(Instant::now);
        let context = InitiativeContext {
            workspace_id: None,
            active_tool: None,
            user_activity: vec![
                ActivityRecord {
                    timestamp: Instant::now() - Duration::from_secs(60),
                    activity_type: "tool_call".to_string(),
                    details: serde_json::json!({}),
                };
                12
            ],
            last_interaction_time: last_interaction,
            current_task: None,
        };
        let proposal = handle_time_based(&context).unwrap();
        assert!(proposal.message.starts_with("看你挺忙的"));
    }
}
