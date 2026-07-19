import { useEffect, useRef, useState, type SVGProps } from 'react';
import {
  checkAccessibilityPermission,
  checkInputMonitoringPermission,
  checkMicrophonePermission,
  isMac,
  openMacPrefPane,
  requestMicrophonePermission,
  restartApp,
  type MacPrefPane,
  type PermissionState,
} from '../../../lib/ipc';
import { SecondaryButton, SettingsHeader } from './settings-ui';

/**
 * Permissions health panel for the main Settings window.
 *
 * Unlike the one-shot onboarding walkthrough, this is always available so a
 * user whose grants get silently revoked (macOS updates, the app moves on
 * disk) can see — at a glance — which of the three macOS permissions are on,
 * and fix the missing ones without re-running onboarding.
 *
 * Behaviour the user asked for:
 *   - green check next to anything already granted,
 *   - for anything NOT granted, a one-time auto-open of the exact System
 *     Settings pane when they land here (a nudge if they forgot), plus a
 *     manual button per row.
 *
 * All three checks return `not-applicable` off macOS, so the page shows a
 * short "nothing to do here" note on Windows / Linux.
 */

interface PermissionDef {
  key: string;
  title: string;
  description: string;
  pane: MacPrefPane;
  poll: () => Promise<PermissionState>;
  /** Microphone can trigger the OS prompt directly; the others can't. */
  request?: () => Promise<PermissionState>;
  /** Input Monitoring / Accessibility only take effect after a restart. */
  appliesAfterRestart?: boolean;
}

const PERMISSIONS: PermissionDef[] = [
  {
    key: 'microphone',
    title: 'Microphone',
    description: 'Lets Murmr hear you. Audio is transcribed on-device and never leaves your Mac.',
    pane: 'microphone',
    poll: checkMicrophonePermission,
    request: requestMicrophonePermission,
  },
  {
    key: 'input-monitoring',
    title: 'Input Monitoring',
    description: 'Lets Murmr notice your dictation hotkey in any app. Without it, the hotkey does nothing.',
    pane: 'input-monitoring',
    poll: checkInputMonitoringPermission,
    appliesAfterRestart: true,
  },
  {
    key: 'accessibility',
    title: 'Accessibility',
    description: 'Lets Murmr type the transcribed text into whatever app you’re using.',
    pane: 'accessibility',
    poll: checkAccessibilityPermission,
    appliesAfterRestart: true,
  },
];

// Auto-open the first unmet permission at most once per app launch, so
// bouncing between Settings tabs doesn't keep re-opening System Settings.
let autoOpenedThisSession = false;

const isGranted = (s: PermissionState | undefined) => s === 'granted' || s === 'not-applicable';

export default function Permissions() {
  const mac = isMac();
  const [statuses, setStatuses] = useState<Record<string, PermissionState>>({});
  const [requesting, setRequesting] = useState<string | null>(null);
  const didAutoOpen = useRef(false);

  // Trigger the OS prompt (mic) or deep-link into the exact System Settings
  // pane for a permission that still needs action.
  const fix = async (perm: PermissionDef) => {
    const current = statuses[perm.key];
    if (perm.request && current === 'not-determined') {
      setRequesting(perm.key);
      try {
        await perm.request();
      } catch {
        /* polling picks up whatever the user chose */
      } finally {
        setRequesting(null);
      }
      return;
    }
    openMacPrefPane(perm.pane).catch(() => {});
  };

  useEffect(() => {
    if (!mac) return;
    let cancelled = false;
    let timer: number | undefined;

    const tick = async () => {
      try {
        const results = await Promise.all(PERMISSIONS.map((p) => p.poll()));
        if (cancelled) return;
        const next: Record<string, PermissionState> = {};
        PERMISSIONS.forEach((p, i) => (next[p.key] = results[i]));
        setStatuses(next);

        // One-time nudge: open the first still-missing pane on arrival.
        if (!didAutoOpen.current && !autoOpenedThisSession) {
          didAutoOpen.current = true;
          const firstMissing = PERMISSIONS.find((p) => !isGranted(next[p.key]));
          if (firstMissing) {
            autoOpenedThisSession = true;
            openMacPrefPane(firstMissing.pane).catch(() => {});
          }
        }
      } catch {
        /* command not ready yet — next tick retries */
      }
    };

    void tick();
    timer = window.setInterval(tick, 1500);
    return () => {
      cancelled = true;
      if (timer) window.clearInterval(timer);
    };
  }, [mac]);

  if (!mac) {
    return (
      <div className="max-w-[640px]">
        <SettingsHeader
          title="Permissions"
          subtitle="Murmr doesn’t need any special system permissions on this platform — you’re all set."
        />
      </div>
    );
  }

  const grantedCount = PERMISSIONS.filter((p) => isGranted(statuses[p.key])).length;
  const total = PERMISSIONS.length;
  const allGranted = grantedCount === total;
  const anyChecked = Object.keys(statuses).length > 0;

  return (
    <div className="max-w-[640px]">
      <SettingsHeader
        title="Permissions"
        subtitle="macOS gates the three things Murmr needs. Green means you’re good to go; anything else opens the right settings so you can flip it on."
      />

      <SummaryBanner allGranted={allGranted} grantedCount={grantedCount} total={total} checked={anyChecked} />

      <div className="mt-4 space-y-2.5">
        {PERMISSIONS.map((perm) => (
          <PermissionCard
            key={perm.key}
            perm={perm}
            status={statuses[perm.key]}
            requesting={requesting === perm.key}
            onFix={() => fix(perm)}
          />
        ))}
      </div>

      <div className="flex items-center justify-between gap-6 pt-6">
        <div className="text-[12px] text-text-tertiary leading-[1.5]">
          Just flipped a switch and Murmr still isn’t reacting? Input Monitoring and
          Accessibility only take effect after a restart.
        </div>
        <div className="flex-shrink-0">
          <SecondaryButton onClick={() => restartApp().catch(() => {})}>
            Restart Murmr
          </SecondaryButton>
        </div>
      </div>
    </div>
  );
}

