# UnifiedFinalEvolution Goal Task Flow Gap Dispatch

> Date: 2026-06-05
> Scope: latest two goal sessions / GoalTask chat executor / AgentTeam lifecycle / ChatTurn anchors
> Baseline: `docs/UnifiedFinalEvolution-20260604.md`
> Status: dispatch-ready

## 1. Executive Conclusion

最新两次 goal 会话从用户视角“跑完了”，但实际只打通了较短路径：

```text
Goal -> AgentTask -> execute_goal_task_via_chat -> ChatMessageProjection -> AgentTask accepted
```

还没有打通目标架构要求的统一闭环：

```text
Goal / GoalCycle / AgentTask
  -> ChatTurn(anchor: goal_id, goal_cycle_id, agent_task_id)
  -> chat_turn_events
  -> ToolCall / AgentRun / AgentTeamMember
  -> L2 read model
  -> L3 UI projection
```

因此这不是单纯的前端感知不足。结果投影已存在，但关键锚点缺失，导致 Goal、Team、Turn、Run、Event 五条线无法可靠合流。

## 2. Evidence Snapshot

### 2.1 Latest Goal Sessions

| Goal | Time (UTC) | Result |
|---|---:|---|
| `goal-3b68280f-729c-4431-906c-34457ac550b3` | 2026-06-05 09:02-09:07 | 4 tasks accepted |
| `goal-da01f2c1-9548-42e8-968f-fca8fc224c88` | 2026-06-05 08:58-09:01 | 1 task accepted |

### 2.2 Confirmed Data Shape

| Layer | Observed | Interpretation |
|---|---|---|
| `goal_runs` / `goal_cycles` | status completed/accepted | Goal lifecycle can finish |
| `agent_tasks` | status accepted, `result_ref=chat:<message_id>` | Task result comes from chat message |
| `chat_messages` | assistant messages exist, `run_id=null` | Result was not produced as AgentRun output |
| `chat_message_projections` | visible finalized projections exist | User-visible projection exists |
| `chat_turns` | all `goal-task-%` rows have `goal_id=null`, `goal_cycle_id=null`, `agent_task_id=null` | ChatTurn anchor missing |
| `agent_team_members` | `status=active`, `run_id=null` | Member state machine is not connected to chat executor |
| `agent_runs` | latest related runs are `failed`, `claude timed out` | `agent.start` can fail while parent chat recovers |

Global check:

```text
chat_turns where request_id like 'goal-task-%':
total=8, with_goal=0, with_task=0
```

## 3. Root Cause

`apps/desktop/src-tauri/src/worker.rs::execute_goal_task_via_chat` executes goal tasks through a throwaway chat session and projects results back into the goal session. This is valid for UX isolation, but it does not pass typed goal/task/cycle context into `send_message_v2_with_session_projection`.

Current creation point:

- `crates/conductor-core/src/chat/send_v2.rs:956` accepts `request_id_override`.
- `crates/conductor-core/src/chat/send_v2.rs:1011-1013` writes `goal_cycle_id: None`, `agent_task_id: None`, `goal_id: None`.
- `apps/desktop/src-tauri/src/worker.rs:1321` encodes task identity only in string `goal-task-{task_id}-{timestamp}`.
- `apps/desktop/src-tauri/src/worker.rs:1349` calls `send_message_v2_with_session_projection(...)` without typed context.

`AgentRun` P0 fixes do not cover this path. They apply when an `AgentRun` with `metadata_json.task_id` finishes. The latest real task path is `GoalTask -> Chat executor`, not `GoalTask -> AgentRun natural finish`.

## 4. Dispatch Plan

### P0-A: Add Typed GoalTask Context To Chat Send API

Owner: backend runtime

Files:

- `crates/conductor-core/src/chat/send_v2.rs`
- `crates/conductor-core/src/chat/turns.rs`
- call sites in `apps/desktop/src-tauri/src/worker.rs`

Tasks:

1. Introduce a small context struct:

   ```rust
   pub struct ChatExecutionContext {
       pub goal_id: Option<String>,
       pub goal_cycle_id: Option<String>,
       pub agent_task_id: Option<String>,
   }
   ```

