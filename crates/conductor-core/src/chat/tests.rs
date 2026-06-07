use super::*;
use crate::{
    config::CoreConfig,
    connectors::{
        ConnectorAuthStatus, ConnectorCapability, ConnectorImplementation, ConnectorRegistry,
        ConnectorSpec,
    },
    paths::Paths,
    skills::{SkillActivation, SkillPackage, SkillSource},
    tasks::{self, Artifact, Task, TaskStatus},
    test_support::TestRoot,
    tools::register_tool,
};
use std::path::PathBuf;

fn task(id: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        source: "claude".to_string(),
        kind: "review-doc".to_string(),
        artifact: Artifact {
            file: Some(PathBuf::from(format!("docs/{id}.md"))),
            anchor: None,
        },
        summary_ref: None,
        est_minutes: Some(5),
        focus_hint: Some("check acceptance notes".to_string()),
        status,
        created_at: chrono::Utc::now(),
        session_id: None,
        terminal_id: None,
        cwd: None,
        current_request: None,
        last_output_summary: None,
        last_event_at: None,
        permission_summary: None,
    }
}

fn ppt_task(id: &str, status: TaskStatus) -> Task {
    Task {
        id: id.to_string(),
        source: "claude".to_string(),
        kind: "review-ppt".to_string(),
        artifact: Artifact {
            file: Some(PathBuf::from(format!("slides/{id}.pptx"))),
            anchor: None,
        },
        summary_ref: None,
        est_minutes: Some(15),
        focus_hint: Some("check slide design".to_string()),
        status,
        created_at: chrono::Utc::now(),
        session_id: None,
        terminal_id: None,
        cwd: None,
        current_request: None,
        last_output_summary: None,
        last_event_at: None,
        permission_summary: None,
    }
}

fn task_with_time(id: &str, status: TaskStatus, minutes: u32) -> Task {
    Task {
        id: id.to_string(),
        source: "claude".to_string(),
        kind: "review-doc".to_string(),
        artifact: Artifact {
            file: Some(PathBuf::from(format!("docs/{id}.md"))),
            anchor: None,
        },
        summary_ref: None,
        est_minutes: Some(minutes),
        focus_hint: Some("check content".to_string()),
        status,
        created_at: chrono::Utc::now(),
        session_id: None,
        terminal_id: None,
        cwd: None,
        current_request: None,
        last_output_summary: None,
        last_event_at: None,
        permission_summary: None,
    }
}

fn default_config() -> CoreConfig {
    CoreConfig {
        focus_window_minutes: 25,
        chat_history_limit: 50,
        ..Default::default()
    }
}

#[tokio::test]
async fn test_parser_what_to_focus() {
    let config = default_config();
    let tasks = vec![
        task("t-001", TaskStatus::Pending),
        task("t-002", TaskStatus::Pending),
    ];
    let answer = commands::rule_based_answer("现在该看什么", &tasks, &config).await;
    assert!(answer.contains("推荐下一个任务"));
    assert!(answer.contains("t-001"));
}

#[tokio::test]
async fn test_parser_time_filter() {
    let _root = TestRoot::new();
    let config = default_config();
    let tasks = vec![
        task_with_time("t-001", TaskStatus::Pending, 5),
        task_with_time("t-002", TaskStatus::Pending, 30),
        task_with_time("t-003", TaskStatus::Pending, 60),
    ];

    // Create tasks in DB so list_tasks_by_budget can find them
    for t in &tasks {
        crate::tasklist::create_task(crate::tasklist::TaskCreateInput {
            subject: t.id.clone(),
            kind: Some(t.kind.clone()),
            est_minutes: t.est_minutes,
            ..Default::default()
        })
        .await
        .expect("create task");
    }

    let answer = commands::rule_based_answer("我有 10 分钟", &tasks, &config).await;
    assert!(answer.contains("t-001"));
    assert!(!answer.contains("t-002"));
    assert!(!answer.contains("t-003"));
}

#[tokio::test]
async fn test_parser_time_filter_20min() {
    let _root = TestRoot::new();
    let config = default_config();
    let tasks = vec![
        task_with_time("t-001", TaskStatus::Pending, 5),
        task_with_time("t-002", TaskStatus::Pending, 15),
        task_with_time("t-003", TaskStatus::Pending, 30),
    ];

    for t in &tasks {
        crate::tasklist::create_task(crate::tasklist::TaskCreateInput {
            subject: t.id.clone(),
            kind: Some(t.kind.clone()),
            est_minutes: t.est_minutes,
            ..Default::default()
        })
        .await
        .expect("create task");
    }

    let answer = commands::rule_based_answer("20min", &tasks, &config).await;
    assert!(answer.contains("t-001"));
    assert!(answer.contains("t-002"));
    assert!(!answer.contains("t-003"));
}

