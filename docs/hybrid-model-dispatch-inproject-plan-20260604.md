# 混合模型指派 · 项目内接入方案（Plan 1）

> 状态：🟡 方案待评审 · 日期：2026-06-04 · 关联：[[statemachine-closure-dispatch-20260531]]（方案二·外部执行器）、`routing.rs`、`llm_profiles.rs`

## 0. 目标与场景

面向中国用户的开发活动，按**角色**把任务分发到不同 baseURL/apiKey 的模型：

| 角色 | 模型 | 用途 | 现有调用点 |
|---|---|---|---|
| 感知 / 用户把关 | Doubao（国内） | 用户感知、表情/状态、轻量总结 | `smart_monitor.rs`、`summarizer.rs` |
| 规划 / 对话主脑 | Claude | 计划、对话编排 | `send_v2.rs` 主循环 |
| 复杂编码 | GPT / Codex | 代码开发子任务 | `agent_runs.rs` 子代理（外部 CLI） |

关键认知：本项目有**两条派发轴**，混合指派必须同时覆盖：
1. **进程内 LLM 调用**——`llm.rs` 经 `config.llm` 单一全局配置（主脑 + 总结 + 感知）。
2. **进程外子代理**——`claude -p <prompt>` 外部 CLI，**当前无 `--model`**。

## 1. 现状评估（已核验代码）

### 1.1 单模型瓶颈
- `CoreConfig.llm` 是**单个** `LlmConfig`（`config.rs:55`），非列表：provider/model/base_url/api_key/temperature。
- 所有进程内调用都显式传 `&config.llm.model`：`send_v2.rs:1670`（流式主循环）、`:2102`（force_final）、`:2142`（recovery）、`summarizer.rs:157`、`smart_monitor.rs:202`。
- `LlmRequestConfig::from_config` 只接受 `&LlmConfig`（`llm.rs:57`），且 `LlmRequestConfig.model` 字段实际**未被使用**（调用方另传 model）——这是混合指派要消除的 footgun。

### 1.2 已建但悬空的路由层（核心可复用资产）
- `llm_profiles.rs`：多 provider 档案表 `LlmProfile{ id, name, provider, model_id, api_base_url, api_key_encrypted, max_tokens, temperature, enabled }`，全 CRUD，表 `db.rs:1747`。provider 限 `openai|anthropic|local`。
- `routing.rs`：`classify_task()`（中英关键词分类器 → `TaskKind` Planning/Coding/Review/Testing/Document/ExternalAction）、`RoutingPolicy{ task_kind → backend_kind + profile_id }`、`route_task()` → `RouteDecision{ backend_kind, profile_id, reason, fallback_used }`，落 `route_decisions` 表。
- **致命断点**：`route_task()`/`route_text()` 仅被 `routing.rs` 自身测试调用；`act.rs` 只 proposed→queued 不 dispatch；`ClaudePAdapter::spawn` 零生产调用方。路由决策**算出来后从不送达任何 spawn 或 LLM 调用**。

### 1.3 引擎能力充足
- `llm.rs` 已支持 OpenAI-compatible **与** Anthropic-compatible 双协议（`protocol()` 按 provider 字符串选择，`llm.rs:354`），含流式 SSE、多 auth 方案重试（Bearer/api-key/x-api-key）、token 字段自适应（max_tokens↔max_completion_tokens）。**任意 OpenAI/Anthropic 兼容端点开箱即用**——Doubao（OpenAI 兼容）无需新协议。

### 1.4 安全现状（必须正视）
- `api_key_encrypted` **名不副实**：全 crate 无 encrypt/decrypt 函数，列里就是明文（测试存 `sk-new-key`）。混合指派会引入更多密钥 → 本方案纳入一个**最小密钥间接层**，不阻塞 MVP 但留好钩子。

### 1.5 总线落点
- `chat_turns` 已带 `model_provider`/`model_name`（`send_v2.rs:1004`，仅 telemetry）。
- `append_chat_stage`（`util.rs:630`）是多 sink 漏斗；`append_turn_event_by_request`（`turns.rs:338`）写 `chat_turn_events` 逻辑总线。路由决策可经此**零新增传输**地观测化。

## 2. 目标架构

核心思想：**不新建总线，复用已建路由层，把"算出来的决策"接到两条派发轴的调用点上。** 引入一个 `ModelResolver` 把 `角色/TaskKind → LlmProfile → 具体调用参数` 收口为单一解析点。

```
                       ┌─────────────────────────────────────────────┐
   role / TaskKind ───▶│ ModelResolver (新增, ~150 行)                 │
   (Plan/Code/Sense)   │  1. classify_task() 或显式 role               │
                       │  2. route_task() → RouteDecision.profile_id   │
                       │  3. get_profile() → LlmProfile                │
                       │  4. resolve_api_key()（明文/env/间接层）       │
                       └───────────────┬───────────────────────────────┘
                                       │ 产出 ResolvedModel{provider,model,base_url,api_key,temp}
                 ┌─────────────────────┼─────────────────────┐
                 ▼                     ▼                     ▼
   进程内 LLM (llm.rs)        进程内 LLM (总结/感知)      进程外子代理 (agent_runs.rs)
   LlmRequestConfig::         summarizer / smart_      claude/codex CLI + --model
   from_resolved()           monitor 同解析            (或按 backend_kind 切 adapter)
                 │                     │                     │
                 └──────── append_turn_event "model.routed" → chat_turn_events ────┘
```

