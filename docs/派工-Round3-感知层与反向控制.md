# 派工 · Round 3 · 感知层与反向控制

> 目标：让 Conductor **从"被动接收 Claude Stop 事件"升级到"主动感知人和 agent 状态"**，并具备最小的反向控制能力（让 Claude 干活、让自己更聪明地提醒）。
>
> 这一轮要解决的三个真实问题：
> 1. UserPromptSubmit hook 真正接通，让写文档的 Claude 知道"人这边还欠什么"。
> 2. Conductor 能用 `claude -p` 子进程**自己开 Claude 跑后台批处理**（生成摘要、查重、夜间整理）。
> 3. Windows OS hook 抓窗口焦点 / 键鼠空闲 + 定时巡检节拍器，做到"agent 卡住能上报、人离开能提醒回来"。
>
> **验收标准**：连续用 5 个工作日后，主观感觉"Conductor 在主动帮我节奏，而不是等我去问它"。

---

## 0. 约定

- **执行人**：你 + 开发 agent（vibe coding 不变）。
- **工作目录**：`I:\personal-agent\`（沿用 Round 1/2 工程）。
- **前置**：Round 2 验收通过（Tauri + Live2D + SQLite 全部跑通）。
- **新增依赖**：
  - `windows`（Windows API 访问，OS hook + UIA）
  - `tokio::process`（已在，跑 `claude -p` 子进程）
  - `notify`（文件变化监控，备用）
- **Round 3 严禁拉进来的东西**：跨设备同步、移动端推送、多模态截图（路径 B）——这些是 Round 4+ 的事。

---

## 1. 本轮架构增量

```
┌──────────────────────────────────────────────────────────────┐
│                         人（我）                              │
└──────────────────────────────────────────────────────────────┘
                ↑ 主动提醒 / 反向建议        ↓ 自然语言对话
        ┌───────────────────────────────────────────┐
        │           Conductor Agent (本体)           │
        │   - 维护 task list                         │
        │   - 生成摘要                               │
        │   - 解析对话 → 更新状态                    │
        │   - 注入 prompt（UserPromptSubmit）★      │
        │   - claude -p 子进程批处理 ★              │
        │   - 定时巡检节拍器 ★                       │
        │   - 反向建议（写 proposed_prompts/）★      │
        └───────────────────────────────────────────┘
            ↑ Stop hook            ↑ UserPromptSubmit hook（★ 真接通）
            ↑ OS focus 事件 ★      ↑ 键鼠空闲事件 ★
        ┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐
        │  Claude Code     │    │  Windows OS      │    │  其他应用         │
        │                  │    │  (UIA / focus)   │    │  (窗口标题黑盒)   │
        └──────────────────┘    └──────────────────┘    └──────────────────┘

★ = Round 3 新增
```

---

## 2. 工程结构增量

```
crates/
├── conductor-core/                # Round 1-2 已有
│   └── src/
│       ├── ...
│       ├── pacer.rs              # ★ 定时巡检节拍器
│       ├── inject.rs             # ★ UserPromptSubmit 注入逻辑
│       ├── subagent.rs           # ★ claude -p 子进程封装
│       └── proposals.rs          # ★ 反向建议（写 proposed_prompts/）
├── conductor-cli/                 # Round 1-2 已有
│   └── src/
│       └── main.rs                # 加 hook user-prompt-submit 真实实现
└── conductor-sense/               # ★ 新 crate：感知层
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── focus.rs               # 前台窗口监听（SetWinEventHook）
        ├── idle.rs                # 键鼠空闲检测（GetLastInputInfo）
        ├── window_title.rs        # 当前前台窗口标题 + 进程名
        └── whitelist.rs           # 白名单过滤
```

---

## 3. 工作分解（T1–T10）

### T1. UserPromptSubmit Hook 真接通

**做什么**：

Round 1 的 `conductor hook user-prompt-submit` 是占位 `Ok(())`，本轮真做。

`conductor-core/src/inject.rs`：

```rust
pub async fn build_injection_for_cwd(cwd: &Path) -> anyhow::Result<String> {
    let tasks = tasks::load().await?;
    let pending: Vec<_> = tasks.tasks.iter()
        .filter(|t| t.status == TaskStatus::Pending)
        .filter(|t| is_related_to_cwd(t, cwd))
        .take(3)
        .collect();

    if pending.is_empty() {
        return Ok(String::new());
    }

    let mut s = String::from("[Conductor 注入] 你目前还欠以下审阅未完成，请避免在同方向堆更多产出：\n");
    for t in pending {
        s.push_str(&format!("- {} （{}）\n",
            t.artifact_label(),
            t.created_at.format("%H:%M")));
    }
    // 控制 300 字内
    if s.chars().count() > 300 {
        s = s.chars().take(297).collect::<String>() + "...";
    }
    Ok(s)
}

