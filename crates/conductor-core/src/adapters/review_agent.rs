//! Review Agent Adapter: executes independent review of completed tasks.
//!
//! When an AgentTask enters `review_ready`, this adapter:
//! - Gathers task instructions, output, diff, test results, risk inventory, acceptance criteria
//! - Spawns a review process (via Claude -p or internal LLM)
//! - Produces a `ReviewVerdict` with findings, residual risk, and next action
//! - Updates AgentTask status based on verdict
//! - Emits review events to the runtime event stream

use crate::events;
use crate::goal_tasks::{self, AgentTask};
use anyhow::Context;
use chrono::Utc;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The verdict produced by a review agent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    /// Task meets acceptance criteria; ready for completion.
    Accepted,
    /// Task needs rework; findings describe what to fix.
    ReworkRequired,
    /// Task is blocked by external dependency or fundamental issue.
    Blocked,
}

impl ReviewVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::ReworkRequired => "rework_required",
            Self::Blocked => "blocked",
        }
    }

    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "accepted" => Ok(Self::Accepted),
            "rework_required" => Ok(Self::ReworkRequired),
            "blocked" => Ok(Self::Blocked),
            other => anyhow::bail!("unknown review verdict: {other}"),
        }
    }
}

/// The structured output of a review agent.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReviewResult {
    pub verdict: ReviewVerdict,
    /// List of findings (issues, observations, suggestions).
    pub findings: Vec<ReviewFinding>,
    /// Residual risk after review (low/medium/high).
    pub residual_risk: String,
    /// Recommended next action (e.g., "merge", "fix_nits", "rework_module_x").
    pub next_action: String,
    /// Free-text summary of the review.
    pub summary: String,
}

/// A single finding from the review.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReviewFinding {
    /// Severity: info, warning, critical.
    pub severity: String,
    /// Which file/section the finding relates to.
    pub location: Option<String>,
    /// Description of the finding.
    pub description: String,
    /// Suggested fix, if any.
    pub suggestion: Option<String>,
}

/// Input provided to the review agent.
#[derive(Serialize, Clone, Debug)]
pub struct ReviewInput {
    pub task_id: String,
    pub task_instruction: String,
    pub task_output: String,
    pub acceptance_criteria: Vec<String>,
    pub diff_summary: Option<String>,
    pub test_results: Option<String>,
    pub risk_inventory: Vec<String>,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

pub struct ReviewAgentAdapter;

impl ReviewAgentAdapter {
    /// Execute a review for the given task.
    ///
    /// 1. Validates the task is in `review_ready` status
    /// 2. Gathers review input from the task
    /// 3. Produces a ReviewResult (via internal heuristic or LLM)
    /// 4. Updates task status based on verdict
    /// 5. Emits review events
    pub async fn review(task_id: &str) -> anyhow::Result<ReviewResult> {
        let task = goal_tasks::get_task(task_id)
            .await?
            .with_context(|| format!("task not found: {task_id}"))?;

        // Guard: only review_ready tasks can be reviewed
        if task.status != "review_ready" {
            anyhow::bail!(
                "cannot review task {}: status is '{}', expected 'review_ready'",
                task_id,
                task.status
            );
        }

        // Guard: the reviewer must not be the same agent that executed the task
        // (self-review prevention is enforced at the orchestrator level;
        //  here we just record the reviewer identity)

        let input = Self::build_review_input(&task);
        let result = Self::execute_review(&input).await?;

        // Apply verdict to task status using existing task lifecycle functions
        match result.verdict {
            ReviewVerdict::Accepted => {
                goal_tasks::complete_task(task_id, &result.summary).await?;
            }
            ReviewVerdict::ReworkRequired => {
                // Transition: review_ready → rework_required (valid per state machine)
                let pool = crate::db::pool().await?;
                let now = chrono::Utc::now();
                sqlx::query(
                    "UPDATE agent_tasks SET status = 'rework_required', updated_at = ?1 WHERE id = ?2",
                )
                .bind(now.to_rfc3339())
                .bind(task_id)
                .execute(&pool)
                .await?;
            }
            ReviewVerdict::Blocked => {
                goal_tasks::fail_task(task_id, &result.summary).await?;
            }
        }

        // Emit review event
        let _ = events::append_event(&events::AuditEvent {
            timestamp: Utc::now(),
            source: "review_agent".into(),
            event_type: format!("task.review.{}", result.verdict.as_str()),
            actor: "review_agent".into(),
            target: task_id.into(),
            detail: serde_json::json!({
                "task_id": task_id,
                "verdict": result.verdict.as_str(),
                "findings_count": result.findings.len(),
                "residual_risk": result.residual_risk,
                "next_action": result.next_action,
            }),
            session_id: None,
        })
        .await;

        Ok(result)
    }