2. Add optional context to `send_message_v2_with_session_projection`.
3. Keep existing `send_message_v2_with_session` API behavior by passing `None` or default context.
4. Populate `ChatTurnCreate.goal_id / goal_cycle_id / agent_task_id` from context.
5. Update tests that construct `ChatTurnCreate`.

Acceptance:

- New `goal-task-%` turns have all three anchors populated.
- Existing non-goal chat turns still have null anchors.
- No parsing of `request_id` is needed for new rows.

### P0-B: Pass GoalTask Context From Worker

Owner: desktop backend worker

Files:

- `apps/desktop/src-tauri/src/worker.rs`

Tasks:

1. In `execute_goal_task_via_chat`, use `task.goal_id`, `task.cycle_id`, and `task.id`.
2. Pass the context into `send_message_v2_with_session_projection`.
3. Include the same IDs in the placeholder projection payload metadata if available.

Acceptance:

- `chat_turns.request_id = goal-task-*` row has:
  - `goal_id = task.goal_id`
  - `goal_cycle_id = task.cycle_id`
  - `agent_task_id = task.id`

### P0-C: Connect Chat Executor To AgentTeamMember State

Owner: backend runtime

Files:

- `apps/desktop/src-tauri/src/worker.rs`
- `crates/conductor-core/src/agent_teams.rs`

Tasks:

1. Add helper to find team member by `metadata_json.task_id`.
2. On goal task execution start:
   - set member status `running`
   - optionally write synthetic `run_id = chat:<turn_id>` after turn creation is known
3. On writeback:
   - `ReviewReady` -> member `completed`
   - `Blocked` -> member `paused` or equivalent non-terminal blocked status
   - execution failure -> member `stopped`
4. After each member update, check all members for terminal status and transition team:
   - all completed -> `AwaitingReview`
   - any failed/stopped -> `ReworkRequired` or leave for manual policy, but do not stay silently `Executing`

Acceptance:

- Latest goal team members no longer remain `active` after task completion.
- Team lifecycle is driven by member state, not only by `goal_tasks` polling.
- UI can explain why a team is still executing or ready for review.

### P0-D: Emit GoalTask-Specific Events

Owner: backend observability

Files:

- `crates/conductor-core/src/chat/turns.rs`
- `apps/desktop/src-tauri/src/worker.rs`
- `crates/conductor-core/src/events.rs` if helper functions are desired

Tasks:

1. Emit `goal_task.execution_started` with `goal_id`, `cycle_id`, `task_id`, `turn_id`, `request_id`.
2. Emit `goal_task.result_projected` with `message_id`, `projection_message_id`, `result_ref`.
3. Emit `goal_task.writeback_succeeded` or `goal_task.writeback_failed`.
4. Prefer `chat_turn_events` when a turn exists; otherwise emit project-level audit event.

Acceptance:

- A single task can be traced without joining on `request_id` string conventions.
- Goal history can show task execution boundaries and result projection.

### P1-A: Frontend GoalConsole Result Awareness

Owner: frontend

Files:

- `apps/desktop/src/windows/GoalConsole.tsx`
- `apps/desktop/src/ipc/invoke.ts`

Tasks:

1. Add result display per task:
   - `result_ref`
   - short projection excerpt
   - turn/tool count if available
2. Distinguish these states:
   - task accepted by review
   - task result projected
   - task execution had recovery
   - subagent/AgentRun failed but parent chat recovered
3. Add a small "trace" affordance that can open or show:
   - request id
   - turn id
   - model route
   - last event type

Acceptance:

- User can see why a task is considered completed.
- The UI does not collapse timeout recovery into a false "all green" impression.

### P1-B: Backfill / Diagnostic Script

Owner: tooling

Files:

- `scripts/` or a Rust maintenance command

Tasks:

1. For old rows only, infer task id from `request_id like goal-task-{task_id}-%`.
2. Join `agent_tasks` to fill missing `goal_id`, `goal_cycle_id`, `agent_task_id`.
3. Dry-run mode by default.

