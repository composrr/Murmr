import { useEffect, useState } from 'react';

interface Props {
  /** Most recent RMS values, oldest first. Length = number of bars to render. */
  rms: number[];
  /** ms timestamp when recording began. */
  startedAt: number;
  /** Cumulative milliseconds in which the user was actually speaking. */
  activeSpeechMs: number;
}

/// ~220 WPM — fast natural dictation rate (the count is an estimate; v1
/// has no streaming transcription).
const WORDS_PER_MS = 220 / 60 / 1000;

export default function Recording({ rms, startedAt, activeSpeechMs }: Props) {
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
      <span style={{ color: 'rgba(212,212,207,0.35)', fontSize: 13 }}>·</span>
      <span style={{ color: 'rgba(212,212,207,0.7)', fontSize: 13 }}>
        {words} {words === 1 ? 'word' : 'words'}
      </span>
    </div>
  );
}
