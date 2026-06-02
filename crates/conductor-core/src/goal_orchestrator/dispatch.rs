// Dispatch logic: parallel governance + dependency scheduling + retry + cycle protection
//
// TASK-095: max_parallel_agents + write_scope conflict detection
// TASK-096: dependency scheduling + failure retry + cycle loop protection

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::decide::PlannedTask;

/// Configuration for dispatch governance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchConfig {
    /// Maximum number of agents that can run in parallel for this goal.
    pub max_parallel_agents: usize,
    /// Maximum retry count per task before giving up.
    pub max_retry_count: usize,
    /// Maximum consecutive failures with the same reason before blocking the goal.
    pub max_consecutive_failures: usize,
    /// Whether write_scope conflicts should block or just warn.
    pub write_scope_conflict_policy: ConflictPolicy,
}

impl Default for DispatchConfig {
    fn default() -> Self {
        Self {
            max_parallel_agents: 3,
            max_retry_count: 2,
            max_consecutive_failures: 3,
            write_scope_conflict_policy: ConflictPolicy::Block,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// Block the conflicting task
    Block,
    /// Warn but allow
    Warn,
}

/// Result of dispatch filtering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchFilterResult {
    /// Tasks approved for dispatch
    pub approved: Vec<PlannedTask>,
    /// Tasks held back (queued for later)
    pub held: Vec<HeldTask>,
    /// Tasks rejected (conflict or cycle protection)
    pub rejected: Vec<RejectedTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeldTask {
    pub task: PlannedTask,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectedTask {
    pub task: PlannedTask,
    pub reason: String,
}

/// Information about a currently running agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveAgent {
    pub agent_id: String,
    pub write_scope: Vec<String>,
    pub task_id: String,
}

/// Failure history for cycle protection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureRecord {
    pub task_title: String,
    pub reason: String,
    pub count: usize,
}

/// Filter planned tasks through governance checks:
/// 1. Dependency check — skip tasks whose dependencies aren't met
/// 2. Parallel limit — hold tasks if max_parallel_agents reached
/// 3. Write scope conflict — block/hold tasks that conflict with running agents
/// 4. Cycle protection — reject tasks that have failed too many times
pub fn filter_dispatch(
    tasks: Vec<PlannedTask>,
    active_agents: &[ActiveAgent],
    completed_task_ids: &[String],
    failure_history: &[FailureRecord],
    config: &DispatchConfig,
) -> DispatchFilterResult {
    let mut approved = Vec::new();
    let mut held = Vec::new();
    let mut rejected = Vec::new();

    // Count non-idle active agents
    let active_count = active_agents.len();

    for task in tasks {
        // 1. Dependency check
        let deps_met = task
            .dependencies
            .iter()
            .all(|dep| completed_task_ids.contains(dep));
        if !deps_met {
            let unmet: Vec<String> = task
                .dependencies
                .iter()
                .filter(|d| !completed_task_ids.contains(*d))
                .cloned()
                .collect();
            held.push(HeldTask {
                task,
                reason: format!("dependencies not met: {:?}", unmet),
            });
            continue;
        }

        // 2. Cycle protection — check if this task has failed too many times
        let cycle_rejected = failure_history
            .iter()
            .find(|f| f.task_title == task.title)
            .filter(|record| record.count >= config.max_consecutive_failures)
            .map(|record| {
                format!(
                    "cycle protection: failed {} times (max {}), reason: {}",
                    record.count, config.max_consecutive_failures, record.reason
                )
            });
        if let Some(reason) = cycle_rejected {
            rejected.push(RejectedTask { task, reason });
            continue;
        }

        // 3. Parallel limit
        if active_count + approved.len() >= config.max_parallel_agents {
            held.push(HeldTask {
                task,
                reason: format!(
                    "parallel limit reached: {} active + {} approved >= max {}",
                    active_count,
                    approved.len(),
                    config.max_parallel_agents
                ),
            });
            continue;
        }

        // 4. Write scope conflict detection
        if config.write_scope_conflict_policy == ConflictPolicy::Block {
            if let Some(conflict) = find_write_scope_conflict(&task, active_agents) {
                held.push(HeldTask {
                    task,
                    reason: format!(
                        "write_scope conflict with agent '{}': {}",
                        conflict.0, conflict.1
                    ),
                });
                continue;
            }
        }

        approved.push(task);
    }

    DispatchFilterResult {
        approved,
        held,
        rejected,
    }
}

/// Check if a task's write_scope overlaps with any active agent's write_scope.
fn find_write_scope_conflict(
    task: &PlannedTask,
    active_agents: &[ActiveAgent],
) -> Option<(String, String)> {
    for agent in active_agents {
        for task_scope in &task.write_scope {
            for agent_scope in &agent.write_scope {
                if paths_overlap(task_scope, agent_scope) {
                    return Some((
                        agent.agent_id.clone(),
                        format!("'{}' overlaps with '{}'", task_scope, agent_scope),
                    ));
                }
            }
        }
    }
    None
}

/// Check if two paths overlap (one is a prefix of the other).
fn paths_overlap(a: &str, b: &str) -> bool {
    let a_norm = normalize_path(a);
    let b_norm = normalize_path(b);
    a_norm.starts_with(&b_norm) || b_norm.starts_with(&a_norm)
}

/// Normalize path separators for comparison.
fn normalize_path(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_string()
}

/// Update failure history after a task fails.
/// Returns the updated record and whether cycle protection should trigger.
pub fn record_failure(
    history: &mut Vec<FailureRecord>,
    task_title: &str,
    reason: &str,
    config: &DispatchConfig,
) -> (FailureRecord, bool) {
    if let Some(record) = history.iter_mut().find(|f| f.task_title == task_title) {
        if record.reason == reason {
            record.count += 1;
        } else {
            // Different reason — reset count
            record.reason = reason.to_string();
            record.count = 1;
        }
        (
            record.clone(),
            record.count >= config.max_consecutive_failures,
        )
    } else {
        let record = FailureRecord {
            task_title: task_title.to_string(),
            reason: reason.to_string(),
            count: 1,
        };
        history.push(record.clone());
        (record, false)
    }
}

/// Determine if a failed task should be retried.
pub fn should_retry(failure_record: &FailureRecord, config: &DispatchConfig) -> bool {
    failure_record.count <= config.max_retry_count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(title: &str, deps: Vec<&str>, scopes: Vec<&str>) -> PlannedTask {
        PlannedTask {
            title: title.to_string(),
            instruction: format!("do {title}"),
            agent_kind: "backend-agent".to_string(),
            write_scope: scopes.iter().map(|s| s.to_string()).collect(),
            read_scope: vec![],
            allowed_tools: vec![],
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            acceptance: vec!["done".to_string()],
        }
    }

    fn make_agent(id: &str, task_id: &str, scopes: Vec<&str>) -> ActiveAgent {
        ActiveAgent {
            agent_id: id.to_string(),
            task_id: task_id.to_string(),
            write_scope: scopes.iter().map(|s| s.to_string()).collect(),
        }
    }

    // ── TASK-095: Parallel governance ────────────────────────────────────

    #[test]
    fn parallel_limit_holds_excess_tasks() {
        let tasks = vec![
            make_task("t1", vec![], vec![]),
            make_task("t2", vec![], vec![]),
            make_task("t3", vec![], vec![]),
            make_task("t4", vec![], vec![]),
        ];
        let config = DispatchConfig {
            max_parallel_agents: 2,
            ..Default::default()
        };

        let result = filter_dispatch(tasks, &[], &[], &[], &config);
        assert_eq!(result.approved.len(), 2);
        assert_eq!(result.held.len(), 2);
        assert!(result.held[0].reason.contains("parallel limit"));
    }

    #[test]
    fn write_scope_conflict_blocks_task() {
        let tasks = vec![make_task("t1", vec![], vec!["src/main.rs"])];
        let active = vec![make_agent("a1", "t0", vec!["src/main.rs"])];
        let config = DispatchConfig {
            write_scope_conflict_policy: ConflictPolicy::Block,
            ..Default::default()
        };

        let result = filter_dispatch(tasks, &active, &[], &[], &config);
        assert_eq!(result.approved.len(), 0);
        assert_eq!(result.held.len(), 1);
        assert!(result.held[0].reason.contains("write_scope conflict"));
    }

    #[test]
    fn write_scope_no_conflict_different_paths() {
        let tasks = vec![make_task("t1", vec![], vec!["src/a.rs"])];
        let active = vec![make_agent("a1", "t0", vec!["src/b.rs"])];
        let config = DispatchConfig {
            write_scope_conflict_policy: ConflictPolicy::Block,
            ..Default::default()
        };

        let result = filter_dispatch(tasks, &active, &[], &[], &config);
        assert_eq!(result.approved.len(), 1);
        assert_eq!(result.held.len(), 0);
    }

    #[test]
    fn write_scope_conflict_parent_path() {
        // Task writes to src/module/file.rs, agent holds src/module/
        let tasks = vec![make_task("t1", vec![], vec!["src/module/file.rs"])];
        let active = vec![make_agent("a1", "t0", vec!["src/module"])];
        let config = DispatchConfig {
            write_scope_conflict_policy: ConflictPolicy::Block,
            ..Default::default()
        };

        let result = filter_dispatch(tasks, &active, &[], &[], &config);
        assert_eq!(result.approved.len(), 0);
        assert_eq!(result.held.len(), 1);
    }

    #[test]
    fn write_scope_conflict_warn_allows() {
        let tasks = vec![make_task("t1", vec![], vec!["src/main.rs"])];
        let active = vec![make_agent("a1", "t0", vec!["src/main.rs"])];
        let config = DispatchConfig {
            write_scope_conflict_policy: ConflictPolicy::Warn,
            ..Default::default()
        };

        let result = filter_dispatch(tasks, &active, &[], &[], &config);
        assert_eq!(result.approved.len(), 1);
        assert_eq!(result.held.len(), 0);
    }

    // ── TASK-096: Dependency scheduling + retry + cycle protection ───────

    #[test]
    fn dependency_check_holds_unmet_deps() {
        let tasks = vec![make_task("t1", vec!["dep-1"], vec![])];
        let config = DispatchConfig::default();

        let result = filter_dispatch(tasks, &[], &[], &[], &config);
        assert_eq!(result.approved.len(), 0);
        assert_eq!(result.held.len(), 1);
        assert!(result.held[0].reason.contains("dependencies not met"));
    }

    #[test]
    fn dependency_check_passes_when_met() {
        let tasks = vec![make_task("t1", vec!["dep-1"], vec![])];
        let completed = vec!["dep-1".to_string()];
        let config = DispatchConfig::default();

        let result = filter_dispatch(tasks, &[], &completed, &[], &config);
        assert_eq!(result.approved.len(), 1);
        assert_eq!(result.held.len(), 0);
    }

    #[test]
    fn cycle_protection_rejects_excessive_failures() {
        let tasks = vec![make_task("t1", vec![], vec![])];
        let failures = vec![FailureRecord {
            task_title: "t1".to_string(),
            reason: "timeout".to_string(),
            count: 3,
        }];
        let config = DispatchConfig {
            max_consecutive_failures: 3,
            ..Default::default()
        };

        let result = filter_dispatch(tasks, &[], &[], &failures, &config);
        assert_eq!(result.approved.len(), 0);
        assert_eq!(result.rejected.len(), 1);
        assert!(result.rejected[0].reason.contains("cycle protection"));
    }

    #[test]
    fn record_failure_increments_count() {
        let mut history = vec![];
        let config = DispatchConfig::default();

        let (record, should_block) = record_failure(&mut history, "t1", "timeout", &config);
        assert_eq!(record.count, 1);
        assert!(!should_block);

        let (record, should_block) = record_failure(&mut history, "t1", "timeout", &config);
        assert_eq!(record.count, 2);
        assert!(!should_block);
    }

    #[test]
    fn record_failure_different_reason_resets_count() {
        let mut history = vec![];
        let config = DispatchConfig::default();

        record_failure(&mut history, "t1", "timeout", &config);
        let (record, _) = record_failure(&mut history, "t1", "oom", &config);
        assert_eq!(record.count, 1); // reset because reason changed
    }

    #[test]
    fn should_retry_within_limit() {
        let config = DispatchConfig {
            max_retry_count: 2,
            ..Default::default()
        };
        let record = FailureRecord {
            task_title: "t1".to_string(),
            reason: "timeout".to_string(),
            count: 1,
        };
        assert!(should_retry(&record, &config));

        let record2 = FailureRecord {
            task_title: "t1".to_string(),
            reason: "timeout".to_string(),
            count: 3,
        };
        assert!(!should_retry(&record2, &config));
    }

    #[test]
    fn combined_governance_all_checks() {
        let tasks = vec![
            make_task("t1", vec![], vec![]),              // should pass
            make_task("t2", vec!["dep-x"], vec![]),       // dep not met
            make_task("t3", vec![], vec!["src/main.rs"]), // scope conflict
            make_task("t4", vec![], vec![]),              // parallel limit
        ];
        let active = vec![make_agent("a1", "t0", vec!["src/main.rs"])];
        let config = DispatchConfig {
            max_parallel_agents: 2,
            write_scope_conflict_policy: ConflictPolicy::Block,
            ..Default::default()
        };

        let result = filter_dispatch(tasks, &active, &[], &[], &config);
        // t1 approved, t2 held (deps), t3 held (conflict), t4 approved (2 active + 0 approved < 2? no: 1+0 < 2 yes, then 1+1=2)
        // Actually: active_count=1, t1 approved (1+0=1 < 2), t2 held, t3 held, t4: 1+1=2 >= 2 → held
        assert_eq!(result.approved.len(), 1); // only t1
        assert_eq!(result.held.len(), 3);
    }
}
