# 状态机驱动的软件产品生命周期与 AgentTeam 开发范式

> 日期：2026-05-29
> 定位：一套可复用的软件产品、需求、开发、测试生命周期范式，可进一步抽成 Codex / Agent Skill。
> 适用场景：高频需求变化、多 Agent 混合开发、vibe coding、人工测试回归、快速产品迭代。

---

## 0. 这套范式解决什么

多 Agent 和 vibe coding 的主要风险不是“开发太快”，而是“设计意图漂移太快”。

常见表现：

| 漂移类型 | 表现 | 后果 |
|---|---|---|
| 需求漂移 | 用户原话被 Agent 拆成了方便实现但不符合意图的任务 | 做完很多，但没解决问题 |
| 状态漂移 | 不同模块对 `pending`、`running`、`done` 的理解不同 | UI、后端、测试各说各的 |
| 边界漂移 | MVP 外能力被顺手做进主流程 | 半成品入口增加，回归成本上升 |
| 测试漂移 | 测试只覆盖页面和 happy path | 状态跳转错误长期隐藏 |
| 记忆漂移 | Agent 把临时判断当成长期事实 | 后续派工越来越偏 |

本范式的核心约束：

```text
任何需求、代码改动、测试、bug、灵感，都必须能落到某个用户/业务对象的状态、迁移、不变量或边界上。
落不上去的内容，不直接进入开发，先进入澄清、Inbox 或 Spike。
```

---

## 1. 核心原则

### 1.1 状态先于功能

不要先问：

```text
要做哪个页面？
要加哪个按钮？
要开哪个接口？
```

先问：

```text
用户是谁？
用户现在处于什么状态？
核心对象是什么？
对象现在处于什么状态？
用户或系统触发了什么动作？
动作后应该进入什么状态？
哪些状态不允许进入？
用户如何知道状态已经变化？
```

功能是状态迁移的触发器。接口是状态迁移的承载。UI 是状态的可视化。测试是迁移正确性的证明。

### 1.2 MVP 是闭合子图

MVP 不是功能少，而是状态图小。

一个合格 MVP 必须包含：

- 一个入口状态。
- 一个成功出口。
- 一个失败出口。
- 一条最短可用路径。
- 一组明确不做的旁支。
- 一组最小可验证验收标准。

### 1.3 LLM 可以灵活实现，但不能灵活解释状态

允许 Agent 在实现上发挥：

- 选择代码组织方式。
- 补充局部测试。
- 优化内部实现。
- 发现遗漏后提出建议。

不允许 Agent 自行决定：

- 新增业务状态。
- 改变已有状态语义。
- 把非 MVP 状态接入主流程。
- 把临时 Spike 合并成正式实现。
- 用自己的理解替换状态账本。

---

## 2. 文档和记忆分层

### 2.1 L0：状态账本

建议文件：

```text
STATE_MODEL.md
```

这是最高优先级文档，只记录稳定事实：

| 字段 | 内容 |
|---|---|
| Actor | 用户、系统、Agent、外部服务 |
| Object | 订单、任务、会话、审批、运行实例、文档等 |
| State | 对象可能处于的状态 |
| Transition | 合法状态迁移 |
| Guard | 迁移前置条件 |
| Side Effect | 迁移产生的副作用 |
| Invariant | 迁移前后必须保持的约束 |
| Illegal Transition | 明确禁止的跳转 |
| MVP Slice | 当前版本覆盖的状态子图 |

L0 不写长篇解释，不记录讨论过程，只写约束。

### 2.2 L1：PRD / 架构说明

L1 解释为什么：

- 产品目标。
- 用户场景。
- 业务边界。
- 架构模块。
- 数据流。
- 关键取舍。
- 为什么状态机这样设计。

L1 与 L0 冲突时，以 L0 为准，再回补 L1。

### 2.3 L2：派工单 / Bug 单 / 回归单

L2 承载高频变化：

- 本轮目标。
- 涉及状态。
- 涉及迁移。
- 允许改什么。
- 禁止改什么。
- 验收方式。
- 人工测试结果。
- 回归结论。

每个 L2 条目必须引用 L0 的状态或迁移。

### 2.4 L3：灵感 / 临时记忆 / Spike

L3 可以混乱，但不能直接驱动正式开发。

适合放：

- 灵光一现。
- UI 想法。
- 技术尝试。
- 暂未验证假设。
- Agent 中间推理。
- 人工测试观察。

