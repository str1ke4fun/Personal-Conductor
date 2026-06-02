# Claude Code → Personal Conductor 功能移植映射方案

> **状态**: ❌ 完全未实现 — 18 个工具(file.glob 等)均未实现；LLM 工具白名单已改为可配置（`config.llm.allowed_tool_ids`），但 `LlmToolsConfig` 结构体未创建，分级暴露未实现
>
> 基于 Claude Code v2.1.88（`I:\package\sdk-tools.d.ts`）与 personal-agent（`I:\personal-agent\crates\conductor-core\src\tools.rs`）的完整对比分析。
>
> **设计原则**：清和是一个有性格的桌面伴生宠物，不是通用编码助手。移植的工具必须经过角色化改造，不能直接照搬 Claude Code 的原始输出格式。

---

## 一、总览

Claude Code 暴露 **18 个工具** 给 LLM，personal-agent 当前注册 **27 个工具**。交集很少（~3 个概念重叠），大量 Claude Code 核心能力在 personal-agent 中完全缺失。

### 1.1 能力差距

| 能力域 | Claude Code | Personal Conductor | Gap | 建议优先级 |
|--------|------------|-------------------|-----|-----------|
| 文件系统搜索 | Glob, Grep | ❌ 无 | **严重** | P1 |
| 文件操作 | FileRead, FileWrite, FileEdit | ❌ 无通用工具 | **严重** | P1 |
| Web | WebSearch, WebFetch | ❌ 无 | **严重** | P2 |
| 富媒体 | Image/PDF/Notebook 读取 | ❌ 无 | **大** | P3 |
| Shell | Bash (timeout, background) | ❌ 无 | **大** | P4（需 developer_mode） |
| Agent | Agent (subagent_type, isolation) | ✅ 基础版 | 中 | - |
| 任务 | TodoWrite | ❌ 无会话级 | 中 | P2 |
| 交互 | AskUserQuestion | ❌ 无 | 中 | P2 |
| MCP | ListMcpResources, ReadMcpResource | ❌ 无资源模型 | 小 | P5 |
| 配置 | Config | ❌ 无 | 小 | P2 |
| 隔离 | EnterWorktree, ExitWorktree | ❌ 无 | 中 | P4 |
| 计划 | ExitPlanMode | ❌ 无 | 中 | P4 |

### 1.2 工具分级策略

所有工具按"桌面宠物适配度"分为三级：

| 级别 | 说明 | 工具 | 启用条件 |
|------|------|------|----------|
| **Tier 1（宠物原生）** | 安全、对话友好 | `file.read`, `file.glob`, `file.grep`, `file.stat`, `todo.write`, `interactive.ask`, `config.get` | 默认启用 |
| **Tier 2（进阶用户）** | 有用但需配置 | `file.write`, `file.edit`, `web.search`, `web.fetch`, `config.set` | 需用户在设置中启用 |
| **Tier 3（开发者模式）** | 高风险，仅限开发者 | `command.run`, `worktree.*` | 需 `developer_mode: true` |

---

## 二、前置条件：LLM 工具白名单

### 2.1 问题

`chat.rs:202-209` 硬编码了 LLM 可调用的工具白名单：

```rust
let allowed_ids = [
    "pet.set_avatar",
    "conductor.pet.set_avatar",
    "task.list",
    "task.get",
];
```

即使注册了 27+ 个工具，LLM 只能调用这 4 个。**新增工具必须先加入白名单才能被 LLM 使用。**

### 2.2 解决方案

将硬编码白名单改为可配置的 allowlist：

```rust
// config.rs 新增
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LlmToolsConfig {
    #[serde(default = "default_allowlist")]
    pub allowlist: Vec<String>,
    #[serde(default)]
    pub denylist: Vec<String>,
}

fn default_allowlist() -> Vec<String> {
    vec![
        "pet.set_avatar".into(),
        "conductor.pet.set_avatar".into(),
        "task.list".into(),
        "task.get".into(),
        // Phase 1 新增
        "file.read".into(),
        "file.glob".into(),
        "file.grep".into(),
        "file.stat".into(),
    ]
}
```

