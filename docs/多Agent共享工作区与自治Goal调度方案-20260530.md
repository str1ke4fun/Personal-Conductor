# 多 Agent 共享工作区与自治 Goal 调度方案

> 日期：2026-05-30  
> 定位：细粒度实现方案 / 可派工文档  
> 适用范围：长时任务、AgentTeam 调度、Claude `-p`、Codex 交互会话、工具权限、跨进程实时协作、Goal/OODA 循环  
> 核心结论：这不是普通 `/goal` 命令，而是一个 AgentOps 控制平面。`docs/workspace.md` 只能作为人类可读投影，不能作为多进程通信总线。

---

## 1. 背景与真实目标

用户期望的能力不是“启动一个长期 prompt 并定时看文件”，而是：

1. 多个 Agent 进程可以在同一个项目工作区中实时协作。
2. 系统可以长期运行一个 Goal，并在每轮任务中执行 OODA 循环。
3. Goal 能智能派工给已有 `AgentTeam`、Claude `-p`、Codex 交互会话和 Review Agent。
4. 所有任务、消息、权限、工具调用、心跳、锁、审查结论都可持久化、可恢复、可审计。
5. UI 能让用户看懂“谁在做什么、卡在哪里、下一步需要谁行动”。

因此目标架构应拆成两层：

```text
Shared Workspace Runtime
  负责多进程实时通信、持久事件账本、任务队列、租约、心跳、权限、审计、UI 投影

Goal Orchestrator
  负责长时目标的 OODA 循环、任务拆解、Agent 调度、Review Gate、重试与收敛
```

关键边界：

- Goal 不直接执行工具。
- Goal 只创建 `AgentTask`、`AgentRun`、`AgentTeam`、`PermissionRequest`、`WorkLease` 等状态对象。
- 工具执行仍必须经过统一工具接入层和权限闸门。
- `workspace.md` 是投影和人工编辑入口之一，不是运行时状态源。

---

## 2. 现有能力复用评估

### 2.1 已有基础

当前项目已经具备一部分正确底座：

| 模块 | 当前能力 | 本方案中的定位 |
|---|---|---|
| `agent_teams.rs` | 已有 `AgentTeamLifecycle`：`Draft -> Planning -> AwaitingPlanApproval -> Executing -> AwaitingReview -> Accepted/ReworkRequired -> Archived` | 作为团队级开发生命周期，不另起炉灶 |
| `agent_runs.rs` | 能启动 `claude -p`，有 `Queued/Running/Succeeded/Failed/Stopped` | 作为一次性外部 Agent 进程适配器 |
| `codex.rs` | 已有 `InteractiveAgentSessionStatus`：`Created/Starting/Ready/Running/AwaitInput/...` | 作为交互式 Agent 会话适配器 |
| `events.rs` | 已有事件记录和 `agent_run.*`、`tool_call.*`、`permission.*` 发射器 | 扩展为运行时事件账本 |
| `chat/session.rs` | 会话摘要已有 `working_since/working_elapsed_ms/working_stage/active_tool_count` | 可复用到 Goal UI 的工作状态展示 |
| `docs/workspace.md` | 已有人类可读 Dispatch Board 雏形 | 改为 Runtime 的 Markdown 投影 |

### 2.2 当前缺口

| 缺口 | 影响 |
|---|---|
| 缺少跨进程实时通信协议 | Claude/Codex/AgentTeam/UI 之间只能靠进程输出或文件间接感知 |
| 缺少共享任务队列和任务租约 | 多 Agent 并行时无法可靠 claim、续租、释放、恢复 |
| 缺少统一 Agent 消息总线 | `agent_team` mailbox 更偏团队内部消息，不能承载所有进程通信 |
| 缺少 GoalRun/GoalCycle | 长时目标没有可恢复的 OODA 状态 |
| 缺少 durable heartbeat | 用户无法判断 Agent 是还在工作、卡住、退出还是等待输入 |
| 缺少统一 Projection | `workspace.md` 与真实运行状态容易漂移 |

---

## 3. 技术路线结论

### 3.1 推荐路线

```text
SQLite/EventLog = L0 canonical ledger
Local HTTP + SSE = 本机多进程实时通信
Tauri events = 桌面 UI 通知
docs/workspace.md = 人类可读投影/导入源
MCP/stdio = 外部工具或 Agent 的适配协议之一，不作为中心总线
```

MVP 推荐采用：

1. SQLite 持久化所有事实状态和事件。
2. conductor-core 内启动 localhost Runtime API。
3. 外部 Agent 通过 localhost HTTP 写入消息、心跳、任务结果、权限请求。
4. 外部 Agent 通过 SSE 订阅工作区事件。
5. Tauri 前端订阅同一事件流或由后端转发 Tauri event。
6. Projection Writer 根据 SQLite 账本生成 `docs/workspace.md`。

### 3.2 技术路线对比

| 方案 | 优点 | 问题 | 结论 |
|---|---|---|---|
| Markdown 文件轮询 | 简单、可人工读写 | 无实时性、无并发控制、无 ack、无权限、易冲突 | 不作为运行时总线 |
| SQLite only | 持久、事务、易审计 | 进程间通知弱，需要轮询或触发器桥接 | 作为 L0 账本 |
| SQLite + Local HTTP/SSE | 持久 + 实时 + 易调试 + 跨语言 | 需要本地服务生命周期和 token | MVP 推荐 |
| WebSocket | 双向实时更强 | 状态恢复、断线重连复杂度高于 SSE | P1 可加，用于交互式 Codex |
| Named Pipe | 本机 IPC 性能好 | Windows/Linux 差异大，调试差 | P2 可用于低层优化 |
| Redis/NATS | 适合多机/高并发 | 桌面单机过重，引入外部依赖 | 暂不采用 |
| MCP/stdio | 生态兼容，适合工具服务器 | 不适合作为所有 Agent 的中心协作总线 | 作为 Adapter |

---

## 4. 总体架构

```text
Desktop UI
  ├─ Goal Console
  ├─ Agent Lanes
  ├─ Review Queue
  └─ Workspace Projection View
        │ Tauri event / invoke
        ▼
conductor-core
  ├─ Shared Workspace Runtime
  │   ├─ Runtime API: HTTP + SSE
  │   ├─ SQLite State Ledger
  │   ├─ Event Log
  │   ├─ Message Bus
  │   ├─ Task Queue
  │   ├─ Lease/Lock Manager
  │   ├─ Heartbeat Monitor
  │   ├─ Permission Broker
  │   └─ Projection Writer -> docs/workspace.md
  │
  └─ Goal Orchestrator
      ├─ Observe
      ├─ Orient
      ├─ Decide
      ├─ Act
      └─ Review
        │
        ├─ AgentTeam Adapter
        ├─ Claude -p Adapter
        ├─ Codex Adapter
        ├─ Review Agent Adapter
        └─ Tool/Permission Adapter

External Processes
  ├─ claude -p worker
  ├─ codex interactive session
  ├─ agentteam workers
  └─ future MCP / custom skill workers
```

---

## 5. 核心对象模型

### 5.1 WorkspaceRuntime

一个项目工作区的运行时实例。

```yaml
WorkspaceRuntime:
  id: string
  workspace_id: string
  root_path: string
  status: starting | running | degraded | stopping | stopped | failed
  api_bind: 127.0.0.1
  api_port: number
  auth_token_hash: string
  started_at: datetime
  updated_at: datetime
  stopped_at: datetime?
```

职责：

- 管理本机 Runtime API。
- 管理事件广播。
- 维护 workspace 级心跳、任务队列、投影。
- 不是 Agent，不负责推理和派工。

### 5.2 GoalRun

长时目标的顶层状态对象。

```yaml
GoalRun:
  id: string
  workspace_id: string
  title: string
  objective: text
  status:
    draft
    planning
    awaiting_plan_approval
    running
    awaiting_review
    accepted
    rework_required
    blocked
    failed
    cancelled
    archived
  priority: p0 | p1 | p2 | p3
  owner: user | system
  budget_json:
    max_cycles: number?
    max_wall_time_minutes: number?
    max_agent_runs: number?
    max_tool_calls: number?
  policy_json:
    auto_dispatch: boolean
    require_plan_approval: boolean
    require_review: boolean
    max_parallel_agents: number
    write_scope: string[]
    allowed_connectors: string[]
  current_cycle_id: string?
  created_at: datetime
  updated_at: datetime
  finished_at: datetime?
  metadata_json: object?
```

