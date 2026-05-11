import { useEffect, useRef, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { listenStatus, type DictationStatus } from '../../lib/ipc';
import Recording from './Recording';
import Thinking from './Thinking';
import ResultPopup from './ResultPopup';

const WAVEFORM_BARS = 13;

/// RMS above this counts as "speaking right now." Aligned with the
/// controller's VAD threshold so the HUD word counter and the actual
/// transcription gate agree on what's speech vs noise. The previous
/// value (0.004) sat below typical room noise on many setups (especially
/// users routing through VoiceMeeter or other virtual mixers), making the
/// counter tick up at a constant rate even during pauses.
///
/// On macOS, built-in MacBook mics record significantly quieter than the
/// headset/desktop mics 0.015 was tuned for — normal speech often lands
/// in the 0.001-0.01 range. Use a Mac-specific value so the word counter
/// responds to typical dictation volume.
const IS_MAC =
  typeof navigator !== 'undefined' &&
  /Mac/i.test(navigator.platform || navigator.userAgent || '');
const SPEECH_RMS_THRESHOLD = IS_MAC ? 0.001 : 0.015;

/// Once we've started speaking, keep counting until we've seen this many ms
/// of sustained sub-threshold audio. This makes natural between-word pauses
/// not stop the counter prematurely.
const SILENCE_HYSTERESIS_MS = 280;

type HudView =
  | { kind: 'hidden' }
  | { kind: 'recording'; startedAt: number }
  | { kind: 'thinking' }
  | { kind: 'result'; text: string };

export default function HudApp() {
  const [view, setView] = useState<HudView>({ kind: 'hidden' });
  const [rmsHistory, setRmsHistory] = useState<number[]>(() => Array(WAVEFORM_BARS).fill(0));
  const [activeSpeechMs, setActiveSpeechMs] = useState(0);

  const lastSeenStateRef = useRef<DictationStatus['kind']>('idle');
  // Speech-tracking state for the word counter (hysteresis).
  const isSpeakingRef = useRef(false);
  const speakingSinceRef = useRef<number | null>(null);
  const lastAboveThresholdAtRef = useRef<number | null>(null);
  const accumulatedMsRef = useRef(0);

  useEffect(() => {
    let unlistenStatus: UnlistenFn | null = null;
    let unlistenRms: UnlistenFn | null = null;
    let cancelled = false;

    const resetCounters = () => {
      isSpeakingRef.current = false;
      speakingSinceRef.current = null;
      lastAboveThresholdAtRef.current = null;
      accumulatedMsRef.current = 0;
      setRmsHistory(Array(WAVEFORM_BARS).fill(0));
      setActiveSpeechMs(0);
    };

    listenStatus((status) => {
      if (cancelled) return;
      if (status.kind === 'recording') {
        if (lastSeenStateRef.current !== 'recording') resetCounters();
        setView({ kind: 'recording', startedAt: Date.now() });
      } else if (status.kind === 'transcribing') {
        setView({ kind: 'thinking' });
      } else if (status.kind === 'cancelled' || status.kind === 'injected' || status.kind === 'error') {
        setView({ kind: 'hidden' });
      }
      lastSeenStateRef.current = status.kind;
    }).then((u) => {
      if (cancelled) u();
      else unlistenStatus = u;
    });

    listen<number>('murmr:audio-rms', (event) => {
      if (cancelled) return;
      const now = Date.now();
      const rms = event.payload;

      // Update the live waveform.
      setRmsHistory((prev) => {
        const next = prev.slice(1);
        next.push(rms);
        return next;
      });

      // Hysteresis state machine for the word counter.
      if (rms > SPEECH_RMS_THRESHOLD) {
        lastAboveThresholdAtRef.current = now;
        if (!isSpeakingRef.current) {
          isSpeakingRef.current = true;
          speakingSinceRef.current = now;
        }
      } else if (isSpeakingRef.current && lastAboveThresholdAtRef.current !== null) {
        const sinceLastSpeech = now - lastAboveThresholdAtRef.current;
        if (sinceLastSpeech > SILENCE_HYSTERESIS_MS) {
          // Lock in the speaking burst.
          const burstMs = lastAboveThresholdAtRef.current - (speakingSinceRef.current ?? now);
          accumulatedMsRef.current += Math.max(0, burstMs);
          isSpeakingRef.current = false;
          speakingSinceRef.current = null;
        }
      }

      // Display value uses `lastAboveThresholdAt` (NOT `now`) so it matches
      // exactly what gets locked in. Result: monotonic — never counts down.
      const inflight =
        isSpeakingRef.current
        && speakingSinceRef.current !== null
        && lastAboveThresholdAtRef.current !== null
          ? Math.max(0, lastAboveThresholdAtRef.current - speakingSinceRef.current)
          : 0;
      setActiveSpeechMs(accumulatedMsRef.current + inflight);
    }).then((u) => {
      if (cancelled) u();
      else unlistenRms = u;
    });

    const debugListenerPromise = listen<{ text: string }>('murmr:hud-debug-result', (event) => {
      if (cancelled) return;
      setView({ kind: 'result', text: event.payload.text });
    });

    return () => {
      cancelled = true;
      if (unlistenStatus) unlistenStatus();
      if (unlistenRms) unlistenRms();
      debugListenerPromise.then((u) => u());
    };
  }, []);

  return (
    <div className="hud-stage">
      {view.kind === 'recording' && (
        <Recording rms={rmsHistory} startedAt={view.startedAt} activeSpeechMs={activeSpeechMs} />
      )}
      {view.kind === 'thinking' && <Thinking />}
      {view.kind === 'result' && (
        <ResultPopup text={view.text} onDismiss={() => setView({ kind: 'hidden' })} />
      )}
    </div>
  );
}
