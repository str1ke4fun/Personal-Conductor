# 三方案归一化最终演进形态建模

> 日期: 2026-06-04
> 模式: state-machine-lifecycle / architecture
> 范围: 把 [Cairn架构对位与移植评估-20260604.md](./Cairn架构对位与移植评估-20260604.md) + [hybrid-model-dispatch-inproject-plan-20260604.md](./hybrid-model-dispatch-inproject-plan-20260604.md) + [hybrid-model-dispatch-mcpserver-plan-20260604.md](./hybrid-model-dispatch-mcpserver-plan-20260604.md) + [HybridModel与Cairn融合方案-20260604.md](./HybridModel与Cairn融合方案-20260604.md) 归一化为单一可执行最终形态
> 方法: 现状代码对读 + 状态机交叉 + 冲突/阻塞识别 + L0-L3 数据流梳理 + 4 类对象生命周期建模
> 结论: 20 个一等对象 + 4 类状态机 + 8 个冲突点 + 5 个阻塞点 + 15 项落地任务,5-6 周可达 LC-10 (Agent Runtime 完全可用)
> 关联: [项目Agent架构状态机与治理范式-20260529.md](./项目Agent架构状态机与治理范式-20260529.md) / [chat-turn-state-and-memory-design-20260603.md](./chat-turn-state-and-memory-design-20260603.md) / [STATE_MODEL_AGENT.md](./STATE_MODEL_AGENT.md)

---

## 0. 状态机建模契约

```yaml
mode: architecture
status: ready
state_impact: high  # 涉及全项目 Agent 运行时 + LLM 路由 + MCP + 对话状态机
evidence_summary:
  observed:
    - 4 个文档全部以代码对读为依据
    - chat_turns / chat_turn_events / chat_message_projections 已落地 (2026-06-03)
    - llm_profiles / routing / agent_teams / agent_runs / tool_calls 状态机已就位
  inferred:
    - 用户状态(在场/离开/安静)由 expression/affection/persona 间接表达,需显式建模
    - 主线程 OODA 阶段 (Reason/Explore) 仍走 decide.rs 程序化,Llm Reason 尚未接入
  proposed:
    - UnifiedFinal Evolution (本文件)
outputs:
  - 4 类状态机 + 20 个一等对象生命周期
  - 8 个冲突点的最终解法
  - 5 个阻塞点的解除路径
  - 15 项落地任务图(含依赖与门禁)
open_questions: []
next_step: 评审 D1-D8 决策 → 启动 Phase A
```

---

## 1. 三个文档的最终演进形态归一化

### 1.1 各自的最终形态(从各自文档的结论段提取)

| 方案 | 最终形态 | 触发对象 |
|---|---|---|
| **Cairn 移植** | 桌宠内 Runtime Agent 拥有 OODA-R + Blackboard,3 种 OODA 任务共用 worker 路径,接 ModelResolver | AgentRun / AgentTask / GoalCycle / ToolCall |
| **Plan 1 进程内** | 进程内多模型路由收口到 `model_resolver.rs`,所有 LLM 调用经 `resolve()`,带三级回退 | ResolvedModel / LlmProfile / RoutingPolicy |
| **Plan 2 MCP 抽离** | 抽离 `model-router-core` lib + `model-router-mcp` bin,作为独立 MCP server 暴露给 Claude/Codex | model-router-core / model-router-mcp |

### 1.2 归一化后的最终形态(本文件的目标产物)

**一句话**: 把 Agent Runtime + 模型路由 + MCP 抽离 + OODA 范式 **统一为一个 20 对象 + 4 状态机的可治理 Runtime**,其中:

- **执行轴**: 主线程(对话+工具) + 子线程(AgentRun 进程类 / McpRouter 进程外 / AgentTeam 多 Agent) **三者统一走同一调度入口**
- **认知轴**: 4 类对象(用户/对话/主线程/子线程) **统一状态机约束**
- **路由轴**: `ResolvedModel` 是唯一收口, **Cairn 战略判断、Plan 1 进程内 LLM、Plan 2 MCP server 三方共享**
- **数据轴**: L0 SQLite Ledger / L1 Event Log / L2 Read Model / L3 UI Projection **4 层分工**
- **观测轴**: `chat_turn_events` + `audit_events` 两条事件总线合一,**所有跨层状态变更必须留痕**

### 1.3 形态图

