use crate::{config::LlmConfig, llm, memory, paths::Paths, transcript::TranscriptMessage};
use anyhow::Context;
use chrono::Utc;
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt};
use tracing::{debug, info, warn};

/// Minimum number of messages in a session before auto-summarization triggers.
pub const AUTO_SUMMARIZE_THRESHOLD: usize = 20;

pub struct SummaryInput<'a> {
    pub transcript_tail: &'a [TranscriptMessage],
    pub recent_files: &'a [PathBuf],
    pub cwd: &'a Path,
}

impl Copy for SummaryInput<'_> {}

impl Clone for SummaryInput<'_> {
    fn clone(&self) -> Self {
        *self
    }
}

pub struct SummaryOutput {
    pub slug: String,
    pub markdown: String,
    pub file_path: PathBuf,
}

static FALLBACK_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn get_fallback_count() -> u64 {
    FALLBACK_COUNT.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn reset_fallback_count() {
    FALLBACK_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
}

pub async fn summarize(input: SummaryInput<'_>) -> anyhow::Result<SummaryOutput> {
    fs::create_dir_all(Paths::summaries_dir()).await?;
    let slug = input
        .recent_files
        .first()
        .and_then(|path| path.file_stem())
        .and_then(|stem| stem.to_str())
        .map(slugify)
        .filter(|slug| !slug.is_empty())
        .unwrap_or_else(|| "unnamed".to_string());
    let ts = Utc::now();
    let filename = format!("{}-{}.md", ts.format("%Y%m%dT%H%M%SZ"), slug);
    let file_path = Paths::summaries_dir().join(filename);
    let markdown = generate_summary_markdown(&slug, ts.to_rfc3339().as_str(), input).await?;
    let mut file = fs::File::create(&file_path).await?;
    file.write_all(markdown.as_bytes()).await?;
    file.flush().await?;
    Ok(SummaryOutput {
        slug,
        markdown,
        file_path,
    })
}

/// Auto-generate and persist a conversation summary if the message count exceeds
/// [`AUTO_SUMMARIZE_THRESHOLD`].
///
/// Returns `Some(ConversationSummary)` when a summary was generated and stored,
/// or `None` when the threshold has not been reached.
///
/// This is the primary entry-point for wiring auto-summary into the chat flow:
/// call it after each message with the current session message count.
pub async fn maybe_auto_summarize(
    message_count: usize,
    input: SummaryInput<'_>,
) -> anyhow::Result<Option<memory::ConversationSummary>> {
    if message_count < AUTO_SUMMARIZE_THRESHOLD {
        debug!(
            "message_count {} < threshold {}, skipping auto-summary",
            message_count, AUTO_SUMMARIZE_THRESHOLD
        );
        return Ok(None);
    }

    info!(
        "message_count {} >= threshold {}, generating auto-summary",
        message_count, AUTO_SUMMARIZE_THRESHOLD
    );

    let output = summarize(input).await?;

    let keywords = extract_keywords(input);
    let summary = memory::add_conversation_summary(&output.markdown, &keywords).await?;

    info!("Auto-summary stored with id={}", summary.id);
    Ok(Some(summary))
}

/// Extract keyword strings from a summary input for storage alongside the summary.
/// Uses the slug derived from the first recent file plus stems of up to 5 recent files.
fn extract_keywords(input: SummaryInput<'_>) -> Vec<String> {
    let mut keywords: Vec<String> = Vec::new();

    for path in input.recent_files.iter().take(5) {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let kw = slugify(stem);
            if !kw.is_empty() && !keywords.contains(&kw) {
                keywords.push(kw);
            }
        }
    }

    // Add the working directory name as a contextual keyword
    if let Some(dir_name) = input.cwd.file_name().and_then(|s| s.to_str()) {
        let kw = slugify(dir_name);
        if !kw.is_empty() && !keywords.contains(&kw) {
            keywords.push(kw);
        }
    }

    keywords
}

async fn generate_summary_markdown(
    slug: &str,
    iso: &str,
    input: SummaryInput<'_>,
) -> anyhow::Result<String> {
    let llm_config = crate::config::load().await?;
    if llm_config.llm.api_key_set || std::env::var("LLM_API_KEY").is_ok() {
        match generate_llm_summary(slug, iso, input.clone(), &llm_config.llm).await {
            Ok(markdown) => {
                debug!("Successfully generated LLM summary");
                return Ok(markdown);
            }
            Err(e) => {
                FALLBACK_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!("LLM summary failed, falling back to template: {}", e);
            }
        }
    } else {
        debug!("No LLM API key configured, using template");
    }
    Ok(render_template(slug, iso, input))
}

