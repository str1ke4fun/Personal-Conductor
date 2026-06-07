use crate::{
    agent_backends::{self, AgentBackend, BackendFilter, BackendKind, HealthStatus},
    db,
    goal_tasks::AgentTask,
};
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

// D-1: WorkKind and OodaPhase are mirrored in model-router-core::types.
/// The kind of work being dispatched — used for backend selection and routing.
/// Renamed from `TaskKind`; use `WorkKind` in new code.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkKind {
    Planning,
    Coding,
    Review,
    Testing,
    Document,
    ExternalAction,
}

/// Backward-compatibility alias — prefer `WorkKind` in new code.
pub type TaskKind = WorkKind;

impl WorkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planning => "planning",
            Self::Coding => "coding",
            Self::Review => "review",
            Self::Testing => "testing",
            Self::Document => "document",
            Self::ExternalAction => "external_action",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "planning" => Ok(Self::Planning),
            "coding" => Ok(Self::Coding),
            "review" => Ok(Self::Review),
            "testing" => Ok(Self::Testing),
            "document" => Ok(Self::Document),
            "external_action" => Ok(Self::ExternalAction),
            other => bail!("unknown work kind: {other}"),
        }
    }

    pub fn default_backend_kind(&self) -> BackendKind {
        match self {
            Self::Coding | Self::Testing => BackendKind::CodexInteractive,
            Self::Review => BackendKind::Review,
            Self::Document | Self::Planning => BackendKind::ClaudeP,
            Self::ExternalAction => BackendKind::AgentTeam,
        }
    }

    fn default_reason_template(&self) -> &'static str {
        match self {
            Self::Coding => "Coding work is routed to Codex CLI by default.",
            Self::Testing => "Test and verification work is routed to Codex CLI by default.",
            Self::Review => "Review work is routed to the review backend by default.",
            Self::Document => "Document work is routed to Claude CLI by default.",
            Self::Planning => "Planning work is routed to Claude CLI by default.",
            Self::ExternalAction => "External action work is routed to AgentTeam by default.",
        }
    }
}

/// Phase within the OODA loop — used to route LLM calls to different models
/// depending on which cognitive stage is active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OodaPhase {
    /// Bootstrap: first attempt to solve the goal directly.
    Bootstrap,
    /// Reason: read the whole graph, judge if goal is met, generate new intents.
    Reason,
    /// Explore: claim one open intent and execute it.
    Explore,
    /// Review: evaluate task output against acceptance criteria.
    Review,
}

