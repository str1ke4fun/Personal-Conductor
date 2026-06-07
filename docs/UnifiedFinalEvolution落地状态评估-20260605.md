# UnifiedFinalEvolution 落地状态评估

> 评估日期: 2026-06-05  
> 对照文件: `docs/UnifiedFinalEvolution-20260604.md`  
> 上下文文件: `docs/HybridModel与Cairn融合方案-20260604.md`  
> 评估范围: 当前工作树实现。工作区存在大量未提交改动，本评估只判断当前文件状态，不代表已合入主干。

## 1. 结论总览

当前实现已经开始铺设 UnifiedFinal / HybridModel / Cairn 融合底座，但状态更接近 **Phase A0/A 的部分骨架 + 若干 B/C/D 前置对象提前出现**，不是可验收的完整落地。

核心判断:

| 维度 | 当前状态 | 结论 |
|---|---:|---|
| GB0: Phase A0 P0 修复 | 部分完成 | **未通过**。P0-2 主线程 LLM 续轮未实现为真实续轮，TC-P0 未形成通过证据。 |
| GB1: conductor-core 全测试绿 | 未完成 | **未通过**。`cargo test -p conductor-core` 编译失败。 |
| ModelResolver 统一收口 | 部分完成 | 有 resolver 骨架和部分调用点，但实际 endpoint/key/temperature 仍走全局 `config.llm`。 |
| OODA/Cairn Reason | 未落地 | `decide.rs` 仍是程序化决策，`contracts.rs` 未进入 OODA 主路径。 |
| MCP 抽离 | 未开始 | 未见 `model-router-core` / `model-router-mcp` crate，也未见 `McpRouter` executor。 |
| UI | 局部存在 | OODA timeline 存在；Models/profile CRUD、Reasoning tab、hint 注入 UI 未见。 |

治理建议: **暂停继续扩散 Phase B/C/D/E，先回到 A0 + A 的验收线。** 目前代码已经越过 A0 做了不少后续骨架，但 GB0/GB1 都未闭合，继续推进会扩大返工面。

## 2. 已落地部分

| 方案要求 | 当前证据 | 判断 |
|---|---|---|
| `WorkKind` / `OodaPhase` 命名拆分 | `crates/conductor-core/src/routing.rs` 已有 `WorkKind`、`OodaPhase`；`TaskKind` 仍保留为 alias。 | 部分落地。命名冲突已缓解，但 alias 说明迁移未收口。 |
| `RoutingPolicy.caller_phase` | `RoutingPolicy` / `CreateRoutingPolicyInput` / SQL select/insert 已包含 `caller_phase`。 | 部分落地。字段存在，但过滤链路不完整。 |
| `LlmProfile.transport` | `llm_profiles.rs` 和 DB schema 已加入 `transport`，并从 legacy `provider` 推导。 | 部分落地。字段已存在，但 provider 仍限制在旧枚举，profile 未成为请求配置唯一来源。 |
| `ModelResolver` | 新增 `model_resolver.rs`，定义 `CallerContext`、`ResolvedModel`、`resolve()`。`send_v2`、`summarizer` 有调用。 | 部分落地。resolver 返回内容不足，调用点没有真正按 profile endpoint/key 发起请求。 |
| A0-1 result_ref 写回 | `agent_runs.rs::finish_spawned_run` 会根据 `metadata_json.task_id` 写回 `goal_tasks.result_ref`。 | 部分落地。依赖 metadata 注入，`subagent.claude_p` 路径没有传 `task_id`。 |
| A0-3 AgentTeam 状态 | `finish_spawned_run` 会读取 `team_id` / `agent_id` 并更新 member status。 | 部分落地。仅覆盖 metadata 完整的 AgentRun，尚需 TC-P0-02 级别验证。 |
| A0-4 Tauri 事件 | desktop worker 调 `set_app_handle`，finish 路径 emit `agent_runs_changed`。 | 部分落地。natural finish 有事件，但需和 LLM 续轮、前端刷新端到端联测。 |
| `goal_hints` | `goal_hints.rs` 有 create/list/dismiss，DB 有 `goal_hints` 表；observe 和 graph API 读取 active hints。 | 后端部分落地。Runtime API 暴露创建/关闭 hint 的路由未见，前端注入 UI 未见。 |
| graph snapshot API | `runtime_api.rs` 已有 `GET /runtime/goals/{goal_id}/graph`，返回 facts/intents/hints/events，支持 format query。 | 部分落地。满足 C4/P5 的雏形，但尚未成为 LLM Reason 的输入链路。 |
| JSON contract | `goal_orchestrator/contracts.rs` 定义 Reason/Explore 输出校验并有单测。 | 局部落地。契约存在，但没有被 `decide.rs` 或 dispatch 主流程消费。 |
| OODA timeline UI | `apps/desktop/src/windows/OodaTimeline.tsx` 存在。 | 局部落地。只覆盖 timeline，不等于 Reasoning tab / hint / model 配置 UI。 |

## 3. 关键断点与边界

### 3.1 验证门禁未过

已执行:

```text
rtk cargo check -p conductor-core      # pass, warnings
rtk cargo check -p conductor-desktop   # pass, warnings
rtk cargo test -p conductor-core       # fail
```

`cargo test -p conductor-core` 失败点:

| 位置 | 错误 | 影响 |
|---|---|---|
| `crates/conductor-core/examples/workspace_projection_smoke.rs:153` | `ToolCallCreate` initializer 缺 `turn_id` | example 编译失败，说明 ChatTurn 字段扩展没有同步到示例。 |
| `crates/conductor-core/src/routing.rs:733` | `CreateRoutingPolicyInput` initializer 缺 `caller_phase` | routing 单测编译失败，直接阻断 GB1。 |

结论: 当前只能说 `check` 通过，不能说测试门禁通过。

### 3.2 A0-2 不是 UnifiedFinal 要求的主线程 LLM 续轮

UnifiedFinal 要求 `send_v2::continue_turn_with_subagent_result` 一类能力: subagent 完成后注入 tool_result，并触发主线程 LLM 续轮。

当前实现边界:

| 当前行为 | 缺口 |
|---|---|
| `agent_runs.rs::notify_turn_of_run_completion` 写 `subagent.completed` 事件。 | 这是事件记录，不是 LLM continuation。 |
| 该函数追加一条可见 assistant 摘要消息。 | 用户能看到结果，但不是“主线程 LLM 消费 subagent output 后主动生成”。 |
| 代码中仍有 TODO 指向后续升级为 LLM continuation。 | 说明实现者也承认当前只是 fallback。 |

结论: A0-2 未完成，因此 GB0 不成立；A0-4 事件即使存在，也缺少 A0-2 触发的新 turn_event 闭环。

### 3.3 `caller_phase` 字段存在，但路由没有按 phase 命中

当前 `RoutingPolicy` 有 `caller_phase`，但 `RoutingPolicyFilter` 未包含 `caller_phase`；`ModelResolver::resolve()` 调 `list_policies` 时只传 `work_kind`。

影响:

| 方案意图 | 当前风险 |
|---|---|
| Reason / Explore / Bootstrap 可按 OODA phase 选择不同模型。 | 同一 `work_kind` 下无法区分 phase，`caller_phase` 只是落库字段。 |
| TC-A-04: `RoutingPolicy.caller_phase = Some(Reason)` 过滤命中。 | 现有过滤接口不支持该验收。 |

结论: A3 只能算 schema 层落地，不能算行为落地。

### 3.4 `ResolvedModel` 不是唯一 LLM 请求配置来源

当前 `ResolvedModel` 只携带 `model_id`、`transport`、profile/policy/backend id 等，未携带 `api_base_url`、`api_key`、temperature 等完整请求配置。

调用边界:

| 调用点 | 当前行为 |
|---|---|
| `chat/send_v2.rs` | 调 resolver 后主要使用 `.model_id`；`LlmRequestConfig::from_config(&config.llm)` 仍从全局配置取 endpoint/key。 |
| `summarizer.rs` | 调 resolver，但请求配置仍由传入 config 生成。 |
| `smart_monitor.rs` | 仍直接 `LlmRequestConfig::from_config(&config.llm)`，未接 resolver。 |

影响: profile-specific endpoint/key routing 没有真正生效。也就是说，“模型 ID 可被解析”，但“这个模型用哪个 transport/provider/base_url/key 发请求”尚未统一。

### 3.5 OODA/Cairn Reason 还未进入 LLM 路径

`goal_orchestrator/decide.rs` 仍是程序化决策；没有 `decide_llm`，也没有通过 `ModelResolver` 选模型后渲染中文 Reason prompt。

已有但未接入的资产:

| 资产 | 当前状态 |
|---|---|
| `goal_orchestrator/contracts.rs` | 有 Reason/Explore JSON validation，但未被 OODA 主流程消费。 |
| `goal_orchestrator/observe.rs` | 会读取 `goal_hints`。 |
| `runtime_api.rs` graph snapshot | 可读图，但没有作为 Reason LLM 输入。 |

结论: Cairn P3 / UnifiedFinal C2 仍未落地；现在只是为后续 Reason 准备了局部数据结构。

### 3.6 ChatTurn ↔ GoalCycle anchor 缺失

UnifiedFinal 明确要求 `ChatTurn.goal_cycle_id`、`ChatTurn.agent_task_id`、`ChatTurn.goal_id` 等锚点，解决 turn 与 goal cycle 的双向追踪。

当前观察:

| 对象 | 当前状态 |
|---|---|
| `chat_turns` table / `ChatTurnRecord` | 未见 `goal_cycle_id`、`agent_task_id` 字段。 |
| `goal_runs.current_cycle_id` / `goal_cycles.last_graph_hash` | 相关字段已存在。 |
| `tool_calls.agent_task_id` | DB 中有该列，但不是 ChatTurn anchor。 |

结论: B1 的核心锚点未完成，ChatTurn 仍不能作为 GoalCycle/AgentTask 的 canonical 追踪面。

### 3.7 事件总线未统一到 ChatTurnEvent

`events.rs` 已有 `emit_model_routed`、`emit_ooda_phase_changed`，但它们走 audit/NDJSON 事件路径，不是 turn-scoped 的 `chat_turn_events`。

另外，`ModelResolver::resolve()` 只在 fallback 分支附近看到 `emit_model_routed`；命中显式 hint 或 policy/profile 的早返回分支没有稳定事件证据。

影响:

| 目标 | 当前缺口 |
|---|---|
| `model.routed` 对所有 resolver 分支可观测。 | 早返回可能无事件。 |
| turn 内事件统一写 `chat_turn_events`。 | 模型路由/OODA 事件仍在另一路径。 |
| UI / debug 能按 ChatTurn 回放完整过程。 | 缺 turn-scoped model/OODA 事件。 |

### 3.8 MCP 抽离未开始

当前未见:

| 方案对象 | 当前状态 |
|---|---|
| `crates/model-router-core` | 不存在。 |
| `crates/model-router-mcp` | 不存在。 |
| `model-router-mcp` server loop / 3 个 model.* tools | 不存在。 |
| `crates/conductor-core/src/agents/dispatcher/executors/mcp.rs` | 不存在。 |

已有 `mcp.rs` 客户端能力，但这不能替代 Plan 2 要求的独立 router MCP server，也不能替代 Cairn 外部 executor。

### 3.9 UI 只落了局部可视化

当前前端只看到 OODA timeline 相关文件。未见:

| UI 目标 | 当前状态 |
|---|---|
| Models tab / profile CRUD | 未见。 |
| Reasoning tab | 未见。 |
| hint 注入 UI | 未见。 |
| GraphView / Blackboard 图 | 未见。 |

结论: E2/E3 还没有到实现阶段。

### 3.10 桌面 worker 的 chat 执行路径是绕行，不是 A0 闭环

`apps/desktop/src-tauri/src/worker.rs::execute_goal_task_via_chat` 会创建临时 chat session 执行 goal task，并把结果写回 `chat:{message_id}`。

这个路径对用户可见执行有价值，但它和 UnifiedFinal §A0 的 `agent.start` / `claude -p AgentRun` / subagent 完成回注主线程 LLM 不是同一条验收链。建议把它标注为 interim fallback，不要用它替代 TC-P0-01/03。

## 4. Phase 状态矩阵

| Phase | 方案目标 | 当前状态 | 验收判断 |
|---|---|---|---|
| A0 | P0-1 result_ref、P0-2 LLM 续轮、P0-3 team 状态、P0-4 finish event | P0-1/P0-3/P0-4 有部分实现；P0-2 是 visible summary fallback | **未通过 GB0** |
| A | WorkKind/OodaPhase、transport、caller_phase、ModelResolver | schema/API 骨架存在；测试失败；phase 过滤与完整请求配置未闭合 | **未通过 GB1** |
| B | resolver 全调用点、ChatTurn anchor、graph_hash、goal_hints | graph_hash/hints 局部存在；anchor 和全调用点未完成 | 未完成 |
| C | LLM Reason、OODA phase driver、graph snapshot、事件统一 | graph snapshot 和 contracts 局部存在；Reason/driver/event bus 未完成 | 未完成 |
| D | model-router-core / model-router-mcp / McpRouter executor | 未见 crate 和 executor | 未开始 |
| E | Models UI、Reasoning tab、GraphView | OODA timeline 局部存在；其余未见 | 未完成 |
| F | 记忆/推理张力/高级治理 | 未见明确落地证据 | 未开始 |

## 5. 进一步派工清单

### T0: 先恢复测试门禁

| 编号 | 任务 | 验收 |
|---|---|---|
| T0-1 | 修复 `workspace_projection_smoke.rs` 中 `ToolCallCreate` 缺 `turn_id`。 | `cargo test -p conductor-core` 不再因 example 编译失败。 |
| T0-2 | 修复 `routing.rs` 测试中 `CreateRoutingPolicyInput` 缺 `caller_phase`。 | routing lib test 编译通过。 |
| T0-3 | 重新跑 `rtk cargo test -p conductor-core`，把失败从“编译失败”降到具体测试失败或全绿。 | 产出完整日志。 |

### A0: 关闭 P0 真链路

| 编号 | 任务 | 验收 |
|---|---|---|
| A0-1 | 明确所有 AgentRun 创建路径必须带 `metadata.task_id` / `team_id` / `agent_id`，尤其是 `subagent.claude_p`。 | TC-P0-01 能从 AgentRun natural finish 写回 `goal_tasks.result_ref`。 |
| A0-2 | 实现真实 `continue_turn_with_subagent_result`，由 finish 路径注入 tool_result 并触发主线程 LLM 续轮。 | TC-P0-03 通过；用户看到的是 LLM 生成的 assistant message，不是硬编码 summary。 |
| A0-3 | 为 AgentTeam member 状态和 team lifecycle 增加端到端测试。 | TC-P0-02 通过。 |
| A0-4 | finish event 增加 payload 并验证前端刷新。 | TC-P0-04/05 通过。 |
| A0-5 | 增加 DND/Asleep 防重与 `subagent_result_consumed` 类标志。 | TC-P0-07 通过，重复 finish 不会重复续轮。 |

### A: 收口路由语义

