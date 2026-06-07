# Docs 文件索引

> 最后更新: 2026-06-07 | 由文档同步助手维护（代码细读同步）
> 上一次扫描: 2026-06-06（Goal 会话 + Shell 权限 + 推理图收口），详见 [wiki/18-维护记录-权限与推理图-20260606.md](../wiki/18-维护记录-权限与推理图-20260606.md)
> 本次同步: 2026-06-07（Graph Snapshot 字段补齐 + `tick_goal` + `goal_hints` + `chat/turns.rs` 模块化），详见 [wiki/19-维护记录-代码同步与图快照-20260607.md](../wiki/19-维护记录-代码同步与图快照-20260607.md)
> 同步状态详情见 [文档同步状态-20260527.md](文档同步状态-20260527.md)
> Release 包见 [release/INDEX.md](../release/INDEX.md)

---

## 近期代码变更 (自动检测)

> 扫描时间: 2026-06-07 | 增量扫描 | 已对齐：Graph Snapshot `chat_turn_request_ids`/`chat_turns` 字段、`GoalOrchestrator::tick_goal` + observe 防抖、`commands.rs::get_goal_graph` 真实实现、`goal_hints` 模块、`chat/turns.rs` 模块化、`model_resolver` / `user_presence` 新增模块。
> 历史快照: 2026-05-29 ~01:30 | Round 4 已实现 | 47 个工具注册 | 65 个前端 invoke / 62 个后端 command | 234 测试全通过 | AUDIT-005 工作区路径感知 | AUDIT-007 会话隔离修复 | AUDIT-008 桌宠状态协调器 | 会话切换端到端打通

### 已验证修复 (外部 agent 完成)

| 修复项 | 验证结果 |
|--------|----------|
| LLM 工具白名单配置化 | ✅ chat.rs:246 从 `config.llm.allowed_tool_ids` 读取 |
| persona.rs 接入 chat | ✅ chat.rs:392-404 `build_system_prompt` 注入人格特质 |
| MultiAgentBacklog 触发器 | ✅ initiative.rs:268 调用 `proposals::list_pending()` |
| EventBased 触发器注册 | ✅ initiative.rs:67-68 注册 `handle_event_based` |
| ToolResultSentiment 移除 | ✅ expression.rs 中已清除 |

### 代码变更

| 文件 | 变更类型 | 影响文档 |
|------|----------|----------|
| `conductor-core/src/smart_monitor.rs` | **新增** (409 行) | 当前架构说明、交接文档-渲染优化 |
| `conductor-core/src/chat.rs` | 修改 (1991 行) | 角色表达与智能增强 |
| `conductor-core/src/llm.rs` | 修改 (1255 行) | 技术栈选型 |
| `conductor-core/src/lib.rs` | 修改 (新增 smart_monitor 模块) | 当前架构说明 |
| `desktop/src-tauri/src/worker.rs` | 修改 (432 行) | 当前架构说明、交接文档-渲染优化 |
| `conductor-core/src/expression.rs` | 修改 (685 行) | 角色表达与智能增强、表情状态机与资产规划 |
| `conductor-core/src/tools.rs` | 修改 (3555 行) | claude-code-porting-mapping |
| `conductor-core/src/avatar.rs` | 修改 (647 行) | 感知层增强、表情状态机与资产规划 |
| `conductor-core/src/workspaces.rs` | **新增** (386 行) | 当前架构说明 |
| `conductor-core/src/mcp.rs` | 重写 (974 行) | 技术架构、AUDIT-004 |
| `conductor-core/src/codex.rs` | 修改 (756 行) | codex session 管理从 stub 变为真实实现 |
| `conductor-core/src/goal_hints.rs` | **新增** (2026-06-07) | wiki 05 §5.8.1、wiki 19 |
| `conductor-core/src/chat/turns.rs` | **新增** (2026-06-07，独立模块化) | wiki 14 §14.5a、wiki 19 |
| `conductor-core/src/model_resolver.rs` | **新增** (2026-06-07) | wiki 19 跟进项 |
| `conductor-core/src/user_presence.rs` | **新增** (2026-06-07) | wiki 19 跟进项 |
| `conductor-core/src/goal_orchestrator/mod.rs` | 修改 (2026-06-07，新增 `tick_goal` + `compute_observe_hash` 防抖) | wiki 05 §5.8、wiki 14 |
| `conductor-core/src/runtime_api.rs` | 修改 (2026-06-07，`GoalGraphSnapshot` 扩展 `chat_turn_request_ids`/`chat_turns` 字段) | wiki 13、wiki 14 §14.5a |
| `apps/desktop/src-tauri/src/commands.rs` | 修改 (2026-06-07，`get_goal_graph` 改为真实实现) | wiki 14 §14.5a |