```rust
// chat.rs 修改
fn build_tool_definitions(config: &CoreConfig) -> Vec<ToolDefinition> {
    let allowlist = &config.llm_tools.allowlist;
    let denylist = &config.llm_tools.denylist;

    tools::list_tools()
        .iter()
        .filter(|spec| {
            if denylist.contains(&spec.id) { return false; }
            if !allowlist.iter().any(|a| matches_pattern(&spec.id, a)) { return false; }
            if config.pet.avatar_locked && is_avatar_tool(&spec.id) { return false; }
            true
        })
        .map(|spec| ToolDefinition { /* ... */ })
        .collect()
}
```

---

## 三、逐个工具移植方案

### 3.1 `file.glob` — 文件搜索

**Claude Code 原型**：
```typescript
interface GlobInput {
  pattern: string;    // Glob 模式，如 "**/*.ts"
  path?: string;      // 搜索根目录
}
```

**Conductor 实现**：
```rust
fn execute_file_glob(_spec: &ToolSpec, input: &Value) -> Result<ToolExecutionResult> {
    let pattern = input["pattern"].as_str().ok_or_else(|| anyhow::anyhow!("missing pattern"))?;
    let root = input["path"].as_str()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let glob_pattern = root.join(pattern);
    let mut matches = Vec::new();
    for entry in glob::glob(glob_pattern.to_str().unwrap_or("*"))? {
        if let Ok(path) = entry {
            matches.push(path.display().to_string());
        }
    }
    matches.sort();
    let count = matches.len();
    Ok(ToolExecutionResult {
        success: true,
        output: json!({ "matches": matches, "count": count }),
        error: None,
        duration_ms: 0,
    })
}
```

**ToolSpec 注册**：
```rust
ToolSpec {
    id: "file.glob".to_string(),
    name: "搜索文件".to_string(),
    description: "按 Glob 模式匹配文件路径".to_string(),
    provider: ToolProviderKind::Internal,
    input_schema: json!({
        "type": "object",
        "properties": {
            "pattern": { "type": "string", "description": "Glob 模式，如 **/*.rs" },
            "path": { "type": "string", "description": "搜索根目录（默认当前目录）" }
        },
        "required": ["pattern"]
    }),
    output_schema: json!({
        "type": "object",
        "properties": {
            "matches": { "type": "array", "items": { "type": "string" } },
            "count": { "type": "integer" }
        }
    }),
    risk_level: RiskLevel::ReadOnly,
    permissions: vec![ToolPermission::ReadWorkspace],
    supports_dry_run: true,
    workspace_required: false,
}
```

**依赖**：`glob = "0.3"`

**测试用例**：
- 空目录返回空列表
- 嵌套模式 `src/**/*.rs`
- 通配符 `*.{ts,tsx}`

---

### 3.2 `file.grep` — 内容搜索

**Claude Code 原型**：
```typescript
interface GrepInput {
  pattern: string;
  path?: string;
  glob?: string;
  output_mode?: "content" | "files_with_matches" | "count";
  "-i"?: boolean;
  "-C"?: number;
  head_limit?: number;
}
```

**Conductor 实现**：复用 `vendor/rg.exe`（ripgrep 已存在）。

```rust
fn execute_file_grep(_spec: &ToolSpec, input: &Value) -> Result<ToolExecutionResult> {
    let pattern = input["pattern"].as_str().ok_or_else(|| anyhow::anyhow!("missing pattern"))?;
    let rg_path = Paths::root().join("vendor").join(if cfg!(windows) { "rg.exe" } else { "rg" });

    let mut cmd = std::process::Command::new(&rg_path);
    cmd.arg("--json").arg("--line-number");

    if input.get("-i").and_then(|v| v.as_bool()).unwrap_or(false) {
        cmd.arg("-i");
    }
    if let Some(ctx) = input.get("-C").and_then(|v| v.as_u64()) {
        cmd.arg("-C").arg(ctx.to_string());
    }
    if let Some(glob) = input.get("glob").and_then(|v| v.as_str()) {
        cmd.arg("--glob").arg(glob);
    }
    if let Some(limit) = input.get("head_limit").and_then(|v| v.as_u64()) {
        cmd.arg("--max-count").arg(limit.to_string());
    }

    cmd.arg(pattern);
    if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
        cmd.arg(path);
    }

    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let matches: Vec<&str> = stdout.lines().take(100).collect();
    let match_count = matches.len();

    Ok(ToolExecutionResult {
        success: output.status.success(),
        output: json!({ "matches": matches, "count": match_count }),
        error: if output.status.success() { None } else { Some(String::from_utf8_lossy(&output.stderr).to_string()) },
        duration_ms: 0,
    })
}
```

