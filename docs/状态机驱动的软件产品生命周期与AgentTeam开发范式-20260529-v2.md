# 状态机驱动的软件产品生命周期与 AgentTeam 开发范式 v2

> 日期：2026-05-29  
> 定位：一套可复用的软件产品、需求、开发、测试、发布与 Agent 治理范式，可进一步抽成 LLM / Agent Skill。  
> v2 修订目标：从“方法论长文”升级为“可触发、可裁剪、可验证、可审计”的运行契约。  
> 适用场景：高频需求变化、多 Agent 混合开发、vibe coding、人工测试回归、快速产品迭代、项目内 Agent 权限治理。

---

## 0. Skill Runtime Contract

本范式首先是一个运行时契约，而不是一篇固定流程说明。每次触发时，LLM 不应默认生成全量 PRD，而应先判断当前输入属于哪种模式，再输出最小充分产物。

### 0.1 何时触发

| 输入类型 | 触发理由 |
|---|---|
| 原始产品想法 | 需要从模糊意图拆出用户、对象、状态和边界 |
| 模糊功能需求 | 需要防止直接拆成前端、后端、数据库任务 |
| Bug / 回归问题 | 需要定位被破坏的迁移、不变量或 read model |
| 体验优化 | 需要定位用户卡在哪个状态，而不是泛化为“优化 UI” |
| 重构 | 需要证明不改变业务状态语义，或明确要修复的状态源问题 |
| 测试补充 | 需要把测试绑定到合法迁移、非法迁移、guard、副作用和恢复 |
| 发布检查 | 需要判断 P0 状态闭环、测试证据、回滚和观测是否齐全 |
| Agent 派工 | 需要按 OODA-R 分配角色、写范围、验收和 review |
| 项目内 Agent 设计 | 需要定义 Agent 生命周期、权限、工具、记忆和审计边界 |
| 灵感 / 临时想法 | 需要进入 Inbox / Spike，而不是直接污染主流程 |

### 0.2 何时不要强行状态机化

| 场景 | 正确处理 |
|---|---|
| 纯文案、格式、注释、排版 | 走 `no_state_impact`，用输入输出不变量验收 |
| 局部内部实现替换 | 走 `no_state_impact`，声明“不改变外部状态语义” |
| 探索性技术 spike | 走 `spike`，不进入 L0，不生成正式开发任务 |
| 信息不足但风险低 | 输出假设和可回退方案，不阻塞轻量探索 |
| 信息不足且风险高 | 输出 `blocked`，尤其是权限、删除、支付、生产写入、外部调用 |

### 0.3 模式选择

| Mode | 适用输入 | 必要输出 |
|---|---|---|
| `triage` | 灵感、反馈、模糊输入 | Demand Card、状态影响判断、状态码 |
| `decompose` | 需求拆解 | 原子需求、状态迁移、开放问题 |
| `prd` | 新能力或较大需求 | State Model Draft、MVP Slice、Acceptance |
| `mvp-slice` | 控制范围 | 闭合子图、非目标、边界外旁支 |
| `architecture` | 方案设计 | Architecture Mapping、owner、状态源 |
| `agent-dispatch` | 多 Agent 开发 | OODA-R 派工单、写范围、Review Gate |
| `test-matrix` | 测试设计 | 合法/非法迁移测试、guard、side effect、恢复 |
| `regression` | Bug 回归 | Broken Transition、Broken Invariant、回归单 |
| `review` | 代码或方案 review | 状态漂移、边界扩张、测试缺口 |
| `release-gate` | 发布前检查 | Traceability、P0 证据、回滚、观测 |
| `agent-boundary` | 项目内 Agent | Agent 状态机、权限矩阵、工具边界、审计 |
| `minimal` | 小任务 | 五元组、guard、验收、非目标、OODA-R 最小单 |
| `self-iterate` | 迭代本范式 | 范式状态机、评审证据、vNext 变更单 |

### 0.4 输入契约

```yaml
raw_input: 用户原话或任务描述
mode: triage | decompose | prd | mvp-slice | architecture | agent-dispatch | test-matrix | regression | review | release-gate | agent-boundary | minimal | self-iterate
context:
  lifecycle_state: LC-xx 或 unknown
  current_artifacts: L0/L1/L2/L3 文档、代码范围、测试范围
  constraints: 时间、成本、MVP、技术栈、风险偏好
  existing_state_model: 已有状态账本或 none
  agent_team: 可用角色、并行限制、写范围限制
  permission_policy: 工具、环境、数据、审批约束
```

