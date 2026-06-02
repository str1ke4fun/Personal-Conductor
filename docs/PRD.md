# Personal Conductor Agent — PRD

> 一个完全为「人」服务的个人桌宠 Agent。它不写代码、不写文档，它只做一件事：**让人不被多个并行 agent 的产出淹没**。

---

## 1. 背景与痛点

### 1.1 现状

我同时在用多个专业 agent 帮我干活：

- **Claude Code** 接入了「写文档 agent」——长期跟进文档任务（PRD、说明、PPT 文案等）。
- **Codex** 接入了「写开发 agent」——长期跟进一项开发任务。
- 同一时间窗口内可能还在并行处理 2–3 个临时文档任务。

这些 agent **产出速度远快于我审阅的速度**。它们不是「跑完一次就结束」的工具，而是会持续吐出需要人确认的中间产物：一段文字、一页 PPT、一段实现、一个测试步骤。

### 1.2 痛点

1. **审阅排队失控**：多个 agent 同时完成，我不知道该先看哪个。
2. **上下文切换贵**：从「文档 A 的第 3 段」切到「PPT B 的第 2 页」再切到「开发任务 C 的测试」，每次切换都要重新加载心智上下文。
3. **摘要靠人脑**：每个 agent 输出一堆东西，我得自己读完才知道它干了什么、和我之前要的是不是一回事。
4. **没有时间感**：我可能在某个 agent 上花了 40 分钟，结果别的 agent 早就完成了堆在那等我。
5. **更远的痛**：除了 Claude/Codex，我还在用浏览器、IDE、聊天工具，工作进度散落在不同应用里，没有一个统一的「人这边今天发生了什么」的视图。

### 1.3 一句话定义这个项目

> 给「人」配一个 agent。它不干活，它只负责**调度人的时间、压缩别的 agent 的产出、在合适的时机提醒人该看什么**。

---

## 2. 目标用户与使用场景

### 2.1 用户

**只有一个用户：我自己。**

这不是 SaaS，不是给团队用的，不需要权限模型、不需要多租户。一切设计都为单人优化。

### 2.2 典型一天

> 早上 9:00，我打开电脑。Conductor 已经在后台跑着。它看了一眼过去 12 小时的 Claude Code Stop hook 日志，知道夜里 Claude 完成了两个文档段落和一页 PPT。它生成了一份「今早待审阅清单」：
>
> - **[15 min]** 审阅文档 A 第 3 段（Claude 已重写，重点看是否与第 2 段口径一致）
> - **[10 min]** 审阅 PPT B 第 5 页（Claude 已基于文档 A 主题 X 生成，重点看图文匹配）
> - **[20 min]** 测试开发任务 C 的原子能力 `parseConfig()`（Codex 已实现，测试步骤：1. 准备畸形 yaml 2. 调用 ...）
>
> 我点开第一项，审完，跟 Conductor 说「OK 第一项过了，第二项我想先放放，先做开发测试」。Conductor 更新清单，把 PPT 挪后，开发测试提前。
>
> 中午我离开电脑去吃饭。回来时 Conductor 告诉我：「你离开的 47 分钟里，Claude 又完成了文档 D 的初稿（约 1200 字），主题是 ...，建议你下午第二个时间块审阅。」

这就是它该有的样子。

---

## 3. 产品边界（很重要）

### 3.1 它**做什么**

- 维护一个**人的 1 小时审阅 task list**，根据对话动态更新。
- 监听其他 agent 的产出事件（先从 Claude Code 开始），自动**生成摘要**。
- 摘要里包含「关联上下文」：这段文字属于哪个文档？这页 PPT 对应哪个主题？这段代码对应哪个原子能力？测试步骤是什么？
- 在合适时机**主动提醒**人：「该看 X 了」「Y 已完成堆积了 N 个待审项」。

### 3.2 它**不做什么**

- 不替人审阅，不替人决策「这段文档写得好不好」。
- 不直接修改其他 agent 的产出（除非人明确指示「让 Claude 重写这段」，由 Conductor 转发指令）。
- MVP 阶段**不接 Codex**，先把 Claude 这条链路跑通。

### 3.3 远期愿景（不在 MVP 范围）

像一个**桌宠**：常驻桌面角落，能看到我在哪个应用、做什么进度，能跨应用提醒（「你已经在浏览器看了 20 分钟，文档 A 还在等你」）。这部分见 `感知层调研.md`。

---

## 4. 核心功能（MVP）

### 4.1 F1：审阅 Task List

- 维护一份**当前 1 小时内**的 task list，存为本地文件（markdown 或 JSON，详见技术架构）。
- 每条 task 包含：
  - 来源 agent（claude / codex / 手动添加）
  - 关联产物（文件路径 / Claude 会话 ID / 片段引用）
  - 预估审阅时长
  - 重点关注什么（由 Conductor 生成的 1–2 句话提示）
  - 状态（待审 / 进行中 / 通过 / 打回重做 / 跳过）
