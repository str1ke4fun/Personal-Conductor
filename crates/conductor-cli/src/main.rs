mod agent_run;

use anyhow::Context;
use chrono::Utc;
use clap::{Parser, Subcommand};
use conductor_core::{
    agent_runs, agent_teams, chat, events, filewatch,
    paths::Paths,
    projection::ProjectionWriter,
    proposals::{self, Proposal, ProposalSource, ProposalStatus, RiskLevel},
    subagent,
    tasks::{self, Artifact, Task, TaskStatus},
    tools, transcript,
};
use serde_json::Value;
use std::{path::Path, path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};

#[derive(Parser)]
#[command(name = "conductor")]
#[command(about = "Personal Task Manager CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Hook {
        #[command(subcommand)]
        command: HookCommand,
    },
    List {
        #[arg(long)]
        all: bool,
    },
    Show {
        id: String,
    },
    Pass {
        id: String,
    },
    Skip {
        id: String,
    },
    Reject {
        id: String,
    },
    Migrate,
    Proposal {
        #[command(subcommand)]
        command: ProposalCommand,
    },
    Sub {
        #[command(subcommand)]
        command: SubCommand,
    },
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    Team {
        #[command(subcommand)]
        command: TeamCommand,
    },
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    Chat {
        #[arg(long)]
        interactive: bool,
        message: Option<String>,
    },
}

#[derive(Clone, clap::ValueEnum)]
enum HookSource {
    Claude,
    Codex,
}

#[derive(Subcommand)]
enum HookCommand {
    Enable {
        #[arg(long, value_enum, default_value = "claude")]
        source: HookSource,
    },
    SessionStart,
    Stop,
    StopFailure,
    SubagentStart,
    SubagentStop,
    UserPromptSubmit,
    Notification,
    PermissionRequest,
    PermissionDenied,
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    Codex {
        #[command(subcommand)]
        command: CodexHookCommand,
    },
}

#[derive(Subcommand)]
enum CodexHookCommand {
    UserPromptSubmit,
    Stop,
    PermissionRequest,
    PostToolUse,
}

#[derive(Subcommand)]
enum ProposalCommand {
    List,
    Show {
        id: String,
    },
    Approve {
        id: String,
    },
    Execute {
        id: String,
    },
    Reject {
        id: String,
    },
    Create {
        #[arg(long)]
        for_cwd: PathBuf,
        #[arg(long)]
        content: String,
        #[arg(long)]
        reason: String,
    },
}

#[derive(Subcommand)]
enum SubCommand {
    Run {
        prompt: String,
        #[arg(long)]
        cwd: Option<PathBuf>,
        #[arg(long, default_value = "300")]
        timeout_seconds: u64,
    },
}

#[derive(Subcommand)]
enum AgentCommand {
    List {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        workspace_id: Option<String>,
    },
    Read {
        run_id: String,
        #[arg(long, default_value = "16384")]
        max_bytes: usize,
    },
    Stop {
        run_id: String,
    },
    Run {
        #[arg(long)]
        runtime_url: Option<String>,
        #[arg(long)]
        token: Option<String>,
        #[arg(long, default_value = "agent-runner")]
        agent_id: String,
        #[arg(long)]
        workspace_id: Option<String>,
        #[arg(long, default_value_t = 900)]
        lease_ttl_seconds: i64,
        #[arg(long, default_value_t = 2_000)]
        poll_interval_ms: u64,
        #[arg(long, default_value = "claude")]
        claude_binary: String,
        #[arg(long, default_value_t = 600)]
        timeout_seconds: u64,
        #[arg(long)]
        once: bool,
    },
}

#[derive(Subcommand)]
enum TeamCommand {
    List {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        workspace_id: Option<String>,
    },
    Create {
        name: String,
        #[arg(long)]
        workspace_id: Option<String>,
    },
    AddMember {
        team_id: String,
        agent_id: String,
        #[arg(long, default_value = "assistant")]
        role: String,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long)]
        cwd: Option<PathBuf>,
    },
    Snapshot {
        team_id: String,
    },
    Send {
        team_id: String,
        content: String,
        #[arg(long, default_value = "conductor")]
        sender: String,
        #[arg(long)]
        recipient: Option<String>,
        #[arg(long)]
        broadcast: bool,
    },
    Read {
        team_id: String,
        #[arg(long)]
        recipient: Option<String>,
        #[arg(long)]
        include_read: bool,
    },
    MarkRead {
        message_id: String,
    },
}

#[derive(Subcommand)]
enum WorkspaceCommand {
    ShowProjection {
        #[arg(long, default_value = "default")]
        workspace_id: String,
    },
    WriteProjection {
        #[arg(long, default_value = "default")]
        workspace_id: String,
    },
}

#[tokio::main]
async fn main() {
    if let Err(err) = init_logging().await {
        eprintln!("failed to initialize logging: {err:#}");
    }

    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Hook { command } => {
            run_hook_command(command).await;
            return;
        }
        Commands::List { all } => run_list(all).await,
        Commands::Show { id } => run_show(&id).await,
        Commands::Pass { id } => set_status(&id, TaskStatus::Passed).await,
        Commands::Skip { id } => set_status(&id, TaskStatus::Skipped).await,
        Commands::Reject { id } => set_status(&id, TaskStatus::Rejected).await,
        Commands::Migrate => run_migrate().await,
        Commands::Proposal { command } => run_proposal(command).await,
        Commands::Sub { command } => run_sub(command).await,
        Commands::Agent { command } => run_agent(command).await,
        Commands::Team { command } => run_team(command).await,
        Commands::Workspace { command } => run_workspace(command).await,
        Commands::Chat {
            interactive,
            message,
        } => run_chat(interactive, message).await,
    };

    if let Err(err) = result {
        tracing::error!(error = ?err, "{err:#}");
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

async fn init_logging() -> anyhow::Result<()> {
    if let Some(parent) = Paths::on_stop_log().parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(Paths::on_stop_log())?;
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "conductor=info,warn".into()),
        )
        .with_ansi(false)
        .try_init()
        .ok();
    Ok(())
}