impl OodaPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Reason => "reason",
            Self::Explore => "explore",
            Self::Review => "review",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RoutingPolicy {
    pub id: String,
    pub work_kind: WorkKind,
    pub caller_phase: Option<OodaPhase>,
    pub backend_kind: BackendKind,
    pub profile_id: Option<String>,
    pub priority: i64,
    pub enabled: bool,
    pub reason_template: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RouteDecision {
    pub id: String,
    pub workspace_id: String,
    pub task_id: Option<String>,
    pub work_kind: WorkKind,
    pub ooda_phase: Option<OodaPhase>,
    pub policy_id: Option<String>,
    pub backend_id: Option<String>,
    pub backend_kind: BackendKind,
    pub profile_id: Option<String>,
    pub reason: String,
    pub fallback_used: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateRoutingPolicyInput {
    pub id: Option<String>,
    pub work_kind: WorkKind,
    pub caller_phase: Option<OodaPhase>,
    pub backend_kind: BackendKind,
    pub profile_id: Option<String>,
    pub priority: i64,
    pub enabled: bool,
    pub reason_template: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct UpdateRoutingPolicyInput {
    pub backend_kind: Option<BackendKind>,
    pub profile_id: Option<Option<String>>,
    pub priority: Option<i64>,
    pub enabled: Option<bool>,
    pub reason_template: Option<Option<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RoutingPolicyFilter {
    pub work_kind: Option<WorkKind>,
    pub caller_phase: Option<OodaPhase>,
    pub enabled: Option<bool>,
    pub limit: Option<u32>,
}

/// Classify a task with deterministic rules before any model routing is used.
// A-1: returns WorkKind (TaskKind is a deprecated alias — prefer WorkKind in new code)
pub fn classify_task(title: &str, instruction: &str) -> WorkKind {
    let text = format!("{title}\n{instruction}").to_lowercase();

    if contains_any(
        &text,
        &[
            "review",
            "code review",
            "audit",
            "verdict",
            "审查",
            "审核",
            "验收",
            "复盘",
        ],
    ) {
        return TaskKind::Review;
    }
    if contains_any(
        &text,
        &[
            "test",
            "tests",
            "cargo test",
            "vitest",
            "pytest",
            "coverage",
            "验证",
            "测试",
        ],
    ) {
        return TaskKind::Testing;
    }
    if contains_any(
        &text,
        &[
            "implement",
            "fix",
            "bug",
            "refactor",
            "api",
            "endpoint",
            "rust",
            "typescript",
            "tsx",
            "component",
            "代码",
            "实现",
            "修复",
            "重构",
        ],
    ) {
        return TaskKind::Coding;
    }
    if contains_any(
        &text,
        &[
            "doc", "docs", "readme", "markdown", "copy", "文档", "说明", "写作", "草稿",
        ],
    ) {
        return TaskKind::Document;
    }
    if contains_any(
        &text,
        &[
            "lark", "feishu", "email", "calendar", "send", "upload", "download", "openapi", "飞书",
            "邮件", "日历", "发送", "上传", "下载",
        ],
    ) {
        return TaskKind::ExternalAction;
    }

    TaskKind::Planning
}

pub async fn create_policy(input: CreateRoutingPolicyInput) -> anyhow::Result<RoutingPolicy> {
    validate_profile(input.profile_id.as_deref()).await?;

    let now = Utc::now();
    let policy = RoutingPolicy {
        id: input
            .id
            .unwrap_or_else(|| format!("route-policy-{}", Uuid::new_v4())),
        reason_template: input
            .reason_template
            .unwrap_or_else(|| input.work_kind.default_reason_template().to_string()),
        work_kind: input.work_kind,
        caller_phase: input.caller_phase,
        backend_kind: input.backend_kind,
        profile_id: input.profile_id,
        priority: input.priority,
        enabled: input.enabled,
        created_at: now,
        updated_at: now,
    };

    insert_policy(&policy).await?;
    Ok(policy)
}

pub async fn get_policy(policy_id: &str) -> anyhow::Result<Option<RoutingPolicy>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, task_kind, caller_phase, backend_kind, profile_id, priority, enabled,
               reason_template, created_at, updated_at
        FROM routing_policies
        WHERE id = ?1
        "#,
    )
    .bind(policy_id)
    .fetch_optional(&pool)
    .await?;

    row.map(row_to_policy).transpose()
}

pub async fn list_policies(filter: RoutingPolicyFilter) -> anyhow::Result<Vec<RoutingPolicy>> {
    let pool = db::pool().await?;
    let limit = filter.limit.unwrap_or(100).clamp(1, 500) as i64;
    let rows = sqlx::query(
        r#"
        SELECT id, task_kind, caller_phase, backend_kind, profile_id, priority, enabled,
               reason_template, created_at, updated_at
        FROM routing_policies
        ORDER BY task_kind ASC, priority DESC, created_at ASC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(&pool)
    .await?;

    let mut policies = rows
        .into_iter()
        .map(row_to_policy)
        .collect::<anyhow::Result<Vec<_>>>()?;

    if let Some(work_kind) = filter.work_kind {
        policies.retain(|policy| policy.work_kind == work_kind);
    }
    if let Some(ref phase) = filter.caller_phase {
        // Policies with caller_phase = None are wildcards that match any phase.
        // Policies with a specific caller_phase must match exactly.
        policies.retain(|policy| {
            policy.caller_phase.is_none() || policy.caller_phase.as_ref() == Some(phase)
        });
    }
    if let Some(enabled) = filter.enabled {
        policies.retain(|policy| policy.enabled == enabled);
    }

    Ok(policies)
}

pub async fn update_policy(
    policy_id: &str,
    input: UpdateRoutingPolicyInput,
) -> anyhow::Result<RoutingPolicy> {
    let mut policy = get_policy(policy_id)
        .await?
        .with_context(|| format!("routing policy not found: {policy_id}"))?;

    if let Some(backend_kind) = input.backend_kind {
        policy.backend_kind = backend_kind;
    }
    if let Some(profile_id) = input.profile_id {
        validate_profile(profile_id.as_deref()).await?;
        policy.profile_id = profile_id;
    }
    if let Some(priority) = input.priority {
        policy.priority = priority;
    }
    if let Some(enabled) = input.enabled {
        policy.enabled = enabled;
    }
    if let Some(reason_template) = input.reason_template {
        policy.reason_template = reason_template
            .unwrap_or_else(|| policy.work_kind.default_reason_template().to_string());
    }
    policy.updated_at = Utc::now();

    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE routing_policies
        SET backend_kind = ?2,
            profile_id = ?3,
            priority = ?4,
            enabled = ?5,
            reason_template = ?6,
            updated_at = ?7
        WHERE id = ?1
        "#,
    )
    .bind(&policy.id)
    .bind(policy.backend_kind.as_str())
    .bind(&policy.profile_id)
    .bind(policy.priority)
    .bind(policy.enabled as i64)
    .bind(&policy.reason_template)
    .bind(policy.updated_at.to_rfc3339())
    .execute(&pool)
    .await?;

    Ok(policy)
}

pub async fn delete_policy(policy_id: &str) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let result = sqlx::query("DELETE FROM routing_policies WHERE id = ?1")
        .bind(policy_id)
        .execute(&pool)
        .await?;
    if result.rows_affected() == 0 {
        bail!("routing policy not found: {policy_id}");
    }
    Ok(())
}

/// Seed the deterministic default routing table used when the user has not
/// configured custom policies yet.
pub async fn seed_default_policies() -> anyhow::Result<()> {
    for work_kind in [
        WorkKind::Planning,
        WorkKind::Coding,
        WorkKind::Review,
        WorkKind::Testing,
        WorkKind::Document,
        WorkKind::ExternalAction,
    ] {
        let id = format!("route-default-{}", work_kind.as_str());
        if get_policy(&id).await?.is_none() {
            create_policy(CreateRoutingPolicyInput {
                id: Some(id),
                backend_kind: work_kind.default_backend_kind(),
                reason_template: Some(work_kind.default_reason_template().to_string()),
                work_kind,
                caller_phase: None,
                profile_id: None,
                priority: 0,
                enabled: true,
            })
            .await?;
        }
    }
    Ok(())
}

pub async fn route_task(task: &AgentTask) -> anyhow::Result<RouteDecision> {
    let work_kind = classify_task(&task.title, &task.instruction);
    route_task_kind(&task.workspace_id, Some(&task.id), work_kind).await
}

pub async fn route_text(
    workspace_id: &str,
    title: &str,
    instruction: &str,
) -> anyhow::Result<RouteDecision> {
    let work_kind = classify_task(title, instruction);
    route_task_kind(workspace_id, None, work_kind).await
}

pub async fn get_decision(decision_id: &str) -> anyhow::Result<Option<RouteDecision>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, workspace_id, task_id, task_kind, policy_id, backend_id,
               backend_kind, profile_id, reason, fallback_used, created_at
        FROM route_decisions
        WHERE id = ?1
        "#,
    )
    .bind(decision_id)
    .fetch_optional(&pool)
    .await?;

    row.map(row_to_decision).transpose()
}

