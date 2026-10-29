-- Schema v4 — dictionary suggestions learned from the user's corrections.
--
-- One row per distinct suggestion. `key` dedupes them (type + lowercased
-- trigger + lowercased expansion); `count` is how many times the user has
-- made that same correction. Suggestions are never applied on their own:
-- they wait in 'pending' until the user accepts or dismisses them.
-- Idempotent (IF NOT EXISTS) so it is safe to re-run under another number.

CREATE TABLE IF NOT EXISTS dictionary_suggestions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    type          TEXT    NOT NULL CHECK (type IN ('word', 'replacement')),
    trigger       TEXT    NOT NULL,
    expansion     TEXT,
    key           TEXT    NOT NULL UNIQUE,
    count         INTEGER NOT NULL DEFAULT 1,
    status        TEXT    NOT NULL DEFAULT 'pending'
                  CHECK (status IN ('pending', 'accepted', 'dismissed')),
    example       TEXT,              -- the corrected sentence, for context
    first_seen_at INTEGER NOT NULL,  -- unix epoch ms
    last_seen_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dictionary_suggestions_status
    ON dictionary_suggestions(status, count DESC, last_seen_at DESC);
