import { useEffect, useRef, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  getSettings,
  isRecordingActive,
  listenStatus,
  type DictationStatus,
} from '../../lib/ipc';
import Recording from './Recording';
import Thinking from './Thinking';
import ResultPopup from './ResultPopup';

/// Which elements of the recording pill to show. Mirrors the three
/// `hud_show_*` Settings toggles. Defaults here are the safe "show it"
/// fallback used until the real settings load — except word count,
/// which defaults off to match the Settings default (it's an estimate,
/// more a fun stat than a fact, so off unless the user opts in).
interface HudDisplay {
  waveform: boolean;
  timer: boolean;
  wordCount: boolean;
}
const DEFAULT_HUD_DISPLAY: HudDisplay = {
  waveform: true,
  timer: true,
  wordCount: false,
};

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
  const [display, setDisplay] = useState<HudDisplay>(DEFAULT_HUD_DISPLAY);

  const lastSeenStateRef = useRef<DictationStatus['kind']>('idle');
  // Speech-tracking state for the word counter (hysteresis).
  const isSpeakingRef = useRef(false);
  const speakingSinceRef = useRef<number | null>(null);
  const lastAboveThresholdAtRef = useRef<number | null>(null);
  const accumulatedMsRef = useRef(0);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    const resetCounters = () => {
      isSpeakingRef.current = false;
      speakingSinceRef.current = null;
      lastAboveThresholdAtRef.current = null;
      accumulatedMsRef.current = 0;
      setRmsHistory(Array(WAVEFORM_BARS).fill(0));
      setActiveSpeechMs(0);
    };

    const enterRecordingView = () => {
      if (lastSeenStateRef.current !== 'recording') resetCounters();
      setView({ kind: 'recording', startedAt: Date.now() });
      lastSeenStateRef.current = 'recording';
    };

    // Cold-mount self-heal: ask Rust whether a recording is already in
    // flight, then enter the recording view if so. Only runs AFTER all
    // listeners are attached (see `boot` below) so we don't lose the
    // intervening live event in the gap between query and listener
    // registration.
    const selfHeal = async () => {
      try {
        const active = await isRecordingActive();
        if (cancelled || !active) return;
        enterRecordingView();
      } catch (e) {
        console.error('isRecordingActive failed', e);
      }
    };

    // --- Listener bodies (extracted so we can register them
    // concurrently AND await them all before triggering self-heal) ---

    const onStatus = (status: DictationStatus) => {
      if (cancelled) return;
      if (status.kind === 'recording') {
        enterRecordingView();
      } else if (status.kind === 'transcribing') {
        setView({ kind: 'thinking' });
        lastSeenStateRef.current = 'transcribing';
      } else if (status.kind === 'cancelled' || status.kind === 'injected' || status.kind === 'error') {
        setView({ kind: 'hidden' });
        lastSeenStateRef.current = status.kind;
      } else {
        lastSeenStateRef.current = status.kind;
      }
    };

    const onRms = (event: { payload: number }) => {
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
    };

    const onDebugResult = (event: { payload: { text: string } }) => {
      if (cancelled) return;
      setView({ kind: 'result', text: event.payload.text });
    };

    // Pull the user's HUD display preferences. Re-pulled on every
    // hud-shown so toggling "Show word count" / waveform / timer in
    // Settings takes effect the next time the pill appears (no need
    // for a live settings-changed broadcast).
    const refreshDisplay = async () => {
      try {
        const s = await getSettings();
        if (cancelled) return;
        setDisplay({
          waveform: s.hud_show_waveform,
          timer: s.hud_show_timer,
          wordCount: s.hud_show_word_count,
        });
      } catch (e) {
        console.error('getSettings (HUD display) failed', e);
      }
    };

    // `murmr:hud-shown` is emitted by the controller after EVERY
    // `show_hud()` call. Listening for it gives us a sharper signal than
    // the timed re-emit fallback: even if `Status::Recording` was lost
    // because the WebView was suspended at emit time, this event will
    // fire as soon as the WebView resumes and processes the queued
    // message. We re-query state on receipt to land in the right view,
    // and re-pull display prefs so toggle changes apply.
    const onHudShown = () => {
      if (cancelled) return;
      void selfHeal();
      void refreshDisplay();
    };

    // Boot sequence: register every listener IN PARALLEL (Promise.all
    // keeps the slowest one from blocking the others), THEN trigger the
    // self-heal query. This guarantees the listener for Status events
    // is alive before we ask Rust "what's the current state?" —
    // anything emitted between mount and listener-ready is now caught
    // by the listener itself; anything emitted before mount is caught
    // by the self-heal. No coverage gap.
    const boot = async () => {
      try {
        const results = await Promise.all([
          listenStatus(onStatus),
          listen<number>('murmr:audio-rms', onRms),
          listen<{ text: string }>('murmr:hud-debug-result', onDebugResult),
          listen('murmr:hud-shown', onHudShown),
        ]);
        if (cancelled) {
          // Component unmounted while we were attaching — undo each
          // listener so we don't leak event handlers.
          for (const u of results) {
            try { u(); } catch {}
          }
          return;
        }
        unlisteners.push(...results);
      } catch (e) {
        console.error('HUD listener attach failed', e);
        return;
      }
      await selfHeal();
      await refreshDisplay();
    };
    void boot();

    return () => {
      cancelled = true;
      for (const u of unlisteners) {
        try { u(); } catch {}
      }
    };
  }, []);

  return (
    <div className="hud-stage">
      {view.kind === 'recording' && (
        <Recording
          rms={rmsHistory}
          startedAt={view.startedAt}
          activeSpeechMs={activeSpeechMs}
          showWaveform={display.waveform}
          showTimer={display.timer}
          showWordCount={display.wordCount}
        />
      )}
      {view.kind === 'thinking' && <Thinking />}
      {view.kind === 'result' && (
        <ResultPopup text={view.text} onDismiss={() => setView({ kind: 'hidden' })} />
      )}
    </div>
  );
}