**关键设计决策**：
- 使用 `--json` 输出模式逐行解析 ripgrep 输出
- 结果数量上限：默认 100 匹配
- 使用 `cfg!(windows)` 处理平台差异
- 单个文件最多读取前 10MB

---

### 3.3 `file.read` — 文件读取

**Claude Code 原型**：
```typescript
interface FileReadInput {
  file_path: string;
  offset?: number;      // 起始行号
  limit?: number;       // 最大行数
}
```

**Conductor 实现**（先实现纯文本，Phase 3 扩展富媒体）：

```rust
fn execute_file_read(_spec: &ToolSpec, input: &Value) -> Result<ToolExecutionResult> {
    let path = input["file_path"].as_str().ok_or_else(|| anyhow::anyhow!("missing file_path"))?;
    let path = PathBuf::from(path);

    // 自动检测文件类型
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" => read_image_file(&path),  // Phase 3
        "pdf" => read_pdf_file(&path, input),                                 // Phase 3
        "ipynb" => read_notebook_file(&path),                                 // Phase 3
        _ => read_text_file(&path, input),
    }
}

fn read_text_file(path: &Path, input: &Value) -> Result<ToolExecutionResult> {
    let offset = input["offset"].and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit = input["limit"].and_then(|v| v.as_u64()).unwrap_or(2000) as usize;

    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = offset.min(lines.len());
    let end = (start + limit).min(lines.len());
    let selected = &lines[start..end];

    let text = selected.iter().enumerate()
        .map(|(i, line)| format!("{}: {}", start + i + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(ToolExecutionResult {
        success: true,
        output: json!({
            "type": "text",
            "text": text,
            "total_lines": lines.len(),
            "offset": start,
            "limit": limit,
        }),
        error: None,
        duration_ms: 0,
    })
}
```

**注意**：`read_text_file` 使用同步 `std::fs::read_to_string`。如果 Phase 0 的异步化重构已完成，应改为 `tokio::fs::read_to_string`。

---

### 3.4 `file.write` — 文件写入

```rust
fn execute_file_write(_spec: &ToolSpec, input: &Value) -> Result<ToolExecutionResult> {
    let path = input["file_path"].as_str().ok_or_else(|| anyhow::anyhow!("missing file_path"))?;
    let content = input["content"].as_str().ok_or_else(|| anyhow::anyhow!("missing content"))?;

    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;

    Ok(ToolExecutionResult {
        success: true,
        output: json!({ "path": path, "bytes": content.len() }),
        error: None,
        duration_ms: 0,
    })
}
```

**风险等级**：`WorkspaceWrite` — 需要 Workspace 权限，走提案系统审批。

---

### 3.5 `file.edit` — 精确编辑

**Claude Code 原型**：
```typescript
interface FileEditInput {
  file_path: string;
  old_string: string;       // 要替换的原文
  new_string: string;       // 新内容
  replace_all?: boolean;
}
```

**Conductor 实现**：
```rust
fn execute_file_edit(_spec: &ToolSpec, input: &Value) -> Result<ToolExecutionResult> {
    let path = input["file_path"].as_str().ok_or_else(|| anyhow::anyhow!("missing file_path"))?;
    let old = input["old_string"].as_str().ok_or_else(|| anyhow::anyhow!("missing old_string"))?;
    let new = input["new_string"].as_str().ok_or_else(|| anyhow::anyhow!("missing new_string"))?;
    let replace_all = input["replace_all"].and_then(|v| v.as_bool()).unwrap_or(false);

    let content = std::fs::read_to_string(path)?;

    let (new_content, count) = if replace_all {
        (content.replace(old, new), content.matches(old).count())
    } else {
        match content.find(old) {
            Some(pos) => {
                let new_c = format!("{}{}{}", &content[..pos], new, &content[pos + old.len()..]);
                (new_c, 1)
            }
            None => return Ok(ToolExecutionResult {
                success: false,
                output: json!({}),
                error: Some("old_string not found in file".to_string()),
                duration_ms: 0,
            }),
        }
    };

    std::fs::write(path, &new_content)?;

    Ok(ToolExecutionResult {
        success: true,
        output: json!({
            "success": true,
            "replacements": count,
            "path": path,
        }),
        error: None,
        duration_ms: 0,
    })
}
```

