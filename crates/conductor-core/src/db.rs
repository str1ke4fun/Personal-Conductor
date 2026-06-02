use crate::paths::Paths;
use anyhow::Context;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Row, SqlitePool,
};
use std::time::Duration;
use tokio::fs;
#[cfg(not(test))]
use tokio::sync::OnceCell;

#[cfg(not(test))]
static DB_POOL: OnceCell<SqlitePool> = OnceCell::const_new();

pub async fn pool() -> anyhow::Result<SqlitePool> {
    #[cfg(not(test))]
    {
        let pool = DB_POOL.get_or_try_init(connect_and_migrate).await?;
        return Ok(pool.clone());
    }

    #[cfg(test)]
    {
        connect_and_migrate().await
    }
}

async fn connect_and_migrate() -> anyhow::Result<SqlitePool> {
    let path = Paths::conductor_sqlite();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(15));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .with_context(|| format!("connect sqlite {}", path.display()))?;
    migrate(&pool).await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tasks (
          id TEXT PRIMARY KEY,
          source TEXT NOT NULL,
          kind TEXT NOT NULL,
          artifact_file TEXT,
          artifact_anchor TEXT,
          summary_ref TEXT,
          est_minutes INTEGER,
          focus_hint TEXT,
          status TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          due_at TEXT,
          snoozed_until TEXT,
          priority TEXT DEFAULT 'normal',
          session_id TEXT,
          terminal_id TEXT,
          cwd TEXT,
          current_request TEXT,
          last_output_summary TEXT,
          last_event_at TEXT,
          permission_summary TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    ensure_column(pool, "tasks", "session_id", "session_id TEXT").await?;
    ensure_column(pool, "tasks", "terminal_id", "terminal_id TEXT").await?;
    ensure_column(pool, "tasks", "cwd", "cwd TEXT").await?;
    ensure_column(pool, "tasks", "current_request", "current_request TEXT").await?;
    ensure_column(
        pool,
        "tasks",
        "last_output_summary",
        "last_output_summary TEXT",
    )
    .await?;
    ensure_column(pool, "tasks", "last_event_at", "last_event_at TEXT").await?;
    ensure_column(
        pool,
        "tasks",
        "permission_summary",
        "permission_summary TEXT",
    )
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tasks_status_created ON tasks(status, created_at DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tasks_claude_session ON tasks(source, status, session_id);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tasks_claude_terminal_cwd ON tasks(source, status, terminal_id, cwd);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS task_lists (
          id TEXT PRIMARY KEY,
          scope TEXT NOT NULL,
          title TEXT NOT NULL,
          workspace_id TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          metadata_json TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agent_tasklist_items (
          task_list_id TEXT NOT NULL,
          id TEXT NOT NULL,
          subject TEXT NOT NULL,
          description TEXT NOT NULL DEFAULT '',
          active_form TEXT,
          owner TEXT,
          status TEXT NOT NULL,
          workspace_id TEXT,
          source TEXT NOT NULL DEFAULT 'manual',
          kind TEXT NOT NULL DEFAULT 'task',
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          metadata_json TEXT,
          PRIMARY KEY(task_list_id, id)
        );
        "#,
    )
    .execute(pool)
    .await?;

    migrate_legacy_agent_tasks_table(pool).await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS task_dependencies (
          task_list_id TEXT NOT NULL,
          from_task_id TEXT NOT NULL,
          to_task_id TEXT NOT NULL,
          PRIMARY KEY(task_list_id, from_task_id, to_task_id)
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_task_lists_scope ON task_lists(scope);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_task_lists_workspace ON task_lists(workspace_id);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_tasklist_items_status ON agent_tasklist_items(task_list_id, status, updated_at DESC);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_tasklist_items_owner ON agent_tasklist_items(task_list_id, owner, status);")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_agent_tasklist_items_workspace ON agent_tasklist_items(workspace_id);",
    )
    .execute(pool)
    .await?;

    // Migration: add legacy_id column for tracking migrated legacy tasks
    ensure_column(pool, "agent_tasklist_items", "legacy_id", "legacy_id TEXT").await?;

    // Migration: add est_minutes column for time budget filtering
    ensure_column(
        pool,
        "agent_tasklist_items",
        "est_minutes",
        "est_minutes INTEGER",
    )
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_task_dependencies_to ON task_dependencies(task_list_id, to_task_id);")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS notification_state (
          id INTEGER PRIMARY KEY CHECK (id = 1),
          quiet_until TEXT,
          last_notified_at TEXT,
          pending_minutes INTEGER DEFAULT 0,
          pending_count INTEGER DEFAULT 0
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO notification_state(id)
        VALUES (1)
        ON CONFLICT(id) DO NOTHING;
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS chat_messages (
          id TEXT PRIMARY KEY,
          role TEXT NOT NULL,
          content TEXT NOT NULL,
          created_at TEXT NOT NULL,
          seq INTEGER,
          tool_calls TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    // Chat session table for tracking workspace/run context.
    // session_id and run_id on chat_messages enable per-message attribution.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS chat_sessions (
          id TEXT PRIMARY KEY,
          workspace_id TEXT,
          run_id TEXT,
          created_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    // Migration: add session_id / run_id columns to chat_messages (for future use)
    let _ = sqlx::query("ALTER TABLE chat_messages ADD COLUMN session_id TEXT")
        .execute(pool)
        .await;
    let _ = sqlx::query("ALTER TABLE chat_messages ADD COLUMN run_id TEXT")
        .execute(pool)
        .await;
    ensure_column(pool, "chat_messages", "seq", "seq INTEGER").await?;

    // Migration: ensure chat_sessions has title, archived, updated_at columns
    ensure_column(pool, "chat_sessions", "title", "title TEXT").await?;
    ensure_column(
        pool,
        "chat_sessions",
        "archived",
        "archived INTEGER DEFAULT 0",
    )
    .await?;
    ensure_column(pool, "chat_sessions", "updated_at", "updated_at TEXT").await?;
    // Migration: session_kind distinguishes chat sessions from goal sessions.
    ensure_column(
        pool,
        "chat_sessions",
        "session_kind",
        "session_kind TEXT DEFAULT 'chat'",
    )
    .await?;
    // Migration: link a chat session to its associated goal (when kind = 'goal').
    ensure_column(pool, "chat_sessions", "goal_id", "goal_id TEXT").await?;

    sqlx::query(
        r#"
        UPDATE chat_messages
        SET seq = rowid
        WHERE seq IS NULL
        "#,
    )
    .execute(pool)
    .await?;

    // Migration: add tool_calls column to existing tables
    let _ = sqlx::query("ALTER TABLE chat_messages ADD COLUMN tool_calls TEXT")
        .execute(pool)
        .await;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_chat_messages_created ON chat_messages(created_at ASC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_chat_messages_seq ON chat_messages(seq ASC, created_at ASC, id ASC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS affection_state (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          value INTEGER NOT NULL,
          last_interaction_at TEXT,
          last_decrease_at TEXT,
          updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS memory_entries (
          id TEXT PRIMARY KEY,
          key TEXT NOT NULL,
          value TEXT NOT NULL,
          category TEXT NOT NULL,
          scope TEXT NOT NULL DEFAULT 'global',
          workspace_id TEXT,
          source TEXT NOT NULL DEFAULT 'user_confirmed',
          confidence REAL NOT NULL DEFAULT 1.0,
          sensitivity TEXT NOT NULL DEFAULT 'normal',
          expires_at TEXT,
          last_used_at TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    ensure_column(
        pool,
        "memory_entries",
        "scope",
        "scope TEXT NOT NULL DEFAULT 'global'",
    )
    .await?;
    ensure_column(pool, "memory_entries", "workspace_id", "workspace_id TEXT").await?;
    ensure_column(
        pool,
        "memory_entries",
        "source",
        "source TEXT NOT NULL DEFAULT 'user_confirmed'",
    )
    .await?;
    ensure_column(
        pool,
        "memory_entries",
        "confidence",
        "confidence REAL NOT NULL DEFAULT 1.0",
    )
    .await?;
    ensure_column(
        pool,
        "memory_entries",
        "sensitivity",
        "sensitivity TEXT NOT NULL DEFAULT 'normal'",
    )
    .await?;
    ensure_column(pool, "memory_entries", "expires_at", "expires_at TEXT").await?;
    ensure_column(pool, "memory_entries", "last_used_at", "last_used_at TEXT").await?;
    ensure_column(
        pool,
        "memory_entries",
        "status",
        "status TEXT NOT NULL DEFAULT 'active'",
    )
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_key ON memory_entries(key);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_category ON memory_entries(category);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_scope ON memory_entries(scope);")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_memory_workspace_id ON memory_entries(workspace_id);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS conversation_summaries (
          id TEXT PRIMARY KEY,
          summary TEXT NOT NULL,
          keywords TEXT NOT NULL,
          timestamp TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_conversation_timestamp ON conversation_summaries(timestamp);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS memory_chunks (
          id TEXT PRIMARY KEY,
          memory_id TEXT NOT NULL,
          workspace_id TEXT,
          scope TEXT NOT NULL,
          category TEXT NOT NULL,
          content TEXT NOT NULL,
          summary TEXT,
          source TEXT NOT NULL,
          sensitivity TEXT NOT NULL,
          confidence REAL NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          expires_at TEXT,
          last_used_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunk_memory_id ON memory_chunks(memory_id);")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_chunk_workspace_id ON memory_chunks(workspace_id);",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunk_scope ON memory_chunks(scope);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunk_category ON memory_chunks(category);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunk_updated_at ON memory_chunks(updated_at);")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS memory_embeddings (
          chunk_id TEXT PRIMARY KEY,
          model TEXT NOT NULL,
          dims INTEGER NOT NULL,
          vector BLOB NOT NULL,
          created_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_embedding_model ON memory_embeddings(model);")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS workspaces (
          id TEXT PRIMARY KEY,
          root TEXT NOT NULL,
          name TEXT NOT NULL,
          kind TEXT NOT NULL,
          trust_level TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          last_active_at TEXT,
          metadata_json TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS avatar_state (
          id TEXT PRIMARY KEY,
          theme TEXT NOT NULL,
          character TEXT NOT NULL,
          color_scheme TEXT NOT NULL,
          size INTEGER NOT NULL,
          position_x INTEGER NOT NULL,
          position_y INTEGER NOT NULL,
          animation_enabled BOOLEAN NOT NULL DEFAULT 1,
          auto_hide BOOLEAN NOT NULL DEFAULT 0,
          last_active_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_avatar_updated_at ON avatar_state(updated_at);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_workspaces_root ON workspaces(root);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_workspaces_kind ON workspaces(kind);")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_workspaces_last_active ON workspaces(last_active_at DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS action_proposals (
          id TEXT PRIMARY KEY,
          workspace_id TEXT,
          for_cwd TEXT NOT NULL,
          source TEXT NOT NULL,
          title TEXT NOT NULL,
          content TEXT NOT NULL,
          reason TEXT NOT NULL,
          tool_id TEXT,
          tool_input_json TEXT,
          risk_level TEXT NOT NULL,
          dry_run INTEGER NOT NULL DEFAULT 1,
          status TEXT NOT NULL,
          result_ref TEXT,
          agent_task_id TEXT,
          grant_id TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    ensure_column(
        pool,
        "action_proposals",
        "workspace_id",
        "workspace_id TEXT",
    )
    .await?;
    ensure_column(
        pool,
        "action_proposals",
        "source",
        "source TEXT NOT NULL DEFAULT 'chat'",
    )
    .await?;
    ensure_column(
        pool,
        "action_proposals",
        "title",
        "title TEXT NOT NULL DEFAULT ''",
    )
    .await?;
    ensure_column(pool, "action_proposals", "tool_id", "tool_id TEXT").await?;
    ensure_column(
        pool,
        "action_proposals",
        "tool_input_json",
        "tool_input_json TEXT",
    )
    .await?;
    ensure_column(
        pool,
        "action_proposals",
        "risk_level",
        "risk_level TEXT NOT NULL DEFAULT 'read_only'",
    )
    .await?;
    ensure_column(
        pool,
        "action_proposals",
        "dry_run",
        "dry_run INTEGER NOT NULL DEFAULT 1",
    )
    .await?;
    ensure_column(pool, "action_proposals", "result_ref", "result_ref TEXT").await?;
    ensure_column(pool, "action_proposals", "updated_at", "updated_at TEXT").await?;

    // Migration: add agent_task_id column to bind proposals to agent tasks
    ensure_column(
        pool,
        "action_proposals",
        "agent_task_id",
        "agent_task_id TEXT",
    )
    .await?;

    sqlx::query(
        r#"
        UPDATE action_proposals
        SET updated_at = created_at
        WHERE updated_at IS NULL OR updated_at = ''
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_proposals_status ON action_proposals(status);")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_proposals_workspace ON action_proposals(workspace_id);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_proposals_created ON action_proposals(created_at DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tool_runs (
          id TEXT PRIMARY KEY,
          proposal_id TEXT,
          workspace_id TEXT,
          tool_id TEXT NOT NULL,
          status TEXT NOT NULL,
          started_at TEXT NOT NULL,
          finished_at TEXT,
          input_ref TEXT,
          output_ref TEXT,
          error TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_runs_proposal ON tool_runs(proposal_id);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_runs_status ON tool_runs(status);")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agent_runs (
          id TEXT PRIMARY KEY,
          agent_id TEXT NOT NULL,
          role TEXT NOT NULL,
          workspace_id TEXT,
          cwd TEXT,
          status TEXT NOT NULL,
          pid INTEGER,
          command_json TEXT,
          input_ref TEXT,
          output_ref TEXT,
          error TEXT,
          started_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          finished_at TEXT,
          metadata_json TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_agent_runs_status ON agent_runs(status, updated_at DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_runs_workspace ON agent_runs(workspace_id, updated_at DESC);")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_agent_runs_agent ON agent_runs(agent_id, updated_at DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agent_teams (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          workspace_id TEXT,
          status TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          metadata_json TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agent_team_members (
          team_id TEXT NOT NULL,
          agent_id TEXT NOT NULL,
          role TEXT NOT NULL,
          run_id TEXT,
          cwd TEXT,
          status TEXT NOT NULL,
          subscriptions_json TEXT NOT NULL DEFAULT '[]',
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          metadata_json TEXT,
          PRIMARY KEY(team_id, agent_id)
        );
        "#,
    )
    .execute(pool)
    .await?;

    // Migration: add lifecycle and write_scope_json columns to agent_teams
    ensure_column(
        pool,
        "agent_teams",
        "lifecycle",
        "lifecycle TEXT NOT NULL DEFAULT 'draft'",
    )
    .await?;
    ensure_column(
        pool,
        "agent_teams",
        "write_scope_json",
        "write_scope_json TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agent_mailbox_messages (
          id TEXT PRIMARY KEY,
          team_id TEXT NOT NULL,
          sender_agent_id TEXT NOT NULL,
          recipient_agent_id TEXT,
          kind TEXT NOT NULL,
          content TEXT NOT NULL,
          read_at TEXT,
          created_at TEXT NOT NULL,
          metadata_json TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_teams_workspace ON agent_teams(workspace_id, updated_at DESC);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_team_members_agent ON agent_team_members(agent_id, status);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_mailbox_recipient ON agent_mailbox_messages(team_id, recipient_agent_id, read_at, created_at DESC);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_mailbox_team ON agent_mailbox_messages(team_id, created_at DESC);")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS todos (
          id TEXT PRIMARY KEY,
          chatsession_id TEXT NOT NULL,
          content TEXT NOT NULL,
          status TEXT NOT NULL DEFAULT 'pending',
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_todos_session ON todos(chatsession_id);")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tool_calls (
          id TEXT PRIMARY KEY,
          session_id TEXT,
          workspace_id TEXT,
          llm_tool_call_id TEXT,
          tool_id TEXT NOT NULL,
          input_json TEXT NOT NULL,
          output_json TEXT,
          status TEXT NOT NULL DEFAULT 'pending',
          error TEXT,
          started_at TEXT NOT NULL,
          completed_at TEXT,
          duration_ms INTEGER,
          agent_run_id TEXT,
          risk_level TEXT,
          proposal_id TEXT,
          permission_grant_id TEXT,
          command_run_id TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    ensure_column(pool, "tool_calls", "workspace_id", "workspace_id TEXT").await?;
    ensure_column(
        pool,
        "tool_calls",
        "llm_tool_call_id",
        "llm_tool_call_id TEXT",
    )
    .await?;
    ensure_column(pool, "tool_calls", "risk_level", "risk_level TEXT").await?;
    ensure_column(pool, "tool_calls", "proposal_id", "proposal_id TEXT").await?;
    ensure_column(
        pool,
        "tool_calls",
        "permission_grant_id",
        "permission_grant_id TEXT",
    )
    .await?;
    ensure_column(pool, "tool_calls", "command_run_id", "command_run_id TEXT").await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_calls_session ON tool_calls(session_id);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_calls_workspace ON tool_calls(workspace_id);")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tool_calls_llm_id ON tool_calls(llm_tool_call_id);",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_calls_tool ON tool_calls(tool_id);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_calls_status ON tool_calls(status);")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tool_calls_command_run ON tool_calls(command_run_id);",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tool_calls_proposal ON tool_calls(proposal_id);")
        .execute(pool)
        .await?;

    // ── permission_grants (TASK-012) ──

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS permission_grants (
          id TEXT PRIMARY KEY,
          workspace_id TEXT,
          tool_id TEXT NOT NULL,
          risk_level TEXT NOT NULL,
          grantee TEXT NOT NULL,
          status TEXT NOT NULL,
          scope_json TEXT,
          expires_at TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_permission_grants_grantee ON permission_grants(grantee);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_permission_grants_workspace ON permission_grants(workspace_id);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_permission_grants_tool ON permission_grants(tool_id, status);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_permission_grants_status ON permission_grants(status);",
    )
    .execute(pool)
    .await?;

    // Migration: add grant_id column to action_proposals
    ensure_column(pool, "action_proposals", "grant_id", "grant_id TEXT").await?;

    // ── command_runs (TASK-020) ──

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS command_runs (
          id TEXT PRIMARY KEY,
          session_id TEXT,
          tool_call_id TEXT,
          agent_run_id TEXT,
          permission_grant_id TEXT,
          risk_level TEXT,
          env_delta_json TEXT,
          command TEXT NOT NULL,
          cwd TEXT NOT NULL,
          status TEXT NOT NULL,
          exit_code INTEGER,
          stdout_tail TEXT NOT NULL DEFAULT '',
          stderr_tail TEXT NOT NULL DEFAULT '',
          pid INTEGER,
          started_at TEXT,
          completed_at TEXT,
          created_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    ensure_column(pool, "command_runs", "tool_call_id", "tool_call_id TEXT").await?;
    ensure_column(pool, "command_runs", "agent_run_id", "agent_run_id TEXT").await?;
    ensure_column(
        pool,
        "command_runs",
        "permission_grant_id",
        "permission_grant_id TEXT",
    )
    .await?;
    ensure_column(pool, "command_runs", "risk_level", "risk_level TEXT").await?;
    ensure_column(
        pool,
        "command_runs",
        "env_delta_json",
        "env_delta_json TEXT",
    )
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_command_runs_session ON command_runs(session_id);")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_command_runs_tool_call ON command_runs(tool_call_id);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_command_runs_agent_run ON command_runs(agent_run_id);",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_command_runs_status ON command_runs(status);")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_command_runs_created ON command_runs(created_at DESC);",
    )
    .execute(pool)
    .await?;

    // ── codex_sessions (TASK-021) ──

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS codex_sessions (
          id TEXT PRIMARY KEY,
          command TEXT NOT NULL,
          cwd TEXT NOT NULL,
          status TEXT NOT NULL,
          pid INTEGER,
          exit_code INTEGER,
          created_at TEXT NOT NULL,
          started_at TEXT,
          completed_at TEXT,
          session_data TEXT NOT NULL DEFAULT '{}'
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_codex_sessions_status ON codex_sessions(status);")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_codex_sessions_created ON codex_sessions(created_at DESC);",
    )
    .execute(pool)
    .await?;

    // ── skill_packages (TASK-065) ──

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS skill_packages (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          version TEXT NOT NULL,
          description TEXT NOT NULL,
          author TEXT,
          activation_json TEXT NOT NULL,
          capabilities_json TEXT NOT NULL,
          source TEXT NOT NULL,
          enabled BOOLEAN NOT NULL DEFAULT 0,
          body TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS skill_capabilities (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          skill_id TEXT NOT NULL,
          capability TEXT NOT NULL,
          UNIQUE(skill_id, capability),
          FOREIGN KEY(skill_id) REFERENCES skill_packages(id) ON DELETE CASCADE
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_skill_capabilities_skill ON skill_capabilities(skill_id);",
    )
    .execute(pool)
    .await?;

    // ── connectors (TASK-067) ──

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS connectors (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          description TEXT NOT NULL DEFAULT '',
          implementation_type TEXT NOT NULL,
          auth_status TEXT NOT NULL DEFAULT 'not_configured',
          enabled BOOLEAN NOT NULL DEFAULT 1,
          config_json TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS capability_grants (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          connector_id TEXT NOT NULL,
          capability TEXT NOT NULL,
          tools_json TEXT NOT NULL,
          risk_level TEXT NOT NULL DEFAULT 'low',
          requires_confirmation BOOLEAN NOT NULL DEFAULT 0,
          UNIQUE(connector_id, capability),
          FOREIGN KEY(connector_id) REFERENCES connectors(id) ON DELETE CASCADE
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_capability_grants_capability ON capability_grants(capability);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_capability_grants_connector ON capability_grants(connector_id);",
    )
    .execute(pool)
    .await?;

    // ── Runtime tables (TASK-077) ──────────────────────────────────────

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS workspace_runtimes (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            root_path TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'starting',
            api_bind TEXT NOT NULL DEFAULT '127.0.0.1',
            api_port INTEGER NOT NULL,
            auth_token_hash TEXT NOT NULL,
            started_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            stopped_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS goal_runs (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            title TEXT NOT NULL,
            objective TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'draft',
            priority TEXT NOT NULL DEFAULT 'p2',
            owner TEXT NOT NULL DEFAULT 'user',
            budget_json TEXT,
            policy_json TEXT,
            current_cycle_id TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            finished_at TEXT,
            metadata_json TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS goal_cycles (
            id TEXT PRIMARY KEY,
            goal_id TEXT NOT NULL,
            cycle_no INTEGER NOT NULL DEFAULT 1,
            status TEXT NOT NULL DEFAULT 'observing',
            observe_snapshot_ref TEXT,
            orientation_json TEXT,
            dispatch_plan_id TEXT,
            review_summary_ref TEXT,
            started_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            finished_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS dispatch_plans (
            id TEXT PRIMARY KEY,
            goal_id TEXT NOT NULL,
            cycle_id TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'draft',
            summary TEXT NOT NULL DEFAULT '',
            tasks_json TEXT NOT NULL DEFAULT '[]',
            risk_json TEXT,
            approval_message_id TEXT,
            created_at TEXT NOT NULL,
            approved_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agent_tasks (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            goal_id TEXT,
            cycle_id TEXT,
            parent_task_id TEXT,
            title TEXT NOT NULL,
            instruction TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'proposed',
            agent_kind TEXT NOT NULL DEFAULT 'claude_p',
            assigned_agent_id TEXT,
            claimed_by TEXT,
            write_scope_json TEXT NOT NULL DEFAULT '[]',
            read_scope_json TEXT NOT NULL DEFAULT '[]',
            allowed_tools_json TEXT NOT NULL DEFAULT '[]',
            dependencies_json TEXT NOT NULL DEFAULT '[]',
            acceptance_json TEXT NOT NULL DEFAULT '[]',
            result_ref TEXT,
            error TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            claimed_at TEXT,
            finished_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agent_run_refs (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            ref_id TEXT NOT NULL,
            status_cache TEXT NOT NULL DEFAULT 'pending',
            status_mirror TEXT,
            status_mirror_at TEXT,
            started_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            finished_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agent_messages (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            goal_id TEXT,
            cycle_id TEXT,
            task_id TEXT,
            sender_id TEXT NOT NULL,
            recipient_id TEXT,
            topic TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'message',
            content TEXT NOT NULL DEFAULT '',
            payload_json TEXT,
            created_at TEXT NOT NULL,
            read_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS work_leases (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            holder_id TEXT NOT NULL,
            task_id TEXT,
            lease_type TEXT NOT NULL,
            scope_json TEXT NOT NULL DEFAULT '[]',
            status TEXT NOT NULL DEFAULT 'active',
            ttl_seconds INTEGER NOT NULL DEFAULT 3600,
            acquired_at TEXT NOT NULL,
            renewed_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            released_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agent_heartbeats (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            process_id INTEGER,
            task_id TEXT,
            goal_id TEXT,
            status TEXT NOT NULL DEFAULT 'idle',
            stage_label TEXT,
            progress_text TEXT,
            active_tool_count INTEGER NOT NULL DEFAULT 0,
            last_event_id TEXT,
            created_at TEXT NOT NULL,
            expires_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS runtime_events (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            source TEXT NOT NULL,
            actor_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            subject_type TEXT NOT NULL,
            subject_id TEXT NOT NULL,
            parent_event_id TEXT,
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS workspace_projection_state (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            last_event_id TEXT,
            last_generated_at TEXT,
            content_hash TEXT,
            drift_detected BOOLEAN NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    // ── Runtime indexes ────────────────────────────────────────────────

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_goal_runs_workspace_status ON goal_runs(workspace_id, status, updated_at DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_goal_cycles_goal_no ON goal_cycles(goal_id, cycle_no DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_agent_tasks_workspace_status ON agent_tasks(workspace_id, status, updated_at DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_agent_tasks_goal_cycle ON agent_tasks(goal_id, cycle_id, status);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_agent_messages_workspace_created ON agent_messages(workspace_id, created_at DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_agent_messages_task ON agent_messages(task_id, created_at ASC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_agent_messages_recipient_unread ON agent_messages(workspace_id, recipient_id, read_at);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_work_leases_active ON work_leases(workspace_id, status, expires_at);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_agent_heartbeats_workspace ON agent_heartbeats(workspace_id, agent_id, expires_at);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_runtime_events_workspace_created ON runtime_events(workspace_id, created_at DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_runtime_events_subject ON runtime_events(subject_type, subject_id, created_at ASC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_dispatch_plans_goal_cycle_status ON dispatch_plans(goal_id, cycle_id, status);",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_run_refs_task ON agent_run_refs(task_id);")
        .execute(pool)
        .await?;

    // ── llm_profiles (TASK-104) ──

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS llm_profiles (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          provider TEXT NOT NULL,
          model_id TEXT NOT NULL,
          api_base_url TEXT NOT NULL,
          api_key_encrypted TEXT,
          max_tokens INTEGER NOT NULL DEFAULT 4096,
          temperature REAL NOT NULL DEFAULT 0.7,
          enabled BOOLEAN NOT NULL DEFAULT 1,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_llm_profiles_provider ON llm_profiles(provider);")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_llm_profiles_enabled ON llm_profiles(enabled);")
        .execute(pool)
        .await?;

    // ── agent_backends (TASK-105) ──

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS agent_backends (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            executable_path TEXT,
            default_env_json TEXT,
            health_check_url TEXT,
            enabled BOOLEAN NOT NULL DEFAULT 1,
            last_health_check_at TEXT,
            health_status TEXT NOT NULL DEFAULT 'unknown',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_backends_kind ON agent_backends(kind);")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_agent_backends_enabled ON agent_backends(enabled);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_agent_backends_health_status ON agent_backends(health_status);",
    )
    .execute(pool)
    .await?;

    // -- routing_policies + route_decisions (TASK-106) --

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS routing_policies (
            id TEXT PRIMARY KEY,
            task_kind TEXT NOT NULL,
            backend_kind TEXT NOT NULL,
            profile_id TEXT,
            priority INTEGER NOT NULL DEFAULT 0,
            enabled BOOLEAN NOT NULL DEFAULT 1,
            reason_template TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS route_decisions (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            task_id TEXT,
            task_kind TEXT NOT NULL,
            policy_id TEXT,
            backend_id TEXT,
            backend_kind TEXT NOT NULL,
            profile_id TEXT,
            reason TEXT NOT NULL,
            fallback_used BOOLEAN NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_routing_policies_kind_enabled_priority ON routing_policies(task_kind, enabled, priority DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_route_decisions_task ON route_decisions(task_id, created_at DESC);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_route_decisions_workspace_created ON route_decisions(workspace_id, created_at DESC);",
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn ensure_column(
    pool: &SqlitePool,
    table_name: &str,
    column_name: &str,
    column_definition: &str,
) -> anyhow::Result<()> {
    let exists = table_has_column(pool, table_name, column_name).await?;

    if !exists {
        sqlx::query(&format!(
            "ALTER TABLE {table_name} ADD COLUMN {column_definition};"
        ))
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn table_exists(pool: &SqlitePool, table_name: &str) -> anyhow::Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name = ?",
    )
    .bind(table_name)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

async fn table_has_column(
    pool: &SqlitePool,
    table_name: &str,
    column_name: &str,
) -> anyhow::Result<bool> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table_name});"))
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .any(|name| name == column_name))
}

async fn migrate_legacy_agent_tasks_table(pool: &SqlitePool) -> anyhow::Result<()> {
    if !table_exists(pool, "agent_tasks").await? {
        return Ok(());
    }

    // The older task-list table was also named `agent_tasks`. If it does not
    // have the runtime `goal_id` column, move its rows to the renamed table so
    // the canonical runtime `agent_tasks` table can be created later.
    if table_has_column(pool, "agent_tasks", "goal_id").await? {
        return Ok(());
    }

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO agent_tasklist_items (
            task_list_id, id, subject, description, active_form, owner, status,
            workspace_id, source, kind, created_at, updated_at, metadata_json
        )
        SELECT task_list_id, id, subject, description, active_form, owner, status,
               workspace_id, source, kind, created_at, updated_at, metadata_json
        FROM agent_tasks
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("DROP TABLE agent_tasks").execute(pool).await?;

    for old_idx in [
        "idx_agent_tasks_status",
        "idx_agent_tasks_owner",
        "idx_agent_tasks_workspace",
    ] {
        let _ = sqlx::query(&format!("DROP INDEX IF EXISTS {old_idx}"))
            .execute(pool)
            .await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestRoot;

    #[tokio::test]
    async fn initializes_sqlite_schema() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification_state WHERE id = 1")
            .fetch_one(&pool)
            .await
            .expect("query notification_state");
        assert_eq!(count, 1);
    }

    // ── 1. chat_messages CRUD ──

    #[tokio::test]
    async fn chat_messages_insert_and_select() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-01-01T00:00:00Z";
        sqlx::query(
            "INSERT INTO chat_messages (id, role, content, created_at, seq) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("msg-1")
        .bind("user")
        .bind("hello world")
        .bind(now)
        .bind(1i64)
        .execute(&pool)
        .await
        .expect("insert");

        let (role, content): (String, String) =
            sqlx::query_as("SELECT role, content FROM chat_messages WHERE id = ?")
                .bind("msg-1")
                .fetch_one(&pool)
                .await
                .expect("select");
        assert_eq!(role, "user");
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn chat_messages_update_content() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        sqlx::query(
            "INSERT INTO chat_messages (id, role, content, created_at, seq) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("msg-2")
        .bind("assistant")
        .bind("original")
        .bind("2026-01-01T00:00:00Z")
        .bind(1i64)
        .execute(&pool)
        .await
        .expect("insert");

        sqlx::query("UPDATE chat_messages SET content = ? WHERE id = ?")
            .bind("updated content")
            .bind("msg-2")
            .execute(&pool)
            .await
            .expect("update");

        let content: String = sqlx::query_scalar("SELECT content FROM chat_messages WHERE id = ?")
            .bind("msg-2")
            .fetch_one(&pool)
            .await
            .expect("select");
        assert_eq!(content, "updated content");
    }

    #[tokio::test]
    async fn chat_messages_delete() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        sqlx::query(
            "INSERT INTO chat_messages (id, role, content, created_at, seq) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("msg-3")
        .bind("user")
        .bind("to be deleted")
        .bind("2026-01-01T00:00:00Z")
        .bind(1i64)
        .execute(&pool)
        .await
        .expect("insert");

        sqlx::query("DELETE FROM chat_messages WHERE id = ?")
            .bind("msg-3")
            .execute(&pool)
            .await
            .expect("delete");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages WHERE id = ?")
            .bind("msg-3")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(count, 0);
    }

    // ── 2. task persistence ──

    #[tokio::test]
    async fn task_insert_and_query_by_status() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-01-15T10:00:00Z";
        sqlx::query(
            r#"INSERT INTO tasks (id, source, kind, status, created_at, updated_at, priority)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("task-1")
        .bind("test")
        .bind("todo")
        .bind("pending")
        .bind(now)
        .bind(now)
        .bind("high")
        .execute(&pool)
        .await
        .expect("insert");

        let status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE id = ?")
            .bind("task-1")
            .fetch_one(&pool)
            .await
            .expect("select");
        assert_eq!(status, "pending");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE status = ?")
            .bind("pending")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn task_update_status_and_timestamps() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let t0 = "2026-01-15T10:00:00Z";
        let t1 = "2026-01-15T11:00:00Z";
        sqlx::query(
            r#"INSERT INTO tasks (id, source, kind, status, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind("task-2")
        .bind("test")
        .bind("bug")
        .bind("pending")
        .bind(t0)
        .bind(t0)
        .execute(&pool)
        .await
        .expect("insert");

        sqlx::query("UPDATE tasks SET status = ?, updated_at = ? WHERE id = ?")
            .bind("done")
            .bind(t1)
            .bind("task-2")
            .execute(&pool)
            .await
            .expect("update");

        let (status, updated_at): (String, String) =
            sqlx::query_as("SELECT status, updated_at FROM tasks WHERE id = ?")
                .bind("task-2")
                .fetch_one(&pool)
                .await
                .expect("select");
        assert_eq!(status, "done");
        assert_eq!(updated_at, t1);
    }

    // ── 3. chat_sessions ──

    #[tokio::test]
    async fn chat_sessions_create_query_update() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-02-01T00:00:00Z";
        sqlx::query(
            r#"INSERT INTO chat_sessions (id, workspace_id, run_id, created_at, title, archived)
               VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind("sess-1")
        .bind("ws-1")
        .bind("run-1")
        .bind(now)
        .bind("My Session")
        .bind(0i64)
        .execute(&pool)
        .await
        .expect("insert");

        let (title, archived): (Option<String>, i64) =
            sqlx::query_as("SELECT title, archived FROM chat_sessions WHERE id = ?")
                .bind("sess-1")
                .fetch_one(&pool)
                .await
                .expect("select");
        assert_eq!(title.as_deref(), Some("My Session"));
        assert_eq!(archived, 0);

        // Update title and archive
        sqlx::query("UPDATE chat_sessions SET title = ?, archived = 1 WHERE id = ?")
            .bind("Archived Session")
            .bind("sess-1")
            .execute(&pool)
            .await
            .expect("update");

        let (title, archived): (Option<String>, i64) =
            sqlx::query_as("SELECT title, archived FROM chat_sessions WHERE id = ?")
                .bind("sess-1")
                .fetch_one(&pool)
                .await
                .expect("select after update");
        assert_eq!(title.as_deref(), Some("Archived Session"));
        assert_eq!(archived, 1);
    }

    // ── 4. migration idempotency ──

    #[tokio::test]
    async fn migration_idempotent() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool first");

        // Insert a row to verify data survives second migrate
        sqlx::query(
            "INSERT INTO chat_messages (id, role, content, created_at, seq) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("idem-msg")
        .bind("user")
        .bind("persist me")
        .bind("2026-01-01T00:00:00Z")
        .bind(1i64)
        .execute(&pool)
        .await
        .expect("insert before second migrate");

        // Call migrate again -- should not error
        migrate(&pool).await.expect("second migrate");

        // Data should still be there
        let content: String = sqlx::query_scalar("SELECT content FROM chat_messages WHERE id = ?")
            .bind("idem-msg")
            .fetch_one(&pool)
            .await
            .expect("select after second migrate");
        assert_eq!(content, "persist me");

        // All expected tables should exist
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx%' ORDER BY name"
        )
        .fetch_all(&pool)
        .await
        .expect("list tables");
        for expected in &[
            "tasks",
            "task_lists",
            "agent_tasklist_items",
            "task_dependencies",
            "notification_state",
            "chat_messages",
            "chat_sessions",
            "affection_state",
            "memory_entries",
            "conversation_summaries",
            "memory_chunks",
            "memory_embeddings",
            "workspaces",
            "avatar_state",
            "action_proposals",
            "tool_runs",
            "agent_runs",
            "agent_teams",
            "agent_team_members",
            "agent_mailbox_messages",
            "todos",
            "tool_calls",
            "permission_grants",
            "command_runs",
            "codex_sessions",
            "skill_packages",
            "skill_capabilities",
            "connectors",
            "capability_grants",
            "llm_profiles",
            "agent_backends",
            "routing_policies",
            "route_decisions",
        ] {
            assert!(
                tables.iter().any(|t| t == expected),
                "missing table: {expected}"
            );
        }
    }

    // ── 5. memory_entries ──

    #[tokio::test]
    async fn memory_entries_insert_query_by_key_and_category() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-03-01T00:00:00Z";
        sqlx::query(
            r#"INSERT INTO memory_entries
              (id, key, value, category, scope, source, confidence, sensitivity, created_at, updated_at)
              VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("mem-1")
        .bind("user_name")
        .bind("Alice")
        .bind("personal")
        .bind("global")
        .bind("user_confirmed")
        .bind(1.0f64)
        .bind("normal")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert");

        // Query by key
        let value: String = sqlx::query_scalar("SELECT value FROM memory_entries WHERE key = ?")
            .bind("user_name")
            .fetch_one(&pool)
            .await
            .expect("query by key");
        assert_eq!(value, "Alice");

        // Query by category
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM memory_entries WHERE category = ?")
                .bind("personal")
                .fetch_one(&pool)
                .await
                .expect("query by category");
        assert_eq!(count, 1);

        // Update value
        sqlx::query("UPDATE memory_entries SET value = ?, updated_at = ? WHERE id = ?")
            .bind("Bob")
            .bind("2026-03-02T00:00:00Z")
            .bind("mem-1")
            .execute(&pool)
            .await
            .expect("update");

        let value: String = sqlx::query_scalar("SELECT value FROM memory_entries WHERE id = ?")
            .bind("mem-1")
            .fetch_one(&pool)
            .await
            .expect("select after update");
        assert_eq!(value, "Bob");
    }

    // ── 6. workspaces ──

    #[tokio::test]
    async fn workspaces_insert_get_by_root_list_all() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-04-01T00:00:00Z";
        sqlx::query(
            r#"INSERT INTO workspaces (id, root, name, kind, trust_level, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("ws-1")
        .bind("/home/user/project")
        .bind("My Project")
        .bind("git")
        .bind("trusted")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert");

        // Get by root
        let name: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE root = ?")
            .bind("/home/user/project")
            .fetch_one(&pool)
            .await
            .expect("query by root");
        assert_eq!(name, "My Project");

        // Insert second workspace
        sqlx::query(
            r#"INSERT INTO workspaces (id, root, name, kind, trust_level, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("ws-2")
        .bind("/tmp/scratch")
        .bind("Scratch")
        .bind("temp")
        .bind("untrusted")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert second");

        // List all
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces")
            .fetch_one(&pool)
            .await
            .expect("count all");
        assert_eq!(count, 2);
    }

    // ── 7. action_proposals ──

    #[tokio::test]
    async fn action_proposals_insert_query_by_status_update() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-05-01T00:00:00Z";
        sqlx::query(
            r#"INSERT INTO action_proposals
              (id, for_cwd, source, title, content, reason, risk_level, dry_run, status, created_at, updated_at)
              VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("prop-1")
        .bind("/workspace")
        .bind("chat")
        .bind("Run tests")
        .bind("cargo test")
        .bind("verify build")
        .bind("read_only")
        .bind(1i64)
        .bind("pending")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert");

        let status: String = sqlx::query_scalar("SELECT status FROM action_proposals WHERE id = ?")
            .bind("prop-1")
            .fetch_one(&pool)
            .await
            .expect("select");
        assert_eq!(status, "pending");

        // Query by status
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM action_proposals WHERE status = ?")
                .bind("pending")
                .fetch_one(&pool)
                .await
                .expect("count");
        assert_eq!(count, 1);

        // Update status
        sqlx::query("UPDATE action_proposals SET status = ?, updated_at = ? WHERE id = ?")
            .bind("approved")
            .bind("2026-05-01T01:00:00Z")
            .bind("prop-1")
            .execute(&pool)
            .await
            .expect("update");

        let status: String = sqlx::query_scalar("SELECT status FROM action_proposals WHERE id = ?")
            .bind("prop-1")
            .fetch_one(&pool)
            .await
            .expect("select after update");
        assert_eq!(status, "approved");
    }

    // ── 8. agent_teams + members ──

    #[tokio::test]
    async fn agent_teams_create_add_member_query() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-06-01T00:00:00Z";

        // Create team
        sqlx::query(
            r#"INSERT INTO agent_teams (id, name, workspace_id, status, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind("team-1")
        .bind("Dev Team")
        .bind("ws-1")
        .bind("active")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert team");

        // Add member
        sqlx::query(
            r#"INSERT INTO agent_team_members
              (team_id, agent_id, role, status, subscriptions_json, created_at, updated_at)
              VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("team-1")
        .bind("agent-alpha")
        .bind("lead")
        .bind("idle")
        .bind("[]")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert member");

        // Add second member
        sqlx::query(
            r#"INSERT INTO agent_team_members
              (team_id, agent_id, role, status, subscriptions_json, created_at, updated_at)
              VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("team-1")
        .bind("agent-beta")
        .bind("worker")
        .bind("idle")
        .bind(r#"["task_complete"]"#)
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert member 2");

        // Query team
        let name: String = sqlx::query_scalar("SELECT name FROM agent_teams WHERE id = ?")
            .bind("team-1")
            .fetch_one(&pool)
            .await
            .expect("select team");
        assert_eq!(name, "Dev Team");

        // Query members
        let member_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM agent_team_members WHERE team_id = ?")
                .bind("team-1")
                .fetch_one(&pool)
                .await
                .expect("count members");
        assert_eq!(member_count, 2);

        // Query by agent role
        let role: String = sqlx::query_scalar(
            "SELECT role FROM agent_team_members WHERE team_id = ? AND agent_id = ?",
        )
        .bind("team-1")
        .bind("agent-alpha")
        .fetch_one(&pool)
        .await
        .expect("select role");
        assert_eq!(role, "lead");
    }

    // ── 9. notification_state ──

    #[tokio::test]
    async fn notification_state_verify_initial_and_update() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");

        // Verify defaults
        let (quiet_until, pending_count): (Option<String>, i64) = sqlx::query_as(
            "SELECT quiet_until, pending_count FROM notification_state WHERE id = 1",
        )
        .fetch_one(&pool)
        .await
        .expect("select");
        assert!(quiet_until.is_none());
        assert_eq!(pending_count, 0);

        // Update fields
        sqlx::query(
            "UPDATE notification_state SET quiet_until = ?, pending_minutes = ?, pending_count = ? WHERE id = 1",
        )
        .bind("2026-12-31T23:59:59Z")
        .bind(30i64)
        .bind(5i64)
        .execute(&pool)
        .await
        .expect("update");

        let (quiet_until, pending_minutes, pending_count): (Option<String>, i64, i64) =
            sqlx::query_as(
                "SELECT quiet_until, pending_minutes, pending_count FROM notification_state WHERE id = 1",
            )
            .fetch_one(&pool)
            .await
            .expect("select after update");
        assert_eq!(quiet_until.as_deref(), Some("2026-12-31T23:59:59Z"));
        assert_eq!(pending_minutes, 30);
        assert_eq!(pending_count, 5);
    }

    // ── 10. indexes exist ──

    #[tokio::test]
    async fn key_indexes_exist() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");

        let indexes: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_all(&pool)
        .await
        .expect("list indexes");

        let expected_indexes = [
            "idx_tasks_status_created",
            "idx_tasks_claude_session",
            "idx_chat_messages_created",
            "idx_chat_messages_seq",
            "idx_memory_key",
            "idx_memory_category",
            "idx_memory_scope",
            "idx_memory_workspace_id",
            "idx_conversation_timestamp",
            "idx_chunk_memory_id",
            "idx_chunk_scope",
            "idx_chunk_category",
            "idx_proposals_status",
            "idx_proposals_created",
            "idx_workspaces_root",
            "idx_workspaces_kind",
            "idx_avatar_updated_at",
            "idx_agent_teams_workspace",
            "idx_agent_team_members_agent",
            "idx_agent_mailbox_recipient",
            "idx_tool_calls_session",
            "idx_tool_calls_tool",
            "idx_tool_calls_status",
            "idx_capability_grants_capability",
            "idx_capability_grants_connector",
        ];

        for expected in &expected_indexes {
            assert!(
                indexes.iter().any(|i| i == expected),
                "missing index: {expected}"
            );
        }
    }

    // ── 11. agent_mailbox_messages ──

    #[tokio::test]
    async fn agent_mailbox_insert_and_query() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-07-01T00:00:00Z";

        // Create team first
        sqlx::query(
            r#"INSERT INTO agent_teams (id, name, status, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind("team-mbox")
        .bind("Mailbox Team")
        .bind("active")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert team");

        sqlx::query(
            r#"INSERT INTO agent_mailbox_messages
              (id, team_id, sender_agent_id, recipient_agent_id, kind, content, created_at)
              VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("mail-1")
        .bind("team-mbox")
        .bind("agent-a")
        .bind("agent-b")
        .bind("task")
        .bind("Please process this file")
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert");

        let content: String =
            sqlx::query_scalar("SELECT content FROM agent_mailbox_messages WHERE id = ?")
                .bind("mail-1")
                .fetch_one(&pool)
                .await
                .expect("select");
        assert_eq!(content, "Please process this file");

        // Query unread messages for recipient
        let unread: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_mailbox_messages WHERE team_id = ? AND recipient_agent_id = ? AND read_at IS NULL",
        )
        .bind("team-mbox")
        .bind("agent-b")
        .fetch_one(&pool)
        .await
        .expect("count unread");
        assert_eq!(unread, 1);
    }

    // ── 12. tool_runs ──

    #[tokio::test]
    async fn tool_runs_insert_and_query() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-08-01T00:00:00Z";

        sqlx::query(
            r#"INSERT INTO tool_runs
              (id, proposal_id, workspace_id, tool_id, status, started_at)
              VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind("tr-1")
        .bind("prop-1")
        .bind("ws-1")
        .bind("shell_exec")
        .bind("running")
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert");

        let status: String = sqlx::query_scalar("SELECT status FROM tool_runs WHERE id = ?")
            .bind("tr-1")
            .fetch_one(&pool)
            .await
            .expect("select");
        assert_eq!(status, "running");

        // Finish the run
        let finished = "2026-08-01T00:05:00Z";
        sqlx::query(
            "UPDATE tool_runs SET status = ?, finished_at = ?, output_ref = ? WHERE id = ?",
        )
        .bind("completed")
        .bind(finished)
        .bind("output/abc123")
        .bind("tr-1")
        .execute(&pool)
        .await
        .expect("update");

        let (status, output_ref): (String, Option<String>) =
            sqlx::query_as("SELECT status, output_ref FROM tool_runs WHERE id = ?")
                .bind("tr-1")
                .fetch_one(&pool)
                .await
                .expect("select after update");
        assert_eq!(status, "completed");
        assert_eq!(output_ref.as_deref(), Some("output/abc123"));
    }

    // ── 13. agent_runs ──

    #[tokio::test]
    async fn agent_runs_insert_and_query() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-09-01T00:00:00Z";

        sqlx::query(
            r#"INSERT INTO agent_runs
              (id, agent_id, role, workspace_id, status, started_at, updated_at)
              VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("ar-1")
        .bind("agent-1")
        .bind("coder")
        .bind("ws-1")
        .bind("running")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert");

        let status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = ?")
            .bind("ar-1")
            .fetch_one(&pool)
            .await
            .expect("select");
        assert_eq!(status, "running");

        // Query by agent_id
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_runs WHERE agent_id = ?")
            .bind("agent-1")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(count, 1);
    }

    // ── 14. avatar_state ──

    #[tokio::test]
    async fn avatar_state_insert_and_query() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-10-01T00:00:00Z";

        sqlx::query(
            r#"INSERT INTO avatar_state
              (id, theme, character, color_scheme, size, position_x, position_y,
               animation_enabled, auto_hide, last_active_at, updated_at)
              VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("avatar-default")
        .bind("dark")
        .bind("cat")
        .bind("blue")
        .bind(64i64)
        .bind(100i64)
        .bind(200i64)
        .bind(1i64)
        .bind(0i64)
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert");

        let (theme, character, size): (String, String, i64) =
            sqlx::query_as("SELECT theme, character, size FROM avatar_state WHERE id = ?")
                .bind("avatar-default")
                .fetch_one(&pool)
                .await
                .expect("select");
        assert_eq!(theme, "dark");
        assert_eq!(character, "cat");
        assert_eq!(size, 64);
    }

    // ── 15. conversation_summaries ──

    #[tokio::test]
    async fn conversation_summaries_insert_and_query() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-11-01T00:00:00Z";

        sqlx::query(
            r#"INSERT INTO conversation_summaries (id, summary, keywords, timestamp)
               VALUES (?, ?, ?, ?)"#,
        )
        .bind("conv-1")
        .bind("Discussed project architecture and decided on microservices")
        .bind("architecture,microservices,design")
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert");

        let summary: String =
            sqlx::query_scalar("SELECT summary FROM conversation_summaries WHERE id = ?")
                .bind("conv-1")
                .fetch_one(&pool)
                .await
                .expect("select");
        assert!(summary.contains("microservices"));

        // Query by timestamp range
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM conversation_summaries WHERE timestamp >= ?")
                .bind("2026-11-01T00:00:00Z")
                .fetch_one(&pool)
                .await
                .expect("count");
        assert_eq!(count, 1);
    }

    // ── 16. memory_chunks ──

    #[tokio::test]
    async fn memory_chunks_insert_and_query() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let now = "2026-12-01T00:00:00Z";

        sqlx::query(
            r#"INSERT INTO memory_chunks
              (id, memory_id, scope, category, content, source, sensitivity, confidence, created_at, updated_at)
              VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("chunk-1")
        .bind("mem-1")
        .bind("global")
        .bind("personal")
        .bind("The user prefers dark mode in all applications")
        .bind("user_confirmed")
        .bind("normal")
        .bind(0.95f64)
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert");

        let content: String = sqlx::query_scalar("SELECT content FROM memory_chunks WHERE id = ?")
            .bind("chunk-1")
            .fetch_one(&pool)
            .await
            .expect("select");
        assert!(content.contains("dark mode"));

        // Query by memory_id
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM memory_chunks WHERE memory_id = ?")
                .bind("mem-1")
                .fetch_one(&pool)
                .await
                .expect("count");
        assert_eq!(count, 1);

        // Query by category
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM memory_chunks WHERE category = ?")
                .bind("personal")
                .fetch_one(&pool)
                .await
                .expect("count by category");
        assert_eq!(count, 1);
    }

    // ── Runtime tables (TASK-077) ─────────────────────────────────────

    #[tokio::test]
    async fn runtime_tables_exist() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let expected = vec![
            "workspace_runtimes",
            "goal_runs",
            "goal_cycles",
            "dispatch_plans",
            "agent_tasks",
            "agent_run_refs",
            "agent_messages",
            "work_leases",
            "agent_heartbeats",
            "runtime_events",
            "workspace_projection_state",
            "routing_policies",
            "route_decisions",
        ];
        for table in &expected {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("query sqlite_master");
            assert_eq!(count, 1, "expected table {table} to exist");
        }
    }

    #[tokio::test]
    async fn migrates_legacy_agent_tasks_name_conflict() {
        let root = TestRoot::new();
        let path = root.path().join("state").join("conductor.sqlite");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.expect("create state dir");
        }

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(15));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("connect sqlite");

        sqlx::query(
            r#"
            CREATE TABLE agent_tasks (
              task_list_id TEXT NOT NULL,
              id TEXT NOT NULL,
              subject TEXT NOT NULL,
              description TEXT NOT NULL DEFAULT '',
              active_form TEXT,
              owner TEXT,
              status TEXT NOT NULL,
              workspace_id TEXT,
              source TEXT NOT NULL DEFAULT 'manual',
              kind TEXT NOT NULL DEFAULT 'task',
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              metadata_json TEXT,
              PRIMARY KEY(task_list_id, id)
            );
            "#,
        )
        .execute(&pool)
        .await
        .expect("create legacy agent_tasks");

        sqlx::query(
            r#"
            INSERT INTO agent_tasks (
                task_list_id, id, subject, description, status, workspace_id,
                source, kind, created_at, updated_at
            )
            VALUES ('default', 'legacy-item', 'Legacy task', 'Move me',
                    'pending', 'ws-default', 'manual', 'task',
                    '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')
            "#,
        )
        .execute(&pool)
        .await
        .expect("insert legacy task");

        migrate(&pool).await.expect("migrate");

        assert!(table_has_column(&pool, "agent_tasks", "goal_id")
            .await
            .expect("check goal_id"));
        assert!(!table_has_column(&pool, "agent_tasks", "task_list_id")
            .await
            .expect("check old column"));

        let migrated: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM agent_tasklist_items WHERE id = ?")
                .bind("legacy-item")
                .fetch_one(&pool)
                .await
                .expect("count migrated task");
        assert_eq!(migrated, 1);
    }

    #[tokio::test]
    async fn migrates_runtime_observability_columns() {
        let root = TestRoot::new();
        let path = root.path().join("state").join("conductor.sqlite");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.expect("create state dir");
        }

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(15));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("connect sqlite");

        sqlx::query(
            r#"
            CREATE TABLE tool_calls (
              id TEXT PRIMARY KEY,
              session_id TEXT,
              tool_id TEXT NOT NULL,
              input_json TEXT NOT NULL,
              output_json TEXT,
              status TEXT NOT NULL DEFAULT 'pending',
              error TEXT,
              started_at TEXT NOT NULL,
              completed_at TEXT,
              duration_ms INTEGER,
              agent_run_id TEXT
            );
            "#,
        )
        .execute(&pool)
        .await
        .expect("create legacy tool_calls");

        sqlx::query(
            r#"
            CREATE TABLE command_runs (
              id TEXT PRIMARY KEY,
              session_id TEXT,
              command TEXT NOT NULL,
              cwd TEXT NOT NULL,
              status TEXT NOT NULL,
              exit_code INTEGER,
              stdout_tail TEXT NOT NULL DEFAULT '',
              stderr_tail TEXT NOT NULL DEFAULT '',
              pid INTEGER,
              started_at TEXT,
              completed_at TEXT,
              created_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&pool)
        .await
        .expect("create legacy command_runs");

        migrate(&pool).await.expect("migrate");

        for column in [
            "workspace_id",
            "llm_tool_call_id",
            "risk_level",
            "proposal_id",
            "permission_grant_id",
            "command_run_id",
        ] {
            assert!(
                table_has_column(&pool, "tool_calls", column)
                    .await
                    .expect("check tool_calls column"),
                "expected tool_calls.{column}"
            );
        }

        for column in [
            "tool_call_id",
            "agent_run_id",
            "permission_grant_id",
            "risk_level",
            "env_delta_json",
        ] {
            assert!(
                table_has_column(&pool, "command_runs", column)
                    .await
                    .expect("check command_runs column"),
                "expected command_runs.{column}"
            );
        }
    }

    #[tokio::test]
    async fn runtime_indexes_exist() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool");
        let expected = vec![
            "idx_goal_runs_workspace_status",
            "idx_goal_cycles_goal_no",
            "idx_agent_tasks_workspace_status",
            "idx_agent_tasks_goal_cycle",
            "idx_agent_messages_workspace_created",
            "idx_agent_messages_task",
            "idx_agent_messages_recipient_unread",
            "idx_work_leases_active",
            "idx_agent_heartbeats_workspace",
            "idx_runtime_events_workspace_created",
            "idx_runtime_events_subject",
            "idx_dispatch_plans_goal_cycle_status",
            "idx_agent_run_refs_task",
            "idx_routing_policies_kind_enabled_priority",
            "idx_route_decisions_task",
            "idx_route_decisions_workspace_created",
        ];
        for index in &expected {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name = ?",
            )
            .bind(index)
            .fetch_one(&pool)
            .await
            .expect("query sqlite_master");
            assert_eq!(count, 1, "expected index {index} to exist");
        }
    }

    #[tokio::test]
    async fn runtime_migration_idempotent() {
        let _root = TestRoot::new();
        let pool = pool().await.expect("pool first call");
        // Second call should also succeed (CREATE TABLE IF NOT EXISTS)
        migrate(&pool).await.expect("migrate second call");
    }
}