| 编号 | 任务 | 验收 |
|---|---|---|
| A-1 | 决定 `TaskKind` alias 的下线策略，至少新增 grep/check 防止新代码继续写 `TaskKind::`。 | TC-A-01 有自动化验证。 |
| A-2 | 给 `RoutingPolicyFilter` 增加 `caller_phase`，并在 SQL 中实现 `caller_phase IS NULL OR caller_phase = ?` 的优先级/匹配语义。 | TC-A-04 通过。 |
| A-3 | `CallerContext::GoalOrchestrator { phase, work_kind }` 进入 resolver filter。 | Reason / Explore 可命中不同 policy。 |
| A-4 | `ResolvedModel` 扩展为完整请求配置载体，或新增 `LlmRequestConfig::from_resolved`。 | chat/summarizer/smart_monitor 不再从全局 `config.llm` 取 profile-specific endpoint/key。 |
| A-5 | `emit_model_routed` 覆盖 resolver 所有返回分支。 | 显式 hint、policy 命中、fallback 都有事件。 |

### B/C: 接上 Cairn 主路径

| 编号 | 任务 | 验收 |
|---|---|---|
| BC-1 | 为 `chat_turns` 增加 `goal_cycle_id`、`agent_task_id`、`goal_id`，并补 create/query API。 | ChatTurn 可反查 GoalCycle/AgentTask。 |
| BC-2 | 把 `goal_hints` 暴露到 Runtime API: create/list/dismiss。 | 前端不需要直接依赖内部 DB helper。 |
| BC-3 | 实现 `decide_llm`: graph snapshot + hints + 中文 prompt + ModelResolver + contracts validation。 | TC-C-01/02/03/05 能跑。 |
| BC-4 | 把 `contracts.rs` 接入 OODA dispatch/review，非 JSON 输出显式 failed。 | TC-D-06 通过。 |
| BC-5 | 将 turn-scoped 的 model/OODA/tool 事件统一写入 `chat_turn_events`。 | UI 能按一个 turn 回放完整链路。 |

### D/E: 在 A/B/C 闭合后再推进

| 编号 | 任务 | 前置 | 验收 |
|---|---|---|---|
| D-1 | 新建 `model-router-core`，抽出 resolver/profile/policy 纯逻辑。 | A-2/A-4 | conductor 和 MCP server 共享同一 resolver 逻辑。 |
| D-2 | 新建 `model-router-mcp`，实现 `model.route` / `model.list` / `model.invoke`。 | D-1 | Claude/Codex 配置后 `tools/list` 可见。 |
| D-3 | 实现 `McpRouter` executor。 | D-2 | `PlannedTask(backend_kind=McpRouter)` 不走 CLI。 |
| E-1 | Models tab / profile CRUD UI。 | A-4 | UI 修改 profile 后实际影响 LLM 请求。 |
| E-2 | Goal Reasoning tab + hint 注入。 | BC-2/BC-3 | hint 写入后下次 Reason 可读取。 |
| E-3 | GraphView / Blackboard 图。 | BC-3 | 10 节点级图 < 1s 渲染。 |

## 6. 建议的下一步顺序

1. 先修两个编译断点，确保 `cargo test -p conductor-core` 能完整运行。
2. 不做 MCP / UI 扩展，先补 A0-2 真实 LLM 续轮；这是 UnifiedFinal 明确的 GB0 前置。
3. 完成 `caller_phase` 过滤和 `ResolvedModel` 完整请求配置，否则 ModelResolver 只是命名层收口。
4. 再接 `decide_llm`，让 graph snapshot / hints / contracts 真正进入 OODA 主路径。
5. 最后再抽 MCP server 和补 UI，因为它们依赖 resolver 与 OODA 语义稳定。

## 7. 当前核验命令结果

```text
rtk cargo check -p conductor-core
结果: 通过，48 warnings

rtk cargo check -p conductor-desktop
结果: 通过，11 warnings

rtk cargo test -p conductor-core
结果: 失败，2 errors / 53 warnings
关键错误:
- error[E0063]: missing field `turn_id` in initializer of `ToolCallCreate`
- error[E0063]: missing field `caller_phase` in initializer of `routing::CreateRoutingPolicyInput`
```

---

## 8. 评估缺漏补全(本节为本评估原版未覆盖,经对照 [UnifiedFinalEvolution-20260604.md](./UnifiedFinalEvolution-20260604.md) 后补齐)

> 本节为对照原方案后识别的缺漏。原评估的 §1-§7 结论**保持不变**,本节只补充治理与可执行性细节。

### 8.1 User Presence 状态机状态(原 §5)

| 对象 | 原始要求 | 当前实现 |
|---|---|---|
| `UserPresence` 枚举(Offline/Active/Idle/Away/Asleep/Dnd) | 一等对象, 影响主线程 LLM 续轮是否触发 | **未建模** |
| 6 种状态转换表 | 显式定义 | **未建模** |
| 状态变更事件 `user.presence.changed` | 应写 `chat_turn_events` | **未实现** |
| Agent 行为耦合(Offline/Asleep/Dnd 时 GoalCycle 暂停) | 主线程 LLM 跳过续轮 | **未实现**(但 A0-2 即将依赖这个) |
| 数据流 conductor-sense → presence_detector → L1 | 应进入 L1 总线 | **未实现** |

**影响**:
- A0-2 (LLM 续轮) 落地时, Dnd/Asleep 防护 (TC-P0-07) 没有 `resolve_presence()` 可调, 只能靠配置 hardcode, 不能"按真实状态"
- D13 决策(2s 抖动窗口)需要 presence 信息做"用户是否在等待"

**派工补强**: T0 后追加 P0-7(presence 最小可用), 由 presence_detector(新) + UserPresence 枚举(新) + `resolve_presence()` 函数组成, **0.5d**, 作为 A0-2 完成的真前置。