fn is_related_to_cwd(task: &Task, cwd: &Path) -> bool {
    task.artifact.file
        .as_ref()
        .map(|f| f.starts_with(cwd))
        .unwrap_or(false)
}
```

`conductor-cli/src/main.rs` 改 `hook_user_prompt_submit`：

```rust
async fn run_hook_user_prompt_submit() -> anyhow::Result<()> {
    let mut buf = String::new();
    tokio::io::stdin().read_to_string(&mut buf).await?;
    let payload: serde_json::Value = serde_json::from_str(&buf)?;
    let cwd = payload["cwd"].as_str().context("missing cwd")?;

    let injection = conductor_core::inject::build_injection_for_cwd(Path::new(cwd)).await?;
    if !injection.is_empty() {
        println!("{}", injection);
    }
    Ok(())
}
```

`.claude/settings.json` 加：

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "matcher": "",
        "hooks": [
          { "type": "command", "command": "I:/personal-agent/target/release/conductor.exe hook user-prompt-submit" }
        ]
      }
    ]
  }
}
```

**关键纪律**：
- stdout 输出的文本会被 Claude Code 前置注入到 user prompt，**控制在 300 字内**，否则污染对话。
- 任何错误**不要写到 stdout**——会被当成注入内容。所有错误日志走 `state/inject.log`。
- 没有待审任务时输出空字符串（不要输出"暂无待审"——那也是污染）。

**验收**：
1. 在挂了 hook 的文档项目里跑 Claude，发任意消息。
2. Claude 的回答开头会带一段 `[Conductor 注入] ...`（如果有待审）。
3. 把所有任务 `pass` 掉再发消息，无注入。
4. 跑 100 次，无一次 hook 报错导致 Claude 卡顿。

---

### T2. `conductor-core::subagent`：`claude -p` 子进程封装

**做什么**：

Round 3 的 P0 反向控制路线——Conductor 用 `claude -p` 跑后台批处理。

```rust
// crates/conductor-core/src/subagent.rs
use tokio::process::Command;

pub struct SubagentResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

pub async fn run_claude_p(
    prompt: &str,
    cwd: Option<&Path>,
    timeout: Duration,
) -> anyhow::Result<SubagentResult> {
    let start = Instant::now();
    let mut cmd = Command::new("claude");
    cmd.arg("-p").arg(prompt);
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let child = cmd.spawn()?;
    let output = tokio::time::timeout(timeout, child.wait_with_output()).await??;

    Ok(SubagentResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code(),
        duration_ms: start.elapsed().as_millis() as u64,
    })
}
```

**调用方约定**：

| 场景 | prompt 示例 | timeout |
|---|---|---|
| 批量摘要升级 | "读 state/summaries/*.md，重写为 What/Where/Why/Check 四段" | 5 min |
| 文档查重 | "对比 doc-A.md 和 doc-B.md，列出冲突点" | 3 min |
| 夜间整理 | "把今天所有 passed 任务归档为 daily.md" | 5 min |

**关键纪律**：
- **永远带 timeout**——子进程卡死必须能杀掉。
- **不要并发跑多个 `claude -p`**——会撞 Anthropic API 速率限制。用 `tokio::sync::Semaphore(1)`。
- **stdout 写日志归档**：`state/subagent-runs/<timestamp>-<slug>.log`，方便回溯。

**验收**：
1. 单测：`run_claude_p("echo hello", None, 10s)` 能跑通（用一个不会失败的 prompt）。
2. timeout 测试：传入会跑很久的 prompt + 2 秒 timeout，能在 ~2 秒后超时并返回错误。
3. 并发测试：起 3 个 task 同时调，验证 Semaphore 串行化。

---

### T3. `conductor-core::proposals`：反向建议（写文件路线）

**做什么**：