Acceptance:

- Current DB can be repaired for local analysis.
- New code path does not depend on the backfill.

## 5. Regression Tests

### TC-GTF-01: New GoalTask ChatTurn Has Anchors

Setup:

- Create goal + cycle + task.
- Execute through `execute_goal_task_via_chat` or extracted test seam.

Assert:

- `chat_turns.goal_id` equals goal id.
- `chat_turns.goal_cycle_id` equals cycle id.
- `chat_turns.agent_task_id` equals task id.

### TC-GTF-02: Team Member Completes After Chat Executor Writeback

Assert:

- Member starts as `running`.
- Member becomes `completed` after review-ready writeback.
- Team transitions toward `awaiting_review` when all members complete.

### TC-GTF-03: Parent Chat Recovery Does Not Hide AgentRun Failure

Setup:

- A task chat turn calls `agent.start`.
- The child `agent_run` fails or times out.
- Parent chat recovers and writes a final answer.

Assert:

- Task result is visible.
- `chat_turn_events` records recovery.
- UI/API can still report child `agent_run.status=failed`.

### TC-GTF-04: Non-Goal Chat Is Unchanged

Assert:

- Ordinary user chat still creates null goal/task anchors.
- Existing projection behavior remains unchanged.

## 6. Open Design Decisions

| ID | Decision | Recommended |
|---|---|---|
| D-GTF-01 | Synthetic run id for chat executor | Use `chat:<turn_id>` only as member run pointer, not as `agent_runs.id` |
| D-GTF-02 | Blocked member status | Use `paused` if the lifecycle expects user intervention, `stopped` only for terminal failure |
| D-GTF-03 | Event bus | Use `chat_turn_events` for turn-bound events, audit only when no turn exists |
| D-GTF-04 | Backfill | One-off dry-run maintenance, not runtime fallback |

## 7. Similar-Pattern Audit Checklist

Use this checklist for the next backend pass:

1. A runtime object has typed columns, but callers encode identity in string IDs.
2. A state machine exists, but a newer executor path updates only a sibling table.
3. UI refresh events are emitted, but no canonical L1 event exists.
4. A "started" API returns success while the underlying async run can later fail.
5. A read model is available, but the canonical ledger cannot reconstruct the same story.
6. A recovery path marks user-facing output successful while child failure becomes invisible.
7. Tests cover direct helper functions, but not the actual desktop worker/runtime path.

## 8. Similar Backend Design Gaps Found

This section records the follow-up backend audit after the initial GoalTask flow finding.

### S1: DispatchPlan Table Exists But Main OODA Path Does Not Persist Plans

Priority: P0

Evidence:

- `dispatch_plans` table exists.
- `goal_cycles.dispatch_plan_id` exists.
- Current DB: `dispatch_plans_total = 0`.
- Current DB: `goal_cycles_total = 32`, `goal_cycles.with_plan = 0`.
- `goal_orchestrator::decide_llm()` returns an in-memory `DispatchPlan`.
- `goal_orchestrator::tick_goal()` calls `act()` directly after deciding.

Impact:

- Review cannot reconstruct why the system dispatched a given set of tasks.
- UI cannot show "approved plan vs executed work" from L0.
- A later replay/debug pass must infer plan from tasks, which loses rejected/held/noop decisions.

Dispatch:

1. Add `goals::create_dispatch_plan(goal_id, cycle_id, plan)` or a dedicated `dispatch_plans` module.
2. Persist every plan result before `act()`, including empty/noop plans.
3. Update `goal_cycles.dispatch_plan_id`.
4. Persist plan status transitions: `proposed`, `approved`, `executing`, `completed`, `rejected`, `noop`.
5. Add regression for completed cycles requiring non-null `dispatch_plan_id`.

Acceptance:

- New completed goal cycles always have `dispatch_plan_id`.
- `dispatch_plans.tasks_json` includes planned, held, rejected, and noop reasons when applicable.

### S2: RouteDecision Table Exists But ModelResolver Does Not Write It