状态迁移：

```text
draft
  -> planning
  -> awaiting_plan_approval
  -> running
  -> awaiting_review
  -> accepted
  -> archived

awaiting_plan_approval -> rework_required -> planning
running -> blocked -> running
running -> failed
running -> cancelled
awaiting_review -> rework_required -> planning
```

约束：

- `running` 前必须有已批准或自动批准的 `DispatchPlan`。
- `accepted` 前必须通过 Review Gate，除非 `policy.require_review=false`。
- `archived` 是终态。

**补充迁移**:

| From | To | Guard |
|------|----|-------|
| blocked | failed | 超时或不可恢复 |
| blocked | cancelled | 用户取消 |
| failed | archived | 归档 |
| cancelled | archived | 归档 |

### 5.3 GoalCycle

一次 OODA 循环。

```yaml
GoalCycle:
  id: string
  goal_id: string
  cycle_no: number
  status:
    observing
    orienting
    deciding
    dispatching
    executing
    reviewing
    summarizing
    completed
    failed
    blocked
  observe_snapshot_ref: string?
  orientation_json: object?
  dispatch_plan_id: string?
  review_summary_ref: string?
  started_at: datetime
  updated_at: datetime
  finished_at: datetime?
```

每轮必须产出：

- 当前状态快照。
- 风险和依赖判断。
- 派工计划或阻塞理由。
- 执行结果。
- Review 结论。
- 下一轮建议。

**状态机**:

```
observing → orienting → deciding → dispatching → executing → reviewing → summarizing → completed
    ↓           ↓          ↓           ↓            ↓          ↓
  failed      failed     failed      failed       failed     failed
    ↓           ↓          ↓           ↓            ↓          ↓
  blocked     blocked    blocked     blocked      blocked    blocked
    ↓           ↓          ↓           ↓            ↓          ↓
  cancelled   cancelled  cancelled   cancelled    cancelled  cancelled
```

**合法迁移**:

| From | To | Guard | Side Effect |
|------|----|-------|-------------|
| observing | orienting | observation 数据完整 | - |
| orienting | deciding | orientation 分析完成 | - |
| deciding | dispatching | decision 产出 DispatchPlan | emit goal_cycle.decided |
| dispatching | executing | 所有 AgentTask 已创建 | emit goal_cycle.dispatched |
| executing | reviewing | 所有 AgentTask 终态 | emit goal_cycle.executed |
| reviewing | summarizing | review 完成 | - |
| summarizing | completed | summary 写入 | emit goal_cycle.completed |
| * | failed | 不可恢复错误 | emit goal_cycle.failed |
| * | blocked | 外部依赖阻塞 | emit goal_cycle.blocked |
| blocked | {previous} | 依赖解除 | - |
| * | cancelled | 用户/系统取消 | emit goal_cycle.cancelled |

**非法迁移**: observing → executing (跳过 orient/decide/dispatch), dispatching → completed (跳过 execute/review)

### 5.4 DispatchPlan

GoalCycle 的决策产物。

```yaml
DispatchPlan:
  id: string
  goal_id: string
  cycle_id: string
  status:
    draft
    awaiting_approval
    approved
    rejected
    active
    completed
    superseded
  summary: text
  tasks_json: AgentTaskDraft[]
  risk_json:
    write_conflicts: object[]
    permission_risks: object[]
    external_side_effects: object[]
  approval_message_id: string?
  created_at: datetime
  approved_at: datetime?
```

原则：

- 派工计划是“将要发生什么”的契约。
- 计划批准后才能进入 `dispatching/executing`。
- 计划变更必须生成新版本，旧版本 `superseded`。

**状态机**:

```
draft → awaiting_approval → approved → active → completed
  ↓           ↓                ↓
superseded  rejected        superseded
```

**合法迁移**:

| From | To | Guard | Side Effect |
|------|----|-------|-------------|
| draft | awaiting_approval | plan 完整 | emit dispatch_plan.submitted |
| awaiting_approval | approved | user/system approval | emit dispatch_plan.approved |
| awaiting_approval | rejected | user/system rejection | emit dispatch_plan.rejected |
| approved | active | execution 开始 | - |
| active | completed | 所有 task 完成 | emit dispatch_plan.completed |
| * | superseded | 新 plan 替代 | - |

**非法迁移**: draft → active (跳过审批), rejected → approved (需重新提交)

### 5.5 AgentTask

可被 Agent 认领和执行的原子任务。

```yaml
AgentTask:
  id: string
  workspace_id: string
  goal_id: string?
  cycle_id: string?
  parent_task_id: string?
  title: string
  instruction: text
  status:
    proposed
    queued
    claimed
    running
    awaiting_permission
    awaiting_input
    review_ready
    accepted
    rework_required
    blocked
    failed
    cancelled
  agent_kind:
    claude_p
    codex_interactive
    agent_team
    review_agent
    test_agent
    human
  assigned_agent_id: string?
  claimed_by: string?
  write_scope_json: string[]
  read_scope_json: string[]
  allowed_tools_json: string[]
  dependencies_json: string[]
  acceptance_json: string[]
  result_ref: string?
  error: text?
  created_at: datetime
  updated_at: datetime
  claimed_at: datetime?
  finished_at: datetime?
```

任务粒度要求：

- 一个任务应有明确输入、边界、验收标准。
- 一个任务不应同时修改多个无关模块。
- 高风险任务必须显式声明写范围和权限。
- Review/Test 可以是独立任务，而不是执行任务的尾注。

**补充迁移**:

| From | To | Guard |
|------|----|-------|
| rework_required | queued | 重新排队 |
| blocked | failed | 超时或不可恢复 |
| blocked | cancelled | 用户取消 |

### 5.6 AgentRunRef

统一引用已有执行体，不重复替代现有表。

```yaml
AgentRunRef:
  id: string
  task_id: string
  kind: agent_run | interactive_session | agent_team | command_run
  ref_id: string
  status_cache: string
  status_mirror: string?          # 运行时状态镜像 (from adapter sync)
  status_mirror_at: datetime?     # 最后同步时间
  started_at: datetime
  updated_at: datetime
  finished_at: datetime?
```

`status_mirror` 取值：`starting` | `running` | `paused` | `completed` | `failed` | `cancelled`

与 `status_cache` 的区别：`status_cache` 是 Runtime 自身维护的任务调度状态；`status_mirror` 是 Adapter 从真实执行体（进程/会话）同步过来的运行时状态。Observe 阶段应优先读取 `status_mirror`，仅在 Adapter 无同步能力时 fallback 到 `status_cache`。

映射关系：

| `kind` | 现有对象 |
|---|---|
| `agent_run` | `agent_runs.rs` 中的 `AgentRun` |
| `interactive_session` | `codex.rs` 中的 `InteractiveAgentSession` |
| `agent_team` | `agent_teams.rs` 中的 `AgentTeam` |
| `command_run` | 后续新增 `CommandRun` |

### 5.7 AgentMessage

跨进程共享消息。

```yaml
AgentMessage:
  id: string
  workspace_id: string
  goal_id: string?
  cycle_id: string?
  task_id: string?
  sender_id: string
  recipient_id: string?      # null = broadcast
  topic: string              # e.g. task.claimed, review.requested
  kind:
    message
    event
    question
    answer
    status
    artifact
    review
    permission_request
    permission_response
  content: text
  payload_json: object?
  created_at: datetime
  read_at: datetime?
```

说明：

- `agent_team` mailbox 可保留，但 Runtime 级消息应成为跨适配器通用消息。
- AgentTeam 内部消息可桥接为 Runtime Message，方便 UI 和 Goal 统一观察。

### 5.8 WorkLease

任务认领和写作用域锁。

```yaml
WorkLease:
  id: string
  workspace_id: string
  holder_id: string
  task_id: string?
  lease_type: task_claim | write_scope | command | review
  scope_json: string[]
  status: active | released | expired | revoked
  ttl_seconds: number
  acquired_at: datetime
  renewed_at: datetime
  expires_at: datetime
  released_at: datetime?
```

锁策略：

- `task_claim`：同一任务只能被一个 holder 认领。
- `write_scope`：重叠路径需要按策略阻断或警告。
- `review`：同一 Review Gate 只能有一个有效 verdict。

### 5.9 Heartbeat

进程活性和工作状态。

