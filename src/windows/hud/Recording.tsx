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
  /** Command mode: blue dot + "Command" label instead of the red dot. */
  command?: boolean;
}

export default function Recording({
  rms,
  startedAt,
  showWaveform,
  showTimer,
  command = false,
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
    <div className="hud-pill gap-[14px]">
      <span className={'hud-dot ' + (command ? 'hud-dot-cmd' : 'hud-dot-rec')} />
      {command && <span style={{ color: '#f4f4f5' }}>Command</span>}
      {!command && !showWaveform && !showTimer && <span style={{ color: '#f4f4f5' }}>Listening</span>}

      {showWaveform && (
        <div className="flex items-end gap-[2px] h-[18px]">
          {rms.map((value, i) => {
            // `value` is an already-normalized [0,1] level from the adaptive
            // normalizer (see lib/waveform.ts). Map it to bar height
            // [3..18px] and opacity [0.45..1].
            const norm = Math.min(1, Math.max(0, value));
            const height = 3 + norm * 15;
            const opacity = 0.45 + norm * 0.55;
            return (
              <span
                key={i}
                style={{
                  display: 'inline-block',
                  width: 2.5,
                  height,
                  background: '#f4f4f5',
                  borderRadius: 2,
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
            color: '#a1a1aa',
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