### 8.2 风险 R1-R10 核对(原 §13.1)

| 风险 | 当前观测 | 状态 |
|---|---|---|
| R1: WorkKind 重命名雪崩 | alias `TaskKind` 仍保留 | **已触发未解决** |
| R2: SubagentHandle 抽象侵入 | SubagentHandle 抽象在文档中存在, 但代码中无 `SubagentHandle` 实体 | 未触发(尚未做) |
| R3: LLM Reason 模型选错 | C2 未做, 风险尚未激活 | N/A |
| R4: MCP 抽离回归 | D2 未做 | N/A |
| R5: chat_turns 加 anchor migration 失败 | 字段尚未加, 风险未激活 | N/A |
| R6: runtime_events 消费方破坏 | 观测统一未做 | N/A |
| R7: Fix 2 LLM 续轮重复触发 | A0-2 尚未真实实现 | N/A |
| R8: AppHandle OnceCell 循环引用 | `set_app_handle` 已存在, 假设已用 OnceCell; 需 grep 确认 | **需 grep 验证** |
| R9: Member 状态机抖动 | A0-3 尚未完成(见 §3.6) | N/A |
| R10: P0 修复与 Phase A 命名冲突 | 已用 A0- 前缀, 暂无冲突 | **已解决** |

**结论**: 当前仅 R1 / R8 处于"已激活状态", R1 已有部分缓解(alias 兼容), R8 需补一个 grep 验证。

**派工补强**: T0-4(0.1d) `grep -rn "OnceCell<.*AppHandle" crates/` 确认 R8 落实。

### 8.3 D1-D14 决策落实状态(原 §14)

| 决策 | 当前落实 |
|---|---|
| D1: Plan 2 下沉 | **未落实**。未抽离 crate。 |
| D2: TaskKind → WorkKind + OodaPhase | **部分落实**。`WorkKind`/`OodaPhase` 存在, `TaskKind` 仍 alias 保留(待 R1 收口)。 |
| D3: RoutingPolicy.caller_phase | **schema 落实, 行为未落实**。见 §3.3。 |
| D4: LlmProfile.transport | **schema 落实, 兼容路径需确认**。从 legacy `provider` 推导, 但 provider 枚举是否已扩展含 `claude_cli`/`codex_cli`/`mcp_router`? **需 grep 验证**。 |
| D5: Resolver 静默回退 | **已落实**(resolver.rs 有 fallback 分支)。 |
| D6: MCP server 端先做 | **未开始**。 |
| D7: LLM Reason 默认 Claude | N/A(Reason 尚未做)。 |
| D8: chat_turn_events 统一 | **未落实**。audit/NDJSON 路径仍独立。 |
| D9: UserPresence 显式建模 | **未落实**(见 §8.1)。 |
| D10: SubagentHandle 统一抽象 | **未落实**。 |
| D11: P0 独立 Round 5.0 | **当前未遵守**。代码已部分扩散(见 §1 概述)。建议回到本决策: 先发 Round 5.0, 再开 Phase A+。 |
| D12: LLM 续轮同步触发 | **未落实**。A0-2 是 visible summary fallback(见 §3.2)。 |
| D13: Member 状态机 2s 抖动 | N/A(未实现)。 |
| D14: AppHandle OnceCell 弱引用 | **待 grep 验证**。 |

**派工补强**: T0-5(0.1d) `grep -n "TaskKind::" crates/` 计数残留; T0-6(0.1d) `grep -n "provider.*=.*\"" llm_profiles.rs` 验证 D4 兼容值。

### 8.4 GB0-GB5 全部状态(原 §13.2)

| 门禁 | 状态 | 说明 |
|---|---|---|
| **GB0** Phase A0 P0 修复 | ❌ 未通过 | P0-2 缺真实续轮(见 §3.2) |
| **GB1** conductor-core 全测试绿 | ❌ 未通过 | 2 编译错误(见 §3.1) |
| **GB2** Resolver fallback 3 分支单测 | ⏸ N/A | 处于 A4 阶段后, 当前 GB1 没过无法验证 |
| **GB3** Round 5 五项全过(B3+B4+B5+C1+C4+D1) | ⏸ N/A | B3/B5 局部存在, C1/C4 局部存在, B4 未完; 未到验收点 |
| **GB4** MCP server 端到端 | ⏸ N/A | D3 未开始 |
| **GB5** LLM Reason 决策质量 A/B | ⏸ N/A | C2 未开始 |

**结论**: 当前只有 GB0/GB1 是可检查的, 其余 4 个门禁需按 §11 顺序完成前置任务后才有意义。

### 8.5 30 项任务对位表(原 §11.1)

> 评估原版只有 Phase 级矩阵。本表按 30 项任务细粒度对位, 标注当前实际状态。