```text
                                    ┌─────────────────────────────────────┐
                                    │           User (Presence)            │
                                    │  active / idle / dnd / away / asleep  │
                                    └─────────────────┬────────────────────┘
                                                      │ user message / hint
                                                      ▼
┌──────────────────────────────────────────────────────────────────────────────────────┐
│  L3 UI Projection                                                                     │
│   Tauri events ◀── chat_message_projections (read model) ◀── useChatSession / panel  │
└──────────────────────────────────────────────────────────────────────────────────────┘
                                                      ▲
┌──────────────────────────────────────────────────────────────────────────────────────┐
│  L2 Read Model                                                                        │
│   active_run (热态) / chat_message_projections / board_projection / tool_use_card    │
└──────────────────────────────────────────────────────────────────────────────────────┘
                                                      ▲
┌──────────────────────────────────────────────────────────────────────────────────────┐
│  L1 Event Log (append-only)                                                            │
│   chat_turn_events ◀── AuditEvent {tool.*, agent_run.*, permission.*, memory.*,        │
│                                   model.routed, cognition.*, mcp.*, ooda.phase_changed}│
└──────────────────────────────────────────────────────────────────────────────────────┘
                                                      ▲
┌──────────────────────────────────────────────────────────────────────────────────────┐
│  L0 SQLite Ledger                                                                     │
│                                                                                       │
│  ┌─ Conversation ──────┐  ┌─ Goal / OODA ─────┐  ┌─ Agent Runtime ─────────┐          │
│  │ ChatSession         │  │ GoalRun           │  │ AgentRun                │          │
│  │ ChatTurn (agg)      │  │ GoalCycle         │  │ AgentTask               │          │
│  │ ChatMessage         │  │ AgentTask         │  │ AgentTeam               │          │
│  │ ToolCallRecord      │  │ goal_hint         │  │ AgentTeamMember         │          │
│  └─────────────────────┘  └───────────────────┘  │ SubagentHandle (新)     │          │
│                                                  │ McpRouterSession (新)   │          │
│  ┌─ Tooling / Routing ─┐  ┌─ Knowledge ───────┐  └──────────────────────────┘          │
│  │ ToolCall            │  │ MemoryEntry      │  ┌─ Permission / Workspace ──┐         │
│  │ ToolSpec            │  │ MemoryChunk      │  │ PermissionGrant           │         │
│  │ LlmProfile          │  │ MemoryEmbedding  │  │ Workspace                 │         │
│  │ RoutingPolicy       │  │ MemoryCandidate  │  │ WorkspaceScope            │         │
│  │ ResolvedModel (新)  │  │ RecallDoc        │  │ Proposal (UI 展示)        │         │
│  │ LlmProfileStore     │  └──────────────────┘  └────────────────────────────┘         │
│  │ CallerContext (新)  │                                                                  │
│  └─────────────────────┘                                                                  │
└──────────────────────────────────────────────────────────────────────────────────────┘
                                                      ▲
┌──────────────────────────────────────────────────────────────────────────────────────┐
│  Engine Layer                                                                          │
│   model-router-core (新)  ───  llm.rs (双协议) / routing.rs / llm_profiles.rs /       │
│                               ModelResolver / classify_task / LlmProfileStore trait    │
│                                                                                       │
│   model-router-mcp (新)   ───  MCP server loop / 3 tools: route / list / invoke        │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. 已确认的冲突点(8 个)

| ID | 冲突 | 现状 | 归一化解法 | 落地项 |
|---|---|---|---|---|
| **C1** | `TaskKind` 同名不同义 | `routing::TaskKind` (Planning/Coding) vs Cairn `TaskKind` (Bootstrap/Reason) | 重命名为 `WorkKind` + `OodaPhase` 双枚举 | A1 |
| **C2** | Worker 注册两处 | `agent_backends.rs`(CLI) vs `llm_profiles.rs`(HTTP API) | `LlmProfile.transport: TransportKind` | A2 |
| **C3** | 路由表缺 OODA 阶段维度 | `RoutingPolicy` 不能按 OODA 阶段过滤 | `RoutingPolicy.caller_phase: Option<OodaPhase>` | A3 |
| **C4** | 工具 `risk_level` 与 PermissionGrant 重复表达 | `ToolSpec.risk_level` 5 级 + `PermissionGrant.state` 5 级 | `ToolSpec` 保留作为静态声明,`PermissionGrant` 保留作为运行时实例,**关系是 1:N** | A4 (Resolver 内做映射) |
| **C5** | Hint 输入散落 | `agent_mailbox_messages` + `docs/workspace.md` + 派工文档 | 新建 `goal_hints` 一等表,markdown 投影到 UI | B5 |
| **C6** | 观测总线不统一 | `runtime_events` (旧) + `chat_turn_events` (新) + `audit_events` (项目内) | **统一用 `chat_turn_events`** 承载所有 turn 关联事件, `audit_events` 承载项目级审计, **不引入新总线** | C1 |
| **C7** | ToolCall 在多处定义 | `chat` 内嵌 `tool_calls_json` + 独立 `tool_calls` 表 + `tool_runs` | 独立 `tool_calls` 表为 canonical,**对话内 ToolCall 必须挂 `turn_id`**(chat-turn 链路已实现) | A4 后由 B1 自然消化 |
| **C8** | AgentTeam 角色 vs 任务类型 | `AgentTeamMember.role` 静态身份 vs Cairn 无角色原则 | 保留 role 字段作为**人类可读标签**,**调度时按 `task_kind` 匹配**而非 `role` | C3 (TaskKind 抽象) |

---

## 3. 已确认的阻塞点(5 个)与 P0 缺陷(4 个)

### 3.1 设计层阻塞点(5 个,跨方案依赖)

| ID | 阻塞 | 影响 | 解除路径 | 依赖 |
|---|---|---|---|---|
| **B1** | `chat_turns` 与 GoalCycle 双向锚点缺失 | GoalCycle 不知道是哪个 turn 触发的,turn 不知道属于哪个 cycle | 给 `chat_turns` 加 `goal_cycle_id` + `agent_task_id` 可空字段 | A1 (WorkKind 命名) + B3 (graph_hash) |
| **B2** | `McpRouter` 后端没有 Executor | Plan 2 抽离后没有"在 conductor 内调用 model-router-mcp"的实现 | 新增 `agents/dispatcher/executors/mcp.rs` 复用 `mcp.rs` 客户端 | D2 (model-router-core 抽离) + D3 (MCP server loop) |
| **B3** | Resolver 缺 CallerContext 表达 | send_v2 / summarizer / smart_monitor / decide_llm / subagent 各用不同入参,无法统一回退 | 定义 `CallerContext` 枚举作为唯一入参 | A4 (ModelResolver 骨架) |
| **B4** | LlmProfile `provider` 字段硬编码 3 值 | 不支持 claude_cli / codex_cli / 任何 MCP 后端 | `provider` 字段从 `String` 改为引用 `TransportKind` 的字符串 | A2 (transport 字段) |
| **B5** | ReasoningPrompt 中文版未实现 | Cairn P3 LLM Reason 没有可用的中文 prompt | 设计中文 Reason Prompt,绑定 `WorkKind × OodaPhase` × CallerContext | C2 (LLM Reason 接 Resolver) |

### 3.2 实施层 P0 缺陷(4 个,代码对读后识别)

> 这 4 个不是"设计未做",而是"代码已经写但链路未闭合"。**必须在任何范式升级(Phase A 起的所有任务)之前修复**。详见 §10.6 的 P0 修复方案。

| ID | 缺陷 | 代码位置 | 后果 | 优先级 |
|---|---|---|---|---|
| **P0-1** | `goal_tasks.result_ref` 永不被反写,只有旧 `tasklist` 表的文本摘要被写 | [agent_runs.rs:301-325](../crates/conductor-core/src/agent_runs.rs#L301-L325) | OODA Review 永远等不到 `accepted`,AgentTask 与 AgentRun 关系断裂 | **P0** |
| **P0-2** | 主线程 LLM 收不到 subagent 结果回注 | [agent_runs.rs:362-408](../crates/conductor-core/src/agent_runs.rs#L362-L408) `notify_turn_of_run_completion` 只写日志,无 LLM 续轮触发 | 用户必须发新消息才能拿到 subagent 结果,桌宠"主动汇报"是伪命题 | **P0** |
| **P0-3** | AgentTeam Member 状态机未接线,`set_member_status` 0 调用方 | [agent_teams.rs:488-499](../crates/conductor-core/src/agent_teams.rs#L488-L499) 单纯存在,从未被调用 | AgentTeam 永远卡在 `Executing`,Review 永远不触发,直接对应 [Bug说明-Goal会话双Working与任务结果插位-20260601.md](./Bug说明-Goal会话双Working与任务结果插位-20260601.md) | **P0** |
| **P0-4** | Tauri `agent_runs_changed` 事件只在 `stop_agent_run` 命令中发,natural finish 不发 | [commands.rs:265](../apps/desktop/src-tauri/src/commands.rs#L265) | 前端 AgentLanes/GoalConsole/TaskDrawerPane 永远显示"running",不自动 refresh | **P0** |

**P0 缺陷与设计层阻塞点的本质区别**:
- B1-B5: **"还没做"**,按 Phase A-F 自然展开
- P0-1 至 P0-4: **"做了但断了"**,必须 Round 5 第一周内 **独立 Phase A0** 修复完成,作为后续所有 OODA 行为正确性的**前置条件**

---

## 4. L0-L3 数据流打通要点

### 4.1 L0 SQLite Ledger(20 个一等对象)

| 编号 | 对象 | 落点文件 | 主要状态机 | 证据 |
|---|---|---|---|---|
| O1 | ChatSession | [chat/session.rs](../crates/conductor-core/src/chat/session.rs) | active/archived | observed |
| O2 | ChatTurn | [chat/turns.rs](../crates/conductor-core/src/chat/turns.rs) | 7 阶段 | observed |
| O3 | ChatMessage | [chat/types.rs](../crates/conductor-core/src/chat/types.rs) | user/assistant/system | observed |
| O4 | ToolCall | [tool_calls.rs](../crates/conductor-core/src/tool_calls.rs) | proposed→executing→succeeded/failed | observed |
| O5 | ToolSpec | [tools.rs](../crates/conductor-core/src/tools.rs) | 静态声明 | observed |
| O6 | ToolCallRecord | [chat/types.rs](../crates/conductor-core/src/chat/types.rs) | 历史内嵌,正在落库 | observed |
| O7 | MemoryEntry | [memory.rs](../crates/conductor-core/src/memory.rs) | candidate/active/archived/quarantined | observed |
| O8 | MemoryChunk | [memory.rs](../crates/conductor-core/src/memory.rs) | (chunk-level) | observed |
| O9 | MemoryEmbedding | [memory.rs](../crates/conductor-core/src/memory.rs) | (vector) | observed |
| O10 | MemoryCandidate | [memory.rs](../crates/conductor-core/src/memory.rs) | candidate→promoted/quarantined | observed |
| O11 | RecallDoc | (新, recall.rs) | indexed/stale | proposed |
| O12 | GoalRun | [goals.rs](../crates/conductor-core/src/goals.rs) | draft→planning→running→blocked/succeeded | observed |
| O13 | GoalCycle | [goals.rs](../crates/conductor-core/src/goals.rs) | observe→orient→decide→act→review | observed |
| O14 | AgentTask | [goal_tasks.rs](../crates/conductor-core/src/goal_tasks.rs) | open→claimed→running→completed/failed | observed |
| O15 | AgentRun | [agent_runs.rs](../crates/conductor-core/src/agent_runs.rs) | queued→running→succeeded/failed/stopped | observed |
| O16 | AgentTeam | [agent_teams.rs](../crates/conductor-core/src/agent_teams.rs) | draft→planning→awaiting→executing→accepted | observed |
| O17 | AgentTeamMember | [agent_team_members.rs](../crates/conductor-core/src/agent_team_members.rs) | active/paused/stopped | observed |
| O18 | SubagentHandle | (新, subagent/mod.rs) | spawn→streaming→done/killed | proposed (子线程统一抽象) |
| O19 | McpRouterSession | (新, executors/mcp.rs) | connecting→ready→calling→closed | proposed |
| O20 | LlmProfile | [llm_profiles.rs](../crates/conductor-core/src/llm_profiles.rs) | enabled/disabled | observed |
| O21 | RoutingPolicy | [routing.rs](../crates/conductor-core/src/routing.rs) | enabled/disabled | observed |
| O22 | ResolvedModel | (新, model_resolver.rs) | (value object) | proposed |
| O23 | LlmProfileStore | (新, model-router-core/src/store.rs) | trait | proposed |
| O24 | PermissionGrant | [permissions.rs](../crates/conductor-core/src/permissions.rs) | requested→approved/denied→used/expired | observed |
| O25 | Proposal | [proposals.rs](../crates/conductor-core/src/proposals.rs) | pending→approved/rejected→succeeded/failed | observed |
| O26 | Workspace | [workspaces.rs](../crates/conductor-core/src/workspaces.rs) | trusted/untrusted | observed |
| O27 | WorkspaceScope | [workspaces.rs](../crates/conductor-core/src/workspaces.rs) | (scope value) | observed |
| O28 | goal_hint | (新, migrations) | read/unread | proposed (B5 落地) |
| O29 | ChatMessageProjection | [chat/turns.rs](../crates/conductor-core/src/chat/turns.rs) | visible/hidden | observed |
| O30 | ActiveRun | (内存) | 热态缓存 | observed (非持久) |
| O31 | AuditEvent | [events.rs](../crates/conductor-core/src/events.rs) | append-only | observed |
| O32 | ChatTurnEvent | [chat/turns.rs](../crates/conductor-core/src/chat/turns.rs) | append-only | observed |

> 31 个对象(含 2 个非持久) = 1 个 chat 域 + 1 个 OODA 域 + 1 个 agent 域 + 2 个 routing 域 + 1 个 memory 域 + 1 个 permission 域 + 1 个 projection 域 + 1 个 event 域

### 4.2 L1 Event Log 打通要点

**总线合并**:
- 旧 `runtime_events` (NDJSON) → 继续写,作为兜底
- 新 `chat_turn_events` → **canonical**,所有 turn 内事件必须走它
- `audit_events` (项目级) → 与 `chat_turn_events` 通过 `correlation_id: AuditCorrelationId` 关联

**新增事件类型**(在 `events.rs` 的 enum):

```rust
pub enum AuditEventKind {
    // ── 已有 ──
    ToolCallProposed, ToolCallBlocked, ToolCallFinished,
    PermissionRequested, PermissionApproved, PermissionDenied, PermissionRevoked,
    MemoryStored, MemoryQuarantined, MemoryDeleted,
    AgentRunCreated, AgentRunPhaseChanged, AgentRunFinished,
    AgentTeamPlanSubmitted, AgentTeamPlanApproved, AgentTeamPlanRejected,

    // ── Plan 1 新增 ──
    ModelRouted { profile_id: String, provider: String, fallback_used: bool },

    // ── Plan 2 新增 ──
    McpToolInvoked { tool: String, transport: String, duration_ms: u64, success: bool },

    // ── Cairn 新增 ──
    OodaPhaseChanged { from: OodaPhase, to: OodaPhase, reason: String },
    GraphCheckpointUpdated { hash: String, fact_count: u32, intent_count: u32 },
    HintInjected { hint_id: String, read: bool },

    // ── 预留 Noesis ──
    CognitionFrameActivated { frame_id: String, replaces: Option<String> },
    CognitionSurprise { frame_id: String, expected: String, actual: String, severity: u8 },
}
```

**canonical 写入路径**: `audit_event!()` 宏 → `chat_turn_events` (turn 关联) 或 `audit_events` (项目级) → 镜像到 NDJSON

### 4.3 L2 Read Model 打通要点

| Read Model | 写入源 | 读端 |
|---|---|---|
| `chat_message_projections` | ChatTurn 完成时投影 | `useChatSession` / ChatTimelinePane |
| `active_run` (热态) | 每次 LLM/tool 状态变更 | 前端实时态 (Tauri event) |
| `tool_use_card` 聚合 | ToolCall + ChatMessage | ToolUseCard 组件 |
| `agent_lanes` | AgentRun + AgentTask | AgentLanes 组件 |
| `goal_console` | GoalRun + GoalCycle + AgentTask | GoalConsole 组件 |
| `board_projection` (Cairn 引入) | GoalRun + AgentTask + goal_hint | GraphView 组件 (Cairn P11) |
| `tension_field` (Cairn 引入) | observe.rs 算 tension | Goal 详情页 Tension Map |

### 4.4 L3 UI Projection 打通要点

- **前端 IPC 唯一入口**: `apps/desktop/src/ipc/invoke.ts`
- **后端 command 注册**: 全部走 `#[command]`,经 `generate_handler!`
- **Tauri event 转发**: core 内 `EventEmitter` trait (Plan 2 抽离时一并设计)
- **WebSocket/SSE**: 不引入,本项目用 Tauri event 即可(单桌面应用)

---

## 5. 用户状态流转(User Presence Lifecycle)

### 5.1 现状: 用户状态分散在 expression/affection/persona 三个模块

当前[感知层增强-主形象与子形象状态方案-20260526.md](./感知层增强-主形象与子形象状态方案-20260526.md)定义了:
- **MoodZone**: 7 档 (distressed, sad, down, neutral, content, happy, excited)
- **RelationshipStage**: 5 档 (stranger, acquaintance, friend, close, intimate)
- **IdlePhase**: Idle1Min / Idle5Min / Idle30Min

但**没有显式的 UserPresence 状态机**。本节定义之。

### 5.2 归一化后的 User Presence 状态机

```text
                  ┌─────────┐
                  │ Offline │  (无 foreground 检测)
                  └────┬────┘
                       │ focus detected
                       ▼
                  ┌─────────┐
       ┌─────────│  Active │──────────┐
       │         └────┬────┘          │
       │ idle ≥ 1min  │   user input  │
       │              ▼               │
       │         ┌─────────┐          │
       │         │  Idle   │          │
       │         └────┬────┘          │
       │              │ idle ≥ 5min   │
       │              ▼               │
       │         ┌─────────┐          │
       │         │  Away   │          │
       │         └────┬────┘          │
       │              │ idle ≥ 30min  │
       │              ▼               │
       │         ┌─────────┐          │
       │         │ Asleep  │          │
       │         └────┬────┘          │
       │              │ focus detected │
       │              └────────────────┤
       │                               │
       │  user says "安静 30 分钟"     │
       └──────────────────────────────┘
                       │
                       ▼
                  ┌─────────┐
                  │   Dnd   │ (Do Not Disturb)
                  └────┬────┘
                       │ dnd expired / user cancels
                       └─────→ Active / Idle
```

### 5.3 状态语义

