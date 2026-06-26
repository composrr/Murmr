import PermissionStep from './PermissionStep';
import type { StepProps } from '../App';
import {
  checkAccessibilityPermission,
  checkInputMonitoringPermission,
  checkMicrophonePermission,
  requestMicrophonePermission,
} from '../../../lib/ipc';

/**
 * The three macOS permission steps, each backed by live status detection
 * (see PermissionStep). Only inserted into the wizard on macOS.
 *
 * Order in the wizard: Microphone → Input Monitoring → Accessibility, all
 * before the mic test (which needs the mic granted to work).
 */

export function MicPermissionStep(props: StepProps) {
  return (
    <PermissionStep
      {...props}
      title="Allow the microphone"
      subtitle="Murmr transcribes your voice entirely on this Mac — audio never leaves your machine. macOS just needs your OK to listen."
      pane="microphone"
      poll={checkMicrophonePermission}
      request={requestMicrophonePermission}
      tips={
        <>
          <strong className="text-text-secondary font-medium">If it's not working:</strong> click
          “Allow microphone…” and choose <em>Allow</em> in the macOS dialog. If you don't see a
          dialog, open System Settings → Privacy &amp; Security → Microphone and switch Murmr on.
          Already recording but Murmr hears nothing? Check that the right input device is selected
          in your Mac's Sound settings (and in any audio router like Loopback / BlackHole).
        </>
      }
    />
  );
}

export function InputMonitoringStep(props: StepProps) {
  return (
    <PermissionStep
      {...props}
      title="Allow Input Monitoring"
      subtitle="This lets Murmr notice when you press your dictation hotkey — anywhere, in any app. Without it, your hotkey does nothing."
      pane="input-monitoring"
      poll={checkInputMonitoringPermission}
      appliesAfterRestart
      tips={
        <>
          <strong className="text-text-secondary font-medium">How to enable:</strong> click “Open
          System Settings,” find <strong className="text-text-primary">Murmr</strong> under Input
          Monitoring, and flip the switch on. macOS only hands the permission to a running app on
          restart — Murmr will restart itself when you finish setup, so you don't need to do
          anything else here.
        </>
      }
    />
  );
}

export function AccessibilityStep(props: StepProps) {
  return (
    <PermissionStep
      {...props}
      title="Allow Accessibility"
      subtitle="This lets Murmr paste your transcribed text into whatever app you're using. Without it, Murmr can hear you but can't type the result."
      pane="accessibility"
      poll={checkAccessibilityPermission}
      appliesAfterRestart
      tips={
        <>
          <strong className="text-text-secondary font-medium">How to enable:</strong> click “Open
          System Settings,” find <strong className="text-text-primary">Murmr</strong> under
          Accessibility, and flip the switch on. Like Input Monitoring, this takes effect when
          Murmr restarts at the end of setup. If pasting still fails later, toggle Murmr off and
          back on in this same list.
        </>
      }
    />
  );
}