| ID | 任务 | 当前状态 | 状态细节 | 下一步 |
|---|---|---|---|---|
| A0-1 | result_ref 反写 | 🟡 部分 | `goal_tasks.result_ref` 写回有, 但仅覆盖 `metadata.task_id` 注入路径; `subagent.claude_p` 路径未传 task_id | T-A0-1 必做 |
| A0-2 | LLM 续轮 | ❌ 未真 | 仅 visible summary fallback, 无 continue_turn_with_subagent_result | T-A0-2 必做 |
| A0-3 | Member 状态机 | 🟡 部分 | finish_spawned_run 已写 status, 但未接 `team_all_members_done` 触发 lifecycle | T-A0-3 必做 |
| A0-4 | Tauri 事件 | 🟡 部分 | finish emit 有, 但需和 LLM 续轮端到端联测 | T-A0-4 必做 |
| A1 | WorkKind 重命名 | 🟡 部分 | alias 保留, R1 残留 | T-A-1 grep check |
| A2 | LlmProfile.transport | 🟡 部分 | 字段有, provider 兼容值需确认 | T0-6 grep |
| A3 | RoutingPolicy.caller_phase | 🟡 部分 | schema 有, filter 缺 | T-A-2 |
| A4 | ModelResolver 骨架 | 🟡 部分 | resolve() 有, 但 endpoint/key 缺(见 §3.4) | T-A-3 + T-A-4 |
| B1 | 全调用点走 resolver | ❌ 未完 | smart_monitor 仍 `LlmRequestConfig::from_config` | T-A-3 |
| B2 | agent_runs 注入 --model | 🟡 部分 | 部分路径有 | T-A-5 |
| B3 | ReasonCheckpoint | 🟡 局部 | graph_hash 局部存在 | T-B-1 |
| B4 | result_ref + anchor | ❌ 未完 | anchor 缺(见 §3.6) | BC-1 |
| B5 | goal_hints API+UI | 🟡 部分 | 后端有, API+UI 缺 | BC-2 + E-2 |
| C1 | 观测总线统一 | 🟡 部分 | 事件函数有, 但未走 chat_turn_events | BC-5 |
| C2 | LLM Reason 接 Resolver | ❌ 未做 | | BC-3 |
| C3 | OodaPhase Driver | ❌ 未做 | | 排期 C 阶段后 |
| C4 | graph snapshot API | 🟡 部分 | 路由有, 尚未成 LLM 输入 | BC-3 |
| D1 | JSON Schema 契约 | 🟡 局部 | contracts.rs 有, 未接 decide | BC-4 |
| D2 | model-router-core 抽离 | ❌ 未做 | | D-1 |
| D3 | model-router-mcp server | ❌ 未做 | | D-2 |
| D4 | McpRouter executor | ❌ 未做 | | D-3 |
| E1 | Models UI | ❌ 未做 | | E-1 |
| E2 | Reasoning tab | ❌ 未做 | | E-2 |
| E3 | GraphView | ❌ 未做 | | E-3 |
| F1-F6 | 后续 | ❌ 未做 | | 排期 F 阶段后 |

**统计**: 30 项任务, 🟡 11 项部分, ❌ 19 项未做(其中 4 项是 P0 必修, 15 项是 A-F 阶段正常未到)。

### 8.6 与 [Bug说明-Goal会话双Working与任务结果插位-20260601.md](./Bug说明-Goal会话双Working与任务结果插位-20260601.md) 的 cross-reference

> 评估原版未明确说"当前 P0 修复是否能解 Bug"。本节补充。

| Bug 根因 | 关联 P0 修复 | 当前修复状态 | 预期消除 |
|---|---|---|---|
| "任务结果插位" = AgentTask.status 永不变 accepted | P0-1 (result_ref) + P0-3 (Member 状态机) | 🟡 部分(A0-1/A0-3 已写 status, 但 lifecycle 推进未验) | **A0-1 + A0-3 全部完成后应自动消除** |
| "Goal 会话双 Working" = OODA 看到 task 状态 stuck | P0-1 + P0-3 | 同上 | 同上 |

**结论**: 严格执行 A0-1 + A0-3 + BC-1 修复后, [Bug说明-Goal会话双Working与任务结果插位-20260601.md](./Bug说明-Goal会话双Working与任务结果插位-20260601.md) **应**自动消失。验收方式: 修复前录屏 vs 修复后录屏, 同一 Goal, 同一 AgentTask 序列, 跑通且无"插位"现象。

**派工补强**: T0-7(0.2d) 录屏测试, 作为 Round 5.0 发布前的必跑冒烟。

### 8.7 LlmRequestConfig 字段缺漏清单(为 A-4 任务细化粒度)

| 字段 | 当前是否在 `ResolvedModel` | 当前是否被 `LlmRequestConfig::from_resolved` 使用 |
|---|---|---|
| `model_id` | ✅ 有 | ✅(send_v2 使用) |
| `transport` | ✅ 有 | ❌ 未注入 |
| `provider` | ❌ 缺(应补) | ❌ 仍走 `config.llm.provider` |
| `api_base_url` | ❌ 缺 | ❌ 仍走 `config.llm.base_url` |
| `api_key` | ❌ 缺(应可空, keyring 后由 resolver 注入) | ❌ 仍走 `config.llm.api_key` |
| `temperature` | ❌ 缺 | ❌ 仍走 `config.llm.temperature` |
| `max_tokens` | ❌ 缺 | ❌ 仍走 `config.llm.max_tokens` |
| `profile_id` | ✅ 有(用于事件 payload) | N/A |
| `policy_id` | ✅ 有(用于事件 payload) | N/A |
| `backend_id` | ✅ 有(用于事件 payload) | N/A |

