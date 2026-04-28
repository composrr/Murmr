import { useEffect, useState } from 'react';
import { useTheme } from '../../../hooks/useTheme';
import { useDictationStatus } from '../../../hooks/useDictationStatus';
import { useSharedUpdater } from '../../../hooks/UpdaterContext';
import { type useUpdater } from '../../../hooks/useUpdater';
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

const THEMES: Theme[] = ['light', 'dark', 'auto'];
const RECORD_SECONDS = 4;

const STATUS_LABEL: Record<string, string> = {
  idle: 'Idle — press Right Ctrl to dictate',
  recording: 'Listening…',
  transcribing: 'Transcribing…',
  injected: 'Injected ✓',
  cancelled: 'Cancelled',
  error: 'Error',
};

const STATUS_DOT: Record<string, string> = {
  idle: 'bg-text-quaternary',
  recording: 'bg-[#e85d4a]',
  transcribing: 'bg-text-secondary',
  injected: 'bg-text-secondary',
  cancelled: 'bg-text-quaternary',
  error: 'bg-[#e85d4a]',
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
    <div className="max-w-[640px]">
      <h1 className="font-serif text-[30px] tracking-[-0.4px] text-text-primary mb-1">
        General
      </h1>
      <p className="text-[13px] text-text-quaternary mb-7">
        {user ? `${user} · ` : ''}Murmr v{pong?.version ?? '0.1.0'}
      </p>

      {/* ---------- Settings rows ---------- */}
      <Row name="Display name" hint='Used in the "Welcome back, …" greeting on Home'>
        <input
          type="text"
          value={settings?.display_name ?? ''}
          onChange={(e) => updateName(e.target.value)}
          placeholder="Jon"
          maxLength={32}
          className="text-[13px] border border-border-control rounded-[7px] px-3 py-[6px] bg-bg-content text-text-primary text-right w-[180px]"
        />
      </Row>

      <Row name="Microphone" hint="Used to capture your voice">
        <select
          disabled
          className="text-[13px] text-text-tertiary border border-border-control rounded-[7px] px-3 py-[6px] bg-bg-content"
        >
          <option>{test.kind === 'done' ? test.result.capture_device : 'System default'}</option>
        </select>
      </Row>

      <Row name="Speech model" hint="Larger is more accurate but slower">
        <select
          disabled
          className="text-[13px] text-text-tertiary border border-border-control rounded-[7px] px-3 py-[6px] bg-bg-content"
        >
          <option>base.en (recommended)</option>
        </select>
      </Row>

      <Row name="Appearance" hint="Light, dark, or follow system">
        <div className="flex gap-[2px] bg-bg-control rounded-[7px] p-[2px]">
          {THEMES.map((t) => (
            <button
              key={t}
              onClick={() => setTheme(t)}
              className={
                'text-[12px] px-[11px] py-[4px] rounded-[5px] capitalize transition-colors ' +
                (theme === t
                  ? 'bg-bg-content text-text-primary font-medium shadow-[0_1px_2px_rgba(0,0,0,0.04)]'
                  : 'bg-transparent text-text-tertiary hover:text-text-secondary')
              }
            >
              {t}
            </button>
          ))}
        </div>
      </Row>

      <Row name="Launch at login" hint="Murmr starts in the tray whenever you sign in">
        <Toggle
          on={autostartActive ?? false}
          onChange={toggleAutostart}
          disabled={autostartActive === null}
        />
      </Row>

      <Row name="Check for updates" hint={updaterHint(updater.state, pong?.version ?? '0.1.0')}>
        <button
          onClick={() =>
            updater.state.kind === 'available'
              ? updater.installNow()
              : updater.checkNow()
          }
          disabled={updater.state.kind === 'checking' || updater.state.kind === 'downloading'}
          className="text-[12px] px-[14px] py-[6px] rounded-[8px] border border-border-control bg-bg-content text-text-primary font-medium hover:bg-bg-control disabled:opacity-50"
        >
          {updater.state.kind === 'checking'
            ? 'Checking…'
            : updater.state.kind === 'downloading'
              ? 'Downloading…'
              : updater.state.kind === 'available'
                ? 'Install now'
                : 'Check now'}
        </button>
      </Row>

      {/* ---------- Diagnostic dictation status (Phase 5 scaffolding; trims away later) ---------- */}
      <div className="mt-10 pt-7 border-t border-border-hairline">
        <h2 className="font-serif text-[22px] text-text-primary mb-4">
          Status & diagnostics
        </h2>

        <div className="bg-bg-row border border-border-hairline rounded-card px-5 py-4 mb-4">
          <div className="flex items-center gap-3">
            <div
              className={`w-2 h-2 rounded-full ${STATUS_DOT[status.kind] ?? 'bg-text-quaternary'} ${
                status.kind === 'recording' ? 'animate-pulse' : ''
              }`}
            />
            <span className="text-[13px] text-text-primary font-medium">
              {STATUS_LABEL[status.kind] ?? status.kind}
            </span>
          </div>

          {status.kind === 'error' && (
            <div className="text-[12px] text-[#e85d4a] mt-3 px-3 py-2 rounded-[8px] bg-bg-content border border-border-hairline">
              {status.message}
            </div>
          )}

          {lastInjected && (
            <>
              <div className="text-[11px] uppercase tracking-[0.6px] text-text-quaternary font-medium mt-4 mb-2">
                Most recent
              </div>
              <p className="font-serif text-[15px] text-text-primary leading-[1.55] m-0">
                {lastInjected}
              </p>
            </>
          )}
        </div>

        <div className="bg-bg-row border border-border-hairline rounded-card px-5 py-4">
          <div className="flex items-center justify-between mb-2">
            <div className="text-[11px] uppercase tracking-[0.6px] text-text-quaternary font-medium">
              Run a {RECORD_SECONDS}-second mic test
            </div>
            <button
              onClick={startTest}
              disabled={isBusy}
              className="bg-[#1f1f1c] text-[#fafaf9] dark:bg-[#d4d4cf] dark:text-[#1f1f1c] text-[12px] font-medium rounded-full px-4 py-1.5 disabled:opacity-50 disabled:cursor-not-allowed"
            >
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
              <p className="font-serif text-[14px] text-text-primary leading-[1.55] mt-3 px-3 py-3 rounded-row bg-bg-content border border-border-hairline">
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
            <p className="text-[12px] text-[#e85d4a] mt-3 px-3 py-2 rounded-[8px] bg-bg-content border border-border-hairline">
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
    case 'error':
      return `Couldn't check: ${state.message}`;
  }
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function Row({ name, hint, children }: { name: string; hint: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 py-5 border-b border-border-hairline">
      <div className="flex-1">
        <div className="text-[14px] text-text-primary font-medium tracking-[-0.1px]">{name}</div>
        <div className="text-[12px] text-text-tertiary tracking-[-0.1px]">{hint}</div>
      </div>
      {children}
    </div>
  );
}

function Toggle({
  on,
  onChange,
  disabled,
}: {
  on: boolean;
  onChange?: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={() => !disabled && onChange?.(!on)}
      disabled={disabled}
      className={
        `w-[38px] h-[22px] rounded-full relative transition-colors ${
          on ? 'bg-[var(--toggle-on-bg)]' : 'bg-[var(--toggle-off-bg)]'
        } ${disabled ? 'opacity-60 cursor-not-allowed' : ''}`
      }
    >
      <span
        className={`absolute top-[2px] w-[18px] h-[18px] rounded-full transition-all ${
          on ? 'right-[2px] bg-[var(--toggle-on-thumb)]' : 'left-[2px] bg-white'
        }`}
      />
    </button>
  );
}
