# 派工 · Round 4 · 对话面板与 Shell 工具

> 目标：让桌宠的 ChatPanel 具备 **Claude Code 级别的对话可视化能力** —— 完整展示 LLM 思考过程、工具调用链路、Shell 命令执行与流式输出，打通"用户说话 → LLM 决策 → 工具执行 → 结果展示"的完整闭环。
>
> 这一轮要解决的三个真实问题：
> 1. ChatPanel 目前只能显示纯文本对话，无法展示 tool_use / tool_result 结构化内容（BUG-001 核心诉求）。
> 2. 没有 Shell 执行能力，LLM 无法调用系统命令、运行脚本、操作文件系统（claude-code-porting-mapping 中 Gap 最大的一项）。
> 3. chat.rs 的对话循环是同步阻塞式的，不支持流式输出和多轮工具调用（Phase 0.2 阻塞项的部分解）。
>
> **验收标准**：用户在桌宠聊天框输入"帮我看看 I:\personal-agent 下有哪些 .rs 文件"，LLM 调用 `file.glob` 工具，ChatPanel 实时显示工具调用卡片（工具名 + 参数 + 执行状态 + 结果），整个过程从用户输入到结果展示 < 5 秒。

---

## 0. 约定

- **执行人**：开发 agent（vibe coding）。
- **工作目录**：`I:\personal-agent\`（沿用 Round 1-3 工程）。
- **前置**：Round 3 验收通过；`file.*` 6 个工具已注册；chat.rs 工具白名单已配置化。
- **Claude Code 源码参考**：`I:\package\cli.js`（v2.1.88 bundled），关键模块映射见下文。
- **新增依赖**：
  - 无新增 crate 依赖（`tokio::process`、`serde_json` 已有）
  - 前端新增：无（纯 React + Tauri IPC）
- **Round 4 严禁拉进来的东西**：MCP 远程执行、Web 搜索工具、Worktree 隔离 —— 这些是 Round 5+ 的事。

---

## 1. 本轮架构增量

```
┌──────────────────────────────────────────────────────────────────────┐
│                          用户                                       │
│                    输入自然语言消息                                    │
└──────────────────────────────────────────────────────────────────────┘
                            ↓ Tauri IPC (invoke: sendChatMessage)
┌──────────────────────────────────────────────────────────────────────┐
│                    ChatPanel.tsx (前端)                               │
│   ┌────────────────────────────────────────────────────────────┐    │
│   │  消息流渲染                                                 │    │
│   │  ├── UserMessage      — 用户输入文本                        │    │
│   │  ├── AssistantText    — LLM 文本回复 (Markdown)             │    │
│   │  ├── ThinkingBlock    — LLM 思考过程 (折叠)       ★ 新增   │    │
│   │  ├── ToolUseCard      — 工具调用卡片             ★ 新增   │    │
│   │  │   ├── 工具名 + 图标                                      │    │
│   │  │   ├── 输入参数 (JSON, 可折叠)                             │    │
│   │  │   ├── 执行状态 (spinner → success/error)                  │    │
│   │  │   └── 输出结果 (stdout/stderr, 可折叠)                    │    │
│   │  └── ToolResultCard   — 独立工具结果展示         ★ 新增   │    │
│   └────────────────────────────────────────────────────────────┘    │
│                            ↑ Tauri event (stream-chat-token)        │
│                            ↑ Tauri event (tool-execution-update)    │
└──────────────────────────────────────────────────────────────────────┘
                            ↓ Tauri IPC
┌──────────────────────────────────────────────────────────────────────┐
│                    chat.rs (后端 Query Loop)           ★ 改造       │
│                                                                      │
│   loop {                                                             │
│     1. 构建 messages (含 history + system prompt + persona)           │
│     2. 调用 LLM (流式) → emit stream-chat-token 逐 token 推送       │
│     3. 解析 response:                                                │
│        ├── text blocks → 累积为 AssistantText                        │
│        ├── thinking blocks → emit thinking-update                    │
│        └── tool_use blocks → dispatch to tool execution  ★ 核心     │
│     4. 执行工具 → emit tool-execution-update (start/progress/done)   │
│     5. 注入 tool_result → 继续 loop                                  │
│     6. 退出条件: 无 tool_use || max_turns(10) || user_abort          │
│   }                                                                  │
└──────────────────────────────────────────────────────────────────────┘
                            ↓ tool dispatch
