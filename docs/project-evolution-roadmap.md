# Personal Conductor 项目演进路线图

> 基于项目源码完整分析（2026-05-27），涵盖 architecture review、数据通路、当前 gap 和未来方向。
>
> **核心原则**：Personal Conductor 是一个有性格的桌面伴生桌宠，不是一个通用编码助手。所有能力扩展必须服务于"清和"这个角色的陪伴体验。

---

## 一、设计理念与架构总览

### 1.1 核心理念

Personal Conductor 是一个 **桌面伴生 AI Agent（桌宠）**，核心定位：

- **角色层**（清和）：穿藏青旗袍的桌面助手，有独立性格、情绪表达、子形象状态系统
- **感知层**（conductor-sense）：通过 Win32 API 持续感知用户当前在做什么（焦点窗口、空闲状态、窗口标题）
- **决策层**（conductor-core）：基于感知信息 + 任务系统 + 记忆系统，决定是否需要主动介入或被动响应
- **执行层**：通过工具注册表（ToolRegistry）+ 提案系统（Proposals）+ 子 Agent（claude -p）执行具体动作
- **呈现层**：Tauri 桌面应用（视频/图片混合渲染的桌宠）+ 内嵌面板（任务/对话/设置）

整体架构遵循 **Observe-Orient-Decide-Act (OODA)** 环：

```
[conductor-sense]
   ├─ focus.rs        → 窗口焦点轮询
   ├─ idle.rs         → 空闲检测
   ├─ whitelist.rs    → 应用白名单
   ├─ window_title.rs → 窗口标题提取
   └─ pacer.rs        → 5分钟心跳
        │
        ▼
[conductor-core]
   ├─ events.rs       → NDJSON 事件日志
   ├─ inject.rs       → 上下文注入 | Claude Code hooks
   ├─ proposals.rs    → 行动提案（Pending→Approved→Running→Succeeded/Failed）
   ├─ initiative.rs   → 主动发起引擎（4种 trigger）
   ├─ skills.rs       → 技能系统（Document/Coding/PetAvatar）
   ├─ chat.rs         → 对话 + LLM 工具调用循环
   ├─ tasks/tasklist  → 任务系统 V1/V2
   ├─ agent_runs      → 后台 Agent 生命周期管理
   ├─ agent_teams     → 多 Agent 团队 + 邮箱通信
   ├─ tools.rs        → 工具注册表（27 个已注册工具）
   ├─ mcp.rs          → MCP 适配层（HTTP → ToolRegistry）
   ├─ memory.rs       → KV 记忆存储 + memory_chunks/embeddings
   └─ config.rs       → 配置管理（JSON）
        │
        ├──► [subagent/claude -p] 子进程执行
        ├──► [LLM] OpenAI-compatible API
        └──► [桌面] Tauri App（PetWindow + Panels）
```

### 1.2 数据通路

```
[用户操作] ──► [hooks (claude-code / codex)]
                   │
                   ▼
              [events.ndjson] ──► [pacer 5min]
                                      │
                                      ▼
                             [initiative engine]
                                      │
                                      ▼
                             [proposal system]
                                      │
                                      ▼
                             [tool execution]
                                      │
                         ┌─────────────┼─────────────┐
                         ▼             ▼             ▼
                   [subagent]    [LLM chat]    [MCP tools]
                         │             │
                         ▼             ▼
                   [agent_runs]   [chat history]
                   [agent_teams]  [memory store]
```

### 1.3 关键瓶颈：LLM 工具白名单

**当前状态**：`chat.rs` 中 `build_tool_definitions()` 硬编码了仅 4 个工具暴露给 LLM：

```rust
let allowed_ids = [
    "pet.set_avatar",
    "conductor.pet.set_avatar",
    "task.list",
    "task.get",
];
```

**影响**：即使注册了 27 个工具，LLM 只能调用这 4 个。新增的 file.*/web.* 工具对 LLM 不可见。

**解决方案**：Phase 0 必须将白名单改为可配置的 allowlist，否则后续所有工具扩展都是死代码。

---

## 二、当前架构优势与 Gap

### 2.1 优势

