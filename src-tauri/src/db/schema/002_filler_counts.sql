-- Cumulative filler-word removal counts. Idempotent so existing v1
-- installs pick up the new table on next launch.

CREATE TABLE IF NOT EXISTS filler_counts (
    word  TEXT PRIMARY KEY,
    count INTEGER NOT NULL DEFAULT 0
);
