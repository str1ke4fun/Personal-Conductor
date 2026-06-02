use crate::tasks::{self, TaskStatus};
use chrono::{Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PacerAlertKind {
    PendingPileUp,
    AgentStalled,
    UserBackFromIdle,
    NoActivityHour,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PacerAlert {
    PendingPileUp {
        count: u32,
        oldest_minutes: u32,
    },
    AgentStalled {
        task_id: String,
        stalled_minutes: u32,
    },
    UserBackFromIdle {
        idle_minutes: u32,
        pending: u32,
    },
    NoActivityHour {
        hour: u8,
    },
}

impl PacerAlert {
    pub fn kind(&self) -> PacerAlertKind {
        match self {
            Self::PendingPileUp { .. } => PacerAlertKind::PendingPileUp,
            Self::AgentStalled { .. } => PacerAlertKind::AgentStalled,
            Self::UserBackFromIdle { .. } => PacerAlertKind::UserBackFromIdle,
            Self::NoActivityHour { .. } => PacerAlertKind::NoActivityHour,
        }
    }
}

pub struct PacerHandle {
    stop: Arc<AtomicBool>,
}

impl PacerHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub async fn spawn_pacer(tx_alerts: UnboundedSender<PacerAlert>) -> PacerHandle {
    spawn_pacer_with_interval(tx_alerts, Duration::from_secs(300)).await
}

pub async fn spawn_pacer_with_interval(
    tx_alerts: UnboundedSender<PacerAlert>,
    interval: Duration,
) -> PacerHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_task = stop.clone();
    tokio::spawn(async move {
        let mut cooldowns = HashMap::<PacerAlertKind, chrono::DateTime<Utc>>::new();
        while !stop_task.load(Ordering::SeqCst) {
            tokio::time::sleep(interval).await;
            if let Ok(alerts) = evaluate_once().await {
                for alert in alerts {
                    let kind = alert.kind();
                    let now = Utc::now();
                    let allow = cooldowns
                        .get(&kind)
                        .map(|last| now.signed_duration_since(*last).num_minutes() >= 60)
                        .unwrap_or(true);
                    if allow {
                        let _ = tx_alerts.send(alert);
                        cooldowns.insert(kind, now);
                    }
                }
            }
        }
    });
    PacerHandle { stop }
}

pub async fn evaluate_once() -> anyhow::Result<Vec<PacerAlert>> {
    let file = tasks::load().await?;
    let now = Utc::now();
    let mut alerts = Vec::new();
    let pending: Vec<_> = file
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Pending && !tasks::is_hook_review_task(task))
        .collect();
    if !pending.is_empty() {
        let oldest_minutes = pending
            .iter()
            .map(|task| {
                now.signed_duration_since(task.created_at)
                    .num_minutes()
                    .max(0) as u32
            })
            .max()
            .unwrap_or(0);
        if pending.len() >= 5 || oldest_minutes > 60 {
            alerts.push(PacerAlert::PendingPileUp {
                count: pending.len() as u32,
                oldest_minutes,
            });
        }
    }
    for task in file
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::InProgress)
    {
        let stalled_minutes = now
            .signed_duration_since(task.created_at)
            .num_minutes()
            .max(0) as u32;
        if stalled_minutes > 30 {
            alerts.push(PacerAlert::AgentStalled {
                task_id: task.id.clone(),
                stalled_minutes,
            });
        }
    }
    if now.minute() == 0 {
        alerts.push(PacerAlert::NoActivityHour {
            hour: now.hour() as u8,
        });
    }
    Ok(alerts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        tasks::{add, Artifact, Task},
        test_support::TestRoot,
    };
    use chrono::Duration as ChronoDuration;
    use std::path::PathBuf;

    #[tokio::test]
    async fn pending_pileup_alerts_once_for_regular_tasks() {
        let _root = TestRoot::new();
        for i in 0..6 {
            add(Task {
                id: format!("t-20260518-{i:03}"),
                source: "test".into(),
                kind: "review-doc".into(),
                artifact: Artifact {
                    file: Some(PathBuf::from(format!("doc-{i}.md"))),
                    anchor: None,
                },
                summary_ref: None,
                est_minutes: Some(5),
                focus_hint: None,
                status: TaskStatus::Pending,
                created_at: Utc::now() - ChronoDuration::minutes(5),
                session_id: None,
                terminal_id: None,
                cwd: None,
                current_request: None,
                last_output_summary: None,
                last_event_at: None,
                permission_summary: None,
            })
            .await
            .expect("add");
        }
        let alerts = evaluate_once().await.expect("evaluate");
        assert!(matches!(
            alerts.first(),
            Some(PacerAlert::PendingPileUp { count: 6, .. })
        ));
    }

    #[tokio::test]
    async fn pending_hook_reviews_do_not_trigger_pileup() {
        let _root = TestRoot::new();
        for i in 0..6 {
            add(Task {
                id: format!("t-20260518-{i:03}"),
                source: "claude".into(),
                kind: "review-doc".into(),
                artifact: Artifact {
                    file: Some(PathBuf::from(format!("doc-{i}.md"))),
                    anchor: None,
                },
                summary_ref: None,
                est_minutes: Some(5),
                focus_hint: Some("review latest output".into()),
                status: TaskStatus::Pending,
                created_at: Utc::now() - ChronoDuration::minutes(90),
                session_id: Some(format!("session-{i}")),
                terminal_id: None,
                cwd: Some(PathBuf::from("I:/personal-agent")),
                current_request: Some("run task".into()),
                last_output_summary: Some("done".into()),
                last_event_at: Some(Utc::now()),
                permission_summary: None,
            })
            .await
            .expect("add");
        }

        let alerts = evaluate_once().await.expect("evaluate");

        assert!(
            !alerts
                .iter()
                .any(|alert| matches!(alert, PacerAlert::PendingPileUp { .. })),
            "completed hook review backlog should not be announced as active task pileup"
        );
    }
}
