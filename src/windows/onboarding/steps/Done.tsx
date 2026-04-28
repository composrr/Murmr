import { useEffect, useState } from 'react';
import { PrimaryButton, ProgressDots, type StepProps } from '../App';
import { getSettings } from '../../../lib/ipc';
import { displayName } from '../../main/pages/HotkeyCapture';

export default function Done({ index, total, finish }: StepProps) {
  const [dictation, setDictation] = useState('ControlRight');
  const [repeat, setRepeat] = useState('');
  const [cancel, setCancel] = useState('Escape');

  useEffect(() => {
    getSettings()
      .then((s) => {
        setDictation(s.dictation_hotkey);
        setRepeat(s.repeat_hotkey);
        setCancel(s.cancel_hotkey);
      })
      .catch(() => {});
  }, []);

  return (
    <div className="flex-1 flex flex-col items-center px-12 pt-2 pb-8 min-h-0 overflow-y-auto">
      <div className="w-[64px] h-[64px] rounded-[14px] bg-[#1f1f1c] grid place-items-center mb-4">
        <svg viewBox="0 0 48 48" width="40" height="40" xmlns="http://www.w3.org/2000/svg">
          <g transform="translate(24, 24)">
            <rect x="-8" y="-17" width="16" height="22" rx="8" fill="#fafaf9" />
            <path
              d="M -13 4 Q -13 17 0 17 Q 13 17 13 4"
              fill="none"
              stroke="#fafaf9"
              strokeWidth="3"
              strokeLinecap="round"
            />
            <line x1="0" y1="17" x2="0" y2="24" stroke="#fafaf9" strokeWidth="3" strokeLinecap="round" />
          </g>
        </svg>
      </div>

      <h1 className="font-serif text-[32px] tracking-[-0.5px] text-text-primary m-0 mb-1.5 text-center">
        You're all set
      </h1>
      <p className="text-[13.5px] text-text-tertiary leading-[1.6] text-center max-w-[460px] m-0 mb-7">
        You can start dictating anywhere — and save the time you used to spend typing.
      </p>

      <div className="grid gap-3 w-full max-w-[480px] mb-7">
        <Reminder
          chord={<Kbd>{displayName(dictation)}</Kbd>}
          label="Tap to toggle, hold for push-to-talk"
        />
        {repeat && (
          <Reminder
            chord={<Kbd>{displayName(repeat)}</Kbd>}
            label="Re-paste the most recent transcription"
          />
        )}
        <Reminder
          chord={<Kbd>{displayName(cancel)}</Kbd>}
          label="Cancel a recording in progress"
        />
      </div>

      <PrimaryButton onClick={finish}>Open Murmr</PrimaryButton>

      <p className="text-[11px] text-text-quaternary mt-5 text-center max-w-[460px] leading-[1.5]">
        Murmr lives in your system tray. Closing the main window keeps it running quietly in the
        background.
      </p>

      <div className="mt-6">
        <ProgressDots index={index} total={total} />
      </div>
    </div>
  );
}

function Reminder({ chord, label }: { chord: React.ReactNode; label: string }) {
  return (
    <div className="flex items-center gap-3 rounded-[10px] bg-bg-row border border-border-hairline px-4 py-2.5">
      <div className="flex-shrink-0 w-[160px]">{chord}</div>
      <div className="text-[12.5px] text-text-secondary leading-[1.5]">{label}</div>
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
