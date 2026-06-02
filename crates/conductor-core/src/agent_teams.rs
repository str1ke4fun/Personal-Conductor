use crate::db;
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTeamStatus {
    Active,
    Archived,
}

impl AgentTeamStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            other => bail!("unknown agent team status: {other}"),
        }
    }
}

/// AgentTeam lifecycle states (OODA-R hard gates).
///
/// Valid transitions:
///   Draft -> Planning
///   Planning -> AwaitingPlanApproval | Executing
///   AwaitingPlanApproval -> Executing (on positive plan_approval_response)
///   AwaitingPlanApproval -> ReworkRequired (on rejected plan_approval_response)
///   Executing -> AwaitingReview (when all members complete)
///   AwaitingReview -> Accepted (on review verdict=accepted)
///   AwaitingReview -> ReworkRequired (on review verdict=failed)
///   ReworkRequired -> Planning (retry cycle)
///   Accepted -> Archived (terminal)
///
/// Illegal transitions (enforced by `validate_transition`):
///   Planning -> Executing is only valid when the approval gate is disabled
///   AwaitingPlanApproval -> Executing without positive approval
///   Executing -> Accepted (must go through review)
///   Any state -> Draft (no going back)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTeamLifecycle {
    Draft,
    Planning,
    AwaitingPlanApproval,
    Executing,
    AwaitingReview,
    Accepted,
    ReworkRequired,
    Archived,
}

impl AgentTeamLifecycle {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Planning => "planning",
            Self::AwaitingPlanApproval => "awaiting_plan_approval",
            Self::Executing => "executing",
            Self::AwaitingReview => "awaiting_review",
            Self::Accepted => "accepted",
            Self::ReworkRequired => "rework_required",
            Self::Archived => "archived",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "draft" => Ok(Self::Draft),
            "planning" => Ok(Self::Planning),
            "awaiting_plan_approval" => Ok(Self::AwaitingPlanApproval),
            "executing" => Ok(Self::Executing),
            "awaiting_review" => Ok(Self::AwaitingReview),
            "accepted" => Ok(Self::Accepted),
            "rework_required" => Ok(Self::ReworkRequired),
            "archived" => Ok(Self::Archived),
            other => bail!("unknown agent team lifecycle: {other}"),
        }
    }

    /// Returns the set of valid next states from this state.
    pub fn valid_transitions(&self) -> Vec<AgentTeamLifecycle> {
        match self {
            Self::Draft => vec![Self::Planning],
            Self::Planning => vec![Self::AwaitingPlanApproval, Self::Executing],
            Self::AwaitingPlanApproval => vec![Self::Executing, Self::ReworkRequired],
            Self::Executing => vec![Self::AwaitingReview],
            Self::AwaitingReview => vec![Self::Accepted, Self::ReworkRequired],
            Self::Accepted => vec![Self::Archived],
            Self::ReworkRequired => vec![Self::Planning],
            Self::Archived => vec![],
        }
    }
}

/// Validate a lifecycle transition. Returns Ok(()) if valid, Err otherwise.
pub fn validate_transition(
    from: &AgentTeamLifecycle,
    to: &AgentTeamLifecycle,
) -> anyhow::Result<()> {
    let valid = from.valid_transitions();
    if valid.contains(to) {
        Ok(())
    } else {
        bail!(
            "illegal lifecycle transition: {} -> {}",
            from.as_str(),
            to.as_str()
        )
    }
}

/// Result of a plan approval response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanApprovalVerdict {
    Approved,
    Rejected,
}

impl PlanApprovalVerdict {
    pub fn from_message_input(
        content: &str,
        metadata: Option<&serde_json::Value>,
    ) -> anyhow::Result<Self> {
        if let Some(verdict) = metadata
            .and_then(|value| value.get("verdict"))
            .and_then(|value| value.as_str())
        {
            return Self::from_str(verdict);
        }
        Self::from_str(content)
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "approved" | "approve" | "accepted" | "accept" | "true" | "yes" => Ok(Self::Approved),
            "rejected" | "reject" | "declined" | "decline" | "false" | "no" => Ok(Self::Rejected),
            other => bail!("unknown plan approval verdict: {other}"),
        }
    }
}

/// Result of a review agent verdict.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Accepted,
    Failed,
}

/// Conflict lock policy for team members controlling write access.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictLockPolicy {
    /// No locking — member can write freely.
    None,
    /// Advisory lock — warns on overlap but does not block.
    Advisory,
    /// Exclusive lock — blocks write if another team holds overlapping scope.
    Exclusive,
}

impl ConflictLockPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Advisory => "advisory",
            Self::Exclusive => "exclusive",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "advisory" => Ok(Self::Advisory),
            "exclusive" => Ok(Self::Exclusive),
            other => bail!("unknown conflict lock policy: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMemberStatus {
    Active,
    Paused,
    Stopped,
}

impl AgentMemberStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "stopped" => Ok(Self::Stopped),
            other => bail!("unknown agent member status: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageKind {
    Message,
    Broadcast,
    ShutdownRequest,
    ShutdownResponse,
    PlanApprovalRequest,
    PlanApprovalResponse,
    ReviewVerdictRequest,
}

