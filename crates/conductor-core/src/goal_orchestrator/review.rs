// Review phase: collect verdicts from completed tasks and decide next step
//
// BC-4 (contracts wiring): This phase is entirely programmatic — it inspects
// task statuses from `ObserveReport` using deterministic logic and never calls
// an LLM or parses raw LLM output.  There is therefore no contract validation
// to add here.  The `validate_reason_output` contract is wired into the LLM
// path in `decide.rs::decide_llm`, which is the only OODA phase that currently
// produces raw LLM text.  If a future `review_llm` function is added that calls
// a model to produce a structured verdict, it must validate via
// `contracts::validate_reason_output` (or a dedicated `ReviewOutput` contract)
// before accepting the result.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::observe::ObserveReport;

/// Verdict from the Review phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewVerdict {
    /// All tasks accepted — goal is done
    pub accepted: bool,
    /// Some tasks need rework — trigger another cycle
    pub rework_required: bool,
    /// Summary notes
    pub notes: String,
    /// Hint for the next cycle if rework is needed
    pub next_cycle_hint: Option<String>,
    /// The request_id of the ChatTurn associated with this cycle, if any.
    /// Provides the bidirectional anchor between GoalCycle and ChatTurn.
    pub chat_turn_request_id: Option<String>,
}

/// Collect verdicts from the current cycle's tasks and determine next step.
pub fn review(report: &ObserveReport) -> Result<ReviewVerdict> {
    let tasks = &report.active_tasks;

    if tasks.is_empty() {
        return Ok(ReviewVerdict {
            accepted: false,
            rework_required: false,
            notes: "no tasks to review".to_string(),
            next_cycle_hint: Some("create tasks first".to_string()),
            chat_turn_request_id: None,
        });
    }

    let total = tasks.len();
    let accepted_count = tasks.iter().filter(|t| t.status == "accepted").count();
    let failed_count = tasks.iter().filter(|t| t.status == "failed").count();
    let blocked_count = tasks.iter().filter(|t| t.status == "blocked").count();
    let rework_count = tasks
        .iter()
        .filter(|t| t.status == "rework_required")
        .count();
    let running_count = tasks
        .iter()
        .filter(|t| matches!(t.status.as_str(), "running" | "claimed" | "queued"))
        .count();

    // All accepted → goal accepted
    if accepted_count == total {
        return Ok(ReviewVerdict {
            accepted: true,
            rework_required: false,
            notes: format!("all {} tasks accepted", total),
            next_cycle_hint: None,
            chat_turn_request_id: None,
        });
    }

    // Any failures → rework or fail
    if failed_count > 0 {
        let failed_tasks: Vec<String> = tasks
            .iter()
            .filter(|t| t.status == "failed")
            .map(|t| t.id.clone())
            .collect();
        return Ok(ReviewVerdict {
            accepted: false,
            rework_required: true,
            notes: format!("{} tasks failed: {:?}", failed_count, failed_tasks),
            next_cycle_hint: Some(format!("retry failed tasks: {:?}", failed_tasks)),
            chat_turn_request_id: None,
        });
    }

    // Blocked → wait or rework
    if blocked_count > 0 {
        return Ok(ReviewVerdict {
            accepted: false,
            rework_required: false,
            notes: format!("{} tasks blocked, waiting for resolution", blocked_count),
            next_cycle_hint: None,
            chat_turn_request_id: None,
        });
    }

    // Rework required → another cycle
    if rework_count > 0 {
        return Ok(ReviewVerdict {
            accepted: false,
            rework_required: true,
            notes: format!("{} tasks need rework", rework_count),
            next_cycle_hint: Some("rework failed tasks".to_string()),
            chat_turn_request_id: None,
        });
    }

    // Still running → not ready for review
    if running_count > 0 {
        return Ok(ReviewVerdict {
            accepted: false,
            rework_required: false,
            notes: format!("{} tasks still running", running_count),
            next_cycle_hint: None,
            chat_turn_request_id: None,
        });
    }

    // Mixed state — partial progress
    Ok(ReviewVerdict {
        accepted: false,
        rework_required: false,
        notes: format!(
            "{}/{} accepted, {} running, {} blocked",
            accepted_count, total, running_count, blocked_count
        ),
        next_cycle_hint: None,
        chat_turn_request_id: None,
    })
}

