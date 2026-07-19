mod audio;
mod audio_duck;
mod controller;
mod db;
mod focus;
mod hotkey;
mod injector;
mod license;
mod notifications;
mod perf_log;
mod permissions;
mod settings;
mod sounds;
mod transcribe;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use tauri::{
    async_runtime,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};

use crate::audio::InputDevice;
use crate::controller::Controller;
use crate::db::{
    AppUsage, DayCount, Db, DictionaryEntry, FillerProgress, PersonalRecords, PhraseCount,
    ThemeMatch, Transcription, UsageTotals, WeekWpm, WordCount,
};
use crate::settings::{Settings, SettingsStore};
use crate::sounds::SoundPlayer;
use crate::transcribe::Transcriber;

// ---------- App state ----------

pub struct AppState {
    transcriber: Arc<Transcriber>,
    db: Arc<Db>,
    settings: Arc<SettingsStore>,
    practice_mode: Arc<AtomicBool>,
}

// ---------- IPC types ----------

#[derive(Serialize)]
struct PingResponse {
    message: String,
    version: String,
}

#[derive(Serialize)]
pub struct TranscriptionResult {
    text: String,
    captured_samples: usize,
    capture_sample_rate: u32,
    capture_channels: u16,
    capture_device: String,
    elapsed_capture_ms: u128,
    elapsed_resample_ms: u128,
    elapsed_transcribe_ms: u128,
}

// ---------- IPC commands ----------

#[derive(Serialize)]
struct UserInfo {
    display_name: String,
    raw_name: String,
}

#[tauri::command]
fn user_info() -> UserInfo {
    // OS user is the obvious source — env vars first, fallback to a friendly default.
    let raw = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "you".into());
    let first = raw.split(['.', '_', '-']).next().unwrap_or(&raw).to_string();
    let mut chars = first.chars();
    let display_name = match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str().to_lowercase().as_str(),
        None => "You".into(),
    };
    UserInfo {
        display_name,
        raw_name: raw,
    }
}

