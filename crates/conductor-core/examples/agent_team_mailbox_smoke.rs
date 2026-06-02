use anyhow::Context;
use conductor_core::agent_teams::{
    self, AgentMessageKind, AgentTeamLifecycle, CreateAgentTeamInput, SendAgentMessageInput,
};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let temp = tempfile::tempdir().context("create temp conductor root")?;
    std::env::set_var("CONDUCTOR_ROOT", temp.path());

    let team = agent_teams::create_team(CreateAgentTeamInput {
        id: Some("team-mailbox-smoke".to_string()),
        name: "Mailbox Smoke Team".to_string(),
        ..Default::default()
    })
    .await
    .context("create team")?;

    agent_teams::add_member(agent_teams::AddAgentTeamMemberInput {
        team_id: team.id.clone(),
        agent_id: "executor".to_string(),
        role: "executor".to_string(),
        run_id: Some("ar-mailbox-smoke".to_string()),
        metadata: Some(json!({
            "task_id": "task-mailbox-smoke",
            "external_session_id": "session-mailbox-smoke"
        })),
        ..Default::default()
    })
    .await
    .context("add executor")?;

    agent_teams::transition_team_lifecycle(&team.id, AgentTeamLifecycle::Planning)
        .await
        .context("transition to planning")?;
    agent_teams::transition_team_lifecycle(&team.id, AgentTeamLifecycle::AwaitingPlanApproval)
        .await
        .context("transition to awaiting plan approval")?;

    let messages = agent_teams::send_message(SendAgentMessageInput {
        team_id: team.id.clone(),
        sender_agent_id: "reviewer".to_string(),
        recipient_agent_id: Some("executor".to_string()),
        kind: Some(AgentMessageKind::PlanApprovalResponse),
        content: "approved".to_string(),
        metadata: Some(json!({ "verdict": "approved" })),
        ..Default::default()
    })
    .await
    .context("send plan approval response")?;

    let snapshot = agent_teams::snapshot(&team.id, 10)
        .await
        .context("snapshot")?;

    anyhow::ensure!(
        snapshot.team.lifecycle == AgentTeamLifecycle::Executing,
        "expected executing lifecycle, got {}",
        snapshot.team.lifecycle.as_str()
    );
    anyhow::ensure!(
        messages
            .iter()
            .all(|message| message.kind == AgentMessageKind::PlanApprovalResponse),
        "expected plan approval response messages"
    );

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "team_id": team.id,
            "message_count": messages.len(),
            "team_lifecycle": snapshot.team.lifecycle.as_str(),
            "member_run_id": snapshot.members.first().and_then(|member| member.run_id.clone()),
            "member_task_id": snapshot.members.first()
                .and_then(|member| member.metadata_json.as_ref())
                .and_then(|metadata| metadata.get("task_id"))
                .and_then(|value| value.as_str()),
        }))?
    );

    Ok(())
}