#[tokio::test]
async fn test_parser_time_filter_half_hour() {
    let _root = TestRoot::new();
    let config = default_config();
    let tasks = vec![
        task_with_time("t-001", TaskStatus::Pending, 30),
        task_with_time("t-002", TaskStatus::Pending, 60),
    ];

    for t in &tasks {
        crate::tasklist::create_task(crate::tasklist::TaskCreateInput {
            subject: t.id.clone(),
            kind: Some(t.kind.clone()),
            est_minutes: t.est_minutes,
            ..Default::default()
        })
        .await
        .expect("create task");
    }

    let answer = commands::rule_based_answer("半小时", &tasks, &config).await;
    assert!(answer.contains("t-001"));
    assert!(!answer.contains("t-002"));
}

#[tokio::test]
async fn test_parser_time_filter_one_hour() {
    let _root = TestRoot::new();
    let config = default_config();
    let tasks = vec![
        task_with_time("t-001", TaskStatus::Pending, 60),
        task_with_time("t-002", TaskStatus::Pending, 90),
    ];

    for t in &tasks {
        crate::tasklist::create_task(crate::tasklist::TaskCreateInput {
            subject: t.id.clone(),
            kind: Some(t.kind.clone()),
            est_minutes: t.est_minutes,
            ..Default::default()
        })
        .await
        .expect("create task");
    }

    let answer = commands::rule_based_answer("1小时", &tasks, &config).await;
    assert!(answer.contains("t-001"));
    assert!(!answer.contains("t-002"));
}

#[tokio::test]
async fn test_parser_pass_action() {
    let config = default_config();
    let tasks = vec![
        task("t-001", TaskStatus::Pending),
        task("t-002", TaskStatus::Pending),
    ];

    let answer = commands::rule_based_answer("第一项过了", &tasks, &config).await;
    assert!(answer.contains("✅"));
    assert!(answer.contains("已通过"));
    assert!(answer.contains("t-001"));
}

#[tokio::test]
async fn test_parser_skip_action() {
    let config = default_config();
    let tasks = vec![
        task("t-001", TaskStatus::Pending),
        task("t-002", TaskStatus::Pending),
    ];

    let answer = commands::rule_based_answer("跳过第二个", &tasks, &config).await;
    assert!(answer.contains("⏭"));
    assert!(answer.contains("已跳过"));
    assert!(answer.contains("t-002"));
}

#[tokio::test]
async fn test_parser_reject_action() {
    let config = default_config();
    let tasks = vec![
        task("t-001", TaskStatus::Pending),
        task("t-002", TaskStatus::Pending),
    ];

    let answer = commands::rule_based_answer("这个不要了", &tasks, &config).await;
    assert!(answer.contains("❌"));
    assert!(answer.contains("已拒绝"));
}

#[tokio::test]
async fn test_parser_snooze_action() {
    let config = default_config();
    let tasks = vec![
        ppt_task("t-ppt-001", TaskStatus::Pending),
        task("t-002", TaskStatus::Pending),
    ];

    let answer = commands::rule_based_answer("PPT 那个推后", &tasks, &config).await;
    assert!(answer.contains("⏰"));
    assert!(answer.contains("已推迟"));
    assert!(answer.contains("t-ppt-001"));
    assert!(answer.contains("review-ppt"));
}

#[tokio::test]
async fn test_parser_snooze_with_time() {
    let config = default_config();
    let tasks = vec![task("t-001", TaskStatus::Pending)];

    let answer = commands::rule_based_answer("第一项推后 30 分钟", &tasks, &config).await;
    assert!(answer.contains("⏰"));
    assert!(answer.contains("30 分钟"));
}