#[tauri::command]
fn ping() -> PingResponse {
    PingResponse {
        message: "Murmr backend is alive".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[tauri::command]
async fn record_and_transcribe(
    seconds: u32,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<TranscriptionResult, String> {
    let transcriber = Arc::clone(&state.transcriber);

    async_runtime::spawn_blocking(move || -> Result<TranscriptionResult, String> {
        // Emit RMS events while we record so test pages / mic-test can show
        // a live waveform. Throttle to ~50 Hz so we don't flood the IPC
        // channel.
        let last_emit = Arc::new(parking_lot::Mutex::new(
            Instant::now() - std::time::Duration::from_secs(1),
        ));
        let app_for_cb = app.clone();
        let cb: audio::RmsCallback = Arc::new(move |rms: f32| {
            let mut g = last_emit.lock();
            if g.elapsed() < std::time::Duration::from_millis(20) {
                return;
            }
            *g = Instant::now();
            drop(g);
            let _ = app_for_cb.emit("murmr:audio-rms", rms);
        });

        let cap_start = Instant::now();
        let cap = audio::record_for_seconds_with_rms(seconds, Some(cb))?;
        let elapsed_capture_ms = cap_start.elapsed().as_millis();

        let res_start = Instant::now();
        let samples_16k = audio::to_whisper_format(&cap.samples, cap.sample_rate, cap.channels)?;
        let elapsed_resample_ms = res_start.elapsed().as_millis();

        let tr_start = Instant::now();
        let text = transcriber.transcribe(&samples_16k, None, false)?;
        let elapsed_transcribe_ms = tr_start.elapsed().as_millis();

        Ok(TranscriptionResult {
            text,
            captured_samples: cap.samples.len(),
            capture_sample_rate: cap.sample_rate,
            capture_channels: cap.channels,
            capture_device: cap.device_name,
            elapsed_capture_ms,
            elapsed_resample_ms,
            elapsed_transcribe_ms,
        })
    })
    .await
    .map_err(|e| format!("background task panicked: {e}"))?
}

#[tauri::command]
fn recent_transcriptions(limit: i64, state: State<'_, AppState>) -> Result<Vec<Transcription>, String> {
    state.db.recent_transcriptions(limit.clamp(1, 1000))
}

#[tauri::command]
fn search_transcriptions(
    query: String,
    limit: i64,
    state: State<'_, AppState>,
) -> Result<Vec<Transcription>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return state.db.recent_transcriptions(limit.clamp(1, 1000));
    }
    // Sanitize for FTS5: wrap each token in quotes so users can type freely.
    let fts_query = trimmed
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ");
    state
        .db
        .search_transcriptions(&fts_query, limit.clamp(1, 1000))
}

#[tauri::command]
fn delete_transcription(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.db.delete_transcription(id)
}

#[tauri::command]
async fn reinsert_text(text: String, state: State<'_, AppState>) -> Result<(), String> {
    // Honor the user's injection-mode choice (clipboard vs per-keystroke).
    let keystroke = state.settings.get().injection_mode == "keystroke";
    // Run on a worker thread so the IPC reply doesn't block the UI while
    // we wait on the OS clipboard / SendInput round-trip.
    async_runtime::spawn_blocking(move || crate::injector::inject_text(&text, keystroke))
        .await
        .map_err(|e| format!("background task panicked: {e}"))?
}

/// Insert edited text from the edit-last bubble. The HUD was focused so the
/// user could type; here we hide it, restore focus to the app they were in
/// when they hit the edit hotkey, then inject.
#[tauri::command]
async fn insert_edited(
    text: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let keystroke = state.settings.get().injection_mode == "keystroke";
    let target = crate::controller::take_edit_target();
    if let Some(hud) = app.get_webview_window("hud") {
        let _ = hud.hide();
    }
    async_runtime::spawn_blocking(move || {
        if let Some(id) = target {
            crate::focus::refocus_window(id);
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        crate::injector::inject_text(&text, keystroke)
    })
    .await
    .map_err(|e| format!("background task panicked: {e}"))?
}

/// Hide the HUD window — used when the user cancels the edit-last bubble so
/// the (temporarily focusable) HUD releases focus.
#[tauri::command]
fn hide_hud_window(app: AppHandle) {
    if let Some(hud) = app.get_webview_window("hud") {
        let _ = hud.hide();
    }
    crate::controller::take_edit_target();
}

#[tauri::command]
fn transcription_count(state: State<'_, AppState>) -> Result<i64, String> {
    state.db.transcription_count()
}

#[derive(Serialize)]
struct UsageSummary {
    totals: UsageTotals,
    heatmap: Vec<DayCount>,
    top_words: Vec<WordCount>,
    top_fillers: Vec<WordCount>,
    total_fillers_removed: i64,
    top_phrases: Vec<PhraseCount>,
    hourly: [i64; 24],
    themes: Vec<ThemeMatch>,
    /// v0.1.43: 12-week weekly WPM trend (oldest → newest).
    weekly_wpm: Vec<WeekWpm>,
    /// v0.1.43: All-time longest dictation / longest duration / highest WPM.
    personal_records: PersonalRecords,
    /// v0.1.43: Top 5 apps by transcription count.
    app_breakdown: Vec<AppUsage>,
    /// v0.1.43: Month-over-month filler progress for the user's #1 filler.
    /// Null when there's not enough data yet to make a comparison.
    filler_progress: Option<FillerProgress>,
}

// ----- Dictionary -----

#[tauri::command]
fn list_dictionary(
    type_filter: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<DictionaryEntry>, String> {
    state.db.list_dictionary(type_filter.as_deref())
}

#[tauri::command]
fn create_dictionary_entry(
    entry_type: String,
    trigger: String,
    expansion: Option<String>,
    description: Option<String>,
    is_regex: bool,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    state.db.create_dictionary_entry(
        &entry_type,
        &trigger,
        expansion.as_deref(),
        description.as_deref(),
        is_regex,
    )
}

#[tauri::command]
fn update_dictionary_entry(
    id: i64,
    entry_type: String,
    trigger: String,
    expansion: Option<String>,
    description: Option<String>,
    is_regex: bool,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.db.update_dictionary_entry(
        id,
        &entry_type,
        &trigger,
        expansion.as_deref(),
        description.as_deref(),
        is_regex,
        enabled,
    )
}

#[tauri::command]
fn delete_dictionary_entry(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.db.delete_dictionary_entry(id)
}

// ----- Settings -----

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.get()
}

// ----- License -----
// Offline Ed25519 license verification. Enforcement is OFF right now (the
// app is free) — these commands expose status so a Settings panel can let a
// user enter/see a key, and so the gate can be flipped on later without any
// backend change. See `src/windows/main/App.tsx` (LICENSE_ENFORCED).

#[tauri::command]
fn license_status(state: State<'_, AppState>) -> license::LicenseStatus {
    let key = state.settings.get().license_key;
    license::validate(&key, &license::now_iso_z())
}

#[tauri::command]
fn set_license_key(
    key: String,
    state: State<'_, AppState>,
) -> Result<license::LicenseStatus, String> {
    let mut s = state.settings.get();
    s.license_key = key.trim().to_string();
    state.settings.replace(s)?;
    Ok(license_status(state))
}

#[tauri::command]
fn save_settings(new_settings: Settings, state: State<'_, AppState>) -> Result<(), String> {
    // Push hotkey changes to the live listener BEFORE persisting so the user
    // never observes a window where the saved string disagrees with the
    // bound key. Parsing failures keep the prior config silently — the
    // Settings page is responsible for surfacing invalid choices.
    let new_hotkeys = hotkey::config_from_strings(
        &new_settings.dictation_hotkey,
        &new_settings.repeat_hotkey,
        &new_settings.edit_last_hotkey,
        &new_settings.cancel_hotkey,
    );
    hotkey::update_config(new_hotkeys);
    hotkey::set_pause_during_fullscreen(new_settings.pause_during_fullscreen);

    state.settings.replace(new_settings)
}

// ----- Onboarding -----

#[tauri::command]
fn set_practice_mode(active: bool, state: State<'_, AppState>) {
    state.practice_mode.store(active, Ordering::Relaxed);
}

#[tauri::command]
fn complete_onboarding(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut s = state.settings.get();
    s.has_completed_onboarding = true;
    state.settings.replace(s)?;
    state.practice_mode.store(false, Ordering::Relaxed);

    if let Some(onboarding) = app.get_webview_window("onboarding") {
        let _ = onboarding.hide();
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
    Ok(())
}

#[tauri::command]
fn reset_onboarding(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut s = state.settings.get();
    s.has_completed_onboarding = false;
    state.settings.replace(s)?;
    state.practice_mode.store(false, Ordering::Relaxed);
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    if let Some(onboarding) = app.get_webview_window("onboarding") {
        let _ = onboarding.show();
        let _ = onboarding.set_focus();
    }
    Ok(())
}

// ----- Auto-launch / retention (Phase 9) -----

#[tauri::command]
async fn set_launch_at_login(
    enabled: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| format!("enable autostart: {e}"))?;
    } else {
        manager.disable().map_err(|e| format!("disable autostart: {e}"))?;
    }
    let mut s = state.settings.get();
    s.launch_at_login = enabled;
    state.settings.replace(s)?;
    Ok(())
}

#[tauri::command]
fn launch_at_login_active(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|e| format!("query autostart: {e}"))
}