impl AgentMessageKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Broadcast => "broadcast",
            Self::ShutdownRequest => "shutdown_request",
            Self::ShutdownResponse => "shutdown_response",
            Self::PlanApprovalRequest => "plan_approval_request",
            Self::PlanApprovalResponse => "plan_approval_response",
            Self::ReviewVerdictRequest => "review_verdict_request",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "message" => Ok(Self::Message),
            "broadcast" => Ok(Self::Broadcast),
            "shutdown_request" => Ok(Self::ShutdownRequest),
            "shutdown_response" => Ok(Self::ShutdownResponse),
            "plan_approval_request" => Ok(Self::PlanApprovalRequest),
            "plan_approval_response" => Ok(Self::PlanApprovalResponse),
            "review_verdict_request" => Ok(Self::ReviewVerdictRequest),
            other => bail!("unknown agent message kind: {other}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentTeam {
    pub id: String,
    pub name: String,
    pub workspace_id: Option<String>,
    pub status: AgentTeamStatus,
    pub lifecycle: AgentTeamLifecycle,
    /// File-level write scope paths (e.g., ["crates/foo/src/bar.rs"]).
    pub write_scope: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata_json: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentTeamMember {
    pub team_id: String,
    pub agent_id: String,
    pub role: String,
    pub run_id: Option<String>,
    pub cwd: Option<PathBuf>,
    pub status: AgentMemberStatus,
    pub subscriptions: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata_json: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentMailboxMessage {
    pub id: String,
    pub team_id: String,
    pub sender_agent_id: String,
    pub recipient_agent_id: Option<String>,
    pub kind: AgentMessageKind,
    pub content: String,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub metadata_json: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CreateAgentTeamInput {
    pub id: Option<String>,
    pub name: String,
    pub workspace_id: Option<String>,
    /// File-level write scope (e.g., ["crates/foo/src/bar.rs"]).
    #[serde(default)]
    pub write_scope: Vec<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AddAgentTeamMemberInput {
    pub team_id: String,
    pub agent_id: String,
    #[serde(default = "default_member_role")]
    pub role: String,
    pub run_id: Option<String>,
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub subscriptions: Vec<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SendAgentMessageInput {
    pub team_id: String,
    #[serde(default = "default_sender")]
    pub sender_agent_id: String,
    pub recipient_agent_id: Option<String>,
    #[serde(default)]
    pub broadcast: bool,
    pub kind: Option<AgentMessageKind>,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AgentMailboxFilter {
    pub team_id: String,
    #[serde(default)]
    pub recipient_agent_id: Option<String>,
    #[serde(default)]
    pub include_read: bool,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentTeamSnapshot {
    pub team: AgentTeam,
    pub members: Vec<AgentTeamMember>,
    pub recent_messages: Vec<AgentMailboxMessage>,
}

fn default_member_role() -> String {
    "assistant".to_string()
}

fn default_sender() -> String {
    "conductor".to_string()
}

pub async fn create_team(input: CreateAgentTeamInput) -> anyhow::Result<AgentTeam> {
    if input.name.trim().is_empty() {
        bail!("team name cannot be empty");
    }

    let now = Utc::now();
    let team = AgentTeam {
        id: input.id.unwrap_or_else(next_team_id),
        name: input.name.trim().to_string(),
        workspace_id: input.workspace_id,
        status: AgentTeamStatus::Active,
        lifecycle: AgentTeamLifecycle::Draft,
        write_scope: input.write_scope,
        created_at: now,
        updated_at: now,
        metadata_json: input.metadata,
    };
    upsert_team(&team).await?;
    Ok(team)
}

pub async fn get_team(id: &str) -> anyhow::Result<AgentTeam> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, name, workspace_id, status, lifecycle, write_scope_json,
               created_at, updated_at, metadata_json
        FROM agent_teams
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .with_context(|| format!("agent team not found: {id}"))?;
    row_to_team(row)
}

pub async fn list_teams(
    workspace_id: Option<&str>,
    include_archived: bool,
) -> anyhow::Result<Vec<AgentTeam>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, name, workspace_id, status, lifecycle, write_scope_json,
               created_at, updated_at, metadata_json
        FROM agent_teams
        ORDER BY updated_at DESC
        "#,
    )
    .fetch_all(&pool)
    .await?;
    let mut teams = rows
        .into_iter()
        .map(row_to_team)
        .collect::<anyhow::Result<Vec<_>>>()?;
    if let Some(workspace_id) = workspace_id {
        teams.retain(|team| team.workspace_id.as_deref() == Some(workspace_id));
    }
    if !include_archived {
        teams.retain(|team| team.status == AgentTeamStatus::Active);
    }
    Ok(teams)
}

pub async fn archive_team(id: &str) -> anyhow::Result<AgentTeam> {
    let mut team = get_team(id).await?;
    team.status = AgentTeamStatus::Archived;
    team.lifecycle = AgentTeamLifecycle::Archived;
    team.updated_at = Utc::now();
    upsert_team(&team).await?;
    Ok(team)
}

pub async fn add_member(input: AddAgentTeamMemberInput) -> anyhow::Result<AgentTeamMember> {
    if input.team_id.trim().is_empty() {
        bail!("team_id cannot be empty");
    }
    if input.agent_id.trim().is_empty() {
        bail!("agent_id cannot be empty");
    }

    get_team(&input.team_id).await?;

    let now = Utc::now();
    let member = AgentTeamMember {
        team_id: input.team_id,
        agent_id: input.agent_id,
        role: if input.role.trim().is_empty() {
            default_member_role()
        } else {
            input.role.trim().to_string()
        },
        run_id: input.run_id,
        cwd: input.cwd,
        status: AgentMemberStatus::Active,
        subscriptions: input.subscriptions,
        created_at: now,
        updated_at: now,
        metadata_json: input.metadata,
    };
    upsert_member(&member).await?;
    Ok(member)
}

pub async fn list_members(team_id: &str) -> anyhow::Result<Vec<AgentTeamMember>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT team_id, agent_id, role, run_id, cwd, status, subscriptions_json,
               created_at, updated_at, metadata_json
        FROM agent_team_members
        WHERE team_id = ?1
        ORDER BY created_at ASC
        "#,
    )
    .bind(team_id)
    .fetch_all(&pool)
    .await?;
    rows.into_iter().map(row_to_member).collect()
}

pub async fn set_member_status(
    team_id: &str,
    agent_id: &str,
    status: AgentMemberStatus,
) -> anyhow::Result<AgentTeamMember> {
    let mut member = get_member(team_id, agent_id).await?;
    member.status = status;
    member.updated_at = Utc::now();
    upsert_member(&member).await?;
    Ok(member)
}

pub async fn bind_member_run_to_task(
    team_id: &str,
    task_id: &str,
    run_id: &str,
    metadata_patch: Option<serde_json::Value>,
) -> anyhow::Result<AgentTeamMember> {
    let mut member = list_members(team_id)
        .await?
        .into_iter()
        .find(|member| {
            member
                .metadata_json
                .as_ref()
                .and_then(|value| value.get("task_id"))
                .and_then(|value| value.as_str())
                == Some(task_id)
        })
        .with_context(|| format!("agent team member not found for task {task_id} in {team_id}"))?;

    member.run_id = Some(run_id.to_string());
    member.updated_at = Utc::now();
    member.metadata_json = merge_metadata(member.metadata_json.take(), metadata_patch)?;
    upsert_member(&member).await?;
    Ok(member)
}

pub async fn send_message(
    input: SendAgentMessageInput,
) -> anyhow::Result<Vec<AgentMailboxMessage>> {
    if input.team_id.trim().is_empty() {
        bail!("team_id cannot be empty");
    }
    if input.content.trim().is_empty() {
        bail!("message content cannot be empty");
    }

    get_team(&input.team_id).await?;
    let kind = input.kind.unwrap_or_else(|| {
        if input.broadcast || input.recipient_agent_id.as_deref() == Some("*") {
            AgentMessageKind::Broadcast
        } else {
            AgentMessageKind::Message
        }
    });

    let recipients = if input.broadcast || input.recipient_agent_id.as_deref() == Some("*") {
        list_members(&input.team_id)
            .await?
            .into_iter()
            .map(|member| Some(member.agent_id))
            .collect::<Vec<_>>()
    } else {
        vec![input.recipient_agent_id.clone()]
    };

    if recipients.is_empty() {
        bail!("broadcast has no team members");
    }

    let created = insert_mailbox_messages(
        &input.team_id,
        &input.sender_agent_id,
        &recipients,
        &kind,
        input.content.trim(),
        input.metadata.clone(),
    )
    .await?;

    if kind == AgentMessageKind::PlanApprovalResponse {
        let verdict =
            PlanApprovalVerdict::from_message_input(&input.content, input.metadata.as_ref())?;
        handle_plan_approval_response(&input.team_id, verdict).await?;
    }

    Ok(created)
}

pub(crate) async fn append_system_message(
    team_id: &str,
    kind: AgentMessageKind,
    content: &str,
    metadata: Option<serde_json::Value>,
) -> anyhow::Result<AgentMailboxMessage> {
    let messages =
        insert_mailbox_messages(team_id, "conductor", &[None], &kind, content, metadata).await?;
    messages
        .into_iter()
        .next()
        .context("system message should create one mailbox record")
}

async fn insert_mailbox_messages(
    team_id: &str,
    sender_agent_id: &str,
    recipients: &[Option<String>],
    kind: &AgentMessageKind,
    content: &str,
    metadata: Option<serde_json::Value>,
) -> anyhow::Result<Vec<AgentMailboxMessage>> {
    let pool = db::pool().await?;
    let mut created = Vec::with_capacity(recipients.len());
    for recipient in recipients {
        let message = AgentMailboxMessage {
            id: next_message_id(),
            team_id: team_id.to_string(),
            sender_agent_id: sender_agent_id.to_string(),
            recipient_agent_id: recipient.clone(),
            kind: kind.clone(),
            content: content.to_string(),
            read_at: None,
            created_at: Utc::now(),
            metadata_json: metadata.clone(),
        };
        insert_message_with_pool(&pool, &message).await?;
        created.push(message);
    }
    Ok(created)
}

pub async fn list_mailbox(filter: AgentMailboxFilter) -> anyhow::Result<Vec<AgentMailboxMessage>> {
    if filter.team_id.trim().is_empty() {
        bail!("team_id cannot be empty");
    }

    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, team_id, sender_agent_id, recipient_agent_id, kind, content,
               read_at, created_at, metadata_json
        FROM agent_mailbox_messages
        WHERE team_id = ?1
        ORDER BY created_at DESC
        LIMIT ?2
        "#,
    )
    .bind(&filter.team_id)
    .bind(filter.limit.unwrap_or(50).clamp(1, 500) as i64)
    .fetch_all(&pool)
    .await?;
    let mut messages = rows
        .into_iter()
        .map(row_to_message)
        .collect::<anyhow::Result<Vec<_>>>()?;
    if let Some(recipient_agent_id) = filter.recipient_agent_id {
        messages.retain(|message| {
            message.recipient_agent_id.as_deref() == Some(recipient_agent_id.as_str())
                || message.recipient_agent_id.is_none()
        });
    }
    if !filter.include_read {
        messages.retain(|message| message.read_at.is_none());
    }
    Ok(messages)
}

pub async fn mark_message_read(id: &str) -> anyhow::Result<AgentMailboxMessage> {
    let pool = db::pool().await?;
    let now = Utc::now();
    sqlx::query(
        r#"
        UPDATE agent_mailbox_messages
        SET read_at = ?1
        WHERE id = ?2
        "#,
    )
    .bind(now.to_rfc3339())
    .bind(id)
    .execute(&pool)
    .await?;
    get_message(id).await
}

/// Transition a team's lifecycle to a new state, validating the transition is legal.
pub async fn transition_team_lifecycle(
    team_id: &str,
    new_lifecycle: AgentTeamLifecycle,
) -> anyhow::Result<AgentTeam> {
    let mut team = get_team(team_id).await?;
    validate_transition(&team.lifecycle, &new_lifecycle)?;
    if new_lifecycle == AgentTeamLifecycle::Executing {
        ensure_active_executor(team_id).await?;
    }
    team.lifecycle = new_lifecycle;
    team.updated_at = Utc::now();
    upsert_team(&team).await?;
    Ok(team)
}

fn member_has_active_executor(member: &AgentTeamMember) -> bool {
    if member
        .run_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return true;
    }

    member
        .metadata_json
        .as_ref()
        .and_then(|value| value.as_object())
        .is_some_and(|metadata| {
            ["task_id", "external_session_id", "session_id"]
                .iter()
                .any(|key| {
                    metadata
                        .get(*key)
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| !value.trim().is_empty())
                })
        })
}