async fn append_inject_log(message: &str) -> anyhow::Result<()> {
    if let Some(parent) = Paths::inject_log().parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(Paths::inject_log())
        .await?;
    file.write_all(format!("{} {message}\n", Utc::now().to_rfc3339()).as_bytes())
        .await?;
    Ok(())
}

async fn run_hook_command(command: HookCommand) {
    let result = tokio::spawn(async move {
        match command {
            HookCommand::Enable { source } => run_hook_enable(source).await,
            HookCommand::SessionStart => run_hook_attention("session_start").await,
            HookCommand::Stop => run_hook_stop().await,
            HookCommand::StopFailure => run_hook_completion("stop_failure").await,
            HookCommand::SubagentStart => run_hook_attention("subagent_start").await,
            HookCommand::SubagentStop => run_hook_completion("subagent_stop").await,
            HookCommand::UserPromptSubmit => run_hook_user_prompt_submit().await,
            HookCommand::Notification => run_hook_attention("notification").await,
            HookCommand::PermissionRequest => run_hook_attention("permission_request").await,
            HookCommand::PermissionDenied => run_hook_attention("permission_denied").await,
            HookCommand::PreToolUse => run_hook_tool_event("pre_tool_use").await,
            HookCommand::PostToolUse => run_hook_tool_event("post_tool_use").await,
            HookCommand::PostToolUseFailure => run_hook_tool_event("post_tool_use_failure").await,
            HookCommand::Codex { command } => run_codex_hook_command(command).await,
        }
    })
    .await;

    match result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            tracing::error!(error = ?err, "{err:#}");
        }
        Err(err) => {
            tracing::error!(error = ?err, "hook task panicked or was cancelled");
        }
    }
}

async fn run_hook_enable(source: HookSource) -> anyhow::Result<()> {
    let exe = std::env::current_exe().context("failed to get current executable path")?;
    let exe_str = exe.display().to_string().replace('\\', "/");
    let cwd = std::env::current_dir().context("failed to get current working directory")?;

    match source {
        HookSource::Claude => {
            let dir = cwd.join(".claude");
            tokio::fs::create_dir_all(&dir)
                .await
                .context("failed to create .claude directory")?;

            let hooks = build_claude_hooks(&exe_str);
            let settings_path = dir.join("settings.json");

            let existing = if settings_path.exists() {
                let content = tokio::fs::read_to_string(&settings_path)
                    .await
                    .context("failed to read existing settings.json")?;
                serde_json::from_str::<Value>(&content).ok()
            } else {
                None
            };

            let merged = match existing {
                Some(mut obj) => {
                    obj.as_object_mut().map(|o| o.insert("hooks".into(), hooks));
                    obj
                }
                None => serde_json::json!({ "hooks": hooks }),
            };

            let output = serde_json::to_string_pretty(&merged)
                .context("failed to serialize settings.json")?;
            tokio::fs::write(&settings_path, output.as_bytes())
                .await
                .context("failed to write settings.json")?;

            println!("\u{2713} Claude Code hooks configured in .claude/settings.json");
        }
        HookSource::Codex => {
            let dir = cwd.join(".codex");
            tokio::fs::create_dir_all(&dir)
                .await
                .context("failed to create .codex directory")?;

            let hooks = build_codex_hooks(&exe_str);
            let hooks_path = dir.join("hooks.json");

            let output =
                serde_json::to_string_pretty(&hooks).context("failed to serialize hooks.json")?;
            tokio::fs::write(&hooks_path, output.as_bytes())
                .await
                .context("failed to write hooks.json")?;

            println!("\u{2713} Codex hooks configured in .codex/hooks.json");
        }
    }

    Ok(())
}

fn build_claude_hooks(exe: &str) -> Value {
    let events = [
        ("SessionStart", "session-start"),
        ("Stop", "stop"),
        ("StopFailure", "stop-failure"),
        ("SubagentStart", "subagent-start"),
        ("SubagentStop", "subagent-stop"),
        ("UserPromptSubmit", "user-prompt-submit"),
        ("Notification", "notification"),
        ("PermissionRequest", "permission-request"),
        ("PermissionDenied", "permission-denied"),
        ("PreToolUse", "pre-tool-use"),
        ("PostToolUse", "post-tool-use"),
        ("PostToolUseFailure", "post-tool-use-failure"),
    ];

    let mut hooks = serde_json::Map::new();
    for (event, subcmd) in &events {
        hooks.insert(
            event.to_string(),
            serde_json::json!([{
                "hooks": [{
                    "type": "command",
                    "command": format!("{exe} hook {subcmd}"),
                    "timeout": 5
                }]
            }]),
        );
    }
    Value::Object(hooks)
}

fn build_codex_hooks(exe: &str) -> Value {
    serde_json::json!([
        {
            "event": "UserPromptSubmit",
            "command": format!("{exe} hook codex user-prompt-submit"),
            "timeout": 5
        },
        {
            "event": "PermissionRequest",
            "matcher": "*",
            "command": format!("{exe} hook codex permission-request"),
            "timeout": 5
        },
        {
            "event": "PostToolUse",
            "matcher": "Bash|apply_patch|Edit|Write|mcp__.*",
            "command": format!("{exe} hook codex post-tool-use"),
            "timeout": 5
        },
        {
            "event": "Stop",
            "command": format!("{exe} hook codex stop"),
            "timeout": 5
        }
    ])
}

async fn run_hook_stop() -> anyhow::Result<()> {
    run_hook_completion("stop").await
}

async fn run_hook_user_prompt_submit() -> anyhow::Result<()> {
    run_hook_user_prompt_submit_with_source("claude").await
}

async fn run_hook_user_prompt_submit_with_source(source: &str) -> anyhow::Result<()> {
    let Some(payload) = read_hook_payload("user_prompt_submit").await? else {
        return Ok(());
    };
    let identity = HookIdentity::from_payload(&payload);
    if identity.cwd.is_none() {
        let _ = append_inject_log("missing cwd field").await;
        return Ok(());
    }

    let user_message = extract_user_message(&payload);
    let task = create_or_update_task_for_session(&identity, &user_message, source).await;

    if let Err(e) = events::append(source, "user_prompt_submit", &payload).await {
        tracing::warn!(error = %e, "failed to append user_prompt_submit event");
    }

    tracing::info!(
        source,
        session_id = ?identity.session_id.as_deref(),
        cwd = ?identity.cwd.as_ref().map(|path| path.display().to_string()),
        task_id = ?task.as_ref().map(|t| t.id.as_str()),
        "UserPromptSubmit hook completed"
    );

    Ok(())
}