/// Returns true while the controller has an active recording (hold-uncertain
/// or toggled state). Called by the HUD on mount so it can self-heal if it
/// missed the original Status::Recording event (cold-mount race, WebView
/// wake-from-suspend, etc) — without this it would render nothing until the
/// next state change, leaving the user with a sound + window but no pill.
#[tauri::command]
fn is_recording_active() -> bool {
    hotkey::is_recording_active()
}

/// Open a specific pane of System Settings → Privacy & Security on macOS.
/// `pane` should be one of: "microphone", "accessibility", "input-monitoring".
/// On non-macOS platforms this is a no-op (frontend only calls it when
/// platform === 'macos').
#[tauri::command]
#[allow(unused_variables)]
fn open_macos_pref_pane(pane: String, app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_opener::OpenerExt;
        // The `x-apple.systempreferences:` URL scheme is the documented way
        // to deep-link into a specific preference pane on macOS 13+. The
        // anchor (`?Privacy_X`) targets a specific row inside Privacy &
        // Security. macOS 14 renamed several anchors; we use the older
        // names because they still work on 13–15 (Apple kept aliases).
        let url = match pane.as_str() {
            "microphone" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
            "accessibility" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            "input-monitoring" => "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
            other => return Err(format!("unknown pref pane: {other}")),
        };
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|e| format!("open pref pane: {e}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Silently succeed on non-Mac so the frontend can call this
        // unconditionally without branching.
        Ok(())
    }
}

// ----- Permission checks (onboarding walkthrough) -----

/// Live status of the three macOS permissions Murmr needs. The onboarding
/// wizard polls these to show "Waiting… → Granted ✓" and auto-advance.
/// All return `NotApplicable` on Windows/Linux.
#[tauri::command]
fn check_microphone_permission() -> permissions::PermissionState {
    permissions::microphone_state()
}

#[tauri::command]
fn check_accessibility_permission() -> permissions::PermissionState {
    permissions::accessibility_state()
}

#[tauri::command]
fn check_input_monitoring_permission() -> permissions::PermissionState {
    permissions::input_monitoring_state()
}

/// Trigger the macOS microphone permission prompt by briefly opening the
/// input device. On first use macOS shows the "allow microphone" dialog;
/// the onboarding then polls `check_microphone_permission` until the user
/// answers. No-op-ish elsewhere (just confirms a device opens). Runs on a
/// blocking worker so the cpal round-trip doesn't stall the UI thread.
#[tauri::command]
async fn request_microphone_permission() -> permissions::PermissionState {
    let _ = async_runtime::spawn_blocking(|| {
        // Opening the default input device is what actually triggers the
        // TCC prompt. We don't care about the captured audio — start, let
        // the OS surface the dialog, then immediately stop.
        let recorder = audio::Recorder::new();
        if recorder.start_with_callbacks(None, None).is_ok() {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let _ = recorder.stop();
        }
    })
    .await;
    permissions::microphone_state()
}

/// Relaunch Murmr. macOS only applies newly-granted Accessibility / Input
/// Monitoring permissions to a process on restart, so the onboarding offers
/// a "Restart Murmr" button after the user enables those.
#[tauri::command]
fn restart_app(app: AppHandle) {
    app.restart();
}

