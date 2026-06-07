# ChatTurn Schema、迁移与接口细化设计

日期：2026-06-03

关联：
- `docs/chat-turn-bus-state-model-20260603.md`
- `docs/chat-turn-state-and-memory-design-20260603.md`

## 1. 目标

本设计文档回答三件事：

1. 这一轮要新增哪些表、字段、索引。
2. 迁移如何兼容当前 `chat_messages / chat_sessions / tool_calls / runtime_events`。
3. Rust 侧需要哪些最小接口，才能把 `send_v2 -> turn -> event -> projection -> memory_candidate` 打通。

这里不再讨论抽象原则，直接落编码级定义。

## 0. 2026-06-04 状态校正

本文的 schema / DAO 设计已经大部分落地，但“本轮实施范围”段落已有两处过时：

1. 前端主读取链路已经通过 `get_chat_session_messages_v2 -> useChatSession` 走 projection 优先、legacy 兜底，不再是“未切到 projection”。
2. `memory_candidates -> memory_entries -> memory_chunks / memory_embeddings` 已有 best-effort promotion 和 recall 闭环，但还不是完整治理层，也没有 `RecallDoc`。

2026-06-04 阻塞修复已补齐的接口：

1. `RecallContext.session_id / goal_id` 已从“只携带”升级为“参与检索过滤”，并由 prompt 构造链路传入。
2. `memory_entries` 已保存 turn promotion 带来的来源 session / turn / message / projection / tool / goal 线索，用于按 session/goal 排除外部记忆。
3. `chat_message_projections.plain_text` 的写入方已使用用户可见文本提取，不再复用给 LLM prompt 的 tool-result 扁平化逻辑。
4. final retry / recovery 已使用独立 `max_tokens`，避免长任务总结在恢复路径被截断。

当前仍应继续补齐的是 `RecallDoc`、治理层、turn-event realtime replay 和历史 backfill。

## 2. 本轮实施范围

本轮只打通主链，不在这一轮完成所有消费方切换。

本轮主链：

```text
send_message_v2_with_session_projection
  -> chat_turns
  -> chat_turn_events
  -> chat_messages(turn_id)
  -> chat_message_projections
  -> tool_calls(turn_id)
  -> memory_candidates
  -> memory_entries
```

本轮明确不做：

1. 前端读取完全切到 `chat_message_projections`
2. `memory_candidates -> memory_entries -> recall_docs` 的完整自动治理
3. 旧 `runtime_events` 的全面替换

## 3. SQLite Schema

### 3.1 `chat_turns`

用途：

- 记录一轮聊天请求的规范事实
- 稳定承接 `request_id`
- 汇总 message/tool/memory 状态

建议 DDL：

```sql
CREATE TABLE IF NOT EXISTS chat_turns (
  id TEXT PRIMARY KEY,
  session_id TEXT,
  projection_session_id TEXT,
  workspace_id TEXT,
  request_id TEXT NOT NULL UNIQUE,
  initiator_kind TEXT NOT NULL DEFAULT 'user',
  task_mode TEXT NOT NULL DEFAULT 'short',
  capability TEXT NOT NULL DEFAULT 'ask_write',
  status TEXT NOT NULL DEFAULT 'received',
  stage TEXT,
  user_message_id TEXT,
  assistant_message_id TEXT,
  llm_round_count INTEGER NOT NULL DEFAULT 0,
  tool_run_count INTEGER NOT NULL DEFAULT 0,
  active_tool_count INTEGER NOT NULL DEFAULT 0,
  projection_status TEXT NOT NULL DEFAULT 'pending',
  memory_status TEXT NOT NULL DEFAULT 'pending',
  model_provider TEXT,
  model_name TEXT,
  error TEXT,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  metadata_json TEXT NOT NULL DEFAULT '{}'
);
```

建议索引：

```sql
CREATE INDEX IF NOT EXISTS idx_chat_turns_session ON chat_turns(session_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_turns_projection_session ON chat_turns(projection_session_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_turns_workspace ON chat_turns(workspace_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_turns_status ON chat_turns(status, started_at DESC);
```