function SummaryBanner({
  allGranted,
  grantedCount,
  total,
  checked,
}: {
  allGranted: boolean;
  grantedCount: number;
  total: number;
  checked: boolean;
}) {
  const label = !checked
    ? 'Checking your permissions…'
    : allGranted
      ? 'You’re all set — every permission Murmr needs is enabled.'
      : `${total - grantedCount} of ${total} still need your attention.`;

  const dot = !checked ? '#9d9485' : allGranted ? '#3fae5a' : '#c9a227';

  return (
    <div
      className="flex items-center gap-3 rounded-[10px] px-4 py-3"
      style={{
        background: 'var(--bg-control, rgba(0,0,0,0.03))',
        border: '0.5px solid var(--border-hairline, rgba(0,0,0,0.06))',
      }}
    >
      <span className="w-[10px] h-[10px] rounded-full flex-shrink-0" style={{ background: dot }} />
      <span className="text-[13px] text-text-primary font-medium">{label}</span>
    </div>
  );
}

function PermissionCard({
  perm,
  status,
  requesting,
  onFix,
}: {
  perm: PermissionDef;
  status: PermissionState | undefined;
  requesting: boolean;
  onFix: () => void;
}) {
  const granted = isGranted(status);
  const restricted = status === 'restricted';

  let statusLabel: string;
  if (status === undefined) statusLabel = 'Checking…';
  else if (granted) statusLabel = 'Enabled';
  else if (restricted) statusLabel = 'Blocked by your administrator';
  else if (status === 'not-determined') statusLabel = 'Not requested yet';
  else statusLabel = 'Not enabled yet';

  // What tapping the card does, spelled out so it's obvious per-item.
  const actionLabel =
    perm.request && status === 'not-determined'
      ? requesting
        ? 'Asking macOS…'
        : 'Allow microphone'
      : granted
        ? 'Open in System Settings'
        : 'Enable in System Settings';

  const statusColor = granted ? '#3fae5a' : restricted ? '#e85d4a' : '#b78a00';

  // The whole card is one tap target. Restricted (MDM) can't be changed, so
  // it's non-interactive; everything else deep-links to its own pane (or
  // fires the mic prompt) — each permission handled individually.
  return (
    <button
      type="button"
      onClick={onFix}
      disabled={restricted || requesting}
      aria-label={`${perm.title}: ${statusLabel}. ${restricted ? '' : actionLabel}`}
      className={
        'w-full text-left flex items-center gap-3.5 rounded-[12px] px-4 py-3.5 border transition-colors ' +
        (granted
          ? 'border-border-hairline bg-bg-content hover:bg-bg-control'
          : restricted
            ? 'border-border-hairline bg-bg-content cursor-default'
            : 'border-[#e6d9a8] bg-[#fbf7e9]/40 hover:bg-[#fbf7e9]/70 dark:border-[#5a4f2e] dark:bg-[#2a2618]/40 dark:hover:bg-[#2a2618]/70')
      }
    >
      <StatusIcon granted={granted} pending={status === undefined} />

      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-[14px] text-text-primary font-medium tracking-[-0.1px]">
            {perm.title}
          </span>
          <span className="text-[11px] font-medium" style={{ color: statusColor }}>
            · {statusLabel}
          </span>
        </div>
        <div className="text-[12px] text-text-tertiary tracking-[-0.1px] leading-[1.5] mt-0.5">
          {perm.description}
        </div>
      </div>

      {!restricted && (
        <div className="flex-shrink-0 flex items-center gap-1.5 text-[12px] font-medium text-text-secondary">
          <span className="hidden sm:inline whitespace-nowrap">{actionLabel}</span>
          <Chevron />
        </div>
      )}
    </button>
  );
}

function Chevron() {
  return (
    <svg
      width={16}
      height={16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className="text-text-quaternary"
    >
      <polyline points="9 18 15 12 9 6" />
    </svg>
  );
}

function StatusIcon({ granted, pending }: { granted: boolean; pending: boolean }) {
  const base: SVGProps<SVGSVGElement> = {
    width: 18,
    height: 18,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 2,
    strokeLinecap: 'round',
    strokeLinejoin: 'round',
  };
  if (granted) {
    return (
      <span className="mt-[1px] flex-shrink-0" style={{ color: '#3fae5a' }}>
        <svg {...base}>
          <circle cx="12" cy="12" r="10" />
          <path d="M8 12l2.5 2.5L16 9" />
        </svg>
      </span>
    );
  }
  return (
    <span
      className="mt-[1px] flex-shrink-0"
      style={{ color: pending ? '#9d9485' : '#c9a227' }}
    >
      <svg {...base}>
        <circle cx="12" cy="12" r="10" />
        {pending ? <path d="M12 8v4" /> : <path d="M12 8v4M12 16h.01" />}
      </svg>
    </span>
  );
}