┌──────────────────────────────────────────────────────────────────────┐
│                    tools.rs (工具注册表)                              │
│   ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────────────┐  │
│   │file.*    │ │task.*    │ │bash.*    │ │ agent.* / pet.* / .. │  │
│   │(6个已有) │ │(5个已有) │ │★ 新增   │ │ (已有)               │  │
│   └──────────┘ └──────────┘ └──────────┘ └──────────────────────┘  │
└──────────────────────────────────────────────────────────────────────┘
                            ↓ bash.execute
┌──────────────────────────────────────────────────────────────────────┐
│                    ShellExecutor (新建抽象层)           ★ 新增       │
│   ┌────────────────────────────────────────────────────────────┐    │
│   │  Provider 选择:                                              │    │
│   │  ├── CmdProvider      — cmd.exe /C <command>  (默认)        │    │
│   │  ├── PowershellProvider — powershell -Command <cmd>          │    │
│   │  └── BashProvider     — bash -c <command>  (Git Bash/WSL)   │    │
│   │                                                              │    │
│   │  流式输出: stdout/stderr → tokio::sync::broadcast            │    │
│   │  超时控制: 可配置 (默认 120s)                                 │    │
│   │  安全层: 命令黑名单 + 路径限制                                │    │
│   └────────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 2. 工程结构增量

```
crates/
├── conductor-core/
│   └── src/
│       ├── chat.rs              # ★ 改造: 流式 Query Loop + 工具调度
│       ├── llm.rs               # 小改: 流式 SSE 解析支持
│       ├── tools.rs             # ★ 改造: 新增 bash.execute + bash.cancel 注册
│       ├── shell/
│       │   ├── mod.rs           # ★ 新建: ShellExecutor 抽象层
│       │   ├── cmd_provider.rs  # ★ 新建: cmd.exe provider
│       │   ├── ps_provider.rs   # ★ 新建: PowerShell provider
│       │   ├── bash_provider.rs # ★ 新建: Git Bash / WSL provider
│       │   └── security.rs      # ★ 新建: 命令安全校验
│       └── ...                  # 其余不变
│
├── conductor-cli/               # 不变
└── conductor-sense/             # 不变

apps/desktop/
├── src/
│   ├── windows/
│   │   ├── ChatPanel.tsx        # ★ 改造: 结构化消息渲染
│   │   ├── ToolUseCard.tsx      # ★ 新建: 工具调用卡片组件
│   │   ├── ToolResultCard.tsx   # ★ 新建: 工具结果卡片组件
│   │   ├── ThinkingBlock.tsx    # ★ 新建: LLM 思考过程展示
│   │   └── StreamText.tsx       # ★ 新建: 流式文本逐字显示
│   ├── ipc/
│   │   └── invoke.ts            # ★ 改造: 新增 invoke 类型定义
│   └── styles/
│       └── app.css              # ★ 改造: 新增工具卡片样式
└── src-tauri/
    └── src/
        └── commands.rs          # ★ 改造: 新增 Tauri command (流式事件)
```

---

## 3. 任务清单

### T1: 消息模型对齐 (前端 + 后端)

**目标**：将 chat 消息格式从纯文本升级为 Anthropic API content blocks 格式。

**后端改动** (`crates/conductor-core/src/chat.rs`)：

```rust
// 新增消息内容块类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

// ChatMessage 改为携带 content blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,                    // "user" | "assistant"
    pub content: Vec<ContentBlock>,      // 替代原来的 single text
    pub uuid: String,
    pub timestamp: i64,
}
```