#[derive(Clone, Debug, Default)]
struct HookIdentity {
    session_id: Option<String>,
    terminal_id: Option<String>,
    cwd: Option<PathBuf>,
    transcript_path: Option<PathBuf>,
}

impl HookIdentity {
    fn from_payload(payload: &Value) -> Self {
        Self {
            session_id: first_string(payload, &["session_id", "sessionId", "session.id"]),
            terminal_id: first_string(
                payload,
                &[
                    "terminal_id",
                    "terminalId",
                    "terminal_hint",
                    "terminalHint",
                    "process_id",
                    "pid",
                ],
            ),
            cwd: first_string(payload, &["cwd", "workspace", "workspace_root"])
                .map(|value| normalize_path(Path::new(&value))),
            transcript_path: first_string(payload, &["transcript_path", "transcriptPath"])
                .map(PathBuf::from),
        }
    }
}

async fn read_hook_payload(kind: &str) -> anyhow::Result<Option<Value>> {
    let mut buf = String::new();
    tokio::io::stdin().read_to_string(&mut buf).await?;

    if buf.trim().is_empty() {
        tracing::warn!(kind, "received empty stdin payload for hook");
        return Ok(None);
    }

    match serde_json::from_str(&buf) {
        Ok(v) => Ok(Some(v)),
        Err(e) => {
            tracing::error!(error = %e, payload = %buf, kind, "failed to parse hook payload");
            Ok(None)
        }
    }
}

async fn run_hook_completion(kind: &str) -> anyhow::Result<()> {
    run_hook_completion_with_source(kind, "claude").await
}

async fn run_hook_completion_with_source(kind: &str, source: &str) -> anyhow::Result<()> {
    let Some(payload) = read_hook_payload(kind).await? else {
        return Ok(());
    };
    let identity = HookIdentity::from_payload(&payload);
    let Some(cwd) = identity.cwd.as_deref() else {
        tracing::warn!(kind, "completion hook missing cwd field");
        let _ = events::append(source, kind, &payload).await;
        return Ok(());
    };

    let recent = match timeout(
        Duration::from_secs(2),
        filewatch::recently_modified(cwd, Duration::from_secs(5 * 60)),
    )
    .await
    {
        Ok(Ok(files)) => files,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, kind, "failed to read recent files");
            Vec::new()
        }
        Err(_) => {
            tracing::warn!(kind, "recent file scan timed out");
            Vec::new()
        }
    };

    let tail = if let Some(tp) = identity.transcript_path.as_deref() {
        match timeout(Duration::from_secs(1), transcript::read_tail(tp, 10)).await {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, kind, "failed to read transcript tail");
                Vec::new()
            }
            Err(_) => {
                tracing::warn!(kind, "transcript tail read timed out");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    let output_summary = build_completion_summary(&payload, &tail, &recent, kind, source);
    let summary_ref = write_fast_summary(
        kind,
        &identity,
        &payload,
        &tail,
        &recent,
        &output_summary,
        source,
    )
    .await
    .ok();

    let existing_tasks = tasks::load().await?;
    let now = Utc::now();
    let display = source_display(source);

    if let Some(existing_task) = find_matching_task(&existing_tasks.tasks, &identity, false, source)
    {
        let task_id = existing_task.id.clone();
        tasks::update(&task_id, |task| {
            task.status = TaskStatus::Pending;
            task.artifact = Artifact {
                file: recent.first().cloned(),
                anchor: None,
            };
            if summary_ref.is_some() {
                task.summary_ref = summary_ref.clone();
            }
            task.focus_hint = Some(format!(
                "Review {}'s latest output and confirm it matches the request",
                display
            ));
            task.session_id = identity
                .session_id
                .clone()
                .or_else(|| task.session_id.clone());
            task.terminal_id = identity
                .terminal_id
                .clone()
                .or_else(|| task.terminal_id.clone());
            task.cwd = identity.cwd.clone().or_else(|| task.cwd.clone());
            task.last_output_summary = Some(output_summary.clone());
            task.last_event_at = Some(now);
            let permission_summary = extract_permission_summary(&payload);
            if permission_summary.is_some() {
                task.permission_summary = permission_summary;
            }
        })
        .await?;

        tracing::info!(task_id = %task_id, kind, "updated existing task to pending");
    } else {
        let task = Task {
            id: match tasks::next_id().await {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!(error = %e, "failed to generate task id");
                    return Ok(());
                }
            },
            source: source.into(),
            kind: "review-doc".into(),
            artifact: Artifact {
                file: recent.first().cloned(),
                anchor: None,
            },
            summary_ref,
            est_minutes: None,
            focus_hint: Some(format!(
                "Review {}'s latest output and confirm it matches the request",
                display
            )),
            status: TaskStatus::Pending,
            created_at: now,
            session_id: identity.session_id.clone(),
            terminal_id: identity.terminal_id.clone(),
            cwd: identity.cwd.clone(),
            current_request: None,
            last_output_summary: Some(output_summary),
            last_event_at: Some(now),
            permission_summary: extract_permission_summary(&payload),
        };

        if let Err(e) = tasks::add(task).await {
            tracing::error!(error = %e, "failed to add task");
            return Ok(());
        }

        tracing::info!(kind, session_id = ?identity.session_id.as_deref(), "created new pending task");
    }

    if let Err(e) = events::append(source, kind, &payload).await {
        tracing::warn!(error = %e, kind, "failed to append completion event");
    }

    Ok(())
}

async fn run_hook_attention(kind: &str) -> anyhow::Result<()> {
    run_hook_attention_with_source(kind, "claude").await
}

