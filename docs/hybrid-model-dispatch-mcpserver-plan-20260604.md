# 混合模型指派 · MCP Server 抽离方案（Plan 2）

> 状态：🟡 方案待评审 · 日期：2026-06-04 · 关联：[[hybrid-model-dispatch-inproject-plan-20260604]]（Plan 1 进程内方案）、`mcp.rs`、`tools/registry.rs`

## 0. 目标

把 Plan 1 的"混合模型指派"能力从本项目抽离，做成一个**独立 MCP Server**，让 **Claude Code / Codex CLI**（任意 MCP host）都能通过标准 MCP 协议调用"按角色/任务把请求分发到不同 baseURL+apiKey 模型"的能力。

与 Plan 1 的分工：
- **Plan 1**：能力长在 conductor 进程内，服务桌宠自身的对话/子代理。
- **Plan 2**：把同一套 `路由 + 多 provider 调用` 抽成**进程外通用工具**，对外暴露为 MCP tools，宿主是 Claude/Codex 而非桌宠。

## 1. 现状评估（已核验代码）

### 1.1 已有：MCP 客户端 + 完整传输层（重大可复用）
- `crates/conductor-core/src/mcp.rs`（~1520 行）是**手写** MCP 实现，**无任何第三方 MCP crate**（全 Cargo.toml 无 `rmcp`/`modelcontextprotocol`）。
- 已具备且经测试的部分：
  - **JSON-RPC 2.0 类型**（`mcp.rs:10-59`）：`JsonRpcRequest`/`Notification`/`Response`/`Error`/`JsonRpcId`。
  - **MCP 协议类型**（`mcp.rs:61-155`）：`InitializeParams/Result`、`ClientCapabilities`、`ServerCapabilities`、`Tool{inputSchema}`、`ToolsListResult`、`ToolsCallParams`、`ToolsCallResult`、`ToolCallContent{Text/Image/EmbeddedResource}`。
  - **stdio 传输**（`mcp.rs:161-238`，`StdioProcess`）：**newline-delimited JSON 帧**（`write_all + '\n' + flush`；`BufReader::read_line + from_str`）。
  - **传输抽象** `enum McpTransport { Http{endpoint,api_key}, Stdio(StdioHandle) }`（`mcp.rs:246`）。
- **方向问题**：以上**全是 client 侧**（`McpClient` 发 `initialize`/`tools/list`/`tools/call` 给外部 server）。**无 server**：没有入站 read-loop、没有 `tools/list`/`tools/call` handler、没有 axum MCP 路由。抽 server 要做的就是**把 I/O 方向倒过来**——帧与类型直接复用。

### 1.2 已有：Tauri-free 引擎核心（可整体搬走）
- `llm.rs`（双协议 + 流式 + auth/token 自适应）、`tools/registry.rs`（`ToolSpec`/`ToolExecutorFn`/`ToolRegistry`）、`routing.rs`、`llm_profiles.rs` **均不依赖 Tauri**。
- 仅 `send_v2.rs` 经 `AppHandle`/`Emitter` 与 UI 耦合；`handler.rs` 旧路径**零 Tauri 依赖**——证明对话引擎与 UI 可干净切分。
- `ToolSpec` 已自带 `input_schema: serde_json::Value`（JSON Schema 形状），→ MCP `Tool.inputSchema` 是**近乎直字段映射**（`discover_tools` 在 `mcp.rs:628` 已做过反向映射）。

### 1.3 边界与 gap
- `validate_input`（`registry.rs:214`）只校验顶层 `required`，非完整 JSON-Schema 校验——MCP server 对外需补一层入参校验。
- tool id 点号 `file.read` 对 LLM 改写为 `file__read`（`tools.rs:203`）——MCP 命名规则需独立约定。
- `api_key_encrypted` 实为明文（无 encrypt/decrypt）——独立进程更需正经密钥来源（env/OS keyring）。

## 2. 目标架构

一个独立可执行体 `model-router-mcp`，作为 **MCP server**（stdio 优先，HTTP/SSE 次选），被 Claude Code / Codex 通过其 `mcpServers` 配置拉起。它对外暴露少量"模型分发"工具，内部复用抽出的引擎 crate 真正去调各家模型。

