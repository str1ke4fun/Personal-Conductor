# AgentTeam 综合评审报告

> 评审日期: 2026-05-29 | 评审方法: State Machine Lifecycle + OODA-R AgentTeam
> 评审范围: 全项目（Rust 核心、Tauri 桌面端、前端、测试、文档、安全）
> 评审结论: **rework_required** | 生命周期定位: LC-08 → LC-09（未达 LC-10）

---

## 一、评审方法

使用 `state-machine-lifecycle` skill 的 AgentTeam 范式，派遣 4 个专职 Agent 并行评审：

| Agent | 职责 | 审查文件数 |
|-------|------|-----------|
| Architecture Agent | 模块边界、技术选型、可维护性 | 8 |
| Product Agent | PRD 对齐、用户体验、信息架构 | 10 |
| Test Agent | 测试覆盖、质量、关键缺口 | 7 + grep 全局 |
| Review Agent | 安全、状态漂移、边界违反、代码质量 | 8 |

总审查文件 30+，产出 45 项发现（6 critical / 12 high / 15 medium / 12 low）。

---

## 二、总体评价

**项目定位精准，技术选型合理，但核心价值尚未完全交付。**

Personal Conductor 的愿景——"管理人类注意力的 AI agent 调度器"——在当前 AI coding agent 爆发的背景下非常有价值。技术栈（Tauri v2 + Rust + SQLite + Live2D）选型得当，桌面宠物品格系统（情绪 + 好感度 + 空闲衰减）设计精巧。但 PRD 定义的核心功能（时间预算审阅队列）仅部分实现，产品已向"通用桌面伴侣 + 工具调用"方向漂移。

---

## 三、四维评审结论

### 3.1 Architecture Agent — `rework_required`

| 维度 | 评分 | 说明 |
|------|------|------|
| 模块拆分 | B+ | 4 crate 分离合理，但 conductor-core 本身是 23K 行的"上帝 crate" |
| 巨型文件 | **F** | tools.rs 3685行/127KB、chat.rs 2938行/98KB，远超可维护阈值 |
| 全局状态 | C | 4+ 处 lazy_static/RwLock/thread_local，TODO_LIST 全局共享会丢数据 |
| Tauri 解耦 | C | core 通过 feature flag 泄漏 Tauri 类型，CLI 依赖不可控 |
| IPC 层 | C | 81 个 handler 机械重复 `.map_err(|e| e.to_string())` |
| CI/CD | **F** | 完全没有自动化流水线 |
| TS 类型同步 | D | 65+ 手动重复的 TS 接口，无 codegen |

**架构优势：**
- S1: Cargo workspace 4 crate 分离（core/cli/sense/desktop）遵循 Rust 最佳实践
- S2: ToolSpec 的 RiskLevel + ToolPermission + workspace trust 三层安全防护
- S3: spawn_blocking + OnceLock 全局 runtime 桥接 sync/async，避免重复创建
- S4: TestRoot 基础设施（mutex + tempdir + env 隔离）在 22 个文件中一致使用
- S5: ChatMessageV2 对齐 Anthropic ContentBlock API，双向转换路径清晰
- S6: Vite 多页面构建 + invoke.ts 类型化 API 层

**架构问题：**
- C1 [HIGH]: tools.rs 3685 行单文件，47 个工具的注册、执行、校验、测试全在一个文件
- C2 [HIGH]: chat.rs 混合领域类型 + LLM 编排 + DB 持久化 + Tauri 事件，严重违反 SRP
- C3 [HIGH]: 无 CI/CD 流水线
- C4 [MED]: commands.rs 81 个函数机械重复同一模式，无抽象
- C5 [MED]: invoke.ts 700+ 行混装类型定义和 API 调用
- C6 [MED]: conductor-core 通过 `#[cfg(feature = "tauri-events")]` 泄漏 Tauri 类型
- C7 [MED]: 4+ 处全局可变状态（lazy_static/OnceLock/thread_local）散布各处
- C8 [LOW]: IPC 层 `.to_string()` 丢失 anyhow 错误链

