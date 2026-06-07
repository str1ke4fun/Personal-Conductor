use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurnRecord {
    pub id: String,
    pub session_id: Option<String>,
    pub projection_session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub request_id: String,
    pub initiator_kind: String,
    pub task_mode: String,
    pub capability: String,
    pub status: String,
    pub stage: Option<String>,
    pub user_message_id: Option<String>,
    pub assistant_message_id: Option<String>,
    pub llm_round_count: i64,
    pub tool_run_count: i64,
    pub active_tool_count: i64,
    pub projection_status: String,
    pub memory_status: String,
    pub model_provider: Option<String>,
    pub model_name: Option<String>,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub metadata_json: serde_json::Value,
    pub goal_cycle_id: Option<String>,
    pub agent_task_id: Option<String>,
    pub goal_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChatTurnCreate {
    pub session_id: Option<String>,
    pub projection_session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub request_id: String,
    pub initiator_kind: String,
    pub task_mode: String,
    pub capability: String,
    pub model_provider: Option<String>,
    pub model_name: Option<String>,
    pub metadata_json: serde_json::Value,
    pub goal_cycle_id: Option<String>,
    pub agent_task_id: Option<String>,
    pub goal_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurnEventRecord {
    pub id: String,
    pub turn_id: String,
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub request_id: String,
    pub seq: i64,
    pub event_type: String,
    pub phase: Option<String>,
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub payload_json: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct MessageProjectionCreate {
    pub request_id: String,
    pub message_id: Option<String>,
    pub role: String,
    pub projection_kind: String,
    pub status: String,
    pub visibility: String,
    pub plain_text: Option<String>,
    pub content_blocks_json: serde_json::Value,
    pub source_event_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageProjectionRecord {
    pub id: String,
    pub turn_id: String,
    pub message_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub role: String,
    pub projection_kind: String,
    pub status: String,
    pub visibility: String,
    pub plain_text: Option<String>,
    pub content_blocks_json: serde_json::Value,
    pub source_event_id: Option<String>,
    pub seq: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct MemoryCandidateCreate {
    pub request_id: String,
    pub source_message_id: Option<String>,
    pub source_projection_id: Option<String>,
    pub source_tool_call_id: Option<String>,
    pub memory_kind: String,
    pub scope_kind: String,
    pub scope_ref: Option<String>,
    pub path_prefix: Option<String>,
    pub key: String,
    pub value_json: serde_json::Value,
    pub summary: String,
    pub evidence_json: serde_json::Value,
    pub extractor_kind: String,
    pub extractor_provider: Option<String>,
    pub extractor_model: Option<String>,
    pub confidence: f64,
    pub status: String,
    pub dedupe_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCandidateRecord {
    pub id: String,
    pub turn_id: String,
    pub session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub source_message_id: Option<String>,
    pub source_projection_id: Option<String>,
    pub source_tool_call_id: Option<String>,
    pub memory_kind: String,
    pub scope_kind: String,
    pub scope_ref: Option<String>,
    pub path_prefix: Option<String>,
    pub key: String,
    pub value_json: serde_json::Value,
    pub summary: String,
    pub evidence_json: serde_json::Value,
    pub extractor_kind: String,
    pub extractor_provider: Option<String>,
    pub extractor_model: Option<String>,
    pub confidence: f64,
    pub status: String,
    pub dedupe_key: String,
    pub promoted_memory_entry_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create_turn(input: ChatTurnCreate) -> Result<ChatTurnRecord> {
    let pool = db::pool().await?;
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let metadata_json = input.metadata_json;
    let metadata_text = serde_json::to_string(&metadata_json)?;

    sqlx::query(
        r#"
        INSERT INTO chat_turns (
            id, session_id, projection_session_id, workspace_id, request_id,
            initiator_kind, task_mode, capability, status, started_at,
            model_provider, model_name, metadata_json,
            goal_cycle_id, agent_task_id, goal_id
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'received', ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        "#,
    )
    .bind(&id)
    .bind(&input.session_id)
    .bind(&input.projection_session_id)
    .bind(&input.workspace_id)
    .bind(&input.request_id)
    .bind(&input.initiator_kind)
    .bind(&input.task_mode)
    .bind(&input.capability)
    .bind(now.to_rfc3339())
    .bind(&input.model_provider)
    .bind(&input.model_name)
    .bind(&metadata_text)
    .bind(&input.goal_cycle_id)
    .bind(&input.agent_task_id)
    .bind(&input.goal_id)
    .execute(&pool)
    .await?;

    Ok(ChatTurnRecord {
        id,
        session_id: input.session_id,
        projection_session_id: input.projection_session_id,
        workspace_id: input.workspace_id,
        request_id: input.request_id,
        initiator_kind: input.initiator_kind,
        task_mode: input.task_mode,
        capability: input.capability,
        status: "received".to_string(),
        stage: Some("received".to_string()),
        user_message_id: None,
        assistant_message_id: None,
        llm_round_count: 0,
        tool_run_count: 0,
        active_tool_count: 0,
        projection_status: "pending".to_string(),
        memory_status: "pending".to_string(),
        model_provider: input.model_provider,
        model_name: input.model_name,
        error: None,
        started_at: now,
        finished_at: None,
        metadata_json,
        goal_cycle_id: input.goal_cycle_id,
        agent_task_id: input.agent_task_id,
        goal_id: input.goal_id,
    })
}

pub async fn find_turn_by_request_id(request_id: &str) -> Result<Option<ChatTurnRecord>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT
            id, session_id, projection_session_id, workspace_id, request_id,
            initiator_kind, task_mode, capability, status, stage,
            user_message_id, assistant_message_id, llm_round_count, tool_run_count,
            active_tool_count, projection_status, memory_status, model_provider,
            model_name, error, started_at, finished_at, metadata_json,
            goal_cycle_id, agent_task_id, goal_id
        FROM chat_turns
        WHERE request_id = ?1
        LIMIT 1
        "#,
    )
    .bind(request_id)
    .fetch_optional(&pool)
    .await?;

    row.map(turn_from_row).transpose()
}

pub async fn get_turn_by_goal_cycle_id(goal_cycle_id: &str) -> Result<Option<ChatTurnRecord>> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"
        SELECT
            id, session_id, projection_session_id, workspace_id, request_id,
            initiator_kind, task_mode, capability, status, stage,
            user_message_id, assistant_message_id, llm_round_count, tool_run_count,
            active_tool_count, projection_status, memory_status, model_provider,
            model_name, error, started_at, finished_at, metadata_json,
            goal_cycle_id, agent_task_id, goal_id
        FROM chat_turns
        WHERE goal_cycle_id = ?1
        ORDER BY started_at DESC, id DESC
        LIMIT 1
        "#,
    )
    .bind(goal_cycle_id)
    .fetch_optional(&pool)
    .await?;

    row.map(turn_from_row).transpose()
}

pub async fn list_turns_by_goal_cycle_id(goal_cycle_id: &str) -> Result<Vec<ChatTurnRecord>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT
            id, session_id, projection_session_id, workspace_id, request_id,
            initiator_kind, task_mode, capability, status, stage,
            user_message_id, assistant_message_id, llm_round_count, tool_run_count,
            active_tool_count, projection_status, memory_status, model_provider,
            model_name, error, started_at, finished_at, metadata_json,
            goal_cycle_id, agent_task_id, goal_id
        FROM chat_turns
        WHERE goal_cycle_id = ?1
        ORDER BY started_at DESC, id DESC
        "#,
    )
    .bind(goal_cycle_id)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(turn_from_row).collect()
}

pub async fn update_turn_stage_by_request(
    request_id: &str,
    status: &str,
    stage: Option<&str>,
    error: Option<&str>,
) -> Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE chat_turns
        SET status = ?1,
            stage = ?2,
            error = COALESCE(?3, error)
        WHERE request_id = ?4
        "#,
    )
    .bind(status)
    .bind(stage)
    .bind(error)
    .bind(request_id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn update_turn_counts_by_request(
    request_id: &str,
    llm_round_count: Option<i64>,
    tool_run_count: Option<i64>,
    active_tool_count: Option<i64>,
) -> Result<()> {
    let pool = db::pool().await?;
    sqlx::query(
        r#"
        UPDATE chat_turns
        SET llm_round_count = COALESCE(?1, llm_round_count),
            tool_run_count = COALESCE(?2, tool_run_count),
            active_tool_count = COALESCE(?3, active_tool_count)
        WHERE request_id = ?4
        "#,
    )
    .bind(llm_round_count)
    .bind(tool_run_count)
    .bind(active_tool_count)
    .bind(request_id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn attach_user_message_by_request(request_id: &str, message_id: &str) -> Result<()> {
    attach_message_by_request(request_id, message_id, "user_message_id").await
}

pub async fn attach_assistant_message_by_request(request_id: &str, message_id: &str) -> Result<()> {
    attach_message_by_request(request_id, message_id, "assistant_message_id").await
}

pub async fn mark_turn_projection_status_by_request(
    request_id: &str,
    projection_status: &str,
) -> Result<()> {
    let pool = db::pool().await?;
    sqlx::query("UPDATE chat_turns SET projection_status = ?1 WHERE request_id = ?2")
        .bind(projection_status)
        .bind(request_id)
        .execute(&pool)
        .await?;
    Ok(())
}

pub async fn mark_turn_memory_status_by_request(
    request_id: &str,
    memory_status: &str,
) -> Result<()> {
    let pool = db::pool().await?;
    sqlx::query("UPDATE chat_turns SET memory_status = ?1 WHERE request_id = ?2")
        .bind(memory_status)
        .bind(request_id)
        .execute(&pool)
        .await?;
    Ok(())
}

pub async fn finish_turn_by_request(
    request_id: &str,
    status: &str,
    error: Option<&str>,
) -> Result<()> {
    let pool = db::pool().await?;
    let now = Utc::now();
    sqlx::query(
        r#"
        UPDATE chat_turns
        SET status = ?1,
            finished_at = ?2,
            error = COALESCE(?3, error)
        WHERE request_id = ?4
        "#,
    )
    .bind(status)
    .bind(now.to_rfc3339())
    .bind(error)
    .bind(request_id)
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn append_turn_event_by_request(
    request_id: &str,
    event_type: &str,
    phase: Option<&str>,
    actor_kind: &str,
    actor_id: Option<&str>,
    payload_json: serde_json::Value,
) -> Result<Option<ChatTurnEventRecord>> {
    let Some(turn) = find_turn_by_request_id(request_id).await? else {
        return Ok(None);
    };

    let pool = db::pool().await?;
    let seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM chat_turn_events WHERE turn_id = ?1",
    )
    .bind(&turn.id)
    .fetch_one(&pool)
    .await?;
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let payload_text = serde_json::to_string(&payload_json)?;

    sqlx::query(
        r#"
        INSERT INTO chat_turn_events (
            id, turn_id, session_id, workspace_id, request_id, seq,
            event_type, phase, actor_kind, actor_id, payload_json, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
    )
    .bind(&id)
    .bind(&turn.id)
    .bind(&turn.session_id)
    .bind(&turn.workspace_id)
    .bind(request_id)
    .bind(seq)
    .bind(event_type)
    .bind(phase)
    .bind(actor_kind)
    .bind(actor_id)
    .bind(&payload_text)
    .bind(now.to_rfc3339())
    .execute(&pool)
    .await?;

    Ok(Some(ChatTurnEventRecord {
        id,
        turn_id: turn.id,
        session_id: turn.session_id,
        workspace_id: turn.workspace_id,
        request_id: request_id.to_string(),
        seq,
        event_type: event_type.to_string(),
        phase: phase.map(str::to_string),
        actor_kind: actor_kind.to_string(),
        actor_id: actor_id.map(str::to_string),
        payload_json,
        created_at: now,
    }))
}

pub async fn append_stage_event_by_request(
    request_id: &str,
    stage: &str,
    payload_json: serde_json::Value,
) -> Result<Option<ChatTurnEventRecord>> {
    let event = append_turn_event_by_request(
        request_id,
        &format!("stage.{stage}"),
        Some(stage),
        "system",
        None,
        payload_json,
    )
    .await?;

    if event.is_some() {
        apply_stage_transition(request_id, stage).await?;
    }

    Ok(event)
}

pub async fn append_tool_event_by_request(
    request_id: &str,
    event_type: &str,
    tool_call_id: &str,
    tool_id: &str,
    status: &str,
    mut payload_json: serde_json::Value,
) -> Result<Option<ChatTurnEventRecord>> {
    if !payload_json.is_object() {
        payload_json = serde_json::json!({ "detail": payload_json });
    }

    if let Some(obj) = payload_json.as_object_mut() {
        obj.insert("tool_call_id".to_string(), serde_json::json!(tool_call_id));
        obj.insert("tool_id".to_string(), serde_json::json!(tool_id));
        obj.insert("status".to_string(), serde_json::json!(status));
    }

    append_turn_event_by_request(
        request_id,
        event_type,
        Some(status),
        "tool",
        Some(tool_call_id),
        payload_json,
    )
    .await
}

#[allow(dead_code)]
pub async fn list_turn_events_by_request(request_id: &str) -> Result<Vec<ChatTurnEventRecord>> {
    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT
            id, turn_id, session_id, workspace_id, request_id, seq,
            event_type, phase, actor_kind, actor_id, payload_json, created_at
        FROM chat_turn_events
        WHERE request_id = ?1
        ORDER BY seq ASC, created_at ASC
        "#,
    )
    .bind(request_id)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(turn_event_from_row).collect()
}

pub async fn list_message_projections_by_session(
    session_id: &str,
    limit: Option<u32>,
) -> Result<Vec<MessageProjectionRecord>> {
    let pool = db::pool().await?;
    let limit = i64::from(limit.unwrap_or(200).clamp(1, 500));
    let rows = sqlx::query(
        r#"
        SELECT
            id, turn_id, message_id, session_id, workspace_id, role,
            projection_kind, status, visibility, plain_text, content_blocks_json,
            source_event_id, seq, created_at, updated_at
        FROM chat_message_projections
        WHERE session_id = ?1
        ORDER BY seq ASC, created_at ASC
        LIMIT ?2
        "#,
    )
    .bind(session_id)
    .bind(limit)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(message_projection_from_row).collect()
}

pub async fn list_message_projections_by_request(
    request_id: &str,
) -> Result<Vec<MessageProjectionRecord>> {
    let Some(turn) = find_turn_by_request_id(request_id).await? else {
        return Ok(Vec::new());
    };

    let pool = db::pool().await?;
    let rows = sqlx::query(
        r#"
        SELECT
            id, turn_id, message_id, session_id, workspace_id, role,
            projection_kind, status, visibility, plain_text, content_blocks_json,
            source_event_id, seq, created_at, updated_at
        FROM chat_message_projections
        WHERE turn_id = ?1
        ORDER BY seq ASC, created_at ASC
        "#,
    )
    .bind(&turn.id)
    .fetch_all(&pool)
    .await?;

    rows.into_iter().map(message_projection_from_row).collect()
}

pub async fn create_message_projection(
    input: MessageProjectionCreate,
) -> Result<MessageProjectionRecord> {
    let Some(turn) = find_turn_by_request_id(&input.request_id).await? else {
        anyhow::bail!("chat turn not found for request_id={}", input.request_id);
    };

    let pool = db::pool().await?;
    let seq: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) + 1 FROM chat_message_projections")
            .fetch_one(&pool)
            .await?;
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let content_text = serde_json::to_string(&input.content_blocks_json)?;

    sqlx::query(
        r#"
        INSERT INTO chat_message_projections (
            id, turn_id, message_id, session_id, workspace_id, role,
            projection_kind, status, visibility, plain_text, content_blocks_json,
            source_event_id, seq, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)
        "#,
    )
    .bind(&id)
    .bind(&turn.id)
    .bind(&input.message_id)
    .bind(&turn.session_id)
    .bind(&turn.workspace_id)
    .bind(&input.role)
    .bind(&input.projection_kind)
    .bind(&input.status)
    .bind(&input.visibility)
    .bind(&input.plain_text)
    .bind(&content_text)
    .bind(&input.source_event_id)
    .bind(seq)
    .bind(now.to_rfc3339())
    .execute(&pool)
    .await?;

    mark_turn_projection_status_by_request(&input.request_id, "stored").await?;
    let event_message_id = input.message_id.clone();
    let event_role = input.role.clone();
    let event_projection_kind = input.projection_kind.clone();
    let event_status = input.status.clone();
    let event_visibility = input.visibility.clone();
    let _ = append_turn_event_by_request(
        &input.request_id,
        "projection.created",
        Some(&input.projection_kind),
        "system",
        input.message_id.as_deref(),
        serde_json::json!({
            "message_id": event_message_id,
            "role": event_role,
            "projection_kind": event_projection_kind,
            "status": event_status,
            "visibility": event_visibility,
        }),
    )
    .await?;

    Ok(MessageProjectionRecord {
        id,
        turn_id: turn.id,
        message_id: input.message_id,
        session_id: turn.session_id,
        workspace_id: turn.workspace_id,
        role: input.role,
        projection_kind: input.projection_kind,
        status: input.status,
        visibility: input.visibility,
        plain_text: input.plain_text,
        content_blocks_json: input.content_blocks_json,
        source_event_id: input.source_event_id,
        seq,
        created_at: now,
        updated_at: now,
    })
}

pub async fn create_memory_candidate(
    input: MemoryCandidateCreate,
) -> Result<MemoryCandidateRecord> {
    let Some(turn) = find_turn_by_request_id(&input.request_id).await? else {
        anyhow::bail!("chat turn not found for request_id={}", input.request_id);
    };

    let pool = db::pool().await?;
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let value_text = serde_json::to_string(&input.value_json)?;
    let evidence_text = serde_json::to_string(&input.evidence_json)?;

    sqlx::query(
        r#"
        INSERT INTO memory_candidates (
            id, turn_id, session_id, workspace_id, source_message_id,
            source_projection_id, source_tool_call_id, memory_kind, scope_kind,
            scope_ref, path_prefix, key, value_json, summary, evidence_json,
            extractor_kind, extractor_provider, extractor_model, confidence,
            status, dedupe_key, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?22)
        "#,
    )
    .bind(&id)
    .bind(&turn.id)
    .bind(&turn.session_id)
    .bind(&turn.workspace_id)
    .bind(&input.source_message_id)
    .bind(&input.source_projection_id)
    .bind(&input.source_tool_call_id)
    .bind(&input.memory_kind)
    .bind(&input.scope_kind)
    .bind(&input.scope_ref)
    .bind(&input.path_prefix)
    .bind(&input.key)
    .bind(&value_text)
    .bind(&input.summary)
    .bind(&evidence_text)
    .bind(&input.extractor_kind)
    .bind(&input.extractor_provider)
    .bind(&input.extractor_model)
    .bind(input.confidence)
    .bind(&input.status)
    .bind(&input.dedupe_key)
    .bind(now.to_rfc3339())
    .execute(&pool)
    .await?;

    let mut candidate = MemoryCandidateRecord {
        id: id.clone(),
        turn_id: turn.id.clone(),
        session_id: turn.session_id.clone(),
        workspace_id: turn.workspace_id.clone(),
        source_message_id: input.source_message_id.clone(),
        source_projection_id: input.source_projection_id.clone(),
        source_tool_call_id: input.source_tool_call_id.clone(),
        memory_kind: input.memory_kind.clone(),
        scope_kind: input.scope_kind.clone(),
        scope_ref: input.scope_ref.clone(),
        path_prefix: input.path_prefix.clone(),
        key: input.key.clone(),
        value_json: input.value_json.clone(),
        summary: input.summary.clone(),
        evidence_json: input.evidence_json.clone(),
        extractor_kind: input.extractor_kind.clone(),
        extractor_provider: input.extractor_provider.clone(),
        extractor_model: input.extractor_model.clone(),
        confidence: input.confidence,
        status: input.status.clone(),
        dedupe_key: input.dedupe_key.clone(),
        promoted_memory_entry_id: None,
        created_at: now,
        updated_at: now,
    };

    mark_turn_memory_status_by_request(&input.request_id, "candidate_emitted").await?;
    let event_memory_kind = input.memory_kind.clone();
    let event_scope_kind = input.scope_kind.clone();
    let event_scope_ref = input.scope_ref.clone();
    let event_key = input.key.clone();
    let event_status = input.status.clone();
    let event_dedupe_key = input.dedupe_key.clone();
    let _ = append_turn_event_by_request(
        &input.request_id,
        "memory.candidate_emitted",
        Some(&input.memory_kind),
        "system",
        input.source_message_id.as_deref(),
        serde_json::json!({
            "memory_kind": event_memory_kind,
            "scope_kind": event_scope_kind,
            "scope_ref": event_scope_ref,
            "key": event_key,
            "status": event_status,
            "dedupe_key": event_dedupe_key,
        }),
    )
    .await?;

    if let Ok(Some(entry_id)) =
        promote_candidate_to_memory_entry(&input.request_id, &candidate).await
    {
        candidate.promoted_memory_entry_id = Some(entry_id);
        candidate.status = "promoted".to_string();
        candidate.updated_at = Utc::now();
    }

    Ok(candidate)
}

async fn attach_message_by_request(request_id: &str, message_id: &str, field: &str) -> Result<()> {
    let Some(turn) = find_turn_by_request_id(request_id).await? else {
        return Ok(());
    };
    let pool = db::pool().await?;

    sqlx::query("UPDATE chat_messages SET turn_id = ?1 WHERE id = ?2")
        .bind(&turn.id)
        .bind(message_id)
        .execute(&pool)
        .await?;

    let sql = format!("UPDATE chat_turns SET {field} = ?1 WHERE request_id = ?2");
    sqlx::query(&sql)
        .bind(message_id)
        .bind(request_id)
        .execute(&pool)
        .await?;
    Ok(())
}

async fn apply_stage_transition(request_id: &str, stage: &str) -> Result<()> {
    match stage {
        "received" => update_turn_stage_by_request(request_id, "received", Some(stage), None).await,
        "user_message_stored" => {
            update_turn_stage_by_request(request_id, "input_stored", Some(stage), None).await
        }
        "context_loaded" => {
            update_turn_stage_by_request(request_id, "context_loaded", Some(stage), None).await
        }
        "llm_turn_start" | "llm_turn_done" | "tool_catalog_injected" => {
            update_turn_stage_by_request(request_id, "llm_running", Some(stage), None).await
        }
        "tool_start" | "tool_done" => {
            update_turn_stage_by_request(request_id, "tool_running", Some(stage), None).await
        }
        "reply_stored" => {
            update_turn_stage_by_request(request_id, "reply_stored", Some(stage), None).await
        }
        "failed_recovered" => {
            update_turn_stage_by_request(request_id, "recovered", Some(stage), None).await
        }
        "timeout" => update_turn_stage_by_request(request_id, "timed_out", Some(stage), None).await,
        "failed" => {
            update_turn_stage_by_request(request_id, "failed", Some("failed"), None).await?;
            finish_turn_by_request(request_id, "failed", None).await
        }
        "done" => {
            update_turn_stage_by_request(request_id, "completed", Some("done"), None).await?;
            finish_turn_by_request(request_id, "completed", None).await
        }
        _ => update_turn_stage_by_request(request_id, "running", Some(stage), None).await,
    }
}

fn turn_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ChatTurnRecord> {
    let started_at =
        DateTime::parse_from_rfc3339(row.try_get::<String, _>("started_at")?.as_str())?
            .with_timezone(&Utc);
    let finished_at = row
        .try_get::<Option<String>, _>("finished_at")?
        .map(|value| DateTime::parse_from_rfc3339(&value).map(|dt| dt.with_timezone(&Utc)))
        .transpose()?;
    let metadata_json = row
        .try_get::<String, _>("metadata_json")
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    Ok(ChatTurnRecord {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        projection_session_id: row.try_get("projection_session_id")?,
        workspace_id: row.try_get("workspace_id")?,
        request_id: row.try_get("request_id")?,
        initiator_kind: row.try_get("initiator_kind")?,
        task_mode: row.try_get("task_mode")?,
        capability: row.try_get("capability")?,
        status: row.try_get("status")?,
        stage: row.try_get("stage")?,
        user_message_id: row.try_get("user_message_id")?,
        assistant_message_id: row.try_get("assistant_message_id")?,
        llm_round_count: row.try_get("llm_round_count")?,
        tool_run_count: row.try_get("tool_run_count")?,
        active_tool_count: row.try_get("active_tool_count")?,
        projection_status: row.try_get("projection_status")?,
        memory_status: row.try_get("memory_status")?,
        model_provider: row.try_get("model_provider")?,
        model_name: row.try_get("model_name")?,
        error: row.try_get("error")?,
        started_at,
        finished_at,
        metadata_json,
        goal_cycle_id: row.try_get("goal_cycle_id")?,
        agent_task_id: row.try_get("agent_task_id")?,
        goal_id: row.try_get("goal_id")?,
    })
}

fn message_projection_from_row(row: sqlx::sqlite::SqliteRow) -> Result<MessageProjectionRecord> {
    let created_at =
        DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc);
    let updated_at =
        DateTime::parse_from_rfc3339(row.try_get::<String, _>("updated_at")?.as_str())?
            .with_timezone(&Utc);
    let content_blocks_json = row
        .try_get::<String, _>("content_blocks_json")
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_else(|| serde_json::json!([]));

    Ok(MessageProjectionRecord {
        id: row.try_get("id")?,
        turn_id: row.try_get("turn_id")?,
        message_id: row.try_get("message_id")?,
        session_id: row.try_get("session_id")?,
        workspace_id: row.try_get("workspace_id")?,
        role: row.try_get("role")?,
        projection_kind: row.try_get("projection_kind")?,
        status: row.try_get("status")?,
        visibility: row.try_get("visibility")?,
        plain_text: row.try_get("plain_text")?,
        content_blocks_json,
        source_event_id: row.try_get("source_event_id")?,
        seq: row.try_get("seq")?,
        created_at,
        updated_at,
    })
}

#[allow(dead_code)]
fn turn_event_from_row(row: sqlx::sqlite::SqliteRow) -> Result<ChatTurnEventRecord> {
    let created_at =
        DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc);
    let payload_json = row
        .try_get::<String, _>("payload_json")
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    Ok(ChatTurnEventRecord {
        id: row.try_get("id")?,
        turn_id: row.try_get("turn_id")?,
        session_id: row.try_get("session_id")?,
        workspace_id: row.try_get("workspace_id")?,
        request_id: row.try_get("request_id")?,
        seq: row.try_get("seq")?,
        event_type: row.try_get("event_type")?,
        phase: row.try_get("phase")?,
        actor_kind: row.try_get("actor_kind")?,
        actor_id: row.try_get("actor_id")?,
        payload_json,
        created_at,
    })
}

async fn promote_candidate_to_memory_entry(
    request_id: &str,
    candidate: &MemoryCandidateRecord,
) -> Result<Option<String>> {
    let value = candidate
        .value_json
        .get("plain_text")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            candidate
                .value_json
                .get("summary")
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| candidate.summary.clone());

    if value.trim().is_empty() {
        return Ok(None);
    }

    let source = match candidate.extractor_kind.as_str() {
        "tool" => "tool",
        "user" => "user",
        _ => "inferred",
    };
    let scope = match candidate.scope_kind.as_str() {
        "workspace" | "path_prefix" => crate::memory::MemoryScope::Workspace,
        "file" | "document" => crate::memory::MemoryScope::Document,
        "session" => crate::memory::MemoryScope::Session,
        _ => crate::memory::MemoryScope::Global,
    };
    let workspace_id = match candidate.scope_kind.as_str() {
        "workspace" | "path_prefix" | "file" | "document" => candidate
            .scope_ref
            .as_deref()
            .or(candidate.workspace_id.as_deref()),
        _ => candidate.workspace_id.as_deref(),
    };

    let entry = crate::memory::set_with_scope_and_path(
        &candidate.key,
        &value,
        &candidate.memory_kind,
        scope,
        workspace_id,
        candidate.path_prefix.as_deref(),
        source,
    )
    .await?;

    let now = Utc::now().to_rfc3339();
    let pool = db::pool().await?;
    let goal_id = goal_id_for_turn(&candidate.turn_id).await?;
    sqlx::query(
        r#"
        UPDATE memory_entries
        SET source_session_id = ?1,
            source_turn_id = ?2,
            source_message_id = ?3,
            source_projection_id = ?4,
            source_tool_call_id = ?5,
            goal_id = ?6
        WHERE id = ?7
        "#,
    )
    .bind(&candidate.session_id)
    .bind(&candidate.turn_id)
    .bind(&candidate.source_message_id)
    .bind(&candidate.source_projection_id)
    .bind(&candidate.source_tool_call_id)
    .bind(&goal_id)
    .bind(&entry.id)
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        UPDATE memory_candidates
        SET status = 'promoted',
            promoted_memory_entry_id = ?1,
            updated_at = ?2
        WHERE id = ?3
        "#,
    )
    .bind(&entry.id)
    .bind(now)
    .bind(&candidate.id)
    .execute(&pool)
    .await?;

    mark_turn_memory_status_by_request(request_id, "promoted").await?;
    let _ = append_turn_event_by_request(
        request_id,
        "memory.entry_promoted",
        Some(&candidate.memory_kind),
        "system",
        candidate.source_message_id.as_deref(),
        serde_json::json!({
            "candidate_id": candidate.id,
            "memory_entry_id": entry.id,
            "memory_kind": candidate.memory_kind,
            "scope_kind": candidate.scope_kind,
            "scope_ref": candidate.scope_ref,
            "key": candidate.key,
            "source": source,
        }),
    )
    .await?;

    Ok(Some(entry.id))
}