**`smart_monitor.rs` 说明**: LLM 驱动的智能监控决策器，替代原 `notify_decider.rs` 的硬编码逻辑。通过 gather_context() 收集告警/任务/心情/好感度/对话上下文，调用 LLM 判断是否通知用户及通知方式。含 fallback 降级逻辑。`notify_decider.rs` 已成为死代码。

**`expression.rs` 变更**: 新增 `IdlePhase` 枚举 (`Idle1Min`/`Idle5Min`/`Idle30Min`)，含 `from_idle_seconds()` 和 `decay_multiplier()` 方法。解决了之前"文档中存在但代码中不存在"的问题。

**`tools.rs` 变更**: 47 个工具已注册（含 `web.fetch`/`config.get`/`todo.write`）。3555 行。

**`codex.rs` 变更**: session 管理从 stub 变为真实实现（756 行）。`send_input`/`interrupt_session`/`resume_session`/`stop_session`/`list_sessions` 均已实现。新增 BUG-008（test_read_output_with_offset panic）。

**`avatar.rs` 变更**: `avatar_events` 表已实现 (建表、索引、INSERT、查询)，解决跨文档问题清单中"代码中不存在"的问题。

### Tauri 命令审计 (2026-05-28 ~14:00 增量扫描)

| 指标 | 数值 |
|------|------|
| 前端独立 invoke 命令数 | 65 |
| 后端 #[tauri::command]/#[command] 定义数 | 62 (commands.rs 59 + main.rs 3) |
| generate_handler 注册数 | 62 |
| 内置工具注册数 | 47 |
| 测试总数 | 229 passed / 1 failed (BUG-008) |
| conductor-core 代码行数 | 21,320 (40 文件) |
| 前端代码行数 | 3,699 (20 文件) |

### Release 准备

已创建 `release/` 目录，包含：
- `INDEX.md` — Release 说明和 MVP 功能范围
- `打包清单.md` — 打包步骤和验证清单
- `路线索引-20260527.md` — 发布路线总览
- `state-template/config.json` — 配置模板 (API key 已替换为占位符)
- `resources/` — Live2D Core + Hiyori Pro runtime + 桌宠形象素材
- `docs/` — 核心文档副本 (8 个文件)

---

## 文件树