```
  Claude Code / Codex CLI (MCP host)
        │  stdio: newline-delimited JSON-RPC 2.0
        │  initialize / tools/list / tools/call
        ▼
  ┌──────────────────────────────────────────────────────────┐
  │ model-router-mcp (新 bin crate)                            │
  │  ┌────────────────────────────────────────────────────┐  │
  │  │ MCP server loop (新): stdin read-loop → dispatch    │  │
  │  │   ← 复用 mcp.rs 的 JsonRpc*/Tool/ToolsCall* 类型     │  │
  │  │   ← 复用 newline-framed 编解码（方向倒转）           │  │
  │  └───────────────┬────────────────────────────────────┘  │
  │                  ▼                                          │
  │  暴露的 MCP tools:                                          │
  │   • model.route   {role|task, prompt}  → 选档并返回答案     │
  │   • model.list_profiles                → 列可用模型档案     │
  │   • model.invoke  {profile_id, prompt} → 指定档案直调       │
  │                  │                                          │
  │                  ▼  复用抽出的引擎                          │
  │   model-router-core (新 lib crate, 从 conductor-core 抽)   │
  │     llm.rs(双协议/流式) · routing.rs · llm_profiles.rs     │
  └──────────────────────────────────────────────────────────┘
        │ OpenAI-compat / Anthropic-compat HTTP
        ▼
   Doubao   /   Claude   /   GPT  (各自 baseURL + apiKey)
```

### 2.1 抽离边界（crate 切分）
新建 workspace 成员，**不动 conductor-core 现有依赖图**：
- `crates/model-router-core`（lib）：从 conductor-core **复制/提取**（非引用，避免反向依赖）：`llm.rs`、`routing.rs`（去掉 backend/agent 耦合，仅留 `classify_task` + profile 选择）、`llm_profiles.rs`（DB 访问改为可插拔 store trait）、新增 `ModelResolver`（同 Plan 1 H2 逻辑）。**零 Tauri、零 SQLite 强依赖**（store 抽象为 trait，默认实现走独立 sqlite 或 TOML 配置文件）。
- `crates/model-router-mcp`（bin）：MCP server loop + 三个工具 handler。

> 抽离策略二选一（评审决策点 D1）：
> - **(a) 复制提取**：把 `llm.rs` 等复制进新 crate，独立演进。优点：零耦合、可单独开源给 Claude/Codex 生态；缺点：双份维护。
> - **(b) 共享下沉**：把 `llm.rs`/`routing.rs`/`llm_profiles.rs` 下沉为 `model-router-core`，conductor-core 反过来**依赖**它。优点：单一真相、Plan 1 与 Plan 2 共用同一引擎；缺点：改动 conductor-core 依赖图，回归面更大。
> **建议 (b)**——与 Plan 1 共引擎，避免双份模型调用逻辑漂移；conductor-core 仅需把 `use crate::llm` 改为 `use model_router_core::llm`。

### 2.2 server loop（要新建的唯一传输代码）
```rust
// model-router-mcp/src/server.rs
// 复用 mcp.rs 的类型；方向：读 stdin → 派发 → 写 stdout
loop {
    let line = stdin.read_line()?;                 // newline-framed（同 mcp.rs:216 解码）
    let req: JsonRpcRequest = serde_json::from_str(&line)?;
    let resp = match req.method.as_str() {
        "initialize" => handle_initialize(req),    // 回 ServerCapabilities{tools:{}}
        "tools/list" => handle_tools_list(),       // 返回三个 model.* 工具的 inputSchema
        "tools/call" => handle_tools_call(req).await, // 解析 name+arguments → ModelResolver
        _ => JsonRpcResponse::method_not_found(req.id),
    };
    stdout.write_all(serde_json::to_string(&resp)?.as_bytes())?; // + '\n' + flush
}
```

