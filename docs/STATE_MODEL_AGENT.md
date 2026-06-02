# Agent 架构 L0 状态账本

> 来源: `项目Agent架构状态机与治理范式-20260529.md` + workspace.md 已实现任务
> 更新: 2026-05-30

---

## §4.1 Canonical Objects

| # | Object | Module | Evidence |
|---|--------|--------|----------|
| 1 | AgentRun | agent_runs.rs | observed |
| 2 | ToolCall | tool_calls.rs | observed |
| 3 | PermissionGrant | permissions.rs | observed |
| 4 | Proposal (ActionProposal) | proposals.rs | observed |
| 5 | AgentTeam | agent_teams.rs | observed |
| 6 | AgentTeamMember | agent_team_members.rs | observed |
| 7 | MemoryEntry | memory.rs | observed |
| 8 | CommandRun | command_runs.rs | observed |
| 9 | InteractiveAgentSession (Codex) | codex.rs | observed |
| 10 | TodoItem | todo.rs | observed |
| 11 | ChatMessage | chat/types.rs | observed |
| 12 | ChatSession | chat/session.rs | observed |
| 13 | AuditEvent | events.rs | observed |
| 14 | McpProvider / McpToolMapping | mcp.rs | observed |
| 15 | Workspace | workspaces.rs | observed |

---

## §4.2 Canonical Source Rules

| State Category | Canonical Source | Read Model | Notes |
|---------------|-----------------|------------|-------|
| Agent lifecycle | agent_runs.rs | agent_runs table | AgentRun CRUD |
| Tool execution | tool_calls.rs | tool_calls table | ToolCall CRUD |
| Permission | permissions.rs | permission_grants table | PermissionGrant CRUD |
| Proposal | proposals.rs | action_proposals table | Proposal CRUD |
| Team orchestration | agent_teams.rs | agent_teams table | AgentTeam + members |
| Memory | memory.rs | memory_entries table | MemoryEntry CRUD |
| Command execution | command_runs.rs | command_runs table | CommandRun CRUD |
| Interactive session | codex.rs | codex_sessions table | CodexSession CRUD |
| Audit trail | events.rs | NDJSON file | append-only |

**Forbidden:**
- No module may create its own canonical object type for another module's domain
- No status string without canonical object mapping
- No implicit state transitions across module boundaries
- No direct DB writes to another module's tables

---

## §5 AgentRun State Machine

**Evidence: observed** (from agent_runs.rs)

```
                    ┌─────────────┐
                    │   Created   │
                    └──────┬──────┘
                           │ start_claude_run
                           ▼
                    ┌─────────────┐
                    │   Queued    │
                    └──────┬──────┘
                           │ spawn_claude
                           ▼
                    ┌─────────────┐
              ┌─────│   Running   │─────┐
              │     └──────┬──────┘     │
              │            │            │
              ▼            ▼            ▼
        ┌──────────┐ ┌──────────┐ ┌──────────┐
        │ Succeeded│ │  Failed  │ │ Stopped  │
        └──────────┘ └──────────┘ └──────────┘
```

| From | To | Trigger | Guard | Side Effect | Evidence |
|------|----|---------|-------|-------------|----------|
| Created | Queued | start_claude_run | prompt non-empty | upsert DB | observed |
| Queued | Running | spawn_claude | claude binary exists | spawn process, emit agent_run.created | observed |
| Running | Succeeded | finish_spawned_run | exit code 0 | persist stdout/stderr | observed |
| Running | Failed | finish_spawned_run | exit code != 0 | persist error | observed |
| Running | Stopped | stop() | user request | kill process | observed |

**Illegal transitions:**
- Succeeded → any (terminal)
- Failed → any (terminal)
- Stopped → any (terminal)

**Invariants:**
- Every AgentRun has exactly one status at any time
- Running state must have a pid
- Terminal states must have finished_at

---

## §6 ToolCall State Machine

**Evidence: observed** (from tool_calls.rs)

```
              ┌───────────┐
              │  Created  │
              └─────┬─────┘
                    │
              ┌─────▼─────┐
              │ Executing  │
              └─────┬─────┘
              ┌─────┼─────┐
              ▼     ▼     ▼
        ┌────────┐ ┌────────┐ ┌──────────┐
        │Succeeded│ │ Failed │ │TimedOut  │
        └────────┘ └────────┘ └──────────┘
```

| From | To | Trigger | Evidence |
|------|----|---------|----------|
| Created | Executing | execute_tool_call | observed |
| Executing | Succeeded | tool returns ok | observed |
| Executing | Failed | tool returns err | observed |
| Executing | TimedOut | timeout exceeded | observed |

---

## §6 PermissionGrant State Machine

**Evidence: observed** (from permissions.rs)

```
              ┌─────────────┐
              │ Unrequested │
              └──────┬──────┘
                     │ request()
                     ▼
              ┌─────────────┐
              │  Requested  │
              └──────┬──────┘
           ┌─────────┼─────────┐
           ▼         ▼         ▼
    ┌──────────┐ ┌──────────┐ ┌──────────┐
    │Approved  │ │Approved  │ │  Denied  │
    │  Once    │ │ Session  │ │          │
    └────┬─────┘ └────┬─────┘ └──────────┘
         │            │
         ▼            ▼
    ┌──────────┐ ┌──────────┐
    │   Used   │ │ Expired  │
    └──────────┘ └──────────┘
```