async fn generate_llm_summary(
    slug: &str,
    iso: &str,
    input: SummaryInput<'_>,
    config: &LlmConfig,
) -> anyhow::Result<String> {
    let user_lang = llm::detect_user_language(input.transcript_tail);
    let system_prompt = build_system_prompt(user_lang);
    let user_prompt = build_user_prompt(input);

    let resolved = crate::model_resolver::ModelResolver::resolve(
        crate::model_resolver::CallerContext::Summarizer,
        None,
    )
    .await
    .unwrap_or_else(|_| crate::model_resolver::ResolvedModel {
        model_id: if config.model.is_empty() {
            "gpt-4o-mini".to_string()
        } else {
            config.model.clone()
        },
        transport: crate::llm_profiles::TransportKind::HttpApi,
        profile_id: None,
        policy_id: None,
        fallback_used: true,
        backend_kind: crate::agent_backends::BackendKind::ClaudeP,
        provider: None,
        api_base_url: None,
        api_key: None,
        temperature: None,
        max_tokens: None,
    });
    let llm_req_config = llm::LlmRequestConfig::from_resolved_with_fallback(&resolved, config);

    let response = llm::call(
        &resolved.model_id,
        &system_prompt,
        &user_prompt,
        &llm_req_config,
    )
    .await
    .context("LLM call failed")?;

    Ok(format!("# {} - {}\n\n{}", slug, iso, response.trim()))
}

fn build_system_prompt(lang: &str) -> String {
    if lang == "zh" {
        r#"你是一个个人任务管理器的摘要生成器。
根据 Claude Code 的对话记录和变更文件，生成简洁的回顾摘要。
请严格按以下格式输出 4 个部分：
1. **What**：一句话描述 agent 做了什么
2. **Where**：产物位置（文件路径/行号/段落）
3. **Why it matters**：和已有上下文的关系
4. **What you should check**：人需要重点看什么（1-3条）

总字数控制在 300 字以内。用中文回复。"#
            .to_string()
    } else {
        r#"You are a summary generator for a personal task manager.
Given Claude Code's transcript and changed files, generate a concise review summary.
Output exactly 4 sections:
1. **What**: One sentence describing what the agent did
2. **Where**: File paths or locations of artifacts
3. **Why it matters**: Context and relationship to existing work
4. **What you should check**: 1-3 specific things to verify

Keep total under 300 characters.
Respond in English."#
            .to_string()
    }
}

fn build_user_prompt(input: SummaryInput<'_>) -> String {
    let files_summary = if input.recent_files.is_empty() {
        "(no recent files)".to_string()
    } else {
        input
            .recent_files
            .iter()
            .take(10)
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };

    let transcript_preview = input
        .transcript_tail
        .iter()
        .take(5)
        .map(|m| format!("[{}]: {}", m.role, m.text_preview))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Changed files:\n{}\n\nRecent transcript:\n{}\n\nWorking directory: {}",
        files_summary,
        transcript_preview,
        input.cwd.display()
    )
}

