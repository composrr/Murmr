import { useEffect, useState } from 'react';
import {
  clearAllTranscriptions,
  clearLast24Hours,
  getSettings,
  openSoundsFolder,
  saveSettings,
  type Settings,
} from '../../../lib/ipc';
import {
  NativeSelect,
  Pill,
  Row,
  SecondaryButton,
  SectionHeader,
  Segmented,
  SettingsHeader,
  Toggle,
} from './settings-ui';

const HUD_POSITIONS = [
  { value: 'near-input', label: 'Near input' },
  { value: 'bottom-center', label: 'Bottom-center' },
  { value: 'top-center', label: 'Top-center' },
  { value: 'bottom-left', label: 'Bottom-left' },
  { value: 'bottom-right', label: 'Bottom-right' },
] as const;

const RETENTION_OPTIONS = [
  { value: 0, label: 'Forever (default)' },
  { value: 30, label: '30 days' },
  { value: 90, label: '90 days' },
  { value: 180, label: '6 months' },
  { value: 365, label: '1 year' },
  { value: 730, label: '2 years' },
];

export default function Preferences() {
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    getSettings().then(setSettings).catch(() => {});
  }, []);

  const update = (patch: Partial<Settings>) => {
    if (!settings) return;
    const next = { ...settings, ...patch };
    setSettings(next);
    saveSettings(next).catch(() => {});
  };

  if (!settings) {
    return (
      <div className="max-w-[640px]">
        <SettingsHeader title="Preferences" />
        <p className="text-[13px] text-text-quaternary">Loading…</p>
      </div>
    );
  }

  return (
    <div className="max-w-[640px]">
      <SettingsHeader
        title="Preferences"
        subtitle="HUD, sounds, post-processing, retention."
      />

      <SectionHeader>HUD</SectionHeader>
      <Row name="Show waveform" hint="Live mic-level bars in the recording pill">
        <Toggle on={settings.hud_show_waveform} onChange={(v) => update({ hud_show_waveform: v })} />
      </Row>
      <Row name="Show timer" hint="Tabular elapsed-seconds counter">
        <Toggle on={settings.hud_show_timer} onChange={(v) => update({ hud_show_timer: v })} />
      </Row>
      <Row name="Position" hint="Where the HUD bubble appears on screen">
        <NativeSelect
          value={settings.hud_position}
          onChange={(v) => update({ hud_position: String(v) })}
          options={[...HUD_POSITIONS]}
        />
      </Row>

      <SectionHeader>Sounds</SectionHeader>
      <Row name="Start sound" hint="Plays the moment you press the dictation key">
        <Toggle on={settings.sound_start_click} onChange={(v) => update({ sound_start_click: v })} />
      </Row>
      <Row name="Stop sound" hint="Plays the moment you release / toggle off">
        <Toggle
          on={settings.sound_complete_chime}
          onChange={(v) => update({ sound_complete_chime: v })}
        />
      </Row>
      <Row name="Error beep" hint="Plays on transcription failure">
        <Toggle on={settings.sound_error_beep} onChange={(v) => update({ sound_error_beep: v })} />
      </Row>
      <Row
        name="Sound volume"
        hint={
          settings.sound_volume === 0
            ? 'Muted — sounds are disabled regardless of the toggles above.'
            : `${Math.round(settings.sound_volume * 100)}% of the file's native level (above 100% boosts).`
        }
      >
        <div className="flex items-center gap-3 w-[260px]">
          <input
            type="range"
            min={0}
            max={1.5}
            step={0.05}
            value={settings.sound_volume ?? 0.7}
            onChange={(e) => update({ sound_volume: parseFloat(e.target.value) })}
            className="flex-1 accent-[#1f1f1c] dark:accent-[#d4d4cf]"
          />
          <span className="text-[12px] text-text-tertiary tabular-nums w-[44px] text-right">
            {Math.round((settings.sound_volume ?? 0.7) * 100)}%
          </span>
        </div>
      </Row>
      <Row
        name="Custom sound files"
        hint="Drop start.wav, complete.wav, or error.wav into this folder to override the defaults"
      >
        <SecondaryButton onClick={() => openSoundsFolder().catch(() => {})}>
          Open
        </SecondaryButton>
      </Row>

      <SectionHeader>Notifications</SectionHeader>
      <Row
        name="Milestone notifications"
        hint="Show a desktop notification when you hit a meaningful milestone (100th transcription, week-long streaks, new personal bests). Rare and never during a recording."
      >
        <Toggle
          on={settings.milestone_notifications}
          onChange={(v) => update({ milestone_notifications: v })}
        />
      </Row>

      <SectionHeader>Games &amp; fullscreen apps</SectionHeader>
      <Row
        name="Pause Murmr in fullscreen apps"
        hint="Ignore the dictation hotkey while a fullscreen game, video, or presentation is focused. Prevents accidental triggers in-game and the stuck-state bug where fullscreen exclusive games eat the key release. Murmr resumes the instant you alt-tab out."
      >
        <Toggle
          on={settings.pause_during_fullscreen}
          onChange={(v) => update({ pause_during_fullscreen: v })}
        />
      </Row>

      <SectionHeader>Post-processing</SectionHeader>
      <Row name="Auto-capitalize" hint="Sentence starts and standalone i → I">
        <Toggle on={settings.auto_capitalize} onChange={(v) => update({ auto_capitalize: v })} />
      </Row>
      <Row name="Auto-period" hint="Append a trailing . if you didn't end with punctuation">
        <Toggle on={settings.auto_period} onChange={(v) => update({ auto_period: v })} />
      </Row>
      <Row name="Strip filler words" hint='Removes "um", "uh", "you know", etc.'>
        <Toggle on={settings.strip_fillers} onChange={(v) => update({ strip_fillers: v })} />
      </Row>
      <Row
        name="Auto numbered lists"
        hint='Say "One. First item. Two. Second item." and Murmr formats it as a real numbered list.'
      >
        <Toggle
          on={settings.auto_numbered_lists}
          onChange={(v) => update({ auto_numbered_lists: v })}
        />
      </Row>
      <Row
        name="Literal mode"
        hint="Type exactly what I said — skip filler-strip, auto-correct, capitalization, and formatting"
      >
        <Toggle on={settings.literal_mode} onChange={(v) => update({ literal_mode: v })} />
      </Row>
      <Row
        name="Auto bulleted lists"
        hint='Say "bullet … bullet …" to format an unordered list'
      >
        <Toggle
          on={settings.auto_bulleted_lists}
          onChange={(v) => update({ auto_bulleted_lists: v })}
        />
      </Row>
      <Row
        name="Smart spacing"
        hint="Add a space between back-to-back dictations so they don't run together"
      >
        <Toggle on={settings.smart_spacing} onChange={(v) => update({ smart_spacing: v })} />
      </Row>
      <Row
        name="Smart lists"
        hint='Turn a spoken list ("I need milk, eggs, and bread") into a clean bulleted or numbered list automatically — no need to say "one, two" or "bullet".'
      >
        <Toggle on={settings.auto_smart_lists} onChange={(v) => update({ auto_smart_lists: v })} />
      </Row>
      <Row
        name="Fuzzy-correct names"
        hint="Snap near-miss words to your Dictionary entries"
      >
        <Toggle on={settings.fuzzy_dictionary} onChange={(v) => update({ fuzzy_dictionary: v })} />
      </Row>

      <SectionHeader>Voice commands</SectionHeader>
      <Row name='"period" → .' hint="Spoken at end of utterance becomes punctuation">
        <Toggle
          on={settings.voice_command_period}
          onChange={(v) => update({ voice_command_period: v })}
        />
      </Row>
      <Row name='"comma" → ,' hint="Useful but turns off if you talk about basketball">
        <Toggle
          on={settings.voice_command_comma}
          onChange={(v) => update({ voice_command_comma: v })}
        />
      </Row>
      <Row name='"question mark" → ?'>
        <Toggle
          on={settings.voice_command_question}
          onChange={(v) => update({ voice_command_question: v })}
        />
      </Row>
      <Row name='"exclamation point" → !'>
        <Toggle
          on={settings.voice_command_exclamation}
          onChange={(v) => update({ voice_command_exclamation: v })}
        />
      </Row>
      <Row name='"new line" → \n'>
        <Toggle
          on={settings.voice_command_new_line}
          onChange={(v) => update({ voice_command_new_line: v })}
        />
      </Row>
      <Row name='"new paragraph" → \n\n'>
        <Toggle
          on={settings.voice_command_new_paragraph}
          onChange={(v) => update({ voice_command_new_paragraph: v })}
        />
      </Row>
      <Row
        name="Code symbols"
        hint='"colon", "open paren", "backtick", etc. become literal symbols'
      >
        <Toggle
          on={settings.voice_command_symbols}
          onChange={(v) => update({ voice_command_symbols: v })}
        />
      </Row>

      <Row name="Filler word list" hint="Edit the words Murmr quietly removes">
        <Segmented
          value="default"
          options={[{ value: 'default', label: `${settings.filler_words.length} words` }]}
          onChange={() => {}}
          disabled
        />
      </Row>

      <SectionHeader>Streamer mode</SectionHeader>
      <Row
        name="Streamer mode"
        hint="Hide the HUD from screen capture and suppress notifications so nothing Murmr-related shows on a broadcast"
      >
        <Toggle on={settings.streamer_mode} onChange={(v) => update({ streamer_mode: v })} />
      </Row>
      <Row
        name="Mute chimes while streaming"
        hint="Also silence Murmr's start/stop/error sounds while streamer mode is on"
      >
        <Toggle
          on={settings.streamer_mode_mute_chimes}
          onChange={(v) => update({ streamer_mode_mute_chimes: v })}
        />
      </Row>

      <SectionHeader>Retention</SectionHeader>
      <Row name="Keep transcriptions for" hint="Older transcriptions are auto-deleted">
        <NativeSelect
          value={settings.retention_days}
          onChange={(v) => update({ retention_days: Number(v) })}
          options={RETENTION_OPTIONS}
        />
      </Row>
      <Row name="Export all to file" hint="Export your full history as a plain .txt file (coming soon)">
        <SecondaryButton disabled>Export</SecondaryButton>
      </Row>
      <Row name="Clear last 24 hours" hint="Drops everything you've dictated since this time yesterday">
        <SecondaryButton
          onClick={() => {
            if (window.confirm('Delete every transcription from the last 24 hours?')) {
              clearLast24Hours().catch((e) => console.error(e));
            }
          }}
        >
          Clear
        </SecondaryButton>
      </Row>
      <Row name="Clear all transcriptions" hint="Wipes the entire transcription history. Cannot be undone.">
        <SecondaryButton
          onClick={() => {
            if (
              window.confirm(
                'Delete EVERY transcription Murmr has ever saved? This cannot be undone.',
              )
            ) {
              clearAllTranscriptions().catch((e) => console.error(e));
            }
          }}
        >
          Clear all
        </SecondaryButton>
      </Row>

      <p className="text-[11px] text-text-quaternary mt-6">
        <Pill>Note</Pill>{' '}
        Every toggle here takes effect immediately and is written to{' '}
        <code className="bg-bg-control px-1 rounded">settings.json</code> in your app-data folder.
      </p>
    </div>
  );
}