进入开发前，L3 必须被 triage 成 L0、L1 或 L2 变更。

---

## 3. 生命周期主状态机

软件产品生命周期本身也按状态机管理。

| 状态 ID | 生命周期状态 | 核心产物 | 入口条件 | 出口条件 |
|---|---|---|---|---|
| `LC-00` | Idea Captured | 原始想法 | 出现灵感、反馈、bug、竞品启发 | 记录来源、问题、预期收益 |
| `LC-01` | Problem Framed | 问题定义 | 想法未被证伪 | 明确用户、场景、痛点、成功标准、非目标 |
| `LC-02` | Actor & Object Mapped | 用户 / 对象清单 | 问题值得继续 | 明确 actor、object、object relation |
| `LC-03` | Core State Modeled | 核心状态机 | 核心对象已识别 | 定义状态、迁移、guard、不变量、非法迁移 |
| `LC-04` | MVP Slice Selected | MVP 状态子图 | 核心状态机成立 | 选出最小闭环路径和边界外旁支 |
| `LC-05` | Requirement Specified | PRD / 验收标准 | MVP 子图确定 | 每条需求绑定状态或迁移 |
| `LC-06` | Architecture Mapped | 架构映射表 | PRD 状态绑定完成 | 状态/迁移映射到模块、接口、存储、事件、UI |
| `LC-07` | Ready For Dev | 派工单 | 架构映射无 ownerless transition | 任务按状态/迁移切分 |
| `LC-08` | In Development | 代码、单测、变更说明 | 派工单可执行 | 完成实现并声明状态影响 |
| `LC-09` | Integrated | 集成版本 | 单项任务完成 | 多 Agent 产物合并，无冲突状态源 |
| `LC-10` | State-Tested | 测试报告 | 集成版本可运行 | 合法、非法、异常、回归路径通过 |
| `LC-11` | Release Gated | 发布候选 | 测试通过 | 回滚、观测、风险清单完成 |
| `LC-12` | Released & Observed | 已发布版本 | 发布批准 | 线上反馈映射回状态机 |
| `LC-13` | Retrospected | 复盘结论 | 版本有反馈 | 更新 L0，沉淀决策，派生下一轮输入 |

---

## 4. LLM 如何正确拆解人的需求

这是本范式最关键的部分。LLM 不应把人的需求直接拆成“前端任务、后端任务、数据库任务”。正确顺序是：

```text
人的原始表达
-> 需求归一化
-> 用户目标
-> 核心对象
-> 当前状态
-> 目标状态
-> 迁移
-> guard / invariant / side effect
-> MVP 子图
-> 原子需求
-> 架构任务
-> 测试任务
```

### 4.1 第一步：识别需求类型

LLM 先把输入归类。

| 类型 | 例子 | 下一步 |
|---|---|---|
| Feature | “支持用户审批任务” | 建模状态迁移 |
| Bug | “点完成后列表还显示进行中” | 找破坏的状态或迁移 |
| Improvement | “这个流程太绕了” | 找用户卡住的状态 |
| Refactor | “把任务模块重构一下” | 找状态源和边界 |
| Experiment / Spike | “试试看能不能接某个 API” | 标记为 Spike，不进主流程 |
| Test | “补一下回归测试” | 绑定状态迁移 |
| Docs | “把流程说明写清楚” | 绑定状态账本或用户流 |
| Release | “准备发版” | 走发布状态检查 |

如果类型不明确，LLM 不应直接派工，而应输出澄清问题。

### 4.2 第二步：拆出需求五元组

任何需求先拆成五元组：

```text
Actor：谁在行动？
Object：被操作的核心对象是什么？
Current State：对象现在是什么状态？
Trigger：什么动作或事件触发变化？
Target State：动作后应该变成什么状态？
```

示例：

```text
原始需求：
“我希望 agent 跑完以后能提醒我验收。”

Actor：
Agent / System / Human

Object：
AgentRun / ReviewTask

Current State：
AgentRun.running

Trigger：
agent run completed

Target State：
ReviewTask.pending_review
```

如果五元组缺任何一个，需求还不能进入开发。

### 4.3 第三步：补 guard、side effect、invariant

五元组只说明“想怎么跳”，还不够。LLM 必须补三类约束。

```text
Guard：什么时候允许跳？
Side Effect：跳转会产生哪些副作用？
Invariant：跳转前后哪些事实必须不变？
```

示例：

