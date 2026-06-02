# 派工 · Round 1 · MVP 管道打通（Rust + tokio 版）

> 目标：**一周内**跑通"Claude 写文档 → Conductor 自动记一条待审 → 人能看清单"这条最小闭环。
> 这一轮**不追求智能**：摘要拙、CLI 丑、没有 GUI、没有 Live2D，都可以接受。
> 验收标准：跑一次 Claude 写文档，`state/tasks.md` 里能自动多出一条记录。
>
> **本轮技术栈决策**：
> - 全 Rust 重写（不再有 Node.js 路径）。
> - 异步运行时直接上 `tokio`。
> - 工程结构按 `技术栈选型.md` §7 的 `crates/` 蓝图打底——这意味着 Round 2 接 Tauri 时**不用再搬代码**。
>
> **配套必读**：`docs/Rust 前置准备.md` —— 列出 agent 替不了你的事，先把那份做完再开 T1。

---

## 0. 约定

- **执行人**：你 + 开发 agent（vibe coding：你做测试/观错/架构决策，agent 推进具体代码）。
- **工作目录**：`I:\personal-agent\`
- **Rust 版本**：rustc ≥ 1.78（stable channel）。
- **运行时**：tokio multi-threaded。
- **状态全走本地文件**：`state/tasks.json` + `state/tasks.md` + `state/events.ndjson` + `state/summaries/`。
- **Week 1 严禁拉进来的东西**：Tauri、SQLite、LLM 调用、GUI、Live2D、Codex。

---

## 1. 工程骨架（Round 1 一开始就按 Round 2+ 的样子搭）

```
I:\personal-agent\
├── Cargo.toml                          # workspace 根
├── rust-toolchain.toml                 # 锁定 stable
├── .cargo\config.toml                  # （可选）Windows 平台编译设置
├── crates\
│   ├── conductor-core\                 # 纯库：任务/事件/锁/摘要/路径
│   │   ├── Cargo.toml
│   │   └── src\
│   │       ├── lib.rs
│   │       ├── paths.rs                # PATHS 常量
│   │       ├── lock.rs                 # 文件锁封装
│   │       ├── tasks.rs                # tasks.json 增删改 + tasks.md 同步
│   │       ├── events.rs               # events.ndjson 追加
│   │       ├── transcript.rs           # Claude transcript jsonl 解析
│   │       ├── filewatch.rs            # 最近改动文件枚举
│   │       └── summarizer.rs           # Week 1 纯规则摘要
│   └── conductor-cli\                  # 唯一可执行文件，所有入口都靠子命令分流
│       ├── Cargo.toml
│       └── src\
│           └── main.rs                 # `conductor hook stop` / `list` / `show` / ...
├── state\                              # 运行期文件（gitignore）
│   ├── tasks.json
│   ├── tasks.md
│   ├── events.ndjson
│   ├── on-stop.log
│   └── summaries\
├── .claude\
│   └── settings.json                   # Stop hook 接线（项目级）
└── docs\
    └── ...                             # 已有文档
```

**关键设计点**：

- `conductor-cli` 是**唯一二进制**，子命令分流：
  - `conductor hook stop` ← Claude Code Stop hook 调
  - `conductor hook user-prompt-submit` ← UserPromptSubmit hook 调（Round 1 占位）
  - `conductor list` / `show` / `pass` / `skip` / `reject` ← 人用
- 这样**只编一次、只发一份 exe**，Round 2 加 Tauri 时核心库 `conductor-core` 直接被桌面壳引用，零搬迁。

---

## 2. workspace 与依赖

### 2.1 根 `Cargo.toml`

```toml
[workspace]
resolver = "2"
members = ["crates/conductor-core", "crates/conductor-cli"]

[workspace.package]
edition = "2021"
rust-version = "1.78"