#[tokio::test]
async fn test_parser_status() {
    let config = default_config();
    let tasks = vec![
        task("t-001", TaskStatus::Pending),
        task("t-002", TaskStatus::Pending),
        task("t-003", TaskStatus::Passed),
        task("t-004", TaskStatus::Rejected),
    ];

    let answer = commands::rule_based_answer("现在状态", &tasks, &config).await;
    assert!(answer.contains("📊"));
    assert!(answer.contains("2"));
    assert!(answer.contains("待处理"));
    assert!(answer.contains("已通过"));
    assert!(answer.contains("已拒绝"));
}

#[tokio::test]
async fn test_parser_help() {
    let config = default_config();
    let tasks: Vec<Task> = vec![];

    let answer = commands::rule_based_answer("帮助", &tasks, &config).await;
    assert!(answer.contains("清和"));
    assert!(answer.contains("任务助手"));
    assert!(answer.contains("可用命令"));
}

#[tokio::test]
async fn test_parser_last_index() {
    let config = default_config();
    let tasks = vec![
        task("t-001", TaskStatus::Pending),
        task("t-002", TaskStatus::Pending),
        task("t-003", TaskStatus::Pending),
    ];

    let answer = commands::rule_based_answer("最后那个过了", &tasks, &config).await;
    assert!(answer.contains("✅"));
    assert!(answer.contains("t-003"));
}

#[tokio::test]
async fn test_parser_numeric_index() {
    let config = default_config();
    let tasks = vec![
        task("t-001", TaskStatus::Pending),
        task("t-002", TaskStatus::Pending),
    ];

    let answer = commands::rule_based_answer("2 passed", &tasks, &config).await;
    assert!(answer.contains("✅"));
    assert!(answer.contains("t-002"));
}

#[tokio::test]
async fn test_parser_english_what_to_focus() {
    let config = default_config();
    let tasks = vec![task("t-001", TaskStatus::Pending)];

    let answer = commands::rule_based_answer("What should I focus on?", &tasks, &config).await;
    assert!(answer.contains("推荐下一个任务"));
    assert!(answer.contains("t-001"));
}

#[tokio::test]
async fn test_parser_english_pass() {
    let config = default_config();
    let tasks = vec![task("t-001", TaskStatus::Pending)];

    let answer = commands::rule_based_answer("1 passed", &tasks, &config).await;
    assert!(answer.contains("✅"));
}

#[tokio::test]
async fn test_parser_english_skip() {
    let config = default_config();
    let tasks = vec![task("t-001", TaskStatus::Pending)];

    let answer = commands::rule_based_answer("skip first", &tasks, &config).await;
    assert!(answer.contains("⏭"));
}

#[tokio::test]
async fn test_parser_empty_tasks() {
    let config = default_config();
    let tasks: Vec<Task> = vec![];

    let answer = commands::rule_based_answer("现在该看什么", &tasks, &config).await;
    assert!(answer.contains("没有"));
}

#[tokio::test]
async fn test_parser_unknown_returns_hint() {
    let config = default_config();
    let tasks: Vec<Task> = vec![];

    let answer = commands::rule_based_answer("随便说点什么", &tasks, &config).await;
    assert!(answer.contains("可以试试"));
    assert!(answer.contains("现在该看什么"));
}

#[tokio::test]
async fn test_in_progress_task_priority() {
    let config = default_config();
    let tasks = vec![
        task("t-001", TaskStatus::Pending),
        task("t-002", TaskStatus::InProgress),
    ];

    let answer = commands::rule_based_answer("现在该看什么", &tasks, &config).await;
    assert!(answer.contains("t-002"));
}

#[tokio::test]
async fn test_list_pending_tasks() {
    let config = default_config();
    let tasks = vec![
        task("t-001", TaskStatus::Pending),
        task("t-002", TaskStatus::Passed),
        task("t-003", TaskStatus::Pending),
    ];

    let answer = commands::rule_based_answer("列出待办", &tasks, &config).await;
    assert!(answer.contains("t-001"));
    assert!(answer.contains("t-003"));
    assert!(!answer.contains("t-002"));
}

#[tokio::test]
async fn test_start_task() {
    let config = default_config();
    let tasks = vec![task("t-001", TaskStatus::Pending)];

    let answer = commands::rule_based_answer("第一个开始", &tasks, &config).await;
    assert!(answer.contains("🚀"));
    assert!(answer.contains("开始任务"));
    assert!(answer.contains("t-001"));
}

#[tokio::test]
async fn test_second_index() {
    let config = default_config();
    let tasks = vec![
        task("t-001", TaskStatus::Pending),
        task("t-002", TaskStatus::Pending),
        task("t-003", TaskStatus::Pending),
    ];

    let answer = commands::rule_based_answer("第二个过了", &tasks, &config).await;
    assert!(answer.contains("t-002"));
}