```text
Transition:
AgentRun.running -> ReviewTask.pending_review

Guard:
- run 已结束
- run 有可展示结果
- 该 run 未生成过 review task

Side Effect:
- 创建待审任务
- 写入结果摘要
- 发出 task_changed 事件

Invariant:
- 同一 run 最多生成一个 active review task
- 已失败 run 不伪装成 completed
- 人未验收前不进入 accepted
```

### 4.4 第四步：识别非法迁移

LLM 必须主动生成非法迁移表。

示例：

| 当前状态 | 非法动作 | 原因 |
|---|---|---|
| `running` | 直接标记 `accepted` | 人还没验收 |
| `failed` | 自动生成 `accepted` | 失败结果不能被当成成功 |
| `pending_review` | 重复生成待审项 | 会造成重复提醒 |
| `accepted` | 静默改回 `running` | 破坏验收事实 |

没有非法迁移表的需求拆解是不完整的。

### 4.5 第五步：拆成原子需求

原子需求的粒度标准：

```text
一个 actor
一个 object
一个 source state
一个 trigger
一个 target state
一组 guard
一组 side effect
一组验收标准
```

太粗的需求：

```text
做一个审批系统。
```

合格拆分：

```text
REQ-001: 创建审批请求
Proposal.draft -> Proposal.pending

REQ-002: 用户批准审批
Proposal.pending -> Proposal.approved

REQ-003: 用户拒绝审批
Proposal.pending -> Proposal.rejected

REQ-004: 执行已批准动作
Proposal.approved -> Proposal.running

REQ-005: 执行成功
Proposal.running -> Proposal.succeeded

REQ-006: 执行失败
Proposal.running -> Proposal.failed
```

### 4.6 第六步：把原子需求组成用户流

原子需求不能散落。LLM 要把它们组合成用户流：

```text
User Goal
-> Flow
-> State Path
-> Atomic Requirements
-> Tasks
-> Tests
```

示例：

```text
用户目标：
安全地批准一个高风险工具调用。

状态路径：
Action.detected
-> Proposal.pending
-> Proposal.approved
-> Proposal.running
-> Proposal.succeeded / Proposal.failed

原子需求：
REQ-001, REQ-002, REQ-004, REQ-005, REQ-006

非 MVP 旁支：
- 批量审批
- 审批撤销
- 审批转交
- 多人审批
```

### 4.7 第七步：选择 MVP 子图

LLM 选择 MVP 时，不按“功能看起来小不小”，而按状态图是否闭环。

MVP 至少包括：

- 主成功路径。
- 至少一个失败路径。
- 至少一个退出路径。
- 最小可见 UI。
- 最小可验证数据。

示例：

```text
MVP 内：
pending -> approved -> running -> succeeded
pending -> rejected
running -> failed

MVP 外：
approved -> cancelled
pending -> delegated
pending -> batch_approved
failed -> auto_retry
```

### 4.8 第八步：转成开发任务

开发任务不能直接从原始需求生成，必须从原子需求或迁移生成。

拆分标准：

| 层级 | 粒度 |
|---|---|
| Product slice | 一个用户目标 |
| Requirement | 一个状态迁移或一个状态展示 |
| Architecture task | 一个状态/迁移到模块和数据的映射 |
| Backend task | 一个迁移函数、guard、持久化、事件 |
| Frontend task | 一个 canonical state 的展示和触发 |
| Test task | 一个合法迁移或非法迁移 |
| Regression task | 一个被 bug 破坏的状态或不变量 |

---

## 5. LLM 需求拆解输出格式

### 5.1 标准输出 Schema

LLM 拆完人的需求后，输出必须包含：

```yaml
demand_id:
raw_input:
demand_type:
user_goal:
non_goals:
actors:
objects:
state_candidates:
transitions:
  - transition_id:
    object:
    from_state:
    trigger:
    to_state:
    guard:
    side_effects:
    invariants:
    illegal_transitions:
atomic_requirements:
  - req_id:
    title:
    actor:
    object:
    from_state:
    trigger:
    to_state:
    acceptance:
    priority:
    mvp:
architecture_tasks:
test_tasks:
open_questions:
assumptions:
```

### 5.2 需求质量闸门

进入开发前，LLM 必须检查：

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
| Acceptance 明确 | 知道如何验收 |
| Non-goal 明确 | 知道本轮不做什么 |

任一 P0 闸门缺失，不应直接进入实现。

### 5.3 什么时候问用户澄清

LLM 不需要事事提问。只有阻塞状态机建模时才问。

必须问：

