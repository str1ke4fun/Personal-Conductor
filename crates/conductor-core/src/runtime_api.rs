use axum::{
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response, Sse},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::json;
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::sync::{broadcast, oneshot};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use crate::events::AuditEvent;

/// HTTP server exposing a local runtime API protected by a bearer token.
pub struct RuntimeApiServer {
    bind: String,
    port: u16,
    token: String,
    shutdown_tx: Option<oneshot::Sender<()>>,
    bound_addr: Option<SocketAddr>,
    event_tx: broadcast::Sender<AuditEvent>,
}

impl RuntimeApiServer {
    /// Create a new server instance. Does **not** start listening yet.
    pub fn new(bind: &str, port: u16, token: &str) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            bind: bind.to_string(),
            port,
            token: token.to_string(),
            shutdown_tx: None,
            bound_addr: None,
            event_tx,
        }
    }

    /// Returns a clone of the broadcast sender for pushing SSE events.
    pub fn event_sender(&self) -> broadcast::Sender<AuditEvent> {
        self.event_tx.clone()
    }

    /// Start the axum server inside a dedicated tokio task.
    ///
    /// The listener is bound synchronously so that bind errors are returned
    /// immediately. The server runs in the background until
    /// [`stop`](Self::stop) is called.
    pub async fn start(&mut self) -> anyhow::Result<()> {
        let token = self.token.clone();
        let addr: SocketAddr = format!("{}:{}", self.bind, self.port).parse()?;

        let listener = tokio::net::TcpListener::bind(addr).await?;
        self.bound_addr = Some(listener.local_addr()?);

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(shutdown_tx);

        let sse_state = SseState {
            tx: self.event_tx.clone(),
        };

        let app = Router::new()
            .route("/runtime/health", get(Self::health_handler))
            .route("/runtime/messages", get(get_messages).post(post_message))
            .route("/runtime/heartbeats", post(post_heartbeat))
            .route("/runtime/tasks/claim", post(claim_next_task))
            .route("/runtime/tasks/{task_id}/claim", post(claim_task))
            .route("/runtime/tasks/{task_id}/start", post(start_task))
            .route("/runtime/tasks/{task_id}/complete", post(complete_task))
            .route("/runtime/tasks/{task_id}/fail", post(fail_task))
            .route("/runtime/tasks/{task_id}/block", post(block_task))
            .route(
                "/runtime/tasks/{task_id}/execute",
                post(execute_task_handler),
            )
            .route(
                "/runtime/goals",
                get(list_goals_handler).post(create_goal_handler),
            )
            .route("/runtime/goals/{goal_id}/start", post(start_goal_handler))
            .route("/runtime/goals/{goal_id}/pause", post(pause_goal_handler))
            .route("/runtime/goals/{goal_id}/cancel", post(cancel_goal_handler))
            .route(
                "/runtime/goals/{goal_id}/approve-plan",
                post(approve_plan_handler),
            )
            .route(
                "/runtime/goals/{goal_id}/review-verdict",
                post(review_verdict_handler),
            )
            .route("/runtime/goals/{goal_id}/cycles", get(list_cycles_handler))
            .route(
                "/runtime/goals/{goal_id}/cycles/{cycle_id}",
                get(get_cycle_handler),
            )
            .route(
                "/runtime/permission-requests",
                post(request_permission_handler),
            )
            .route(
                "/runtime/permissions/{request_id}/approve",
                post(approve_permission_handler),
            )
            .route(
                "/runtime/permissions/{request_id}/deny",
                post(deny_permission_handler),
            )
            .route("/runtime/events", get(events_sse_handler))
            .with_state(sse_state)
            .layer(middleware::from_fn(
                move |headers: HeaderMap, req: Request, next: Next| {
                    let token = token.clone();
                    async move { Self::bearer_auth_middleware(&token, &headers, req, next).await }
                },
            ));

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("runtime API server error");
        });

        Ok(())
    }

    /// Return the address the server is actually listening on.
    ///
    /// Only available after [`start`](Self::start) has been called.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.bound_addr
    }

    /// Gracefully stop the server.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    // -- internal handlers --------------------------------------------------

    async fn health_handler() -> Json<serde_json::Value> {
        Json(json!({ "status": "ok" }))
    }

    async fn bearer_auth_middleware(
        expected_token: &str,
        headers: &HeaderMap,
        req: Request,
        next: Next,
    ) -> Response {
        // Extract optional agent identity headers (for logging/future use).
        let _agent_id = headers
            .get("X-Agent-Id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let _agent_kind = headers
            .get("X-Agent-Kind")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Validate bearer token.
        let auth_header = match headers.get("authorization") {
            Some(v) => v,
            None => {
                return (StatusCode::UNAUTHORIZED, "missing Authorization header").into_response();
            }
        };

        let auth_str = match auth_header.to_str() {
            Ok(s) => s,
            Err(_) => {
                return (StatusCode::UNAUTHORIZED, "invalid Authorization header").into_response();
            }
        };

        let token = match auth_str.strip_prefix("Bearer ") {
            Some(t) => t,
            None => {
                return (StatusCode::UNAUTHORIZED, "invalid Authorization scheme").into_response();
            }
        };

        if token != expected_token {
            return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
        }

        next.run(req).await
    }
}

// ---------------------------------------------------------------------------
// SSE types
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SseState {
    tx: broadcast::Sender<AuditEvent>,
}

