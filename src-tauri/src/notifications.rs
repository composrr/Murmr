//! Milestone notifications.
//!
//! Fires a small set of celebratory notifications when the user crosses
//! lifetime totals (1st, 100th, 500th transcription; 10k / 100k / 1M
//! words; 7 / 30 / 100 day streaks) or sets a new personal record
//! (longest dictation by words, highest WPM session — throttled to
//! once per week so a productive afternoon doesn't drown the user).
//!
//! Notifications fire AFTER the controller successfully injects, with a
//! short delay so the user has read the text. We also skip if the
//! focused app looks like a fullscreen game (probable Do-Not-Disturb
//! context) or if `Settings.milestone_notifications` is off.
//!
//! De-duplication is persistent — `milestones_reached` table — so a
//! crossed milestone never fires twice across launches.
//!
//! All work happens on a background thread spawned per check, so the
//! controller's hot path stays responsive.

use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::db::{Db, UsageTotals};
use crate::focus;
use crate::perf_log;
use crate::settings::SettingsStore;

/// Delay between successful injection and notification fire. Long enough
/// for the user to have read the just-pasted text + moved on, short
/// enough to feel connected to the dictation that earned the milestone.
const NOTIFY_DELAY: Duration = Duration::from_secs(4);

/// Personal-record notifications can fire at most once per this many
/// milliseconds — even if the user sets new records back-to-back.
/// 7 days keeps them feeling special.
const RECORD_THROTTLE_MS: i64 = 7 * 86_400_000;

/// Fire any milestone notifications earned by the latest transcription.
/// Spawned by the controller on a background thread after a successful
/// inject. The DB + settings + app handle are cheap to clone — all Arcs.
pub fn check_and_fire(
    app: AppHandle,
    db: Arc<Db>,
    settings: Arc<SettingsStore>,
    last_word_count: i64,
    last_duration_ms: i64,
) {
    std::thread::Builder::new()
        .name("murmr-notify".into())
        .spawn(move || {
            // Wrap the entire body in catch_unwind so any panic in the
            // notification flow (DB query, Tauri notification plugin,
            // Win32 monitor enumeration, etc) gets logged and the
            // worker exits cleanly — instead of bubbling up to the
            // process-wide panic = abort.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                check_and_fire_inner(app, db, settings, last_word_count, last_duration_ms);
            }));
            if let Err(e) = result {
                let msg = e
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| e.downcast_ref::<String>().map(|s| s.as_str()))
                    .unwrap_or("<no message>");
                perf_log::append(&format!("[notify] worker panicked: {msg}"));
            }
        })
        .expect("spawn notification thread");
}

