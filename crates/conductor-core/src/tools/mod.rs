use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::OnceLock;

// Submodule declarations
mod agent;
mod codex;
mod fs;
mod mcp;
mod memory;
mod misc;
mod office;
mod registry;
mod shell;
mod task;

// Re-exports for public API compatibility
pub use registry::{
    execute_tool, execute_tool_with_workspace, execute_tool_with_workspace_async, get_tool,
    list_tools, needs_approval, register_tool, ToolExecutionResult, ToolExecutorFn, ToolPermission,
    ToolProviderKind, ToolRegistry, ToolSpec,
};

/// Shared tokio runtime used by all sync tool executors that need to bridge into async.
/// Avoids the overhead of creating ~22 separate runtimes per tool invocation.
pub(crate) fn shared_runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Runtime::new().expect("failed to create shared tokio runtime")
    })
}

// ── Workspace root management ───────────────────────────────────────────────

thread_local! {
    static WORKSPACE_ROOT_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub(crate) struct WorkspaceRootGuard {
    previous: Option<PathBuf>,
}

impl WorkspaceRootGuard {
    pub(crate) fn push(root: Option<PathBuf>) -> Self {
        let previous = WORKSPACE_ROOT_OVERRIDE.with(|slot| {
            let previous = slot.borrow().clone();
            *slot.borrow_mut() = root;
            previous
        });
        Self { previous }
    }
}

impl Drop for WorkspaceRootGuard {
    fn drop(&mut self) {
        WORKSPACE_ROOT_OVERRIDE.with(|slot| {
            *slot.borrow_mut() = self.previous.clone();
        });
    }
}

pub(crate) fn current_workspace_root() -> PathBuf {
    WORKSPACE_ROOT_OVERRIDE
        .with(|slot| slot.borrow().clone())
        .unwrap_or_else(crate::paths::root)
}

fn normalize_existing_or_virtual(path: &std::path::Path) -> Result<PathBuf, anyhow::Error> {
    if path.exists() {
        return Ok(std::fs::canonicalize(path)?);
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path has no parent: {}", path.display()))?;
    let normalized_parent = if parent.exists() {
        std::fs::canonicalize(parent)?
    } else {
        normalize_existing_or_virtual(parent)?
    };
    Ok(normalized_parent.join(path.file_name().unwrap_or_default()))
}

fn ensure_within_workspace(path: &std::path::Path) -> Result<PathBuf, anyhow::Error> {
    let workspace_root = current_workspace_root();
    let normalized_root = normalize_existing_or_virtual(&workspace_root)?;
    let normalized_path = normalize_existing_or_virtual(path)?;
    if normalized_path.starts_with(&normalized_root) {
        Ok(normalized_path)
    } else {
        anyhow::bail!(
            "path {} is outside current workspace {}; choose another workspace or request approval",
            normalized_path.display(),
            normalized_root.display()
        );
    }
}

/// Resolve a file path against the active workspace root and reject paths outside it.
pub(crate) fn resolve_workspace_path(path: &str) -> Result<std::path::PathBuf, anyhow::Error> {
    let p = std::path::Path::new(path);
    let candidate = if p.is_absolute() {
        p.to_path_buf()
    } else {
        current_workspace_root().join(path)
    };
    ensure_within_workspace(&candidate)
}

/// Return a display-friendly path, relative to workspace root if possible.
pub(crate) fn display_path(path: &std::path::Path) -> String {
    let root = current_workspace_root();
    path.strip_prefix(&root)
        .map(|rel| rel.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

// ── Registration ────────────────────────────────────────────────────────────

pub fn register_builtin_tools() {
    let mut reg = crate::app_state::AppState::global()
        .tool_registry()
        .write()
        .unwrap();
    misc::register(&mut reg);
    task::register(&mut reg);
    agent::register(&mut reg);
    memory::register(&mut reg);
    fs::register(&mut reg);
    shell::register(&mut reg);
    codex::register(&mut reg);
    office::register(&mut reg);
}

pub fn register_mcp_tool_executor(_provider_id: String, _tool_name: String) {}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposals::RiskLevel;
    use chrono::Utc;

    #[test]
    fn register_and_execute_echo_tool() {
        register_builtin_tools();

        let result = execute_tool(
            "demo.echo",
            &serde_json::json!({ "message": "hello world" }),
        )
        .unwrap();
        assert!(result.success);
        assert_eq!(result.output["echo"], "hello world");
    }

    #[test]
    fn execute_tasks_list() {
        let _root = crate::test_support::TestRoot::new();
        register_builtin_tools();

        let result = execute_tool("tasks.list", &serde_json::json!({})).unwrap();
        assert!(result.success);
        assert!(result.output["tasks"].is_array());
    }

    #[test]
    fn risk_level_ordering() {
        assert!(RiskLevel::ReadOnly < RiskLevel::DraftOnly);
        assert!(RiskLevel::DraftOnly < RiskLevel::WorkspaceWrite);
        assert!(RiskLevel::WorkspaceWrite < RiskLevel::ExternalSideEffect);
        assert!(RiskLevel::ExternalSideEffect < RiskLevel::Destructive);
    }

    #[test]
    fn list_tools_by_risk() {
        register_builtin_tools();

        let reg = crate::app_state::AppState::global()
            .tool_registry()
            .read()
            .unwrap();
        let tools = reg.list_by_risk(RiskLevel::ReadOnly);
        assert!(!tools.is_empty());
        for tool in tools {
            assert!(tool.risk_level <= RiskLevel::ReadOnly);
        }
    }

    #[test]
    fn tool_search_finds_registered_tools() {
        register_builtin_tools();

        let result = execute_tool(
            "tool.search",
            &serde_json::json!({
                "query": "avatar",
                "limit": 5
            }),
        )
        .expect("tool.search");
        assert!(result.success);
        let tools = result.output["tools"].as_array().expect("tools array");
        assert!(tools.iter().any(|tool| tool["id"] == "pet.set_avatar"));
    }

    #[test]
    fn registers_subagent_claude_p_tool() {
        register_builtin_tools();

        let tool = get_tool("subagent.claude_p")
            .map(|(spec, _)| spec)
            .expect("subagent tool");
        assert_eq!(tool.provider, ToolProviderKind::Subagent);
        assert_eq!(tool.risk_level, RiskLevel::ExternalSideEffect);
        assert!(tool.workspace_required);
    }

    #[test]
    fn registers_agent_run_tools() {
        register_builtin_tools();

        let start = get_tool("agent.start")
            .map(|(spec, _)| spec)
            .expect("agent.start tool");
        assert_eq!(start.provider, ToolProviderKind::Subagent);
        assert_eq!(start.risk_level, RiskLevel::ExternalSideEffect);
        assert!(start.workspace_required);

        let read = get_tool("agent.read_output")
            .map(|(spec, _)| spec)
            .expect("agent.read_output tool");
        assert_eq!(read.risk_level, RiskLevel::ReadOnly);

        let stop = get_tool("agent.stop")
            .map(|(spec, _)| spec)
            .expect("agent.stop tool");
        assert!(stop.permissions.contains(&ToolPermission::SystemControl));
    }

    #[test]
    fn agent_read_output_and_stop_execute_without_claude() {
        let _root = crate::test_support::TestRoot::new();
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let now = Utc::now();
        runtime
            .block_on(crate::agent_runs::upsert(&crate::agent_runs::AgentRun {
                id: "ar-tool-test".to_string(),
                agent_id: "claude".to_string(),
                role: "assistant".to_string(),
                workspace_id: None,
                cwd: None,
                status: crate::agent_runs::AgentRunStatus::Running,
                pid: None,
                command_json: None,
                input_ref: None,
                output_ref: None,
                error: None,
                started_at: now,
                updated_at: now,
                finished_at: None,
                metadata_json: None,
            }))
            .expect("upsert agent run");
        register_builtin_tools();

        let read_result = execute_tool(
            "agent.read_output",
            &serde_json::json!({ "run_id": "ar-tool-test" }),
        )
        .expect("read output");
        assert!(read_result.success);
        assert_eq!(read_result.output["run"]["id"], "ar-tool-test");

        let stop_result = execute_tool(
            "agent.stop",
            &serde_json::json!({ "run_id": "ar-tool-test" }),
        )
        .expect("stop agent");
        assert!(stop_result.success);
        assert_eq!(stop_result.output["status"], "stopped");
    }

    #[test]
    fn registers_agent_team_and_mailbox_tools() {
        register_builtin_tools();

        for tool_id in [
            "agent.team.create",
            "agent.team.list",
            "agent.team.add_member",
            "agent.team.snapshot",
            "agent.team.plan_verdict",
            "agent.team.review_verdict",
            "agent.mailbox.send",
            "agent.mailbox.list",
            "agent.mailbox.mark_read",
        ] {
            assert!(get_tool(tool_id).is_some(), "missing {tool_id}");
        }
    }

    #[test]
    fn agent_team_tools_round_trip() {
        let _root = crate::test_support::TestRoot::new();
        register_builtin_tools();

        let team = execute_tool(
            "agent.team.create",
            &serde_json::json!({
                "id": "team-tool",
                "name": "Tool Team"
            }),
        )
        .expect("create team");
        assert!(team.success);
        assert_eq!(team.output["id"], "team-tool");

        let member = execute_tool(
            "agent.team.add_member",
            &serde_json::json!({
                "team_id": "team-tool",
                "agent_id": "reviewer",
                "role": "review"
            }),
        )
        .expect("add member");
        assert!(member.success);

        let sent = execute_tool(
            "agent.mailbox.send",
            &serde_json::json!({
                "team_id": "team-tool",
                "recipient_agent_id": "reviewer",
                "content": "check this"
            }),
        )
        .expect("send message");
        assert!(sent.success);
        let message_id = sent.output["messages"][0]["id"]
            .as_str()
            .expect("message id")
            .to_string();

        let mailbox = execute_tool(
            "agent.mailbox.list",
            &serde_json::json!({
                "team_id": "team-tool",
                "recipient_agent_id": "reviewer"
            }),
        )
        .expect("list mailbox");
        assert_eq!(mailbox.output["messages"].as_array().unwrap().len(), 1);

        let read = execute_tool(
            "agent.mailbox.mark_read",
            &serde_json::json!({ "message_id": message_id }),
        )
        .expect("mark read");
        assert!(read.output["read_at"].is_string());
    }

    #[test]
    fn agent_mailbox_plan_approval_response_advances_team() {
        let _root = crate::test_support::TestRoot::new();
        register_builtin_tools();

        execute_tool(
            "agent.team.create",
            &serde_json::json!({
                "id": "team-tool-approval",
                "name": "Tool Approval Team"
            }),
        )
        .expect("create team");
        execute_tool(
            "agent.team.add_member",
            &serde_json::json!({
                "team_id": "team-tool-approval",
                "agent_id": "executor",
                "role": "executor",
                "run_id": "ar-tool-approval",
                "metadata": {
                    "task_id": "task-tool-approval"
                }
            }),
        )
        .expect("add executor");

        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime
            .block_on(crate::agent_teams::transition_team_lifecycle(
                "team-tool-approval",
                crate::agent_teams::AgentTeamLifecycle::Planning,
            ))
            .expect("planning");
        runtime
            .block_on(crate::agent_teams::transition_team_lifecycle(
                "team-tool-approval",
                crate::agent_teams::AgentTeamLifecycle::AwaitingPlanApproval,
            ))
            .expect("awaiting approval");

        let sent = execute_tool(
            "agent.mailbox.send",
            &serde_json::json!({
                "team_id": "team-tool-approval",
                "recipient_agent_id": "executor",
                "kind": "plan_approval_response",
                "content": "approved",
                "metadata": {
                    "verdict": "approved"
                }
            }),
        )
        .expect("send approval response");
        assert!(sent.success);

        let snapshot = execute_tool(
            "agent.team.snapshot",
            &serde_json::json!({
                "team_id": "team-tool-approval"
            }),
        )
        .expect("snapshot");
        assert_eq!(snapshot.output["team"]["lifecycle"], "executing");
    }

    #[test]
    fn agent_team_review_verdict_accepts_team() {
        let _root = crate::test_support::TestRoot::new();
        register_builtin_tools();

        execute_tool(
            "agent.team.create",
            &serde_json::json!({
                "id": "team-tool-review",
                "name": "Tool Review Team"
            }),
        )
        .expect("create team");
        execute_tool(
            "agent.team.add_member",
            &serde_json::json!({
                "team_id": "team-tool-review",
                "agent_id": "executor",
                "role": "executor",
                "run_id": "ar-tool-review",
                "metadata": {
                    "task_id": "task-tool-review"
                }
            }),
        )
        .expect("add executor");

        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime
            .block_on(crate::agent_teams::transition_team_lifecycle(
                "team-tool-review",
                crate::agent_teams::AgentTeamLifecycle::Planning,
            ))
            .expect("planning");
        runtime
            .block_on(crate::agent_teams::transition_team_lifecycle(
                "team-tool-review",
                crate::agent_teams::AgentTeamLifecycle::AwaitingPlanApproval,
            ))
            .expect("awaiting approval");

        execute_tool(
            "agent.team.plan_verdict",
            &serde_json::json!({
                "team_id": "team-tool-review",
                "verdict": "approved"
            }),
        )
        .expect("approve plan");
        runtime
            .block_on(crate::agent_teams::transition_team_lifecycle(
                "team-tool-review",
                crate::agent_teams::AgentTeamLifecycle::AwaitingReview,
            ))
            .expect("awaiting review");

        let accepted = execute_tool(
            "agent.team.review_verdict",
            &serde_json::json!({
                "team_id": "team-tool-review",
                "verdict": "accepted"
            }),
        )
        .expect("accept review");
        assert!(accepted.success);
        assert_eq!(accepted.output["lifecycle"], "accepted");
    }

    #[test]
    fn workspace_context_blocks_writes_in_read_only_workspace() {
        let _root = crate::test_support::TestRoot::new();
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime
            .block_on(crate::workspaces::create(crate::workspaces::Workspace {
                id: "ws-readonly".to_string(),
                root: std::path::PathBuf::from("I:/readonly"),
                name: "readonly".to_string(),
                kind: crate::workspaces::WorkspaceKind::Code,
                trust_level: crate::workspaces::TrustLevel::ReadOnly,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                last_active_at: None,
                metadata: serde_json::json!({}),
            }))
            .expect("create workspace");
        register_builtin_tools();

        let result = execute_tool_with_workspace(
            "task.create",
            &serde_json::json!({ "subject": "write task" }),
            Some("ws-readonly"),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("read_only"));
    }

    #[test]
    fn office_export_text_is_read_only() {
        let _root = crate::test_support::TestRoot::new();
        let dir = tempfile::tempdir().expect("tempdir");
        let input_path = dir.path().join("input.txt");
        let output_path = dir.path().join("output.txt");
        std::fs::write(&input_path, "hello office").expect("write input");
        register_builtin_tools();

        let result = execute_tool(
            "office.export_text",
            &serde_json::json!({
                "path": input_path.display().to_string(),
                "output_path": output_path.display().to_string()
            }),
        )
        .expect("export text");

        assert!(result.success);
        assert_eq!(result.output["text"], "hello office");
        assert!(!output_path.exists());
    }

    #[test]
    fn file_glob_finds_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("b.txt"), "hello").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("c.rs"), "mod x;").unwrap();
        register_builtin_tools();

        let result = execute_tool(
            "file.glob",
            &serde_json::json!({ "pattern": "**/*.rs", "path": dir.path().display().to_string() }),
        )
        .unwrap();

        assert!(result.success);
        let count = result.output["count"].as_u64().unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn file_grep_finds_matches() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("test.rs"), "fn hello() {}\nfn world() {}\n").unwrap();
        register_builtin_tools();

        let result = execute_tool(
            "file.grep",
            &serde_json::json!({ "pattern": "fn \\w+", "path": dir.path().display().to_string() }),
        )
        .unwrap();

        assert!(result.success);
        let count = result.output["count"].as_u64().unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn file_read_returns_content_with_line_numbers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "line1\nline2\nline3\n").unwrap();
        register_builtin_tools();

        let result = execute_tool(
            "file.read",
            &serde_json::json!({ "file_path": path.display().to_string() }),
        )
        .unwrap();

        assert!(result.success);
        assert_eq!(result.output["total_lines"], 3);
        let text = result.output["text"].as_str().unwrap();
        assert!(text.contains("1: line1"));
        assert!(text.contains("2: line2"));
    }

    #[test]
    fn file_read_with_offset_and_limit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "a\nb\nc\nd\ne\n").unwrap();
        register_builtin_tools();

        let result = execute_tool(
            "file.read",
            &serde_json::json!({ "file_path": path.display().to_string(), "offset": 1, "limit": 2 }),
        )
        .unwrap();

        assert!(result.success);
        let text = result.output["text"].as_str().unwrap();
        assert!(text.contains("2: b"));
        assert!(text.contains("3: c"));
        assert!(!text.contains("4: d"));
    }

    #[test]
    fn file_write_creates_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out.txt");
        register_builtin_tools();

        let result = execute_tool(
            "file.write",
            &serde_json::json!({ "file_path": path.display().to_string(), "content": "hello world" }),
        )
        .unwrap();

        assert!(result.success);
        assert_eq!(result.output["bytes"], 11);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[test]
    fn file_edit_replaces_string() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("edit.txt");
        std::fs::write(&path, "hello world\nhello rust\n").unwrap();
        register_builtin_tools();

        let result = execute_tool(
            "file.edit",
            &serde_json::json!({
                "file_path": path.display().to_string(),
                "old_string": "hello",
                "new_string": "bye",
                "replace_all": true
            }),
        )
        .unwrap();

        assert!(result.success);
        assert_eq!(result.output["replacements"], 2);
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("bye world"));
        assert!(content.contains("bye rust"));
    }

    #[test]
    fn file_edit_fails_when_old_string_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("edit.txt");
        std::fs::write(&path, "hello world").unwrap();
        register_builtin_tools();

        let result = execute_tool(
            "file.edit",
            &serde_json::json!({
                "file_path": path.display().to_string(),
                "old_string": "notfound",
                "new_string": "bye"
            }),
        )
        .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("未在文件中找到"));
    }

    #[test]
    fn file_stat_returns_metadata() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("meta.txt");
        std::fs::write(&path, "content").unwrap();
        register_builtin_tools();

        let result = execute_tool(
            "file.stat",
            &serde_json::json!({ "file_path": path.display().to_string() }),
        )
        .unwrap();

        assert!(result.success);
        assert_eq!(result.output["size"], 7);
        assert_eq!(result.output["is_file"], true);
        assert_eq!(result.output["is_dir"], false);
    }
}