### 2.3 暴露的工具（MCP tools/list）
| 工具 | 入参 schema | 行为 | 出参（ToolCallContent::Text） |
|---|---|---|---|
| `model.route` | `{role?: "plan\|code\|sense", task?: string, prompt: string}` | classify_task→选档→调对应模型→返回答案 | 模型回答 + `_meta{profile_id, provider, fallback_used}` |
| `model.list_profiles` | `` | 列 enabled 档案（不含 key） | profiles JSON |
| `model.invoke` | `{profile_id: string, prompt: string, system?: string}` | 指定档案直调 | 模型回答 |

## 3. 任务分解（子线程派发）

> 关键路径：M1 → M2 → M3 → M4 →（M5 ∥ M6）。

| ID | 任务 | 关键文件 | 验收（含集成） | 依赖 |
|---|---|---|---|---|
| **M1** | 决策 D1（复制 vs 下沉），建 `model-router-core` crate 骨架 + `LlmProfileStore` trait | 新 crate + workspace Cargo.toml | crate 编译；conductor-core 仍全绿 | — |
| **M2** | 引擎落位：`llm.rs`（双协议/流式）+ `ModelResolver` 进 core，store 默认实现（TOML 或独立 sqlite） | `model-router-core/src/*` | 单测：OpenAI-compat & Anthropic-compat 各打通一次（可用 mock server） | M1 |
| **M3** | MCP server loop：复用 `mcp.rs` 类型，新建入站 read-loop + `initialize`/`tools/list`/`tools/call` 派发 | `model-router-mcp/src/server.rs` | 用 `echo '{...initialize...}' \| model-router-mcp` 手测三方法往返 | M2 |
| **M4** | 三个工具 handler（route/list_profiles/invoke）+ 入参 JSON-Schema 校验层 | `model-router-mcp/src/tools.rs` | 集成：Claude Code 配置 mcpServers 后 `tools/list` 可见三工具，`model.route` 返回真实模型回答 | M3 |
| **M5** | 密钥来源：env / OS keyring 取代明文；配置文件 `~/.model-router/config.toml`（profiles + 角色映射） | `model-router-core/src/store.rs` | key 不落明文 DB；缺 key 时报清晰错误而非泄漏 | M2 |
| **M6** | 接入文档 + 配置样例：Claude Code `mcpServers` 与 Codex 的 MCP 配置片段 | 新 README | 两边宿主各跑通一次端到端调用 | M4 |

## 4. 与 Plan 1 的关系与复用
- 若 D1 选**下沉(b)**：Plan 1 的 H1/H2（`ResolvedModel`+`ModelResolver`）与 Plan 2 的 M2 是**同一份代码**，只实现一次，分别被 conductor-core 与 mcp server 复用。强烈建议两方案排期时 **H1/H2 与 M1/M2 合并为同一批**。
- `mcp.rs` 现有 client 不删——Plan 2 server 与现有 client 共享类型，未来 conductor 自身也能当 host 调用这个 router（自举）。

## 5. 风险
- **R1 stdio 帧格式（已澄清，非风险）**：MCP stdio 传输规范即 **newline-delimited JSON-RPC 2.0**（每条消息单行、不含内嵌换行；stdin/stdout 走消息、stderr 走日志），**不是** LSP 风格的 Content-Length 帧。`mcp.rs` 现有帧（`write_all + '\n' + flush` / `read_line + from_str`）正是规范格式，server 端**直接复用即可**，无需双帧支持。（本条原列为前置核验项，经核实为既定规范，降级；M3 仅需按宿主 `initialize` 协商协议版本即可。）
- **R2 协议版本**：`mcp.rs` 写死 `protocolVersion "2024-11-05"`；server 端须回报宿主可接受的版本，按宿主 `initialize` 协商。
- **R3 抽离回归**：选(b)下沉会改 conductor-core 依赖；M1 验收强制"conductor-core 全测试绿"作为硬门禁。
- **R4 密钥**：独立进程不再有桌宠的信任边界，M5（env/keyring）是上线前置，不可用明文 TOML 长期跑。

## 6. MVP 切片
M1+M2+M3+M4（用 env 提供 key 的临时版）：在 Claude Code 里配好 `model-router-mcp`，`model.route{role:"sense", prompt:...}` 命中 Doubao、`role:"plan"` 命中 Claude，端到端返回回答。M5（keyring）、M6（Codex 接入文档）为第二切片。