### 0.5 输出状态码

| Status | 含义 | 下一步 |
|---|---|---|
| `ready` | 可进入下一生命周期状态 | 生成对应产物或派工 |
| `needs_clarification` | 缺少关键建模信息 | 最多问 3 个会影响状态机的问题 |
| `inbox` | 有价值但未 triage | 进入 L3，不进入开发 |
| `spike` | 需要探索验证 | 限定时间和产出，不写入 L0 |
| `no_state_impact` | 不改变业务状态 | 用输入输出不变量和回归测试验收 |
| `blocked` | 高风险条件不清或缺权限 | 等待用户、审批或外部状态变化 |
| `release_gate_failed` | 发布证据不足 | 回到测试、开发或 L0 修订 |

### 0.6 澄清策略

只在阻塞状态机建模时提问。每次最多 3 个问题，优先问会改变状态、权限、数据或验收的问题。

必须提问的情况：

- 不知道 actor、object、source state、target state 或 trigger。
- 成功标准无法测试。
- 权限、删除、支付、生产写入、外部调用等高风险 guard 不明确。
- 用户输入包含多个互斥目标。
- 当前需求与 L0 稳定事实冲突。

不必提问的情况：

- 函数命名、文件命名、轻微 UI 文案。
- 可由已有架构惯例确定的实现细节。
- 可以通过 `no_state_impact` 验收的小改动。

---

## 1. 核心原则

### 1.1 状态先于功能

不要先问要做哪个页面、按钮、接口。先问：

```text
用户是谁？
核心对象是什么？
对象现在是什么状态？
什么动作或事件触发变化？
目标状态是什么？
哪些 guard 必须满足？
哪些状态禁止进入？
迁移会产生哪些副作用？
哪些不变量不能破坏？
用户如何知道状态变化已经发生？
失败后如何恢复、补偿或退出？
```

功能是状态迁移的触发器。接口是状态迁移的承载。UI 是状态的可视化。测试是迁移正确性的证据。

### 1.2 LLM 可以灵活实现，不能灵活解释状态

允许 Agent 发挥的部分：

- 选择局部代码组织方式。
- 补充局部测试。
- 优化实现细节。
- 发现遗漏后提出 L0 / L1 / L2 变更建议。

不允许 Agent 自行决定的部分：

- 新增业务状态。
- 改变已有状态语义。
- 把非 MVP 状态接入主流程。
- 把临时 Spike 合并成正式实现。
- 用推断替换状态账本。
- 把工具输出或中间推理写成长期事实。

### 1.3 MVP 是闭合子图

MVP 不是功能少，而是状态图小。一个合格 MVP 至少包含：

- 一个入口状态。
- 一个成功出口。
- 一个失败出口。
- 一个退出路径。
- 一条最短可用路径。
- 一组明确不做的旁支。
- 一组最小可验证验收标准。

### 1.4 所有结论必须带证据等级

LLM 输出状态判断时，必须标注来源类型：

| Evidence | 含义 | 可进入 L0 |
|---|---|---|
| `observed` | 来自用户明确陈述、已有 L0、代码、测试、日志等可验证事实 | 可以 |
| `inferred` | 基于事实推导，但用户未明确确认 | 不直接进入 L0 |
| `assumed` | 为推进任务临时假设 | 不进入 L0，需显式标注 |
| `unknown` | 尚不明确 | 触发澄清或 Spike |

证据优先级：

```text
L0 状态账本 > 用户当前明确指令 > 可观察代码/测试/日志 > L1/L2 文档 > Agent 推断 > Agent 假设
```

---

## 2. 状态机元模型

### 2.1 禁止裸状态名

`pending`、`running`、`done` 这类裸状态名会导致语义漂移。必须带命名空间：

| 命名空间 | 含义 | 示例 |
|---|---|---|
| `BusinessState` | 业务对象生命周期 | `Proposal.pending_review` |
| `LifecycleState` | 产品开发生命周期 | `LC.RequirementSpecified` |
| `AgentRunState` | Agent 执行实例状态 | `AgentRun.awaiting_approval` |
| `ToolCallState` | 工具调用状态 | `ToolCall.executing` |
| `PermissionState` | 权限授权状态 | `PermissionGrant.approved_once` |
| `MemoryState` | 记忆条目状态 | `MemoryEntry.classified` |
| `ReadModelState` | 派生展示状态 | `DashboardItem.action_required` |
| `UIState` | 纯 UI 展示态 | `Button.disabled` |

