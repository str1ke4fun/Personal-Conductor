//! AgentTeamAdapter bridges AgentTeam lifecycle and mailbox into the Goal/Event runtime.
//!
//! - `bind_to_goal` links a team to a goal and cycle.
//! - `on_lifecycle_change` emits goal.cycle.* / task.* audit events when the team transitions.
//! - `bridge_mailbox_to_agent_message` converts an `AgentMailboxMessage` into the unified
//!   `AgentMessage` format so downstream consumers have a single message type.

use crate::agent_messages;
use crate::agent_teams::{self, AgentMailboxMessage, AgentMessageKind, AgentTeamLifecycle};
use crate::events;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A binding record that associates an AgentTeam with a GoalRun + GoalCycle.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TeamGoalBinding {
    pub team_id: String,
    pub goal_id: String,
    pub cycle_id: String,
}

/// Adapter that bridges AgentTeam lifecycle/mailbox into the Goal runtime.
pub struct AgentTeamAdapter;

impl AgentTeamAdapter {
    /// Bind an AgentTeam to a GoalRun and GoalCycle.
    ///
    /// Validates that the team exists, then emits a `goal.cycle.team_bound` event.
    pub async fn bind_to_goal(
        team_id: &str,
        goal_id: &str,
        cycle_id: &str,
    ) -> anyhow::Result<TeamGoalBinding> {
        // Verify the team exists
        let team = agent_teams::get_team(team_id)
            .await
            .with_context(|| format!("bind_to_goal: team not found: {team_id}"))?;

        let binding = TeamGoalBinding {
            team_id: team.id.clone(),
            goal_id: goal_id.to_string(),
            cycle_id: cycle_id.to_string(),
        };

        // Emit bind event
        let payload = json!({
            "team_id": binding.team_id,
            "goal_id": binding.goal_id,
            "cycle_id": binding.cycle_id,
        });
        events::append("goal", "goal.cycle.team_bound", &payload).await?;

        Ok(binding)
    }

    /// Called when an AgentTeam undergoes a lifecycle transition.
    ///
    /// Maps the lifecycle state to the appropriate goal.cycle.* or task.* event and emits it.
    ///
    /// Lifecycle → event mapping:
    ///   Planning           → goal.cycle.planning_started
    ///   AwaitingPlanApproval → goal.cycle.plan_submitted
    ///   Executing          → goal.cycle.executing
    ///   AwaitingReview     → goal.cycle.review_started
    ///   Accepted           → goal.cycle.completed
    ///   ReworkRequired     → goal.cycle.rework_required
    ///   Archived           → goal.cycle.archived
    ///   Draft              → (no event, initial state)
    pub async fn on_lifecycle_change(
        team_id: &str,
        from: &AgentTeamLifecycle,
        to: &AgentTeamLifecycle,
        goal_id: &str,
        cycle_id: &str,
    ) -> anyhow::Result<()> {
        let event_kind = match to {
            AgentTeamLifecycle::Draft => return Ok(()),
            AgentTeamLifecycle::Planning => "goal.cycle.planning_started",
            AgentTeamLifecycle::AwaitingPlanApproval => "goal.cycle.plan_submitted",
            AgentTeamLifecycle::Executing => "goal.cycle.executing",
            AgentTeamLifecycle::AwaitingReview => "goal.cycle.review_started",
            AgentTeamLifecycle::Accepted => "goal.cycle.completed",
            AgentTeamLifecycle::ReworkRequired => "goal.cycle.rework_required",
            AgentTeamLifecycle::Archived => "goal.cycle.archived",
        };

        let payload = json!({
            "team_id": team_id,
            "goal_id": goal_id,
            "cycle_id": cycle_id,
            "from": from.as_str(),
            "to": to.as_str(),
        });
        events::append("goal", event_kind, &payload).await?;

        emit_mailbox_request(team_id, to, goal_id, cycle_id).await?;

        // When entering "executing", also emit a task.started event
        if *to == AgentTeamLifecycle::Executing {
            let task_payload = json!({
                "team_id": team_id,
                "goal_id": goal_id,
                "cycle_id": cycle_id,
                "status": "started",
            });
            events::append("task", "task.started", &task_payload).await?;
        }

        // When accepted, also emit task.completed
        if *to == AgentTeamLifecycle::Accepted {
            let task_payload = json!({
                "team_id": team_id,
                "goal_id": goal_id,
                "cycle_id": cycle_id,
                "status": "completed",
            });
            events::append("task", "task.completed", &task_payload).await?;
        }

        Ok(())
    }

