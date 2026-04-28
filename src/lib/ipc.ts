import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface PingResponse {
  message: string;
  version: string;
}

export interface UserInfo {
  display_name: string;
  raw_name: string;
}

export function userInfo(): Promise<UserInfo> {
  return invoke<UserInfo>('user_info');
}

export interface TranscriptionResult {
  text: string;
  captured_samples: number;
  capture_sample_rate: number;
  capture_channels: number;
  capture_device: string;
  elapsed_capture_ms: number;
  elapsed_resample_ms: number;
  elapsed_transcribe_ms: number;
}

export interface Transcription {
  id: number;
  text: string;
  word_count: number;
  duration_ms: number;
  target_app: string | null;
  created_at: number;
}

export type DictationStatus =
  | { kind: 'idle' }
  | { kind: 'recording' }
  | { kind: 'transcribing' }
  | { kind: 'injected'; text: string; source_app: string | null }
  | { kind: 'cancelled' }
  | { kind: 'error'; message: string };

export function ping(): Promise<PingResponse> {
  return invoke<PingResponse>('ping');
}

export function recordAndTranscribe(seconds: number): Promise<TranscriptionResult> {
  return invoke<TranscriptionResult>('record_and_transcribe', { seconds });
}

export function recentTranscriptions(limit = 200): Promise<Transcription[]> {
  return invoke<Transcription[]>('recent_transcriptions', { limit });
}

export function searchTranscriptions(query: string, limit = 200): Promise<Transcription[]> {
  return invoke<Transcription[]>('search_transcriptions', { query, limit });
}

export function deleteTranscription(id: number): Promise<void> {
  return invoke<void>('delete_transcription', { id });
}

export function reinsertText(text: string): Promise<void> {
  return invoke<void>('reinsert_text', { text });
}

export function transcriptionCount(): Promise<number> {
  return invoke<number>('transcription_count');
}

export interface UsageSummary {
  totals: {
    total_transcriptions: number;
    total_words: number;
    total_speech_ms: number;
    current_streak: number;
    longest_streak: number;
  };
  heatmap: Array<{ day: number; count: number }>;
  top_words: Array<{ word: string; count: number }>;
  top_fillers: Array<{ word: string; count: number }>;
  total_fillers_removed: number;
  top_phrases: Array<{ phrase: string; count: number }>;
  hourly: number[];
  themes: Array<{
    theme: string;
    label: string;
    transcription_count: number;
    sample_words: string[];
  }>;
}

export function usageSummary(): Promise<UsageSummary> {
  return invoke<UsageSummary>('usage_summary');
}

// ----- Dictionary -----

export type DictionaryType = 'word' | 'replacement' | 'snippet';

export interface DictionaryEntry {
  id: number;
  type: DictionaryType;
  trigger: string;
  expansion: string | null;
  description: string | null;
  is_regex: boolean;
  enabled: boolean;
  created_at: number;
  updated_at: number;
}

export function listDictionary(typeFilter?: DictionaryType): Promise<DictionaryEntry[]> {
  return invoke<DictionaryEntry[]>('list_dictionary', { typeFilter: typeFilter ?? null });
}

export function createDictionaryEntry(args: {
  entry_type: DictionaryType;
  trigger: string;
  expansion?: string | null;
  description?: string | null;
  is_regex?: boolean;
}): Promise<number> {
  return invoke<number>('create_dictionary_entry', {
    entryType: args.entry_type,
    trigger: args.trigger,
    expansion: args.expansion ?? null,
    description: args.description ?? null,
    isRegex: args.is_regex ?? false,
  });
}

export function updateDictionaryEntry(args: {
  id: number;
  entry_type: DictionaryType;
  trigger: string;
  expansion?: string | null;
  description?: string | null;
  is_regex?: boolean;
  enabled?: boolean;
}): Promise<void> {
  return invoke<void>('update_dictionary_entry', {
    id: args.id,
    entryType: args.entry_type,
    trigger: args.trigger,
    expansion: args.expansion ?? null,
    description: args.description ?? null,
    isRegex: args.is_regex ?? false,
    enabled: args.enabled ?? true,
  });
}

export function deleteDictionaryEntry(id: number): Promise<void> {
  return invoke<void>('delete_dictionary_entry', { id });
}

// ----- Settings -----

