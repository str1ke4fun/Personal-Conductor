use crate::events;
use crate::goal_tasks::AgentTask;
use crate::goals::GoalRun;
use crate::paths;
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
};
use tokio::fs;

/// HTML comment markers that delimit the auto-generated projection section.
pub const PROJECTION_START: &str = "<!-- PROJECTION START -->";
pub const PROJECTION_END: &str = "<!-- PROJECTION END -->";

/// Goal statuses that are considered "active" (non-terminal).
const ACTIVE_GOAL_STATUSES: &[&str] = &[
    "draft",
    "planning",
    "awaiting_plan_approval",
    "running",
    "blocked",
    "rework_required",
];

/// Task statuses that are considered "active" (non-terminal).
const ACTIVE_TASK_STATUSES: &[&str] = &[
    "proposed",
    "queued",
    "claimed",
    "running",
    "awaiting_permission",
    "awaiting_input",
    "blocked",
];

/// Task statuses that indicate the task is awaiting review.
const REVIEW_TASK_STATUSES: &[&str] = &["review_ready", "rework_required"];

/// Maximum number of recent events to show in the projection.
const RECENT_EVENT_LIMIT: u32 = 10;
const DEFAULT_ACTIVITY_LIMIT: u32 = 20;
const ACTIVE_AGENT_RUN_STATUSES: &[&str] = &["queued", "running"];
const ACTIVE_TOOL_CALL_STATUSES: &[&str] = &["pending", "executing", "approval_required"];
const ACTIVE_COMMAND_RUN_STATUSES: &[&str] =
    &["prepared", "awaiting_permission", "starting", "streaming"];