#[tokio::test]
async fn test_reject_specific_task() {
    let config = default_config();
    let tasks = vec![
        task("t-001", TaskStatus::Pending),
        task("t-002", TaskStatus::Pending),
    ];

    let answer = commands::rule_based_answer("拒绝第一个", &tasks, &config).await;
    assert!(answer.contains("❌"));
    assert!(answer.contains("t-001"));
}

#[tokio::test]
async fn test_no_task_in_time_range() {
    let _root = TestRoot::new();
    let config = default_config();
    let tasks = vec![
        task_with_time("t-001", TaskStatus::Pending, 30),
        task_with_time("t-002", TaskStatus::Pending, 45),
    ];

    for t in &tasks {
        crate::tasklist::create_task(crate::tasklist::TaskCreateInput {
            subject: t.id.clone(),
            kind: Some(t.kind.clone()),
            est_minutes: t.est_minutes,
            ..Default::default()
        })
        .await
        .expect("create task");
    }

    let answer = commands::rule_based_answer("我有 10 分钟", &tasks, &config).await;
    assert!(answer.contains("没有在 10 分钟内能完成的任务"));
}

#[tokio::test]
async fn send_answers_next_task_and_persists_history() {
    let _root = TestRoot::new();
    tasks::add(task("t-20260518-001", TaskStatus::Pending))
        .await
        .expect("add task");

    let reply = handler::send("现在该看什么".to_string())
        .await
        .expect("send chat");

    assert!(reply.message.content.contains("t-20260518-001"));
    let persisted = db::history(10).await.expect("history");
    assert_eq!(persisted.len(), 2);
    assert!(tokio::fs::try_exists(Paths::conductor_sqlite())
        .await
        .expect("sqlite exists"));
}

#[tokio::test]
async fn history_orders_by_seq_when_timestamps_match() {
    let _root = TestRoot::new();
    let pool = crate::db::pool().await.expect("pool");
    let created_at = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO chat_messages (id, role, content, created_at, seq, tool_calls)
        VALUES (?1, 'assistant', ?2, ?3, ?4, NULL)
        "#,
    )
    .bind("msg-second")
    .bind("second")
    .bind(&created_at)
    .bind(2_i64)
    .execute(&pool)
    .await
    .expect("insert second");
    sqlx::query(
        r#"
        INSERT INTO chat_messages (id, role, content, created_at, seq, tool_calls)
        VALUES (?1, 'user', ?2, ?3, ?4, NULL)
        "#,
    )
    .bind("msg-first")
    .bind("first")
    .bind(&created_at)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("insert first");

    let messages = db::history(10).await.expect("history");
    assert_eq!(messages[0].id, "msg-first");
    assert_eq!(messages[1].id, "msg-second");
}

#[tokio::test]
async fn send_status_command() {
    let _root = TestRoot::new();
    tasks::add(task("t-001", TaskStatus::Pending))
        .await
        .expect("add task");
    tasks::add(task("t-002", TaskStatus::Passed))
        .await
        .expect("add task");

    let reply = handler::send("现在状态".to_string())
        .await
        .expect("send chat");

    assert!(reply.message.content.contains("📊"));
    assert!(reply.message.content.contains("待处理"));
    assert!(reply.message.content.contains("已通过"));
}

#[tokio::test]
async fn send_help_command() {
    let _root = TestRoot::new();

    let reply = handler::send("帮助".to_string()).await.expect("send chat");

    assert!(reply.message.content.contains("清和"));
    assert!(reply.message.content.contains("任务助手"));
}

#[test]
fn chat_session_new_has_uuid_and_defaults() {
    let session =
        session::ChatSession::new(Some("ws-abc".to_string()), Some("run-123".to_string()));

    assert!(!session.id.is_empty());
    assert_eq!(session.workspace_id.as_deref(), Some("ws-abc"));
    assert_eq!(session.run_id.as_deref(), Some("run-123"));
    assert!(session.messages.is_empty());
    assert!(session.tool_records.is_empty());
}

#[test]
fn chat_session_new_with_none_ids() {
    let session = session::ChatSession::new(None, None);

    assert!(!session.id.is_empty());
    assert!(session.workspace_id.is_none());
    assert!(session.run_id.is_none());
}