- 不知道 actor 是谁。
- 不知道核心对象是什么。
- 不知道当前状态或目标状态。
- 成功标准无法测试。
- 权限、数据安全、支付、删除等高风险 guard 不明确。
- 用户说法同时包含多个互斥目标。

不必问：

- 内部函数怎么命名。
- 文件怎么组织。
- UI 文案的轻微措辞。
- 可由现有架构惯例决定的实现细节。

澄清问题最多 3 个，优先问会影响状态机的问题。

---

## 6. 常见需求的拆解范式

### 6.1 “加一个功能”

原话：

```text
加一个导出报告功能。
```

不要直接拆成：

```text
前端加按钮
后端加接口
生成 PDF
```

先拆成：

```text
Object:
Report

State Path:
Report.ready -> Report.export_requested -> Report.exporting -> Report.exported
Report.exporting -> Report.export_failed

Atomic Requirements:
- 用户从 ready 状态发起导出请求
- 系统创建导出任务
- 导出成功后生成可下载文件
- 导出失败后展示错误并允许重试

Illegal Transitions:
- Report.draft 不能导出
- Report.exporting 不能重复导出同一请求
```

### 6.2 “修一个 bug”

原话：

```text
任务明明失败了，列表还显示完成。
```

拆解：

```text
Bug Type:
展示状态错误 / 状态映射错误

Broken Transition:
Task.running -> Task.failed

Expected:
canonical_status = failed
UI section = failed items

Actual:
canonical_status = completed

Broken Invariant:
失败结果不能被展示为完成

Regression:
模拟失败 run，验证后端 DTO、前端展示、刷新后状态一致
```

### 6.3 “优化体验”

原话：

```text
这个审批流程太麻烦了，优化一下。
```

不要直接让 Agent “优化 UI”。先定位用户卡住的状态：

```text
可能卡点：
Proposal.pending 不知道风险是什么
Proposal.pending 不知道批准后会发生什么
Proposal.running 不知道是否还在执行
Proposal.failed 不知道如何恢复

拆解方式：
- 为 pending 增加风险摘要
- 为 approved 前增加 side effect preview
- 为 running 增加进度可见性
- 为 failed 增加重试/退出路径
```

### 6.4 “重构”

原话：

```text
把任务模块重构一下。
```

必须先问：

```text
重构要解决哪个状态源问题？
是状态定义重复？
是迁移散落？
是 UI 拼接多个 raw source？
是测试无法覆盖？
```

重构任务必须绑定到：

- 消除重复状态源。
- 建立 canonical read model。
- 合并迁移入口。
- 提高测试可覆盖性。
- 不改变外部状态语义。

### 6.5 “灵光一现”

原话：

```text
我突然想到可以加一个智能推荐下一步。
```

不要直接开发，进入 Inbox：

```text
Captured:
智能推荐下一步

Triaged:
影响对象：TaskQueue / UserFocus
可能状态：User.available, User.focused, Task.pending_review
可能迁移：Task.pending_review -> Task.suggested

Decision:
Accepted / Parked / Rejected

如果 Accepted:
进入下一轮 L2 派工
```

---

## 7. LLM 侧建模一：开发 AgentTeam 与 OODA-R

用户侧状态机回答“产品中的人和业务对象如何变化”。LLM 侧也必须建模，因为开发过程本身也是一个多 Agent 系统。

开发 AgentTeam 的目标不是“尽快写代码”，而是：

```text
先观察上下文
再定向理解问题
再决策设计方案
再落地实现
最后 review 和回归
```

也就是 OODA-R：

```text
Observe -> Orient -> Decide -> Act -> Review
```

### 7.1 OODA-R 循环

| 阶段 | Agent 要做什么 | 输出 | 禁止 |
|---|---|---|---|
| Observe | 读取需求、L0 状态账本、相关代码、现有测试、约束 | 事实清单、未知项、相关状态/迁移 | 直接写代码 |
| Orient | 判断需求类型、状态影响、架构影响、风险、边界 | 状态影响分析、设计选项 | 只按文件名猜测 |
| Decide | 选择最小方案、切分任务、确定验收和回归 | 实施计划、文件范围、测试计划 | 无计划开工 |
| Act | 在允许范围内实现 | 代码、测试、文档增量 | 越界重构、静默改状态语义 |
| Review | 自查 diff、测试、状态机影响、边界、回归 | review 结论、风险、返工项 | 只报“已完成” |

硬规则：