| 维度 | 说明 |
|------|------|
| **模块化** | Rust 多 crate 结构，core/cli/sense/desktop 职责清晰 |
| **工具抽象** | ToolSpec + ToolRegistry + 5 级风险模型 + 权限系统，扩展性强 |
| **事务性提案** | 提案全生命周期（Pending→Approved→Running→Succeeded/Failed），可审计 |
| **多 Agent 通信** | 团队 + 邮箱模型，支持 PlanApprovalRequest/Response 等协议消息 |
| **事件驱动** | NDJSON 事件日志可回放、可审计 |
| **桌面深度集成** | Win32 前台窗口跟踪、空闲检测、Tauri 系统托盘/通知 |
| **角色系统** | 主形象 + 子形象（9种 ActivityVariant），LLM 调用时自动切换表情 |
| **主动对话** | 焦点检测 + initiative 引擎 + 可配置 tool_triggers |

### 2.2 Gap 分析

| 领域 | 当前状态 | 问题 | 优先级 |
|------|---------|------|--------|
| **LLM 工具白名单** | 硬编码 4 个 ID | 新工具对 LLM 不可见，是所有扩展的前置条件 | **P0** |
| **执行器技术债** | 每个 executor 创建 `Runtime::new()` | 阻塞、低效，新增异步工具前必须重构 | **P0** |
| **代码理解工具** | 无 Glob/Grep/FileEdit 工具 | 无法进行代码搜索和精确编辑 | P1 |
| **文件读写能力** | 仅有 office 模块的读文档功能 | 缺乏通用 FileRead(offset/limit)/FileWrite | P1 |
| **Web 能力** | 无 WebSearch/WebFetch | 无法进行在线研究和文档查阅 | P1 |
| **角色表达** | 仅 9 种 ActivityVariant | 缺乏情绪反应、工具结果反馈、空闲行为 | P1 |
| **富媒体处理** | 无 Image/PDF/Notebook 工具 | 无法查看图片、PDF、Jupyter 笔记本 | P2 |
| **交互式 UX** | 仅有纯文本对话 | 缺乏 AskUserQuestion 这样的多选交互 | P2 |
| **会话内任务** | 有持久化任务系统 | 缺乏 TodoWrite 会话级任务跟踪 | P2 |
| **沙箱隔离** | 无 Worktree 机制 | 高风险操作无隔离环境 | P2 |
| **Office CLI** | 占位实现 | office.inspect_document 仅返回文件系统元数据 | P2 |
| **Embedding** | 表结构已建 | 无实际 embedding 计算/检索 | P3 |
| **UI 完备性** | 基础面板存在 | Workspace/Proposal/Memory/Team UI 均有缺口 | P3 |
| **MCP 远程执行** | 基础 HTTP 桥接 | 缺乏完整协议握手和动态发现 | P3 |

---

## 三、演进方向

### Phase 0：基础设施修复（前置条件）

**目标**：解决所有后续工作的阻塞项。

#### 0.1 LLM 工具白名单可配置化

**问题**：`chat.rs:202-209` 硬编码 `allowed_ids`，新注册工具对 LLM 不可见。

**方案**：
- 在 `CoreConfig` 中新增 `llm_tools` 配置段：
  ```rust
  pub struct LlmToolsConfig {
      pub allowlist: Vec<String>,    // 允许 LLM 调用的工具 ID
      pub denylist: Vec<String>,     // 禁止的工具 ID（优先级高于 allowlist）
  }
  ```
- `build_tool_definitions()` 读取配置，支持 `*` 通配符（如 `"file.*"` 匹配所有文件工具）
- 默认 allowlist 包含当前 4 个 + 新增的安全工具

#### 0.2 执行器异步化重构

**问题**：每个 executor 内部 `tokio::runtime::Runtime::new()` 创建独立 runtime，阻塞且低效。

**方案**：
- 新增 `AsyncToolExecutorFn` 类型别名：
  ```rust
  pub type AsyncToolExecutorFn = fn(&ToolSpec, &serde_json::Value)
      -> Pin<Box<dyn Future<Output = Result<ToolExecutionResult>> + Send>>;
  ```
- `ToolRegistry` 同时支持同步和异步 executor
- 新工具使用异步 executor，现有工具逐步迁移
- 消除所有 `Runtime::new()` 调用

#### 0.3 技术债务清理