```
docs/
├── INDEX.md                          ← 本文件 (文档索引)
├── 文档同步状态-20260527.md           ← 代码-文档同步审查报告
│
├── 📐 架构与规划
│   ├── 技术架构.md                    ✅ 完全同步
│   ├── 技术栈选型.md                  🟡 部分完成
│   ├── 当前架构说明-用户操作流程-20260526.md   🟡 部分完成
│   ├── project-evolution-roadmap.md   🟡 部分完成
│   └── PRD.md                        🟡 部分完成
│
├── 📋 实施方案与派工
│   ├── MVP-实施方案.md                ✅ 完全同步
│   ├── 派工-Round1-MVP管道.md         ✅ 完全同步
│   ├── 派工-Round2-桌面壳与Live2D集成.md  ✅ 完全同步
│   ├── 派工-Round3-感知层与反向控制.md     ✅ 完全同步
│   ├── 派工-Round4-对话面板与Shell工具.md   ✅ 完全同步
│   ├── 派工-REQ002-前端文字专项优化.md     ✅ 完全同步
│   └── Hook支持类型与任务线映射计划-20260521.md  ✅ 完全同步
│
├── 📐 功能规格
│   ├── SPEC-功能增强-20260519.md       🟡 部分完成
│   ├── 桌宠工具能力与记忆工作区方案-20260526.md  🟡 部分完成
│   ├── 场景化记忆系统细粒度落地方案-20260530.md  📋 规划中
│   ├── Skill导入与工具接入层方案-20260530.md  📋 规划中
│   ├── 对话面板与工具感知整改方案-20260530.md  📋 待派工
│   ├── 多Agent共享工作区与自治Goal调度方案-20260530.md  📋 规划中
│   ├── 用户侧Agent使用场景与状态机模型-20260531.md  📋 新增建模
│   ├── LLM工具与多Agent执行状态机模型-20260531.md  📋 新增建模
│   ├── Agent场景状态机现状差距与整改问题-20260531.md  📋 新增审查
│   ├── hybrid-model-dispatch-inproject-plan-20260604.md  🟡 方案待评审
│   ├── hybrid-model-dispatch-mcpserver-plan-20260604.md  🟡 方案待评审
│   ├── 感知层增强-主形象与子形象状态方案-20260526.md  🟡 部分完成
│   ├── 角色表达与智能增强-机制设计-20260527.md   🟡 部分完成
│   └── 表情状态机与资产规划-20260527.md    🟡 部分完成
│
├── 🤝 交接文档
│   ├── 交接文档-桌宠工具能力与记忆工作区-20260526.md  ✅ 完全同步
│   └── 交接文档-桌宠渲染与主动对话优化-20260527.md   ✅ 完全同步
│
├── 🎨 Live2D 相关 (⏸️ 不再推进)
│   ├── 新中式旗袍Live2D角色设定归档.md     ⏸️ 不再推进
│   ├── Live2D-资产准备.md              ⏸️ 不再推进
│   ├── Live2D-个人推进清单.md           ⏸️ 不再推进
│   ├── Live2D-Qipao动作迁移与自动化边界整合.md  ⏸️ 不再推进
│   └── Live2D-Cubism操作清单-MVP版.md   ⏸️ 不再推进
│
├── 📊 评审与审计
│   └── AgentTeam综合评审-20260529.md    ✅ 完全同步 (rework_required, 45 项发现)
│
├── 📚 参考与调研
│   ├── 感知层调研.md                   🟡 部分完成
│   ├── Rust 前置准备.md                ✅ 完全同步
│   ├── 高强度Agent开发工作流调研问卷.md      ✅ 完全同步
│   ├── 状态机驱动的软件产品生命周期与AgentTeam开发范式-20260529.md  ✅ 完全同步
│   └── claude-code-porting-mapping.md  🟡 部分完成
│
├── 📦 发布与路线
│   ├── Steam上架路线-暂不推进-20260527.md   📋 规划中
│   └── 打包路线-内测版与正式版-20260527.md   📋 规划中
│
└── 📎 附件素材
    ├── ytrazYI6pFXD.png              (5.8 MB)
    ├── WWUwPbcNl_J6.png              (13.4 MB)
    ├── video1.mp4                    (11.4 MB)
    ├── 视频_锐化增强.mp4               (11.0 MB)
    └── 微信图片_20260520172903_1776_60.jpg  (68 KB)
```

---

## 分类说明

### 📐 架构与规划

描述系统整体架构、技术选型和演进路线的文档。这类文档容易因架构迭代而过时，需要定期对照代码更新。

| 文档 | 状态 | 最后修改 | 说明 |
|------|------|----------|------|
| [技术架构.md](技术架构.md) | ✅ 完全同步 | 2026-05-28 | 已重写为 Rust+SQLite+Tauri 架构，涵盖 expression/affection/persona/smart_monitor/workspaces |
| [技术栈选型.md](技术栈选型.md) | ✅ 完全同步 | 2026-05-27 | 已更新：Live2D 替代 Lottie，补充 conductor-sense、fastembed |
| [当前架构说明-用户操作流程-20260526.md](当前架构说明-用户操作流程-20260526.md) | ✅ 完全同步 | 2026-05-27 | 已修正快捷键，补充 expression/affection/persona/smart_monitor 模块 |
| [project-evolution-roadmap.md](project-evolution-roadmap.md) | 🟡 部分完成 | 2026-05-26 | Phase 0 阻塞未动；Phase 1.2/6.2 已完成；Phase 2-4 大部分未实现 |
| [PRD.md](PRD.md) | 🟡 部分完成 | 2026-05-22 | 核心功能已实现；Hook 完整流程部分接通 |

