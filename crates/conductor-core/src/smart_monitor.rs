use crate::{
    affection, chat, config::CoreConfig, expression, llm::LlmRequestConfig, pacer::PacerAlert,
    tasks,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartDecision {
    pub notify: bool,
    pub message: Option<String>,
    pub urgency: Urgency,
    pub pet_state: PetState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PetState {
    Idle,
    Working,
    Update,
    Quiet,
    NewTask,
}

impl PetState {
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Update => "update",
            Self::Quiet => "quiet",
            Self::NewTask => "new_task",
        }
    }
}

impl Default for SmartDecision {
    fn default() -> Self {
        Self {
            notify: false,
            message: None,
            urgency: Urgency::Low,
            pet_state: PetState::Idle,
        }
    }
}

/// Build a context summary for the LLM judge.
async fn gather_context(alert: &PacerAlert) -> String {
    let mut ctx = String::new();

    // Alert info
    ctx.push_str(&format!("## 告警类型\n{}\n\n", format_alert(alert)));

    // Task summary
    match tasks::load().await {
        Ok(file) => {
            let stats = tasks::activity_stats(&file.tasks);
            ctx.push_str(&format!(
                "## 任务状态\n运行中 Claude/Codex 会话: {}，待审 Hook 输出: {}，其他待办: {}\n",
                stats.active_hook_sessions, stats.pending_hook_reviews, stats.pending_other
            ));
            let recent_tasks: Vec<_> = file
                .tasks
                .iter()
                .filter(|t| {
                    t.status == tasks::TaskStatus::Pending
                        || t.status == tasks::TaskStatus::InProgress
                })
                .take(5)
                .collect();
            if !recent_tasks.is_empty() {
                ctx.push_str("最近任务:\n");
                for t in &recent_tasks {
                    ctx.push_str(&format!(
                        "- [{}] {} {}\n",
                        t.id,
                        format!("{:?}", t.status).to_lowercase(),
                        t.kind
                    ));
                }
            }
        }
        Err(_) => {
            ctx.push_str("## 任务状态\n无法加载任务列表\n");
        }
    }

    // Mood state
    match expression::load_mood().await {
        Ok(mood) => {
            let zone = mood.zone();
            ctx.push_str(&format!(
                "\n## 心情状态\n心情区域: {}（{}），效价: {:.2}，唤醒度: {:.2}\n",
                zone.as_str(),
                zone.label_zh(),
                mood.valence,
                mood.arousal
            ));
        }
        Err(_) => {}
    }

    // Affection state
    match affection::load().await {
        Ok(state) => {
            ctx.push_str(&format!(
                "\n## 关系状态\n阶段: {}（好感度 {}/100），连续登录 {} 天\n",
                state.stage.label_zh(),
                state.value,
                state.consecutive_days
            ));
        }
        Err(_) => {}
    }

    // Recent chat
    match chat::history(5).await {
        Ok(messages) => {
            if !messages.is_empty() {
                ctx.push_str("\n## 最近对话\n");
                for msg in &messages {
                    let preview: String = msg.content.chars().take(50).collect();
                    ctx.push_str(&format!("- {}: {}\n", msg.role.as_str(), preview));
                }
            }
        }
        Err(_) => {}
    }

    ctx
}

fn format_alert(alert: &PacerAlert) -> String {
    match alert {
        PacerAlert::PendingPileUp {
            count,
            oldest_minutes,
        } => {
            format!(
                "任务堆积：{} 个待处理任务，最老的任务已等待 {} 分钟",
                count, oldest_minutes
            )
        }
        PacerAlert::AgentStalled {
            task_id,
            stalled_minutes,
        } => {
            format!(
                "任务卡住：任务 {} 已进行 {} 分钟无进展",
                task_id, stalled_minutes
            )
        }
        PacerAlert::UserBackFromIdle {
            idle_minutes,
            pending,
        } => {
            format!(
                "用户回来：用户空闲了 {} 分钟后回来，有 {} 个待处理任务",
                idle_minutes, pending
            )
        }
        PacerAlert::NoActivityHour { hour } => {
            format!("整点提醒：当前 {} 点", hour)
        }
    }
}

/// Call LLM to decide what action to take for a given alert.
/// Returns None if LLM is unavailable or returns unparseable output.
async fn llm_decide(alert: &PacerAlert, config: &CoreConfig) -> Option<SmartDecision> {
    let context = gather_context(alert).await;

    let system_prompt = r#"你是桌宠的智能监控决策器。根据告警信息和当前上下文，决定是否需要通知用户。

返回 JSON 格式：
{
  "notify": true/false,
  "message": "通知内容（notify=true时必填，简短自然的中文，不超过30字）",
  "urgency": "low/medium/high",
  "pet_state": "idle/working/update/quiet/new_task"
}

决策原则：
- 如果用户正在专注工作（mood.arousal高、有进行中任务），减少打扰
- 如果是工作时间的整点提醒，只在有重要待办时才通知
- 任务堆积严重时提高urgency
- 任务卡住超过1小时建议高urgency
- 消息要自然，像朋友提醒而非系统通知
- 如果判断不需要通知，notify设为false，message可为null

只返回 JSON，不要有其他文字。"#;

    let request_config = LlmRequestConfig::from_config(&config.llm);
    let user_prompt = format!("当前上下文：\n{}", context);

    let response = crate::llm::call(
        &config.llm.model,
        system_prompt,
        &user_prompt,
        &request_config,
    )
    .await
    .ok()?;

    // Parse JSON from response
    let trimmed = response.trim();
    // Try to extract JSON from possible markdown code block
    let json_str = if trimmed.starts_with('{') {
        trimmed.to_string()
    } else if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            trimmed[start..=end].to_string()
        } else {
            return None;
        }
    } else {
        return None;
    };

    serde_json::from_str::<SmartDecision>(&json_str).ok()
}