Round 3 的「保留路线」——Conductor 把"下一步建议"写到 `state/proposed_prompts/`，等人下次开 Claude 时由 UserPromptSubmit hook 注入。

```rust
// crates/conductor-core/src/proposals.rs

pub struct Proposal {
    pub id: String,                    // "p-YYYYMMDD-NNN"
    pub for_cwd: PathBuf,              // 适用于哪个项目
    pub content: String,               // 建议给 Claude 的下一步指示
    pub reason: String,                // Conductor 为什么这么建议
    pub created_at: DateTime<Utc>,
    pub status: ProposalStatus,        // Pending / Approved / Rejected / Expired
}

pub async fn create(p: Proposal) -> anyhow::Result<()>;
pub async fn list_pending_for_cwd(cwd: &Path) -> anyhow::Result<Vec<Proposal>>;
pub async fn approve(id: &str) -> anyhow::Result<()>;
pub async fn reject(id: &str) -> anyhow::Result<()>;
```

存储：`state/proposals.json`（结构和 tasks.json 类似，走文件锁）。

**T1 的 `build_injection_for_cwd` 要扩展**：

```rust
pub async fn build_injection_for_cwd(cwd: &Path) -> anyhow::Result<String> {
    let pending_tasks = ...;
    let approved_proposals = proposals::list_approved_for_cwd(cwd).await?;

    let mut s = String::new();
    if !pending_tasks.is_empty() { /* 待审清单 */ }
    if !approved_proposals.is_empty() {
        s.push_str("[Conductor 建议下一步]\n");
        for p in approved_proposals {
            s.push_str(&format!("- {}\n", p.content));
        }
    }
    Ok(s)
}
```

**人审批 UI**（暂时走 CLI，Round 4 可挪进桌面壳）：

```bash
conductor proposal list           # 列出 pending
conductor proposal show <id>      # 看具体内容 + reason
conductor proposal approve <id>   # 批准 → 下次 UserPromptSubmit 时注入
conductor proposal reject <id>    # 拒绝
```

**关键纪律**：
- Conductor **永远不能自动 approve**——必须人点过才注入。
- approved 的建议被注入一次后立刻转为 `Used` 状态，**不重复注入**。

**验收**：
1. `create` 一条 proposal → `list pending` 能看到。
2. `approve` 后，在对应 cwd 跑 Claude，注入内容带「Conductor 建议下一步」段。
3. 同一 proposal 不会被注入第二次。

---

### T4. `conductor-sense` 新 crate：项目骨架

**做什么**：

```bash
cargo new --lib crates/conductor-sense
```

`crates/conductor-sense/Cargo.toml`：

```toml
[package]
name = "conductor-sense"
version = "0.1.0"
edition.workspace = true

[dependencies]
tokio.workspace = true
anyhow.workspace = true
tracing.workspace = true
windows = { version = "0.56", features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_UI_Accessibility",
    "Win32_System_Threading",
    "Win32_System_ProcessStatus",
] }
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
```

根 `Cargo.toml` 加 member：`"crates/conductor-sense"`。

**验收**：`cargo build --workspace` 通过。

---

### T5. `conductor-sense::focus`：前台窗口监听

**做什么**：

用 Windows API `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` 监听前台窗口切换。

```rust
// crates/conductor-sense/src/focus.rs
pub struct FocusEvent {
    pub ts: DateTime<Utc>,
    pub hwnd: isize,
    pub title: String,
    pub process_name: String,
    pub process_path: Option<PathBuf>,
}

pub fn spawn_focus_watcher(
    tx: tokio::sync::mpsc::UnboundedSender<FocusEvent>,
) -> anyhow::Result<FocusWatcherHandle>;
```

实现要点：
- `SetWinEventHook` 必须在有消息循环的线程里跑——单独 spawn 一个 OS 线程，里面跑 `GetMessage` 循环。
- 回调里拿 hwnd，调 `GetWindowTextW` 拿标题，`GetWindowThreadProcessId` + `OpenProcess` + `GetModuleFileNameExW` 拿进程路径。
- 跨线程把事件 send 到 tokio channel。
- 关闭时调 `UnhookWinEvent` 释放。

**白名单**（`whitelist.rs`）：