#[tauri::command]
fn purge_older_transcriptions(state: State<'_, AppState>) -> Result<usize, String> {
    let days = state.settings.get().retention_days as i64;
    state.db.purge_older_than(days)
}

#[tauri::command]
fn clear_last_24_hours(state: State<'_, AppState>) -> Result<usize, String> {
    state.db.clear_last_24_hours()
}

#[tauri::command]
fn clear_all_transcriptions(state: State<'_, AppState>) -> Result<usize, String> {
    state.db.clear_all_transcriptions()
}

// ----- Audio devices -----

#[tauri::command]
fn list_input_devices() -> Result<Vec<InputDevice>, String> {
    audio::list_input_devices()
}

// ----- Open paths -----

#[derive(Serialize)]
struct AppPaths {
    db_path: String,
    settings_path: String,
    model_path: String,
    log_path: Option<String>,
}

#[tauri::command]
fn app_paths(app: AppHandle, state: State<'_, AppState>) -> Result<AppPaths, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app-data dir: {e}"))?;
    Ok(AppPaths {
        db_path: data_dir.join("murmr.db").to_string_lossy().to_string(),
        settings_path: state.settings.data_path().to_string_lossy().to_string(),
        model_path: resolve_model_path(&state.settings.get().model_name)
            .to_string_lossy()
            .to_string(),
        log_path: None,
    })
}

#[tauri::command]
fn open_app_data_folder(app: AppHandle) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app-data dir: {e}"))?;
    open_path_in_explorer(&dir)
}

#[tauri::command]
fn open_perf_log(app: AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app-data dir: {e}"))?
        .join("perf.log");
    if !path.exists() {
        // Make sure the file exists so the editor doesn't error.
        let _ = std::fs::write(&path, "(no transcriptions logged yet)\n");
    }
    open_path_in_explorer(&path)
}

#[tauri::command]
fn open_sounds_folder(app: AppHandle) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app-data dir: {e}"))?
        .join("sounds");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create sounds dir: {e}"))?;
    open_path_in_explorer(&dir)
}

#[cfg(target_os = "windows")]
fn open_path_in_explorer(path: &std::path::Path) -> Result<(), String> {
    // Folder names that look like a file extension (e.g. `app.murmr.desktop`)
    // confuse the Windows shell — without help, explorer treats the trailing
    // ".desktop" as a file extension and refuses to open. The /e, switch
    // forces Explorer to render the path as a folder regardless.
    let s = path.to_string_lossy().to_string();
    let arg = format!("/e,{}", s.trim_end_matches('\\'));
    std::process::Command::new("explorer.exe")
        .arg(arg)
        .spawn()
        .map_err(|e| format!("explorer: {e}"))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_path_in_explorer(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|e| format!("open: {e}"))?;
    Ok(())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn open_path_in_explorer(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| format!("xdg-open: {e}"))?;
    Ok(())
}

#[tauri::command]
fn usage_summary(state: State<'_, AppState>) -> Result<UsageSummary, String> {
    Ok(UsageSummary {
        totals: state.db.usage_totals()?,
        heatmap: state.db.heatmap_days(280)?,
        top_words: state.db.top_words(10)?,
        top_fillers: state.db.top_fillers(5)?,
        total_fillers_removed: state.db.total_fillers_removed()?,
        top_phrases: state.db.top_phrases(8)?,
        hourly: state.db.hourly_distribution()?,
        themes: state.db.topic_breakdown()?,
        // 12 weeks of WPM history gives a roughly-quarterly trend
        // without dominating the card visually.
        weekly_wpm: state.db.weekly_wpm_trend(12)?,
        personal_records: state.db.personal_records()?,
        // 5 apps is enough to see a meaningful breakdown without the
        // card feeling like a forensics dashboard.
        app_breakdown: state.db.app_breakdown(5)?,
        // 30-day comparison window — long enough to smooth daily
        // noise, short enough to feel responsive to behavior change.
        filler_progress: state.db.filler_progress(30)?,
    })
}

// ---------- Window + tray ----------

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Murmr", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause dictation", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Murmr", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&open, &pause, &separator, &quit])?;

    TrayIconBuilder::with_id("murmr-tray")
        .tooltip("Murmr")
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(handle_tray_event)
        .build(app)?;

    Ok(())
}

fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id.as_ref() {
        "open" => show_main_window(app),
        "pause" => {
            // Phase 9 wires actual pause/resume into the recording loop.
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}

fn handle_tray_event(tray: &tauri::tray::TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        show_main_window(tray.app_handle());
    }
}

// ---------- Model loading ----------