**风险等级**：`WorkspaceWrite` — 走提案系统审批。

---

### 3.6 `file.stat` — 文件元数据

```rust
fn execute_file_stat(_spec: &ToolSpec, input: &Value) -> Result<ToolExecutionResult> {
    let path = input["file_path"].as_str().ok_or_else(|| anyhow::anyhow!("missing file_path"))?;
    let metadata = std::fs::metadata(path)?;

    Ok(ToolExecutionResult {
        success: true,
        output: json!({
            "path": path,
            "size": metadata.len(),
            "is_dir": metadata.is_dir(),
            "is_file": metadata.is_file(),
            "modified": metadata.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
            "readonly": metadata.permissions().readonly(),
        }),
        error: None,
        duration_ms: 0,
    })
}
```

**风险等级**：`ReadOnly`

---

### 3.7 `web.search` — Web 搜索

**Conductor 实现**：
```rust
fn execute_web_search(_spec: &ToolSpec, input: &Value) -> Result<ToolExecutionResult> {
    let query = input["query"].as_str().ok_or_else(|| anyhow::anyhow!("missing query"))?;

    let runtime = tokio::runtime::Runtime::new()?;
    let config = runtime.block_on(crate::config::load())?;
    let search_config = config.web_search.as_ref()
        .ok_or_else(|| anyhow::anyhow!("web search not configured"))?;

    let results = runtime.block_on(do_search(query, search_config))?;

    Ok(ToolExecutionResult {
        success: true,
        output: json!({ "results": results, "query": query }),
        error: None,
        duration_ms: 0,
    })
}
```

**搜索引擎选项**（在 `CoreConfig.web_search.provider` 中配置）：
- `serpapi` — 推荐，简单 SaaS
- `bing` — Azure Bing Search API
- `searxng` — 自托管，隐私友好

**风险等级**：`ReadOnly` + `Network` permission

**隐私注意**：
- API key 使用系统 keyring 加密存储，不写入 config.json
- 支持域名白名单（`allowed_domains`）和黑名单（`blocked_domains`）

---

### 3.8 `web.fetch` — URL 获取

```rust
fn execute_web_fetch(_spec: &ToolSpec, input: &Value) -> Result<ToolExecutionResult> {
    let url = input["url"].as_str().ok_or_else(|| anyhow::anyhow!("missing url"))?;

    let runtime = tokio::runtime::Runtime::new()?;
    let resp = runtime.block_on(reqwest::get(url))?;
    let content = runtime.block_on(resp.text())?;

    // 截断到 100KB
    let truncated = content.len() > 100_000;
    let text = if truncated { &content[..100_000] } else { &content };

    Ok(ToolExecutionResult {
        success: true,
        output: json!({ "content": text, "url": url, "truncated": truncated }),
        error: None,
        duration_ms: 0,
    })
}
```

**风险等级**：`ReadOnly` + `Network`

---

### 3.9 `interactive.ask` — 用户交互

**需要 Tauri 前端协作**：

```rust
fn execute_interactive_ask(_spec: &ToolSpec, input: &Value) -> Result<ToolExecutionResult> {
    let questions = input["questions"].as_array()
        .ok_or_else(|| anyhow::anyhow!("missing questions"))?;

    // 通过 Tauri IPC 发送到前端，等待用户回答
    // 注意：这需要在异步上下文中运行，使用 channel 等待前端回复
    let answer = tauri_ask_questions(questions)?;

    Ok(ToolExecutionResult {
        success: true,
        output: json!({ "answers": answer }),
        error: None,
        duration_ms: 0,
    })
}
```

**前端组件**（`apps/desktop/src/components/QuestionDialog.tsx`）：
- 模态对话框，选项按钮（单选/多选）
- 支持 Markdown 预览
- 超时自动取消（30s）

**风险等级**：`ReadOnly`

---

### 3.10 `todo.write` — 会话任务

