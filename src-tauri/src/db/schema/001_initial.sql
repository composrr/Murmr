-- Murmr schema v1 — transcription history, dictionary, stats counters.

CREATE TABLE IF NOT EXISTS transcriptions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    text        TEXT    NOT NULL,
    word_count  INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    target_app  TEXT,
    created_at  INTEGER NOT NULL  -- unix epoch ms
);

CREATE INDEX IF NOT EXISTS idx_transcriptions_created
    ON transcriptions(created_at DESC);

-- FTS5 virtual table mirroring `text` for fast search on the Home page.
CREATE VIRTUAL TABLE IF NOT EXISTS transcriptions_fts USING fts5(
    text,
    content='transcriptions',
    content_rowid='id'
);

CREATE TRIGGER IF NOT EXISTS transcriptions_ai AFTER INSERT ON transcriptions BEGIN
    INSERT INTO transcriptions_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TRIGGER IF NOT EXISTS transcriptions_ad AFTER DELETE ON transcriptions BEGIN
    INSERT INTO transcriptions_fts(transcriptions_fts, rowid, text)
        VALUES ('delete', old.id, old.text);
END;

CREATE TRIGGER IF NOT EXISTS transcriptions_au AFTER UPDATE ON transcriptions BEGIN
    INSERT INTO transcriptions_fts(transcriptions_fts, rowid, text)
        VALUES ('delete', old.id, old.text);
    INSERT INTO transcriptions_fts(rowid, text) VALUES (new.id, new.text);
END;

-- Per plan §8 — unified words / replacements / snippets.
CREATE TABLE IF NOT EXISTS dictionary_entries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    type        TEXT    NOT NULL CHECK (type IN ('word', 'replacement', 'snippet')),
    trigger     TEXT    NOT NULL,
    expansion   TEXT,
    description TEXT,
    is_regex    INTEGER NOT NULL DEFAULT 0,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_dictionary_type
    ON dictionary_entries(type);

-- Key/value bag for cached counters (top filler words, totals, etc.).
CREATE TABLE IF NOT EXISTS stats (
    key       TEXT PRIMARY KEY,
    value_int INTEGER,
    value_text TEXT
);

-- Per-day transcription tally for the streak heatmap.
-- `day` is yyyymmdd in the user's local timezone, with the "day starts at
-- 4am local" convention from plan §13 #8.
CREATE TABLE IF NOT EXISTS streak_days (
    day                 INTEGER PRIMARY KEY,
    transcription_count INTEGER NOT NULL DEFAULT 0
);

-- Cumulative filler-word removal counts. `word` is stored lowercase; the
-- value increments by 1 per stripped occurrence.
CREATE TABLE IF NOT EXISTS filler_counts (
    word  TEXT PRIMARY KEY,
    count INTEGER NOT NULL DEFAULT 0
);