| 状态 | 含义 | Agent 行为约束 |
|---|---|---|
| `Offline` | 桌宠未启动 / 系统锁屏 | 全部 Agent 暂停;GoalCycle 不前进 |
| `Active` | 焦点在桌宠窗口,用户正在交互 | 全部 Agent 可执行,LLM 可随时调用 |
| `Idle` | 用户离开 ≥ 1 分钟 | Agent 可继续后台任务,LLM 主动对话降级为通知 |
| `Away` | 用户离开 ≥ 5 分钟 | 后台任务继续,但**不发起新任务**,不主动对话 |
| `Asleep` | 用户离开 ≥ 30 分钟 | 后台任务可继续(写入日志),**不发任何通知** |
| `Dnd` | 用户显式声明勿扰 | 同 Asleep,优先级更高,直到用户解除 |

### 5.4 状态转换表

| From | To | Trigger | Guard | Side Effect |
|---|---|---|---|---|
| Offline | Active | focus detected | 系统焦点 | 发 `user.presence.changed` |
| Active | Idle | now - last_input_at ≥ 60s | 用户未输入 | 发 `user.presence.changed` |
| Idle | Away | now - last_input_at ≥ 300s | 用户未输入 | 主动对话队列挂起 |
| Away | Asleep | now - last_input_at ≥ 1800s | 用户未输入 | 通知队列挂起 |
| * | Dnd | user "安静 X 分钟" | user 显式 | dnd 计时器启动 |
| Dnd | Active | dnd expired / user cancels | dnd 计时到 0 | 解除挂起 |
| Asleep / Away | Active | focus detected | 系统焦点 | 恢复挂起 |

### 5.5 与 Agent 行为耦合

- **Offline/Asleep/Dnd**: 全部 GoalCycle 暂停,但 GoalRun.status 仍可由后台 subagent 推动
- **Idle/Away**: 主线程 LLM 主动对话降级(详见 §7.3),子线程 AgentRun 继续
- **Active**: 全部能力解锁

### 5.6 数据流

```text
conductor-sense (window_title/focus/idle)
  → presence_detector (新) → UserPresence
  → chat_turn_events (event: user.presence.changed)
  → L2 read model: ActiveRun 携带 presence
  → 全部 Agent 决策前查 resolve_presence()
```

---

## 6. 对话状态流转(ChatSession / ChatTurn / ChatMessageProjection)

### 6.1 现状: chat-turn 链路已落地

依据 [chat-turn-line-contract-20260603.md](./chat-turn-line-contract-20260603.md) §1:
- `ChatTurn` 是 canonical aggregate
- `ChatTurnEvent` 是 chat 域 append-only 事件总线
- `chat_message_projections` 是 read model
- `tool_calls.turn_id` 已接入新写入链路

### 6.2 归一化后的 ChatSession 状态机

```text
              ┌────────┐
              │  New   │  (刚创建,无消息)
              └───┬────┘
                  │ first user message
                  ▼
              ┌────────┐
       ┌─────│ Active │──────┐
       │      └───┬────┘      │
       │          │ idle > N  │ user archive
       │          ▼           │
       │      ┌────────┐      │
       │      │ Stale  │      │
       │      └───┬────┘      │
       │          │ new msg   │
       │          └───────────┤
       │                      │
       │                      ▼
       │                ┌──────────┐
       │                │ Archived │
       │                └──────────┘
       │
       │ bound to GoalRun
       ▼
   ┌──────────────┐
   │ GoalBound    │  (session_kind = goal_task_exec)
   └──────────────┘
```

### 6.3 归一化后的 ChatTurn 状态机(7 阶段)

```text
  ┌────────┐
  │  New   │  (request_id 落库)
  └────┬───┘
       │ first LLM token received
       ▼
  ┌────────────┐
  │ Streaming  │  (text/thinking 流式)
  └────┬───────┘
       │ tool_call emitted
       ▼
  ┌──────────────┐
  │ ToolCalling  │  (1+ tool_call 等待执行)
  └────┬─────────┘
       │ tool_call finished
       ▼
  ┌─────────────┐
  │ ToolResult  │  (回注 LLM 等待下一轮)
  └────┬────────┘
       │ (回到 Streaming) 或 LLM done
       ▼
  ┌────────────┐
  │ Concluding │  (生成 final reply)
  └────┬───────┘
       │ final message + projection written
       ▼
  ┌────────────┐
  │  Done      │  (terminal)
  └────────────┘
       
       ┌─────────────┐
       │ Cancelled   │  (user cancel)
       └─────────────┘
       
       ┌─────────────┐
       │  Failed     │  (LLM error / recover)
       └─────────────┘
```

### 6.4 ChatTurn 状态转换表

| From | To | Trigger | Guard | Side Effect |
|---|---|---|---|---|
| New | Streaming | first token from LLM | `llm_call_started` | emit `chat_turn.stage:streaming` |
| Streaming | ToolCalling | LLM emit tool_call | `tool_call_emitted` | create ToolCall, emit `tool.call_proposed` |
| ToolCalling | ToolResult | tool finished | `tool_result_received` | emit `tool.call_finished` |
| ToolResult | Streaming | LLM continue | (back-edge) | (no event) |
| Streaming | Concluding | LLM stop | `stop_reason=end_turn` | emit `chat_turn.stage:concluding` |
| Concluding | Done | projection + memory candidate written | `projection_created` | emit `chat_turn.stage:done` |
| * | Cancelled | user cancel | `user_cancel` | kill LLM, emit `chat_turn.stage:cancelled` |
| * | Failed | LLM error / unrecoverable | retry exhausted | emit `chat_turn.stage:failed` |

### 6.5 ChatMessageProjection 生命周期

- **写入**: ChatTurn 完成后,由 `send_v2` 镜像写入
- **字段**: `plain_text` 只来自 `text / plan / completion / blocked` 用户可见块;`thinking / tool_use / tool_result` 留在 `content_blocks_json`
- **读取**: 前端 `useChatSession` 优先读 projections,旧 `chat_messages` 兜底
- **更新**: projection 不可变(append-only),修订走 `update_message_projection`(写新版本号)

### 6.6 与 GoalCycle 的双向锚点(解除阻塞 B1)

```text
ChatTurn.goal_cycle_id (Option<String>)  ──  ChatTurn 由 GoalCycle 触发
ChatTurn.agent_task_id (Option<String>)  ──  ChatTurn 由 AgentTask 触发
ChatTurn.goal_id (Option<String>)        ──  ChatTurn 由 GoalRun 触发
```

> 这三个字段**当前部分缺失**,在 A1 (WorkKind 命名) + B3 (graph_hash) 之后由 migration 补齐。

---

## 7. 主线程 Agent 状态流转(3 条路径)

### 7.1 主线程的 3 条独立路径(从 [运行时可观察性与Agent链路收口-20260531.md](./运行时可观察性与Agent链路收口-20260531.md) §0 抽取)

1. **对话 LLM 路径**: `send_v2.rs` 驱动流式回复
2. **工具式 Agent 路径**: LLM 调 `agent.start` → `agent_runs` 启动 `claude -p`
3. **OODA 决策路径**: `goal_orchestrator` 跑 Reason/Explore cycle

归一化后,3 条路径**共享一个主线程入口**和**一个 Resolver**。

### 7.2 主线程入口(`MainLoop`)

```text
  ┌──────────────────────────────────────────────────────────────┐
  │ MainLoop (新, crates/conductor-core/src/main_loop.rs)        │
  │                                                               │
  │  接收:                                                       │
  │    - user message (from ChatSession)                          │
  │    - tool_call result (from ToolExecutor)                     │
  │    - subagent event (from SubagentHandle)                     │
  │    - ooda trigger (from GoalOrchestrator)                     │
  │    - mcp event (from McpRouterSession)                        │
  │                                                               │
  │  调度:                                                        │
  │    - resolve_presence() → 决定是否继续                       │
  │    - resolve_model(CallerContext::ChatMainLoop) → ResolvedModel │
  │    - decide_what_to_do() → RoutingDecision                    │
  │    - dispatch() → 调 LLM / 调 Tool / 触发 OODA cycle         │
  │                                                               │
  │  收尾:                                                        │
  │    - 写 ChatTurnEvent                                          │
  │    - 触发 memory_candidate 评估                                │
  │    - 触发 ChatMessageProjection                                 │
  └──────────────────────────────────────────────────────────────┘
```

### 7.3 主线程 LLM 调用状态机

```text
       ┌──────────┐
       │  Idle    │  (无 user input, 不主动对话)
       └────┬─────┘
            │ user input OR ooda trigger OR subagent event
            ▼
       ┌──────────┐
       │  Resolve │  (model_resolver::resolve)
       └────┬─────┘
            │ ResolvedModel
            ▼
       ┌──────────┐
       │  CallLLM │  (llm.rs stream)
       └────┬─────┘
            │ first token
            ▼
       ┌──────────────┐
       │  Streaming   │
       └────┬─────────┘
            │ tool_call OR stop
            ├───────────────┐
            ▼               ▼
       ┌──────────┐    ┌──────────┐
       │  WaitTool│    │  Done    │
       └────┬─────┘    └──────────┘
            │ tool result
            └──────→ CallLLM (back-edge)
```

### 7.4 主线程工具调用状态机(沿用 [STATE_MODEL_AGENT.md §6](./STATE_MODEL_AGENT.md) 扩展)

```text
              ┌───────────┐
              │  Created  │  (LLM emit tool_call)
              └─────┬─────┘
                    │
              ┌─────▼─────┐
              │ Classified│  (risk_level 分类)
              └─────┬─────┘
                    │
            ┌───────┼───────┐
            ▼       ▼       ▼
     ┌──────────┐┌──────┐┌──────────┐
     │Executing ││Perm  ││Rejected  │  (read_only 直接执行,
     └─────┬────┘│Req   │└──────────┘   workspace_write 需审批)
           │     │      │
           │     ▼      │
           │  ┌──────────┐
           │  │Awaiting  │
           │  │Approval  │
           │  └────┬─────┘
           │       │ approved
           │       └──────→ Executing
           ▼
   ┌──────────────────────┐
   │ Succeeded/Failed/    │
   │ TimedOut/ApprovalReq │  (terminal, 必须挂 turn_id)
   └──────────────────────┘
```

### 7.5 主线程 OODA 决策状态机(归一化)

```text
       ┌──────────┐
       │  Idle    │  (无 Goal 触发)
       └────┬─────┘
            │ goal created OR cycle trigger
            ▼
       ┌──────────┐
       │ Observe  │  (compute graph_hash)
       └────┬─────┘
            │ graph_hash changed OR force_replan
            ▼
       ┌──────────┐
       │ Orient   │  (orient.rs: blockers, agent_fit, tension)
       └────┬─────┘
            │ OrientReport
            ▼
       ┌──────────┐
       │ Decide   │  (decide_llm + ModelResolver → DispatchPlan)
       └────┬─────┘
            │ plan.approved
            ▼
       ┌──────────┐
       │ Act      │  (dispatch tasks: explore/bootstrap/reason)
       └────┬─────┘
            │ tasks spawned
            ▼
       ┌──────────┐
       │ Review   │  (collect results, check goal)
       └────┬─────┘
            │ goal met OR rework needed
            ├────────────────┐
            ▼                ▼
       ┌──────────┐    ┌──────────┐
       │ Advance  │    │  Replan  │  (back to Observe)
       └────┬─────┘    └──────────┘
            │
            ▼
       ┌──────────┐
       │  Done    │
       └──────────┘
```

**关键点**: Decide 阶段必须经过 ModelResolver(P3 落地的核心),LLM 选模型由 RoutingPolicy 决定。

### 7.6 主线程状态转换表(汇总)