#[test]
fn chat_session_summary_format() {
    let session = session::ChatSession::new(Some("ws-test".to_string()), None);
    let summary = session.summary();

    assert!(summary.contains("ChatSession"));
    assert!(summary.contains(&session.id));
    assert!(summary.contains("ws-test"));
    assert!(summary.contains("messages=0"));
    assert!(summary.contains("tools=0"));
}

#[test]
fn chat_session_serializes_to_json() {
    let session = session::ChatSession::new(None, None);
    let json = serde_json::to_string(&session).expect("serialize");
    let deserialized: session::ChatSession = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.id, session.id);
    assert_eq!(deserialized.workspace_id, session.workspace_id);
    assert_eq!(deserialized.run_id, session.run_id);
}

#[test]
fn truncate_tool_result_short_content_unchanged() {
    let content = "hello";
    assert_eq!(util::truncate_tool_result(content, 8192), "hello");
}

#[test]
fn truncate_tool_result_exact_boundary_unchanged() {
    let content = "a".repeat(8192);
    assert_eq!(util::truncate_tool_result(&content, 8192), content);
}

#[test]
fn truncate_tool_result_long_ascii_truncated() {
    let content = "x".repeat(10000);
    let result = util::truncate_tool_result(&content, 8192);
    assert!(result.ends_with("...(truncated)"));
    assert!(result.len() <= 8192);
}

#[test]
fn truncate_tool_result_cjk_boundary_safe() {
    // Each Chinese char is 3 bytes in UTF-8.
    // Build content that crosses the limit with multi-byte chars.
    let content = "\u{4e00}".repeat(5000); // 15000 bytes
    let result = util::truncate_tool_result(&content, 8192);
    assert!(result.ends_with("...(truncated)"));
    assert!(result.len() <= 8192);
    // Verify no replacement characters (would indicate broken UTF-8).
    assert!(!result.contains('\u{FFFD}'));
}

#[test]
fn truncate_tool_result_emoji_boundary_safe() {
    // Emojis are 4 bytes each.
    let content = "\u{1F600}".repeat(3000); // 12000 bytes
    let result = util::truncate_tool_result(&content, 8192);
    assert!(result.ends_with("...(truncated)"));
    assert!(result.len() <= 8192);
    assert!(!result.contains('\u{FFFD}'));
}

// -- TASK-049: Prompt assembly with memory context tests --

#[tokio::test]
async fn prompt_includes_memory_context_when_entries_exist() {
    let _root = TestRoot::new();
    crate::memory::init_db().await.expect("init memory db");
    crate::expression::init_db()
        .await
        .expect("init expression db");
    crate::affection::init_db()
        .await
        .expect("init affection db");
    crate::memory::set_embedding_model(Box::new(crate::memory::HashEmbeddingModel::default()));

    // Store a memory entry
    crate::memory::set("fav_color", "my favorite color is blue", "preference")
        .await
        .expect("set memory");

    let config = default_config();
    let tasks: Vec<Task> = vec![];
    let prompt =
        prompt::build_system_prompt(&tasks, &config, "what is my favorite color", None).await;

    assert!(
        prompt.contains("## 记忆上下文"),
        "prompt should contain memory context section when memories exist"
    );
    assert!(
        prompt.contains("fav_color"),
        "prompt should contain the memory key"
    );
    assert!(
        prompt.contains("blue"),
        "prompt should contain the memory value"
    );
}

#[tokio::test]
async fn prompt_includes_conversation_summaries() {
    let _root = TestRoot::new();
    crate::memory::init_db().await.expect("init memory db");
    crate::expression::init_db()
        .await
        .expect("init expression db");
    crate::affection::init_db()
        .await
        .expect("init affection db");
    crate::memory::set_embedding_model(Box::new(crate::memory::HashEmbeddingModel::default()));

    // Store a conversation summary
    crate::memory::add_conversation_summary(
        "We discussed the new authentication module design",
        &["auth".to_string(), "design".to_string()],
    )
    .await
    .expect("add summary");

    let config = default_config();
    let tasks: Vec<Task> = vec![];
    let prompt = prompt::build_system_prompt(&tasks, &config, "authentication module", None).await;

    assert!(
        prompt.contains("## 记忆上下文"),
        "prompt should contain memory context section"
    );
    assert!(
        prompt.contains("### 近期对话"),
        "prompt should contain conversation summaries subsection"
    );
    assert!(
        prompt.contains("authentication"),
        "prompt should contain summary content"
    );
}