```text
未完成 Observe / Orient，不允许进入 Act。
未完成 Decide，不允许改代码。
未完成 Review，不允许标记任务完成。
Review 发现状态语义变化，必须回到 Decide 或更新 L0。
```

### 7.2 开发 Agent 生命周期状态机

开发期每个 Agent 都应处于明确状态。

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

| 状态 | 定义 | 进入条件 | 退出条件 |
|---|---|---|---|
| `Idle` | 未接任务 | 无 active assignment | 收到派工单 |
| `Assigned` | 已接任务但未观察 | 派工单明确 | 开始读取上下文 |
| `Observing` | 收集事实 | 读取需求、状态账本、代码、测试 | 输出事实清单和未知项 |
| `Oriented` | 已理解问题空间 | 完成状态影响分析 | 输出设计选项和风险 |
| `DesignReady` | 方案可执行 | 选定方案、文件范围、验收方式 | 进入实现 |
| `Implementing` | 正在编码 | 方案通过 | 代码和测试完成 |
| `SelfReviewing` | 自查阶段 | 完成实现 | 输出 diff 解释、测试结果、状态影响 |
| `ReviewReady` | 等待外部 review | 自查通过 | Review Agent / 人通过或打回 |
| `Accepted` | 任务完成 | review 通过 | 进入集成或归档 |
| `ReworkRequired` | 需要返工 | review 发现问题 | 回到 Orient / Decide / Act |
| `Blocked` | 无法推进 | 缺需求、权限、上下文、外部依赖 | 用户或上游 Agent 解除阻塞 |

### 7.3 开发 Agent 角色

| 角色 | 负责 | 禁止 |
|---|---|---|
| Product Agent | 用户目标、PRD、状态机变更建议 | 直接改代码 |
| Architecture Agent | 状态到模块、数据、接口、事件的映射 | 绕过 L0 做实现 |
| Implementation Agent | 按派工单实现代码 | 静默新增状态或扩大 MVP |
| Test Agent | 合法/非法迁移测试、回归路径 | 只测页面 happy path |
| Review Agent | 发现状态漂移、边界扩张、重复状态源 | 做无边界重构 |
| Docs Agent | 同步已确认事实到 L0/L1/L2 | 记录所有讨论和中间推理 |

角色边界：

```text
Product Agent 可以建议状态变化，但不落代码。
Architecture Agent 可以设计承载方式，但不偷改需求。
Implementation Agent 只能实现已通过 Decide 的方案。
Test Agent 以状态迁移为测试对象。
Review Agent 优先找状态源分裂、语义漂移、MVP 越界。
Docs Agent 只同步已确认事实。
```

### 7.4 AgentTeam 分工拓扑

推荐把一轮开发拆成串并结合：

```text
Product Agent
    -> Architecture Agent
        -> Implementation Agent(s)
        -> Test Agent
    -> Review Agent
        -> Docs Agent
```

并行规则：

- Product 和 Architecture 可以先后串行，不能跳过。
- 多个 Implementation Agent 可以并行，但必须拥有不重叠的写范围。
- Test Agent 可以在实现前先写测试矩阵，也可以在实现中并行补回归。
- Review Agent 不参与实现，保持独立判断。
- Docs Agent 只在 review 后同步确认事实。

### 7.5 OODA-R 派工单模板

```md
# Agent 派工单

## 任务 ID

TASK-YYYYMMDD-XXX

## 原始需求

用户原话或已归一化需求。

## 本次目标

只完成什么。

## OODA-R 要求

Observe：

- 必须读取哪些文档、代码、测试、状态账本。

Orient：

- 需要判断哪些状态、迁移、边界、风险。

Decide：

- 必须先输出方案、文件范围、测试计划。

Act：

- 允许修改的文件、模块、接口。

Review：

- 必须自查哪些状态影响、测试结果、边界和风险。

## 关联状态机

对象：

状态：

迁移：

不变量：

非法迁移：

## 允许修改范围

文件、模块、接口、测试范围。

## 禁止修改范围

本次不允许做什么。

## 验收方式

自动测试：

人工测试：

状态机回归：

## 输出要求

说明新增/修改状态、迁移、不变量、测试证据、风险。
```

### 7.6 Agent 输出格式

```md
## Observe

读取了什么，确认了哪些事实，还有哪些未知项。

## Orient

影响哪些状态、迁移、不变量、模块和风险。

## Decide

选择的方案、放弃的方案、原因、文件范围、测试计划。

## Act

实际完成内容。

## Review

自查结果、测试结果、状态机影响、风险、建议回归点。
```