async fn run_hook_attention_with_source(kind: &str, source: &str) -> anyhow::Result<()> {
    let Some(payload) = read_hook_payload(kind).await? else {
        return Ok(());
    };
    let identity = HookIdentity::from_payload(&payload);
    let display = source_display(source);
    let note = extract_permission_summary(&payload)
        .or_else(|| first_string(&payload, &["message", "notification", "text"]))
        .unwrap_or_else(|| format!("{display} emitted {kind}"));
    update_permission_context(&identity, note, false, source).await?;

    if let Err(e) = events::append(source, kind, &payload).await {
        tracing::warn!(error = %e, kind, "failed to append attention event");
    }
    Ok(())
}

async fn run_hook_tool_event(kind: &str) -> anyhow::Result<()> {
    run_hook_tool_event_with_source(kind, "claude").await
}

async fn run_hook_tool_event_with_source(kind: &str, source: &str) -> anyhow::Result<()> {
    let Some(payload) = read_hook_payload(kind).await? else {
        return Ok(());
    };
    let identity = HookIdentity::from_payload(&payload);
    let note = extract_permission_summary(&payload)
        .unwrap_or_else(|| summarize_tool_event(kind, &payload));
    update_permission_context(&identity, note, false, source).await?;

    if let Err(e) = events::append(source, kind, &payload).await {
        tracing::warn!(error = %e, kind, "failed to append tool event");
    }
    Ok(())
}

async fn run_codex_hook_command(command: CodexHookCommand) -> anyhow::Result<()> {
    match command {
        CodexHookCommand::UserPromptSubmit => {
            run_hook_user_prompt_submit_with_source("codex").await
        }
        CodexHookCommand::Stop => run_hook_completion_with_source("stop", "codex").await,
        CodexHookCommand::PermissionRequest => {
            run_hook_attention_with_source("permission_request", "codex").await
        }
        CodexHookCommand::PostToolUse => {
            run_hook_tool_event_with_source("post_tool_use", "codex").await
        }
    }
}

async fn update_permission_context(
    identity: &HookIdentity,
    note: String,
    mark_pending: bool,
    source: &str,
) -> anyhow::Result<()> {
    let existing_tasks = tasks::load().await?;
    if let Some(existing_task) = find_matching_task(&existing_tasks.tasks, identity, false, source)
    {
        let task_id = existing_task.id.clone();
        let now = Utc::now();
        tasks::update(&task_id, |task| {
            task.status = if mark_pending {
                TaskStatus::Pending
            } else {
                TaskStatus::InProgress
            };
            task.permission_summary = Some(note.clone());
            task.last_event_at = Some(now);
            task.session_id = identity
                .session_id
                .clone()
                .or_else(|| task.session_id.clone());
            task.terminal_id = identity
                .terminal_id
                .clone()
                .or_else(|| task.terminal_id.clone());
            task.cwd = identity.cwd.clone().or_else(|| task.cwd.clone());
        })
        .await?;
    } else if has_task_identity(identity) {
        let now = Utc::now();
        let task = Task {
            id: tasks::next_id().await?,
            source: source.into(),
            kind: "review-doc".into(),
            artifact: Artifact {
                file: None,
                anchor: None,
            },
            summary_ref: None,
            est_minutes: None,
            focus_hint: Some(build_session_focus_hint(identity, source)),
            status: if mark_pending {
                TaskStatus::Pending
            } else {
                TaskStatus::InProgress
            },
            created_at: now,
            session_id: identity.session_id.clone(),
            terminal_id: identity.terminal_id.clone(),
            cwd: identity.cwd.clone(),
            current_request: None,
            last_output_summary: None,
            last_event_at: Some(now),
            permission_summary: Some(note),
        };
        tasks::add(task).await?;
    }
    Ok(())
}

fn has_task_identity(identity: &HookIdentity) -> bool {
    identity
        .session_id
        .as_deref()
        .is_some_and(|value| !value.is_empty())
        || (identity
            .terminal_id
            .as_deref()
            .is_some_and(|value| !value.is_empty())
            && identity.cwd.is_some())
}

async fn create_or_update_task_for_session(
    identity: &HookIdentity,
    user_message: &str,
    source: &str,
) -> anyhow::Result<Task> {
    let existing_tasks = tasks::load().await?;
    let now = Utc::now();

    let user_prompt_summary = if user_message.is_empty() {
        "User submitted a request".to_string()
    } else {
        user_message.chars().take(100).collect()
    };

    let kind = "review-doc".to_string();
    let focus_hint = Some(build_session_focus_hint(identity, source));

    if let Some(existing_task) = find_matching_task(&existing_tasks.tasks, identity, false, source)
    {
        let task_id = existing_task.id.clone();
        tasks::update(&task_id, |task| {
            task.kind = kind;
            task.focus_hint = focus_hint;
            task.current_request = Some(user_prompt_summary.clone());
            task.session_id = identity
                .session_id
                .clone()
                .or_else(|| task.session_id.clone());
            task.terminal_id = identity
                .terminal_id
                .clone()
                .or_else(|| task.terminal_id.clone());
            task.cwd = identity.cwd.clone().or_else(|| task.cwd.clone());
            task.last_event_at = Some(now);
            task.status = TaskStatus::InProgress;
        })
        .await?;

        let mut updated = existing_task.clone();
        updated.kind = "review-doc".into();
        updated.focus_hint = Some(build_session_focus_hint(identity, source));
        updated.current_request = Some(user_prompt_summary);
        updated.session_id = identity.session_id.clone().or(updated.session_id);
        updated.terminal_id = identity.terminal_id.clone().or(updated.terminal_id);
        updated.cwd = identity.cwd.clone().or(updated.cwd);
        updated.last_event_at = Some(now);
        updated.status = TaskStatus::InProgress;
        Ok(updated)
    } else {
        let task = Task {
            id: tasks::next_id().await?,
            source: source.into(),
            kind,
            artifact: Artifact {
                file: None,
                anchor: None,
            },
            summary_ref: None,
            est_minutes: None,
            focus_hint,
            status: TaskStatus::InProgress,
            created_at: now,
            session_id: identity.session_id.clone(),
            terminal_id: identity.terminal_id.clone(),
            cwd: identity.cwd.clone(),
            current_request: Some(user_prompt_summary),
            last_output_summary: None,
            last_event_at: Some(now),
            permission_summary: None,
        };

        tasks::add(task.clone()).await?;
        Ok(task)
    }
}