| 入口 | 来源 | 调 Resolver 时机 | 状态机归属 |
|---|---|---|---|
| User input | ChatSession.send_message | CallLLM 之前 | 主线程 LLM |
| LLM emit tool_call | CallLLM streaming | N/A (ToolSpec 已含) | 主线程 Tool |
| OODA trigger | GoalOrchestrator tick | Decide 阶段 (decide_llm) | 主线程 OODA |
| Subagent event | SubagentHandle notify | 仅当需要回注 LLM | 主线程 LLM (back) |
| Mcp event | McpRouterSession notify | 同上 | 主线程 LLM (back) |

---

## 8. 子线程/AgentTeam 状态流转(归一化后**所有子线程统一为 1 个抽象**)

### 8.1 关键归一化: 三种"子线程"统一为 SubagentHandle

依据 [运行时可观察性与Agent链路收口-20260531.md](./运行时可观察性与Agent链路收口-20260531.md),目前项目有 3 种"子线程"执行方式:

1. **`claude -p` 子进程**(`agent.start` 工具)
2. **`subagent.claude_p` 一次性 CLI**(`sub run` 或工具)
3. **MCP 调用** (Plan 2 抽离后)

加上 AgentTeam 的多 Agent 协调,共 4 种。**归一化为 2 个一等对象**:

- **SubagentHandle** (单 Agent 子线程)
  - 实现方式: `CliSubprocess` (claude/codex) / `McpRouter` (调用 model-router-mcp) / `Direct` (内部 fn)
- **AgentTeam** (多 Agent 协调容器,内含 N 个 SubagentHandle)

> 归一化原则: 不管最终怎么调度,都是"一个子线程"。**调度方式(CLI/MCP/内联) 是 SubagentHandle 的 `transport_kind` 字段**。

### 8.2 SubagentHandle 状态机(单子线程)

```text
  ┌─────────┐
  │ Created │  (start_subagent 调用)
  └────┬────┘
       │ resolve_model + spawn
       ▼
  ┌─────────┐
  │ Spawning│  (创建进程/连接 MCP)
  └────┬────┘
       │ pid received / mcp ready
       ▼
  ┌─────────────┐
  │  Running    │  (流式 stdout / MCP event)
  └────┬────────┘
       │ done / error / timeout / killed
       ├──────────────┬──────────────┐
       ▼              ▼              ▼
  ┌────────┐    ┌────────┐    ┌────────┐
  │ Done   │    │ Failed │    │Killed  │
  └────────┘    └────────┘    └────────┘
```

### 8.3 SubagentHandle 状态转换表

| From | To | Trigger | Guard | Side Effect |
|---|---|---|---|---|
| Created | Spawning | `subagent.start` | 资源可用 | 创建 AgentRun / McpRouterSession |
| Spawning | Running | pid / mcp ready | process 启动成功 | emit `agent_run.running` |
| Running | Done | exit 0 / mcp done | 正常返回 | 写 output_ref, emit `agent_run.succeeded` |
| Running | Failed | exit != 0 / mcp error | 错误不可恢复 | 写 error, emit `agent_run.failed` |
| Running | Killed | user stop / timeout | 用户或系统 | kill pid / close mcp, emit `agent_run.killed` |
| * | Stopped | `subagent.stop` | (any state) | 强杀 |
| Created/Spawning | Cancelled | 启动前取消 | 资源不可用 | 无副作用 |

### 8.4 McpRouterSession 状态机(独立于 SubagentHandle)

```text
  ┌───────────┐
  │ Connecting│  (stdio 子进程启动中)
  └─────┬─────┘
        │ initialize handshake ok
        ▼
  ┌───────────┐
  │   Ready   │  (可调 tools/list, tools/call)
  └─────┬─────┘
        │ tools/call invoked
        ▼
  ┌───────────┐
  │  Calling  │  (等待 MCP response)
  └─────┬─────┘
        │ response / timeout / error
        ├──────────────┐
        ▼              ▼
  ┌──────────┐   ┌──────────┐
  │  Ready   │   │ Stalled  │  (reconnect)
  └──────────┘   └────┬─────┘
                       │ reconnected
                       └─────→ Ready
       
       ┌──────────┐
       │  Closed  │  (用户/系统关闭)
       └──────────┘
```

### 8.5 AgentTeam 状态机(多 Agent 协调)

```text
  ┌────────┐
  │  Draft │
  └────┬───┘
       │ create_team
       ▼
  ┌────────┐
  │ Planning│  (生成 plan + 分配 members)
  └────┬───┘
       │ plan submitted
       ▼
  ┌──────────────────┐
  │ AwaitingPlanApvl │
  └────┬─────────────┘
       │ positive response
       ▼
  ┌──────────┐
  │ Executing│  (N 个 members 跑 N 个 SubagentHandle)
  └────┬─────┘
       │ all members done
       ▼
  ┌────────────────┐
  │ AwaitingReview │
  └────┬───────────┘
       │ verdict
       ├──────────┐
       ▼          ▼
  ┌────────┐  ┌────────────┐
  │Accepted│  │ReworkReq   │
  └────┬───┘  └────┬───────┘
       │            │ retry
       │            └──────→ Planning
       ▼
  ┌────────┐
  │Archived│
  └────────┘
```

### 8.6 AgentTeam 状态转换表(沿用 [STATE_MODEL_AGENT.md §7](./STATE_MODEL_AGENT.md) 扩展)

| From | To | Trigger | Side Effect |
|---|---|---|---|
| Draft | Planning | `create_team` | 初始化 members |
| Planning | AwaitingPlanApproval | plan 提交 | 通知 reviewer |
| AwaitingPlanApproval | Executing | 批准 | 启动 N 个 SubagentHandle |
| AwaitingPlanApproval | ReworkRequired | 拒绝 | 记录拒绝原因 |
| Executing | AwaitingReview | 全部 member 完成 | 触发 reviewer |
| AwaitingReview | Accepted | verdict=Accepted | 提交结果 |
| AwaitingReview | ReworkRequired | verdict=Failed | 进入 rework |
| ReworkRequired | Planning | `rework` 触发 | 重新规划 |
| Accepted | Archived | 7 天后自动 | 归档 |

**write scope 规则**(沿用):
- Executing 阶段检查 `member.write_scope` 重叠 → 串行执行
- 涉及状态机修改 → 全部 implementation agent 暂停,通知 Reviewer

### 8.7 AgentTeamMember 状态机

```text
  ┌────────┐
  │  Idle  │
  └────┬───┘
       │ team Executing + assigned
       ▼
  ┌──────────┐
  │ Assigned │
  └────┬─────┘
       │ SubagentHandle Spawning
       ▼
  ┌──────────┐
  │ Running  │  (持有 SubagentHandle ref)
  └────┬─────┘
       │ SubagentHandle Done/Failed
       ▼
  ┌──────────┐
  │ Reporting│  (写 result_ref)
  └────┬─────┘
       │ team AwaitingReview
       ▼
  ┌──────────┐
  │ Completed│
  └──────────┘
       
       * ─→ Paused (人停) / Stopped (kill)
```

### 8.8 子线程与主线程的耦合

```text
MainLoop
  │
  ├─→ resolve_model(CallerContext::Subagent) → ResolvedModel
  │   │
  │   └─→ SubagentHandle.spawn(transport_kind, resolved)
  │       │
  │       ├─→ CliSubprocess: spawn claude/codex with --model
  │       ├─→ McpRouter:    invoke model.invoke on mcp session
  │       └─→ Direct:       call internal fn
  │
  └─→ on_subagent_event → MainLoop 收 event
       │
       ├─→ ToolCalling: 调主线程 Tool 状态机
       ├─→ ToolResult:  回注主线程 LLM
       └─→ Done:        写 AgentTask.result_ref
```

---

## 9. 对象生命周期总表(20 个一等对象)

> 仅列一等对象,衍生对象 (如 ActiveRun 内存态) 不列入。

| 对象 | 生命周期起点 | 状态 | 终止条件 | 持久化 | 主线程/子线程 |
|---|---|---|---|---|---|
| ChatSession | first user message | active → stale → archived | user archive / workspace 关闭 | ✅ | 主 |
| ChatTurn | user request | new → streaming → ... → done | LLM done + projection 写完 | ✅ | 主 |
| ChatMessage | user/assistant speak | 不可变 | (append-only) | ✅ | 主 |
| ChatMessageProjection | ChatTurn 收尾 | visible | (immutable, 仅 revision) | ✅ | 主(投影) |
| ToolCall | LLM emit tool_call | proposed → ... → recorded | recorded | ✅ | 主(可被 sub 调用) |
| ToolSpec | compile time | 静态 | (never changes) | ✅ in code | - |
| MemoryEntry | new | candidate → active → ... → forgotten | forgotten | ✅ | 主+sub |
| MemoryChunk | memory_entry.chunk() | active | (with entry) | ✅ | - |
| MemoryEmbedding | new | indexed | index refresh | ✅ | - |
| MemoryCandidate | ChatTurn 收尾 | pending → promoted / quarantined | promoted or quarantined | ✅ | 主(衍生) |
| RecallDoc | new | indexed | stale / deleted | ✅ | - |
| GoalRun | user create goal | draft → planning → running → succeeded/failed | terminal | ✅ | 主(OODA) |
| GoalCycle | OODA tick | observe → orient → decide → act → review | cycle complete | ✅ | 主(OODA) |
| AgentTask | Decide phase | open → claimed → running → completed/failed | terminal | ✅ | 主+sub |
| AgentRun | SubagentHandle spawn | queued → running → terminal | terminal | ✅ | sub |
| SubagentHandle | start_subagent | created → spawning → running → done | terminal | 内存(关联 AgentRun) | sub |
| McpRouterSession | first MCP call | connecting → ready → calling → closed | closed | 内存 | sub(进程外) |
| AgentTeam | create_team | draft → ... → archived | archived | ✅ | sub(协调) |
| AgentTeamMember | team Executing | idle → ... → completed | terminal | ✅ | sub(成员) |
| LlmProfile | user create profile | enabled/disabled | user delete | ✅ | - |
| RoutingPolicy | user create policy | enabled/disabled | user delete | ✅ | - |
| ResolvedModel | resolve() call | value object | 调用结束 | 内存 | - |
| PermissionGrant | tool need approval | requested → ... → used/expired/revoked | terminal | ✅ | 主(可被 sub 引用) |
| Proposal | LLM suggest high-risk action | pending → ... → succeeded/failed | terminal | ✅ | 主 |
| Workspace | user create workspace | trusted/untrusted | user delete | ✅ | - |
| WorkspaceScope | AgentRun / PermissionGrant 引用 | value object | (with owner) | ✅ | - |
| goal_hint | user inject | unread → read | read + use | ✅ | 主 |
| AuditEvent | 任何 state change | append-only | never | ✅ | 全部 |
| ChatTurnEvent | ChatTurn 内事件 | append-only | never | ✅ | 主(可被子引用) |

**统计**: 28 个一等对象,其中 1 个静态, 25 个有完整生命周期, 2 个 append-only 事件。

---

## 10. 阻塞点解除路径

### 10.1 阻塞 B1 解除: ChatTurn ↔ GoalCycle 双向锚点

**当前**: `chat_turns` 缺 `goal_cycle_id` / `agent_task_id` / `goal_id`

**解除步骤**:
1. A1 (WorkKind 命名) 完成后,写 migration `000X_chat_turn_anchors.sql`
2. 加字段:
   ```sql
   ALTER TABLE chat_turns ADD COLUMN goal_id TEXT;
   ALTER TABLE chat_turns ADD COLUMN goal_cycle_id TEXT;
   ALTER TABLE chat_turns ADD COLUMN agent_task_id TEXT;
   CREATE INDEX idx_chat_turns_goal ON chat_turns(goal_id, created_at);
   ```