```rust
pub fn is_interesting(process_name: &str, title: &str) -> bool {
    const INTERESTING_PROCESSES: &[&str] = &[
        "Code.exe",          // VS Code
        "chrome.exe",
        "msedge.exe",
        "firefox.exe",
        "WindowsTerminal.exe",
        "pwsh.exe",
        "wechat.exe",
        "Lark.exe",
        "Feishu.exe",
    ];
    INTERESTING_PROCESSES.iter().any(|p| process_name.eq_ignore_ascii_case(p))
}
```

**事件落盘**：每条 focus 事件追加到 `state/events.ndjson`：

```json
{ "ts": "...", "source": "os", "kind": "focus", "payload": { "process": "...", "title": "..." } }
```

**关键纪律**：
- 切换太频繁（< 2 秒）的事件**合并丢弃**，只记录稳定停留 > 2 秒的窗口。
- 隐私：**永远不抓密码框 / 私聊内容**——MVP 只抓窗口标题 + 进程名。

**验收**：
1. 跑起来后，切到 VS Code → 事件流里出现 `focus { process: "Code.exe" }`。
2. 切到非白名单应用（如系统设置），事件被过滤不上报。
3. 切回切去 5 次，`events.ndjson` 多出 5 条 focus 事件。

---

### T6. `conductor-sense::idle`：键鼠空闲检测

**做什么**：

```rust
// crates/conductor-sense/src/idle.rs
pub async fn current_idle_seconds() -> u64;
// 内部：GetLastInputInfo()，返回距离上次键鼠输入的秒数

pub fn spawn_idle_watcher(
    threshold_seconds: u64,         // 比如 600（10 分钟）
    tx: UnboundedSender<IdleEvent>,
) -> IdleWatcherHandle;
```

行为：
- 每 30 秒 poll 一次 `GetLastInputInfo`。
- 跨过 threshold 时 emit `IdleStarted`；回到活跃时 emit `IdleEnded`。
- 不要 emit 中间状态（每 30 秒 emit 没用）。

事件落盘：

```json
{ "ts": "...", "source": "os", "kind": "idle_started", "payload": { "since_seconds": 600 } }
{ "ts": "...", "source": "os", "kind": "idle_ended",   "payload": { "duration_seconds": 1834 } }
```

**验收**：
1. 故意不动 11 分钟 → 出现一条 `idle_started`。
2. 动一下鼠标 → 出现一条 `idle_ended`，`duration_seconds` ≈ 660。

---

### T7. `conductor-core::pacer`：定时巡检节拍器

**做什么**：

后台跑一个 tick（每 5 分钟一次），扫描状态并决定要不要做事。

```rust
// crates/conductor-core/src/pacer.rs
pub async fn spawn_pacer(tx_alerts: UnboundedSender<PacerAlert>) -> PacerHandle;

pub enum PacerAlert {
    PendingPileUp { count: u32, oldest_minutes: u32 },        // 待审堆积
    AgentStalled  { task_id: String, stalled_minutes: u32 },  // agent 跑了很久没动
    UserBackFromIdle { idle_minutes: u32, pending: u32 },     // 人刚回来 + 有待审
    NoActivityHour { hour: u8 },                              // 整点报告
}
```

每个 tick 做的事：

```rust
loop {
    tokio::time::sleep(Duration::from_secs(300)).await; // 5 min

    let tasks = tasks::load().await?;
    let pending = tasks.pending();

    // 规则 1：待审堆积 > 5 条 或 最老的 > 60 min
    if pending.len() >= 5 || oldest_minutes(&pending) > 60 {
        tx_alerts.send(PacerAlert::PendingPileUp { ... });
    }

    // 规则 2：检查是否有任务 in_progress 超 30 min 没更新
    for t in tasks.in_progress() {
        if elapsed_since_update(t) > 30 {
            tx_alerts.send(PacerAlert::AgentStalled { ... });
        }
    }

    // 规则 3：人刚从 idle 回来 + 有待审 → 提醒
    // （这条要配合 conductor-sense::idle 的事件流，本 tick 只是兜底）
}
```

**告警的下游**：
- 在 Tauri 后台 worker 里订阅 `PacerAlert`，决定是 emit 给 webview（桌宠表情切换）还是发 Windows 通知。

**关键纪律**：
- 每个 alert 类型加冷却时间（同一类 alert 1 小时内最多 emit 1 次），避免刷屏。
- pacer 自身不发任何 UI——只 emit 事件，UI 决策在 worker 里。

