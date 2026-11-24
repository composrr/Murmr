/// Browser-only preview of the main window with sample data.
///
/// Loaded by `src/main.tsx` only when `import.meta.env.DEV` is true AND the
/// page is not running inside Tauri (e.g. `npm run dev`, then open
/// http://localhost:1420 in a normal browser). It never ships in a build.

import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import type {
  DictionaryEntry,
  DictionarySuggestion,
  Settings,
  Transcription,
  UsageSummary,
} from '../lib/ipc';

const DAY = 86_400_000;
const now = Date.now();

function seeded(seed: number) {
  let s = seed;
  return () => {
    s = (s * 16807) % 2147483647;
    return (s - 1) / 2147483646;
  };
}

const SAMPLE_TEXT = [
  'Can you send me the final cut of the trailer by Thursday? I want to review the colour grade before we lock it.',
  'Quick reminder to update the Tauri build script so the Mac runner signs the app before notarizing.',
  'Thanks for the notes. I moved the title card earlier and trimmed the intro by about four seconds.',
  'Let us book the studio for Monday morning and keep the afternoon free for pickups.',
  'Draft reply: sounds great, I will have the revised budget over to you by end of day.',
  'Add milk, eggs, coffee and a new pack of AA batteries to the shopping list.',
  'The new insights page should feel a lot more alive. Big numbers, a real chart, and a heatmap.',
  'Meeting notes: agreed on the release date, Jon owns the changelog, Sam owns the website copy.',
];
const APPS = ['Slack', 'Chrome', 'Visual Studio Code', 'Mail', 'Notion', 'Premiere Pro'];

const rand = seeded(42);
const transcriptions: Transcription[] = Array.from({ length: 36 }, (_, i) => {
  const text = SAMPLE_TEXT[i % SAMPLE_TEXT.length];
  const words = text.split(/\s+/).length;
  const created = now - i * (DAY / 5) - Math.floor(rand() * 3_000_000);
  return {
    id: 1000 - i,
    text,
    word_count: words,
    duration_ms: Math.round((words / (1.6 + rand())) * 1000),
    target_app: APPS[i % APPS.length],
    created_at: created,
  };
});

function heatmap(): UsageSummary['heatmap'] {
  const r = seeded(7);
  const out: UsageSummary['heatmap'] = [];
  const today = Math.floor(now / DAY);
  for (let d = 0; d < 34 * 7; d++) {
    const day = today - d;
    const v = r();
    const weekend = new Date(day * DAY).getDay() % 6 === 0;
    const count = v < 0.18 ? 0 : Math.round((weekend ? 3 : 9) * v * v + (d < 12 ? 4 : 0));
    const dt = new Date(day * DAY);
    const key = dt.getFullYear() * 10000 + (dt.getMonth() + 1) * 100 + dt.getDate();
    out.push({ day: key, count });
  }
  return out;
}

const usage: UsageSummary = {
  totals: {
    total_transcriptions: 2318,
    total_words: 184_320,
    total_speech_ms: Math.round((184_320 / 112) * 60_000),
    current_streak: 12,
    longest_streak: 31,
  },
  heatmap: heatmap(),
  top_words: [
    ['trailer', 142],
    ['Murmr', 128],
    ['studio', 96],
    ['edit', 91],
    ['budget', 74],
    ['colour', 66],
    ['release', 61],
    ['Tauri', 52],
    ['Thursday', 47],
    ['Premiere', 44],
    ['pickups', 39],
    ['changelog', 31],
  ].map(([word, count]) => ({ word: word as string, count: count as number })),
  top_fillers: [
    { word: 'um', count: 512 },
    { word: 'uh', count: 341 },
    { word: 'like', count: 218 },
    { word: 'you know', count: 126 },
    { word: 'so', count: 87 },
  ],
  total_fillers_removed: 1284,
  top_phrases: [
    { phrase: 'by end of day', count: 41 },
    { phrase: 'sounds great', count: 37 },
    { phrase: 'let me know', count: 33 },
    { phrase: 'the final cut', count: 22 },
  ],
  hourly: [0, 0, 0, 0, 0, 1, 4, 12, 38, 61, 72, 58, 31, 44, 66, 81, 63, 40, 22, 14, 9, 5, 2, 1],
  themes: [
    { theme: 'work', label: 'Work', transcription_count: 1210, sample_words: ['budget', 'release', 'meeting'] },
    { theme: 'creative', label: 'Creative', transcription_count: 640, sample_words: ['trailer', 'edit', 'colour'] },
    { theme: 'code', label: 'Code', transcription_count: 288, sample_words: ['Tauri', 'build', 'runner'] },
  ],
  weekly_wpm: Array.from({ length: 16 }, (_, i) => ({
    week_start_ms: now - (15 - i) * 7 * DAY,
    avg_wpm: Math.round(92 + i * 1.3 + Math.sin(i * 1.4) * 6),
    words: 8000 + Math.round(Math.cos(i) * 2000) + i * 300,
  })),
  personal_records: {
    longest_words: { ...transcriptions[6], word_count: 412 },
    longest_duration: { ...transcriptions[2], duration_ms: 263_000 },
    highest_wpm: transcriptions[4],
    highest_wpm_value: 168,
  },
  app_breakdown: [
    { app: 'Slack', transcription_count: 812, total_speech_ms: 0 },
    { app: 'Chrome', transcription_count: 544, total_speech_ms: 0 },
    { app: 'Visual Studio Code', transcription_count: 401, total_speech_ms: 0 },
    { app: 'Mail', transcription_count: 286, total_speech_ms: 0 },
    { app: 'Notion', transcription_count: 175, total_speech_ms: 0 },
  ],
  filler_progress: { word: 'um', current_count: 62, prior_count: 100, window_days: 30 },
};

