// GoalOrchestrator — single-goal OODA loop
//
// Observe → Orient → Decide → Act → Review
//
// Guard: Act can only create Task/Lease/Message/Event, never execute tools directly.

pub mod act;
pub mod decide;
pub mod dispatch;
pub mod observe;
pub mod orient;
pub mod review;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::adapters::agent_team_adapter::AgentTeamAdapter;
use crate::agent_teams::{self, AgentTeamLifecycle};

use self::act::ActResult;
use self::decide::{Budget, DispatchPlan};
use self::observe::ObserveReport;
use self::orient::OrientReport;
use self::review::ReviewVerdict;

/// Configuration for the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    pub workspace_id: String,
    pub budget: Budget,
    /// Whether the plan approval gate is enabled
    pub require_plan_approval: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            workspace_id: "default".to_string(),
            budget: Budget::default(),
            require_plan_approval: false,
        }
    }
}

/// Result of a single cycle execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleResult {
    pub cycle_id: String,
    pub observe: ObserveReport,
    pub orient: OrientReport,
    pub plan: DispatchPlan,
    pub act_result: Option<ActResult>,
    pub review: ReviewVerdict,
}

/// The orchestrator — coordinates the OODA loop for a single goal.
pub struct GoalOrchestrator {
    config: OrchestratorConfig,
}

impl GoalOrchestrator {
    pub fn new(config: OrchestratorConfig) -> Self {
        Self { config }
    }

