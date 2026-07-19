//! Adaptive level normalizer for the live mic waveform.
//!
//! The backend emits a raw per-block RMS (root-mean-square amplitude in
//! roughly [0, 1]) on `murmr:audio-rms`. The absolute level varies wildly by
//! device and platform: built-in MacBook mics land around 0.001–0.01 for
//! ordinary speech, while headset/desktop mics on Windows sit at 0.02–0.1.
//! The old renderers divided by a fixed 0.4 ceiling, so quiet mics (every
//! Mac laptop) left the bars pinned at their floor — the waveform looked dead.
//!
//! Instead of a hardcoded ceiling, this tracks an adaptive one: a peak-hold
//! meter that rises quickly toward louder samples and releases slowly back
//! down. The current level is scaled between a fixed silence floor and that
//! moving ceiling, so the bars fill the available range for whatever the mic
//! is actually producing — loud or quiet — and stay flat only during genuine
//! silence. Platform-agnostic by construction: it calibrates to the signal,
//! not to an assumed input volume.

/** Below this raw RMS we treat the input as silence and render nothing. */
const SILENCE_FLOOR = 0.0006;

/**
 * The adaptive ceiling never drops below this. Keeps ambient room hiss from
 * being auto-gained up into a lively waveform when nobody is speaking, and
 * gives quiet-but-real Mac speech a sensible range to fill.
 */
const MIN_CEILING = 0.004;

/** Per-sample rate at which the ceiling rises toward a louder sample (fast). */
const ATTACK = 0.4;

/** Per-sample rate at which the ceiling relaxes toward quieter input (slow). */
const RELEASE = 0.02;

export interface LevelNormalizer {
  /** Feed one raw RMS sample; returns a display level in [0, 1]. */
  push(rms: number): number;
  /** Forget the adapted ceiling (call at the start of a new recording). */
  reset(): void;
}

/**
 * Create a stateful normalizer. Keep one per waveform (e.g. in a `useRef`) so
 * its adapted ceiling persists across the stream of RMS events, and `reset()`
 * it when a fresh recording session begins.
 */
export function createLevelNormalizer(): LevelNormalizer {
  let ceiling = MIN_CEILING;

  return {
    push(rms: number): number {
      // Track the loudest recent level: jump up fast, ease down slow. Floored
      // at MIN_CEILING so silence can't collapse the range to zero.
      const target = Math.max(rms, MIN_CEILING);
      const rate = target > ceiling ? ATTACK : RELEASE;
      ceiling += (target - ceiling) * rate;

      if (rms <= SILENCE_FLOOR) return 0;

      const norm = (rms - SILENCE_FLOOR) / (ceiling - SILENCE_FLOOR);
      const clamped = norm < 0 ? 0 : norm > 1 ? 1 : norm;
      // Perceptual curve: lifts quiet speech so small changes are visible
      // without letting loud passages clip flat at the top.
      return Math.sqrt(clamped);
    },
    reset(): void {
      ceiling = MIN_CEILING;
    },
  };
}
