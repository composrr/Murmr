import { useEffect, useRef, useState, type ReactNode } from 'react';
import StepFrame from './StepFrame';
import type { StepProps } from '../App';
import {
  openMacPrefPane,
  type MacPrefPane,
  type PermissionState,
} from '../../../lib/ipc';

/**
 * One macOS permission, with LIVE status detection. Polls the supplied
 * `poll` command every ~1.2s and reflects the real grant state:
 *
 *   not-determined → "Waiting for your approval…"
 *   denied         → "Not enabled yet" + Open System Settings
 *   granted        → "Enabled ✓"  (auto-advances if the user just granted it)
 *
 * `request` (microphone only) fires the OS prompt by briefly opening the
 * device. `appliesAfterRestart` (Accessibility / Input Monitoring) shows a
 * note that the grant is detected now but takes effect when Murmr restarts
 * at the end of setup — we deliberately DON'T restart mid-wizard (that would
 * throw the user back to the Welcome screen).
 */
interface PermissionStepProps extends StepProps {
  title: string;
  subtitle: ReactNode;
  /** macOS Privacy & Security pane to deep-link into. */
  pane: MacPrefPane;
  /** Live status poll command. */
  poll: () => Promise<PermissionState>;
  /** Optional OS-prompt trigger (microphone). */
  request?: () => Promise<PermissionState>;
  /** True for capabilities that only take effect on app restart. */
  appliesAfterRestart?: boolean;
  /** Inline "if it's not working…" troubleshooting. */
  tips: ReactNode;
}

export default function PermissionStep({
  title,
  subtitle,
  pane,
  poll,
  request,
  appliesAfterRestart,
  tips,
  ...step
}: PermissionStepProps) {
  const [status, setStatus] = useState<PermissionState | null>(null);
  const [requesting, setRequesting] = useState(false);
  // Was it granted the LAST time we polled? Used to detect the
  // non-granted → granted transition so we only auto-advance when the
  // user grants it DURING this step (not when it was already granted on
  // mount — that would flash the step by).
  const wasGrantedRef = useRef(false);
  const autoAdvancedRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;

    const tick = async () => {
      try {
        const s = await poll();
        if (cancelled) return;
        setStatus(s);
        const granted = s === 'granted' || s === 'not-applicable';
        if (granted && !wasGrantedRef.current && !autoAdvancedRef.current) {
          // Just transitioned to granted — celebrate briefly, then move on.
          autoAdvancedRef.current = true;
          window.setTimeout(() => {
            if (!cancelled) step.next();
          }, 900);
        }
        wasGrantedRef.current = granted;
      } catch {
        // Ignore poll errors (command not ready yet, etc.) — next tick retries.
      }
    };

    void tick();
    timer = window.setInterval(tick, 1200);
    return () => {
      cancelled = true;
      if (timer) window.clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const granted = status === 'granted' || status === 'not-applicable';

  const handleRequest = async () => {
    if (!request) return;
    setRequesting(true);
    try {
      await request();
    } catch {
      /* polling will pick up whatever the user chose */
    } finally {
      setRequesting(false);
    }
  };

  return (
    <StepFrame
      {...step}
      title={title}
      subtitle={subtitle}
      primaryLabel={granted ? 'Continue' : 'Skip for now'}
    >
      <div className="space-y-4">
        <StatusBanner status={status} appliesAfterRestart={appliesAfterRestart} />

        {!granted && (
          <div className="flex flex-wrap gap-2">
            {request && (
              <button
                type="button"
                disabled={requesting}
                onClick={handleRequest}
                className="text-[13px] font-medium rounded-full px-4 py-[8px] bg-[#1f1f1c] text-[#fafaf9] disabled:opacity-50"
              >
                {requesting ? 'Asking macOS…' : 'Allow microphone…'}
              </button>
            )}
            <button
              type="button"
              onClick={() => {
                openMacPrefPane(pane).catch(() => {});
              }}
              className="text-[13px] font-medium rounded-full px-4 py-[8px] text-text-primary"
              style={{ border: '0.5px solid var(--border-control, rgba(0,0,0,0.18))' }}
            >
              Open System Settings
            </button>
          </div>
        )}

        <div
          className="rounded-lg p-4 text-[12px] text-text-tertiary leading-[1.6]"
          style={{
            background: 'var(--bg-control, rgba(0,0,0,0.03))',
            border: '0.5px solid var(--border-hairline, rgba(0,0,0,0.06))',
          }}
        >
          {tips}
        </div>
      </div>
    </StepFrame>
  );
}

function StatusBanner({
  status,
  appliesAfterRestart,
}: {
  status: PermissionState | null;
  appliesAfterRestart?: boolean;
}) {
  const granted = status === 'granted' || status === 'not-applicable';

  let dot = '#c9a227'; // amber default (waiting / not enabled)
  let label = 'Checking…';
  let sub: string | null = null;

  if (status === null) {
    dot = '#9d9485';
    label = 'Checking…';
  } else if (granted) {
    dot = '#3fae5a';
    label = 'Enabled';
    sub = appliesAfterRestart
      ? 'Detected — takes effect when Murmr restarts at the end of setup.'
      : null;
  } else if (status === 'not-determined') {
    dot = '#c9a227';
    label = 'Waiting for your approval…';
  } else if (status === 'denied') {
    dot = '#e85d4a';
    label = 'Not enabled yet';
  } else if (status === 'restricted') {
    dot = '#e85d4a';
    label = 'Blocked by your administrator';
    sub = 'A device-management profile is preventing this. Contact whoever manages this Mac.';
  } else {
    dot = '#c9a227';
    label = 'Not enabled yet';
  }

  return (
    <div className="flex items-start gap-2.5">
      <span
        className="mt-[5px] w-[9px] h-[9px] rounded-full flex-shrink-0"
        style={{ background: dot }}
      />
      <div>
        <div className="text-[14px] text-text-primary font-medium">{label}</div>
        {sub && <div className="text-[12px] text-text-quaternary mt-0.5 leading-[1.5]">{sub}</div>}
      </div>
    </div>
  );
}
