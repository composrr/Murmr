import { useEffect, useState } from 'react';
import { useTheme } from '../../../hooks/useTheme';
import { useDictationStatus } from '../../../hooks/useDictationStatus';
import { useSharedUpdater } from '../../../hooks/UpdaterContext';
import { type useUpdater } from '../../../hooks/useUpdater';
import { displayName } from './HotkeyCapture';
import {
  getSettings,
  launchAtLoginActive,
  ping,
  recordAndTranscribe,
  saveSettings,
  setLaunchAtLogin,
  transcriptionCount,
  userInfo,
  type PingResponse,
  type Settings,
  type TranscriptionResult,
} from '../../../lib/ipc';
import type { Theme } from '../../../lib/theme';
import { Row, SectionHeader, Segmented, SecondaryButton, SettingsHeader, Toggle } from './settings-ui';

const THEMES: Theme[] = ['light', 'dark', 'auto'];
const RECORD_SECONDS = 4;

const STATUS_LABEL: Record<string, string> = {
  // 'idle' is built dynamically below using the configured dictation key.
  recording: 'Listening…',
  transcribing: 'Transcribing…',
  injected: 'Injected ✓',
  cancelled: 'Cancelled',
  error: 'Error',
};

const STATUS_DOT: Record<string, string> = {
  idle: 'bg-good',
  recording: 'bg-danger',
  transcribing: 'bg-accent',
  injected: 'bg-good',
  cancelled: 'bg-text-quaternary',
  error: 'bg-danger',
};

type TestState =
  | { kind: 'idle' }
  | { kind: 'recording'; secondsRemaining: number }
  | { kind: 'transcribing' }
  | { kind: 'done'; result: TranscriptionResult }
  | { kind: 'error'; message: string };