    /// Bridge an `AgentMailboxMessage` (team-scoped) into the unified `AgentMessage` table.
    ///
    /// This allows downstream consumers (goal runtime, event bus) to read team mailbox
    /// traffic through the standard `agent_messages` API.
    pub async fn bridge_mailbox_to_agent_message(
        workspace_id: &str,
        goal_id: Option<&str>,
        cycle_id: Option<&str>,
        mailbox_msg: &AgentMailboxMessage,
    ) -> anyhow::Result<agent_messages::AgentMessage> {
        let topic = format!("team.{}", mailbox_msg.team_id);
        let kind = mailbox_msg.kind.as_str();

        agent_messages::post_message(
            workspace_id,
            goal_id,
            cycle_id,
            None, // task_id
            &mailbox_msg.sender_agent_id,
            mailbox_msg.recipient_agent_id.as_deref(),
            &topic,
            kind,
            &mailbox_msg.content,
            mailbox_msg.metadata_json.clone(),
        )
        .await
        .with_context(|| "bridge_mailbox_to_agent_message: post_message failed")
    }
}

async fn emit_mailbox_request(
    team_id: &str,
    lifecycle: &AgentTeamLifecycle,
    goal_id: &str,
    cycle_id: &str,
) -> anyhow::Result<()> {
    let team = match agent_teams::get_team(team_id).await {
        Ok(team) => team,
        Err(_) => return Ok(()),
    };

    let (kind, content, metadata) = match lifecycle {
        AgentTeamLifecycle::AwaitingPlanApproval => (
            AgentMessageKind::PlanApprovalRequest,
            "Plan ready for approval. Review write scope and approve execution.".to_string(),
            json!({
                "goal_id": goal_id,
                "cycle_id": cycle_id,
                "write_scope": team.write_scope,
                "lifecycle": lifecycle.as_str(),
            }),
        ),
        AgentTeamLifecycle::AwaitingReview => (
            AgentMessageKind::ReviewVerdictRequest,
            "Execution finished. Review the result and submit a verdict.".to_string(),
            json!({
                "goal_id": goal_id,
                "cycle_id": cycle_id,
                "write_scope": team.write_scope,
                "lifecycle": lifecycle.as_str(),
            }),
        ),
        _ => return Ok(()),
    };

    let _ = agent_teams::append_system_message(team_id, kind, &content, Some(metadata)).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_teams::{
        self, AgentTeamLifecycle, CreateAgentTeamInput, SendAgentMessageInput,
    };
    use crate::test_support::TestRoot;

    // ── Test 1: bind_to_goal succeeds and emits event ───────────────────────

    #[tokio::test]
    async fn bind_to_goal_creates_binding_and_emits_event() {
        let _root = TestRoot::new();

        // Create a team first
        let team = agent_teams::create_team(CreateAgentTeamInput {
            id: Some("team-bind-test".to_string()),
            name: "Bind Test Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");

        // Bind to a goal+cycle
        let binding = AgentTeamAdapter::bind_to_goal(&team.id, "goal-001", "cycle-001")
            .await
            .expect("bind_to_goal");

        assert_eq!(binding.team_id, team.id);
        assert_eq!(binding.goal_id, "goal-001");
        assert_eq!(binding.cycle_id, "cycle-001");

        // Verify the event was emitted
        let events = crate::events::query_events(
            crate::events::EventFilter {
                event_type: Some("goal.cycle.team_bound".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .expect("query events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].detail["team_id"], team.id);
        assert_eq!(events[0].detail["goal_id"], "goal-001");
        assert_eq!(events[0].detail["cycle_id"], "cycle-001");
    }

    // ── Test 2: bind_to_goal fails for nonexistent team ─────────────────────

    #[tokio::test]
    async fn bind_to_goal_fails_for_nonexistent_team() {
        let _root = TestRoot::new();

        let result =
            AgentTeamAdapter::bind_to_goal("nonexistent-team", "goal-001", "cycle-001").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("team not found"));
    }

    // ── Test 3: on_lifecycle_change emits correct events ────────────────────

    #[tokio::test]
    async fn on_lifecycle_change_emits_goal_cycle_and_task_events() {
        let _root = TestRoot::new();

        let team = agent_teams::create_team(CreateAgentTeamInput {
            id: Some("team-lifecycle".to_string()),
            name: "Lifecycle Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");

        // Transition Draft -> Planning (emit goal.cycle.planning_started)
        AgentTeamAdapter::on_lifecycle_change(
            &team.id,
            &AgentTeamLifecycle::Draft,
            &AgentTeamLifecycle::Planning,
            "goal-100",
            "cycle-100",
        )
        .await
        .expect("lifecycle: planning");

        // Transition Planning -> Executing (emit goal.cycle.executing + task.started)
        AgentTeamAdapter::on_lifecycle_change(
            &team.id,
            &AgentTeamLifecycle::Planning,
            &AgentTeamLifecycle::Executing,
            "goal-100",
            "cycle-100",
        )
        .await
        .expect("lifecycle: executing");

        // Transition Executing -> Accepted (emit goal.cycle.completed + task.completed)
        AgentTeamAdapter::on_lifecycle_change(
            &team.id,
            &AgentTeamLifecycle::Executing,
            &AgentTeamLifecycle::Accepted,
            "goal-100",
            "cycle-100",
        )
        .await
        .expect("lifecycle: accepted");

        // Verify events
        let all_goal_events = crate::events::query_events(
            crate::events::EventFilter {
                source: Some("goal".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .expect("query goal events");

        let kinds: Vec<&str> = all_goal_events
            .iter()
            .map(|e| e.event_type.as_str())
            .collect();

        assert!(kinds.contains(&"goal.cycle.planning_started"));
        assert!(kinds.contains(&"goal.cycle.executing"));
        assert!(kinds.contains(&"goal.cycle.completed"));

        // Verify task.started and task.completed were emitted
        let task_events = crate::events::query_events(
            crate::events::EventFilter {
                source: Some("task".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .expect("query task events");

        let task_kinds: Vec<&str> = task_events.iter().map(|e| e.event_type.as_str()).collect();

        assert!(task_kinds.contains(&"task.started"));
        assert!(task_kinds.contains(&"task.completed"));
    }

    // ── Test 4: Draft lifecycle transition emits no event ────────────────────

    #[tokio::test]
    async fn on_lifecycle_change_draft_emits_no_event() {
        let _root = TestRoot::new();

        // Record baseline event count
        let before = crate::events::query_events(crate::events::EventFilter::default(), Some(1000))
            .await
            .expect("query before")
            .len();

        AgentTeamAdapter::on_lifecycle_change(
            "team-draft",
            &AgentTeamLifecycle::Draft,
            &AgentTeamLifecycle::Draft,
            "goal-200",
            "cycle-200",
        )
        .await
        .expect("lifecycle: draft noop");

        let after = crate::events::query_events(crate::events::EventFilter::default(), Some(1000))
            .await
            .expect("query after")
            .len();

        assert_eq!(before, after, "Draft transition should emit no events");
    }

    // ── Test 5: bridge_mailbox_to_agent_message ─────────────────────────────

    #[tokio::test]
    async fn bridge_mailbox_to_agent_message_round_trip() {
        let _root = TestRoot::new();

        // Create a team and member, send a mailbox message
        let team = agent_teams::create_team(CreateAgentTeamInput {
            id: Some("team-bridge".to_string()),
            name: "Bridge Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");

        agent_teams::add_member(agent_teams::AddAgentTeamMemberInput {
            team_id: team.id.clone(),
            agent_id: "agent-alpha".to_string(),
            ..Default::default()
        })
        .await
        .expect("add member");

        let mailbox_messages = agent_teams::send_message(SendAgentMessageInput {
            team_id: team.id.clone(),
            sender_agent_id: "conductor".to_string(),
            recipient_agent_id: Some("agent-alpha".to_string()),
            content: "Start working on the plan".to_string(),
            ..Default::default()
        })
        .await
        .expect("send message");

        assert_eq!(mailbox_messages.len(), 1);
        let mailbox_msg = &mailbox_messages[0];

        // Bridge to AgentMessage
        let agent_msg = AgentTeamAdapter::bridge_mailbox_to_agent_message(
            "ws-bridge-test",
            Some("goal-bridge"),
            Some("cycle-bridge"),
            mailbox_msg,
        )
        .await
        .expect("bridge");

        assert_eq!(agent_msg.workspace_id, "ws-bridge-test");
        assert_eq!(agent_msg.goal_id.as_deref(), Some("goal-bridge"));
        assert_eq!(agent_msg.cycle_id.as_deref(), Some("cycle-bridge"));
        assert_eq!(agent_msg.sender_id, "conductor");
        assert_eq!(agent_msg.recipient_id.as_deref(), Some("agent-alpha"));
        assert_eq!(agent_msg.topic, format!("team.{}", team.id));
        assert_eq!(agent_msg.kind, "message");
        assert_eq!(agent_msg.content, "Start working on the plan");

        // Verify it can be fetched via standard agent_messages API
        let fetched = crate::agent_messages::get_message(&agent_msg.id)
            .await
            .expect("get_message")
            .expect("message should exist");
        assert_eq!(fetched.content, "Start working on the plan");
        assert_eq!(fetched.topic, format!("team.{}", team.id));
    }

    // ── Test 6: lifecycle rework event ──────────────────────────────────────

    #[tokio::test]
    async fn on_lifecycle_change_rework_emits_correct_event() {
        let _root = TestRoot::new();

        AgentTeamAdapter::on_lifecycle_change(
            "team-rework",
            &AgentTeamLifecycle::AwaitingReview,
            &AgentTeamLifecycle::ReworkRequired,
            "goal-rework",
            "cycle-rework",
        )
        .await
        .expect("lifecycle: rework");

        let events = crate::events::query_events(
            crate::events::EventFilter {
                event_type: Some("goal.cycle.rework_required".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .expect("query events");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].detail["team_id"], "team-rework");
        assert_eq!(events[0].detail["from"], "awaiting_review");
        assert_eq!(events[0].detail["to"], "rework_required");
    }

    #[tokio::test]
    async fn awaiting_plan_approval_emits_mailbox_request_with_write_scope() {
        let _root = TestRoot::new();

        let team = agent_teams::create_team(CreateAgentTeamInput {
            id: Some("team-plan-request".to_string()),
            name: "Plan Request Team".to_string(),
            write_scope: vec!["crates/conductor-core/src/chat/send_v2.rs".to_string()],
            ..Default::default()
        })
        .await
        .expect("create team");

        AgentTeamAdapter::on_lifecycle_change(
            &team.id,
            &AgentTeamLifecycle::Planning,
            &AgentTeamLifecycle::AwaitingPlanApproval,
            "goal-plan-request",
            "cycle-plan-request",
        )
        .await
        .expect("emit plan approval request");

        let mailbox = agent_teams::list_mailbox(agent_teams::AgentMailboxFilter {
            team_id: team.id.clone(),
            include_read: true,
            ..Default::default()
        })
        .await
        .expect("list mailbox");

        assert_eq!(mailbox.len(), 1);
        assert_eq!(mailbox[0].kind.as_str(), "plan_approval_request");
        assert!(
            mailbox[0].content.contains("Plan ready for approval"),
            "unexpected plan request content: {}",
            mailbox[0].content
        );
        assert_eq!(
            mailbox[0]
                .metadata_json
                .as_ref()
                .and_then(|value| value.get("write_scope")),
            Some(&json!(["crates/conductor-core/src/chat/send_v2.rs"]))
        );
    }

    #[tokio::test]
    async fn awaiting_review_emits_review_verdict_request() {
        let _root = TestRoot::new();

        let team = agent_teams::create_team(CreateAgentTeamInput {
            id: Some("team-review-request".to_string()),
            name: "Review Request Team".to_string(),
            ..Default::default()
        })
        .await
        .expect("create team");

        AgentTeamAdapter::on_lifecycle_change(
            &team.id,
            &AgentTeamLifecycle::Executing,
            &AgentTeamLifecycle::AwaitingReview,
            "goal-review-request",
            "cycle-review-request",
        )
        .await
        .expect("emit review verdict request");

        let mailbox = agent_teams::list_mailbox(agent_teams::AgentMailboxFilter {
            team_id: team.id.clone(),
            include_read: true,
            ..Default::default()
        })
        .await
        .expect("list mailbox");

        assert_eq!(mailbox.len(), 1);
        assert_eq!(mailbox[0].kind.as_str(), "review_verdict_request");
        assert!(
            mailbox[0]
                .content
                .contains("Execution finished. Review the result"),
            "unexpected review request content: {}",
            mailbox[0].content
        );
    }
}