```yaml
AgentHeartbeat:
  id: string
  workspace_id: string
  agent_id: string
  process_id: number?
  task_id: string?
  goal_id: string?
  status:
    idle
    observing
    planning
    working
    awaiting_permission
    awaiting_input
    reviewing
    blocked
    stopping
  stage_label: string?
  progress_text: text?
  active_tool_count: number
  last_event_id: string?
  created_at: datetime
  expires_at: datetime
```

UI 使用规则：

- 如果 `expires_at > now` 且状态非 idle，显示 `Working mm:ss`。
- 如果心跳过期，显示 `stale`，并提示可恢复或重派。
- 左侧会话列表应优先显示 working 时长，而不是“几分钟前”。

### 5.10 RuntimeEvent / AuditEvent

所有状态变化都进入事件账本。

```yaml
RuntimeEvent:
  id: string
  workspace_id: string
  source: runtime | goal | agent | tool | ui | system
  actor_id: string
  event_type: string
  subject_type: string
  subject_id: string
  parent_event_id: string?
  payload_json: object
  created_at: datetime
```

事件命名示例：

```text
goal.created
goal.phase_changed
goal.cycle_started
goal.cycle_completed
dispatch_plan.created
dispatch_plan.approved
task.queued
task.claimed
task.phase_changed
agent.heartbeat
agent.message_posted
lease.acquired
lease.renewed
lease.expired
permission.requested
permission.approved
tool_call.proposed
tool_call.finished
review.requested
review.verdict
projection.workspace_md_written
```

---

### 5.11 ToolCall 对象模型

**Status 状态机**:
```
proposed → risk_classified → [awaiting_permission?] → approved → executing → succeeded/failed → recorded
                                                            ↓
                                                        denied → blocked
```

**字段**:
| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID | 主键 |
| workspace_id | UUID | 所属工作区 |
| task_id | UUID? | 关联 AgentTask（可空，用户直接发起时） |
| tool_id | String | 工具标识 (e.g. bash.execute, file.write) |
| input_json | JSON | 工具输入参数 |
| risk_level | String | low/medium/high/critical |
| status | String | 状态机当前态 |
| output_ref | String? | 输出引用（执行后填充） |
| created_at | DateTime | 创建时间 |
| finished_at | DateTime? | 完成时间 |

**合法迁移**:
| From | To | Guard |
|------|----|-------|
| proposed | risk_classified | risk_classifier 完成 |
| risk_classified | awaiting_permission | risk_level >= medium |
| risk_classified | approved | risk_level < medium (auto-approve) |
| awaiting_permission | approved | user/system approval |
| awaiting_permission | denied | user/system denial |
| approved | executing | executor 开始 |
| executing | succeeded | 执行成功 |
| executing | failed | 执行失败 |
| succeeded | recorded | 持久化完成 |
| failed | recorded | 持久化完成 |

**非法迁移**: proposed → executing (跳过风险评估), awaiting_permission → executing (跳过审批)

**Guard**: risk_classified → awaiting_permission 仅当 risk_level >= medium; approved → executing 仅当 executor 可用

**Side Effects**: executing → succeeded 时触发 output 写入 + 审计事件; awaiting_permission → approved 时触发 permission.approved 事件

---

### 5.12 PermissionRequest 对象模型

**Status 状态机**:
```
pending → approved
   ↓        ↓
 denied   revoked
```

**字段**:
| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID | 主键 |
| workspace_id | UUID | 所属工作区 |
| task_id | UUID? | 关联 AgentTask |
| requester_id | String | 请求者 (agent_id or "user") |
| scope_json | JSON | 权限范围 (tool_ids, paths, duration) |
| risk_level | String | low/medium/high/critical |
| reason | String | 请求原因 |
| approver_id | String? | 审批者 |
| approved_at | DateTime? | 审批时间 |

**合法迁移**:
| From | To | Guard |
|------|----|-------|
| pending | approved | approver 授权 |
| pending | denied | approver 拒绝 |
| approved | revoked | 超时/主动撤销 |

**非法迁移**: denied → approved (需重新申请), revoked → approved (需重新申请)

**Guard**: pending → approved 仅当 approver 有权限且 scope 合法

**Side Effects**: pending → approved 时触发 permission.approved 审计事件 + 写入 capability_grants; approved → revoked 时触发 permission.revoked 审计事件 + 撤销 capability_grants

---

## 6. SQLite 表设计

### 6.1 新增表清单

```text
workspace_runtimes
goal_runs
goal_cycles
dispatch_plans
agent_tasks
agent_run_refs
agent_messages
work_leases
agent_heartbeats
runtime_events
workspace_projection_state
```

### 6.2 索引要求

```sql
CREATE INDEX idx_goal_runs_workspace_status
ON goal_runs(workspace_id, status, updated_at DESC);

CREATE INDEX idx_goal_cycles_goal_no
ON goal_cycles(goal_id, cycle_no DESC);

CREATE INDEX idx_agent_tasks_workspace_status
ON agent_tasks(workspace_id, status, updated_at DESC);

CREATE INDEX idx_agent_tasks_goal_cycle
ON agent_tasks(goal_id, cycle_id, status);

CREATE INDEX idx_agent_messages_workspace_created
ON agent_messages(workspace_id, created_at DESC);

CREATE INDEX idx_agent_messages_task
ON agent_messages(task_id, created_at ASC);

CREATE INDEX idx_work_leases_active
ON work_leases(workspace_id, status, expires_at);

CREATE INDEX idx_agent_heartbeats_workspace
ON agent_heartbeats(workspace_id, agent_id, expires_at);

CREATE INDEX idx_runtime_events_workspace_created
ON runtime_events(workspace_id, created_at DESC);

CREATE INDEX idx_runtime_events_subject
ON runtime_events(subject_type, subject_id, created_at ASC);
```

### 6.3 事务边界

必须放在同一事务内的操作：

| 操作 | 同事务内容 |
|---|---|
| 创建任务 | `agent_tasks` insert + `runtime_events.task.queued` |
| 认领任务 | 检查状态 + 更新 `claimed` + 创建 `task_claim` lease + 发事件 |
| 获取写锁 | 检查重叠 active lease + insert lease + 发事件 |
| 完成任务 | 更新任务状态 + 写 result_ref + 释放 lease + 发事件 |
| 计划批准 | 更新 `dispatch_plans` + 更新 `goal_runs` + 发事件 |
| Review verdict | 写 verdict message + 更新目标/任务状态 + 发事件 |

---

## 7. Runtime API 设计

### 7.1 认证

Runtime API 只绑定 `127.0.0.1`，但仍需要 bearer token。

```text
Authorization: Bearer <workspace_runtime_token>
X-Agent-Id: claude-plan-001
X-Agent-Kind: claude_p
```

token 来源：

- Runtime 启动时生成，写入仅当前用户可读的 state 文件。
- 由系统启动的 Claude/Codex/AgentTeam 子进程通过环境变量获得。
- 用户手工外接进程需要在 UI 中显式复制短期 token。

### 7.2 Endpoint 草案

```text
GET  /runtime/health
GET  /runtime/events?workspace_id=...&since_event_id=...       # SSE

POST /runtime/heartbeats
POST /runtime/messages
GET  /runtime/messages?workspace_id=...&topic=...

GET  /runtime/goals
POST /runtime/goals
POST /runtime/goals/{goal_id}/start
POST /runtime/goals/{goal_id}/pause
POST /runtime/goals/{goal_id}/cancel
POST /runtime/goals/{goal_id}/approve-plan
POST /runtime/goals/{goal_id}/review-verdict

GET  /runtime/tasks?workspace_id=...&status=queued
POST /runtime/tasks
POST /runtime/tasks/{task_id}/claim
POST /runtime/tasks/{task_id}/heartbeat
POST /runtime/tasks/{task_id}/complete
POST /runtime/tasks/{task_id}/fail
POST /runtime/tasks/{task_id}/block
POST /runtime/tasks/{task_id}/release

POST /runtime/leases/acquire
POST /runtime/leases/{lease_id}/renew
POST /runtime/leases/{lease_id}/release

POST /runtime/permissions/request
POST /runtime/permissions/{request_id}/approve
POST /runtime/permissions/{request_id}/deny

POST /runtime/tool-calls/proposed
POST /runtime/tool-calls/{tool_call_id}/started
POST /runtime/tool-calls/{tool_call_id}/finished

POST /runtime/projections/workspace-md/regenerate
```