export interface Settings {
  appearance: string;
  launch_at_login: boolean;
  microphone_device: string | null;
  microphone_gain_db: number;
  noise_suppression: boolean;
  /** 0.0–1.0 — amount to dim system master volume while recording.
   * 0 disables ducking. Default 0.3. */
  audio_duck_amount: number;
  tap_threshold_ms: number;
  /** rdev key name for the dictation hotkey (e.g. "ControlRight", "F8"). */
  dictation_hotkey: string;
  /** rdev key name for the standalone re-paste hotkey. Empty = disabled. */
  repeat_hotkey: string;
  /** rdev key name for the cancel-recording key (e.g. "Escape"). */
  cancel_hotkey: string;
  hud_show_waveform: boolean;
  hud_show_timer: boolean;
  hud_show_word_count: boolean;
  hud_position: string;
  sound_start_click: boolean;
  sound_complete_chime: boolean;
  sound_error_beep: boolean;
  auto_capitalize: boolean;
  auto_period: boolean;
  strip_fillers: boolean;
  voice_command_period: boolean;
  voice_command_comma: boolean;
  voice_command_question: boolean;
  voice_command_exclamation: boolean;
  voice_command_new_line: boolean;
  voice_command_new_paragraph: boolean;
  filler_words: string[];
  retention_days: number;
  injection_mode: string;
  log_level: string;
  force_cpu: boolean;
  has_completed_onboarding: boolean;
  display_name: string;
  license_key: string;
}

// ----- License -----

export type LicenseStatus =
  | { kind: 'missing' }
  | { kind: 'malformed'; reason: string }
  | { kind: 'bad-signature' }
  | { kind: 'expired'; email: string; expired_at: string }
  | { kind: 'valid'; email: string; tier: string | null; expires_at: string | null };

export function getLicenseStatus(): Promise<LicenseStatus> {
  return invoke<LicenseStatus>('license_status');
}

export function setLicenseKey(key: string): Promise<LicenseStatus> {
  return invoke<LicenseStatus>('set_license_key', { key });
}

export function getSettings(): Promise<Settings> {
  return invoke<Settings>('get_settings');
}

export function saveSettings(s: Settings): Promise<void> {
  return invoke<void>('save_settings', { newSettings: s });
}

// ----- Audio devices -----

export interface InputDevice {
  name: string;
  is_default: boolean;
}

export function listInputDevices(): Promise<InputDevice[]> {
  return invoke<InputDevice[]>('list_input_devices');
}

// ----- Paths / files -----

export interface AppPaths {
  db_path: string;
  settings_path: string;
  model_path: string;
  log_path: string | null;
}

export function appPaths(): Promise<AppPaths> {
  return invoke<AppPaths>('app_paths');
}

export function openAppDataFolder(): Promise<void> {
  return invoke<void>('open_app_data_folder');
}

export function openSoundsFolder(): Promise<void> {
  return invoke<void>('open_sounds_folder');
}

export function openPerfLog(): Promise<void> {
  return invoke<void>('open_perf_log');
}

// ----- Onboarding -----

export function completeOnboarding(): Promise<void> {
  return invoke<void>('complete_onboarding');
}

export function resetOnboarding(): Promise<void> {
  return invoke<void>('reset_onboarding');
}

export function setPracticeMode(active: boolean): Promise<void> {
  return invoke<void>('set_practice_mode', { active });
}

// ----- Auto-launch / retention -----

export function setLaunchAtLogin(enabled: boolean): Promise<void> {
  return invoke<void>('set_launch_at_login', { enabled });
}

export function launchAtLoginActive(): Promise<boolean> {
  return invoke<boolean>('launch_at_login_active');
}

export function purgeOlderTranscriptions(): Promise<number> {
  return invoke<number>('purge_older_transcriptions');
}

export function clearLast24Hours(): Promise<number> {
  return invoke<number>('clear_last_24_hours');
}

export function clearAllTranscriptions(): Promise<number> {
  return invoke<number>('clear_all_transcriptions');
}

export function listenStatus(handler: (status: DictationStatus) => void): Promise<UnlistenFn> {
  return listen<DictationStatus>('murmr:status', (event) => handler(event.payload));
}

export function listenTranscriptionSaved(handler: () => void): Promise<UnlistenFn> {
  return listen<null>('murmr:transcription-saved', () => handler());
}
