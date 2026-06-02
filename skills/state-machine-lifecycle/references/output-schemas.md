# Output Schemas

Use these schemas as compact templates. Fill only fields relevant to the selected mode.

## Demand Card

```yaml
demand_id:
raw_input:
normalized:
  problem:
  desired_outcome:
  solution_hint:
demand_type: feature | bug | improvement | refactor | test | docs | release | spike | agent-boundary
mode:
status:
user_goal:
non_goals:
actors:
objects:
state_impact:
  changes_business_state: true | false
  changes_read_model: true | false
  changes_permission: true | false
  changes_agent_runtime: true | false
evidence:
  observed:
  inferred:
  assumed:
  unknown:
open_questions:
```

## Atomic Requirement

```yaml
req_id:
title:
actor:
object:
from_state:
trigger:
to_state:
guard:
side_effects:
invariants:
illegal_transitions:
compensation:
read_model_impact:
ui_expectation:
observability_event:
idempotency:
retry_or_recovery:
acceptance:
  state_assertion:
  data_assertion:
  ui_assertion:
  side_effect_assertion:
  audit_assertion:
  regression_assertion:
priority: P0 | P1 | P2
mvp: true | false
evidence:
```

## No State Impact

```yaml
status: no_state_impact
reason:
external_behavior_changed: false
business_state_changed: false
permission_changed: false
read_model_changed: false
invariants:
  - Existing state transitions unchanged
  - Existing API contracts unchanged
  - Existing tests should continue passing
verification:
  - targeted automated test
  - smoke test
  - manual check if needed
```

## MVP Slice

```yaml
mvp_slice_id:
entry_state:
success_exit:
failure_exit:
exit_or_cancel_path:
minimum_path:
included_transitions:
excluded_branches:
visible_ui:
minimum_data:
acceptance:
```

## Architecture Mapping

```yaml
transition_id:
object:
from_state:
to_state:
command_or_event:
domain_owner:
api_or_handler:
guard_location:
state_store:
event_emitted:
read_model:
ui_surface:
side_effect_owner:
compensation_owner:
test_ids:
observability:
```

Ownerless transition indicators:

- Transition has no module owner.
- State has no canonical fact source.
- Side effect has no owner.
- UI status has no read model.
- Illegal transition has no rejection point.
- P0 transition has no test owner.

## Agent Dispatch Pack

```yaml
task_id:
raw_input:
mode:
goal:
non_goals:
related_machine:
  machine_id:
  states:
  transitions:
  invariants:
ooda_r:
  observe:
    must_read:
    facts_to_collect:
  orient:
    state_impact:
    risks:
    boundaries:
  decide:
    required_plan:
    write_scope:
    test_plan:
  act:
    allowed_changes:
    forbidden_changes:
  review:
    required_checks:
    evidence:
agent_role:
write_scope:
approval_required:
acceptance:
regression_points:
```

## Test Matrix

```yaml
test_id:
transition_id:
from_state:
trigger:
guard:
expected_to_state:
forbidden_states:
side_effects:
invariants:
failure_or_compensation:
ui_expectation:
test_type: unit | integration | e2e | manual
priority: P0 | P1 | P2
evidence:
```

P0 transition minimum tests:

- Legal transition.
- Illegal transition rejection.
- Guard behavior.
- Side-effect idempotency.
- Failure recovery, compensation, or exit.
- Refresh/restart canonical state.
- Read model consistency.

## Regression Card

```yaml
bug_id:
raw_report:
broken_transition:
broken_invariant:
expected_state:
actual_state:
state_source:
read_model:
root_cause:
fix_strategy:
regression_tests:
manual_verification:
release_evidence:
```

## Release Gate

```yaml
release_status: ready | release_gate_failed
traceability:
  raw_input:
  demand_id:
  transition_id:
  req_id:
  architecture_owner:
  task_id:
  test_id:
  release_evidence:
checks:
  states_registered:
  transitions_registered:
  p0_legal_tests:
  p0_illegal_tests:
  p0_guard_tests:
  failure_recovery:
  mvp_boundary_clean:
  canonical_read_model:
  rollback_plan:
  observability_plan:
blockers:
```

