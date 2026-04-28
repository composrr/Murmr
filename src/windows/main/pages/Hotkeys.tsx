import { useEffect, useState } from 'react';
import { getSettings, saveSettings, type Settings } from '../../../lib/ipc';
import HotkeyCapture, { displayName } from './HotkeyCapture';
import { Pill, Row, SettingsHeader } from './settings-ui';

const REPEAT_MODIFIERS: Array<{ value: string; label: string }> = [
  { value: 'Shift', label: 'Shift' },
  { value: 'Ctrl', label: 'Ctrl' },
  { value: 'Alt', label: 'Alt' },
  { value: 'Meta', label: 'Cmd / Win' },
  { value: 'None', label: 'No modifier (disable)' },
];

export default function Hotkeys() {
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

  const threshold = settings?.tap_threshold_ms ?? 250;
  const dictation = settings?.dictation_hotkey ?? 'ControlRight';
  const cancel = settings?.cancel_hotkey ?? 'Escape';
  const repeatMod = settings?.repeat_modifier ?? 'Shift';

  return (
    <div className="max-w-[640px]">
      <SettingsHeader
        title="Hotkeys"
        subtitle="Recording shortcut, cancel key, and tap-vs-hold threshold."
      />

      <Row name="Recording shortcut" hint="Tap to toggle, hold to push-to-talk. Click and press the key you want.">
        <HotkeyCapture
          value={dictation}
          onChange={(next) => update({ dictation_hotkey: next })}
          allowBareModifiers
          forbidden={[cancel]}
          disabled={!settings}
        />
      </Row>

      <Row
        name="Re-paste modifier"
        hint={
          repeatMod === 'None'
            ? 'Re-paste shortcut disabled.'
            : `Hold ${labelFor(repeatMod)} + ${displayName(dictation)} to re-inject the most recent transcription.`
        }
      >
        <select
          value={repeatMod}
          onChange={(e) => update({ repeat_modifier: e.target.value })}
          disabled={!settings}
          className="w-[260px] rounded-[8px] border border-border-control bg-bg-control px-3 py-[6px] text-[12px] font-medium text-text-primary"
        >
          {REPEAT_MODIFIERS.map((m) => (
            <option key={m.value} value={m.value}>
              {m.label}
            </option>
          ))}
        </select>
      </Row>

      <Row name="Cancel key" hint="Stops the current recording and discards it.">
        <HotkeyCapture
          value={cancel}
          onChange={(next) => update({ cancel_hotkey: next })}
          forbidden={[dictation]}
          disabled={!settings}
        />
      </Row>

      <Row
        name="Tap-vs-hold threshold"
        hint={`Holding longer than this triggers push-to-talk (${threshold} ms).`}
      >
        <div className="flex items-center gap-3 w-[260px]">
          <input
            type="range"
            min={100}
            max={500}
            step={10}
            value={threshold}
            onChange={(e) =>
              update({ tap_threshold_ms: parseInt(e.target.value, 10) || 250 })
            }
            disabled={!settings}
            className="flex-1 accent-[#1f1f1c] dark:accent-[#d4d4cf]"
          />
          <span className="text-[12px] text-text-tertiary tabular-nums w-[56px] text-right">
            {threshold} ms
          </span>
        </div>
      </Row>

      <div className="mt-9 pt-7 border-t border-border-hairline">
        <p className="text-[12px] text-text-tertiary leading-[1.55]">
          <Pill kind="info">Tip</Pill>{' '}
          Click a key chip and press the key you want — it'll save immediately and the global
          listener picks it up without restarting Murmr.
        </p>
        <p className="text-[12px] text-text-tertiary leading-[1.55] mt-3">
          Why <strong className="text-text-secondary">{displayName(dictation)}</strong> by default?
          Right Ctrl tap-vs-hold ergonomics without the menu-bar focus-steal that Right Alt causes
          on Windows. Function keys (F1–F12) and Caps Lock also work great if you'd rather repurpose
          one.
        </p>
      </div>
    </div>
  );
}

function labelFor(value: string): string {
  return REPEAT_MODIFIERS.find((m) => m.value === value)?.label ?? value;
}