### 7.3 SSE 事件格式

```json
{
  "event_id": "ev-20260530-000001",
  "workspace_id": "ws-main",
  "event_type": "task.phase_changed",
  "subject_type": "agent_task",
  "subject_id": "task-001",
  "actor_id": "claude-architect",
  "payload": {
    "from": "queued",
    "to": "claimed",
    "title": "设计 Runtime API migration"
  },
  "created_at": "2026-05-30T12:00:00Z"
}
```

客户端断线重连：

1. 客户端保存最后 `event_id`。
2. 重连时带 `since_event_id`。
3. Runtime 先补发 SQLite 中缺失事件，再进入实时订阅。

---

## 8. Goal OODA 调度循环

### 8.1 循环总览

```text
Observe
  读取 GoalRun、上一轮 Cycle、任务队列、Agent 心跳、租约、权限、最近事件、代码/文档状态

Orient
  判断目标差距、依赖关系、风险、写冲突、Agent 适配度、是否需要用户澄清

Decide
  生成 DispatchPlan：任务拆解、Agent 分配、写范围、验收标准、Review 策略

Act
  创建 AgentTask / AgentRun / AgentTeam / Codex Session / PermissionRequest / WorkLease

Review
  收集输出、测试、diff、审查意见，决定 accepted / rework_required / blocked / next_cycle
```

### 8.2 Observe 输入

每轮 Observe 必须读取：

- `goal_runs` 当前状态。
- `goal_cycles` 最近 3 轮结果。
- 当前未完成 `agent_tasks`。
- active `work_leases`。
- active `agent_heartbeats`。
- 最近 `runtime_events`。
- 相关 `agent_messages`。
- 权限请求队列。
- Review 队列。
- 可选：`git status --short`、测试结果摘要、`docs/workspace.md` 投影状态。

### 8.3 Orient 判断

Orient 产物需要结构化记录：

```yaml
orientation:
  goal_gap: 当前距离目标还缺什么
  blockers:
    - type: permission | user_input | conflict | failed_test | missing_context
      detail: string
  dependencies:
    - before_task_id: string
      after_task_id: string
  risks:
    - type: write_conflict | external_side_effect | long_running | ambiguous_requirement
      severity: low | medium | high
  agent_fit:
    - agent_kind: claude_p
      fit: planning | implementation | review | low
      reason: string
```

### 8.4 Decide 派工策略

默认策略：

| 任务类型 | 首选执行体 | 说明 |
|---|---|---|
| 方案/拆解/审查 | Claude `-p` 或 Review Agent | 一次性产物，适合非交互 |
| 代码落地 | Codex interactive 或 AgentTeam | 需要多轮工具和修复 |
| 多模块并行 | AgentTeam | 需要 write_scope 和 Review Gate |
| 高风险命令 | 先生成 PermissionRequest | 不允许 Goal 直接执行 |
| UI 体验核验 | Codex + Playwright/人工检查任务 | 需要截图和用户感知反馈 |
| 外部系统操作 | Connector Adapter | 必须走能力授权 |

### 8.5 Act 执行规则

Goal Orchestrator 的 Act 只允许做这些事：

- 创建 `AgentTask`。
- 创建或复用 `AgentTeam`。
- 启动 `AgentRun` 或 `InteractiveAgentSession`。
- 申请 `WorkLease`。
- 申请 `PermissionRequest`。
- 发送 `AgentMessage`。
- 写入 `RuntimeEvent`。

禁止：

- 直接调用文件写入工具。
- 直接执行 shell 命令。
- 绕过权限闸门启动外部副作用。
- 用 Markdown 文件内容覆盖 L0 状态。

### 8.6 Review Gate

Review Gate 至少包含：

```yaml
review_gate:
  required: true
  reviewers:
    - review_agent
    - test_agent?
    - human?
  evidence:
    - changed_files
    - test_results
    - task_acceptance_checklist
    - risk_summary
  verdict:
    accepted | rework_required | blocked | failed
```

Review Agent 输出必须绑定状态迁移：

| Verdict | 状态变化 |
|---|---|
| `accepted` | task -> accepted；如果所有任务 accepted，则 cycle -> completed |
| `rework_required` | task -> rework_required；Goal 进入下一轮 planning |
| `blocked` | task/goal -> blocked；等待用户、权限或外部状态 |
| `failed` | task/goal -> failed 或生成恢复任务 |

---

## 9. Adapter 设计

### 9.1 Claude `-p` Adapter

适用场景：

- 方案拆解。
- Review。
- 文档生成。
- 单轮代码审查。
- 无需交互式工具循环的任务。

实现：

1. 从 `AgentTask` 生成 prompt。
2. 注入 Runtime API token、workspace_id、task_id。
3. 通过现有 `start_claude_run` 启动。
4. stdout/stderr 写入 sidecar。
5. 完成后把结果写回 `AgentTask.result_ref`。
6. 发 `task.review_ready` 或 `task.failed`。

需要补强：

- `AgentRunStatus` 只能表达进程状态，需额外维护 `AgentTask.status`。
- `claude -p` 输出需要结构化约束：summary、changes、risks、next_steps。
- 如果 Claude 需要工具能力，不应直接获得全部系统工具，应通过 Runtime API 请求任务或权限。

### 9.2 Codex Interactive Adapter

适用场景：

- 代码落地。
- 多轮修复。
- 需要命令、测试、文件编辑的任务。
- 需要中断/恢复的长时任务。

实现：

1. 创建或复用 `InteractiveAgentSession`。
2. 将 `AgentTask` 指令发送到 Codex stdin。
3. 解析 Codex 输出中的 Running/Ran/tool transcript。
4. 将活动转换为 `RuntimeEvent` 和 `AgentHeartbeat`。
5. Codex 完成或等待输入时更新任务状态。

P0 要求：

- 每个 Codex session 必须绑定 `workspace_id` 和 `task_id`。
- Codex session 必须持续 heartbeat。
- 中断后任务进入 `awaiting_input` 或 `blocked`，不能静默消失。
- 文件写范围需要和 `WorkLease` 对齐。

### 9.3 AgentTeam Adapter

适用场景：

- 多角色并行：架构、实现、测试、审查。
- 多模块拆分。
- 需要计划批准和 Review Gate 的复杂任务。

复用现有：

```text
Draft -> Planning -> AwaitingPlanApproval -> Executing -> AwaitingReview -> Accepted/ReworkRequired -> Archived
```

补强要求：

- AgentTeam 创建时必须绑定 `goal_id/cycle_id`。
- 每个 member 需要 `read_scope/write_scope/allowed_tools`。
- AgentTeam mailbox 消息桥接到 Runtime `AgentMessage`。
- AgentTeam 生命周期变化写入 `RuntimeEvent`。
- Review verdict 反向驱动 GoalCycle。

### 9.4 Review Agent Adapter

Review Agent 是独立执行体，不能由执行 Agent 自审通过。

输入：

- 任务指令。
- 任务输出。
- diff 或 changed files。
- 测试结果。
- 风险清单。
- 验收标准。

输出：

```yaml
verdict: accepted | rework_required | blocked | failed
findings:
  - severity: critical | high | medium | low
    file: string?
    line: number?
    issue: string
    recommendation: string
residual_risk: string
next_action: string
```

### 9.5 Tool/Permission Adapter

原则：

- Skill 或 Goal 不直接授予工具。
- Connector/Tool 能力由 capability manifest 和 Permission Broker 控制。
- 每次高风险调用都必须进入 `ToolCall` 和 `PermissionRequest` 账本。

工具调用链路：

```text
ToolCall.Proposed
  -> RiskClassified
  -> AwaitPermission? 
  -> Approved/Denied
  -> Executing
  -> Succeeded/Failed
  -> Recorded
```

### 9.6 Adapter 状态同步协议

**目的**：`AgentRunRef.status_mirror` 反映 `AgentRun` / `InteractiveAgentSession` 的真实状态，Observe 阶段读取 `status_mirror` 而非轮询实际进程。

**同步时机**：

| Adapter | 触发条件 | 同步内容 |
|---------|---------|---------|
| Claude `-p` | process exit / stdout line | `status_mirror` = `completed` / `failed`，`status_mirror_at` = now |
| Codex Interactive | session state change event | `status_mirror` = `running` / `paused` / `completed` / `failed` |
| AgentTeam | lifecycle hook (`on_start` / `on_complete` / `on_fail`) | `status_mirror` = `starting` / `running` / `completed` / `failed` |