fn merge_metadata(
    current: Option<serde_json::Value>,
    patch: Option<serde_json::Value>,
) -> anyhow::Result<Option<serde_json::Value>> {
    match (current, patch) {
        (None, None) => Ok(None),
        (Some(existing), None) => Ok(Some(existing)),
        (None, Some(patch)) => Ok(Some(patch)),
        (Some(existing), Some(patch)) => {
            let mut merged = existing.as_object().cloned().unwrap_or_default();
            let patch = patch
                .as_object()
                .cloned()
                .context("metadata patch must be a JSON object")?;
            for (key, value) in patch {
                merged.insert(key, value);
            }
            Ok(Some(serde_json::Value::Object(merged)))
        }
    }
}

async fn ensure_active_executor(team_id: &str) -> anyhow::Result<()> {
    let members = list_members(team_id).await?;
    if members.iter().any(member_has_active_executor) {
        return Ok(());
    }

    bail!(
        "team {team_id} cannot enter executing without an active executor (run_id/task_id/external_session_id)"
    )
}

fn linked_goal_id(team: &AgentTeam) -> Option<&str> {
    team.metadata_json
        .as_ref()
        .and_then(|value| value.get("goal_id"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
}

/// Handle a plan_approval_response message: drives a hard state transition.
///
/// - Approved: AwaitingPlanApproval -> Executing
/// - Rejected: AwaitingPlanApproval -> ReworkRequired
pub async fn handle_plan_approval_response(
    team_id: &str,
    verdict: PlanApprovalVerdict,
) -> anyhow::Result<AgentTeam> {
    let team = get_team(team_id).await?;
    if team.lifecycle != AgentTeamLifecycle::AwaitingPlanApproval {
        bail!(
            "team {} is in lifecycle '{}', expected 'awaiting_plan_approval'",
            team_id,
            team.lifecycle.as_str()
        );
    }

    if let Some(goal_id) = linked_goal_id(&team) {
        match verdict {
            PlanApprovalVerdict::Approved => {
                crate::goal_orchestrator::approve_goal_plan(goal_id).await?;
                let orchestrator = crate::goal_orchestrator::GoalOrchestrator::new(
                    crate::goal_orchestrator::OrchestratorConfig {
                        workspace_id: team.workspace_id.clone().unwrap_or_default(),
                        ..Default::default()
                    },
                );
                orchestrator.tick_goal(goal_id).await?;
            }
            PlanApprovalVerdict::Rejected => {
                crate::goal_orchestrator::reject_goal_plan(goal_id).await?;
                transition_team_lifecycle(team_id, AgentTeamLifecycle::ReworkRequired).await?;
            }
        }
        return get_team(team_id).await;
    }

    let target = match verdict {
        PlanApprovalVerdict::Approved => AgentTeamLifecycle::Executing,
        PlanApprovalVerdict::Rejected => AgentTeamLifecycle::ReworkRequired,
    };
    transition_team_lifecycle(team_id, target).await
}

/// Handle a review agent verdict: drives a hard state transition.
///
/// - Accepted: AwaitingReview -> Accepted
/// - Failed: AwaitingReview -> ReworkRequired
pub async fn handle_review_verdict(
    team_id: &str,
    verdict: ReviewVerdict,
) -> anyhow::Result<AgentTeam> {
    let team = get_team(team_id).await?;
    if team.lifecycle != AgentTeamLifecycle::AwaitingReview {
        bail!(
            "team {} is in lifecycle '{}', expected 'awaiting_review'",
            team_id,
            team.lifecycle.as_str()
        );
    }

    if let Some(goal_id) = linked_goal_id(&team) {
        crate::goal_orchestrator::apply_goal_review_verdict(
            goal_id,
            matches!(verdict, ReviewVerdict::Accepted),
        )
        .await?;
        return get_team(team_id).await;
    }

    let target = match verdict {
        ReviewVerdict::Accepted => AgentTeamLifecycle::Accepted,
        ReviewVerdict::Failed => AgentTeamLifecycle::ReworkRequired,
    };
    transition_team_lifecycle(team_id, target).await
}

/// Check for write scope overlap between two teams.
///
/// Returns the set of overlapping file paths. Empty set means no conflict.
pub fn detect_write_scope_overlap(scope_a: &[String], scope_b: &[String]) -> HashSet<String> {
    let set_a: HashSet<&String> = scope_a.iter().collect();
    let set_b: HashSet<&String> = scope_b.iter().collect();
    set_a.intersection(&set_b).map(|s| (*s).clone()).collect()
}

/// Check if a team can acquire a write lock on the given paths.
///
/// Returns Ok(()) if no other active team holds an exclusive lock on overlapping paths,
/// or Err with the conflicting team IDs if a conflict is detected.
pub async fn check_write_scope_conflict(
    team_id: &str,
    write_scope: &[String],
) -> anyhow::Result<()> {
    let all_teams = list_teams(None, false).await?;
    for other in &all_teams {
        if other.id == team_id {
            continue;
        }
        // Only check teams in active execution phases
        if !matches!(
            other.lifecycle,
            AgentTeamLifecycle::Executing | AgentTeamLifecycle::AwaitingReview
        ) {
            continue;
        }
        let overlap = detect_write_scope_overlap(write_scope, &other.write_scope);
        if !overlap.is_empty() {
            bail!(
                "write scope conflict: team '{}' overlaps with team '{}' on paths: {:?}",
                team_id,
                other.id,
                overlap
            );
        }
    }
    Ok(())
}

pub async fn snapshot(team_id: &str, message_limit: u32) -> anyhow::Result<AgentTeamSnapshot> {
    Ok(AgentTeamSnapshot {
        team: get_team(team_id).await?,
        members: list_members(team_id).await?,
        recent_messages: list_mailbox(AgentMailboxFilter {
            team_id: team_id.to_string(),
            recipient_agent_id: None,
            include_read: true,
            limit: Some(message_limit),
        })
        .await?,
    })
}

async fn get_member(team_id: &str, agent_id: &str) -> anyhow::Result<AgentTeamMember> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT team_id, agent_id, role, run_id, cwd, status, subscriptions_json,
               created_at, updated_at, metadata_json
        FROM agent_team_members
        WHERE team_id = ?1 AND agent_id = ?2
        "#,
    )
    .bind(team_id)
    .bind(agent_id)
    .fetch_one(&pool)
    .await
    .with_context(|| format!("agent team member not found: {team_id}/{agent_id}"))?;
    row_to_member(row)
}

