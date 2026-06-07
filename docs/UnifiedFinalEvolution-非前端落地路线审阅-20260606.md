# UnifiedFinalEvolution 非前端落地路线审阅

> 日期: 2026-06-06  
> 范围: 对 `UnifiedFinalEvolution` / `HybridModel + Cairn` / goal-task-flow / 清和记忆系统中**除前端视觉设计外**的落地路线复核。  
> 口径: 以当前工作树源码与本地可执行检查为准；8 份输入文档作为需求与历史声明，不作为完成证据。当前工作区存在大量未提交改动，本审阅只代表当前文件状态。

输入文档:

- `docs/UnifiedFinalEvolution-goal-task-flow-dispatch-20260605.md`
- `docs/UnifiedFinalEvolution落地交接-20260605.md`
- `docs/UnifiedFinalEvolution落地状态评估-20260605.md`
- `docs/桌宠记忆系统持久化与感知分层设计-20260604.md`
- `docs/UnifiedFinalEvolution-20260604.md`
- `docs/HybridModel与Cairn融合方案-20260604.md`
- `docs/派工-Round5-清和系统三文档融合落地.md`
- `docs/清和系统三文档融合设计-20260605.md`

## 1. 总结结论

当前代码已经不是早期文档中“只有骨架”的状态：`ChatTurn` 锚点、`ModelResolver`、phase-aware routing、`dispatch_plans` / `route_decisions`、LLM Reason、goal hints、Graph snapshot、memory chunks/embeddings、MCP router crate、桌面 IPC 入口等都已经进入源码。

但也不能按交接文档直接判定为“UnifiedFinalEvolution 已完整落地”。真实状态更准确地说是:

```text
Goal/OODA 主链路:         基本可跑，依赖 goal_tasks 状态推进，仍缺强端到端门禁
ChatTurn 锚点/事件:       主 chat 与 goal-task executor 已接，但观测字段仍有漂移
ModelResolver:            已成为主要 LLM 配置入口，但审计持久化是 best-effort
AgentTeamMember:          有成员表和事件，但状态语义不足，chat executor 未真正驱动 member 运行/完成态
MCP Router:               crate 和 executor 有入口，但 model.invoke 不可用，仍是静态 route/list 骨架
Memory:                   entries/summaries -> chunks/embeddings -> recall -> prompt 基本打通，需验证真实 provider 与脱敏策略
测试门禁:                 `cargo check` 通过，关键窄测通过；完整 `conductor-core --lib` 与 `tick_goal` 测试在 124s 超时
```

建议把当前路线重新定级为 **“主链路接通后的收口与验收阶段”**，不要继续扩散 F 阶段或更复杂的多 agent 语义，先补齐可观测一致性、成员生命周期语义、MCP 真实调用、端到端验收与死代码清理。

## 2. 当前真实数据流

### 2.1 Goal -> OODA -> AgentTask -> Chat Executor

当前主路径已经存在:

1. `goal_orchestrator::tick_goal` 在 `dispatching` 阶段先尝试 `decide_llm()`，失败后 fallback 到程序化 `decide()`。
2. `DispatchPlan` 会写入 `dispatch_plans`，并回填 `goal_cycles.dispatch_plan_id`。
3. `act()` 创建 `goal_tasks`，同时创建 `team-{cycle_id}` 与 `agent_team_members`，member metadata 写入 `task_id/goal_id/cycle_id`。
4. 桌面 worker 监听 `state/exec-signals/*.exec`，按 `agent_kind` 分流到 chat executor 或 mcp router executor。
5. `execute_goal_task_via_chat()` 创建隔离执行 session，通过 `send_message_v2_with_session_projection_ctx()` 执行，并把结果投影回 goal session。
6. `ChatExecutionContext` 会把 `goal_id / goal_cycle_id / agent_task_id` 写入 `chat_turns`。
7. worker 根据工具/能力请求结果把 `goal_tasks` 置为 `review_ready` 或 `blocked`。
8. `tick_executing()` 自动接受 `review_ready` task；无进行中/阻塞任务时推进到 `awaiting_review` 与 cycle `reviewing`；`tick_reviewing()` 在全部终态且无失败时调用 `apply_goal_review_verdict(..., true)`。