规则：

```text
BusinessState 是事实源。
ReadModelState 是派生视图。
UIState 只能展示或触发，不应成为业务事实源。
AgentRunState、ToolCallState、PermissionState 不能与业务状态混用。
```

### 2.2 State Machine Meta Model

```yaml
machine_id: 唯一 ID
machine_type: business | lifecycle | dev_agent | runtime_agent | permission | memory | tool
version: semver
owner: human 或 team
object_type: 被建模对象
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

### 2.3 跨对象建模

真实产品常有跨对象流程，不能强行压成“一个 object 一个 transition”。跨对象关系分三类：

| 关系 | 定义 | 建模方式 |
|---|---|---|
| `causal_link` | A 的迁移触发 B 的迁移 | 记录事件、失败策略、重试策略 |
| `saga` | 多对象分步完成，失败需补偿 | 每步有 transition 和 compensation |
| `read_model` | 多对象状态派生展示 | 声明事实源、派生规则、过期策略 |

跨对象流程必须声明一致性边界：

```text
哪些状态必须强一致？
哪些状态可以最终一致？
哪些副作用可以重试？
哪些副作用必须补偿？
用户看到的是事实源还是派生视图？
```

---

## 3. 文档与记忆分层

### 3.1 L0：状态账本

建议文件：`STATE_MODEL.md`。

L0 只记录稳定事实：

| 字段 | 内容 |
|---|---|
| Machine | 状态机 ID、类型、版本、owner |
| Object | 被建模对象 |
| State | 合法状态、终态、超时规则 |
| Command / Event | 触发来源 |
| Transition | from、to、trigger、guard、side effect、invariant |
| Illegal Transition | 明确禁止的跳转 |
| MVP Slice | 当前版本覆盖的闭合子图 |
| Test IDs | 覆盖每条 P0 迁移的测试 |
| Changelog | 修改原因、日期、影响范围 |

L0 不记录讨论过程，不记录未验证想法，不记录 Agent 中间推理。

### 3.2 L1：PRD / 架构说明

L1 解释为什么：

- 产品目标。
- 用户场景。
- 业务边界。
- 架构模块。
- 数据流。
- 状态机设计取舍。

L1 与 L0 冲突时，以 L0 为准；如果 L1 更新意味着状态语义变化，必须提出 L0 变更请求。

### 3.3 L2：派工单 / Bug 单 / 回归单

L2 承载高频变化：

- 本轮目标。
- 涉及状态和迁移。
- 允许改什么。
- 禁止改什么。
- 验收方式。
- 自动测试和人工测试结果。
- 回归结论。

每个 L2 条目必须引用 `machine_id`、`transition_id` 或声明 `no_state_impact`。

### 3.4 L3：灵感 / 临时记忆 / Spike

L3 可以混乱，但不能直接驱动正式开发。

适合放：

- 灵光一现。
- UI 想法。
- 技术尝试。
- 暂未验证假设。
- Agent 中间推理。
- 人工测试观察。

进入开发前，L3 必须被 triage 成 L0、L1、L2 或 Rejected。

---

## 4. LLM 如何拆解人的需求

### 4.1 正确顺序

```text
人的原始表达
-> 归一化：problem / desired outcome / solution hint
-> 需求类型
-> Actor / Object / Current State / Trigger / Target State
-> Guard / Side Effect / Invariant / Compensation
-> 合法迁移 / 非法迁移
-> 原子需求
-> 用户流
-> MVP 子图
-> 架构映射
-> 测试矩阵
-> Agent 派工
```

关键点：用户给出的方案只是 `solution hint`，不能直接当成需求。LLM 要先还原问题和期望结果。

### 4.2 Demand Card

```yaml
demand_id:
raw_input:
normalized:
  problem:
  desired_outcome:
  solution_hint:
demand_type: feature | bug | improvement | refactor | test | docs | release | spike | agent-boundary
status: ready | needs_clarification | inbox | spike | no_state_impact | blocked
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

### 4.3 对象与状态维度

LLM 至少区分六类对象：

| 对象类型 | 示例 | 典型状态 |
|---|---|---|
| 业务对象 | 订单、任务、审批、文档 | draft、pending、approved、failed |
| 执行对象 | Job、Run、Workflow | queued、running、succeeded、failed |
| 权限对象 | Grant、Policy、Role | requested、approved、denied、revoked |
| 事件对象 | Event、Message、Callback | emitted、handled、dead_lettered |
| Read Model | DashboardItem、ListRow | stale、fresh、action_required |
| UI 对象 | Button、Panel、Modal | visible、disabled、loading |