### 2.1 新增类型（最小集）
```rust
// llm.rs —— 把 RequestConfig 从“只能借 config.llm”解放出来
pub struct ResolvedModel {           // owned, 可来自 profile 或 config.llm
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub temperature: f64,
}
impl<'a> LlmRequestConfig<'a> {
    pub fn from_resolved(m: &'a ResolvedModel) -> Self { /* 同 from_config 形状 */ }
}

// model_resolver.rs（新文件）—— 角色/任务 → ResolvedModel
pub enum ModelRole { Planner, Coder, Sense, Summarize }  // 映射到 TaskKind
pub async fn resolve_model(role: ModelRole, hint: Option<&str>) -> Result<ResolvedModel>;
//   1) 查 routing_policies（role→TaskKind→policy.profile_id）
//   2) profile 命中 → 用 profile；未命中/未启用 → 回退 config.llm（永不 panic）
//   3) api_key: profile.api_key_encrypted ?? env(provider) ?? config.llm
```

### 2.2 关键设计决策
- **回退优先**：任一环节（无 policy / profile disabled / 无 key）都**静默回退 `config.llm`**，保证现有单模型路径零回归。路由是增强不是前置依赖。
- **消除 footgun**：把分散的 `&config.llm.model` 调用收敛为 `ResolvedModel.model`，`LlmRequestConfig.model` 不再是死字段。
- **进程外轴对齐方案二**：子代理不在进程内选模型，而是给 `spawn_claude`（`agent_runs.rs:206`）注入 `--model`，或按 `RouteDecision.backend_kind` 切换 claude/codex adapter。`command_json`（`agent_runs.rs:165`）记录解析出的模型供审计。
- **密钥间接层（最小）**：`resolve_api_key()` 统一从 `profile → env(PROVIDER_API_KEY) → config` 三级解析，不在 DB 存明文长期项；MVP 阶段允许 profile 明文但**日志/事件中只记 provider 不记 key**（遵循 secret 不回显）。

## 3. 任务分解（按方案二 Batch 风格，子线程派发）

> 关键路径：H1 → H2 → H3 →（H4 ∥ H5）→ H6。每个任务验收强制含**集成/挂载验证**（收口 "accepted≠integrated" 教训）。

| ID | 任务 | 关键文件 | 验收（含集成） | 依赖 |
|---|---|---|---|---|
| **H1** | `ResolvedModel` + `from_resolved`；`config.llm` 改经 `ResolvedModel::from_config` 适配 | `llm.rs:45-67` | 现有 send_v2/summarizer 全绿，行为不变（纯重构） | — |
| **H2** | `model_resolver.rs`：`ModelRole`、`resolve_model()`、三级 key 解析、回退逻辑 | 新文件 + `routing.rs`/`llm_profiles.rs` 复用 | 单测：命中 profile / disabled 回退 / 无 policy 回退 三分支 | H1 |
| **H3** | 主循环接 resolver：`send_v2.rs` 主/recovery/force_final 三处 LLM 调用经 `resolve_model(Planner)` | `send_v2.rs:1670/2102/2142` | 配 Claude profile，对话主脑走 Claude；turn 的 `model_name` 反映实际命中 | H2 |
| **H4** | 感知/总结接 resolver：`summarizer.rs`、`smart_monitor.rs` 走 `resolve_model(Sense)` → Doubao | `summarizer.rs:157`、`smart_monitor.rs:202` | 配 Doubao profile，总结调用命中 Doubao base_url（集成测试断言 request host） | H2 |
| **H5** | 子代理模型注入：`StartAgentRunInput`/工具 schema 加 `model`/`profile_id`；`spawn_claude` 注入 `--model`；`command_json` 记录 | `agent_runs.rs:66/165/206`、`agent.rs:335/380` | `subagent.claude_p` 带 profile → 子进程命令行含 `--model`；output.json 与审计可见 | H2 |
| **H6** | 路由观测化：resolver 决策经 `append_turn_event_by_request` 发 `model.routed`{role,profile_id,provider,fallback_used} | `turns.rs:338`、`util.rs:630` | `chat_turn_events` 出现 model.routed；不记 api_key | H3,H4 |
| **H7** | 设置面板：profile CRUD + 角色→policy 绑定 UI（前端） | `apps/desktop/src/...` + IPC | 用户可在 UI 建 Doubao/Claude/GPT 三档并绑定角色 | H2 |

## 4. 风险与回归边界
- **R1 回退失效致全线挂**：resolver 必须 infallible-fallback，任何 DB/网络错误回退 config.llm；H2 单测三分支为硬门禁。
- **R2 子代理 `--model` 兼容性**：`claude -p` 是否支持 `--model` 取决于 CLI 版本——H5 先探测 CLI 能力，不支持则降级为"按 backend_kind 切 codex adapter"（与方案二 TASK-117 外部 runner 并轨）。
- **R3 密钥泄漏**：事件/日志/`command_json` 一律不记 api_key；secret 类内存只在 resolve 瞬间持有。
- **R4 与方案二冲突**：H5 与 TASK-117（外部 CLI runner）有重叠——建议 H5 作为 TASK-117 的"模型选择"子能力合并实现，不另起 spawn 路径。

## 5. MVP 切片（最小可演示）
H1 + H2 + H3 + H7：用户在 UI 配 Claude（规划）+ Doubao（感知）两档，对话主脑走 Claude、总结走 Doubao，子代理暂仍默认 claude。可观测、可回退、零现有回归。H5（GPT/Codex 子代理）作为第二切片。
