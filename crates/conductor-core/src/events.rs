use crate::db;
use crate::paths::Paths;
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    fs::{self, OpenOptions},
    io::AsyncWriteExt,
};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub ts: DateTime<Utc>,
    pub source: String,
    pub kind: String,
    pub payload: Value,
}

/// Canonical audit event with typed fields.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub source: String,
    pub event_type: String,
    pub actor: String,
    pub target: String,
    pub detail: Value,
    pub session_id: Option<String>,
}

/// Optional filters for querying events.
#[derive(Default)]
pub struct EventFilter {
    pub source: Option<String>,
    pub event_type: Option<String>,
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
}

pub async fn recent(limit: usize) -> anyhow::Result<Vec<Event>> {
    let path = Paths::events();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .await
        .with_context(|| format!("read events file {}", path.display()))?;

    let mut events: Vec<Event> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    events.sort_by(|a, b| b.ts.cmp(&a.ts));
    Ok(events.into_iter().take(limit).collect())
}

/// Append a canonical `AuditEvent` to the NDJSON log.
pub async fn append_event(event: &AuditEvent) -> anyhow::Result<()> {
    // Primary write path: SQLite
    if let Err(e) = append_to_db(event).await {
        tracing::warn!("SQLite event write failed, falling back to NDJSON: {e:#}");
        // Fallback: write to NDJSON
        append_to_ndjson(event).await?;
        return Ok(());
    }
    // Also write to NDJSON for backward compatibility (fire-and-forget)
    let _ = append_to_ndjson(event).await;
    Ok(())
}

/// Write an event to the NDJSON file (legacy path).
async fn append_to_ndjson(event: &AuditEvent) -> anyhow::Result<()> {
    let path = Paths::events();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let mut bytes = serde_json::to_vec(event)?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
        .with_context(|| format!("open events file {}", path.display()))?;
    file.write_all(&bytes).await?;
    file.flush().await?;
    Ok(())
}

/// Write an event to the `runtime_events` SQLite table.
pub async fn append_to_db(event: &AuditEvent) -> anyhow::Result<()> {
    let pool = db::pool().await?;
    let id = Uuid::new_v4().to_string();

    let workspace_id = event
        .detail
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();

    let actor_id = if event.actor.is_empty() {
        "system".to_string()
    } else {
        event.actor.clone()
    };

    let subject_type = event
        .detail
        .get("subject_type")
        .and_then(|v| v.as_str())
        .unwrap_or(&event.target)
        .to_string();

    let subject_id = event
        .detail
        .get("subject_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&event.target)
        .to_string();

    let payload_json = serde_json::to_string(&event.detail)?;
    let created_at = event.timestamp.to_rfc3339();

    sqlx::query(
        r#"INSERT INTO runtime_events
          (id, workspace_id, source, actor_id, event_type, subject_type, subject_id, parent_event_id, payload_json, created_at)
          VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)"#,
    )
    .bind(&id)
    .bind(&workspace_id)
    .bind(&event.source)
    .bind(&actor_id)
    .bind(&event.event_type)
    .bind(&subject_type)
    .bind(&subject_id)
    .bind(&payload_json)
    .bind(&created_at)
    .execute(&pool)
    .await
    .with_context(|| "insert runtime_event")?;

    Ok(())
}

/// Convenience wrapper that builds an `AuditEvent` from simple arguments
/// and appends it. Backward-compatible with existing call sites.
pub async fn append(source: &str, kind: &str, payload: &Value) -> anyhow::Result<()> {
    let event = AuditEvent {
        timestamp: Utc::now(),
        source: source.to_string(),
        event_type: kind.to_string(),
        actor: String::new(),
        target: String::new(),
        detail: payload.clone(),
        session_id: None,
    };
    append_event(&event).await
}

// ── P0 convenience emitters ─────────────────────────────────────────────────

