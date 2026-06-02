---
name: state-machine-lifecycle
description: Turn raw product ideas, vague requirements, bug reports, refactors, tests, release checks, AgentTeam dispatches, or project-internal Agent designs into state-machine-driven PRDs, MVP slices, architecture mappings, OODA-R task splits, test matrices, regression plans, release gates, and Agent permission models. Use for multi-agent or vibe-coding software work where requirements, state semantics, MVP boundaries, tests, memories, permissions, or design intent may drift.
---

# State Machine Lifecycle

## Overview

Use this skill to keep product, development, testing, and Agent governance tied to explicit states, transitions, guards, side effects, invariants, and evidence. Prefer the smallest mode that answers the user's request; do not generate a full PRD when a triage card or `no_state_impact` check is enough.

## Runtime Contract

Start every run by choosing one mode and one status.

Modes:

- `triage`: classify raw ideas, feedback, bugs, or vague requests.
- `decompose`: split a requirement into atomic state-impact units.
- `prd`: draft a state-machine PRD and acceptance criteria.
- `mvp-slice`: define the smallest closed state subgraph.
- `architecture`: map transitions to modules, APIs, storage, events, UI, and owners.
- `agent-dispatch`: create OODA-R AgentTeam assignments and review gates.
- `test-matrix`: generate legal, illegal, guard, side-effect, and recovery tests.
- `regression`: map a bug to a broken transition or invariant.
- `review`: inspect a design or diff for state drift and boundary expansion.
- `release-gate`: check traceability and P0 release evidence.
- `agent-boundary`: model project-internal Agent lifecycle, permissions, tools, memory, and audit.
- `minimal`: answer small tasks with five-tuple, guard, acceptance, non-goals, and OODA-R only.
- `self-iterate`: improve this methodology or skill using its own state machine.

Statuses:

- `ready`: sufficient information exists to produce the requested artifact.
- `needs_clarification`: key actor, object, state, trigger, target, guard, or acceptance is missing.
- `inbox`: useful idea, not ready for development.
- `spike`: bounded exploration is needed before formalizing.
- `no_state_impact`: change does not affect business state, permissions, read models, or external behavior.
- `blocked`: high-risk guard, permission, data, deletion, payment, production write, or external call is unclear.
- `release_gate_failed`: release evidence is insufficient.

Ask at most 3 clarification questions. Ask only when the answer changes the state model, permission boundary, data boundary, release gate, or acceptance criteria.

## Workflow

1. Normalize the user's input into `problem`, `desired_outcome`, and `solution_hint`.
2. Choose the smallest useful mode and status.
3. Identify state impact: business state, read model, UI state, permission, Agent runtime, lifecycle, or `no_state_impact`.
4. For state-impacting work, derive actor, object, current state, trigger, target state, guard, side effects, invariants, illegal transitions, and recovery.
5. Mark every important conclusion as `observed`, `inferred`, `assumed`, or `unknown`.
6. Produce only the artifacts needed for the chosen mode.
7. If producing development work, include OODA-R constraints: Observe, Orient, Decide, Act, Review.
8. If producing test or release work, include traceability from raw input to transition, task, test, and evidence.

## Core Rules

- State precedes functionality. UI, APIs, storage, and tests implement or expose state transitions.
- MVP is a closed state subgraph, not a short feature list.
- LLMs may choose implementation details, but must not silently redefine states, guards, permissions, or MVP boundaries.
- Never write Agent assumptions, tool outputs, or temporary reasoning into long-term facts without evidence.
- For pure copy, formatting, comments, or behavior-preserving refactors, use `no_state_impact` instead of inventing states.
- For high-risk actions, prefer `blocked` over guessing.

## Reference Loading

Load only the references needed for the current mode:

- `references/meta-model.md`: state namespaces, evidence contract, L0-L3 layers, lifecycle states.
- `references/output-schemas.md`: Demand Card, State Model, MVP Slice, Architecture Mapping, Dispatch Pack, Test Matrix, Release Gate schemas.
- `references/agent-team.md`: OODA-R AgentTeam workflow, role boundaries, write-scope isolation, review gates.
- `references/runtime-agent-security.md`: project-internal Agent lifecycle, permissions, tools, memory, audit, incident handling.
- `references/examples.md`: examples for feature, bug, refactor, idea, no-state-impact, and Agent boundary prompts.

## Output Discipline

Default response shape:

```yaml
mode:
status:
state_impact:
evidence_summary:
outputs:
open_questions:
next_step:
```

When implementing code, do not stop at documents if the user asked for changes. Use this skill to constrain the work, then proceed through OODA-R and verify.
