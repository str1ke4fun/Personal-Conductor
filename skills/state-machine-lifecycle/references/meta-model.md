# State-Machine Meta Model

Use this reference for `triage`, `decompose`, `prd`, `mvp-slice`, `architecture`, `review`, `release-gate`, and `self-iterate` modes.

## State Namespaces

Never use naked states such as `pending`, `running`, or `done` without an object and namespace.

| Namespace | Purpose | Example |
|---|---|---|
| `BusinessState` | Business object lifecycle | `Proposal.pending_review` |
| `LifecycleState` | Product/development lifecycle | `LC.RequirementSpecified` |
| `AgentRunState` | Runtime execution instance | `AgentRun.awaiting_approval` |
| `ToolCallState` | Tool invocation lifecycle | `ToolCall.executing` |
| `PermissionState` | Authorization lifecycle | `PermissionGrant.approved_once` |
| `MemoryState` | Memory lifecycle | `MemoryEntry.classified` |
| `ReadModelState` | Derived view state | `DashboardItem.action_required` |
| `UIState` | UI-only presentation state | `Button.disabled` |

Rules:

- `BusinessState` is the canonical fact source.
- `ReadModelState` is derived and must declare source states and stale policy.
- `UIState` must not become a business fact source.
- Agent, tool, permission, and memory states must not be mixed with business states.

## Meta Model

```yaml
machine_id:
machine_type: business | lifecycle | dev_agent | runtime_agent | permission | memory | tool
version:
owner:
object_type:
states:
  - state_id:
    namespace:
    description:
    terminal: true | false
    timeout_rule:
    evidence:
commands:
  - command_id:
    actor:
    intent:
events:
  - event_id:
    source:
    payload_contract:
transitions:
  - transition_id:
    from_state:
    to_state:
    trigger: command_id | event_id | timer | external_callback
    guard:
    side_effects:
    invariants:
    compensation:
    illegal_transitions:
    priority: P0 | P1 | P2
    owner:
    test_ids:
    evidence:
read_models:
  - read_model_id:
    source_states:
    derivation_rule:
    stale_policy:
```

## Evidence Contract

Mark conclusions as:

| Evidence | Meaning | Can enter L0 |
|---|---|---|
| `observed` | From user explicit statement, L0, code, tests, logs, or other verifiable facts | Yes |
| `inferred` | Derived from observed facts but not confirmed | No |
| `assumed` | Temporary assumption for progress | No |
| `unknown` | Missing information | No |

Priority order:

```text
L0 state ledger > current user instruction > observed code/tests/logs > L1/L2 docs > inference > assumption
```

## Document Layers

| Layer | Purpose | Rule |
|---|---|---|
| L0 `STATE_MODEL.md` | Stable states, transitions, guards, invariants, illegal transitions, MVP slice | Highest priority |
| L1 PRD/Architecture | Product goals, scenarios, architecture rationale | Explains L0 |
| L2 Task/Bug/Regression | Current work, allowed scope, tests, manual results | Must reference L0 or declare `no_state_impact` |
| L3 Inbox/Spike/Memory | Ideas, observations, temporary assumptions | Cannot directly drive development |

## Lifecycle States

```text
LC-00 Idea Captured
LC-01 Problem Framed
LC-02 Actor & Object Mapped
LC-03 Core State Modeled
LC-04 MVP Slice Selected
LC-05 Requirement Specified
LC-06 Architecture Mapped
LC-07 Ready For Dev
LC-08 In Development
LC-09 Integrated
LC-10 State-Tested
LC-11 Release Gated
LC-12 Released & Observed
LC-13 Retrospected
```

Return to earlier lifecycle states when evidence fails. Examples:

- Object is wrong: return `LC-03 -> LC-02`.
- MVP is not closed: return `LC-04 -> LC-03`.
- Architecture has ownerless transitions: return `LC-06 -> LC-05`.
- Tests lack P0 evidence: return `LC-10 -> LC-08`.

## Cross-Object Modeling

Use:

- `causal_link` when one transition triggers another asynchronously.
- `saga` when multiple objects must complete a workflow with compensation.
- `read_model` when UI or query state is derived from several source states.

Always declare consistency boundary, retry policy, compensation, and whether the user sees canonical facts or derived views.

