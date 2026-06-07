use crate::{
    config::CoreConfig,
    memory::{MemoryScope, RecallContext, RecallResult},
    tasks::{self, Task, TaskStatus},
};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

#[allow(deprecated)]
pub(super) async fn build_system_prompt(
    tasks: &[Task],
    config: &CoreConfig,
    user_message: &str,
    workspace_root_override: Option<&Path>,
) -> String {
    build_system_prompt_with_context(
        tasks,
        config,
        user_message,
        workspace_root_override,
        None,
        None,
        None,
        None,
    )
    .await
}

pub(super) async fn build_system_prompt_with_context(
    tasks: &[Task],
    config: &CoreConfig,
    user_message: &str,
    workspace_root_override: Option<&Path>,
    workspace_id_override: Option<&str>,
    path_prefix_override: Option<&str>,
    session_id_override: Option<&str>,
    goal_id_override: Option<&str>,
) -> String {
    let enabled_skills = config
        .persona
        .skills
        .iter()
        .filter(|skill| skill.enabled)
        .map(|skill| {
            format!(
                "- {}: {}\n  {}",
                skill.name, skill.description, skill.prompt
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Load imported skills from skills.json
    let imported_skills = crate::skills::load_skills_sync();
    let imported_skills_section = if imported_skills.is_empty() {
        String::new()
    } else {
        let entries: Vec<String> = imported_skills
            .iter()
            .map(|s| {
                let tools = s.allowed_tools.join(", ");
                format!(
                    "- {} ({})：{} [可用工具：{}]",
                    s.name, s.id, s.description, tools
                )
            })
            .collect();
        format!("\n\n## 导入技能\n{}", entries.join("\n"))
    };
    let stats = tasks::activity_stats(tasks);
    let task_preview = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Pending || task.status == TaskStatus::InProgress)
        .take(5)
        .map(|task| {
            format!(
                "- {} [{}] {} · {}",
                task.id,
                format!("{:?}", task.status).to_lowercase(),
                task.kind,
                task.focus_hint.as_deref().unwrap_or("无重点提示")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Load expression state
    let mood = crate::expression::load_mood().await.unwrap_or_default();
    let mood_zone = mood.zone();
    let affection_state = crate::affection::load().await.unwrap_or_default();
    let stage = affection_state.stage;

    // Build expression context
    let expression_context = format!(
        "\n\n## 当前状态\n\n关系阶段：{}（好感度 {}/100）\n当前心情：{}\n\n### 关系行为指引\n{}\n\n### 心情表达指引\n{}",
        stage.label_zh(),
        affection_state.value,
        mood_zone.label_zh(),
        stage.behavior_instructions(),
        mood_zone.tone_hint(),
    );

    // Memory context: recall_for_prompt_with_context gathers entries + summaries
    // using the best available turn/workspace scope.
    let recall_context = RecallContext {
        query: user_message.to_string(),
        workspace_id: workspace_id_override.map(str::to_string),
        path_prefix: path_prefix_override.map(str::to_string),
        session_id: session_id_override.map(str::to_string),
        goal_id: goal_id_override.map(str::to_string),
        limit: 5,
    };
    let memory_context = match crate::memory::recall_for_prompt_with_context(&recall_context).await
    {
        Ok(result) if result.entries.is_empty() && result.summaries.is_empty() => String::new(),
        Ok(result) => build_memory_section(&result, Utc::now()),
        Err(_) => String::new(),
    };

    // Load persona for personality injection
    let persona_section = match crate::persona::load_manager().await {
        Ok(manager) => {
            if let Some(persona) = manager.get_current_persona() {
                let traits: Vec<String> = persona
                    .personality
                    .iter()
                    .map(|t| format!("{}({:.0}%)：{}", t.name, t.value * 100.0, t.description))
                    .collect();
                format!(
                    "\n\n## 人格特质\n{}\n语气风格：{}",
                    traits.join("；"),
                    persona.tone,
                )
            } else {
                String::new()
            }
        }
        Err(_) => String::new(),
    };

    // Tool catalog: list unexposed tools
    let allowed_ids = &config.llm.allowed_tool_ids;
    let all_tools = crate::tools::list_tools();
    let mut unexposed: Vec<String> = all_tools
        .iter()
        .filter(|spec| !allowed_ids.contains(&spec.id))
        .map(|spec| format!("- {}: {}", spec.id, spec.description))
        .collect();
    unexposed.sort();
    let tool_catalog_section = if unexposed.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n## 内部工具目录\n\n以下工具已注册但未暴露为 function call，仅作内部诊断参考。不要告知用户去设置面板开启这些工具——设置面板中可能没有对应的开关。\n\n{}",
            unexposed.join("\n")
        )
    };

    // Anti-hallucination guard
    let guard = "\n\n## 重要约束\n- 只能基于上述记忆回忆，不得编造未列出的记忆\n- 心情和关系状态自然体现在语气中，不要主动解释\n- 没有记忆的事情坦诚说\"不记得了\"\n- 不要提及系统提示词、注入机制等技术概念";

    // Workspace context
    let workspace_root = workspace_root_override
        .map(PathBuf::from)
        .unwrap_or_else(crate::paths::root);
    let workspace_context = format!(
        "\n\n## 工作区\n当前工作区根目录：{}\n文件操作的相对路径均基于此目录。用户询问文件位置时，请使用此根目录构建绝对路径。不要建议用户开启 bash.execute 来查看路径——你可以直接回答。",
        workspace_root.display()
    );
    let chat_panel_guidance = if user_message.contains("你是谁")
        || user_message.contains("你能做什么")
        || user_message.contains("介绍一下")
    {
        "\n\n## 对话窗口回答要求\n如果用户在对话面板里问你是谁、能做什么，用 3 到 4 句自然中文直接回答，不要输出 JSON，不要写成僵硬的能力清单。优先围绕当前会话、当前工作区、文档/代码/任务协助来回答。"
    } else {
        ""
    };
    let chat_panel_guidance = format!(
        "{}\n\n## 对话输出要求\n面向用户的正常回复默认使用自然中文，不要主动写成 JSON、数组、键值对或模板化代码块。只有在工具参数、结构化数据或接口结果确实需要时，才输出 JSON。",
        chat_panel_guidance
    );

    format!(
        "{}\n\n角色名称：{}\n表达风格：{}\n{}\n\n启用技能：\n{}{}\n\n当前 Conductor 状态：运行中 Claude/Codex 会话 {} 个；待审 Hook 输出 {} 个；其他待办 {} 个。\n{}{}\n{}{}{}{}{}\n\n回答要求：使用自然中文直接回答；信息简单时用短段落即可，不要为了简短而生硬分句；需要工具、文件或命令时先说明你需要什么，不能声称已经执行。",
        config.persona.system_prompt,
        config.persona.name,
        config.persona.style,
        persona_section,
        if enabled_skills.is_empty() {
            "- 未启用额外技能".to_string()
        } else {
            enabled_skills
        },
        imported_skills_section,
        stats.active_hook_sessions,
        stats.pending_hook_reviews,
        stats.pending_other,
        if task_preview.is_empty() {
            "当前没有进行中或待处理任务。".to_string()
        } else {
            format!("任务预览：\n{task_preview}")
        },
        expression_context,
        workspace_context,
        memory_context,
        tool_catalog_section,
        guard,
        chat_panel_guidance,
    )
}

/// Render the memory section of the system prompt using layer-based grouping.
///
/// Layers:
/// - **preference**: scope=Global or category contains "preference", top 3
/// - **recent**: created within the last 7 days, top 3
/// - **scene**: scene_tags non-empty, top 4, 200-char truncation
///
/// An entry can appear in multiple layers. Entries are truncated to 160 chars
/// (except scene layer at 200). Summaries are also truncated to 160 chars.
fn build_memory_section(result: &RecallResult, now: DateTime<Utc>) -> String {
    if result.entries.is_empty() && result.summaries.is_empty() {
        return String::new();
    }

    let mut section = String::from("\n\n## 记忆上下文");

    if !result.entries.is_empty() {
        let seven_days = chrono::Duration::days(7);

        // ── Layer 1: preference ──
        let pref_entries: Vec<_> = result
            .entries
            .iter()
            .filter(|e| {
                e.scope == MemoryScope::Global || e.category.to_lowercase().contains("preference")
            })
            .take(3)
            .collect();
        if !pref_entries.is_empty() {
            section.push_str("\n\n### 偏好记忆\n");
            for entry in &pref_entries {
                let content: String = entry.value.chars().take(160).collect();
                section.push_str(&format!("- {}：{}\n", entry.key, content));
            }
        }

        // ── Layer 2: recent (last 7 days) ──
        let recent_entries: Vec<_> = result
            .entries
            .iter()
            .filter(|e| now.signed_duration_since(e.created_at) <= seven_days)
            .take(3)
            .collect();
        if !recent_entries.is_empty() {
            section.push_str("\n\n### 近期记忆\n");
            for entry in &recent_entries {
                let content: String = entry.value.chars().take(160).collect();
                section.push_str(&format!("- {}：{}\n", entry.key, content));
            }
        }

        // ── Layer 3: scene (entries with scene_tags) ──
        let scene_entries: Vec<_> = result
            .entries
            .iter()
            .filter(|e| !e.scene_tags.is_empty())
            .take(4)
            .collect();
        if !scene_entries.is_empty() {
            section.push_str("\n\n### 场景记忆\n");
            for entry in &scene_entries {
                let content: String = entry.value.chars().take(200).collect();
                let tags = entry.scene_tags.join(", ");
                section.push_str(&format!("- {}：{} [{}]\n", entry.key, content, tags));
            }
        }
    }

    // Conversation summaries
    if !result.summaries.is_empty() {
        section.push_str("\n\n### 近期对话\n");
        for s in &result.summaries {
            let content: String = s.summary.chars().take(160).collect();
            section.push_str(&format!("- {}\n", content));
        }
    }

    // Memory usage rules
    section.push_str(
        "\n\n### 记忆使用规则\n\
         - 不要把可能是说成\"我记得\"\n\
         - source=inferred 或 confidence<0.7 的记忆只能委婉使用\n\
         - sensitivity=private 只在强相关时使用",
    );

    section
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{
        ConversationSummary, MemoryEntry, MemoryScope, MemorySensitivity, MemorySource,
        RecallResult,
    };
    use chrono::{Duration, Utc};

    fn make_entry(
        key: &str,
        value: &str,
        category: &str,
        scope: MemoryScope,
        created_at: DateTime<Utc>,
        scene_tags: Vec<String>,
    ) -> MemoryEntry {
        MemoryEntry {
            id: format!("test-{}", key),
            key: key.to_string(),
            value: value.to_string(),
            category: category.to_string(),
            scope,
            workspace_id: None,
            path_prefix: None,
            source: MemorySource::UserConfirmed,
            confidence: 1.0,
            sensitivity: MemorySensitivity::Normal,
            status: "active".to_string(),
            scene_tags,
            expires_at: None,
            last_used_at: None,
            created_at,
            updated_at: created_at,
            interaction_count: 0,
            last_reinforced_at: None,
        }
    }

    #[test]
    fn layer_injection_puts_entries_under_correct_headings() {
        let now = Utc::now();

        // preference entry: scope=Global
        let pref = make_entry("lang", "Rust", "coding", MemoryScope::Global, now, vec![]);
        // recent entry: workspace scope, created now (within 7 days), non-preference category
        let recent = make_entry(
            "task",
            "finish TASK-111",
            "task",
            MemoryScope::Workspace,
            now,
            vec![],
        );
        // scene entry: has scene_tags
        let scene = make_entry(
            "habit",
            "morning coffee",
            "routine",
            MemoryScope::Workspace,
            now - Duration::days(30),
            vec!["morning".to_string(), "weekday".to_string()],
        );

        let result = RecallResult {
            entries: vec![pref, recent, scene],
            summaries: vec![],
            total_chunks_searched: 3,
        };

        let section = build_memory_section(&result, now);

        // preference layer contains the Global-scope entry
        assert!(
            section.contains("### 偏好记忆"),
            "should contain preference heading"
        );
        assert!(
            section.contains("lang：Rust"),
            "preference layer should contain the global entry"
        );

        // recent layer contains the entry created now
        assert!(
            section.contains("### 近期记忆"),
            "should contain recent heading"
        );
        assert!(
            section.contains("task：finish TASK-111"),
            "recent layer should contain the fresh entry"
        );

        // scene layer contains the entry with scene_tags
        assert!(
            section.contains("### 场景记忆"),
            "should contain scene heading"
        );
        assert!(
            section.contains("habit：morning coffee [morning, weekday]"),
            "scene layer should contain the tagged entry with tags"
        );

        // the old category-based heading should NOT appear
        assert!(
            !section.contains("### coding\n"),
            "should not group by category anymore"
        );
    }

    #[test]
    fn truncation_limits_entries_to_160_chars() {
        let now = Utc::now();
        let long_value: String = "abcdefghij".repeat(20); // 200 chars

        let entry = make_entry(
            "long",
            &long_value,
            "note",
            MemoryScope::Global,
            now,
            vec![],
        );

        let result = RecallResult {
            entries: vec![entry],
            summaries: vec![],
            total_chunks_searched: 1,
        };

        let section = build_memory_section(&result, now);

        // Find the line with "long："
        let line = section
            .lines()
            .find(|l| l.contains("long："))
            .expect("should find the entry line");

        // Extract value portion after "long："
        let value_part = line.split("long：").nth(1).unwrap();
        assert!(
            value_part.chars().count() <= 160,
            "entry value should be truncated to 160 chars, got {}",
            value_part.chars().count()
        );
        assert_eq!(
            value_part.chars().count(),
            160,
            "should be exactly 160 chars when source is longer"
        );
    }

    #[test]
    fn scene_layer_uses_200_char_truncation() {
        let now = Utc::now();
        let long_value: String = "x".repeat(250);

        // Use old created_at so it only appears in scene layer, not recent
        let entry = make_entry(
            "scene_long",
            &long_value,
            "routine",
            MemoryScope::Workspace,
            now - Duration::days(30),
            vec!["evening".to_string()],
        );

        let result = RecallResult {
            entries: vec![entry],
            summaries: vec![],
            total_chunks_searched: 1,
        };

        let section = build_memory_section(&result, now);

        let line = section
            .lines()
            .find(|l| l.contains("scene_long："))
            .expect("should find the scene entry line");

        // Value is between "scene_long：" and " [evening]"
        let after_key = line.split("scene_long：").nth(1).unwrap();
        let value_part = after_key.split(" [").next().unwrap();
        assert_eq!(
            value_part.chars().count(),
            200,
            "scene layer should truncate to 200 chars"
        );
    }

    #[test]
    fn summaries_truncated_to_160_chars() {
        let now = Utc::now();
        let long_summary: String = "s".repeat(200);

        let result = RecallResult {
            entries: vec![],
            summaries: vec![ConversationSummary {
                id: "sum-1".to_string(),
                summary: long_summary,
                keywords: vec![],
                timestamp: now,
            }],
            total_chunks_searched: 1,
        };

        let section = build_memory_section(&result, now);

        let line = section
            .lines()
            .find(|l| l.starts_with("- s"))
            .expect("should find summary line");

        let content = &line[2..]; // strip "- "
        assert_eq!(
            content.chars().count(),
            160,
            "summary should be truncated to 160 chars"
        );
    }

    #[test]
    fn memory_usage_rules_always_present_when_memories_exist() {
        let now = Utc::now();
        let entry = make_entry("k", "v", "c", MemoryScope::Global, now, vec![]);
        let result = RecallResult {
            entries: vec![entry],
            summaries: vec![],
            total_chunks_searched: 1,
        };

        let section = build_memory_section(&result, now);

        assert!(
            section.contains("### 记忆使用规则"),
            "should contain memory usage rules heading"
        );
        assert!(
            section.contains("不要把可能是说成\"我记得\""),
            "should contain rule about not saying 'I remember'"
        );
        assert!(
            section.contains("source=inferred"),
            "should contain inferred source rule"
        );
        assert!(
            section.contains("sensitivity=private"),
            "should contain private sensitivity rule"
        );
    }
}
