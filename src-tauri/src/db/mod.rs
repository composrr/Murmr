//! SQLite persistence for Murmr.
//!
//! Owns the `transcriptions`, `dictionary_entries`, and `stats` tables (per
//! plan §5 + §8 schema). All access goes through the `Db` struct's pooled
//! `Connection` (single-writer SQLite is fine for our throughput; WAL mode
//! lets the main window read while the controller writes).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;

const SCHEMA_V1: &str = include_str!("schema/001_initial.sql");
const SCHEMA_V2: &str = include_str!("schema/002_filler_counts.sql");

#[derive(Debug, Clone, Serialize)]
pub struct Transcription {
    pub id: i64,
    pub text: String,
    pub word_count: i64,
    pub duration_ms: i64,
    pub target_app: Option<String>,
    pub created_at: i64,
}

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// Open (or create) the database at `<app-data>/murmr.db`.
    pub fn open(app_data_dir: &Path) -> Result<Arc<Self>, String> {
        let path = app_data_dir.join("murmr.db");
        std::fs::create_dir_all(app_data_dir)
            .map_err(|e| format!("create app-data dir {app_data_dir:?}: {e}"))?;
        let mut conn = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| format!("open sqlite at {path:?}: {e}"))?;

        // Plan §13 #9 — WAL avoids reader-writer lock contention.
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("set WAL: {e}"))?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| format!("set synchronous: {e}"))?;

        // Apply migrations idempotently.
        Self::apply_migrations(&mut conn)?;

        Ok(Arc::new(Self {
            conn: Mutex::new(conn),
        }))
    }

    fn apply_migrations(conn: &mut Connection) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                 version INTEGER PRIMARY KEY
             );",
        )
        .map_err(|e| format!("schema_version table: {e}"))?;

        let current_version: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |row| row.get(0),
            )
            .map_err(|e| format!("read schema version: {e}"))?;

        if current_version < 1 {
            conn.execute_batch(SCHEMA_V1)
                .map_err(|e| format!("apply schema v1: {e}"))?;
            conn.execute("INSERT INTO schema_version (version) VALUES (1)", [])
                .map_err(|e| format!("record schema v1: {e}"))?;
        }
        if current_version < 2 {
            conn.execute_batch(SCHEMA_V2)
                .map_err(|e| format!("apply schema v2: {e}"))?;
            conn.execute("INSERT INTO schema_version (version) VALUES (2)", [])
                .map_err(|e| format!("record schema v2: {e}"))?;
        }

        Ok(())
    }

    pub fn insert_transcription(
        &self,
        text: &str,
        word_count: i64,
        duration_ms: i64,
        target_app: Option<&str>,
    ) -> Result<i64, String> {
        let now = unix_ms_now();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO transcriptions (text, word_count, duration_ms, target_app, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![text, word_count, duration_ms, target_app, now],
        )
        .map_err(|e| format!("insert transcription: {e}"))?;
        Ok(conn.last_insert_rowid())
    }

    pub fn recent_transcriptions(&self, limit: i64) -> Result<Vec<Transcription>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, text, word_count, duration_ms, target_app, created_at
                 FROM transcriptions ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|e| format!("prepare recent: {e}"))?;
        let rows = stmt
            .query_map(params![limit], |row| {
                Ok(Transcription {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    word_count: row.get(2)?,
                    duration_ms: row.get(3)?,
                    target_app: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| format!("query recent: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect recent: {e}"))
    }

    pub fn search_transcriptions(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<Transcription>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.text, t.word_count, t.duration_ms, t.target_app, t.created_at
                 FROM transcriptions_fts f
                 JOIN transcriptions t ON t.id = f.rowid
                 WHERE transcriptions_fts MATCH ?1
                 ORDER BY t.created_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| format!("prepare search: {e}"))?;
        let rows = stmt
            .query_map(params![query, limit], |row| {
                Ok(Transcription {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    word_count: row.get(2)?,
                    duration_ms: row.get(3)?,
                    target_app: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| format!("query search: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect search: {e}"))
    }

    pub fn delete_transcription(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM transcriptions WHERE id = ?1", params![id])
            .map_err(|e| format!("delete transcription: {e}"))?;
        Ok(())
    }

    /// Drop transcriptions older than `days` days (where day boundaries use
    /// the same 4 am-shifted local convention as `bump_streak_day`).
    /// `days <= 0` means "keep forever" — returns 0 deletions.
    pub fn purge_older_than(&self, days: i64) -> Result<usize, String> {
        if days <= 0 {
            return Ok(0);
        }
        let cutoff_ms = unix_ms_now() - days * 86_400_000;
        let conn = self.conn.lock();
        let n = conn
            .execute(
                "DELETE FROM transcriptions WHERE created_at < ?1",
                params![cutoff_ms],
            )
            .map_err(|e| format!("purge transcriptions: {e}"))?;
        Ok(n)
    }

    pub fn clear_all_transcriptions(&self) -> Result<usize, String> {
        let conn = self.conn.lock();
        let n = conn
            .execute("DELETE FROM transcriptions", [])
            .map_err(|e| format!("clear all: {e}"))?;
        Ok(n)
    }

    pub fn clear_last_24_hours(&self) -> Result<usize, String> {
        let cutoff_ms = unix_ms_now() - 86_400_000;
        let conn = self.conn.lock();
        let n = conn
            .execute(
                "DELETE FROM transcriptions WHERE created_at >= ?1",
                params![cutoff_ms],
            )
            .map_err(|e| format!("clear 24h: {e}"))?;
        Ok(n)
    }

    pub fn transcription_count(&self) -> Result<i64, String> {
        let conn = self.conn.lock();
        conn.query_row("SELECT COUNT(*) FROM transcriptions", [], |row| row.get(0))
            .map_err(|e| format!("count transcriptions: {e}"))
    }

    // ----- Dictionary entries -----

    pub fn list_dictionary(&self, type_filter: Option<&str>) -> Result<Vec<DictionaryEntry>, String> {
        let conn = self.conn.lock();
        let sql = "SELECT id, type, trigger, expansion, description, is_regex, enabled, created_at, updated_at
                   FROM dictionary_entries
                   WHERE (?1 IS NULL OR type = ?1)
                   ORDER BY created_at DESC";
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare dict: {e}"))?;
        let rows = stmt
            .query_map(params![type_filter], |row| {
                Ok(DictionaryEntry {
                    id: row.get(0)?,
                    entry_type: row.get(1)?,
                    trigger: row.get(2)?,
                    expansion: row.get(3)?,
                    description: row.get(4)?,
                    is_regex: row.get::<_, i64>(5)? != 0,
                    enabled: row.get::<_, i64>(6)? != 0,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .map_err(|e| format!("query dict: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect dict: {e}"))
    }

    pub fn create_dictionary_entry(
        &self,
        entry_type: &str,
        trigger: &str,
        expansion: Option<&str>,
        description: Option<&str>,
        is_regex: bool,
    ) -> Result<i64, String> {
        let now = unix_ms_now();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO dictionary_entries
                (type, trigger, expansion, description, is_regex, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)",
            params![entry_type, trigger, expansion, description, is_regex as i64, now],
        )
        .map_err(|e| format!("insert dict: {e}"))?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_dictionary_entry(
        &self,
        id: i64,
        entry_type: &str,
        trigger: &str,
        expansion: Option<&str>,
        description: Option<&str>,
        is_regex: bool,
        enabled: bool,
    ) -> Result<(), String> {
        let now = unix_ms_now();
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE dictionary_entries
             SET type=?1, trigger=?2, expansion=?3, description=?4, is_regex=?5, enabled=?6, updated_at=?7
             WHERE id=?8",
            params![entry_type, trigger, expansion, description, is_regex as i64, enabled as i64, now, id],
        )
        .map_err(|e| format!("update dict: {e}"))?;
        Ok(())
    }

    pub fn delete_dictionary_entry(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM dictionary_entries WHERE id=?1", params![id])
            .map_err(|e| format!("delete dict: {e}"))?;
        Ok(())
    }

    // ----- Filler counts -----

    /// Increment the per-word filler counters by the supplied amounts.
    /// Words are stored lowercase.
    pub fn bump_fillers(&self, counts: &[(String, i64)]) -> Result<(), String> {
        if counts.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock();
        let tx = conn
            .transaction()
            .map_err(|e| format!("filler tx begin: {e}"))?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO filler_counts (word, count) VALUES (?1, ?2)
                     ON CONFLICT(word) DO UPDATE SET count = count + excluded.count",
                )
                .map_err(|e| format!("filler prepare: {e}"))?;
            for (word, n) in counts {
                stmt.execute(params![word.to_lowercase(), n])
                    .map_err(|e| format!("filler insert: {e}"))?;
            }
        }
        tx.commit().map_err(|e| format!("filler commit: {e}"))?;
        Ok(())
    }

    pub fn top_fillers(&self, limit: i64) -> Result<Vec<WordCount>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT word, count FROM filler_counts WHERE count > 0
                 ORDER BY count DESC, word ASC LIMIT ?1",
            )
            .map_err(|e| format!("top fillers prepare: {e}"))?;
        let rows = stmt
            .query_map(params![limit], |row| {
                Ok(WordCount {
                    word: row.get(0)?,
                    count: row.get(1)?,
                })
            })
            .map_err(|e| format!("top fillers query: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("top fillers collect: {e}"))
    }

    pub fn total_fillers_removed(&self) -> Result<i64, String> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT COALESCE(SUM(count), 0) FROM filler_counts",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("total fillers: {e}"))
    }

    /// Top 2-word phrases (case-insensitive) across all transcriptions.
    /// Filters out grams that lead with a preposition / determiner
    /// ("of the", "in a") since those drown out the interesting phrases.
    pub fn top_phrases(&self, limit: i64) -> Result<Vec<PhraseCount>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT text FROM transcriptions")
            .map_err(|e| format!("prepare phrases: {e}"))?;
        let texts: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("query phrases: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect phrases: {e}"))?;
        drop(stmt);
        drop(conn);

        let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for t in texts {
            // Sentence-aware so phrases never span periods.
            for sentence in t.split(|c: char| matches!(c, '.' | '!' | '?' | '\n')) {
                let tokens: Vec<String> = sentence
                    .split(|c: char| !c.is_alphanumeric() && c != '\'')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_lowercase())
                    .collect();
                for window in tokens.windows(2) {
                    if window.iter().any(|w| w.len() < 2) {
                        continue;
                    }
                    if PHRASE_LEAD_BLOCK.contains(&window[0].as_str()) {
                        continue;
                    }
                    if window.iter().all(|w| STOPWORDS.contains(&w.as_str())) {
                        continue;
                    }
                    let phrase = format!("{} {}", window[0], window[1]);
                    *counts.entry(phrase).or_insert(0) += 1;
                }
            }
        }

        let mut sorted: Vec<PhraseCount> = counts
            .into_iter()
            .filter(|(_, c)| *c >= 2)
            .map(|(phrase, count)| PhraseCount { phrase, count })
            .collect();
        sorted.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.phrase.cmp(&b.phrase)));
        sorted.truncate(limit as usize);
        Ok(sorted)
    }

    /// Group transcriptions into a few coarse "topic" buckets by keyword
    /// match. Returns each theme with the count of transcriptions touching
    /// it + the top sample words (from the user's actual text) that fired
    /// the match. Pure heuristic — accurate enough for the Insights persona
    /// card without needing an LLM.
    pub fn topic_breakdown(&self) -> Result<Vec<ThemeMatch>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT text FROM transcriptions")
            .map_err(|e| format!("prepare topics: {e}"))?;
        let texts: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("query topics: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect topics: {e}"))?;
        drop(stmt);
        drop(conn);

        let mut results: Vec<ThemeMatch> = Vec::with_capacity(THEMES.len());
        for theme in THEMES {
            let mut transcription_hits: i64 = 0;
            let mut word_hits: std::collections::HashMap<&'static str, i64> =
                std::collections::HashMap::new();
            for t in &texts {
                let lower = t.to_lowercase();
                let mut hit = false;
                for kw in theme.keywords {
                    let count = count_word_occurrences(&lower, kw);
                    if count > 0 {
                        hit = true;
                        *word_hits.entry(*kw).or_insert(0) += count as i64;
                    }
                }
                if hit {
                    transcription_hits += 1;
                }
            }
            if transcription_hits > 0 {
                let mut samples: Vec<(String, i64)> = word_hits
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect();
                samples.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                samples.truncate(4);
                results.push(ThemeMatch {
                    theme: theme.id.to_string(),
                    label: theme.label.to_string(),
                    transcription_count: transcription_hits,
                    sample_words: samples.into_iter().map(|(w, _)| w).collect(),
                });
            }
        }
        results.sort_by(|a, b| b.transcription_count.cmp(&a.transcription_count));
        Ok(results)
    }

    /// 24 buckets, one per hour-of-day in the user's local time.
    pub fn hourly_distribution(&self) -> Result<[i64; 24], String> {
        let mut buckets = [0i64; 24];
        let offset_secs = local_utc_offset_seconds() as i64;
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT created_at FROM transcriptions")
            .map_err(|e| format!("prepare hours: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(|e| format!("query hours: {e}"))?;
        for ts in rows {
            let ts = ts.map_err(|e| format!("read hour ts: {e}"))?;
            let local_secs = ts / 1000 + offset_secs;
            let hour = ((local_secs.rem_euclid(86_400)) / 3600) as usize;
            if hour < 24 {
                buckets[hour] += 1;
            }
        }
        Ok(buckets)
    }

    /// Increment today's tally in `streak_days`. Per plan §13 #8, the day
    /// boundary should respect a user-local 4am rollover (so late-night
    /// dictating doesn't split a streak); we approximate that here by
    /// shifting local time back 4 hours before extracting the date.
    pub fn bump_streak_day(&self, now_unix_ms: i64) -> Result<(), String> {
        let day = local_day_with_4am_offset(now_unix_ms);
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO streak_days (day, transcription_count) VALUES (?1, 1)
             ON CONFLICT(day) DO UPDATE SET transcription_count = transcription_count + 1",
            params![day],
        )
        .map_err(|e| format!("bump streak day: {e}"))?;
        Ok(())
    }

    /// Return per-day transcription counts for the last `days` calendar days
    /// (inclusive of today). Days with zero activity return 0.
    pub fn heatmap_days(&self, days: i64) -> Result<Vec<DayCount>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT day, transcription_count FROM streak_days
                 ORDER BY day DESC LIMIT ?1",
            )
            .map_err(|e| format!("prepare heatmap: {e}"))?;
        let rows = stmt
            .query_map(params![days], |row| {
                Ok(DayCount {
                    day: row.get(0)?,
                    count: row.get(1)?,
                })
            })
            .map_err(|e| format!("query heatmap: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect heatmap: {e}"))
    }

    /// Aggregated counts used by the Insights page: total transcriptions,
    /// total words, total speech ms, longest/current streak.
    pub fn usage_totals(&self) -> Result<UsageTotals, String> {
        let conn = self.conn.lock();
        let (total_transcriptions, total_words, total_speech_ms): (i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(word_count), 0), COALESCE(SUM(duration_ms), 0)
                 FROM transcriptions",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|e| format!("totals query: {e}"))?;

        let mut day_rows: Vec<i64> = conn
            .prepare("SELECT day FROM streak_days WHERE transcription_count > 0 ORDER BY day DESC")
            .map_err(|e| format!("prepare days: {e}"))?
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(|e| format!("query days: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect days: {e}"))?;

        // Walk consecutive days from today backward to find the current streak.
        let today = local_day_with_4am_offset(unix_ms_now());
        let yesterday = previous_day(today);
        let mut current_streak: i64 = 0;
        if day_rows.first() == Some(&today) || day_rows.first() == Some(&yesterday) {
            // Allow today to be missing (user hasn't dictated yet today)
            // and still count yesterday-anchored streaks.
            let mut expected = if day_rows.first() == Some(&today) { today } else { yesterday };
            for d in &day_rows {
                if *d == expected {
                    current_streak += 1;
                    expected = previous_day(expected);
                } else {
                    break;
                }
            }
        }

        // Longest streak: walk all days and count longest run of consecutive ones.
        day_rows.sort();
        let mut longest_streak: i64 = 0;
        let mut run: i64 = 0;
        let mut prev: Option<i64> = None;
        for d in &day_rows {
            if let Some(p) = prev {
                if *d == next_day(p) {
                    run += 1;
                } else {
                    longest_streak = longest_streak.max(run);
                    run = 1;
                }
            } else {
                run = 1;
            }
            prev = Some(*d);
        }
        longest_streak = longest_streak.max(run);

        Ok(UsageTotals {
            total_transcriptions,
            total_words,
            total_speech_ms,
            current_streak,
            longest_streak,
        })
    }

    /// Top `limit` non-stopword tokens across all transcription text.
    pub fn top_words(&self, limit: i64) -> Result<Vec<WordCount>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT text FROM transcriptions")
            .map_err(|e| format!("prepare text: {e}"))?;
        let texts = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("query text: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect text: {e}"))?;

        let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for t in texts {
            for token in t
                .split(|c: char| !c.is_alphanumeric() && c != '\'')
                .filter(|s| !s.is_empty())
            {
                let lower = token.to_lowercase();
                if STOPWORDS.contains(&lower.as_str()) {
                    continue;
                }
                if lower.len() < 3 {
                    continue;
                }
                *counts.entry(lower).or_insert(0) += 1;
            }
        }

        let mut sorted: Vec<WordCount> = counts
            .into_iter()
            .map(|(word, count)| WordCount { word, count })
            .collect();
        sorted.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.word.cmp(&b.word)));
        sorted.truncate(limit as usize);
        Ok(sorted)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DayCount {
    pub day: i64,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageTotals {
    pub total_transcriptions: i64,
    pub total_words: i64,
    pub total_speech_ms: i64,
    pub current_streak: i64,
    pub longest_streak: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WordCount {
    pub word: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhraseCount {
    pub phrase: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeMatch {
    /// Stable internal id (`coding`, `personal`, etc.) — useful for the
    /// frontend to pick an icon or copy snippet.
    pub theme: String,
    pub label: String,
    pub transcription_count: i64,
    pub sample_words: Vec<String>,
}

struct Theme {
    id: &'static str,
    label: &'static str,
    keywords: &'static [&'static str],
}

/// Coarse life-area buckets. Order doesn't matter; results are sorted by
/// transcription count. Keep keywords lowercase, single-word, common.
/// Keep lists tight — too many keywords means everything matches everything.
const THEMES: &[Theme] = &[
    Theme {
        id: "building",
        label: "Building & tech",
        keywords: &[
            "code", "coding", "function", "class", "method", "bug", "debug", "deploy",
            "build", "compile", "repo", "branch", "commit", "merge", "api", "framework",
            "library", "server", "database", "query", "schema", "frontend", "backend",
            "model", "prompt", "ai", "llm",
        ],
    },
    Theme {
        id: "coordinating",
        label: "Coordinating with people",
        keywords: &[
            "meeting", "email", "slack", "team", "sync", "standup", "agenda", "follow",
            "reply", "thread", "message", "call", "schedule", "zoom",
            "feedback", "review",
        ],
    },
    Theme {
        id: "planning",
        label: "Planning & lists",
        keywords: &[
            "todo", "tomorrow", "today", "plan", "deadline", "calendar", "reminder",
            "task", "project", "goal", "priority", "next", "before", "after", "weekly",
            "monthly", "quarterly", "list",
        ],
    },
    Theme {
        id: "personal",
        label: "Personal & family",
        keywords: &[
            "family", "friend", "weekend", "dinner", "kids", "parents", "home",
            "partner", "wife", "husband", "mom", "dad", "brother", "sister",
            "birthday", "house", "dog", "cat",
        ],
    },
    Theme {
        id: "writing",
        label: "Writing & ideas",
        keywords: &[
            "write", "writing", "blog", "post", "article", "draft", "edit", "revise",
            "story", "idea", "concept", "thought", "essay", "chapter", "outline",
            "content", "copy", "headline",
        ],
    },
    Theme {
        id: "money",
        label: "Money & business",
        keywords: &[
            "dollar", "dollars", "pay", "invoice", "bill", "cost", "budget", "expense",
            "save", "spend", "money", "client", "customer", "revenue", "subscription",
            "price", "sale", "deal",
        ],
    },
    Theme {
        id: "travel",
        label: "Travel & movement",
        keywords: &[
            "flight", "hotel", "trip", "drive", "plane", "vacation", "airport",
            "rental", "hotel", "uber", "lyft", "subway", "train", "road", "miles",
            "miles", "destination",
        ],
    },
    Theme {
        id: "leisure",
        label: "Leisure & culture",
        keywords: &[
            "movie", "show", "episode", "game", "watch", "music", "song", "podcast",
            "book", "novel", "read", "play", "stream", "netflix", "youtube",
            "spotify", "concert",
        ],
    },
    Theme {
        id: "health",
        label: "Health & body",
        keywords: &[
            "workout", "gym", "run", "running", "sleep", "diet", "eat", "ate", "food",
            "weight", "exercise", "water", "nap", "rest", "stretch", "yoga", "doctor",
            "health",
        ],
    },
    Theme {
        id: "errands",
        label: "Errands & logistics",
        keywords: &[
            "buy", "order", "amazon", "cart", "store", "ship", "delivery", "pickup",
            "appointment", "grocery", "errand", "package", "mail", "drop", "return",
        ],
    },
];

/// Count whole-word occurrences of `kw` (case-insensitive — caller passes
/// already-lowercased text). Cheap pass with a manual scan to avoid building
/// one regex per keyword per transcription.
fn count_word_occurrences(haystack: &str, kw: &str) -> usize {
    if kw.is_empty() {
        return 0;
    }
    let mut count = 0;
    let bytes = haystack.as_bytes();
    let needle = kw.as_bytes();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let before_ok = i == 0 || !is_word_char(bytes[i - 1]);
            let after_ok =
                i + needle.len() == bytes.len() || !is_word_char(bytes[i + needle.len()]);
            if before_ok && after_ok {
                count += 1;
                i += needle.len();
                continue;
            }
        }
        i += 1;
    }
    count
}

fn is_word_char(b: u8) -> bool {
    (b'a'..=b'z').contains(&b) || (b'A'..=b'Z').contains(&b) || (b'0'..=b'9').contains(&b) || b == b'\''
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct DictionaryEntry {
    pub id: i64,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub trigger: String,
    pub expansion: Option<String>,
    pub description: Option<String>,
    pub is_regex: bool,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Words that, when leading a 2-gram, almost always make it boring
/// ("of the", "in a", "to be"). Distinct from STOPWORDS — we still allow
/// stopwords inside a phrase so things like "let me know" make it through.
const PHRASE_LEAD_BLOCK: &[&str] = &[
    "the", "a", "an", "of", "to", "in", "on", "at", "for", "is", "was", "be", "are",
    "and", "but", "or", "so", "if", "as", "by", "with", "from", "that", "this",
    "it", "its", "i", "we", "you", "he", "she", "they",
];

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "but", "you", "are", "was", "were", "this", "that", "with",
    "from", "have", "has", "had", "will", "would", "could", "should", "their", "them",
    "they", "your", "yours", "his", "her", "him", "she", "out", "into", "over",
    "than", "then", "there", "here", "what", "when", "which", "who", "whom", "how",
    "why", "all", "any", "some", "most", "other", "such", "only", "own", "same",
    "very", "just", "also", "now", "more", "any", "not", "yes", "ok", "okay",
    "got", "get", "make", "made", "see", "say", "said", "like", "know", "think",
    "want", "need", "going", "come", "came", "let", "lets", "look", "looks", "way",
    "use", "used", "really", "actually", "things", "thing", "stuff", "yeah",
    "kind", "sort",
];

fn unix_ms_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Convert a unix-ms timestamp into the `yyyymmdd` integer for the user's
/// local "dictation day" — defined as their local calendar day shifted by
/// -4 hours so a 2am dictation still belongs to "yesterday" (plan §13 #8).
fn local_day_with_4am_offset(unix_ms: i64) -> i64 {
    let secs = unix_ms / 1000;
    let local_offset_secs = local_utc_offset_seconds() as i64;
    let local_secs = secs + local_offset_secs - 4 * 3600;
    let days_since_epoch = local_secs.div_euclid(86_400);
    civil_date_from_days(days_since_epoch)
}

/// Walk one day backward in yyyymmdd land.
fn previous_day(yyyymmdd: i64) -> i64 {
    let days = days_since_epoch_from_civil(yyyymmdd) - 1;
    civil_date_from_days(days)
}

fn next_day(yyyymmdd: i64) -> i64 {
    let days = days_since_epoch_from_civil(yyyymmdd) + 1;
    civil_date_from_days(days)
}

/// Cheap probe of the system's UTC offset. Avoids pulling in chrono just for
/// timezone math; this is good enough for streak day boundaries (a couple
/// of edge cases on DST night that we treat as documented in plan §13 #8).
fn local_utc_offset_seconds() -> i32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Compare a tm in UTC vs local for the same instant to derive the offset.
    let local = local_components(now);
    let utc = utc_components(now);
    let local_secs = ymd_to_secs(local) + (local.3 * 3600 + local.4 * 60 + local.5) as i64;
    let utc_secs = ymd_to_secs(utc) + (utc.3 * 3600 + utc.4 * 60 + utc.5) as i64;
    (local_secs - utc_secs) as i32
}

#[cfg(target_os = "windows")]
fn local_components(unix_secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    use std::os::raw::c_int;
    extern "C" {
        fn _localtime64_s(out: *mut Tm, time: *const i64) -> c_int;
    }
    #[repr(C)]
    #[derive(Default)]
    struct Tm {
        tm_sec: c_int,
        tm_min: c_int,
        tm_hour: c_int,
        tm_mday: c_int,
        tm_mon: c_int,
        tm_year: c_int,
        tm_wday: c_int,
        tm_yday: c_int,
        tm_isdst: c_int,
    }
    let mut tm = Tm::default();
    unsafe {
        _localtime64_s(&mut tm, &unix_secs);
    }
    (
        tm.tm_year + 1900,
        (tm.tm_mon + 1) as u32,
        tm.tm_mday as u32,
        tm.tm_hour as u32,
        tm.tm_min as u32,
        tm.tm_sec as u32,
    )
}

#[cfg(not(target_os = "windows"))]
fn local_components(unix_secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    // Posix path uses libc::localtime_r — same idea. Wired when we ship Mac.
    utc_components(unix_secs)
}

fn utc_components(unix_secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = unix_secs.div_euclid(86_400);
    let secs_in_day = unix_secs.rem_euclid(86_400) as u32;
    let yyyymmdd = civil_date_from_days(days);
    let year = (yyyymmdd / 10000) as i32;
    let month = ((yyyymmdd / 100) % 100) as u32;
    let day = (yyyymmdd % 100) as u32;
    let hour = secs_in_day / 3600;
    let minute = (secs_in_day / 60) % 60;
    let second = secs_in_day % 60;
    (year, month, day, hour, minute, second)
}

fn ymd_to_secs(c: (i32, u32, u32, u32, u32, u32)) -> i64 {
    let yyyymmdd = (c.0 as i64) * 10000 + (c.1 as i64) * 100 + c.2 as i64;
    days_since_epoch_from_civil(yyyymmdd) * 86_400
}

/// Howard Hinnant's days-from-civil → days-from-epoch (1970-01-01 = 0).
fn days_since_epoch_from_civil(yyyymmdd: i64) -> i64 {
    let y = yyyymmdd / 10000;
    let m = (yyyymmdd / 100) % 100;
    let d = yyyymmdd % 100;
    let yr = if m <= 2 { y - 1 } else { y };
    let era = yr.div_euclid(400);
    let yoe = yr - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse: days-from-epoch → yyyymmdd integer.
fn civil_date_from_days(days: i64) -> i64 {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    y * 10000 + m * 100 + d
}