### 7.7 开发期硬闸门

| 闸门 | 不满足时 |
|---|---|
| 需求未映射状态 / 迁移 | 不进入开发 |
| Agent 未完成 Observe / Orient | 不允许写代码 |
| Agent 未输出 Decide | 不允许落地 |
| 方案改变状态语义 | 先更新 L0 或交给 Product / Architecture Agent |
| 实现越过允许写范围 | 打回 |
| 测试未覆盖 P0 迁移 | 不允许标记完成 |
| Review 发现状态源分裂 | 打回到 Architecture |

---

## 8. LLM 侧建模二：项目内 Agent 状态机与权限边界

项目内部运行的 Agent 也必须建模。它不是“一个能调用 LLM 的黑盒”，而是有生命周期、能力范围、权限状态、工具调用状态、记忆状态和审计边界的系统对象。

### 8.1 项目内 Agent 的核心对象

| 对象 | 含义 |
|---|---|
| `Agent` | 具备目标、角色、能力和约束的执行体 |
| `AgentTask` | Agent 被要求完成的工作项 |
| `AgentRun` | 一次具体执行实例 |
| `ToolCall` | Agent 触发的一次工具调用 |
| `PermissionGrant` | 用户或系统授予的权限 |
| `WorkspaceScope` | Agent 可读写的边界 |
| `MemoryEntry` | Agent 可读或可写的记忆 |
| `AuditEvent` | 可追踪的行为记录 |

### 8.2 Agent 生命周期状态机

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

| 状态 | 定义 | 允许动作 |
|---|---|---|
| `Created` | Agent 已创建但未配置 | 设置角色、能力、边界 |
| `Configured` | 角色和权限边界已定义 | 激活、禁用 |
| `Idle` | 等待任务 | 接收任务 |
| `Planning` | 生成计划和风险评估 | 输出 plan，不执行高风险动作 |
| `AwaitingApproval` | 等待用户或策略批准 | approve / reject / modify |
| `Running` | 正在执行任务 | 调用低风险能力、生成中间结果 |
| `ToolCalling` | 正在执行工具 | 记录 tool call、结果、错误 |
| `Paused` | 被用户、策略或错误暂停 | resume / stop |
| `Succeeded` | 执行成功 | 记录结果、等待验收或归档 |
| `Failed` | 执行失败 | 解释错误、允许重试或退出 |
| `Stopped` | 被中止 | 释放资源、记录原因 |
| `Archived` | 已归档 | 只读查看 |

非法迁移示例：

| 当前状态 | 非法迁移 | 原因 |
|---|---|---|
| `Created` | 直接 `Running` | 未配置角色和边界 |
| `Planning` | 直接执行高风险工具 | 未完成风险评估和审批 |
| `AwaitingApproval` | 静默进入 `Running` | 绕过用户或策略授权 |
| `Failed` | 伪装成 `Succeeded` | 破坏结果可信度 |
| `Archived` | 继续调用工具 | 归档态应只读 |

### 8.3 AgentTask 状态机

```text
Captured
-> Classified
-> Planned
-> Assigned
-> InProgress
-> AwaitingHuman
-> Completed / Failed / Cancelled
-> Reviewed
```

关键约束：

- `Captured` 只表示任务被记录，不表示 Agent 可以执行。
- `Classified` 必须明确任务类型、风险、所需能力。
- `Planned` 必须有执行计划和边界。
- `Assigned` 必须绑定具体 Agent 和 WorkspaceScope。
- `AwaitingHuman` 必须说明等待人做什么。
- `Completed` 不等于业务成功，仍可能需要 human review。

### 8.4 ToolCall 状态机

```text
Proposed
-> RiskClassified
-> Approved / Rejected
-> Executing
-> Succeeded / Failed / TimedOut
-> Recorded
```

ToolCall 必须记录：

```text
tool_id
agent_id
task_id
workspace_scope
input_summary
risk_level
permission_grant
started_at
finished_at
result_summary
error
audit_ref
```

非法行为：

- 未分类风险就执行。
- 高风险工具无授权执行。
- 工具结果不写审计。
- 失败工具伪装成功。
- 工具输出直接污染长期记忆。

### 8.5 PermissionGrant 状态机

```text
Unrequested
-> Requested
-> ApprovedOnce / ApprovedSession / Denied
-> Expired / Revoked
```

权限必须区分：