/// Emit a `tool_call.proposed` event before tool execution begins.
pub async fn emit_tool_call_proposed(tool_id: &str, tool_input: &serde_json::Value) {
    let event = AuditEvent {
        timestamp: Utc::now(),
        source: "conductor".into(),
        event_type: "tool_call.proposed".into(),
        actor: "agent".into(),
        target: tool_id.to_string(),
        detail: serde_json::json!({ "input": tool_input }),
        session_id: None,
    };
    let _ = append_event(&event).await;
}

pub async fn emit_tool_call_lifecycle(
    event_type: &str,
    tool_call_id: &str,
    workspace_id: Option<&str>,
    session_id: Option<&str>,
    tool_id: &str,
    status: &str,
    mut detail: serde_json::Value,
) {
    if !detail.is_object() {
        detail = serde_json::json!({ "value": detail });
    }

    if let Some(obj) = detail.as_object_mut() {
        obj.insert("subject_type".into(), serde_json::json!("tool_call"));
        obj.insert("subject_id".into(), serde_json::json!(tool_call_id));
        obj.insert("tool_call_id".into(), serde_json::json!(tool_call_id));
        obj.insert("tool_id".into(), serde_json::json!(tool_id));
        obj.insert("status".into(), serde_json::json!(status));
        if let Some(workspace_id) = workspace_id {
            obj.insert("workspace_id".into(), serde_json::json!(workspace_id));
        }
        if let Some(session_id) = session_id {
            obj.insert("session_id".into(), serde_json::json!(session_id));
        }
    }

    let event = AuditEvent {
        timestamp: Utc::now(),
        source: "conductor".into(),
        event_type: event_type.into(),
        actor: "agent".into(),
        target: tool_call_id.to_string(),
        detail,
        session_id: session_id.map(str::to_string),
    };
    let _ = append_event(&event).await;
}

/// Emit a `tool_call.finished` event after tool execution completes.
pub async fn emit_tool_call_finished(tool_id: &str, success: bool, duration_ms: u64) {
    let event = AuditEvent {
        timestamp: Utc::now(),
        source: "conductor".into(),
        event_type: "tool_call.finished".into(),
        actor: "agent".into(),
        target: tool_id.to_string(),
        detail: serde_json::json!({ "success": success, "duration_ms": duration_ms }),
        session_id: None,
    };
    let _ = append_event(&event).await;
}

/// Emit a `permission.requested` event when a proposal (permission request) is created.
pub async fn emit_permission_requested(proposal_id: &str, tool_id: &str, risk_level: &str) {
    let event = AuditEvent {
        timestamp: Utc::now(),
        source: "conductor".into(),
        event_type: "permission.requested".into(),
        actor: "agent".into(),
        target: proposal_id.to_string(),
        detail: serde_json::json!({ "tool_id": tool_id, "risk_level": risk_level }),
        session_id: None,
    };
    let _ = append_event(&event).await;
}

/// Emit an `agent_run.created` event when a new agent run is created.
pub async fn emit_agent_run_created(run_id: &str, agent_id: &str, status: &str) {
    let event = AuditEvent {
        timestamp: Utc::now(),
        source: "conductor".into(),
        event_type: "agent_run.created".into(),
        actor: "agent".into(),
        target: run_id.to_string(),
        detail: serde_json::json!({ "agent_id": agent_id, "status": status }),
        session_id: None,
    };
    let _ = append_event(&event).await;
}

/// Emit an `agent_run.phase_changed` event when an agent run transitions phase.
pub async fn emit_agent_run_phase_changed(run_id: &str, from_phase: &str, to_phase: &str) {
    let event = AuditEvent {
        timestamp: Utc::now(),
        source: "conductor".into(),
        event_type: "agent_run.phase_changed".into(),
        actor: "agent".into(),
        target: run_id.to_string(),
        detail: serde_json::json!({ "from": from_phase, "to": to_phase }),
        session_id: None,
    };
    let _ = append_event(&event).await;
}