| From | To | Trigger | Evidence |
|------|----|---------|----------|
| Unrequested | Requested | request() | observed |
| Requested | ApprovedOnce | approve_once() | observed |
| Requested | ApprovedSession | approve_session() | observed |
| Requested | Denied | deny() | observed |
| ApprovedOnce | Used | mark_used() | observed |
| ApprovedSession | Expired | mark_expired() | observed |
| Any Active | Revoked | revoke() | observed |

---

## §7 AgentTeam State Machine

**Evidence: observed** (from agent_teams.rs)

```
    ┌───────────┐
    │   Draft   │
    └─────┬─────┘
          │
    ┌─────▼─────┐
    │  Planning  │
    └─────┬─────┘
          │
    ┌─────▼──────────────┐
    │ AwaitingPlanApproval│
    └─────┬──────────────┘
     ┌────┴────┐
     ▼         ▼
┌─────────┐ ┌──────────────┐
│Executing │ │ReworkRequired│
└────┬─────┘ └──────────────┘
     │              ▲
     ▼              │
┌──────────────┐    │
│AwaitingReview│────┘ (verdict=Failed)
└──────┬───────┘
       │ (verdict=Accepted)
       ▼
  ┌──────────┐
  │ Accepted │
  └────┬─────┘
       │
       ▼
  ┌──────────┐
  │ Archived │
  └──────────┘
```

| From | To | Guard | Evidence |
|------|----|-------|----------|
| Draft | Planning | team created | observed |
| Planning | AwaitingPlanApproval | plan submitted | observed |
| AwaitingPlanApproval | Executing | plan_approval_response positive | observed |
| AwaitingPlanApproval | ReworkRequired | plan_approval_response negative | observed |
| Executing | AwaitingReview | all members done | observed |
| AwaitingReview | Accepted | review verdict=Accepted | observed |
| AwaitingReview | ReworkRequired | review verdict=Failed | observed |
| ReworkRequired | Planning | rework started | observed |
| Accepted | Archived | archive_team() | observed |

**Write scope rules:**
- Executing teams with overlapping write_scope → file-level write lock, serial execution
- check_write_scope_conflict() checks against other Executing/AwaitingReview teams

---

## §8 MemoryEntry State Machine

**Evidence: observed** (from memory.rs)

```
              ┌───────────┐
              │ Candidate  │◄──── tool/inferred source
              └─────┬─────┘
           ┌────────┼────────┐
           ▼        ▼        ▼
    ┌──────────┐ ┌──────────┐ ┌───────────┐
    │  Active  │ │ Archived │ │ Forgotten │
    └────┬─────┘ └──────────┘ └───────────┘
         │
         ▼
  ┌─────────────┐
  │ Quarantined │
  └──────┬──────┘
         │ restore_from_quarantine
         ▼
    Candidate (needs re-classify)
```

| From | To | Trigger | Guard | Evidence |
|------|----|---------|-------|----------|
| (new) | Active | set() / set_with_source(source="user") | source=user | observed |
| (new) | Candidate | set_with_source(source="tool"/"inferred") | source=tool/inferred | observed |
| Candidate | Active | classify(key, "active") | status=candidate | observed |
| Candidate | Archived | classify(key, "archived") | status=candidate | observed |
| Active | Archived | archive(key) | status=active | observed |
| Any | Forgotten | forget(key) | status!=forgotten | observed |
| Active/Candidate | Quarantined | quarantine(key) | status in active/candidate | observed |
| Quarantined | Candidate | restore_from_quarantine(key) | status=quarantined | observed |

**Write gate rules:**
- source="user" → status=active, confidence=1.0
- source="tool" → status=candidate, confidence=0.7
- source="inferred" → status=candidate, confidence=0.5

**Forbidden:**
- inferred → active (must go through classify)
- tool → active (must go through classify)
- quarantined → active (must go through candidate first)

---

## §8 CommandRun State Machine

**Evidence: observed** (from command_runs.rs)

```
    ┌───────────┐
    │ Prepared  │
    └─────┬─────┘
     ┌────┴────┐
     ▼         ▼
┌──────────┐ ┌─────────────────┐
│ Starting │ │AwaitingPermission│
└────┬─────┘ └─────────────────┘
     │
     ▼
┌──────────┐
│Streaming │
└────┬─────┘
  ┌──┼──┐
  ▼  ▼  ▼
┌───┐┌───────┐┌───────┐
│   ││TimedOut││Killed │
│   │└───────┘└───────┘
│   │
│   │
└───┘
Exited
```

---

## §8 InteractiveAgentSession (Codex) State Machine

**Evidence: observed** (from codex.rs)

```
    ┌─────────┐
    │ Created │
    └────┬────┘
         │
    ┌────▼────┐
    │Starting │
    └────┬────┘
         │
    ┌────▼────┐
    │  Ready  │
    └────┬────┘
    ┌────┴────┐
    ▼         ▼
┌───────┐ ┌───────────┐
│Running│ │AwaitInput │
└───┬───┘ └───────────┘
    │
    ▼
┌────────────┐
│Interrupted │
└──────┬─────┘
       │
  ┌────▼─────┐
  │Resumable │
  └──────────┘
```

Terminal: Completed, Failed

---

## Evidence Level Summary

| Level | Count | Description |
|-------|-------|-------------|
| observed | ~30 | Code exists and tests pass |
| inferred | ~15 | Documented in architecture docs, some code exists |
| proposed | ~10 | Not yet implemented (e.g., full AgentRun lifecycle in chat flow) |