#[tokio::test]
async fn prompt_skips_memory_context_when_empty() {
    let _root = TestRoot::new();
    crate::memory::init_db().await.expect("init memory db");
    crate::expression::init_db()
        .await
        .expect("init expression db");
    crate::affection::init_db()
        .await
        .expect("init affection db");
    crate::memory::set_embedding_model(Box::new(crate::memory::HashEmbeddingModel::default()));

    // No memories stored

    let config = default_config();
    let tasks: Vec<Task> = vec![];
    let prompt = prompt::build_system_prompt(&tasks, &config, "hello world", None).await;

    assert!(
        !prompt.contains("## 记忆上下文"),
        "prompt should not contain memory context section when no memories exist"
    );
}

#[tokio::test]
async fn prompt_includes_multiple_memory_entries() {
    let _root = TestRoot::new();
    crate::memory::init_db().await.expect("init memory db");
    crate::expression::init_db()
        .await
        .expect("init expression db");
    crate::affection::init_db()
        .await
        .expect("init affection db");
    crate::memory::set_embedding_model(Box::new(crate::memory::HashEmbeddingModel::default()));

    // Store entries in different categories
    crate::memory::set("lang_pref", "prefer rust over python", "coding")
        .await
        .expect("set coding memory");
    crate::memory::set("food_pref", "likes spicy food", "food")
        .await
        .expect("set food memory");

    let config = default_config();
    let tasks: Vec<Task> = vec![];
    let prompt = prompt::build_system_prompt(&tasks, &config, "rust python spicy food", None).await;

    assert!(
        prompt.contains("## 记忆上下文"),
        "prompt should contain memory context section"
    );
    assert!(
        prompt.contains("lang_pref"),
        "prompt should contain the coding memory key"
    );
    assert!(
        prompt.contains("food_pref"),
        "prompt should contain the food memory key"
    );
}

#[tokio::test]
async fn prompt_uses_workspace_scoped_memory_context() {
    let _root = TestRoot::new();
    crate::memory::init_db().await.expect("init memory db");
    crate::expression::init_db()
        .await
        .expect("init expression db");
    crate::affection::init_db()
        .await
        .expect("init affection db");
    crate::memory::set_embedding_model(Box::new(crate::memory::HashEmbeddingModel::default()));

    crate::memory::set_for_workspace(
        "auth_module",
        "workspace alpha authentication guidance",
        "project",
        "ws-alpha",
        "user",
    )
    .await
    .expect("set workspace alpha memory");
    crate::memory::set_for_workspace(
        "auth_module",
        "workspace beta authentication guidance",
        "project",
        "ws-beta",
        "user",
    )
    .await
    .expect("set workspace beta memory");

    let config = default_config();
    let tasks: Vec<Task> = vec![];
    let alpha_prompt = prompt::build_system_prompt_with_context(
        &tasks,
        &config,
        "authentication guidance",
        None,
        Some("ws-alpha"),
        Some("crates/conductor-core/src/chat"),
        None,
        None,
    )
    .await;
    let beta_prompt = prompt::build_system_prompt_with_context(
        &tasks,
        &config,
        "authentication guidance",
        None,
        Some("ws-beta"),
        Some("apps/desktop/src"),
        None,
        None,
    )
    .await;

    assert!(
        alpha_prompt.contains("workspace alpha authentication guidance"),
        "alpha workspace prompt should include alpha-scoped memory"
    );
    assert!(
        !alpha_prompt.contains("workspace beta authentication guidance"),
        "alpha workspace prompt should exclude beta-scoped memory"
    );
    assert!(
        beta_prompt.contains("workspace beta authentication guidance"),
        "beta workspace prompt should include beta-scoped memory"
    );
}