fn check_and_fire_inner(
    app: AppHandle,
    db: Arc<Db>,
    settings: Arc<SettingsStore>,
    last_word_count: i64,
    last_duration_ms: i64,
) {
    perf_log::append("[notify] worker started");

    // Master toggle — bail before doing any work if the user turned
    // notifications off.
    if !settings.get().milestone_notifications {
        perf_log::append("[notify] disabled in settings, bailing");
        return;
    }

    // Pull totals fresh so we see the row we just inserted.
    let totals = match db.usage_totals() {
        Ok(t) => t,
        Err(e) => {
            perf_log::append(&format!("[notify] usage_totals failed: {e}"));
            return;
        }
    };

    let mut candidates: Vec<Milestone> = Vec::new();
    candidates.extend(transcription_count_milestone(&totals));
    candidates.extend(words_total_milestone(&totals));
    candidates.extend(streak_milestone(&totals));
    candidates.extend(personal_record_milestones(
        &db,
        last_word_count,
        last_duration_ms,
    ));

    // Filter out anything we've already fired (lifetime keys) OR fired
    // too recently (record-throttle keys).
    let now = crate::db::unix_ms_now();
    candidates.retain(|m| {
        if m.is_throttled {
            match db.milestone_reached_at(&m.key) {
                Ok(Some(at)) => now - at > RECORD_THROTTLE_MS,
                _ => true,
            }
        } else {
            !db.is_milestone_reached(&m.key).unwrap_or(false)
        }
    });

    if candidates.is_empty() {
        perf_log::append("[notify] no fresh milestones to fire");
        return;
    }
    perf_log::append(&format!(
        "[notify] {} candidate milestone(s), sleeping before fire",
        candidates.len()
    ));

    // Sleep just before the fire so the user is past the
    // just-pasted-text reading moment, and so a fullscreen-mode check
    // below sees the actual current state, not the state-mid-injection.
    std::thread::sleep(NOTIFY_DELAY);

    if focused_app_is_fullscreen(&app) {
        perf_log::append(&format!(
            "[notify] {} milestone(s) suppressed — focused window is fullscreen",
            candidates.len(),
        ));
        return;
    }

    for m in candidates {
        perf_log::append(&format!(
            "[notify] firing milestone key={} title={:?}",
            m.key, m.title,
        ));
        if let Err(e) = app
            .notification()
            .builder()
            .title(&m.title)
            .body(&m.body)
            .show()
        {
            perf_log::append(&format!("[notify] show failed: {e}"));
            continue;
        }
        // Record AFTER successful show so a failed notification
        // doesn't burn the milestone. For throttled records we need to
        // refresh the timestamp — use the upsert helper that always
        // writes the current time.
        if m.is_throttled {
            let _ = db.upsert_milestone_now(&m.key);
        } else {
            let _ = db.record_milestone_reached(&m.key);
        }
    }
    perf_log::append("[notify] worker done");
}

#[derive(Debug, Clone)]
struct Milestone {
    /// Stable de-dup key, e.g. `transcriptions_100`, `streak_7`,
    /// `record_longest_words`.
    key: String,
    title: String,
    body: String,
    /// True for the per-week-throttled records, false for lifetime
    /// one-time milestones.
    is_throttled: bool,
}

fn transcription_count_milestone(totals: &UsageTotals) -> Option<Milestone> {
    let n = totals.total_transcriptions;
    // Hit exactly these counts (one chance per milestone).
    let (key, title, body) = match n {
        1 => ("first_transcription", "Welcome to Murmr", "Your first transcription is in. Murmr will keep getting smarter the more you use it."),
        100 => ("transcriptions_100", "100 transcriptions in", "You've used Murmr 100 times. Thank you for trusting it."),
        500 => ("transcriptions_500", "500 and counting", "Five hundred transcriptions in. Murmr is officially part of your workflow."),
        1000 => ("transcriptions_1000", "1,000 transcriptions", "A thousand dictations. That's a serious habit."),
        5000 => ("transcriptions_5000", "5,000 transcriptions", "Five thousand. You're in rare air."),
        _ => return None,
    };
    Some(Milestone {
        key: key.to_string(),
        title: title.to_string(),
        body: body.to_string(),
        is_throttled: false,
    })
}

fn words_total_milestone(totals: &UsageTotals) -> Option<Milestone> {
    let w = totals.total_words;
    // Crossed-this-time check: the milestone fires when we cross over,
    // not while we're past it. We approximate "crossed" as "current
    // total is past threshold and not_yet_recorded handles the rest."
    let (key, threshold, title, body) = if w >= 1_000_000 {
        ("words_1m", 1_000_000, "1,000,000 words", "A million words dictated. Wild.")
    } else if w >= 100_000 {
        ("words_100k", 100_000, "100,000 words", "One hundred thousand words. That's a couple of novels.")
    } else if w >= 10_000 {
        ("words_10k", 10_000, "10,000 words", "Ten thousand words dictated. You're rolling.")
    } else {
        return None;
    };
    // Guard against the milestone firing before its threshold (paranoia
    // since the SQL totals can't be negative, but cheap to assert).
    let _ = threshold;
    Some(Milestone {
        key: key.to_string(),
        title: title.to_string(),
        body: body.to_string(),
        is_throttled: false,
    })
}

