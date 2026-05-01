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
      <Row name="Show estimated word count" hint="Steps up while you're actually speaking">
        <Toggle on={settings.hud_show_word_count} onChange={(v) => update({ hud_show_word_count: v })} />
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

      <Row name="Filler word list" hint="Edit the words Murmr quietly removes">
        <Segmented
          value="default"
          options={[{ value: 'default', label: `${settings.filler_words.length} words` }]}
          onChange={() => {}}
          disabled
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
      <Row name="Export all to file" hint="Exports as plain .txt; coming in Phase 10">
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
        <Pill>Heads up</Pill>{' '}
        Toggles persist immediately — but post-processing, sound, retention, and HUD-toggle
        wiring lands in Phases 7–9. The settings file ({' '}
        <code className="bg-bg-control px-1 rounded">settings.json</code>) is already on disk.
      </p>
    </div>
  );
}
