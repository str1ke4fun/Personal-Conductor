use crate::{db, proposals::RiskLevel};
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

// ── PermissionGrant Status State Machine ──────────────────────────────────
//
//  unrequested -> requested -> approved_once / approved_session / denied
//                                    |                |
//                                    v                v
//                                  used          expired / revoked

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGrantStatus {
    Unrequested,
    Requested,
    ApprovedOnce,
    ApprovedSession,
    Denied,
    Expired,
    Revoked,
    Used,
}

impl PermissionGrantStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unrequested => "unrequested",
            Self::Requested => "requested",
            Self::ApprovedOnce => "approved_once",
            Self::ApprovedSession => "approved_session",
            Self::Denied => "denied",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
            Self::Used => "used",
        }
    }

    pub fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "unrequested" => Ok(Self::Unrequested),
            "requested" => Ok(Self::Requested),
            "approved_once" => Ok(Self::ApprovedOnce),
            "approved_session" => Ok(Self::ApprovedSession),
            "denied" => Ok(Self::Denied),
            "expired" => Ok(Self::Expired),
            "revoked" => Ok(Self::Revoked),
            "used" => Ok(Self::Used),
            other => bail!("unknown permission grant status: {other}"),
        }
    }

    /// Whether this status represents an actionable (non-terminal) grant.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::ApprovedOnce | Self::ApprovedSession)
    }

    /// Whether this status is terminal (no further transitions).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Denied | Self::Expired | Self::Revoked | Self::Used
        )
    }

    /// Valid transitions from the current state.
    pub fn valid_transitions(&self) -> &'static [PermissionGrantStatus] {
        match self {
            Self::Unrequested => &[Self::Requested],
            Self::Requested => &[Self::ApprovedOnce, Self::ApprovedSession, Self::Denied],
            Self::ApprovedOnce => &[Self::Used, Self::Expired, Self::Revoked],
            Self::ApprovedSession => &[Self::Expired, Self::Revoked],
            Self::Denied => &[],
            Self::Expired => &[],
            Self::Revoked => &[],
            Self::Used => &[],
        }
    }

    /// Validate a state transition.
    pub fn can_transition_to(&self, target: &Self) -> bool {
        self.valid_transitions().contains(target)
    }
}

// ── WorkspaceScope ────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceScope {
    /// Allowed workspace IDs (empty = all workspaces).
    pub workspace_ids: Vec<String>,
    /// Allowed tool ID prefixes (empty = all tools).
    pub tool_prefixes: Vec<String>,
    /// Maximum risk level this scope permits.
    pub max_risk_level: RiskLevel,
}

impl WorkspaceScope {
    /// Create a scope that permits everything.
    pub fn unrestricted() -> Self {
        Self {
            workspace_ids: Vec::new(),
            tool_prefixes: Vec::new(),
            max_risk_level: RiskLevel::Destructive,
        }
    }

    /// Create a scope restricted to read-only operations.
    pub fn read_only() -> Self {
        Self {
            workspace_ids: Vec::new(),
            tool_prefixes: Vec::new(),
            max_risk_level: RiskLevel::ReadOnly,
        }
    }

    /// Check whether a specific (workspace_id, tool_id, risk_level) is allowed.
    pub fn allows(
        &self,
        workspace_id: Option<&str>,
        tool_id: &str,
        risk_level: &RiskLevel,
    ) -> bool {
        // Risk level check
        if risk_level > &self.max_risk_level {
            return false;
        }

        // Workspace check
        if !self.workspace_ids.is_empty() {
            match workspace_id {
                Some(ws) => {
                    if !self.workspace_ids.iter().any(|w| w == ws) {
                        return false;
                    }
                }
                None => return false,
            }
        }

        // Tool prefix check
        if !self.tool_prefixes.is_empty() {
            if !self
                .tool_prefixes
                .iter()
                .any(|prefix| tool_id.starts_with(prefix))
            {
                return false;
            }
        }

        true
    }