**Observe 输入清单**：

- `AgentRunRef.status_mirror`（而非 pid / process handle）
- `AgentRunRef.status_mirror_at`（判断是否超时）
- `AgentRunRef.exit_code`（仅 `completed` / `failed` 时有意义）

**超时判定**：若 `status_mirror_at` 距今 > `expected_duration * 2`，标记为 `timeout`。

---

## 10. 多进程通信协议

### 10.1 Agent 注册

外部 Agent 启动后先注册：

```http
POST /runtime/agents/register
```

```json
{
  "agent_id": "codex-impl-001",
  "agent_kind": "codex_interactive",
  "workspace_id": "ws-main",
  "capabilities": ["code_edit", "test_run", "review"],
  "pid": 12345
}
```

返回：

```json
{
  "ok": true,
  "heartbeat_interval_seconds": 10,
  "event_stream_url": "/runtime/events?workspace_id=ws-main"
}
```

### 10.2 任务认领

```http
POST /runtime/tasks/task-001/claim
```

```json
{
  "agent_id": "codex-impl-001",
  "lease_ttl_seconds": 60,
  "requested_write_scope": ["crates/conductor-core/src/runtime.rs"]
}
```

可能返回：

| 状态 | 说明 |
|---|---|
| `claimed` | 成功认领 |
| `conflict` | 写范围与 active lease 冲突 |
| `already_claimed` | 任务被其他 Agent 持有 |
| `permission_required` | 需要用户授权 |

### 10.3 心跳

```http
POST /runtime/heartbeats
```

```json
{
  "agent_id": "codex-impl-001",
  "workspace_id": "ws-main",
  "task_id": "task-001",
  "status": "working",
  "stage_label": "running cargo check",
  "progress_text": "修复 runtime API 编译错误",
  "active_tool_count": 1,
  "last_event_id": "ev-000120",
  "ttl_seconds": 30
}
```

### 10.4 消息

```http
POST /runtime/messages
```

```json
{
  "workspace_id": "ws-main",
  "goal_id": "goal-001",
  "task_id": "task-001",
  "sender_id": "review-agent-001",
  "recipient_id": "codex-impl-001",
  "topic": "review.rework_required",
  "kind": "review",
  "content": "需要补充租约过期恢复测试。",
  "payload": {
    "severity": "medium",
    "acceptance_id": "ACC-LEASE-003"
  }
}
```

---

## 11. UI 方案

### 11.1 Goal Console

用于管理长时目标。

必须展示：

- Goal 标题、目标、状态、运行时长。
- 当前 OODA 阶段。
- 当前 Cycle 编号。
- 当前阻塞项。
- 最近事件。
- 预算使用：cycles、agent_runs、tool_calls、wall_time。
- 操作：Start、Pause、Resume、Cancel、Approve Plan、Request Review、Archive。

### 11.2 Agent Lanes

按 Agent 展示实时工作状态：

```text
Claude Planner     Working 04:12   Orienting requirements
Codex Impl         Working 11:35   Running cargo check
Review Agent       Awaiting        Waiting for task output
AgentTeam Runtime  Blocked         write_scope conflict
```

每条 lane 展示：

- Agent 类型。
- 当前任务。
- 当前阶段。
- working 时长。
- 最近工具调用。
- 最近消息。
- 心跳状态。

### 11.3 OODA Timeline

以 Cycle 为单位展示：

```text
Cycle 1
  Observe   completed
  Orient    completed
  Decide    awaiting approval
  Act       not started
  Review    not started
```

点击每一步可查看：

- 输入快照。
- 结构化判断。
- 决策依据。
- 产生的任务。
- Review 证据。

### 11.4 Review Queue

集中展示需要用户或 Review Agent 处理的事项：

- 计划批准。
- 权限请求。
- 高风险工具调用。
- Review verdict。
- 阻塞问题。
- 人工输入请求。

### 11.5 Workspace Projection View

`docs/workspace.md` 在 UI 中作为只读投影视图优先展示。

允许的人工操作：

- 从 Markdown 中导入新的 Goal 草稿。
- 将 Markdown 中人工标记的 `approved/rejected` 转成 Runtime 事件。

禁止：

- 把整份 Markdown 当作事实源覆盖 SQLite。
- 通过文件变更隐式触发高风险操作。

---

## 12. `docs/workspace.md` 投影规范

### 12.1 角色

`workspace.md` 是 L2 文档投影，服务于人类和外部 Agent 阅读。

```text
SQLite Runtime Ledger -> Projection Writer -> docs/workspace.md
```

它可以被人工编辑，但编辑结果必须通过 Importer 解析为显式命令，再写入 Runtime。

### 12.2 投影结构

```markdown
# Agent Dispatch Board

> Generated from Runtime Ledger at 2026-05-30 12:00:00
> Do not treat this file as the runtime bus.

## Active Goals

| Goal | Status | Cycle | Working | Blocker |
|---|---|---:|---:|---|
| goal-001 长时 Goal 调度 | running | 3 | 42m | none |

## Active Tasks

| Task | Agent | Status | Lease | Updated |
|---|---|---|---|---|
| task-001 Runtime API | codex-impl-001 | running | active | 12:00 |

## Review Queue

| Item | Type | Owner | Status |
|---|---|---|---|
| review-001 | plan_approval | user | awaiting |

## Recent Events

- 12:00 task.claimed task-001 by codex-impl-001
- 11:58 dispatch_plan.approved plan-001 by user
```

### 12.3 Importer 范围

Importer MVP 只支持显式命令块：

```markdown
```agent-runtime-command
type: approve_plan
goal_id: goal-001
plan_id: plan-001
reason: 人工确认
```
```

Importer 行为：

1. 解析命令块。
2. 校验 schema。
3. 展示 UI 确认。
4. 写入 Runtime API。
5. 生成 `projection.imported_command` 事件。

---

## 13. 权限与安全边界

### 13.1 权限衰减

Goal 的权限必须下放为更小的任务权限：

```text
Goal policy.write_scope
  >= DispatchPlan task write_scope
    >= AgentTask write_scope
      >= WorkLease scope
        >= ToolCall effective scope
```

任一层不能扩大上一层权限。

### 13.2 写范围归一化

所有路径必须：

- 转为 workspace root 下相对路径。
- 解析 `..`、软链接、大小写差异。
- 禁止逃逸 workspace root。
- Windows 下做大小写归一化比较。

### 13.3 冲突策略

| 冲突类型 | MVP 行为 |
|---|---|
| 两个 active write lease 路径完全相同 | 阻断后者 |
| 一个路径是另一个路径父目录 | 阻断或要求人工批准 |
| 只读任务与写任务冲突 | 允许，但 UI 提示 |
| Review 任务与写任务冲突 | 允许 Review，但禁止 Review Agent 写入同范围 |

### 13.4 高风险操作

必须进入 Permission Broker 的操作：

- 文件写入、删除、移动。
- shell 命令。
- 网络请求。
- 外部系统写操作。
- 发送消息/邮件/审批。
- 访问敏感凭据。
- 修改 Runtime 配置。

### 13.5 事故恢复

必须支持：

- Agent 心跳过期后释放 task claim lease。
- 写锁过期后自动标记 `expired`，不直接删除。
- 进程异常退出后任务进入 `blocked` 或 `failed`。
- Runtime 重启后从 SQLite 恢复 active goals/tasks/leases。
- 投影文件可重新生成。

---

## 14. 分阶段实现计划

### Phase 0：数据库与事件账本

目标：先建立 L0 状态源，不接复杂 UI。

任务：

| ID | 任务 | 验收 |
|---|---|---|
| G0-01 | 新增 runtime 相关 migration | 表和索引创建成功，重复启动不报错 |
| G0-02 | 新增 `runtime_events` 写入 API | 每个状态变化都能写事件 |
| G0-03 | 新增 GoalRun/GoalCycle CRUD | 单测覆盖创建、状态迁移、非法迁移 |
| G0-04 | 新增 AgentTask CRUD | 支持 queued/claim/complete/fail/block |
| G0-05 | 新增 WorkLease 管理 | 支持 acquire/renew/release/expire 和冲突检测 |
| G0-06 | 新增 Heartbeat 表和过期扫描 | 过期心跳能被标记并发事件 |

### Phase 1：Shared Workspace Runtime API