    /// Start a goal: transition from draft → planning.
    pub async fn start(&self, goal_id: &str) -> Result<()> {
        let goal = crate::goals::get_goal(goal_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;

        if goal.status != "draft" {
            bail!(
                "can only start a draft goal, current status: {}",
                goal.status
            );
        }

        crate::goals::update_goal_status(goal_id, "planning").await?;
        Ok(())
    }

    /// Pause a running goal.
    pub async fn pause(&self, goal_id: &str) -> Result<()> {
        let goal = crate::goals::get_goal(goal_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;

        if goal.status != "running" {
            bail!(
                "can only pause a running goal, current status: {}",
                goal.status
            );
        }

        crate::goals::update_goal_status(goal_id, "blocked").await?;
        Ok(())
    }

    /// Resume a blocked goal.
    pub async fn resume(&self, goal_id: &str) -> Result<()> {
        let goal = crate::goals::get_goal(goal_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;

        if goal.status != "blocked" {
            bail!(
                "can only resume a blocked goal, current status: {}",
                goal.status
            );
        }

        if let Some(cycle_id) = goal.current_cycle_id.as_deref() {
            let tasks = crate::goal_tasks::list_tasks_by_cycle(cycle_id).await?;
            for task in tasks.iter().filter(|task| task.status == "blocked") {
                crate::goal_tasks::resume_blocked_task(&task.id).await?;
            }
        }

        crate::goals::update_goal_status(goal_id, "running").await?;
        Ok(())
    }

    /// Cancel a goal.
    pub async fn cancel(&self, goal_id: &str) -> Result<()> {
        let goal = crate::goals::get_goal(goal_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;

        if matches!(goal.status.as_str(), "accepted" | "archived" | "cancelled") {
            bail!("cannot cancel goal in status: {}", goal.status);
        }

        crate::goals::update_goal_status(goal_id, "cancelled").await?;
        Ok(())
    }

    /// Run a single OODA cycle for the goal.
    ///
    /// 1. Observe: read current state
    /// 2. Orient: analyze state
    /// 3. Decide: generate plan
    /// 4. Act: create tasks (if plan approved)
    /// 5. Review: collect verdicts
    pub async fn run_cycle(&self, goal_id: &str) -> Result<CycleResult> {
        // Determine cycle number
        let cycles = crate::goals::list_cycles_by_goal(goal_id).await?;
        let cycle_no = cycles.len() as i64 + 1;
        let cycle = crate::goals::create_cycle(goal_id, cycle_no).await?;

        // Observe
        let observe_report = self.observe(goal_id).await?;
        crate::goals::advance_cycle_phase(&cycle.id, "orienting").await?;

        // Orient
        let orient_report = self.orient(&observe_report)?;
        crate::goals::advance_cycle_phase(&cycle.id, "deciding").await?;

        // Decide
        let mut plan = self.decide(&orient_report, &observe_report.goal.objective)?;
        crate::goals::advance_cycle_phase(&cycle.id, "dispatching").await?;

        // Plan approval gate
        if self.config.require_plan_approval {
            // Auto-approve if no blockers and budget is fine
            let (exhausted, _) = plan.budget_remaining.is_exhausted();
            plan.approved = orient_report.blockers.is_empty() && !exhausted;
        } else {
            plan.approved = true;
        }

        // Act (only if plan approved)
        let act_result = if plan.approved {
            crate::goals::advance_cycle_phase(&cycle.id, "executing").await?;
            crate::goals::update_goal_status(goal_id, "running")
                .await
                .ok();

            let result = self
                .act(goal_id, &cycle.id, &observe_report.goal.workspace_id, &plan)
                .await?;
            Some(result)
        } else {
            // Plan not approved — skip executing, go directly to reviewing
            crate::goals::advance_cycle_phase(&cycle.id, "executing").await?;
            crate::goals::advance_cycle_phase(&cycle.id, "reviewing").await?;

            let review_verdict = self.review(&observe_report)?;
            return Ok(CycleResult {
                cycle_id: cycle.id,
                observe: observe_report,
                orient: orient_report,
                plan,
                act_result: None,
                review: review_verdict,
            });
        };

        // Review
        crate::goals::advance_cycle_phase(&cycle.id, "reviewing").await?;
        let review_verdict = self.review(&observe_report)?;

        // Complete or rework cycle
        if review_verdict.accepted {
            crate::goals::advance_cycle_phase(&cycle.id, "completed").await?;
            crate::goals::update_goal_status(goal_id, "accepted").await?;
        } else if review_verdict.rework_required {
            crate::goals::advance_cycle_phase(&cycle.id, "completed").await?;
            crate::goals::update_goal_status(goal_id, "rework_required").await?;
        } else {
            crate::goals::advance_cycle_phase(&cycle.id, "completed").await?;
        }

        Ok(CycleResult {
            cycle_id: cycle.id,
            observe: observe_report,
            orient: orient_report,
            plan,
            act_result,
            review: review_verdict,
        })
    }

    /// Advance a goal by at most one production-ready step.
    ///
    /// Unlike `run_cycle`, this method does not assume tasks finish immediately.
    /// It progresses the goal until it reaches an external wait point
    /// (queued/running tasks, blocked review, or pending plan approval).
    pub async fn tick_goal(&self, goal_id: &str) -> Result<()> {
        let mut report = self.observe(goal_id).await?;
        let mut goal = report.goal.clone();

        if matches!(
            goal.status.as_str(),
            "draft" | "blocked" | "failed" | "cancelled" | "accepted" | "archived"
        ) {
            return Ok(());
        }

        if goal.status == "rework_required" {
            goal = crate::goals::update_goal_status(goal_id, "planning").await?;
            report.goal = goal.clone();
        }

        let cycle = match report.current_cycle.clone() {
            Some(cycle) => cycle,
            None if goal.status == "planning" => {
                let cycle_no = crate::goals::list_cycles_by_goal(goal_id).await?.len() as i64 + 1;
                crate::goals::create_cycle(goal_id, cycle_no).await?
            }
            None => return Ok(()),
        };

        match cycle.status.as_str() {
            "observing" => {
                crate::goals::advance_cycle_phase(&cycle.id, "orienting").await?;
            }
            "orienting" => {
                crate::goals::advance_cycle_phase(&cycle.id, "deciding").await?;
            }
            "deciding" => {
                if self.config.require_plan_approval && goal.status == "planning" {
                    crate::goals::update_goal_status(goal_id, "awaiting_plan_approval").await?;
                }
                crate::goals::advance_cycle_phase(&cycle.id, "dispatching").await?;
            }
            "dispatching" => {
                let orient_report = self.orient(&report)?;
                let budget = budget_for_goal(&report);
                let mut plan = decide::decide(&orient_report, &budget, &report.goal.objective)?;
                apply_plan_approval(&mut plan, &orient_report, self.config.require_plan_approval);

                // If plan approval is required and not yet given, park in awaiting state.
                if self.config.require_plan_approval {
                    if goal.status == "planning" {
                        crate::goals::update_goal_status(goal_id, "awaiting_plan_approval").await?;
                        goal.status = "awaiting_plan_approval".to_string();
                    }
                    if goal.status == "awaiting_plan_approval" {
                        ensure_plan_collaboration_team(
                            goal_id,
                            &cycle.id,
                            &report.goal.workspace_id,
                            &plan.write_scope,
                        )
                        .await?;
                        return Ok(());
                    }
                }

                if !plan.approved {
                    return Ok(());
                }

                // Transition to running and execute the plan.
                if matches!(goal.status.as_str(), "planning" | "awaiting_plan_approval") {
                    crate::goals::update_goal_status(goal_id, "running").await?;
                }

                crate::goals::advance_cycle_phase(&cycle.id, "executing").await?;
                self.act(goal_id, &cycle.id, &report.goal.workspace_id, &plan)
                    .await?;
            }
            "executing" => {
                self.tick_executing(goal_id, &cycle.id).await?;
            }
            "reviewing" => {
                self.tick_reviewing(goal_id, &cycle.id).await?;
            }
            "summarizing" => {
                crate::goals::advance_cycle_phase(&cycle.id, "completed").await?;
            }
            "completed" | "failed" | "blocked" | "cancelled" => {}
            other => {
                bail!("unknown cycle status: {other}");
            }
        }

        Ok(())
    }

    // ── OODA phase wrappers ──────────────────────────────────────────────

    /// Observe: read all relevant state for the goal.
    pub async fn observe(&self, goal_id: &str) -> Result<ObserveReport> {
        observe::observe(goal_id).await
    }

    /// Orient: analyze the observe report.
    pub fn orient(&self, report: &ObserveReport) -> Result<OrientReport> {
        orient::orient(report)
    }

    /// Decide: generate a dispatch plan.
    pub fn decide(&self, orient_report: &OrientReport, objective: &str) -> Result<DispatchPlan> {
        decide::decide(orient_report, &self.config.budget, objective)
    }

    /// Act: create tasks from the plan.
    pub async fn act(
        &self,
        goal_id: &str,
        cycle_id: &str,
        workspace_id: &str,
        plan: &DispatchPlan,
    ) -> Result<ActResult> {
        act::act(goal_id, cycle_id, workspace_id, plan).await
    }

    /// Review: collect verdicts from the current cycle.
    pub fn review(&self, report: &ObserveReport) -> Result<ReviewVerdict> {
        review::review(report)
    }

    async fn tick_executing(&self, goal_id: &str, cycle_id: &str) -> Result<()> {
        let tasks = crate::goal_tasks::list_tasks_by_cycle(cycle_id).await?;

        for task in tasks.iter().filter(|task| task.status == "review_ready") {
            crate::goal_tasks::accept_review_ready_task(&task.id).await?;
        }

        let tasks = crate::goal_tasks::list_tasks_by_cycle(cycle_id).await?;
        let has_progressing_work = tasks.iter().any(|task| {
            matches!(
                task.status.as_str(),
                "proposed" | "queued" | "claimed" | "running"
            )
        });
        let has_blocked_work = tasks.iter().any(|task| {
            matches!(
                task.status.as_str(),
                "blocked" | "awaiting_permission" | "awaiting_input"
            )
        });

        if has_progressing_work {
            return Ok(());
        }

        if has_blocked_work {
            let goal = crate::goals::get_goal(goal_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;
            if goal.status == "running" {
                crate::goals::update_goal_status(goal_id, "blocked").await?;
            }
            return Ok(());
        }

        let goal = crate::goals::get_goal(goal_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;
        if goal.status == "running" {
            crate::goals::update_goal_status(goal_id, "awaiting_review").await?;
        }

        sync_team_lifecycle(goal_id, cycle_id, AgentTeamLifecycle::AwaitingReview).await?;
        crate::goals::advance_cycle_phase(cycle_id, "reviewing").await?;
        Ok(())
    }

    async fn tick_reviewing(&self, goal_id: &str, cycle_id: &str) -> Result<()> {
        let cycle_tasks = crate::goal_tasks::list_tasks_by_cycle(cycle_id).await?;
        let goal = crate::goals::get_goal(goal_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;

        if cycle_tasks.iter().any(|task| task.status == "blocked") {
            if goal.status == "awaiting_review" || goal.status == "running" {
                crate::goals::update_goal_status(goal_id, "blocked").await?;
            }
            return Ok(());
        }

        if goal.status == "running" {
            crate::goals::update_goal_status(goal_id, "awaiting_review").await?;
        }

        // Only auto-verdict when ALL tasks have reached a terminal state.
        // Non-terminal tasks (queued/claimed/running) mean execution is still in progress.
        let has_pending = cycle_tasks.iter().any(|t| {
            matches!(
                t.status.as_str(),
                "proposed"
                    | "queued"
                    | "claimed"
                    | "running"
                    | "awaiting_permission"
                    | "awaiting_input"
            )
        });
        if has_pending {
            return Ok(());
        }

        // All tasks are terminal — decide verdict.
        let has_failure = cycle_tasks
            .iter()
            .any(|t| matches!(t.status.as_str(), "rework_required" | "failed" | "blocked"));
        if !has_failure {
            let _ = crate::goal_orchestrator::apply_goal_review_verdict(goal_id, true).await;
        }
        // If there are failures, leave in awaiting_review for manual action.

        Ok(())
    }
}

fn budget_for_goal(report: &ObserveReport) -> Budget {
    let mut budget = report
        .goal
        .budget_json
        .clone()
        .and_then(|value| serde_json::from_value::<Budget>(value).ok())
        .unwrap_or_default();
    budget.cycles_used = report
        .current_cycle
        .as_ref()
        .map(|cycle| cycle.cycle_no)
        .unwrap_or_default();
    budget.agent_runs_used = report.active_tasks.len() as i64;
    budget
}

fn apply_plan_approval(
    plan: &mut DispatchPlan,
    orient_report: &OrientReport,
    require_plan_approval: bool,
) {
    if require_plan_approval {
        let (exhausted, _) = plan.budget_remaining.is_exhausted();
        plan.approved = orient_report.blockers.is_empty() && !exhausted;
    } else {
        plan.approved = true;
    }
}

async fn sync_team_lifecycle(
    goal_id: &str,
    cycle_id: &str,
    target: AgentTeamLifecycle,
) -> Result<()> {
    let team_id = format!("team-{cycle_id}");
    let team = match agent_teams::get_team(&team_id).await {
        Ok(team) => team,
        Err(_) => return Ok(()),
    };
    if team.lifecycle == target {
        return Ok(());
    }
    let previous = team.lifecycle.clone();
    agent_teams::transition_team_lifecycle(&team_id, target.clone()).await?;
    AgentTeamAdapter::on_lifecycle_change(&team_id, &previous, &target, goal_id, cycle_id).await?;
    Ok(())
}

async fn ensure_plan_collaboration_team(
    goal_id: &str,
    cycle_id: &str,
    workspace_id: &str,
    write_scope: &[String],
) -> Result<()> {
    let team_id = format!("team-{cycle_id}");
    let existing = agent_teams::get_team(&team_id).await.ok();
    let (team, created) = match existing {
        Some(team) => (team, false),
        None => (
            agent_teams::create_team(agent_teams::CreateAgentTeamInput {
                id: Some(team_id.clone()),
                name: format!("Goal {goal_id} / Cycle {}", cycle_id),
                workspace_id: Some(workspace_id.to_string()),
                write_scope: write_scope.to_vec(),
                metadata: Some(serde_json::json!({
                    "goal_id": goal_id,
                    "cycle_id": cycle_id,
                })),
            })
            .await?,
            true,
        ),
    };

    if created {
        AgentTeamAdapter::bind_to_goal(&team.id, goal_id, cycle_id).await?;
    }

    if team.lifecycle == AgentTeamLifecycle::Draft {
        let previous = team.lifecycle.clone();
        agent_teams::transition_team_lifecycle(&team.id, AgentTeamLifecycle::Planning).await?;
        AgentTeamAdapter::on_lifecycle_change(
            &team.id,
            &previous,
            &AgentTeamLifecycle::Planning,
            goal_id,
            cycle_id,
        )
        .await?;
    }
    let current = agent_teams::get_team(&team.id).await?;
    if current.lifecycle == AgentTeamLifecycle::Planning {
        let previous = current.lifecycle.clone();
        agent_teams::transition_team_lifecycle(&team.id, AgentTeamLifecycle::AwaitingPlanApproval)
            .await?;
        AgentTeamAdapter::on_lifecycle_change(
            &team.id,
            &previous,
            &AgentTeamLifecycle::AwaitingPlanApproval,
            goal_id,
            cycle_id,
        )
        .await?;
    }

    Ok(())
}

pub async fn approve_goal_plan(goal_id: &str) -> Result<()> {
    let goal = crate::goals::get_goal(goal_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;
    if goal.status != "awaiting_plan_approval" {
        bail!(
            "can only approve a goal plan awaiting approval, current status: {}",
            goal.status
        );
    }
    crate::goals::update_goal_status(goal_id, "running").await?;
    Ok(())
}

pub async fn reject_goal_plan(goal_id: &str) -> Result<()> {
    let goal = crate::goals::get_goal(goal_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;
    if goal.status != "awaiting_plan_approval" {
        bail!(
            "can only reject a goal plan awaiting approval, current status: {}",
            goal.status
        );
    }
    crate::goals::update_goal_status(goal_id, "rework_required").await?;
    Ok(())
}

pub async fn apply_goal_review_verdict(goal_id: &str, accepted: bool) -> Result<()> {
    let goal = crate::goals::get_goal(goal_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("goal not found: {goal_id}"))?;
    if goal.status != "awaiting_review" {
        bail!(
            "can only review a goal awaiting review, current status: {}",
            goal.status
        );
    }
    let cycle_id = goal
        .current_cycle_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("goal {goal_id} has no active cycle"))?;
    crate::goals::advance_cycle_phase(&cycle_id, "summarizing").await?;
    crate::goals::advance_cycle_phase(&cycle_id, "completed").await?;
    let team_id = format!("team-{cycle_id}");
    if accepted {
        if let Ok(team) = agent_teams::get_team(&team_id).await {
            if team.lifecycle == AgentTeamLifecycle::AwaitingReview {
                let _ =
                    agent_teams::transition_team_lifecycle(&team_id, AgentTeamLifecycle::Accepted)
                        .await;
            }
        }
        crate::goals::update_goal_status(goal_id, "accepted").await?;
    } else {
        if let Ok(team) = agent_teams::get_team(&team_id).await {
            if team.lifecycle == AgentTeamLifecycle::AwaitingReview {
                let _ = agent_teams::transition_team_lifecycle(
                    &team_id,
                    AgentTeamLifecycle::ReworkRequired,
                )
                .await;
            }
        }
        crate::goals::update_goal_status(goal_id, "rework_required").await?;
    }
    Ok(())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn start_transitions_draft_to_planning() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-1",
            "Test Goal",
            "test objective",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");

        let orch = GoalOrchestrator::new(OrchestratorConfig {
            workspace_id: "ws-1".to_string(),
            require_plan_approval: false,
            ..Default::default()
        });

        orch.start(&goal.id).await.expect("start");

        let updated = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();
        assert_eq!(updated.status, "planning");
    }

    #[tokio::test]
    async fn start_non_draft_fails() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal("ws-1", "Test", "obj", "normal", "test", None, None)
            .await
            .expect("create");

        crate::goals::update_goal_status(&goal.id, "planning")
            .await
            .expect("transition");

        let orch = GoalOrchestrator::new(OrchestratorConfig::default());
        let result = orch.start(&goal.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn tick_goal_advances_to_executing_and_queues_work() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-1",
            "Tick Goal",
            "ship something",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");

        crate::goals::update_goal_status(&goal.id, "planning")
            .await
            .expect("to planning");

        let orch = GoalOrchestrator::new(OrchestratorConfig {
            workspace_id: "ws-1".to_string(),
            require_plan_approval: false,
            ..Default::default()
        });

        for _ in 0..4 {
            orch.tick_goal(&goal.id).await.expect("tick goal");
        }

        let goal = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();
        let cycle = crate::goals::get_cycle(goal.current_cycle_id.as_deref().unwrap())
            .await
            .unwrap()
            .unwrap();
        let tasks = crate::goal_tasks::list_tasks_by_cycle(&cycle.id)
            .await
            .expect("list cycle tasks");

        assert_eq!(goal.status, "running");
        assert_eq!(cycle.status, "executing");
        assert!(!tasks.is_empty());
        assert!(tasks.iter().all(|task| task.status == "queued"));
    }

    #[tokio::test]
    async fn tick_goal_stops_at_plan_approval_until_user_approves() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-approval",
            "Approval Gate",
            "wait for explicit plan approval",
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

        let orch = GoalOrchestrator::new(OrchestratorConfig {
            workspace_id: "ws-approval".to_string(),
            require_plan_approval: true,
            ..Default::default()
        });

        for _ in 0..4 {
            orch.tick_goal(&goal.id).await.expect("tick goal");
        }

        let waiting_goal = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();
        let waiting_cycle =
            crate::goals::get_cycle(waiting_goal.current_cycle_id.as_deref().unwrap())
                .await
                .unwrap()
                .unwrap();
        let waiting_tasks = crate::goal_tasks::list_tasks_by_cycle(&waiting_cycle.id)
            .await
            .expect("list cycle tasks");
        let waiting_team = crate::agent_teams::get_team(&format!("team-{}", waiting_cycle.id))
            .await
            .expect("plan collaboration team");
        let waiting_members = crate::agent_teams::list_members(&waiting_team.id)
            .await
            .expect("members");

        assert_eq!(waiting_goal.status, "awaiting_plan_approval");
        assert_eq!(waiting_cycle.status, "dispatching");
        assert!(waiting_tasks.is_empty());
        assert_eq!(waiting_team.lifecycle.as_str(), "awaiting_plan_approval");
        assert!(waiting_members.is_empty());

        approve_goal_plan(&goal.id).await.expect("approve plan");
        orch.tick_goal(&goal.id).await.expect("tick after approval");

        let running_goal = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();
        let running_cycle =
            crate::goals::get_cycle(running_goal.current_cycle_id.as_deref().unwrap())
                .await
                .unwrap()
                .unwrap();
        let running_tasks = crate::goal_tasks::list_tasks_by_cycle(&running_cycle.id)
            .await
            .expect("list cycle tasks");
        let running_team = crate::agent_teams::get_team(&format!("team-{}", running_cycle.id))
            .await
            .expect("execution team");

        assert_eq!(running_goal.status, "running");
        assert_eq!(running_cycle.status, "executing");
        assert!(!running_tasks.is_empty());
        assert_eq!(running_team.lifecycle.as_str(), "executing");
    }

    #[tokio::test]
    async fn tick_goal_auto_accepts_after_successful_review() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-review",
            "Review Gate",
            "wait for explicit review verdict",
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

        let orch = GoalOrchestrator::new(OrchestratorConfig {
            workspace_id: "ws-review".to_string(),
            require_plan_approval: true,
            ..Default::default()
        });

        for _ in 0..4 {
            orch.tick_goal(&goal.id).await.expect("tick to approval");
        }
        approve_goal_plan(&goal.id).await.expect("approve plan");
        orch.tick_goal(&goal.id).await.expect("tick to execute");

        let running_goal = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();
        let cycle_id = running_goal.current_cycle_id.clone().unwrap();
        let task = crate::goal_tasks::list_tasks_by_cycle(&cycle_id)
            .await
            .expect("tasks")
            .into_iter()
            .next()
            .expect("one task");
        crate::goal_tasks::claim_task(&task.id, "agent-review", 300)
            .await
            .expect("claim");
        crate::goal_tasks::start_task(&task.id)
            .await
            .expect("start");
        crate::goal_tasks::complete_task(&task.id, "result/ref")
            .await
            .expect("complete");

        orch.tick_goal(&goal.id).await.expect("tick to review");
        orch.tick_goal(&goal.id).await.expect("review wait tick");

        let accepted_goal = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();
        let accepted_cycle = crate::goals::get_cycle(&cycle_id).await.unwrap().unwrap();
        assert_eq!(accepted_goal.status, "accepted");
        assert_eq!(accepted_cycle.status, "completed");
    }

    #[tokio::test]
    async fn pause_and_resume_cycle() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal("ws-1", "Test", "obj", "normal", "test", None, None)
            .await
            .expect("create");

        crate::goals::update_goal_status(&goal.id, "planning")
            .await
            .expect("to planning");
        crate::goals::update_goal_status(&goal.id, "awaiting_plan_approval")
            .await
            .expect("to awaiting");
        crate::goals::update_goal_status(&goal.id, "running")
            .await
            .expect("to running");

        let orch = GoalOrchestrator::new(OrchestratorConfig::default());

        orch.pause(&goal.id).await.expect("pause");
        let g = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();
        assert_eq!(g.status, "blocked");

        orch.resume(&goal.id).await.expect("resume");
        let g = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();
        assert_eq!(g.status, "running");
    }

    #[tokio::test]
    async fn tick_goal_moves_running_goal_to_blocked_when_only_blocked_tasks_remain() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-blocked",
            "Blocked Goal",
            "wait for approval",
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

        let orch = GoalOrchestrator::new(OrchestratorConfig {
            workspace_id: "ws-blocked".to_string(),
            require_plan_approval: false,
            ..Default::default()
        });

        for _ in 0..4 {
            orch.tick_goal(&goal.id).await.expect("tick to executing");
        }

        let running_goal = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();
        let cycle_id = running_goal.current_cycle_id.clone().unwrap();
        let task = crate::goal_tasks::list_tasks_by_cycle(&cycle_id)
            .await
            .expect("tasks")
            .into_iter()
            .next()
            .expect("one task");

        crate::goal_tasks::claim_task(&task.id, "agent-blocked", 300)
            .await
            .expect("claim");
        crate::goal_tasks::start_task(&task.id)
            .await
            .expect("start");
        crate::goal_tasks::set_task_result_ref_blocked(
            &task.id,
            "chat:message-blocked",
            "waiting for approval",
        )
        .await
        .expect("block task");

        orch.tick_goal(&goal.id).await.expect("tick blocked");

        let blocked_goal = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();
        assert_eq!(blocked_goal.status, "blocked");
    }

    #[tokio::test]
    async fn resume_requeues_blocked_tasks_in_current_cycle() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-resume",
            "Resume Goal",
            "retry blocked work",
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

        let orch = GoalOrchestrator::new(OrchestratorConfig {
            workspace_id: "ws-resume".to_string(),
            require_plan_approval: false,
            ..Default::default()
        });

        for _ in 0..4 {
            orch.tick_goal(&goal.id).await.expect("tick to executing");
        }

        let running_goal = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();
        let cycle_id = running_goal.current_cycle_id.clone().unwrap();
        let task = crate::goal_tasks::list_tasks_by_cycle(&cycle_id)
            .await
            .expect("tasks")
            .into_iter()
            .next()
            .expect("one task");

        crate::goal_tasks::claim_task(&task.id, "agent-resume", 300)
            .await
            .expect("claim");
        crate::goal_tasks::start_task(&task.id)
            .await
            .expect("start");
        crate::goal_tasks::set_task_result_ref_blocked(
            &task.id,
            "chat:message-blocked",
            "waiting for approval",
        )
        .await
        .expect("block task");

        orch.tick_goal(&goal.id).await.expect("tick blocked");
        orch.resume(&goal.id).await.expect("resume goal");

        let resumed_task = crate::goal_tasks::get_task(&task.id)
            .await
            .expect("get task")
            .expect("task");
        let resumed_goal = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();

        assert_eq!(resumed_goal.status, "running");
        assert_eq!(resumed_task.status, "queued");
        assert!(resumed_task.error.is_none());
        assert!(resumed_task.result_ref.is_none());
    }

    #[tokio::test]
    async fn cancel_goal() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal("ws-1", "Test", "obj", "normal", "test", None, None)
            .await
            .expect("create");

        // Move to running first (draft → planning → awaiting → running)
        crate::goals::update_goal_status(&goal.id, "planning")
            .await
            .unwrap();
        crate::goals::update_goal_status(&goal.id, "awaiting_plan_approval")
            .await
            .unwrap();
        crate::goals::update_goal_status(&goal.id, "running")
            .await
            .unwrap();

        let orch = GoalOrchestrator::new(OrchestratorConfig::default());
        orch.cancel(&goal.id).await.expect("cancel");

        let g = crate::goals::get_goal(&goal.id).await.unwrap().unwrap();
        assert_eq!(g.status, "cancelled");
    }

    #[tokio::test]
    async fn run_cycle_full_ooda() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-1",
            "Test Cycle",
            "run a full OODA cycle",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create goal");

        // Transition to running
        crate::goals::update_goal_status(&goal.id, "planning")
            .await
            .expect("to planning");
        crate::goals::update_goal_status(&goal.id, "awaiting_plan_approval")
            .await
            .expect("to awaiting");
        crate::goals::update_goal_status(&goal.id, "running")
            .await
            .expect("to running");

        let orch = GoalOrchestrator::new(OrchestratorConfig {
            workspace_id: "ws-1".to_string(),
            require_plan_approval: false,
            ..Default::default()
        });

        let result = orch.run_cycle(&goal.id).await.expect("run_cycle");

        // Verify cycle was created
        assert!(!result.cycle_id.is_empty());

        // Verify OODA phases produced output
        assert_eq!(result.observe.goal.id, goal.id);
        assert!(!result.orient.goal_gap.is_empty());
        // Plan should have been generated (even if empty for an initial goal)
        // Act result depends on plan
        // Review should have a verdict
        assert!(!result.review.notes.is_empty());
    }