#[derive(Deserialize)]
struct EventsQuery {
    workspace_id: String,
    since_event_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Request body types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PostMessageBody {
    workspace_id: String,
    goal_id: Option<String>,
    cycle_id: Option<String>,
    task_id: Option<String>,
    sender_id: String,
    recipient_id: Option<String>,
    topic: String,
    kind: String,
    content: String,
    payload_json: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct GetMessagesQuery {
    workspace_id: String,
    topic: Option<String>,
    since: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct PostHeartbeatBody {
    workspace_id: String,
    agent_id: String,
    process_id: Option<i64>,
    task_id: Option<String>,
    goal_id: Option<String>,
    status: String,
    stage_label: Option<String>,
    progress_text: Option<String>,
    active_tool_count: i64,
    ttl_seconds: i64,
}

#[derive(Deserialize)]
struct ClaimTaskBody {
    agent_id: String,
    lease_ttl_seconds: i64,
    workspace_id: Option<String>,
}

#[derive(Deserialize)]
struct CompleteTaskBody {
    result_ref: Option<String>,
}

#[derive(Deserialize)]
struct FailTaskBody {
    error: Option<String>,
}

#[derive(Deserialize)]
struct BlockTaskBody {
    reason: Option<String>,
}

#[derive(Deserialize)]
struct CreateGoalBody {
    workspace_id: String,
    title: String,
    objective: String,
    priority: String,
    owner: String,
    budget_json: Option<serde_json::Value>,
    policy_json: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ListGoalsQuery {
    workspace_id: String,
    status: Option<String>,
    limit: Option<i64>,
}

#[derive(Deserialize)]
struct ReviewVerdictBody {
    verdict: String,
}

#[derive(Deserialize)]
struct RequestPermissionBody {
    tool_id: String,
    risk_level: String,
    grantee: String,
    workspace_id: Option<String>,
    scope: Option<PermissionScopeBody>,
}

#[derive(Deserialize)]
struct PermissionScopeBody {
    workspace_ids: Vec<String>,
    tool_prefixes: Vec<String>,
    max_risk_level: String,
}

#[derive(Deserialize)]
struct ApprovePermissionBody {
    mode: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn json_error(status: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(json!({"error": message})))
}

// ---------------------------------------------------------------------------
// Message handlers
// ---------------------------------------------------------------------------

async fn post_message(
    Json(body): Json<PostMessageBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let msg = crate::agent_messages::post_message(
        &body.workspace_id,
        body.goal_id.as_deref(),
        body.cycle_id.as_deref(),
        body.task_id.as_deref(),
        &body.sender_id,
        body.recipient_id.as_deref(),
        &body.topic,
        &body.kind,
        &body.content,
        body.payload_json,
    )
    .await
    .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    Ok((StatusCode::CREATED, Json(json!(msg))))
}

async fn get_messages(
    Query(q): Query<GetMessagesQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let messages = crate::agent_messages::get_messages(
        &q.workspace_id,
        q.topic.as_deref(),
        q.since.as_deref(),
        q.limit,
    )
    .await
    .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    Ok(Json(json!(messages)))
}

// ---------------------------------------------------------------------------
// Heartbeat handlers
// ---------------------------------------------------------------------------

async fn post_heartbeat(
    Json(body): Json<PostHeartbeatBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let hb = crate::heartbeat::upsert_heartbeat(
        &body.workspace_id,
        &body.agent_id,
        body.process_id,
        body.task_id.as_deref(),
        body.goal_id.as_deref(),
        &body.status,
        body.stage_label.as_deref(),
        body.progress_text.as_deref(),
        body.active_tool_count,
        body.ttl_seconds,
    )
    .await
    .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    Ok(Json(json!(hb)))
}

// ---------------------------------------------------------------------------
// Task handlers
// ---------------------------------------------------------------------------

async fn claim_task(
    Path(task_id): Path<String>,
    Json(body): Json<ClaimTaskBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let task = crate::goal_tasks::claim_task(&task_id, &body.agent_id, body.lease_ttl_seconds)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                json_error(StatusCode::NOT_FOUND, &msg)
            } else {
                json_error(StatusCode::CONFLICT, &msg)
            }
        })?;

    Ok(Json(json!(task)))
}

async fn claim_next_task(
    Json(body): Json<ClaimTaskBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let Some(task) = crate::goal_tasks::claim_next_queued_task(
        body.workspace_id.as_deref(),
        &body.agent_id,
        body.lease_ttl_seconds,
    )
    .await
    .map_err(|e| json_error(StatusCode::CONFLICT, &e.to_string()))?
    else {
        return Err(json_error(
            StatusCode::NOT_FOUND,
            "no queued task available",
        ));
    };

    Ok(Json(json!(task)))
}

async fn start_task(
    Path(task_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let task = crate::goal_tasks::start_task(&task_id).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("not found") {
            json_error(StatusCode::NOT_FOUND, &msg)
        } else {
            json_error(StatusCode::CONFLICT, &msg)
        }
    })?;

    Ok(Json(json!(task)))
}

async fn complete_task(
    Path(task_id): Path<String>,
    Json(body): Json<CompleteTaskBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let task = crate::goal_tasks::complete_task(&task_id, body.result_ref.as_deref().unwrap_or(""))
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                json_error(StatusCode::NOT_FOUND, &msg)
            } else {
                json_error(StatusCode::INTERNAL_SERVER_ERROR, &msg)
            }
        })?;

    Ok(Json(json!(task)))
}

async fn fail_task(
    Path(task_id): Path<String>,
    Json(body): Json<FailTaskBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let task = crate::goal_tasks::fail_task(&task_id, body.error.as_deref().unwrap_or(""))
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                json_error(StatusCode::NOT_FOUND, &msg)
            } else {
                json_error(StatusCode::INTERNAL_SERVER_ERROR, &msg)
            }
        })?;

    Ok(Json(json!(task)))
}

async fn block_task(
    Path(task_id): Path<String>,
    Json(body): Json<BlockTaskBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let task = crate::goal_tasks::block_task(&task_id, body.reason.as_deref().unwrap_or(""))
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                json_error(StatusCode::NOT_FOUND, &msg)
            } else {
                json_error(StatusCode::INTERNAL_SERVER_ERROR, &msg)
            }
        })?;

    Ok(Json(json!(task)))
}

// ---------------------------------------------------------------------------
// Goal handlers
// ---------------------------------------------------------------------------

/// POST /runtime/tasks/{task_id}/execute
///
/// Signals the desktop to execute this task using the built-in conductor
/// chat API (full tool set, all 47 tools). The task must be in "running" state.
/// Execution is async — the caller should poll the task status or listen for
/// task.review_ready / task.failed events.
///
/// This endpoint queues the task for execution by the desktop process via
/// the task_signal mechanism. The desktop's execute_task command will pick
/// it up.
async fn execute_task_handler(
    Path(task_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let task = crate::goal_tasks::get_task(&task_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "task not found"))?;

    if task.status != "running" {
        return Err(json_error(
            StatusCode::CONFLICT,
            &format!("task status is '{}', expected 'running'", task.status),
        ));
    }

    // Write a signal file that the desktop worker picks up to execute the task.
    let signal_path = crate::paths::Paths::task_execution_signal(&task_id);
    if let Some(parent) = signal_path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let _ = tokio::fs::write(&signal_path, &task_id).await;

    // Also touch the main task signal so the watcher wakes up.
    crate::tasks::touch_signal_file().await;

    Ok(Json(
        json!({ "task_id": task_id, "status": "queued_for_execution" }),
    ))
}

