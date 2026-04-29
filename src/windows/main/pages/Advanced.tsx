import { useEffect, useState } from 'react';
import {
  appPaths,
  getSettings,
  openAppDataFolder,
  openPerfLog,
  resetOnboarding,
  saveSettings,
  type AppPaths,
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
} from './settings-ui';

const LOG_LEVELS = [
  { value: 'error', label: 'Error' },
  { value: 'warn', label: 'Warn' },
  { value: 'info', label: 'Info (default)' },
  { value: 'debug', label: 'Debug' },
  { value: 'trace', label: 'Trace' },
];

export default function Advanced() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [paths, setPaths] = useState<AppPaths | null>(null);

  useEffect(() => {
    getSettings().then(setSettings).catch(() => {});
    appPaths().then(setPaths).catch(() => {});
  }, []);

  const update = (patch: Partial<Settings>) => {
    if (!settings) return;
    const next = { ...settings, ...patch };
    setSettings(next);
    saveSettings(next).catch(() => {});
  };

  if (!settings || !paths) {
    return (
      <div className="max-w-[640px]">
        <SettingsHeader title="Advanced" />
        <p className="text-[13px] text-text-quaternary">Loading…</p>
      </div>
    );
  }

  return (
    <div className="max-w-[640px]">
      <SettingsHeader
        title="Advanced"
        subtitle="Speech model, injection mode, log level."
      />

      <SectionHeader>Speech model</SectionHeader>
      <Row name="Model file">
        <span className="text-[12px] text-text-tertiary tabular-nums max-w-[420px] block text-right truncate">
          {paths.model_path}
        </span>
      </Row>
      <Row name="Compute backend">
        <span className="text-[12px] text-text-primary font-medium">CPU</span>
      </Row>
      {/* Removed "Force CPU" toggle + "GPU backend in a future phase" — we
          run CPU-only on every platform right now. Add the toggle back
          when CUDA / Metal lands as a real path. */}

      <SectionHeader>Injection</SectionHeader>
      <Row name="Injection mode" hint="Clipboard-paste is the default; per-keystroke is a fallback for paste-blocking apps.">
        <Segmented
          value={settings.injection_mode}
          onChange={(v) => update({ injection_mode: v })}
          options={[
            { value: 'clipboard', label: 'Clipboard' },
            { value: 'keystroke', label: 'Per-keystroke' },
          ]}
        />
      </Row>

      <SectionHeader>Logging</SectionHeader>
      <Row name="Log level">
        <NativeSelect
          value={settings.log_level}
          onChange={(v) => update({ log_level: String(v) })}
          options={LOG_LEVELS}
        />
      </Row>
      <Row name="Open log file" hint="Murmr's debug log (tail this if something goes wrong)">
        <SecondaryButton disabled>Open</SecondaryButton>
      </Row>
      <Row
        name="Open transcribe perf log"
        hint="One line per dictation: audio length, threads, full() time, total ms, real-time ratio"
      >
        <SecondaryButton onClick={() => openPerfLog().catch(() => {})}>
          Open
        </SecondaryButton>
      </Row>

      <SectionHeader>Files</SectionHeader>
      <Row name="App data folder" hint="Database, settings, models, logs">
        <SecondaryButton onClick={() => openAppDataFolder().catch(() => {})}>
          Open
        </SecondaryButton>
      </Row>
      <Row name="Database">
        <span className="text-[12px] text-text-tertiary tabular-nums max-w-[420px] block text-right truncate">
          {paths.db_path}
        </span>
      </Row>
      <Row name="Settings">
        <span className="text-[12px] text-text-tertiary tabular-nums max-w-[420px] block text-right truncate">
          {paths.settings_path}
        </span>
      </Row>

      <SectionHeader>Reset</SectionHeader>
      <Row
        name="Re-run first-run wizard"
        hint="Closes the main window and reopens the welcome / mic test / practice flow"
      >
        <SecondaryButton onClick={() => resetOnboarding().catch(() => {})}>
          Re-run
        </SecondaryButton>
      </Row>
      <Row name="Reset Murmr to defaults" hint="Wipes settings; transcriptions/dictionary stay unless you explicitly clear them">
        <SecondaryButton disabled>Reset</SecondaryButton>
      </Row>

      <p className="text-[11px] text-text-quaternary mt-6">
        <Pill kind="info">Note</Pill>{' '}
        Path display + open-folder buttons are live. The injection-mode toggle, log-level
        selector, and reset are persisted but not yet wired to the backend.
      </p>
    </div>
  );
}