const ACTIVE_LEGACY_TASK_STATUSES: &[&str] = &["pending", "in_progress"];
const ACTIVE_TEAM_LIFECYCLES: &[&str] = &[
    "draft",
    "planning",
    "awaiting_plan_approval",
    "executing",
    "awaiting_review",
    "rework_required",
];

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ActivityArtifactRef {
    pub label: String,
    pub file: Option<String>,
    pub summary_ref: Option<String>,
    pub output_ref: Option<String>,
    pub result_ref: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivityToolCallRef {
    pub id: String,
    pub tool_id: String,
    pub status: String,
    pub command_run_id: Option<String>,
    pub risk_level: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivityCommandRunRef {
    pub id: String,
    pub command: String,
    pub status: String,
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivityAgentRunRef {
    pub id: String,
    pub agent_id: String,
    pub status: String,
    pub output_ref: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivityAgentTeamRef {
    pub id: String,
    pub name: String,
    pub lifecycle: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivityProjectionItem {
    pub activity_id: String,
    pub kind: String,
    pub status: String,
    pub title: String,
    pub actor: String,
    pub started_at: String,
    pub updated_at: String,
    pub session_id: Option<String>,
    pub goal_id: Option<String>,
    pub task_id: Option<String>,
    pub assistant_message: Option<String>,
    pub tool_calls: Vec<ActivityToolCallRef>,
    pub command_runs: Vec<ActivityCommandRunRef>,
    pub agent_runs: Vec<ActivityAgentRunRef>,
    pub agent_teams: Vec<ActivityAgentTeamRef>,
    pub artifacts: Vec<ActivityArtifactRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceActivityProjection {
    pub workspace_id: String,
    pub active: Vec<ActivityProjectionItem>,
    pub records: Vec<ActivityProjectionItem>,
}

#[derive(Clone, Debug)]
struct ActivityBundle {
    activity_id: String,
    kind: String,
    status: String,
    title: String,
    actor: String,
    started_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    session_id: Option<String>,
    goal_id: Option<String>,
    task_id: Option<String>,
    assistant_message: Option<String>,
    tool_calls: Vec<ActivityToolCallRef>,
    command_runs: Vec<ActivityCommandRunRef>,
    agent_runs: Vec<ActivityAgentRunRef>,
    agent_teams: Vec<ActivityAgentTeamRef>,
    artifacts: Vec<ActivityArtifactRef>,
    active: bool,
}

impl ActivityBundle {
    fn new(
        activity_id: String,
        kind: impl Into<String>,
        status: impl Into<String>,
        title: impl Into<String>,
        actor: impl Into<String>,
        started_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            activity_id,
            kind: kind.into(),
            status: status.into(),
            title: title.into(),
            actor: actor.into(),
            started_at,
            updated_at,
            session_id: None,
            goal_id: None,
            task_id: None,
            assistant_message: None,
            tool_calls: Vec::new(),
            command_runs: Vec::new(),
            agent_runs: Vec::new(),
            agent_teams: Vec::new(),
            artifacts: Vec::new(),
            active: false,
        }
    }

    fn touch(&mut self, timestamp: DateTime<Utc>) {
        if timestamp < self.started_at {
            self.started_at = timestamp;
        }
        if timestamp > self.updated_at {
            self.updated_at = timestamp;
        }
    }

    fn push_artifact(&mut self, artifact: ActivityArtifactRef) {
        let key = format!(
            "{}|{}|{}|{}|{}",
            artifact.label,
            artifact.file.as_deref().unwrap_or_default(),
            artifact.summary_ref.as_deref().unwrap_or_default(),
            artifact.output_ref.as_deref().unwrap_or_default(),
            artifact.result_ref.as_deref().unwrap_or_default()
        );
        let exists = self.artifacts.iter().any(|existing| {
            format!(
                "{}|{}|{}|{}|{}",
                existing.label,
                existing.file.as_deref().unwrap_or_default(),
                existing.summary_ref.as_deref().unwrap_or_default(),
                existing.output_ref.as_deref().unwrap_or_default(),
                existing.result_ref.as_deref().unwrap_or_default()
            ) == key
        });
        if !exists {
            self.artifacts.push(artifact);
        }
    }

    fn into_item(self) -> ActivityProjectionItem {
        ActivityProjectionItem {
            activity_id: self.activity_id,
            kind: self.kind,
            status: self.status,
            title: self.title,
            actor: self.actor,
            started_at: self.started_at.to_rfc3339(),
            updated_at: self.updated_at.to_rfc3339(),
            session_id: self.session_id,
            goal_id: self.goal_id,
            task_id: self.task_id,
            assistant_message: self.assistant_message,
            tool_calls: self.tool_calls,
            command_runs: self.command_runs,
            agent_runs: self.agent_runs,
            agent_teams: self.agent_teams,
            artifacts: self.artifacts,
        }
    }
}

pub async fn list_workspace_activities(
    workspace_id: &str,
    limit: Option<u32>,
) -> anyhow::Result<WorkspaceActivityProjection> {
    let workspace = crate::workspaces::get(workspace_id).await?;
    let limit = limit.unwrap_or(DEFAULT_ACTIVITY_LIMIT).clamp(1, 100);
    let root = workspace.root.clone();

    let sessions = crate::chat::list_chat_sessions(Some(limit * 5)).await?;
    let sessions: Vec<_> = sessions
        .into_iter()
        .filter(|session| session.workspace_id.as_deref() == Some(workspace_id))
        .collect();
    let session_ids: BTreeSet<String> = sessions.iter().map(|session| session.id.clone()).collect();

    let tool_calls = crate::tool_calls::list(crate::tool_calls::ToolCallFilter {
        workspace_id: Some(workspace_id.to_string()),
        limit: Some(limit * 10),
        ..Default::default()
    })
    .await?;

    let agent_runs = crate::agent_runs::list(crate::agent_runs::AgentRunFilter {
        workspace_id: Some(workspace_id.to_string()),
        limit: Some(limit * 10),
        ..Default::default()
    })
    .await?;

    let goal_tasks = crate::goal_tasks::list_tasks(workspace_id).await?;
    let agent_teams = crate::agent_teams::list_teams(Some(workspace_id), true).await?;

    let command_runs = crate::command_runs::list_filtered(crate::command_runs::CommandRunFilter {
        limit: Some(limit * 20),
        ..Default::default()
    })
    .await?;

    let legacy_tasks = crate::tasks::load()
        .await?
        .tasks
        .into_iter()
        .filter(|task| {
            task.session_id
                .as_ref()
                .is_some_and(|session_id| session_ids.contains(session_id))
                || task
                    .cwd
                    .as_ref()
                    .is_some_and(|cwd| path_in_workspace(cwd, &root))
                || task
                    .artifact
                    .file
                    .as_deref()
                    .is_some_and(|file| path_in_workspace(file, &root))
        })
        .collect::<Vec<_>>();

    let command_runs = command_runs
        .into_iter()
        .filter(|run| {
            run.session_id
                .as_ref()
                .is_some_and(|session_id| session_ids.contains(session_id))
                || run.agent_run_id.as_ref().is_some_and(|agent_run_id| {
                    agent_runs.iter().any(|run| &run.id == agent_run_id)
                })
                || run.tool_call_id.as_ref().is_some_and(|tool_call_id| {
                    tool_calls.iter().any(|call| &call.id == tool_call_id)
                })
                || file_in_workspace(&run.cwd, &root)
        })
        .collect::<Vec<_>>();

    let mut bundles: HashMap<String, ActivityBundle> = HashMap::new();
    let mut run_to_activity: HashMap<String, String> = HashMap::new();
    let mut tool_call_to_activity: HashMap<String, String> = HashMap::new();
    let mut cycle_to_activities: HashMap<String, Vec<String>> = HashMap::new();
    let mut goal_to_activities: HashMap<String, Vec<String>> = HashMap::new();

    for session in &sessions {
        let messages = crate::chat::get_chat_session_messages(&session.id, Some(40)).await?;
        let assistant_message = messages
            .iter()
            .rev()
            .find(|message| message.role == crate::chat::ChatRole::Assistant)
            .map(|message| compact_text(&message.content, 240))
            .or_else(|| session.last_message_preview.clone());

        if !session.working && assistant_message.is_none() {
            continue;
        }

        let activity_id = format!("chat:{}", session.id);
        let mut bundle = ActivityBundle::new(
            activity_id.clone(),
            "chat_turn",
            if session.working {
                session
                    .working_stage
                    .clone()
                    .unwrap_or_else(|| "working".to_string())
            } else {
                "completed".to_string()
            },
            session.title.clone(),
            "assistant",
            session.created_at,
            session.updated_at,
        );
        bundle.session_id = Some(session.id.clone());
        bundle.assistant_message = assistant_message;
        bundle.active = session.working;
        bundles.insert(activity_id, bundle);
    }

    for task in goal_tasks {
        let activity_id = format!("goal_task:{}", task.id);
        let started_at = task.claimed_at.unwrap_or(task.created_at);
        let updated_at = task.finished_at.unwrap_or(task.updated_at);
        let mut bundle = ActivityBundle::new(
            activity_id.clone(),
            "goal_cycle",
            task.status.clone(),
            task.title.clone(),
            task.claimed_by
                .clone()
                .or(task.assigned_agent_id.clone())
                .unwrap_or_else(|| task.agent_kind.clone()),
            started_at,
            updated_at,
        );
        bundle.goal_id = task.goal_id.clone();
        bundle.task_id = Some(task.id.clone());
        bundle.active = bundle.active || ACTIVE_TASK_STATUSES.contains(&task.status.as_str());
        if task.result_ref.is_some() || task.error.is_some() {
            bundle.push_artifact(ActivityArtifactRef {
                label: "goal_task".to_string(),
                file: None,
                summary_ref: None,
                output_ref: None,
                result_ref: task.result_ref.clone().or(task.error.clone()),
            });
        }
        if let Some(goal_id) = task.goal_id.clone() {
            goal_to_activities
                .entry(goal_id)
                .or_default()
                .push(activity_id.clone());
        }
        if let Some(cycle_id) = task.cycle_id.clone() {
            cycle_to_activities
                .entry(cycle_id)
                .or_default()
                .push(activity_id.clone());
        }
        bundles.insert(activity_id, bundle);
    }

    for run in agent_runs {
        let activity_id = run
            .metadata_json
            .as_ref()
            .and_then(|value| get_json_string(value, "task_id"))
            .map(|task_id| format!("goal_task:{task_id}"))
            .unwrap_or_else(|| format!("agent_run:{}", run.id));
        let started_at = run.started_at;
        let updated_at = run.finished_at.unwrap_or(run.updated_at);
        let bundle = bundles.entry(activity_id.clone()).or_insert_with(|| {
            ActivityBundle::new(
                activity_id.clone(),
                "code_job",
                run.status.as_str().to_string(),
                run.metadata_json
                    .as_ref()
                    .and_then(|value| get_json_string(value, "prompt"))
                    .map(|text| compact_text(&text, 120))
                    .unwrap_or_else(|| format!("{} {}", run.agent_id, run.role)),
                run.agent_id.clone(),
                started_at,
                updated_at,
            )
        });
        bundle.status = run.status.as_str().to_string();
        bundle.actor = run.agent_id.clone();
        bundle.touch(updated_at);
        bundle.active = bundle.active || ACTIVE_AGENT_RUN_STATUSES.contains(&run.status.as_str());
        bundle.agent_runs.push(ActivityAgentRunRef {
            id: run.id.clone(),
            agent_id: run.agent_id.clone(),
            status: run.status.as_str().to_string(),
            output_ref: run.output_ref.clone(),
            error: run.error.clone(),
        });
        if let Some(task_id) = run
            .metadata_json
            .as_ref()
            .and_then(|value| get_json_string(value, "task_id"))
        {
            bundle.task_id = Some(task_id);
        }
        if let Some(prompt) = run
            .metadata_json
            .as_ref()
            .and_then(|value| get_json_string(value, "prompt"))
        {
            bundle.title = compact_text(&prompt, 120);
        }
        if run.output_ref.is_some() || run.error.is_some() {
            bundle.push_artifact(ActivityArtifactRef {
                label: "agent_run".to_string(),
                file: None,
                summary_ref: None,
                output_ref: run.output_ref.clone(),
                result_ref: run.error.clone(),
            });
        }
        run_to_activity.insert(run.id.clone(), activity_id);
    }

    for call in tool_calls {
        let activity_id = call
            .session_id
            .as_ref()
            .map(|session_id| format!("chat:{session_id}"))
            .or_else(|| {
                call.agent_run_id
                    .as_ref()
                    .and_then(|agent_run_id| run_to_activity.get(agent_run_id).cloned())
            })
            .unwrap_or_else(|| format!("tool_call:{}", call.id));
        let bundle = bundles.entry(activity_id.clone()).or_insert_with(|| {
            ActivityBundle::new(
                activity_id.clone(),
                "tool_use",
                call.status.clone(),
                call.tool_id.clone(),
                "tool",
                call.started_at,
                call.completed_at.unwrap_or(call.started_at),
            )
        });
        bundle.kind = if bundle.kind == "tool_use" && call.session_id.is_some() {
            "chat_turn".to_string()
        } else {
            bundle.kind.clone()
        };
        bundle.status = call.status.clone();
        bundle.touch(call.completed_at.unwrap_or(call.started_at));
        bundle.active = bundle.active || ACTIVE_TOOL_CALL_STATUSES.contains(&call.status.as_str());
        if bundle.session_id.is_none() {
            bundle.session_id = call.session_id.clone();
        }
        bundle.tool_calls.push(ActivityToolCallRef {
            id: call.id.clone(),
            tool_id: call.tool_id.clone(),
            status: call.status.clone(),
            command_run_id: call.command_run_id.clone(),
            risk_level: call.risk_level.clone(),
        });
        if bundle.title.is_empty() || bundle.title == bundle.kind {
            bundle.title = call.tool_id.clone();
        }
        if call.output_json.is_some() || call.error.is_some() {
            bundle.push_artifact(ActivityArtifactRef {
                label: "tool_call".to_string(),
                file: None,
                summary_ref: None,
                output_ref: None,
                result_ref: call.error.clone().or(call.output_json.clone()),
            });
        }
        tool_call_to_activity.insert(call.id.clone(), activity_id);
    }

    for run in command_runs {
        let activity_id = run
            .tool_call_id
            .as_ref()
            .and_then(|tool_call_id| tool_call_to_activity.get(tool_call_id).cloned())
            .or_else(|| {
                run.agent_run_id
                    .as_ref()
                    .and_then(|agent_run_id| run_to_activity.get(agent_run_id).cloned())
            })
            .or_else(|| {
                run.session_id
                    .as_ref()
                    .map(|session_id| format!("chat:{session_id}"))
            })
            .unwrap_or_else(|| format!("command_run:{}", run.id));
        let timestamp = run
            .completed_at
            .or(run.started_at)
            .unwrap_or(run.created_at);
        let bundle = bundles.entry(activity_id.clone()).or_insert_with(|| {
            ActivityBundle::new(
                activity_id.clone(),
                "command_run",
                run.status.as_str().to_string(),
                compact_text(&run.command, 120),
                "shell",
                run.started_at.unwrap_or(run.created_at),
                timestamp,
            )
        });
        bundle.status = run.status.as_str().to_string();
        bundle.touch(timestamp);
        bundle.active = bundle.active || ACTIVE_COMMAND_RUN_STATUSES.contains(&run.status.as_str());
        bundle.command_runs.push(ActivityCommandRunRef {
            id: run.id.clone(),
            command: compact_text(&run.command, 120),
            status: run.status.as_str().to_string(),
            exit_code: run.exit_code,
        });
        bundle.push_artifact(ActivityArtifactRef {
            label: "command_run".to_string(),
            file: None,
            summary_ref: None,
            output_ref: None,
            result_ref: Some(compact_text(
                &if run.stderr_tail.trim().is_empty() {
                    run.stdout_tail.clone()
                } else {
                    format!("{} {}", run.stdout_tail, run.stderr_tail)
                },
                160,
            )),
        });
    }

    for task in legacy_tasks {
        let activity_id = task
            .session_id
            .as_ref()
            .map(|session_id| format!("chat:{session_id}"))
            .unwrap_or_else(|| format!("task:{}", task.id));
        let timestamp = task.last_event_at.unwrap_or(task.created_at);
        let bundle = bundles.entry(activity_id.clone()).or_insert_with(|| {
            ActivityBundle::new(
                activity_id.clone(),
                "artifact",
                task.status.as_str().to_string(),
                task.current_request
                    .clone()
                    .or(task.focus_hint.clone())
                    .unwrap_or_else(|| task.kind.clone()),
                task.source.clone(),
                task.created_at,
                timestamp,
            )
        });
        bundle.touch(timestamp);
        bundle.active =
            bundle.active || ACTIVE_LEGACY_TASK_STATUSES.contains(&task.status.as_str());
        if bundle.session_id.is_none() {
            bundle.session_id = task.session_id.clone();
        }
        if bundle.assistant_message.is_none() {
            bundle.assistant_message = task.last_output_summary.clone();
        }
        bundle.push_artifact(ActivityArtifactRef {
            label: task.kind.clone(),
            file: task
                .artifact
                .file
                .as_ref()
                .map(|path| path.display().to_string()),
            summary_ref: task.summary_ref.clone(),
            output_ref: None,
            result_ref: task
                .last_output_summary
                .clone()
                .or(task.permission_summary.clone()),
        });
    }

    for team in agent_teams {
        let started_at = team.created_at;
        let updated_at = team.updated_at;
        let team_ref = ActivityAgentTeamRef {
            id: team.id.clone(),
            name: team.name.clone(),
            lifecycle: team.lifecycle.as_str().to_string(),
        };
        let active_team = ACTIVE_TEAM_LIFECYCLES.contains(&team.lifecycle.as_str());
        let goal_id = team
            .metadata_json
            .as_ref()
            .and_then(|value| get_json_string(value, "goal_id"));
        let cycle_id = team
            .metadata_json
            .as_ref()
            .and_then(|value| get_json_string(value, "cycle_id"));

        let mut matched = false;
        if let Some(cycle_id) = cycle_id.as_deref() {
            if let Some(activity_ids) = cycle_to_activities.get(cycle_id) {
                for activity_id in activity_ids {
                    if let Some(bundle) = bundles.get_mut(activity_id) {
                        bundle.agent_teams.push(team_ref.clone());
                        bundle.active = bundle.active || active_team;
                        bundle.touch(updated_at);
                    }
                }
                matched = !activity_ids.is_empty();
            }
        }
        if !matched {
            if let Some(goal_id) = goal_id.as_deref() {
                if let Some(activity_ids) = goal_to_activities.get(goal_id) {
                    for activity_id in activity_ids {
                        if let Some(bundle) = bundles.get_mut(activity_id) {
                            bundle.agent_teams.push(team_ref.clone());
                            bundle.active = bundle.active || active_team;
                            bundle.touch(updated_at);
                        }
                    }
                    matched = !activity_ids.is_empty();
                }
            }
        }
        if matched {
            continue;
        }

        let activity_id = format!("agent_team:{}", team.id);
        let bundle = bundles.entry(activity_id.clone()).or_insert_with(|| {
            ActivityBundle::new(
                activity_id.clone(),
                "agent_team",
                team.lifecycle.as_str().to_string(),
                team.name.clone(),
                "agent_team",
                started_at,
                updated_at,
            )
        });
        bundle.status = team.lifecycle.as_str().to_string();
        bundle.title = team.name.clone();
        bundle.touch(updated_at);
        bundle.active = bundle.active || active_team;
        if bundle.goal_id.is_none() {
            bundle.goal_id = goal_id;
        }
        bundle.agent_teams.push(team_ref);
    }

    let mut active = Vec::new();
    let mut records = Vec::new();
    for bundle in bundles.into_values() {
        if bundle.active {
            active.push(bundle.clone().into_item());
        }
        if !bundle.active
            && (bundle.assistant_message.is_some()
                || !bundle.tool_calls.is_empty()
                || !bundle.command_runs.is_empty()
                || !bundle.agent_runs.is_empty()
                || !bundle.agent_teams.is_empty()
                || !bundle.artifacts.is_empty())
        {
            records.push(bundle.into_item());
        }
    }

    active.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    records.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    active.truncate(limit as usize);
    records.truncate(limit as usize);

    Ok(WorkspaceActivityProjection {
        workspace_id: workspace_id.to_string(),
        active,
        records,
    })
}

fn path_in_workspace(path: &Path, workspace_root: &Path) -> bool {
    path.starts_with(workspace_root)
}

fn file_in_workspace(path: &str, workspace_root: &Path) -> bool {
    let path = Path::new(path);
    if path.is_absolute() {
        path.starts_with(workspace_root)
    } else {
        true
    }
}

fn get_json_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn compact_text(text: &str, max_len: usize) -> String {
    let normalized = text
        .replace('\n', " ")
        .replace('\r', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    truncate(&normalized, max_len)
}

// ---------------------------------------------------------------------------
// ProjectionWriter
// ---------------------------------------------------------------------------

/// Generates `workspace.md` from the Runtime DB and writes it to disk,
/// preserving any hand-edited content outside the `<!-- PROJECTION -->` markers.
pub struct ProjectionWriter {
    workspace_id: String,
}

impl ProjectionWriter {
    pub fn new(workspace_id: &str) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
        }
    }

    /// Generate the projection markdown string (without file I/O).
    pub async fn generate_workspace_md(&self) -> anyhow::Result<String> {
        let goals = self.fetch_active_goals().await?;
        let tasks = self.fetch_active_tasks().await?;
        let review_tasks = self.fetch_review_tasks().await?;
        let recent_events = self.fetch_recent_events().await?;

        let mut md = String::new();

        // Header
        md.push_str("# Workspace Projection\n\n");
        md.push_str(&format!(
            "> Auto-generated at `{}` for workspace `{}`\n\n",
            Utc::now().to_rfc3339(),
            self.workspace_id,
        ));

        // Active Goals
        md.push_str("## Active Goals\n\n");
        if goals.is_empty() {
            md.push_str("_No active goals._\n\n");
        } else {
            md.push_str("| ID | Title | Status | Priority | Owner |\n");
            md.push_str("|----|-------|--------|----------|-------|\n");
            for g in &goals {
                md.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    truncate(&g.id, 20),
                    truncate(&g.title, 40),
                    g.status,
                    g.priority,
                    g.owner,
                ));
            }
            md.push('\n');
        }

        // Active Tasks
        md.push_str("## Active Tasks\n\n");
        if tasks.is_empty() {
            md.push_str("_No active tasks._\n\n");
        } else {
            md.push_str("| ID | Title | Status | Agent Kind | Assigned To |\n");
            md.push_str("|----|-------|--------|------------|-------------|\n");
            for t in &tasks {
                md.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    truncate(&t.id, 20),
                    truncate(&t.title, 40),
                    t.status,
                    t.agent_kind,
                    t.assigned_agent_id.as_deref().unwrap_or("-"),
                ));
            }
            md.push('\n');
        }

        // Review Queue
        md.push_str("## Review Queue\n\n");
        if review_tasks.is_empty() {
            md.push_str("_No tasks awaiting review._\n\n");
        } else {
            md.push_str("| ID | Title | Status | Agent Kind |\n");
            md.push_str("|----|-------|--------|------------|\n");
            for t in &review_tasks {
                md.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    truncate(&t.id, 20),
                    truncate(&t.title, 40),
                    t.status,
                    t.agent_kind,
                ));
            }
            md.push('\n');
        }

        // Recent Events
        md.push_str("## Recent Events\n\n");
        if recent_events.is_empty() {
            md.push_str("_No recent events._\n\n");
        } else {
            md.push_str("| Timestamp | Event Type | Actor | Subject |\n");
            md.push_str("|-----------|------------|-------|--------|\n");
            for ev in &recent_events {
                md.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    ev.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    ev.event_type,
                    ev.actor,
                    truncate(&ev.target, 30),
                ));
            }
            md.push('\n');
        }

        Ok(md)
    }

    /// Write the projection to `docs/workspace.md`, preserving content outside
    /// the `<!-- PROJECTION START/END -->` markers.
    ///
    /// Returns the path to the written file.
    pub async fn write_to_file(&self) -> anyhow::Result<PathBuf> {
        let content = self.generate_workspace_md().await?;
        let path = workspace_md_path();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Build the full projection block with markers
        let projection_block = format!("{PROJECTION_START}\n{content}{PROJECTION_END}\n");

        // Read existing file (if any) and merge
        let final_content = if path.exists() {
            let existing = fs::read_to_string(&path)
                .await
                .with_context(|| format!("read {}", path.display()))?;

            if let Some(merged) = replace_projection_section(&existing, &projection_block) {
                merged
            } else {
                // No markers found -- append the projection at the end
                format!("{existing}\n{projection_block}")
            }
        } else {
            projection_block
        };

        fs::write(&path, &final_content)
            .await
            .with_context(|| format!("write {}", path.display()))?;

        // Emit event
        let _ = events::append(
            "projection",
            "projection.workspace_md_written",
            &json!({
                "workspace_id": self.workspace_id,
                "path": path.display().to_string(),
            }),
        )
        .await;

        Ok(path)
    }

    // -- private helpers --

    async fn fetch_active_goals(&self) -> anyhow::Result<Vec<GoalRun>> {
        let goals = crate::goals::list_goals(&self.workspace_id, None, None).await?;
        Ok(goals
            .into_iter()
            .filter(|g| ACTIVE_GOAL_STATUSES.contains(&g.status.as_str()))
            .collect())
    }

    async fn fetch_active_tasks(&self) -> anyhow::Result<Vec<AgentTask>> {
        let tasks = crate::goal_tasks::list_tasks(&self.workspace_id).await?;
        Ok(tasks
            .into_iter()
            .filter(|t| ACTIVE_TASK_STATUSES.contains(&t.status.as_str()))
            .collect())
    }

    async fn fetch_review_tasks(&self) -> anyhow::Result<Vec<AgentTask>> {
        let tasks = crate::goal_tasks::list_tasks(&self.workspace_id).await?;
        Ok(tasks
            .into_iter()
            .filter(|t| REVIEW_TASK_STATUSES.contains(&t.status.as_str()))
            .collect())
    }

    async fn fetch_recent_events(&self) -> anyhow::Result<Vec<events::AuditEvent>> {
        events::query_events_db(&self.workspace_id, None, Some(RECENT_EVENT_LIMIT)).await
    }
}