    /// Compute the intersection of two scopes (most restrictive wins).
    pub fn intersect(&self, other: &Self) -> Self {
        let workspace_ids = if self.workspace_ids.is_empty() {
            other.workspace_ids.clone()
        } else if other.workspace_ids.is_empty() {
            self.workspace_ids.clone()
        } else {
            self.workspace_ids
                .iter()
                .filter(|w| other.workspace_ids.contains(w))
                .cloned()
                .collect()
        };

        let tool_prefixes = if self.tool_prefixes.is_empty() {
            other.tool_prefixes.clone()
        } else if other.tool_prefixes.is_empty() {
            self.tool_prefixes.clone()
        } else {
            self.tool_prefixes
                .iter()
                .filter(|p| {
                    other
                        .tool_prefixes
                        .iter()
                        .any(|op| p.starts_with(op.as_str()) || op.starts_with(p.as_str()))
                })
                .cloned()
                .collect()
        };

        // Use Ord trait (implemented in tools::registry) to pick the most restrictive level
        let max_risk_level = std::cmp::min(&self.max_risk_level, &other.max_risk_level).clone();

        Self {
            workspace_ids,
            tool_prefixes,
            max_risk_level,
        }
    }
}

impl Default for WorkspaceScope {
    fn default() -> Self {
        Self::unrestricted()
    }
}

// ── PermissionGrant struct ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PermissionGrant {
    pub id: String,
    pub workspace_id: Option<String>,
    pub tool_id: String,
    pub risk_level: RiskLevel,
    pub grantee: String,
    pub status: PermissionGrantStatus,
    pub scope: WorkspaceScope,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── Risk Level Gate Logic (§6.3) ──────────────────────────────────────────

/// Returns true if the given risk level requires a PermissionGrant.
pub fn requires_grant(risk_level: &RiskLevel) -> bool {
    matches!(
        risk_level,
        RiskLevel::WorkspaceWrite | RiskLevel::ExternalSideEffect | RiskLevel::Destructive
    )
}

/// Returns the default grant status for a given risk level.
/// - Destructive: denied by default
/// - Others that require grant: requested (needs explicit approval)
pub fn default_status_for_risk(risk_level: &RiskLevel) -> PermissionGrantStatus {
    match risk_level {
        RiskLevel::Destructive => PermissionGrantStatus::Denied,
        RiskLevel::WorkspaceWrite | RiskLevel::ExternalSideEffect => {
            PermissionGrantStatus::Requested
        }
        _ => PermissionGrantStatus::Unrequested,
    }
}

/// Gate check: verify that a tool execution is permitted.
/// Returns Ok(()) if allowed, Err with explanation if blocked.
pub async fn check_gate(
    tool_id: &str,
    risk_level: &RiskLevel,
    workspace_id: Option<&str>,
    grant_id: Option<&str>,
) -> anyhow::Result<()> {
    if !requires_grant(risk_level) {
        return Ok(());
    }

    let grant_id = grant_id.ok_or_else(|| {
        anyhow::anyhow!(
            "tool '{}' with risk_level '{}' requires a PermissionGrant, but none was provided",
            tool_id,
            risk_level.as_str()
        )
    })?;

    let grant = get(grant_id).await?;

    if !grant.status.is_active() {
        bail!(
            "permission grant '{}' is not active (status={})",
            grant_id,
            grant.status.as_str()
        );
    }

    // Check expiry
    if let Some(expires_at) = grant.expires_at {
        if Utc::now() > expires_at {
            // Auto-expire the grant
            set_status(grant_id, PermissionGrantStatus::Expired).await?;
            bail!("permission grant '{}' has expired", grant_id);
        }
    }

    // Check scope
    if !grant.scope.allows(workspace_id, tool_id, risk_level) {
        bail!(
            "permission grant '{}' scope does not allow tool '{}' with risk_level '{}' in workspace {:?}",
            grant_id,
            tool_id,
            risk_level.as_str(),
            workspace_id
        );
    }

    Ok(())
}

// ── CRUD ──────────────────────────────────────────────────────────────────