### 📋 实施方案与派工

按轮次拆分的实施计划，每轮有明确的任务清单。这类文档与代码高度对应，状态较准确。

| 文档 | 状态 | 最后修改 | 说明 |
|------|------|----------|------|
| [MVP-实施方案.md](MVP-实施方案.md) | ✅ 完全同步 | 2026-05-22 | 最小闭环已实现，6 验收标准全部满足 |
| [hybrid-model-dispatch-inproject-plan-20260604.md](hybrid-model-dispatch-inproject-plan-20260604.md) | 🟡 方案待评审 | 2026-06-04 | 混合模型指派·项目内接入(Plan 1)：ModelResolver 复用悬空 routing/profile 层，H1-H7，MVP=Claude规划+Doubao感知 |
| [hybrid-model-dispatch-mcpserver-plan-20260604.md](hybrid-model-dispatch-mcpserver-plan-20260604.md) | 🟡 方案待评审 | 2026-06-04 | 混合模型指派·MCP Server 抽离(Plan 2)：复用 mcp.rs 帧/类型倒转方向做 server，暴露 model.route/list/invoke 给 Claude/Codex，M1-M6 |
| [派工-Round1-MVP管道.md](派工-Round1-MVP管道.md) | ✅ 完全同步 | 2026-05-22 | T1-T11 全部实现 |
| [派工-Round2-桌面壳与Live2D集成.md](派工-Round2-桌面壳与Live2D集成.md) | ✅ 完全同步 | 2026-05-22 | T1-T10 全部实现 |
| [派工-Round3-感知层与反向控制.md](派工-Round3-感知层与反向控制.md) | ✅ 完全同步 | 2026-05-22 | T1-T10 全部实现 (T10 需人工验证) |
| [派工-Round4-对话面板与Shell工具.md](派工-Round4-对话面板与Shell工具.md) | 🟡 部分完成 | 2026-05-28 | T1-T5 已实现：消息模型、Shell 抽象层、bash.execute、Query Loop、前端工具卡片(ToolUseCard)；codex session 管理已真实实现；T6 需 LLM 端到端验证 |
| [派工-REQ002-前端文字专项优化.md](派工-REQ002-前端文字专项优化.md) | ✅ 完全同步 | 2026-05-28 | 116/116 处已修复，193 测试全通过 |
| [Hook支持类型与任务线映射计划-20260521.md](Hook支持类型与任务线映射计划-20260521.md) | ✅ 完全同步 | 2026-05-22 | 12 个 Claude Code 事件 + 4 个 Codex 事件全部实现 |

### 📐 功能规格

详细的功能设计文档，描述具体模块的接口、状态机和数据结构。这类文档的"核心逻辑"通常已实现，但集成细节和边界情况可能未落地。

