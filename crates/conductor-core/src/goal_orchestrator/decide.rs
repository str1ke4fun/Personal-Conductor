// Decide phase: generate a DispatchPlan from the OrientReport

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::contracts::{validate_reason_output, ReasonOutput};
use super::observe::ObserveReport;
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

/// LLM-based Reason phase: calls the configured model to produce a `DispatchPlan`.
///
/// This is the async LLM-backed counterpart of `decide()`.  The caller can
/// switch to this path once it is wired into `tick_goal`; until then it can be
/// invoked standalone for testing.
pub async fn decide_llm(
    observe: &ObserveReport,
    orient: &OrientReport,
    budget: &Budget,
    objective: &str,
    goal_id: &str,
) -> Result<DispatchPlan> {
    // ── 1. Budget gate (same fast-path as decide()) ──────────────────────────
    let (exhausted, _reason) = budget.is_exhausted();
    if exhausted {
        return Ok(DispatchPlan {
            tasks: vec![],
            write_scope: vec![],
            acceptance: vec![],
            budget_remaining: budget.clone(),
            approved: false,
        });
    }

    // ── 2. Resolve model ─────────────────────────────────────────────────────
    use crate::model_resolver::{CallerContext, ModelResolver};
    use crate::routing::{OodaPhase, WorkKind};

    let resolved = ModelResolver::resolve(
        CallerContext::GoalOrchestrator {
            phase: OodaPhase::Reason,
            work_kind: WorkKind::Planning,
        },
        None,
    )
    .await?;

    // ── 3. Load global config for LlmRequestConfig fallback ─────────────────
    let config = crate::config::load().await.unwrap_or_default();
    let llm_cfg = crate::llm::LlmRequestConfig::from_resolved_with_fallback(&resolved, &config.llm);

    // ── 4. Build Chinese-language system prompt ──────────────────────────────
    let graph_yaml = build_reason_graph_yaml(observe);
    let fact_ids_text = format_reason_fact_ids(observe);
    let open_intents_text = format_reason_open_intents(observe);
    let hints_text = format_reason_hints(observe);

    let facts_text = orient
        .agent_fit
        .iter()
        .map(|a| {
            format!(
                "  - agent={} load={} available={}",
                a.agent_id, a.current_load, a.is_available
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let system_prompt = format!(
        r#"你是一个目标编排 Agent 的推理模块（Reason 阶段）。
你的任务是根据当前目标状态，决定下一步要派发的工作意图（intents）。

# 目标信息
- goal_id: {goal_id}
- 目标描述: {objective}

# 当前状态分析
- 目标缺口: {goal_gap}
- 阻碍项: {blockers}
- 依赖项: {dependencies}
- 风险项: {risks}
- Agent 状态:
{facts_text}

# 输出格式
请严格输出以下三种 JSON 格式之一，不要输出任何其他内容：

1. 如果目标已完成：
{{"complete": {{"from": ["fact-id"], "description": "完成说明"}}}}

2. 如果需要派发新意图（最多 5 个）：
{{"intents": [{{"from": ["fact-id"], "title": "简短标题", "instruction": "详细执行指令"}}]}}

3. 如果暂时无需操作：
{{}}

注意：
- "from" 字段引用与该意图相关的 fact ID，若无具体 fact 可用 ["root"] 作为占位符
- 每个 intent 的 title 不能为空
- intents 数量不超过 5 个
"#,
        goal_id = goal_id,
        objective = if objective.is_empty() {
            "（未指定）"
        } else {
            objective
        },
        goal_gap = orient.goal_gap,
        blockers = if orient.blockers.is_empty() {
            "无".to_string()
        } else {
            orient.blockers.join("；")
        },
        dependencies = if orient.dependencies.is_empty() {
            "无".to_string()
        } else {
            orient.dependencies.join("；")
        },
        risks = if orient.risks.is_empty() {
            "无".to_string()
        } else {
            orient.risks.join("；")
        },
        facts_text = if facts_text.is_empty() {
            "  （无在线 Agent）".to_string()
        } else {
            facts_text
        },
    );

    let graph_context = format!(
        r#"You are running the Cairn-style Reason step for this goal.
Treat the graph below as the primary public contract:
- Facts are goal-scoped observations already written back.
- Open intents are unresolved work directions.
- Hints are human strategy inputs that should steer the next decision.

Reason over the full graph before deciding whether the goal is complete,
whether to create new intents, or whether to return noop.

## Graph YAML
```yaml
{graph_yaml}
```

## Fact IDs
```json
{fact_ids_text}
```

## Open Intents
```json
{open_intents_text}
```

## Hints
```json
{hints_text}
```
"#
    );
    let system_prompt = format!("{graph_context}\n\n{system_prompt}");

    let user_msg = "请根据以上状态，输出下一步的 Reason 结论（JSON 格式）。";

    // ── 5. Call LLM ──────────────────────────────────────────────────────────
    let raw_response = crate::llm::call(&resolved.model_id, &system_prompt, user_msg, &llm_cfg)
        .await
        .context("decide_llm: LLM call failed")?;

    // ── 6. Strip markdown fences if present, then parse + validate ───────────
    let json_str = extract_json(&raw_response);
    let reason_output = validate_reason_output(json_str)
        .map_err(|e| anyhow::anyhow!("decide_llm: contract validation failed: {e}"))?;

    // ── 7. Convert ReasonOutput → DispatchPlan ───────────────────────────────
    let (tasks, approved) = match reason_output {
        ReasonOutput::Complete { .. } => {
            // Goal is complete — return an approved, empty plan so tick_goal
            // can transition the goal to "completed".
            (vec![], true)
        }
        ReasonOutput::Intents { intents } => {
            let tasks = intents
                .into_iter()
                .map(|intent| PlannedTask {
                    title: intent.title,
                    instruction: intent.instruction,
                    agent_kind: "backend-agent".to_string(),
                    write_scope: vec![],
                    read_scope: vec![],
                    allowed_tools: vec![],
                    dependencies: vec![],
                    acceptance: vec!["done".to_string()],
                })
                .collect();
            (tasks, false)
        }
        ReasonOutput::Noop {} => (vec![], false),
    };

    let write_scope: Vec<String> = tasks.iter().flat_map(|t| t.write_scope.clone()).collect();
    let acceptance: Vec<String> = tasks.iter().flat_map(|t| t.acceptance.clone()).collect();

    Ok(DispatchPlan {
        tasks,
        write_scope,
        acceptance,
        budget_remaining: budget.clone(),
        approved,
    })
}

fn build_reason_graph_yaml(report: &ObserveReport) -> String {
    let facts = report
        .facts
        .iter()
        .map(|fact| {
            serde_json::json!({
                "id": fact.id,
                "key": fact.key,
                "category": fact.category,
                "description": truncate_reason_text(&fact.value, 1200),
                "updated_at": fact.updated_at,
            })
        })
        .collect::<Vec<_>>();
    let open_intents = open_intent_values(report);
    let hints = hint_values(report);
    let graph = serde_json::json!({
        "goal": {
            "id": report.goal.id,
            "title": report.goal.title,
            "objective": report.goal.objective,
            "status": report.goal.status,
        },
        "facts": facts,
        "open_intents": open_intents,
        "hints": hints,
    });

    serde_yaml::to_string(&graph).unwrap_or_else(|_| graph.to_string())
}

fn format_reason_fact_ids(report: &ObserveReport) -> String {
    let ids = report
        .facts
        .iter()
        .map(|fact| fact.id.clone())
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&ids).unwrap_or_else(|_| "[]".to_string())
}

fn format_reason_open_intents(report: &ObserveReport) -> String {
    serde_json::to_string_pretty(&open_intent_values(report)).unwrap_or_else(|_| "[]".to_string())
}

fn format_reason_hints(report: &ObserveReport) -> String {
    serde_json::to_string_pretty(&hint_values(report)).unwrap_or_else(|_| "[]".to_string())
}

fn open_intent_values(report: &ObserveReport) -> Vec<serde_json::Value> {
    report
        .active_tasks
        .iter()
        .filter(|task| matches!(task.status.as_str(), "proposed" | "queued" | "claimed"))
        .map(|task| {
            serde_json::json!({
                "id": task.id,
                "title": task.title,
                "status": task.status,
                "instruction": task.instruction,
                "agent_kind": task.agent_kind,
                "updated_at": task.updated_at.to_rfc3339(),
            })
        })
        .collect()
}

fn hint_values(report: &ObserveReport) -> Vec<serde_json::Value> {
    report
        .recent_hints
        .iter()
        .filter(|hint| hint.status == "active")
        .map(|hint| {
            serde_json::json!({
                "id": hint.id,
                "kind": hint.kind,
                "content": hint.content,
                "created_at": hint.created_at.to_rfc3339(),
            })
        })
        .collect()
}

fn truncate_reason_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

/// Extract the first JSON object from an LLM response, stripping markdown
/// code fences (` ```json ... ``` `) if present.
fn extract_json(raw: &str) -> &str {
    let trimmed = raw.trim();
    // Strip ```json ... ``` or ``` ... ```
    if let Some(inner) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    {
        if let Some(end) = inner.rfind("```") {
            return inner[..end].trim();
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goal_hints::GoalHint;
    use crate::goal_orchestrator::observe::{ObserveReport, ObservedFact};
    use crate::goal_orchestrator::orient::AgentFit;
    use crate::goal_tasks::AgentTask;
    use crate::goals::GoalRun;
    use chrono::Utc;

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

    fn make_reason_observe_report() -> ObserveReport {
        let now = Utc::now();
        ObserveReport {
            goal: GoalRun {
                id: "goal-1".to_string(),
                workspace_id: "ws-1".to_string(),
                title: "Reason graph".to_string(),
                objective: "Use graph as reason input".to_string(),
                status: "planning".to_string(),
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
            current_cycle: None,
            facts: vec![ObservedFact {
                id: "fact-1".to_string(),
                key: "scan.result".to_string(),
                value: "The workspace scan completed.".to_string(),
                category: "assistant_final_answer".to_string(),
                updated_at: now.to_rfc3339(),
            }],
            active_tasks: vec![AgentTask {
                id: "intent-1".to_string(),
                workspace_id: "ws-1".to_string(),
                goal_id: Some("goal-1".to_string()),
                cycle_id: None,
                parent_task_id: None,
                title: "Follow up".to_string(),
                instruction: "Investigate the next gap.".to_string(),
                status: "queued".to_string(),
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
            }],
            heartbeats: vec![],
            active_leases: vec![],
            recent_events: vec![],
            unread_messages: vec![],
            recent_hints: vec![GoalHint {
                id: "hint-1".to_string(),
                goal_id: "goal-1".to_string(),
                cycle_id: None,
                kind: "user".to_string(),
                content: "Prefer the graph route.".to_string(),
                status: "active".to_string(),
                created_at: now,
                updated_at: now,
                expires_at: None,
            }],
        }
    }

    #[test]
    fn reason_graph_context_contains_facts_intents_and_hints() {
        let report = make_reason_observe_report();
        let yaml = build_reason_graph_yaml(&report);

        assert!(yaml.contains("fact-1"));
        assert!(yaml.contains("The workspace scan completed."));
        assert!(yaml.contains("intent-1"));
        assert!(yaml.contains("Investigate the next gap."));
        assert!(yaml.contains("hint-1"));
        assert!(yaml.contains("Prefer the graph route."));
        assert!(format_reason_fact_ids(&report).contains("fact-1"));
        assert!(format_reason_open_intents(&report).contains("intent-1"));
        assert!(format_reason_hints(&report).contains("hint-1"));
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
