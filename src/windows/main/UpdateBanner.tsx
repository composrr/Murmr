import { type UpdaterState } from '../../hooks/useUpdater';

export default function UpdateBanner({
  state,
  onInstall,
  onDismiss,
}: {
  state: UpdaterState;
  onInstall: () => void;
  onDismiss: () => void;
}) {
  if (state.kind === 'available') {
    return (
      <div className="flex items-center gap-3 px-4 py-2.5 border-b border-border-hairline bg-bg-row">
        <span className="w-[8px] h-[8px] rounded-full bg-[#1f1f1c] dark:bg-[#d4d4cf]" />
        <div className="flex-1 min-w-0 text-[13px] text-text-primary">
          <span className="font-medium">Murmr {state.version}</span>{' '}
          <span className="text-text-tertiary">is available</span>{' '}
          <span className="text-text-quaternary">
            (you're on {state.currentVersion})
          </span>
        </div>
        <button
          onClick={() =>
            window.dispatchEvent(new CustomEvent('murmr:open-release-notes'))
          }
          className="text-[12px] text-text-tertiary hover:text-text-primary px-2.5 py-1.5 underline-offset-2 hover:underline"
        >
          What's new
        </button>
        <button
          onClick={onInstall}
          className="bg-[#1f1f1c] text-[#fafaf9] dark:bg-[#d4d4cf] dark:text-[#1f1f1c] text-[12px] font-medium rounded-full px-4 py-1.5"
        >
          Install & restart
        </button>
        <button
          onClick={onDismiss}
          className="text-[12px] text-text-tertiary hover:text-text-primary px-2.5 py-1.5"
        >
          Later
        </button>
      </div>
    );
  }

  if (state.kind === 'downloading') {
    const pct =
      state.total && state.total > 0
        ? Math.round((state.downloaded / state.total) * 100)
        : null;
    return (
      <div className="flex items-center gap-3 px-4 py-2.5 border-b border-border-hairline bg-bg-row">
        <span className="w-[8px] h-[8px] rounded-full bg-[#1f1f1c] dark:bg-[#d4d4cf] animate-pulse" />
        <div className="flex-1 min-w-0 text-[13px] text-text-primary">
          Downloading update{pct !== null ? ` — ${pct}%` : '…'}
        </div>
      </div>
    );
  }

  if (state.kind === 'ready') {
    return (
      <div className="flex items-center gap-3 px-4 py-2.5 border-b border-border-hairline bg-bg-row">
        <span className="w-[8px] h-[8px] rounded-full bg-[#1f1f1c] dark:bg-[#d4d4cf]" />
        <div className="flex-1 text-[13px] text-text-primary">
          Update installed — restarting…
        </div>
      </div>
    );
  }

  if (state.kind === 'error') {
    return (
      <div className="flex items-center gap-3 px-4 py-2.5 border-b border-border-hairline bg-[#fef2eb] dark:bg-[#3a1f15]">
        <span className="w-[8px] h-[8px] rounded-full bg-[#c14a2b] dark:bg-[#e87a5e]" />
        <div className="flex-1 min-w-0 text-[13px] text-text-primary">
          <span className="font-medium">Update failed.</span>{' '}
          <span className="text-text-tertiary break-all">{state.message}</span>
        </div>
        <button
          onClick={onDismiss}
          className="text-[12px] text-text-tertiary hover:text-text-primary px-2.5 py-1.5"
        >
          Dismiss
        </button>
      </div>
    );
  }

  return null;
}