/// Emit a `tool_call.blocked` event when a tool call is blocked (e.g., AskWrite approval_required).
pub async fn emit_tool_call_blocked(tool_id: &str, proposal_id: &str, reason: &str) {
    let event = AuditEvent {
        timestamp: Utc::now(),
        source: "conductor".into(),
        event_type: "tool_call.blocked".into(),
        actor: "agent".into(),
        target: tool_id.to_string(),
        detail: serde_json::json!({ "proposal_id": proposal_id, "reason": reason }),
        session_id: None,
    };
    let _ = append_event(&event).await;
}

/// Emit a `permission.approved` event when a proposal is approved.
pub async fn emit_permission_approved(proposal_id: &str, tool_id: &str) {
    let event = AuditEvent {
        timestamp: Utc::now(),
        source: "conductor".into(),
        event_type: "permission.approved".into(),
        actor: "user".into(),
        target: proposal_id.to_string(),
        detail: serde_json::json!({ "tool_id": tool_id }),
        session_id: None,
    };
    let _ = append_event(&event).await;
}

/// Emit a `permission.denied` event when a proposal is denied.
pub async fn emit_permission_denied(proposal_id: &str, tool_id: &str) {
    let event = AuditEvent {
        timestamp: Utc::now(),
        source: "conductor".into(),
        event_type: "permission.denied".into(),
        actor: "user".into(),
        target: proposal_id.to_string(),
        detail: serde_json::json!({ "tool_id": tool_id }),
        session_id: None,
    };
    let _ = append_event(&event).await;
}

/// Emit a `permission.revoked` event when a grant is revoked.
pub async fn emit_permission_revoked(grant_id: &str, tool_id: &str) {
    let event = AuditEvent {
        timestamp: Utc::now(),
        source: "conductor".into(),
        event_type: "permission.revoked".into(),
        actor: "user".into(),
        target: grant_id.to_string(),
        detail: serde_json::json!({ "tool_id": tool_id }),
        session_id: None,
    };
    let _ = append_event(&event).await;
}

/// Query events from the NDJSON log with optional filters.
///
/// Results are sorted newest-first and capped at `limit` (default 100).
pub async fn query_events(
    filter: EventFilter,
    limit: Option<usize>,
) -> anyhow::Result<Vec<AuditEvent>> {
    let path = Paths::events();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .await
        .with_context(|| format!("read events file {}", path.display()))?;

    let cap = limit.unwrap_or(100);

    let mut events: Vec<AuditEvent> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<AuditEvent>(line).ok())
        .filter(|ev| {
            if let Some(ref src) = filter.source {
                if &ev.source != src {
                    return false;
                }
            }
            if let Some(ref et) = filter.event_type {
                if &ev.event_type != et {
                    return false;
                }
            }
            if let Some(after) = filter.after {
                if ev.timestamp < after {
                    return false;
                }
            }
            if let Some(before) = filter.before {
                if ev.timestamp > before {
                    return false;
                }
            }
            true
        })
        .collect();

    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    events.truncate(cap);
    Ok(events)
}

/// Query events from the `runtime_events` SQLite table.
///
/// If `since_event_id` is provided, only events created *after* that event are returned
/// (ordered by `created_at ASC`). Otherwise returns the most recent `limit` events
/// (default 100, newest first).
pub async fn query_events_db(
    workspace_id: &str,
    since_event_id: Option<&str>,
    limit: Option<u32>,
) -> anyhow::Result<Vec<AuditEvent>> {
    let pool = db::pool().await?;
    let cap = limit.unwrap_or(100) as i64;

    if let Some(after_id) = since_event_id {
        // Look up the timestamp of the reference event, then fetch everything after it.
        let created_at_ref: Option<String> =
            sqlx::query_scalar("SELECT created_at FROM runtime_events WHERE id = ?")
                .bind(after_id)
                .fetch_optional(&pool)
                .await?;

        if let Some(after_ts) = created_at_ref {
            let rows = sqlx::query(
                r#"SELECT id, workspace_id, source, actor_id, event_type, subject_type, subject_id,
                          parent_event_id, payload_json, created_at
                   FROM runtime_events
                   WHERE workspace_id = ? AND created_at > ?
                   ORDER BY created_at ASC
                   LIMIT ?"#,
            )
            .bind(workspace_id)
            .bind(&after_ts)
            .bind(cap)
            .fetch_all(&pool)
            .await?;

            let events = rows
                .iter()
                .filter_map(|row| row_to_audit_event(row).ok())
                .collect();
            return Ok(events);
        }
    }

    // Default: most recent N events, newest first.
    let rows = sqlx::query(
        r#"SELECT id, workspace_id, source, actor_id, event_type, subject_type, subject_id,
                  parent_event_id, payload_json, created_at
           FROM runtime_events
           WHERE workspace_id = ?
           ORDER BY created_at DESC
           LIMIT ?"#,
    )
    .bind(workspace_id)
    .bind(cap)
    .fetch_all(&pool)
    .await?;

    let events = rows
        .iter()
        .filter_map(|row| row_to_audit_event(row).ok())
        .collect();
    Ok(events)
}