目标：让多个本机进程可以实时通信。

任务：

| ID | 任务 | 验收 |
|---|---|---|
| G1-01 | 在 conductor-core 增加 localhost HTTP server | Tauri 启动后可访问 `/runtime/health` |
| G1-02 | 实现 bearer token 校验 | 无 token 返回 401 |
| G1-03 | 实现 SSE `/runtime/events` | 两个客户端能同时收到事件 |
| G1-04 | 实现 messages API | 外部进程可发消息，UI 能看到 |
| G1-05 | 实现 heartbeats API | UI 可显示 working 时长 |
| G1-06 | 实现 tasks claim/complete API | 并发 claim 同任务只有一个成功 |

### Phase 2：Adapter 接入

目标：把已有 Claude/Codex/AgentTeam 接入 Runtime。

任务：

| ID | 任务 | 验收 |
|---|---|---|
| G2-01 | Claude `-p` 启动时注入 runtime env | 子进程能回写 heartbeat/message |
| G2-02 | `AgentRun` 完成后自动更新 AgentTask | 成功/失败状态一致 |
| G2-03 | Codex session 绑定 task_id/workspace_id | UI 能看到 Codex 正在执行哪个任务 |
| G2-04 | Codex 输出转 RuntimeEvent | Running/Ran/awaiting input 能进入事件流 |
| G2-05 | AgentTeam lifecycle 桥接 RuntimeEvent | 团队状态变化能驱动 GoalCycle |
| G2-06 | AgentTeam mailbox 桥接 AgentMessage | 团队消息能在统一消息流查看 |

### Phase 3：单 Goal OODA Orchestrator

目标：支持一个 Goal 的完整闭环。

任务：

| ID | 任务 | 验收 |
|---|---|---|
| G3-01 | 实现 GoalOrchestrator runner | 能启动、暂停、恢复、取消 |
| G3-02 | 实现 Observe 快照 | 快照包含任务、心跳、租约、事件 |
| G3-03 | 实现 Orient 结构化产物 | 能识别 blocker/risk/dependency |
| G3-04 | 实现 DispatchPlan 生成 | 可生成任务草案和写范围 |
| G3-05 | 实现计划审批 | 未批准不能进入 Act |
| G3-06 | 实现 Act 派发任务 | 能创建 AgentTask 并启动对应 Adapter |
| G3-07 | 实现 Review Gate | Review verdict 能改变 Goal/Cycle 状态 |

### Phase 4：多 Agent 并行与冲突治理

目标：让多个 Agent 安全并行。

任务：

| ID | 任务 | 验收 |
|---|---|---|
| G4-01 | 实现 max_parallel_agents | 超过并行上限的任务保持 queued |
| G4-02 | 实现 write_scope 冲突检测 | 重叠写范围被阻断或要求批准 |
| G4-03 | 实现任务依赖调度 | 依赖未完成时不派发 |
| G4-04 | 实现 lease 续租与过期恢复 | Agent 失联后任务可重派 |
| G4-05 | 实现失败重试策略 | 可配置 retry 次数和回退到 planning |
| G4-06 | 实现循环防护 | 相同失败原因连续 N 次后 Goal blocked |

### Phase 5：UI 控制台

目标：让用户能看懂长时任务。

任务：

| ID | 任务 | 验收 |
|---|---|---|
| G5-01 | Goal Console | 展示 goal 状态、cycle、预算、操作按钮 |
| G5-02 | Agent Lanes | 展示每个 Agent working 时长和阶段 |
| G5-03 | OODA Timeline | 展示每轮 Observe/Orient/Decide/Act/Review |
| G5-04 | Review Queue | 汇总计划审批、权限、review、阻塞 |
| G5-05 | Event Transcript | Codex/Claude Code 风格聚合事件展示 |
| G5-06 | Left Session Working Indicator | 会话列表 working 时显示运行时长 |

### Phase 6：Workspace Markdown Projection

目标：保留人类友好的派工板，但不让它承担总线职责。

任务：

| ID | 任务 | 验收 |
|---|---|---|
| G6-01 | Projection Writer | Runtime 状态变更后生成 `docs/workspace.md` |
| G6-02 | Projection section template | Active Goals/Tasks/Review Queue/Events |
| G6-03 | Importer command block parser | 只解析显式 `agent-runtime-command` |
| G6-04 | Importer UI confirmation | 导入命令必须人工确认 |
| G6-05 | Projection drift detector | 文件人工改动不直接覆盖 L0 |

### Phase 7：硬化与可观测性

目标：达到长期运行的可靠性。

任务：

| ID | 任务 | 验收 |
|---|---|---|
| G7-01 | Runtime restart recovery | 重启后恢复 active goals/tasks/leases |
| G7-02 | Audit bundle export | 可导出某 Goal 的事件、任务、消息、权限 |
| G7-03 | Incident panel | 展示失联、冲突、失败、权限拒绝 |
| G7-04 | Metrics | cycle 耗时、任务成功率、重试次数 |
| G7-05 | 数据清理策略 | archived goal 可压缩但保留审计 |

---

## 15. 测试矩阵

### 15.1 单元测试

| 测试 | 覆盖 |
|---|---|
| GoalRun 合法迁移 | 正常路径、rework、blocked、cancelled |
| GoalRun 非法迁移 | running 不能直接 accepted |
| AgentTask claim | 并发 claim 只有一个成功 |
| WorkLease overlap | 同路径、父子路径、无关路径 |
| Heartbeat expiry | 过期后事件生成，任务进入可恢复状态 |
| DispatchPlan approval | 未批准不能 Act |
| RuntimeEvent ordering | 同 workspace 事件按创建顺序可重放 |

### 15.2 集成测试

| 测试 | 验收 |
|---|---|
| 两个外部进程订阅 SSE | 同时收到 task/message/heartbeat 事件 |
| Claude `-p` 执行任务 | 任务从 queued 到 review_ready |
| Codex session 中断恢复 | 任务进入 awaiting_input/resumable |
| AgentTeam 执行与 Review | lifecycle 驱动 GoalCycle |
| Runtime 重启 | active 状态从 SQLite 恢复 |
| Projection 再生成 | 删除 `workspace.md` 后可完整再生成 |

### 15.3 端到端验收场景

场景 A：单 Agent 文档任务

1. 用户创建 Goal：生成某方案文档。
2. Orchestrator 生成 DispatchPlan。
3. 用户批准。
4. Claude `-p` 执行。
5. Review Agent 接受。
6. Goal accepted，`workspace.md` 更新。

场景 B：多 Agent 代码任务

1. 用户创建 Goal：实现 Runtime API。
2. Planner 拆成 schema/API/UI/test。
3. Codex 认领 API，Test Agent 等待依赖。
4. 写范围冲突被 Lease Manager 阻断。
5. Codex 完成后 Test Agent 运行测试。
6. Review Agent 要求 rework。
7. 下一轮 OODA 只派发修复任务。

场景 C：Agent 失联恢复

1. Codex 认领任务并持有 lease。
2. 进程退出，heartbeat 过期。
3. Runtime 标记 lease expired。
4. 任务进入 blocked/requeue。
5. Orchestrator 在下一轮决定重派或请求用户。

---

## 16. 派工建议

第一批建议只做运行时底座，不做完整智能调度：

```text
Batch 1: Runtime Ledger + API
  G0-01 ~ G0-06
  G1-01 ~ G1-06

Batch 2: Existing Adapter Bridge
  G2-01 ~ G2-06

Batch 3: Single Goal OODA
  G3-01 ~ G3-07

Batch 4: Parallel Governance + UI
  G4-01 ~ G5-06

Batch 5: Projection + Hardening
  G6-01 ~ G7-05
```

优先级判断：

1. 先做 SQLite 状态账本和 SSE，因为这是所有后续能力的共同底座。
2. 再接入已有 Claude/Codex/AgentTeam，避免设计脱离真实执行体。
3. 再做 Goal Orchestrator，否则长时任务仍没有可观察状态。
4. 最后做智能化调度增强和 UI 细化。

---

## 17. 风险与开放问题

### 17.1 主要风险