/// Review with ChatTurn lookup: produces the same verdict as [`review`] but also
/// resolves the `chat_turn_request_id` anchor by looking up the ChatTurn
/// associated with the current cycle.
pub async fn review_with_turn(report: &ObserveReport) -> Result<ReviewVerdict> {
    let mut verdict = review(report)?;
    if let Some(ref cycle) = report.current_cycle {
        if let Ok(Some(turn)) = crate::chat::get_turn_by_goal_cycle_id(&cycle.id).await {
            verdict.chat_turn_request_id = Some(turn.request_id);
        }
    }
    Ok(verdict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal_tasks::AgentTask;
    use crate::goals::{GoalCycle, GoalRun};
    use chrono::Utc;

    fn make_task(id: &str, status: &str) -> AgentTask {
        let now = Utc::now();
        AgentTask {
            id: id.to_string(),
            workspace_id: "ws-1".to_string(),
            goal_id: Some("g1".to_string()),
            cycle_id: Some("c1".to_string()),
            parent_task_id: None,
            title: format!("task {id}"),
            instruction: "do something".to_string(),
            status: status.to_string(),
            agent_kind: "backend-agent".to_string(),
            assigned_agent_id: None,
            claimed_by: None,
            write_scope_json: vec![],
            read_scope_json: vec![],
            allowed_tools_json: vec![],
            dependencies_json: vec![],
            acceptance_json: vec![],
            result_ref: None,
            error: None,
            created_at: now,
            updated_at: now,
            claimed_at: None,
            finished_at: None,
        }
    }

    fn make_report(tasks: Vec<AgentTask>) -> ObserveReport {
        let now = Utc::now();
        ObserveReport {
            goal: GoalRun {
                id: "g1".to_string(),
                workspace_id: "ws-1".to_string(),
                title: "test".to_string(),
                objective: "obj".to_string(),
                status: "running".to_string(),
                priority: "normal".to_string(),
                owner: "test".to_string(),
                budget_json: None,
                policy_json: None,
                current_cycle_id: None,
                created_at: now,
                updated_at: now,
                finished_at: None,
                metadata_json: None,
            },
            current_cycle: Some(GoalCycle {
                id: "c1".to_string(),
                goal_id: "g1".to_string(),
                cycle_no: 1,
                status: "reviewing".to_string(),
                observe_snapshot_ref: None,
                orientation_json: None,
                dispatch_plan_id: None,
                review_summary_ref: None,
                last_graph_hash: None,
                started_at: now,
                updated_at: now,
                finished_at: None,
            }),
            facts: vec![],
            active_tasks: tasks,
            heartbeats: vec![],
            active_leases: vec![],
            recent_events: vec![],
            unread_messages: vec![],
            recent_hints: vec![],
        }
    }

    #[test]
    fn review_all_accepted() {
        let tasks = vec![make_task("t1", "accepted"), make_task("t2", "accepted")];
        let report = make_report(tasks);
        let v = review(&report).unwrap();
        assert!(v.accepted);
        assert!(!v.rework_required);
    }

    #[test]
    fn review_failed_triggers_rework() {
        let tasks = vec![make_task("t1", "accepted"), make_task("t2", "failed")];
        let report = make_report(tasks);
        let v = review(&report).unwrap();
        assert!(!v.accepted);
        assert!(v.rework_required);
    }

    #[test]
    fn review_blocked_waits() {
        let tasks = vec![make_task("t1", "accepted"), make_task("t2", "blocked")];
        let report = make_report(tasks);
        let v = review(&report).unwrap();
        assert!(!v.accepted);
        assert!(!v.rework_required);
        assert!(v.notes.contains("blocked"));
    }

    #[test]
    fn review_rework_required() {
        let tasks = vec![
            make_task("t1", "accepted"),
            make_task("t2", "rework_required"),
        ];
        let report = make_report(tasks);
        let v = review(&report).unwrap();
        assert!(!v.accepted);
        assert!(v.rework_required);
    }

    #[test]
    fn review_still_running() {
        let tasks = vec![make_task("t1", "accepted"), make_task("t2", "running")];
        let report = make_report(tasks);
        let v = review(&report).unwrap();
        assert!(!v.accepted);
        assert!(!v.rework_required);
        assert!(v.notes.contains("running"));
    }

    #[test]
    fn review_empty_tasks() {
        let report = make_report(vec![]);
        let v = review(&report).unwrap();
        assert!(!v.accepted);
        assert!(v.notes.contains("no tasks"));
    }
}