3. `send_v2` 在 OODA 触发的 turn 创建时填字段
4. 反向: GoalCycle 收尾时回查最新 chat_turn(经 goal_cycle_id)写入 review_summary_ref

**工作量**: 0.5d。**落地**: A1 完成后立即。

### 10.2 阻塞 B2 解除: McpRouter 后端 Executor

**当前**: Plan 2 抽离后,conductor 端没有"调 model-router-mcp"的实现

**解除步骤**:
1. D2 (model-router-core) + D3 (MCP server loop) 完成后
2. 新建 `crates/conductor-core/src/agents/dispatcher/executors/mcp.rs`
3. 复用 `mcp.rs` 的 `McpClient`(已存在)
4. 实现 `SubagentHandle.spawn(transport_kind=McpRouter)`
5. 写入 `SubagentHandle.transport_kind` 字段

**工作量**: 1.5d(已包含在 D4)。**落地**: D4。

### 10.3 阻塞 B3 解除: CallerContext 表达

**当前**: 5 个调用点各用不同入参,无法统一

**解除步骤**:
1. A4 中定义 `CallerContext` 枚举
2. 每个调用点改写为 `resolve(ctx, hint).await?`
3. 单测覆盖 5 个分支

**工作量**: A4 内部。**落地**: A4。

### 10.4 阻塞 B4 解除: LlmProfile.provider 字段解耦

**当前**: `provider: String` 硬编码 `openai|anthropic|local`

**解除步骤**:
1. A2 中加 `transport: TransportKind`
2. `provider` 字段保留,语义改为"API 协议族"
3. 新增允许值: `claude_cli` / `codex_cli` / `mcp_router`
4. migration 兼容旧值

**工作量**: 1d。**落地**: A2。

### 10.5 阻塞 B5 解除: 中文 Reason Prompt

**当前**: 无

**解除步骤**:
1. C2 (LLM Reason) 阶段
2. 设计中文 prompt(参考 [cairn/dispatcher/prompts/default/reason.md](../tools/Cairn/cairn/src/cairn/dispatcher/prompts/default/reason.md) 但用中文)
3. 字段:`{goal, graph_yaml, open_intents, hints, ooda_phase, work_kind, hint_text}`
4. 输出契约(走 P10 JSON Schema): `{complete | intents | noop}`

**工作量**: 包含在 C2。**落地**: C2。

### 10.6 P0 修复: Subagent 结果回写与感知层投影(必须先于 Phase A)