Priority: P0

Evidence:

- `routing::route_task()` writes `route_decisions`.
- Main LLM path uses `model_resolver::resolve_with_request()`.
- `resolve_with_request()` emits `model.routed` but does not insert `route_decisions`.
- Current DB: `route_decisions_total = 0`.
- Current DB: `routing_policies_total = 0`, `llm_profiles_total = 0`; latest goal-task turns therefore route by config fallback.

Impact:

- Per-turn model route is visible only as event payload, not as canonical route decision.
- Goal/Task-level routing cannot be audited across runs.
- The `route_decisions` read APIs are disconnected from real chat/goal execution.

Dispatch:

1. Add a route-decision write path to `ModelResolver`.
2. Extend resolver context with optional `workspace_id`, `task_id`, `turn_id`, and `request_id`.
3. Persist every resolver outcome, including direct hint and fallback.
4. Link `route_decisions.task_id` for goal tasks once P0-A/B context is passed.
5. Keep `model.routed` event as L1 projection of the canonical L0 decision.

Acceptance:

- New goal-task chat turns produce one `route_decisions` row.
- `chat_turn_events.model.routed.payload_json` carries `route_decision_id`.
- Fallback routing is queryable without scanning events.

### S3: AgentTeam Lifecycle Has State But No Team/Member Event Line

Priority: P1

Evidence:

- Current DB: `agent_team_members.total = 44`.
- Current DB: `agent_team_members.with_run = 5`.
- Current DB: `agent_team_members.non_active = 0`.
- Runtime event counts: `agent_team.% = 0`.
- `agent_teams::transition_team_lifecycle()` validates and upserts, but does not append a runtime event.
- `agent_teams::set_member_status()` upserts, but does not append a runtime event.

Impact:

- UI refresh can notice a table change only when Tauri emits manually.
- There is no canonical member status timeline.
- Team lifecycle appears frozen or opaque even when goal/task lifecycle finishes.

Dispatch:

1. Emit `agent_team.lifecycle_changed`.
2. Emit `agent_team.member_status_changed`.
3. Emit `agent_team.member_run_bound`.
4. Include `goal_id`, `cycle_id`, and `task_id` from metadata when available.
5. Make Tauri refresh events secondary to canonical runtime events.

Acceptance:

- New team lifecycle transitions are visible in `runtime_events`.
- Member status changes can be displayed as a timeline without polling raw rows.

### S4: Task Writeback Skips Review-Ready / Blocked Events

Priority: P1

Evidence:

- `goal_tasks::claim_task()` emits `task.claimed`.
- `goal_tasks::accept_review_ready_task()` emits `task.accepted`.
- `goal_tasks::set_task_result_ref_review_ready()` does not emit `task.review_ready`.
- `goal_tasks::set_task_result_ref_blocked()` does not emit `task.blocked` in this writeback path.
- Current runtime events show task accepted, but not enough writeback provenance.

Impact:

- The moment where an executor produced a result is not first-class.
- The system can show task accepted but cannot explain the result-writeback boundary.
- Blocked partial results can become hard to separate from generic task blockers.

Dispatch:

1. Add event emission to `set_task_result_ref_review_ready`.
2. Add event emission to `set_task_result_ref_blocked`.
3. Payload must include `task_id`, `result_ref`, `executor_kind`, and optional `turn_id`.
4. Use these events to drive GoalConsole result timeline.

Acceptance:

- A task result writeback is visible before review acceptance.
- Blocked writeback includes the partial result ref.

### S5: Dispatch Governance Module Is Tested But Not Wired Into Main Path

Priority: P1

Evidence:

- `goal_orchestrator/dispatch.rs` implements:
  - max parallel agents
  - dependency scheduling
  - write-scope conflict detection
  - failure retry / cycle protection
- Search shows only tests call `filter_dispatch()`.
- `tick_goal()` currently goes `decide -> act` without dispatch filtering.

Impact:

- The architecture claims dispatch governance, but production path dispatches all planned tasks directly.
- Write-scope conflicts and parallel limits do not protect real goal execution.
- Failure loop protection is not active.