export default function General() {
  const { theme, setTheme } = useTheme();
  const { status, lastInjected } = useDictationStatus();
  // Shared instance with App.tsx's banner — clicking Check Now here will
  // update the top-of-window banner and vice versa.
  const updater = useSharedUpdater();
  const [pong, setPong] = useState<PingResponse | null>(null);
  const [user, setUser] = useState<string | null>(null);
  const [count, setCount] = useState<number | null>(null);
  const [test, setTest] = useState<TestState>({ kind: 'idle' });
  const [settings, setSettings] = useState<Settings | null>(null);
  const [autostartActive, setAutostartActive] = useState<boolean | null>(null);

  useEffect(() => {
    ping().then(setPong).catch(() => {});
    userInfo().then((u) => setUser(u.raw_name)).catch(() => {});
    transcriptionCount().then(setCount).catch(() => {});
    getSettings().then(setSettings).catch(() => {});
    launchAtLoginActive().then(setAutostartActive).catch(() => setAutostartActive(false));
  }, []);

  const toggleAutostart = async (enabled: boolean) => {
    setAutostartActive(enabled);
    if (settings) {
      const next = { ...settings, launch_at_login: enabled };
      setSettings(next);
    }
    try {
      await setLaunchAtLogin(enabled);
    } catch (e) {
      console.error('autostart toggle failed', e);
      // Re-query truth from the OS in case it failed.
      try {
        const actual = await launchAtLoginActive();
        setAutostartActive(actual);
      } catch {}
    }
  };

  const updateName = (name: string) => {
    if (!settings) return;
    const next = { ...settings, display_name: name };
    setSettings(next);
    saveSettings(next).catch(() => {});
  };

  // Refresh count whenever we leave the recording → injected loop.
  useEffect(() => {
    if (status.kind === 'injected') {
      transcriptionCount().then(setCount).catch(() => {});
    }
  }, [status]);

  useEffect(() => {
    if (test.kind !== 'recording') return;
    if (test.secondsRemaining <= 0) {
      setTest({ kind: 'transcribing' });
      return;
    }
    const id = setTimeout(
      () => setTest({ kind: 'recording', secondsRemaining: test.secondsRemaining - 1 }),
      1000,
    );
    return () => clearTimeout(id);
  }, [test]);

  const startTest = async () => {
    setTest({ kind: 'recording', secondsRemaining: RECORD_SECONDS });
    try {
      const result = await recordAndTranscribe(RECORD_SECONDS);
      setTest({ kind: 'done', result });
    } catch (e) {
      setTest({ kind: 'error', message: String(e) });
    }
  };

  const isBusy = test.kind === 'recording' || test.kind === 'transcribing';

  return (
    <div className="max-w-[720px]">
      <SettingsHeader
        title="General"
        subtitle={`${user ? `${user} · ` : ''}Murmr v${pong?.version ?? '0.1.0'}`}
      />

      {/* ---------- Settings rows ---------- */}
      <Row name="Display name" hint='Used in the "Welcome back, …" greeting on Home'>
        <input
          type="text"
          value={settings?.display_name ?? ''}
          onChange={(e) => updateName(e.target.value)}
          placeholder="Jon"
          maxLength={32}
          className="input-glass text-right w-[180px] !h-[34px]"
        />
      </Row>

      <Row name="Microphone" hint="Used to capture your voice">
        <select disabled className="input-glass !h-[34px] !text-text-tertiary opacity-70">
          <option>{test.kind === 'done' ? test.result.capture_device : 'System default'}</option>
        </select>
      </Row>

      <Row name="Speech model" hint="Larger is more accurate but slower">
        <select disabled className="input-glass !h-[34px] !text-text-tertiary opacity-70">
          <option>base.en (recommended)</option>
        </select>
      </Row>

      <Row name="Appearance" hint="Light, dark, or follow system">
        <Segmented
          value={theme}
          onChange={setTheme}
          options={THEMES.map((t) => ({ value: t, label: t[0].toUpperCase() + t.slice(1) }))}
        />
      </Row>

      <Row name="Launch at login" hint="Murmr starts in the tray whenever you sign in">
        <Toggle
          on={autostartActive ?? false}
          onChange={toggleAutostart}
          disabled={autostartActive === null}
        />
      </Row>

      <Row name="Check for updates" hint={updaterHint(updater.state, pong?.version ?? '0.1.0')}>
        <SecondaryButton
          onClick={() =>
            updater.state.kind === 'available'
              ? updater.installNow()
              : updater.checkNow()
          }
          disabled={updater.state.kind === 'checking' || updater.state.kind === 'downloading'}
        >
          {updater.state.kind === 'checking'
            ? 'Checking…'
            : updater.state.kind === 'downloading'
              ? 'Downloading…'
              : updater.state.kind === 'available'
                ? 'Install now'
                : 'Check now'}
        </SecondaryButton>
      </Row>

      <Row name="Release notes" hint="See what's new in each Murmr version">
        <SecondaryButton
          onClick={() => window.dispatchEvent(new CustomEvent('murmr:open-release-notes'))}
        >
          What's new
        </SecondaryButton>
      </Row>

      {/* ---------- Diagnostic dictation status (Phase 5 scaffolding; trims away later) ---------- */}
      <div>
        <SectionHeader>Status &amp; diagnostics</SectionHeader>

        <div className="glass px-5 py-4 mb-3">
          <div className="flex items-center gap-3">
            <div
              className={`w-2 h-2 rounded-full ${STATUS_DOT[status.kind] ?? 'bg-text-quaternary'} ${
                status.kind === 'recording' ? 'animate-pulse' : ''
              }`}
            />
            <span className="text-[13px] text-text-primary font-medium">
              {status.kind === 'idle'
                ? `Idle — press ${displayName(settings?.dictation_hotkey ?? 'ControlRight')} to dictate`
                : (STATUS_LABEL[status.kind] ?? status.kind)}
            </span>
          </div>

          {status.kind === 'error' && (
            <div className="text-[12px] text-danger mt-3 px-3 py-2 rounded-[10px] bg-danger/10 border border-danger/30">
              {status.message}
            </div>
          )}

          {lastInjected && (
            <>
              <div className="eyebrow mt-4 mb-2">Most recent</div>
              <p className="text-[14px] text-text-primary leading-[1.55] m-0">
                {lastInjected}
              </p>
            </>
          )}
        </div>

        <div className="glass px-5 py-4">
          <div className="flex items-center justify-between mb-2">
            <div className="text-[14px] font-medium text-text-primary">
              Run a {RECORD_SECONDS}-second mic test
            </div>
            <button onClick={startTest} disabled={isBusy} className="btn-primary !h-[30px] !text-[12px]">
              {test.kind === 'recording'
                ? `Recording… ${test.secondsRemaining}s`
                : test.kind === 'transcribing'
                  ? 'Transcribing…'
                  : 'Start'}
            </button>
          </div>
          <p className="text-[12px] text-text-tertiary leading-[1.5]">
            Captures {RECORD_SECONDS} seconds, resamples to 16 kHz mono, runs Whisper locally,
            and reports timings. Not saved to the transcription history.
          </p>
          {test.kind === 'done' && (
            <>
              <p className="text-[14px] text-text-primary leading-[1.55] mt-3 px-3 py-3 rounded-[12px] bg-bg-content border border-border-hairline">
                {test.result.text || (
                  <span className="text-text-quaternary italic">(no speech detected)</span>
                )}
              </p>
              <div className="grid grid-cols-2 gap-x-4 gap-y-1 mt-3 text-[11px]">
                <span className="text-text-quaternary">Capture</span>
                <span className="text-text-primary text-right tabular-nums">
                  {test.result.capture_sample_rate.toLocaleString()} Hz · {test.result.capture_channels}ch
                </span>
                <span className="text-text-quaternary">Whisper time</span>
                <span className="text-text-primary text-right tabular-nums">
                  {test.result.elapsed_transcribe_ms} ms
                </span>
              </div>
            </>
          )}
          {test.kind === 'error' && (
            <p className="text-[12px] text-danger mt-3 px-3 py-2 rounded-[10px] bg-danger/10 border border-danger/30">
              {test.message}
            </p>
          )}
        </div>

        <p className="text-[11px] text-text-quaternary mt-3">
          Total transcriptions stored: <span className="tabular-nums">{count ?? '—'}</span>
        </p>
      </div>
    </div>
  );
}

function updaterHint(
  state: ReturnType<typeof useUpdater>['state'],
  currentVersion: string,
): string {
  switch (state.kind) {
    case 'idle':
      return `You're on Murmr v${currentVersion}`;
    case 'checking':
      return 'Talking to the update server…';
    case 'up-to-date':
      return `You're on the latest (v${currentVersion}) — checked just now`;
    case 'available':
      return `v${state.version} is available`;
    case 'downloading':
      return state.total
        ? `Downloaded ${formatBytes(state.downloaded)} of ${formatBytes(state.total)}`
        : `Downloaded ${formatBytes(state.downloaded)}`;
    case 'ready':
      return 'Update installed — restarting Murmr…';
    case 'check-failed':
      // Routine: server unreachable, no internet, etc. Stay friendly —
      // user can click Check now to retry.
      return `Couldn't reach the update server. You're on v${currentVersion}.`;
    case 'error':
      return `Install failed: ${state.message}`;
  }
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
