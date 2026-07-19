//! Append-only timing log for transcription performance debugging.
//!
//! Production builds don't have a console attached, so `eprintln` output
//! is lost. This module writes timestamped lines to `<app_data>/perf.log`
//! so users can share the file when investigating slowness.
//!
//! **Dual-write** (v0.1.51): on Windows installs launched from
//! Explorer/Start-menu, `%APPDATA%` is sometimes not propagated to the
//! child process — in that case `<app_data>/perf.log` lands somewhere
//! useless and we lose all diagnostic trail. As a belt-and-suspenders
//! safeguard we ALSO write each line to `<exe_dir>/perf.log`, which is
//! always writable on a per-user NSIS install. The exe-adjacent file is
//! the diagnostic-of-last-resort: it exists even when settings/DB
//! writes silently fail.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::SystemTime;

static LOG_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static FALLBACK_LOG_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Set once at startup with the resolved app-data dir. Also computes
/// (and caches) the exe-adjacent fallback path. Subsequent calls are
/// no-ops, so it's safe to call from anywhere.
pub fn init(app_data_dir: PathBuf) {
    let path = app_data_dir.join("perf.log");
    let _ = LOG_PATH.set(Some(path));

    // Compute the fallback path: <dir of murmr.exe>/perf.log. This is the
    // diagnostic-of-last-resort path that always works regardless of
    // launch-context env var quirks (Windows Explorer/Start-menu installs
    // where %APPDATA% isn't propagated — see module docs).
    //
    // NEVER do this on macOS. There the executable lives INSIDE the signed
    // .app bundle (Contents/MacOS/), so writing a file next to it breaks the
    // code-signature seal. Gatekeeper then refuses to launch with
    // "\u{201c}Murmr\u{201d} is damaged and can't be opened" — even though the
    // app is correctly signed and notarized. The app-data path above is the
    // correct, always-writable location on macOS, so we leave the fallback
    // unset there.
    #[cfg(not(target_os = "macos"))]
    let fallback = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("perf.log")));
    #[cfg(target_os = "macos")]
    let fallback: Option<PathBuf> = None;

    let _ = FALLBACK_LOG_PATH.set(fallback);
}

/// Append a single line. Best-effort: any IO error is swallowed since
/// missing logs shouldn't crash the dictation flow. Writes to BOTH the
/// primary `<app_data>/perf.log` AND the fallback `<exe_dir>/perf.log`
/// — if one path is broken (permissions, missing env var, etc.) the
/// other still captures the trail.
pub fn append(line: &str) {
    let ts = format_timestamp();
    let formatted = format!("{ts}  {line}\n");

    let mut wrote_any = false;
    if let Some(Some(path)) = LOG_PATH.get() {
        if write_line_to(path, &formatted) {
            wrote_any = true;
        }
    }
    if let Some(Some(path)) = FALLBACK_LOG_PATH.get() {
        if write_line_to(path, &formatted) {
            wrote_any = true;
        }
    }
    if !wrote_any {
        // Last-resort: stderr (lost in release builds, useful in dev).
        eprintln!("[perf] {line}");
    }
}

/// Try to append `line` (already includes trailing newline) to `path`,
/// creating the parent dir if needed. Returns true on success.
fn write_line_to(path: &std::path::Path, line: &str) -> bool {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut f) => f.write_all(line.as_bytes()).is_ok(),
        Err(_) => false,
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