pub async fn list_decisions_for_task(task_id: &str) -> anyhow::Result<Vec<RouteDecision>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, workspace_id, task_id, task_kind, policy_id, backend_id,
               backend_kind, profile_id, reason, fallback_used, created_at
        FROM route_decisions
        WHERE task_id = ?1
        ORDER BY created_at DESC
        "#,
    )
    .bind(task_id)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(row_to_decision).collect()
}

async fn route_task_kind(
    workspace_id: &str,
    task_id: Option<&str>,
    work_kind: WorkKind,
) -> anyhow::Result<RouteDecision> {
    seed_default_policies().await?;

    let policy = select_policy(&work_kind).await?;
    let preferred_kind = policy.backend_kind.clone();
    let (backend, selected_kind, backend_fallback) = select_backend(&preferred_kind).await?;
    let profile_id = active_policy_profile_id(&policy).await?;

    let mut reason = policy.reason_template.clone();
    if backend_fallback {
        reason.push_str(" Fallback backend selected because the preferred backend is unavailable.");
    }
    if profile_id.is_none() && policy.profile_id.is_some() {
        reason.push_str(" Configured profile was unavailable and was skipped.");
    }

    let decision = RouteDecision {
        id: format!("route-decision-{}", Uuid::new_v4()),
        workspace_id: workspace_id.to_string(),
        task_id: task_id.map(str::to_string),
        work_kind,
        ooda_phase: None,
        policy_id: Some(policy.id.clone()),
        backend_id: backend.as_ref().map(|b| b.id.clone()),
        backend_kind: selected_kind,
        profile_id,
        reason,
        fallback_used: backend_fallback,
        created_at: Utc::now(),
    };
    insert_decision(&decision).await?;
    Ok(decision)
}