#[tokio::test]
async fn prompt_uses_session_and_goal_scoped_memory_context() {
    let _root = TestRoot::new();
    crate::memory::init_db().await.expect("init memory db");
    crate::expression::init_db()
        .await
        .expect("init expression db");
    crate::affection::init_db()
        .await
        .expect("init affection db");
    crate::memory::set_embedding_model(Box::new(crate::memory::HashEmbeddingModel::default()));

    let matching = crate::memory::set(
        "prompt_scope_matching",
        "amber falcon prompt memory from matching session",
        "project",
    )
    .await
    .expect("set matching memory");
    let other = crate::memory::set(
        "prompt_scope_other",
        "amber falcon prompt memory from other session",
        "project",
    )
    .await
    .expect("set other memory");

    let pool = crate::db::pool().await.expect("pool");
    sqlx::query(
        r#"
        UPDATE memory_entries
        SET source_session_id = ?1, goal_id = ?2
        WHERE id = ?3
        "#,
    )
    .bind("session-a")
    .bind("goal-a")
    .bind(&matching.id)
    .execute(&pool)
    .await
    .expect("tag matching memory");
    sqlx::query(
        r#"
        UPDATE memory_entries
        SET source_session_id = ?1, goal_id = ?2
        WHERE id = ?3
        "#,
    )
    .bind("session-b")
    .bind("goal-b")
    .bind(&other.id)
    .execute(&pool)
    .await
    .expect("tag other memory");

    let config = default_config();
    let tasks: Vec<Task> = vec![];
    let prompt = prompt::build_system_prompt_with_context(
        &tasks,
        &config,
        "amber falcon prompt memory",
        None,
        None,
        None,
        Some("session-a"),
        Some("goal-a"),
    )
    .await;

    assert!(
        prompt.contains("amber falcon prompt memory from matching session"),
        "prompt should include memory from the active session and goal"
    );
    assert!(
        !prompt.contains("amber falcon prompt memory from other session"),
        "prompt should exclude memory from another explicit session and goal"
    );
}

// ── TASK-107: PolicyEngine integration in build_tool_definitions ──

/// Register a dummy tool in the global registry for testing.
fn register_test_tool(tool_id: &str) {
    register_tool(
        crate::tools::ToolSpec {
            id: tool_id.to_string(),
            name: tool_id.to_string(),
            description: format!("test tool {tool_id}"),
            provider: crate::tools::ToolProviderKind::Internal,
            input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            risk_level: crate::proposals::RiskLevel::ReadOnly,
            permissions: vec![],
            supports_dry_run: false,
            workspace_required: false,
        },
        |_spec, _input| {
            Ok(crate::tools::ToolExecutionResult {
                success: true,
                output: serde_json::json!({}),
                error: None,
                duration_ms: 0,
            })
        },
    );
}

fn make_test_connector(
    id: &str,
    enabled: bool,
    auth: ConnectorAuthStatus,
    caps: Vec<ConnectorCapability>,
) -> ConnectorSpec {
    ConnectorSpec {
        id: id.to_string(),
        name: format!("Connector {id}"),
        description: format!("test connector {id}"),
        implementation_type: ConnectorImplementation::NativeRust,
        capabilities: caps,
        auth_status: auth,
        enabled,
        config_json: None,
    }
}

fn make_test_cap(
    capability: &str,
    tools: &[&str],
    risk: &str,
    confirm: bool,
) -> ConnectorCapability {
    ConnectorCapability {
        capability: capability.to_string(),
        tools: tools.iter().map(|t| t.to_string()).collect(),
        risk_level: risk.to_string(),
        requires_confirmation: confirm,
    }
}

fn make_test_skill(id: &str, keywords: Vec<&str>, caps: Vec<&str>) -> SkillPackage {
    SkillPackage {
        id: id.to_string(),
        name: format!("Skill {id}"),
        version: "1.0.0".to_string(),
        description: format!("test skill {id}"),
        author: None,
        activation: SkillActivation {
            keywords: keywords.into_iter().map(String::from).collect(),
            apps: vec![],
            url_patterns: vec![],
            file_patterns: vec![],
        },
        capabilities: caps.into_iter().map(String::from).collect(),
        source: SkillSource::Builtin,
        enabled: true,
        body: String::new(),
    }
}