**前端改动** (`apps/desktop/src/windows/ChatPanel.tsx`)：
- `renderMessage()` 根据 `content[].type` 分发渲染：
  - `text` → 现有 Markdown 渲染
  - `thinking` → `<ThinkingBlock>` (默认折叠)
  - `tool_use` → `<ToolUseCard>`
  - `tool_result` → `<ToolResultCard>`

**验收**：发送消息后，ChatPanel 能渲染包含 text + tool_use + tool_result 的复合消息。

---

### T2: Shell 抽象层 (后端)

**目标**：实现跨平台 Shell 命令执行，支持流式输出。

**新建** `crates/conductor-core/src/shell/mod.rs`：

```rust
use tokio::process::Command;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellRequest {
    pub command: String,
    pub provider: ShellProvider,     // Cmd | Powershell | Bash
    pub working_dir: Option<String>,
    pub timeout_secs: Option<u64>,   // 默认 120
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShellProvider {
    Cmd,          // cmd.exe /C
    Powershell,   // powershell -NoProfile -Command
    Bash,         // bash -c (Git Bash 或 WSL)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShellEvent {
    Stdout(String),
    Stderr(String),
    Exit(i32),
    Timeout,
    Error(String),
}

pub struct ShellExecutor {
    event_tx: broadcast::Sender<ShellEvent>,
}

impl ShellExecutor {
    pub fn new() -> Self { ... }

    /// 执行命令，返回 event receiver 用于流式读取输出
    pub async fn execute(&self, req: ShellRequest) -> Result<broadcast::Receiver<ShellEvent>> {
        let provider = resolve_provider(&req.provider)?;
        let mut cmd = provider.build_command(&req.command, req.working_dir.as_deref())?;

        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let mut rx = self.event_tx.subscribe();

        // 流式读取 stdout
        let tx_out = self.event_tx.clone();
        let stdout = child.stdout.take().unwrap();
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(stdout);
            let mut line = String::new();
            loop {
                match tokio::io::BufRead::read_line(&mut reader, &mut line).await {
                    Ok(0) => break,
                    Ok(_) => { let _ = tx_out.send(ShellEvent::Stdout(line.clone())); line.clear(); }
                    Err(e) => { let _ = tx_out.send(ShellEvent::Error(e.to_string())); break }
                }
            }
        });

        // 同理读取 stderr，等待 exit code
        // 超时处理: tokio::time::timeout

        Ok(rx)
    }
}
```

**Provider 实现**：

| Provider | 命令构建 | 说明 |
|----------|---------|------|
| `Cmd` | `cmd.exe /C <command>` | Windows 默认，兼容性最好 |
| `Powershell` | `powershell -NoProfile -NonInteractive -Command <command>` | 支持管道、脚本块 |
| `Bash` | `bash -c <command>` | 需 Git Bash 或 WSL 在 PATH 中 |

**安全层** (`crates/conductor-core/src/shell/security.rs`)：

```rust
/// 命令黑名单 — 阻止危险操作
const BLOCKED_COMMANDS: &[&str] = &[
    "rm -rf /", "format", "del /s", "rd /s",
    "shutdown", "taskkill", "reg delete",
];

/// 校验命令安全性
pub fn validate_command(command: &str, risk_level: RiskLevel) -> Result<()> {
    let lower = command.to_lowercase();
    for blocked in BLOCKED_COMMANDS {
        if lower.contains(blocked) {
            return Err(anyhow::anyhow!("blocked dangerous command: {}", blocked));
        }
    }
    // RiskLevel::Destructive 需要用户确认
    Ok(())
}
```

**依赖**：无新增（`tokio::process` 已在 workspace 中）。

**验收**：
- `cmd.exe /C echo hello` 返回 stdout "hello"
- `powershell -Command "Get-Date"` 返回当前时间
- 长命令超时后正确终止进程
- 危险命令被拦截

---

### T3: bash.execute 工具注册 (后端)

**目标**：将 Shell 执行能力注册为 LLM 可调用的工具。

**改动** (`crates/conductor-core/src/tools.rs`)：

