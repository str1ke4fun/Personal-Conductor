# Runtime Agent Security Model

Use for `agent-boundary`, project-internal Agent design, permissions, memory, tools, and audit.

## Core Objects

| Object | Meaning |
|---|---|
| `Agent` | Execution entity with role, abilities, and constraints |
| `AgentTask` | Work item assigned to an Agent |
| `AgentRun` | One execution instance |
| `ToolCall` | One tool invocation |
| `PermissionGrant` | Human or policy authorization |
| `WorkspaceScope` | Read/write boundary |
| `MemoryEntry` | Memory item |
| `AuditEvent` | Traceable behavior record |

## AgentRun State Machine

```text
Created
-> Configured
-> Idle
-> Planning
-> AwaitingApproval
-> Running
-> ToolCalling
-> Paused
-> Succeeded / Failed / Stopped
-> Archived
```

Illegal transitions:

| From | Illegal To | Reason |
|---|---|---|
| `Created` | `Running` | Role and boundary are not configured |
| `Planning` | high-risk `ToolCalling` | Risk and approval are missing |
| `AwaitingApproval` | `Running` | Authorization bypass |
| `Failed` | `Succeeded` | Failure cannot become success silently |
| `Archived` | `ToolCalling` | Archived runs are read-only |

## ToolCall State Machine

```text
Proposed
-> RiskClassified
-> Approved / Rejected
-> Executing
-> Succeeded / Failed / TimedOut
-> Recorded
```

Record:

```yaml
tool_call_id:
agent_id:
task_id:
workspace_scope:
input_summary:
risk_level:
permission_grant_id:
started_at:
finished_at:
result_summary:
error:
audit_ref:
```

## PermissionGrant State Machine

```text
Unrequested
-> Requested
-> ApprovedOnce / ApprovedSession / Denied
-> Expired / Revoked
```

Rules:

- Default to least privilege.
- Write, execute, network, external system, production, secret, deletion, payment, and deployment actions must be auditable.
- Permission cannot be reused across workspaces silently.
- Low-risk approval cannot imply high-risk approval.
- Revoked permission requires a fresh request.
- Child Agent permissions are the intersection of parent task scope and explicit grant; never inherit broad session permissions automatically.

## Permission Matrix

| Subject | Resource | Action | Environment | Effect | Approval | Audit |
|---|---|---|---|---|---|---|
| Agent | Workspace file | read | local/dev | allow | none | optional |
| Agent | Workspace file | write | local/dev | allow | task scope | required |
| Agent | Command | execute | local/dev | conditional | risk-based | required |
| Agent | Production data | write | prod | deny by default | human explicit | required |
| Agent | Secret | read | any | deny by default | explicit need | required |
| Child Agent | Parent scope | delegate | any | attenuated allow | parent task scope | required |

## Memory Lifecycle

```text
Candidate
-> Classified
-> Approved
-> Stored
-> Retrieved
-> Expired / Deleted
```

Before storing memory, check:

- Is it a stable fact?
- Will it affect future decisions?
- Is it sensitive?
- Is there a deletion path?
- Is it already captured in L0/L1?

Tool output, intermediate reasoning, temporary errors, and unverified observations do not enter long-term memory by default.

## Incident Handling

| Incident | Response |
|---|---|
| Unauthorized tool call | Kill switch, revoke grant, write audit, notify human owner |
| Memory pollution | Isolate entry, delete or mark untrusted, review downstream decisions |
| Wrong state migration | Pause Agent, compensate state, create regression card |
| Child Agent scope violation | Stop delegation chain, revoke child grants, audit parent |
| External tool anomaly | Pause retry, enter Failed or AwaitingHuman |

