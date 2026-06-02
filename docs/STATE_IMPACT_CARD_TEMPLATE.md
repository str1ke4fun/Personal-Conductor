# State Impact Card 模板

> 用途: Agent 相关 PR 必须附带 State Impact Card，用于 review 时检查状态影响。

---

## 1. State Impact Card

```yaml
# 用于：任何影响业务状态的代码变更
task_id: TASK-XXX
title: "简述变更内容"
state_impact:
  object: "受影响的 canonical object 名称"
  current_state: "变更前的状态描述"
  trigger: "触发变更的操作/事件"
  target_state: "变更后的状态描述"
  guard: "变更的前置条件"
  side_effects: "变更的副作用"
  illegal_transitions: "禁止的状态转换"
  recovery: "回滚方案"
evidence_level: observed | inferred | proposed
```

**示例：**
```yaml
task_id: TASK-001
title: "Shell 安全加固：blocklist → allowlist"
state_impact:
  object: ToolCall / shell::security
  current_state: blocklist + 子串匹配，working_dir 未校验
  trigger: 任何 shell 命令执行
  target_state: allowlist + working_dir 校验 + 环境变量展开拦截
  guard: 命令必须在 WorkspaceScope.allowed_commands 内
  side_effects: 现有绕过路径被阻断
  illegal_transitions: blocklist 模式不能回退
  recovery: 需要迁移现有 allowlist 配置
evidence_level: observed
```

---

## 2. Tool Registration Card

```yaml
# 用于：注册新工具或修改现有工具属性
tool_id: "tool.name"
risk_level: ReadOnly | DraftOnly | WorkspaceWrite | ExternalSideEffect | Destructive
workspace_required: true | false
permissions: ["read", "write", "execute"]
audit_events:
  - event: "tool_call.proposed"
    when: "执行前"
  - event: "tool_call.finished"
    when: "执行后"
failure_handling: "错误处理策略"
```

**示例：**
```yaml
tool_id: "file.write"
risk_level: WorkspaceWrite
workspace_required: true
permissions: ["write"]
audit_events:
  - event: "tool_call.proposed"
    when: "写入前"
  - event: "tool_call.finished"
    when: "写入后（成功或失败）"
failure_handling: "返回错误，不创建 Proposal"
```

---

## 3. Agent Dispatch Card

```yaml
# 用于：派发 Agent 任务
task_id: "TASK-XXX"
agent: "agent-name"
ooda_phase: assigned | observing | orienting | deciding | acting | reviewing
write_scope:
  - "path/to/file1.rs"
  - "path/to/file2.rs"
forbidden_scope:
  - "path/to/forbidden.rs"
expected_output:
  - "预期输出 1"
  - "预期输出 2"
acceptance:
  - "验收标准 1"
  - "验收标准 2"
blocked_by: []
estimated_effort: "Xh"
```

---

## 4. Memory Write Card

```yaml
# 用于：Agent 写入记忆
key: "memory_key"
category: "category_name"
source: user | tool | inferred
sensitivity: normal | private | secret
scope: global | workspace | document | session
expected_status: active | candidate
guard: "写入前置条件"
```

**示例：**
```yaml
key: "user_preferred_language"
category: "preferences"
source: user
sensitivity: normal
scope: global
expected_status: active
guard: "用户显式设置"
```

---

## 使用说明

1. **何时使用**: 任何涉及 canonical object 状态变更、新工具注册、Agent 派发、记忆写入的 PR
2. **放在哪里**: PR 描述中，或作为独立文档链接
3. **谁负责**: PR 作者填写，Review Agent 检查
4. **检查清单**:
   - [ ] state_impact.object 是否为 canonical object？
   - [ ] 是否有非法转换？
   - [ ] guard 是否充分？
   - [ ] side_effects 是否评估？
   - [ ] recovery 方案是否存在？
