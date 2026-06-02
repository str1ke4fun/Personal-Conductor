use super::{
    commands::{fallback_unknown_answer, rule_based_answer},
    db,
    prompt::build_system_prompt,
    tools::{build_tool_definitions, tool_id_from_llm_name},
    types::{ChatMessage, ChatReply, ChatRole, ChatTaskMode, ToolCallRecord},
    util::{generate_bubble_summary, plain_text_for_llm},
};
use crate::{
    affection,
    avatar::{self, ActivityVariant},
    chat_parser::parse,
    config::{self, CoreConfig},
    expression,
    llm::{self, LlmRequestConfig, OpenaiMessage},
    tasks::{Task, TaskStatus},
};
use anyhow::bail;

pub async fn send(content: String) -> anyhow::Result<ChatReply> {
    let content = content.trim().to_string();
    if content.is_empty() {
        bail!("消息不能为空");
    }

    // Ensure expression/affection DBs are initialized
    let _ = expression::init_db().await;
    let _ = affection::init_db().await;

    crate::tools::register_builtin_tools();
    let pool = crate::db::pool().await?;
    db::insert_message(&pool, ChatRole::User, content.clone(), None).await?;

    let config = config::load().await?;
    let snapshot = crate::tasks::load().await?;
    let recent_history = db::history(config.chat_history_limit)
        .await
        .unwrap_or_default();
    let (answer, tool_calls) =
        answer_message(&content, &snapshot.tasks, &config, &recent_history).await;
    let bubble = generate_bubble_summary(&answer, &[]);
    let assistant = db::insert_message(&pool, ChatRole::Assistant, answer, tool_calls).await?;
    let history = db::history(config.chat_history_limit).await?;

    Ok(ChatReply {
        message: assistant,
        history,
        bubble_summary: Some(bubble),
    })
}

async fn answer_message(
    prompt: &str,
    tasks: &[Task],
    config: &CoreConfig,
    recent_history: &[ChatMessage],
) -> (String, Option<Vec<ToolCallRecord>>) {
    match parse(prompt) {
        crate::chat_parser::ChatIntent::Unknown { .. } => {
            match llm_answer(prompt, tasks, config, recent_history).await {
                Ok((answer, records)) if !answer.trim().is_empty() => (answer, Some(records)),
                Ok(_) => (fallback_unknown_answer(tasks), None),
                Err(err) => {
                    let _ = avatar::set_activity_variant(ActivityVariant::Error).await;
                    // Update mood on LLM error
                    if let Ok(mut mood) = expression::load_mood().await {
                        mood.on_llm_error();
                        let _ = expression::save_mood(&mood).await;
                    }
                    (format!("思考遇到了点麻烦……{err}"), None)
                }
            }
        }
        _ => (rule_based_answer(prompt, tasks, config).await, None),
    }
}

async fn llm_answer(
    prompt: &str,
    tasks: &[Task],
    config: &CoreConfig,
    recent_history: &[ChatMessage],
) -> anyhow::Result<(String, Vec<ToolCallRecord>)> {
    let _ = avatar::set_activity_variant(ActivityVariant::Thinking).await;
    let mut all_records: Vec<ToolCallRecord> = Vec::new();

    // Load expression state
    let mut mood = expression::load_mood().await.unwrap_or_default();
    mood.on_user_message();
    let _affection_state = affection::load().await.unwrap_or_default();

    let request_config = LlmRequestConfig::from_config(&config.llm);
    let system = build_system_prompt(tasks, config, prompt, None).await;
    let tools = build_tool_definitions(config, prompt, ChatTaskMode::Short, false).await;

    let mut messages: Vec<OpenaiMessage> = vec![OpenaiMessage {
        role: "system".to_string(),
        content: Some(system),
        tool_calls: None,
        tool_call_id: None,
    }];
    let mut included_current_prompt = false;
    let mut recent = recent_history.iter().rev().take(12).collect::<Vec<_>>();
    recent.reverse();
    for message in recent {
        if message.role == ChatRole::User && message.content == prompt {
            included_current_prompt = true;
        }
        messages.push(OpenaiMessage {
            role: message.role.as_str().to_string(),
            content: Some(plain_text_for_llm(&message.content)),
            tool_calls: None,
            tool_call_id: None,
        });
    }
    if !included_current_prompt {
        messages.push(OpenaiMessage {
            role: "user".to_string(),
            content: Some(prompt.to_string()),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    let mut last_response: llm::LlmResponse = llm::call_with_tools_with_messages(
        &config.llm.model,
        &messages,
        &request_config,
        if tools.is_empty() { None } else { Some(tools) },
    )
    .await?;

    let mut rounds = 0;
    while rounds < 3 {
        let Some(tool_calls) = last_response.tool_calls.take() else {
            break;
        };
        if tool_calls.is_empty() {
            break;
        }

        messages.push(OpenaiMessage {
            role: "assistant".to_string(),
            content: last_response.content.clone(),
            tool_calls: Some(tool_calls.clone()),
            tool_call_id: None,
        });

        for tc in &tool_calls {
            let _ = avatar::set_activity_variant(ActivityVariant::ToolCalling).await;

            let args: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));
            let tool_id = tool_id_from_llm_name(&tc.function.name);
            let result =
                crate::tools::execute_tool_with_workspace_async(&tool_id, &args, None).await;
            let (result, is_success, _duration_ms) = match result {
                Ok(result) => (
                    serde_json::to_string(&result.output).unwrap_or_else(|_| "{}".to_string()),
                    result.success,
                    result.duration_ms,
                ),
                Err(err) => (
                    serde_json::json!({ "error": err.to_string() }).to_string(),
                    false,
                    0,
                ),
            };

            // Update mood based on tool result
            if is_success {
                mood.on_tool_success();
            } else {
                mood.on_tool_failure();
            }

            // Record tool call for frontend display
            all_records.push(ToolCallRecord {
                tool_name: tc.function.name.clone(),
                arguments: tc.function.arguments.clone(),
                result: result.clone(),
                success: is_success,
            });

            // Inject raw JSON for LLM continuation; pet-formatted text is UI-only
            messages.push(OpenaiMessage {
                role: "tool".to_string(),
                content: Some(result.clone()),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
            });
        }

        let _ = avatar::set_activity_variant(ActivityVariant::Thinking).await;
        let tools_ref = build_tool_definitions(config, prompt, ChatTaskMode::Short, false).await;
        last_response = llm::call_with_tools_with_messages(
            &config.llm.model,
            &messages,
            &request_config,
            if tools_ref.is_empty() {
                None
            } else {
                Some(tools_ref)
            },
        )
        .await?;

        rounds += 1;
    }

    // Update mood on LLM success
    mood.on_llm_success();
    let _ = expression::save_mood(&mood).await;

    // Record affection interaction
    let _ = affection::record_interaction(affection::InteractionType::Chat).await;

    // Update avatar activity
    let has_pending = tasks.iter().any(|t| t.status == TaskStatus::Pending);
    if has_pending {
        let _ = avatar::set_activity_variant(ActivityVariant::WaitingUser).await;
    } else {
        let _ = avatar::set_activity_variant(ActivityVariant::Done).await;
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let _ = avatar::set_activity_variant(ActivityVariant::Idle).await;
        });
    }

    Ok((last_response.content.unwrap_or_default(), all_records))
}