| 文档 | 状态 | 最后修改 | 说明 |
|------|------|----------|------|
| [SPEC-功能增强-20260519.md](SPEC-功能增强-20260519.md) | 🟡 部分完成 | 2026-05-22 | 核心逻辑已实现；proposals UI 缺失 |
| [桌宠工具能力与记忆工作区方案-20260526.md](桌宠工具能力与记忆工作区方案-20260526.md) | 🟡 部分完成 | 2026-05-28 | 47 工具已注册(含 codex.* 6 个)；office 工具为占位实现；无 Proposer 模式 |
| [场景化记忆系统细粒度落地方案-20260530.md](场景化记忆系统细粒度落地方案-20260530.md) | 📋 规划中 | 2026-05-30 | 基于当前真实实现，细化 memory_entries/conversation_summaries 到 chunks/embeddings、场景召回、prompt 注入和治理 UI 的落地路径 |
| [Skill导入与工具接入层方案-20260530.md](Skill导入与工具接入层方案-20260530.md) | 📋 规划中 | 2026-05-30 | 拆分人设 Prompt、Markdown Skill 与 Connector 工具接入层，定义 capability 授权、Lark 接入和迁移路径 |
| [对话面板与工具感知整改方案-20260530.md](对话面板与工具感知整改方案-20260530.md) | 📋 待派工 | 2026-05-30 | 修复 LLM 正文不可见，补充本轮计时、左侧会话 Working 时长、工具调用聚合 transcript 和超时历史一致性 |
| [运行时可观察性与Agent链路收口-20260531.md](运行时可观察性与Agent链路收口-20260531.md) | 📋 待收口 | 2026-05-31 | 记录 `claude -p`、`agent.start`、AgentTeam、外部 hook 的触发条件和用户可感知断点，明确 in_progress 与落地记录展示要求 |
| [多Agent共享工作区与自治Goal调度方案-20260530.md](多Agent共享工作区与自治Goal调度方案-20260530.md) | 📋 已派工 | 2026-05-30 | 34 tasks (TASK-073~106) 已派工到 workspace.md，11 waves，关键阻塞 TASK-073 表名冲突，Wave A 可立即开工 |
| [用户侧Agent使用场景与状态机模型-20260531.md](用户侧Agent使用场景与状态机模型-20260531.md) | 📋 新增建模 | 2026-05-31 | 梳理短任务与长任务两类入口，以及普通聊天、文档协作、代码协作等短任务子场景的状态流转和可感知投影 |
| [LLM工具与多Agent执行状态机模型-20260531.md](LLM工具与多Agent执行状态机模型-20260531.md) | 📋 新增建模 | 2026-05-31 | 梳理 LLM 工具暴露、ToolCall、`claude -p`、AgentTeam、Goal/OODA 和多进程同步状态机 |
| [Agent场景状态机现状差距与整改问题-20260531.md](Agent场景状态机现状差距与整改问题-20260531.md) | 📋 新增审查 | 2026-05-31 | 基于两项建模审查当前系统完成度、用户感知缺口和下一步整改顺序 |
| [Agent状态机收口-评审与派工方案-20260531.md](Agent状态机收口-评审与派工方案-20260531.md) | 📋 待派工 | 2026-05-31 | 四份状态机/可观察性文档的合并评审+统一 backlog；8 项评审补强 + 14 个可派工任务 (TASK-113~126)，决策记录走方案二(外部执行器+Runtime API) |
| [感知层增强-主形象与子形象状态方案-20260526.md](感知层增强-主形象与子形象状态方案-20260526.md) | 🟡 部分完成 | 2026-05-27 | AvatarId/ActivityVariant/avatar_events 已实现；MOOD_MANIFEST 未创建 |
| [角色表达与智能增强-机制设计-20260527.md](角色表达与智能增强-机制设计-20260527.md) | 🟡 部分完成 | 2026-05-27 | 情绪/关系系统已实现；persona.rs 已接入 chat；IdlePhase 已实现 |
| [表情状态机与资产规划-20260527.md](表情状态机与资产规划-20260527.md) | 🟡 部分完成 | 2026-05-27 | 7 MoodZone + 5 RelationshipStage 已实现；情绪素材图未产出 |

### 🤝 交接文档

面向新协作者的上下文交接材料，描述当前已实现的功能和未完成项。这类文档应保持最新。

| 文档 | 状态 | 最后修改 | 说明 |
|------|------|----------|------|
| [交接文档-桌宠工具能力与记忆工作区-20260526.md](交接文档-桌宠工具能力与记忆工作区-20260526.md) | ✅ 完全同步 | 2026-05-26 | 准确描述已实现功能和未完成项 |
| [交接文档-桌宠渲染与主动对话优化-20260527.md](交接文档-桌宠渲染与主动对话优化-20260527.md) | ✅ 完全同步 | 2026-05-27 | 320x420 窗口、ActivityVariant、白名单等均验证通过 |

### 🎨 Live2D 相关 (⏸️ 不再推进)

项目方向已调整，Live2D 路线暂停。相关文档保留作为历史参考。

| 文档 | 状态 | 最后修改 | 说明 |
|------|------|----------|------|
| [新中式旗袍Live2D角色设定归档.md](新中式旗袍Live2D角色设定归档.md) | ⏸️ 不再推进 | 2026-05-26 | 设计归档保留 |
| [Live2D-资产准备.md](Live2D-资产准备.md) | ⏸️ 不再推进 | 2026-05-22 | Live2D 路线暂停 |
| [Live2D-个人推进清单.md](Live2D-个人推进清单.md) | ⏸️ 不再推进 | 2026-05-22 | Live2D 路线暂停 |
| [Live2D-Qipao动作迁移与自动化边界整合.md](Live2D-Qipao动作迁移与自动化边界整合.md) | ⏸️ 不再推进 | 2026-05-23 | Live2D 路线暂停 |
| [Live2D-Cubism操作清单-MVP版.md](Live2D-Cubism操作清单-MVP版.md) | ⏸️ 不再推进 | 2026-05-22 | Live2D 路线暂停 |

