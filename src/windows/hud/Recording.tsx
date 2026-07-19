import { useEffect, useState } from 'react';

interface Props {
  /** Most recent RMS values, oldest first. Length = number of bars to render. */
  rms: number[];
  /** ms timestamp when recording began. */
  startedAt: number;
  /** Show the live waveform bars (Settings → hud_show_waveform). */
  showWaveform: boolean;
  /** Show the elapsed timer (Settings → hud_show_timer). */
  showTimer: boolean;
}

export default function Recording({
  rms,
  startedAt,
  showWaveform,
  showTimer,
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
            // `value` is an already-normalized [0,1] level from the adaptive
            // normalizer (see lib/waveform.ts). Map it to bar height
            // [3..18px] and opacity [0.5..0.95].
            const norm = Math.min(1, Math.max(0, value));
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
    </div>
  );
}
