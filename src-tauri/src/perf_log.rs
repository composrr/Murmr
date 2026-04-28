//! Append-only timing log for transcription performance debugging.
//!
//! Production builds don't have a console attached, so `eprintln` output
//! is lost. This module writes timestamped lines to `<app_data>/perf.log`
//! so users can share the file when investigating slowness.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::SystemTime;

static LOG_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Set once at startup with the resolved app-data dir. Subsequent calls are
/// no-ops, so it's safe to call from anywhere.
pub fn init(app_data_dir: PathBuf) {
    let path = app_data_dir.join("perf.log");
    let _ = LOG_PATH.set(Some(path));
}

/// Append a single line. Best-effort: any IO error is swallowed since
/// missing logs shouldn't crash the dictation flow.
pub fn append(line: &str) {
    let Some(Some(path)) = LOG_PATH.get() else {
        // Fall back to stderr if init wasn't called yet (dev mode).
        eprintln!("[perf] {line}");
        return;
    };
    let ts = format_timestamp();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{ts}  {line}");
    }
}

fn format_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let secs_in_day = (now.rem_euclid(86_400)) as u32;
    let h = secs_in_day / 3600;
    let m = (secs_in_day / 60) % 60;
    let s = secs_in_day % 60;
    format!("{h:02}:{m:02}:{s:02}")
}
