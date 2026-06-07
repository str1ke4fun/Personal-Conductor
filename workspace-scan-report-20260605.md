# 工作区快速扫描报告

> 扫描时间: 2026-06-05 | 扫描范围: `I:\personal-agent`

---

## 一、项目定位

**Personal Conductor** — 一个面向 Windows 的桌宠式桌面搭子（桌宠）。  
核心角色为 **"清和"**（穿藏青旗袍的桌面助手），定位不是通用编码助手，而是**有性格的桌面伴生 AI Agent**。

---

## 二、技术栈与模块划分

### 技术栈

| 层 | 技术选型 |
|---|---------|
| 桌面壳 | Tauri (Rust 后端 + WebView 前端) |
| 前端 | React + Vite + TypeScript |
| 后端核心 | Rust (conductor-core, 约 21K 行 / 40 文件) |
| CLI | Rust (conductor-cli) |
| 感知层 | Rust (conductor-sense, Win32 API) |
| 存储 | SQLite (state/conductor.sqlite) |
| 事件日志 | NDJSON (state/events.ndjson) |
| 模型路由 | 新增 model-router-core / model-router-mcp crate |
| 任务系统 | TaskList V1/V2 (SQLite) |
| 表情/头像 | PNG + 子形象状态机 (Live2D 路线已暂停) |
| 打包 | zip 便携包 (非安装器) |

### 模块划分（4 crate + 1 app）

```
personal-agent/
├── apps/desktop/          # Tauri 桌面应用 (React + Vite)
│   ├── src/               # 前端组件 (PetWindow, TaskPanel, ChatPanel 等)
│   └── src-tauri/src/     # Tauri 后端 (commands, tray, worker)
├── crates/
│   ├── conductor-core/    # 核心运行时 (聊天/工具/记忆/任务/Agent/Goal/OODA)
│   ├── conductor-cli/     # CLI 入口
│   ├── conductor-sense/   # 前台感知 (焦点/空闲/窗口标题)
│   ├── model-router-core/ # 模型路由核心 (新增)
│   └── model-router-mcp/  # MCP 路由服务器 (新增)
├── docs/                  # 73 个文档文件
├── release/               # 便携版骨架
├── scripts/               # 构建/打包/辅助脚本
├── tools/                 # 工具目录
├── wiki/                  # Wiki 资料
├── skills/                # 技能系统
```

---

## 三、当前文档完整度

### 3.1 文档体系

`docs/INDEX.md` 维护了完整的文档索引（280 行），文档按分类组织：

| 分类 | 数量 | 同步状态 |
|------|------|---------|
| 📐 架构与规划 | 5 | ✅3 🟡2 |
| 📋 实施方案与派工 | 9 | ✅7 🟡2 |
| 📐 功能规格 | 11 | 🟡5 📋6 |
| 🤝 交接文档 | 2 | ✅2 |
| 🎨 Live2D 相关 | 5 | ⏸️ 暂停 |
| 📊 评审与审计 | 1 | ✅1 |
| 📚 参考与调研 | 5 | ✅3 🟡2 |
| 📦 发布与路线 | 2 | 📋规划中 |

### 3.2 核心文档

| 文档 | 内容要点 |
|------|---------|
| **README.md / README.zh-CN.md** | 项目简介、仓库布局、开发指引、打包说明 |
| **PRD.md** | 产品需求文档 (🟡 部分完成) |
| **技术架构.md** | Rust+SQLite+Tauri 架构 (✅ 完全同步) |
| **技术栈选型.md** | 技术选型说明 (✅ 已更新) |
| **CHANGES.md** | 变更记录：一体化交互重构、纯观察模式重构 |
| **project-evolution-roadmap.md** | 演进路线图：Phase 0-5 (🟡 部分完成) |
| **UnifiedFinalEvolution-20260604.md** | 三方案归一化最终演进形态 (1766 行) |
| **UnifiedFinalEvolution-goal-task-flow-dispatch-20260605.md** | Goal 任务流缺口派工 (555 行) |
| **UnifiedFinalEvolution落地状态评估-20260605.md** | 落地状态评估 (572 行) |

---

## 四、项目当前状态

### 4.1 已实现的核心能力