/// Return the path to `docs/workspace.md` under the conductor root.
fn workspace_md_path() -> PathBuf {
    paths::root().join("docs").join("workspace.md")
}

/// Replace the content between `<!-- PROJECTION START -->` and `<!-- PROJECTION END -->`
/// markers. Returns `Some(merged)` if markers were found, `None` otherwise.
fn replace_projection_section(existing: &str, new_block: &str) -> Option<String> {
    let (start_idx, start_line_end) = marker_line_bounds(existing, PROJECTION_START, 0)?;
    let (_, end_idx) = marker_line_bounds(existing, PROJECTION_END, start_line_end)?;

    let mut result = String::with_capacity(existing.len() + new_block.len());
    result.push_str(&existing[..start_idx]);
    result.push_str(new_block);
    result.push_str(&existing[end_idx..]);
    Some(result)
}

fn marker_line_bounds(existing: &str, marker: &str, from_idx: usize) -> Option<(usize, usize)> {
    let mut offset = 0;
    for line in existing.split_inclusive('\n') {
        let line_start = offset;
        let line_end = offset + line.len();
        offset = line_end;

        if line_start < from_idx {
            continue;
        }

        let line_text = line.trim_end_matches(['\r', '\n']);
        if line_text.trim() == marker {
            return Some((line_start, line_end));
        }
    }

    None
}

