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

    /// Signed license key, base64url `<payload>.<signature>`. Empty = none.
    /// Verified offline against the build-time public key (see
    /// `license::validate`). Enforcement is currently a no-op — the app is
    /// free — but the key is stored so the gate can be switched on later.
    pub license_key: String,

    pub microphone_device: Option<String>,
    pub microphone_gain_db: f32,
    pub noise_suppression: bool,

    /// How aggressively to lower per-app audio sessions while recording.
    /// 0.0 = no ducking, 1.0 = mute. Defaults to 0.8 (80% reduction) — at
    /// that level background music drops to a murmur but Murmr's own start
    /// chime still cuts through (we exclude our own session from the duck).
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
    pub hud_position: String,

    pub sound_start_click: bool,
    pub sound_complete_chime: bool,
    pub sound_error_beep: bool,
    /// Master volume scalar applied to ALL Murmr UI sounds (start, stop,
    /// error). 0.0 = silent, 1.0 = file's native level. Default 0.7 so the
    /// embedded WAVs don't blast at full level on first launch.
    pub sound_volume: f32,

    pub auto_capitalize: bool,
    pub auto_period: bool,
    pub strip_fillers: bool,
    /// When you say "One. Item one. Two. Item two. Three. Item three." in
    /// a single dictation, automatically reformat it as a numbered list.
    /// Markers must be in strict order starting from 1; sequences that
    /// break (e.g. mid-prose mentions of numbers) are left alone.
    pub auto_numbered_lists: bool,
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

    /// When true, fire OS notifications for milestone events (100th
    /// transcription, week streaks, personal bests). Notifications are
    /// always rare + meaningful — see `notifications.rs` for the catalog.
    /// Off → notifications are computed but never shown.
    #[serde(default = "default_true")]
    pub milestone_notifications: bool,

    /// When true, ignore the dictation hotkey while a fullscreen app
    /// (game, video, presentation) has focus. Prevents (a) the stuck-
    /// hotkey state where fullscreen exclusive games eat the key
    /// release, (b) accidental in-game triggers, (c) Murmr's global
    /// keyboard hook looking like macroing software to anti-cheat
    /// systems on the keypress side. The hook itself stays installed
    /// — only the dictation match is suppressed — so alt-tabbing out
    /// resumes normal behavior instantly. Default ON.
    #[serde(default = "default_true")]
    pub pause_during_fullscreen: bool,

    // ---- Developer-grade formatting ----
    /// Master "type exactly what I said" switch. When true, the whole
    /// post-processing pipeline (self-corrections, filler-strip, voice
    /// commands, capitalization, lists, dictionary) is bypassed and the
    /// raw transcript is injected verbatim. Off by default.
    #[serde(default)]
    pub literal_mode: bool,
    /// Detect spoken bulleted lists ("bullet … bullet …") and format them as
    /// a real "- " list, mirroring numbered lists.
    #[serde(default = "default_true")]
    pub auto_bulleted_lists: bool,
    /// Recognize spoken code/prose symbols as literal punctuation:
    /// "colon" → :, "semicolon" → ;, "open/close paren" → ( ), "backtick"
    /// → `, "hyphen" → -. Off by default because these words appear in
    /// normal speech; developers opt in.
    #[serde(default)]
    pub voice_command_symbols: bool,
    /// Insert a separating space (or newline) between back-to-back
    /// dictations so consecutive bursts don't butt together
    /// ("…the manifest.Next I'll…"). On by default.
    #[serde(default = "default_true")]
    pub smart_spacing: bool,
    /// Intelligently format spoken-but-unmarked lists: an enumerative lead-in
    /// followed by a comma/"and" series of items ("I need to buy milk, eggs,
    /// and bread") becomes a clean bulleted (or numbered) list — no need to
    /// say "one, two" or "bullet". Conservative so ordinary prose is left
    /// alone. On by default.
    #[serde(default = "default_true")]
    pub auto_smart_lists: bool,

    // ---- Dictionary trust ----
    /// After transcription, fuzzy-correct near-miss tokens against your
    /// enabled "word" dictionary entries (proper nouns / brands) so a name
    /// the model almost got is snapped to the intended spelling. OFF by
    /// default: because it rewrites any close token, it can occasionally
    /// catch a real word that happens to resemble one of your entries — so
    /// it's opt-in for people who add names and want them auto-fixed.
    #[serde(default)]
    pub fuzzy_dictionary: bool,

    // ---- Model / accuracy ----
    /// Filename of the Whisper model to load, resolved inside the app's
    /// models directory (e.g. "ggml-base.en.bin", "ggml-small.en.bin").
    /// Lets users drop in a larger model and select it without a rebuild.
    #[serde(default = "default_model_name")]
    pub model_name: String,
    /// Trade speed for accuracy: use beam-search decoding instead of the
    /// fast greedy path. Noticeably better on jargon/accents, a bit slower.
    /// Off by default.
    #[serde(default)]
    pub accuracy_mode: bool,

    // ---- Streamer mode ----
    /// When true: hide the HUD from screen capture (OBS etc.) and suppress
    /// milestone/desktop notifications so nothing Murmr-related leaks onto
    /// a broadcast. Off by default.
    #[serde(default)]
    pub streamer_mode: bool,
    /// While streamer mode is on, also mute Murmr's own start/stop/error
    /// chimes (so they don't play over captured audio). Off by default.
    #[serde(default)]
    pub streamer_mode_mute_chimes: bool,

    // ---- Edit-last ----
    /// Standalone hotkey that pops the most recent transcript into an
    /// editable HUD bubble so a single mis-heard word can be fixed and
    /// re-injected. Empty string disables it. Same name format as
    /// `dictation_hotkey`.
    #[serde(default)]
    pub edit_last_hotkey: String,
}

fn default_true() -> bool {
    true
}

fn default_model_name() -> String {
    "ggml-base.en.bin".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: "auto".into(),
            launch_at_login: false,
            license_key: String::new(),

            microphone_device: None,
            microphone_gain_db: 0.0,
            noise_suppression: false,

            audio_duck_amount: 0.8,

            tap_threshold_ms: 250,

            dictation_hotkey: "ControlRight".into(),
            repeat_hotkey: String::new(), // disabled by default — opt-in
            cancel_hotkey: "Escape".into(),

            hud_show_waveform: true,
            hud_show_timer: true,
            hud_position: "near-input".into(),

            sound_start_click: true,
            sound_complete_chime: true,
            sound_error_beep: true,
            sound_volume: 0.7,

            auto_capitalize: true,
            auto_period: true,
            strip_fillers: true,
            auto_numbered_lists: true,
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
            milestone_notifications: true,
            pause_during_fullscreen: true,

            literal_mode: false,
            auto_bulleted_lists: true,
            voice_command_symbols: false,
            smart_spacing: true,
            auto_smart_lists: true,

            fuzzy_dictionary: true,

            model_name: default_model_name(),
            accuracy_mode: false,

            streamer_mode: false,
            streamer_mode_mute_chimes: false,

            edit_last_hotkey: String::new(),
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