| 项目 | 描述 | 优先级 |
|------|------|--------|
| `lazy_static` → `std::sync::LazyLock` | 现代化 | P2 |
| DB 迁移用 `ensure_column` 逐列 diff | 应使用正规 migration | P2 |
| 测试中 `register_builtin_tools()` 有副作用 | 应隔离 per-test registry | P1 |

---

### Phase 1：代码理解能力 + 角色表达增强（第 1-4 周）

**目标**：让清和能帮用户看代码，同时在工具执行过程中展现角色性格。

#### 1.1 文件工具族

```
新工具：
  ├─ file.glob        → glob 模式匹配
  ├─ file.grep        → ripgrep 正则搜索（vendor/rg.exe 已存在）
  ├─ file.read        → offset/limit 分段读取 + 行号
  ├─ file.write       → 创建/覆盖文件
  ├─ file.edit        → search-and-replace 编辑
  └─ file.stat        → 文件元数据（大小、修改时间、类型）
```

**实施要点**：
- `file.read` 和 `file.glob`/`file.grep` 为只读工具，直接暴露给 LLM
- `file.write` 和 `file.edit` 为写入工具，走提案系统审批
- `file.grep` 调用 `vendor/rg.exe`，需 `cfg(windows)` 守卫
- `file.read` 输出带行号，限制 2000 行防止上下文溢出
- 所有 file 工具在 workspace 范围内操作，跨 workspace 需 `ReadExternalPath` 权限

**依赖**：`glob = "0.3"`

#### 1.2 工具结果→角色表达反馈

**目标**：工具执行结果通过清和的性格表达出来，而不是返回原始 JSON。

```
新增能力：
  ├─ tool_result_formatter → 将 ToolExecutionResult 转为角色化文本
  ├─ avatar_reaction       → 工具执行成功/失败时切换子形象表情
  └─ chat_bubble_adapter   → 截断长输出（≤500字）、保留关键信息
```

**实施要点**：
- 在 `chat.rs` 的 `execute_tool_call()` 返回后，调用 formatter
- 成功：Done 表情 + 简短确认（"找到了 3 个文件"）
- 失败：Error 表情 + 角色化道歉（"这个文件我打不开呢"）
- 长输出自动截断，保留前 500 字 + "... 还有 N 行"

#### 1.3 主动对话增强

**目标**：基于已有的 focus watcher + initiative 引擎，增加更多触发场景。

```
改进：
  ├─ 时间段感知    → 早/中/晚不同问候风格
  ├─ 工作时长提醒  → 连续工作 2h 提醒休息
  └─ 日程感知      → 读取日历/提醒事项，主动提示
```

---

### Phase 2：Web 能力 + 交互体验（第 5-8 周）

#### 2.1 Web 工具

```
新工具：
  ├─ web.search        → 带域名过滤的 Web 搜索
  └─ web.fetch         → 获取 URL 内容 + 摘要处理
```

**实施要点**：
- 需在 `CoreConfig` 新增 `WebSearchConfig`（搜索引擎 API key）
- `web.fetch` 限制超时 30s + 最大内容 100KB
- 搜索结果通过角色化文本呈现，不返回原始 JSON

**依赖**：`reqwest`（已有）

#### 2.2 交互式体验

```
新能力：
  ├─ interactive.ask   → 多选问题（2-4 个选项，支持多选）
  ├─ todo.write        → 会话内任务跟踪清单
  └─ config.get/set    → 运行时配置读写
```

**实施要点**：
- `interactive.ask` 需要 Tauri 前端新组件 `QuestionDialog`
- `todo.write` 是内存中的会话任务（不同于持久化 tasks）
- 配置读写走 `config.rs` 现有体系

#### 2.3 子形象系统扩展

```
新增 ActivityVariant：
  ├─ Reading     → 阅读文件/文档时
  ├─ Searching   → Web 搜索时
  ├─ Celebrating → 任务完成时
  └─ Curious     → 发现有趣内容时
```

**实施要点**：
- 在 `avatar.rs` 的 `ActivityVariant` 枚举中新增变体
- 对应图片资源放入 `public/avatar/{avatar_id}/` 目录
- `chat.rs` 在对应工具执行时自动切换

---

### Phase 3：富媒体 + Office（第 9-12 周）

#### 3.1 富媒体理解

