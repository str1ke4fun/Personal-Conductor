// Orient phase: analyze the observe report and produce structured insights

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::observe::ObserveReport;

/// Structured analysis of the current goal state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrientReport {
    /// What is missing to reach the goal
    pub goal_gap: String,
    /// Blocking issues preventing progress
    pub blockers: Vec<String>,
    /// Unresolved dependencies
    pub dependencies: Vec<String>,
    /// Identified risks
    pub risks: Vec<String>,
    /// Which agents can handle which work
    pub agent_fit: Vec<AgentFit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFit {
    pub agent_id: String,
    pub capabilities: Vec<String>,
    pub current_load: i64,
    pub is_available: bool,
}

/// Analyze the observe report and produce an orient report.
pub fn orient(report: &ObserveReport) -> Result<OrientReport> {
    let mut blockers = Vec::new();
    let mut dependencies = Vec::new();
    let mut risks = Vec::new();

    // Analyze active tasks for blockers
    for task in &report.active_tasks {
        if task.status == "blocked" {
            blockers.push(format!(
                "task '{}' is blocked: {}",
                task.id,
                task.error.as_deref().unwrap_or("unknown")
            ));
        }
        if task.status == "rework_required" {
            blockers.push(format!("task '{}' needs rework", task.id));
        }
        // Check dependencies
        for dep_id in &task.dependencies_json {
            let dep_completed = report
                .active_tasks
                .iter()
                .any(|t| t.id == *dep_id && t.status == "accepted");
            if !dep_completed {
                dependencies.push(format!("task '{}' depends on '{}'", task.id, dep_id));
            }
        }
    }

    // Analyze heartbeats for agent availability
    let active_agent_count = report
        .heartbeats
        .iter()
        .filter(|h| h.status != "idle")
        .count();
    if active_agent_count == 0 && !report.active_tasks.is_empty() {
        risks.push("no active agents but tasks are pending".to_string());
    }

    // Check for stale heartbeats
    for hb in &report.heartbeats {
        if hb.status == "stale" || hb.status == "blocked" {
            risks.push(format!("agent '{}' is {}", hb.agent_id, hb.status));
        }
    }

    // Build agent fit from heartbeats
    let agent_fit: Vec<AgentFit> = report
        .heartbeats
        .iter()
        .map(|h| AgentFit {
            agent_id: h.agent_id.clone(),
            capabilities: vec![], // would be populated from agent_backends
            current_load: h.active_tool_count,
            is_available: h.status == "idle" || h.status == "working",
        })
        .collect();

    // Determine goal gap
    let completed_tasks = report
        .active_tasks
        .iter()
        .filter(|t| t.status == "accepted")
        .count();
    let total_tasks = report.active_tasks.len();
    let goal_gap = if report.active_tasks.is_empty() {
        "no tasks planned yet".to_string()
    } else if blockers.is_empty() && dependencies.is_empty() {
        format!("{}/{} tasks completed", completed_tasks, total_tasks)
    } else {
        format!(
            "{}/{} tasks completed, {} blockers, {} dependencies",
            completed_tasks,
            total_tasks,
            blockers.len(),
            dependencies.len()
        )
    };

    Ok(OrientReport {
        goal_gap,
        blockers,
        dependencies,
        risks,
        agent_fit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal_tasks::AgentTask;
    use crate::goals::{GoalCycle, GoalRun};
    use chrono::Utc;

    fn make_observe_report(goal_id: &str, tasks: Vec<AgentTask>) -> ObserveReport {
        let now = Utc::now();
        ObserveReport {
            goal: GoalRun {
                id: goal_id.to_string(),
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
                id: "cycle-1".to_string(),
                goal_id: goal_id.to_string(),
                cycle_no: 1,
                status: "executing".to_string(),
                observe_snapshot_ref: None,
                orientation_json: None,
                dispatch_plan_id: None,
                review_summary_ref: None,
                started_at: now,
                updated_at: now,
                finished_at: None,
            }),
            active_tasks: tasks,
            heartbeats: vec![],
            active_leases: vec![],
            recent_events: vec![],
            unread_messages: vec![],
        }
    }

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

    #[test]
    fn orient_empty_tasks_reports_gap() {
        let report = make_observe_report("g1", vec![]);
        let result = orient(&report).unwrap();
        assert_eq!(result.goal_gap, "no tasks planned yet");
        assert!(result.blockers.is_empty());
    }

    #[test]
    fn orient_blocked_task_appears_in_blockers() {
        let mut task = make_task("t1", "blocked");
        task.error = Some("missing dependency".to_string());
        let report = make_observe_report("g1", vec![task]);
        let result = orient(&report).unwrap();
        assert_eq!(result.blockers.len(), 1);
        assert!(result.blockers[0].contains("blocked"));
    }

    #[test]
    fn orient_completed_tasks_counted() {
        let tasks = vec![
            make_task("t1", "accepted"),
            make_task("t2", "running"),
            make_task("t3", "accepted"),
        ];
        let report = make_observe_report("g1", tasks);
        let result = orient(&report).unwrap();
        assert!(result.goal_gap.contains("2/3"));
    }

    #[test]
    fn orient_rework_task_appears_in_blockers() {
        let task = make_task("t1", "rework_required");
        let report = make_observe_report("g1", vec![task]);
        let result = orient(&report).unwrap();
        assert_eq!(result.blockers.len(), 1);
        assert!(result.blockers[0].contains("rework"));
    }
}
