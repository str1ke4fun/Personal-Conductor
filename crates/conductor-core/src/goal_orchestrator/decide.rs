// Decide phase: generate a DispatchPlan from the OrientReport

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::orient::OrientReport;

/// Budget limits for a goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    pub max_cycles: Option<i64>,
    pub max_wall_time_secs: Option<i64>,
    pub max_agent_runs: Option<i64>,
    pub max_tool_calls: Option<i64>,
    pub cycles_used: i64,
    pub wall_time_used_secs: i64,
    pub agent_runs_used: i64,
    pub tool_calls_used: i64,
}

impl Budget {
    /// Check if any budget limit is exhausted.
    pub fn is_exhausted(&self) -> (bool, Option<String>) {
        if let Some(max) = self.max_cycles {
            if self.cycles_used >= max {
                return (true, Some(format!("max_cycles ({}) exhausted", max)));
            }
        }
        if let Some(max) = self.max_wall_time_secs {
            if self.wall_time_used_secs >= max {
                return (true, Some(format!("max_wall_time ({}s) exhausted", max)));
            }
        }
        if let Some(max) = self.max_agent_runs {
            if self.agent_runs_used >= max {
                return (true, Some(format!("max_agent_runs ({}) exhausted", max)));
            }
        }
        if let Some(max) = self.max_tool_calls {
            if self.tool_calls_used >= max {
                return (true, Some(format!("max_tool_calls ({}) exhausted", max)));
            }
        }
        (false, None)
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_cycles: Some(10),
            max_wall_time_secs: Some(3600),
            max_agent_runs: Some(50),
            max_tool_calls: Some(200),
            cycles_used: 0,
            wall_time_used_secs: 0,
            agent_runs_used: 0,
            tool_calls_used: 0,
        }
    }
}

/// A single planned task in the dispatch plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedTask {
    pub title: String,
    pub instruction: String,
    pub agent_kind: String,
    pub write_scope: Vec<String>,
    pub read_scope: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub dependencies: Vec<String>,
    pub acceptance: Vec<String>,
}

/// The dispatch plan produced by the Decide phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchPlan {
    pub tasks: Vec<PlannedTask>,
    pub write_scope: Vec<String>,
    pub acceptance: Vec<String>,
    pub budget_remaining: Budget,
    pub approved: bool,
}

/// Generate a DispatchPlan from the orient report.
///
/// If there are blockers or unresolved dependencies, the plan will be empty
/// (nothing to dispatch until blockers are cleared).
pub fn decide(orient: &OrientReport, budget: &Budget, objective: &str) -> Result<DispatchPlan> {
    // Check budget first
    let (exhausted, reason) = budget.is_exhausted();
    if exhausted {
        return Ok(DispatchPlan {
            tasks: vec![],
            write_scope: vec![],
            acceptance: vec![],
            budget_remaining: budget.clone(),
            approved: false,
        });
    }

    // If there are blockers, don't dispatch new work
    if !orient.blockers.is_empty() {
        return Ok(DispatchPlan {
            tasks: vec![],
            write_scope: vec![],
            acceptance: vec![],
            budget_remaining: budget.clone(),
            approved: false,
        });
    }

    // Generate tasks based on the goal gap
    // This is a simplified planner — a real implementation would use LLM
    let tasks: Vec<PlannedTask> = if orient.goal_gap.contains("no tasks planned") {
        // Direct execution task: carry the user's original intent verbatim.
        // The runner LLM should execute the goal, not just plan it.
        let instruction = if objective.is_empty() {
            "Execute the goal and produce a written summary of results.".to_string()
        } else {
            format!(
                "{}\n\n\
                When done, produce a written summary of what you did, \
                which files were created or modified, and what the next steps are. \
                Write any significant output to a file in the workspace and include \
                the file path in your summary.",
                objective
            )
        };
        vec![PlannedTask {
            title: "execute_goal".to_string(),
            instruction,
            agent_kind: "backend-agent".to_string(),
            write_scope: vec![],
            read_scope: vec![],
            // Empty means "use the long-mode default tool policy" so Goal tasks
            // can still reach agent/team workflows unless a later planner
            // narrows the task to an explicit allowlist.
            allowed_tools: vec![],
            dependencies: vec![],
            acceptance: vec!["done".to_string()],
        }]
    } else {
        // For ongoing goals, create tasks based on agent availability
        orient
            .agent_fit
            .iter()
            .filter(|a| a.is_available)
            .map(|a| PlannedTask {
                title: format!("execute_{}", a.agent_id),
                instruction: "Continue working on the goal".to_string(),
                agent_kind: a.agent_id.clone(),
                write_scope: vec![],
                read_scope: vec![],
                allowed_tools: vec![],
                dependencies: vec![],
                acceptance: vec![],
            })
            .collect()
    };

    let write_scope: Vec<String> = tasks.iter().flat_map(|t| t.write_scope.clone()).collect();
    let acceptance: Vec<String> = tasks.iter().flat_map(|t| t.acceptance.clone()).collect();

    Ok(DispatchPlan {
        tasks,
        write_scope,
        acceptance,
        budget_remaining: budget.clone(),
        approved: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal_orchestrator::orient::AgentFit;

    fn make_orient(goal_gap: &str, blockers: Vec<String>) -> OrientReport {
        OrientReport {
            goal_gap: goal_gap.to_string(),
            blockers,
            dependencies: vec![],
            risks: vec![],
            agent_fit: vec![AgentFit {
                agent_id: "agent-1".to_string(),
                capabilities: vec!["code".to_string()],
                current_load: 0,
                is_available: true,
            }],
        }
    }

    #[test]
    fn decide_with_blockers_returns_empty_plan() {
        let orient = make_orient("gap", vec!["blocked".to_string()]);
        let budget = Budget::default();
        let plan = decide(&orient, &budget, "").unwrap();
        assert!(plan.tasks.is_empty());
    }

    #[test]
    fn decide_with_exhausted_budget_returns_empty() {
        let orient = make_orient("gap", vec![]);
        let budget = Budget {
            max_cycles: Some(5),
            cycles_used: 5,
            ..Default::default()
        };
        let plan = decide(&orient, &budget, "").unwrap();
        assert!(plan.tasks.is_empty());
    }

    #[test]
    fn decide_initial_plan_creates_analyze_task() {
        let orient = make_orient("no tasks planned yet", vec![]);
        let budget = Budget::default();
        let plan = decide(&orient, &budget, "").unwrap();
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].title, "execute_goal");
    }

    #[test]
    fn decide_initial_plan_uses_objective() {
        let orient = make_orient("no tasks planned yet", vec![]);
        let budget = Budget::default();
        let plan = decide(&orient, &budget, "Refactor the auth module").unwrap();
        assert_eq!(plan.tasks.len(), 1);
        assert!(plan.tasks[0]
            .instruction
            .contains("Refactor the auth module"));
    }

    #[test]
    fn decide_plan_starts_unapproved() {
        let orient = make_orient("gap", vec![]);
        let budget = Budget::default();
        let plan = decide(&orient, &budget, "").unwrap();
        assert!(!plan.approved);
    }

    #[test]
    fn budget_exhaustion_reports_reason() {
        let budget = Budget {
            max_wall_time_secs: Some(600),
            wall_time_used_secs: 601,
            ..Default::default()
        };
        let (exhausted, reason) = budget.is_exhausted();
        assert!(exhausted);
        assert!(reason.unwrap().contains("max_wall_time"));
    }
}
