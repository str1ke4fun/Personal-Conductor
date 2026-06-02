use crate::{
    proposals::{self, Proposal, ProposalSource, ProposalStatus, RiskLevel},
    tasks::{self, Task, TaskStatus},
};
use std::path::Path;

pub async fn build_injection_for_cwd(cwd: &Path) -> anyhow::Result<String> {
    let tasks = tasks::load().await?;
    let pending = tasks
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Pending)
        .filter(|task| is_related_to_cwd(task, cwd))
        .take(3)
        .collect::<Vec<_>>();
    let approved = proposals::list_for_cwd(cwd, Some(ProposalStatus::Approved)).await?;

    let mut out = String::new();
    if !pending.is_empty() {
        out.push_str("[Conductor injection] You still have unfinished review items. Avoid piling up more work in the same direction:\n");
        for task in pending {
            out.push_str(&format!(
                "- {} ({})\n",
                task.artifact_label(),
                task.created_at.format("%H:%M")
            ));
        }
    }
    if !approved.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("[Conductor suggested next step]\n");
        for proposal in approved {
            out.push_str(&format!("- {}\n", proposal.content));
            proposals::mark_used(&proposal.id).await?;
        }
    }

    if out.chars().count() > 300 {
        out = out.chars().take(297).collect::<String>() + "...";
    }
    Ok(out)
}

fn is_related_to_cwd(task: &Task, cwd: &Path) -> bool {
    task.artifact
        .file
        .as_ref()
        .map(|file| file.starts_with(cwd))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        proposals::{Proposal, ProposalStatus},
        tasks::{add, Artifact},
        test_support::TestRoot,
    };
    use chrono::Utc;

    #[tokio::test]
    async fn builds_injection_for_related_pending_tasks() {
        let root = TestRoot::new();
        let cwd = root.path().join("work");
        tokio::fs::create_dir_all(&cwd).await.expect("mkdir work");
        add(Task {
            id: "t-20260518-001".into(),
            source: "claude".into(),
            kind: "review-doc".into(),
            artifact: Artifact {
                file: Some(cwd.join("doc.md")),
                anchor: None,
            },
            summary_ref: None,
            est_minutes: None,
            focus_hint: None,
            status: TaskStatus::Pending,
            created_at: Utc::now(),
            session_id: None,
            terminal_id: None,
            cwd: None,
            current_request: None,
            last_output_summary: None,
            last_event_at: None,
            permission_summary: None,
        })
        .await
        .expect("add task");

        let text = build_injection_for_cwd(&cwd).await.expect("injection");
        assert!(text.contains("[Conductor injection]"));
        assert!(text.contains("doc.md"));
        assert!(text.chars().count() <= 300);
    }

    #[tokio::test]
    async fn approved_proposal_is_injected_once() {
        let root = TestRoot::new();
        let cwd = root.path().join("work");
        tokio::fs::create_dir_all(&cwd).await.expect("mkdir work");
        proposals::create(Proposal {
            id: "p-20260518-001".into(),
            workspace_id: None,
            for_cwd: cwd.clone(),
            source: ProposalSource::Chat,
            title: "Test Proposal".into(),
            content: "Review doc-A before creating doc-B.".into(),
            reason: "pending backlog".into(),
            tool_id: None,
            tool_input_json: None,
            risk_level: RiskLevel::ReadOnly,
            dry_run: false,
            status: ProposalStatus::Approved,
            result_ref: None,
            agent_task_id: None,
            grant_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await
        .expect("create proposal");

        let first = build_injection_for_cwd(&cwd).await.expect("first");
        let second = build_injection_for_cwd(&cwd).await.expect("second");
        assert!(first.contains("Review doc-A"));
        assert!(!second.contains("Review doc-A"));
    }
}