### 3.2 `chat_turn_events`

用途：

- 作为 chat 域 append-only 事件流
- 记录 turn 内阶段、投影、记忆、恢复等关键动作

建议 DDL：

```sql
CREATE TABLE IF NOT EXISTS chat_turn_events (
  id TEXT PRIMARY KEY,
  turn_id TEXT NOT NULL,
  session_id TEXT,
  workspace_id TEXT,
  request_id TEXT NOT NULL,
  seq INTEGER NOT NULL,
  event_type TEXT NOT NULL,
  phase TEXT,
  actor_kind TEXT NOT NULL DEFAULT 'system',
  actor_id TEXT,
  payload_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);
```

建议索引：

```sql
CREATE UNIQUE INDEX IF NOT EXISTS idx_chat_turn_events_turn_seq ON chat_turn_events(turn_id, seq);
CREATE INDEX IF NOT EXISTS idx_chat_turn_events_request ON chat_turn_events(request_id, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_chat_turn_events_session ON chat_turn_events(session_id, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_chat_turn_events_type ON chat_turn_events(event_type, created_at DESC);
```

### 3.3 `chat_message_projections`

用途：

- 为前端和审计保留稳定消息投影
- 与 `chat_messages` 并存，逐步替换

建议 DDL：

```sql
CREATE TABLE IF NOT EXISTS chat_message_projections (
  id TEXT PRIMARY KEY,
  turn_id TEXT NOT NULL,
  message_id TEXT,
  session_id TEXT,
  workspace_id TEXT,
  role TEXT NOT NULL,
  projection_kind TEXT NOT NULL,
  status TEXT NOT NULL,
  visibility TEXT NOT NULL DEFAULT 'visible',
  plain_text TEXT,
  content_blocks_json TEXT NOT NULL DEFAULT '[]',
  source_event_id TEXT,
  seq INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

建议索引：

```sql
CREATE INDEX IF NOT EXISTS idx_chat_msg_proj_turn ON chat_message_projections(turn_id, seq ASC);
CREATE INDEX IF NOT EXISTS idx_chat_msg_proj_session ON chat_message_projections(session_id, seq ASC);
CREATE INDEX IF NOT EXISTS idx_chat_msg_proj_message ON chat_message_projections(message_id);
CREATE INDEX IF NOT EXISTS idx_chat_msg_proj_status ON chat_message_projections(status, updated_at DESC);
```

### 3.4 `memory_candidates`

用途：

- 记录 turn 结束后抽出的待治理记忆
- 与 `memory_entries` 解耦

建议 DDL：

```sql
CREATE TABLE IF NOT EXISTS memory_candidates (
  id TEXT PRIMARY KEY,
  turn_id TEXT NOT NULL,
  session_id TEXT,
  workspace_id TEXT,
  source_message_id TEXT,
  source_projection_id TEXT,
  source_tool_call_id TEXT,
  memory_kind TEXT NOT NULL,
  scope_kind TEXT NOT NULL,
  scope_ref TEXT,
  path_prefix TEXT,
  key TEXT NOT NULL,
  value_json TEXT NOT NULL DEFAULT '{}',
  summary TEXT NOT NULL,
  evidence_json TEXT NOT NULL DEFAULT '{}',
  extractor_kind TEXT NOT NULL DEFAULT 'rule',
  extractor_provider TEXT,
  extractor_model TEXT,
  confidence REAL NOT NULL DEFAULT 0.0,
  status TEXT NOT NULL DEFAULT 'proposed',
  dedupe_key TEXT NOT NULL,
  promoted_memory_entry_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

建议索引：

```sql
CREATE INDEX IF NOT EXISTS idx_memory_candidates_turn ON memory_candidates(turn_id, created_at ASC);
CREATE INDEX IF NOT EXISTS idx_memory_candidates_workspace ON memory_candidates(workspace_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_memory_candidates_status ON memory_candidates(status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_memory_candidates_dedupe ON memory_candidates(dedupe_key);
```

### 3.5 旧表新增字段

#### `chat_messages`

新增：

```sql
ALTER TABLE chat_messages ADD COLUMN turn_id TEXT;
```

用途：

- 将旧消息表与新 `chat_turns` 挂接
- 保留兼容，同时让旧查询能反查 turn

建议索引：

```sql
CREATE INDEX IF NOT EXISTS idx_chat_messages_turn ON chat_messages(turn_id, seq ASC);
```

#### `tool_calls`

新增：

```sql
ALTER TABLE tool_calls ADD COLUMN turn_id TEXT;
```

用途：

- 让工具事实直接归属于某个 turn

建议索引：

```sql
CREATE INDEX IF NOT EXISTS idx_tool_calls_turn ON tool_calls(turn_id, started_at ASC);
```

## 4. 迁移策略

### Phase 1: 增量建表

先新增：

- `chat_turns`
- `chat_turn_events`
- `chat_message_projections`
- `memory_candidates`

这一步不改旧逻辑。

### Phase 2: 旧表增列

再对旧表补列：

- `chat_messages.turn_id`
- `tool_calls.turn_id`

这一步也不要求立即补全历史数据。

### Phase 3: 新请求写新链路

只有新产生的 turn 会写：

- `chat_turns`
- `chat_turn_events`
- `chat_message_projections`
- `memory_candidates`

旧数据继续按旧表读取。

### Phase 4: 渐进回填

历史回填可以后置，不阻塞本轮主链：

1. 根据 `chat_messages.session_id + run_id/request_id 痕迹` 回推旧 turn
2. 根据 `tool_calls.session_id + llm_tool_call_id` 回挂旧 tool turn
3. 根据旧 assistant message 重建 message projection

本轮不做。

## 5. Rust 模块与接口

## 5.1 新模块位置

建议新增：

- `crates/conductor-core/src/chat/turns.rs`

职责：

- turn 主表
- turn event 表
- message projection 表
- memory candidate 表

不把这些逻辑散进 `chat/db.rs`，避免旧 message DAO 和新 turn DAO 混杂。

## 5.2 类型定义

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurnRecord {
    pub id: String,
    pub session_id: Option<String>,
    pub projection_session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub request_id: String,
    pub initiator_kind: String,
    pub task_mode: String,
    pub capability: String,
    pub status: String,
    pub stage: Option<String>,
    pub user_message_id: Option<String>,
    pub assistant_message_id: Option<String>,
    pub llm_round_count: i64,
    pub tool_run_count: i64,
    pub active_tool_count: i64,
    pub projection_status: String,
    pub memory_status: String,
    pub model_provider: Option<String>,
    pub model_name: Option<String>,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub metadata_json: serde_json::Value,
}

pub struct ChatTurnCreate {
    pub session_id: Option<String>,
    pub projection_session_id: Option<String>,
    pub workspace_id: Option<String>,
    pub request_id: String,
    pub initiator_kind: String,
    pub task_mode: String,
    pub capability: String,
    pub model_provider: Option<String>,
    pub model_name: Option<String>,
    pub metadata_json: serde_json::Value,
}

pub struct ChatTurnEventRecord { ... }
pub struct MessageProjectionRecord { ... }
pub struct MemoryCandidateRecord { ... }
```

## 5.3 最小接口

### Turn

```rust
pub async fn create_turn(input: ChatTurnCreate) -> anyhow::Result<ChatTurnRecord>;
pub async fn find_turn_by_request_id(request_id: &str) -> anyhow::Result<Option<ChatTurnRecord>>;
pub async fn update_turn_stage_by_request(
    request_id: &str,
    status: &str,
    stage: Option<&str>,
    error: Option<&str>,
) -> anyhow::Result<()>;
pub async fn update_turn_counts_by_request(
    request_id: &str,
    llm_round_count: Option<i64>,
    tool_run_count: Option<i64>,
    active_tool_count: Option<i64>,
) -> anyhow::Result<()>;
pub async fn attach_user_message_by_request(
    request_id: &str,
    message_id: &str,
) -> anyhow::Result<()>;
pub async fn attach_assistant_message_by_request(
    request_id: &str,
    message_id: &str,
) -> anyhow::Result<()>;
pub async fn mark_turn_projection_status_by_request(
    request_id: &str,
    projection_status: &str,
) -> anyhow::Result<()>;
pub async fn mark_turn_memory_status_by_request(
    request_id: &str,
    memory_status: &str,
) -> anyhow::Result<()>;
pub async fn finish_turn_by_request(
    request_id: &str,
    status: &str,
    error: Option<&str>,
) -> anyhow::Result<()>;
```

### Event

```rust
pub async fn append_turn_event_by_request(
    request_id: &str,
    event_type: &str,
    phase: Option<&str>,
    actor_kind: &str,
    actor_id: Option<&str>,
    payload_json: serde_json::Value,
) -> anyhow::Result<Option<ChatTurnEventRecord>>;
```

### Message projection

```rust
pub struct MessageProjectionCreate {
    pub request_id: String,
    pub message_id: Option<String>,
    pub role: String,
    pub projection_kind: String,
    pub status: String,
    pub visibility: String,
    pub plain_text: Option<String>,
    pub content_blocks_json: serde_json::Value,
    pub source_event_id: Option<String>,
}

pub async fn create_message_projection(
    input: MessageProjectionCreate,
) -> anyhow::Result<MessageProjectionRecord>;
```

### Memory candidate

```rust
pub struct MemoryCandidateCreate {
    pub request_id: String,
    pub source_message_id: Option<String>,
    pub source_projection_id: Option<String>,
    pub source_tool_call_id: Option<String>,
    pub memory_kind: String,
    pub scope_kind: String,
    pub scope_ref: Option<String>,
    pub path_prefix: Option<String>,
    pub key: String,
    pub value_json: serde_json::Value,
    pub summary: String,
    pub evidence_json: serde_json::Value,
    pub extractor_kind: String,
    pub extractor_provider: Option<String>,
    pub extractor_model: Option<String>,
    pub confidence: f64,
    pub status: String,
    pub dedupe_key: String,
}

pub async fn create_memory_candidate(
    input: MemoryCandidateCreate,
) -> anyhow::Result<MemoryCandidateRecord>;
```

本轮实际已补充的治理闭环：

```rust
pub async fn list_turn_events_by_request(
    request_id: &str,
) -> anyhow::Result<Vec<ChatTurnEventRecord>>;

pub async fn list_message_projections_by_session(
    session_id: &str,
    limit: Option<u32>,
) -> anyhow::Result<Vec<MessageProjectionRecord>>;

pub async fn list_message_projections_by_request(
    request_id: &str,
) -> anyhow::Result<Vec<MessageProjectionRecord>>;

pub async fn get_chat_session_messages_v2(
    session_id: &str,
    limit: Option<u32>,
) -> anyhow::Result<Vec<ChatMessageV2>>;
```

以及一条 best-effort promotion 路径：

```text
memory_candidates
  -> memory_entries
  -> memory_chunks + memory_embeddings
  -> recall_for_prompt_with_context(...)
```

### Tool call 关联

在 `tool_calls.rs` 增加：

```rust
pub async fn attach_turn(id: &str, turn_id: &str) -> Result<()>;
```

在 `ToolCallCreate` 增加：

```rust
pub turn_id: Option<String>,
```

## 6. `send_v2` 具体接线顺序

## 6.1 外层 `send_message_v2_with_session_projection`

顺序：

1. 生成 `request_id`
2. 解析 `workspace_id`
3. 创建 `chat_turns`
4. 注册 `active_run`
5. 调用现有 `append_chat_stage("received")`

关键点：

- `append_chat_stage()` 内部将新增一条支路：镜像写 `chat_turn_events`
- 这样不用改所有阶段调用点

## 6.2 内层 `send_message_v2_inner`

顺序：

1. 插入 user message，拿到 `message_id`
2. `attach_user_message_by_request()`
3. 写一条 user `chat_message_projection`
4. `append_chat_stage("user_message_stored")`
5. 进入 LLM/tool loop
6. 关键阶段继续用现有 `append_chat_stage()`

## 6.3 工具执行

在 `chat/tools.rs::execute_tool_call()`：

1. 创建 `tool_calls` 时写入 `turn_id`
2. 或在 create 后 `attach_turn()`

由于 `send_v2` 已知 `request_id`，推荐给 `execute_tool_call()` 增加参数：

```rust
request_id: Option<&str>
```

然后在工具创建时查 turn 并写 `turn_id`。

## 6.4 回复落库

assistant message 写入后：

1. `attach_assistant_message_by_request()`
2. 创建 assistant `chat_message_projection`
3. 标记 `projection_status='stored'`
4. `append_chat_stage("reply_stored")`
5. `finish_turn_by_request(status='completed')`

## 6.5 timeout / error recovery

timeout 和 error recovery 走同一规范：

1. assistant fallback message 同样挂 turn
2. 同样生成 assistant projection
3. turn status 分别标记为：
   - `recovered`
   - `timed_out`
   - `failed`

若已经有 fallback message，则最终应写成：

- `status='completed'`
- `metadata_json.recovered_from='timeout|error'`

## 6.6 memory candidate

在 assistant final message 写入后立即抽一份最小候选：

规则版最小实现：

1. 若 `final_content/plain_text` 为空，则不生成
2. `memory_kind='assistant_final_answer'`
3. scope 规则：
   - 有 `workspace_id` -> `scope_kind='workspace'`
   - 否则有 `session_id` -> `scope_kind='session'`
   - 否则 `global`
4. `key='turn:{request_id}:assistant_final_answer'`
5. `summary=assistant_plain_text[:500]`
6. `evidence_json` 附：
   - `request_id`
   - `assistant_message_id`
   - `tool_record_count`
   - `tool_call_ids`

这一步只证明主链已通，不在本轮做复杂抽取器。

## 7. `append_chat_stage()` 的新职责

当前职责：

- 写 `crate::events::append("chat", "v2_stage", ...)`

本轮新增职责：

1. 根据 `request_id` 找 turn
2. 追加一条 `chat_turn_events`
3. 基于 stage 更新 `chat_turns.status/stage`

建议 stage 到 status 的映射：

| stage | chat_turns.status | chat_turns.stage |
|---|---|---|
| `received` | `received` | `received` |
| `user_message_stored` | `input_stored` | `user_message_stored` |
| `context_loaded` | `context_loaded` | `context_loaded` |
| `llm_turn_start` | `llm_running` | `llm_turn_start` |
| `tool_start` | `tool_running` | `tool_start` |
| `tool_done` | `tool_running` | `tool_done` |
| `tool_catalog_injected` | `llm_running` | `tool_catalog_injected` |
| `reply_stored` | `reply_stored` | `reply_stored` |
| `failed_recovered` | `recovered` | `failed_recovered` |
| `timeout` | `timed_out` | `timeout` |
| `failed` | `failed` | `failed` |
| `done` | `completed` | `done` |

如果 `request_id` 尚未有 turn，`append_chat_stage()` 不应报错，只跳过新支路。

## 8. 本轮代码验收标准

这一轮认为“主链打通”的标准是：

1. 新请求进入后，`chat_turns` 有一行
2. `append_chat_stage()` 调用能在 `chat_turn_events` 看到多条事件
3. user / assistant message 在 `chat_messages.turn_id` 上能挂到 turn
4. `chat_message_projections` 至少能看到 user final 和 assistant final 两条
5. tool 调用若发生，`tool_calls.turn_id` 不为空
6. turn 完成后，`memory_candidates` 至少能看到一条规则版候选

这六项满足，就说明数据流已形成闭环。