**架构气味：**
- AS1: conductor-core 是 34 模块 23K 行的单一编译单元，任何改动触发全量重编译
- AS2: commands.rs 是零逻辑的机械委托层，添加新命令需改 3 处
- AS3: sync/async 阻抗不匹配，`block_on` 嵌套 runtime 有 panic 风险

**技术选型评估：**

| 技术 | 评价 |
|------|------|
| Tauri v2 | **好选择** — 轻量、安全默认、多窗口支持完善 |
| sqlx + SQLite | **好选择** — 编译期查询检查，适合单用户桌面 |
| fastembed | **好选择** — 本地优先，避免云依赖 |
| lazy_static + RwLock | **欠佳** — 应统一到 OnceLock/std |
| anyhow | **可接受** — 但 IPC 边界应用 thiserror |
| 无 TS codegen | **有风险** — 65+ 类型手动同步会漂移 |

**建议优先级：**
1. 拆分 tools.rs 为 tools/ 模块目录（最高 ROI 重构）
2. conductor-core 与 Tauri 解耦（EventEmitter trait）
3. 添加 CI/CD 流水线
4. 拆分 chat.rs（types/persistence/orchestrator）
5. 集成 ts-rs 或 specta 自动生成 TS 类型
6. 全局状态合并为 AppState 结构体
7. IPC 层引入结构化错误类型

---

### 3.2 Product Agent — `rework_required`

| 维度 | 评分 | 说明 |
|------|------|------|
| 人设系统 | **A** | PAD 情绪模型 + 5 阶段好感度 + 空闲衰减，设计精巧 |
| 非打扰设计 | A | 安静模式、气泡通知、用户先发消息 |
| 自然语言交互 | A | "等你看看"、"安静半小时"等口语化标签 |
| 核心价值交付 | **D** | "我有20分钟"杀手功能未实现 |
| 信息架构 | D | 4 个入口窗口 + "(旧)"标签，用户困惑 |
| 新手引导 | **F** | 完全没有 onboarding |
| 认知负荷 | C | 三套任务概念并存，session_id 暴露给用户 |

**产品优势：**
- S1: 情绪系统 — 7 MoodZone（PAD 模型）+ 中文语气提示 + 工具结果措辞注入
- S2: 好感度系统 — 5 阶段（陌生人→密友）+ 迟滞防抖（+3/-3）+ 连续回归奖励
- S3: 非打扰 — 安静模式带倒计时、提议截断为 5 条、30 秒最小检查间隔
- S4: 口语化 — 状态标签"等你看看"/"搞定了"，菜单"安静半小时"

**用户体验问题：**
- UX1 [HIGH]: 任务面板展示 session_id/terminal_id/cwd 等实现细节，PRD 要求 What/Where/Why/Check
- UX2 [HIGH]: 两套任务概念（hook tasks vs agent tasks）边界不清，"整理旧记录"按钮暴露内部迁移
- UX3 [MED]: 菜单标签"待办（旧）"/"聊天（旧）"造成选择焦虑
- UX4 [MED]: 主动触发使用固定 prompt，非上下文化
- UX5 [MED]: config.json 明文存储 API key
- UX6 [LOW]: PetWindow 内联聊天与 ChatPanel 可能创建重复会话

**人设系统评估：**
- 表情系统：expression.rs 实现 7 个 MoodZone，每个有中文语气提示
- 好感度系统：affection.rs 实现 5 个阶段，迟滞防抖，称呼从"您"到"你"
- 空闲衰减：IdlePhase 跟踪 1/5/30 分钟，衰减倍率 1x/2x/5x
- 关注点：人格效果依赖 LLM 对 system prompt 的遵从度，无质量门控

**PRD 功能完成度：**

| PRD 功能 | 状态 | 说明 |
|----------|------|------|
| F1: 时间预算审阅列表 | 部分 | est_minutes 字段存在，但无动态筛选/排序 |
| F2: What/Where/Why/Check 摘要 | 部分 | 有 current_request 和 last_output_summary，缺 "Why" |
| F3: 对话式调度 | **未实现** | 无时间预算感知的任务过滤 |
| F5: 反向派发 | 已实现 | subagent tool tier 存在，但 UI 集成不完整 |
| Codex 集成 | 按计划推迟 | config 中 enabled: false |
| 晨间摘要/离开摘要 | **未实现** | dailyDigest: true 但无实现 |