```
新能力（file.read 子能力）：
  ├─ 图片读取   → base64 + dimensions（image crate）
  ├─ PDF 读取   → 文本提取 + 分页（lopdf crate）
  └─ Notebook   → Jupyter .ipynb 解析
```

**依赖**：`image = "0.25"`, `lopdf = "0.34"`, `base64 = "0.22"`

#### 3.2 Office CLI 真实落地

```
改进：
  └─ office.inspect_document  → 真实解析 docx/xlsx/pptx
  └─ office.export_text       → 使用 calamine/docx-rs
  └─ office.patch_dry_run     → 真实 dry-run 预览
  └─ office.apply_patch       → 实际写入（新增）
```

**依赖**：`calamine = "0.24"`, `docx-rs = "0.4"`

---

### Phase 4：沙箱 + Agent UI（第 13-16 周）

#### 4.1 沙箱隔离

```
新能力：
  ├─ worktree.enter     → 创建 git worktree（隔离副本）
  └─ worktree.exit      → 清理 worktree（keep 或 discard 变更）
```

**注意**：仅在 `developer_mode: true` 时启用，默认关闭。

#### 4.2 Agent 团队 UI

```
新 UI 组件：
  ├─ TeamPanel         → 查看团队、成员、状态
  ├─ MailboxView       → 邮箱消息流查看
  ├─ AgentTimeline     → Agent 运行时间线
  └─ ProposalDetail    → 提案详情（工具输入、风险、结果）
```

---

### Phase 5：记忆 + MCP（第 17-20 周）

#### 5.1 记忆系统落地

```
新能力：
  ├─ memory.search      → 语义搜索（需要 embedding provider）
  ├─ memory.set         → KV 写入
  ├─ memory.delete      → 删除条目
  └─ memory.ui          → 管理面板
```

**依赖**：`fastembed = "4"` 或 OpenAI embedding API

#### 5.2 MCP 远程执行

```
改进：
  ├─ mcp.client.connect → 完整的 MCP 客户端握手
  ├─ mcp.client.discover→ 动态发现工具列表
  └─ mcp.server         → Conductor 暴露自己的工具给外部调用
```

---

### Phase 6：智能增强（持续）

#### 6.1 主动预测

```
新能力：
  ├─ initiative 增强    → 从规则引擎 → 轻量 ML 模型
  ├─ 无聊检测          → 长时间无交互时主动问候
  └─ 工作节奏感知      → 根据工作强度调整打扰频率
```

#### 6.2 亲密度系统

```
新能力：
  ├─ affection 分数    → 基于交互频率/质量的长期分数
  ├─ 关系阶段          → 陌生人→同事→朋友→密友
  └─ 性格微调          → 根据亲密度调整说话风格
```

**注意**：这是桌面宠物的灵魂，不应被视为"锦上添花"。Phase 6 是持续迭代，而非最低优先级。

---

## 四、依赖关系图

```
Phase 0（基础设施）
  │
  ├──► Phase 1（代码理解 + 角色表达）
  │       │
  │       ├──► Phase 2（Web + 交互）
  │       │       │
  │       │       └──► Phase 3（富媒体 + Office）
  │       │
  │       └──► Phase 4（沙箱 + Agent UI）
  │
  └──► Phase 5（记忆 + MCP）──► Phase 6（智能增强）
```

**关键约束**：
- Phase 0 是所有后续 Phase 的前置条件（白名单 + 异步化）
- Phase 1 的角色表达增强可与 Phase 2 并行
- Phase 5 不依赖 Phase 2-4，可在 Phase 1 后启动
- Phase 6 是持续迭代，不阻塞其他 Phase

---

## 五、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 工具白名单配置化引入安全漏洞 | 高 | 默认 denylist + 显式 allowlist，不支持通配符提升 |
| 异步化重构破坏现有工具 | 高 | 逐步迁移，先新增 AsyncToolExecutorFn，再迁移旧工具 |
| 角色表达与工具功能冲突 | 中 | formatter 是可选中间件，不修改工具返回值 |
| 文件工具被滥用（写入敏感文件） | 中 | workspace 范围限制 + 提案审批 + 权限检查 |
| Web 工具隐私泄露 | 中 | API key 加密存储 + 域名白名单 + 用户确认 |