### 📊 评审与审计

全项目综合评审报告，基于 state-machine-lifecycle skill 的 AgentTeam OODA-R 范式。

| 文档 | 状态 | 最后修改 | 说明 |
|------|------|----------|------|
| [AgentTeam综合评审-20260529.md](AgentTeam综合评审-20260529.md) | ✅ 完全同步 | 2026-05-29 | 4 Agent 并行评审，45 项发现（6 critical/12 high/15 medium/12 low），结论 rework_required，LC-08→LC-09 |

### 📚 参考与调研

环境配置指南、调研问卷和工具映射规划。这类文档多为信息性或提案性质。

| 文档 | 状态 | 最后修改 | 说明 |
|------|------|----------|------|
| [感知层调研.md](感知层调研.md) | 🟡 部分完成 | 2026-05-22 | Path A (OS hook) 已实现且超出文档范围；Path B (截图) 未实现 |
| [Rust 前置准备.md](Rust 前置准备.md) | ✅ 完全同步 | 2026-05-22 | 环境配置指南，与实际 toolchain 一致 |
| [高强度Agent开发工作流调研问卷.md](高强度Agent开发工作流调研问卷.md) | ✅ 完全同步 | 2026-05-26 | 调研问卷，无需代码对应 |
| [状态机驱动的软件产品生命周期与AgentTeam开发范式-20260529.md](状态机驱动的软件产品生命周期与AgentTeam开发范式-20260529.md) | ✅ 完全同步 | 2026-05-29 | 通用状态机驱动范式：需求拆解、LLM/Agent 建模、OODA-R 派工、权限边界、测试回归和生命周期治理 |
| [claude-code-porting-mapping.md](claude-code-porting-mapping.md) | 🟡 部分完成 | 2026-05-27 | 6 个文件工具已实现 (file.glob/grep/read/write/edit/stat)；web/interactive 未实现 |

### 📎 附件素材

图片和视频素材文件，供文档引用。

| 文件 | 大小 | 说明 |
|------|------|------|
| ytrazYI6pFXD.png | 5.8 MB | 文档配图 |
| WWUwPbcNl_J6.png | 13.4 MB | 文档配图 |
| video1.mp4 | 11.4 MB | 演示视频 |
| 视频_锐化增强.mp4 | 11.0 MB | 演示视频 (锐化版) |
| 微信图片_20260520172903_1776_60.jpg | 68 KB | 微信截图 |

---

## 维护规则

### 文档状态标记

| 标记 | 含义 | 触发条件 |
|------|------|----------|
| ✅ 完全同步 | 文档与代码一致 | 审查通过，无需更新 |
| 🟡 部分完成 | 核心已实现，细节有缺口 | 核心逻辑代码存在，但集成/边界/前端未完成 |
| 🧪 需人工测试 | 代码存在但未验证 | 涉及 GUI 操作或主观体验，无法纯代码审查 |
| ⚠️ 大部分已过时 | 文档描述已不适用 | 架构/方案已被重构，文档需重写 |
| ❌ 完全未实现 | 纯规划文档 | 文档为提案，代码未动 |
| ⏸️ 不再推进 | 项目方向调整暂停 | 该路线已暂停，文档保留作历史参考 |

### 更新频率

- **交接文档**: 每次代码变更后 24h 内更新
- **功能规格**: 功能完成时更新
- **架构文档**: 架构变更时更新
- **实施/派工**: 任务完成时勾选
- **本索引**: 每次审查后自动更新

### 新增文档规范

新增文档时请：
1. 放入对应分类目录（本文件树）
2. 在本索引中添加条目
3. 标注初始状态（通常为 ❌ 完全未实现）
4. 在 [文档同步状态-20260527.md](文档同步状态-20260527.md) 中添加审查行