[workspace.dependencies]
tokio       = { version = "1", features = ["macros", "rt-multi-thread", "fs", "io-util", "process", "sync", "time"] }
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
anyhow      = "1"
thiserror   = "1"
chrono      = { version = "0.4", features = ["serde", "clock"] }
clap        = { version = "4", features = ["derive"] }
fs4         = { version = "0.8", features = ["tokio"] }   # 跨平台文件锁
walkdir     = "2"
tracing     = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid        = { version = "1", features = ["v4"] }
```

**为什么这套依赖**：
- `tokio` 全功能但只开必要 feature，编译时间可控。
- `fs4` 而非 `proper-lockfile`（那是 Node.js 的）——Rust 生态里 `fs4` 是事实标准，支持 tokio。
- `clap` derive 风格——子命令派生最快。
- `tracing` 替代日志框架，将来桌面壳也复用。

### 2.2 `rust-toolchain.toml`

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

---

## 3. 工作分解（按依赖顺序，每项独立可验收）

> ⚠️ **开始 T1 之前，请先完成 `docs/Rust 前置准备.md` 全部前置项**。
> 那些是 agent 替不了你的事（装环境、选 IDE、跑 hello world、加 PATH、确认中文输出能正常）。

### T1. workspace 骨架

**做什么**：

1. 在 `I:\personal-agent\` 下 `cargo new --lib crates/conductor-core` 和 `cargo new crates/conductor-cli`。
2. 写根 `Cargo.toml`（§2.1）和 `rust-toolchain.toml`。
3. 两个 crate 各写一个 `hello` 函数被 `main.rs` 调一下，确认 workspace 联编。
4. `state/` 目录 + 4 个空模板文件（`tasks.json`/`tasks.md`/`events.ndjson`/`summaries/.gitkeep`）。

**验收**：
- `cargo build --workspace` 通过。
- `cargo run -p conductor-cli -- --help` 显示一个最简 clap 帮助。

---

### T2. `conductor-core::paths`

**做什么**：

```rust
// crates/conductor-core/src/paths.rs
use std::path::{Path, PathBuf};

pub fn root() -> PathBuf { PathBuf::from(r"I:\personal-agent") }
pub fn state() -> PathBuf { root().join("state") }

pub struct Paths;
impl Paths {
    pub fn tasks_json()    -> PathBuf { state().join("tasks.json") }
    pub fn tasks_md()      -> PathBuf { state().join("tasks.md") }
    pub fn events()        -> PathBuf { state().join("events.ndjson") }
    pub fn summaries_dir() -> PathBuf { state().join("summaries") }
    pub fn on_stop_log()   -> PathBuf { state().join("on-stop.log") }
}
```

**验收**：单元测试 `cargo test -p conductor-core paths::` 通过；路径打印符合预期。

---

### T3. `conductor-core::lock`：异步文件锁

**做什么**：用 `fs4` 封装一个 `with_lock`：

```rust
pub async fn with_lock<F, Fut, T>(path: &Path, f: F) -> anyhow::Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>;
```

- 在同目录创建 `<file>.lock` 作为锁文件，避免直接锁数据文件本身。
- 异常路径必须释放锁（用 RAII guard 或 `try { ... } finally` 等价物）。

**验收**：写一个并发测试，两个 task 同时尝试递增 `tasks.json.counter`，1000 次后值精确。

---

### T4. `conductor-core::tasks`：tasks.json 与 tasks.md

**做什么**：

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: String,                 // "t-YYYYMMDD-NNN"
    pub source: String,             // "claude"
    pub kind: String,               // "review-doc"
    pub artifact: Artifact,
    pub summary_ref: Option<String>,
    pub est_minutes: Option<u32>,
    pub focus_hint: Option<String>,
    pub status: TaskStatus,         // Pending / InProgress / Passed / Rejected / Skipped
    pub created_at: DateTime<Utc>,
}

pub async fn load() -> anyhow::Result<TasksFile>;
pub async fn add(task: Task) -> anyhow::Result<()>;
pub async fn update<F: FnOnce(&mut Task)>(id: &str, f: F) -> anyhow::Result<()>;
pub async fn render_markdown() -> anyhow::Result<()>;
```

