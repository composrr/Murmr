import { useEffect, useState } from 'react';
import { getSettings, saveSettings, type Settings } from '../../../lib/ipc';
import HotkeyCapture, { displayName } from './HotkeyCapture';
import { Pill, Row, SettingsHeader } from './settings-ui';

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
  const repeat = settings?.repeat_hotkey ?? '';

  // Build the forbid list per-row: a key bound to one shortcut shouldn't be
  // re-bindable to another. Empty repeat = no conflict to track.
  const allBound = [dictation, cancel, repeat].filter((k) => k.length > 0);

  return (
    <div className="max-w-[640px]">
      <SettingsHeader
        title="Hotkeys"
        subtitle="Recording shortcut, re-paste shortcut, cancel key, and tap-vs-hold threshold."
      />

      <Row name="Recording shortcut" hint="Tap to toggle, hold to push-to-talk. Click and press the key you want.">
        <HotkeyCapture
          value={dictation}
          onChange={(next) => update({ dictation_hotkey: next })}
          allowBareModifiers
          forbidden={allBound.filter((k) => k !== dictation)}
          disabled={!settings}
        />
      </Row>

      <Row
        name="Re-paste shortcut"
        hint={
          repeat
            ? `Press ${displayName(repeat)} from anywhere to re-inject the most recent transcription.`
            : 'Set a key here to enable re-paste. Click the chip → press a key. Click "clear" to disable.'
        }
      >
        <div className="flex items-center gap-2 w-[260px]">
          <div className="flex-1">
            <HotkeyCapture
              value={repeat || 'click to set'}
              onChange={(next) => update({ repeat_hotkey: next })}
              allowBareModifiers
              forbidden={allBound.filter((k) => k !== repeat)}
              disabled={!settings}
            />
          </div>
          {repeat && (
            <button
              onClick={() => update({ repeat_hotkey: '' })}
              className="text-[11px] text-text-tertiary hover:text-text-primary px-2 py-1 rounded-[6px]"
              title="Disable re-paste shortcut"
            >
              clear
            </button>
          )}
        </div>
      </Row>

      <Row name="Cancel key" hint="Stops the current recording and discards it.">
        <HotkeyCapture
          value={cancel}
          onChange={(next) => update({ cancel_hotkey: next })}
          forbidden={allBound.filter((k) => k !== cancel)}
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
          Click a key chip and press the key you want — saves immediately and the global listener
          picks it up without restarting Murmr. The key won't pass through to your focused app, so
          binding to <code className="text-text-secondary">~</code> or{' '}
          <code className="text-text-secondary">]</code> is safe.
        </p>
      </div>
    </div>
  );
}