fn find_matching_task<'a>(
    tasks: &'a [Task],
    identity: &HookIdentity,
    only_in_progress: bool,
    source: &str,
) -> Option<&'a Task> {
    let candidates = tasks.iter().filter(|task| {
        task.source == source
            && (!only_in_progress || task.status == TaskStatus::InProgress)
            && matches!(task.status, TaskStatus::InProgress | TaskStatus::Pending)
    });

    if let Some(session_id) = identity
        .session_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        if let Some(task) = candidates
            .clone()
            .filter(|task| task.session_id.as_deref() == Some(session_id))
            .max_by_key(|task| task.last_event_at.unwrap_or(task.created_at))
        {
            return Some(task);
        }
    }

    if let (Some(terminal_id), Some(cwd)) = (
        identity
            .terminal_id
            .as_deref()
            .filter(|value| !value.is_empty()),
        identity.cwd.as_deref(),
    ) {
        if let Some(task) = candidates
            .clone()
            .filter(|task| task.terminal_id.as_deref() == Some(terminal_id))
            .filter(|task| task.cwd.as_deref().map(normalize_path) == Some(normalize_path(cwd)))
            .max_by_key(|task| task.last_event_at.unwrap_or(task.created_at))
        {
            return Some(task);
        }
    }

    None
}

fn first_string(payload: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = payload.get(*key).and_then(Value::as_str) {
            if !value.trim().is_empty() {
                return Some(value.trim().to_string());
            }
        }
        if let Some(value) = payload
            .pointer(&format!("/{}", key.replace('.', "/")))
            .and_then(Value::as_str)
        {
            if !value.trim().is_empty() {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

fn extract_user_message(payload: &Value) -> String {
    first_string(
        payload,
        &[
            "user_message",
            "userMessage",
            "prompt",
            "message",
            "input",
            "text",
        ],
    )
    .unwrap_or_default()
}

fn extract_permission_summary(payload: &Value) -> Option<String> {
    first_string(
        payload,
        &[
            "permission_summary",
            "permissionSummary",
            "permission.message",
            "notification",
            "message",
            "tool_name",
            "toolName",
        ],
    )
    .map(|value| truncate_chars(&value, 180))
}

fn summarize_tool_event(kind: &str, payload: &Value) -> String {
    let tool = first_string(payload, &["tool_name", "toolName", "tool.name"])
        .unwrap_or_else(|| "tool".into());
    let decision = first_string(payload, &["decision", "result", "status"])
        .map(|value| format!(" ({value})"))
        .unwrap_or_default();
    format!("{kind}: {tool}{decision}")
}

fn source_display(source: &str) -> String {
    let mut chars = source.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn build_session_focus_hint(identity: &HookIdentity, source: &str) -> String {
    let display = source_display(source);
    let session = identity.session_id.as_deref().unwrap_or("unknown-session");
    let terminal = identity
        .terminal_id
        .as_deref()
        .unwrap_or("unknown-terminal");
    let cwd = identity
        .cwd
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "unknown-cwd".into());
    format!("{display} session {session} · terminal {terminal} · {cwd}")
}

fn build_completion_summary(
    payload: &Value,
    tail: &[transcript::TranscriptMessage],
    recent: &[PathBuf],
    kind: &str,
    source: &str,
) -> String {
    let display = source_display(source);
    if let Some(summary) = first_string(
        payload,
        &[
            "summary",
            "completion_summary",
            "completionSummary",
            "message",
            "notification",
        ],
    ) {
        return truncate_chars(&summary, 240);
    }

    if let Some(message) = tail.last() {
        return truncate_chars(&message.text_preview, 240);
    }

    if let Some(file) = recent.first() {
        return format!(
            "{display} {kind} event completed with recent changes in {}",
            file.display()
        );
    }

    format!("{display} {kind} event completed; no transcript or recent file summary was available")
}

async fn write_fast_summary(
    kind: &str,
    identity: &HookIdentity,
    payload: &Value,
    tail: &[transcript::TranscriptMessage],
    recent: &[PathBuf],
    output_summary: &str,
    source: &str,
) -> anyhow::Result<String> {
    let dir = Paths::summaries_dir();
    tokio::fs::create_dir_all(&dir).await?;
    let slug = recent
        .first()
        .and_then(|path| path.file_stem())
        .and_then(|name| name.to_str())
        .map(slugify)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| kind.replace('_', "-"));
    let filename = format!("{}-{}.md", Utc::now().format("%Y%m%dT%H%M%SZ"), slug);
    let rel = format!("summaries/{filename}");
    let path = dir.join(filename);
    let files = if recent.is_empty() {
        "- (no recent files)\n".to_string()
    } else {
        recent
            .iter()
            .take(10)
            .map(|path| format!("- {}", path.display()))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    };
    let transcript_tail = if tail.is_empty() {
        "- (no transcript tail)\n".to_string()
    } else {
        tail.iter()
            .take(5)
            .map(|message| format!("- [{}] {}", message.role, message.text_preview))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    };
    let cwd = identity
        .cwd
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "(unknown cwd)".into());
    let request = first_string(payload, &["user_message", "prompt", "message"])
        .map(|value| truncate_chars(&value, 240))
        .unwrap_or_else(|| "(not provided by this event)".into());
    let display = source_display(source);
    let content = format!(
        "# {display} {kind} - {}\n\n**What**: {output_summary}\n\n**Where**:\n{files}\n**Why it matters**: This updates the {display} task associated with session `{}` in `{cwd}`.\n\n**What you should check**:\n- Confirm the output matches the latest user request.\n- Review the listed files or transcript tail before passing the task.\n\n**Current request**: {request}\n\n**Transcript tail**:\n{transcript_tail}",
        Utc::now().to_rfc3339(),
        identity.session_id.as_deref().unwrap_or("unknown")
    );

    let mut file = tokio::fs::File::create(&path).await?;
    file.write_all(content.as_bytes()).await?;
    file.flush().await?;
    Ok(rel)
}

fn normalize_path(path: &Path) -> PathBuf {
    let display = path.display().to_string().replace('\\', "/");
    PathBuf::from(display.trim_end_matches('/').to_ascii_lowercase())
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut output = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        output.push_str("...");
    }
    output
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

async fn run_list(all: bool) -> anyhow::Result<()> {
    let mut file = tasks::load().await?;
    file.tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    for task in file
        .tasks
        .iter()
        .filter(|task| all || task.status == TaskStatus::Pending)
        .take(10)
    {
        println!(
            "{} [{:?}] {} {}",
            task.id,
            task.status,
            task.kind,
            task.artifact
                .file
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "(no file)".to_string())
        );
    }
    Ok(())
}

