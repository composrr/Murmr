import { useEffect, useMemo, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useTheme } from '../../hooks/useTheme';
import { completeOnboarding, isMac } from '../../lib/ipc';
import Welcome from './steps/Welcome';
import Name from './steps/Name';
import HotkeyChoice from './steps/HotkeyChoice';
import {
  MicPermissionStep,
  InputMonitoringStep,
  AccessibilityStep,
} from './steps/MacPermissionSteps';
import MicTest from './steps/MicTest';
import Practice from './steps/Practice';
import Done from './steps/Done';

// Flow: Welcome → Name → Hotkey walkthrough → [mac permissions] → mic test →
// hands-on practice run → Done. The hotkey step teaches tap-vs-hold; the
// practice step lets the user try the real hotkey with live transcript (never
// pasted/saved) before finishing. Practice PRECEDES Done and advances via
// next() — Done stays the single terminal step so it can complete onboarding
// and, on macOS, restart to apply the Input Monitoring / Accessibility grants.
//
// The three permission steps are macOS-only — Windows permissions are either
// silent (mic, after a one-time prompt) or automatic, while macOS gates
// Microphone, Input Monitoring (hotkey), and Accessibility (paste) behind
// TCC. Each step does LIVE detection: it polls the real grant status and
// auto-advances when the user grants it. They sit BEFORE the mic test so the
// mic is granted by the time we record. Input Monitoring + Accessibility are
// detected live but only take effect on restart — handled at the Done step,
// which restarts Murmr so the grants apply without bouncing the user back to
// the Welcome screen mid-wizard.
const ALL_STEPS = [
  { key: 'welcome', component: Welcome, macOnly: false },
  { key: 'name', component: Name, macOnly: false },
  { key: 'hotkey', component: HotkeyChoice, macOnly: false },
  { key: 'mic-permission', component: MicPermissionStep, macOnly: true },
  { key: 'input-monitoring', component: InputMonitoringStep, macOnly: true },
  { key: 'accessibility', component: AccessibilityStep, macOnly: true },
  { key: 'mic-test', component: MicTest, macOnly: false },
  { key: 'practice', component: Practice, macOnly: false },
  { key: 'done', component: Done, macOnly: false },
] as const;

const STEPS = ALL_STEPS.filter((s) => !s.macOnly || isMac());

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
