import { useEffect, useState } from 'react';
import StepFrame from './StepFrame';
import type { StepProps } from '../App';
import { getSettings } from '../../../lib/ipc';
import { displayName } from '../../main/pages/HotkeyCapture';

export default function HotkeyChoice(props: StepProps) {
  // Show the user's actual configured key — they may have changed it before
  // re-running onboarding, and even on first launch the default is the
  // canonical truth (whatever defaults to in Settings::default()).
  const [dictation, setDictation] = useState('ControlRight');
  const [repeatMod, setRepeatMod] = useState('Shift');
  const [cancel, setCancel] = useState('Escape');
  const [threshold, setThreshold] = useState(250);

  useEffect(() => {
    getSettings()
      .then((s) => {
        setDictation(s.dictation_hotkey);
        setRepeatMod(s.repeat_modifier);
        setCancel(s.cancel_hotkey);
        setThreshold(s.tap_threshold_ms);
      })
      .catch(() => {});
  }, []);

  const MOD_LABELS: Record<string, string> = {
    Shift: 'Shift',
    Ctrl: 'Ctrl',
    Alt: 'Alt',
    Meta: 'Cmd / Win',
  };
  const repeatChord =
    repeatMod === 'None' || !MOD_LABELS[repeatMod]
      ? null
      : `${MOD_LABELS[repeatMod]} + ${displayName(dictation)}`;

  return (
    <StepFrame
      {...props}
      title="Pick a hotkey"
      subtitle={
        <>
          The hotkey starts and stops dictation, anywhere in any app.
          <br />
          Two ways to use it.
        </>
      }
    >
      <div className="grid gap-3 max-w-[560px]">
        <Card
          title="Tap"
          chord={
            <span>
              <Kbd>{displayName(dictation)}</Kbd>{' '}
              <span className="text-text-tertiary text-[12px] ml-1">under {threshold} ms</span>
            </span>
          }
          desc={`Toggle mode — recording starts immediately and keeps going until you tap ${displayName(dictation)} again.`}
        />
        <Card
          title="Hold"
          chord={
            <span>
              <Kbd>{displayName(dictation)}</Kbd>{' '}
              <span className="text-text-tertiary text-[12px] ml-1">over {threshold} ms</span>
            </span>
          }
          desc="Push-to-talk — recording stops the moment you let go."
        />
        {repeatChord && (
          <Card
            title="Re-paste the most recent"
            chord={<Kbd>{repeatChord}</Kbd>}
            desc="Useful when you want the same dictation to appear in another field."
          />
        )}
        <Card
          title="Cancel mid-recording"
          chord={<Kbd>{displayName(cancel)}</Kbd>}
          desc="Throws the audio away — no transcription, no insert."
        />

        <div className="text-[12px] text-text-tertiary leading-[1.55] mt-2">
          You can change any of these in Settings → Hotkeys.{' '}
          {dictation === 'ControlRight' && (
            <span>
              Right Ctrl is the default because it has the same tap-vs-hold ergonomics as Right
              Alt without stealing focus to the menu bar on Windows.
            </span>
          )}
        </div>
      </div>
    </StepFrame>
  );
}

function Card({ title, chord, desc }: { title: string; chord: React.ReactNode; desc: string }) {
  return (
    <div className="rounded-[12px] bg-bg-row border border-border-hairline p-4 flex items-start gap-4">
      <div className="flex-1">
        <div className="text-[13px] text-text-primary font-medium mb-0.5">{title}</div>
        <div className="text-[12px] text-text-tertiary leading-[1.5]">{desc}</div>
      </div>
      <div className="flex-shrink-0 mt-0.5">{chord}</div>
    </div>
  );
}

function Kbd({ children }: { children: React.ReactNode }) {
  return (
    <kbd className="inline-block bg-bg-control border border-border-control rounded-[7px] px-3 py-[5px] text-[12px] font-medium text-text-primary">
      {children}
    </kbd>
  );
}