Dispatch:

1. Insert `dispatch::filter_dispatch()` between decide and act.
2. Persist held/rejected reasons into `dispatch_plans`.
3. Convert rejected tasks into task records or plan records according to product policy.
4. Make `max_parallel_agents` configurable from goal policy.

Acceptance:

- A plan with conflicting write scopes does not create all tasks as executable at once.
- Held/rejected tasks are visible in plan history.

### S6: McpRouter Executor Is Mostly Unreachable And Has A State-Machine Conflict

Priority: P1

Evidence:

- Worker branches to `execute_goal_task_via_mcp_router()` only when `task.agent_kind == "mcp_router"`.
- `decide_llm()` currently emits `agent_kind = "backend-agent"`.
- deterministic `decide()` also defaults to `backend-agent` or agent ids, not `mcp_router`.
- `/runtime/tasks/{task_id}/execute` requires task status `running`.
- `execute_goal_task_via_mcp_router()` calls `goal_tasks::claim_task()`, which expects `proposed/queued -> claimed`.

Impact:

- D-3 code may exist but is not reached by normal OODA decisions.
- Manually forcing `mcp_router` through the execute signal can fail on status transition.

Dispatch:

1. Decide whether `mcp_router` is an `agent_kind`, backend kind, or transport.
2. If it remains an executor branch, make planner able to produce `agent_kind = "mcp_router"`.
3. Remove or guard `claim_task()` in `execute_goal_task_via_mcp_router()` when task is already running.
4. Emit the same task writeback and team/member events as chat executor.

Acceptance:

- A synthetic mcp_router task can execute end to end from queued/claimed/running.
- Normal routing policy can intentionally choose McpRouter.

### S7: Heartbeat Model Exists But GoalTask Chat Executor Does Not Emit Heartbeats

Priority: P2

Evidence:

- `agent_heartbeats` table and runtime APIs exist.
- `GoalConsole` reads active heartbeats.
- Current DB: `agent_heartbeats = 0`.
- Heartbeats are wired into `CodexAdapter`, not into `execute_goal_task_via_chat`.

Impact:

- GoalConsole "执行进展" remains empty for chat-executed goal tasks.
- Users see completed task status only after writeback, not live progress.

Dispatch:

1. Add heartbeat upsert around chat goal executor start/progress/done.
2. Use `agent_id = backend-agent:<task_id>` to match team member.
3. Expire or set idle on completion.

Acceptance:

- While a goal task is executing through chat, GoalConsole shows a live heartbeat.

### S8: OODA Event Naming Drift

Priority: P2

Evidence:

- `events::emit_ooda_phase_changed()` exists.
- Main path uses `goals::advance_cycle_phase()` which emits `goal_cycle.phase_changed`.
- Current DB has `goal_cycle.phase_changed`, but no `ooda.phase_changed`.

Impact:

- Consumers built against the architecture document will query the wrong event type.
- OODA and goal-cycle vocabularies are not normalized.

Dispatch:

1. Decide canonical event name.
2. If `goal_cycle.phase_changed` remains canonical, update architecture docs and UI contracts.
3. If `ooda.phase_changed` is canonical, have `advance_cycle_phase()` call the helper or dual-emit during migration.

Acceptance:

- One documented event type is canonical.
- Tests assert the chosen event name.

## 9. Backend Audit Summary

The same structural issue appears in multiple places:

```text
Typed object/table exists
  -> helper/test exists
  -> actual desktop/runtime path uses a newer shortcut
  -> shortcut updates enough state for UX
  -> canonical L0/L1/L2 relationship is incomplete
```

Highest-priority fixes should happen in this order:

1. GoalTask chat turn anchors and AgentTeam member status (P0-A/B/C).
2. DispatchPlan persistence (S1).
3. RouteDecision persistence from ModelResolver (S2).
4. Task writeback and AgentTeam events (S3/S4).
5. Dispatch governance wiring (S5).
6. McpRouter executor reachability/state fix (S6).
7. Heartbeat and OODA naming cleanup (S7/S8).
