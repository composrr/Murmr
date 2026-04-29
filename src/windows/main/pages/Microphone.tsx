import { useEffect, useRef, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  getSettings,
  listInputDevices,
  recordAndTranscribe,
  saveSettings,
  type InputDevice,
  type Settings,
  type TranscriptionResult,
} from '../../../lib/ipc';
import {
  NativeSelect,
  Pill,
  Row,
  SettingsHeader,
} from './settings-ui';

const WAVEFORM_BARS = 25;

const TEST_SECONDS = 3;

export default function Microphone() {
  const [devices, setDevices] = useState<InputDevice[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [test, setTest] = useState<
    | { kind: 'idle' }
    | { kind: 'recording'; remaining: number }
    | { kind: 'transcribing' }
    | { kind: 'done'; result: TranscriptionResult }
    | { kind: 'error'; message: string }
  >({ kind: 'idle' });
  const [rms, setRms] = useState<number[]>(() => Array(WAVEFORM_BARS).fill(0));
  const tickRef = useRef<number | null>(null);

  useEffect(() => {
    listInputDevices().then(setDevices).catch(() => {});
    getSettings().then(setSettings).catch(() => {});
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    listen<number>('murmr:audio-rms', (event) => {
      setRms((prev) => {
        const next = prev.slice(1);
        next.push(event.payload);
        return next;
      });
    }).then((u) => (unlisten = u));
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  useEffect(() => () => {
    if (tickRef.current) clearInterval(tickRef.current);
  }, []);

  const update = (patch: Partial<Settings>) => {
    if (!settings) return;
    const next = { ...settings, ...patch };
    setSettings(next);
    saveSettings(next).catch(() => {});
  };

  const startTest = async () => {
    setTest({ kind: 'recording', remaining: TEST_SECONDS });
    setRms(Array(WAVEFORM_BARS).fill(0));
    tickRef.current = window.setInterval(() => {
      setTest((prev) =>
        prev.kind === 'recording'
          ? prev.remaining <= 1
            ? { kind: 'transcribing' }
            : { kind: 'recording', remaining: prev.remaining - 1 }
          : prev,
      );
    }, 1000);
    try {
      const result = await recordAndTranscribe(TEST_SECONDS);
      setTest({ kind: 'done', result });
    } catch (e) {
      setTest({ kind: 'error', message: String(e) });
    } finally {
      if (tickRef.current) {
        clearInterval(tickRef.current);
        tickRef.current = null;
      }
    }
  };

  const isBusy = test.kind === 'recording' || test.kind === 'transcribing';

  const deviceOptions = [
    { value: '', label: 'System default' },
    ...devices.map((d) => ({
      value: d.name,
      label: d.is_default ? `${d.name} (default)` : d.name,
    })),
  ];

  return (
    <div className="max-w-[640px]">
      <SettingsHeader
        title="Microphone"
        subtitle="Pick the input device, set gain, test the mic."
      />

      <Row name="Input device" hint="Murmr captures from this microphone">
        <NativeSelect
          value={settings?.microphone_device ?? ''}
          onChange={(v) => update({ microphone_device: v ? String(v) : null })}
          options={deviceOptions}
          disabled={!settings}
        />
      </Row>

      <Row name="Input gain" hint="-12 dB to +12 dB. Wired in Phase 9.">
        <div className="flex items-center gap-3 w-[260px]">
          <input
            type="range"
            min={-12}
            max={12}
            step={0.5}
            value={settings?.microphone_gain_db ?? 0}
            onChange={(e) => update({ microphone_gain_db: parseFloat(e.target.value) })}
            disabled={!settings}
            className="flex-1 accent-[#1f1f1c] dark:accent-[#d4d4cf]"
          />
          <span className="text-[12px] text-text-tertiary tabular-nums w-[44px] text-right">
            {(settings?.microphone_gain_db ?? 0).toFixed(1)} dB
          </span>
        </div>
      </Row>

      {/* Noise suppression — removed for now. The toggle was cosmetic; we
          never wired it to a real DSP path. Re-add when we plug in RNNoise
          or webrtc-ns. */}

      <Row
        name="Duck system audio"
        hint={
          (settings?.audio_duck_amount ?? 0) === 0
            ? 'Off — system volume stays at full while recording.'
            : `Lowers the master output by ${Math.round(((settings?.audio_duck_amount ?? 0) * 100))}% during recording so background music gets out of the way.`
        }
      >
        <div className="flex items-center gap-3 w-[260px]">
          <input
            type="range"
            min={0}
            max={0.7}
            step={0.05}
            value={settings?.audio_duck_amount ?? 0.3}
            onChange={(e) => update({ audio_duck_amount: parseFloat(e.target.value) })}
            disabled={!settings}
            className="flex-1 accent-[#1f1f1c] dark:accent-[#d4d4cf]"
          />
          <span className="text-[12px] text-text-tertiary tabular-nums w-[44px] text-right">
            {((settings?.audio_duck_amount ?? 0.3) * 100).toFixed(0)}%
          </span>
        </div>
      </Row>

      <div className="mt-9 pt-7 border-t border-border-hairline">
        <h2 className="font-serif text-[22px] tracking-[-0.3px] text-text-primary mt-0 mb-3">
          Test the mic
        </h2>
        <p className="text-[12px] text-text-tertiary leading-[1.55] mb-4">
          Captures {TEST_SECONDS} seconds from the selected device, runs Whisper, shows the
          result. Not saved to history.
        </p>

        <div
          className={
            'rounded-[14px] border bg-bg-row p-5 mb-4 transition-colors ' +
            (test.kind === 'recording' ? 'border-[#e85d4a]/60' : 'border-border-hairline')
          }
        >
          <div className="flex items-center gap-4">
            <button
              onClick={startTest}
              disabled={isBusy}
              className={
                'relative w-[52px] h-[52px] rounded-full grid place-items-center transition-all flex-shrink-0 ' +
                (test.kind === 'recording'
                  ? 'bg-[#e85d4a] text-white shadow-[0_0_0_5px_rgba(232,93,74,0.16)]'
                  : 'bg-[#1f1f1c] text-[#fafaf9] dark:bg-[#d4d4cf] dark:text-[#1f1f1c] hover:scale-[1.04] disabled:opacity-50')
              }
              aria-label="Run mic test"
            >
              <svg viewBox="0 0 48 48" width="24" height="24">
                <g transform="translate(24, 24)" fill="currentColor">
                  <rect x="-7" y="-15" width="14" height="20" rx="7" />
                  <path d="M -12 3 Q -12 15 0 15 Q 12 15 12 3" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" />
                  <line x1="0" y1="15" x2="0" y2="22" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" />
                </g>
              </svg>
            </button>
            <div className="flex-1 min-w-0">
              <div className="text-[13px] font-medium text-text-primary mb-1.5">
                {test.kind === 'recording'
                  ? `Listening… ${test.remaining}s`
                  : test.kind === 'transcribing'
                    ? 'Transcribing…'
                    : test.kind === 'done'
                      ? 'Test complete'
                      : 'Tap to start'}
              </div>
              <div className="flex items-end gap-[2px] h-[24px]">
                {rms.map((value, i) => {
                  const norm = Math.min(1, value / 0.4);
                  const height = test.kind === 'recording' ? 4 + norm * 20 : 4;
                  const opacity = test.kind === 'recording' ? 0.5 + norm * 0.45 : 0.25;
                  return (
                    <span
                      key={i}
                      style={{
                        display: 'inline-block',
                        width: 3,
                        height,
                        background: 'var(--text-secondary)',
                        borderRadius: 1.5,
                        opacity,
                        transition: 'height 80ms linear, opacity 80ms linear',
                      }}
                    />
                  );
                })}
              </div>
            </div>
          </div>
        </div>
        {test.kind === 'done' && (
          <>
            <p className="font-serif text-[14px] text-text-primary leading-[1.55] py-3 px-3 rounded-row bg-bg-row border border-border-hairline m-0">
              {test.result.text || (
                <span className="text-text-quaternary italic">(no speech detected)</span>
              )}
            </p>
            <div className="grid grid-cols-2 gap-x-4 gap-y-1 mt-3 text-[11px]">
              <span className="text-text-quaternary">Device used</span>
              <span className="text-text-primary text-right truncate">
                {test.result.capture_device}
              </span>
              <span className="text-text-quaternary">Capture rate</span>
              <span className="text-text-primary text-right tabular-nums">
                {test.result.capture_sample_rate.toLocaleString()} Hz · {test.result.capture_channels}ch
              </span>
              <span className="text-text-quaternary">Whisper time</span>
              <span className="text-text-primary text-right tabular-nums">
                {test.result.elapsed_transcribe_ms} ms
              </span>
            </div>
          </>
        )}
        {test.kind === 'error' && (
          <p className="text-[12px] text-[#e85d4a] py-2 px-3 rounded-[8px] bg-bg-row border border-border-hairline">
            {test.message}
          </p>
        )}
      </div>

      <p className="text-[11px] text-text-quaternary mt-6">
        <Pill>Heads up</Pill>{' '}
        Device selection persists immediately, but Murmr currently captures from the OS default
        device. Honoring the selection lands when we wire it through cpal in Phase 9.
      </p>
    </div>
  );
}