- 人可以用自然语言更新：「第一项过了」「PPT 那个推后」「现在我要专心做开发测试 30 分钟，别打扰我」。

### 4.2 F2：Claude Code 任务状态与自动摘要

- 通过 Claude Code hook 触发，但任务从 **UserPromptSubmit** 开始，而不是等 Stop 后才出现。
- `UserPromptSubmit` 触发时，Conductor 读取用户 prompt、`cwd`、`session_id`、`transcript_path` 和终端/进程标识，创建或更新一条 `in_progress` 任务。
- `Stop` / `SubagentStop` / 权限请求相关事件触发时，Conductor 读取同一会话/终端的上下文，把任务更新为 `pending`（产品文案显示“待审”）。
- 摘要的结构化字段：
  - **What**：这次 agent 干了什么（一句话）
  - **Where**：产物位置（文件路径 / 行号 / 段落）
  - **Why it matters**：和已有上下文的关系（「这段是对昨天文档 A 第 2 段的扩写」）
  - **What you should check**：人需要重点看什么（1–3 条）
- 摘要的生成方式：Conductor 读 hook 传入的 `transcript_path`、用户请求、权限请求摘要和最近改动文件，优先快速生成可展示摘要；重型 LLM 摘要必须异步或快速失败，不能拖慢 Claude Code。

### 4.3 F3：对话式调度

- 我可以随时跟 Conductor 说话，它据此 update task list。
- 关键交互：
  - 「现在有什么要看的？」→ 列出 top 3
  - 「我有 20 分钟」→ 按时长筛选最匹配的任务
  - 「这个先跳过」「让 Claude 重写这段」→ 状态更新 / 转发指令
- 调度上下文（人的偏好、当前精力状态）保存为人的「档案」，长期累积。

### 4.4 F4：UserPromptSubmit Hook 任务开始记录

- 当我在 Claude Code 里提交 prompt 时，hook 只做旁路状态同步：记录“哪个终端/会话开始推进什么任务”，并把任务卡显示为“推进中”。
- Hook 默认不向 Claude Code 写入 `additionalContext`，不改写 prompt，不要求 Claude 等待 Conductor，也不要求用户额外确认。
- 等代码侧闭环验证通过后，才由我显式把非打扰 hook 启用到 `.claude/settings.json`。当前空 hook 只是临时保护，避免未验证实现影响我正常使用 Claude Code。
- 未来如果要做“给 Claude 的短上下文提示”，必须作为独立开关启用，并且仍然不能阻塞或改变用户原始 prompt 的主流程。

---

## 5. 体验目标（纯定性）

成功的标志，全凭我自己感觉。**不设量化指标**。大致是这些感受：

- 早上打开电脑，**不再发懵**「我昨天到哪了」。
- 切任务时，**不再花 5 分钟回忆上下文**——Conductor 的摘要直接帮我加载好。
- 不再有「啊原来 Claude 半小时前就写完了我都没看到」的事故。
- 跟 Conductor 说话的感觉是**自然的**，不像在填表。
- 最理想：感觉自己**比以前从容**，哪怕实际做的事一样多。

---

## 6. 范围与优先级

| 优先级 | 功能 | MVP? |
|--------|------|------|
| P0 | F1 审阅 task list（本地文件 + 自然语言更新） | ✅ |
| P0 | F2 Claude hook 任务状态与自动摘要（Claude 侧） | ✅ |
| P0 | F3 对话式调度 | ✅ |
| P0 | F5 反向调度：`claude -p` 子进程下发下一步指令（首批先做） | ✅ |
| P1 | F4 UserPromptSubmit hook 任务开始记录 | ✅ |
| P1 | F6 复合调度循环：事件触发（hook）+ 定时巡检（卡住/堆积/空闲） | ✅ |
| P1 | F7 反向调度备选：写文件让下一次会话自然带入 | ⏳ 留方案，择机推进 |
| P2 | 桌宠形态：Tauri v2 + Live2D 桌面壳 | ❌ 验证完 P0 后立刻做 |
| P2 | 接入 Codex 一侧 | ❌ 推迟 |
| P2 | 跨应用感知（OS hook + 白名单 / 多模态兜底） | ❌ 调研中，见专门文档 |
| P3 | 跨设备同步 / 移动端提醒 | ❌ 远期 |

---

## 7. 已消除的分歧（决策记录）

以下是反复讨论后**已经拍板**的事项，写进来避免后续来回反复：

1. **桌宠形态 = Live2D**：不做 Toast 替代版，MVP 就直接上 Tauri v2 + Live2D 桌宠。资产准备与自学路径见 `docs/Live2D-资产准备.md`。
2. **跨应用感知 = 三条腿复合**：事件触发（hook / SDK）为主 + 定时巡检（按节拍盘点堆积/阻塞）为辅 + 多模态截图兜底（仅在前两层看不懂时启用）。详见 `docs/感知层调研.md` §3 与 §5。
3. **Conductor → Claude Code 反向写命令 = 复合方案**：
   - **P0 路线**：`claude -p` 子进程（非交互式、最早实现）。
   - **保留路线**：写文件让下次会话自然带入（不绑定 Claude Code 私有目录，走项目内 `state/proposed_prompts/`）。
   - **永远不做**：GUI SendInput 注入。
   - 详见 `docs/感知层调研.md` §4。