| 风险 | 缓解 |
|---|---|
| Runtime API 生命周期和 Tauri 生命周期耦合复杂 | 先只在桌面进程内启动，后续再拆 sidecar |
| 外部 Agent 不遵守协议 | 系统启动的 Agent 注入 wrapper；手工 Agent 降权 |
| SSE 在长连接下断线 | 支持 `since_event_id` 补放 |
| SQLite 写并发 | 单机桌面可控；关键操作事务化 |
| Goal 循环失控 | max_cycles、预算、连续失败阻断 |
| 权限扩大 | 逐层权限衰减和 write_scope 校验 |
| Markdown 投影被误用为总线 | 文档头部声明 + Importer 只接受显式命令块 |

### 17.2 开放问题

1. Runtime API 是否随 Tauri 主进程启动，还是作为独立 sidecar 进程启动？
2. Codex 输出解析应先做简单 transcript 聚合，还是直接引入结构化事件协议？
3. Review Agent 默认用 Claude `-p`、Codex、还是项目内 LLM chat？
4. 是否允许外部用户自定义 Agent 通过 Runtime API 注册？
5. 权限 token 的有效期和 UI 展示策略如何设定？

---

## 18. 最小可行闭环定义

MVP 完成标准：

1. 用户能创建一个 Goal。
2. Runtime 能为 Goal 创建 Cycle 和 DispatchPlan。
3. 用户能批准计划。
4. Runtime 能派发一个 AgentTask 给 Claude `-p` 或 Codex。
5. Agent 能通过 Runtime API 发送 heartbeat/message/result。
6. UI 能显示本轮 Goal 正在工作和工作时长。
7. Review Agent 或用户能给出 verdict。
8. Goal 能进入 accepted/rework_required/blocked。
9. `docs/workspace.md` 能从 Runtime 状态重新生成。
10. Runtime 重启后不会丢失 Goal、Task、Event、Message、Lease。

这个 MVP 不要求一开始就“非常聪明”。它要求先把事实账本、实时通信、状态机和可恢复性做正确。智能派工是在这个底座上逐步增强的能力。

---

## 19. 用户如何感知“更 Goal 的 Goal”

### 19.1 先区分三种工作形态

用户不应被迫理解内部对象，例如 `GoalRun`、`GoalCycle`、`AgentTask`、`AgentTeamLifecycle`。产品上应该把当前工作形态分成三类：

| 形态 | 用户理解 | 系统行为 | UI 信号 |
|---|---|---|---|
| 普通聊天 | 问一句，答一句 | 不创建长期状态，只保留聊天历史 | 普通气泡 |
| 工具回合 | 本轮需要查文件、跑命令、调工具 | 创建本轮 tool transcript，结束后回到聊天 | `Working mm:ss` + 工具 transcript |
| 长期 Goal | 这件事要持续推进、派工、复盘、可恢复 | 创建 GoalRun/Cycle/Task/Lease/Event | Goal 控制条 + Agent lanes + OODA timeline |

入口设计：

- 用户显式说“持续推进/派工/长期关注/让 agent team 做”时，创建 Goal 草稿。
- 普通聊天中检测到任务超过单轮复杂度时，提示“这更像一个长期目标，是否创建 Goal？”
- 用户可以把当前对话升级为 Goal，也可以把 Goal 降级为普通待办。

### 19.2 默认不要暴露 OODA 术语

内部仍使用 OODA，但 UI 默认展示人类语言：

| 内部阶段 | 用户可见文案 |
|---|---|
| Observe | 看上下文 |
| Orient | 判断方向 |
| Decide | 定计划 |
| Act | 派工执行 |
| Review | 验收复盘 |

高级视图中再显示 OODA 字段，方便开发者和重度用户排查。

### 19.3 Goal 顶部控制条

每个长期 Goal 在对话顶部展示一个稳定控制条：

```text
Goal: 多 Agent 共享工作区与自治调度
Running 42:18 · Cycle 3 · 派工执行 · 3 agents working · 1 review waiting

[暂停] [停止] [查看计划] [查看派工] [打开审查队列]
```

必须包含：

- 当前目标。
- 总运行时长。
- 当前 Cycle。
- 当前阶段。
- 正在工作的 Agent 数。
- 等待用户处理的事项数。
- 最近阻塞原因。
- 暂停/停止/批准/查看详情入口。

### 19.4 左侧会话列表的 Goal 感知

左侧列表不能只显示“几分钟前”。当会话内有 active Goal 或 active run 时：

```text
多 Agent Goal      Working 42:18
对话面板修复        Waiting approval
普通聊天            5 分钟前
```

优先级：

1. `Working mm:ss`
2. `Waiting approval`
3. `Blocked`
4. `Reviewing`
5. 普通更新时间

这能解决用户感知上的核心问题：不是“卡住了”，而是“仍在工作，工作多久了，卡在哪里”。

---

## 20. 用户如何感知 Agent 派工

### 20.1 像 Codex 一样显示事件，不显示内部噪音

派工感知应采用 transcript 聚合，而不是把每个工具、每个模型、每条中间消息都摊开。

推荐展示：

```text
• Planning goal
  └ 读取现有方案、代码状态和未完成任务

• Dispatching 3 tasks
  ├ Claude Planner: 细化 Runtime API migration
  ├ Codex Impl: 落地 SQLite + SSE 基础
  └ Review Agent: 等待实现完成后审查

• Running Codex Impl · Working 08:41
  └ cargo check 正在执行

• Waiting for approval
  └ Codex Impl 请求写入 crates/conductor-core/src/runtime.rs

• Ran cargo check
  └ 0 errors, 3 warnings

• Review requested
  └ Review Agent 正在检查 changed files 和验收项
```

原则：

- 默认只显示关键状态事件。
- 工具原始 stdout/stderr、prompt、模型响应、JSON payload 放在“展开详情”。
- 多个工具调用在短时间内合并为一条“Ran N tools”。
- 长时间无输出时继续更新时间，不重复刷屏。

### 20.2 Dispatch Plan Card

每次正式派工前展示计划卡：

```text
Dispatch Plan · 等待批准

目标：实现 Runtime Ledger + Local SSE

任务：
1. Claude Planner · 方案细化 · 只读 docs/crates
2. Codex Impl · 代码落地 · 写 crates/conductor-core/src/runtime*
3. Test Agent · 回归测试 · 只运行 cargo test
4. Review Agent · 审查 · 只读 diff/test output

风险：
- 需要新增 SQLite migration
- 可能影响 Tauri 启动流程

[批准执行] [修改计划] [取消]
```

用户需要看到：

- 派给谁。
- 做什么。
- 能改哪里。
- 会用哪些高风险能力。
- 为什么需要批准。

用户默认不需要看到：

- 完整系统 prompt。
- 每个模型的 API 参数。
- 内部事件 payload。
- AgentTeam 的全部状态字段。

### 20.3 Agent Lane

派工后展示 Agent lane：

```text
Codex Impl
  Backend: Codex CLI · Model: managed by Codex
  Task: 实现 runtime_events migration
  Status: Working 11:22
  Scope: crates/conductor-core/src/db.rs
  Last: Ran cargo check

Claude Planner
  Backend: Claude CLI · Model: managed by Claude Code
  Task: 拆解 G1 API
  Status: Completed
  Last: Produced dispatch notes
```

模型信息默认弱展示，放在 Backend 下；只有用户打开高级详情时才显示 provider/model/base_url。

### 20.4 “为什么派给它”

每个派工结果都应有可解释理由：

```text
为什么派给 Codex Impl？
因为该任务需要多轮文件编辑、命令执行和测试反馈；Codex CLI 当前可用，并且具备 code_edit/test_run 能力。
```

这条解释来自 `RouteDecision`，不是临时文案。

---

## 21. 多 LLM 聚合配置与智能路由

### 21.1 当前实现判断

当前项目还没有多模型路由的一等抽象：

| 现状 | 含义 |
|---|---|
| `CoreConfig.llm` 只有单一 `LlmConfig` | 项目内聊天/总结默认只能使用一个 provider/model |
| `CodexConfig` 只有 `api_endpoint/workspace_root/codex_binary` | Codex 的真实模型通常由 Codex CLI 自己管理 |
| `AgentRun` 有 `agent_id/role/command_json/metadata_json` | 可以记录一次运行，但不能表达可调度能力和模型能力 |
| `AgentTeamMember` 有 `role/subscriptions/metadata_json` | 能表达成员身份，但没有统一 backend/profile |
| Skill/Policy 有 `allowed_tools` | 能限制工具，不负责模型选择 |

所以不能让主 Agent 只靠 prompt 判断“Claude Code 里是 DeepSeek，Codex 里是 GPT”。必须新增：