| 能力 | 状态 |
|------|------|
| 桌宠窗口 + 托盘集成 | ✅ 完成 |
| 三种面板（任务/对话/设置） | ✅ 完成 |
| 一体化交互界面（内嵌面板替代多窗口） | ✅ 完成 |
| 快捷键支持（Ctrl+Shift+T/C/Q, ESC） | ✅ 完成 |
| 状态指示器（working/update/idle/quiet） | ✅ 完成 |
| 专注模式倒计时 | ✅ 完成 |
| 47 个内置工具注册（含 file.*/codex.*/agent.*/web.fetch 等） | ✅ 完成 |
| bash.execute Shell 工具（cmd/powershell/bash） | ✅ 完成 |
| Codex session 管理（start/stop/resume/send_input 等） | ✅ 完成 |
| Agent 团队 + 邮箱通信 | ✅ 完成 |
| Goal/OODA 任务系统 | ✅ 完成 |
| 记忆系统（KV + chunks/embeddings 表） | ✅ 后端完成 |
| 角色/情绪/好感度系统 | ✅ 后端完成 |
| 感知层（焦点/空闲/窗口标题） | ✅ 完成 |
| 主动对话引擎 (initiative) | ✅ 完成 |
| 测试总数 234 (229 passed / 1 failed BUG-008) | 🟡 基本通过 |
| MVP 最小闭环 | ✅ 实现 |
| Round 1-4 全部/部分完成 | ✅ 完成 |

### 4.2 已识别但未解决的问题

| 问题 | 优先级 | 说明 |
|------|--------|------|
| **P0: LLM 工具白名单** | **P0** | 硬编码仅 4 个工具暴露给 LLM，新工具不可见 |
| **P0: 执行器异步化** | **P0** | 每个 executor 创建独立 Runtime |
| **P0: ChatTurn 锚点缺失** | **P0** | goal_id/goal_cycle_id/agent_task_id 未传入 |
| **P0: 主线程 LLM 续轮** | **P0** | subagent 结果未触发 LLM continuation |
| **P0: 测试未全绿** | **P0** | cargo test -p conductor-core 编译失败 |
| P1: 代码理解工具 | P1 | file.glob/grep/edit 已注册但 LLM 不可见 |
| P1: Web 搜索能力 | P1 | web.fetch 已注册但 LLM 不可见 |
| P1: 角色表达反馈 | P1 | 工具结果未角色化表达 |
| P2: 富媒体处理 | P2 | 图片/PDF/Notebook 未实现 |
| P2: 交互式 UX | P2 | 缺少 AskUserQuestion 多选交互 |
| P3: Embedding 检索 | P3 | 表结构已建，无实际检索 |
| P3: UI 完备性 | P3 | Workspace/Proposal/Memory/Team UI 均有缺口 |

### 4.3 近期重点工作

根据 `UnifiedFinalEvolution` 系列文档，当前正处于 **Phase A0 验收+Phase A 推进** 的关口：

1. **A0 门禁闭合**: LLM 续轮、测试全绿、ChatTurn 锚点注入
2. **ModelResolver 统一收口**: profile 配置真正生效
3. **OODA/Cairn Reason 落地**: decide.rs 从程序化→LLM 决策
4. **事件总线统一**: chat_turn_events 作为单一事件总线
5. **MCP 抽离**: model-router-mcp 独立 server

### 4.4 明显缺失的信息

| 缺失项 | 影响 |
|--------|------|
| 💥 LLM 白名单至今未配置化 | 所有新工具对 LLM 不可见，清和实际上只能调 4 个工具 |
| 🔴 测试门禁未通过 | 无法可靠进行回归验证 |
| 📋 前端 Workspace/Proposal/Memory/Team UI | 高级功能无用户交互入口 |
| 🔗 ChatTurn ↔ GoalCycle 锚点缺失 | 无法追踪 Goal 任务到聊天会话 |
| 🧪 embedding 检索未实现 | 记忆系统只有存储无语义搜索 |
| 📄 Office 工具为占位实现 | 无法真实读写文档 |
| 🖼️ 情绪素材图未产出 | 子形象表情变化无实际图片 |
| 📦 正式安装器未规划 | 目前仅有 zip 便携包 |

---

## 五、关键文件清单