async fn list_goals_handler(
    Query(q): Query<ListGoalsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let goals = crate::goals::list_goals(&q.workspace_id, q.status.as_deref(), q.limit)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    Ok(Json(json!(goals)))
}

async fn create_goal_handler(
    Json(body): Json<CreateGoalBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let goal = crate::goals::create_goal(
        &body.workspace_id,
        &body.title,
        &body.objective,
        &body.priority,
        &body.owner,
        body.budget_json,
        body.policy_json,
    )
    .await
    .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    Ok((StatusCode::CREATED, Json(json!(goal))))
}

async fn start_goal_handler(
    Path(goal_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let goal = crate::goals::get_goal(&goal_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, &format!("goal not found: {goal_id}")))?;
    let orchestrator = crate::goal_orchestrator::GoalOrchestrator::new(
        crate::goal_orchestrator::OrchestratorConfig {
            workspace_id: goal.workspace_id.clone(),
            ..Default::default()
        },
    );
    orchestrator.start(&goal_id).await.map_err(|e| {
        let msg = e.to_string();
        json_error(StatusCode::CONFLICT, &msg)
    })?;
    let goal = crate::goals::get_goal(&goal_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, &format!("goal not found: {goal_id}")))?;

    Ok(Json(json!(goal)))
}

async fn pause_goal_handler(
    Path(goal_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let goal = crate::goals::get_goal(&goal_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, &format!("goal not found: {goal_id}")))?;
    let orchestrator = crate::goal_orchestrator::GoalOrchestrator::new(
        crate::goal_orchestrator::OrchestratorConfig {
            workspace_id: goal.workspace_id.clone(),
            ..Default::default()
        },
    );
    orchestrator.pause(&goal_id).await.map_err(|e| {
        let msg = e.to_string();
        json_error(StatusCode::CONFLICT, &msg)
    })?;
    let goal = crate::goals::get_goal(&goal_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, &format!("goal not found: {goal_id}")))?;

    Ok(Json(json!(goal)))
}

async fn cancel_goal_handler(
    Path(goal_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let goal = crate::goals::get_goal(&goal_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, &format!("goal not found: {goal_id}")))?;
    let orchestrator = crate::goal_orchestrator::GoalOrchestrator::new(
        crate::goal_orchestrator::OrchestratorConfig {
            workspace_id: goal.workspace_id.clone(),
            ..Default::default()
        },
    );
    orchestrator.cancel(&goal_id).await.map_err(|e| {
        let msg = e.to_string();
        json_error(StatusCode::CONFLICT, &msg)
    })?;
    let goal = crate::goals::get_goal(&goal_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, &format!("goal not found: {goal_id}")))?;

    Ok(Json(json!(goal)))
}

async fn approve_plan_handler(
    Path(goal_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    crate::goal_orchestrator::approve_goal_plan(&goal_id)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                json_error(StatusCode::NOT_FOUND, &msg)
            } else {
                json_error(StatusCode::CONFLICT, &msg)
            }
        })?;
    let goal = crate::goals::get_goal(&goal_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, &format!("goal not found: {goal_id}")))?;

    Ok(Json(json!(goal)))
}

async fn review_verdict_handler(
    Path(goal_id): Path<String>,
    Json(body): Json<ReviewVerdictBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let accepted = match body.verdict.as_str() {
        "accepted" => true,
        "rework_required" => false,
        other => {
            return Err(json_error(
                StatusCode::BAD_REQUEST,
                &format!("unsupported review verdict: {other}"),
            ))
        }
    };
    crate::goal_orchestrator::apply_goal_review_verdict(&goal_id, accepted)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                json_error(StatusCode::NOT_FOUND, &msg)
            } else {
                json_error(StatusCode::CONFLICT, &msg)
            }
        })?;
    let goal = crate::goals::get_goal(&goal_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, &format!("goal not found: {goal_id}")))?;

    Ok(Json(json!(goal)))
}

