CREATE TABLE tasks (
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
  priority TEXT DEFAULT 'normal'
);

CREATE INDEX idx_tasks_status_created ON tasks(status, created_at DESC);

CREATE TABLE notification_state (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  quiet_until TEXT,
  last_notified_at TEXT,
  pending_minutes INTEGER DEFAULT 0,
  pending_count INTEGER DEFAULT 0
);

INSERT INTO notification_state(id) VALUES (1);