## 8. 剩余开放问题

1. **摘要的「准」与「省 token」的平衡**：每次都跑一次 LLM 会贵，纯模板又不够智能。Week 1 先模板兜底，Week 2 上 Haiku。
2. **打扰策略具体阈值**：堆积多少分钟该跨应用打断？空闲多少分钟该提醒回来？需要自己用一周后用真实数据校准。
3. **Live2D 模型最终选哪一个**：在自学完 Cubism 基本流程后再决定，先用免费样例模型跑通管道。

---

## 9. 相关文档

- `docs/MVP-实施方案.md` — 三周内能跑起来的最小闭环
- `docs/技术架构.md` — 数据流、文件结构、hook 接入细节
- `docs/技术栈选型.md` — task list 弹窗、桌宠、提醒能力的桌面技术选型
- `docs/感知层调研.md` — 跨应用感知的方案对比与可行性

---

---

## 10. 2026-05-21 修订：Claude Code Hook 产品流程

本节覆盖 PRD 中旧的 F2 / F4 Claude hook 描述。MVP 只做 Claude Code 版本，Codex 延后。

### 10.1 核心产品判断

任务不应等 Claude Code 完成后才出现。只要用户在 Claude Code 里提交 prompt，Conductor 就应该知道“一个任务开始了”，并在任务列表中显示。

因此：

- `UserPromptSubmit` = 创建/更新进行中任务。
- `Stop` / `SubagentStop` / 权限相关事件 = 更新该任务为待审。
- Hook 不打扰 Claude Code，只做状态和上下文读取。

### 10.2 用户可见流程

1. 我在某个 Claude Code 终端提交 prompt。
2. 桌宠任务列表立即出现一条“推进中”任务。
3. 任务卡片显示哪个 Claude Code 终端/会话、我刚提交的任务摘要、所在仓库/cwd、当前上下文摘要。
4. Claude Code 请求提权或完成一轮输出时，Conductor 更新同一张任务卡。
5. 任务卡显示“Claude 已完成了什么”“改了哪里”“我需要检查什么”。
6. 状态变为“待审”。
7. 我在 Conductor 中 pass / reject / skip。

### 10.3 功能需求调整

#### F2：UserPromptSubmit 任务创建

- 触发时机：Claude Code `UserPromptSubmit`。
- 输入：hook payload、用户 prompt 摘要、cwd、session_id、terminal/process hint、transcript_path。
- 输出：一条 `in_progress` 任务。
- UI：任务列表和桌宠状态显示“Claude 正在推进中”。
- 不允许：默认注入额外上下文影响 Claude Code 回答。

#### F4：完成/提权事件更新为待审

- 触发时机：Claude Code `Stop`、`SubagentStop`、权限请求相关 hook。
- 输入：transcript tail、最近改动文件、权限请求摘要、cwd、session_id。
- 输出：更新同一任务，状态设为 `pending`。
- UI 文案：`pending` 显示为“待审”。
- 摘要内容：Claude 已完成什么、产物位置或改动文件、需要人检查什么、是否发生过提权请求。

### 10.4 多终端任务归属

PRD 要求支持多个 Claude Code 终端并行。任务必须保存：

- source：`claude`
- session_id
- terminal_id 或 process hint
- cwd
- created_at / last_event_at
- 当前用户请求摘要
- 最近 Claude 输出摘要

匹配同一任务时优先使用 `session_id`，其次使用 terminal/process hint，最后才允许使用 `cwd + 时间窗口`。

### 10.5 非打扰约束

这是硬约束：

- Hook 不弹窗。
- Hook 不阻塞 Claude Code 正常工作。
- Hook 不要求用户在 Claude Code 内做额外确认。
- Hook 不默认写入 `additionalContext`。
- Hook 失败时只记录日志，不中断 Claude Code。
- `.claude/settings.json` 当前保持空 hook 只是过渡保护：代码侧闭环未验证前，不启用会影响 Claude Code 正常工作的 hook。最终目标仍然是实现并显式启用非打扰 hook。

### 10.6 启用闸门

Hook 实现必须先在代码侧闭环通过以下检查，再写入 `.claude/settings.json`：

- `UserPromptSubmit` 能创建/更新 `in_progress` 任务，并能展示当前请求摘要、cwd、session_id、终端/进程标识。
- `Stop` / `SubagentStop` / 权限请求相关事件能更新同一任务为 `pending`，并展示 Claude 已完成内容、产物位置和待检查点。
- 多个 Claude Code 终端在同一仓库并行时，不会只按 cwd 错误合并任务。
- Hook 失败时只记录日志，不阻塞、不弹窗、不向 Claude Code 注入额外上下文。

---

*Owner: 我自己 / Last updated: 2026-05-21*