这条链路的核心业务状态是通过 `goal_tasks` 推进的，能解释为什么用户层面“任务跑完了”。它不是完全死链。

### 2.2 ChatTurn 与事件链路

`chat_turns` schema 已包含 `goal_cycle_id / agent_task_id / goal_id`，`ChatTurnCreate` 与 `send_v2` 已写入这些字段。`ModelResolver::resolve_with_request()` 会写 `model.routed` 到 audit event，并在有 `request_id` 时镜像到 `chat_turn_events`。

不足是事件总线仍不是严格统一:

- OODA phase event helper 支持 turn-scoped 写入，但 `goals::advance_cycle_phase()` 仍主要写旧式 `events::append("goal", ...)`。
- goal-task 事件目前写 audit event，`goal_task.result_projected` 没带 projected message id，且没有写回 `chat_turn_events`。
- `send_v2` 的部分 stage payload 仍记录 `config.llm.provider/model`，而实际请求已用 `ResolvedModel`，观测数据可能和真实模型不一致。

### 2.3 ModelResolver / 路由

已落地:

- `ResolvedModel` 包含 `provider/api_base_url/api_key/temperature/max_tokens`。
- `LlmRequestConfig::from_resolved_with_fallback()` 已被 chat/summarizer/smart_monitor/decide_llm 使用。
- `RoutingPolicyFilter.caller_phase` 已实现 null-or-match 语义。
- resolver 5 个窄测通过。
- `route_decisions` 持久化存在。

仍需收口:

- `persist_route_decision()` 的错误被 `let _ = ...` 吞掉，审计缺失不会影响主链路，也不会暴露给测试。
- route_decisions 中 `workspace_id` 固定写空字符串，`backend_id` 直接复用 `backend_kind`，只能算弱审计。
- hint override 返回 `BackendKind::ClaudeP`，没有从 profile/catalog 推断 provider 配置，适合临时 hint，不适合作为长期治理入口。

### 2.4 AgentTeam / AgentTeamMember

这里是当前最大设计偏差。

`AgentTeam` 生命周期本身会由 `sync_team_lifecycle()` 跟随 goal task 推到 `AwaitingReview`。但是 `AgentTeamMember` 的状态枚举只有:

```text
active / paused / stopped
```

没有 `running`、`completed`、`failed`、`blocked` 这样的执行态。`act()` 创建 member 时状态就是 `active`；chat executor 完成 task 后也没有调用 `find_member_by_task_id()` 或 `set_member_status()`。因此:

- UI 不能从 member 行判断“尚未执行 / 执行中 / 已完成”。
- `active` 同时表示“可用成员”和“完成后仍活跃”，语义混淆。
- `find_member_by_task_id()` 是已写但主路径未用的入口。
- AgentRun finish 路径会更新 member，但 chat executor 是当前 goal task 的主要路径，覆盖不到。

这不是 Goal 完全卡死的问题，而是 Team/Member projection 与调度语义没有真正落地。

### 2.5 MCP Router

`model-router-core` / `model-router-mcp` crate 已存在，`model-router-mcp` 可 `cargo check`。桌面 worker 对 `agent_kind == "mcp_router"` 有分支。

但当前 MCP router 仍是静态、半闭环:

- `model.route` / `model.list` 是静态表，无 conductor DB / profile / policy 集成。
- `model.invoke` 明确返回 “requires conductor-core runtime” 错误。
- worker 的 `execute_goal_task_via_mcp_router()` 只调用 `model.route`，把 stdout 截断写成 `mcp_router:{work_kind}:...` 的 `result_ref`，没有执行真实模型调用，也没有投影到 chat timeline。
- 找不到 binary 时会降级为 `"unavailable"`，但仍把 task 置为 `review_ready`。

所以 D 阶段只能算“router server 与 executor stub 有入口”，不能算“外部 MCP 子代理后端真正可用”。

### 2.6 Memory / 感知数据底座

这条链路已经明显推进:

- `memory_entries` 写入后会触发 `index_memory_entry()`。
- `add_conversation_summary()` 会触发 `index_conversation_summary()`。
- `reindex_memory_chunk()`、`rebuild_embeddings()`、`memory_rebuild_embeddings` IPC、`MemoryPanel` 入口都存在。
- `chat/prompt.rs` 调用 `recall_for_prompt_with_context()`，把 memory/summary recall 注入 system prompt。
- `summarizer::maybe_auto_summarize()` 会调用 `memory::add_conversation_summary()`，摘要能进入 conversation_summaries 并被索引。

边界:

- `0002_memory_rebuild.sql` 只 backfill chunks，不生成 embeddings；真实补全依赖 Rust `memory_rebuild_embeddings()` 手动/IPC 入口。
- 默认全局 provider 仍可能是 hash fallback；真实语义召回质量要靠环境里 embedding 服务验证。
- Secret/Private 有 schema 与搜索过滤能力，但审计/面板默认脱敏没有在本次做完整 UI/运行态核验。
- L2 主动气泡数据入口已有 `pet_self_bubble` event，但是否形成“summary -> persona 改写 -> SelfBubble queue”的完整体验，不在本次后端证据中证明。

## 3. 死代码与未打通入口

| 位置 | 判断 | 影响 |
|---|---|---|
| `agent_teams::find_member_by_task_id()` | 已实现但未被主路径调用 | P0-C 声明的“chat executor 驱动 member state”没有真正闭合 |
| `worker.rs::goal_task_projection_placeholder_content()` / `emit_goal_task_projection_started()` | 保留但当前被注释为 suppress placeholder，未调用 | 旧投影方案残留；容易误导后续维护 |
| `chat::turns::get_turn_by_goal_cycle_id()` | 未使用 | 需求是双向锚点，但目前查询入口未进入业务/read model |
| `model-router-mcp::handle_invoke()` | 明确返回不可用 | MCP server 对外暴露的三工具中一个是占位能力 |
| `AgentTeamMember.status = active` | 不是死代码，但语义退化 | “active” 承担 idle/running/completed 多重含义 |
| `route_decisions` 持久化 | 写入是 best-effort 且字段弱 | 看起来有审计表，但无法作为强验收证据 |

## 4. 与设计不符的部分

1. **SubagentHandle 抽象未真正统一三类子线程**  
   文档要求 AgentRun 进程类、McpRouter 进程外、AgentTeam 多 agent 统一到一个抽象。当前仍是 `AgentRun` finish、chat executor、mcp_router executor 三条分支，各自写回逻辑不同。

2. **ChatTurn 锚点已补，但不是所有下游都按锚点查询**  
   字段进入 schema 和 write path，但 Goal/Graph/Team read model 仍主要依赖 `goal_tasks`、`goal_cycles`、session projection，`get_turn_by_goal_cycle_id()` 未进入主路径。

3. **事件统一只完成了一部分**  
   `model.routed` 能 mirror 到 `chat_turn_events`，但 OODA/goal_task/team 仍以 audit event 和旧 append 为主。现状是“双总线并行”，不是文档要求的统一事件回放。

4. **MCP 抽离被实现为静态路由服务，不是共享 ModelResolver**  
   `model-router-mcp` 没有接 `llm_profiles` / `routing_policies` / `ModelResolver`，所以 Plan 1 与 Plan 2 还没有真正共用一套路由治理。

5. **UserPresence 只有 DND guard，没有完整状态机输入**  
   `blocks_llm_continuation()` 被 AgentRun continuation 使用，但 Offline/Active/Idle/Away/Asleep 的真实传感器输入与全局调度约束没有形成统一 runtime policy。

6. **AgentTeamMember 状态机与设计不符**  
   设计需要成员执行态驱动 team/review；当前成员状态不表达执行完成，team 主要由 goal task 状态同步。

## 5. 更优落地路线

### P0: 先补强验收与语义缺口

