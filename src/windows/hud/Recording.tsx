import { useEffect, useState } from 'react';

interface Props {
  /** Most recent RMS values, oldest first. Length = number of bars to render. */
  rms: number[];
  /** ms timestamp when recording began. */
  startedAt: number;
  /** Cumulative milliseconds in which the user was actually speaking. */
  activeSpeechMs: number;
  /** Show the live waveform bars (Settings → hud_show_waveform). */
  showWaveform: boolean;
  /** Show the elapsed timer (Settings → hud_show_timer). */
  showTimer: boolean;
  /** Show the live word-count estimate (Settings → hud_show_word_count). */
  showWordCount: boolean;
}

/// Live word-count estimate rate. We can't know the real word count until
/// transcription finishes (no streaming), so the pill estimates from how
/// long the user has actively been speaking × an assumed words-per-minute.
///
/// 150 WPM is a realistic average dictation rate. The previous 220 WPM was
/// auctioneer-fast and made the estimate run ~1.4× high — which, because
/// English averages ~1.4 syllables per word, made the live count read like
/// it was counting SYLLABLES rather than words. 150 tracks normal speech
/// much more closely. (The saved/history word count is exact — computed
/// from the actual transcript — so only this in-progress pill is an
/// estimate.)
const WORDS_PER_MS = 150 / 60 / 1000;

export default function Recording({
  rms,
  startedAt,
  activeSpeechMs,
  showWaveform,
  showTimer,
  showWordCount,
}: Props) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 200);
    return () => clearInterval(id);
  }, []);

  const elapsedSeconds = Math.max(0, (now - startedAt) / 1000);
  const minutes = Math.floor(elapsedSeconds / 60);
  const seconds = Math.floor(elapsedSeconds % 60);
  const timer = `${minutes}:${seconds.toString().padStart(2, '0')}`;

  // Estimate words from active-speech time only — the timer keeps moving even
  // when the user is paused, but the word count shouldn't.
  const words = Math.round(activeSpeechMs * WORDS_PER_MS);

  return (
    <div
      className="flex items-center gap-[14px] rounded-full"
      style={{
        background: '#1f1f1c',
        padding: '11px 22px',
        border: '0.5px solid rgba(255,255,255,0.06)',
        boxShadow: '0 6px 28px rgba(0,0,0,0.22)',
      }}
    >
      <span className="block w-2 h-2 rounded-full" style={{ background: '#e85d4a' }} />

      {showWaveform && (
        <div className="flex items-end gap-[2px] h-[18px]">
          {rms.map((value, i) => {
            // Map RMS [0..0.4] → bar height [3..18px], opacity [0.5..0.95].
            // Speech RMS rarely exceeds 0.4 even on loud passages.
            const norm = Math.min(1, value / 0.4);
            const height = 3 + norm * 15;
            const opacity = 0.5 + norm * 0.45;
            return (
              <span
                key={i}
                style={{
                  display: 'inline-block',
                  width: 2,
                  height,
                  background: '#d4d4cf',
                  borderRadius: 1,
                  opacity,
                  transition: 'height 80ms linear, opacity 80ms linear',
                }}
              />
            );
          })}
        </div>
      )}

      {showTimer && (
        <span
          style={{
            color: '#d4d4cf',
            fontSize: 13,
            fontVariantNumeric: 'tabular-nums',
            fontWeight: 500,
          }}
        >
          {timer}
        </span>
      )}

      {showWordCount && (
        <>
          {/* Only show the separator dot when there's a timer before it,
              so the word count doesn't lead with a stray "·". */}
          {showTimer && (
            <span style={{ color: 'rgba(212,212,207,0.35)', fontSize: 13 }}>·</span>
          )}
          <span style={{ color: 'rgba(212,212,207,0.7)', fontSize: 13 }}>
            {words} {words === 1 ? 'word' : 'words'}
          </span>
        </>
      )}
    </div>
  );
}