**建议优先级：**
1. [P0] 统一任务模型 — 合并 tasks/agent tasks/proposals 为单一"审阅队列"
2. [P0] 实现时间预算功能 — "我有20分钟"→ 按 est_minutes 筛选排序
3. [P1] 清除 "(旧)" 标签 — commit 到当前布局或完成 workbench 迁移
4. [P1] 情绪/好感度可视化 — 用户不知道这些系统存在
5. [P2] 最小 onboarding — 首次启动 3 步引导
6. [P2] 离开摘要 — Idle30Min 后生成"你不在时发生了什么"
7. [P3] 主动触发上下文化 — 替换固定 prompt 为基于活动计数的动态消息

---

### 3.3 Test Agent — `rework_required`

| 维度 | 评分 | 说明 |
|------|------|------|
| Rust 单元测试 | B | 234 通过，chat.rs 34 个、tools.rs 20 个质量不错 |
| db.rs 测试 | **F** | 仅 1 个 schema 初始化测试 |
| subagent.rs 测试 | **F** | 零测试 |
| Tauri 后端测试 | **F** | 7 源文件 0 测试 |
| 前端测试 | **F** | 25 文件仅 2 个测试（8%） |
| P0 转换覆盖 | B+ | 主要生命周期转换有测试 |
| 集成测试 | **F** | 无 cross-crate 测试 |

**测试覆盖概览：**
- Rust: 163 sync + 68 async = ~231 个 #[test]，覆盖 38 文件
- conductor-core: 35/40 文件有测试模块（87.5%）
- conductor-cli: 1/1（6 个测试）
- conductor-sense: 2/5（window_title 7 测试、focus 4 测试）
- 前端: 2 个测试文件（stateMap.test.ts、NotificationBubble.test.tsx）
- 验收测试: .acceptance/ 中 ~15 轮手动产物

**测试质量优势：**
- chat.rs: 34 个测试覆盖中文/英文命令解析、时间过滤、任务操作、边界情况
- tools.rs: 20 个测试覆盖注册、执行、风险排序、工作区权限
- conductor-cli: 6 个测试验证 hook 身份解析、session 匹配优先级、源隔离
- codex.rs: 8 个异步测试覆盖 session 生命周期，mutex 串行化
- TestRoot: 全局 mutex + tempdir 模式，22 个文件复用
- conductor-sense: 隐私测试（密码屏蔽、手机号脱敏）

**关键缺口：**

| 优先级 | 模块 | 原因 |
|--------|------|------|
| P0 | db.rs | SQLite 是持久化骨干，仅 1 个测试。Schema 迁移 bug 会静默丢数据 |
| P0 | subagent.rs | 生成外部 Claude 进程，零测试。挂起会阻塞整个 agent 管道 |
| P0 | Tauri 后端 | IPC 桥接是最脆弱的层，7 文件 0 测试 |
| P1 | llm.rs | SSE 流、重试逻辑、provider 切换高风险路径测试不足 |
| P1 | 前端 hooks | useChatSession.ts、usePetVisualState.ts 零测试 |
| P2 | conductor-sense | idle.rs、whitelist.rs 未测试，平台敏感 |
| P2 | inject.rs/summarizer.rs | 核心智能管道测试不足 |

**P0 转换覆盖详情：**

已测试：
- task.pending → in_progress（cli 测试 + chat.rs）
- in_progress → passed/rejected/skipped（chat.rs）
- pending → snoozed + 时间参数（chat.rs）
- 时间过滤守卫（5 个测试：10/20/30 分钟、1 小时）
- 空任务守卫（chat.rs）
- session 重用恢复（cli）
- 源隔离（claude vs codex 不混合）
- workspace 只读阻止写入（tools.rs）

缺失：
- snoozed → pending 定时器到期
- chat session 归档状态转换
- 并发任务修改竞争条件

**建议优先级：**
1. db.rs CRUD 测试（2-3 小时，高风险降低）
2. subagent.rs 测试（2 小时，mock echo 命令）
3. Tauri command 测试（3-4 小时）
4. 前端 useChatSession/invoke 测试（2-3 小时）
5. vitest 覆盖率阈值（1 小时）
6. TestRoot 去重（30 分钟）

