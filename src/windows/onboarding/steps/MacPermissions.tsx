import StepFrame from './StepFrame';
import type { StepProps } from '../App';
import { openMacPrefPane, type MacPrefPane } from '../../../lib/ipc';

/**
 * Mac-only onboarding step. Walks the user through the three Privacy &
 * Security permissions Murmr needs (Input Monitoring, Microphone,
 * Accessibility) and links straight to each pane in System Settings.
 *
 * We can't auto-detect permission state from the WebView, so this is a
 * "guided hand-off" — explain what each permission does, why Murmr needs
 * it, when macOS will prompt, and provide a one-click button into the
 * exact System Settings pane in case the user missed the prompt or wants
 * to enable it before triggering the action.
 *
 * Conditionally inserted into the wizard's STEPS array on macOS — never
 * shown on Windows / Linux (their permissions are silent or automatic).
 */
export default function MacPermissions(props: StepProps) {
  return (
    <StepFrame
      {...props}
      title="Three Mac permissions"
      subtitle={
        <>
          Murmr needs three things from macOS to listen to your mic, hear your
          dictation hotkey, and paste the result into your focused field.
          You can grant them now, or wait until macOS prompts you the first
          time each is needed.
        </>
      }
    >
      <div className="space-y-3">
        <PermissionCard
          number={1}
          title="Input Monitoring"
          why="Lets Murmr's global hotkey work. Without this, pressing your dictation key does nothing."
          when="macOS prompts the first time Murmr starts. Granting it requires quitting and reopening Murmr — it's a Mac quirk, not a Murmr bug."
          pane="input-monitoring"
        />
        <PermissionCard
          number={2}
          title="Microphone"
          why="Lets Murmr capture audio for transcription."
          when="macOS prompts the first time you press your dictation hotkey to record."
          pane="microphone"
        />
        <PermissionCard
          number={3}
          title="Accessibility"
          why="Lets Murmr paste the transcribed text into your focused app via Cmd+V."
          when="macOS prompts the first time Murmr tries to paste a result. Granting it also requires quitting and reopening Murmr."
          pane="accessibility"
        />
      </div>

      <p className="text-[12px] text-text-quaternary mt-5 leading-[1.6]">
        All three live under <span className="font-medium">System Settings →
          Privacy &amp; Security</span>. You can revisit any of them later from
        Murmr's Settings → General.
      </p>
    </StepFrame>
  );
}

function PermissionCard({
  number,
  title,
  why,
  when,
  pane,
}: {
  number: number;
  title: string;
  why: string;
  when: string;
  pane: MacPrefPane;
}) {
  return (
    <div
      className="rounded-lg p-4"
      style={{
        background: 'var(--bg-control, rgba(0,0,0,0.03))',
        border: '0.5px solid var(--border-hairline, rgba(0,0,0,0.06))',
      }}
    >
      <div className="flex items-start gap-3">
        <span
          className="mt-[1px] w-[20px] h-[20px] rounded-full grid place-items-center flex-shrink-0 text-[12px] font-medium"
          style={{
            background: '#1f1f1c',
            color: '#fafaf9',
          }}
        >
          {number}
        </span>
        <div className="flex-1 min-w-0">
          <div className="flex items-center justify-between gap-3 mb-1">
            <div className="text-[14px] text-text-primary font-medium">{title}</div>
            <button
              type="button"
              onClick={() => {
                openMacPrefPane(pane).catch((e) =>
                  console.error('open pref pane failed', e),
                );
              }}
              className="text-[12px] text-text-secondary hover:text-text-primary px-3 py-[5px] rounded-full whitespace-nowrap"
              style={{
                border: '0.5px solid var(--border-hairline, rgba(0,0,0,0.12))',
              }}
            >
              Open in System Settings
            </button>
          </div>
          <div className="text-[12.5px] text-text-tertiary leading-[1.55] mb-1.5">
            {why}
          </div>
          <div className="text-[12px] text-text-quaternary leading-[1.55]">
            {when}
          </div>
        </div>
      </div>
    </div>
  );
}
