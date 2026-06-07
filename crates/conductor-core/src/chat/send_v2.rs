use super::{
    active_run, db,
    prompt::build_system_prompt_with_context,
    session::{auto_title_session, resolve_session_workspace},
    tools::{
        build_tool_definitions, build_tool_definitions_for_catalog_selection,
        build_tool_definitions_with_allowed_ids, execute_tool_call,
        should_use_progressive_tool_discovery, tool_id_from_llm_name, tool_name_for_llm,
    },
    turns,
    types::{
        CapabilityRequest, ChatCapability, ChatMessage, ChatReply, ChatRole, ChatTaskMode,
        ContentBlock, GoalSeed, StreamChatTokenEvent, ThinkingUpdateEvent, ToolCallRecord,
        ToolExecutionUpdateEvent,
    },
    util::{
        append_chat_stage, build_content_blocks_for_db, chat_elapsed_ms, generate_bubble_summary,
        plain_text_for_llm, spawn_chat_stage, update_post_chat_avatar, user_visible_plain_text,
    },
};
use crate::{
    affection,
    avatar::{self, ActivityVariant},
    config, expression,
    llm::{self, LlmRequestConfig, LlmStreamEvent, OpenaiMessage},
    summarizer, tasks,
    transcript::TranscriptMessage,
};
use anyhow::bail;
use chrono::Utc;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::Emitter;
use uuid::Uuid;

lazy_static::lazy_static! {
    /// Tracks sessions that have already been auto-summarized (idempotency guard).
    static ref SUMMARIZED_SESSIONS: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
}

const FINAL_ANSWER_MAX_TOKENS: u32 = 8_000;

/// Compute simple keyword overlap between two texts (Jaccard-like).
/// Returns a value in [0.0, 1.0]; lower means more different topics.
fn keywords_overlap(a: &str, b: &str) -> f64 {
    let kw_a: HashSet<&str> = a.split_whitespace().filter(|w| w.len() > 2).collect();
    let kw_b: HashSet<&str> = b.split_whitespace().filter(|w| w.len() > 2).collect();
    if kw_a.is_empty() || kw_b.is_empty() {
        return 0.0;
    }
    let intersection = kw_a.intersection(&kw_b).count();
    let union = kw_a.union(&kw_b).count();
    intersection as f64 / union as f64
}

fn should_offer_goal_upgrade(content: &str, task_mode: ChatTaskMode, plan_only: bool) -> bool {
    if plan_only {
        return false;
    }

    if matches!(task_mode, ChatTaskMode::Long) {
        return true;
    }

    let lower = content.to_ascii_lowercase();
    return [
        "goal",
        "long task",
        "long-task",
        "long running",
        "long-running",
        "continue until",
        "work until",
        "persistent",
        "持续推进",
        "长期目标",
        "长任务",
        "直到完成",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    /*
    [
        "goal",
        "long task",
        "long-task",
        "long running",
        "long-running",
        "continue until",
        "work until",
        "persistent",
        "持续推进",
        "长期目标",
        "长任务",
        "直到完成",
        "拆解",
        "派工",
        "长任务",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    */
}

fn extract_catalog_tool_ids(result: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(result) else {
        return vec![];
    };

    value
        .get("tools")
        .and_then(|tools| tools.as_array())
        .into_iter()
        .flatten()
        .filter_map(|tool| tool.get("id").and_then(|id| id.as_str()))
        .map(str::to_string)
        .collect()
}

fn build_goal_seed(content: &str, history: &[ChatMessage]) -> GoalSeed {
    let title_line = content.lines().next().unwrap_or(content).trim();
    let title = if title_line.is_empty() {
        "Long task from chat".to_string()
    } else {
        title_line.chars().take(72).collect()
    };

    let context_tail = history
        .iter()
        .rev()
        .take(4)
        .map(|message| {
            format!(
                "- {}: {}",
                message.role.as_str(),
                plain_text_for_llm(&message.content)
                    .chars()
                    .take(180)
                    .collect::<String>()
            )
        })
        .collect::<Vec<_>>();

    let mut objective = format!("User request:\n{}\n", content.trim());
    if !context_tail.is_empty() {
        objective.push_str("\nConversation context:\n");
        for line in context_tail.into_iter().rev() {
            objective.push_str(&line);
            objective.push('\n');
        }
    }

    GoalSeed { title, objective }
}

fn build_capability_request(
    content: &str,
    history: &[ChatMessage],
    task_mode: ChatTaskMode,
) -> CapabilityRequest {
    let reason = if matches!(task_mode, ChatTaskMode::Long) {
        "This request is marked as a long task and should move into Goal orchestration.".to_string()
    } else {
        "This request asks for background agents or multi-agent delegation, which is gated in short-task mode."
            .to_string()
    };

    CapabilityRequest {
        reason,
        suggested_mode: ChatTaskMode::Long.as_str().to_string(),
        goal_seed: build_goal_seed(content, history),
    }
}

const PLAN_ONLY_SYSTEM_INSTRUCTION: &str = concat!(
    "The user selected plan-only mode. ",
    "You may inspect context with read-only tools, but you must not write files, run mutating commands, or trigger side effects. ",
    "Return a concise execution plan with these sections in order: ",
    "'Plan', 'Write scope', and 'Diff preview'. ",
    "The diff preview must be illustrative only and must not claim that edits were already applied."
);

const TOOL_COMPLETION_SYSTEM_INSTRUCTION: &str = concat!(
    "If you use tools, you must still end with a direct user-facing answer. ",
    "Do not stop at tool traces alone. Summarize what you found, what changed, the concrete outputs or artifacts, and any remaining blockers. ",
    "When writing long reports or files, keep each tool call argument small: create or overwrite the file with file.write, then append additional chunks with file.append."
);

const LONG_TASK_SYSTEM_INSTRUCTION: &str = concat!(
    "You are in long-task/goal mode. Continue across multiple tool rounds when needed, ",
    "but only stop once you can provide a concrete, reviewable result or an explicit blocker. ",
    "When writing long reports or files, keep each tool call argument small: create or overwrite the file with file.write, then append additional chunks with file.append."
);

fn execution_mode_system_instruction(task_mode: ChatTaskMode) -> &'static str {
    match task_mode {
        ChatTaskMode::Short => TOOL_COMPLETION_SYSTEM_INSTRUCTION,
        ChatTaskMode::Long => LONG_TASK_SYSTEM_INSTRUCTION,
    }
}

fn overall_timeout_for_mode(task_mode: ChatTaskMode) -> Duration {
    match task_mode {
        ChatTaskMode::Short => Duration::from_secs(120),
        ChatTaskMode::Long => Duration::from_secs(30 * 60),
    }
}

fn max_turns_for_mode(task_mode: ChatTaskMode) -> usize {
    match task_mode {
        ChatTaskMode::Short => 10,
        ChatTaskMode::Long => 24,
    }
}

#[derive(Clone, Default)]
struct TimeoutRecoveryState {
    final_text: String,
    tool_records: Vec<ToolCallRecord>,
}

fn set_timeout_recovery_text(timeout_recovery: &Arc<Mutex<TimeoutRecoveryState>>, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    if let Ok(mut recovery) = timeout_recovery.lock() {
        recovery.final_text = text.to_string();
    }
}

fn push_timeout_recovery_tool_record(
    timeout_recovery: &Arc<Mutex<TimeoutRecoveryState>>,
    record: &ToolCallRecord,
) {
    if let Ok(mut recovery) = timeout_recovery.lock() {
        recovery.tool_records.push(record.clone());
    }
}

fn snapshot_timeout_recovery(
    timeout_recovery: &Arc<Mutex<TimeoutRecoveryState>>,
) -> TimeoutRecoveryState {
    timeout_recovery
        .lock()
        .map(|recovery| recovery.clone())
        .unwrap_or_default()
}

async fn persist_final_turn_artifacts(
    request_id: &str,
    assistant: &ChatMessage,
    projection_kind: &str,
    plain_text: &str,
    scope_kind: &str,
    scope_ref: Option<String>,
    path_prefix: Option<String>,
    tool_call_ids: &[String],
    tool_records: &[ToolCallRecord],
) -> anyhow::Result<()> {
    turns::attach_assistant_message_by_request(request_id, &assistant.id).await?;

    let projection = turns::create_message_projection(turns::MessageProjectionCreate {
        request_id: request_id.to_string(),
        message_id: Some(assistant.id.clone()),
        role: "assistant".to_string(),
        projection_kind: projection_kind.to_string(),
        status: "finalized".to_string(),
        visibility: "visible".to_string(),
        plain_text: Some(plain_text.to_string()),
        content_blocks_json: serde_json::to_value(assistant.to_v2().content_blocks.clone())?,
        source_event_id: None,
    })
    .await?;

    let summary = if plain_text.trim().is_empty() {
        "assistant final answer".to_string()
    } else {
        plain_text.chars().take(500).collect()
    };
    let source_tool_call_id = tool_call_ids.first().cloned();
    let assistant_message_id = assistant.id.clone();
    let projection_id = projection.id.clone();
    let evidence_json = serde_json::json!({
        "request_id": request_id,
        "assistant_message_id": assistant_message_id,
        "projection_id": projection_id,
        "tool_call_ids": tool_call_ids,
        "tool_records": tool_records.len(),
        "projection_kind": projection_kind,
    });
    let value_json = serde_json::json!({
        "summary": summary.clone(),
        "plain_text": plain_text,
        "tool_call_ids": tool_call_ids,
        "tool_records": tool_records,
    });

    // Only emit a memory candidate when there is substantive content to remember.
    if !plain_text.trim().is_empty() || !tool_call_ids.is_empty() {
        let _ = turns::create_memory_candidate(turns::MemoryCandidateCreate {
            request_id: request_id.to_string(),
            source_message_id: Some(assistant.id.clone()),
            source_projection_id: Some(projection.id.clone()),
            source_tool_call_id,
            memory_kind: "assistant_final_answer".to_string(),
            scope_kind: scope_kind.to_string(),
            scope_ref,
            path_prefix,
            key: format!("turn:{request_id}:assistant_final_answer"),
            value_json,
            summary,
            evidence_json,
            extractor_kind: "rule".to_string(),
            extractor_provider: None,
            extractor_model: None,
            confidence: 0.4,
            status: "proposed".to_string(),
            dedupe_key: format!("turn:{request_id}:assistant_final_answer:{projection_kind}"),
        })
        .await?;
    }

    Ok(())
}

