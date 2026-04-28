import { useEffect, useMemo, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useTheme } from '../../hooks/useTheme';
import { completeOnboarding } from '../../lib/ipc';
import Welcome from './steps/Welcome';
import Name from './steps/Name';
import MicTest from './steps/MicTest';
import Done from './steps/Done';

// The mic-test screen already verifies end-to-end (mic → Whisper → text), so
// a separate "try it out" step would just be a duplicate. We jump straight
// to the celebratory Done screen after the test.
const STEPS = [
  { key: 'welcome', component: Welcome },
  { key: 'name', component: Name },
  { key: 'mic-test', component: MicTest },
  { key: 'done', component: Done },
] as const;

export type StepProps = {
  index: number;
  total: number;
  next: () => void;
  back: () => void;
  finish: () => Promise<void>;
};

export default function App() {
  useTheme();
  const [index, setIndex] = useState(0);

  const next = () => setIndex((i) => Math.min(i + 1, STEPS.length - 1));
  const back = () => setIndex((i) => Math.max(i - 1, 0));
  const finish = async () => {
    try {
      await completeOnboarding();
    } catch (e) {
      console.error('completeOnboarding failed', e);
    }
  };

  // Esc backs up one step (except on Welcome / Done).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && index > 0 && index < STEPS.length - 1) back();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [index]);

  const Step = useMemo(() => STEPS[index].component, [index]);

  const closeWindow = async () => {
    try {
      await getCurrentWindow().hide();
    } catch (e) {
      console.error('hide failed', e);
    }
  };

  return (
    <div className="wizard-shell text-text-primary">
      <div className="wizard-titlebar" data-tauri-drag-region>
        <button
          type="button"
          onClick={closeWindow}
          className="wizard-close"
          title="Close (you can re-open from Settings → Advanced)"
          aria-label="Close"
        >
          <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
      <div className="flex-1 min-h-0 flex flex-col">
        <Step
          index={index}
          total={STEPS.length}
          next={next}
          back={back}
          finish={finish}
        />
      </div>
    </div>
  );
}

export function ProgressDots({ index, total }: { index: number; total: number }) {
  return (
    <div className="flex items-center justify-center gap-[6px]">
      {Array.from({ length: total }).map((_, i) => (
        <span
          key={i}
          className={
            'w-[6px] h-[6px] rounded-full transition-colors ' +
            (i === index
              ? 'bg-text-primary'
              : 'bg-[var(--toggle-off-bg)]')
          }
        />
      ))}
    </div>
  );
}

export function PrimaryButton({
  children,
  onClick,
  disabled,
}: {
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="bg-[#1f1f1c] text-[#fafaf9] dark:bg-[#d4d4cf] dark:text-[#1f1f1c] text-[14px] font-medium rounded-full px-[28px] py-[10px] disabled:opacity-50 disabled:cursor-not-allowed"
    >
      {children}
    </button>
  );
}

export function SecondaryButton({
  children,
  onClick,
  disabled,
}: {
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="text-[14px] text-text-tertiary hover:text-text-primary px-[12px] py-[7px] disabled:opacity-50"
    >
      {children}
    </button>
  );
}