- 所有写操作走 `with_lock(Paths::tasks_json(), ...)`。
- `render_markdown` 写完 `tasks.md` 后落盘 fsync。

**空模板**：
```json
{ "updated_at": "<ISO>", "horizon_minutes": 60, "tasks": [] }
```

**markdown 模板**：
```markdown
# 当前审阅清单 · <更新时间> · <horizon> 分钟窗口

- [ ] [<est>min] <kind> — <artifact.file>
      ↳ <focus_hint>
      ↳ 摘要：<summary_ref>
```

**验收**：
1. 单元测试：`add` 3 次 → `load().tasks.len() == 3`，且 markdown 也是 3 行。
2. `update(id, |t| t.status = Passed)` 后，markdown 该行 checkbox 变 `[x]`。

---

### T5. `conductor-core::events`：events.ndjson 追加

**做什么**：单函数 `append(source, kind, payload) -> Result<()>`，原子追加一行 JSON。

```rust
#[derive(Serialize)]
struct EventLine<'a> {
    ts: DateTime<Utc>,
    source: &'a str,
    kind: &'a str,
    payload: &'a serde_json::Value,
}
```

文件用 `OpenOptions::new().append(true).create(true)`，追加无需全局锁（append 是原子的，除非超过 PIPE_BUF）。Week 1 单条消息 < 4KB 时安全。

**验收**：并发写 100 条，文件行数精确 100，每行可被 `serde_json::from_str` 解析。

---

### T6. `conductor-core::transcript`：Claude transcript 解析

**做什么**：

```rust
pub struct TranscriptMessage {
    pub role: String,            // "assistant" / "user" / "tool" ...
    pub text_preview: String,    // 前 80 字
    pub raw: serde_json::Value,
}

pub async fn read_tail(transcript_path: &Path, n: usize) -> anyhow::Result<Vec<TranscriptMessage>>;
```

- JSONL：每行一个 JSON。
- Week 1 可一次性读完（性能优化推迟到 Round 2）。
- 跳过解析失败的行（可能是 Claude 正在写一半）。
- Week 1 只关心 `role == "assistant"` 且包含 text 的消息。

**验收**：手工准备一份 mock transcript，能正确返回最后 N 条 assistant 消息的 80 字预览。

---

### T7. `conductor-core::filewatch`：最近改动文件

**做什么**：

```rust
pub async fn recently_modified(cwd: &Path, within: Duration) -> anyhow::Result<Vec<PathBuf>>;
```

- 用 `walkdir`，过滤目录黑名单：`node_modules`、`.git`、`state`、`dist`、`build`、`target`。
- 仅返回普通文件，按 mtime 倒序。
- 性能：Week 1 同步遍历可接受，`tokio::task::spawn_blocking` 包一层避免阻塞 runtime。

**验收**：单测在 tempdir 里造 3 个文件，2 个 mtime 在 5min 内、1 个超出，函数返回 2 个。

---

### T8. `conductor-core::summarizer`：纯规则摘要（Week 1 版）

**做什么**：

```rust
pub struct SummaryInput<'a> {
    pub transcript_tail: &'a [TranscriptMessage],
    pub recent_files: &'a [PathBuf],
    pub cwd: &'a Path,
}

pub struct SummaryOutput {
    pub slug: String,
    pub markdown: String,
    pub file_path: PathBuf,        // summaries/<timestamp>-<slug>.md 的绝对路径
}

pub async fn summarize(input: SummaryInput<'_>) -> anyhow::Result<SummaryOutput>;
```

- `slug` = 第一个改动文件 basename 去后缀；空则 `unnamed`。
- 写到 `summaries_dir().join(format!("{ts}-{slug}.md"))`。

**markdown 内容**：