```rust
// 注册 bash.execute
register_tool(ToolSpec {
    id: "bash.execute".to_string(),
    name: "执行命令".to_string(),
    description: "在系统 Shell 中执行命令并返回输出。支持 cmd、PowerShell、bash 三种 shell。".to_string(),
    provider: ToolProviderKind::Internal,
    input_schema: json!({
        "type": "object",
        "properties": {
            "command": {
                "type": "string",
                "description": "要执行的 Shell 命令"
            },
            "provider": {
                "type": "string",
                "enum": ["cmd", "powershell", "bash"],
                "description": "Shell 类型 (默认 cmd)"
            },
            "working_dir": {
                "type": "string",
                "description": "工作目录 (默认当前目录)"
            },
            "timeout_secs": {
                "type": "integer",
                "description": "超时秒数 (默认 120)"
            }
        },
        "required": ["command"]
    }),
    output_schema: json!({
        "type": "object",
        "properties": {
            "stdout": { "type": "string" },
            "stderr": { "type": "string" },
            "exit_code": { "type": "integer" },
            "timed_out": { "type": "boolean" }
        }
    }),
    risk_level: RiskLevel::ExternalSideEffect,
    permissions: vec![ToolPermission::SystemControl],
    supports_dry_run: false,
    workspace_required: false,
});

// 注册 bash.cancel (终止运行中的命令)
register_tool(ToolSpec {
    id: "bash.cancel".to_string(),
    name: "终止命令".to_string(),
    description: "终止一个正在运行的 Shell 命令。".to_string(),
    // ...
    risk_level: RiskLevel::ReadOnly,
});
```

**白名单更新** (`config.rs`)：
- `default_allowed_tool_ids()` 新增 `"bash.execute"` 和 `"bash.cancel"`
- 或者按 REQ-001 分级暴露方案，`bash.*` 放在 `developer_mode` 层级

**验收**：LLM 能通过 tool_use 调用 `bash.execute`，后端正确执行并返回结果。

---

### T4: Chat Query Loop 改造 (后端核心)

**目标**：将 `chat.rs` 的单次 LLM 调用改造为支持多轮工具调用的流式 Query Loop。

**参考 Claude Code**：`src/services/api/query.ts` → `queryLoop()` (line 392957)

**当前 `chat.rs` 流程**（简化）：
```
用户消息 → 构建 prompt → 调用 LLM → 返回文本 → 存储
```

**改造后流程**：
```rust
/// 流式查询循环 — 核心改动
pub async fn query_loop(
    db: &SqlitePool,
    config: &CoreConfig,
    user_message: &str,
    app_handle: &tauri::AppHandle,
) -> Result<ChatMessage> {
    let mut messages = build_history(db).await?;
    messages.push(ContentBlock::Text { text: user_message.to_string() });

    let tool_definitions = build_tool_definitions(config);
    let max_turns = 10;

    for turn in 0..max_turns {
        // 1. 流式调用 LLM
        let response = stream_llm_call(
            config,
            &messages,
            &tool_definitions,
            app_handle,       // 用于 emit 流式 token
        ).await?;

        // 2. 解析 response content blocks
        let mut assistant_blocks = Vec::new();
        let mut tool_calls = Vec::new();

        for block in &response.content {
            match block {
                ContentBlock::Text { text } => {
                    assistant_blocks.push(block.clone());
                    // 流式 token 已通过 event 推送
                }
                ContentBlock::Thinking { .. } => {
                    assistant_blocks.push(block.clone());
                    app_handle.emit("thinking-update", block)?;
                }
                ContentBlock::ToolUse { id, name, input } => {
                    assistant_blocks.push(block.clone());
                    tool_calls.push((id.clone(), name.clone(), input.clone()));
                }
                _ => {}
            }
        }

        // 3. 存储 assistant 消息
        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: assistant_blocks,
            uuid: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        });

        // 4. 如果没有 tool_use，退出循环
        if tool_calls.is_empty() {
            break;
        }

        // 5. 执行工具（支持并发）
        let tool_results = execute_tools(&tool_calls, app_handle).await;

        // 6. 注入 tool_result 到 messages
        for (tool_use_id, result) in tool_results {
            messages.push(ContentBlock::ToolResult {
                tool_use_id,
                content: result.output.to_string(),
                is_error: !result.success,
            });
        }
    }

    // 7. 存储最终消息到 DB
    save_message(db, &messages.last().unwrap()).await?;
    Ok(messages.last().unwrap().clone())
}
```