```rust
use std::sync::RwLock;

struct TodoItem {
    id: String,
    content: String,
    status: TodoStatus,
}

static TODO_STATE: std::sync::LazyLock<RwLock<Vec<TodoItem>>> =
    std::sync::LazyLock::new(|| RwLock::new(Vec::new()));

fn execute_todo_write(_spec: &ToolSpec, input: &Value) -> Result<ToolExecutionResult> {
    let todos = input["todos"].as_array()
        .ok_or_else(|| anyhow::anyhow!("missing todos"))?;

    let mut state = TODO_STATE.write().unwrap();
    state.clear();

    for item in todos {
        state.push(TodoItem {
            id: uuid::Uuid::new_v4().to_string(),
            content: item["content"].as_str().unwrap_or("").to_string(),
            status: match item["status"].as_str().unwrap_or("pending") {
                "in_progress" => TodoStatus::InProgress,
                "completed" => TodoStatus::Completed,
                _ => TodoStatus::Pending,
            },
        });
    }

    Ok(ToolExecutionResult {
        success: true,
        output: json!({ "count": state.len() }),
        error: None,
        duration_ms: 0,
    })
}
```

**风险等级**：`ReadOnly`

---

### 3.11 `worktree.*` — Git Worktree 隔离

```rust
fn execute_worktree_enter(_spec: &ToolSpec, input: &Value) -> Result<ToolExecutionResult> {
    let name = input["name"].as_str().ok_or_else(|| anyhow::anyhow!("missing name"))?;

    let output = std::process::Command::new("git")
        .args(["worktree", "add", &format!("../.worktree/{}", name), name])
        .output()?;

    Ok(ToolExecutionResult {
        success: output.status.success(),
        output: json!({
            "worktree_path": format!("../.worktree/{}", name),
            "success": output.status.success(),
        }),
        error: if output.status.success() { None } else { Some(String::from_utf8_lossy(&output.stderr).to_string()) },
        duration_ms: 0,
    })
}
```

**风险等级**：`WorkspaceWrite` — 仅在 `developer_mode: true` 时启用。

---

### 3.12 `config.get` / `config.set`

```rust
fn execute_config_get(_spec: &ToolSpec, input: &Value) -> Result<ToolExecutionResult> {
    let setting = input["setting"].as_str().ok_or_else(|| anyhow::anyhow!("missing setting"))?;

    let runtime = tokio::runtime::Runtime::new()?;
    let config = runtime.block_on(crate::config::load())?;

    // 通过 setting path 访问嵌套字段
    let value = get_nested_config(&config, setting);

    Ok(ToolExecutionResult {
        success: true,
        output: json!({ "setting": setting, "value": value }),
        error: None,
        duration_ms: 0,
    })
}
```

**风险等级**：`config.get` = `ReadOnly`，`config.set` = `WorkspaceWrite`

---

## 四、角色化输出中间件

### 4.1 问题

Claude Code 工具返回原始 JSON/机器可读输出。作为桌面宠物，清和应该用角色化的语言呈现结果。

### 4.2 方案

在 `chat.rs` 的工具执行结果返回后，调用 formatter：

```rust
fn format_tool_result_for_pet(tool_id: &str, result: &ToolExecutionResult) -> String {
    if !result.success {
        return match tool_id {
            "file.read" => "这个文件我打不开呢，可能是路径不对或者权限不够。",
            "file.grep" => "搜索遇到了问题，换个关键词试试？",
            _ => "操作没有成功，我再看看。",
        }.to_string();
    }

    match tool_id {
        "file.glob" => {
            let count = result.output["count"].as_u64().unwrap_or(0);
            format!("找到了 {} 个文件。", count)
        }
        "file.grep" => {
            let count = result.output["count"].as_u64().unwrap_or(0);
            format!("搜到 {} 处匹配。", count)
        }
        "file.read" => {
            let total = result.output["total_lines"].as_u64().unwrap_or(0);
            let text = result.output["text"].as_str().unwrap_or("");
            if text.len() > 500 {
                format!("文件共 {} 行，以下是前 500 字：\n{}...", total, &text[..500])
            } else {
                format!("文件共 {} 行：\n{}", total, text)
            }
        }
        _ => serde_json::to_string(&result.output).unwrap_or_default(),
    }
}
```

### 4.3 子形象反馈

工具执行时自动切换子形象：

| 工具状态 | ActivityVariant | 说明 |
|----------|-----------------|------|
| 开始执行 | `ToolCalling` | 已有 |
| 读取文件 | `Reading` | 新增 |
| Web 搜索 | `Searching` | 新增 |
| 执行成功 | `Done` → `Idle`（2s延迟） | 已有 |
| 执行失败 | `Error` | 新增 |

---

## 五、集成点

### 5.1 权限模型映射

