# MVP 实施方案 — Personal Conductor Agent

> 目标：**三周内**跑通 Claude Code 一侧的最小闭环。Codex 侧、桌宠形态都推迟。
> 验收标准（定性）：早上打开电脑，能从 Conductor 那里拿到一份「今早该审什么」的清单，并且这份清单是它**自己根据夜里 Claude 的产出生成**的，不是我手填的。

---

## 1. MVP 的最小闭环

```
Claude Code UserPromptSubmit
        │
        ▼
Conductor 读用户请求、cwd、session_id、transcript_path、终端/进程标识
        │
        ▼
创建或更新 task list 里的 in_progress 任务
        │
        ▼
任务列表显示“哪个终端正在推进什么”
        │
        ▼
Claude Code Stop / SubagentStop / 权限请求相关事件
        │
        ▼
Conductor 读 transcript tail、最近改动文件、权限请求摘要
        │
        ▼
更新同一任务为 pending（待审），展示 Claude 已完成什么
        │
        ▼
人打开 Conductor 对话端 → pass / reject / skip / 自然语言更新
```

跑通这一圈，MVP 就算成立。

---

## 2. 三周排期

### Week 1：管道打通（不追求智能）

- [ ] 在 `I:\personal-agent\` 下立项目骨架（见技术架构文档）。
- [ ] 写 UserPromptSubmit hook 入口：被 Claude Code 调用时，把用户请求、`cwd`、`session_id`、`transcript_path`、终端/进程标识追加到 `events.ndjson`，并创建/更新 `in_progress` 任务。
- [ ] 写 Stop / SubagentStop / 权限请求相关 hook 入口：读取同一会话/终端对应任务，记录 transcript tail、最近改动文件、权限请求摘要，并把任务更新为 `pending`。
- [ ] 写一个最弱的「摘要器」：先**纯规则**（看用户请求、transcript 末尾几条 + 文件名）拼一句话摘要，写入任务上下文。
- [ ] 写最简单的 CLI：`conductor list` 看当前 task list。

**Week 1 验收**：提交一次 Claude Code prompt 后，任务列表立刻多出一条 `in_progress` 任务；Claude 完成或请求提权后，同一任务变成 `pending`（待审），哪怕摘要写得很拙。

### Week 2：摘要变聪明 + 对话式更新

- [ ] 摘要器升级：调一次轻量 LLM（Haiku / 本地小模型都行），按 PRD 4.2 的 4 字段结构（What / Where / Why it matters / What you should check）输出。
- [ ] 加 `conductor chat` 子命令：进入对话循环，能用自然语言 update task 状态（「第一项过了」「PPT 那个推后」）。
- [ ] task list 加「预估审阅时长」字段，由摘要器估。
- [ ] 加「现在有什么要看的？」「我有 20 分钟」这两个高频查询的快捷答法。

**Week 2 验收**：能跟 Conductor 用自然语言聊清单。

### Week 3：闭环回流 + 体验打磨

- [ ] 把 hook 从测试入口接入到 `.claude/settings.json`：仅在 Week 1/2 闭环验证后显式启用，确保不阻塞、不弹窗、不注入额外上下文。
- [ ] 加「打扰策略」最简版：人主动说「专心 30 分钟」时进入静默；否则只在 Conductor 任务列表/桌宠状态里聚合提示，不让 hook 直接打断 Claude Code。
- [ ] 跑一周自己用，记录每个「不顺手」的瞬间，回头改。

**Week 3 验收**：连续用 5 个工作日后，主观感觉「比之前从容」就算过。

---

## 3. 砍刀清单（如果时间不够，先砍这些）

1. **多 agent 来源**：MVP 只接 Claude Code 一侧，Codex 砍掉。
2. **UI 复杂度**：核心闭环先保留 CLI + markdown；但 task list 弹窗和桌宠已经确定是桌面壳方向，选型见 `技术栈选型.md`。时间不够时先砍复杂动画，不砍 task list 入口。
3. **持久化复杂度**：核心闭环可先用本地文件；进入桌面壳后切到 SQLite + `events.ndjson`，不上云同步。
4. **跨设备**：完全不做。
5. **复杂调度算法**：不上「最优排序」，先纯按时间倒序 + 人工拖动。

---

## 4. 出 MVP 后的「下一步」候选

按可能的优先级排，**不承诺**：

1. 桌面壳：task list 弹窗 + 桌宠 + 提醒策略。技术栈已选 Tauri v2，见 `技术栈选型.md`。
2. 接 Codex 一侧（开发任务的测试步骤生成）。
3. Conductor 反向写命令给 Claude Code（让它重写、让它换主题），这块要先做权限边界设计。
4. 跨设备提醒（手机推送 / 飞书机器人）。

---

## 5. 风险与对策

| 风险 | 触发条件 | 对策 |
|------|----------|------|
| 摘要不准，反而误导 | LLM 摘要质量差 | Week 1 先纯规则保底；Week 2 跑通后人工对比 5 个样本调 prompt |
| Hook 把 Claude Code 拖慢 | Stop hook 同步阻塞，或 UserPromptSubmit 里做重型摘要 | Hook 只做「写一行 NDJSON + 快速状态同步」，重活异步队列消费 |
| 通知太烦人，反而变干扰 | 每条产出都弹 | 默认聚合后批量提醒；「专心模式」一键静默 |
| 自己懒得打开 Conductor 看 | 没养成习惯 | UserPromptSubmit hook 任务记录是关键 —— 让任务**主动出现在我已经在推进的地方对应的清单里** |

---

## 6. 2026-05-21 修订：MVP 任务流以 UserPromptSubmit 开始

本节覆盖上文旧的“Stop hook 完成后才写入 task list”的 MVP 流程。MVP 只做 Claude Code 版本。

### 6.1 新的最小闭环

```text
Claude Code UserPromptSubmit
        │
        ▼