/// Main entry point: evaluate an alert using LLM intelligence.
/// Falls back to a sensible default if LLM is unavailable.
pub async fn evaluate(alert: &PacerAlert, config: &CoreConfig) -> SmartDecision {
    if let Some(decision) = llm_decide(alert, config).await {
        return decision;
    }

    // Fallback: hardcoded defaults (matching current NotifyDecider behavior)
    fallback_decision(alert)
}

fn fallback_decision(alert: &PacerAlert) -> SmartDecision {
    match alert {
        PacerAlert::PendingPileUp { count, .. } => {
            if *count > 0 {
                SmartDecision {
                    notify: false,
                    message: None,
                    urgency: Urgency::Low,
                    pet_state: PetState::Working,
                }
            } else {
                SmartDecision {
                    notify: false,
                    message: None,
                    urgency: Urgency::Low,
                    pet_state: PetState::Idle,
                }
            }
        }
        PacerAlert::AgentStalled {
            task_id,
            stalled_minutes,
        } => SmartDecision {
            notify: *stalled_minutes > 60,
            message: Some(format!("任务 {} 好像卡住了，要帮忙看看吗？", task_id)),
            urgency: if *stalled_minutes > 60 {
                Urgency::Medium
            } else {
                Urgency::Low
            },
            pet_state: PetState::Working,
        },
        PacerAlert::UserBackFromIdle { pending, .. } => {
            if *pending > 0 {
                SmartDecision {
                    notify: true,
                    message: Some(format!("回来了，还有 {} 个任务等着你", pending)),
                    urgency: Urgency::Low,
                    pet_state: PetState::Update,
                }
            } else {
                SmartDecision {
                    notify: false,
                    message: None,
                    urgency: Urgency::Low,
                    pet_state: PetState::Idle,
                }
            }
        }
        PacerAlert::NoActivityHour { .. } => SmartDecision {
            notify: false,
            message: None,
            urgency: Urgency::Low,
            pet_state: PetState::Idle,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_pending_pileup() {
        let alert = PacerAlert::PendingPileUp {
            count: 3,
            oldest_minutes: 45,
        };
        let decision = fallback_decision(&alert);
        assert_eq!(decision.pet_state.to_str(), "working");
        assert!(!decision.notify);
    }

    #[test]
    fn test_fallback_agent_stalled_long() {
        let alert = PacerAlert::AgentStalled {
            task_id: "t-001".to_string(),
            stalled_minutes: 90,
        };
        let decision = fallback_decision(&alert);
        assert!(decision.notify);
        assert_eq!(decision.urgency, Urgency::Medium);
    }

    #[test]
    fn test_fallback_agent_stalled_short() {
        let alert = PacerAlert::AgentStalled {
            task_id: "t-001".to_string(),
            stalled_minutes: 20,
        };
        let decision = fallback_decision(&alert);
        assert!(!decision.notify);
        assert_eq!(decision.urgency, Urgency::Low);
    }

    #[test]
    fn test_fallback_user_back_with_pending() {
        let alert = PacerAlert::UserBackFromIdle {
            idle_minutes: 10,
            pending: 3,
        };
        let decision = fallback_decision(&alert);
        assert!(decision.notify);
        assert!(decision.message.is_some());
    }

    #[test]
    fn test_fallback_user_back_no_pending() {
        let alert = PacerAlert::UserBackFromIdle {
            idle_minutes: 10,
            pending: 0,
        };
        let decision = fallback_decision(&alert);
        assert!(!decision.notify);
    }

    #[test]
    fn test_fallback_no_activity_hour() {
        let alert = PacerAlert::NoActivityHour { hour: 14 };
        let decision = fallback_decision(&alert);
        assert!(!decision.notify);
        assert_eq!(decision.pet_state.to_str(), "idle");
    }

    #[test]
    fn test_smart_decision_json_roundtrip() {
        let decision = SmartDecision {
            notify: true,
            message: Some("测试消息".to_string()),
            urgency: Urgency::Medium,
            pet_state: PetState::Working,
        };
        let json = serde_json::to_string(&decision).unwrap();
        let parsed: SmartDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.notify, true);
        assert_eq!(parsed.message.unwrap(), "测试消息");
        assert_eq!(parsed.urgency, Urgency::Medium);
    }

    #[test]
    fn test_parse_json_from_markdown_block() {
        let response = r#"```json
{"notify": true, "message": "你好", "urgency": "low", "pet_state": "idle"}
```"#;
        let trimmed = response.trim();
        let json_str = if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                &trimmed[start..=end]
            } else {
                panic!("no closing brace");
            }
        } else {
            panic!("no opening brace");
        };
        let parsed: SmartDecision = serde_json::from_str(json_str).unwrap();
        assert!(parsed.notify);
    }
}