| 工具 | Conductor RiskLevel | Conductor Permission | 提案审批 |
|------|--------------------|--------------------|----------|
| `file.glob` | `ReadOnly` | `ReadWorkspace` | 否 |
| `file.grep` | `ReadOnly` | `ReadWorkspace` | 否 |
| `file.read` | `ReadOnly` | `ReadWorkspace` | 否 |
| `file.stat` | `ReadOnly` | `ReadWorkspace` | 否 |
| `file.write` | `WorkspaceWrite` | `WriteWorkspace` | **是** |
| `file.edit` | `WorkspaceWrite` | `WriteWorkspace` | **是** |
| `web.search` | `ReadOnly` | `Network` | 否 |
| `web.fetch` | `ReadOnly` | `Network` | 否 |
| `interactive.ask` | `ReadOnly` | — | 否 |
| `todo.write` | `ReadOnly` | — | 否 |
| `config.get` | `ReadOnly` | `ReadWorkspace` | 否 |
| `config.set` | `WorkspaceWrite` | `WriteWorkspace` | **是** |
| `worktree.*` | `WorkspaceWrite` | `WriteWorkspace` | **是** |
| `command.run` | `Destructive` | `SystemControl` | **是** |

### 5.2 Workspace 集成

- `file.*` 操作应在当前 workspace 范围内
- 跨 workspace 访问需要 `ReadExternalPath` / `WriteExternalPath` 权限
- 通过 `validate_workspace_context_async` 检查信任级别

---

## 六、实施计划

### Phase 0（第 0 周）：前置条件

| 任务 | 说明 |
|------|------|
| LLM 工具白名单可配置化 | 修改 `chat.rs:build_tool_definitions()` |
| 异步执行器类型 | 新增 `AsyncToolExecutorFn` |
| 配置扩展 | `config.rs` 新增 `LlmToolsConfig` + `WebSearchConfig` |

### Phase 1（第 1-4 周）：文件工具

| 工具 | 代码量 | 依赖 |
|------|--------|------|
| `file.glob` | ~40 行 | `glob = "0.3"` |
| `file.grep` | ~60 行 | vendor/rg.exe |
| `file.read` (文本) | ~40 行 | — |
| `file.write` | ~20 行 | — |
| `file.edit` | ~50 行 | — |
| `file.stat` | ~20 行 | — |
| **角色化 formatter** | ~80 行 | — |
| **子形象扩展** | ~30 行 | 新增图片资源 |
| **合计** | ~340 行 | |

### Phase 2（第 5-8 周）：Web + 交互

| 工具 | 代码量 | 依赖 |
|------|--------|------|
| `web.search` | ~50 行 | reqwest（已有） |
| `web.fetch` | ~30 行 | reqwest |
| `interactive.ask` | ~40 行 | Tauri IPC |
| `todo.write` | ~30 行 | — |
| `config.get/set` | ~40 行 | — |
| **合计** | ~190 行 | |

### Phase 3（第 9-12 周）：富媒体

| 工具 | 代码量 | 依赖 |
|------|--------|------|
| `file.read` (图片) | ~40 行 | `image = "0.25"`, `base64 = "0.22"` |
| `file.read` (PDF) | ~30 行 | `lopdf = "0.34"` |
| `file.read` (Notebook) | ~30 行 | — |
| **合计** | ~100 行 | |

### Phase 4（第 13-16 周）：沙箱

| 工具 | 代码量 | 依赖 |
|------|--------|------|
| `worktree.enter` | ~30 行 | git CLI |
| `worktree.exit` | ~30 行 | git CLI |
| **合计** | ~60 行 | |

---

## 七、风险评估

| 改动 | 影响范围 | 风险 | 缓解 |
|------|---------|------|------|
| 白名单配置化 | `chat.rs` | 中（核心路径） | 严格测试 allowlist/denylist 逻辑 |
| 异步化重构 | 所有 executor | 高（破坏性） | 逐步迁移，保持旧签名兼容 |
| 新增 14 个工具 | `tools.rs` | 低（纯新增） | 现有测试不变 |
| 角色化 formatter | `chat.rs` | 低（可选中间件） | formatter 失败时回退原始输出 |
| 前端组件 | `apps/desktop/` | 低（独立组件） | 不影响现有 UI |
| 配置扩展 | `config.rs` | 低（向后兼容） | `#[serde(default)]` |

**向后兼容性**：除异步化重构外，全部为新增，不影响现有 27 个工具。现有测试不变。
