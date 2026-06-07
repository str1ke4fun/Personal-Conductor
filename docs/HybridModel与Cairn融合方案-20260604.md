# 混合模型指派方案与 Cairn 移植架构融合评估

> 日期: 2026-06-04
> 范围: 把 [hybrid-model-dispatch-inproject-plan-20260604.md](./hybrid-model-dispatch-inproject-plan-20260604.md)（Plan 1）+ [hybrid-model-dispatch-mcpserver-plan-20260604.md](./hybrid-model-dispatch-mcpserver-plan-20260604.md)（Plan 2）并入 [Cairn架构对位与移植评估-20260604.md](./Cairn架构对位与移植评估-20260604.md)（Cairn Round 5-7）
> 方法: 现状代码对读 + 三方案状态机交叉 + 冲突识别 + 合并任务图
> 核心结论: **三方案共享同一根（`routing.rs` + `llm_profiles.rs`），应合并为单一路线 15 项任务**；Cairn 的 P3（LLM Reason）是让三方案价值最大化的杠杆
> 关联: [Cairn架构对位与移植评估-20260604.md](./Cairn架构对位与移植评估-20260604.md) / [AgentTeam综合评审-20260529.md](./AgentTeam综合评审-20260529.md) / [多Agent共享工作区与自治Goal调度方案-20260530.md](./多Agent共享工作区与自治Goal调度方案-20260530.md)

---

## 0. 三方案现状速览

### 0.1 各自的关注点

| 方案 | 焦点 | 关键产物 | 状态 |
|---|---|---|---|
| **Cairn 移植** ([doc](./Cairn架构对位与移植评估-20260604.md)) | Agent 认知范式: OODA + Blackboard + 3 种任务类型 | P1-P12 共 12 个改造点,分 Round 5-7 | 📋 规划 |
| **Plan 1 进程内** ([doc](./hybrid-model-dispatch-inproject-plan-20260604.md)) | 多模型路由: role/TaskKind → LlmProfile | H1-H7 共 7 个任务 | 📋 方案待评审 |
| **Plan 2 MCP 抽离** ([doc](./hybrid-model-dispatch-mcpserver-plan-20260604.md)) | 模型路由作为 MCP server 暴露给 Claude/Codex | M1-M6 共 6 个任务 | 📋 方案待评审 |

### 0.2 已识别的自合并点

Plan 2 文档第 4 节已经明确:
> 若 D1 选**下沉(b)**: Plan 1 的 H1/H2 与 Plan 2 的 M1/M2 是**同一份代码**,只实现一次。

本次评估**确认并扩展**这个自合并点:Cairn 移植架构的 P3 (LLM Reason) 也应该消费这个统一引擎。下图是三方案的依赖关系:

```text
                          ┌──────────────────────────────┐
                          │  model-router-core (新 crate) │
                          │   - llm.rs (双协议)           │
                          │   - routing.rs                │
                          │   - llm_profiles.rs           │
                          │   - ModelResolver (新)        │
                          │   - classify_task (复用)      │
                          └──────────┬───────────────────┘
                                     │ 共享下沉
            ┌────────────────────────┼────────────────────────┐
            ▼                        ▼                        ▼
   Plan 1 进程内 H1-H7      Plan 2 MCP Server M1-M6    Cairn P3 LLM Reason
   (send_v2/summarizer/     (model.route / list /      (decide_llm 走
    smart_monitor 调用)      invoke 三工具)              ModelResolver)
            │                        │                        │
            └────────────────────────┼────────────────────────┘
                                     ▼
                          chat_turn_events (统一观测总线)
                          含 model.routed + frame.surprise
                                     ▲
                                     │
                          Cairn P5 graph snapshot API
                          (含 fact/intent/hint/routing_decision)
```

---

## 1. 关键冲突识别(必须先解决)

### 1.1 冲突 C1: 两套 TaskKind 同名不同义 ⚠️ 高优先级

**现状**:

| 来源 | 枚举值 | 含义 |
|---|---|---|
| `routing.rs:14-21` (Plan 1 用) | `Planning / Coding / Review / Testing / Document / ExternalAction` | **工作内容**类型(在做什么) |
| Cairn P2 提议 | `Bootstrap / Reason / Explore` | **OODA 阶段**(在 OODA 循环的哪一步) |

**冲突**: 都是 `TaskKind`,但语义不同。如果两边都叫 TaskKind 会出现 `routing.TaskKind::Planning` 和 `goal.TaskKind::Reason` 互相转换的混乱。

**解决方案**: 拆开命名,语义分层

```rust
// crates/conductor-core/src/routing.rs (保留, 含义不变)
pub enum WorkKind {                    // 重命名: TaskKind → WorkKind
    Planning, Coding, Review, Testing, Document, ExternalAction
}

// crates/conductor-core/src/goal_orchestrator/act.rs (Cairn P2 落地)
pub enum OodaPhase {                   // 重命名: TaskKind → OodaPhase
    Bootstrap, Reason, Explore
}
```

**改造量**: 0.5 人天(纯重命名 + 调用点调整)。**必须**作为 Round 5 的 P0 任务先做。

---

### 1.2 冲突 C2: Worker 注册在两处定义 ⚠️ 中优先级

**现状**:

| 来源 | Worker 类型 | 注册位置 |
|---|---|---|
| `agent_backends.rs` (Cairn P8 提议) | Claude / Codex CLI 子进程 | `BackendKind` 枚举 |
| `llm_profiles.rs` (Plan 1 用) | Claude / Doubao / GPT HTTP API | `LlmProfile.provider` 字段 |

**冲突**: 一个"模型"(如 Claude)既能作为子进程(CLI)又能作为 HTTP API。`LlmProfile.provider` 只限 `openai|anthropic|local`(`llm_profiles.rs:38`),**没有"claude_cli" / "codex_cli"** 这种进程类。

**解决方案**: 在 `LlmProfile` 上加 `transport: TransportKind` 字段

```rust
// llm_profiles.rs:13
pub struct LlmProfile {
    // ... 现有字段 ...
    pub transport: TransportKind,       // 新增
}

pub enum TransportKind {
    HttpApi,        // 走 llm.rs 现有协议(openai/anthropic 兼容)
    CliSubprocess,  // 走 agent_runs.rs (claude -p / codex CLI)
}
```

**改造量**: 1 人天(migration + 后端选择逻辑分支)。**优先级**: P1。

---

### 1.3 冲突 C3: 路由表缺少 OODA 阶段维度 ⚠️ 中优先级

**现状**:
- `routing.rs` 的 `RoutingPolicy` 字段:`task_kind, backend_kind, profile_id, priority, enabled, reason_template`
- 它能回答"Planning 工作走哪个 Claude profile"
- 但**不能**回答"Reason 阶段用哪个模型" (这是 Cairn P3 关心的)

**解决方案**: 加 `caller_phase: OodaPhase` 字段(可选)

```rust
pub struct RoutingPolicy {
    pub task_kind: WorkKind,
    pub backend_kind: BackendKind,
    pub profile_id: Option<String>,
    pub priority: i64,
    pub enabled: bool,
    pub reason_template: String,
    pub caller_phase: Option<OodaPhase>,  // 新增: None = 任意阶段
    // ...
}
```

**典型用法**:
- 路由 1: `WorkKind=Planning, caller_phase=Some(Reason)` → 规划阶段 Reason 走 Claude Sonnet
- 路由 2: `WorkKind=Document, caller_phase=Some(Explore)` → 文档执行 Explore 走 Doubao

**改造量**: 0.5 人天。**优先级**: P1。

---

## 2. 六个核心融合点

### 2.1 融合点 I: TaskKind × ModelRole 统一建模

**目标**: 让 `WorkKind`(原 routing.TaskKind) + `OodaPhase`(原 Cairn TaskKind) + `ModelRole`(Plan 1) 三个维度**共存于同一 registry**,由 `RoutingPolicy` 作为 JOIN TABLE。