### 源文件（Rust 核心）

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/conductor-core/src/lib.rs` | - | 核心库入口 |
| `crates/conductor-core/src/chat/mod.rs` + 子模块 | ~1991 | 对话系统 + LLM 工具调用 |
| `crates/conductor-core/src/tools/` (8 文件) | ~3555 | 工具注册表 + 各工具实现 |
| `crates/conductor-core/src/llm.rs` | ~1255 | LLM 请求封装 |
| `crates/conductor-core/src/mcp.rs` | ~974 | MCP 适配层 |
| `crates/conductor-core/src/codex.rs` | ~756 | Codex session 管理 |
| `crates/conductor-core/src/avatar.rs` | ~647 | 头像/子形象系统 |
| `crates/conductor-core/src/expression.rs` | ~685 | 情绪/表情系统 |
| `crates/conductor-core/src/goal_orchestrator/` (8 文件) | - | OODA 任务编排 |
| `crates/conductor-core/src/model_resolver.rs` | 新增 | 模型路由解析器 |
| `crates/conductor-core/src/smart_monitor.rs` | ~409 | LLM 驱动的主动通知决策 |

### 前端文件

| 文件 | 说明 |
|------|------|
| `apps/desktop/src/windows/PetWindow.tsx` | 主桌宠窗口（双模式/三标签/状态指示） |
| `apps/desktop/src/windows/TaskPanelContent.tsx` | 任务面板 |
| `apps/desktop/src/windows/ChatPanel.tsx` | 对话面板 |
| `apps/desktop/src/windows/SettingsPanel.tsx` | 设置面板 |
| `apps/desktop/src/windows/OodaTimeline.tsx` | OODA 时间线 |
| `apps/desktop/src-tauri/src/worker.rs` | 后端工作线程 (GoalTask -> Chat executor) |

### 配置/构建

| 文件 | 说明 |
|------|------|
| `Cargo.toml` | Rust 工作区配置 |
| `package.json` | npm 配置 |
| `rust-toolchain.toml` | Rust 工具链配置 |
| `dev.ps1` | 开发启动脚本 |
| `start-dev.bat` | 批处理启动入口 |
| `.gitignore` | Git 忽略规则 |
| `.claude/settings.json` | Claude 桌面端设置 |
| `.codex/hooks.json` | Codex 钩子配置 |

### 文档（最关键）

| 文件 | 说明 |
|------|------|
| `docs/INDEX.md` | 文档索引 + 同步状态 |
| `docs/UnifiedFinalEvolution-20260604.md` | 最终演进形态建模 (1766 行) |
| `docs/UnifiedFinalEvolution落地状态评估-20260605.md` | 落地状态评估 (572 行) |
| `docs/UnifiedFinalEvolution-goal-task-flow-dispatch-20260605.md` | Goal 任务流缺口派工 (555 行) |
| `docs/project-evolution-roadmap.md` | 演进路线图 Phase 0-5 (413 行) |

---

## 六、下一步演进计划（推荐）

### 短期（当前 Sprint）

1. **闭合 A0 门禁**（GB0/GB1）
   - 修复 ChatTurn 锚点缺失（P0-A 派工）
   - 修复测试编译失败（routing + example）
   - 实现主线程 LLM 续轮（agent_runs → send_v2 续轮）

2. **LLM 白名单配置化**
   - `CoreConfig.llm_tools.allowlist` 取代硬编码
   - 解锁 file.*/web.*/memory.* 工具对 LLM 可见

### 中期（Phase 1-2）

3. **ModelResolver 完整收口**
   - `ResolvedModel` 携带 endpoint/key/temperature
   - 所有 LLM 调用统一走 resolve()

4. **OODA/Cairn Reason 落地**
   - decide.rs 接入 LLM Reason
   - contracts.rs 契约进入主流程

5. **角色表达增强**
   - 工具结果角色化格式化
   - 新增 ActivityVariant (Reading/Searching/Celebrating)

6. **事件总线统一**
   - chat_turn_events 作为单一跨层审计总线

### 长期（Phase 3-5）

7. **MCP 抽离**(model-router-mcp 独立 server)
8. **富媒体理解**(图片/PDF/Notebook)
9. **Office 工具真实落地**
10. **Embedding 检索全线接通**
11. **UI 完备化**(Workspace/Proposal/Memory/Team/Reasoning tab)
12. **沙箱隔离**(Worktree)