LLM 至少区分六类状态维度：

| 状态维度 | 解决的问题 |
|---|---|
| 业务生命周期状态 | 对象事实是什么 |
| 执行状态 | 操作是否正在跑、成功或失败 |
| 权限状态 | 谁能做什么 |
| 展示状态 | 用户看到什么 |
| 同步状态 | read model 是否过期 |
| 错误/恢复状态 | 失败后如何退出 |

### 4.4 原子需求粒度

原子需求不是“一个前端任务”或“一个后端任务”，而是一个可验证的状态影响单元。

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
```

如果一个需求包含多个 actor、多个对象、多个 source state 或多个 target state，应拆成多个原子需求，或显式建模为 `saga`。

### 4.5 No State Impact 路径

不是所有改动都需要进入状态机。无状态影响任务必须明确声明：

```yaml
status: no_state_impact
reason:
external_behavior_changed: false
business_state_changed: false
permission_changed: false
read_model_changed: false
invariants:
  - 现有状态迁移不变
  - 现有 API contract 不变
  - 现有测试应继续通过
verification:
  - 相关单测
  - 快照或视觉检查
  - 手工 smoke test
```

这条路径用于保持敏捷，不把小任务拖入重流程。

### 4.6 需求质量闸门

进入开发前，P0 需求必须通过：

| 闸门 | 通过条件 |
|---|---|
| Actor 明确 | 知道是谁触发 |
| Object 明确 | 知道哪个对象变化 |
| Source State 明确 | 知道从哪里来 |
| Target State 明确 | 知道到哪里去 |
| Trigger 明确 | 知道什么导致变化 |
| Guard 明确 | 知道何时允许 |
| Side Effect 明确 | 知道会写什么、通知什么、调用什么 |
| Invariant 明确 | 知道什么不能被破坏 |
| Illegal Transition 明确 | 知道哪些不允许 |
| Recovery 明确 | 知道失败后如何恢复、补偿或退出 |
| Acceptance 明确 | 知道如何验收 |
| Non-goal 明确 | 知道本轮不做什么 |
| Evidence 标注 | 知道哪些是事实、推断、假设、未知 |

---

## 5. MVP Slice

### 5.1 MVP 判定

MVP 是闭合状态子图。判定规则：

| 要素 | 要求 |
|---|---|
| 入口 | 用户或系统能进入起点状态 |
| 成功出口 | 至少一条完整成功路径 |
| 失败出口 | 至少一条明确失败路径 |
| 退出路径 | 用户或系统能取消、停止、回退或归档 |
| 非目标 | 明确列出不做的旁支 |
| 可见性 | 用户能理解当前状态和下一步 |
| 可测试 | P0 迁移有自动或人工证据 |

### 5.2 MVP 外能力

MVP 外能力不能暴露主入口。常见处理：

| 能力 | 处理 |
|---|---|
| 批量操作 | 放到非目标 |
| 多人审批 | 放到非目标 |
| 自动重试 | 可放 P1，P0 先提供人工重试或失败出口 |
| 高级筛选 | 不影响核心状态闭环时延后 |
| 智能推荐 | 先进入 Spike 或 read-only 建议，不自动迁移业务状态 |

### 5.3 P0 / P1 / P2

| Priority | 判定标准 |
|---|---|
| P0 | 没有它，MVP 闭环不成立，或非法迁移会造成严重风险 |
| P1 | 提升体验、效率或恢复能力，但不阻断主闭环 |
| P2 | 辅助能力、优化项、可延后旁支 |

---

## 6. 生命周期状态机

软件产品生命周期本身也按状态机管理。

| ID | 状态 | 核心产物 | 出口证据 |
|---|---|---|---|
| `LC-00` | Idea Captured | 原始想法 | 来源、问题、预期收益 |
| `LC-01` | Problem Framed | 问题定义 | 用户、场景、痛点、成功标准、非目标 |
| `LC-02` | Actor & Object Mapped | Actor/Object 清单 | 对象关系和状态维度 |
| `LC-03` | Core State Modeled | L0 草案 | 状态、迁移、guard、不变量、非法迁移 |
| `LC-04` | MVP Slice Selected | MVP 子图 | 闭合路径、失败路径、边界外旁支 |
| `LC-05` | Requirement Specified | PRD / 验收 | 每条需求绑定状态或 `no_state_impact` |
| `LC-06` | Architecture Mapped | 架构映射 | 无 ownerless transition |
| `LC-07` | Ready For Dev | 派工单 | OODA-R、写范围、验收、回归点明确 |
| `LC-08` | In Development | 代码和测试 | 实现声明状态影响 |
| `LC-09` | Integrated | 集成版本 | 无状态源冲突、无写范围冲突 |
| `LC-10` | State-Tested | 测试报告 | 合法、非法、异常、恢复路径通过 |
| `LC-11` | Release Gated | 发布候选 | 回滚、观测、风险清单完成 |
| `LC-12` | Released & Observed | 已发布版本 | 线上反馈映射回状态机 |
| `LC-13` | Retrospected | 复盘结论 | 更新 L0/L1/L2/L3 或生成下一轮输入 |

### 6.1 返工路径

| Trigger | From | To |
|---|---|---|
| 问题被证伪 | `LC-01` | `LC-00` |
| 对象识别错误 | `LC-03` | `LC-02` |
| MVP 子图不闭合 | `LC-04` | `LC-03` |
| PRD 出现游离需求 | `LC-05` | `LC-04` |
| 架构无法承载迁移 | `LC-06` | `LC-05` |
| Agent 越界或状态漂移 | `LC-08` | `LC-07` |
| 集成发现状态源冲突 | `LC-09` | `LC-06` |
| 测试缺 P0 证据 | `LC-10` | `LC-08` |
| 发布门禁失败 | `LC-11` | `LC-10` |
| 线上 P0 故障 | `LC-12` | `LC-08` |

### 6.2 非法生命周期迁移

| From | 非法 To | 原因 |
|---|---|---|
| `LC-00` | `LC-08` | 跳过问题定义、状态建模和 PRD |
| `LC-03` | `LC-08` | 跳过 MVP 和架构映射 |
| `LC-07` | `LC-10` | 跳过开发产物 |
| `LC-10` | `LC-12` | 跳过发布门禁 |
| `LC-13` | `LC-08` | 复盘结论不能绕过新一轮 triage |

---

## 7. 架构映射

架构设计的目标不是先分层，而是保证每条状态迁移有唯一 owner、事实源、读模型和测试证据。

### 7.1 Architecture Mapping

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

### 7.2 Ownerless Transition

出现以下任一情况，说明架构不可进入开发：

- 有迁移但没有模块 owner。
- 有状态但不知道事实源在哪。
- 有 side effect 但没有 owner。
- 有 UI 展示但不知道来自哪个 canonical read model。
- 有 illegal transition 但没有拒绝位置。
- 有 P0 迁移但没有测试 owner。

### 7.3 状态源分裂检测

常见状态源分裂：

| 表现 | 风险 |
|---|---|
| 前端用多个字段拼 `status` | UI 和业务事实不一致 |
| 后端 DTO 和数据库状态语义不同 | API 消费方理解漂移 |
| 多个模块各自定义 enum | 新增状态时漏改 |
| read model 不声明过期策略 | 用户看到旧状态 |
| 测试绕过 canonical state | 测试证明不了真实流程 |

修复原则：

```text
一个业务对象只有一个 canonical state source。
其他展示状态必须是 read model 或 UIState。
所有 read model 必须声明派生规则和 stale policy。
```

---

## 8. 开发 AgentTeam 与 OODA-R

开发 AgentTeam 不是“多找几个 Agent 写代码”，而是把开发过程本身状态机化。

### 8.1 OODA-R

```text
Observe -> Orient -> Decide -> Act -> Review
```

| 阶段 | 输出 | 禁止 |
|---|---|---|
| Observe | 事实清单、已读上下文、未知项 | 直接写代码 |
| Orient | 状态影响、风险、边界、设计选项 | 只按文件名猜测 |
| Decide | 方案、文件范围、测试计划、回滚点 | 无计划开工 |
| Act | 代码、测试、文档增量 | 越界重构、静默改状态语义 |
| Review | diff 自查、测试结果、状态影响、返工项 | 只报“已完成” |

硬规则：

```text
未完成 Observe / Orient，不进入 Act。
未完成 Decide，不改代码。
Review 发现状态语义变化，必须回到 Decide 或提出 L0 变更。
```

### 8.2 开发 Agent 生命周期

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

### 8.3 角色边界

| Role | 负责 | 禁止 |
|---|---|---|
| Product Agent | 用户目标、PRD、状态机变更建议 | 直接改代码 |
| Architecture Agent | 状态到模块、接口、存储、事件的映射 | 绕过 L0 做实现 |
| Implementation Agent | 按派工单实现 | 静默新增状态、扩大 MVP |
| Test Agent | 合法/非法迁移测试、回归路径 | 只测 happy path |
| Review Agent | 状态漂移、边界扩张、重复状态源 | 做无边界重构 |
| Docs Agent | 同步确认事实到 L0/L1/L2 | 记录未验证推理为事实 |

### 8.4 并行规则

- Product 和 Architecture 可以串行，不能跳过。
- 多个 Implementation Agent 可以并行，但写范围不能重叠。
- Test Agent 可以在实现前先写测试矩阵。
- Review Agent 不参与实现，保持独立判断。
- Docs Agent 只在 review 后同步确认事实。
- 如果两个 Agent 必须改同一状态源，应串行执行，后者先 Observe 前者 diff。

### 8.5 Agent Dispatch Pack

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

---

## 9. 项目内 Agent 状态机与权限边界

开发 Agent 是“帮你开发系统的 Agent”。项目内 Agent 是“产品运行时的一部分”。后者必须像业务对象一样建模。

### 9.1 核心对象

| 对象 | 含义 |
|---|---|
| `Agent` | 具备角色、能力和约束的执行体 |
| `AgentTask` | Agent 被要求完成的工作项 |
| `AgentRun` | 一次具体执行实例 |
| `ToolCall` | 一次工具调用 |
| `PermissionGrant` | 用户或策略授予的权限 |
| `WorkspaceScope` | 可读写边界 |
| `MemoryEntry` | 可读写记忆 |
| `AuditEvent` | 可追踪行为记录 |

### 9.2 AgentRun 状态机

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

非法迁移：

| From | 非法 To | 原因 |
|---|---|---|
| `Created` | `Running` | 未配置角色和边界 |
| `Planning` | `ToolCalling` 高风险工具 | 未完成风险评估和审批 |
| `AwaitingApproval` | `Running` | 绕过授权 |
| `Failed` | `Succeeded` | 失败不能伪装成功 |
| `Archived` | `ToolCalling` | 归档态只读 |

### 9.3 ToolCall 状态机

```text
Proposed
-> RiskClassified
-> Approved / Rejected
-> Executing
-> Succeeded / Failed / TimedOut
-> Recorded
```

ToolCall 必须记录：

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

### 9.4 PermissionGrant 状态机

```text
Unrequested
-> Requested
-> ApprovedOnce / ApprovedSession / Denied
-> Expired / Revoked
```

权限规则：

- 默认最小权限。
- 写操作、命令执行、网络访问、外部系统调用必须可审计。
- 权限不能跨 workspace 静默复用。
- 低风险授权不能推导为高风险授权。
- 用户撤销后，后续动作必须重新请求。
- 子 Agent 权限是父任务范围与显式授权的交集，不能自动继承 session 权限。

### 9.5 身份、授权、数据、环境

| 维度 | 必须建模 |
|---|---|
| 身份 | human owner、agent identity、service account、delegation chain |
| 授权 | subject、resource、action、effect、reason code |
| 环境 | local、sandbox、dev、staging、prod |
| 数据 | secret、PII、业务敏感、日志、工具输出 |
| 工具 | allowlist、risk level、timeout、audit |
| 记忆 | 临时、观察、决策、稳定事实、删除路径 |

### 9.6 权限矩阵

| Subject | Resource | Action | Environment | Effect | Approval | Audit |
|---|---|---|---|---|---|---|
| Agent | Workspace file | read | local/dev | allow | none | optional |
| Agent | Workspace file | write | local/dev | allow | task scope | required |
| Agent | Command | execute | local/dev | conditional | risk-based | required |
| Agent | Production data | write | prod | deny by default | human explicit | required |
| Agent | Secret | read | any | deny by default | explicit need | required |
| Child Agent | Parent scope | delegate | any | attenuated allow | parent task scope | required |

### 9.7 事故处理

项目内 Agent 必须有事故状态和恢复动作：

| 事故 | 动作 |
|---|---|
| 越权工具调用 | kill switch、撤销授权、写审计、通知 human owner |
| 记忆污染 | 隔离 MemoryEntry、回滚或删除、标记不可信 |
| 状态误迁移 | 暂停 Agent、执行补偿、创建回归单 |
| 子 Agent 越界 | 终止委托链、收回权限、审计父 Agent |
| 外部工具异常 | 暂停重试、进入 Failed 或 AwaitingHuman |

---

## 10. 测试、回归与发布门禁

### 10.1 Traceability Matrix

每个 P0 迁移都应可追踪：

```text
raw_input
-> demand_id
-> transition_id
-> req_id
-> architecture_owner
-> task_id
-> test_id
-> release_evidence
```

缺任一环节，发布门禁应失败或降级。

### 10.2 Test Matrix

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

### 10.3 P0 迁移最低测试集

每个 P0 transition 至少需要：

- 合法迁移测试。
- 非法迁移拒绝测试。
- Guard 测试。
- Side effect 幂等测试。
- 失败恢复、补偿或退出测试。
- 刷新 / 重启后的 canonical state 测试。
- Read model 与事实源一致性测试。

### 10.4 Bug 回归单

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

### 10.5 Release Gate

| 检查项 | 发布要求 |
|---|---|
| 新增/修改状态已登记 | 必须 |
| 新增/修改迁移已登记 | 必须 |
| P0 合法迁移有测试证据 | 必须 |
| P0 非法迁移有拒绝测试 | 必须 |
| P0 guard 有测试证据 | 必须 |
| 失败态可见、可解释、可恢复或可退出 | 必须 |
| MVP 边界外能力未暴露主入口 | 必须 |
| 前端展示使用统一 read model | 必须 |
| Bug 回归单绑定状态和迁移 | 必须 |
| 回滚、观测、风险清单完成 | 必须 |

发布判定：

```text
P0 状态闭环不完整，不发布。
P0 非法迁移不能被阻止，不发布。
状态源分裂，只能灰度或内部发布。
失败态不可恢复，必须有降级方案。
MVP 边界被突破，必须回收入口或升级 PRD。
```

---

## 11. 常见输入范式

### 11.1 Feature

原话：

```text
加一个导出报告功能。
```

拆解：

```text
Object: Report
State Path:
Report.ready -> Report.export_requested -> Report.exporting -> Report.exported
Report.exporting -> Report.export_failed