**关键设计决策**：
- **流式 token 推送**：通过 `app_handle.emit("stream-chat-token", token)` 实现逐字显示
- **工具执行事件**：通过 `app_handle.emit("tool-execution-update", event)` 推送状态变更
- **最大轮次**：默认 10 轮，防止无限循环
- **并发执行**：多个 tool_use 可并发执行（参考 Claude Code 的 `partitionToolCalls`）

**新增 Tauri Commands** (`apps/desktop/src-tauri/src/commands.rs`)：

```rust
#[tauri::command]
pub async fn send_chat_message(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    message: String,
) -> Result<String, String> {
    let db = state.db.lock().await;
    let config = state.config.lock().await;
    chat::query_loop(&db, &config, &message, &app_handle)
        .await
        .map(|m| serde_json::to_string(&m).unwrap())
        .map_err(|e| e.to_string())
}
```

**验收**：
- 用户输入"查看当前目录文件" → LLM 调用 `bash.execute` → 返回文件列表 → LLM 总结 → ChatPanel 显示完整链路
- 最多 10 轮工具调用后自动停止
- 任意时刻用户可中断（发送新消息 abort 当前 loop）

---

### T5: 前端工具调用卡片 (前端)

**目标**：实现 Claude Code 风格的工具调用可视化组件。

**新建** `apps/desktop/src/windows/ToolUseCard.tsx`：

```tsx
interface ToolUseCardProps {
  toolId: string;         // e.g. "bash.execute", "file.glob"
  input: Record<string, any>;
  status: 'pending' | 'running' | 'success' | 'error';
  result?: {
    stdout?: string;
    stderr?: string;
    exitCode?: number;
    output?: any;
  };
  durationMs?: number;
}

export function ToolUseCard({ toolId, input, status, result, durationMs }: ToolUseCardProps) {
  const [expanded, setExpanded] = useState(false);
  const icon = getToolIcon(toolId);  // bash→terminal, file→document, etc.

  return (
    <div className={`tool-use-card tool-status-${status}`}>
      {/* 头部: 工具名 + 状态 */}
      <div className="tool-header" onClick={() => setExpanded(!expanded)}>
        <span className="tool-icon">{icon}</span>
        <span className="tool-name">{getToolDisplayName(toolId)}</span>
        <span className="tool-status">
          {status === 'running' && <Spinner />}
          {status === 'success' && <CheckIcon />}
          {status === 'error' && <ErrorIcon />}
        </span>
        {durationMs && <span className="tool-duration">{formatDuration(durationMs)}</span>}
      </div>

      {/* 输入参数 (默认折叠) */}
      {expanded && (
        <div className="tool-input">
          <code>{JSON.stringify(input, null, 2)}</code>
        </div>
      )}

      {/* 输出结果 */}
      {result && expanded && (
        <div className="tool-result">
          {result.stdout && (
            <div className="tool-stdout">
              <pre>{result.stdout}</pre>
            </div>
          )}
          {result.stderr && (
            <div className="tool-stderr">
              <pre>{result.stderr}</pre>
            </div>
          )}
          {result.exitCode !== undefined && result.exitCode !== 0 && (
            <div className="tool-exit-code">exit code: {result.exitCode}</div>
          )}
        </div>
      )}
    </div>
  );
}
```

**新建** `apps/desktop/src/windows/ThinkingBlock.tsx`：

```tsx
export function ThinkingBlock({ thinking }: { thinking: string }) {
  const [visible, setVisible] = useState(false);
  return (
    <div className="thinking-block">
      <button className="thinking-toggle" onClick={() => setVisible(!visible)}>
        {visible ? '收起思考' : '查看思考过程'}
      </button>
      {visible && <div className="thinking-content">{thinking}</div>}
    </div>
  );
}
```