```markdown
# <slug> — <ISO>

**What**：Claude 修改了 <文件 1>、<文件 2>。
**Where**：
- <绝对路径 1>
- <绝对路径 2>
**Why it matters**：（Week 1 占位 — Week 2 由 LLM 填）
**What you should check**：
- 这些文件是否符合你的预期
- transcript 末尾：<assistant 末条 80 字预览>
```

**验收**：单元测试 `summarize(...)` 返回对象后，对应文件落盘且字符串包含三段标题。

---

### T9. `conductor-cli`：子命令骨架 + `hook stop` 真实接通

**做什么**：

`clap` 子命令树：

```text
conductor
├── hook
│   ├── stop                    # 从 stdin 读 Claude payload，跑全流程
│   └── user-prompt-submit      # Round 1 占位：exit 0
├── list [--all]
├── show <id>
├── pass <id>
├── skip <id>
└── reject <id>
```

**`hook stop` 流程**（核心，照抄即可）：

```rust
async fn run_hook_stop() -> anyhow::Result<()> {
    // 1. 从 stdin 读 JSON
    let mut buf = String::new();
    tokio::io::stdin().read_to_string(&mut buf).await?;
    let payload: serde_json::Value = serde_json::from_str(&buf)?;

    let transcript_path = payload["transcript_path"].as_str().context("missing transcript_path")?;
    let cwd            = payload["cwd"].as_str().context("missing cwd")?;

    // 2. 读 transcript 尾部
    let tail = transcript::read_tail(Path::new(transcript_path), 10).await?;

    // 3. 最近改动文件
    let recent = filewatch::recently_modified(Path::new(cwd), Duration::from_secs(5 * 60)).await?;

    // 4. 摘要
    let summary = summarizer::summarize(SummaryInput {
        transcript_tail: &tail,
        recent_files: &recent,
        cwd: Path::new(cwd),
    }).await?;

    // 5. 登记 task
    let task = Task {
        id: tasks::next_id().await?,           // "t-YYYYMMDD-NNN"
        source: "claude".into(),
        kind: "review-doc".into(),
        artifact: Artifact { file: recent.first().cloned(), anchor: None },
        summary_ref: Some(format!("summaries/{}", summary.file_path.file_name().unwrap().to_string_lossy())),
        est_minutes: None,
        focus_hint: Some("（Week 1 占位）".into()),
        status: TaskStatus::Pending,
        created_at: Utc::now(),
    };
    tasks::add(task).await?;
    tasks::render_markdown().await?;

    // 6. 落事件
    events::append("claude", "stop", &payload).await?;

    Ok(())
}
```

**关键纪律**：

- `main` 里包一层 `match run_hook_stop().await`：**任何错误只 tracing::error! 写日志、不向上返回非零退出码**，避免 Claude Code 视 hook 为失败而打断对话。
- 日志接到 `state/on-stop.log`（用 `tracing-subscriber` 的 file appender）。

**Claude Code hook 接线**（项目级 `.claude/settings.json`）：

```json
{
  "hooks": {
    "Stop": [
      {
        "matcher": "",
        "hooks": [
          { "type": "command", "command": "I:/personal-agent/target/release/conductor.exe hook stop" }
        ]
      }
    ]
  }
}
```

> 注意：用 release 构建路径，**不要用 `cargo run`**——cargo run 启动延迟会让 Claude 等几秒，体验差。

**验收**：见 §4 总演练。

---

### T10. `conductor-cli` 人用子命令

每条命令的实现都不到 20 行（核心都在 `conductor-core`）：

- `list` → 调 `tasks::load()`，过滤 `Pending`，按 `created_at` 倒序，最多 10 条。
- `list --all` → 不过滤状态。
- `show <id>` → 打印 task 全部字段 + `summary_ref` 文件内容。
- `pass / skip / reject` → `tasks::update(id, |t| t.status = X)` + `render_markdown`。

**Windows 中文输出**：在 `main` 开头执行：