Illegal:
Report.draft 不能导出
Report.exporting 不能重复导出同一请求

MVP:
ready 发起导出，成功生成文件，失败展示原因并可退出
```

### 11.2 Bug

原话：

```text
任务明明失败了，列表还显示完成。
```

拆解：

```text
Broken Transition: Task.running -> Task.failed
Broken Invariant: failed 不能展示为 completed
Possible Cause: read model 派生错误或前端使用非 canonical status
Regression: 后端 DTO、read model、前端刷新后状态一致
```

### 11.3 Refactor

原话：

```text
把任务模块重构一下。
```

先问：

```text
重构要解决哪个状态源问题？
是否消除重复 enum？
是否建立 canonical read model？
是否合并迁移入口？
是否保证外部状态语义不变？
```

如果答案都是“不改变状态语义”，走 `no_state_impact`。

### 11.4 灵感

原话：

```text
我突然想到可以智能推荐下一步。
```

处理：

```text
Status: inbox 或 spike
Potential Object: TaskQueue / UserFocus
Potential Read Model: NextActionSuggestion
Guard: 推荐不能自动迁移业务状态
Next: 时间盒探索，确认是否进入 L1/L2
```

### 11.5 项目内 Agent

原话：

```text
做一个能自动处理用户文件的 Agent。
```

先建模：

```text
AgentTask: captured -> classified -> planned -> assigned -> in_progress -> completed/failed
ToolCall: proposed -> risk_classified -> approved/rejected -> executing -> recorded
Permission: unrequested -> requested -> approved_once/denied
Boundary: 能读哪些文件，能写哪些文件，能否联网，是否能写长期记忆
Audit: 每次工具调用必须记录 input_summary 和 result_summary
```

---

## 12. 每轮同步仪式

每轮开发结束后，用 10-15 分钟做状态同步。

```md
状态机同步检查：