**A-4 任务粒度**:
```rust
// 应当实现的 LlmRequestConfig::from_resolved
impl LlmRequestConfig {
    pub fn from_resolved(r: &ResolvedModel) -> Self {
        Self {
            provider: r.provider.clone(),
            model: r.model_id.clone(),
            base_url: r.api_base_url.clone(),
            api_key: r.api_key.clone(),
            temperature: r.temperature,
            max_tokens: r.max_tokens,
            transport: r.transport.into(),
            profile_id: r.profile_id.clone(),
        }
    }
}
```

**派工补强**: A-4 任务验收需增加"显式从 `ResolvedModel` 读取 `api_base_url` / `api_key` / `temperature` / `max_tokens`" 4 个字段的单元测试, 而非只验"model_id 正确"。

### 8.8 派工并行度分析

> 原 §5 派工清单按 T0 → A0 → A → BC → D → E 排序, 但**没说明并行度**。本节补全。

```text
T0-1 ─┐
T0-2 ─┼─→ T0-3 (cargo test 全绿)
T0-4 ─┤
T0-5 ─┤
T0-6 ─┘

T0-3 完成后:

   A0-1 (result_ref) ──┬─→ A0-3 (Member) ──→ T-A0-3 (e2e test)
                        │
                        ├─→ A0-2 (LLM 续轮) ──→ T-A0-2 (e2e test)
                        │     │
                        │     └─→ A0-4 (Tauri 事件) ──→ T-A0-4 (e2e test)
                        │
                        └─→ P0-7 (presence) ← 0.5d 串行, 必在 A0-2 之前
                                              (D13 决策 2s 抖动需要 presence)

A0-2 + A0-3 + A0-4 完成后:

   A-1 (alias 收口) ──┐
   A-2 (caller_phase 过滤) ──→  A-3 (GoalOrchestrator 调 resolver) ──→ A-4 (完整 LlmRequestConfig)
                                                                    │
                                                                    └─→ B1 (全调用点走 resolver)
```

**关键路径**:
- 串行强制顺序: T0-3 → A0-1 → A0-2 → A0-3/A0-4 → A-2 → A-3 → A-4 → B1
- **可并行**: T0-1 ~ T0-6(6 项, 半天内可并行完成); A0-3 与 A0-4(都在 A0-2 完成后, 可并行)
- **A0-3 与 A0-4 不依赖 A0-2 全部代码**, 仅依赖 A0-1 的"task_id → member_id 反查"工具函数

**派工补强**: 在 §5 派工清单中, **A0-3 和 A0-4 可由两个工程师并行**。这影响排期, 1.5d 排期实际可压到 1d。

### 8.9 AgentRun metadata 模式确认

> 评估原版 §2 提到 `metadata_json.task_id`, 但 UnifiedFinal §10.6 Fix 1 设计的是 `agent_runs.metadata.task_id`(JSON 字段), 需确认当前实现是否真的写到 `metadata_json` 列, 还是单独的 `task_id` 字段。

| 当前实现猜测 | 风险 |
|---|---|
| A) 单独 `task_id` 列 | 修复方案需调整: 不在 metadata JSON 写, 改写列 |
| B) JSON `metadata_json` 列 | 符合 UnifiedFinal 设计 |
| C) 混合(两个都有) | 数据可能不一致, 需以 JSON 为准, 单独列废弃 |

**派工补强**: T0-8(0.1d) `grep -n "metadata_json\|task_id" crates/conductor-core/migrations/` 查 migration 真实 schema; 同步查 `agent_runs.rs::finish_spawned_run` 实际写哪个字段。

### 8.10 chat_turns ↔ tool_calls 反查 SQL 现状

> 评估原版 §3.6 提到 ChatTurn anchor 缺, 但**没确认**已存在的反查 SQL 是否仍工作。