async fn run_show(id: &str) -> anyhow::Result<()> {
    let file = tasks::load().await?;
    let task = file
        .tasks
        .iter()
        .find(|task| task.id == id)
        .with_context(|| format!("task not found: {id}"))?;
    println!("{}", serde_json::to_string_pretty(task)?);
    if let Some(summary_ref) = &task.summary_ref {
        let summary_path = conductor_core::paths::state().join(summary_ref);
        match tokio::fs::read_to_string(&summary_path).await {
            Ok(summary) => println!("\n--- Summary ---\n{summary}"),
            Err(err) => println!("\n--- Summary unavailable: {err} ---"),
        }
    }
    Ok(())
}

async fn set_status(id: &str, status: TaskStatus) -> anyhow::Result<()> {
    tasks::update(id, |task| task.status = status).await
}

async fn run_migrate() -> anyhow::Result<()> {
    let count = tasks::migrate_legacy_json().await?;
    println!("Migrated {count} tasks");
    Ok(())
}

async fn run_proposal(command: ProposalCommand) -> anyhow::Result<()> {
    match command {
        ProposalCommand::List => {
            for proposal in proposals::list_pending().await? {
                println!(
                    "{} [{:?}] {}",
                    proposal.id,
                    proposal.status,
                    proposal.for_cwd.display()
                );
            }
        }
        ProposalCommand::Show { id } => {
            let proposal = proposals::get(&id).await?;
            println!("{}", serde_json::to_string_pretty(&proposal)?);
        }
        ProposalCommand::Approve { id } => proposals::approve(&id).await?,
        ProposalCommand::Execute { id } => {
            tools::register_builtin_tools();
            let result = proposals::execute_proposal(&id).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "success": result.success,
                    "output": result.output,
                    "error": result.error,
                    "duration_ms": result.duration_ms
                }))?
            );
        }
        ProposalCommand::Reject { id } => proposals::reject(&id).await?,
        ProposalCommand::Create {
            for_cwd,
            content,
            reason,
        } => {
            let id = proposals::next_id().await?;
            let now = Utc::now();
            proposals::create(Proposal {
                id: id.clone(),
                workspace_id: None,
                for_cwd,
                source: ProposalSource::Chat,
                title: content.chars().take(80).collect(),
                content,
                reason,
                tool_id: None,
                tool_input_json: None,
                risk_level: RiskLevel::ReadOnly,
                dry_run: false,
                status: ProposalStatus::Pending,
                result_ref: None,
                agent_task_id: None,
                grant_id: None,
                created_at: now,
                updated_at: now,
            })
            .await?;
            println!("{id}");
        }
    }
    Ok(())
}

async fn run_sub(command: SubCommand) -> anyhow::Result<()> {
    match command {
        SubCommand::Run {
            prompt,
            cwd,
            timeout_seconds,
        } => {
            let result = subagent::run_claude_p(
                &prompt,
                cwd.as_deref(),
                Duration::from_secs(timeout_seconds),
            )
            .await?;
            print!("{}", result.stdout);
            if !result.stderr.is_empty() {
                eprintln!("{}", result.stderr);
            }
            if let Some(path) = result.log_path {
                eprintln!("Log: {}", path.display());
            }
        }
    }
    Ok(())
}

async fn run_agent(command: AgentCommand) -> anyhow::Result<()> {
    match command {
        AgentCommand::List { all, workspace_id } => {
            let mut runs = agent_runs::list(agent_runs::AgentRunFilter {
                workspace_id,
                ..Default::default()
            })
            .await?;
            if !all {
                runs.retain(|run| {
                    matches!(
                        run.status,
                        agent_runs::AgentRunStatus::Queued | agent_runs::AgentRunStatus::Running
                    )
                });
            }
            for run in runs {
                println!(
                    "{} [{}] {} {}",
                    run.id,
                    run.status.as_str(),
                    run.agent_id,
                    run.cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "(no cwd)".to_string())
                );
            }
        }
        AgentCommand::Read { run_id, max_bytes } => {
            let output = agent_runs::read_output(&run_id, max_bytes).await?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        AgentCommand::Stop { run_id } => {
            let run = agent_runs::stop(&run_id).await?;
            println!("{}", serde_json::to_string_pretty(&run)?);
        }
        AgentCommand::Run {
            runtime_url,
            token,
            agent_id,
            workspace_id,
            lease_ttl_seconds,
            poll_interval_ms,
            claude_binary,
            timeout_seconds,
            once,
        } => {
            agent_run::run(agent_run::AgentRunnerConfig {
                runtime_url,
                token,
                agent_id,
                workspace_id,
                lease_ttl_seconds,
                poll_interval_ms,
                once,
                claude_binary,
                timeout_seconds,
            })
            .await?;
        }
    }
    Ok(())
}