async fn get_message(id: &str) -> anyhow::Result<AgentMailboxMessage> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, team_id, sender_agent_id, recipient_agent_id, kind, content,
               read_at, created_at, metadata_json
        FROM agent_mailbox_messages
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .with_context(|| format!("agent mailbox message not found: {id}"))?;
    row_to_message(row)
}

async fn upsert_team(team: &AgentTeam) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let write_scope_json = serde_json::to_string(&team.write_scope)?;
    sqlx::query(
        r#"
        INSERT INTO agent_teams (
            id, name, workspace_id, status, lifecycle, write_scope_json,
            created_at, updated_at, metadata_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            workspace_id = excluded.workspace_id,
            status = excluded.status,
            lifecycle = excluded.lifecycle,
            write_scope_json = excluded.write_scope_json,
            updated_at = excluded.updated_at,
            metadata_json = excluded.metadata_json
        "#,
    )
    .bind(&team.id)
    .bind(&team.name)
    .bind(&team.workspace_id)
    .bind(team.status.as_str())
    .bind(team.lifecycle.as_str())
    .bind(&write_scope_json)
    .bind(team.created_at.to_rfc3339())
    .bind(team.updated_at.to_rfc3339())
    .bind(
        team.metadata_json
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
    )
    .execute(&pool)
    .await?;
    Ok(())
}

async fn upsert_member(member: &AgentTeamMember) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        INSERT INTO agent_team_members (
            team_id, agent_id, role, run_id, cwd, status, subscriptions_json,
            created_at, updated_at, metadata_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(team_id, agent_id) DO UPDATE SET
            role = excluded.role,
            run_id = excluded.run_id,
            cwd = excluded.cwd,
            status = excluded.status,
            subscriptions_json = excluded.subscriptions_json,
            updated_at = excluded.updated_at,
            metadata_json = excluded.metadata_json
        "#,
    )
    .bind(&member.team_id)
    .bind(&member.agent_id)
    .bind(&member.role)
    .bind(&member.run_id)
    .bind(member.cwd.as_ref().map(|path| path.display().to_string()))
    .bind(member.status.as_str())
    .bind(serde_json::to_string(&member.subscriptions)?)
    .bind(member.created_at.to_rfc3339())
    .bind(member.updated_at.to_rfc3339())
    .bind(
        member
            .metadata_json
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
    )
    .execute(&pool)
    .await?;
    Ok(())
}

