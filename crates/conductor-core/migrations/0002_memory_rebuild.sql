-- 0002_memory_rebuild.sql
-- Backfill migration: rebuild memory_chunks and memory_embeddings from memory_entries.
-- Run via: sqlite3 <db_path> < 0002_memory_rebuild.sql
-- Or invoke reindex_memory_chunk() per chunk from Rust after startup.
--
-- This migration is SAFE to re-run (idempotent via INSERT OR IGNORE).
-- It does NOT delete existing chunks/embeddings — use cleanup_entry_chunks() for that.
--
-- Step 1: Ensure all memory_entries have a corresponding memory_chunk row.
--         The chunk ID convention is 'entry-<memory_id>'.
INSERT OR IGNORE INTO memory_chunks (
    id,
    memory_id,
    workspace_id,
    scope,
    category,
    content,
    summary,
    source,
    sensitivity,
    confidence,
    scene_tags,
    created_at,
    updated_at,
    expires_at,
    last_used_at
)
SELECT
    'entry-' || id         AS id,
    id                     AS memory_id,
    workspace_id,
    scope,
    category,
    '类别: ' || category || char(10) || '键: ' || key || char(10) || '内容: ' || value AS content,
    NULL                   AS summary,
    source,
    sensitivity,
    confidence,
    '[]'                   AS scene_tags,
    created_at,
    updated_at,
    expires_at,
    last_used_at
FROM memory_entries
WHERE status NOT IN ('forgotten', 'quarantined');

-- Step 2: Embeddings are generated at runtime by reindex_memory_chunk().
--         This script only ensures the chunk rows exist so the Rust
--         startup reindex pass has something to work with.
--
-- To trigger full reindex from Rust:
--   for chunk in list_chunks_without_embeddings():
--       reindex_memory_chunk(&chunk.id).await?;