async fn run_team(command: TeamCommand) -> anyhow::Result<()> {
    match command {
        TeamCommand::List { all, workspace_id } => {
            let teams = agent_teams::list_teams(workspace_id.as_deref(), all).await?;
            for team in teams {
                println!(
                    "{} [{}] {} {}",
                    team.id,
                    team.status.as_str(),
                    team.name,
                    team.workspace_id.as_deref().unwrap_or("(no workspace)")
                );
            }
        }
        TeamCommand::Create { name, workspace_id } => {
            let team = agent_teams::create_team(agent_teams::CreateAgentTeamInput {
                id: None,
                name,
                workspace_id,
                write_scope: vec![],
                metadata: None,
            })
            .await?;
            println!("{}", serde_json::to_string_pretty(&team)?);
        }
        TeamCommand::AddMember {
            team_id,
            agent_id,
            role,
            run_id,
            cwd,
        } => {
            let member = agent_teams::add_member(agent_teams::AddAgentTeamMemberInput {
                team_id,
                agent_id,
                role,
                run_id,
                cwd,
                subscriptions: Vec::new(),
                metadata: None,
            })
            .await?;
            println!("{}", serde_json::to_string_pretty(&member)?);
        }
        TeamCommand::Snapshot { team_id } => {
            let snapshot = agent_teams::snapshot(&team_id, 50).await?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        TeamCommand::Send {
            team_id,
            content,
            sender,
            recipient,
            broadcast,
        } => {
            let messages = agent_teams::send_message(agent_teams::SendAgentMessageInput {
                team_id,
                sender_agent_id: sender,
                recipient_agent_id: recipient,
                broadcast,
                kind: None,
                content,
                metadata: None,
            })
            .await?;
            println!("{}", serde_json::to_string_pretty(&messages)?);
        }
        TeamCommand::Read {
            team_id,
            recipient,
            include_read,
        } => {
            let messages = agent_teams::list_mailbox(agent_teams::AgentMailboxFilter {
                team_id,
                recipient_agent_id: recipient,
                include_read,
                limit: Some(100),
            })
            .await?;
            println!("{}", serde_json::to_string_pretty(&messages)?);
        }
        TeamCommand::MarkRead { message_id } => {
            let message = agent_teams::mark_message_read(&message_id).await?;
            println!("{}", serde_json::to_string_pretty(&message)?);
        }
    }
    Ok(())
}

async fn run_workspace(command: WorkspaceCommand) -> anyhow::Result<()> {
    match command {
        WorkspaceCommand::ShowProjection { workspace_id } => {
            let md = ProjectionWriter::new(&workspace_id)
                .generate_workspace_md()
                .await?;
            print!("{md}");
        }
        WorkspaceCommand::WriteProjection { workspace_id } => {
            let path = ProjectionWriter::new(&workspace_id).write_to_file().await?;
            println!("{}", path.display());
        }
    }
    Ok(())
}

async fn run_chat(interactive: bool, message: Option<String>) -> anyhow::Result<()> {
    if interactive || message.is_none() {
        interactive_chat().await
    } else {
        single_query(message.unwrap()).await
    }
}

async fn interactive_chat() -> anyhow::Result<()> {
    println!("Conductor Chat (输入 'exit' 或 'quit' 退出)");
    println!("---");

    loop {
        print!("> ");
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut input = String::new();
        if std::io::stdin().read_line(&mut input)? == 0 {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        if input == "exit" || input == "quit" {
            break;
        }

        match chat::send(input.to_string()).await {
            Ok(reply) => {
                println!("{}", reply.message.content);
                println!("---");
            }
            Err(err) => {
                eprintln!("错误: {:#}", err);
                println!("---");
            }
        }
    }
    Ok(())
}

async fn single_query(message: String) -> anyhow::Result<()> {
    let reply = chat::send(message).await?;
    println!("{}", reply.message.content);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use conductor_core::tasks::Artifact;
    use serde_json::json;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct TestRoot {
        _guard: MutexGuard<'static, ()>,
        temp: tempfile::TempDir,
        previous: Option<std::ffi::OsString>,
    }

    impl TestRoot {
        fn new() -> Self {
            let guard = ENV_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .expect("test env lock poisoned");
            let previous = std::env::var_os("CONDUCTOR_ROOT");
            let temp = tempfile::tempdir().expect("create temp conductor root");
            std::env::set_var("CONDUCTOR_ROOT", temp.path());
            Self {
                _guard: guard,
                temp,
                previous,
            }
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var("CONDUCTOR_ROOT", previous);
            } else {
                std::env::remove_var("CONDUCTOR_ROOT");
            }
        }
    }

    fn claude_task(
        id: &str,
        status: TaskStatus,
        session_id: Option<&str>,
        terminal_id: Option<&str>,
        cwd: &str,
        minutes_ago: i64,
    ) -> Task {
        let ts = Utc::now() - ChronoDuration::minutes(minutes_ago);
        Task {
            id: id.to_string(),
            source: "claude".into(),
            kind: "review-doc".into(),
            artifact: Artifact {
                file: None,
                anchor: None,
            },
            summary_ref: None,
            est_minutes: None,
            focus_hint: None,
            status,
            created_at: ts,
            session_id: session_id.map(str::to_string),
            terminal_id: terminal_id.map(str::to_string),
            cwd: Some(normalize_path(Path::new(cwd))),
            current_request: None,
            last_output_summary: None,
            last_event_at: Some(ts),
            permission_summary: None,
        }
    }

    fn codex_task(
        id: &str,
        status: TaskStatus,
        session_id: Option<&str>,
        terminal_id: Option<&str>,
        cwd: &str,
        minutes_ago: i64,
    ) -> Task {
        let mut task = claude_task(id, status, session_id, terminal_id, cwd, minutes_ago);
        task.source = "codex".into();
        task
    }

    #[test]
    fn hook_identity_reads_terminal_hint_and_normalizes_cwd() {
        let payload = json!({
            "session_id": "s1",
            "terminal_hint": "term-a",
            "cwd": "I:\\Personal-Agent\\"
        });

        let identity = HookIdentity::from_payload(&payload);

        assert_eq!(identity.session_id.as_deref(), Some("s1"));
        assert_eq!(identity.terminal_id.as_deref(), Some("term-a"));
        assert_eq!(
            identity.cwd.as_ref().map(|path| path.display().to_string()),
            Some("i:/personal-agent".to_string())
        );
    }

    #[test]
    fn find_matching_task_prefers_session_id_over_terminal() {
        let tasks = vec![
            claude_task(
                "wrong-terminal",
                TaskStatus::InProgress,
                Some("other"),
                Some("term-a"),
                "I:/personal-agent",
                1,
            ),
            claude_task(
                "right-session",
                TaskStatus::InProgress,
                Some("s1"),
                Some("term-b"),
                "I:/personal-agent",
                10,
            ),
        ];
        let identity = HookIdentity {
            session_id: Some("s1".into()),
            terminal_id: Some("term-a".into()),
            cwd: Some(normalize_path(Path::new("I:\\personal-agent"))),
            transcript_path: None,
        };

        let found = find_matching_task(&tasks, &identity, false, "claude").expect("match");

        assert_eq!(found.id, "right-session");
    }

    #[test]
    fn find_matching_task_uses_terminal_and_cwd_when_session_missing() {
        let tasks = vec![
            claude_task(
                "wrong-cwd",
                TaskStatus::InProgress,
                None,
                Some("term-a"),
                "I:/other",
                1,
            ),
            claude_task(
                "right-terminal-cwd",
                TaskStatus::InProgress,
                None,
                Some("term-a"),
                "I:/personal-agent",
                2,
            ),
        ];
        let identity = HookIdentity {
            session_id: None,
            terminal_id: Some("term-a".into()),
            cwd: Some(normalize_path(Path::new("I:\\personal-agent\\"))),
            transcript_path: None,
        };

        let found = find_matching_task(&tasks, &identity, false, "claude").expect("match");

        assert_eq!(found.id, "right-terminal-cwd");
    }

    #[tokio::test]
    async fn create_or_update_task_reuses_same_session_after_pending() {
        let _root = TestRoot::new();
        let identity = HookIdentity {
            session_id: Some("s1".into()),
            terminal_id: Some("term-a".into()),
            cwd: Some(normalize_path(Path::new("I:/personal-agent"))),
            transcript_path: None,
        };

        let first = create_or_update_task_for_session(&identity, "write docs", "claude")
            .await
            .expect("create task");
        tasks::update(&first.id, |task| task.status = TaskStatus::Pending)
            .await
            .expect("mark pending");
        let second = create_or_update_task_for_session(&identity, "continue docs", "claude")
            .await
            .expect("update task");

        let loaded = tasks::load().await.expect("load tasks");
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(first.id, second.id);
        assert_eq!(loaded.tasks[0].status, TaskStatus::InProgress);
        assert_eq!(
            loaded.tasks[0].current_request.as_deref(),
            Some("continue docs")
        );
    }

    #[tokio::test]
    async fn update_permission_context_creates_running_task_when_prompt_was_missed() {
        let _root = TestRoot::new();
        let identity = HookIdentity {
            session_id: Some("s-missed".into()),
            terminal_id: Some("term-a".into()),
            cwd: Some(normalize_path(Path::new("I:/personal-agent"))),
            transcript_path: None,
        };

        update_permission_context(&identity, "Read Cargo.toml".to_string(), false, "claude")
            .await
            .expect("update permission context");

        let loaded = tasks::load().await.expect("load tasks");
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].source, "claude");
        assert_eq!(loaded.tasks[0].status, TaskStatus::InProgress);
        assert_eq!(
            loaded.tasks[0].permission_summary.as_deref(),
            Some("Read Cargo.toml")
        );
        assert_eq!(loaded.tasks[0].session_id.as_deref(), Some("s-missed"));
    }

    #[tokio::test]
    async fn update_permission_context_restores_pending_task_to_in_progress() {
        let _root = TestRoot::new();
        let identity = HookIdentity {
            session_id: Some("s-resume".into()),
            terminal_id: Some("term-a".into()),
            cwd: Some(normalize_path(Path::new("I:/personal-agent"))),
            transcript_path: None,
        };

        let task = create_or_update_task_for_session(&identity, "review dependencies", "claude")
            .await
            .expect("create task");
        tasks::update(&task.id, |task| task.status = TaskStatus::Pending)
            .await
            .expect("mark pending");
        update_permission_context(
            &identity,
            "PostToolUse Read completed".to_string(),
            false,
            "claude",
        )
        .await
        .expect("update permission context");

        let loaded = tasks::load().await.expect("load tasks");
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].status, TaskStatus::InProgress);
        assert_eq!(
            loaded.tasks[0].permission_summary.as_deref(),
            Some("PostToolUse Read completed")
        );
    }

    #[tokio::test]
    async fn create_or_update_task_keeps_parallel_sessions_separate() {
        let _root = TestRoot::new();
        for session_id in ["s1", "s2", "s3"] {
            let identity = HookIdentity {
                session_id: Some(session_id.into()),
                terminal_id: None,
                cwd: Some(normalize_path(Path::new("I:/personal-agent"))),
                transcript_path: None,
            };
            create_or_update_task_for_session(&identity, "continue implementation", "claude")
                .await
                .expect("create task");
        }

        let loaded = tasks::load().await.expect("load tasks");
        let running = loaded
            .tasks
            .iter()
            .filter(|task| task.status == TaskStatus::InProgress)
            .count();

        assert_eq!(loaded.tasks.len(), 3);
        assert_eq!(running, 3);
    }

    #[test]
    fn find_matching_task_does_not_reuse_cwd_without_identity() {
        let tasks = vec![claude_task(
            "existing",
            TaskStatus::InProgress,
            None,
            None,
            "I:/personal-agent",
            1,
        )];
        let identity = HookIdentity {
            session_id: None,
            terminal_id: None,
            cwd: Some(normalize_path(Path::new("I:/personal-agent"))),
            transcript_path: None,
        };

        assert!(find_matching_task(&tasks, &identity, false, "claude").is_none());
    }

    #[test]
    fn find_matching_task_does_not_mix_sources() {
        let tasks = vec![
            claude_task(
                "claude-task",
                TaskStatus::InProgress,
                Some("s1"),
                Some("term-a"),
                "I:/personal-agent",
                1,
            ),
            codex_task(
                "codex-task",
                TaskStatus::InProgress,
                Some("s1"),
                Some("term-a"),
                "I:/personal-agent",
                1,
            ),
        ];
        let identity = HookIdentity {
            session_id: Some("s1".into()),
            terminal_id: Some("term-a".into()),
            cwd: Some(normalize_path(Path::new("I:/personal-agent"))),
            transcript_path: None,
        };

        let claude_match =
            find_matching_task(&tasks, &identity, false, "claude").expect("claude match");
        assert_eq!(claude_match.id, "claude-task");

        let codex_match =
            find_matching_task(&tasks, &identity, false, "codex").expect("codex match");
        assert_eq!(codex_match.id, "codex-task");
    }

    #[test]
    fn source_display_capitalizes() {
        assert_eq!(source_display("claude"), "Claude");
        assert_eq!(source_display("codex"), "Codex");
    }
}