- 是否新增状态：是 / 否
- 是否新增迁移：是 / 否
- 是否修改状态语义：是 / 否
- 是否新增非法状态组合：是 / 否
- 是否存在 UI 展示状态和事实源不一致：是 / 否
- 是否存在多个模块各自拼接状态：是 / 否
- 是否有 P0 状态无法通过测试进入：是 / 否
- 是否有非终态无法退出：是 / 否
- 是否有新灵感进入 Inbox：是 / 否
- 是否有稳定事实需要进入 L0：是 / 否
- 是否有临时判断误写长期记忆：是 / 否
- 是否有 Agent 权限或工具边界变化：是 / 否
```

下一轮任务只能来自：

- 未完成的 L2 任务。
- 人工测试失败项。
- 已 Accepted 的 Inbox idea。
- L0 标记的状态机缺口。
- 发布后观察到的高频失败迁移。
- Review Agent 打回项。

---

## 13. 范式自我迭代状态机

本范式也应按状态机迭代，否则方法论本身也会发生设计意图漂移。

### 13.1 Methodology 状态机

```text
Draft
-> Reviewed
-> GapsIdentified
-> RevisionPlanned
-> Revised
-> Validated
-> Published
-> Retrospected
```

| 状态 | 产物 | 出口条件 |
|---|---|---|
| `Draft` | 初版方法论 | 有可评审文本 |
| `Reviewed` | AgentTeam / 人工评审 | 至少一个产品视角和一个 review 视角 |
| `GapsIdentified` | 缺口清单 | 缺口可映射到章节或输出契约 |
| `RevisionPlanned` | vNext 变更计划 | 明确保留、删除、增加什么 |
| `Revised` | 新版本文档 | 变更已落地 |
| `Validated` | 验收结果 | 能处理 feature、bug、no_state_impact、agent-boundary |
| `Published` | 可复用范式或 Skill | 有触发规则、输入输出、最小路径 |
| `Retrospected` | 下一轮输入 | 反馈进入 Inbox 或 L0 变更 |

### 13.2 本次 v2 迭代记录

| 评审发现 | v2 处理 |
|---|---|
| v1 更像长文方法论，不像 Skill 契约 | 前置 `Skill Runtime Contract`、模式选择、状态码 |
| 状态机元模型不够形式化 | 新增 `State Machine Meta Model` 和命名空间 |
| 生命周期偏线性 | 增加返工路径和非法生命周期迁移 |
| LLM 结论缺少证据约束 | 新增 `observed / inferred / assumed / unknown` |
| 测试闭环缺 traceability | 新增 `raw_input -> release_evidence` 矩阵 |
| 小任务容易被重流程拖慢 | 新增 `minimal` 和 `no_state_impact` 路径 |
| 跨对象流程难以表达 | 新增 `causal_link / saga / read_model` |
| 项目内 Agent 权限边界不足 | 增加身份、授权、环境、数据、委托链、事故处理 |

---

## 14. 最小执行版本

如果不想引入完整流程，只保留四件事：

1. 一个 `STATE_MODEL.md`。
2. 一个固定 OODA-R 派工单模板。
3. 一个项目内 Agent 权限边界清单。
4. 每轮结束后的状态机同步检查。

每次需求进入时，只问：

```text
Actor 是谁？
Object 是什么？
From State 是什么？
Trigger 是什么？
To State 是什么？
Guard 是什么？
验收怎么证明？
本轮不做什么？
是否 no_state_impact？
```

每次派发开发 Agent 前，只问：

```text
Observe 要读取什么？
Orient 要判断什么状态影响？
Decide 的方案和文件范围是什么？
Act 允许改哪里？
Review 如何证明没有状态漂移？
```

每次设计项目内 Agent 前，只问：

```text
Agent 能做什么？
不能做什么？
能读哪里？
能写哪里？
哪些工具需要审批？
哪些行为必须审计？
失败后如何停止或恢复？
子 Agent 权限如何衰减？
```

---

## 15. Skill Packaging Notes

如果真正落成 Skill，建议拆成：

| 文件 | 内容 |
|---|---|
| `SKILL.md` | 触发规则、模式选择、执行顺序、澄清策略、最小输出 |
| `references/meta-model.md` | 状态机元模型、命名空间、证据契约 |
| `references/output-schemas.md` | Demand Card、State Model、MVP Slice、Dispatch Pack、Test Matrix |
| `references/agent-team.md` | OODA-R、角色边界、派工单、Review Gate |
| `references/runtime-agent-security.md` | 项目内 Agent 权限、工具、记忆、审计 |
| `references/examples.md` | Feature、Bug、Refactor、No State Impact、Agent Boundary 示例 |

`SKILL.md` 不应承载整篇方法论。它只负责让 LLM 在当前任务中选择正确模式、调用对应模板，并输出可验证产物。长模板和示例放入 `references/`，避免每次触发都把上下文撑大。
