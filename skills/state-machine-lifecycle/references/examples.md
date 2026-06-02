# Examples

Use these examples to shape outputs without loading the full methodology document.

## Feature

Input:

```text
加一个导出报告功能。
```

Output shape:

```yaml
mode: decompose
status: ready
object: Report
state_path:
  - Report.ready -> Report.export_requested
  - Report.export_requested -> Report.exporting
  - Report.exporting -> Report.exported
  - Report.exporting -> Report.export_failed
illegal_transitions:
  - Report.draft cannot export
  - Report.exporting cannot start duplicate export for same request
mvp:
  entry: Report.ready
  success_exit: Report.exported
  failure_exit: Report.export_failed
  non_goals:
    - batch export
    - scheduled export
```

## Bug

Input:

```text
任务明明失败了，列表还显示完成。
```

Output shape:

```yaml
mode: regression
status: ready
broken_transition: Task.running -> Task.failed
broken_invariant: failed must not display as completed
likely_fault:
  - read model derivation error
  - UI uses noncanonical status
regression:
  - backend canonical status is failed
  - list read model maps failed correctly
  - refresh keeps failed display
```

## Refactor

Input:

```text
把任务模块重构一下。
```

First classify:

```yaml
mode: triage
status: needs_clarification
questions:
  - 重构要解决哪个状态源问题？
  - 是否要求外部状态语义完全不变？
  - 哪些接口或测试必须保持兼容？
```

If behavior-preserving:

```yaml
status: no_state_impact
verification:
  - existing tests pass
  - API contract unchanged
  - canonical states unchanged
```

## Idea

Input:

```text
我突然想到可以智能推荐下一步。
```

Output shape:

```yaml
mode: triage
status: inbox
potential_object:
  - TaskQueue
  - UserFocus
potential_read_model: NextActionSuggestion
guard: recommendation must not automatically migrate business state
next_step: spike recommendation quality and UX before adding to MVP
```

## Project-Internal Agent

Input:

```text
做一个能自动处理用户文件的 Agent。
```

Output shape:

```yaml
mode: agent-boundary
status: needs_clarification
agent_states:
  - Created
  - Configured
  - Planning
  - AwaitingApproval
  - Running
  - ToolCalling
  - Succeeded
  - Failed
boundaries:
  workspace:
  tool_allowlist:
  network:
  memory:
  audit:
questions:
  - Agent 能读写哪些文件范围？
  - 哪些工具调用需要人工批准？
  - 是否允许写长期记忆？
```

## No State Impact

Input:

```text
把按钮文案从“提交”改成“确认提交”。
```

Output shape:

```yaml
mode: minimal
status: no_state_impact
reason: UI copy change only
invariants:
  - Button trigger unchanged
  - Business transition unchanged
verification:
  - visual/manual check
  - existing button behavior test if present
```