/// Convert an `AuditEvent` into an SSE-friendly JSON value.
pub fn event_to_sse_json(event: &AuditEvent) -> Value {
    serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "event": event.event_type,
        "data": {
            "timestamp": event.timestamp.to_rfc3339(),
            "source": event.source,
            "actor": event.actor,
            "target": event.target,
            "detail": event.detail,
            "session_id": event.session_id,
        }
    })
}

/// Return the total number of rows in the `runtime_events` table.
pub async fn event_count() -> anyhow::Result<i64> {
    let pool = db::pool().await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runtime_events")
        .fetch_one(&pool)
        .await?;
    Ok(count)
}

/// Map a SQLite row back into an `AuditEvent`.
fn row_to_audit_event(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<AuditEvent> {
    use sqlx::Row;
    let payload_json: String = row.try_get("payload_json")?;
    let detail: Value = serde_json::from_str(&payload_json)?;
    let created_at: String = row.try_get("created_at")?;
    let timestamp: DateTime<Utc> = DateTime::parse_from_rfc3339(&created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let actor_id: String = row.try_get("actor_id")?;
    let subject_id: String = row.try_get("subject_id")?;

    Ok(AuditEvent {
        timestamp,
        source: row.try_get("source")?,
        event_type: row.try_get("event_type")?,
        actor: actor_id.clone(),
        target: subject_id,
        detail,
        session_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;
    use serde_json::json;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_append_writes_parseable_ndjson_lines() {
        let _root = TestRoot::new();

        let mut handles = Vec::new();
        for i in 0..100 {
            handles.push(tokio::spawn(async move {
                append("test", "event", &json!({ "i": i })).await
            }));
        }
        for handle in handles {
            handle
                .await
                .expect("append task panicked")
                .expect("append event");
        }

        let content = fs::read_to_string(Paths::events())
            .await
            .expect("read events");
        let lines = content.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 100);
        for line in lines {
            let value: Value = serde_json::from_str(line).expect("event line is valid json");
            assert_eq!(value["source"], "test");
            assert_eq!(value["event_type"], "event");
            assert!(value["detail"]["i"].as_u64().is_some());
        }
    }

    #[tokio::test]
    async fn append_and_query_roundtrip() {
        let _root = TestRoot::new();

        let event = AuditEvent {
            timestamp: Utc::now(),
            source: "desktop".into(),
            event_type: "tool_call".into(),
            actor: "user-1".into(),
            target: "shell.exec".into(),
            detail: json!({"cmd": "ls"}),
            session_id: Some("sess-abc".into()),
        };
        append_event(&event).await.expect("append_event");

        let results = query_events(EventFilter::default(), None)
            .await
            .expect("query_events");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "desktop");
        assert_eq!(results[0].event_type, "tool_call");
        assert_eq!(results[0].actor, "user-1");
        assert_eq!(results[0].target, "shell.exec");
        assert_eq!(results[0].session_id, Some("sess-abc".into()));
    }

    #[tokio::test]
    async fn filter_by_source() {
        let _root = TestRoot::new();

        append("desktop", "tool_call", &json!({})).await.unwrap();
        append("cli", "state_change", &json!({})).await.unwrap();
        append("desktop", "error", &json!({})).await.unwrap();

        let filter = EventFilter {
            source: Some("cli".into()),
            ..Default::default()
        };
        let results = query_events(filter, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "cli");
    }

    #[tokio::test]
    async fn filter_by_time_range() {
        let _root = TestRoot::new();

        // Write events with explicit timestamps.
        let t1: DateTime<Utc> = "2025-01-01T00:00:00Z".parse().unwrap();
        let t2: DateTime<Utc> = "2025-06-15T12:00:00Z".parse().unwrap();
        let t3: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();

        for (ts, label) in [(t1, "old"), (t2, "mid"), (t3, "new")] {
            let ev = AuditEvent {
                timestamp: ts,
                source: "system".into(),
                event_type: "test".into(),
                actor: "a".into(),
                target: "t".into(),
                detail: json!({"label": label}),
                session_id: None,
            };
            append_event(&ev).await.unwrap();
        }

        // Query events between t1 (inclusive) and t3 (exclusive).
        let filter = EventFilter {
            after: Some(t1),
            before: Some(t3 - chrono::Duration::nanoseconds(1)),
            ..Default::default()
        };
        let results = query_events(filter, None).await.unwrap();
        assert_eq!(results.len(), 2);
        // newest first
        assert_eq!(results[0].detail["label"], "mid");
        assert_eq!(results[1].detail["label"], "old");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_append_event_writes_parseable_ndjson_lines() {
        let _root = TestRoot::new();

        let mut handles = Vec::new();
        for i in 0..50 {
            handles.push(tokio::spawn(async move {
                let ev = AuditEvent {
                    timestamp: Utc::now(),
                    source: "agent".into(),
                    event_type: "tool_call".into(),
                    actor: format!("agent-{i}"),
                    target: "resource".into(),
                    detail: json!({"i": i}),
                    session_id: None,
                };
                append_event(&ev).await
            }));
        }
        for handle in handles {
            handle.await.expect("task panicked").expect("append_event");
        }

        let results = query_events(EventFilter::default(), Some(200))
            .await
            .expect("query");
        assert_eq!(results.len(), 50);
        for ev in &results {
            assert_eq!(ev.source, "agent");
            assert_eq!(ev.event_type, "tool_call");
            assert!(ev.detail["i"].as_u64().is_some());
        }
    }

    #[tokio::test]
    async fn query_empty_file_returns_empty_vec() {
        let _root = TestRoot::new();

        let results = query_events(EventFilter::default(), None).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn audit_event_serializes_to_valid_json() {
        let event = AuditEvent {
            timestamp: "2025-03-15T10:30:00Z".parse().unwrap(),
            source: "cli".into(),
            event_type: "user_action".into(),
            actor: "user-42".into(),
            target: "config.update".into(),
            detail: json!({"key": "theme", "value": "dark"}),
            session_id: Some("sess-xyz".into()),
        };

        let json_str = serde_json::to_string(&event).unwrap();
        let parsed: Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed["source"], "cli");
        assert_eq!(parsed["event_type"], "user_action");
        assert_eq!(parsed["actor"], "user-42");
        assert_eq!(parsed["target"], "config.update");
        assert_eq!(parsed["detail"]["key"], "theme");
        assert_eq!(parsed["session_id"], "sess-xyz");
        // Verify timestamp serialized as RFC 3339 string
        assert!(parsed["timestamp"].as_str().unwrap().contains("2025-03-15"));
    }

    // ── P0 emitter integration tests ────────────────────────────────────────

    #[tokio::test]
    async fn emit_tool_call_proposed_creates_event() {
        let _root = TestRoot::new();

        let input = json!({"path": "/tmp/test.txt"});
        emit_tool_call_proposed("file.read", &input).await;

        let results = query_events(
            EventFilter {
                event_type: Some("tool_call.proposed".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "conductor");
        assert_eq!(results[0].event_type, "tool_call.proposed");
        assert_eq!(results[0].actor, "agent");
        assert_eq!(results[0].target, "file.read");
        assert_eq!(results[0].detail["input"]["path"], "/tmp/test.txt");
    }

    #[tokio::test]
    async fn emit_tool_call_finished_creates_event() {
        let _root = TestRoot::new();

        emit_tool_call_finished("bash.execute", true, 42).await;

        let results = query_events(
            EventFilter {
                event_type: Some("tool_call.finished".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "conductor");
        assert_eq!(results[0].event_type, "tool_call.finished");
        assert_eq!(results[0].target, "bash.execute");
        assert_eq!(results[0].detail["success"], true);
        assert_eq!(results[0].detail["duration_ms"], 42);
    }

    #[tokio::test]
    async fn emit_permission_requested_creates_event() {
        let _root = TestRoot::new();

        emit_permission_requested("p-20260529-001", "file.write", "workspace_write").await;

        let results = query_events(
            EventFilter {
                event_type: Some("permission.requested".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "conductor");
        assert_eq!(results[0].event_type, "permission.requested");
        assert_eq!(results[0].actor, "agent");
        assert_eq!(results[0].target, "p-20260529-001");
        assert_eq!(results[0].detail["tool_id"], "file.write");
        assert_eq!(results[0].detail["risk_level"], "workspace_write");
    }

    #[tokio::test]
    async fn emit_agent_run_created_creates_event() {
        let _root = TestRoot::new();

        emit_agent_run_created("run-001", "claude", "queued").await;

        let results = query_events(
            EventFilter {
                event_type: Some("agent_run.created".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].target, "run-001");
        assert_eq!(results[0].detail["agent_id"], "claude");
        assert_eq!(results[0].detail["status"], "queued");
    }

    #[tokio::test]
    async fn emit_agent_run_phase_changed_creates_event() {
        let _root = TestRoot::new();

        emit_agent_run_phase_changed("run-001", "queued", "running").await;

        let results = query_events(
            EventFilter {
                event_type: Some("agent_run.phase_changed".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].detail["from"], "queued");
        assert_eq!(results[0].detail["to"], "running");
    }

    #[tokio::test]
    async fn emit_tool_call_blocked_creates_event() {
        let _root = TestRoot::new();

        emit_tool_call_blocked("bash.execute", "prop-001", "approval_required").await;

        let results = query_events(
            EventFilter {
                event_type: Some("tool_call.blocked".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].target, "bash.execute");
        assert_eq!(results[0].detail["proposal_id"], "prop-001");
        assert_eq!(results[0].detail["reason"], "approval_required");
    }

    #[tokio::test]
    async fn emit_permission_approved_creates_event() {
        let _root = TestRoot::new();

        emit_permission_approved("prop-001", "file.write").await;

        let results = query_events(
            EventFilter {
                event_type: Some("permission.approved".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].actor, "user");
        assert_eq!(results[0].target, "prop-001");
        assert_eq!(results[0].detail["tool_id"], "file.write");
    }

    #[tokio::test]
    async fn emit_permission_denied_creates_event() {
        let _root = TestRoot::new();

        emit_permission_denied("prop-002", "bash.execute").await;

        let results = query_events(
            EventFilter {
                event_type: Some("permission.denied".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].actor, "user");
        assert_eq!(results[0].target, "prop-002");
        assert_eq!(results[0].detail["tool_id"], "bash.execute");
    }

    #[tokio::test]
    async fn emit_permission_revoked_creates_event() {
        let _root = TestRoot::new();

        emit_permission_revoked("grant-001", "shell.exec").await;

        let results = query_events(
            EventFilter {
                event_type: Some("permission.revoked".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].actor, "user");
        assert_eq!(results[0].target, "grant-001");
        assert_eq!(results[0].detail["tool_id"], "shell.exec");
    }

    // ── SQLite migration tests (TASK-078) ──────────────────────────────────

    #[tokio::test]
    async fn write_event_to_sqlite() {
        let _root = TestRoot::new();

        let event = AuditEvent {
            timestamp: Utc::now(),
            source: "conductor".into(),
            event_type: "test.write".into(),
            actor: "agent".into(),
            target: "resource-1".into(),
            detail: json!({"key": "value"}),
            session_id: None,
        };
        append_to_db(&event).await.expect("append_to_db");

        let count = event_count().await.expect("event_count");
        assert!(count >= 1, "expected at least 1 event, got {count}");
    }

    #[tokio::test]
    async fn query_events_db_roundtrip() {
        let _root = TestRoot::new();

        let ev1 = AuditEvent {
            timestamp: "2026-01-01T00:00:00Z".parse().unwrap(),
            source: "conductor".into(),
            event_type: "test.first".into(),
            actor: "agent".into(),
            target: "res-1".into(),
            detail: json!({"workspace_id": "ws-test", "n": 1}),
            session_id: None,
        };
        append_to_db(&ev1).await.expect("write ev1");

        // Small delay so created_at ordering is deterministic
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let ev2 = AuditEvent {
            timestamp: "2026-01-02T00:00:00Z".parse().unwrap(),
            source: "conductor".into(),
            event_type: "test.second".into(),
            actor: "agent".into(),
            target: "res-2".into(),
            detail: json!({"workspace_id": "ws-test", "n": 2}),
            session_id: None,
        };
        append_to_db(&ev2).await.expect("write ev2");

        let results = query_events_db("ws-test", None, None)
            .await
            .expect("query_events_db");
        assert!(results.len() >= 2, "expected >= 2, got {}", results.len());
        // Newest first (default ordering)
        assert_eq!(results[0].event_type, "test.second");
        assert_eq!(results[1].event_type, "test.first");
    }

    #[tokio::test]
    async fn query_events_db_since_event_id_filter() {
        let _root = TestRoot::new();

        // Insert first event and capture its id from the DB.
        let ev1 = AuditEvent {
            timestamp: "2026-02-01T00:00:00Z".parse().unwrap(),
            source: "test".into(),
            event_type: "since.before".into(),
            actor: "agent".into(),
            target: "t".into(),
            detail: json!({"workspace_id": "ws-since"}),
            session_id: None,
        };
        append_to_db(&ev1).await.expect("write ev1");

        // Grab the id of the event we just wrote.
        let pool = db::pool().await.unwrap();
        let first_id: String = sqlx::query_scalar(
            "SELECT id FROM runtime_events WHERE event_type = 'since.before' LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .expect("get first id");

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let ev2 = AuditEvent {
            timestamp: "2026-02-02T00:00:00Z".parse().unwrap(),
            source: "test".into(),
            event_type: "since.after".into(),
            actor: "agent".into(),
            target: "t".into(),
            detail: json!({"workspace_id": "ws-since"}),
            session_id: None,
        };
        append_to_db(&ev2).await.expect("write ev2");

        // Query with since_event_id should only return the second event.
        let results = query_events_db("ws-since", Some(&first_id), None)
            .await
            .expect("query since");
        assert_eq!(
            results.len(),
            1,
            "expected 1 event after since filter, got {}",
            results.len()
        );
        assert_eq!(results[0].event_type, "since.after");
    }

    #[tokio::test]
    async fn ndjson_fallback_still_works() {
        let _root = TestRoot::new();

        // append_event should dual-write. Verify NDJSON file still gets data.
        let event = AuditEvent {
            timestamp: Utc::now(),
            source: "fallback".into(),
            event_type: "ndjson.check".into(),
            actor: "agent".into(),
            target: "file".into(),
            detail: json!({"check": true}),
            session_id: None,
        };
        append_event(&event).await.expect("append_event");

        // NDJSON file should contain the event
        let content = fs::read_to_string(Paths::events())
            .await
            .expect("read NDJSON");
        assert!(
            content.contains("ndjson.check"),
            "NDJSON file should contain the event"
        );

        // SQLite should also contain it
        let count = event_count().await.expect("event_count");
        assert!(count >= 1, "SQLite should also have the event");
    }
}
