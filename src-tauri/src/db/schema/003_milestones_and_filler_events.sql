-- Schema v3 — milestone-reached de-duplication + time-windowed filler events.
--
-- Both tables support the v0.1.43 Insights expansion.

-- One row per milestone the user has crossed. Used to make sure we never
-- fire the same milestone notification twice. The key is a string like
-- "transcriptions_100" or "streak_30"; reached_at is the unix-ms timestamp
-- of the first crossing.
CREATE TABLE IF NOT EXISTS milestones_reached (
    key        TEXT    PRIMARY KEY NOT NULL,
    reached_at INTEGER NOT NULL
);

-- One row per (word, removed_at) tuple for stripped fillers. The cumulative
-- `filler_counts` table tells us totals; this table answers time-windowed
-- questions like "did the user say 'um' less this month than last." Each
-- transcription writes 0–N rows (one per distinct filler word it stripped).
-- The index keeps the month-vs-month query a quick range scan.
CREATE TABLE IF NOT EXISTS filler_events (
    word       TEXT    NOT NULL,
    removed_at INTEGER NOT NULL,
    count      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_filler_events_removed_at
    ON filler_events(removed_at);

CREATE INDEX IF NOT EXISTS idx_filler_events_word_removed_at
    ON filler_events(word, removed_at);