const dictionary: DictionaryEntry[] = [
  { id: 1, type: 'word', trigger: 'Murmr', expansion: null, description: null, is_regex: false, enabled: true, created_at: now, updated_at: now },
  { id: 2, type: 'word', trigger: 'Tauri', expansion: null, description: null, is_regex: false, enabled: true, created_at: now, updated_at: now },
  { id: 3, type: 'replacement', trigger: 'color', expansion: 'colour', description: null, is_regex: false, enabled: true, created_at: now, updated_at: now },
  { id: 4, type: 'snippet', trigger: 'my address', expansion: '12 Example Street, Springfield', description: 'Home address', is_regex: false, enabled: true, created_at: now, updated_at: now },
  { id: 5, type: 'snippet', trigger: 'sign off', expansion: 'Thanks,\nJon', description: null, is_regex: false, enabled: false, created_at: now, updated_at: now },
];

const suggestions: DictionarySuggestion[] = [
  { id: 1, type: 'replacement', trigger: 'mermer', expansion: 'Murmr', count: 3, example: 'open mermer settings', first_seen_at: now - DAY, last_seen_at: now },
  { id: 2, type: 'replacement', trigger: 'tory', expansion: 'Tauri', count: 2, example: 'the tory build', first_seen_at: now - DAY, last_seen_at: now },
];

let settings: Settings = {
  appearance: 'dark',
  launch_at_login: true,
  microphone_device: null,
  noise_suppression: true,
  audio_duck_amount: 0.3,
  tap_threshold_ms: 250,
  dictation_hotkey: 'ControlRight',
  repeat_hotkey: 'F9',
  cancel_hotkey: 'Escape',
  hud_show_waveform: true,
  hud_show_timer: true,
  hud_position: 'bottom',
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
  filler_words: ['um', 'uh', 'like', 'you know'],
  retention_days: 0,
  injection_mode: 'paste',
  log_level: 'info',
  force_cpu: false,
  has_completed_onboarding: true,
  display_name: 'Jon',
  milestone_notifications: true,
  pause_during_fullscreen: true,
  literal_mode: false,
  auto_bulleted_lists: true,
  voice_command_symbols: false,
  smart_spacing: true,
  auto_smart_lists: true,
  fuzzy_dictionary: false,
  model_name: 'ggml-large-v3-turbo-q5_0.bin',
  auto_model: true,
  accuracy_mode: false,
  streamer_mode: false,
  streamer_mode_mute_chimes: false,
  edit_last_hotkey: 'F10',
  vad_enabled: true,
  ai_cleanup: true,
  command_hotkey: 'AltRight',
  live_preview: true,
};

export function installMockIpc() {
  mockWindows('main');
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case 'ping':
          return { message: 'pong', version: '0.1.77' };
        case 'user_info':
          return { display_name: 'Jon', raw_name: 'jon' };
        case 'recent_transcriptions':
          return transcriptions;
        case 'search_transcriptions': {
          const q = String(a.query ?? '').toLowerCase();
          return transcriptions.filter((t) => t.text.toLowerCase().includes(q));
        }
        case 'transcription_count':
          return usage.totals.total_transcriptions;
        case 'usage_summary':
          return usage;
        case 'list_dictionary':
          return a.typeFilter ? dictionary.filter((d) => d.type === a.typeFilter) : dictionary;
        case 'list_dictionary_suggestions':
          return suggestions;
        case 'get_settings':
          return settings;
        case 'save_settings':
          settings = a.newSettings as Settings;
          return null;
        case 'list_input_devices':
          return [
            { name: 'MacBook Pro Microphone', is_default: true },
            { name: 'Shure MV7', is_default: false },
          ];
        case 'list_models':
          return ['ggml-base.en.bin', 'ggml-large-v3-turbo-q5_0.bin'];
        case 'app_paths':
          return { db_path: 'C:/Users/me/AppData/murmr.db', settings_path: 'C:/Users/me/AppData/settings.json', model_path: 'C:/Users/me/AppData/models', log_path: null };
        case 'launch_at_login_active':
          return true;
        case 'is_recording_active':
          return false;
        case 'engine_status':
          return {
            model_name: 'ggml-large-v3-turbo-q5_0.bin',
            backend: { kind: 'Vulkan', device: 'NVIDIA GeForce RTX 4070' },
            gpu_available: true,
            gpu_pending: false,
            gpu_device: 'NVIDIA GeForce RTX 4070',
            gpu_note: null,
            restart_required: false,
            vad_active: true,
            loading: null,
            last_error: null,
          };
        case 'model_catalog':
          return [
            { file: 'ggml-base.en.bin', label: 'Base', description: 'Fast, good for most dictation.', size_bytes: 147_000_000, downloaded: true },
            { file: 'ggml-large-v3-turbo-q5_0.bin', label: 'Turbo', description: 'Most accurate. Best with a GPU.', size_bytes: 574_000_000, downloaded: true },
          ];
        case 'llm_key_status':
          return { has_key: true, hint: 'sk-ant-…WXYZ' };
        case 'llm_test_key':
          return { ok: true, latency_ms: 640, message: 'Looks good.' };
        default:
          if (cmd.startsWith('check_') && cmd.endsWith('_permission')) return 'granted';
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