**验收**：
1. 手动塞 6 条 pending 任务 → 等下一次 tick → 出现 `PendingPileUp` alert。
2. 同一条件 1 小时内不重复 alert。
3. pacer 跑 1 小时不会泄漏内存（task 数量不增）。

---

### T8. 打扰策略 v1（pacer + idle + focus 联动）

**做什么**：

在 Tauri 后台 worker（Round 2 的 T9）里加一个**决策模块**：

```rust
// apps/desktop/src-tauri/src/notify_decider.rs
pub async fn decide_action(alert: PacerAlert, ctx: &Context) -> Action;

pub enum Action {
    UpdatePetExpression(PetState),    // 只换桌宠表情
    EmitToTaskPanel(...),             // 在 task panel 顶部加 banner
    SystemNotification { title, body, urgency },  // 系统通知（最重）
    Silent,                           // 什么都不做（如静默时段）
}
```

决策树（最简版，可后续调）：

```
1. 当前是「专注模式」或「静默时段」？
   → Silent

2. alert = PendingPileUp？
   → 累计 > 30 min: SystemNotification
   → 累计 > 15 min: EmitToTaskPanel + 桌宠 pending_review 表情
   → 否则: 仅切桌宠表情

3. alert = UserBackFromIdle？
   → 有 pending 且 idle > 10 min: SystemNotification "你刚回来，有 N 条待审"
   → 否则: 桌宠摆头一下

4. alert = AgentStalled？
   → 写 proposal "task X 似乎卡住，要不要换个方式？"
   → 同时 SystemNotification 提示
```

**关键纪律**：
- 系统通知**最稀缺**——任何场景里 1 小时不能超过 1 条。
- 桌宠表情切换**不算打扰**——可以频繁切。

**验收**：
1. 制造 PendingPileUp 累计 > 30 min → 收到 1 条 Windows toast。
2. 同一条件 1 小时内再次触发，不重复弹 toast，但桌宠表情维持 pending_review。
3. 在专注模式下制造任何 alert，无 toast 弹出。

---

### T9. 反向控制 CLI 命令

**做什么**：

把 T2 / T3 暴露成人能用的 CLI 子命令：

```bash
# T2 subagent
conductor sub run "<prompt>" [--cwd <path>] [--timeout 5m]
# 立刻起一个 claude -p 子进程跑，stdout 实时输出，结束时把记录写入 subagent-runs/

# T3 proposals
conductor proposal list                # 看所有 pending
conductor proposal show <id>           # 看 reason + content
conductor proposal approve <id>        # 批准 → 下次 UserPromptSubmit 注入
conductor proposal reject <id>
conductor proposal create --for-cwd <path> --content "..." --reason "..."
# 手动给自己写个建议（测试用）
```

**验收**：
1. `conductor sub run "测试一下" --timeout 30s` 能跑出 Claude 的回答。
2. `conductor proposal create ... && conductor proposal approve ...` 后，在对应 cwd 起一次 Claude 对话能看到注入。

---

### T10. 总演练：连续用 5 天

**这一项不是写代码，是体验验证**。

每天结束记录：

| 日期 | 注入触发次数 | proposal 被 approve 次数 | 桌宠表情切换次数 | 系统通知次数 | 「不顺手」记录 |
|------|--------------|---------------------------|------------------|--------------|----------------|
| Day 1 | | | | | |
| Day 2 | | | | | |
| Day 3 | | | | | |
| Day 4 | | | | | |
| Day 5 | | | | | |

**Round 3 通过标准**：

5 天后主观回答这 3 个问题，**至少 2 个是"YES"**：

1. Conductor 是否**主动**让我做过对的事？（不是我去问它）
2. 注入到 Claude 的内容**真的**让 Claude 减少了堆积？
3. 桌宠表情切换让我"看一眼就知道现在该做什么"？

---

## 4. 总验收演练（一次性走一遍）

**前置**：T1–T9 全部完成，所有 hook 已挂在你常用的文档项目里。

**步骤**：

1. **场景一：UserPromptSubmit 注入**
   - 让 Claude 改 doc-A.md。完成后 conductor 自动记 1 条待审。
   - 不审，直接在同项目里发新对话「帮我改 doc-B.md」。
   - **期望**：Claude 收到注入「你目前还欠 doc-A.md §3 待审」，回答里能体现"我注意到 doc-A 还没看"。