/// New path: PolicyEngine approves tools → they appear in the result.
#[tokio::test]
async fn build_tool_definitions_new_path_policy_engine_passes() {
    let _root = TestRoot::new();

    // Register a test tool in the global tool registry.
    register_test_tool("policy.approved.tool");

    // Register a connector that provides capability "test.read" → tool "policy.approved.tool".
    let connector = make_test_connector(
        "test-conn",
        true,
        ConnectorAuthStatus::Authenticated,
        vec![make_test_cap(
            "test.read",
            &["policy.approved.tool"],
            "low",
            false,
        )],
    );
    ConnectorRegistry::register(connector).await.unwrap();

    // Import a skill with capability "test.read" that matches the prompt.
    let _skill = make_test_skill("reader_skill", vec!["read", "document"], vec!["test.read"]);
    crate::skills::import_skill_markdown(&format!(
        r#"---
id: reader_skill
name: Skill reader_skill
version: "1.0.0"
description: test skill reader_skill
activation:
  keywords:
    - read
    - document
capabilities:
  - test.read
---
body"#
    ))
    .await
    .unwrap();
    // Enable the skill
    crate::skills::update_skill_enabled("reader_skill", true)
        .await
        .unwrap();

    let config = CoreConfig {
        llm: crate::config::LlmConfig {
            allowed_tool_ids: vec![], // no legacy allowed tools
            ..Default::default()
        },
        ..Default::default()
    };

    let defs = tools::build_tool_definitions(
        &config,
        "please read this document",
        crate::chat::ChatTaskMode::Short,
        false,
    )
    .await;

    // The PolicyEngine-approved tool should be present.
    let names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();
    assert!(
        names.contains(&"policy__approved__tool"),
        "expected policy.approved.tool in results, got: {names:?}"
    );
}

#[tokio::test]
async fn build_tool_definitions_soft_exposes_connector_tools_when_auth_not_ready() {
    let _root = TestRoot::new();

    register_test_tool("policy.auth.pending.tool");

    let connector = make_test_connector(
        "pending-auth-conn",
        true,
        ConnectorAuthStatus::NotConfigured,
        vec![make_test_cap(
            "test.pending_auth",
            &["policy.auth.pending.tool"],
            "low",
            false,
        )],
    );
    ConnectorRegistry::register(connector).await.unwrap();

    crate::skills::import_skill_markdown(
        r#"---
id: pending_auth_skill
name: Pending Auth Skill
version: "1.0.0"
description: exposes tools before connector login
activation:
  keywords:
    - pending
    - auth
capabilities:
  - test.pending_auth
---
body"#,
    )
    .await
    .unwrap();
    crate::skills::update_skill_enabled("pending_auth_skill", true)
        .await
        .unwrap();

    let config = CoreConfig {
        llm: crate::config::LlmConfig {
            allowed_tool_ids: vec![],
            ..Default::default()
        },
        ..Default::default()
    };

    let defs = tools::build_tool_definitions(
        &config,
        "pending auth document lookup",
        crate::chat::ChatTaskMode::Short,
        false,
    )
    .await;

    let names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();
    assert!(
        names.contains(&"policy__auth__pending__tool"),
        "expected pending-auth connector tool to be exposed, got: {names:?}"
    );
}

/// Legacy fallback: PolicyEngine returns empty AND allowed_tool_ids is non-empty → legacy path.
#[tokio::test]
async fn build_tool_definitions_legacy_fallback_when_policy_engine_empty() {
    let _root = TestRoot::new();

    // Register a test tool.
    register_test_tool("legacy.tool");

    // No connectors/skills registered → PolicyEngine returns empty.
    // But allowed_tool_ids is non-empty → legacy fallback.
    let config = CoreConfig {
        llm: crate::config::LlmConfig {
            allowed_tool_ids: vec!["legacy.tool".to_string()],
            ..Default::default()
        },
        ..Default::default()
    };

    let defs = tools::build_tool_definitions(
        &config,
        "unrelated prompt",
        crate::chat::ChatTaskMode::Short,
        false,
    )
    .await;

    let names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();
    assert!(
        names.contains(&"legacy__tool"),
        "expected legacy.tool in legacy fallback results, got: {names:?}"
    );
}

/// Empty PolicyEngine result AND no allowed_tool_ids → empty result.
#[tokio::test]
async fn build_tool_definitions_empty_when_no_policy_and_no_legacy() {
    let _root = TestRoot::new();

    // No connectors, no skills, no allowed_tool_ids.
    let config = CoreConfig {
        llm: crate::config::LlmConfig {
            allowed_tool_ids: vec![],
            ..Default::default()
        },
        ..Default::default()
    };

    let defs = tools::build_tool_definitions(
        &config,
        "random prompt",
        crate::chat::ChatTaskMode::Short,
        false,
    )
    .await;

    assert!(
        defs.is_empty(),
        "expected empty definitions when no policy results and no allowed_tool_ids, got {} tools",
        defs.len()
    );
}