Conductor 读取用户请求、cwd、session_id、终端上下文
        │
        ▼
创建或更新一条任务，状态设为 in_progress
        │
        ▼
桌宠 / task list 显示“哪个终端正在推进什么”
        │
        ▼
Claude Code Stop / SubagentStop / 权限相关事件
        │
        ▼
Conductor 读取 transcript tail、最近改动文件、权限请求摘要
        │
        ▼
更新同一任务的上下文摘要和产物位置，状态设为 pending（待审）
        │
        ▼
人打开 Conductor pass / reject / skip
```

### 6.2 Week 1 验收调整

- [ ] `UserPromptSubmit` hook 被调用时，任务列表立即出现新任务。
- [ ] 新任务状态为 `in_progress`，不是 `pending`。
- [ ] 任务卡片显示用户请求摘要、cwd、Claude Code 会话/终端标识。
- [ ] Hook 不向 Claude Code 注入额外指令，不阻塞用户 prompt。
- [ ] `Stop` / `SubagentStop` / 权限相关事件被调用后，同一任务更新为 `pending`，界面文案显示“待审”。
- [ ] Stop 摘要显示 Claude 已完成什么、改了哪里、需要人检查什么。

### 6.3 非打扰原则

Hook 只能做旁路记录和状态同步：

- 不弹窗。
- 不要求 Claude Code 等待重型 LLM 摘要。
- 不默认输出 `additionalContext` 干预 Claude Code。
- 重型摘要应异步或快速失败，失败时使用轻量模板摘要。
- `.claude/settings.json` 当前保持空 hook 只是过渡保护：代码侧闭环未验证前，不启用会影响 Claude Code 正常工作的 hook。最终目标仍然是实现并显式启用非打扰 hook。

### 6.4 多终端识别

MVP 必须区分多个 Claude Code 终端。任务归属优先按：

1. Claude `session_id`
2. 终端/进程标识
3. `cwd + 最近活跃时间窗口`

同一仓库多个终端并行时，不能把任务错误合并成一条。

---

*关联：PRD.md / 技术架构.md / 技术栈选型.md / 感知层调研.md*