| 权限类型 | 含义 |
|---|---|
| `ApprovedOnce` | 只允许本次动作 |
| `ApprovedSession` | 当前会话内允许 |
| `Denied` | 明确拒绝 |
| `Expired` | 超时失效 |
| `Revoked` | 用户或策略主动撤销 |

规则：

- 默认最小权限。
- 写操作、命令执行、网络访问、外部系统调用必须可审计。
- 权限不能跨 workspace 静默复用。
- 权限不能从低风险工具推导到高风险工具。
- 用户撤销后，后续动作必须重新请求。

### 8.6 能力分级

| 能力级别 | 允许行为 | 典型 guard |
|---|---|---|
| `Observe` | 只读上下文、状态、日志 | 不读取敏感区 |
| `Read` | 读取工作区文件或数据 | 限定 workspace scope |
| `Propose` | 生成建议、计划、patch preview | 不直接落地 |
| `Write` | 修改文件、数据或状态 | 需要 workspace trust 或 approval |
| `Execute` | 执行命令、工具、外部调用 | 需要风险分类和授权 |
| `Delegate` | 派发子任务或启动子 Agent | 需要任务边界和审计 |
| `Remember` | 写入长期记忆 | 需要记忆分类和可删除性 |

### 8.7 边界设定

项目内 Agent 至少需要这些边界：

| 边界 | 约束 |
|---|---|
| Workspace boundary | 只能访问授权工作区 |
| Tool boundary | 只能调用白名单工具 |
| Data boundary | 敏感数据不进入非必要上下文 |
| Memory boundary | 临时上下文不自动写长期记忆 |
| Network boundary | 外部访问需声明目的和风险 |
| Command boundary | 命令执行需超时、日志、风险拦截 |
| Human boundary | 高风险动作等待人批准 |
| Delegation boundary | 子 Agent 不能继承超出任务范围的权限 |

### 8.8 记忆状态机

```text
Candidate
-> Classified
-> Approved
-> Stored
-> Retrieved
-> Expired / Deleted
```

记忆写入前必须判断：

- 是否是稳定事实。
- 是否会影响未来决策。
- 是否包含敏感信息。
- 是否有删除路径。
- 是否已经进入正式文档。

工具输出、Agent 中间推理、临时错误现象，默认不写长期记忆。

### 8.9 项目内 Agent 设计清单

设计一个项目内 Agent 前，必须回答：

```text
Agent 的角色是什么？
它能处理哪些 Task 类型？
它有哪些状态？
它能调用哪些工具？
每个工具的风险级别是什么？
哪些动作需要人批准？
它能读写哪些 workspace？
它能写长期记忆吗？
它能派发子 Agent 吗？
失败后如何恢复？
所有行为如何审计？
```

如果这些问题没有答案，这个 Agent 不能进入生产主流程，只能作为 Spike。

---

## 9. 测试、验收与回归

### 9.1 测试矩阵

| 字段 | 含义 |
|---|---|
| State | 当前状态 |
| Trigger | 用户动作、系统事件、Agent 事件、定时器、外部回调 |
| Guard | 迁移前置条件 |
| Expected Next State | 期望下一状态 |
| Forbidden Next State | 明确禁止跳转的状态 |
| Side Effects | 数据写入、通知、任务创建、文件变更、Agent 调用 |
| Invariant | 迁移前后必须保持的约束 |
| UI Expectation | 用户看到的状态、按钮、提示 |
| Test Type | 单元 / 集成 / E2E / 手工 |
| Priority | P0 / P1 / P2 |

### 9.2 自动化测试负责什么

自动化优先覆盖：

- P0 合法迁移。
- P0 非法迁移。
- guard 判断。
- side effect 是否只发生一次。
- 失败、重试、停止。
- 幂等。
- 刷新 / 重启恢复。
- canonical read model 输出。

### 9.3 人工测试负责什么

人工测试重点看：

- 用户是否理解当前状态。
- 用户是否知道下一步做什么。
- 失败是否可解释、可恢复。
- MVP 边界是否干净。
- Agent 输出是否满足原始意图。
- 灵感是否污染了当前主流程。

### 9.4 Bug 回归单模板

```md
# Bug 回归单

## Bug ID

BUG-xxx

## 影响状态

`STATE.xxx`

## 影响迁移

`TRANSITION.xxx`

## 错误前置状态

...

## 实际后置状态

...

## 期望后置状态

...

## 破坏的不变量

...

## 回归路径

1. 构造状态。
2. 触发迁移。
3. 验证状态。
4. 验证 UI。
5. 验证副作用。
6. 刷新 / 重启后再次验证。
```