**落点文件**:
- [crates/conductor-core/src/routing.rs:14-21](../crates/conductor-core/src/routing.rs#L14-L21) — 重命名 `TaskKind → WorkKind`
- [crates/conductor-core/src/llm_profiles.rs:13-24](../crates/conductor-core/src/llm_profiles.rs#L13-L24) — 加 `transport` + `preferred_phases`
- 新增 [crates/conductor-core/src/goal_orchestrator/phase.rs](../crates/conductor-core/src/goal_orchestrator/observe.rs) — `OodaPhase` 枚举

**映射表**(评审决策点 D2):

| WorkKind | OodaPhase | 典型 ModelRole | 典型 LlmProfile(provider) |
|---|---|---|---|
| Planning | Reason | Planner | anthropic / claude-sonnet |
| Planning | Explore | Planner | anthropic / claude-sonnet |
| Coding | Explore | Coder | openai / gpt-4 (or codex CLI) |
| Review | Reason | Reviewer | anthropic / claude-sonnet |
| Document | Explore | Summarize | openai / doubao-pro (国内) |
| ExternalAction | Explore | Coder | openai / codex CLI |
| (任意) | Bootstrap | Sense | openai / doubao-lite (轻量) |
| (任意) | Reason | Planner | anthropic / claude-sonnet (战略) |

**改动收益**: 一次配置,所有派发路径共享。

**工作量**: 1.5 人天(含迁移 + UI)。**难度**: 中。

---

### 2.2 融合点 II: ModelResolver 统一收口

**目标**: 把 Plan 1 的 `model_resolver.rs` 升级为**全项目唯一的"角色→模型"解析点**。Cairn 的 Reason task、Plan 1 的 send_v2、Plan 2 的 MCP server **都调用同一个 resolver**。

**新文件**: [crates/conductor-core/src/model_resolver.rs](../crates/conductor-core/src/llm.rs)(新建)

**签名**:
```rust
pub struct ResolvedModel {
    pub profile_id: Option<String>,
    pub transport: TransportKind,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub temperature: f64,
}

pub enum CallerContext {
    GoalOrchestrator { phase: OodaPhase, work_kind: WorkKind },
    ChatMainLoop,
    Summarizer,
    SmartMonitor,
    Subagent,
    McpTool { tool: String },
}

pub async fn resolve(ctx: CallerContext, hint: Option<&str>) -> Result<ResolvedModel>;
```

**关键设计**:
- `CallerContext` 显式表达"谁在调用、什么阶段、什么工作",resolver 内部按 `RoutingPolicy` 表查询
- `hint` 透传(对应 Cairn H1 的 hint 文本)
- **任何错误都回退 `config.llm`**(Plan 1 R1)

**三方案消费方**:

| 调用方 | CallerContext | 落点 |
|---|---|---|
| Plan 1 send_v2 | `ChatMainLoop` | `send_v2.rs:1670/2102/2142` |
| Plan 1 summarizer | `Summarizer` | `summarizer.rs:157` |
| Plan 1 smart_monitor | `SmartMonitor` | `smart_monitor.rs:202` |
| Plan 1 subagent | `Subagent` | `agent_runs.rs:206` |
| **Cairn P3** | `GoalOrchestrator { phase: Reason, work_kind: ... }` | `goal_orchestrator/decide.rs` |
| Plan 2 MCP | `McpTool { tool: "model.route" }` | `model-router-mcp/src/tools.rs` |

**工作量**: 2 人天(纯新增)。**难度**: 中。**前置依赖**: C1+C2+C3 全部解决。

---

### 2.3 融合点 III: Cairn P3 (LLM Reason) 接 ModelResolver

**目标**: 让 Cairn 的 Reason 任务能选不同模型。例如:
- Reason 阶段战略判断 → Claude Sonnet(高质量)
- Explore 阶段执行 Coding → Codex CLI(可写文件)
- Bootstrap 阶段快速试探 → Doubao Lite(便宜)

**落点**:
- [crates/conductor-core/src/goal_orchestrator/decide.rs](../crates/conductor-core/src/goal_orchestrator/decide.rs) — 拆出 `decide_llm(graph, caller_ctx)`
- [crates/conductor-core/src/agents/dispatcher/prompts/reason.md](../crates/conductor-core/src/agents/dispatcher/prompts/default/reason.md)(新) — Reason Prompt 模板

**改动**(在 Plan 1 H2 之后追加):

```rust
// decide.rs
pub async fn decide_llm(
    graph_snapshot: &GraphSnapshot,
    budget: &Budget,
    objective: &str,
) -> Result<DispatchPlan> {
    let resolved = model_resolver::resolve(
        CallerContext::GoalOrchestrator {
            phase: OodaPhase::Reason,
            work_kind: WorkKind::Planning,  // 默认
        },
        None,
    ).await?;

    let prompt = render_reason_prompt(graph_snapshot, objective);
    let raw = llm_call_with_resolved(&resolved, &prompt).await?;
    let parsed: ReasonOutput = serde_json::from_str(&raw)?;

    append_turn_event("model.routed", json!({
        "phase": "reason",
        "profile_id": resolved.profile_id,
        "provider": resolved.provider,
        "model": resolved.model,
    })).await?;

    Ok(plan_from_reason_output(parsed, budget)?)
}
```

**关键点**: P3 的 LLM 调用**不**直接走 `config.llm`,而是走 ModelResolver。这就是 Cairn 范式和 Plan 1 的真正交点。

**工作量**: 3-4 人天(Prompt 工程占一半)。**难度**: 中。**前置**: 融合点 II。

---

### 2.4 融合点 IV: MCP Server 作为 Cairn 外部子代理后端

**目标**: Plan 2 的 `model-router-mcp` **不只**服务 Claude/Codex 外部,**同时**是 Cairn P2 中"Explore 任务"可选的外部执行器之一。

**架构**:

```text
Cairn P2 PlannedTask(task_kind=Explore)
  │
  ▼
  Goal Orchestrator Act 阶段
  │
  ├── 进程内执行 (走 llm.rs, 如 summarizer)
  │
  ├── 进程外 Claude/Codex CLI (走 agent_runs.rs)
  │
  └── 进程外 MCP 调用 (新, 走 ModelResolver → model-router-mcp)
        │
        ▼
   model-router-mcp (stdio)
        │ tools/call: model.invoke
        ▼
   任意 LLM (Claude/Doubao/GPT)
```

**落点**:
- [crates/conductor-core/src/agents/dispatcher/executors/mcp.rs](../crates/conductor-core/src/agents/dispatcher/executors/explore.rs)(新) — MCP 后端 executor
- [crates/conductor-core/src/agent_backends.rs](../crates/conductor-core/src/agent_backends.rs) — `BackendKind::McpRouter` 新增

**改动量**: 1.5 人天(复用 mcp.rs 客户端)。**难度**: 中。

**收益**: 未来 Cairn Explore 任务可以调用任意 LLM,不再受 `claude -p` 是否支持 `--model` 限制(Plan 1 R2 直接解决)。

---

### 2.5 融合点 V: 观测总线统一

**目标**: 把三方案的观测事件都走 `chat_turn_events` 一条总线。

**事件类型**(新增到 `events.rs` 的 enum):

| Event | 触发方 | Payload |
|---|---|---|
| `model.routed` | Plan 1 resolver | `{role, profile_id, provider, fallback_used, caller_ctx}` |
| `cognition.frame_activated` | Cairn P9 (Tension 字段后续) | `{frame_id, replaces, tension_score}` |
| `cognition.surprise` | Cairn 后续 Noesis | `{frame_id, expected, actual, severity}` |
| `mcp.tool_invoked` | Plan 2 | `{tool, transport, duration_ms, success}` |
| `goal.ooda_phase_changed` | Cairn P2 | `{goal_id, from_phase, to_phase, trigger}` |

**落点**: [crates/conductor-core/src/turns.rs:338](../crates/conductor-core/src/turns.rs#L338) `append_turn_event_by_request`

**关键设计**: **统一不允许记 api_key**(Plan 1 R3),secret 类内存只在 resolve 瞬间持有。

**工作量**: 1 人天。**难度**: 低。

---

### 2.6 融合点 VI: UI 配置面板合并

**目标**: 三方案的配置 UI 都进 Settings 面板,避免三个独立设置页。

**Plan 1 H7**: Profile CRUD + 角色绑定
**Cairn P11**: Blackboard 图视图(独立)
**Cairn P4**: Hint 注入按钮(在 Goal 详情页)

**合并方案**:
- Settings 面板加 **"Models"** 标签页(Plan 1 H7)
- Goal 详情页加 **"Reasoning"** 标签页(包含 Hint 注入 + Reasoning Trace 视图 + 当前 OODA 阶段指示)
- 独立 **"Blackboard"** 全屏视图(只在需要时打开)

**前端改动量**:
- Settings panel: 2-3 人天
- Goal Reasoning tab: 1-2 人天
- 共享组件(Phase 指示器 / Tension gauge): 0.5 人天

**难度**: 中。

---

## 3. 合并后的统一任务图(15 项)

把三方案按依赖关系重排为 15 个任务,标注每个任务的来源:

### Phase A: 基础合并(Week 1,共 4 项)

| ID | 任务 | 来源 | 工作量 | 验收 |
|---|---|---|---|---|
| **A1** | 解决 C1 冲突: 重命名 `TaskKind → WorkKind`, 新建 `OodaPhase` | 融合 C1 | 0.5d | 全 crate 编译, grep `TaskKind::` 全部更新 |
| **A2** | 解决 C2 冲突: `LlmProfile.transport` 新字段 + migration | 融合 C2 | 1d | migration 通过,`LlmProfile` CRUD 走 transport-aware 路径 |
| **A3** | 解决 C3 冲突: `RoutingPolicy.caller_phase` 新字段 + migration | 融合 C3 | 0.5d | routing 表支持按 phase 过滤 |
| **A4** | 新增 `model_resolver.rs`: `CallerContext` + `ResolvedModel` + `resolve()` 骨架 | 融合点 II | 1.5d | 单测: 命中 profile / disabled 回退 / 无 policy 回退 三分支 |

**累计**: 3.5 人天。

### Phase B: 进程内替换(Week 2,共 5 项)

| ID | 任务 | 来源 | 工作量 | 验收 |
|---|---|---|---|---|
| **B1** | send_v2/summarizer/smart_monitor 走 `resolve()` | Plan 1 H3+H4 | 2d | 配 Doubao profile, 总结调用命中 Doubao base_url (集成测试断言 request host) |
| **B2** | 子代理 `agent_runs.rs` 走 `resolve()` + 注入 `--model` 或切换 adapter | Plan 1 H5 | 1.5d | `command_json` 记录 profile, `subagent.claude_p` 带 `--model` |
| **B3** | Cairn P1: ReasonCheckpoint 防抖 | Cairn P1 | 0.5d | 连续 run_cycle 第二次返回 skipped |
| **B4** | Cairn P12: result_ref 闭环 | Cairn P12 | 1d | AgentRun 完成 → AgentTask.result_ref 反写 |
| **B5** | Cairn P4: goal_hints 表 + UI 注入 | Cairn P4 | 1d | Reason 阶段读图能拿到新 hint |

**累计**: 6 人天。

### Phase C: 观测 + 范式升级(Week 3,共 4 项)

| ID | 任务 | 来源 | 工作量 | 验收 |
|---|---|---|---|---|
| **C1** | 路由观测化: `model.routed` 事件 + chat_turn_events 接入 | Plan 1 H6 + 融合 V | 1d | `chat_turn_events` 出现 `model.routed` 事件, 不含 api_key |
| **C2** | Cairn P3: LLM Reason 任务接 ModelResolver | 融合 III | 3-4d | Reason LLM 可选 Claude/Doubao, prompt 中文版,契约校验生效 |
| **C3** | Cairn P2: TaskKind(Bootstrap/Reason/Explore) Driver 抽象 | Cairn P2 | 2d | 三种任务类型共用 worker, 只换 prompt |
| **C4** | Cairn P5: graph snapshot API | Cairn P5 | 1d | `GET /v1/goals/{id}/graph?format=yaml` 返回稳定 YAML |

**累计**: 7-8 人天。

### Phase D: MCP 抽离 + 范式外延(Week 4,共 4 项)

| ID | 任务 | 来源 | 工作量 | 验收 |
|---|---|---|---|---|
| **D1** | Cairn P10: JSON Schema 契约(reason/explore 输出) | Cairn P10 | 1d | contracts.rs 单测, 非 JSON 输出被显式拒绝 |
| **D2** | Plan 2 M1+M2: `model-router-core` crate 抽离 (选下沉 b) | Plan 2 M1+M2 | 2d | 复用 A1-A4 + B1 的所有代码, conductor-core 全绿 |
| **D3** | Plan 2 M3+M4: MCP server loop + 三工具 | Plan 2 M3+M4 | 2d | Claude Code 配置 mcpServers 后三工具可见, 端到端调用 |
| **D4** | 融合点 IV: McpRouter executor (Cairn 外部子代理) | 融合 IV | 1.5d | PlannedTask(backend_kind=McpRouter) 走 mcp 调用 |

**累计**: 6.5 人天。

### Phase E: UI + 完善(Week 5,共 3 项)

| ID | 任务 | 来源 | 工作量 | 验收 |
|---|---|---|---|---|
| **E1** | Settings Panel: Models 标签页(Plan 1 H7) | Plan 1 H7 + 融合 VI | 2-3d | 用户在 UI 建 Claude/Doubao/GPT 三档, 绑定角色 |
| **E2** | Goal 详情页: Reasoning tab (Hint 注入 + 阶段指示) | Cairn P4 + 融合 VI | 1-2d | hint 按钮 + 当前 OODA 阶段可见 |
| **E3** | Cairn P11: Blackboard 图视图 (Cytoscape) | Cairn P11 | 3-5d | GraphView 组件渲染 10 节点 < 1s |

**累计**: 6-10 人天。

### Phase F: 范式外延(Week 6+,后续可选项)

| ID | 任务 | 来源 | 工作量 | 备注 |
|---|---|---|---|---|
| **F1** | Cairn P7: 任务 claim 协议 | Cairn P7 | 1.5d | 防双跑 |
| **F2** | Cairn P6: Worker 健康熔断 | Cairn P6 | 1.5d | 失败 worker 冷却 |
| **F3** | Cairn P9: Graph Tension 字段 | Cairn P9 | 1.5d | 决策优先 |
| **F4** | Cairn P8: Worker 声明式注册 (与 E1 共享) | Cairn P8 | 2d | 与 Settings Panel 同步做 |
| **F5** | Plan 2 M5: 密钥 keyring 化 | Plan 2 M5 | 1d | 上线前置 |
| **F6** | Plan 2 M6: Claude/Codex 接入文档 | Plan 2 M6 | 1d | 上线前置 |

**累计**: 8.5 人天。

---

## 4. 合并后的依赖图

```text
A1 (WorkKind 重命名)
  ├─→ A2 (LlmProfile.transport)
  │     └─→ A3 (RoutingPolicy.caller_phase)
  │           └─→ A4 (model_resolver)
  │                 ├─→ B1 (send_v2/summarizer/smart_monitor 走 resolver)
  │                 ├─→ B2 (子代理走 resolver)
  │                 ├─→ C2 (Cairn P3 LLM Reason)
  │                 └─→ D2 (Plan 2 model-router-core 抽离)
  │                       └─→ D3 (MCP server)
  │                             └─→ D4 (McpRouter executor)
  ├─→ B3 (Cairn P1 ReasonCheckpoint) ─────────────────────┐
  ├─→ B4 (Cairn P12 result_ref)                            │
  ├─→ B5 (Cairn P4 goal_hints) ─→ C4 (Cairn P5 graph API)  │
  │                                      └─→ C2 (P3)     │
  └─→ C1 (观测) ─→ C3 (P2 TaskKind 抽象) ─→ D1 (P10)     │
                                                          ▼
                                              (B3+B4+B5+C1-C4+D1 并行)
                                                          │
                                              E1-E3 (UI)  │
                                                          │
                                              F1-F6 (后续) │
```

**关键路径**: A1 → A2 → A3 → A4 → B1 → C2 → D2 → D3 (4 周)

---

## 5. 合并测试矩阵

### 5.1 Phase A 测试(冲突解决)

| ID | 用例 | 期望 |
|---|---|---|
| TC-A-01 | grep `TaskKind::` 在全 crate 替换为 `WorkKind::` | 无残留引用 |
| TC-A-02 | `LlmProfile.transport = HttpApi` 创建 | DB 行有 transport 字段 |
| TC-A-03 | `LlmProfile.transport = CliSubprocess` 创建 + 解析 | resolver 走 agent_runs 路径 |
| TC-A-04 | `RoutingPolicy.caller_phase = Some(Reason)` 过滤 | query 返回只匹配 Reason 阶段的 policy |
| TC-A-05 | `resolve()` 命中 profile | 返回该 profile 的 ResolvedModel |
| TC-A-06 | `resolve()` profile disabled | 静默回退 config.llm, 不 panic |
| TC-A-07 | `resolve()` 无 policy | 静默回退 config.llm, 不 panic |
| TC-A-08 | `resolve()` DB 错误 | 静默回退 config.llm, 记 error log |

### 5.2 Phase B 测试(进程内替换)

| ID | 用例 | 期望 |
|---|---|---|
| TC-B-01 | send_v2 主循环配 Claude profile | LLM 请求 base_url 命中 Claude |
| TC-B-02 | summarizer 配 Doubao profile | 总结调用 base_url 命中 doubao.com |
| TC-B-03 | smart_monitor 配 Sense 角色 | 监控调用命中 Sense profile |
| TC-B-04 | agent_run 注入 profile.model | `command_json` 记录 model 字段, 子进程命令行含 `--model` |
| TC-B-05 | Goal 内 graph 无变化, 连续 run_cycle | 第二次返回 CycleResult::skipped |
| TC-B-06 | AgentRun 完成时 metadata.task_id 有值 | AgentTask.result_ref 反写 |
| TC-B-07 | Hint 注入 → Reason 阶段读图 | observe.recent_hints 非空 |

### 5.3 Phase C 测试(范式升级)

| ID | 用例 | 期望 |
|---|---|---|
| TC-C-01 | resolve() 决策写 chat_turn_events | 出现 model.routed 事件, payload 不含 api_key |
| TC-C-02 | LLM Reason 收到 goal 已满足的 graph | 返回 complete, Goal 切到 succeeded |
| TC-C-03 | LLM Reason 选不同 model profile | turn 的 model_name 反映实际命中 profile |
| TC-C-04 | PlannedTask.task_kind = Reason | dispatch 走 reason.md prompt |
| TC-C-05 | GET /v1/goals/{id}/graph?format=yaml | 返回含 facts/intents/hints 稳定键名的 YAML |

### 5.4 Phase D 测试(MCP 抽离)

| ID | 用例 | 期望 |
|---|---|---|
| TC-D-01 | `model-router-core` crate 单独编译 | 通过 |
| TC-D-02 | conductor-core 全测试在 model-router-core 抽离后 | 全绿 (无回归) |
| TC-D-03 | Claude Code 配置 mcpServers 启动 model-router-mcp | `tools/list` 返回 3 个 model.* 工具 |
| TC-D-04 | `model.route{role:"sense"}` 命中 Doubao | 端到端返回 |
| TC-D-05 | PlannedTask(backend_kind=McpRouter) | 走 mcp 调用而非 CLI |
| TC-D-06 | LLM Reason 输出非 JSON | contracts.rs 返回明确错误, 任务标记 failed |

### 5.5 Phase E 测试(UI)

| ID | 用例 | 期望 |
|---|---|---|
| TC-E-01 | Settings → Models 新建 Claude profile | 列表显示, enabled |
| TC-E-02 | 角色绑定: Planner → Claude, Sense → Doubao | resolve() 正确分流 |
| TC-E-03 | Goal 详情 → Reasoning → 注入 hint | hint 写入 goal_hints, 下次 Reason 读到 |
| TC-E-04 | Goal 详情 → 阶段指示器 | 显示当前 OODA 阶段 (Reason/Explore/...) |
| TC-E-05 | Blackboard 图视图渲染 10 节点 | < 1s 加载 |

---

## 6. 冲突解决后的命名与接口规范

### 6.1 命名规范

| 旧名 | 新名 | 出处 |
|---|---|---|
| `routing::TaskKind` | `routing::WorkKind` | 融合 C1 |
| `goal_orchestrator::TaskKind`(Cairn 提议) | `goal_orchestrator::OodaPhase` | 融合 C1 |
| `model_resolver::ModelRole`(Plan 1 提议) | `model_resolver::CallerContext` | 融合 II |
| `LlmProfile` | (保留) | — |
| `LlmProfile.provider`(string) | `LlmProfile.transport + provider` | 融合 C2 |

### 6.2 关键接口

```rust
// crates/conductor-core/src/model_resolver.rs
pub struct ResolvedModel {
    pub profile_id: Option<String>,
    pub transport: TransportKind,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub temperature: f64,
    pub source: ResolvedSource,  // Profile / Config / Fallback
}

pub enum TransportKind {
    HttpApi,
    CliSubprocess,
}

pub enum ResolvedSource {
    Profile { id: String, name: String },
    Config,
    Fallback { reason: String },
}

pub enum CallerContext {
    GoalOrchestrator { phase: OodaPhase, work_kind: WorkKind },
    ChatMainLoop,
    Summarizer,
    SmartMonitor,
    Subagent,
    McpTool { tool: String },
}

pub async fn resolve(ctx: CallerContext, hint: Option<&str>) -> Result<ResolvedModel>;
```

---

## 7. 风险与门禁

### 7.1 风险

| ID | 风险 | 概率 | 影响 | 缓解 |
|---|---|---|---|---|
| R1 | 重命名 TaskKind 引发编译雪崩 | 高 | 中 | 一次性 PR, 全 crate 同步编译 |
| R2 | LlmProfile.transport migration 失败 | 中 | 高 | D2 验收硬门禁: conductor-core 全测试绿 |
| R3 | ModelResolver 性能回退 | 低 | 中 | B1 完成后做一轮 benchmark, 延迟 < 5ms |
| R4 | LLM Reason 选错模型导致决策质量差 | 中 | 高 | C2 加 A/B 对照, 默认走 Claude, 用户可降级 |
| R5 | MCP server 抽离破坏现有 mcp.rs 客户端 | 低 | 中 | 复用 mcp.rs 类型, 不改 client 路径 |
| R6 | 三方案排期冲突 | 中 | 中 | Phase A 严格按依赖顺序, B/C/D 可并行 |

### 7.2 硬门禁

| 门禁 | 触发点 | 失败动作 |
|---|---|---|
| **GB1**: conductor-core 全测试绿 | A2 / A3 / A4 / B1 / D2 后 | 不允许 merge |
| **GB2**: Resolver fallback 测试 3 分支全通过 | A4 后 | 不允许进入 Phase B |
| **GB3**: Round 5 5 项 (B3+B4+B5+C1+C4+D1) 全通过 | Phase B+C 完成后 | 不允许进入 Phase D |
| **GB4**: Plan 2 MCP server 端到端 | D3 后 | 不允许进入 Phase E |
| **GB5**: LLM Reason 决策质量 A/B | C2 后 | 默认配置, 人工审核通过 |

---

## 8. 推荐执行顺序

### 8.1 调整后的轮次

| 轮次 | 内容 | 周期 | 关键产出 |
|---|---|---|---|
| **Round 5 (本次合并)** | Phase A 全部 + Phase B 全部 + Phase C 部分 | 2 周 | 进程内混合模型路由 + Cairn 防抖/hint/result_ref |
| **Round 6** | Phase C 剩余 (C2 LLM Reason) + Phase D 全部 | 2 周 | LLM Reason 战略判断 + MCP server |
| **Round 7** | Phase E 全部 (UI) + Phase F 部分 | 2-3 周 | 用户可配置 + 范式外延 |
| **Round 8+** | Phase F 剩余 + Noesis 评估 | 1 月+ | 完整 OODA + Tension + 迈向 Sensemaking |

### 8.2 派工卡片模板(可直接套用)

每个 Phase A/B/C/D 任务都需要 State Impact Card(参考 [STATE_IMPACT_CARD_TEMPLATE.md](./STATE_IMPACT_CARD_TEMPLATE.md))。例:

```yaml
# A1: 解决 C1 冲突 - TaskKind 重命名
mode: architecture
status: ready
object: routing::TaskKind, goal_orchestrator::TaskKind
current_state: 两处都用 TaskKind 命名, 语义不同
trigger: 三方案融合必须先统一命名
target_state: 
  - routing::TaskKind → routing::WorkKind
  - 新增 goal_orchestrator::OodaPhase{Bootstrap, Reason, Explore}
guard: 单元测试全绿, 编译无 warning
side_effects: 全 crate import 路径变更
illegal_transitions: 无
recovery: 纯重命名, 可 git revert
canonical_source: 命名集中在 routing.rs + goal_orchestrator/phase.rs
read_models: chat_turn_events 事件 payload 用 work_kind / ooda_phase 字段
tests: TC-A-01
```

### 8.3 给项目 owner 的建议

1. **本周**: 启动 Phase A (4 项, 3.5 人天)。这是合并三方案的**真正起点**,不能跳过。
2. **下周**: Phase A 全绿后启动 Phase B (5 项, 6 人天), 这部分把 Plan 1 落地 + 修两个已知问题 (Cairn P1 防抖 + P12 result_ref)。
3. **评审点**: Phase B 完成后做一次"是否继续 Phase C (LLM Reason)" 的评审——这一步是范式跃迁,需要 prompt 工程投入。
4. **同步**: 把本文并入 [INDEX.md](./INDEX.md) "参考与调研" 分类, 与 [Cairn架构对位与移植评估-20260604.md](./Cairn架构对位与移植评估-20260604.md) 互相引用。

---

## 9. 决策记录(评审决策点)

| ID | 决策点 | 备选 | 建议 | 影响范围 |
|---|---|---|---|---|
| **D1** | Plan 2 抽离策略 | (a) 复制提取 (b) 共享下沉 | **b** | conductor-core 依赖图改, 单引擎双用 |
| **D2** | 三 TaskKind 命名 | 三套名 / 两套名 / 一套名 | **两套**(`WorkKind` + `OodaPhase`) | 命名清晰, 改动量适中 |
| **D3** | RoutingPolicy 是否加 caller_phase | 加 / 不加 | **加** (Option<OodaPhase>, 默认 None = 任意) | 按 phase 路由成为可能 |
| **D4** | LlmProfile 是否加 transport | 加 / 不加 (用 provider 推断) | **加** | 显式区分 HTTP vs CLI, 减少隐式 |
| **D5** | Resolver 错误回退 | panic / 静默回退 config.llm | **静默回退** | Plan 1 R1, 零回归 |
| **D6** | MCP server 端是否先做 | 是 / 否 | **是** (Phase D 第一批) | 验证抽离边界, 为 Cairn 外部 executor 铺路 |
| **D7** | LLM Reason 是否默认 Claude | 是 (推荐) / 否 (用户配置) | **是, 可降级** | R4 风险控制 |
| **D8** | 观测总线统一 | chat_turn_events / 单独 events 表 | **chat_turn_events** | 复用现有漏斗, 不开新总线 |

---

## 10. 附录 A: 任务来源对照表

| 合并后 ID | Plan 1 来源 | Plan 2 来源 | Cairn 来源 | 新增 |
|---|---|---|---|---|
| A1 | — | — | — | ✅ 重命名 |
| A2 | — | — | — | ✅ transport 字段 |
| A3 | — | — | — | ✅ caller_phase 字段 |
| A4 | H2 骨架 | M2 共享下沉 | — | ✅ resolver |
| B1 | H3 + H4 | — | — | — |
| B2 | H5 | — | — | — |
| B3 | — | — | P1 | — |
| B4 | — | — | P12 | — |
| B5 | — | — | P4 | — |
| C1 | H6 | — | — | ✅ 观测融合 |
| C2 | — | — | P3 | ✅ 接 resolver |
| C3 | — | — | P2 | — |
| C4 | — | — | P5 | — |
| D1 | — | — | P10 | — |
| D2 | H1 + H2 | M1 + M2 | — | ✅ 合并实现 |
| D3 | — | M3 + M4 | — | — |
| D4 | — | M3 延伸 | — | ✅ 新 executor |
| E1 | H7 | — | — | — |
| E2 | — | — | P4 UI | ✅ 融合 |
| E3 | — | — | P11 | — |

**统计**:
- Plan 1 任务 7 项 → 合并后保留 5 项 (B1/B2/C1/D2/E1), 2 项并入 A4/D2
- Plan 2 任务 6 项 → 合并后保留 4 项 (D2/D3/D4/F5/F6), 2 项并入 A4/D2
- Cairn 任务 12 项 → 合并后保留 11 项 (B3/B4/B5/C2/C3/C4/D1/E2/E3/F1-F4), 1 项 (P8) 与 E1 共享
- 新增 6 项 (A1/A2/A3/A4/C1/D4) 解决融合冲突

---

## 11. 附录 B: 关键文件改动预测

| 文件 | 改动 | 合并 ID |
|---|---|---|
| `crates/conductor-core/src/routing.rs` | TaskKind → WorkKind 重命名 | A1 |
| `crates/conductor-core/src/goal_orchestrator/phase.rs` | (新) OodaPhase 枚举 | A1 |
| `crates/conductor-core/src/llm_profiles.rs` | 加 transport 字段 | A2 |
| `crates/conductor-core/src/routing.rs` | RoutingPolicy 加 caller_phase | A3 |
| `crates/conductor-core/src/model_resolver.rs` | (新) CallerContext + ResolvedModel + resolve() | A4 |
| `crates/conductor-core/src/llm.rs` | LlmRequestConfig::from_resolved | H1 / A4 |
| `crates/conductor-core/src/send_v2.rs` | 主/force_final/recovery 走 resolver | B1 |
| `crates/conductor-core/src/summarizer.rs` | 走 resolver(Sense) | B1 |
| `crates/conductor-core/src/smart_monitor.rs` | 走 resolver(Sense) | B1 |
| `crates/conductor-core/src/agent_runs.rs` | spawn_claude 注入 --model | B2 |
| `crates/conductor-core/src/goal_orchestrator/mod.rs` | 加 graph_hash 防抖 | B3 |
| `crates/conductor-core/src/agent_runs.rs` | result_ref 反写 | B4 |
| `crates/conductor-core/migrations/000X_goal_hints.sql` | (新) hint 表 | B5 |
| `crates/conductor-core/src/turns.rs` | model.routed 事件 | C1 |
| `crates/conductor-core/src/goal_orchestrator/decide.rs` | decide_llm 接 resolver | C2 |
| `crates/conductor-core/src/agents/dispatcher/phase.rs` | (新) Bootstrap/Reason/Explore Driver | C3 |
| `crates/conductor-core/src/runtime_api.rs` | /v1/goals/{id}/graph API | C4 |
| `crates/conductor-core/src/agents/dispatcher/contracts.rs` | (新) JSON Schema 校验 | D1 |
| `crates/model-router-core/` | (新 crate) llm + routing + profiles 抽离 | D2 |
| `crates/model-router-mcp/` | (新 crate) MCP server loop | D3 |
| `crates/conductor-core/src/agents/dispatcher/executors/mcp.rs` | (新) McpRouter executor | D4 |
| `apps/desktop/src/windows/SettingsPanel.tsx` | Models 标签页 | E1 |
| `apps/desktop/src/windows/GoalConsole.tsx` | Reasoning tab | E2 |
| `apps/desktop/src/windows/GraphView.tsx` | (新) Blackboard 图视图 | E3 |

**总计**: 23 个文件改动, 其中 7 个新建。

---

## 12. 附录 C: 与现有文档的引用关系

| 文档 | 关系 |
|---|---|
| [Cairn架构对位与移植评估-20260604.md](./Cairn架构对位与移植评估-20260604.md) | P1-P12 是本文 B3-E3 的子集 |
| [hybrid-model-dispatch-inproject-plan-20260604.md](./hybrid-model-dispatch-inproject-plan-20260604.md) | H1-H7 是本文 A4 + B1-B2 + C1 + E1 的子集 |
| [hybrid-model-dispatch-mcpserver-plan-20260604.md](./hybrid-model-dispatch-mcpserver-plan-20260604.md) | M1-M6 是本文 D2-D4 + F5/F6 的子集 |
| [项目Agent架构状态机与治理范式-20260529.md](./项目Agent架构状态机与治理范式-20260529.md) | 状态机硬门禁, 不被本文破坏 |
| [多Agent共享工作区与自治Goal调度方案-20260530.md](./多Agent共享工作区与自治Goal调度方案-20260530.md) | Runtime API 基础设施, 被本文 B3/C4 复用 |
| [state-machine-lifecycle skill](../skills/state-machine-lifecycle/SKILL.md) | State Impact Card 模板, 用于每个任务派工 |

---

## 13. 一句话结论

> **三方案共享同一根(`routing.rs` + `llm_profiles.rs`);先解决 3 个命名/字段冲突(A1-A3),再让 `ModelResolver` 成为唯一收口(A4-B2);然后 Cairn 的 P3 (LLM Reason) 是把三方案价值最大化的杠杆, 它既消费混合模型路由,又驱动 Plan 2 MCP server 抽离。15 项任务,5-6 周可全部完成 Round 5-7。**