fn truncate_chars_with_suffix(text: &str, max_chars: usize) -> String {
    const SUFFIX: &str = "...(truncated)";
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(SUFFIX.chars().count());
    let mut truncated: String = text.chars().take(keep).collect();
    truncated.push_str(SUFFIX);
    truncated
}

fn extract_reviewable_json_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        serde_json::Value::Array(items) => {
            let parts = items
                .iter()
                .filter_map(extract_reviewable_json_text)
                .take(3)
                .collect::<Vec<_>>();
            (!parts.is_empty()).then(|| parts.join("\n\n"))
        }
        serde_json::Value::Object(map) => {
            for key in [
                "summary", "text", "stdout", "message", "content", "result", "diff", "stderr",
            ] {
                if let Some(text) = map.get(key).and_then(extract_reviewable_json_text) {
                    return Some(text);
                }
            }
            None
        }
        _ => None,
    }
}

fn reviewable_result_text(result: &str) -> String {
    let trimmed = result.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(text) = extract_reviewable_json_text(&value) {
            return text;
        }
    }
    trimmed.to_string()
}

fn compact_result_excerpt(result: &str, max_chars: usize) -> String {
    let flattened = reviewable_result_text(result)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    truncate_chars_with_suffix(&flattened, max_chars)
}

fn compact_argument_excerpt(arguments: &str, max_chars: usize) -> String {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let normalized = serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .and_then(|value| serde_json::to_string(&value).ok())
        .unwrap_or_else(|| trimmed.to_string());
    truncate_chars_with_suffix(&normalized, max_chars)
}

fn build_final_answer_recovery_prompt(
    user_request: &str,
    previous_text: &str,
    tool_records: &[ToolCallRecord],
) -> String {
    let mut prompt = format!("User request:\n{}\n\n", user_request.trim());
    let previous = previous_text.trim();
    if !previous.is_empty() {
        prompt.push_str("Last visible draft (may be incomplete):\n");
        prompt.push_str(previous);
        prompt.push_str("\n\n");
    }
    let success_count = tool_records.iter().filter(|record| record.success).count();
    let failed_count = tool_records.len().saturating_sub(success_count);
    let write_artifacts = tool_records
        .iter()
        .filter(|record| {
            record.success
                && matches!(
                    record.tool_name.as_str(),
                    "file.write" | "file.append" | "file.edit"
                )
        })
        .filter_map(|record| {
            serde_json::from_str::<serde_json::Value>(&record.result)
                .ok()
                .and_then(|value| {
                    value
                        .get("path")
                        .or_else(|| value.get("absolute_path"))
                        .and_then(|path| path.as_str())
                        .map(|path| path.to_string())
                })
        })
        .collect::<Vec<_>>();
    prompt.push_str(&format!(
        "Tool status summary: total {}, succeeded {}, failed {}.\n",
        tool_records.len(),
        success_count,
        failed_count
    ));
    if write_artifacts.is_empty() {
        prompt.push_str("Verified written artifacts: none.\n\n");
    } else {
        prompt.push_str("Verified written artifacts:\n");
        for artifact in write_artifacts {
            prompt.push_str(&format!("- {artifact}\n"));
        }
        prompt.push('\n');
    }
    prompt.push_str("Verified tool results:\n");
    let recent_records = tool_records.iter().rev().take(12).collect::<Vec<_>>();
    if recent_records.is_empty() {
        prompt.push_str("- No tool results were captured.\n");
    } else {
        let omitted_count = tool_records.len().saturating_sub(recent_records.len());
        for (display_index, record) in recent_records.into_iter().rev().enumerate() {
            prompt.push_str(&format!(
                "Tool {}: {} ({})\n",
                display_index + 1,
                record.tool_name,
                if record.success {
                    "succeeded"
                } else {
                    "failed"
                }
            ));
            let input = compact_argument_excerpt(&record.arguments, 220);
            if !input.is_empty() && input != "{}" {
                prompt.push_str(&format!("Input: {input}\n"));
            }
            let excerpt = truncate_chars_with_suffix(&reviewable_result_text(&record.result), 1200);
            if !excerpt.is_empty() {
                prompt.push_str("Result:\n");
                prompt.push_str(&excerpt);
                prompt.push_str("\n\n");
            } else if !record.success {
                prompt.push_str("Result:\n(tool failed without captured output)\n\n");
            }
        }
        if omitted_count > 0 {
            prompt.push_str(&format!(
                "Additional earlier tool results omitted: {omitted_count}\n"
            ));
        }
    }
    prompt.push_str(
        "\nWrite the final answer the user should see. Be concrete about findings, changes, artifacts, and blockers. Do not ask to call more tools. Do not claim files or artifacts were written unless they are listed under verified written artifacts.",
    );
    prompt
}

fn synthesize_reviewable_fallback(
    previous_text: &str,
    tool_records: &[ToolCallRecord],
    exhausted_tool_loop: bool,
) -> String {
    let previous = previous_text.trim();
    if tool_records.is_empty() {
        return if !previous.is_empty() {
            previous.to_string()
        } else if exhausted_tool_loop {
            "本轮执行在模型收尾前结束了，没有生成可审阅的最终结论。".to_string()
        } else {
            "本轮执行已完成，但模型没有返回可审阅的最终结论。".to_string()
        };
    }

    let success_count = tool_records.iter().filter(|record| record.success).count();
    let failed_count = tool_records.len().saturating_sub(success_count);
    let mut lines = vec![format!(
        "已完成 {} 个工具调用（成功 {}，失败 {}）。",
        tool_records.len(),
        success_count,
        failed_count
    )];
    lines.push(if exhausted_tool_loop {
        "模型在工具阶段结束前没有给出最终结论，下面保留最近结果供审阅。".to_string()
    } else {
        "模型没有返回最终结论，下面保留最近结果供审阅。".to_string()
    });

    if let Some(last_record) = tool_records
        .iter()
        .rev()
        .find(|record| !record.result.trim().is_empty())
    {
        lines.push(format!("最近工具：{}", last_record.tool_name));
        lines.push(format!(
            "结果摘录：{}",
            compact_result_excerpt(&last_record.result, 1200)
        ));
    } else if !previous.is_empty() {
        lines.push(format!("上一条可见说明：{}", previous));
    }

    lines.join("\n")
}

fn build_timeout_reviewable_message(
    recovery: &TimeoutRecoveryState,
    overall_timeout: Duration,
) -> String {
    let prefix = format!(
        "This turn hit the {}s time limit before the final answer was completed.",
        overall_timeout.as_secs()
    );
    if recovery.final_text.trim().is_empty() && recovery.tool_records.is_empty() {
        return format!("{prefix}\n\nNo reviewable partial result was captured before timeout.");
    }

    let partial =
        synthesize_reviewable_fallback(&recovery.final_text, &recovery.tool_records, false);
    format!("{prefix}\n\nPartial result:\n{partial}")
}

fn build_error_reviewable_message(recovery: &TimeoutRecoveryState, error: &str) -> String {
    if recovery.final_text.trim().is_empty() && recovery.tool_records.is_empty() {
        return format!(
            "This turn failed before the final answer could be stored.\n\nError:\n{}",
            compact_result_excerpt(error, 280)
        );
    }

    let partial =
        synthesize_reviewable_fallback(&recovery.final_text, &recovery.tool_records, false);
    format!(
        "This turn failed while closing out the final answer.\n\nError:\n{}\n\nRecovered result:\n{}",
        compact_result_excerpt(error, 280),
        partial
    )
}

async fn recover_error_reply_text(
    app_handle: &tauri::AppHandle,
    event_session_id: Option<String>,
    request_id: &str,
    started: std::time::Instant,
    request_config: &LlmRequestConfig<'_>,
    model: &str,
    user_request: &str,
    recovery: &TimeoutRecoveryState,
    turn: usize,
    error: &str,
) -> String {
    match recover_final_answer_from_tool_results(
        app_handle,
        event_session_id,
        request_id,
        started,
        request_config,
        model,
        user_request,
        &recovery.final_text,
        &recovery.tool_records,
        turn,
    )
    .await
    {
        Ok(Some(text)) => text,
        Ok(None) => build_error_reviewable_message(recovery, error),
        Err(recovery_err) => {
            append_chat_stage(
                request_id,
                "final_answer_recovery_failed",
                started,
                serde_json::json!({
                    "turn": turn,
                    "error": format!("{recovery_err:#}"),
                    "recovery_context": "outer_error_handler",
                }),
            )
            .await;
            build_error_reviewable_message(recovery, error)
        }
    }
}

async fn recover_timeout_reply_text(
    app_handle: &tauri::AppHandle,
    event_session_id: Option<String>,
    request_id: &str,
    started: std::time::Instant,
    request_config: &LlmRequestConfig<'_>,
    model: &str,
    user_request: &str,
    recovery: &TimeoutRecoveryState,
    turn: usize,
    overall_timeout: Duration,
) -> String {
    match recover_final_answer_from_tool_results(
        app_handle,
        event_session_id,
        request_id,
        started,
        request_config,
        model,
        user_request,
        &recovery.final_text,
        &recovery.tool_records,
        turn,
    )
    .await
    {
        Ok(Some(text)) => text,
        Ok(None) => build_timeout_reviewable_message(recovery, overall_timeout),
        Err(recovery_err) => {
            append_chat_stage(
                request_id,
                "final_answer_recovery_failed",
                started,
                serde_json::json!({
                    "turn": turn,
                    "error": format!("{recovery_err:#}"),
                    "recovery_context": "outer_timeout_handler",
                }),
            )
            .await;
            build_timeout_reviewable_message(recovery, overall_timeout)
        }
    }
}