---

### 3.4 Review Agent — `rework_required`

| 维度 | 评分 | 说明 |
|------|------|------|
| Shell 安全 | **D** | blocklist 可绕过，working_dir 未校验 |
| 错误处理 | C | IPC 层全量 `.to_string()` 丢失错误链 |
| 状态漂移 | C | TODO_LIST 全局内存态、工具注册重复调用 |
| 边界违反 | C | chat.rs 导入 10 个模块，IPC 混入 avatar 逻辑 |
| 潜在 Bug | C | normalize_existing_or_virtual 可能无限递归 |

**安全问题：**
- SC-1 [HIGH]: Shell security.rs 使用 blocklist + 子串匹配，编码绕过、环境变量展开、base64 payload 均可规避。working_dir 未在安全模块校验
- SC-2 [MED]: execute_bash_tool 校验 working_dir 但 shell 安全模块不校验，LLM 可设置 C:\Windows 执行命令
- SC-3 [MED]: TOOL_REGISTRY 全局 RwLock 每次 `unwrap()`，poisoned mutex 会导致应用崩溃
- SC-4 [LOW]: subagent 的 claude -p prompt 未消毒，存在 prompt injection 风险

**错误处理：**
- EH-1 [MED]: commands.rs 所有 handler 使用 `.map_err(|err| err.to_string())`，丢失错误上下文
- EH-2 [LOW]: llm.rs 仅重试 401/403/429，不重试 502/503/504
- EH-3 [LOW]: 30s 单次超时 vs 120s 总体超时的关系未文档化

**状态漂移：**
- SD-1 [HIGH]: TODO_LIST 是进程全局 `RwLock<Vec<Value>>`，重启丢失，多会话互相覆盖
- SD-2 [MED]: register_builtin_tools() 在每次 send_chat_message 时调用，不必要地获取写锁
- SD-3 [MED]: ChatSession 内存消息与 DB 历史可能分歧

**边界违反：**
- BV-1 [MED]: chat.rs 导入 affection/avatar/chat_parser/config/db/expression/llm/memory/tasks/tools 共 10 个模块
- BV-2 [LOW]: send_chat_message IPC handler 直接操作 avatar 状态

**代码组织：**
- CO-1 [HIGH]: 单文件大小极端（tools.rs 3685 行、chat.rs 2938 行、llm.rs 2087 行）
- CO-2 [MED]: 工具注册模式冗长，每个工具 ~30 行内联 JSON schema
- CO-3 [LOW]: glob_to_regex 手动实现，应使用 globset crate

**潜在 Bug：**
- PB-1 [MED]: normalize_existing_or_virtual 递归上溯路径，深层不存在路径可能栈溢出
- PB-2 [MED]: to_v2 静默将非 JSON 内容包装为 Text block，无验证
- PB-3 [LOW]: duration_ms u128→u64 强转
- PB-4 [LOW]: list_tools 双重 clone

**建议优先级：**
1. Shell 安全：blocklist → allowlist + working_dir 校验
2. tools.rs 拆分为 per-category 模块
3. TODO_LIST 移入数据库
4. IPC 结构化错误类型
5. llm.rs 添加 5xx 重试
6. register_builtin_tools 移至启动时

---

## 四、关键发现汇总

### P0 — 必须修复

| # | 来源 | 问题 | 影响 |
|---|------|------|------|
| 1 | Product | 时间预算审阅队列未实现 | PRD 核心价值未交付 |
| 2 | Review | Shell 安全 blocklist 可绕过 | LLM 工具可执行任意命令 |
| 3 | Test | db.rs 几乎无测试 | Schema 变更静默丢数据 |
| 4 | Test | subagent.rs 零测试 | 子进程挂起阻塞 agent 管道 |
| 5 | Review | TODO_LIST 全局共享 + 内存态 | 多会话覆盖，重启丢失 |
| 6 | Architecture | tools.rs 3685 行单文件 | 不可维护 |

### P1 — 强烈建议