**新建** `apps/desktop/src/windows/StreamText.tsx`：

```tsx
/// 流式文本显示 — 逐字渲染 LLM 输出
export function StreamText({ tokens, isStreaming }: { tokens: string[]; isStreaming: boolean }) {
  const text = tokens.join('');
  return (
    <div className="stream-text">
      <Markdown>{text}</Markdown>
      {isStreaming && <span className="cursor-blink">|</span>}
    </div>
  );
}
```

**IPC 监听** (`apps/desktop/src/ipc/invoke.ts` 新增)：

```typescript
// 监听后端流式事件
import { listen } from '@tauri-apps/api/event';

export function onStreamChatToken(callback: (token: string) => void) {
  return listen<string>('stream-chat-token', (event) => callback(event.payload));
}

export function onToolExecutionUpdate(callback: (update: ToolExecutionUpdate) => void) {
  return listen<ToolExecutionUpdate>('tool-execution-update', (event) => callback(event.payload));
}

export function onThinkingUpdate(callback: (thinking: string) => void) {
  return listen<string>('thinking-update', (event) => callback(event.payload));
}

interface ToolExecutionUpdate {
  tool_use_id: string;
  tool_name: string;
  status: 'started' | 'progress' | 'completed' | 'error';
  input?: Record<string, any>;
  output?: any;
  error?: string;
  duration_ms?: number;
}
```

**CSS** (`apps/desktop/src/styles/app.css` 新增)：

```css
/* 工具调用卡片 */
.tool-use-card {
  border: 1px solid var(--border-color);
  border-radius: 8px;
  margin: 8px 0;
  overflow: hidden;
}
.tool-use-card .tool-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  cursor: pointer;
  background: var(--bg-secondary);
}
.tool-status-running .tool-header { border-left: 3px solid var(--accent-blue); }
.tool-status-success .tool-header { border-left: 3px solid var(--accent-green); }
.tool-status-error .tool-header { border-left: 3px solid var(--accent-red); }

.tool-use-card pre {
  background: var(--bg-code);
  padding: 8px;
  border-radius: 4px;
  overflow-x: auto;
  font-size: 12px;
  max-height: 300px;
}

/* 思考过程 */
.thinking-block { margin: 4px 0; }
.thinking-toggle {
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  border: none;
  background: none;
}
.thinking-content {
  padding: 8px;
  background: var(--bg-tertiary);
  border-radius: 4px;
  font-style: italic;
  color: var(--text-secondary);
}

/* 流式光标 */
.cursor-blink {
  animation: blink 1s step-end infinite;
}
@keyframes blink { 50% { opacity: 0; } }
```

**验收**：
- 工具调用显示为卡片，点击展开查看参数和结果
- 执行中显示 spinner，成功显示绿色对勾，失败显示红色叉
- bash.execute 的 stdout 超过 200 行时自动截断，显示"展开全部"按钮
- LLM 思考过程默认折叠，可点击查看

---

### T6: 集成测试与端到端验证

**测试场景**：

| # | 场景 | 预期 |
|---|------|------|
| 1 | "帮我看看 Rust 版本" | LLM 调用 `bash.execute { command: "rustc --version" }`，显示版本号 |
| 2 | "搜索项目中所有 .rs 文件" | LLM 调用 `file.glob { pattern: "**/*.rs" }`，显示文件列表 |
| 3 | "读取 Cargo.toml 的前 20 行" | LLM 调用 `file.read { file_path: "...", limit: 20 }`，显示文件内容 |
| 4 | "帮我写一个 hello world 到 test.txt" | LLM 调用 `file.write` → 工具卡片显示写入确认 |
| 5 | "查一下 tokio 的版本" → "再查一下 sqlx 的版本" | 两次独立对话，各展示工具调用链路 |
| 6 | 连续发送 3 条消息 | 每条消息独立触发 query_loop，不会互相干扰 |
| 7 | LLM 调用不存在的工具 | 工具卡片显示错误，对话继续 |
| 8 | Shell 命令超时 (> 120s) | 自动终止，显示超时提示 |