实际代码 [agent_runs.rs:368-408](../crates/conductor-core/src/agent_runs.rs#L362-L408) `notify_turn_of_run_completion` 写的 SQL 是:

```sql
SELECT tc.turn_id, ct.request_id
FROM tool_calls tc JOIN chat_turns ct ON ct.id = tc.turn_id
WHERE tc.agent_run_id = ?1
LIMIT 1
```

**这是合理的反查路径**, 锚点是 `tool_calls.agent_run_id → tool_calls.turn_id → chat_turns.request_id`, **不依赖** `chat_turns.goal_cycle_id` / `agent_task_id` 等 anchor。

**结论**:
- B1 (ChatTurn ↔ GoalCycle anchor) 是 **B 类问题**(GoalCycle 反查 ChatTurn 用), 不影响 A0-2 (AgentRun → ChatTurn 反查)
- §3.6 把 B1 列为 P0 是**误判**, 应降级为 P1
- 派工 BC-1 仍要做, 但**不阻塞** A0-2 完成

**派工补强**: T0-9(0.1d) 在评估 §3.6 标题后加注"anchor 缺失影响 GoalCycle→ChatTurn 反查, 不影响 AgentRun→ChatTurn 反查"。

### 8.11 LlmProfile 存量与 provider 兼容

> 评估原版 §2 提到 LlmProfile.transport 已加入, 但**没**核对:
> 1. DB 中现有 LlmProfile 记录数
> 2. provider 字段当前支持值是否含 `claude_cli` / `codex_cli` / `mcp_router`

**派工补强**: T0-10(0.1d) `sqlite3 .tables llm_profiles` 查存量; `grep -n "provider.*=.*\"" llm_profiles.rs` 查支持值。若存量 > 0, migration 需保证 transport 字段可空(默认从 provider 推导); 若支持值仅 `openai|anthropic|local`, D4 决策的 `claude_cli` / `codex_cli` / `mcp_router` 需扩 provider 枚举或新建 `kind` 字段。

### 8.12 评估证据补全(可复现性)

> 评估原版 §7 给出了 cargo test 失败信息, 但**没**列:
> 1. 当前测试用例总数(应能跑通的)
> 2. 评估日期前最后一次全绿 commit
> 3. 与本评估的差异点

**派工补强**: T0-11(0.1d) 跑 `cargo test -p conductor-core --no-run 2>&1 | grep "test result"` (即使有 compile error 也能拿到已编译测试数); 把数字加到 §7。

### 8.13 §6 建议的最小可行发布路径(增量)

> 评估原版 §6 给了 5 步建议。本节从中抽出**最小可行发布路径**(MVP-Round-5.0):

**MVP-Round-5.0 完成定义**(只含以下, 其它不动):
1. T0-1, T0-2 (修两编译错误) ✓
2. T0-3 (cargo test -p conductor-core 编译通过 + 全绿) ✓
3. T0-4, T0-5, T0-6 (grep 验证 D8/D4/R8) ✓
4. T0-7 (录屏冒烟: Bug 不复现) ✓
5. A0-1 (result_ref 真链路) ✓
6. A0-2 (LLM 续轮真链路) ✓
7. A0-3 (Member 状态机真链路) ✓
8. A0-4 (Tauri 事件真链路) ✓
9. P0-7 (presence 最小可用, 作为 A0-2 真前置) ✓
10. BC-1 anchor 同步解决(可选, 若 §8.10 判断有依赖)

**MVP-Round-5.0 完成时**:
- GB0 通过
- GB1 通过
- [Bug说明-Goal会话双Working与任务结果插位-20260601.md](./Bug说明-Goal会话双Working与任务结果插位-20260601.md) 自动消除
- 不修复 P0-2 的 visible summary fallback(回退到当前 hardcoded summary)

**MVP-Round-5.0 之后**才进入 Phase A 命名+Resolver+CallerContext 收口。

### 8.14 评估自查清单(本节也是)

| 自查项 | 状态 |
|---|---|
| 评估覆盖了 P0 修复 | ✓ §2-§3 |
| 评估覆盖了 Phase A 命名/字段 | ✓ §3.3, §3.4 |
| 评估覆盖了 OODA 决策 | ✓ §3.5 |
| 评估覆盖了 MCP 抽离 | ✓ §3.8 |
| 评估覆盖了 UI 任务 | ✓ §3.9 |
| **评估覆盖了 User Presence 建模** | ❌ 漏(§8.1) |
| **评估覆盖了 10 项风险核对** | ❌ 漏(§8.2) |
| **评估覆盖了 14 项决策落实** | ❌ 漏(§8.3) |
| **评估覆盖了 5 项硬门禁** | ❌ 部分漏(§8.4) |
| **评估覆盖了 30 项任务细粒度** | ❌ 漏(§8.5) |
| **评估覆盖了与 Bug 文档溯源** | ❌ 漏(§8.6) |
| **评估覆盖了 LlmRequestConfig 字段缺漏** | ❌ 漏(§8.7) |
| **评估覆盖了派工并行度** | ❌ 漏(§8.8) |
| **评估覆盖了 AgentRun metadata 模式** | ❌ 漏(§8.9) |
| **评估覆盖了反查 SQL 现状** | ❌ 漏(§8.10) |
| **评估覆盖了 LlmProfile 存量** | ❌ 漏(§8.11) |
| **评估覆盖了证据可复现性** | ❌ 漏(§8.12) |
| **评估给出了 MVP-Round-5.0 路径** | ❌ 漏(§8.13) |

**评估完整性**: 17 项自查, 9 项覆盖, 8 项**本节补全**。

---

## 9. 评估加强总结

本评估原版 §1-§7 结论**全部成立**,本节 (§8) 补充了治理与可执行性细节。补全后:

1. **风险有 3 项需 grep 验证**: R1 / R8 / D4 兼容
2. **决策有 11 项未落实**: D1/D2 alias/D3 行为/D4 兼容/D6/D7/D8/D9/D10/D11/D12(其中 D2 alias/D3 行为/D4 兼容/D8/D11/D12 处于"可立即处理"状态)
3. **MVP-Round-5.0 路径明确**: 11 项任务, 3-4d, GB0+GB1 同时通过, Bug 自动消除
4. **派工并行度可优化**: A0-3 与 A0-4 可并行, 排期 1.5d 压到 1d
5. **决策 D11 (独立 Round 5.0) 未遵守**: 当前代码已部分扩散, 建议立即回滚未完成部分, 先发 Round 5.0

**最终建议**: 严格按 §8.13 MVP-Round-5.0 路径执行, **不要继续扩散** Phase B/C/D/E。评估原版 §1 的"暂停扩散"判断 + 本节的"先发 Round 5.0"判断, 共同构成下一阶段的工作方针。

