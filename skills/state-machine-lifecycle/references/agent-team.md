# AgentTeam OODA-R

Use for `agent-dispatch`, `review`, `architecture`, `test-matrix`, and implementation planning.

## OODA-R Loop

```text
Observe -> Orient -> Decide -> Act -> Review
```

| Phase | Output | Forbidden |
|---|---|---|
| Observe | Facts, read context, unknowns | Writing code immediately |
| Orient | State impact, risk, boundary, design options | Guessing from filenames only |
| Decide | Plan, file scope, test plan, rollback points | Starting without a plan |
| Act | Code, tests, document deltas | Boundaryless refactors or silent state changes |
| Review | Diff review, test evidence, state impact, rework items | Reporting only “done” |

Hard gates:

- Do not enter Act before Observe and Orient.
- Do not edit files before Decide.
- If Review discovers state semantic change, return to Decide or propose L0 update.

## Development Agent States

```text
Idle
-> Assigned
-> Observing
-> Oriented
-> DesignReady
-> Implementing
-> SelfReviewing
-> ReviewReady
-> Accepted / ReworkRequired / Blocked
```

## Role Boundaries

| Role | Owns | Must not do |
|---|---|---|
| Product Agent | User goal, PRD, state-change proposal | Direct implementation |
| Architecture Agent | Transition-to-module/API/storage/event/UI mapping | Bypass L0 |
| Implementation Agent | Implement assigned scope | Add states or expand MVP silently |
| Test Agent | Legal/illegal transition tests and regression | Only test happy path |
| Review Agent | State drift, boundary expansion, duplicate state sources | Unbounded refactor |
| Docs Agent | Sync confirmed facts to L0/L1/L2 | Store unverified reasoning as fact |

## Parallelism Rules

- Product and Architecture are sequential and cannot be skipped.
- Implementation Agents can run in parallel only with non-overlapping write scopes.
- Test Agent can start from the test matrix before implementation finishes.
- Review Agent should stay independent from implementation.
- Docs Agent syncs only after review confirms facts.
- If write scopes overlap, serialize work; the later Agent must Observe the earlier diff.

## Review Checklist

Use this for reviews:

```yaml
state_drift:
  changed_state_semantics:
  added_unregistered_state:
  changed_guard:
boundary_expansion:
  mvp_outside_entry_exposed:
  unrelated_refactor:
state_source:
  duplicate_enum_or_status:
  noncanonical_ui_status:
tests:
  p0_legal:
  p0_illegal:
  guard:
  recovery:
evidence:
  automated_tests:
  manual_checks:
decision: accepted | rework_required | blocked
```

