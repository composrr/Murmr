//! Persistent user settings.
//!
//! Plain JSON file at `<app-data>/settings.json`. Schema is the
//! `Settings` struct below (serde-tagged so we can evolve it). Plan §7
//! calls for `tauri-plugin-store`; this is a tiny in-house equivalent
//! that avoids a dep until we need anything store-specific.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: String,
    pub launch_at_login: bool,

    pub microphone_device: Option<String>,
    pub microphone_gain_db: f32,
    pub noise_suppression: bool,

    /// How aggressively to lower the system master volume while recording.
    /// 0.0 = no ducking, 1.0 = mute. Defaults to 0.3 (30% reduction) which
    /// is loud enough that the start/stop chimes still cut through but
    /// quiet enough that background music/video gets out of the way.
    pub audio_duck_amount: f32,

    pub tap_threshold_ms: u32,

    /// Primary dictation hotkey. Stored as the rdev::Key debug name
    /// (e.g. "ControlRight", "F8", "CapsLock"). Parser lives in
    /// `hotkey::parse_key`. An invalid string falls back to ControlRight.
    pub dictation_hotkey: String,
    /// Standalone hotkey that re-injects the most recent transcription.
    /// Empty string disables it. Same name format as `dictation_hotkey`
    /// (e.g. "F9", "RightBracket"). Doesn't have to be related to the
    /// dictation key — pick anything you like.
    pub repeat_hotkey: String,
    /// Key that cancels an in-flight recording.
    pub cancel_hotkey: String,

    pub hud_show_waveform: bool,
    pub hud_show_timer: bool,
    pub hud_show_word_count: bool,
    pub hud_position: String,

    pub sound_start_click: bool,
    pub sound_complete_chime: bool,
    pub sound_error_beep: bool,

    pub auto_capitalize: bool,
    pub auto_period: bool,
    pub strip_fillers: bool,
    pub voice_command_period: bool,
    pub voice_command_comma: bool,
    pub voice_command_question: bool,
    pub voice_command_exclamation: bool,
    pub voice_command_new_line: bool,
    pub voice_command_new_paragraph: bool,
    pub filler_words: Vec<String>,

    pub retention_days: i32, // 0 = forever; otherwise N days

    pub injection_mode: String, // "clipboard" | "keystroke"
    pub log_level: String,      // "error" | "warn" | "info" | "debug" | "trace"
    pub force_cpu: bool,

    /// Set to true after the user finishes the first-run onboarding wizard.
    /// Defaults false on a fresh settings file; the absence of the file
    /// (first run) and presence-with-false both trigger the wizard.
    pub has_completed_onboarding: bool,

    /// What the user wants to be called in greetings. Empty falls back to
    /// the OS user's display name.
    pub display_name: String,

    /// User-entered license key. Empty until first paid activation; the
    /// `license` module validates this against the bundled Ed25519 public
    /// key on every startup. Empty / invalid → paywall screen.
    pub license_key: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: "auto".into(),
            launch_at_login: false,

            microphone_device: None,
            microphone_gain_db: 0.0,
            noise_suppression: false,

            audio_duck_amount: 0.3,

            tap_threshold_ms: 250,

            dictation_hotkey: "ControlRight".into(),
            repeat_hotkey: String::new(), // disabled by default — opt-in
            cancel_hotkey: "Escape".into(),

            hud_show_waveform: true,
            hud_show_timer: true,
            hud_show_word_count: true,
            hud_position: "near-input".into(),

            sound_start_click: true,
            sound_complete_chime: true,
            sound_error_beep: true,

            auto_capitalize: true,
            auto_period: true,
            strip_fillers: true,
            voice_command_period: true,
            voice_command_comma: true,
            voice_command_question: true,
            voice_command_exclamation: true,
            voice_command_new_line: true,
            voice_command_new_paragraph: true,
            filler_words: vec![
                "um".into(),
                "uh".into(),
                "er".into(),
                "ah".into(),
                "mhm".into(),
                "hmm".into(),
            ],

            retention_days: 0,

            injection_mode: "clipboard".into(),
            log_level: "info".into(),
            force_cpu: false,

            has_completed_onboarding: false,
            display_name: String::new(),
            license_key: String::new(),
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    cache: Mutex<Settings>,
}

impl SettingsStore {
    pub fn open(app_data_dir: &Path) -> Result<Arc<Self>, String> {
        std::fs::create_dir_all(app_data_dir)
            .map_err(|e| format!("create app-data dir: {e}"))?;
        let path = app_data_dir.join("settings.json");

        let settings = if path.exists() {
            let raw = fs::read_to_string(&path).map_err(|e| format!("read settings: {e}"))?;
            serde_json::from_str::<Settings>(&raw).unwrap_or_default()
        } else {
            Settings::default()
        };

        Ok(Arc::new(Self {
            path,
            cache: Mutex::new(settings),
        }))
    }

    pub fn get(&self) -> Settings {
        self.cache.lock().clone()
    }

    pub fn replace(&self, new_settings: Settings) -> Result<(), String> {
        let serialized = serde_json::to_string_pretty(&new_settings)
            .map_err(|e| format!("serialize settings: {e}"))?;
        fs::write(&self.path, serialized).map_err(|e| format!("write settings: {e}"))?;
        *self.cache.lock() = new_settings;
        Ok(())
    }

    pub fn data_path(&self) -> &PathBuf {
        &self.path
    }
}