```rust
#[cfg(windows)]
{
    // 把 stdout 切成 UTF-8，避免乱码
    use windows::Win32::System::Console::SetConsoleOutputCP;
    unsafe { SetConsoleOutputCP(65001); }
}
```

如果嫌 `windows` crate 太重，可以让你自己启动前手动 `chcp 65001`，并在 Rust 前置准备文档里列为前置项。

**验收**：

1. `conductor list` 在 Windows Terminal 与 PowerShell 都显示中文不乱码。
2. `conductor pass t-...` 后，再 `list` 该条消失，`tasks.md` 同步打勾。

---

### T11. `conductor-cli hook user-prompt-submit`：占位

Round 1 真的就是：

```rust
async fn run_hook_user_prompt_submit() -> anyhow::Result<()> {
    Ok(())   // Round 3 真正实现
}
```

留这个壳是为了**接线先打通**，Round 3 改一行代码就生效。

---

## 4. 总验收演练（一次性走一遍）

**前置**：T1–T11 全做完，`cargo build --release` 通过，hook 已挂在你长期跟进的文档项目里。

**步骤**：

1. 在该文档项目里跑 Claude Code，命令它："把 doc-A.md 第 2 段改得更口语化"。
2. Claude 跑完。
3. 切到 `I:\personal-agent\`：
   ```bash
   target\release\conductor.exe list
   ```
4. **期望**：看到 1 条 `review-doc — <doc-A.md 路径>`。
5. `conductor show t-<id>` 能看到对应 summary 完整 markdown。
6. `conductor pass t-<id>`。
7. 再 `conductor list`：该条消失。
8. `state/tasks.md` 该条 `[x]`。
9. `state/events.ndjson` 多 1 行可解析的 JSON。
10. `state/on-stop.log` 无 ERROR 级别条目。

**全部对得上 = Round 1 通过。**

---

## 5. Round 1 严禁扩张

- ❌ 不写 LLM 摘要（Week 2）。
- ❌ 不写对话式 chat 子命令（Week 2）。
- ❌ 不实现 UserPromptSubmit 真实逻辑（Week 3）。
- ❌ 不上 Tauri / GUI / Live2D（Round 2+）。
- ❌ 不接 Codex。
- ❌ 不上 SQLite。
- ❌ 不做 transcript 流式读取优化。
- ❌ 不写跨平台抽象——Windows 优先，Linux/macOS 视情况再说。

---

## 6. 派工给开发 agent 时的标准提示模板

> 「按 `I:/personal-agent/docs/派工-Round1-MVP管道.md` 实现 T<编号>。
> 工程是 Rust workspace，crates 在 `crates/conductor-core` 与 `crates/conductor-cli`。
> 严格遵守'不做什么'清单。完成后请：
> 1. 列出新增/修改文件 + diff 概要。
> 2. 给出该任务对应的'验收'命令，方便我手工跑。
> 3. 列出本次新增的依赖（如有）及理由。
> 4. 不要顺手扩展到下一个 T。」

---

## 7. 时间预算（粗估，Rust 版）

| 任务 | 预估 |
|------|------|
| 前置准备（见 `Rust 前置准备.md`） | 0.5–1 天 |
| T1 workspace 骨架 | 0.5 天 |
| T2 paths + T3 lock | 0.5 天 |
| T4 tasks | 1 天 |
| T5 events + T6 transcript + T7 filewatch | 1 天 |
| T8 summarizer | 0.5 天 |
| T9 hook stop 接通 | 1 天 |
| T10 CLI 命令 | 0.5 天 |
| 总演练 + 修小 bug | 1 天 |
| **合计** | **~6 个工作日** |

比 Node 版多约 1 天，主要在编译/类型系统的学习成本。换来的好处：Round 2 接 Tauri 时**核心库零搬迁**，整个项目就一份 Rust 工程。

---

*依据：PRD §4 / 技术架构 §2–§5 / 技术栈选型 §7 / MVP-实施方案 §2 Week 1。
配套必读：`docs/Rust 前置准备.md`。*