pub async fn create(grant: PermissionGrant) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let scope_json = serde_json::to_string(&grant.scope)?;
    sqlx::query(
        r#"
        INSERT INTO permission_grants (
            id, workspace_id, tool_id, risk_level, grantee, status,
            scope_json, expires_at, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(id) DO UPDATE SET
            workspace_id = excluded.workspace_id,
            tool_id = excluded.tool_id,
            risk_level = excluded.risk_level,
            grantee = excluded.grantee,
            status = excluded.status,
            scope_json = excluded.scope_json,
            expires_at = excluded.expires_at,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&grant.id)
    .bind(&grant.workspace_id)
    .bind(&grant.tool_id)
    .bind(grant.risk_level.as_str())
    .bind(&grant.grantee)
    .bind(grant.status.as_str())
    .bind(&scope_json)
    .bind(grant.expires_at.map(|dt| dt.to_rfc3339()))
    .bind(grant.created_at.to_rfc3339())
    .bind(grant.updated_at.to_rfc3339())
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn get(id: &str) -> anyhow::Result<PermissionGrant> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT id, workspace_id, tool_id, risk_level, grantee, status,
               scope_json, expires_at, created_at, updated_at
        FROM permission_grants
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .with_context(|| format!("permission grant not found: {id}"))?;
    row_to_grant(row)
}

pub async fn list_by_grantee(grantee: &str) -> anyhow::Result<Vec<PermissionGrant>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, workspace_id, tool_id, risk_level, grantee, status,
               scope_json, expires_at, created_at, updated_at
        FROM permission_grants
        WHERE grantee = ?1
        ORDER BY created_at DESC
        "#,
    )
    .bind(grantee)
    .fetch_all(&pool)
    .await?;
    rows.into_iter().map(row_to_grant).collect()
}

pub async fn list_by_workspace(workspace_id: &str) -> anyhow::Result<Vec<PermissionGrant>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, workspace_id, tool_id, risk_level, grantee, status,
               scope_json, expires_at, created_at, updated_at
        FROM permission_grants
        WHERE workspace_id = ?1
        ORDER BY created_at DESC
        "#,
    )
    .bind(workspace_id)
    .fetch_all(&pool)
    .await?;
    rows.into_iter().map(row_to_grant).collect()
}

pub async fn list_active_by_tool(tool_id: &str) -> anyhow::Result<Vec<PermissionGrant>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT id, workspace_id, tool_id, risk_level, grantee, status,
               scope_json, expires_at, created_at, updated_at
        FROM permission_grants
        WHERE tool_id = ?1 AND status IN ('approved_once', 'approved_session')
        ORDER BY created_at DESC
        "#,
    )
    .bind(tool_id)
    .fetch_all(&pool)
    .await?;
    rows.into_iter().map(row_to_grant).collect()
}