> 范围: §3.2 列出的 4 个 P0 缺陷。
> 触发点: 修 [agent_runs.rs:261-409](../crates/conductor-core/src/agent_runs.rs#L261-L409) `finish_spawned_run` 函数 + [send_v2.rs](../crates/conductor-core/src/chat/send_v2.rs) 加新 API + [agent_teams.rs:488-499](../crates/conductor-core/src/agent_teams.rs#L488-L499) `set_member_status` 接线。
> 落点: Round 5 第一周,作为 Phase A 之前的"Phase A0"。

#### Fix 1 (P0-1) — 反写 `goal_tasks.result_ref`

**目标**: `finish_spawned_run` 完成时, 把 `agent_runs.output_ref` 真正写到 `goal_tasks.result_ref`, 而不是写 `tasklist` 表的文本摘要。

**改动点**:
- 新增 `goal_tasks::set_task_result(task_id, output_ref: &str, summary: Option<&str>)` API
- `goal_tasks` 表加 `result_ref: Option<String>` 字段 + migration `000X_goal_tasks_result_ref.sql`
- `agent_runs.rs:301-325` 改为: 优先查 `goal_tasks` 表, 写 `result_ref` + `status = completed/pending`, **tasklist 旧表保留作为 fallback**

**关键伪代码**:
```rust
// 替换 agent_runs.rs:301-325 的 tasklist::update_task_status_by_id 调用
if let Some(task_id) = run.metadata_json...get("task_id")? {
    let output_ref = run.output_ref.clone()
        .unwrap_or_else(|| format!("runs/{}-output.json", run.id));
    let summary = run.error.clone()
        .or_else(|| Some(format!("Run {} done: {}", run.id, run.status.as_str())));

    // ① 主路径: goal_tasks
    let _ = crate::goal_tasks::set_task_result(
        task_id, &output_ref, summary.as_deref()
    ).await;

    // ② 后向兼容: tasklist 旧表 (保留)
    let _ = crate::tasklist::update_task_status_by_id(
        task_id, new_status, summary.as_deref()
    ).await;
}
```

**工作量**: 0.5d(含 migration + 单测)。**优先级**: **P0 第一个**。

#### Fix 2 (P0-2) — 触发主线程 LLM 续轮回注

**目标**: subagent 完成后, 把它的 `output_ref` 作为 `tool_result` 注入回当前 turn, 让 LLM 主动总结并回复用户。

**改动点**:
- `chat::send_v2` 加新方法 `continue_turn_with_subagent_result(request_id, agent_run_id, output_payload)`
- `agent_runs.rs:362` `notify_turn_of_run_completion` 之后调 `trigger_main_thread_reinjection(&run)`
- 复用现有 `tool_calls.agent_run_id` 反查 `chat_turns.request_id` 的 SQL(在 `notify_turn_of_run_completion` 中已有)

**关键伪代码**:
```rust
// agent_runs.rs:362 之后
let _ = trigger_main_thread_reinjection(&run).await;

async fn trigger_main_thread_reinjection(run: &AgentRun) -> Result<()> {
    let pool = db::pool().await?;
    let row = sqlx::query(
        r#"SELECT tc.turn_id, ct.request_id
           FROM tool_calls tc JOIN chat_turns ct ON ct.id = tc.turn_id
           WHERE tc.agent_run_id = ?1 LIMIT 1"#
    ).bind(&run.id).fetch_optional(&pool).await?;

    let Some(row) = row else { return Ok(()); };
    let request_id: String = row.try_get("request_id")?;

    // 读 output_ref 内容
    let output_payload = read_output_payload(&run).await?;

    // 调 send_v2 新 API: 注入 tool_result + 触发 LLM 续轮
    crate::chat::send_v2::continue_turn_with_subagent_result(
        &request_id, &run.id, &output_payload,
    ).await?;
    Ok(())
}
```

**send_v2 新 API 签名**:
```rust
// crates/conductor-core/src/chat/send_v2.rs
pub async fn continue_turn_with_subagent_result(
    app: &tauri::AppHandle,
    request_id: &str,
    agent_run_id: &str,
    output_payload: &serde_json::Value,
) -> Result<()> {
    // 1. 写 chat_turn_events 注入 tool_result block
    append_turn_event_by_request(request_id, "tool.subagent_result", ...).await?;
    // 2. 触发 LLM 续轮 (走 send_v2 的 streaming 路径)
    continue_existing_turn(app, request_id, ToolResultInjection { ... }).await?;
    // 3. emit "thinking-update" Tauri event
    Ok(())
}
```

**关键设计**:
- `continue_existing_turn` 是 `send_v2` 现有 main loop 的复用,**不是新路径**
- LLM 看到的是正常 `tool_result` block, 不会意识到这是 subagent 结果
- `UserPresence::Dnd/Asleep` 时, **跳过** LLM 续轮, 仅保留 `tool_result` 块(让 LLM 在用户回到 Active 后能继续)

**工作量**: 1-1.5d。**优先级**: **P0 第二个**。

#### Fix 3 (P0-3) — Member 状态机接线

**目标**: AgentRun 完成后, 自动更新对应 Member 状态 + 触发 AgentTeam lifecycle 推进。

**改动点**:
- `agent_runs.rs:301` task_id 分支里加 member_id 反查
- 调 `agent_teams::set_member_status(member_id, new_status)`
- 检查 team 全员完成, 触发 `transition_team_lifecycle(team_id, AwaitingReview)`

**关键伪代码**:
```rust
// 替换 agent_runs.rs:301 整段
if let Some(task_id) = ... {
    // ... Fix 1 的 goal_tasks::set_task_result ...

    // ── Member 状态机接线 ──
    if let Some(member_id) = find_member_id_by_task_id(task_id).await? {
        let new_status = match run.status {
            AgentRunStatus::Succeeded => AgentMemberStatus::Active,  // 进入 Reporting 子态
            AgentRunStatus::Failed | AgentRunStatus::Stopped => AgentMemberStatus::Stopped,
            _ => return Ok(()),
        };
        agent_teams::set_member_status(&member_id, new_status).await?;
    }

    // ── Team 自动推进 ──
    if let Some(team_id) = find_team_id_by_task_id(task_id).await? {
        if team_all_members_done(&team_id).await? {
            agent_teams::transition_team_lifecycle(
                &team_id,
                AgentTeamLifecycle::AwaitingReview,
            ).await?;
        }
    }
}
```

**新增工具函数**:
```rust
// crates/conductor-core/src/agent_teams.rs
pub async fn find_member_id_by_task_id(task_id: &str) -> Result<Option<String>>;
pub async fn find_team_id_by_task_id(task_id: &str) -> Result<Option<String>>;
pub async fn team_all_members_done(team_id: &str) -> Result<bool>;
```

**工作量**: 1d。**优先级**: **P0 第三个**。

#### Fix 4 (P0-4) — Tauri `agent_runs_changed` 事件在 finish 时发

**目标**: AgentRun 自然完成时, 前端 `AgentLanes` / `GoalConsole` / `TaskDrawerPane` 自动 refresh。

**改动点**:
- `agent_runs` 模块需要持有 `AppHandle`(注入而不是全局拿)
- `finish_spawned_run` 完成时 `app_handle.emit("agent_runs_changed", payload)`
- `events.rs` 加 `emit_agent_run_finished` 写 audit_event 镜像

**关键伪代码**:
```rust
// agent_runs.rs:298-299 之后
upsert(&run).await?;

// ── Fix 4: Tauri 事件发出 ──
if let Some(app) = get_app_handle() {
    let _ = app.emit("agent_runs_changed", serde_json::json!({
        "run_id": run.id,
        "agent_id": run.agent_id,
        "status": run.status.as_str(),
        "output_ref": run.output_ref,
        "error": run.error,
    }));
}

// audit_event 镜像 (在 events.rs 加)
crate::events::emit_agent_run_finished(&run).await;
```

**新增 `events.rs` 函数**:
```rust
pub async fn emit_agent_run_finished(run: &AgentRun) {
    let event = AuditEvent {
        timestamp: Utc::now(),
        source: "agent_runs".to_string(),
        event_type: "agent_run.finished".to_string(),
        actor: run.agent_id.clone(),
        target: run.id.clone(),
        detail: json!({
            "run_id": run.id,
            "agent_id": run.agent_id,
            "status": run.status.as_str(),
            "output_ref": run.output_ref,
            "error": run.error,
        }),
        ..Default::default()
    };
    append_event(&event).await;
}
```

**AppHandle 注入**:
- `start_claude_run(input, app: AppHandle)` — 在 commands.rs 调用方传入
- `finish_spawned_run` 内部用 `OnceCell<AppHandle>` 或 channel 拿到(单进程)

**工作量**: 0.5-1d。**优先级**: **P0 第四个**。

#### P0 修复的依赖与执行顺序

```text
Fix 1 (goal_tasks.result_ref)   ←  最先 (migration + 简单 SQL)
   │
   ├─→ Fix 3 (Member 状态机)    ←  依赖 Fix 1 (task_id 反查才能找到 member)
   │
   └─→ Fix 2 (LLM 续轮)         ←  依赖 Fix 1 (output_ref 是注入的来源)
            │
            └─→ Fix 4 (Tauri 事件) ←  依赖 Fix 2 (LLM 续轮会触发新一轮 turn_event)
```

**总计**: 4 个 fix,3-4 人天,Round 5 第一周完成,作为 Phase A 起点。

---

## 11. 最终细粒度融合方案

### 11.1 30 项任务图(沿用 [HybridModel与Cairn融合方案-20260604.md](./HybridModel与Cairn融合方案-20260604.md) §3,带本文件补充 + Phase A0)

| Phase | ID | 任务 | 阻塞解除 | 难度 | 工作量 |
|---|---|---|---|---|---|
| **A0** | **A0-1** | **Fix 1**: `goal_tasks.result_ref` 反写 + migration | P0-1 | 中 | 0.5d |
| | **A0-2** | **Fix 2**: `send_v2::continue_turn_with_subagent_result` 新 API + 注入主线程 LLM | P0-2 | 高 | 1-1.5d |
| | **A0-3** | **Fix 3**: `set_member_status` 接线 + team 全员完成检查 | P0-3 | 中 | 1d |
| | **A0-4** | **Fix 4**: Tauri `agent_runs_changed` 在 finish 时发 + AppHandle 注入 | P0-4 | 中 | 0.5-1d |
| **A** | A1 | WorkKind 重命名 + OodaPhase 新建 | B1 | 中 | 0.5d |
| | A2 | LlmProfile.transport + migration | B4 | 中 | 1d |
| | A3 | RoutingPolicy.caller_phase + migration | - | 低 | 0.5d |
| | A4 | ModelResolver + CallerContext + resolve() | B3 | 中 | 1.5d |
| **B** | B1 | send_v2 / summarizer / smart_monitor 走 resolver | - | 中 | 2d |
| | B2 | agent_runs 注入 --model (或 McpRouter 替代) | - | 中 | 1.5d |
| | B3 | ReasonCheckpoint (graph_hash) | - | 低 | 0.5d |
| | B4 | result_ref 反写 (含 ChatTurn.goal_cycle_id 锚点) | B1 | 低 | 1d |
| | B5 | goal_hints 表 + UI 注入 | - | 低 | 1d |
| **C** | C1 | 观测总线统一 (model.routed + ooda.phase_changed) | - | 低 | 1d |
| | C2 | LLM Reason 任务接 Resolver | B5 | 高 | 3-4d |
| | C3 | OodaPhase Driver 抽象 (Bootstrap/Reason/Explore) | - | 中 | 2d |
| | C4 | graph snapshot API | - | 低 | 1d |
| | D1 | JSON Schema 契约 (reason/explore 输出) | - | 低 | 1d |
| **D** | D2 | model-router-core 抽离 (Plan 2 M1+M2) | B2, B4 | 高 | 2d |
| | D3 | model-router-mcp server loop (Plan 2 M3+M4) | - | 高 | 2d |
| | D4 | McpRouter executor (Cairn 外部子代理) | B2 | 中 | 1.5d |
| **E** | E1 | Settings → Models 标签页 | - | 中 | 2-3d |
| | E2 | Goal Console → Reasoning tab | - | 中 | 1-2d |
| | E3 | GraphView 组件 (Cairn P11) | - | 中 | 3-5d |
| **F** | F1 | 任务 claim 协议 | - | 中 | 1.5d |
| | F2 | Worker 健康熔断 | - | 中 | 1.5d |
| | F3 | Graph Tension 字段 | - | 中 | 1.5d |
| | F4 | Worker 声明式注册 (与 E1 共享) | - | 中 | 2d |
| | F5 | 密钥 keyring 化 | - | 中 | 1d |
| | F6 | Claude/Codex MCP 接入文档 | - | 低 | 1d |

**总计**: 30 项任务(原 26 项 + 4 项 P0),约 35-45 人天。

**Phase A0 的硬约束**:
- 必须在 Phase A1 之前完成,**不可推迟**
- A0-1 (Fix 1) 单独可发版(只动 goal_tasks 表 + agent_runs.rs 一个函数)
- A0-2/A0-3/A0-4 必须一起发版(都动 `finish_spawned_run` 同一段)
- Phase A0 完成时, 全部 TC-P0-* (见 §12.5) 必须通过

### 11.2 依赖图(最终)

```text
A0-1 (Fix 1: result_ref) ──┬─→ A0-3 (Fix 3: Member 状态机)
                            │
                            └─→ A0-2 (Fix 2: LLM 续轮) ──→ A0-4 (Fix 4: Tauri 事件)
                                      │
                                      ▼
                              A1 ──→ A2 ──→ A3 ──→ A4 ──→ B1 ──→ C1 ──→ C2 ──→ D2 ──→ D3 ──→ D4
                                       │              │      │             │       │       │
                                       │              │      ├─→ B2 ───────┘       │       │
                                       │              │      └─→ B3 ──→ B4          │       │
                                       │              │             │                │       │
                                       │              │             └─→ C4 ─────────┘       │
                                       │              │                                      │
                                       │              └─→ B5                                 │
                                       │                                                     │
                                       └─→ F1 F2 F3 F4 (与主线并行)                          │
                                                                                             │
                                                                    E1 ──→ E2 ──→ E3 (UI 串行)     │
                                                                                            │
                                                                                F5 F6 (D3 之后) ←──┘
```

**关键路径**:
1. **A0 关键路径**: A0-1 → A0-2 → A0-4 (3d, 串行)
2. **主关键路径**: A1 → A2 → A3 → A4 → B1 → C1 → C2 → D2 → D3 → D4 (5 周)
3. **合并关键路径**: A0 完成 → A1 启动 → ... → D4 (5 周 + 3 天)

**硬约束**:
- A0 必须在 A1 之前完成(否则 OODA review 永远等不到 accepted)
- A0-2 之后才能进 B1(否则 send_v2 走 resolver 时遇到 LLM 续轮的递归)

### 11.3 落地轮次(最终)

| 轮次 | 内容 | 周期 | 完成时 LC |
|---|---|---|---|
| **Round 5.0** | **Phase A0 全部 4 项 (P0 修复)** | **3-4 天** | **LC-07.5(链路闭合)** |
| **Round 5** | Phase A 全部 + Phase B 全部 + C1 + C4 + D1 | 1.5 周 | LC-07.5 → LC-09 |
| **Round 6** | C2 + C3 + Phase D 全部 | 2 周 | LC-09 → LC-10(初) |
| **Round 7** | Phase E 全部 | 2 周 | LC-10(UI 完整) |
| **Round 8** | Phase F + 评估 Noesis | 2-4 周 | LC-11(范式外延) |

**Round 5.0 与 Round 5 的关系**:
- Round 5.0 **必须**作为独立小版本发布,**不允许**与 Round 5 混发
- 发布前必须通过全部 TC-P0-* 测试 + GoalConsole 端到端冒烟
- 走 **fix 标签** 而非 **feature 标签**(因为是 bug fix,不是新功能)
- Round 5.0 发版后, [Bug说明-Goal会话双Working与任务结果插位-20260601.md](./Bug说明-Goal会话双Working与任务结果插位-20260601.md) 中的"任务结果插位"问题**应当**自动消失

**回归测试最小集**:
- TC-P0-01 至 TC-P0-06 (见 §12.5) 全部通过
- 现有 TestRoot 测试全绿(conductor-core 全部 unit/integration)
- 手工冒烟: 启动一个 Goal, 跑 2 个 AgentTask, 看到 LLM 主动总结结果

---

## 12. 测试矩阵(扩展)

### 12.1 用户状态机测试

| ID | 用例 | 期望 |
|---|---|---|
| TC-U-01 | focus detected → UserPresence=Active | emit `user.presence.changed` |
| TC-U-02 | Active + 60s 无输入 | → Idle |
| TC-U-03 | Idle + 240s 无输入 | → Away |
| TC-U-04 | Away + 1500s 无输入 | → Asleep |
| TC-U-05 | Asleep + focus detected | → Active (back) |
| TC-U-06 | user 声明"安静 30 分钟" | → Dnd,30min 后自动回 Active |
| TC-U-07 | Asleep 时 GoalCycle 触发 | 暂停 cycle,记录 paused 原因 |
| TC-U-08 | Dnd 时 LLM 主动对话 | 阻止,记录 blocked |

### 12.2 ChatTurn 状态机测试

| ID | 用例 | 期望 |
|---|---|---|
| TC-T-01 | send_message → first token | New → Streaming |
| TC-T-02 | LLM emit tool_call | Streaming → ToolCalling,创建 ToolCall 带 turn_id |
| TC-T-03 | tool_call finished | ToolCalling → ToolResult,emit `tool.call_finished` |
| TC-T-04 | LLM stop end_turn | Streaming → Concluding |
| TC-T-05 | projection + memory_candidate 写完 | Concluding → Done |
| TC-T-06 | user cancel mid-stream | → Cancelled,kill LLM |
| TC-T-07 | LLM error 3 次 | → Failed |
| TC-T-08 | goal_cycle 触发的 turn | goal_cycle_id 字段非空 |

### 12.3 SubagentHandle / McpRouterSession 测试

| ID | 用例 | 期望 |
|---|---|---|
| TC-S-01 | SubagentHandle.spawn(CliSubprocess, profile) | 创建 AgentRun, 命令行含 --model |
| TC-S-02 | SubagentHandle.spawn(McpRouter, profile) | 创建 McpRouterSession, 调 model.invoke |
| TC-S-03 | SubagentHandle exit 0 | → Done, output_ref 写 |
| TC-S-04 | SubagentHandle exit 1 | → Failed, error 写 |
| TC-S-05 | McpRouterSession initialize | → Ready |
| TC-S-06 | McpRouterSession tools/call 超时 | → Stalled, 自动 reconnect |
| TC-S-07 | AgentTeam Executing, 2 member write_scope 重叠 | 串行执行, 后者 wait |
| TC-S-08 | AgentTeam plan rejected | → ReworkRequired,reviewer 记录 |

### 12.4 数据流打通测试

| ID | 用例 | 期望 |
|---|---|---|
| TC-D-01 | user message → ChatTurn 创建 | chat_turns 行 + chat_turn_events stage:created |
| TC-D-02 | ChatTurn 收尾 → ChatMessageProjection | projection 行 + 内容含 final reply |
| TC-D-03 | ChatTurn 收尾 → MemoryCandidate | memory_candidates 行 (best-effort) |
| TC-D-04 | tool_call 完成 → chat_turn_events | `tool.call_finished` 事件 |
| TC-D-05 | resolve() 决策 → audit_events | `model.routed` 事件, payload 不含 api_key |
| TC-D-06 | OodaPhase 变化 → audit_events | `ooda.phase_changed` 事件 |
| TC-D-07 | goal_hint 注入 → 下次 Reason 读图 | observe.recent_hints 非空 |

### 12.5 P0 修复验证测试(Phase A0 验收)

| ID | 用例 | 期望 | 关联 Fix |
|---|---|---|---|
| **TC-P0-01** | 主线程 LLM emit `tool_call(agent.start)`, claude -p 跑完 | `goal_tasks.result_ref = "runs/{id}-output.json"`, `goal_tasks.status = "completed"` (不是旧 `tasklist` 表) | Fix 1 |
| **TC-P0-02** | AgentTeam 内 3 个 member 各跑 1 个 AgentRun, 都 succeeded | 全部 member 状态变 `Active`(Reporting 子态), team 自动进入 `AwaitingReview` | Fix 3 |
| **TC-P0-03** | subagent 完成后, 主线程 LLM 续轮 | LLM 主动生成总结消息, 含 subagent output_ref 引用, 用户看到的是 assistant message | Fix 2 |
| **TC-P0-04** | AgentRun 完成后, 前端 AgentLanes 自动 refresh | 收到 `agent_runs_changed` 事件, 状态从 `running` → `succeeded`, 不需要手动刷新 | Fix 4 |
| **TC-P0-05** | subagent 跑 30s timeout 失败 | `finish_spawned_run` 写 `error="claude timed out"`, emit `agent_runs_changed`, 前端看到 `failed` | Fix 1+3+4 |
| **TC-P0-06** | OODA cycle 看到所有 task 状态 `accepted` | Goal 切到 `awaiting_review`, 不再永远卡 `running` | Fix 1+3 |
| **TC-P0-07** | UserPresence=Dnd 时 subagent 跑完 | Fix 2 的 LLM 续轮被跳过, 仅写 `tool.subagent_result` 块, 等用户回 Active 时再触发 | Fix 2 |
| **TC-P0-08** | AgentRun 启动后用户立刻调 `stop_agent_run` | 既有 stop 路径不破坏, emit 仍然只发一次(由 commands.rs:265 控制) | Fix 4 兼容 |

**TC-P0-01 详细断言**:
```rust
let task = goal_tasks::get_task(&task_id).await?;
assert_eq!(task.status, "completed");
assert_eq!(task.result_ref.as_deref(), Some("runs/abc-output.json"));
assert!(task.error.is_none());

// 反向: 旧 tasklist 表行不应被改
let old_task = tasklist::get_task(&task_id).await?;
assert!(old_task.status == "completed" || old_task.status == "pending");
```

**TC-P0-03 详细断言**:
```rust
// subagent 跑完后, 等 5s
tokio::time::sleep(Duration::from_secs(5)).await;

// 应该有新一轮的 chat_turn_events (LLM 续轮)
let events = chat_turns::list_events_by_request(&request_id).await?;
let subagent_result = events.iter()
    .find(|e| e.event_type == "tool.subagent_result")
    .expect("subagent result event");

let llm_continued = events.iter()
    .find(|e| e.event_type == "chat_turn.stage" 
        && e.payload["phase"] == "streaming")
    .filter(|e| e.created_at > subagent_result.created_at)
    .expect("LLM continued turn after subagent result");

// 不依赖 user 输入
assert!(llm_continued.payload["triggered_by"] == "subagent_completion");
```

---

## 13. 风险与硬门禁

### 13.1 风险

| ID | 风险 | 概率 | 影响 | 缓解 |
|---|---|---|---|---|
| R1 | 重命名 WorkKind 引发全 crate 编译雪崩 | 高 | 中 | 一次性 PR + grep 校验 |
| R2 | SubagentHandle 抽象侵入现有 AgentRun | 中 | 高 | 保留 AgentRun 作为持久化镜像,SubagentHandle 作为运行时抽象 |
| R3 | Llm Reason 模型选错致决策质量差 | 中 | 高 | C2 加 A/B,默认 Claude,可降级 |
| R4 | MCP server 抽离回归 | 中 | 高 | D2 验收硬门禁:conductor-core 全测试绿 |
| R5 | chat_turns 加 goal_cycle_id 触发历史数据迁移失败 | 中 | 中 | 历史数据允许 goal_cycle_id 为空,新写入必填 |
| R6 | 观测总线统一破坏现有 runtime_events 消费方 | 低 | 中 | 保留 runtime_events 作为镜像,不删除 |
| **R7** | **Fix 2 (LLM 续轮) 在并发场景下重复触发** | **中** | **高** | 在 `tool_calls` 表加 `subagent_result_consumed: bool` 标志, 保证只触发一次 |
| **R8** | **Fix 4 AppHandle 注入破坏单进程模型,导致循环引用** | **低** | **高** | 用 `OnceCell<AppHandle>` 弱引用, 不持有所有权 |
| **R9** | **Fix 3 Member 状态机触发后, AgentTeam 提前进 Review, 用户还没看到结果** | **中** | **中** | Member 转 Reporting 后延迟 2s 再判断 `team_all_members_done`, 避免抖动 |
| **R10** | **P0 修复与后续 Phase A 命名冲突(都是 A0/A1)** | **低** | **低** | P0 显式用 A0- 前缀, 后续 A1+ 不变 |

### 13.2 硬门禁(5 个,新增 GB0)

| 门禁 | 触发点 | 失败动作 |
|---|---|---|
| **GB0** | **Phase A0 4 个 P0 fix 全部完成, TC-P0-01 至 TC-P0-08 全通过** | **不允许进入 Phase A** |
| **GB1** | conductor-core 全测试绿 | 每次 refactor 后,不允许 merge |
| **GB2** | ModelResolver fallback 3 分支单测全过 | A4 后,不允许进入 Phase B |
| **GB3** | Round 5 五项全过 (B3+B4+B5+C1+C4+D1) | Phase C 完成后,不允许进入 Phase D |
| **GB4** | MCP server 端到端 | D3 后,不允许进入 Phase E |
| **GB5** | LLM Reason 决策质量 A/B 对照 | C2 后,默认 Claude,人工审核通过 |

**GB0 是新增的"入场券"门禁,优先级最高**:
- Phase A0 任一 fix 失败 → 整个 Round 5 推迟
- TC-P0-08 (兼容 stop 路径) 失败 → 现有 stop_agent_run 行为破坏,必须 rollback
- Fix 1 的 migration 失败 → 必须保证 `goal_tasks.result_ref` 字段可空,旧数据无 impact

---

## 14. 决策记录(8 个 D)

| ID | 决策点 | 备选 | 建议 | 影响范围 |
|---|---|---|---|---|
| D1 | Plan 2 抽离: 复制(a) vs 下沉(b) | a/b | **b 共享下沉** | conductor-core 依赖图改 |
| D2 | TaskKind 命名 | 三套/两套/一套 | **两套 (WorkKind + OodaPhase)** | 命名清晰 |
| D3 | RoutingPolicy.caller_phase | 加/不加 | **加 (Option, None=任意)** | 按 phase 路由 |
| D4 | LlmProfile.transport | 加/不加 (用 provider 推断) | **加** | 显式区分 HTTP/CLI/MCP |
| D5 | Resolver 错误回退 | panic/静默 | **静默回退 config.llm** | Plan 1 R1 |
| D6 | MCP server 端先做 | 是/否 | **是** | 验证抽离边界 |
| D7 | LLM Reason 默认模型 | Claude (推荐) / 用户配置 | **Claude,可降级** | R4 风险控制 |
| D8 | 观测总线 | chat_turn_events / 单独 events | **chat_turn_events** | 复用现有漏斗 |

**新增 2 个决策**(本文件):

| ID | 决策点 | 备选 | 建议 |
|---|---|---|---|
| D9 | 用户状态机建模 | 显式 Presence / 隐式 (expression) | **显式 UserPresence 枚举** |
| D10 | 子线程统一抽象 | 3 种独立 / 1 个 SubagentHandle + transport_kind | **1 个 SubagentHandle, transport 区分** |

**P0 修复决策(本节新增 D11-D14)**:

| ID | 决策点 | 备选 | 建议 | 影响范围 |
|---|---|---|---|---|
| **D11** | P0 修复与 Phase A 顺序 | (a) 混发 Round 5 (b) 独立 Round 5.0 先发 | **b 独立 Round 5.0** | 避免回归风险传染 |
| **D12** | LLM 续轮触发位置 | (a) finish_spawned_run 同步触发 (b) chat_turn_events 监听器异步触发 | **a 同步触发, 配合 `subagent_result_consumed` 防重** | 简单可靠, 不引入新订阅模型 |
| **D13** | Member 状态机转换点 | (a) AgentRun 完成后立即 (b) AgentRun 完成后延迟 N 秒 | **a + 2s 抖动窗口** | 避免子线程 racing 误判 |
| **D14** | AppHandle 持有方式 | (a) 全局 OnceCell (b) 函数参数注入 | **a OnceCell, 弱引用** | 不破坏单进程模型, 不增加函数签名复杂度 |

---

## 15. 附录 A: 4 类状态机速查

### 15.1 User Presence 状态机速查

```
Offline → Active → Idle → Away → Asleep → (back to Active)
                                       ↘ Dnd (独立)
```

### 15.2 ChatTurn 状态机速查

```
New → Streaming → ToolCalling → ToolResult → Streaming (loop)
                                          ↘ Concluding → Done
                                          ↘ Cancelled / Failed (any state)
```

### 15.3 主线程 LLM 状态机速查

```
Idle → Resolve → CallLLM → Streaming → (WaitTool → ToolResult → CallLLM) | Done
```

### 15.4 主线程 OODA 状态机速查

```
Idle → Observe → Orient → Decide → Act → Review → Advance | Replan
                                            ↘ Done
```

### 15.5 SubagentHandle 状态机速查

```
Created → Spawning → Running → Done | Failed | Killed
                                   ↘ Stopped (any)
```

### 15.6 McpRouterSession 状态机速查

```
Connecting → Ready → Calling → Ready (loop) | Stalled → Ready (reconnect)
                                  ↘ Closed
```

### 15.7 AgentTeam 状态机速查

```
Draft → Planning → AwaitingPlanApvl → Executing → AwaitingReview
                                            ↕                ↕
                                        ReworkRequired  ReworkRequired
                                            ↓
                                       Planning (retry)
                                            ↓
                                       Accepted → Archived
```

### 15.8 AgentTeamMember 状态机速查

```
Idle → Assigned → Running → Reporting → Completed
                                      ↘ Paused / Stopped
```

---

## 16. 附录 B: 数据流速查(用户输入 → LLM 响应)

```text
User
  │ (1) 键入消息
  ▼
ChatPanel
  │ (2) invoke('send_message_v2', content, session_id)
  ▼
commands.rs::send_message_v2
  │ (3) 创建 ChatTurn (L0: chat_turns)
  │ (4) emit chat_turn.stage:created (L1: chat_turn_events)
  ▼
MainLoop::run
  │ (5) resolve_presence() ─→ UserPresence::Active
  │ (6) resolve_model(CallerContext::ChatMainLoop) → ResolvedModel (L0: llm_profiles × routing.rs)
  │ (7) emit model.routed (L1)
  │ (8) call_llm(resolved, prompt_with_memory_recall) → stream
  │     (memory recall: MemoryChunk + MemoryEmbedding + RecallDoc)
  │ (9) emit chat_turn.stage:streaming
  ▼
LLM stream
  │ (10) 收到 tool_call
  │ (11) emit tool.call_proposed (L1)
  │ (12) 创建 ToolCall (L0: tool_calls, turn_id 关联)
  ▼
ToolCall state machine (主线程 Tool)
  │ (13) classify risk_level
  │ (14) 如果 need_approval: 创建 PermissionGrant + Proposal
  │ (15) UI: 等待用户批准
  │ (16) 用户批准 → 执行工具
  │ (17) emit tool.call_finished
  │ (18) 回注 LLM (回到 CallLLM streaming)
  ▼
LLM done
  │ (19) emit chat_turn.stage:concluding
  │ (20) 写 ChatMessage (L0: chat_messages)
  │ (21) 写 ChatMessageProjection (L0: chat_message_projections)
  │ (22) emit projection.created
  │ (23) 评估 MemoryCandidate (L0: memory_candidates)
  │ (24) best-effort promote → MemoryEntry (L0)
  │ (25) emit chat_turn.stage:done
  ▼
UI receive Tauri event
  │ (26) ChatPanel 追加 user + assistant 消息
  │ (27) 触发 persona/affection/expression 更新
  │ (28) emit user.presence transition (如用户已离开)
```

---

## 17. 附录 C: 数据流速查(用户创建 Goal → OODA cycle)

```text
User
  │ (1) 桌宠侧边栏"新建 Goal" → invoke('create_goal', ...)
  ▼
commands.rs::create_goal
  │ (2) 创建 GoalRun (L0: goals, status=draft)
  │ (3) emit goal.created
  ▼
User
  │ (4) 确认 Goal → invoke('start_goal', goal_id)
  ▼
GoalOrchestrator::start
  │ (5) GoalRun.draft → planning
  │ (6) 触发首次 OODA cycle
  ▼
GoalCycle 1: observe
  │ (7) observe.rs 拉数据
  │ (8) compute graph_hash
  │ (9) GoalCycle.status = observe, snapshot_ref = hash
  ▼
GoalCycle 1: orient
  │ (10) orient.rs 算 blockers, agent_fit, tension
  │ (11) GoalCycle.orientation_json = OrientReport
  ▼
GoalCycle 1: decide
  │ (12) resolve_model(CallerContext::GoalOrchestrator{phase: Reason, work_kind: Planning}) → ResolvedModel
  │ (13) decide_llm(graph_snapshot, ResolvedModel) → DispatchPlan
  │ (14) emit model.routed
  │ (15) 如果 require_plan_approval: GoalRun → awaiting_plan_approval
  │ (16) 用户在 UI 批准 → positive
  ▼
GoalCycle 1: act
  │ (17) DispatchPlan.tasks 创建 AgentTask (L0: agent_tasks)
  │ (18) 每个 AgentTask 调 SubagentHandle.spawn
  │     ├─→ CliSubprocess: spawn claude with --model
  │     └─→ McpRouter: invoke model.invoke
  │ (19) emit goal.ooda.phase_changed
  ▼
SubagentHandle events (async)
  │ (20) SubagentHandle.Running → emit agent_run.running
  │ (21) tool_call 出现: emit tool.call_proposed
  │ (22) SubagentHandle.Done → result_ref 写 AgentTask
  │ (23) emit agent_run.succeeded
  ▼
GoalCycle 1: review
  │ (24) 收集所有 AgentTask.result_ref
  │ (25) 检查 GoalRun.objective 是否满足
  │ (26) emit goal.ooda.phase_changed → done
  ▼
如果未满足: 回到 observe 触发 cycle 2 (graph_hash 变化)
如果满足: GoalRun.status = succeeded
```

---

## 18. 附录 D: 数据流速查(子线程 AgentTeam 跑 Explore)

```text
MainLoop Act 阶段
  │ (1) DispatchPlan.tasks 包含 team_goal_explore
  │ (2) 创建 AgentTeam (L0: agent_teams, status=draft)
  │ (3) 分配 AgentTeamMember × N (L0: agent_team_members)
  ▼
AgentTeam.planning
  │ (4) 生成 plan (LLM call, 走 Resolver)
  │ (5) AgentTeam.status = awaiting_plan_approval
  ▼
Reviewer Agent
  │ (6) 收到 plan_approval_request (mailbox)
  │ (7) verdict: approved
  │ (8) AgentTeam.status = executing
  ▼
每个 Member 并行:
  │ (9) Member.assigned → running
  │ (10) SubagentHandle.spawn(transport_kind = ...)
  │ (11) SubagentHandle.Running → emit agent_run.running
  │ (12) SubagentHandle 流式返回
  │ (13) SubagentHandle.Done → Member.reporting
  │ (14) 写 Member.result_ref
  ▼
所有 Member 完成
  │ (15) AgentTeam.executing → awaiting_review
  ▼
Reviewer Agent verdict
  │ (16) accepted → AgentTeam.accepted
  │ (17) rejected → AgentTeam.rework_required
  │ (18) rework → 回到 planning
  ▼
AgentTeam.accepted
  │ (19) 7 天后 → archived
  │ (20) emit agent_team.archived
```

---

## 18.5 附录 E: P0 修复后的子线程回写数据流(本文件核心新增)

> 这是 §3.2 P0 缺陷全部修复后的子线程 → 主线程 → 用户感知层 完整闭环。**Round 5.0 完成时,本数据流必须**全部走得通。

```text
                      主线程                              子线程
                        │                                    │
User click "运行"        │                                    │
  │                     │                                    │
  ▼                     │                                    │
LLM emit                │                                    │
tool_call(agent.start)  │                                    │
  │                     │                                    │
  ├─→ L0: tool_calls 行 (turn_id, agent_run_id=null)       │
  ├─→ L1: chat_turn_events tool.call_proposed              │
  ▼                     │                                    │
start_claude_run        │                                    │
(inject_runtime_env)    │                                    │
  │                     │                                    │
  ├─→ L0: agent_runs 行 (status=running,                   │
  │      task_id=metadata.task_id)                          │
  ├─→ bind_executor_run_to_team_member (若 AgentTeam)      │
  ├─→ Tauri: app.emit("agent_runs_changed")                │
  │     [Fix 4: 需要在 finish_spawned_run 也发]             │
  ▼                     │                                    │
                        │  ╔═══════════════════════════════ │
                        │  ║  P0 修复点 1 (start)         │ │
                        │  ╚═══════════════════════════════ │
                        │                                    │
                        │     ┌──────────────────────────┐  │
                        │     │ claude -p / codex 子进程 │  │
                        │     │ (SubagentHandle.Running) │  │
                        │     └────────────┬─────────────┘  │
                        │                  │                │
                        │                  ▼                │
                        │     exit 0 (或 timeout)            │
                        │                  │                │
                        │     ┌────────────▼─────────────┐  │
                        │     │ finish_spawned_run         │  │
                        │     │ [agent_runs.rs:261-409]    │  │
                        │     └────────────┬─────────────┘  │
                        │                  │                │
                        │  ╔═══════════════▼═══════════════ │
                        │  ║  P0 修复点 2 (核心)         │ │
                        │  ╚═══════════════════════════════ │
                        │                  │                │
                        │  ┌─ ① L0: agent_runs.upsert      │
                        │  │     status=succeeded/failed,  │
                        │  │     output_ref, error         │
                        │  │                                │
                        │  ├─ ② L0 [Fix 1]:                │
                        │  │     goal_tasks::set_task_result│
                        │  │     result_ref = output_ref    │
                        │  │     status = completed         │
                        │  │     (旧 tasklist 表同时更新)   │
                        │  │                                │
                        │  ├─ ③ L0 [Fix 3]:                │
                        │  │     member_id = find_by_task() │
                        │  │     set_member_status(Active)  │
                        │  │     if team_all_done:          │
                        │  │       transition_team(AwaitingReview)
                        │  │                                │
                        │  ├─ ④ L1: notify_turn_of_run_    │
                        │  │     completion                │
                        │  │     chat_turn_events:          │
                        │  │     "subagent.completed"      │
                        │  │                                │
                        │  ├─ ⑤ L1 [Fix 2]:                │
                        │  │     trigger_main_thread_       │
                        │  │     reinjection                │
                        │  │     ↓                          │
                        │  │   send_v2::continue_turn_with_ │
                        │  │   subagent_result             │
                        │  │     ↓                          │
                        │  │   1. append_turn_event         │
                        │  │      (tool.subagent_result)    │
                        │  │   2. continue_existing_turn    │
                        │  │      (UserPresence check)      │
                        │  │   3. LLM 主动总结 + 回复        │
                        │  │                                │
                        │  └─ ⑥ L1 [Fix 4]:                │
                        │        emit agent_run_finished   │
                        │        + Tauri:                  │
                        │        app.emit(                 │
                        │          "agent_runs_changed",   │
                        │          { run_id, status,        │
                        │            output_ref, error })   │
                        │                                    │
                        ▼                                    │
                ┌─────────────────────┐                     │
                │ Tauri Event Bus     │                     │
                └──────────┬──────────┘                     │
                           │                                │
        ┌──────────────────┼──────────────────┐             │
        ▼                  ▼                  ▼             │
  AgentLanes.tsx   GoalConsole.tsx   TaskDrawerPane.tsx     │
  listen(          listen(           listen(                │
    agent_runs_      agent_runs_       agent_runs_          │
    changed)         changed)          changed)             │
        │                  │                  │             │
        └──────────┬───────┴──────────┬───────┘             │
                   ▼                  ▼                     │
            fetch list_         fetch list_                 │
            agent_runs          goals                       │
                   │                  │                     │
                   ▼                  ▼                     │
            L2 Read Model:       L2 Read Model:            │
            active_run           goal_console               │
            (Tauri event 推)     (Tauri event 推)            │
                   │                                          │
                   ▼                                          │
              ┌────────────────────┐                         │
              │ ChatPanel 渲染      │                         │
              │  - assistant 消息   │  ← Fix 2 触发的新消息  │
              │  - tool_use_card    │  ← Fix 2 注入的        │
              │  - GoalConsole     │  ← Fix 3 推动 team     │
              │  - AgentLanes      │  ← Fix 4 自动 refresh  │
              └────────────────────┘                         │
```

**关键观察**:
- 修复后, **4 条独立的"完成"事件**汇聚到 Tauri Bus, 前端 **统一**通过 listen 拿
- **不需要**前端轮询 / 手动刷新
- **不需要**用户发新消息 — Fix 2 让 LLM 主动总结
- **不需要**OODA 等待 — Fix 3 让 team 状态机自动推进

**修复前 vs 修复后对比**:

| 路径 | 修复前 | 修复后 |
|---|---|---|
| `goal_tasks.result_ref` | 永为空 | 写 `runs/{id}-output.json` |
| 主线程 LLM 看到 subagent 结果 | ❌ 必须 user 发消息 | ✅ 自动续轮 |
| AgentTeam Member 状态 | 永远 Active | 自动 Reporting → Completed |
| AgentTeam 整体状态 | 永远 Executing | 全员完成 → AwaitingReview |
| 前端 AgentLanes | 永远显示 running | 自动 refresh 成功 |
| OODA review 等待 accepted | 永远等不到 | 几秒内完成 |

---

## 19. 一句话总结

> **本项目未来 5-6 周的目标是 28 个一等对象 + 8 类状态机(用户/对话/主线程 LLM/主线程 OODA/主线程 Tool/SubagentHandle/McpRouterSession/AgentTeam) + 4 层数据流(L0/L1/L2/L3) 全部归一化到统一 Runtime;8 个冲突点 (C1-C8) 和 5 个阻塞点 (B1-B5) 已识别并附解除路径;4 个 P0 缺陷 (P0-1 至 P0-4) — Subagent 结果回写与感知层投影 — 已在 §3.2 标注, §10.6 给出 4 个 fix 的具体实现路径, 作为 Round 5.0 (3-4 天) 独立先发,作为后续 Round 5-7 的前置条件 (GB0 硬门禁)。其中 WorkKind / OodaPhase / SubagentHandle / CallerContext / goal_cycle_id 锚点 / LlmProfile.transport / result_ref 闭环 / LLM 续轮 八个新抽象与修复是关键的"合页",而 LLM Reason 任务接 ModelResolver 是范式跃迁的真正杠杆。LC-10 (Agent Runtime 完全可用 + 范式升级) 预计 Round 7 结束达成。**

**特别强调**: P0 修复 (Round 5.0) **应当**消除 [Bug说明-Goal会话双Working与任务结果插位-20260601.md](./Bug说明-Goal会话双Working与任务结果插位-20260601.md) 中描述的"任务结果插位"问题,因为该问题的根因正是 P0-1 (result_ref 未反写) + P0-3 (Member 状态机未接线)。