fn render_template(slug: &str, iso: &str, input: SummaryInput<'_>) -> String {
    let files = if input.recent_files.is_empty() {
        "(no recent files)".to_string()
    } else {
        input
            .recent_files
            .iter()
            .take(10)
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let where_lines = input
        .recent_files
        .iter()
        .take(10)
        .map(|path| format!("- {}", path.display()))
        .collect::<Vec<_>>()
        .join("\n");
    let where_lines = if where_lines.is_empty() {
        "- (no recent files)".to_string()
    } else {
        where_lines
    };
    let preview = input
        .transcript_tail
        .last()
        .map(|message| message.text_preview.as_str())
        .unwrap_or("(no assistant transcript tail)");

    format!(
        "# {slug} - {iso}\n\n**What**: Claude modified {files}.\n\n**Where**:\n{where_lines}\n\n**Why it matters**: Template summary (LLM unavailable).\n\n**What you should check**:\n- Whether these files match your expectation\n- Transcript tail: {preview}\n- CWD: {}\n",
        input.cwd.display()
    )
}

fn slugify(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;
    use serde_json::json;

    #[tokio::test]
    async fn summarize_writes_markdown_file_with_expected_sections() {
        let root = TestRoot::new();
        let cwd = root.path().join("work");
        fs::create_dir_all(&cwd).await.expect("create cwd");
        let recent = vec![cwd.join("Doc A.md")];
        let tail = vec![TranscriptMessage {
            role: "assistant".to_string(),
            text_preview: "updated the second paragraph".to_string(),
            raw: json!({ "role": "assistant" }),
        }];

        let output = summarize(SummaryInput {
            transcript_tail: &tail,
            recent_files: &recent,
            cwd: &cwd,
        })
        .await
        .expect("summarize");

        assert_eq!(output.slug, "doc-a");
        assert!(output.file_path.starts_with(Paths::summaries_dir()));
        let persisted = fs::read_to_string(&output.file_path)
            .await
            .expect("read summary");
        assert_eq!(persisted, output.markdown);
        assert!(persisted.contains("**What**"));
        assert!(persisted.contains("**Where**"));
        assert!(persisted.contains("**Why it matters**"));
        assert!(persisted.contains("**What you should check**"));
        assert!(persisted.contains("updated the second paragraph"));
    }

    #[test]
    fn test_build_system_prompt_zh() {
        let prompt = build_system_prompt("zh");
        assert!(prompt.contains("中文"));
        assert!(prompt.contains("**What**"));
        assert!(prompt.contains("**Where**"));
    }

    #[test]
    fn test_build_system_prompt_en() {
        let prompt = build_system_prompt("en");
        assert!(prompt.contains("English"));
        assert!(prompt.contains("**What**"));
        assert!(prompt.contains("**Where**"));
    }

    #[test]
    fn test_build_user_prompt_with_files() {
        let root = TestRoot::new();
        let cwd = root.path().join("work");
        let recent = vec![cwd.join("main.rs"), cwd.join("lib.rs")];
        let tail = vec![TranscriptMessage {
            role: "user".to_string(),
            text_preview: "help me refactor".to_string(),
            raw: json!({}),
        }];
        let input = SummaryInput {
            transcript_tail: &tail,
            recent_files: &recent,
            cwd: &cwd,
        };
        let prompt = build_user_prompt(input);
        assert!(prompt.contains("main.rs"));
        assert!(prompt.contains("lib.rs"));
        assert!(prompt.contains("help me refactor"));
    }

    #[test]
    fn test_fallback_count_tracking() {
        reset_fallback_count();
        assert_eq!(get_fallback_count(), 0);
        FALLBACK_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(get_fallback_count(), 1);
    }

    // ── Auto-summarize tests (TASK-047) ───────────────────────────────────

    #[test]
    fn test_auto_summarize_threshold_constant() {
        assert_eq!(AUTO_SUMMARIZE_THRESHOLD, 20);
    }

    #[tokio::test]
    async fn maybe_auto_summarize_returns_none_below_threshold() {
        let root = TestRoot::new();
        let cwd = root.path().join("work");
        fs::create_dir_all(&cwd).await.expect("create cwd");

        let tail = vec![TranscriptMessage {
            role: "assistant".to_string(),
            text_preview: "hello".to_string(),
            raw: json!({ "role": "assistant" }),
        }];
        let input = SummaryInput {
            transcript_tail: &tail,
            recent_files: &[],
            cwd: &cwd,
        };

        // Well below the threshold of 20
        let result = maybe_auto_summarize(5, input)
            .await
            .expect("should not error");
        assert!(result.is_none(), "should return None when below threshold");
    }

    #[tokio::test]
    async fn maybe_auto_summarize_returns_none_at_threshold_minus_one() {
        let root = TestRoot::new();
        let cwd = root.path().join("work");
        fs::create_dir_all(&cwd).await.expect("create cwd");

        let tail = vec![TranscriptMessage {
            role: "assistant".to_string(),
            text_preview: "hello".to_string(),
            raw: json!({ "role": "assistant" }),
        }];
        let input = SummaryInput {
            transcript_tail: &tail,
            recent_files: &[],
            cwd: &cwd,
        };

        let result = maybe_auto_summarize(AUTO_SUMMARIZE_THRESHOLD - 1, input)
            .await
            .expect("should not error");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn maybe_auto_summarize_triggers_at_threshold() {
        let root = TestRoot::new();
        let cwd = root.path().join("work");
        fs::create_dir_all(&cwd).await.expect("create cwd");
        crate::memory::init_db().await.expect("init db");

        let recent = vec![cwd.join("task.rs")];
        let tail = vec![TranscriptMessage {
            role: "assistant".to_string(),
            text_preview: "implemented the feature".to_string(),
            raw: json!({ "role": "assistant" }),
        }];
        let input = SummaryInput {
            transcript_tail: &tail,
            recent_files: &recent,
            cwd: &cwd,
        };

        let result = maybe_auto_summarize(AUTO_SUMMARIZE_THRESHOLD, input)
            .await
            .expect("should succeed");

        assert!(result.is_some(), "should return Some at threshold");
        let summary = result.unwrap();
        assert!(!summary.summary.is_empty());
        assert!(!summary.id.is_empty());
    }

    #[tokio::test]
    async fn maybe_auto_summarize_triggers_above_threshold() {
        let root = TestRoot::new();
        let cwd = root.path().join("work");
        fs::create_dir_all(&cwd).await.expect("create cwd");
        crate::memory::init_db().await.expect("init db");

        let recent = vec![cwd.join("lib.rs")];
        let tail = vec![TranscriptMessage {
            role: "assistant".to_string(),
            text_preview: "refactored the module".to_string(),
            raw: json!({ "role": "assistant" }),
        }];
        let input = SummaryInput {
            transcript_tail: &tail,
            recent_files: &recent,
            cwd: &cwd,
        };

        let result = maybe_auto_summarize(50, input)
            .await
            .expect("should succeed");

        assert!(result.is_some(), "should return Some above threshold");
        let summary = result.unwrap();
        assert!(!summary.summary.is_empty());
    }

    #[tokio::test]
    async fn maybe_auto_summarize_stored_in_db() {
        let root = TestRoot::new();
        let cwd = root.path().join("work");
        fs::create_dir_all(&cwd).await.expect("create cwd");
        crate::memory::init_db().await.expect("init db");

        let recent = vec![cwd.join("api.rs")];
        let tail = vec![TranscriptMessage {
            role: "assistant".to_string(),
            text_preview: "added endpoints".to_string(),
            raw: json!({ "role": "assistant" }),
        }];
        let input = SummaryInput {
            transcript_tail: &tail,
            recent_files: &recent,
            cwd: &cwd,
        };

        let result = maybe_auto_summarize(AUTO_SUMMARIZE_THRESHOLD, input)
            .await
            .expect("should succeed")
            .expect("should be Some");

        // Verify the summary was persisted via memory module
        let recent = crate::memory::get_recent_conversations(10)
            .await
            .expect("get recent");
        assert!(
            recent.iter().any(|s| s.id == result.id),
            "auto-summary should be stored in conversation_summaries"
        );
    }

    #[test]
    fn test_extract_keywords_from_files() {
        let root = TestRoot::new();
        let cwd = root.path().join("my-project");
        let recent = vec![
            cwd.join("src").join("main.rs"),
            cwd.join("src").join("lib.rs"),
            cwd.join("tests").join("integration.rs"),
        ];
        let tail = vec![];
        let input = SummaryInput {
            transcript_tail: &tail,
            recent_files: &recent,
            cwd: &cwd,
        };

        let keywords = extract_keywords(input);
        assert!(keywords.contains(&"main".to_string()));
        assert!(keywords.contains(&"lib".to_string()));
        assert!(keywords.contains(&"integration".to_string()));
        assert!(keywords.contains(&"my-project".to_string()));
    }

    #[test]
    fn test_extract_keywords_empty_files() {
        let root = TestRoot::new();
        let cwd = root.path().join("workspace");
        let tail = vec![];
        let input = SummaryInput {
            transcript_tail: &tail,
            recent_files: &[],
            cwd: &cwd,
        };

        let keywords = extract_keywords(input);
        assert!(keywords.contains(&"workspace".to_string()));
    }

    #[test]
    fn test_extract_keywords_no_duplicates() {
        let root = TestRoot::new();
        let cwd = root.path().join("project");
        let recent = vec![cwd.join("main.rs"), cwd.join("main_test.rs")];
        let tail = vec![];
        let input = SummaryInput {
            transcript_tail: &tail,
            recent_files: &recent,
            cwd: &cwd,
        };

        let keywords = extract_keywords(input);
        let main_count = keywords.iter().filter(|k| *k == "main").count();
        assert_eq!(main_count, 1, "should not have duplicate keywords");
    }
}
