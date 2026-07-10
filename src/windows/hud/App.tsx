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

/// Which elements of the recording pill to show. Mirrors the two
/// `hud_show_*` Settings toggles. Defaults here are the safe "show it"
/// fallback used until the real settings load.
interface HudDisplay {
  waveform: boolean;
  timer: boolean;
}
const DEFAULT_HUD_DISPLAY: HudDisplay = {
  waveform: true,
  timer: true,
};

const WAVEFORM_BARS = 13;

type HudView =
  | { kind: 'hidden' }
  | { kind: 'recording'; startedAt: number }
  | { kind: 'thinking' }
  | { kind: 'result'; text: string }
  | { kind: 'edit'; text: string }
  | { kind: 'no-speech' };

export default function HudApp() {
  const [view, setView] = useState<HudView>({ kind: 'hidden' });
  const [rmsHistory, setRmsHistory] = useState<number[]>(() => Array(WAVEFORM_BARS).fill(0));
  const [display, setDisplay] = useState<HudDisplay>(DEFAULT_HUD_DISPLAY);

  const lastSeenStateRef = useRef<DictationStatus['kind']>('idle');

  useEffect(() => {
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    const resetCounters = () => {
      setRmsHistory(Array(WAVEFORM_BARS).fill(0));
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
      } else if (status.kind === 'no-speech') {
        // Briefly tell the user we didn't catch anything, then hide — much
        // better than the pill just silently vanishing.
        setView({ kind: 'no-speech' });
        lastSeenStateRef.current = status.kind;
        window.setTimeout(() => {
          if (!cancelled) {
            setView((v) => (v.kind === 'no-speech' ? { kind: 'hidden' } : v));
          }
        }, 1600);
      } else if (status.kind === 'cancelled' || status.kind === 'injected' || status.kind === 'error') {
        setView({ kind: 'hidden' });
        lastSeenStateRef.current = status.kind;
      } else {
        lastSeenStateRef.current = status.kind;
      }
    };

    const onRms = (event: { payload: number }) => {
      if (cancelled) return;
      const rms = event.payload;

      // Update the live waveform.
      setRmsHistory((prev) => {
        const next = prev.slice(1);
        next.push(rms);
        return next;
      });
    };

    const onDebugResult = (event: { payload: { text: string } }) => {
      if (cancelled) return;
      setView({ kind: 'result', text: event.payload.text });
    };

    // Edit-last hotkey: the controller sends the most recent transcript so
    // the user can tweak a word and re-inject it.
    const onEditLast = (event: { payload: string }) => {
      if (cancelled) return;
      setView({ kind: 'edit', text: event.payload });
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
          listen<string>('murmr:edit-last', onEditLast),
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
          showWaveform={display.waveform}
          showTimer={display.timer}
        />
      )}
      {view.kind === 'thinking' && <Thinking />}
      {view.kind === 'result' && (
        <ResultPopup text={view.text} onDismiss={() => setView({ kind: 'hidden' })} />
      )}
      {view.kind === 'edit' && (
        <ResultPopup
          text={view.text}
          editable
          onDismiss={() => setView({ kind: 'hidden' })}
        />
      )}
      {view.kind === 'no-speech' && <NoSpeech />}
    </div>
  );
}

/// Tiny transient pill shown when a recording contained no usable speech.
function NoSpeech() {
  return (
    <div
      className="flex items-center gap-[10px] rounded-full"
      style={{
        background: 'var(--hud-bg, #1f1f1c)',
        padding: '11px 22px',
        border: '0.5px solid rgba(255,255,255,0.06)',
        boxShadow: '0 6px 28px rgba(0,0,0,0.22)',
      }}
    >
      <span className="block w-2 h-2 rounded-full" style={{ background: '#c8a24a' }} />
      <span style={{ color: 'rgba(212,212,207,0.85)', fontSize: 13 }}>Didn't catch that</span>
    </div>
  );
}