/// Bump our process to HIGH on Windows so Whisper's CPU threads stay
/// responsive when Murmr isn't the foreground window. Console-launched
/// processes (`cargo run`) inherit foreground priority for free; installed
/// background GUI apps don't, which can cost ~3-5× wall time on CPU-bound
/// work. No-op on macOS / Linux which schedule GUI apps fairly by default.
#[cfg(target_os = "windows")]
fn bump_process_priority() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, SetPriorityClass, HIGH_PRIORITY_CLASS,
    };
    unsafe {
        let _ = SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS);
    }
}

#[cfg(not(target_os = "windows"))]
fn bump_process_priority() {}

/// Resolve the user's app-data directory (where settings.json + the SQLite
/// DB live) **without** needing an `AppHandle`. We need this because we
/// open both BEFORE the Tauri builder runs — anything else races with the
/// webview's first IPC calls.
///
/// Mirrors Tauri's `path::BaseDirectory::AppData` resolution: identifier
/// ("app.murmr.desktop") joined under the platform's per-user data root.
fn resolve_app_data_dir() -> PathBuf {
    let identifier = "app.murmr.desktop";

    #[cfg(target_os = "windows")]
    {
        // Preferred: %APPDATA% (the canonical Roaming path on Windows).
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join(identifier);
        }
        // Fallback A: construct from %USERPROFILE%. We've observed
        // user-reported cases on Windows where the installed binary
        // launches without %APPDATA% propagated by Explorer, so it
        // would silently fall through to `current_dir()` and lose all
        // settings/perf/db writes. %USERPROFILE% is set in almost
        // every launch context (UAC-elevated processes, services,
        // logon scripts, normal Explorer launch) so this is a strong
        // backstop.
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(profile)
                .join("AppData")
                .join("Roaming")
                .join(identifier);
        }
        // Fallback B: %LOCALAPPDATA% sibling — almost always set even
        // when its Roaming counterpart isn't, and we can derive
        // Roaming from it.
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            if let Some(parent) = PathBuf::from(local).parent() {
                return parent.join("Roaming").join(identifier);
            }
        }
        // Fallback C (last-resort): next to the running exe. NOT ideal
        // (will be wiped on uninstall + can pollute Program Files
        // installs) but better than current_dir which can be
        // literally anywhere.
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                return dir.join(identifier);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library/Application Support")
                .join(identifier);
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join(identifier);
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".local/share").join(identifier);
        }
    }

    std::env::current_dir().unwrap_or_default().join(identifier)
}

/// Resolve the on-disk path of the bundled Whisper model **without** needing
/// an `AppHandle` — must be callable before the Tauri builder runs, since
/// loading the 147 MB model is slow and we don't want to delay
/// `app.manage(AppState{...})` (which would race with the frontend's first
/// IPC calls).
///
/// Production layout (verified empirically from a live install):
///   - Windows NSIS:  `<exe_dir>/models/ggml-base.en.bin`  (resources land
///                    directly next to the exe — NOT under `resources/`)
///   - Windows MSI:   `<exe_dir>/resources/models/...`     (older layout)
///   - macOS bundle:  `<exe_dir>/../Resources/models/...`
///
/// Dev fallback: `<CARGO_MANIFEST_DIR>/models/ggml-base.en.bin`.
fn resolve_model_path(model_name: &str) -> PathBuf {
    // Sanitize to a bare filename ending in .bin so a settings value can never
    // escape the models directory; fall back to the bundled base model.
    let safe = std::path::Path::new(model_name)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| s.ends_with(".bin"))
        .unwrap_or("ggml-base.en.bin");
    let model_rel_string = format!("models/{safe}");
    let model_rel: &str = &model_rel_string;
    let mut attempts: Vec<PathBuf> = Vec::new();

    let exe = std::env::current_exe().ok();
    perf_log::append(&format!("[model] current_exe = {:?}", exe));

    if let Some(exe) = exe.as_ref() {
        if let Some(exe_dir) = exe.parent() {
            // Tauri 2 NSIS (Windows): bundled resources land DIRECTLY beside
            // the exe — `<install_dir>/models/...`. This is the actual layout
            // produced by the v2 installer; check it first.
            let candidate = exe_dir.join(model_rel);
            attempts.push(candidate.clone());
            if candidate.exists() {
                perf_log::append(&format!("[model] using bundled (exe-adjacent): {candidate:?}"));
                return candidate;
            }
            // Tauri 2 MSI / older layouts: `<exe_dir>/resources/models/...`.
            let candidate = exe_dir.join("resources").join(model_rel);
            attempts.push(candidate.clone());
            if candidate.exists() {
                perf_log::append(&format!("[model] using bundled (resources/): {candidate:?}"));
                return candidate;
            }
            // Sometimes the resource ends up under a per-identifier subdir.
            let candidate = exe_dir
                .join("resources")
                .join("app.murmr.desktop")
                .join(model_rel);
            attempts.push(candidate.clone());
            if candidate.exists() {
                perf_log::append(&format!("[model] using bundled (id-scoped): {candidate:?}"));
                return candidate;
            }
            // macOS: exe in `Contents/MacOS/`, resources in `Contents/Resources/`.
            if let Some(macos_dir) = exe_dir.parent() {
                let candidate = macos_dir.join("Resources").join(model_rel);
                attempts.push(candidate.clone());
                if candidate.exists() {
                    perf_log::append(&format!("[model] using bundled (mac): {candidate:?}"));
                    return candidate;
                }
                // macOS sometimes nests the resource under the identifier.
                let candidate = macos_dir
                    .join("Resources")
                    .join("app.murmr.desktop")
                    .join(model_rel);
                attempts.push(candidate.clone());
                if candidate.exists() {
                    perf_log::append(&format!("[model] using bundled (mac id-scoped): {candidate:?}"));
                    return candidate;
                }
            }
        }
    }

    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(model_rel);
    perf_log::append(&format!(
        "[model] no bundled resource found (tried {:?}); falling back to dev path {dev_path:?}",
        attempts
    ));
    dev_path
}