pub async fn set_status(id: &str, status: PermissionGrantStatus) -> anyhow::Result<()> {
    let current = get(id).await?;
    if !current.status.can_transition_to(&status) {
        bail!(
            "invalid transition from '{}' to '{}' for grant '{}'",
            current.status.as_str(),
            status.as_str(),
            id
        );
    }

    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE permission_grants
        SET status = ?1, updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(status.as_str())
    .bind(Utc::now().to_rfc3339())
    .bind(id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn approve_once(id: &str) -> anyhow::Result<()> {
    set_status(id, PermissionGrantStatus::ApprovedOnce).await
}

pub async fn approve_session(id: &str) -> anyhow::Result<()> {
    set_status(id, PermissionGrantStatus::ApprovedSession).await
}

pub async fn deny(id: &str) -> anyhow::Result<()> {
    set_status(id, PermissionGrantStatus::Denied).await
}

pub async fn revoke(id: &str) -> anyhow::Result<()> {
    set_status(id, PermissionGrantStatus::Revoked).await?;
    // Best-effort audit event
    if let Ok(grant) = get(id).await {
        crate::events::emit_permission_revoked(id, &grant.tool_id).await;
    }
    Ok(())
}

pub async fn mark_used(id: &str) -> anyhow::Result<()> {
    set_status(id, PermissionGrantStatus::Used).await
}

pub async fn mark_expired(id: &str) -> anyhow::Result<()> {
    set_status(id, PermissionGrantStatus::Expired).await
}

/// Request a new grant: creates it in Requested status.
pub async fn request(
    id: String,
    workspace_id: Option<String>,
    tool_id: String,
    risk_level: RiskLevel,
    grantee: String,
    scope: WorkspaceScope,
    expires_at: Option<DateTime<Utc>>,
) -> anyhow::Result<PermissionGrant> {
    let now = Utc::now();
    let grant = PermissionGrant {
        id: id.clone(),
        workspace_id,
        tool_id,
        risk_level,
        grantee,
        status: PermissionGrantStatus::Requested,
        scope,
        expires_at,
        created_at: now,
        updated_at: now,
    };
    create(grant.clone()).await?;
    Ok(grant)
}

/// Auto-request a grant for a tool with the default status for its risk level.
/// For destructive tools, the grant starts as Denied.
pub async fn auto_request(
    id: String,
    workspace_id: Option<String>,
    tool_id: String,
    risk_level: RiskLevel,
    grantee: String,
    scope: WorkspaceScope,
    expires_at: Option<DateTime<Utc>>,
) -> anyhow::Result<PermissionGrant> {
    let status = default_status_for_risk(&risk_level);
    let now = Utc::now();
    let grant = PermissionGrant {
        id,
        workspace_id,
        tool_id,
        risk_level,
        grantee,
        status,
        scope,
        expires_at,
        created_at: now,
        updated_at: now,
    };
    create(grant.clone()).await?;
    Ok(grant)
}

/// Create a child agent permission grant by intersecting with parent scope.
/// The child's effective scope is the intersection of its own scope and the parent's scope.
pub async fn create_child_grant(
    id: String,
    parent_grant_id: &str,
    child_grantee: String,
    child_scope: WorkspaceScope,
    expires_at: Option<DateTime<Utc>>,
) -> anyhow::Result<PermissionGrant> {
    let parent = get(parent_grant_id).await?;

    if !parent.status.is_active() {
        bail!(
            "parent grant '{}' is not active (status={})",
            parent_grant_id,
            parent.status.as_str()
        );
    }

    // Child scope = intersection of parent scope and child scope
    let effective_scope = parent.scope.intersect(&child_scope);

    let now = Utc::now();
    let grant = PermissionGrant {
        id,
        workspace_id: parent.workspace_id.clone(),
        tool_id: parent.tool_id.clone(),
        risk_level: parent.risk_level.clone(),
        grantee: child_grantee,
        status: PermissionGrantStatus::Requested,
        scope: effective_scope,
        expires_at,
        created_at: now,
        updated_at: now,
    };
    create(grant.clone()).await?;
    Ok(grant)
}

/// Generate the next permission grant ID.
pub async fn next_id() -> anyhow::Result<String> {
    let pool = db::pool().await?;
    let date = Utc::now().format("%Y%m%d").to_string();
    let prefix = format!("pg-{date}-");

    let max_num: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT MAX(CAST(SUBSTR(id, LENGTH(?1) + 1) AS INTEGER))
        FROM permission_grants
        WHERE id LIKE ?1 || '%'
        "#,
    )
    .bind(&prefix)
    .fetch_one(&pool)
    .await?;

    let next = max_num.unwrap_or(0) + 1;
    Ok(format!("{prefix}{next:03}"))
}

// ── Row mapper ────────────────────────────────────────────────────────────

fn row_to_grant(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<PermissionGrant> {
    let scope_json: Option<String> = row.try_get("scope_json")?;
    let scope: WorkspaceScope = scope_json
        .map(|json| serde_json::from_str(&json))
        .transpose()?
        .unwrap_or_default();

    Ok(PermissionGrant {
        id: row.try_get("id")?,
        workspace_id: row.try_get("workspace_id")?,
        tool_id: row.try_get("tool_id")?,
        risk_level: RiskLevel::from_str(row.try_get::<String, _>("risk_level")?.as_str())?,
        grantee: row.try_get("grantee")?,
        status: PermissionGrantStatus::from_str(row.try_get::<String, _>("status")?.as_str())?,
        scope,
        expires_at: row
            .try_get::<Option<String>, _>("expires_at")?
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)))
            .transpose()?,
        created_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("updated_at")?.as_str())?
            .with_timezone(&Utc),
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    fn test_scope() -> WorkspaceScope {
        WorkspaceScope {
            workspace_ids: vec!["ws-1".into()],
            tool_prefixes: vec!["file.".into(), "shell.".into()],
            max_risk_level: RiskLevel::ExternalSideEffect,
        }
    }

    fn make_grant(id: &str, status: PermissionGrantStatus) -> PermissionGrant {
        let now = Utc::now();
        PermissionGrant {
            id: id.into(),
            workspace_id: Some("ws-1".into()),
            tool_id: "file.write".into(),
            risk_level: RiskLevel::WorkspaceWrite,
            grantee: "agent-1".into(),
            status,
            scope: test_scope(),
            expires_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    // ── 1. Create and retrieve ──

    #[tokio::test]
    async fn create_and_get_grant() {
        let _root = TestRoot::new();
        let grant = make_grant("pg-20260529-001", PermissionGrantStatus::Requested);
        create(grant).await.expect("create");

        let retrieved = get("pg-20260529-001").await.expect("get");
        assert_eq!(retrieved.id, "pg-20260529-001");
        assert_eq!(retrieved.status, PermissionGrantStatus::Requested);
        assert_eq!(retrieved.tool_id, "file.write");
        assert_eq!(retrieved.grantee, "agent-1");
    }

    // ── 2. Status transitions ──

    #[tokio::test]
    async fn valid_status_transitions() {
        let _root = TestRoot::new();
        let grant = make_grant("pg-20260529-002", PermissionGrantStatus::Requested);
        create(grant).await.expect("create");

        approve_once("pg-20260529-002").await.expect("approve_once");
        let g = get("pg-20260529-002").await.expect("get");
        assert_eq!(g.status, PermissionGrantStatus::ApprovedOnce);

        mark_used("pg-20260529-002").await.expect("mark_used");
        let g = get("pg-20260529-002").await.expect("get");
        assert_eq!(g.status, PermissionGrantStatus::Used);
    }

    // ── 3. Invalid transition blocked ──

    #[tokio::test]
    async fn invalid_transition_blocked() {
        let _root = TestRoot::new();
        let grant = make_grant("pg-20260529-003", PermissionGrantStatus::Denied);
        create(grant).await.expect("create");

        let result = approve_once("pg-20260529-003").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid transition"));
    }

    // ── 4. Expired grant cannot be reused ──

    #[tokio::test]
    async fn expired_grant_cannot_be_reused() {
        let _root = TestRoot::new();
        let mut grant = make_grant("pg-20260529-004", PermissionGrantStatus::ApprovedOnce);
        grant.expires_at = Some(Utc::now() - chrono::Duration::hours(1));
        create(grant).await.expect("create");

        // Manually expire
        mark_expired("pg-20260529-004").await.expect("expire");
        let g = get("pg-20260529-004").await.expect("get");
        assert_eq!(g.status, PermissionGrantStatus::Expired);

        // Cannot transition from expired
        let result = approve_once("pg-20260529-004").await;
        assert!(result.is_err());
    }

    // ── 5. Risk level gate: workspace_write requires grant ──

    #[tokio::test]
    async fn workspace_write_requires_grant() {
        assert!(requires_grant(&RiskLevel::WorkspaceWrite));
        assert!(requires_grant(&RiskLevel::ExternalSideEffect));
        assert!(requires_grant(&RiskLevel::Destructive));
        assert!(!requires_grant(&RiskLevel::ReadOnly));
        assert!(!requires_grant(&RiskLevel::DraftOnly));
    }

    // ── 6. Gate check passes with valid active grant ──

    #[tokio::test]
    async fn gate_check_passes_with_active_grant() {
        let _root = TestRoot::new();
        let grant = make_grant("pg-20260529-005", PermissionGrantStatus::ApprovedOnce);
        create(grant).await.expect("create");

        let result = check_gate(
            "file.write",
            &RiskLevel::WorkspaceWrite,
            Some("ws-1"),
            Some("pg-20260529-005"),
        )
        .await;
        assert!(result.is_ok());
    }

    // ── 7. Gate check fails without grant ──

    #[tokio::test]
    async fn gate_check_fails_without_grant() {
        let _root = TestRoot::new();

        let result = check_gate("file.write", &RiskLevel::WorkspaceWrite, Some("ws-1"), None).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("requires a PermissionGrant"));
    }

    // ── 8. Gate check fails with non-active grant ──

    #[tokio::test]
    async fn gate_check_fails_with_non_active_grant() {
        let _root = TestRoot::new();
        let grant = make_grant("pg-20260529-006", PermissionGrantStatus::Requested);
        create(grant).await.expect("create");

        let result = check_gate(
            "file.write",
            &RiskLevel::WorkspaceWrite,
            Some("ws-1"),
            Some("pg-20260529-006"),
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not active"));
    }

    // ── 9. Destructive operations default to denied ──

    #[tokio::test]
    async fn destructive_defaults_to_denied() {
        let status = default_status_for_risk(&RiskLevel::Destructive);
        assert_eq!(status, PermissionGrantStatus::Denied);
    }

    // ── 10. WorkspaceScope allows/rejects ──

    #[tokio::test]
    async fn workspace_scope_allows_and_rejects() {
        let scope = test_scope();

        // Allowed: matching workspace + tool prefix + risk level
        assert!(scope.allows(Some("ws-1"), "file.write", &RiskLevel::WorkspaceWrite));
        assert!(scope.allows(Some("ws-1"), "shell.exec", &RiskLevel::ReadOnly));

        // Rejected: wrong workspace
        assert!(!scope.allows(Some("ws-2"), "file.write", &RiskLevel::WorkspaceWrite));

        // Rejected: tool not in prefix
        assert!(!scope.allows(Some("ws-1"), "agent.start", &RiskLevel::WorkspaceWrite));

        // Rejected: risk level too high
        assert!(!scope.allows(Some("ws-1"), "file.write", &RiskLevel::Destructive));
    }

    // ── 11. WorkspaceScope intersection ──

    #[tokio::test]
    async fn workspace_scope_intersection() {
        let parent = WorkspaceScope {
            workspace_ids: vec!["ws-1".into(), "ws-2".into()],
            tool_prefixes: vec!["file.".into(), "shell.".into()],
            max_risk_level: RiskLevel::ExternalSideEffect,
        };
        let child = WorkspaceScope {
            workspace_ids: vec!["ws-2".into(), "ws-3".into()],
            tool_prefixes: vec!["shell.".into()],
            max_risk_level: RiskLevel::Destructive,
        };

        let intersection = parent.intersect(&child);

        // Only ws-2 is in both
        assert_eq!(intersection.workspace_ids, vec!["ws-2"]);
        // Only "shell." is in both
        assert_eq!(intersection.tool_prefixes, vec!["shell."]);
        // Min risk level wins
        assert_eq!(intersection.max_risk_level, RiskLevel::ExternalSideEffect);
    }

    // ── 12. Child agent permission intersection ──

    #[tokio::test]
    async fn child_grant_scope_is_subset_of_parent() {
        let _root = TestRoot::new();

        // Create parent grant with broad scope
        let parent_scope = WorkspaceScope {
            workspace_ids: vec!["ws-1".into(), "ws-2".into()],
            tool_prefixes: vec![],
            max_risk_level: RiskLevel::ExternalSideEffect,
        };
        let parent = PermissionGrant {
            id: "pg-20260529-010".into(),
            workspace_id: Some("ws-1".into()),
            tool_id: "file.write".into(),
            risk_level: RiskLevel::WorkspaceWrite,
            grantee: "parent-agent".into(),
            status: PermissionGrantStatus::ApprovedSession,
            scope: parent_scope,
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        create(parent).await.expect("create parent");

        // Create child grant with restricted scope
        let child_scope = WorkspaceScope {
            workspace_ids: vec!["ws-2".into()],
            tool_prefixes: vec!["file.read".into()],
            max_risk_level: RiskLevel::ReadOnly,
        };
        let child = create_child_grant(
            "pg-20260529-011".into(),
            "pg-20260529-010",
            "child-agent".into(),
            child_scope,
            None,
        )
        .await
        .expect("create child");

        // Child scope should be intersection: ws-2 only, file.read prefix, ReadOnly risk
        assert_eq!(child.scope.workspace_ids, vec!["ws-2"]);
        assert_eq!(child.scope.tool_prefixes, vec!["file.read"]);
        assert_eq!(child.scope.max_risk_level, RiskLevel::ReadOnly);

        // Child should NOT be allowed to access ws-1
        assert!(!child
            .scope
            .allows(Some("ws-1"), "file.read", &RiskLevel::ReadOnly));
        // Child SHOULD be allowed to access ws-2 with file.read
        assert!(child
            .scope
            .allows(Some("ws-2"), "file.read", &RiskLevel::ReadOnly));
    }

    // ── 13. Auto-request for destructive defaults to denied ──

    #[tokio::test]
    async fn auto_request_destructive_starts_denied() {
        let _root = TestRoot::new();
        let grant = auto_request(
            "pg-20260529-012".into(),
            Some("ws-1".into()),
            "shell.rm_rf".into(),
            RiskLevel::Destructive,
            "agent-1".into(),
            WorkspaceScope::unrestricted(),
            None,
        )
        .await
        .expect("auto_request");

        assert_eq!(grant.status, PermissionGrantStatus::Denied);
    }

    // ── 14. Auto-request for workspace_write starts as requested ──

    #[tokio::test]
    async fn auto_request_workspace_write_starts_requested() {
        let _root = TestRoot::new();
        let grant = auto_request(
            "pg-20260529-013".into(),
            Some("ws-1".into()),
            "file.write".into(),
            RiskLevel::WorkspaceWrite,
            "agent-1".into(),
            WorkspaceScope::unrestricted(),
            None,
        )
        .await
        .expect("auto_request");

        assert_eq!(grant.status, PermissionGrantStatus::Requested);
    }

    // ── 15. Gate check for read_only passes without grant ──

    #[tokio::test]
    async fn read_only_does_not_require_grant() {
        let _root = TestRoot::new();
        let result = check_gate("file.read", &RiskLevel::ReadOnly, None, None).await;
        assert!(result.is_ok());
    }

    // ── 16. List by grantee ──

    #[tokio::test]
    async fn list_grants_by_grantee() {
        let _root = TestRoot::new();

        for i in 0..3 {
            let mut grant = make_grant(
                &format!("pg-20260529-02{}", i),
                PermissionGrantStatus::Requested,
            );
            grant.tool_id = format!("file.write{}", i);
            create(grant).await.expect("create");
        }

        let grants = list_by_grantee("agent-1").await.expect("list");
        assert_eq!(grants.len(), 3);
    }

    // ── 17. Revoked grant blocks gate ──

    #[tokio::test]
    async fn revoked_grant_blocks_gate() {
        let _root = TestRoot::new();
        let grant = make_grant("pg-20260529-030", PermissionGrantStatus::ApprovedSession);
        create(grant).await.expect("create");

        revoke("pg-20260529-030").await.expect("revoke");

        let result = check_gate(
            "file.write",
            &RiskLevel::WorkspaceWrite,
            Some("ws-1"),
            Some("pg-20260529-030"),
        )
        .await;
        assert!(result.is_err());
    }

    // ── 18. next_id generates sequential IDs ──

    #[tokio::test]
    async fn next_id_is_sequential() {
        let _root = TestRoot::new();
        let id1 = next_id().await.expect("next_id 1");
        assert!(id1.starts_with("pg-"));

        // Insert a grant so the next call increments
        let grant = make_grant(&id1, PermissionGrantStatus::Requested);
        create(grant).await.expect("insert first");

        let id2 = next_id().await.expect("next_id 2");
        assert!(id2.starts_with("pg-"));
        assert_ne!(id1, id2);
    }
}