1. 为 `execute_goal_task_via_chat()` 接入 `agent_teams::find_member_by_task_id()`：开始时写 `run_id = chat:<request_id 或 turn_id>`，完成时写明确状态。建议把 `AgentMemberStatus` 扩展为 `idle/running/completed/blocked/failed/stopped`，不要继续复用 `active`。
2. 将 goal-task 的 `execution_started/result_projected/writeback_failed` 在有 `request_id` 时也写入 `chat_turn_events`，payload 带 `goal_id/cycle_id/task_id/result_ref/projected_message_id`。
3. `route_decisions` 写入失败应至少 warning，并在测试中验证 `workspace_id/task_id/policy_id/profile_id/fallback_used`。
4. 修正 `send_v2` stage payload，记录 `resolved_model.*`，避免 UI/debug 看到旧 config。
5. 把完整测试门禁拆开：`model_resolver`、`chat::turns`、`memory recall`、`goal tick no-network deterministic`、`worker writeback` 分组，避免 `tick_goal` 类测试长期超时掩盖真实结果。

### P1: 收敛子线程抽象

把 `GoalTaskExecutor` 抽成 trait:

```text
execute(task) -> ExecutionHandle { kind, handle_id, turn_request_id?, agent_run_id?, mcp_session_id? }
complete(handle) -> TaskWriteback
emit_events(handle, state)
```

然后让 chat executor、AgentRun executor、McpRouter executor 共享:

- task status 写回
- member run binding
- ChatTurn/audit event 写入
- result projection
- blocked/review_ready 判定

这样比继续在 `worker.rs` 里堆分支更适合 UnifiedFinal 的长期形态。

### P2: MCP Router 接真实治理

二选一:

- **短期**: 删除/隐藏 `model.invoke` 工具，只承诺 `model.route/list`，避免对外暴露不可用工具。
- **长期**: 把 `model-router-core` 变成真正共享 crate，抽出 profile/policy store trait；conductor-core 和 model-router-mcp 分别实现 store。MCP server 的 route/list/invoke 都走同一 resolver 语义。

### P3: Memory 从“能召回”升级到“可信可控”

1. 启动时检测 chunks without embeddings，给出明确 health 状态，而不是只靠用户手动点 rebuild。
2. 在 `MemoryPanel` 和事件面板统一使用 sensitivity 脱敏组件。
3. 为 `recall_for_prompt_with_context()` 增加真实 DB fixture，覆盖 workspace/path/session/goal/sensitivity 组合。
4. 把 `pet_self_bubble` 的 payload 明确为 story event，而不是普通 chat message。

## 6. 验证记录

本次实际执行:

```text
rtk cargo check -p conductor-core                         PASS, 50 warnings
rtk cargo check -p conductor-desktop                      PASS, 15 warnings
rtk cargo check -p model-router-mcp                       PASS
rtk cargo test -p conductor-core model_resolver --lib     PASS, 5 tests
rtk cargo test -p conductor-core chat::turns --lib        PASS, 3 tests
rtk cargo test -p conductor-core decide --lib             PASS, 6 tests
rtk cargo test -p conductor-core --lib                    TIMEOUT at 124s
rtk cargo test -p conductor-core goal_orchestrator --lib  TIMEOUT at 124s
rtk cargo test -p conductor-core tick_goal --lib          TIMEOUT at 124s
```

因此不能把 GB1 写成“全绿”。当前能证明的是核心 crate 可编译、部分关键单元测试通过；不能证明完整 goal runtime 端到端验收已经完成。

## 7. 审阅结论

真正的落地路线应从“继续实现更多设计点”改为“把已经出现的多条半闭环收束成一条可审计主路径”。

当前最应该守住的 canonical 数据流是:

```text
GoalCycle
  -> DispatchPlan
  -> AgentTask
  -> ExecutionHandle(chat / agent_run / mcp)
  -> ChatTurn(anchor)
  -> GoalTask writeback
  -> AgentTeamMember execution state
  -> AgentTeam lifecycle
  -> chat_turn_events + audit_events
  -> read model / UI projection
```

现在 `GoalCycle -> DispatchPlan -> AgentTask -> chat -> goal_tasks -> goal lifecycle` 已基本打通；缺的是 `ExecutionHandle` 统一抽象、AgentTeamMember 状态语义、事件总线一致性、MCP 真实执行能力和完整门禁证据。