2. **场景二：claude -p 后台批处理**
   - `conductor sub run "把所有 passed 任务整理成今日总结 daily-2026-05-20.md" --timeout 3m`
   - **期望**：3 分钟内 stdout 看到 Claude 跑完，`subagent-runs/` 落一份 log，且 daily 文件生成。

3. **场景三：focus + idle 联动**
   - 在 VS Code 工作 10 分钟，切到浏览器看微博 25 分钟。
   - **期望**：events.ndjson 里能看到 focus 事件序列；25 分钟后 idle_started 触发；桌宠表情进 pending_review。

4. **场景四：proposal 流**
   - 等 conductor 自己生成一条 proposal（或手动 `conductor proposal create`）。
   - `conductor proposal list` → 看到。
   - `conductor proposal show <id>` → reason 和 content 清楚。
   - `conductor proposal approve <id>`。
   - 起 Claude 对话 → 看到注入。

5. **场景五：pacer 巡检**
   - 故意累积 6 条 pending 任务 + 等 5 分钟。
   - **期望**：弹一次 Windows toast「Conductor: 待审堆积 6 条」；桌宠切 pending_review。

5 个场景全过 = **Round 3 通过**。

---

## 5. Round 3 严禁扩张

- ❌ 不做多模态截图（路径 B，Round 4+）。
- ❌ 不做跨设备同步 / 手机推送（Round 4+）。
- ❌ 不接 Codex（仍然推迟）。
- ❌ 不做 UIA 控件级抓取（仅窗口标题 + 进程名，UIA 深度集成留 Round 4）。
- ❌ 不做 GUI 注入（SendInput）——任何场景都不允许。
- ❌ 不在 pacer 里跑昂贵 LLM——pacer 只看规则，LLM 决策放在 T2 subagent 调用里。

---

## 6. 派工给开发 agent 的标准提示模板

> 「按 `I:/personal-agent/docs/派工-Round3-感知层与反向控制.md` 实现 T<编号>。
> 工程是 Rust workspace，新 crate 是 `conductor-sense`。
> 严格遵守'不做什么'清单。完成后请：
> 1. 列出新增/修改文件 + diff 概要。
> 2. 给出该任务对应的'验收'命令，方便我手工跑。
> 3. 列出本次新增的依赖（如有）及理由——特别是 `windows` crate 的 feature 选择。
> 4. 任何涉及 unsafe Win32 调用的代码，给出错误处理与释放保证的注释。
> 5. 不要顺手扩展到下一个 T。」

---

## 7. 时间预算（粗估）

| 任务 | 预估 |
|------|------|
| T1 UserPromptSubmit 真接通 | 0.5 天 |
| T2 subagent（claude -p 子进程） | 1 天 |
| T3 proposals（反向建议） | 1 天 |
| T4 conductor-sense 骨架 | 0.5 天 |
| T5 focus 监听（Win32 hook 调通最难） | 1.5 天 |
| T6 idle 检测 | 0.5 天 |
| T7 pacer 节拍器 | 1 天 |
| T8 打扰策略 v1 | 1 天 |
| T9 反向控制 CLI | 0.5 天 |
| T10 连续用 5 天体验验证 | 5 天（不阻塞下一轮） |
| **合计开发** | **~7.5 个工作日** |
| **合计含体验验证** | **~12.5 天** |

体验验证（T10）期间可以并行启动 Round 4 调研。

---

## 8. Round 3 完成后的下一步候选

按可能优先级，**不承诺**：

1. **多模态截图兜底**（路径 B）：仅在 focus 命中白名单外应用 + 停留 > 10 min 时，截图给 Claude 看一眼。
2. **UIA 深度集成**：抓 VS Code 当前文件、Chrome 当前 tab URL。
3. **跨设备**：飞书机器人推送、手机端 task list。
4. **多 agent 来源**：终于该接 Codex 了。
5. **本地小模型**：让 pacer 的「这段 transcript 算不算稳定输出」用本地模型判断，省 API。

---

*依据：PRD §3.3 / §4 F4 / §7 / 感知层调研.md §3 §4 / 技术架构.md §6 远期感知层。
配套：派工-Round2-桌面壳与Live2D集成.md（前置）。*
