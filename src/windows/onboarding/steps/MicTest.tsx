import { useEffect, useRef, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  listInputDevices,
  recordAndTranscribe,
  type InputDevice,
} from '../../../lib/ipc';
import StepFrame from './StepFrame';
import type { StepProps } from '../App';

const TEST_SECONDS = 3;
const WAVEFORM_BARS = 31;

export default function MicTest(props: StepProps) {
  const [devices, setDevices] = useState<InputDevice[]>([]);
  const [selected, setSelected] = useState<string>('');
  const [phase, setPhase] = useState<'idle' | 'recording' | 'transcribing' | 'done' | 'error'>(
    'idle',
  );
  const [seconds, setSeconds] = useState(TEST_SECONDS);
  const [transcript, setTranscript] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [rms, setRms] = useState<number[]>(() => Array(WAVEFORM_BARS).fill(0));
  const tickRef = useRef<number | null>(null);

  useEffect(() => {
    listInputDevices().then(setDevices).catch(() => {});
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

  const startTest = async () => {
    setPhase('recording');
    setSeconds(TEST_SECONDS);
    setTranscript('');
    setError(null);
    setRms(Array(WAVEFORM_BARS).fill(0));
    tickRef.current = window.setInterval(() => {
      setSeconds((s) => {
        if (s <= 1) {
          setPhase('transcribing');
          return 0;
        }
        return s - 1;
      });
    }, 1000);
    try {
      const result = await recordAndTranscribe(TEST_SECONDS);
      setTranscript(result.text);
      setPhase('done');
    } catch (e) {
      setError(String(e));
      setPhase('error');
    } finally {
      if (tickRef.current) {
        clearInterval(tickRef.current);
        tickRef.current = null;
      }
    }
  };

  const passed = phase === 'done' && transcript.trim().length > 0;
  const isBusy = phase === 'recording' || phase === 'transcribing';

  const label =
    phase === 'recording'
      ? `Listening… ${seconds}s`
      : phase === 'transcribing'
        ? 'Transcribing…'
        : passed
          ? 'Test again'
          : 'Test';

  return (
    <StepFrame
      {...props}
      title="Microphone test"
      subtitle="Press Test and speak for three seconds. The waveform shows what Murmr hears."
      primaryDisabled={!passed}
    >
      <div className="flex flex-col items-center w-full">
        <div className="w-full max-w-[420px] mb-6">
          <label className="text-[11px] uppercase tracking-[0.6px] text-text-quaternary font-medium block mb-2 text-center">
            Input device
          </label>
          <select
            value={selected}
            onChange={(e) => setSelected(e.target.value)}
            disabled={isBusy}
            className="w-full text-[13px] border border-border-control rounded-[7px] px-3 py-[7px] bg-bg-content text-text-primary"
          >
            <option value="">System default</option>
            {devices.map((d) => (
              <option key={d.name} value={d.name}>
                {d.is_default ? `${d.name} (default)` : d.name}
              </option>
            ))}
          </select>
        </div>

        <div className="text-[12px] uppercase tracking-[0.8px] text-text-quaternary font-medium mb-3">
          {label}
        </div>

        <button
          onClick={startTest}
          disabled={isBusy}
          aria-label="Test microphone"
          className={
            'relative w-[88px] h-[88px] rounded-full grid place-items-center transition-all flex-shrink-0 mb-5 ' +
            (phase === 'recording'
              ? 'bg-[#e85d4a] text-white shadow-[0_0_0_8px_rgba(232,93,74,0.16)]'
              : passed
                ? 'bg-[var(--toggle-on-bg)] text-[var(--toggle-on-thumb)]'
                : 'bg-[#1f1f1c] text-[#fafaf9] dark:bg-[#d4d4cf] dark:text-[#1f1f1c] hover:scale-[1.04] disabled:opacity-50')
          }
        >
          {passed ? (
            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="20 6 9 17 4 12" />
            </svg>
          ) : (
            <svg viewBox="0 0 48 48" width="38" height="38">
              <g transform="translate(24, 24)" fill="currentColor">
                <rect x="-7" y="-15" width="14" height="20" rx="7" />
                <path d="M -12 3 Q -12 15 0 15 Q 12 15 12 3" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" />
                <line x1="0" y1="15" x2="0" y2="22" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" />
              </g>
            </svg>
          )}
        </button>

        <Waveform rms={rms} active={phase === 'recording'} />

        {(transcript || phase === 'done') && (
          <div className="w-full max-w-[480px] font-serif text-[14px] leading-[1.55] text-text-primary py-3 px-4 rounded-[10px] bg-bg-row border border-border-hairline mt-6 text-center">
            {transcript || (
              <span className="text-text-quaternary italic">
                (no speech detected — try again, a little louder)
              </span>
            )}
          </div>
        )}

        {error && (
          <div className="w-full max-w-[480px] text-[12px] text-[#e85d4a] py-2 px-3 rounded-[8px] bg-bg-row border border-border-hairline mt-4">
            {error}
          </div>
        )}
      </div>
    </StepFrame>
  );
}

function Waveform({ rms, active }: { rms: number[]; active: boolean }) {
  return (
    <div className="flex items-end gap-[3px] h-[36px]">
      {rms.map((value, i) => {
        const norm = Math.min(1, value / 0.4);
        const height = active ? 5 + norm * 31 : 5;
        const opacity = active ? 0.5 + norm * 0.45 : 0.22;
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
  );
}
