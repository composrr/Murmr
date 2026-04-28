import { useEffect, useRef, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import StepFrame from './StepFrame';
import type { StepProps } from '../App';
import {
  getSettings,
  listenStatus,
  recordAndTranscribe,
  setPracticeMode,
} from '../../../lib/ipc';
import { displayName } from '../../main/pages/HotkeyCapture';

const TEST_SECONDS = 5;
const WAVEFORM_BARS = 31;

export default function Practice(props: StepProps) {
  const [text, setText] = useState('');
  const [hasSucceeded, setHasSucceeded] = useState(false);
  const [phase, setPhase] = useState<'idle' | 'recording' | 'transcribing'>('idle');
  const [rms, setRms] = useState<number[]>(() => Array(WAVEFORM_BARS).fill(0));
  const [seconds, setSeconds] = useState(TEST_SECONDS);
  const [dictationKey, setDictationKey] = useState('ControlRight');

  useEffect(() => {
    getSettings()
      .then((s) => setDictationKey(s.dictation_hotkey))
      .catch(() => {});
  }, []);
  const tickRef = useRef<number | null>(null);
  const buttonBusyRef = useRef(false);

  // Practice mode = transcription is emitted to the UI but never pasted /
  // saved. Toggle it on while this step is mounted.
  useEffect(() => {
    setPracticeMode(true).catch(() => {});
    return () => {
      setPracticeMode(false).catch(() => {});
    };
  }, []);

  useEffect(() => () => {
    if (tickRef.current) clearInterval(tickRef.current);
  }, []);

  // Listen for status events from the global hotkey path (Right Ctrl).
  useEffect(() => {
    let unStatus: UnlistenFn | null = null;
    let unRms: UnlistenFn | null = null;
    let cancelled = false;

    listenStatus((status) => {
      if (cancelled) return;
      if (status.kind === 'recording') {
        setPhase('recording');
        setRms(Array(WAVEFORM_BARS).fill(0));
      } else if (status.kind === 'transcribing') {
        setPhase('transcribing');
      } else if (status.kind === 'injected') {
        const t = status.text.trim();
        setText((prev) => (prev ? `${prev}\n${t}` : t));
        setHasSucceeded(true);
        setPhase('idle');
      } else if (status.kind === 'cancelled' || status.kind === 'error') {
        setPhase('idle');
      }
    }).then((u) => {
      if (cancelled) u();
      else unStatus = u;
    });

    listen<number>('murmr:audio-rms', (event) => {
      if (cancelled) return;
      setRms((prev) => {
        const next = prev.slice(1);
        next.push(event.payload);
        return next;
      });
    }).then((u) => {
      if (cancelled) u();
      else unRms = u;
    });

    return () => {
      cancelled = true;
      if (unStatus) unStatus();
      if (unRms) unRms();
    };
  }, []);

  // Button-based fallback path: directly call the backend, bypassing the
  // global hotkey. Practice mode is on so injection is suppressed and the
  // result is shown via the same status event that the hotkey path uses.
  const startButtonTest = async () => {
    if (buttonBusyRef.current || phase !== 'idle') return;
    buttonBusyRef.current = true;
    setPhase('recording');
    setSeconds(TEST_SECONDS);
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
      const t = result.text.trim();
      if (t) {
        setText((prev) => (prev ? `${prev}\n${t}` : t));
        setHasSucceeded(true);
      }
      setPhase('idle');
    } catch (e) {
      console.error('practice test failed', e);
      setPhase('idle');
    } finally {
      if (tickRef.current) {
        clearInterval(tickRef.current);
        tickRef.current = null;
      }
      buttonBusyRef.current = false;
    }
  };

  const recording = phase === 'recording';
  const transcribing = phase === 'transcribing';
  const isBusy = recording || transcribing;
  const label = recording
    ? `Listening… ${seconds}s`
    : transcribing
      ? 'Transcribing…'
      : hasSucceeded
        ? 'Tap to test again'
        : 'Tap to test';

  return (
    <StepFrame
      {...props}
      title="Try it out"
      subtitle={
        <>
          Tap the button below for a five-second test, or press{' '}
          <kbd className="inline-block bg-bg-control border border-border-control rounded-[7px] px-3 py-[3px] text-[12px] font-medium text-text-primary align-[1px]">
            {displayName(dictationKey)}
          </kbd>{' '}
          anywhere. The transcript appears below — nothing is pasted or saved.
        </>
      }
      primaryLabel="Finish"
      primaryDisabled={!hasSucceeded}
      onPrimary={props.finish}
    >
      <div className="flex flex-col items-center w-full">
        <div className="text-[12px] uppercase tracking-[0.8px] text-text-quaternary font-medium mb-3">
          {label}
        </div>

        <button
          onClick={startButtonTest}
          disabled={isBusy}
          aria-label="Test dictation"
          className={
            'relative w-[88px] h-[88px] rounded-full grid place-items-center transition-all flex-shrink-0 mb-5 ' +
            (recording
              ? 'bg-[#e85d4a] text-white shadow-[0_0_0_8px_rgba(232,93,74,0.16)]'
              : hasSucceeded
                ? 'bg-[var(--toggle-on-bg)] text-[var(--toggle-on-thumb)] hover:scale-[1.04]'
                : 'bg-[#1f1f1c] text-[#fafaf9] dark:bg-[#d4d4cf] dark:text-[#1f1f1c] hover:scale-[1.04] disabled:opacity-50')
          }
        >
          {hasSucceeded && !recording && !transcribing ? (
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

        <Waveform rms={rms} active={recording} />

        {text && (
          <div className="w-full max-w-[520px] mt-6 rounded-[12px] border border-border-hairline bg-bg-row p-5 font-serif text-[15px] leading-[1.55] text-text-primary whitespace-pre-line text-center">
            {text}
          </div>
        )}

        {hasSucceeded && (
          <div className="bg-bg-control border border-border-hairline rounded-[10px] px-4 py-3 mt-4 flex items-center gap-2.5 max-w-[520px]">
            <span className="w-[16px] h-[16px] rounded-full bg-[#1f1f1c] dark:bg-[#d4d4cf] grid place-items-center text-[#fafaf9] dark:text-[#1f1f1c] text-[9px] font-bold flex-shrink-0">
              ✓
            </span>
            <span className="text-[12px] text-text-primary">
              Nice. Murmr is working. Click Finish whenever you're ready.
            </span>
          </div>
        )}
      </div>
    </StepFrame>
  );
}

function Waveform({ rms, active }: { rms: number[]; active: boolean }) {
  return (
    <div className="flex items-end gap-[3px] h-[28px]">
      {rms.map((value, i) => {
        const norm = Math.min(1, value / 0.4);
        const height = active ? 4 + norm * 24 : 4;
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