async fn fail_incomplete_tool_calls_for_request(request_id: &str, error: &str) -> u64 {
    let Some(turn) = turns::find_turn_by_request_id(request_id)
        .await
        .ok()
        .flatten()
    else {
        return 0;
    };

    crate::tool_calls::fail_incomplete_for_turn(&turn.id, error)
        .await
        .unwrap_or(0)
}

async fn force_final_answer(
    app_handle: &tauri::AppHandle,
    event_session_id: Option<String>,
    request_id: &str,
    started: std::time::Instant,
    request_config: &LlmRequestConfig<'_>,
    model: &str,
    messages: &[OpenaiMessage],
    turn: usize,
) -> anyhow::Result<Option<String>> {
    if let Some(ref sid) = event_session_id {
        active_run::update_active_phase(sid, request_id, Some("finalizing_answer".to_string()));
    }
    let _ = app_handle.emit(
        "thinking-update",
        ThinkingUpdateEvent {
            session_id: event_session_id.clone(),
            request_id: request_id.to_string(),
            phase: "finalizing_answer".to_string(),
            message: "正在补全最终结论...".to_string(),
            turn: turn as i32,
            timestamp: Utc::now().to_rfc3339(),
        },
    );
    append_chat_stage(
        request_id,
        "final_answer_retry_start",
        started,
        serde_json::json!({
            "turn": turn,
            "messages": messages.len(),
            "max_tokens": FINAL_ANSWER_MAX_TOKENS,
        }),
    )
    .await;

    let mut final_messages = messages.to_vec();
    final_messages.push(OpenaiMessage {
        role: "system".to_string(),
        content: Some(TOOL_COMPLETION_SYSTEM_INSTRUCTION.to_string()),
        tool_calls: None,
        tool_call_id: None,
    });

    let mut final_request_config = request_config.clone();
    final_request_config.tool_choice = None;

    let response = llm::call_with_tools_with_messages_max_tokens(
        model,
        &final_messages,
        &final_request_config,
        None,
        FINAL_ANSWER_MAX_TOKENS,
    )
    .await?;
    let final_text = response
        .content
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());

    append_chat_stage(
        request_id,
        "final_answer_retry_done",
        started,
        serde_json::json!({
            "turn": turn,
            "text_chars": final_text.as_ref().map(|text| text.chars().count()).unwrap_or(0),
        }),
    )
    .await;

    Ok(final_text)
}

async fn recover_final_answer_from_tool_results(
    app_handle: &tauri::AppHandle,
    event_session_id: Option<String>,
    request_id: &str,
    started: std::time::Instant,
    request_config: &LlmRequestConfig<'_>,
    model: &str,
    user_request: &str,
    previous_text: &str,
    tool_records: &[ToolCallRecord],
    turn: usize,
) -> anyhow::Result<Option<String>> {
    const FINAL_ANSWER_RECOVERY_SYSTEM_INSTRUCTION: &str = concat!(
        "You are recovering a final user-facing answer after a tool-execution turn. ",
        "Use only the verified tool results in the prompt. ",
        "Do not call more tools. ",
        "Return a direct answer for the user, including concrete findings, changes, artifacts, and blockers if the work is incomplete. ",
        "Never claim files were written unless the prompt lists verified written artifacts."
    );

    if tool_records.is_empty() {
        return Ok(None);
    }

    if let Some(ref sid) = event_session_id {
        active_run::update_active_phase(sid, request_id, Some("recovering_answer".to_string()));
    }
    let _ = app_handle.emit(
        "thinking-update",
        ThinkingUpdateEvent {
            session_id: event_session_id.clone(),
            request_id: request_id.to_string(),
            phase: "recovering_answer".to_string(),
            message: "正在根据工具结果恢复最终答复...".to_string(),
            turn: turn as i32,
            timestamp: Utc::now().to_rfc3339(),
        },
    );
    append_chat_stage(
        request_id,
        "final_answer_recovery_start",
        started,
        serde_json::json!({
            "turn": turn,
            "tool_records": tool_records.len(),
            "max_tokens": FINAL_ANSWER_MAX_TOKENS,
        }),
    )
    .await;

    let prompt = build_final_answer_recovery_prompt(user_request, previous_text, tool_records);
    let text = llm::call_with_max_tokens(
        model,
        FINAL_ANSWER_RECOVERY_SYSTEM_INSTRUCTION,
        &prompt,
        request_config,
        FINAL_ANSWER_MAX_TOKENS,
    )
    .await?;
    let final_text = (!text.trim().is_empty()).then(|| text.trim().to_string());

    append_chat_stage(
        request_id,
        "final_answer_recovery_done",
        started,
        serde_json::json!({
            "turn": turn,
            "text_chars": final_text.as_ref().map(|text| text.chars().count()).unwrap_or(0),
        }),
    )
    .await;

    Ok(final_text)
}

async fn store_capability_request_reply(
    pool: &sqlx::SqlitePool,
    session_id: Option<&str>,
    request: &CapabilityRequest,
    history_limit: u32,
) -> anyhow::Result<ChatReply> {
    let content = serde_json::to_string(&vec![ContentBlock::CapabilityRequest {
        request: request.clone(),
    }])?;

    let assistant =
        db::insert_message_with_session(pool, ChatRole::Assistant, content, None, session_id)
            .await?;

    let history = if let Some(session_id) = session_id {
        db::history_for_session(session_id, history_limit).await?
    } else {
        db::history(history_limit).await?
    };

    Ok(ChatReply {
        message: assistant,
        history,
        bubble_summary: Some("This should be upgraded into a long-running goal.".to_string()),
    })
}

/// Spawn auto-summarization in the background (non-blocking).
/// Idempotent per session: once a summary is successfully generated,
/// the session is marked and subsequent calls are no-ops.
fn spawn_auto_summarize(
    session_id: &str,
    message_count: usize,
    history: &[ChatMessage],
    workspace_root: Option<&std::path::Path>,
) {
    // Idempotency: skip if already successfully summarized
    {
        let set = SUMMARIZED_SESSIONS.lock().unwrap();
        if set.contains(session_id) {
            return;
        }
    }

    let sid = session_id.to_string();
    let tail: Vec<TranscriptMessage> = history
        .iter()
        .rev()
        .take(10)
        .map(|m| TranscriptMessage {
            role: m.role.as_str().to_string(),
            text_preview: m.content.chars().take(200).collect(),
            raw: serde_json::json!({ "role": m.role.as_str() }),
        })
        .collect();
    let cwd = workspace_root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    tokio::spawn(async move {
        let input = summarizer::SummaryInput {
            transcript_tail: &tail,
            recent_files: &[],
            cwd: &cwd,
        };
        match summarizer::maybe_auto_summarize(message_count, input).await {
            Ok(Some(_)) => {
                if let Ok(mut set) = SUMMARIZED_SESSIONS.lock() {
                    set.insert(sid);
                }
            }
            Ok(None) => { /* below threshold, will retry on next trigger */ }
            Err(e) => {
                eprintln!("auto-summarize failed for session {}: {e:#}", &sid);
            }
        }
    });
}

/// V2 query loop with streaming Tauri event emission.
/// Emits: "stream-chat-token", "tool-execution-update", "thinking-update"
/// Has a 120s overall timeout to prevent the frontend from hanging.
pub async fn send_message_v2(
    content: String,
    app_handle: &tauri::AppHandle,
) -> anyhow::Result<ChatReply> {
    send_message_v2_with_session(
        content,
        app_handle,
        None,
        ChatTaskMode::Short,
        ChatCapability::AskWrite,
        false,
        None,
        None,
        None,
    )
    .await
}

/// V2 query loop with optional session_id for session-scoped history.
pub async fn send_message_v2_with_session(
    content: String,
    app_handle: &tauri::AppHandle,
    session_id: Option<String>,
    task_mode: ChatTaskMode,
    capability: ChatCapability,
    plan_only: bool,
    approved_write_scope: Option<Vec<String>>,
    allowed_tool_ids_override: Option<Vec<String>>,
    request_id_override: Option<String>,
) -> anyhow::Result<ChatReply> {
    send_message_v2_with_session_projection(
        content,
        app_handle,
        session_id,
        None,
        task_mode,
        capability,
        plan_only,
        approved_write_scope,
        allowed_tool_ids_override,
        request_id_override,
    )
    .await
}

/// Optional goal/task context for chat turns created by the goal task executor. (P0-A)
#[derive(Debug, Clone, Default)]
pub struct ChatExecutionContext {
    pub goal_id: Option<String>,
    pub goal_cycle_id: Option<String>,
    pub agent_task_id: Option<String>,
}

pub async fn send_message_v2_with_session_projection(
    content: String,
    app_handle: &tauri::AppHandle,
    session_id: Option<String>,
    projection_session_id: Option<String>,
    task_mode: ChatTaskMode,
    capability: ChatCapability,
    plan_only: bool,
    approved_write_scope: Option<Vec<String>>,
    allowed_tool_ids_override: Option<Vec<String>>,
    request_id_override: Option<String>,
) -> anyhow::Result<ChatReply> {
    send_message_v2_with_session_projection_ctx(
        content,
        app_handle,
        session_id,
        projection_session_id,
        task_mode,
        capability,
        plan_only,
        approved_write_scope,
        allowed_tool_ids_override,
        request_id_override,
        None,
    )
    .await
}