async fn list_cycles_handler(
    Path(goal_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cycles = crate::goals::list_cycles_by_goal(&goal_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    Ok(Json(json!(cycles)))
}

async fn get_cycle_handler(
    Path((goal_id, cycle_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let cycle = crate::goals::get_cycle(&cycle_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        .ok_or_else(|| {
            json_error(
                StatusCode::NOT_FOUND,
                &format!("cycle not found: {cycle_id}"),
            )
        })?;

    if cycle.goal_id != goal_id {
        return Err(json_error(
            StatusCode::NOT_FOUND,
            &format!("cycle {cycle_id} does not belong to goal {goal_id}"),
        ));
    }

    Ok(Json(json!(cycle)))
}

// ---------------------------------------------------------------------------
// Permission handlers
// ---------------------------------------------------------------------------

async fn request_permission_handler(
    Json(body): Json<RequestPermissionBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let risk_level = crate::proposals::RiskLevel::from_str(&body.risk_level)
        .map_err(|e| json_error(StatusCode::BAD_REQUEST, &e.to_string()))?;

    let scope = match body.scope {
        Some(s) => {
            let max_risk = crate::proposals::RiskLevel::from_str(&s.max_risk_level)
                .map_err(|e| json_error(StatusCode::BAD_REQUEST, &e.to_string()))?;
            crate::permissions::WorkspaceScope {
                workspace_ids: s.workspace_ids,
                tool_prefixes: s.tool_prefixes,
                max_risk_level: max_risk,
            }
        }
        None => crate::permissions::WorkspaceScope::unrestricted(),
    };

    let id = crate::permissions::next_id()
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    let grant = crate::permissions::request(
        id,
        body.workspace_id,
        body.tool_id,
        risk_level,
        body.grantee,
        scope,
        None,
    )
    .await
    .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    Ok((StatusCode::CREATED, Json(json!(grant))))
}

async fn approve_permission_handler(
    Path(request_id): Path<String>,
    Json(body): Json<ApprovePermissionBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mode = body.mode.as_deref().unwrap_or("once");

    match mode {
        "once" => {
            crate::permissions::approve_once(&request_id)
                .await
                .map_err(|e| {
                    let msg = e.to_string();
                    if msg.contains("not found") {
                        json_error(StatusCode::NOT_FOUND, &msg)
                    } else {
                        json_error(StatusCode::CONFLICT, &msg)
                    }
                })?;
        }
        "session" => {
            crate::permissions::approve_session(&request_id)
                .await
                .map_err(|e| {
                    let msg = e.to_string();
                    if msg.contains("not found") {
                        json_error(StatusCode::NOT_FOUND, &msg)
                    } else {
                        json_error(StatusCode::CONFLICT, &msg)
                    }
                })?;
        }
        other => {
            return Err(json_error(
                StatusCode::BAD_REQUEST,
                &format!("unknown approval mode: {other}"),
            ));
        }
    }

    let grant = crate::permissions::get(&request_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    Ok(Json(json!(grant)))
}

async fn deny_permission_handler(
    Path(request_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    crate::permissions::deny(&request_id).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("not found") {
            json_error(StatusCode::NOT_FOUND, &msg)
        } else {
            json_error(StatusCode::CONFLICT, &msg)
        }
    })?;

    let grant = crate::permissions::get(&request_id)
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    Ok(Json(json!(grant)))
}

// ---------------------------------------------------------------------------
// SSE event stream handler
// ---------------------------------------------------------------------------

/// SSE endpoint: `GET /runtime/events?workspace_id=...&since_event_id=...`
///
/// On connection, replays any events stored after `since_event_id` (if provided),
/// then streams real-time events from the broadcast channel.
async fn events_sse_handler(
    Query(params): Query<EventsQuery>,
    State(state): State<SseState>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let workspace_id = params.workspace_id.clone();
    let since_event_id = params.since_event_id.clone();

    // Build the historical replay stream (may be empty).
    let historical: Vec<Result<axum::response::sse::Event, Infallible>> =
        match crate::events::query_events_db(&workspace_id, since_event_id.as_deref(), None).await
        {
            Ok(events) => events
                .into_iter()
                .map(|ev| {
                    let data = serde_json::json!({
                        "id": uuid::Uuid::new_v4().to_string(),
                        "workspace_id": workspace_id,
                        "event_type": ev.event_type,
                        "subject_type": ev.detail.get("subject_type").and_then(|v| v.as_str()).unwrap_or(""),
                        "subject_id": ev.detail.get("subject_id").and_then(|v| v.as_str()).unwrap_or(""),
                        "actor_id": ev.actor,
                        "payload": ev.detail,
                        "created_at": ev.timestamp.to_rfc3339(),
                    });
                    Ok(axum::response::sse::Event::default().data(data.to_string()))
                })
                .collect(),
            Err(e) => {
                tracing::warn!("SSE replay query failed: {e:#}");
                Vec::new()
            }
        };

    let historical_stream = tokio_stream::iter(historical);

    // Real-time stream from broadcast channel.
    let rx = state.tx.subscribe();
    let live_stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(ev) => {
            let data = serde_json::json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "workspace_id": ev.detail.get("workspace_id").and_then(|v| v.as_str()).unwrap_or("default"),
                "event_type": ev.event_type,
                "subject_type": ev.detail.get("subject_type").and_then(|v| v.as_str()).unwrap_or(""),
                "subject_id": ev.detail.get("subject_id").and_then(|v| v.as_str()).unwrap_or(""),
                "actor_id": ev.actor,
                "payload": ev.detail,
                "created_at": ev.timestamp.to_rfc3339(),
            });
            Some(Ok(axum::response::sse::Event::default().data(data.to_string())))
        }
        Err(_) => None, // lagged — skip
    });

    let stream = historical_stream.chain(live_stream);
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

/// Generate a random 32-character hex token for runtime API authentication.
pub fn generate_runtime_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    // Use a simple hash-like mixing of nanoseconds + thread id to produce 32 hex chars.
    // This is not cryptographically secure but sufficient for a local-only token.
    let tid = std::thread::current().id();
    let tid_num = format!("{tid:?}")
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<u128>()
        .unwrap_or(0);
    let mixed = nanos
        .wrapping_mul(6364136223846793005)
        .wrapping_add(tid_num);
    format!("{:032x}", mixed)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_endpoint_returns_ok() {
        let token = "test-token-123";
        let mut server = RuntimeApiServer::new("127.0.0.1", 0, token);
        server.start().await.expect("server start failed");
        let port = server.local_addr().unwrap().port();

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{port}/runtime/health"))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("request failed");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json parse");
        assert_eq!(body["status"], "ok");

        server.stop();
    }

    #[tokio::test]
    async fn missing_token_returns_401() {
        let mut server = RuntimeApiServer::new("127.0.0.1", 0, "secret");
        server.start().await.expect("server start failed");
        let port = server.local_addr().unwrap().port();

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{port}/runtime/health"))
            .send()
            .await
            .expect("request failed");

        assert_eq!(resp.status(), 401);
        server.stop();
    }

    #[tokio::test]
    async fn wrong_token_returns_401() {
        let mut server = RuntimeApiServer::new("127.0.0.1", 0, "correct-token");
        server.start().await.expect("server start failed");
        let port = server.local_addr().unwrap().port();

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{port}/runtime/health"))
            .header("Authorization", "Bearer wrong-token")
            .send()
            .await
            .expect("request failed");

        assert_eq!(resp.status(), 401);
        server.stop();
    }

    #[test]
    fn generate_token_is_unique() {
        let a = generate_runtime_token();
        let b = generate_runtime_token();
        assert_eq!(a.len(), 32, "token should be 32 hex chars");
        assert_eq!(b.len(), 32);
        assert_ne!(a, b, "sequential tokens must differ");
    }

    // -- Test helper -----------------------------------------------------------

    use crate::test_support::TestRoot;

    /// Start the server on a random port with a fresh DB. Returns (base_url, token, _root).
    async fn start_test_server() -> (String, String, TestRoot) {
        let root = TestRoot::new();
        let token = "test-token-rest";
        let mut server = RuntimeApiServer::new("127.0.0.1", 0, token);
        server.start().await.expect("server start failed");
        let port = server.local_addr().unwrap().port();

        (format!("http://127.0.0.1:{port}"), token.to_string(), root)
    }

    fn auth(token: &str) -> String {
        format!("Bearer {token}")
    }

    // -- Messages tests --------------------------------------------------------

    #[tokio::test]
    async fn post_message_returns_201() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/messages"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-api-1",
                "sender_id": "agent-a",
                "topic": "status",
                "kind": "message",
                "content": "hello from api"
            }))
            .send()
            .await
            .expect("request");

        assert_eq!(resp.status(), 201);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["workspace_id"], "ws-api-1");
        assert_eq!(body["sender_id"], "agent-a");
        assert_eq!(body["content"], "hello from api");
        assert!(body["id"].as_str().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn get_messages_with_topic_filter() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        // Post two messages with different topics.
        for (topic, content) in [("logs", "log entry"), ("alerts", "alert!")] {
            client
                .post(format!("{base}/runtime/messages"))
                .header("Authorization", auth(&token))
                .json(&json!({
                    "workspace_id": "ws-api-2",
                    "sender_id": "agent-a",
                    "topic": topic,
                    "kind": "message",
                    "content": content
                }))
                .send()
                .await
                .expect("post");
        }

        // GET with topic=logs should return only the log message.
        let resp = client
            .get(format!(
                "{base}/runtime/messages?workspace_id=ws-api-2&topic=logs"
            ))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("get");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        let msgs = body.as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["topic"], "logs");
        assert_eq!(msgs[0]["content"], "log entry");
    }

    // -- Heartbeat tests -------------------------------------------------------

    #[tokio::test]
    async fn post_heartbeat_returns_200() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/heartbeats"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-hb-1",
                "agent_id": "agent-hb",
                "status": "working",
                "active_tool_count": 2,
                "ttl_seconds": 300
            }))
            .send()
            .await
            .expect("request");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["workspace_id"], "ws-hb-1");
        assert_eq!(body["agent_id"], "agent-hb");
        assert_eq!(body["status"], "working");
        assert_eq!(body["active_tool_count"], 2);
    }

    // -- Task tests ------------------------------------------------------------

    #[tokio::test]
    async fn claim_task_success() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        // Create a task directly via the module.
        let task = crate::goal_tasks::create_task(
            "ws-claim-api",
            None,
            None,
            "Claimable via API",
            "Do something",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        let resp = client
            .post(format!("{base}/runtime/tasks/{}/claim", task.id))
            .header("Authorization", auth(&token))
            .json(&json!({"agent_id": "agent-api", "lease_ttl_seconds": 600}))
            .send()
            .await
            .expect("request");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "claimed");
        assert_eq!(body["claimed_by"], "agent-api");
    }

    #[tokio::test]
    async fn claim_task_conflict_returns_409() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let task = crate::goal_tasks::create_task(
            "ws-conflict-api",
            None,
            None,
            "Contested via API",
            "Only one can claim",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        // First claim succeeds.
        crate::goal_tasks::claim_task(&task.id, "agent-first", 3600)
            .await
            .expect("first claim");

        // Second claim via API should return 409.
        let resp = client
            .post(format!("{base}/runtime/tasks/{}/claim", task.id))
            .header("Authorization", auth(&token))
            .json(&json!({"agent_id": "agent-second", "lease_ttl_seconds": 600}))
            .send()
            .await
            .expect("request");

        assert_eq!(resp.status(), 409);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert!(
            body["error"].as_str().unwrap().contains("conflict")
                || body["error"]
                    .as_str()
                    .unwrap()
                    .contains("invalid task transition")
        );
    }

    #[tokio::test]
    async fn complete_task_success() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let task = crate::goal_tasks::create_task(
            "ws-complete-api",
            None,
            None,
            "Completable via API",
            "Finish it",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        crate::goal_tasks::claim_task(&task.id, "agent-a", 3600)
            .await
            .expect("claim");
        crate::goal_tasks::start_task(&task.id)
            .await
            .expect("start");

        let resp = client
            .post(format!("{base}/runtime/tasks/{}/complete", task.id))
            .header("Authorization", auth(&token))
            .json(&json!({"result_ref": "output/ref-api"}))
            .send()
            .await
            .expect("request");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "accepted");
        assert_eq!(body["result_ref"], "output/ref-api");
    }

    #[tokio::test]
    async fn fail_task_success() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let task = crate::goal_tasks::create_task(
            "ws-fail-api",
            None,
            None,
            "Failable via API",
            "This will fail",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        crate::goal_tasks::claim_task(&task.id, "agent-a", 3600)
            .await
            .expect("claim");
        crate::goal_tasks::start_task(&task.id)
            .await
            .expect("start");

        let resp = client
            .post(format!("{base}/runtime/tasks/{}/fail", task.id))
            .header("Authorization", auth(&token))
            .json(&json!({"error": "timeout via API"}))
            .send()
            .await
            .expect("request");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "failed");
        assert_eq!(body["error"], "timeout via API");
    }

    #[tokio::test]
    async fn block_task_success() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let task = crate::goal_tasks::create_task(
            "ws-block-api",
            None,
            None,
            "Blockable via API",
            "Might get blocked",
            "claude_p",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("create");

        crate::goal_tasks::claim_task(&task.id, "agent-a", 3600)
            .await
            .expect("claim");
        crate::goal_tasks::start_task(&task.id)
            .await
            .expect("start");

        let resp = client
            .post(format!("{base}/runtime/tasks/{}/block", task.id))
            .header("Authorization", auth(&token))
            .json(&json!({"reason": "waiting for approval"}))
            .send()
            .await
            .expect("request");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "blocked");
        assert_eq!(body["error"], "waiting for approval");
    }

    // -- SSE event stream tests ------------------------------------------------

    use crate::events::AuditEvent;
    use chrono::Utc;

    #[tokio::test]
    async fn broadcast_channel_send_receive() {
        // Direct test of the broadcast channel without HTTP layer.
        let server = RuntimeApiServer::new("127.0.0.1", 0, "tok");
        let tx = server.event_sender();
        let mut rx = tx.subscribe();

        let event = AuditEvent {
            timestamp: Utc::now(),
            source: "test".into(),
            event_type: "broadcast.test".into(),
            actor: "tester".into(),
            target: "channel".into(),
            detail: serde_json::json!({"key": "value"}),
            session_id: None,
        };

        tx.send(event.clone()).expect("send");

        let received = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout waiting for broadcast")
            .expect("recv");

        assert_eq!(received.event_type, "broadcast.test");
        assert_eq!(received.actor, "tester");
    }

    #[tokio::test]
    async fn broadcast_channel_multiple_receivers() {
        let server = RuntimeApiServer::new("127.0.0.1", 0, "tok");
        let tx = server.event_sender();
        let mut rx1 = tx.subscribe();
        let mut rx2 = tx.subscribe();

        let event = AuditEvent {
            timestamp: Utc::now(),
            source: "test".into(),
            event_type: "multi.test".into(),
            actor: "tester".into(),
            target: "channel".into(),
            detail: serde_json::json!({}),
            session_id: None,
        };

        tx.send(event).expect("send");

        let r1 = tokio::time::timeout(std::time::Duration::from_secs(2), rx1.recv())
            .await
            .expect("timeout rx1")
            .expect("recv rx1");

        let r2 = tokio::time::timeout(std::time::Duration::from_secs(2), rx2.recv())
            .await
            .expect("timeout rx2")
            .expect("recv rx2");

        assert_eq!(r1.event_type, "multi.test");
        assert_eq!(r2.event_type, "multi.test");
    }

    #[tokio::test]
    async fn sse_endpoint_returns_event_stream() {
        let _root = TestRoot::new();
        let token = "sse-test-token";
        let mut server = RuntimeApiServer::new("127.0.0.1", 0, token);
        server.start().await.expect("server start failed");
        let port = server.local_addr().unwrap().port();

        let client = reqwest::Client::new();
        let resp = client
            .get(format!(
                "http://127.0.0.1:{port}/runtime/events?workspace_id=ws-sse-1"
            ))
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "text/event-stream")
            .send()
            .await
            .expect("SSE request");

        assert_eq!(resp.status(), 200);
        let content_type = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            content_type.contains("text/event-stream"),
            "expected SSE content type, got: {content_type}"
        );

        server.stop();
    }

    #[tokio::test]
    async fn sse_receives_realtime_event() {
        let _root = TestRoot::new();
        let token = "sse-rt-token";
        let mut server = RuntimeApiServer::new("127.0.0.1", 0, token);
        let tx = server.event_sender();
        server.start().await.expect("server start failed");
        let port = server.local_addr().unwrap().port();

        // Connect to SSE stream.
        let client = reqwest::Client::new();
        let resp = client
            .get(format!(
                "http://127.0.0.1:{port}/runtime/events?workspace_id=ws-sse-rt"
            ))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("SSE request");

        assert_eq!(resp.status(), 200);

        // Spawn a task to send an event after a short delay.
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let event = AuditEvent {
                timestamp: Utc::now(),
                source: "test".into(),
                event_type: "sse.realtime".into(),
                actor: "tester".into(),
                target: "sse-client".into(),
                detail: serde_json::json!({"workspace_id": "ws-sse-rt", "hello": "world"}),
                session_id: None,
            };
            let _ = tx_clone.send(event);
        });

        // Read the response body as a stream with timeout.
        let body = resp.bytes_stream();
        use tokio_stream::StreamExt as _;
        let mut stream = body;

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut found = false;

        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(std::time::Duration::from_secs(1), stream.next()).await {
                Ok(Some(Ok(chunk))) => {
                    let text = String::from_utf8_lossy(&chunk);
                    if text.contains("sse.realtime") {
                        found = true;
                        break;
                    }
                }
                Ok(Some(Err(_))) => break,
                Ok(None) => break,
                Err(_) => continue, // timeout on this chunk, keep trying
            }
        }

        assert!(
            found,
            "should have received sse.realtime event within timeout"
        );
        server.stop();
    }

    #[tokio::test]
    async fn sse_replays_missed_events() {
        let _root = TestRoot::new();
        let token = "sse-replay-token";
        let mut server = RuntimeApiServer::new("127.0.0.1", 0, token);
        server.start().await.expect("server start failed");
        let port = server.local_addr().unwrap().port();

        // Write events to SQLite before connecting.
        let ev1 = AuditEvent {
            timestamp: "2026-01-01T00:00:00Z".parse().unwrap(),
            source: "test".into(),
            event_type: "sse.before".into(),
            actor: "agent".into(),
            target: "res-1".into(),
            detail: serde_json::json!({"workspace_id": "ws-sse-replay", "n": 1}),
            session_id: None,
        };
        crate::events::append_to_db(&ev1).await.expect("write ev1");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Grab the id of ev1 to use as since_event_id.
        let pool = crate::db::pool().await.unwrap();
        let first_id: String = sqlx::query_scalar(
            "SELECT id FROM runtime_events WHERE event_type = 'sse.before' LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .expect("get first id");

        let ev2 = AuditEvent {
            timestamp: "2026-01-02T00:00:00Z".parse().unwrap(),
            source: "test".into(),
            event_type: "sse.after".into(),
            actor: "agent".into(),
            target: "res-2".into(),
            detail: serde_json::json!({"workspace_id": "ws-sse-replay", "n": 2}),
            session_id: None,
        };
        crate::events::append_to_db(&ev2).await.expect("write ev2");

        // Connect with since_event_id = first_id to get only ev2 in replay.
        let client = reqwest::Client::new();
        let resp = client
            .get(format!(
                "http://127.0.0.1:{port}/runtime/events?workspace_id=ws-sse-replay&since_event_id={first_id}"
            ))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .expect("SSE request");

        assert_eq!(resp.status(), 200);

        // Read the stream — should see sse.after in the replay.
        let body = resp.bytes_stream();
        use tokio_stream::StreamExt as _;
        let mut stream = body;

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut found = false;

        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(std::time::Duration::from_secs(1), stream.next()).await {
                Ok(Some(Ok(chunk))) => {
                    let text = String::from_utf8_lossy(&chunk);
                    if text.contains("sse.after") {
                        found = true;
                        break;
                    }
                    // Also stop if we see sse.before (should NOT happen).
                    if text.contains("sse.before") {
                        panic!("should NOT have received sse.before event (it was before since_event_id)");
                    }
                }
                Ok(Some(Err(_))) => break,
                Ok(None) => break,
                Err(_) => continue,
            }
        }

        assert!(
            found,
            "should have received sse.after replay event within timeout"
        );
        server.stop();
    }

    // -- Goal API tests --------------------------------------------------------

    #[tokio::test]
    async fn create_goal_returns_201() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/goals"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-goal-1",
                "title": "Ship v1.0",
                "objective": "Release first version",
                "priority": "p1",
                "owner": "user"
            }))
            .send()
            .await
            .expect("request");

        assert_eq!(resp.status(), 201);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["workspace_id"], "ws-goal-1");
        assert_eq!(body["title"], "Ship v1.0");
        assert_eq!(body["status"], "draft");
        assert!(body["id"].as_str().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn list_goals_returns_all() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        // Create two goals.
        for title in ["Goal A", "Goal B"] {
            client
                .post(format!("{base}/runtime/goals"))
                .header("Authorization", auth(&token))
                .json(&json!({
                    "workspace_id": "ws-goal-list",
                    "title": title,
                    "objective": "obj",
                    "priority": "p1",
                    "owner": "user"
                }))
                .send()
                .await
                .expect("post");
        }

        let resp = client
            .get(format!("{base}/runtime/goals?workspace_id=ws-goal-list"))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("get");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        let goals = body.as_array().unwrap();
        assert_eq!(goals.len(), 2);
    }

    #[tokio::test]
    async fn list_goals_with_status_filter() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        // Create two goals.
        let resp1 = client
            .post(format!("{base}/runtime/goals"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-goal-filter",
                "title": "Goal 1",
                "objective": "obj",
                "priority": "p1",
                "owner": "user"
            }))
            .send()
            .await
            .expect("post");
        let g1: serde_json::Value = resp1.json().await.expect("json");

        client
            .post(format!("{base}/runtime/goals"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-goal-filter",
                "title": "Goal 2",
                "objective": "obj",
                "priority": "p2",
                "owner": "user"
            }))
            .send()
            .await
            .expect("post");

        // Move g1 to planning.
        client
            .post(format!("{base}/runtime/goals/{}/start", g1["id"]))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("start");

        // Filter by draft.
        let resp = client
            .get(format!(
                "{base}/runtime/goals?workspace_id=ws-goal-filter&status=draft"
            ))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("get");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        let goals = body.as_array().unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0]["title"], "Goal 2");
    }

    #[tokio::test]
    async fn start_goal_transitions_to_planning() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/goals"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-goal-start",
                "title": "Startable",
                "objective": "obj",
                "priority": "p1",
                "owner": "user"
            }))
            .send()
            .await
            .expect("create");
        let goal: serde_json::Value = resp.json().await.expect("json");

        let resp = client
            .post(format!("{base}/runtime/goals/{}/start", goal["id"]))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("start");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "planning");
    }

    #[tokio::test]
    async fn start_goal_not_found_returns_404() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/goals/nonexistent-goal/start"))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("request");

        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn approve_plan_transitions_to_running() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/goals"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-goal-approve",
                "title": "Approvable",
                "objective": "obj",
                "priority": "p1",
                "owner": "user"
            }))
            .send()
            .await
            .expect("create");
        let goal: serde_json::Value = resp.json().await.expect("json");
        let goal_id = goal["id"].as_str().unwrap();

        // draft -> planning -> awaiting_plan_approval -> running
        client
            .post(format!("{base}/runtime/goals/{goal_id}/start"))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("start");

        // Advance to awaiting_plan_approval via goals module.
        crate::goals::update_goal_status(goal_id, "awaiting_plan_approval")
            .await
            .expect("to awaiting_plan_approval");

        let resp = client
            .post(format!("{base}/runtime/goals/{goal_id}/approve-plan"))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("approve");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "running");
    }

    #[tokio::test]
    async fn review_verdict_accepted() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/goals"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-goal-review",
                "title": "Reviewable",
                "objective": "obj",
                "priority": "p1",
                "owner": "user"
            }))
            .send()
            .await
            .expect("create");
        let goal: serde_json::Value = resp.json().await.expect("json");
        let goal_id = goal["id"].as_str().unwrap();

        // Walk to awaiting_review.
        crate::goals::update_goal_status(goal_id, "planning")
            .await
            .unwrap();
        crate::goals::update_goal_status(goal_id, "awaiting_plan_approval")
            .await
            .unwrap();
        crate::goals::update_goal_status(goal_id, "running")
            .await
            .unwrap();
        crate::goals::update_goal_status(goal_id, "awaiting_review")
            .await
            .unwrap();

        let resp = client
            .post(format!("{base}/runtime/goals/{goal_id}/review-verdict"))
            .header("Authorization", auth(&token))
            .json(&json!({"verdict": "accepted"}))
            .send()
            .await
            .expect("review");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "accepted");
    }

    #[tokio::test]
    async fn review_verdict_conflict_returns_409() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/goals"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-goal-review-conflict",
                "title": "Conflict",
                "objective": "obj",
                "priority": "p1",
                "owner": "user"
            }))
            .send()
            .await
            .expect("create");
        let goal: serde_json::Value = resp.json().await.expect("json");
        let goal_id = goal["id"].as_str().unwrap();

        // draft -> accepted is invalid.
        let resp = client
            .post(format!("{base}/runtime/goals/{goal_id}/review-verdict"))
            .header("Authorization", auth(&token))
            .json(&json!({"verdict": "accepted"}))
            .send()
            .await
            .expect("review");

        assert_eq!(resp.status(), 409);
    }

    #[tokio::test]
    async fn list_cycles_returns_cycles() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/goals"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-goal-cycles",
                "title": "Has Cycles",
                "objective": "obj",
                "priority": "p1",
                "owner": "user"
            }))
            .send()
            .await
            .expect("create");
        let goal: serde_json::Value = resp.json().await.expect("json");
        let goal_id = goal["id"].as_str().unwrap();

        // Create cycles via module.
        crate::goals::create_cycle(goal_id, 1)
            .await
            .expect("cycle 1");
        crate::goals::create_cycle(goal_id, 2)
            .await
            .expect("cycle 2");

        let resp = client
            .get(format!("{base}/runtime/goals/{goal_id}/cycles"))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("get");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        let cycles = body.as_array().unwrap();
        assert_eq!(cycles.len(), 2);
    }

    #[tokio::test]
    async fn get_cycle_detail() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/goals"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-goal-cycle-detail",
                "title": "Cycle Detail",
                "objective": "obj",
                "priority": "p1",
                "owner": "user"
            }))
            .send()
            .await
            .expect("create");
        let goal: serde_json::Value = resp.json().await.expect("json");
        let goal_id = goal["id"].as_str().unwrap();

        let cycle = crate::goals::create_cycle(goal_id, 1).await.expect("cycle");

        let resp = client
            .get(format!(
                "{base}/runtime/goals/{goal_id}/cycles/{}",
                cycle.id
            ))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("get");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["goal_id"], goal_id);
        assert_eq!(body["status"], "observing");
    }

    #[tokio::test]
    async fn get_cycle_wrong_goal_returns_404() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        // Create two goals.
        let resp1 = client
            .post(format!("{base}/runtime/goals"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-goal-cycle-mismatch",
                "title": "Goal 1",
                "objective": "obj",
                "priority": "p1",
                "owner": "user"
            }))
            .send()
            .await
            .expect("create");
        let g1: serde_json::Value = resp1.json().await.expect("json");

        let resp2 = client
            .post(format!("{base}/runtime/goals"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-goal-cycle-mismatch",
                "title": "Goal 2",
                "objective": "obj",
                "priority": "p1",
                "owner": "user"
            }))
            .send()
            .await
            .expect("create");
        let g2: serde_json::Value = resp2.json().await.expect("json");

        // Create cycle under g1.
        let cycle = crate::goals::create_cycle(g1["id"].as_str().unwrap(), 1)
            .await
            .expect("cycle");

        // Try to fetch cycle under g2 — should be 404.
        let resp = client
            .get(format!(
                "{base}/runtime/goals/{}/cycles/{}",
                g2["id"], cycle.id
            ))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("get");

        assert_eq!(resp.status(), 404);
    }

    // -- Permission API tests --------------------------------------------------

    #[tokio::test]
    async fn request_permission_returns_201() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/permission-requests"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "tool_id": "file.write",
                "risk_level": "workspace_write",
                "grantee": "agent-api"
            }))
            .send()
            .await
            .expect("request");

        assert_eq!(resp.status(), 201);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["tool_id"], "file.write");
        assert_eq!(body["status"], "requested");
        assert_eq!(body["grantee"], "agent-api");
    }

    #[tokio::test]
    async fn request_permission_bad_risk_level_returns_400() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/permission-requests"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "tool_id": "file.write",
                "risk_level": "nonexistent",
                "grantee": "agent-api"
            }))
            .send()
            .await
            .expect("request");

        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn approve_permission_once() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        // Create a permission request.
        let resp = client
            .post(format!("{base}/runtime/permission-requests"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "tool_id": "file.write",
                "risk_level": "workspace_write",
                "grantee": "agent-approve"
            }))
            .send()
            .await
            .expect("request");
        let perm: serde_json::Value = resp.json().await.expect("json");
        let perm_id = perm["id"].as_str().unwrap();

        let resp = client
            .post(format!("{base}/runtime/permissions/{perm_id}/approve"))
            .header("Authorization", auth(&token))
            .json(&json!({"mode": "once"}))
            .send()
            .await
            .expect("approve");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "approved_once");
    }

    #[tokio::test]
    async fn approve_permission_session() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/permission-requests"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "tool_id": "shell.exec",
                "risk_level": "external_side_effect",
                "grantee": "agent-session"
            }))
            .send()
            .await
            .expect("request");
        let perm: serde_json::Value = resp.json().await.expect("json");
        let perm_id = perm["id"].as_str().unwrap();

        let resp = client
            .post(format!("{base}/runtime/permissions/{perm_id}/approve"))
            .header("Authorization", auth(&token))
            .json(&json!({"mode": "session"}))
            .send()
            .await
            .expect("approve");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "approved_session");
    }

    #[tokio::test]
    async fn deny_permission() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/permission-requests"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "tool_id": "shell.rm",
                "risk_level": "workspace_write",
                "grantee": "agent-deny"
            }))
            .send()
            .await
            .expect("request");
        let perm: serde_json::Value = resp.json().await.expect("json");
        let perm_id = perm["id"].as_str().unwrap();

        let resp = client
            .post(format!("{base}/runtime/permissions/{perm_id}/deny"))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("deny");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "denied");
    }

    #[tokio::test]
    async fn approve_permission_not_found_returns_404() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!(
                "{base}/runtime/permissions/nonexistent-perm/approve"
            ))
            .header("Authorization", auth(&token))
            .json(&json!({}))
            .send()
            .await
            .expect("request");

        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn cancel_goal_transitions_to_cancelled() {
        let (base, token, _root) = start_test_server().await;
        let client = reqwest::Client::new();

        let resp = client
            .post(format!("{base}/runtime/goals"))
            .header("Authorization", auth(&token))
            .json(&json!({
                "workspace_id": "ws-goal-cancel",
                "title": "Cancellable",
                "objective": "obj",
                "priority": "p1",
                "owner": "user"
            }))
            .send()
            .await
            .expect("create");
        let goal: serde_json::Value = resp.json().await.expect("json");
        let goal_id = goal["id"].as_str().unwrap();

        // Walk to running.
        crate::goals::update_goal_status(goal_id, "planning")
            .await
            .unwrap();
        crate::goals::update_goal_status(goal_id, "awaiting_plan_approval")
            .await
            .unwrap();
        crate::goals::update_goal_status(goal_id, "running")
            .await
            .unwrap();

        let resp = client
            .post(format!("{base}/runtime/goals/{goal_id}/cancel"))
            .header("Authorization", auth(&token))
            .send()
            .await
            .expect("cancel");

        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "cancelled");
    }
}