**验收命令**（在桌宠聊天框中输入）：

```
用户: 帮我看看 I:\personal-agent 下有哪些 Cargo.toml
LLM: [调用 file.glob { pattern: "**/Cargo.toml", path: "I:\\personal-agent" }]
     [返回结果]
     在 I:\personal-agent 下找到了 4 个 Cargo.toml：
     - I:\personal-agent\Cargo.toml
     - I:\personal-agent\crates\conductor-core\Cargo.toml
     - I:\personal-agent\crates\conductor-cli\Cargo.toml
     - I:\personal-agent\crates\conductor-sense\Cargo.toml
```

---

## 4. 风险评估

| 改动 | 影响范围 | 风险 | 缓解 |
|------|---------|------|------|
| 消息模型改造 | `chat.rs` + `ChatPanel.tsx` | **高** — 破坏现有消息格式 | 新旧格式兼容：`ContentBlock::Text` 兼容旧的纯文本消息 |
| Query Loop 改造 | `chat.rs` | **高** — 核心路径 | 保留旧的 `send_message` 作为 fallback，新 loop 用 `send_message_v2` |
| ShellExecutor | 新模块 | **低** — 纯新增 | 独立模块，不影响现有代码 |
| bash.execute 工具 | `tools.rs` | **中** — 高风险工具 | 默认不在白名单，需 `developer_mode: true` 或手动启用 |
| 前端组件改造 | `ChatPanel.tsx` | **中** — UI 变更 | 渐进式：先支持新格式，旧格式回退为纯文本 |

---

## 5. Claude Code 参考映射

| Round 4 模块 | Claude Code 对应 | 参考位置 | 移植策略 |
|---|---|---|---|
| 消息模型 `ContentBlock` | `src/utils/messages.ts` content blocks | line 409717 | 直接对齐 Anthropic API 格式 |
| Query Loop | `src/services/api/query.ts` `queryLoop()` | line 392957 | 简化版：去掉 context compaction，保留核心循环 |
| 流式 token 推送 | `src/services/api/claude.ts` `queryModel()` | line 424516 | Tauri event 替代 Ink re-render |
| ToolUseCard | `src/components/messages/AssistantToolUseMessage.tsx` | line 324134 | HTML/CSS 重写，不移植 Ink 组件 |
| BashTool | `src/tools/BashTool/BashTool.tsx` | line 384749 | Rust 重写 `exec3()`，去掉 sandbox |
| ShellExecutor | `src/utils/Shell.ts` `exec3()` | line 347867 | 简化：去掉 shell snapshot，保留 spawn + stream |
| ThinkingBlock | `src/components/messages/AssistantThinkingMessage.tsx` | line 323839 | 直接用 React 组件 |
| 工具并发执行 | `src/services/tools/toolOrchestration.ts` | line 286741 | 简化：先串行，后续加并发 |

---

## 6. 实施顺序与依赖

```
T1 (消息模型)  ←── T5 (前端组件) 依赖 T1 的数据结构
  ↓
T2 (Shell 抽象层) ←── T3 (bash.execute 注册) 依赖 T2
  ↓
T4 (Query Loop 改造) ←── 依赖 T1 + T3
  ↓
T6 (集成测试) ←── 依赖 T4 + T5
```

**推荐执行顺序**：T1 → T2 → T3 → T4 → T5 → T6

T1 和 T2 可以并行（无代码依赖），T5 可以在 T1 完成后立即开始（用 mock 数据开发）。

---

## 7. 向后兼容

- 旧的 `sendChatMessage` Tauri command 保留，新 command 叫 `sendChatMessageV2`
- `ChatPanel.tsx` 检测消息格式：如果 `content` 是 `string`（旧格式），包装为 `[{ type: "text", text: content }]`
- `bash.execute` 默认不在白名单，需要用户在设置中启用 `developer_mode` 或手动添加到 `allowed_tool_ids`
- 现有 33 个工具的注册代码不变，只新增 `bash.execute` 和 `bash.cancel`