pub async fn send_message_v2_with_session_projection_ctx(
    content: String,
    app_handle: &tauri::AppHandle,
    session_id: Option<String>,
    projection_session_id: Option<String>,
    task_mode: ChatTaskMode,
    capability: ChatCapability,
    plan_only: bool,
    approved_write_scope: Option<Vec<String>>,
    allowed_tool_ids_override: Option<Vec<String>>,
    request_id_override: Option<String>,
    execution_context: Option<ChatExecutionContext>,
) -> anyhow::Result<ChatReply> {
    let content = content.trim().to_string();
    if content.is_empty() {
        bail!("消息不能为空");
    }

    // Long-running goal work needs a materially larger wall-clock budget than
    // foreground short chats, otherwise the UI looks like the run just stops.
    let overall_timeout = overall_timeout_for_mode(task_mode);
    let request_id = request_id_override.unwrap_or_else(|| Uuid::new_v4().to_string());
    let started = std::time::Instant::now();
    let event_session_id = projection_session_id.clone().or_else(|| session_id.clone());
    let timeout_recovery = Arc::new(Mutex::new(TimeoutRecoveryState::default()));
    let original_user_request = content.clone();
    let turn_workspace =
        resolve_session_workspace(session_id.as_deref().or(event_session_id.as_deref()))
            .await
            .ok()
            .flatten();
    let config_snapshot = config::load().await.ok();
    let resolved_model = crate::model_resolver::ModelResolver::resolve_with_context(
        crate::model_resolver::CallerContext::ChatMainLoop,
        None,
        Some(&request_id),
        None,
        turn_workspace.as_ref().map(|ws| ws.id.as_str()),
    )
    .await
    .unwrap_or_else(|_| crate::model_resolver::ResolvedModel {
        model_id: config_snapshot
            .as_ref()
            .map(|cfg| cfg.llm.model.clone())
            .unwrap_or_else(|| "gpt-4.1-mini".to_string()),
        transport: crate::llm_profiles::TransportKind::HttpApi,
        profile_id: None,
        policy_id: None,
        fallback_used: true,
        backend_kind: crate::agent_backends::BackendKind::ClaudeP,
        provider: config_snapshot
            .as_ref()
            .map(|cfg| cfg.llm.provider.as_str().to_string()),
        api_base_url: None,
        api_key: None,
        temperature: None,
        max_tokens: None,
    });
    let turn_metadata = serde_json::json!({
        "plan_only": plan_only,
        "has_projection_session": projection_session_id.is_some(),
        "allowed_tool_ids_override_count": allowed_tool_ids_override.as_ref().map(|items| items.len()).unwrap_or(0),
    });
    let _turn = turns::create_turn(turns::ChatTurnCreate {
        session_id: session_id.clone(),
        projection_session_id: projection_session_id.clone(),
        workspace_id: turn_workspace.as_ref().map(|ws| ws.id.clone()),
        request_id: request_id.clone(),
        initiator_kind: if session_id.is_some() {
            "chat".to_string()
        } else {
            "direct".to_string()
        },
        task_mode: task_mode.as_str().to_string(),
        capability: capability.as_str().to_string(),
        model_provider: resolved_model
            .provider
            .as_deref()
            .map(|p| p.to_string())
            .or_else(|| {
                config_snapshot
                    .as_ref()
                    .map(|cfg| cfg.llm.provider.as_str().to_string())
            }),
        model_name: Some(resolved_model.model_id.clone()),
        metadata_json: turn_metadata,
        goal_cycle_id: execution_context
            .as_ref()
            .and_then(|c| c.goal_cycle_id.clone()),
        agent_task_id: execution_context
            .as_ref()
            .and_then(|c| c.agent_task_id.clone()),
        goal_id: execution_context.as_ref().and_then(|c| c.goal_id.clone()),
    })
    .await?;
    let is_projected_execution = match (projection_session_id.as_deref(), session_id.as_deref()) {
        (Some(projection_session_id), Some(session_id)) => projection_session_id != session_id,
        (Some(_), None) => true,
        _ => false,
    };
    if let Some(ref sid) = event_session_id {
        active_run::register_active_run(sid, &request_id);
    }
    append_chat_stage(
        &request_id,
        "received",
        started,
        serde_json::json!({
            "content_chars": content.chars().count(),
            "session_id": session_id.as_deref(),
            "task_mode": task_mode.as_str(),
            "capability": capability.as_str(),
            "plan_only": plan_only,
        }),
    )
    .await;

    match tokio::time::timeout(
        overall_timeout,
        send_message_v2_inner(
            content,
            app_handle,
            request_id.clone(),
            started,
            session_id.clone(),
            event_session_id.clone(),
            task_mode,
            capability,
            plan_only,
            approved_write_scope,
            allowed_tool_ids_override,
            timeout_recovery.clone(),
            resolved_model.clone(),
        ),
    )
    .await
    {
        Ok(Ok(reply)) => {
            if let Some(ref sid) = event_session_id {
                active_run::remove_active_run(sid, &request_id);
            }
            let _ = app_handle.emit(
                "reply_stored",
                serde_json::json!({
                    "message_id": reply.message.id.as_str(),
                    "session_id": session_id,
                    "request_id": request_id.as_str(),
                }),
            );
            Ok(reply)
        }
        Ok(Err(err)) => {
            if let Some(ref sid) = event_session_id {
                active_run::remove_active_run(sid, &request_id);
            }
            let abandoned_tool_calls = fail_incomplete_tool_calls_for_request(
                &request_id,
                "turn failed before tool execution completed",
            )
            .await;
            let error_recovery_snapshot = snapshot_timeout_recovery(&timeout_recovery);
            if !error_recovery_snapshot.final_text.trim().is_empty()
                || !error_recovery_snapshot.tool_records.is_empty()
            {
                let config = config::load()
                    .await
                    .unwrap_or_else(|_| config::CoreConfig::default());
                let request_config =
                    LlmRequestConfig::from_resolved_with_fallback(&resolved_model, &config.llm);
                let recovery_reply_text = recover_error_reply_text(
                    app_handle,
                    event_session_id.clone(),
                    &request_id,
                    started,
                    &request_config,
                    &resolved_model.model_id,
                    &original_user_request,
                    &error_recovery_snapshot,
                    max_turns_for_mode(task_mode),
                    &format!("{err:#}"),
                )
                .await;
                append_chat_stage(
                    &request_id,
                    "failed_recovered",
                    started,
                    serde_json::json!({
                        "error": format!("{err:#}"),
                        "tool_records": error_recovery_snapshot.tool_records.len(),
                        "final_text_chars": error_recovery_snapshot.final_text.chars().count(),
                        "projected_execution": is_projected_execution,
                        "abandoned_tool_calls": abandoned_tool_calls,
                    }),
                )
                .await;
                let recovery_msg =
                    db::store_timeout_reply_with_text(session_id.as_deref(), &recovery_reply_text)
                        .await?;
                let recovery_plain_text = user_visible_plain_text(&recovery_msg.content);
                let recovery_scope_kind = if turn_workspace.is_some() {
                    "workspace"
                } else if session_id.is_some() {
                    "session"
                } else {
                    "global"
                };
                let recovery_scope_ref = turn_workspace.as_ref().map(|ws| ws.id.clone());
                let recovery_path_prefix = turn_workspace
                    .as_ref()
                    .map(|ws| ws.root.to_string_lossy().to_string());
                let recovery_tool_call_ids: Vec<String> = Vec::new();
                persist_final_turn_artifacts(
                    &request_id,
                    &recovery_msg,
                    "assistant_recovery",
                    &recovery_plain_text,
                    recovery_scope_kind,
                    recovery_scope_ref,
                    recovery_path_prefix,
                    &recovery_tool_call_ids,
                    &error_recovery_snapshot.tool_records,
                )
                .await?;
                let history_limit = config::load()
                    .await
                    .unwrap_or_else(|_| config::CoreConfig::default())
                    .chat_history_limit;
                let history = if let Some(session_id) = session_id.as_deref() {
                    db::history_for_session(session_id, history_limit).await?
                } else {
                    db::history(history_limit).await?
                };
                let bubble = generate_bubble_summary(
                    &recovery_msg.content,
                    &error_recovery_snapshot.tool_records,
                );
                let _ = app_handle.emit(
                    "reply_stored",
                    serde_json::json!({
                        "message_id": recovery_msg.id.as_str(),
                        "session_id": session_id,
                        "request_id": request_id.as_str(),
                    }),
                );
                append_chat_stage(
                    &request_id,
                    "reply_stored",
                    started,
                    serde_json::json!({
                        "assistant_message_id": recovery_msg.id.as_str(),
                        "history": history.len(),
                        "tool_records": error_recovery_snapshot.tool_records.len(),
                        "final_text_chars": recovery_reply_text.chars().count(),
                        "recovered_from_error": true,
                    }),
                )
                .await;
                append_chat_stage(
                    &request_id,
                    "done",
                    started,
                    serde_json::json!({ "recovered_from_error": true }),
                )
                .await;
                return Ok(ChatReply {
                    message: recovery_msg,
                    history,
                    bubble_summary: Some(bubble),
                });
            }
            append_chat_stage(
                &request_id,
                "failed",
                started,
                serde_json::json!({
                    "error": format!("{err:#}"),
                    "abandoned_tool_calls": abandoned_tool_calls,
                }),
            )
            .await;
            let _ = app_handle.emit(
                "reply_stored",
                serde_json::json!({
                    "message_id": serde_json::Value::Null,
                    "session_id": session_id,
                    "request_id": request_id.as_str(),
                }),
            );
            Err(err)
        }
        Err(_) => {
            if let Some(ref sid) = event_session_id {
                active_run::remove_active_run(sid, &request_id);
            }
            let abandoned_tool_calls = fail_incomplete_tool_calls_for_request(
                &request_id,
                "turn timed out before tool execution completed",
            )
            .await;
            let timeout_recovery_snapshot = snapshot_timeout_recovery(&timeout_recovery);
            let config = config::load()
                .await
                .unwrap_or_else(|_| config::CoreConfig::default());
            let request_config =
                LlmRequestConfig::from_resolved_with_fallback(&resolved_model, &config.llm);
            let timeout_reply_text = recover_timeout_reply_text(
                app_handle,
                event_session_id.clone(),
                &request_id,
                started,
                &request_config,
                &resolved_model.model_id,
                &original_user_request,
                &timeout_recovery_snapshot,
                max_turns_for_mode(task_mode),
                overall_timeout,
            )
            .await;
            append_chat_stage(
                &request_id,
                "timeout",
                started,
                serde_json::json!({
                    "timeout_ms": overall_timeout.as_millis() as u64,
                    "tool_records": timeout_recovery_snapshot.tool_records.len(),
                    "final_text_chars": timeout_recovery_snapshot.final_text.chars().count(),
                    "projected_execution": is_projected_execution,
                    "abandoned_tool_calls": abandoned_tool_calls,
                }),
            )
            .await;
            let timeout_msg =
                db::store_timeout_reply_with_text(session_id.as_deref(), &timeout_reply_text)
                    .await?;
            let timeout_plain_text = user_visible_plain_text(&timeout_msg.content);
            let timeout_scope_kind = if turn_workspace.is_some() {
                "workspace"
            } else if session_id.is_some() {
                "session"
            } else {
                "global"
            };
            let timeout_scope_ref = turn_workspace.as_ref().map(|ws| ws.id.clone());
            let timeout_path_prefix = turn_workspace
                .as_ref()
                .map(|ws| ws.root.to_string_lossy().to_string());
            let timeout_tool_call_ids: Vec<String> = Vec::new();
            persist_final_turn_artifacts(
                &request_id,
                &timeout_msg,
                "assistant_timeout",
                &timeout_plain_text,
                timeout_scope_kind,
                timeout_scope_ref,
                timeout_path_prefix,
                &timeout_tool_call_ids,
                &timeout_recovery_snapshot.tool_records,
            )
            .await?;
            let history_limit = config::load()
                .await
                .unwrap_or_else(|_| config::CoreConfig::default())
                .chat_history_limit;
            let history = if let Some(session_id) = session_id.as_deref() {
                db::history_for_session(session_id, history_limit).await?
            } else {
                db::history(history_limit).await?
            };
            let bubble = generate_bubble_summary(&timeout_msg.content, &[]);
            let _ = app_handle.emit(
                "reply_stored",
                serde_json::json!({
                    "message_id": timeout_msg.id.as_str(),
                    "session_id": session_id,
                    "request_id": request_id.as_str(),
                }),
            );
            append_chat_stage(
                &request_id,
                "reply_stored",
                started,
                serde_json::json!({
                    "assistant_message_id": timeout_msg.id.as_str(),
                    "history": history.len(),
                    "timeout_ms": overall_timeout.as_millis() as u64,
                    "tool_records": timeout_recovery_snapshot.tool_records.len(),
                    "final_text_chars": timeout_recovery_snapshot.final_text.chars().count(),
                }),
            )
            .await;
            append_chat_stage(
                &request_id,
                "done",
                started,
                serde_json::json!({ "timed_out": true }),
            )
            .await;
            return Ok(ChatReply {
                message: timeout_msg,
                history,
                bubble_summary: Some(bubble),
            });
        }
    }
}
async fn send_message_v2_inner(
    content: String,
    app_handle: &tauri::AppHandle,
    request_id: String,
    started: std::time::Instant,
    session_id: Option<String>,
    event_session_id: Option<String>,
    task_mode: ChatTaskMode,
    capability: ChatCapability,
    plan_only: bool,
    approved_write_scope: Option<Vec<String>>,
    allowed_tool_ids_override: Option<Vec<String>>,
    timeout_recovery: Arc<Mutex<TimeoutRecoveryState>>,
    resolved_model: crate::model_resolver::ResolvedModel,
) -> anyhow::Result<ChatReply> {
    let _ = expression::init_db().await;
    let _ = affection::init_db().await;
    append_chat_stage(
        &request_id,
        "local_state_initialized",
        started,
        serde_json::json!({}),
    )
    .await;

    crate::tools::register_builtin_tools();
    let pool = crate::db::pool().await?;
    append_chat_stage(&request_id, "db_pool_ready", started, serde_json::json!({})).await;
    let user_message = db::insert_message_with_session(
        &pool,
        ChatRole::User,
        content.clone(),
        None,
        session_id.as_deref(),
    )
    .await?;

    turns::attach_user_message_by_request(&request_id, &user_message.id).await?;
    let user_blocks = user_message.to_v2().content_blocks;
    let user_projection = turns::create_message_projection(turns::MessageProjectionCreate {
        request_id: request_id.clone(),
        message_id: Some(user_message.id.clone()),
        role: "user".to_string(),
        projection_kind: "user_input".to_string(),
        status: "visible".to_string(),
        visibility: "visible".to_string(),
        plain_text: Some(content.clone()),
        content_blocks_json: serde_json::to_value(user_blocks)?,
        source_event_id: None,
    })
    .await?;

    // Auto-title session from first user message
    if let Some(ref sid) = session_id {
        auto_title_session(sid, &content).await;
    }
    append_chat_stage(
        &request_id,
        "user_message_stored",
        started,
        serde_json::json!({
            "message_id": user_message.id.as_str(),
            "projection_id": user_projection.id.as_str(),
        }),
    )
    .await;

    let config = config::load().await?;
    let snapshot = tasks::load().await?;
    let recent_history = if let Some(ref sid) = session_id {
        db::history_for_session(sid, config.chat_history_limit)
            .await
            .unwrap_or_default()
    } else {
        db::history(config.chat_history_limit)
            .await
            .unwrap_or_default()
    };
    let session_workspace = resolve_session_workspace(session_id.as_deref())
        .await
        .ok()
        .flatten();
    append_chat_stage(
        &request_id,
        "context_loaded",
        started,
        serde_json::json!({
            "resolved_model": {
                "model_id": resolved_model.model_id,
                "provider": resolved_model.provider,
                "profile_id": resolved_model.profile_id,
                "policy_id": resolved_model.policy_id,
                "fallback_used": resolved_model.fallback_used,
            },
            "tasks": snapshot.tasks.len(),
            "history": recent_history.len(),
            "session_id": session_id.as_deref(),
            "task_mode": task_mode.as_str(),
            "capability": capability.as_str(),
            "plan_only": plan_only,
            "approved_write_scope_count": approved_write_scope.as_ref().map(|items| items.len()).unwrap_or(0),
            "allowed_tool_override_count": allowed_tool_ids_override.as_ref().map(|items| items.len()).unwrap_or(0),
        }),
    )
    .await;

    // ── Auto-summarize triggers 2 & 3: topic change / idle gap ──────────
    if let Some(ref sid) = session_id {
        // Trigger 2: topic change — keyword overlap < 20%
        if let Some(last_user) = recent_history
            .iter()
            .rev()
            .find(|m| m.role == ChatRole::User)
        {
            if keywords_overlap(&content, &last_user.content) < 0.2 {
                let count = recent_history.len() + 1; // +1 for just-inserted user msg
                spawn_auto_summarize(
                    sid,
                    count,
                    &recent_history,
                    session_workspace.as_ref().map(|ws| ws.root.as_path()),
                );
            }
        }
        // Trigger 3: idle gap >= 15 min
        if let Some(last_msg) = recent_history.last() {
            let idle_minutes = (Utc::now() - last_msg.created_at).num_minutes();
            if idle_minutes >= 15 {
                let count = recent_history.len() + 1;
                spawn_auto_summarize(
                    sid,
                    count,
                    &recent_history,
                    session_workspace.as_ref().map(|ws| ws.root.as_path()),
                );
            }
        }
    }

    // If the session is already a goal session, skip the upgrade gate —
    // the user is chatting within an active goal, not requesting to create one.
    let execution_session_summary = if let Some(ref sid) = session_id {
        crate::chat::session::get_chat_session(sid)
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    let session_is_goal = execution_session_summary
        .as_ref()
        .map(|s| s.session_kind == "goal")
        .unwrap_or(false);

    if !session_is_goal && should_offer_goal_upgrade(&content, task_mode, plan_only) {
        append_chat_stage(
            &request_id,
            "capability_request",
            started,
            serde_json::json!({
                "task_mode": task_mode.as_str(),
            }),
        )
        .await;

        let request = build_capability_request(&content, &recent_history, task_mode);
        return store_capability_request_reply(
            &pool,
            session_id.as_deref(),
            &request,
            config.chat_history_limit,
        )
        .await;
    }

    // Build messages for LLM
    let recall_path_prefix = session_workspace
        .as_ref()
        .map(|ws| ws.root.to_string_lossy().to_string());
    let projection_goal_id = match event_session_id.as_deref() {
        Some(event_sid) if session_id.as_deref() != Some(event_sid) => {
            crate::chat::session::get_chat_session(event_sid)
                .await
                .ok()
                .flatten()
                .and_then(|session| session.goal_id)
        }
        _ => None,
    };
    let recall_goal_id = projection_goal_id.or_else(|| {
        execution_session_summary
            .as_ref()
            .and_then(|s| s.goal_id.clone())
    });
    let recall_session_id = session_id.as_deref().or(event_session_id.as_deref());
    let system = build_system_prompt_with_context(
        &snapshot.tasks,
        &config,
        &content,
        session_workspace.as_ref().map(|ws| ws.root.as_path()),
        session_workspace.as_ref().map(|ws| ws.id.as_str()),
        recall_path_prefix.as_deref(),
        recall_session_id,
        recall_goal_id.as_deref(),
    )
    .await;
    let mut tool_defs = if let Some(allowed_tool_ids_override) = allowed_tool_ids_override
        .as_deref()
        .filter(|tool_ids| !tool_ids.is_empty())
    {
        build_tool_definitions_with_allowed_ids(
            &config,
            &content,
            task_mode,
            plan_only,
            Some(allowed_tool_ids_override),
        )
        .await
    } else {
        build_tool_definitions(&config, &content, task_mode, plan_only).await
    };
    // Long-task / goal-session mode gets the full tool set directly.
    // Progressive discovery would replace it with only tool.search, blinding the LLM.
    let progressive_tool_discovery = !matches!(task_mode, ChatTaskMode::Long)
        && should_use_progressive_tool_discovery(&content, task_mode, tool_defs.len());
    if progressive_tool_discovery {
        tool_defs = build_tool_definitions_for_catalog_selection(
            &config,
            &[String::from("tool.search")],
            task_mode,
            plan_only,
        )
        .await?;
    }
    append_chat_stage(
        &request_id,
        "prompt_built",
        started,
        serde_json::json!({
            "system_chars": system.chars().count(),
            "tool_defs": tool_defs.len(),
            "progressive_tool_discovery": progressive_tool_discovery,
            "task_mode": task_mode.as_str(),
            "capability": capability.as_str(),
            "plan_only": plan_only,
        }),
    )
    .await;

    let mut messages: Vec<OpenaiMessage> = vec![OpenaiMessage {
        role: "system".to_string(),
        content: Some(system),
        tool_calls: None,
        tool_call_id: None,
    }];
    if plan_only {
        messages.push(OpenaiMessage {
            role: "system".to_string(),
            content: Some(PLAN_ONLY_SYSTEM_INSTRUCTION.to_string()),
            tool_calls: None,
            tool_call_id: None,
        });
    }
    messages.push(OpenaiMessage {
        role: "system".to_string(),
        content: Some(execution_mode_system_instruction(task_mode).to_string()),
        tool_calls: None,
        tool_call_id: None,
    });

    // Add recent history
    let mut included_current_prompt = false;
    let mut recent = recent_history.iter().rev().take(12).collect::<Vec<_>>();
    recent.reverse();
    for message in &recent {
        if message.role == ChatRole::User && message.content == content {
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
            content: Some(content.clone()),
            tool_calls: None,
            tool_call_id: None,
        });
    }

    let request_config =
        LlmRequestConfig::from_resolved_with_fallback(&resolved_model, &config.llm);
    let mut all_tool_records: Vec<ToolCallRecord> = Vec::new();
    let mut all_tool_call_ids: Vec<String> = Vec::new();
    let mut all_reasoning_content: Vec<String> = Vec::new();
    let max_turns = max_turns_for_mode(task_mode);
    let mut final_content = String::new();
    let mut completed_with_final_answer = false;
    let mut observed_tool_calls = false;

    // Load expression state
    let mut mood = expression::load_mood().await.unwrap_or_default();
    mood.on_user_message();

    for _turn in 0..max_turns {
        let _ = avatar::set_activity_variant(ActivityVariant::Thinking).await;
        if let Ok(state) = avatar::get_current_avatar().await {
            let _ = app_handle.emit(
                "pet_avatar_changed",
                serde_json::json!({
                    "id": state.id,
                    "avatar_id": state.avatar_id.as_str(),
                    "activity_variant": state.activity_variant.as_str(),
                    "updated_at": state.updated_at.to_rfc3339(),
                }),
            );
        }

        // Emit planning phase before LLM call
        if let Some(ref sid) = event_session_id {
            active_run::update_active_phase(sid, &request_id, Some("planning".to_string()));
        }
        let _ = app_handle.emit(
            "thinking-update",
            ThinkingUpdateEvent {
                session_id: event_session_id.clone(),
                request_id: request_id.clone(),
                phase: "planning".to_string(),
                message: "正在分析...".to_string(),
                turn: _turn as i32,
                timestamp: Utc::now().to_rfc3339(),
            },
        );

        append_chat_stage(
            &request_id,
            "llm_turn_start",
            started,
            serde_json::json!({
                "turn": _turn,
                "resolved_model": {
                    "model_id": resolved_model.model_id,
                    "provider": resolved_model.provider,
                    "profile_id": resolved_model.profile_id,
                    "policy_id": resolved_model.policy_id,
                    "fallback_used": resolved_model.fallback_used,
                },
                "messages": messages.len(),
                "tool_defs": tool_defs.len(),
            }),
        )
        .await;
        let _ =
            turns::update_turn_counts_by_request(&request_id, Some((_turn + 1) as i64), None, None)
                .await;
        let llm_started = std::time::Instant::now();
        let mut first_text_token_logged = false;
        let stream_request_id = request_id.clone();
        let response_result = llm::call_with_tools_with_messages_streaming(
            &resolved_model.model_id,
            &messages,
            &request_config,
            if tool_defs.is_empty() {
                None
            } else {
                Some(tool_defs.clone())
            },
            |event| match event {
                LlmStreamEvent::Text(delta) => {
                    if !first_text_token_logged {
                        first_text_token_logged = true;
                        // Switch avatar to Writing on first text token (not after response)
                        tokio::spawn(async {
                            let _ = avatar::set_activity_variant(ActivityVariant::Writing).await;
                        });
                        spawn_chat_stage(
                            stream_request_id.clone(),
                            "llm_first_token",
                            started,
                            serde_json::json!({
                                "turn": _turn,
                                "llm_elapsed_ms": chat_elapsed_ms(llm_started),
                            }),
                        );
                    }
                    let _ = app_handle.emit(
                        "stream-chat-token",
                        StreamChatTokenEvent {
                            session_id: event_session_id.clone(),
                            request_id: request_id.clone(),
                            token: delta,
                        },
                    );
                }
                // Keep provider reasoning out of the visible token stream.
                // A bounded summary is persisted below for history/debugging.
                LlmStreamEvent::Reasoning(_) => {}
            },
        )
        .await;
        let response = match response_result {
            Ok(response) => {
                append_chat_stage(
                    &request_id,
                    "llm_turn_done",
                    started,
                    serde_json::json!({
                        "turn": _turn,
                        "llm_elapsed_ms": chat_elapsed_ms(llm_started),
                        "text_chars": response
                            .content
                            .as_deref()
                            .map(|text| text.chars().count())
                            .unwrap_or(0),
                        "tool_calls": response
                            .tool_calls
                            .as_ref()
                            .map(|calls| calls.len())
                            .unwrap_or(0),
                        "first_text_token": first_text_token_logged,
                    }),
                )
                .await;
                response
            }
            Err(err) => {
                append_chat_stage(
                    &request_id,
                    "llm_turn_failed",
                    started,
                    serde_json::json!({
                        "turn": _turn,
                        "llm_elapsed_ms": chat_elapsed_ms(llm_started),
                        "error": format!("{err:#}"),
                    }),
                )
                .await;
                return Err(err);
            }
        };

        // Collect thinking/reasoning content for bounded persistence.
        if let Some(ref reasoning) = response.reasoning_content {
            if !reasoning.is_empty() {
                all_reasoning_content.push(reasoning.clone());
            }
        }

        if let Some(ref text) = response.content {
            if !text.is_empty() {
                let _ = avatar::set_activity_variant(ActivityVariant::Writing).await;
                if let Ok(state) = avatar::get_current_avatar().await {
                    let _ = app_handle.emit(
                        "pet_avatar_changed",
                        serde_json::json!({
                            "id": state.id,
                            "avatar_id": state.avatar_id.as_str(),
                            "activity_variant": state.activity_variant.as_str(),
                            "updated_at": state.updated_at.to_rfc3339(),
                        }),
                    );
                }
                final_content = text.clone();
                set_timeout_recovery_text(&timeout_recovery, &final_content);
            }
        }

        // Check for tool calls
        let Some(tool_calls) = response.tool_calls.clone() else {
            // No tool calls — this is the final response
            messages.push(OpenaiMessage {
                role: "assistant".to_string(),
                content: response.content.clone(),
                tool_calls: None,
                tool_call_id: None,
            });
            completed_with_final_answer = response
                .content
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty());
            break;
        };

        if tool_calls.is_empty() {
            messages.push(OpenaiMessage {
                role: "assistant".to_string(),
                content: response.content.clone(),
                tool_calls: None,
                tool_call_id: None,
            });
            completed_with_final_answer = response
                .content
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty());
            break;
        }
        observed_tool_calls = true;

        // Add assistant message with tool_calls
        messages.push(OpenaiMessage {
            role: "assistant".to_string(),
            content: response.content.clone(),
            tool_calls: Some(tool_calls.clone()),
            tool_call_id: None,
        });

        // Emit tool_calling phase
        if let Some(ref sid) = event_session_id {
            active_run::update_active_phase(sid, &request_id, Some("tool_calling".to_string()));
        }
        let _ = app_handle.emit(
            "thinking-update",
            ThinkingUpdateEvent {
                session_id: event_session_id.clone(),
                request_id: request_id.clone(),
                phase: "tool_calling".to_string(),
                message: format!("正在调用 {} 个工具...", tool_calls.len()),
                turn: _turn as i32,
                timestamp: Utc::now().to_rfc3339(),
            },
        );

        // Execute each tool call
        let mut tool_run_count: u32 = 0;
        let mut active_tool_count: u32 = 0;
        let mut catalog_tool_ids = Vec::new();
        for tc in &tool_calls {
            let _ = avatar::set_activity_variant(ActivityVariant::ToolCalling).await;
            if let Ok(state) = avatar::get_current_avatar().await {
                let _ = app_handle.emit(
                    "pet_avatar_changed",
                    serde_json::json!({
                        "id": state.id,
                        "avatar_id": state.avatar_id.as_str(),
                        "activity_variant": state.activity_variant.as_str(),
                        "updated_at": state.updated_at.to_rfc3339(),
                    }),
                );
            }

            // Emit tool start event
            let _ = app_handle.emit(
                "tool-execution-update",
                ToolExecutionUpdateEvent {
                    session_id: event_session_id.clone(),
                    request_id: request_id.clone(),
                    tool_use_id: tc.id.clone(),
                    tool_name: tc.function.name.clone(),
                    status: "started".to_string(),
                    input: Some(
                        serde_json::from_str::<serde_json::Value>(&tc.function.arguments)
                            .unwrap_or_default(),
                    ),
                    output: None,
                    duration_ms: None,
                },
            );
            active_tool_count += 1;
            if let Some(ref sid) = event_session_id {
                active_run::update_active_tool_count(
                    sid,
                    &request_id,
                    tool_run_count,
                    active_tool_count,
                );
            }
            let _ = turns::update_turn_counts_by_request(
                &request_id,
                None,
                Some(tool_run_count as i64),
                Some(active_tool_count as i64),
            )
            .await;
            append_chat_stage(
                &request_id,
                "tool_start",
                started,
                serde_json::json!({
                    "turn": _turn,
                    "tool_use_id": tc.id.as_str(),
                    "tool_name": tc.function.name.as_str(),
                }),
            )
            .await;
            let (result, is_success, duration_ms, tool_status) = execute_tool_call(
                tc,
                session_id.as_deref(),
                session_workspace.as_ref().map(|ws| ws.id.as_str()),
                approved_write_scope.as_deref(),
                Some(&request_id),
                app_handle,
            )
            .await;
            append_chat_stage(
                &request_id,
                "tool_done",
                started,
                serde_json::json!({
                    "turn": _turn,
                    "tool_use_id": tc.id.as_str(),
                    "tool_name": tc.function.name.as_str(),
                    "success": is_success,
                    "status": tool_status,
                    "duration_ms": duration_ms,
                }),
            )
            .await;

            // Update mood
            if tool_status == "completed" && is_success {
                mood.on_tool_success();
            } else if tool_status == "error" {
                mood.on_tool_failure();
            }

            // Record for storage
            let tool_record = ToolCallRecord {
                tool_name: tc.function.name.clone(),
                arguments: tc.function.arguments.clone(),
                result: result.clone(),
                success: is_success,
            };
            all_tool_records.push(tool_record.clone());
            push_timeout_recovery_tool_record(&timeout_recovery, &tool_record);
            all_tool_call_ids.push(tc.id.clone());

            tool_run_count += 1;
            active_tool_count = active_tool_count.saturating_sub(1);
            if let Some(ref sid) = event_session_id {
                active_run::update_active_tool_count(
                    sid,
                    &request_id,
                    tool_run_count,
                    active_tool_count,
                );
            }
            let _ = turns::update_turn_counts_by_request(
                &request_id,
                None,
                Some(tool_run_count as i64),
                Some(active_tool_count as i64),
            )
            .await;
            // Emit tool completion event with duration_ms
            let _ = app_handle.emit(
                "tool-execution-update",
                ToolExecutionUpdateEvent {
                    session_id: event_session_id.clone(),
                    request_id: request_id.clone(),
                    tool_use_id: tc.id.clone(),
                    tool_name: tc.function.name.clone(),
                    status: tool_status.to_string(),
                    input: None,
                    output: Some(
                        serde_json::from_str::<serde_json::Value>(&result).unwrap_or_default(),
                    ),
                    duration_ms: Some(duration_ms),
                },
            );

            // Inject raw JSON for LLM continuation; pet-formatted text is UI-only (emitted above)
            messages.push(OpenaiMessage {
                role: "tool".to_string(),
                content: Some(result.clone()),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
            });

            if progressive_tool_discovery
                && tool_id_from_llm_name(&tc.function.name) == "tool.search"
            {
                catalog_tool_ids.extend(extract_catalog_tool_ids(&result));
            }
        }

        // ── Auto-summarize trigger 4: after tool execution ──────────────
        if let Some(ref sid) = session_id {
            if !tool_calls.is_empty() {
                let count = recent_history.len() + 1;
                spawn_auto_summarize(
                    sid,
                    count,
                    &recent_history,
                    session_workspace.as_ref().map(|ws| ws.root.as_path()),
                );
            }
        }

        if progressive_tool_discovery && !catalog_tool_ids.is_empty() {
            if let Some(ref sid) = event_session_id {
                active_run::update_active_phase(
                    sid,
                    &request_id,
                    Some("discovering_tools".to_string()),
                );
            }
            let _ = app_handle.emit(
                "thinking-update",
                ThinkingUpdateEvent {
                    session_id: event_session_id.clone(),
                    request_id: request_id.clone(),
                    phase: "discovering_tools".to_string(),
                    message: "正在查找可用工具".to_string(),
                    turn: _turn as i32,
                    timestamp: Utc::now().to_rfc3339(),
                },
            );

            let previous_tool_names: HashSet<String> = tool_defs
                .iter()
                .map(|def| def.function.name.clone())
                .collect();
            match build_tool_definitions_for_catalog_selection(
                &config,
                &catalog_tool_ids,
                task_mode,
                plan_only,
            )
            .await
            {
                Ok(expanded_defs) => {
                    let new_tool_names: HashSet<String> = expanded_defs
                        .iter()
                        .map(|def| def.function.name.clone())
                        .collect();
                    let injected_count = new_tool_names.difference(&previous_tool_names).count();
                    append_chat_stage(
                        &request_id,
                        "tool_catalog_injected",
                        started,
                        serde_json::json!({
                            "turn": _turn,
                            "catalog_results": catalog_tool_ids.len(),
                            "authorized_tools": expanded_defs.len(),
                            "new_tools": injected_count,
                            "tool_names": expanded_defs
                                .iter()
                                .map(|def| tool_id_from_llm_name(&def.function.name))
                                .collect::<Vec<_>>(),
                        }),
                    )
                    .await;

                    if injected_count > 0
                        || !new_tool_names.contains(&tool_name_for_llm("tool.search"))
                    {
                        tool_defs = expanded_defs;
                        continue;
                    }
                }
                Err(err) => {
                    append_chat_stage(
                        &request_id,
                        "tool_catalog_failed",
                        started,
                        serde_json::json!({
                            "turn": _turn,
                            "error": format!("{err:#}"),
                        }),
                    )
                    .await;
                }
            }
        }

        // Emit summarizing phase after tool execution, before next LLM call
        if let Some(ref sid) = event_session_id {
            active_run::update_active_phase(sid, &request_id, Some("summarizing".to_string()));
        }
        let _ = app_handle.emit(
            "thinking-update",
            ThinkingUpdateEvent {
                session_id: event_session_id.clone(),
                request_id: request_id.clone(),
                phase: "summarizing".to_string(),
                message: "正在整理结果...".to_string(),
                turn: _turn as i32,
                timestamp: Utc::now().to_rfc3339(),
            },
        );
    }

    let exhausted_tool_loop = !completed_with_final_answer;
    if !completed_with_final_answer && (observed_tool_calls || final_content.trim().is_empty()) {
        match force_final_answer(
            app_handle,
            event_session_id.clone(),
            &request_id,
            started,
            &request_config,
            &resolved_model.model_id,
            &messages,
            max_turns,
        )
        .await
        {
            Ok(Some(text)) => {
                final_content = text.clone();
                set_timeout_recovery_text(&timeout_recovery, &final_content);
                messages.push(OpenaiMessage {
                    role: "assistant".to_string(),
                    content: Some(text),
                    tool_calls: None,
                    tool_call_id: None,
                });
                completed_with_final_answer = true;
            }
            Ok(None) => {}
            Err(err) => {
                append_chat_stage(
                    &request_id,
                    "final_answer_retry_failed",
                    started,
                    serde_json::json!({
                        "turn": max_turns,
                        "error": format!("{err:#}"),
                    }),
                )
                .await;
            }
        }
    }

    if !completed_with_final_answer && !all_tool_records.is_empty() {
        match recover_final_answer_from_tool_results(
            app_handle,
            event_session_id.clone(),
            &request_id,
            started,
            &request_config,
            &resolved_model.model_id,
            &content,
            &final_content,
            &all_tool_records,
            max_turns,
        )
        .await
        {
            Ok(Some(text)) => {
                final_content = text;
                set_timeout_recovery_text(&timeout_recovery, &final_content);
                completed_with_final_answer = true;
            }
            Ok(None) => {}
            Err(err) => {
                append_chat_stage(
                    &request_id,
                    "final_answer_recovery_failed",
                    started,
                    serde_json::json!({
                        "turn": max_turns,
                        "error": format!("{err:#}"),
                    }),
                )
                .await;
            }
        }
    }

    if !completed_with_final_answer {
        final_content =
            synthesize_reviewable_fallback(&final_content, &all_tool_records, exhausted_tool_loop);
        set_timeout_recovery_text(&timeout_recovery, &final_content);
    } else if final_content.trim().is_empty() {
        final_content =
            synthesize_reviewable_fallback(&final_content, &all_tool_records, exhausted_tool_loop);
        set_timeout_recovery_text(&timeout_recovery, &final_content);
    }

    // Emit done phase
    if let Some(ref sid) = event_session_id {
        active_run::update_active_phase(sid, &request_id, Some("done".to_string()));
    }
    let _ = app_handle.emit(
        "thinking-update",
        ThinkingUpdateEvent {
            session_id: event_session_id.clone(),
            request_id: request_id.clone(),
            phase: "done".to_string(),
            message: "完成".to_string(),
            turn: max_turns as i32,
            timestamp: Utc::now().to_rfc3339(),
        },
    );

    // Update mood and affection
    mood.on_llm_success();
    let _ = expression::save_mood(&mood).await;
    let _ = affection::record_interaction(affection::InteractionType::Chat).await;

    // Update avatar
    update_post_chat_avatar(&snapshot.tasks, app_handle).await;

    // Store the final message
    let tool_calls_json = if all_tool_records.is_empty() {
        None
    } else {
        Some(all_tool_records.clone())
    };

    let content_for_db = build_content_blocks_for_db(
        &all_reasoning_content,
        &final_content,
        &all_tool_records,
        &all_tool_call_ids,
        plan_only,
    );

    let bubble = generate_bubble_summary(&content_for_db, &all_tool_records);
    let assistant = db::insert_message_with_session(
        &pool,
        ChatRole::Assistant,
        content_for_db,
        tool_calls_json,
        session_id.as_deref(),
    )
    .await?;
    let assistant_plain_text = user_visible_plain_text(&assistant.content);
    let scope_kind = if session_workspace.is_some() {
        "workspace"
    } else if session_id.is_some() {
        "session"
    } else {
        "global"
    };
    let scope_ref = session_workspace.as_ref().map(|ws| ws.id.clone());
    let path_prefix = session_workspace
        .as_ref()
        .map(|ws| ws.root.to_string_lossy().to_string());
    persist_final_turn_artifacts(
        &request_id,
        &assistant,
        "assistant_final",
        &assistant_plain_text,
        scope_kind,
        scope_ref,
        path_prefix,
        &all_tool_call_ids,
        &all_tool_records,
    )
    .await?;
    let history = if let Some(ref sid) = session_id {
        db::history_for_session(sid, config.chat_history_limit).await?
    } else {
        db::history(config.chat_history_limit).await?
    };

    // ── Auto-summarize trigger 1: message count 8-12 range ─────────────
    if let Some(ref sid) = session_id {
        let count = history.len();
        if (8..=12).contains(&count) {
            spawn_auto_summarize(
                sid,
                count,
                &history,
                session_workspace.as_ref().map(|ws| ws.root.as_path()),
            );
        }
    }

    append_chat_stage(
        &request_id,
        "reply_stored",
        started,
        serde_json::json!({
            "assistant_message_id": assistant.id.as_str(),
            "history": history.len(),
            "tool_records": all_tool_records.len(),
            "final_text_chars": final_content.chars().count(),
        }),
    )
    .await;
    append_chat_stage(&request_id, "done", started, serde_json::json!({})).await;

    Ok(ChatReply {
        message: assistant,
        history,
        bubble_summary: Some(bubble),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_overlap_different_topics() {
        let a = "I love programming in Rust language";
        let b = "The weather is nice today outside";
        assert!(
            keywords_overlap(a, b) < 0.2,
            "different topics should have low overlap"
        );
    }

    #[test]
    fn keywords_overlap_similar_topics() {
        let a = "I love programming in Rust language";
        let b = "Rust programming is really great";
        assert!(
            keywords_overlap(a, b) > 0.3,
            "similar topics should have high overlap"
        );
    }

    #[test]
    fn keywords_overlap_empty_input() {
        assert_eq!(keywords_overlap("", "hello world"), 0.0);
        assert_eq!(keywords_overlap("hello world", ""), 0.0);
        assert_eq!(keywords_overlap("", ""), 0.0);
    }

    #[test]
    fn idempotency_guard_prevents_duplicate() {
        // Reset state
        {
            let mut set = SUMMARIZED_SESSIONS.lock().unwrap();
            set.clear();
        }

        // First check: not summarized
        {
            let set = SUMMARIZED_SESSIONS.lock().unwrap();
            assert!(!set.contains("test-session-109"));
        }

        // Simulate successful summarization
        {
            let mut set = SUMMARIZED_SESSIONS.lock().unwrap();
            set.insert("test-session-109".to_string());
        }

        // Second check: already summarized
        {
            let set = SUMMARIZED_SESSIONS.lock().unwrap();
            assert!(set.contains("test-session-109"));
        }

        // Cleanup
        {
            let mut set = SUMMARIZED_SESSIONS.lock().unwrap();
            set.remove("test-session-109");
        }
    }

    #[test]
    fn short_task_agent_request_does_not_force_goal_upgrade() {
        assert!(!should_offer_goal_upgrade(
            "Please start an agent team and coordinate the next steps",
            ChatTaskMode::Short,
            false,
        ));
        assert!(should_offer_goal_upgrade(
            "Turn this into a long-running goal and continue until it is complete",
            ChatTaskMode::Short,
            false,
        ));
    }

    #[test]
    fn long_task_mode_always_triggers_upgrade_when_not_plan_only() {
        assert!(should_offer_goal_upgrade(
            "Implement this feature end to end",
            ChatTaskMode::Long,
            false,
        ));
        assert!(!should_offer_goal_upgrade(
            "Implement this feature end to end",
            ChatTaskMode::Long,
            true,
        ));
    }

    #[test]
    fn goal_seed_captures_request_and_context() {
        let history = vec![ChatMessage {
            id: "m1".to_string(),
            role: ChatRole::Assistant,
            content: "Earlier context".to_string(),
            created_at: Utc::now(),
            seq: 1,
            tool_calls: None,
        }];

        let seed = build_goal_seed("Ship the runtime runner and review flow", &history);
        assert!(seed.title.contains("Ship the runtime runner"));
        assert!(seed.objective.contains("User request"));
        assert!(seed.objective.contains("Conversation context"));
        assert!(seed.objective.contains("Earlier context"));
    }

    #[test]
    fn long_task_extends_timeout_and_turn_budget() {
        assert_eq!(
            overall_timeout_for_mode(ChatTaskMode::Short),
            Duration::from_secs(120)
        );
        assert_eq!(
            overall_timeout_for_mode(ChatTaskMode::Long),
            Duration::from_secs(30 * 60)
        );
        assert_eq!(max_turns_for_mode(ChatTaskMode::Short), 10);
        assert_eq!(max_turns_for_mode(ChatTaskMode::Long), 24);
    }

    #[test]
    fn fallback_summary_mentions_recent_tool_output() {
        let summary = synthesize_reviewable_fallback(
            "",
            &[ToolCallRecord {
                tool_name: "file.read".to_string(),
                arguments: r#"{"path":"docs/spec.md"}"#.to_string(),
                result: r#"{"content":"Final patch summary and remaining risk"}"#.to_string(),
                success: true,
            }],
            true,
        );

        assert!(summary.contains("已完成 1 个工具调用"));
        assert!(summary.contains("最近工具：file.read"));
        assert!(summary.contains("Final patch summary and remaining risk"));
    }

    #[test]
    fn timeout_reviewable_message_mentions_partial_result() {
        let message = build_timeout_reviewable_message(
            &TimeoutRecoveryState {
                final_text: String::new(),
                tool_records: vec![ToolCallRecord {
                    tool_name: "file.read".to_string(),
                    arguments: r#"{"path":"docs/spec.md"}"#.to_string(),
                    result: r#"{"content":"Partial rollout summary"}"#.to_string(),
                    success: true,
                }],
            },
            Duration::from_secs(120),
        );

        assert!(message.contains("120s"));
        assert!(message.contains("Partial result"));
        assert!(message.contains("Partial rollout summary"));
    }

    #[test]
    fn compact_result_excerpt_prefers_structured_text_and_marks_truncation() {
        let excerpt = compact_result_excerpt(
            &serde_json::json!({
                "text": "A".repeat(1400),
                "path": "docs/spec.md"
            })
            .to_string(),
            120,
        );

        assert!(excerpt.starts_with(&"A".repeat(40)));
        assert!(excerpt.ends_with("...(truncated)"));
    }

    #[test]
    fn final_answer_recovery_prompt_includes_user_request_and_tool_digest() {
        let prompt = build_final_answer_recovery_prompt(
            "Review the repository and explain the risks.",
            "",
            &[ToolCallRecord {
                tool_name: "file.read".to_string(),
                arguments: r#"{"file_path":"docs/spec.md"}"#.to_string(),
                result: serde_json::json!({
                    "text": "Security finding summary"
                })
                .to_string(),
                success: true,
            }],
        );

        assert!(prompt.contains("User request"));
        assert!(prompt.contains("Review the repository"));
        assert!(prompt.contains("Tool 1: file.read"));
        assert!(prompt.contains("Security finding summary"));
    }

    #[test]
    fn final_answer_recovery_prompt_includes_failed_tools_and_artifact_guard() {
        let prompt = build_final_answer_recovery_prompt(
            "Review the repository and write the summary.",
            "",
            &[
                ToolCallRecord {
                    tool_name: "file.read".to_string(),
                    arguments: r#"{"file_path":"missing.md"}"#.to_string(),
                    result: "file not found: missing.md; did you mean .claude/skills/x/missing.md?"
                        .to_string(),
                    success: false,
                },
                ToolCallRecord {
                    tool_name: "file.read".to_string(),
                    arguments: r#"{"file_path":"docs/spec.md"}"#.to_string(),
                    result: serde_json::json!({ "text": "Review finding" }).to_string(),
                    success: true,
                },
            ],
        );

        assert!(prompt.contains("failed 1"));
        assert!(prompt.contains("Verified written artifacts: none"));
        assert!(prompt.contains("Tool 1: file.read (failed)"));
        assert!(prompt.contains("did you mean .claude/skills"));
        assert!(prompt.contains("Do not claim files or artifacts were written"));
    }

    #[test]
    fn timeout_reviewable_message_handles_empty_recovery() {
        let message = build_timeout_reviewable_message(
            &TimeoutRecoveryState::default(),
            Duration::from_secs(120),
        );

        assert!(message.contains("120s"));
        assert!(message.contains("No reviewable partial result"));
    }
}