/// Directory where Whisper model `.bin` files live (the folder containing the
/// bundled base model). Used to enumerate models the user can select and to
/// let them drop in a larger one (e.g. `ggml-small.en.bin`).
fn models_dir() -> Option<PathBuf> {
    resolve_model_path("ggml-base.en.bin")
        .parent()
        .map(|p| p.to_path_buf())
}

/// List the Whisper models available to select — every `ggml-*.bin` in the
/// models directory. Powers the model picker in Advanced settings.
#[tauri::command]
fn list_models() -> Vec<String> {
    let Some(dir) = models_dir() else {
        return vec!["ggml-base.en.bin".into()];
    };
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.starts_with("ggml-") && n.ends_with(".bin"))
                .collect()
        })
        .unwrap_or_default();
    if names.is_empty() {
        names.push("ggml-base.en.bin".into());
    }
    names.sort();
    names
}

/// Load the Transcriber for the user's selected model, falling back to the
/// bundled base model if the selection can't be loaded (missing file, etc).
fn load_transcriber(model_name: &str) -> Result<Transcriber, String> {
    let path = resolve_model_path(model_name);
    match Transcriber::new(&path.to_string_lossy()) {
        Ok(t) => Ok(t),
        Err(e) if model_name != "ggml-base.en.bin" => {
            perf_log::append(&format!(
                "[model] selected model '{model_name}' failed to load ({e}); falling back to base.en"
            ));
            let base = resolve_model_path("ggml-base.en.bin");
            Transcriber::new(&base.to_string_lossy())
        }
        Err(e) => Err(e),
    }
}