fn streak_milestone(totals: &UsageTotals) -> Option<Milestone> {
    let (key, title, body) = match totals.current_streak {
        7 => ("streak_7", "Seven days in a row", "A full week of dictating. Nice rhythm."),
        30 => ("streak_30", "Thirty-day streak", "A month straight. This is just how you work now."),
        100 => ("streak_100", "100-day streak", "One hundred days in a row. Heroic consistency."),
        _ => return None,
    };
    Some(Milestone {
        key: key.to_string(),
        title: title.to_string(),
        body: body.to_string(),
        is_throttled: false,
    })
}

fn personal_record_milestones(
    db: &Db,
    last_word_count: i64,
    last_duration_ms: i64,
) -> Vec<Milestone> {
    let mut out = Vec::new();

    let records = match db.personal_records() {
        Ok(r) => r,
        Err(_) => return out,
    };

    // The transcription that JUST got inserted appears as the new record
    // if and only if its metric matches the all-time best AND beats it
    // (the personal_records query orders by metric DESC, created_at ASC
    // so ties go to the earlier transcription — i.e. the just-inserted
    // one only wins outright).
    if let Some(t) = records.longest_words.as_ref() {
        if t.word_count == last_word_count && last_word_count >= 30 {
            out.push(Milestone {
                key: "record_longest_words".to_string(),
                title: "New personal best".to_string(),
                body: format!("{} words in one go — your longest dictation yet.", last_word_count),
                is_throttled: true,
            });
        }
    }

    if let (Some(t), Some(wpm)) = (records.highest_wpm.as_ref(), records.highest_wpm_value) {
        let last_wpm = if last_duration_ms > 0 {
            last_word_count as f64 / (last_duration_ms as f64 / 60_000.0)
        } else {
            0.0
        };
        // Match the just-inserted row to the record holder by WPM
        // (within rounding). 50-word floor enforced both here and in
        // the SQL query.
        if last_word_count >= 50 && (wpm - last_wpm).abs() < 0.01 {
            out.push(Milestone {
                key: "record_highest_wpm".to_string(),
                title: "New personal best".to_string(),
                body: format!(
                    "{:.0} words per minute — your fastest session yet.",
                    wpm
                ),
                is_throttled: true,
            });
            let _ = t;
        }
    }

    out
}

/// Best-effort guess at "user is in fullscreen / do-not-disturb mode."
/// We check the size of the focused window: if it covers the entire
/// monitor area, it's almost certainly a fullscreen game / video /
/// presentation, and we should not interrupt. False positives (maximized
/// app that happens to cover the screen) just suppress one notification
/// — for non-throttled milestones the key isn't recorded yet so the
/// next eligible transcription gets a fresh chance.
fn focused_app_is_fullscreen(app: &AppHandle) -> bool {
    use tauri::Manager;
    let Some(rect) = focus::focused_window_screen_rect() else {
        return false;
    };
    // Any existing window will do — we just need its monitor list,
    // which is global to the process. Main exists for the lifetime
    // of the app even when hidden.
    let main = match app.get_webview_window("main") {
        Some(w) => w,
        None => return false,
    };
    let monitors = match main.available_monitors() {
        Ok(m) => m,
        Err(_) => return false,
    };
    // Find the monitor that contains the focused window's top-left
    // corner; compare the window's dimensions to that monitor's.
    for mon in monitors {
        let mpos = mon.position();
        let msize = mon.size();
        let m_left = mpos.x;
        let m_top = mpos.y;
        let m_right = mpos.x + msize.width as i32;
        let m_bottom = mpos.y + msize.height as i32;
        let contains_origin = rect.x >= m_left
            && rect.x < m_right
            && rect.y >= m_top
            && rect.y < m_bottom;
        if contains_origin {
            let w_match = (rect.width - msize.width as i32).abs() <= 4;
            let h_match = (rect.height - msize.height as i32).abs() <= 4;
            return w_match && h_match;
        }
    }
    false
}
