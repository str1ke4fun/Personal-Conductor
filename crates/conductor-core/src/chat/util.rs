use super::types::{CompletionStep, ContentBlock, PlanStep, ToolCallRecord};

/// Generate a short 1-2 sentence summary for the pet bubble UI.
/// Avoids showing raw JSON content blocks or overly long text.
pub(super) fn generate_bubble_summary(content: &str, tool_records: &[ToolCallRecord]) -> String {
    // Try to extract meaningful text from content
    let text = if content.trim().starts_with('[') {
        // Content is likely JSON ContentBlock array — extract text blocks
        serde_json::from_str::<Vec<serde_json::Value>>(content)
            .ok()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| {
                        if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                            b.get("text").and_then(|t| t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    } else {
        content.to_string()
    };

    let text = text.trim();

    if text.is_empty() {
        if tool_records.is_empty() {
            return "处理完了。".to_string();
        }
        let tool_count = tool_records.len();
        let success_count = tool_records.iter().filter(|r| r.success).count();
        return if success_count == tool_count {
            format!("调用了 {} 个工具，全部完成。", tool_count)
        } else {
            format!("调用了 {} 个工具，{} 个成功。", tool_count, success_count)
        };
    }

    // Take first sentence or first 100 chars
    let summary = if let Some(idx) =
        text.find(|c| c == '。' || c == '！' || c == '？' || c == '.' || c == '!' || c == '?')
    {
        let end = idx + cjk_char_len(text, idx);
        text[..end].to_string()
    } else if text.chars().count() > 100 {
        let truncated: String = text.chars().take(100).collect();
        format!("{}…", truncated)
    } else {
        text.to_string()
    };

    // Append tool info if there were tool calls, including file paths
    if !tool_records.is_empty() {
        let tool_count = tool_records.len();
        // Extract file paths from file.write/file.edit tool results
        let file_paths: Vec<String> = tool_records
            .iter()
            .filter(|r| r.tool_name.starts_with("file."))
            .filter_map(|r| {
                let v: serde_json::Value = serde_json::from_str(&r.result).ok()?;
                let path = v.get("path")?.as_str()?.to_string();
                Some(path)
            })
            .collect();
        if !file_paths.is_empty() {
            format!("{}（写入 {}）", summary, file_paths.join(", "))
        } else {
            format!("{}（调用了 {} 个工具）", summary, tool_count)
        }
    } else {
        summary
    }
}

/// Truncate a tool result string to fit within `max_bytes` bytes.
///
/// If the content exceeds the limit it is sliced at a UTF-8 char boundary
/// and a `"...(truncated)"` suffix is appended.
pub fn truncate_tool_result(content: &str, max_bytes: usize) -> String {
    let suffix = "...(truncated)";
    if content.len() <= max_bytes {
        return content.to_string();
    }
    let budget = max_bytes.saturating_sub(suffix.len());
    // Find the largest valid char boundary <= budget.
    let mut end = budget;
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &content[..end], suffix)
}

/// Get byte length of the character at the given byte index.
fn cjk_char_len(s: &str, byte_idx: usize) -> usize {
    s[byte_idx..]
        .chars()
        .next()
        .map(|c| c.len_utf8())
        .unwrap_or(1)
}

pub(super) fn plain_text_for_llm(content: &str) -> String {
    let trimmed = content.trim();
    if !trimmed.starts_with('[') {
        return trimmed.to_string();
    }

    let Ok(blocks) = serde_json::from_str::<Vec<ContentBlock>>(trimmed) else {
        return trimmed.to_string();
    };

    let text = blocks
        .into_iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text),
            ContentBlock::CapabilityRequest { request } => Some(request.reason),
            ContentBlock::Plan {
                title,
                steps,
                write_scope,
                ..
            } => {
                let mut parts = vec![format!("Plan: {title}")];
                if !steps.is_empty() {
                    parts.push(
                        steps
                            .into_iter()
                            .map(|step| match step.detail {
                                Some(detail) if !detail.trim().is_empty() => {
                                    format!("- {}: {}", step.title.trim(), detail.trim())
                                }
                                _ => format!("- {}", step.title.trim()),
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                }
                if !write_scope.is_empty() {
                    parts.push(format!("Write scope: {}", write_scope.join(", ")));
                }
                Some(parts.join("\n"))
            }
            ContentBlock::Completion { title, summary, .. } => Some(match summary {
                Some(summary) if !summary.trim().is_empty() => {
                    format!("{title}\n{}", summary.trim())
                }
                _ => title,
            }),
            ContentBlock::Blocked {
                title,
                reason,
                action_items,
            } => {
                let mut parts = vec![title, reason];
                if !action_items.is_empty() {
                    parts.push(format!("Action needed: {}", action_items.join("; ")));
                }
                Some(parts.join("\n"))
            }
            ContentBlock::ToolResult {
                content,
                is_error: false,
                ..
            } => serde_json::from_str::<serde_json::Value>(&content)
                .ok()
                .and_then(|value| {
                    value
                        .get("message")
                        .and_then(|message| message.as_str())
                        .map(|message| message.trim().to_string())
                })
                .or_else(|| {
                    let content = content.trim().to_string();
                    (!content.is_empty()).then_some(content)
                }),
            ContentBlock::RuntimeProjection { .. } => None,
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    if text.trim().is_empty() {
        trimmed.to_string()
    } else {
        text
    }
}

pub(super) fn user_visible_plain_text(content: &str) -> String {
    let trimmed = content.trim();
    if !trimmed.starts_with('[') {
        return trimmed.to_string();
    }

    let Ok(blocks) = serde_json::from_str::<Vec<ContentBlock>>(trimmed) else {
        return trimmed.to_string();
    };

    let text_parts = blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } if !text.trim().is_empty() => Some(text.trim().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !text_parts.is_empty() {
        return text_parts.join("\n");
    }

    let fallback_parts = blocks
        .into_iter()
        .filter_map(|block| match block {
            ContentBlock::CapabilityRequest { request } => Some(request.reason),
            ContentBlock::Plan {
                title,
                steps,
                write_scope,
                ..
            } => Some(plan_block_plain_text(title, steps, write_scope)),
            ContentBlock::Completion { title, summary, .. } => Some(match summary {
                Some(summary) if !summary.trim().is_empty() => {
                    format!("{title}\n{}", summary.trim())
                }
                _ => title,
            }),
            ContentBlock::Blocked {
                title,
                reason,
                action_items,
            } => {
                let mut parts = vec![title, reason];
                if !action_items.is_empty() {
                    parts.push(format!("Action needed: {}", action_items.join("; ")));
                }
                Some(parts.join("\n"))
            }
            ContentBlock::RuntimeProjection { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::ToolUse { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::Text { .. } => None,
        })
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();

    fallback_parts.join("\n")
}

fn plan_block_plain_text(title: String, steps: Vec<PlanStep>, write_scope: Vec<String>) -> String {
    let mut parts = vec![format!("Plan: {title}")];
    if !steps.is_empty() {
        parts.push(
            steps
                .into_iter()
                .map(|step| match step.detail {
                    Some(detail) if !detail.trim().is_empty() => {
                        format!("- {}: {}", step.title.trim(), detail.trim())
                    }
                    _ => format!("- {}", step.title.trim()),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    if !write_scope.is_empty() {
        parts.push(format!("Write scope: {}", write_scope.join(", ")));
    }
    parts.join("\n")
}

pub(super) fn extract_plan_block(final_content: &str, status: &str) -> Option<ContentBlock> {
    let trimmed = final_content.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lines: Vec<&str> = trimmed.lines().collect();
    let title = lines
        .iter()
        .map(|line| sanitize_plan_line(line))
        .find(|line| !line.is_empty())
        .unwrap_or_else(|| "Execution plan".to_string());

    let mut steps = Vec::new();
    let mut write_scope = Vec::new();
    let mut diff_preview = extract_diff_fence(trimmed);

    enum Section {
        None,
        Steps,
        WriteScope,
        DiffPreview,
    }

    let mut section = Section::None;
    let mut diff_lines = Vec::new();
    let mut in_fence = false;

    for raw_line in &lines {
        let line = raw_line.trim();
        if line.is_empty() {
            if matches!(section, Section::DiffPreview) && !in_fence {
                section = Section::None;
            }
            continue;
        }

        if line.starts_with("```") {
            if matches!(section, Section::DiffPreview) {
                if in_fence {
                    let preview = diff_lines.join("\n").trim().to_string();
                    if !preview.is_empty() {
                        diff_preview = Some(preview);
                    }
                    diff_lines.clear();
                    section = Section::None;
                }
                in_fence = !in_fence;
                continue;
            }
            if line.contains("diff") {
                section = Section::DiffPreview;
                in_fence = true;
                continue;
            }
        }

        let lower = line.to_ascii_lowercase();
        if is_plan_heading(&lower, &["plan", "steps", "implementation plan"]) {
            section = Section::Steps;
            continue;
        }
        if is_plan_heading(
            &lower,
            &[
                "write scope",
                "scope",
                "files",
                "modified files",
                "edit scope",
            ],
        ) || line.contains("修改范围")
            || line.contains("变更范围")
            || line.contains("写入范围")
        {
            section = Section::WriteScope;
            continue;
        }
        if is_plan_heading(&lower, &["diff preview", "patch preview"]) || line.contains("差异预览")
        {
            section = Section::DiffPreview;
            continue;
        }

        if in_fence && matches!(section, Section::DiffPreview) {
            diff_lines.push((*raw_line).to_string());
            continue;
        }

        match section {
            Section::WriteScope => {
                let item = sanitize_plan_line(line);
                if !item.is_empty() {
                    write_scope.push(item);
                }
            }
            Section::Steps | Section::None => {
                if let Some(step) = parse_step_line(line) {
                    steps.push(step);
                    if matches!(section, Section::None) {
                        section = Section::Steps;
                    }
                }
            }
            Section::DiffPreview => {
                diff_lines.push((*raw_line).to_string());
            }
        }
    }

    if steps.is_empty() {
        steps = trimmed
            .lines()
            .map(sanitize_plan_line)
            .filter(|line| !line.is_empty())
            .take(3)
            .map(|title| PlanStep {
                title,
                detail: None,
            })
            .collect();
    }

    if steps.is_empty() {
        return None;
    }

    Some(ContentBlock::Plan {
        title,
        steps,
        status: status.to_string(),
        write_scope,
        diff_preview,
    })
}

fn is_plan_heading(line: &str, headings: &[&str]) -> bool {
    let normalized = line.trim_start_matches('#').trim().trim_end_matches(':');
    headings.iter().any(|heading| normalized == *heading)
}

fn sanitize_plan_line(line: &str) -> String {
    let trimmed = line.trim();
    let trimmed = trimmed
        .trim_start_matches('#')
        .trim_start_matches('-')
        .trim_start_matches('*')
        .trim();

    let mut chars = trimmed.chars().peekable();
    while matches!(chars.peek(), Some(c) if c.is_ascii_digit()) {
        chars.next();
    }
    if matches!(chars.peek(), Some('.') | Some(')')) {
        chars.next();
    }

    chars.collect::<String>().trim().to_string()
}

fn parse_step_line(line: &str) -> Option<PlanStep> {
    let trimmed = line.trim();
    let is_step = trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false);
    if !is_step {
        return None;
    }

    let normalized = sanitize_plan_line(trimmed);
    if normalized.is_empty() {
        return None;
    }

    let (title, detail) = if let Some((left, right)) = normalized.split_once(": ") {
        (left.trim().to_string(), Some(right.trim().to_string()))
    } else if let Some((left, right)) = normalized.split_once(" - ") {
        (left.trim().to_string(), Some(right.trim().to_string()))
    } else {
        (normalized, None)
    };

    Some(PlanStep { title, detail })
}

fn extract_diff_fence(content: &str) -> Option<String> {
    let mut in_diff = false;
    let mut lines = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if in_diff {
                break;
            }
            if trimmed.contains("diff") {
                in_diff = true;
            }
            continue;
        }
        if in_diff {
            lines.push(line.to_string());
        }
    }
    let preview = lines.join("\n").trim().to_string();
    (!preview.is_empty()).then_some(preview)
}

fn summarize_completion_text(text: &str, max_chars: usize) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let summary = if let Some(idx) =
        trimmed.find(|c| c == '。' || c == '！' || c == '？' || c == '.' || c == '!' || c == '?')
    {
        let end = idx + cjk_char_len(trimmed, idx);
        trimmed[..end].to_string()
    } else if trimmed.chars().count() > max_chars {
        let truncated: String = trimmed.chars().take(max_chars).collect();
        format!("{truncated}…")
    } else {
        trimmed.to_string()
    };

    (!summary.trim().is_empty()).then_some(summary)
}

fn summarize_written_paths(tool_records: &[ToolCallRecord]) -> Vec<String> {
    let mut paths = tool_records
        .iter()
        .filter(|record| record.tool_name.starts_with("file."))
        .filter_map(|record| {
            serde_json::from_str::<serde_json::Value>(&record.result)
                .ok()
                .and_then(|value| {
                    value
                        .get("path")
                        .and_then(|path| path.as_str())
                        .map(|path| path.to_string())
                })
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

#[cfg(any(test, feature = "tauri-events"))]
fn truncate_json_preview(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    let mut preview: String = value.chars().take(max_chars).collect();
    preview.push_str("...(truncated)");
    preview
}

#[cfg(any(test, feature = "tauri-events"))]
fn tool_input_block_from_arguments(arguments: &str) -> serde_json::Value {
    match serde_json::from_str::<serde_json::Value>(arguments) {
        Ok(value) if value.is_object() => value,
        Ok(value) => serde_json::json!({
            "_invalid_tool_arguments": true,
            "reason": format!("expected JSON object, got {}", match value {
                serde_json::Value::Null => "null",
                serde_json::Value::Bool(_) => "boolean",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::String(_) => "string",
                serde_json::Value::Array(_) => "array",
                serde_json::Value::Object(_) => "object",
            }),
            "raw_arguments_preview": truncate_json_preview(arguments, 600),
        }),
        Err(error) => serde_json::json!({
            "_invalid_tool_arguments": true,
            "reason": error.to_string(),
            "raw_arguments_preview": truncate_json_preview(arguments, 600),
        }),
    }
}

fn build_completion_block(
    final_content: &str,
    all_tool_records: &[ToolCallRecord],
) -> Option<ContentBlock> {
    let summary = summarize_completion_text(final_content, 180);
    if summary.is_none() && all_tool_records.is_empty() {
        return None;
    }

    let success_count = all_tool_records
        .iter()
        .filter(|record| record.success)
        .count();
    let failed_count = all_tool_records.len().saturating_sub(success_count);
    let mut steps = Vec::new();

    if !all_tool_records.is_empty() {
        steps.push(CompletionStep {
            label: "工具调用".to_string(),
            detail: Some(format!(
                "共 {} 个，成功 {} 个，失败 {} 个",
                all_tool_records.len(),
                success_count,
                failed_count
            )),
            status: if failed_count > 0 {
                "failed".to_string()
            } else {
                "done".to_string()
            },
        });
    }

    let written_paths = summarize_written_paths(all_tool_records);
    if !written_paths.is_empty() {
        let detail = if written_paths.len() == 1 {
            written_paths[0].clone()
        } else {
            format!("{} +{}", written_paths[0], written_paths.len() - 1)
        };
        steps.push(CompletionStep {
            label: "文件结果".to_string(),
            detail: Some(detail),
            status: "done".to_string(),
        });
    }

    Some(ContentBlock::Completion {
        title: if failed_count > 0 {
            "已生成可审阅的阶段结果".to_string()
        } else {
            "已生成可审阅结果".to_string()
        },
        summary,
        steps,
        duration_ms: None,
    })
}

// ── Tauri event helpers ──────────────────────────────────────────────────────

#[cfg(feature = "tauri-events")]
pub(super) fn chat_elapsed_ms(started: std::time::Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(feature = "tauri-events")]
pub(super) async fn append_chat_stage(
    request_id: &str,
    stage: &str,
    started: std::time::Instant,
    payload: serde_json::Value,
) {
    let stage_payload = serde_json::json!({
        "request_id": request_id,
        "stage": stage,
        "elapsed_ms": chat_elapsed_ms(started),
        "payload": payload.clone(),
    });
    let _ = crate::events::append("chat", "v2_stage", &stage_payload).await;

    let _ = super::turns::append_stage_event_by_request(request_id, stage, payload).await;
}

#[cfg(feature = "tauri-events")]
pub(super) fn spawn_chat_stage(
    request_id: String,
    stage: &'static str,
    started: std::time::Instant,
    payload: serde_json::Value,
) {
    tokio::spawn(async move {
        append_chat_stage(&request_id, stage, started, payload).await;
    });
}

// ── Content block builder for DB storage ─────────────────────────────────────

#[cfg(feature = "tauri-events")]
pub(super) fn build_content_blocks_for_db(
    all_reasoning_content: &[String],
    final_content: &str,
    all_tool_records: &[ToolCallRecord],
    all_tool_call_ids: &[String],
    plan_only: bool,
) -> String {
    let mut content_blocks: Vec<ContentBlock> = Vec::new();
    let mut has_plan_block = false;
    let mut deferred_text_block: Option<ContentBlock> = None;

    // Persist thinking/reasoning as summary (truncated to 500 chars)
    if !all_reasoning_content.is_empty() {
        let full_reasoning = all_reasoning_content.join("\n");
        let summary: String = full_reasoning.chars().take(500).collect();
        content_blocks.push(ContentBlock::Thinking { thinking: summary });
    }

    if plan_only {
        if let Some(plan_block) = extract_plan_block(final_content, "awaiting_approval") {
            content_blocks.push(plan_block);
            has_plan_block = true;
        }
    }

    if !plan_only {
        if let Some(completion_block) = build_completion_block(final_content, all_tool_records) {
            content_blocks.push(completion_block);
        }
    }

    if !final_content.is_empty() && !(plan_only && has_plan_block) {
        deferred_text_block = Some(ContentBlock::Text {
            text: final_content.to_string(),
        });
    }

    if let Some(text_block) = deferred_text_block.take() {
        content_blocks.push(text_block);
    }

    for (record, tc_id) in all_tool_records.iter().zip(all_tool_call_ids.iter()) {
        content_blocks.push(ContentBlock::ToolUse {
            id: tc_id.clone(),
            name: record.tool_name.clone(),
            input: tool_input_block_from_arguments(&record.arguments),
        });
        content_blocks.push(ContentBlock::ToolResult {
            tool_use_id: tc_id.clone(),
            content: truncate_tool_result(&record.result, 8192),
            is_error: !record.success,
        });
    }

    // If the LLM finished with tool calls but produced no final text, synthesize
    // a text block from the last successful tool result so the reply is never empty.
    let has_text = content_blocks
        .iter()
        .any(|b| matches!(b, ContentBlock::Text { .. }));
    if !has_text && !all_tool_records.is_empty() {
        if let Some(last_success) = all_tool_records.iter().rev().find(|r| r.success) {
            let snippet: String = last_success.result.chars().take(600).collect();
            if !snippet.trim().is_empty() {
                content_blocks.push(ContentBlock::Text {
                    text: format!("（工具调用已完成，最后一步结果：\n{}）", snippet.trim()),
                });
            }
        }
    }

    if content_blocks.is_empty() {
        final_content.to_string()
    } else if content_blocks.len() == 1
        && matches!(content_blocks.first(), Some(ContentBlock::Text { .. }))
    {
        final_content.to_string()
    } else {
        serde_json::to_string(&content_blocks).unwrap_or_else(|_| final_content.to_string())
    }
}

// ── Post-chat avatar/mood finalization ───────────────────────────────────────

#[cfg(feature = "tauri-events")]
pub(super) async fn update_post_chat_avatar(
    tasks: &[crate::tasks::Task],
    app_handle: &tauri::AppHandle,
) {
    use crate::avatar::{self, ActivityVariant};
    use crate::tasks::TaskStatus;
    use tauri::Emitter;

    let has_pending = tasks.iter().any(|t| t.status == TaskStatus::Pending);
    if has_pending {
        let _ = avatar::set_activity_variant(ActivityVariant::WaitingUser).await;
    } else {
        let _ = avatar::set_activity_variant(ActivityVariant::Done).await;
    }
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
    if !has_pending {
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let _ = avatar::set_activity_variant(ActivityVariant::Idle).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_plan_block, tool_input_block_from_arguments, truncate_tool_result};
    use crate::chat::types::ContentBlock;

    #[test]
    fn truncate_tool_result_keeps_content_within_max_bytes() {
        let input = "hello";
        assert_eq!(truncate_tool_result(input, 32), "hello");
    }

    #[test]
    fn truncate_tool_result_preserves_utf8_boundaries() {
        let input = "你好，世界";
        let truncated = truncate_tool_result(input, 10);
        assert!(truncated.ends_with("...(truncated)"));
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }

    #[test]
    fn tool_input_block_preserves_invalid_argument_preview() {
        let input =
            tool_input_block_from_arguments(r#"{"file_path":"report.md","content":"unterminated"#);

        assert_eq!(input["_invalid_tool_arguments"], true);
        assert!(input["raw_arguments_preview"]
            .as_str()
            .is_some_and(|preview| preview.contains("file_path")));
    }

    #[test]
    fn extract_plan_block_reads_scope_and_diff() {
        let block = extract_plan_block(
            r#"
# Refactor plan

## Plan
1. Update backend filters: block write tools in plan-only mode
2. Render plan card: show write scope and diff preview

## Write scope
- crates/conductor-core/src/chat/send_v2.rs
- apps/desktop/src/windows/ChatTimelinePane.tsx

## Diff preview
```diff
+ add plan block
- remove direct write tool
```
"#,
            "awaiting_approval",
        )
        .expect("plan block");

        match block {
            ContentBlock::Plan {
                title,
                steps,
                write_scope,
                diff_preview,
                status,
                ..
            } => {
                assert_eq!(title, "Refactor plan");
                assert_eq!(status, "awaiting_approval");
                assert_eq!(steps.len(), 2);
                assert_eq!(write_scope.len(), 2);
                assert!(diff_preview.unwrap().contains("+ add plan block"));
            }
            other => panic!("unexpected block: {other:?}"),
        }
    }

    #[cfg(feature = "tauri-events")]
    #[test]
    fn build_content_blocks_for_db_promotes_plan_only_reply_to_plan_block() {
        let content = super::build_content_blocks_for_db(
            &[],
            r#"
# Runtime refactor

## Plan
1. Tighten the write scope checks
2. Run cargo check and capture the output

## Write scope
- crates/conductor-core/src/chat/tools.rs
- apps/desktop/src/windows/ReviewQueue.tsx

## Diff preview
```diff
+ add a review projection row
- remove direct write access
```
"#,
            &[],
            &[],
            true,
        );

        let blocks: Vec<ContentBlock> =
            serde_json::from_str(&content).expect("plan-only content blocks");
        assert_eq!(
            blocks.len(),
            1,
            "plan-only replies should store a structured plan block"
        );

        match &blocks[0] {
            ContentBlock::Plan {
                title,
                steps,
                write_scope,
                diff_preview,
                status,
            } => {
                assert_eq!(title, "Runtime refactor");
                assert_eq!(status, "awaiting_approval");
                assert_eq!(steps.len(), 2);
                assert_eq!(
                    write_scope,
                    &vec![
                        "crates/conductor-core/src/chat/tools.rs".to_string(),
                        "apps/desktop/src/windows/ReviewQueue.tsx".to_string(),
                    ]
                );
                assert!(diff_preview
                    .as_deref()
                    .is_some_and(|preview| preview.contains("+ add a review projection row")));
            }
            other => panic!("unexpected block: {other:?}"),
        }
    }

    #[cfg(feature = "tauri-events")]
    #[test]
    fn build_content_blocks_for_db_promotes_reviewable_result_before_tool_trace() {
        let content = super::build_content_blocks_for_db(
            &["inspect repo".to_string()],
            "Final reviewable conclusion",
            &[crate::chat::types::ToolCallRecord {
                tool_name: "bash.execute".to_string(),
                arguments: r#"{"command":"echo hi"}"#.to_string(),
                result: r#"{"stdout":"hi"}"#.to_string(),
                success: true,
            }],
            &["tool-1".to_string()],
            false,
        );

        let blocks: Vec<ContentBlock> =
            serde_json::from_str(&content).expect("tool trace content blocks");
        assert!(matches!(
            blocks.first(),
            Some(ContentBlock::Thinking { .. })
        ));
        assert!(matches!(
            blocks.get(1),
            Some(ContentBlock::Completion { .. })
        ));
        assert!(matches!(
            blocks.get(2),
            Some(ContentBlock::Text { text }) if text == "Final reviewable conclusion"
        ));
        assert!(matches!(blocks.get(3), Some(ContentBlock::ToolUse { .. })));
        assert!(matches!(
            blocks.get(4),
            Some(ContentBlock::ToolResult { .. })
        ));
    }

    #[test]
    fn plain_text_for_llm_ignores_runtime_projection_placeholders() {
        let content = serde_json::to_string(&vec![
            ContentBlock::RuntimeProjection {
                request_id: "bg-1".to_string(),
                label: "Goal projection placeholder".to_string(),
            },
            ContentBlock::Text {
                text: "Final reviewable conclusion".to_string(),
            },
        ])
        .expect("serialize blocks");

        let flattened = super::plain_text_for_llm(&content);
        assert_eq!(flattened, "Final reviewable conclusion");
        assert!(!flattened.contains("runtime_projection"));
    }

    #[test]
    fn user_visible_plain_text_prefers_final_text_and_omits_tool_trace() {
        let content = serde_json::to_string(&vec![
            ContentBlock::Thinking {
                thinking: "internal trace".to_string(),
            },
            ContentBlock::Completion {
                title: "Completed".to_string(),
                summary: Some("duplicated summary".to_string()),
                steps: vec![],
                duration_ms: None,
            },
            ContentBlock::Text {
                text: "Final reviewable conclusion".to_string(),
            },
            ContentBlock::ToolResult {
                tool_use_id: "tool-1".to_string(),
                content: r#"{"text":"raw tool output"}"#.to_string(),
                is_error: false,
            },
        ])
        .expect("serialize blocks");

        let flattened = super::user_visible_plain_text(&content);
        assert_eq!(flattened, "Final reviewable conclusion");
        assert!(!flattened.contains("raw tool output"));
        assert!(!flattened.contains("duplicated summary"));
    }

    #[test]
    fn user_visible_plain_text_uses_completion_when_no_text_block_exists() {
        let content = serde_json::to_string(&vec![
            ContentBlock::ToolResult {
                tool_use_id: "tool-1".to_string(),
                content: "raw tool output".to_string(),
                is_error: false,
            },
            ContentBlock::Completion {
                title: "Completed".to_string(),
                summary: Some("Short user summary".to_string()),
                steps: vec![],
                duration_ms: None,
            },
        ])
        .expect("serialize blocks");

        let flattened = super::user_visible_plain_text(&content);
        assert_eq!(flattened, "Completed\nShort user summary");
        assert!(!flattened.contains("raw tool output"));
    }
}