async fn select_policy(work_kind: &WorkKind) -> anyhow::Result<RoutingPolicy> {
    let policies = list_policies(RoutingPolicyFilter {
        work_kind: Some(work_kind.clone()),
        caller_phase: None,
        enabled: Some(true),
        limit: Some(25),
    })
    .await?;

    policies
        .into_iter()
        .next()
        .with_context(|| format!("no enabled routing policy for {}", work_kind.as_str()))
}

async fn select_backend(
    preferred_kind: &BackendKind,
) -> anyhow::Result<(Option<AgentBackend>, BackendKind, bool)> {
    let backends = agent_backends::list(BackendFilter {
        enabled: Some(true),
        limit: Some(500),
        ..Default::default()
    })
    .await?;

    if let Some(backend) = backends
        .iter()
        .find(|backend| {
            backend.kind == *preferred_kind && backend.health_status != HealthStatus::Unhealthy
        })
        .cloned()
    {
        return Ok((Some(backend), preferred_kind.clone(), false));
    }

    for kind in [
        BackendKind::CodexInteractive,
        BackendKind::ClaudeP,
        BackendKind::AgentTeam,
        BackendKind::Review,
    ] {
        if let Some(backend) = backends
            .iter()
            .find(|backend| {
                backend.kind == kind && backend.health_status != HealthStatus::Unhealthy
            })
            .cloned()
        {
            return Ok((Some(backend), kind, true));
        }
    }

    bail!(
        "no enabled agent backend available for preferred kind {}",
        preferred_kind.as_str()
    )
}

async fn active_policy_profile_id(policy: &RoutingPolicy) -> anyhow::Result<Option<String>> {
    let Some(profile_id) = policy.profile_id.as_deref() else {
        return Ok(None);
    };

    let Some(profile) = crate::llm_profiles::get_profile(profile_id).await? else {
        return Ok(None);
    };

    Ok(profile.enabled.then(|| profile.id))
}

async fn validate_profile(profile_id: Option<&str>) -> anyhow::Result<()> {
    let Some(profile_id) = profile_id else {
        return Ok(());
    };
    let profile = crate::llm_profiles::get_profile(profile_id)
        .await?
        .with_context(|| format!("llm profile not found: {profile_id}"))?;
    if !profile.enabled {
        bail!("llm profile is disabled: {profile_id}");
    }
    Ok(())
}

async fn insert_policy(policy: &RoutingPolicy) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        INSERT INTO routing_policies (
            id, task_kind, caller_phase, backend_kind, profile_id, priority, enabled,
            reason_template, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
    )
    .bind(&policy.id)
    .bind(policy.work_kind.as_str())
    .bind(policy.caller_phase.as_ref().map(|p| p.as_str()))
    .bind(policy.backend_kind.as_str())
    .bind(&policy.profile_id)
    .bind(policy.priority)
    .bind(policy.enabled as i64)
    .bind(&policy.reason_template)
    .bind(policy.created_at.to_rfc3339())
    .bind(policy.updated_at.to_rfc3339())
    .execute(&pool)
    .await?;
    Ok(())
}

async fn insert_decision(decision: &RouteDecision) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        INSERT INTO route_decisions (
            id, workspace_id, task_id, task_kind, policy_id, backend_id,
            backend_kind, profile_id, reason, fallback_used, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
    )
    .bind(&decision.id)
    .bind(&decision.workspace_id)
    .bind(&decision.task_id)
    .bind(decision.work_kind.as_str())
    .bind(&decision.policy_id)
    .bind(&decision.backend_id)
    .bind(decision.backend_kind.as_str())
    .bind(&decision.profile_id)
    .bind(&decision.reason)
    .bind(decision.fallback_used as i64)
    .bind(decision.created_at.to_rfc3339())
    .execute(&pool)
    .await?;
    Ok(())
}

fn row_to_policy(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<RoutingPolicy> {
    Ok(RoutingPolicy {
        id: row.try_get("id")?,
        work_kind: WorkKind::from_str(row.try_get::<String, _>("task_kind")?.as_str())?,
        caller_phase: row
            .try_get::<Option<String>, _>("caller_phase")
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str::<OodaPhase>(&format!("\"{}\"", s)).ok()),
        backend_kind: BackendKind::from_str(row.try_get::<String, _>("backend_kind")?.as_str())?,
        profile_id: row.try_get("profile_id")?,
        priority: row.try_get("priority")?,
        enabled: row.try_get::<i64, _>("enabled")? != 0,
        reason_template: row.try_get("reason_template")?,
        created_at: parse_utc(row.try_get::<String, _>("created_at")?.as_str())?,
        updated_at: parse_utc(row.try_get::<String, _>("updated_at")?.as_str())?,
    })
}