```text
LLM Profile Registry
Agent Backend Registry
Routing Policy
Route Decision Ledger
```

### 21.2 核心概念

#### LlmProfile

表示一个可直接调用的模型配置。

```yaml
LlmProfile:
  id: string
  display_name: string
  provider: openai_compatible | anthropic_compatible | local | custom
  model: string
  base_url: string
  api_key_ref: string?
  enabled: boolean
  capabilities:
    tool_calling: boolean
    streaming: boolean
    reasoning: none | basic | strong
    vision: boolean
    long_context: boolean
    code_strength: low | medium | high
    chinese_strength: low | medium | high
  limits:
    max_context_tokens: number?
    max_output_tokens: number?
    timeout_seconds: number
    max_parallel: number
  cost:
    tier: cheap | standard | expensive
    input_per_million: number?
    output_per_million: number?
  data_policy:
    allow_sensitive: boolean
    allow_source_code: boolean
    allow_external_network: boolean
```

#### AgentBackend

表示一个可被调度的执行体，不一定等于一个 LLM。

```yaml
AgentBackend:
  id: string
  display_name: string
  kind:
    llm_chat
    claude_cli
    codex_cli
    agent_team
    review_agent
    mcp_agent
    human
  status: enabled | disabled | degraded
  model_mode:
    direct_profile       # 使用 LlmProfile
    cli_managed          # 模型由 Claude/Codex CLI 自己管理
    team_composed        # AgentTeam 内部再分配
  default_llm_profile_id: string?
  effective_model_label: string?
  command_template: string?
  capabilities:
    - planning
    - code_edit
    - test_run
    - review
    - document_write
    - long_running
    - interactive
    - external_tools
  tool_policy_id: string?
  max_parallel_runs: number
  health:
    last_check_at: datetime?
    last_error: string?
```

关键点：

- Claude Code 配了 DeepSeek，不代表项目能直接知道；它应被建模为 `claude_cli + cli_managed + effective_model_label=DeepSeek(用户标注或探测)`。
- Codex 配了 GPT，同理是 `codex_cli + cli_managed + effective_model_label=GPT(用户标注或探测)`。
- 对 CLI-managed backend，系统优先按能力调度，而不是按模型名调度。
- 只有 direct_profile 才由项目直接控制 base_url/model/api_key。

#### RouteDecision

每次选择 Agent/模型都要落账。

```yaml
RouteDecision:
  id: string
  task_id: string
  goal_id: string?
  selected_backend_id: string
  selected_llm_profile_id: string?
  candidate_backend_ids: string[]
  task_classification:
    task_kind: planning | coding | review | testing | document | external_action
    complexity: low | medium | high
    interaction: single_turn | multi_turn | long_running
    risk: low | medium | high
    needs_tools: string[]
    needs_write: boolean
    confidentiality: public | project | sensitive
  reason: string
  fallback_backend_ids: string[]
  created_at: datetime
```

这既服务 UI 解释，也服务审计和后续调优。

### 21.3 智能路由策略

路由输入：

- 任务类型：方案、代码、测试、审查、文档、外部系统操作。
- 复杂度：低/中/高。
- 是否需要交互式多轮。
- 是否需要工具、文件写入、shell、外部副作用。
- 写范围和权限风险。
- 上下文长度。
- 中文/英文能力要求。
- 隐私和代码外发策略。
- 当前 backend 健康状态。
- 并行额度和成本预算。
- 用户偏好，例如“代码优先 Codex，方案优先 Claude”。

默认规则：

| 任务 | 首选 | 备选 |
|---|---|---|
| 简单聊天/解释 | cheap direct LLM profile | default LLM |
| 方案拆解 | Claude CLI 或 strong reasoning profile | AgentTeam planner |
| 代码落地 | Codex CLI | Claude CLI / AgentTeam |
| 测试运行 | Codex CLI 或 command runner | Test Agent |
| Review | Review Agent + strong reasoning profile | Claude CLI |
| 文档整理 | direct LLM profile | Claude CLI |
| 外部系统动作 | Connector backend | human approval |
| 高敏感代码 | local profile 或用户指定 backend | blocked |

路由输出必须包含：

- 选了哪个 backend。
- 是否选了具体 LLM profile。
- 为什么这样选。
- fallback 是什么。
- 需要哪些权限。

### 21.4 多 LLM 聚合配置是否值得做

结论：值得做，但不要和 Goal MVP 绑死。

推荐分层：

```text
P0: 保持现有单 LLM 配置，新增 AgentBackend Registry，先能描述 Claude/Codex/AgentTeam
P1: 新增多个 LlmProfile，项目内聊天/总结/review 可按 profile 调用
P2: 新增 RoutingPolicy，让 Goal/AgentTask 自动选 backend/profile
P3: 增加质量/成本/延迟指标，做自适应路由
```

理由：

- 用户真正要调度的是“能力实体”，不是裸模型。
- Claude Code/Codex 这种外部 CLI 的模型配置通常不归本项目直接控制。
- 多 LLM profile 对项目内 LLM 调用很有价值，但无法完全替代 CLI backend。
- 智能路由应该先可解释、可审计，再逐步智能化。

### 21.5 配置 UI

新增三个配置页：

1. `LLM Profiles`
   - OpenAI compatible / Anthropic compatible / Local。
   - 测试连接。
   - 标注能力和数据策略。

2. `Agent Backends`
   - Codex CLI、Claude CLI、AgentTeam、Review Agent。
   - 显示健康状态。
   - 显示 effective model label。
   - 设置 capabilities 和并发上限。

3. `Routing Rules`
   - 代码任务优先 Codex。
   - 方案任务优先 Claude。
   - 高敏感任务只用 local。
   - Review 必须独立 backend。
   - 失败 fallback 链。

### 21.6 安全边界

模型路由不能绕过权限系统。

规则：

- `RouteDecision` 只决定谁来做，不决定能做什么。
- `AgentBackend.capabilities` 是能力声明，不是授权。
- 真实工具权限仍由 `Permission Broker + WorkLease + ToolPolicy` 决定。
- 外部模型的数据策略必须参与路由；敏感上下文不能发给不允许的 profile/backend。
- CLI-managed backend 的真实模型如果无法验证，UI 必须标注 `managed by CLI`，不能伪装成已知模型。

---

## 22. 增量数据模型与派工项

### 22.1 AgentTask 增量字段

在第 5.5 节的 `AgentTask` 上补充：

```yaml
AgentTask:
  requested_capabilities: string[]
  preferred_backend_ids: string[]
  selected_backend_id: string?
  selected_llm_profile_id: string?
  route_decision_id: string?
  confidentiality: public | project | sensitive
  routing_reason: string?
```

### 22.2 新增表

```text
llm_profiles
agent_backends
agent_backend_health
routing_policies
route_decisions
```

### 22.3 新增派工批次

```text
Batch 6: User Perception
  U1-01 Goal 顶部控制条
  U1-02 左侧会话列表 Working/Blocked/Reviewing 状态
  U1-03 Dispatch Plan Card
  U1-04 Agent Lanes
  U1-05 Codex-style Agent Transcript 聚合
  U1-06 RouteDecision “为什么派给它” 展示

Batch 7: Backend Registry
  R1-01 设计 llm_profiles migration
  R1-02 设计 agent_backends migration
  R1-03 配置 UI：LLM Profiles
  R1-04 配置 UI：Agent Backends
  R1-05 backend health check
  R1-06 CLI-managed effective_model_label 手工标注

Batch 8: Intelligent Routing
  R2-01 TaskClassifier：规则优先，LLM 辅助
  R2-02 RoutingPolicy：任务类型到 backend/profile
  R2-03 RouteDecision 持久化
  R2-04 Goal Orchestrator 接入 router
  R2-05 fallback backend 链路
  R2-06 路由审计与指标
```

### 22.4 最小闭环

多模型/智能路由的 MVP 不要求自动识别所有模型，只要求：

1. 用户能配置多个 LLM Profile。
2. 用户能配置 Claude CLI、Codex CLI、AgentTeam 等 Agent Backend。
3. CLI backend 可以标注 `effective_model_label`，例如 `DeepSeek via Claude Code`、`GPT via Codex`。
4. 创建 AgentTask 时系统能按规则选择 backend。
5. 每次选择写入 RouteDecision。
6. UI 能解释“为什么派给它”。
7. 权限系统不因换模型或换 backend 被绕过。