| # | 来源 | 问题 |
|---|------|------|
| 7 | Product | 三套任务概念需统一 |
| 8 | Product | "(旧)"标签需清除 |
| 9 | Architecture | conductor-core 与 Tauri 解耦 |
| 10 | Architecture | 添加 CI/CD 流水线 |
| 11 | Product | 情绪/好感度对用户不可见 |
| 12 | Architecture | chat.rs 拆分 |
| 13 | Review | IPC 结构化错误类型 |

### P2 — 建议改进

| # | 来源 | 问题 |
|---|------|------|
| 14 | Product | 添加 onboarding |
| 15 | Product | 实现离开摘要 |
| 16 | Architecture | TS 类型 codegen |
| 17 | Architecture | 全局状态合并为 AppState |
| 18 | Test | 前端 hook 测试 |
| 19 | Product | 主动触发上下文化 |

---

## 五、亮点

1. **人设系统** — PAD 情绪模型 + 好感度迟滞 + 空闲衰减 + 口语化语气提示，同类项目少见的深度设计
2. **非打扰原则** — 安静模式、气泡通知、用户先发消息，严格遵守 PRD 硬约束
3. **工具风险分级** — ToolSpec 的 RiskLevel + ToolPermission + workspace trust 三层防护
4. **TestRoot 基础设施** — 22 个文件复用的 mutex + tempdir 隔离方案
5. **Hook 集成** — 12 种 Claude Code 生命周期事件 + Codex hook，真正的 agent-native 架构

---

## 六、下一步路线

### 阶段 1：止血（1-2 周）

1. Shell 安全加固：blocklist → allowlist，working_dir 校验移入安全模块
2. TODO_LIST 移入数据库：消除全局内存态，按 session 隔离
3. db.rs 补测试：chat_messages CRUD、task 持久化、migration 幂等性
4. subagent.rs 补测试：mock echo 命令测 semaphore、timeout、cleanup

### 阶段 2：核心价值交付（2-4 周）

5. 统一任务模型：合并 tasks/agent tasks/proposals 为单一"审阅队列"
6. 实现时间预算功能："我有20分钟"→ 按 est_minutes 筛选排序
7. 清除 "(旧)" 标签：完成 workbench 迁移或去掉标签
8. 添加 CI：cargo test + clippy + tsc --noEmit + npm test

### 阶段 3：架构治理（4-8 周）

9. tools.rs 拆分：按 category 拆为 tools/*.rs，引入注册宏
10. chat.rs 拆分：types / persistence / orchestrator / streaming
11. Tauri 解耦：EventEmitter trait 替代 feature flag
12. IPC 错误类型：thiserror enum 替代 `.to_string()`
13. TS 类型 codegen：ts-rs 或 specta

### 阶段 4：体验提升（持续）

14. Onboarding：首次启动 3 步引导
15. 情绪可视化：桌宠窗口显示心情/关系状态
16. 离开摘要：Idle30Min 后生成"你不在时发生了什么"
17. 主动触发上下文化：基于活动计数的动态消息

---

## 七、附录

### 评审工具链

- 评审框架: state-machine-lifecycle skill (OODA-R AgentTeam)
- 生命周期定位: LC-08 In Development → LC-09 Integrated
- 证据分类: observed / inferred / assumed / unknown

### 评审覆盖的文件

**Architecture Agent:**
- Cargo.toml, crates/conductor-core/src/lib.rs, tools.rs, chat.rs
- apps/desktop/src-tauri/src/main.rs, commands.rs, vite.config.ts, invoke.ts

**Product Agent:**
- docs/PRD.md, 当前架构说明-用户操作流程-20260526.md
- PetWindow.tsx, ChatPanel.tsx, SettingsPanel.tsx, TaskPanelContent.tsx
- state/config.json, expression.rs, affection.rs, initiative.rs

**Test Agent:**
- lib.rs (test harness), db.rs, chat.rs, tools.rs (test modules)
- stateMap.test.ts, NotificationBubble.test.tsx, vitest.config.ts
- Grep: #[cfg(test)] 覆盖率扫描

**Review Agent:**
- shell/ (security), tools.rs, chat.rs, commands.rs, llm.rs
- memory.rs, agent_teams.rs, app.css