    #[tokio::test]
    async fn run_cycle_with_plan_approval_gate() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-1",
            "Test Gate",
            "test plan approval",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create");

        crate::goals::update_goal_status(&goal.id, "planning")
            .await
            .unwrap();
        crate::goals::update_goal_status(&goal.id, "awaiting_plan_approval")
            .await
            .unwrap();
        crate::goals::update_goal_status(&goal.id, "running")
            .await
            .unwrap();

        let orch = GoalOrchestrator::new(OrchestratorConfig {
            workspace_id: "ws-1".to_string(),
            require_plan_approval: true,
            ..Default::default()
        });

        let result = orch.run_cycle(&goal.id).await.expect("run_cycle");

        // With no blockers, plan should be auto-approved
        assert!(result.plan.approved);
    }

    #[tokio::test]
    async fn run_cycle_budget_exhausted_blocks_act() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal(
            "ws-1",
            "Test Budget",
            "test budget exhaustion",
            "normal",
            "test",
            None,
            None,
        )
        .await
        .expect("create");

        crate::goals::update_goal_status(&goal.id, "planning")
            .await
            .unwrap();
        crate::goals::update_goal_status(&goal.id, "awaiting_plan_approval")
            .await
            .unwrap();
        crate::goals::update_goal_status(&goal.id, "running")
            .await
            .unwrap();

        let orch = GoalOrchestrator::new(OrchestratorConfig {
            workspace_id: "ws-1".to_string(),
            budget: Budget {
                max_cycles: Some(1),
                cycles_used: 1,
                ..Default::default()
            },
            require_plan_approval: true,
        });

        let result = orch.run_cycle(&goal.id).await.expect("run_cycle");

        // Budget exhausted → plan not approved → no act result
        assert!(!result.plan.approved);
        assert!(result.act_result.is_none());
    }

    #[tokio::test]
    async fn cancel_accepted_goal_fails() {
        let _root = TestRoot::new();
        let goal = crate::goals::create_goal("ws-1", "Test", "obj", "normal", "test", None, None)
            .await
            .expect("create");

        // Move to accepted
        crate::goals::update_goal_status(&goal.id, "planning")
            .await
            .unwrap();
        crate::goals::update_goal_status(&goal.id, "awaiting_plan_approval")
            .await
            .unwrap();
        crate::goals::update_goal_status(&goal.id, "running")
            .await
            .unwrap();
        crate::goals::update_goal_status(&goal.id, "awaiting_review")
            .await
            .unwrap();
        crate::goals::update_goal_status(&goal.id, "accepted")
            .await
            .unwrap();

        let orch = GoalOrchestrator::new(OrchestratorConfig::default());
        let result = orch.cancel(&goal.id).await;
        assert!(result.is_err());
    }

    #[test]
    fn budget_default_limits() {
        let budget = Budget::default();
        assert_eq!(budget.max_cycles, Some(10));
        assert_eq!(budget.max_wall_time_secs, Some(3600));
        assert!(!budget.is_exhausted().0);
    }
}