---

## 10. 每轮开发后的同步仪式

每轮结束后，用 10-15 分钟做状态同步。

```md
## 状态机同步检查

- 是否新增状态：是 / 否
- 是否新增迁移：是 / 否
- 是否修改状态语义：是 / 否
- 是否新增非法状态组合：是 / 否
- 是否存在 UI 展示状态和后端状态不一致：是 / 否
- 是否存在多个模块各自拼接状态：是 / 否
- 是否有状态无法通过测试进入：是 / 否
- 是否有状态无法退出：是 / 否
- 是否有新灵感进入 Inbox：是 / 否
- 是否有稳定事实需要进入长期记忆：是 / 否
```

下一轮任务只能来自：

- 未完成的 L2 任务。
- 人工测试失败项。
- 已 Accepted 的 Inbox idea。
- L0 标记的状态机缺口。
- 发布后观察到的高频失败迁移。

---

## 11. 发布检查

发布前必须确认：

| 检查项 | 要求 |
|---|---|
| 新增 / 修改状态已登记 | 必须 |
| 新增 / 修改迁移已登记 | 必须 |
| P0 合法迁移有测试证据 | 必须 |
| P0 非法迁移有拒绝测试 | 必须 |
| MVP 边界外能力未暴露主入口 | 必须 |
| Bug 回归单绑定状态和迁移 | 必须 |
| 前端展示使用统一状态源 | 必须 |
| 失败态可见、可解释、可恢复或可退出 | 必须 |
| 发布说明包含状态机变更摘要 | 必须 |

发布判定：

```text
P0 状态闭环不完整，不发布。
P0 非法迁移不能被阻止，不发布。
状态源不统一，只能灰度或内部发布。
失败态不可恢复，必须明确降级方案。
MVP 边界被突破，必须回收入口或升级 PRD。
```

---

## 12. Skill 化使用方式

如果把这份范式抽成 Agent Skill，建议 skill 的触发描述覆盖这些场景：

```text
Use when turning raw product ideas, user requirements, bug reports, or vague feature requests into a state-machine-driven PRD, architecture plan, Agent task split, test matrix, regression plan, or lifecycle governance workflow. Especially useful for multi-agent software development, vibe coding, MVP scoping, and reducing design-intent drift.
```

Skill 执行时应固定按以下顺序工作：

```text
1. 读取用户原始需求。
2. 判断需求类型。
3. 拆 Actor / Object / Current State / Trigger / Target State。
4. 补 Guard / Side Effect / Invariant。
5. 生成合法迁移和非法迁移。
6. 拆原子需求。
7. 组合用户流。
8. 选择 MVP 子图。
9. 生成架构映射。
10. 建模开发 AgentTeam 的 OODA-R 分工和闸门。
11. 建模项目内 Agent 的生命周期、权限和工具边界。
12. 生成 Agent 派工单。
13. 生成测试矩阵和回归清单。
14. 标记需要澄清的问题和不应进入开发的灵感。
```

Skill 的核心输出不应该是“任务列表”，而应该是：

```text
状态模型
原子需求
MVP 子图
架构映射
开发 AgentTeam OODA-R 分工
项目内 Agent 状态机与权限模型
派工单
测试矩阵
回归点
开放问题
```

---

## 13. 最小执行版本

如果不想引入完整流程，只保留三件事：

1. 一个 `STATE_MODEL.md`。
2. 一个固定 OODA-R Agent 派工单模板。
3. 一个项目内 Agent 权限边界清单。
4. 每轮结束后的状态机同步检查。

每次需求进入时，只做最小拆解：

```text
Actor 是谁？
Object 是什么？
From State 是什么？
Trigger 是什么？
To State 是什么？
Guard 是什么？
验收怎么证明？
本轮不做什么？
```

每次派发开发 Agent 前，只做最小 OODA-R 检查：

```text
Observe 要读取什么？
Orient 要判断什么状态影响？
Decide 的方案和文件范围是什么？
Act 允许改哪里？
Review 如何证明没有状态漂移？
```

每次设计项目内 Agent 前，只做最小权限检查：

```text
Agent 能做什么？
不能做什么？
能读哪里？
能写哪里？
哪些工具需要审批？
哪些行为必须审计？
失败后如何停止或恢复？
```

只要这一步坚持执行，就能在保持敏捷速度的同时，把设计意图漂移控制在可见范围内。