/// Truncate a string to `max_len` characters, appending "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        ".".repeat(max_len)
    } else {
        let prefix: String = s.chars().take(max_len - 3).collect();
        format!("{prefix}...")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;
    use crate::{
        command_runs::{CommandRun, CommandRunStatus},
        tasks::{Artifact, Task, TaskStatus},
    };
    use chrono::Utc;

    #[test]
    fn replace_projection_section_basic() {
        let existing = "\
# Title

Some hand-edited content here.

<!-- PROJECTION START -->
old projection content
<!-- PROJECTION END -->

More hand-edited content.
";

        let new_block = "\
<!-- PROJECTION START -->
new projection content
<!-- PROJECTION END -->
";

        let result = replace_projection_section(existing, new_block).unwrap();
        assert!(result.contains("new projection content"));
        assert!(!result.contains("old projection content"));
        assert!(result.contains("# Title"));
        assert!(result.contains("Some hand-edited content here."));
        assert!(result.contains("More hand-edited content."));
    }

    #[test]
    fn replace_projection_section_no_markers_returns_none() {
        let existing = "# Just a regular file\n\nNo markers here.\n";
        let new_block = "<!-- PROJECTION START -->\ndata\n<!-- PROJECTION END -->\n";
        assert!(replace_projection_section(existing, new_block).is_none());
    }

    #[test]
    fn replace_projection_section_ignores_inline_marker_examples() {
        let existing = "\
# Dispatch Board

- The projection uses `<!-- PROJECTION START -->` / `<!-- PROJECTION END -->` markers.
";
        let new_block = "<!-- PROJECTION START -->\ndata\n<!-- PROJECTION END -->\n";

        assert!(replace_projection_section(existing, new_block).is_none());
    }

    #[test]
    fn replace_projection_section_preserves_surrounding_content() {
        let before = "# Dispatch Board\n\nRules and instructions...\n\n";
        let after = "\n## Manual Notes\n\nDo not touch this section.\n";
        let old_proj = "<!-- PROJECTION START -->\nold\n<!-- PROJECTION END -->\n";
        let existing = format!("{before}{old_proj}{after}");

        let new_proj = "<!-- PROJECTION START -->\nupdated\n<!-- PROJECTION END -->\n";
        let result = replace_projection_section(&existing, new_proj).unwrap();

        assert!(result.starts_with(before));
        assert!(result.contains("updated"));
        assert!(result.contains("## Manual Notes"));
        assert!(result.contains("Do not touch this section."));
    }

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string_with_ellipsis() {
        let result = truncate("this is a very long string", 10);
        assert_eq!(result, "this is...");
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn truncate_handles_multibyte_utf8_without_panicking() {
        let result = truncate("用户想要使用 Agent Team 来快速扫描整个项目", 12);
        assert_eq!(result, "用户想要使用 Ag...");
        assert_eq!(result.chars().count(), 12);
    }

    #[test]
    fn truncate_tiny_limit_returns_only_ellipsis_chars() {
        assert_eq!(truncate("abcdef", 2), "..");
    }

    #[tokio::test]
    async fn generate_workspace_md_produces_valid_sections() {
        let _root = TestRoot::new();

        let writer = ProjectionWriter::new("ws-test");
        let md = writer
            .generate_workspace_md()
            .await
            .expect("generate_workspace_md");

        assert!(md.contains("# Workspace Projection"));
        assert!(md.contains("## Active Goals"));
        assert!(md.contains("## Active Tasks"));
        assert!(md.contains("## Review Queue"));
        assert!(md.contains("## Recent Events"));
        // With no data, should show empty-state messages
        assert!(md.contains("_No active goals._"));
        assert!(md.contains("_No active tasks._"));
        assert!(md.contains("_No tasks awaiting review._"));
    }

    #[tokio::test]
    async fn write_to_file_creates_markers() {
        let _root = TestRoot::new();

        let writer = ProjectionWriter::new("ws-marker-test");
        let path = writer.write_to_file().await.expect("write_to_file");

        assert!(path.exists());
        let content = fs::read_to_string(&path).await.expect("read");
        assert!(content.contains(PROJECTION_START));
        assert!(content.contains(PROJECTION_END));
        assert!(content.contains("# Workspace Projection"));
    }

    #[tokio::test]
    async fn write_to_file_idempotent_preserves_outside_content() {
        let _root = TestRoot::new();

        let path = workspace_md_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.expect("create docs dir");
        }

        // Write initial file with hand-edited content outside markers
        let initial = "\
# My Dispatch Board

Hand-edited rules go here.

<!-- PROJECTION START -->
old stuff
<!-- PROJECTION END -->

## Manual Notes
Do not overwrite this.
";
        fs::write(&path, initial).await.expect("write initial");

        // Run projection writer
        let writer = ProjectionWriter::new("ws-idempotent");
        writer.write_to_file().await.expect("write_to_file");

        // Verify hand-edited content is preserved
        let content = fs::read_to_string(&path).await.expect("read after");
        assert!(content.contains("# My Dispatch Board"));
        assert!(content.contains("Hand-edited rules go here."));
        assert!(content.contains("## Manual Notes"));
        assert!(content.contains("Do not overwrite this."));
        // Projection content was updated
        assert!(content.contains("# Workspace Projection"));
        assert!(!content.contains("old stuff"));
        // Markers are still present
        assert!(content.contains(PROJECTION_START));
        assert!(content.contains(PROJECTION_END));
    }

    #[tokio::test]
    async fn generate_md_with_real_goal_and_task() {
        let _root = TestRoot::new();

        // Create a goal
        let goal = crate::goals::create_goal(
            "ws-real",
            "Ship Feature X",
            "Deliver the feature by end of week",
            "p1",
            "agent-alpha",
            None,
            None,
        )
        .await
        .expect("create_goal");

        // Move it to running (draft -> planning -> awaiting_plan_approval -> running)
        crate::goals::update_goal_status(&goal.id, "planning")
            .await
            .unwrap();
        crate::goals::update_goal_status(&goal.id, "awaiting_plan_approval")
            .await
            .unwrap();
        crate::goals::update_goal_status(&goal.id, "running")
            .await
            .unwrap();

        // Create a task
        let task = crate::goal_tasks::create_task(
            "ws-real",
            Some(&goal.id),
            None,
            "Implement API endpoint",
            "Write the /api/feature endpoint",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec!["endpoint responds 200".to_string()],
        )
        .await
        .expect("create_task");

        // Claim + start the task
        crate::goal_tasks::claim_task(&task.id, "agent-alpha", 3600)
            .await
            .expect("claim");
        crate::goal_tasks::start_task(&task.id)
            .await
            .expect("start");

        let writer = ProjectionWriter::new("ws-real");
        let md = writer.generate_workspace_md().await.expect("generate");

        // Goal should appear
        assert!(md.contains("Ship Feature X"));
        assert!(md.contains("running"));

        // Task should appear in Active Tasks
        assert!(md.contains("Implement API endpoint"));
        assert!(md.contains("claude_p"));
    }

    #[tokio::test]
    async fn workspace_activity_projection_aggregates_outputs() {
        let root = TestRoot::new();
        let workspace =
            crate::workspaces::create_or_attach(root.path(), Some("Projection WS".into()), None)
                .await
                .expect("workspace");

        let session = crate::chat::create_chat_session(
            Some("Projection Session".to_string()),
            Some(workspace.id.clone()),
        )
        .await
        .expect("chat session");

        let pool = crate::db::pool().await.expect("pool");
        sqlx::query(
            "INSERT INTO chat_messages (id, role, content, created_at, seq, tool_calls, session_id) VALUES (?1, 'assistant', ?2, ?3, ?4, NULL, ?5)",
        )
        .bind("msg-projection")
        .bind("Built workspace projection")
        .bind(Utc::now().to_rfc3339())
        .bind(1_i64)
        .bind(&session.id)
        .execute(&pool)
        .await
        .expect("insert assistant message");

        let tool_call = crate::tool_calls::create(crate::tool_calls::ToolCallCreate {
            id: "tc-projection".to_string(),
            session_id: Some(session.id.clone()),
            workspace_id: Some(workspace.id.clone()),
            turn_id: None,
            llm_tool_call_id: Some("llm-projection".to_string()),
            tool_id: "files.read".to_string(),
            input_json: r#"{"path":"README.md"}"#.to_string(),
            agent_run_id: None,
            risk_level: Some("read_only".to_string()),
        })
        .await
        .expect("tool call");

        let mut command_run = CommandRun::new(
            "cargo check".to_string(),
            workspace.root.display().to_string(),
            Some(session.id.clone()),
        );
        command_run.tool_call_id = Some(tool_call.id.clone());
        command_run
            .transition(CommandRunStatus::Starting)
            .expect("starting");
        command_run
            .transition(CommandRunStatus::Streaming)
            .expect("streaming");
        command_run.stdout_tail = "projection ok".to_string();
        command_run
            .transition(CommandRunStatus::Exited)
            .expect("exited");
        crate::command_runs::insert(&command_run)
            .await
            .expect("insert command run");
        crate::tool_calls::attach_command_run(&tool_call.id, &command_run.id)
            .await
            .expect("attach command");
        crate::tool_calls::complete(&tool_call.id, r#"{"content":"README"}"#)
            .await
            .expect("complete tool call");

        let goal = crate::goals::create_goal(
            &workspace.id,
            "Projection Goal",
            "Aggregate activity and artifacts",
            "normal",
            "user",
            None,
            None,
        )
        .await
        .expect("goal");
        crate::goals::update_goal_status(&goal.id, "planning")
            .await
            .expect("planning");
        let task = crate::goal_tasks::create_task(
            &workspace.id,
            Some(&goal.id),
            None,
            "Render projection drawer",
            "Show unified activity in the drawer",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec!["drawer renders projection".to_string()],
        )
        .await
        .expect("goal task");
        crate::goal_tasks::claim_task(&task.id, "agent-projection", 3600)
            .await
            .expect("claim");
        crate::goal_tasks::start_task(&task.id)
            .await
            .expect("start");

        crate::tasks::add(Task {
            id: "legacy-projection-task".to_string(),
            source: "codex".to_string(),
            kind: "doc_update".to_string(),
            artifact: Artifact {
                file: Some(workspace.root.join("docs").join("workspace.md")),
                anchor: None,
            },
            summary_ref: Some("summary://workspace-projection".to_string()),
            est_minutes: Some(10),
            focus_hint: Some("Refresh workspace projection".to_string()),
            status: TaskStatus::Passed,
            created_at: Utc::now(),
            session_id: Some(session.id.clone()),
            terminal_id: None,
            cwd: Some(workspace.root.clone()),
            current_request: Some("Refresh the workspace projection".to_string()),
            last_output_summary: Some("Workspace projection updated".to_string()),
            last_event_at: Some(Utc::now()),
            permission_summary: None,
        })
        .await
        .expect("legacy task");

        let projection = list_workspace_activities(&workspace.id, Some(10))
            .await
            .expect("projection");

        assert!(projection
            .active
            .iter()
            .any(|item| item.task_id.as_deref() == Some(task.id.as_str())));
        assert!(projection.records.iter().any(|item| {
            item.assistant_message
                .as_deref()
                .is_some_and(|message| message.contains("Built workspace projection"))
        }));
        assert!(projection.records.iter().any(|item| {
            item.tool_calls
                .iter()
                .any(|tool| tool.tool_id == "files.read")
                && item
                    .command_runs
                    .iter()
                    .any(|run| run.command.contains("cargo check"))
        }));
        assert!(projection.records.iter().any(|item| {
            item.artifacts.iter().any(|artifact| {
                artifact.summary_ref.as_deref() == Some("summary://workspace-projection")
            })
        }));
    }

    #[tokio::test]
    async fn workspace_activity_projection_merges_team_run_tool_and_artifact_on_goal_work() {
        let root = TestRoot::new();
        let workspace =
            crate::workspaces::create_or_attach(root.path(), Some("Projection Merge".into()), None)
                .await
                .expect("workspace");

        let goal = crate::goals::create_goal(
            &workspace.id,
            "Projection Merge Goal",
            "Answer who worked, what ran, and where it landed",
            "normal",
            "user",
            None,
            None,
        )
        .await
        .expect("goal");
        crate::goals::update_goal_status(&goal.id, "running")
            .await
            .expect("running");
        let cycle = crate::goals::create_cycle(&goal.id, 1)
            .await
            .expect("cycle");
        crate::goals::advance_cycle_phase(&cycle.id, "executing")
            .await
            .expect("cycle executing");

        let task = crate::goal_tasks::create_task(
            &workspace.id,
            Some(&goal.id),
            Some(&cycle.id),
            "Merged projection task",
            "Build the merged activity record",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec!["projection answers who/what/where".to_string()],
        )
        .await
        .expect("task");
        crate::goal_tasks::claim_task(&task.id, "agent-projection", 3600)
            .await
            .expect("claim");
        crate::goal_tasks::start_task(&task.id)
            .await
            .expect("start");

        let team = crate::agent_teams::create_team(crate::agent_teams::CreateAgentTeamInput {
            id: Some(format!("team-{}", cycle.id)),
            name: "Merged Execution Team".to_string(),
            workspace_id: Some(workspace.id.clone()),
            metadata: Some(json!({
                "cycle_id": cycle.id,
            })),
            ..Default::default()
        })
        .await
        .expect("team");
        crate::agent_teams::add_member(crate::agent_teams::AddAgentTeamMemberInput {
            team_id: team.id.clone(),
            agent_id: "claude_p:merged".to_string(),
            role: "executor".to_string(),
            run_id: None,
            ..Default::default()
        })
        .await
        .expect("add member");
        crate::agent_teams::transition_team_lifecycle(
            &team.id,
            crate::agent_teams::AgentTeamLifecycle::Planning,
        )
        .await
        .expect("planning");
        crate::agent_teams::transition_team_lifecycle(
            &team.id,
            crate::agent_teams::AgentTeamLifecycle::AwaitingPlanApproval,
        )
        .await
        .expect("awaiting");

        let run = crate::agent_runs::AgentRun {
            id: "ar-projection-merged".to_string(),
            agent_id: "claude_p".to_string(),
            role: "task_agent".to_string(),
            workspace_id: Some(workspace.id.clone()),
            cwd: Some(workspace.root.clone()),
            status: crate::agent_runs::AgentRunStatus::Running,
            pid: Some(4242),
            command_json: Some(json!({
                "program": "claude",
                "args": ["-p", "merged projection task"],
            })),
            input_ref: None,
            output_ref: Some("runs/ar-projection-merged-output.json".to_string()),
            error: None,
            started_at: Utc::now(),
            updated_at: Utc::now(),
            finished_at: None,
            metadata_json: Some(json!({
                "task_id": task.id,
                "prompt": "Merged projection task prompt",
            })),
        };
        crate::agent_runs::upsert(&run).await.expect("upsert run");
        crate::agent_teams::bind_member_run_to_task(
            &team.id,
            &task.id,
            &run.id,
            Some(json!({
                "agent_run_id": run.id,
                "task_id": task.id,
            })),
        )
        .await
        .expect("bind run");
        crate::agent_teams::handle_plan_approval_response(
            &team.id,
            crate::agent_teams::PlanApprovalVerdict::Approved,
        )
        .await
        .expect("approve team");

        let tool_call = crate::tool_calls::create(crate::tool_calls::ToolCallCreate {
            id: "tc-projection-merged".to_string(),
            session_id: None,
            workspace_id: Some(workspace.id.clone()),
            turn_id: None,
            llm_tool_call_id: Some("llm-projection-merged".to_string()),
            tool_id: "shell.exec".to_string(),
            input_json: r#"{"command":"cargo check"}"#.to_string(),
            agent_run_id: Some(run.id.clone()),
            risk_level: Some("workspace_write".to_string()),
        })
        .await
        .expect("tool call");
        crate::tool_calls::mark_executing(&tool_call.id)
            .await
            .expect("tool executing");

        let mut command_run = CommandRun::new(
            "cargo check".to_string(),
            workspace.root.display().to_string(),
            None,
        );
        command_run.tool_call_id = Some(tool_call.id.clone());
        command_run.agent_run_id = Some(run.id.clone());
        command_run
            .transition(CommandRunStatus::Starting)
            .expect("starting");
        crate::command_runs::insert(&command_run)
            .await
            .expect("insert command run");
        crate::tool_calls::attach_command_run(&tool_call.id, &command_run.id)
            .await
            .expect("attach command");

        let projection = list_workspace_activities(&workspace.id, Some(10))
            .await
            .expect("projection");
        let merged = projection
            .active
            .iter()
            .find(|item| item.task_id.as_deref() == Some(task.id.as_str()))
            .expect("merged goal task activity");

        assert_eq!(merged.goal_id.as_deref(), Some(goal.id.as_str()));
        assert!(merged
            .agent_runs
            .iter()
            .any(|agent_run| agent_run.id == run.id && agent_run.agent_id == "claude_p"));
        assert!(merged
            .tool_calls
            .iter()
            .any(|tool| tool.id == tool_call.id && tool.tool_id == "shell.exec"));
        assert!(merged
            .command_runs
            .iter()
            .any(|command| command.command.contains("cargo check")));
        assert!(merged.agent_teams.iter().any(|team_ref| {
            team_ref.id == team.id
                && team_ref.name == "Merged Execution Team"
                && team_ref.lifecycle == "executing"
        }));
        assert!(merged.artifacts.iter().any(|artifact| {
            artifact.output_ref.as_deref() == Some("runs/ar-projection-merged-output.json")
        }));
    }
}