fn row_to_decision(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<RouteDecision> {
    Ok(RouteDecision {
        id: row.try_get("id")?,
        workspace_id: row.try_get("workspace_id")?,
        task_id: row.try_get("task_id")?,
        work_kind: WorkKind::from_str(row.try_get::<String, _>("task_kind")?.as_str())?,
        ooda_phase: None,
        policy_id: row.try_get("policy_id")?,
        backend_id: row.try_get("backend_id")?,
        backend_kind: BackendKind::from_str(row.try_get::<String, _>("backend_kind")?.as_str())?,
        profile_id: row.try_get("profile_id")?,
        reason: row.try_get("reason")?,
        fallback_used: row.try_get::<i64, _>("fallback_used")? != 0,
        created_at: parse_utc(row.try_get::<String, _>("created_at")?.as_str())?,
    })
}

fn parse_utc(value: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("parse RFC3339 datetime: {value}"))?
        .with_timezone(&Utc))
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        agent_backends::{CreateBackendInput, UpdateBackendInput},
        test_support::TestRoot,
    };

    async fn create_backend(name: &str, kind: BackendKind) -> AgentBackend {
        agent_backends::create(CreateBackendInput {
            name: name.to_string(),
            kind,
            executable_path: None,
            default_env_json: None,
            health_check_url: None,
            enabled: true,
        })
        .await
        .expect("create backend")
    }

    #[test]
    fn classifier_detects_core_task_kinds() {
        assert_eq!(
            classify_task("Implement API endpoint", "write Rust handler"),
            WorkKind::Coding
        );
        assert_eq!(
            classify_task("Review workspace.md", "produce verdict"),
            WorkKind::Review
        );
        assert_eq!(
            classify_task("Run cargo test", "verify coverage"),
            WorkKind::Testing
        );
        assert_eq!(
            classify_task("Update README docs", "write markdown"),
            WorkKind::Document
        );
        assert_eq!(
            classify_task("Send Lark notice", "飞书发送消息"),
            WorkKind::ExternalAction
        );
        assert_eq!(
            classify_task("Plan next wave", "方案拆解"),
            WorkKind::Planning
        );
    }

    #[tokio::test]
    async fn policy_crud_round_trip() {
        let _root = TestRoot::new();

        let profile = crate::llm_profiles::create_profile(
            "Claude Writing",
            "anthropic",
            "claude-3-5-sonnet",
            "https://api.anthropic.com",
            None,
            4096,
            0.7,
        )
        .await
        .expect("create profile");

        let policy = create_policy(CreateRoutingPolicyInput {
            id: None,
            work_kind: WorkKind::Document,
            caller_phase: None,
            backend_kind: BackendKind::ClaudeP,
            profile_id: Some(profile.id.clone()),
            priority: 10,
            enabled: true,
            reason_template: Some("writing policy".to_string()),
        })
        .await
        .expect("create policy");

        let loaded = get_policy(&policy.id)
            .await
            .expect("get policy")
            .expect("policy exists");
        assert_eq!(loaded.work_kind, WorkKind::Document);
        assert_eq!(loaded.profile_id.as_deref(), Some(profile.id.as_str()));

        let updated = update_policy(
            &policy.id,
            UpdateRoutingPolicyInput {
                priority: Some(20),
                enabled: Some(false),
                reason_template: Some(Some("new reason".to_string())),
                ..Default::default()
            },
        )
        .await
        .expect("update policy");
        assert_eq!(updated.priority, 20);
        assert!(!updated.enabled);
        assert_eq!(updated.reason_template, "new reason");

        let disabled = list_policies(RoutingPolicyFilter {
            enabled: Some(false),
            ..Default::default()
        })
        .await
        .expect("list disabled");
        assert_eq!(disabled.len(), 1);

        delete_policy(&policy.id).await.expect("delete policy");
        assert!(get_policy(&policy.id).await.expect("get deleted").is_none());
    }

    #[tokio::test]
    async fn default_policies_are_seeded_once() {
        let _root = TestRoot::new();

        seed_default_policies().await.expect("seed defaults");
        seed_default_policies().await.expect("seed defaults again");

        let policies = list_policies(RoutingPolicyFilter {
            enabled: Some(true),
            ..Default::default()
        })
        .await
        .expect("list policies");
        assert_eq!(policies.len(), 6);
        assert!(policies.iter().any(|p| {
            p.work_kind == WorkKind::Coding && p.backend_kind == BackendKind::CodexInteractive
        }));
        assert!(policies
            .iter()
            .any(|p| p.work_kind == WorkKind::Document && p.backend_kind == BackendKind::ClaudeP));
    }

    #[tokio::test]
    async fn coding_task_prefers_codex_backend_and_persists_decision() {
        let _root = TestRoot::new();
        let codex = create_backend("Codex", BackendKind::CodexInteractive).await;
        create_backend("Claude", BackendKind::ClaudeP).await;

        let decision = route_text(
            "ws-routing",
            "Implement feature",
            "Fix the Rust API endpoint",
        )
        .await
        .expect("route task");

        assert_eq!(decision.work_kind, WorkKind::Coding);
        assert_eq!(decision.backend_kind, BackendKind::CodexInteractive);
        assert_eq!(decision.backend_id.as_deref(), Some(codex.id.as_str()));
        assert!(!decision.fallback_used);

        let persisted = get_decision(&decision.id)
            .await
            .expect("get decision")
            .expect("decision exists");
        assert_eq!(persisted.backend_id, decision.backend_id);
    }

    #[tokio::test]
    async fn document_task_prefers_claude_backend() {
        let _root = TestRoot::new();
        create_backend("Codex", BackendKind::CodexInteractive).await;
        let claude = create_backend("Claude", BackendKind::ClaudeP).await;

        let decision = route_text("ws-routing", "Update docs", "write markdown guide")
            .await
            .expect("route document");

        assert_eq!(decision.work_kind, WorkKind::Document);
        assert_eq!(decision.backend_kind, BackendKind::ClaudeP);
        assert_eq!(decision.backend_id.as_deref(), Some(claude.id.as_str()));
    }

    #[tokio::test]
    async fn route_task_links_decision_to_agent_task() {
        let _root = TestRoot::new();
        create_backend("Codex", BackendKind::CodexInteractive).await;

        let task = crate::goal_tasks::create_task(
            "ws-routing",
            None,
            None,
            "Implement routing",
            "Rust code task",
            "unrouted",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create task");

        let decision = route_task(&task).await.expect("route task");
        assert_eq!(decision.task_id.as_deref(), Some(task.id.as_str()));

        let task_decisions = list_decisions_for_task(&task.id)
            .await
            .expect("list decisions");
        assert_eq!(task_decisions.len(), 1);
        assert_eq!(task_decisions[0].id, decision.id);
    }

    #[tokio::test]
    async fn fallback_uses_available_backend_when_preferred_missing() {
        let _root = TestRoot::new();
        let claude = create_backend("Claude", BackendKind::ClaudeP).await;

        let decision = route_text("ws-routing", "Fix Rust bug", "implement patch")
            .await
            .expect("route coding with fallback");

        assert_eq!(decision.work_kind, WorkKind::Coding);
        assert_eq!(decision.backend_kind, BackendKind::ClaudeP);
        assert_eq!(decision.backend_id.as_deref(), Some(claude.id.as_str()));
        assert!(decision.fallback_used);
        assert!(decision.reason.contains("Fallback backend selected"));
    }

    #[tokio::test]
    async fn unhealthy_preferred_backend_is_skipped() {
        let _root = TestRoot::new();
        let codex = create_backend("Codex", BackendKind::CodexInteractive).await;
        let claude = create_backend("Claude", BackendKind::ClaudeP).await;
        agent_backends::update(
            &codex.id,
            UpdateBackendInput {
                enabled: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("update backend");
        let pool = db::pool().await.expect("pool");
        sqlx::query("UPDATE agent_backends SET health_status = 'unhealthy' WHERE id = ?1")
            .bind(&codex.id)
            .execute(&pool)
            .await
            .expect("mark unhealthy");

        let decision = route_text("ws-routing", "Implement patch", "fix API")
            .await
            .expect("route");

        assert_eq!(decision.backend_id.as_deref(), Some(claude.id.as_str()));
        assert_eq!(decision.backend_kind, BackendKind::ClaudeP);
        assert!(decision.fallback_used);
    }
}