// ---------- Entry point ----------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Force OpenMP to actually parallelize. MSVC's OpenMP 2.0 runtime
    // (vcomp140.dll) frequently defaults to 1 thread even when the
    // `#pragma omp parallel num_threads(N)` clause asks for more — at
    // which point whisper.cpp's `n_threads = omp_get_num_threads();`
    // call collapses the whole pipeline to single-threaded execution.
    // Setting these two env vars BEFORE any OpenMP runtime init gives
    // us the parallel behaviour we expect. Must run before the first
    // Whisper context is created.
    if std::env::var_os("OMP_NUM_THREADS").is_none() {
        // 8 matches our FullParams::set_n_threads(8) cap. Going higher on a
        // 12-core / 24-thread Ryzen doesn't help because Whisper's matmul
        // shapes saturate around 8 threads (more threads = synchronisation
        // overhead).
        std::env::set_var("OMP_NUM_THREADS", "8");
    }
    if std::env::var_os("OMP_DYNAMIC").is_none() {
        std::env::set_var("OMP_DYNAMIC", "FALSE");
    }

    // Bump our process priority so Whisper's CPU threads don't get throttled
    // when Murmr isn't the foreground window.
    bump_process_priority();

    // Resolve the app-data dir without an AppHandle so we can open DB +
    // settings + perf log BEFORE the Tauri builder. Anything left to setup()
    // runs concurrent with the WebView, which races against the React app's
    // first IPC calls.
    let app_data_dir = resolve_app_data_dir();
    perf_log::init(app_data_dir.clone());
    perf_log::append("[startup] Murmr launching");
    perf_log::append(&format!("[startup] app_data_dir = {:?}", app_data_dir));
    perf_log::append(&format!(
        "[startup] version={} target_os={}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
    ));
    // Launch-context env diagnostics. Useful when troubleshooting
    // "Murmr is running but settings/db/perf aren't writing" — that
    // turns out to be `%APPDATA%` not being propagated by the launching
    // process (NSIS shortcut, Explorer, scheduled task, etc). Log
    // presence (not values — these are paths, not secrets, but keeping
    // the line short).
    #[cfg(target_os = "windows")]
    perf_log::append(&format!(
        "[startup] env present: APPDATA={} USERPROFILE={} LOCALAPPDATA={}",
        std::env::var_os("APPDATA").is_some(),
        std::env::var_os("USERPROFILE").is_some(),
        std::env::var_os("LOCALAPPDATA").is_some(),
    ));

    // Install a panic hook so unexpected panics on background threads —
    // audio callbacks, hotkey grab thread, controller, transcribe worker —
    // leave a footprint in perf.log instead of vanishing into a silent
    // process abort. With `panic = "abort"` in Cargo.toml the process dies
    // immediately after the hook runs, so this is our only chance to
    // capture diagnostic info AND restore any audio sessions we ducked
    // (otherwise the user's per-app volumes stay stuck low after a crash).
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "<unknown>".into());
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<no message>");
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_string();
        perf_log::append(&format!(
            "[panic] thread={thread} location={location} msg={msg:?}"
        ));
        // Best-effort recovery before abort. Wrapped in catch_unwind so a
        // panic INSIDE the hook (e.g. COM in a bad state) doesn't deadlock
        // or trigger a recursive abort.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            audio_duck::unduck();
        }));
    }));
    perf_log::append(&format!(
        "[startup] OMP_NUM_THREADS={} OMP_DYNAMIC={}",
        std::env::var("OMP_NUM_THREADS").unwrap_or_default(),
        std::env::var("OMP_DYNAMIC").unwrap_or_default()
    ));

    // Install ggml/whisper log capture so the model's CPU-feature detection
    // and threading info land in perf.log alongside our own measurements.
    transcribe::install_log_hook();
    perf_log::append(&format!(
        "[startup] system_info: {}",
        transcribe::whisper_system_info()
    ));

    // Open DB + settings (~ms each on a warm disk).
    let db = Db::open(&app_data_dir).expect("failed to open Murmr database");
    let settings = SettingsStore::open(&app_data_dir).expect("failed to open Murmr settings store");

    // Load the Whisper model (~2-5 s). Uses the user's selected model, with an
    // automatic fallback to the bundled base model if the selection can't load.
    let selected_model = settings.get().model_name;
    perf_log::append(&format!("[startup] loading Whisper model '{selected_model}'"));
    println!("[murmr] loading Whisper model '{selected_model}'");
    let transcriber = std::sync::Arc::new(
        load_transcriber(&selected_model).unwrap_or_else(|e| {
            panic!(
                "failed to load Whisper model '{selected_model}': {e}\n\
                 If running from source, did you run\n\
                 `curl -L -o src-tauri/models/ggml-base.en.bin \
                 https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin`?"
            )
        }),
    );

    // Pre-construct the practice-mode flag so AppState (managed at builder
    // time) and the Controller (built in setup) reference the same atomic.
    let practice_mode = Arc::new(AtomicBool::new(false));

    let app_state = AppState {
        transcriber: Arc::clone(&transcriber),
        db: Arc::clone(&db),
        settings: Arc::clone(&settings),
        practice_mode: Arc::clone(&practice_mode),
    };
    perf_log::append("[startup] state ready, starting Tauri builder");

    let builder = tauri::Builder::default()
        // Single-instance guard MUST be the first plugin registered so it
        // runs before anything else touches global state. When a second
        // Murmr launches, this plugin hands the new process's argv to the
        // already-running instance (firing the callback below) and the
        // second process exits during builder init — crucially BEFORE
        // setup() runs, so it never installs a second keyboard hook or a
        // second audio-duck manager. That double-hook + duck race was the
        // cause of "audio ducks and never comes back" after an accidental
        // double-launch (autostart entry + manual launch, etc).
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            perf_log::append(
                "[startup] second instance launch detected — surfacing existing window, second process will exit",
            );
            // We're the FIRST (surviving) instance. Bring our main window
            // forward so the user sees Murmr is already running rather
            // than wondering why their double-click did nothing.
            show_main_window(app);
        }))
        // Manage state at BUILDER time so it's registered before any window
        // exists. Doing this in setup() races with WebView load — IPC calls
        // can fire before setup() returns.
        .manage(app_state)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));

    // Auto-updater. Enabled on both platforms now that macOS builds are
    // signed with a stable Developer ID Application identity (Team 8Y9C2L5SW5)
    // and notarized. Previously disabled on macOS: ad-hoc-signed rebuilds each
    // produced a new cdhash, so macOS silently invalidated the user's
    // Accessibility + Input Monitoring grants on every update. With a Developer
    // ID signature the app's designated requirement is keyed to the (stable)
    // team + bundle identifier instead of the per-build cdhash, so TCC grants
    // now persist across updates and self-updating is safe.
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            // AppState is already managed via Builder::manage() above.
            // Anything in this setup() block runs concurrent with WebView
            // load — keep stuff that the React app needs out of here.

            // macOS: run as a menu-bar "accessory" app — no Dock icon, no
            // app-switcher entry, lives entirely in the top menu bar (tray)
            // and its windows. We set this at runtime (not just via
            // Info.plist's LSUIElement) because the implicit plist merge into
            // the built bundle isn't guaranteed, whereas this call always
            // takes effect. Windows have their own taskbar behavior and are
            // unaffected.
            #[cfg(target_os = "macos")]
            {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            build_tray(app.handle())?;

            // Wire global hotkey → controller → recording loop. Bindings come
            // from Settings (live-updated by save_settings()), so the user
            // can rebind without restarting Murmr.
            let (hotkey_tx, hotkey_rx) = crossbeam_channel::unbounded();
            let initial_hotkeys = {
                let s = settings.get();
                hotkey::set_pause_during_fullscreen(s.pause_during_fullscreen);
                hotkey::config_from_strings(
                    &s.dictation_hotkey,
                    &s.repeat_hotkey,
                    &s.edit_last_hotkey,
                    &s.cancel_hotkey,
                )
            };
            hotkey::spawn(hotkey_tx, initial_hotkeys);
            let sounds = SoundPlayer::new(Arc::clone(&settings), app.handle());

            let controller = Controller::new(
                Arc::clone(&transcriber),
                Arc::clone(&db),
                Arc::clone(&settings),
                Arc::clone(&sounds),
                app.handle().clone(),
                Arc::clone(&practice_mode),
            );
            controller.spawn(hotkey_rx);

            let s = settings.get();
            println!(
                "[murmr] hotkey listener active — dictation={} cancel={} (re-bind in Settings → Hotkeys)",
                s.dictation_hotkey, s.cancel_hotkey
            );

            // Retention purge — runs once per launch per plan §11 #32.
            let retention_days = settings.get().retention_days as i64;
            if retention_days > 0 {
                match db.purge_older_than(retention_days) {
                    Ok(n) if n > 0 => {
                        println!("[db] retention purge dropped {n} transcription(s) older than {retention_days} days");
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("[db] retention purge failed: {e}"),
                }
            }

            // First-run dispatch: open the onboarding wizard if the user
            // hasn't finished it; otherwise reveal the main window.
            let has_onboarded = settings.get().has_completed_onboarding;
            if has_onboarded {
                if let Some(main) = app.get_webview_window("main") {
                    let _ = main.show();
                }
            } else {
                if let Some(onboarding) = app.get_webview_window("onboarding") {
                    let _ = onboarding.show();
                    let _ = onboarding.set_focus();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" || window.label() == "onboarding" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            user_info,
            record_and_transcribe,
            recent_transcriptions,
            search_transcriptions,
            delete_transcription,
            reinsert_text,
            transcription_count,
            usage_summary,
            list_dictionary,
            create_dictionary_entry,
            update_dictionary_entry,
            delete_dictionary_entry,
            get_settings,
            save_settings,
            license_status,
            set_license_key,
            list_input_devices,
            app_paths,
            open_app_data_folder,
            open_sounds_folder,
            open_perf_log,
            complete_onboarding,
            reset_onboarding,
            set_practice_mode,
            set_launch_at_login,
            launch_at_login_active,
            open_macos_pref_pane,
            is_recording_active,
            check_microphone_permission,
            check_accessibility_permission,
            check_input_monitoring_permission,
            request_microphone_permission,
            restart_app,
            purge_older_transcriptions,
            clear_last_24_hours,
            clear_all_transcriptions,
            list_models,
            insert_edited,
            hide_hud_window,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            match event {
                // Intercept the window-close button. On macOS we run as an
                // LSUIElement (no Dock icon), so closing the main window
                // would otherwise also remove Murmr's only visible
                // presence besides the menu-bar tray icon — which feels
                // like a quit even though we're still running. Hiding
                // instead is what every Mac menu-bar app does (Slack tray
                // mode, WhisperFlow, Maccy, etc). The user explicitly
                // quits via the tray icon's "Quit Murmr" item, which
                // triggers RunEvent::Exit below.
                //
                // We apply the same close-to-hide behavior on Windows for
                // consistency with the tray-icon model; clicking the
                // tray icon brings the window back.
                tauri::RunEvent::WindowEvent {
                    label,
                    event: tauri::WindowEvent::CloseRequested { api, .. },
                    ..
                } if label == "main" => {
                    api.prevent_close();
                    if let Some(main) = _app.get_webview_window("main") {
                        let _ = main.hide();
                    }
                    perf_log::append("[lifecycle] main window close intercepted → hidden (use tray to quit)");
                }

                // Restore any audio sessions Murmr left ducked. Without this,
                // quitting Murmr mid-recording — or even after a clean stop, if
                // the controller's unduck path was skipped for any reason —
                // strands per-app volumes at the ducked level until the user
                // notices and fixes them by hand. Fires on graceful exit (tray
                // quit, window close, OS shutdown).
                tauri::RunEvent::Exit => {
                    perf_log::append("[shutdown] RunEvent::Exit → unducking audio sessions");
                    audio_duck::unduck();
                }

                _ => {}
            }
        });
}