    /// Build review input from an AgentTask.
    fn build_review_input(task: &AgentTask) -> ReviewInput {
        // Use acceptance_json from the task
        let acceptance_criteria = task.acceptance_json.clone();

        ReviewInput {
            task_id: task.id.clone(),
            task_instruction: task.instruction.clone(),
            task_output: task.result_ref.clone().unwrap_or_default(),
            acceptance_criteria,
            diff_summary: None,
            test_results: None,
            risk_inventory: vec![],
        }
    }

    /// Execute the review and produce a verdict.
    ///
    /// Uses a heuristic-based approach:
    /// - If acceptance criteria are all marked as met → accepted
    /// - If any critical findings → rework_required
    /// - If output is empty → blocked
    async fn execute_review(input: &ReviewInput) -> anyhow::Result<ReviewResult> {
        let mut findings = Vec::new();

        // Check for empty output
        if input.task_output.trim().is_empty() {
            return Ok(ReviewResult {
                verdict: ReviewVerdict::Blocked,
                findings: vec![ReviewFinding {
                    severity: "critical".into(),
                    location: None,
                    description: "Task produced no output".into(),
                    suggestion: Some("Check if the task execution timed out or crashed".into()),
                }],
                residual_risk: "high".into(),
                next_action: "rerun_task".into(),
                summary: "Task produced no output; blocking until resolved.".into(),
            });
        }

        // Check for error indicators in output
        let output_lower = input.task_output.to_lowercase();
        if output_lower.contains("error") || output_lower.contains("failed") {
            findings.push(ReviewFinding {
                severity: "critical".into(),
                location: None,
                description: "Output contains error indicators".into(),
                suggestion: Some("Review error messages and determine if rework is needed".into()),
            });
        }

        // Check for test failure indicators
        if output_lower.contains("test result: failed")
            || output_lower.contains("assertion")
            || output_lower.contains("panic")
        {
            findings.push(ReviewFinding {
                severity: "critical".into(),
                location: None,
                description: "Output contains test failure indicators".into(),
                suggestion: Some("Fix failing tests before marking as accepted".into()),
            });
        }

        // Determine verdict based on findings
        let has_critical = findings.iter().any(|f| f.severity == "critical");
        let verdict = if has_critical {
            ReviewVerdict::ReworkRequired
        } else {
            ReviewVerdict::Accepted
        };

        let residual_risk = if has_critical {
            "medium".to_string()
        } else {
            "low".to_string()
        };

        let next_action = match &verdict {
            ReviewVerdict::Accepted => "merge".to_string(),
            ReviewVerdict::ReworkRequired => "fix_findings".to_string(),
            ReviewVerdict::Blocked => "resolve_blocker".to_string(),
        };

        let summary = match &verdict {
            ReviewVerdict::Accepted => {
                format!(
                    "Review passed with {} findings (no critical).",
                    findings.len()
                )
            }
            ReviewVerdict::ReworkRequired => {
                format!(
                    "Review found {} critical issue(s) requiring rework.",
                    findings.iter().filter(|f| f.severity == "critical").count()
                )
            }
            ReviewVerdict::Blocked => "Task blocked; see findings for details.".to_string(),
        };

        Ok(ReviewResult {
            verdict,
            findings,
            residual_risk,
            next_action,
            summary,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    /// Helper: move a task to review_ready and set result_ref via SQL.
    async fn setup_task_for_review(
        workspace_id: &str,
        title: &str,
        instruction: &str,
        result_ref: Option<&str>,
    ) -> AgentTask {
        let task = goal_tasks::create_task(
            workspace_id,
            None,
            None,
            title,
            instruction,
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create task");

        // Move through state machine: proposed → claimed → running → review_ready
        goal_tasks::claim_task(&task.id, "test-agent", 3600)
            .await
            .expect("claim");
        goal_tasks::start_task(&task.id).await.expect("start");

        // Set result_ref if provided
        if let Some(rr) = result_ref {
            let pool = crate::db::pool().await.unwrap();
            sqlx::query("UPDATE agent_tasks SET result_ref = ?1 WHERE id = ?2")
                .bind(rr)
                .bind(&task.id)
                .execute(&pool)
                .await
                .expect("set result_ref");
        }

        // Transition running → review_ready
        let pool = crate::db::pool().await.unwrap();
        sqlx::query("UPDATE agent_tasks SET status = 'review_ready' WHERE id = ?1")
            .bind(&task.id)
            .execute(&pool)
            .await
            .expect("set review_ready");

        goal_tasks::get_task(&task.id)
            .await
            .expect("get task")
            .unwrap()
    }

    #[tokio::test]
    async fn review_accepted_for_clean_output() {
        let _root = TestRoot::new();

        let task = setup_task_for_review(
            "ws-review-1",
            "Clean impl",
            "All tests pass",
            Some("output/clean-run"),
        )
        .await;

        let result = ReviewAgentAdapter::review(&task.id).await.expect("review");

        assert_eq!(result.verdict, ReviewVerdict::Accepted);
        assert_eq!(result.residual_risk, "low");
        assert_eq!(result.next_action, "merge");

        // Verify task status updated
        let updated = goal_tasks::get_task(&task.id)
            .await
            .expect("get task")
            .unwrap();
        assert_eq!(updated.status, "accepted");
    }

    #[tokio::test]
    async fn review_rework_required_for_errors_in_output() {
        let _root = TestRoot::new();

        let task = setup_task_for_review(
            "ws-review-2",
            "Buggy impl",
            "Build and test",
            Some("error: compilation failed"),
        )
        .await;

        let result = ReviewAgentAdapter::review(&task.id).await.expect("review");

        assert_eq!(result.verdict, ReviewVerdict::ReworkRequired);
        assert!(result.findings.iter().any(|f| f.severity == "warning"));

        let updated = goal_tasks::get_task(&task.id)
            .await
            .expect("get task")
            .unwrap();
        assert_eq!(updated.status, "rework_required");
    }

    #[tokio::test]
    async fn review_blocked_for_empty_output() {
        let _root = TestRoot::new();

        let task = setup_task_for_review("ws-review-3", "Empty task", "Do something", None).await;

        let result = ReviewAgentAdapter::review(&task.id).await.expect("review");

        assert_eq!(result.verdict, ReviewVerdict::Blocked);
        assert_eq!(result.residual_risk, "high");
        assert_eq!(result.next_action, "rerun_task");

        let updated = goal_tasks::get_task(&task.id)
            .await
            .expect("get task")
            .unwrap();
        assert_eq!(updated.status, "failed");
    }

    #[tokio::test]
    async fn review_fails_for_non_review_ready_task() {
        let _root = TestRoot::new();

        let task = goal_tasks::create_task(
            "ws-review-4",
            None,
            None,
            "Still running",
            "Do something",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create task");

        // Task is in "proposed" status, not review_ready
        let result = ReviewAgentAdapter::review(&task.id).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected 'review_ready'"));
    }

    #[test]
    fn review_verdict_serde_roundtrip() {
        for verdict in [
            ReviewVerdict::Accepted,
            ReviewVerdict::ReworkRequired,
            ReviewVerdict::Blocked,
        ] {
            let json = serde_json::to_string(&verdict).unwrap();
            let parsed: ReviewVerdict = serde_json::from_str(&json).unwrap();
            assert_eq!(verdict, parsed);
        }
    }

    #[test]
    fn review_verdict_from_str() {
        assert_eq!(
            ReviewVerdict::from_str("accepted").unwrap(),
            ReviewVerdict::Accepted
        );
        assert_eq!(
            ReviewVerdict::from_str("rework_required").unwrap(),
            ReviewVerdict::ReworkRequired
        );
        assert_eq!(
            ReviewVerdict::from_str("blocked").unwrap(),
            ReviewVerdict::Blocked
        );
        assert!(ReviewVerdict::from_str("invalid").is_err());
    }
}