async fn goal_id_for_turn(turn_id: &str) -> Result<Option<String>> {
    let pool = db::pool().await?;
    let goal_id = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT COALESCE(projected_session.goal_id, execution_session.goal_id)
        FROM chat_turns turn_row
        LEFT JOIN chat_sessions execution_session ON execution_session.id = turn_row.session_id
        LEFT JOIN chat_sessions projected_session ON projected_session.id = turn_row.projection_session_id
        WHERE turn_row.id = ?1
        "#,
    )
    .bind(turn_id)
    .fetch_optional(&pool)
    .await?
    .flatten();
    Ok(goal_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn create_turn_and_append_stage_events() {
        let _root = TestRoot::new();
        let turn = create_turn(ChatTurnCreate {
            session_id: Some("session-1".to_string()),
            projection_session_id: Some("session-1".to_string()),
            workspace_id: Some("ws-1".to_string()),
            request_id: "req-1".to_string(),
            initiator_kind: "user".to_string(),
            task_mode: "short".to_string(),
            capability: "ask_write".to_string(),
            model_provider: Some("openai".to_string()),
            model_name: Some("gpt-test".to_string()),
            metadata_json: serde_json::json!({ "plan_only": false }),
            goal_cycle_id: None,
            agent_task_id: None,
            goal_id: None,
        })
        .await
        .expect("create turn");

        assert_eq!(turn.request_id, "req-1");

        append_stage_event_by_request("req-1", "context_loaded", serde_json::json!({}))
            .await
            .expect("append event");
        let refreshed = find_turn_by_request_id("req-1")
            .await
            .expect("find turn")
            .expect("turn exists");
        assert_eq!(refreshed.status, "context_loaded");
        assert_eq!(refreshed.stage.as_deref(), Some("context_loaded"));

        let events = list_turn_events_by_request("req-1")
            .await
            .expect("list turn events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "stage.context_loaded");
    }

    #[tokio::test]
    async fn append_tool_event_records_tool_metadata() {
        let _root = TestRoot::new();
        create_turn(ChatTurnCreate {
            session_id: Some("session-tool".to_string()),
            projection_session_id: Some("session-tool".to_string()),
            workspace_id: Some("ws-tool".to_string()),
            request_id: "req-tool".to_string(),
            initiator_kind: "user".to_string(),
            task_mode: "short".to_string(),
            capability: "ask_write".to_string(),
            model_provider: None,
            model_name: None,
            metadata_json: serde_json::json!({}),
            goal_cycle_id: None,
            agent_task_id: None,
            goal_id: None,
        })
        .await
        .expect("create turn");

        append_tool_event_by_request(
            "req-tool",
            "tool.call_started",
            "tool-call-1",
            "demo.echo",
            "executing",
            serde_json::json!({
                "llm_tool_call_id": "llm-tool-1",
                "risk_level": "read_only",
            }),
        )
        .await
        .expect("append tool event");

        let events = list_turn_events_by_request("req-tool")
            .await
            .expect("list tool events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "tool.call_started");
        assert_eq!(events[0].actor_kind, "tool");
        assert_eq!(events[0].actor_id.as_deref(), Some("tool-call-1"));
        assert_eq!(
            events[0]
                .payload_json
                .get("tool_id")
                .and_then(|value| value.as_str()),
            Some("demo.echo")
        );
    }

    #[tokio::test]
    async fn create_projection_and_memory_candidate() {
        let _root = TestRoot::new();
        crate::memory::set_embedding_model(Box::new(crate::memory::HashEmbeddingModel::default()));
        create_turn(ChatTurnCreate {
            session_id: Some("session-2".to_string()),
            projection_session_id: Some("session-2".to_string()),
            workspace_id: Some("ws-2".to_string()),
            request_id: "req-2".to_string(),
            initiator_kind: "user".to_string(),
            task_mode: "short".to_string(),
            capability: "ask_write".to_string(),
            model_provider: None,
            model_name: None,
            metadata_json: serde_json::json!({}),
            goal_cycle_id: None,
            agent_task_id: None,
            goal_id: None,
        })
        .await
        .expect("create turn");

        let projection = create_message_projection(MessageProjectionCreate {
            request_id: "req-2".to_string(),
            message_id: Some("msg-2".to_string()),
            role: "assistant".to_string(),
            projection_kind: "assistant_final".to_string(),
            status: "finalized".to_string(),
            visibility: "visible".to_string(),
            plain_text: Some("done".to_string()),
            content_blocks_json: serde_json::json!([{ "type": "text", "text": "done" }]),
            source_event_id: None,
        })
        .await
        .expect("projection");

        assert_eq!(projection.workspace_id.as_deref(), Some("ws-2"));

        let candidate = create_memory_candidate(MemoryCandidateCreate {
            request_id: "req-2".to_string(),
            source_message_id: Some("msg-2".to_string()),
            source_projection_id: Some(projection.id.clone()),
            source_tool_call_id: None,
            memory_kind: "assistant_final_answer".to_string(),
            scope_kind: "workspace".to_string(),
            scope_ref: Some("ws-2".to_string()),
            path_prefix: None,
            key: "turn:req-2:assistant_final_answer".to_string(),
            value_json: serde_json::json!({ "summary": "done" }),
            summary: "done".to_string(),
            evidence_json: serde_json::json!({ "request_id": "req-2" }),
            extractor_kind: "rule".to_string(),
            extractor_provider: None,
            extractor_model: None,
            confidence: 0.4,
            status: "proposed".to_string(),
            dedupe_key: "turn:req-2:assistant_final_answer".to_string(),
        })
        .await
        .expect("candidate");

        assert_eq!(candidate.scope_kind, "workspace");
        assert!(
            candidate.promoted_memory_entry_id.is_some(),
            "candidate should be promoted into memory entries"
        );

        let events = list_turn_events_by_request("req-2")
            .await
            .expect("list projection and memory events");
        assert!(events
            .iter()
            .any(|event| event.event_type == "projection.created"));
        assert!(events
            .iter()
            .any(|event| event.event_type == "memory.candidate_emitted"));
        assert!(events
            .iter()
            .any(|event| event.event_type == "memory.entry_promoted"));

        let promoted_id = candidate
            .promoted_memory_entry_id
            .as_deref()
            .expect("promoted entry id");
        let pool = crate::db::pool().await.expect("db pool");
        let stored_value: Option<String> =
            sqlx::query_scalar("SELECT value FROM memory_entries WHERE id = ?1 LIMIT 1")
                .bind(promoted_id)
                .fetch_optional(&pool)
                .await
                .expect("load promoted memory entry");
        assert_eq!(stored_value.as_deref(), Some("done"));
    }
}