async fn insert_message_with_pool(
    pool: &SqlitePool,
    message: &AgentMailboxMessage,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO agent_mailbox_messages (
            id, team_id, sender_agent_id, recipient_agent_id, kind, content,
            read_at, created_at, metadata_json
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )
    .bind(&message.id)
    .bind(&message.team_id)
    .bind(&message.sender_agent_id)
    .bind(&message.recipient_agent_id)
    .bind(message.kind.as_str())
    .bind(&message.content)
    .bind(message.read_at.map(|value| value.to_rfc3339()))
    .bind(message.created_at.to_rfc3339())
    .bind(
        message
            .metadata_json
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
    )
    .execute(pool)
    .await?;
    Ok(())
}

fn row_to_team(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<AgentTeam> {
    let lifecycle_str: String = row.try_get("lifecycle")?;
    let write_scope_json: String = row.try_get("write_scope_json")?;
    Ok(AgentTeam {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        workspace_id: row.try_get("workspace_id")?,
        status: AgentTeamStatus::from_str(row.try_get::<String, _>("status")?.as_str())?,
        lifecycle: AgentTeamLifecycle::from_str(&lifecycle_str)?,
        write_scope: serde_json::from_str(&write_scope_json).unwrap_or_default(),
        created_at: parse_datetime(row.try_get::<String, _>("created_at")?.as_str())?,
        updated_at: parse_datetime(row.try_get::<String, _>("updated_at")?.as_str())?,
        metadata_json: parse_json(row.try_get("metadata_json")?)?,
    })
}

fn row_to_member(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<AgentTeamMember> {
    let subscriptions_json: String = row.try_get("subscriptions_json")?;
    Ok(AgentTeamMember {
        team_id: row.try_get("team_id")?,
        agent_id: row.try_get("agent_id")?,
        role: row.try_get("role")?,
        run_id: row.try_get("run_id")?,
        cwd: row.try_get::<Option<String>, _>("cwd")?.map(PathBuf::from),
        status: AgentMemberStatus::from_str(row.try_get::<String, _>("status")?.as_str())?,
        subscriptions: serde_json::from_str(&subscriptions_json).unwrap_or_default(),
        created_at: parse_datetime(row.try_get::<String, _>("created_at")?.as_str())?,
        updated_at: parse_datetime(row.try_get::<String, _>("updated_at")?.as_str())?,
        metadata_json: parse_json(row.try_get("metadata_json")?)?,
    })
}

fn row_to_message(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<AgentMailboxMessage> {
    Ok(AgentMailboxMessage {
        id: row.try_get("id")?,
        team_id: row.try_get("team_id")?,
        sender_agent_id: row.try_get("sender_agent_id")?,
        recipient_agent_id: row.try_get("recipient_agent_id")?,
        kind: AgentMessageKind::from_str(row.try_get::<String, _>("kind")?.as_str())?,
        content: row.try_get("content")?,
        read_at: row
            .try_get::<Option<String>, _>("read_at")?
            .map(|value| parse_datetime(&value))
            .transpose()?,
        created_at: parse_datetime(row.try_get::<String, _>("created_at")?.as_str())?,
        metadata_json: parse_json(row.try_get("metadata_json")?)?,
    })
}

fn parse_datetime(value: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

fn parse_json(value: Option<String>) -> anyhow::Result<Option<serde_json::Value>> {
    value
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(Into::into)
}

fn next_team_id() -> String {
    format!("team-{}", Uuid::new_v4())
}

fn next_message_id() -> String {
    format!("msg-{}", Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;
    use serde_json::json;

    async fn add_executor_member(team_id: &str, agent_id: &str) -> AgentTeamMember {
        add_member(AddAgentTeamMemberInput {
            team_id: team_id.to_string(),
            agent_id: agent_id.to_string(),
            role: "assistant".to_string(),
            run_id: Some(format!("ar-{agent_id}")),
            cwd: None,
            subscriptions: vec![],
            metadata: Some(json!({
                "task_id": format!("task-{agent_id}"),
                "external_session_id": format!("session-{agent_id}")
            })),
        })
        .await
        .expect("add executor member")
    }

    #[tokio::test]
    async fn team_member_and_mailbox_round_trip() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-test".to_string()),
            name: "Test Team".to_string(),
            workspace_id: Some("ws-test".to_string()),
            write_scope: vec!["src/foo.rs".to_string()],
            metadata: Some(serde_json::json!({ "purpose": "test" })),
        })
        .await
        .expect("create team");
        assert_eq!(team.status, AgentTeamStatus::Active);
        assert_eq!(team.lifecycle, AgentTeamLifecycle::Draft);
        assert_eq!(team.write_scope, vec!["src/foo.rs".to_string()]);

        let member = add_member(AddAgentTeamMemberInput {
            team_id: team.id.clone(),
            agent_id: "writer".to_string(),
            role: "document_writer".to_string(),
            run_id: Some("ar-1".to_string()),
            cwd: Some(PathBuf::from("I:/personal-agent")),
            subscriptions: vec!["plan".to_string()],
            metadata: None,
        })
        .await
        .expect("add member");
        assert_eq!(member.agent_id, "writer");

        let messages = send_message(SendAgentMessageInput {
            team_id: team.id.clone(),
            sender_agent_id: "conductor".to_string(),
            recipient_agent_id: Some("writer".to_string()),
            broadcast: false,
            kind: Some(AgentMessageKind::PlanApprovalRequest),
            content: "Review this plan".to_string(),
            metadata: None,
        })
        .await
        .expect("send message");
        assert_eq!(messages.len(), 1);

        let unread = list_mailbox(AgentMailboxFilter {
            team_id: team.id.clone(),
            recipient_agent_id: Some("writer".to_string()),
            include_read: false,
            limit: None,
        })
        .await
        .expect("list mailbox");
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].content, "Review this plan");

        let read = mark_message_read(&unread[0].id).await.expect("mark read");
        assert!(read.read_at.is_some());
        let unread_after = list_mailbox(AgentMailboxFilter {
            team_id: team.id.clone(),
            recipient_agent_id: Some("writer".to_string()),
            include_read: false,
            limit: None,
        })
        .await
        .expect("list unread");
        assert!(unread_after.is_empty());
    }

    #[tokio::test]
    async fn broadcast_creates_message_per_member() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-broadcast".to_string()),
            name: "Broadcast Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");
        for agent_id in ["one", "two"] {
            add_member(AddAgentTeamMemberInput {
                team_id: team.id.clone(),
                agent_id: agent_id.to_string(),
                ..Default::default()
            })
            .await
            .expect("add member");
        }

        let messages = send_message(SendAgentMessageInput {
            team_id: team.id.clone(),
            recipient_agent_id: Some("*".to_string()),
            content: "Stand down".to_string(),
            ..Default::default()
        })
        .await
        .expect("broadcast");

        assert_eq!(messages.len(), 2);
        assert!(messages
            .iter()
            .all(|message| message.kind == AgentMessageKind::Broadcast));
    }

    // ── Lifecycle state machine tests ──

    #[test]
    fn valid_transition_draft_to_planning() {
        assert!(
            validate_transition(&AgentTeamLifecycle::Draft, &AgentTeamLifecycle::Planning).is_ok()
        );
    }

    #[test]
    fn valid_transition_full_lifecycle() {
        // Draft -> Planning -> AwaitingPlanApproval -> Executing -> AwaitingReview -> Accepted -> Archived
        let transitions = [
            (AgentTeamLifecycle::Draft, AgentTeamLifecycle::Planning),
            (
                AgentTeamLifecycle::Planning,
                AgentTeamLifecycle::AwaitingPlanApproval,
            ),
            (
                AgentTeamLifecycle::AwaitingPlanApproval,
                AgentTeamLifecycle::Executing,
            ),
            (
                AgentTeamLifecycle::Executing,
                AgentTeamLifecycle::AwaitingReview,
            ),
            (
                AgentTeamLifecycle::AwaitingReview,
                AgentTeamLifecycle::Accepted,
            ),
            (AgentTeamLifecycle::Accepted, AgentTeamLifecycle::Archived),
        ];
        for (from, to) in &transitions {
            assert!(
                validate_transition(from, to).is_ok(),
                "expected valid: {} -> {}",
                from.as_str(),
                to.as_str()
            );
        }
    }

    #[test]
    fn valid_transition_planning_to_executing_without_approval_gate() {
        assert!(
            validate_transition(
                &AgentTeamLifecycle::Planning,
                &AgentTeamLifecycle::Executing,
            )
            .is_ok(),
            "planning should be able to enter executing when approval is disabled"
        );
    }

    #[test]
    fn invalid_transition_planning_to_accepted() {
        let result =
            validate_transition(&AgentTeamLifecycle::Planning, &AgentTeamLifecycle::Accepted);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("illegal lifecycle transition"),
            "error should mention illegal transition"
        );
    }

    #[test]
    fn invalid_transition_executing_to_accepted_without_review() {
        let result = validate_transition(
            &AgentTeamLifecycle::Executing,
            &AgentTeamLifecycle::Accepted,
        );
        assert!(result.is_err());
    }

    #[test]
    fn invalid_transition_backward_to_draft() {
        for state in [
            AgentTeamLifecycle::Planning,
            AgentTeamLifecycle::AwaitingPlanApproval,
            AgentTeamLifecycle::Executing,
            AgentTeamLifecycle::AwaitingReview,
            AgentTeamLifecycle::Accepted,
            AgentTeamLifecycle::ReworkRequired,
        ] {
            let result = validate_transition(&state, &AgentTeamLifecycle::Draft);
            assert!(
                result.is_err(),
                "expected invalid: {} -> draft",
                state.as_str()
            );
        }
    }

    #[test]
    fn rework_required_can_go_back_to_planning() {
        assert!(validate_transition(
            &AgentTeamLifecycle::ReworkRequired,
            &AgentTeamLifecycle::Planning
        )
        .is_ok());
    }

    #[test]
    fn archived_is_terminal() {
        for target in [
            AgentTeamLifecycle::Draft,
            AgentTeamLifecycle::Planning,
            AgentTeamLifecycle::Executing,
            AgentTeamLifecycle::Accepted,
        ] {
            let result = validate_transition(&AgentTeamLifecycle::Archived, &target);
            assert!(
                result.is_err(),
                "archived should be terminal, but got: archived -> {}",
                target.as_str()
            );
        }
    }

    #[tokio::test]
    async fn plan_approval_approved_transitions_to_executing() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-plan-approved".to_string()),
            name: "Plan Approved Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");
        add_executor_member(&team.id, "planner").await;

        // Advance to AwaitingPlanApproval
        let team = transition_team_lifecycle(&team.id, AgentTeamLifecycle::Planning)
            .await
            .expect("to planning");
        let team = transition_team_lifecycle(&team.id, AgentTeamLifecycle::AwaitingPlanApproval)
            .await
            .expect("to awaiting_plan_approval");

        // Handle approval
        let team = handle_plan_approval_response(&team.id, PlanApprovalVerdict::Approved)
            .await
            .expect("approve");
        assert_eq!(team.lifecycle, AgentTeamLifecycle::Executing);
    }

    #[tokio::test]
    async fn plan_approval_rejected_transitions_to_rework_required() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-plan-rejected".to_string()),
            name: "Plan Rejected Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");

        // Advance to AwaitingPlanApproval
        transition_team_lifecycle(&team.id, AgentTeamLifecycle::Planning)
            .await
            .expect("to planning");
        transition_team_lifecycle(&team.id, AgentTeamLifecycle::AwaitingPlanApproval)
            .await
            .expect("to awaiting_plan_approval");

        // Handle rejection
        let team = handle_plan_approval_response(&team.id, PlanApprovalVerdict::Rejected)
            .await
            .expect("reject");
        assert_eq!(team.lifecycle, AgentTeamLifecycle::ReworkRequired);
    }

    #[tokio::test]
    async fn review_verdict_accepted_transitions_to_accepted() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-review-accepted".to_string()),
            name: "Review Accepted Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");
        add_executor_member(&team.id, "reviewer").await;

        // Advance to AwaitingReview
        for next in [
            AgentTeamLifecycle::Planning,
            AgentTeamLifecycle::AwaitingPlanApproval,
            AgentTeamLifecycle::Executing,
            AgentTeamLifecycle::AwaitingReview,
        ] {
            transition_team_lifecycle(&team.id, next.clone())
                .await
                .unwrap_or_else(|e| panic!("transition to {}: {}", next.as_str(), e));
        }

        // Handle review accepted
        let team = handle_review_verdict(&team.id, ReviewVerdict::Accepted)
            .await
            .expect("review accepted");
        assert_eq!(team.lifecycle, AgentTeamLifecycle::Accepted);
    }

    #[tokio::test]
    async fn review_verdict_failed_transitions_to_rework_required() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-review-failed".to_string()),
            name: "Review Failed Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");
        add_executor_member(&team.id, "reviewer-failed").await;

        // Advance to AwaitingReview
        for next in [
            AgentTeamLifecycle::Planning,
            AgentTeamLifecycle::AwaitingPlanApproval,
            AgentTeamLifecycle::Executing,
            AgentTeamLifecycle::AwaitingReview,
        ] {
            transition_team_lifecycle(&team.id, next.clone())
                .await
                .expect("transition");
        }

        // Handle review failed
        let team = handle_review_verdict(&team.id, ReviewVerdict::Failed)
            .await
            .expect("review failed");
        assert_eq!(team.lifecycle, AgentTeamLifecycle::ReworkRequired);
    }

    #[tokio::test]
    async fn handle_plan_approval_wrong_state_errors() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-wrong-state".to_string()),
            name: "Wrong State Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");

        // Team is in Draft, not AwaitingPlanApproval
        let result = handle_plan_approval_response(&team.id, PlanApprovalVerdict::Approved).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected 'awaiting_plan_approval'"));
    }

    #[tokio::test]
    async fn handle_review_verdict_wrong_state_errors() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-review-wrong".to_string()),
            name: "Review Wrong State Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");

        // Team is in Draft, not AwaitingReview
        let result = handle_review_verdict(&team.id, ReviewVerdict::Accepted).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected 'awaiting_review'"));
    }

    #[tokio::test]
    async fn rework_cycle_back_to_planning() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-rework-cycle".to_string()),
            name: "Rework Cycle Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");

        // Full cycle: Draft -> Planning -> AwaitingPlanApproval -> ReworkRequired -> Planning
        transition_team_lifecycle(&team.id, AgentTeamLifecycle::Planning)
            .await
            .expect("to planning");
        transition_team_lifecycle(&team.id, AgentTeamLifecycle::AwaitingPlanApproval)
            .await
            .expect("to awaiting");
        handle_plan_approval_response(&team.id, PlanApprovalVerdict::Rejected)
            .await
            .expect("reject");
        let team = transition_team_lifecycle(&team.id, AgentTeamLifecycle::Planning)
            .await
            .expect("back to planning");
        assert_eq!(team.lifecycle, AgentTeamLifecycle::Planning);
    }

    #[test]
    fn write_scope_overlap_detection() {
        let scope_a = vec![
            "crates/foo/src/bar.rs".to_string(),
            "crates/foo/src/baz.rs".to_string(),
        ];
        let scope_b = vec![
            "crates/foo/src/baz.rs".to_string(),
            "crates/foo/src/qux.rs".to_string(),
        ];
        let overlap = detect_write_scope_overlap(&scope_a, &scope_b);
        assert_eq!(overlap.len(), 1);
        assert!(overlap.contains("crates/foo/src/baz.rs"));
    }

    #[test]
    fn write_scope_no_overlap() {
        let scope_a = vec!["crates/foo/src/bar.rs".to_string()];
        let scope_b = vec!["crates/foo/src/qux.rs".to_string()];
        let overlap = detect_write_scope_overlap(&scope_a, &scope_b);
        assert!(overlap.is_empty());
    }

    #[tokio::test]
    async fn write_scope_conflict_detection_between_teams() {
        let _root = TestRoot::new();

        // Create team A executing with a write scope
        let team_a = create_team(CreateAgentTeamInput {
            id: Some("team-a".to_string()),
            name: "Team A".to_string(),
            write_scope: vec!["crates/shared/src/lib.rs".to_string()],
            ..Default::default()
        })
        .await
        .expect("create team A");
        add_executor_member(&team_a.id, "team-a-executor").await;
        for next in [
            AgentTeamLifecycle::Planning,
            AgentTeamLifecycle::AwaitingPlanApproval,
            AgentTeamLifecycle::Executing,
        ] {
            transition_team_lifecycle(&team_a.id, next.clone())
                .await
                .expect("transition A");
        }

        // Create team B with overlapping scope
        let team_b = create_team(CreateAgentTeamInput {
            id: Some("team-b".to_string()),
            name: "Team B".to_string(),
            write_scope: vec![
                "crates/shared/src/lib.rs".to_string(),
                "crates/other/src/main.rs".to_string(),
            ],
            ..Default::default()
        })
        .await
        .expect("create team B");

        // check_write_scope_conflict should detect the overlap
        let result = check_write_scope_conflict(&team_b.id, &team_b.write_scope).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("write scope conflict"));
    }

    #[tokio::test]
    async fn write_scope_no_conflict_non_executing_team() {
        let _root = TestRoot::new();

        // Create team A in Draft (not executing) with a write scope
        let team_a = create_team(CreateAgentTeamInput {
            id: Some("team-a-draft".to_string()),
            name: "Team A Draft".to_string(),
            write_scope: vec!["crates/shared/src/lib.rs".to_string()],
            ..Default::default()
        })
        .await
        .expect("create team A");

        // Create team B with overlapping scope
        let team_b = create_team(CreateAgentTeamInput {
            id: Some("team-b-draft".to_string()),
            name: "Team B Draft".to_string(),
            write_scope: vec!["crates/shared/src/lib.rs".to_string()],
            ..Default::default()
        })
        .await
        .expect("create team B");

        // No conflict because team A is not executing
        let result = check_write_scope_conflict(&team_b.id, &team_b.write_scope).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn plan_approval_response_drives_hard_state_transition() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-hard-transition".to_string()),
            name: "Hard Transition Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");
        add_executor_member(&team.id, "hard-transition").await;

        // Verify initial state
        assert_eq!(team.lifecycle, AgentTeamLifecycle::Draft);

        // Transition through the approval gate
        transition_team_lifecycle(&team.id, AgentTeamLifecycle::Planning)
            .await
            .expect("planning");
        transition_team_lifecycle(&team.id, AgentTeamLifecycle::AwaitingPlanApproval)
            .await
            .expect("awaiting");

        // The plan_approval_response drives a hard state change
        let team = handle_plan_approval_response(&team.id, PlanApprovalVerdict::Approved)
            .await
            .expect("approved");
        assert_eq!(team.lifecycle, AgentTeamLifecycle::Executing);

        // Verify the team in DB reflects the new state
        let fetched = get_team(&team.id).await.expect("get team");
        assert_eq!(fetched.lifecycle, AgentTeamLifecycle::Executing);
    }

    #[tokio::test]
    async fn send_plan_approval_response_message_drives_transition() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-mailbox-approval".to_string()),
            name: "Mailbox Approval Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");

        add_executor_member(&team.id, "executor").await;
        transition_team_lifecycle(&team.id, AgentTeamLifecycle::Planning)
            .await
            .expect("planning");
        transition_team_lifecycle(&team.id, AgentTeamLifecycle::AwaitingPlanApproval)
            .await
            .expect("awaiting approval");

        let messages = send_message(SendAgentMessageInput {
            team_id: team.id.clone(),
            sender_agent_id: "reviewer".to_string(),
            recipient_agent_id: Some("executor".to_string()),
            kind: Some(AgentMessageKind::PlanApprovalResponse),
            content: "approved".to_string(),
            metadata: Some(json!({ "verdict": "approved" })),
            ..Default::default()
        })
        .await
        .expect("send approval response");

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].kind, AgentMessageKind::PlanApprovalResponse);

        let fetched = get_team(&team.id).await.expect("get team");
        assert_eq!(fetched.lifecycle, AgentTeamLifecycle::Executing);
    }

    #[tokio::test]
    async fn plan_approval_approved_without_executor_stays_blocked() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-no-executor".to_string()),
            name: "No Executor Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");

        transition_team_lifecycle(&team.id, AgentTeamLifecycle::Planning)
            .await
            .expect("planning");
        transition_team_lifecycle(&team.id, AgentTeamLifecycle::AwaitingPlanApproval)
            .await
            .expect("awaiting approval");

        let err = handle_plan_approval_response(&team.id, PlanApprovalVerdict::Approved)
            .await
            .expect_err("executing should be rejected without executor");
        assert!(err.to_string().contains("active executor"));

        let fetched = get_team(&team.id).await.expect("get team");
        assert_eq!(fetched.lifecycle, AgentTeamLifecycle::AwaitingPlanApproval);
    }

    #[tokio::test]
    async fn bind_member_run_to_task_updates_executor_member() {
        let _root = TestRoot::new();
        let team = create_team(CreateAgentTeamInput {
            id: Some("team-bind-run".to_string()),
            name: "Bind Run Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");

        add_member(AddAgentTeamMemberInput {
            team_id: team.id.clone(),
            agent_id: "claude_p:task-1".to_string(),
            role: "executor".to_string(),
            run_id: None,
            cwd: None,
            subscriptions: vec![],
            metadata: Some(json!({ "task_id": "task-1", "goal_id": "goal-1" })),
        })
        .await
        .expect("add member");

        let member = bind_member_run_to_task(
            &team.id,
            "task-1",
            "run-123",
            Some(json!({ "agent_run_id": "run-123", "session_id": "session-9" })),
        )
        .await
        .expect("bind run");

        assert_eq!(member.run_id.as_deref(), Some("run-123"));
        assert_eq!(member.metadata_json.as_ref().unwrap()["task_id"], "task-1");
        assert_eq!(
            member.metadata_json.as_ref().unwrap()["agent_run_id"],
            "run-123"
        );
        assert_eq!(
            member.metadata_json.as_ref().unwrap()["session_id"],
            "session-9"
        );
    }

    #[test]
    fn member_executor_detection_accepts_run_and_metadata_executor() {
        let member = AgentTeamMember {
            team_id: "team-1".to_string(),
            agent_id: "agent-1".to_string(),
            role: "assistant".to_string(),
            run_id: None,
            cwd: None,
            status: AgentMemberStatus::Active,
            subscriptions: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata_json: Some(json!({ "task_id": "task-1" })),
        };
        assert!(member_has_active_executor(&member));

        let empty_member = AgentTeamMember {
            metadata_json: Some(json!({ "note": "no executor" })),
            ..member
        };
        assert!(!member_has_active_executor(&empty_member));
    }

    #[tokio::test]
    async fn goal_linked_team_plan_approval_dispatches_goal_work() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-team-approval",
            "Goal-linked team approval",
            "route team approval through the goal orchestrator",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");
        crate::goals::update_goal_status(&goal.id, "planning")
            .await
            .expect("planning");

        let orchestrator = crate::goal_orchestrator::GoalOrchestrator::new(
            crate::goal_orchestrator::OrchestratorConfig {
                workspace_id: "ws-team-approval".to_string(),
                require_plan_approval: true,
                ..Default::default()
            },
        );

        for _ in 0..4 {
            orchestrator
                .tick_goal(&goal.id)
                .await
                .expect("tick to approval");
        }

        let waiting_goal = crate::goals::get_goal(&goal.id)
            .await
            .expect("get goal")
            .expect("goal exists");
        let cycle_id = waiting_goal
            .current_cycle_id
            .clone()
            .expect("current cycle id");
        let team_id = format!("team-{cycle_id}");

        let team = handle_plan_approval_response(&team_id, PlanApprovalVerdict::Approved)
            .await
            .expect("approve linked team plan");
        let running_goal = crate::goals::get_goal(&goal.id)
            .await
            .expect("get running goal")
            .expect("goal exists");
        let running_cycle = crate::goals::get_cycle(&cycle_id)
            .await
            .expect("get cycle")
            .expect("cycle exists");
        let tasks = crate::goal_tasks::list_tasks_by_cycle(&cycle_id)
            .await
            .expect("list tasks");
        let members = list_members(&team_id).await.expect("list team members");

        assert_eq!(running_goal.status, "running");
        assert_eq!(running_cycle.status, "executing");
        assert_eq!(team.lifecycle, AgentTeamLifecycle::Executing);
        assert!(!tasks.is_empty(), "approval should materialize queued work");
        assert!(
            members.iter().any(member_has_active_executor),
            "approval should bind executor members before entering executing"
        );
    }

    #[tokio::test]
    async fn goal_linked_team_review_verdict_updates_goal_and_team() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-team-review",
            "Goal-linked team review",
            "route team review through the goal orchestrator",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");
        crate::goals::update_goal_status(&goal.id, "planning")
            .await
            .expect("planning");

        let orchestrator = crate::goal_orchestrator::GoalOrchestrator::new(
            crate::goal_orchestrator::OrchestratorConfig {
                workspace_id: "ws-team-review".to_string(),
                require_plan_approval: true,
                ..Default::default()
            },
        );

        for _ in 0..4 {
            orchestrator
                .tick_goal(&goal.id)
                .await
                .expect("tick to approval");
        }

        let waiting_goal = crate::goals::get_goal(&goal.id)
            .await
            .expect("get goal")
            .expect("goal exists");
        let cycle_id = waiting_goal
            .current_cycle_id
            .clone()
            .expect("current cycle id");
        let team_id = format!("team-{cycle_id}");

        handle_plan_approval_response(&team_id, PlanApprovalVerdict::Approved)
            .await
            .expect("approve linked team plan");

        let task = crate::goal_tasks::list_tasks_by_cycle(&cycle_id)
            .await
            .expect("list tasks")
            .into_iter()
            .next()
            .expect("task exists");
        crate::goal_tasks::claim_task(&task.id, "agent-team-review", 300)
            .await
            .expect("claim task");
        crate::goal_tasks::start_task(&task.id)
            .await
            .expect("start task");
        crate::goal_tasks::complete_task(&task.id, "result/ref")
            .await
            .expect("complete task");

        orchestrator
            .tick_goal(&goal.id)
            .await
            .expect("tick to awaiting review");

        let review_team = get_team(&team_id).await.expect("team before review");
        assert_eq!(review_team.lifecycle, AgentTeamLifecycle::AwaitingReview);

        let reviewed_team = handle_review_verdict(&team_id, ReviewVerdict::Accepted)
            .await
            .expect("accept linked team review");
        let accepted_goal = crate::goals::get_goal(&goal.id)
            .await
            .expect("get accepted goal")
            .expect("goal exists");
        let accepted_cycle = crate::goals::get_cycle(&cycle_id)
            .await
            .expect("get accepted cycle")
            .expect("cycle exists");

        assert_eq!(reviewed_team.lifecycle, AgentTeamLifecycle::Accepted);
        assert_eq!(accepted_goal.status, "accepted");
        assert_eq!(accepted_cycle.status, "completed");
    }
}
