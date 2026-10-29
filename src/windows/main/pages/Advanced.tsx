import { useEffect, useState } from 'react';
import {
  appPaths,
  cancelModelDownload,
  deleteModel,
  downloadModel,
  engineStatus,
  getSettings,
  listenEngineStatus,
  listenModelDownload,
  listModels,
  modelCatalog,
  openAppDataFolder,
  openPerfLog,
  resetOnboarding,
  restartApp,
  retryGpuDetection,
  saveSettings,
  type AppPaths,
  type CatalogModel,
  type EngineStatus,
  type ModelDownloadProgress,
  type Settings,
} from '../../../lib/ipc';
import {
  NativeSelect,
  Pill,
  Row,
  SecondaryButton,
  SectionHeader,
  Segmented,
  SettingsHeader,
  Toggle,
} from './settings-ui';

const BASE_MODEL = 'ggml-base.en.bin';

function formatMB(bytes: number): string {
  return `${Math.round(bytes / 1_000_000)} MB`;
}

function backendLabel(st: EngineStatus | null): string {
  if (!st) return '…';
  if (st.backend.kind === 'CPU') return 'CPU';
  return st.backend.device ? `${st.backend.kind} · ${st.backend.device}` : st.backend.kind;
}

const LOG_LEVELS = [
  { value: 'error', label: 'Error' },
  { value: 'warn', label: 'Warn' },
  { value: 'info', label: 'Info (default)' },
  { value: 'debug', label: 'Debug' },
  { value: 'trace', label: 'Trace' },
];

export default function Advanced() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [paths, setPaths] = useState<AppPaths | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [engine, setEngine] = useState<EngineStatus | null>(null);
  const [catalog, setCatalog] = useState<CatalogModel[]>([]);
  const [download, setDownload] = useState<ModelDownloadProgress | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);

  const refreshModels = () => {
    listModels().then(setModels).catch(() => {});
    modelCatalog().then(setCatalog).catch(() => {});
  };

  useEffect(() => {
    getSettings().then(setSettings).catch(() => {});
    appPaths().then(setPaths).catch(() => {});
    refreshModels();
    engineStatus().then(setEngine).catch(() => {});
    const unlisten = [
      listenEngineStatus(setEngine),
      listenModelDownload((p) => {
        setDownload(p.state === 'done' || p.state === 'cancelled' ? null : p);
        if (p.state === 'error') setDownloadError(p.error ?? 'Download failed');
      }),
    ];
    return () => {
      unlisten.forEach((u) => u.then((fn) => fn()).catch(() => {}));
    };
  }, []);

  const update = (patch: Partial<Settings>) => {
    if (!settings) return;
    const next = { ...settings, ...patch };
    setSettings(next);
    saveSettings(next).catch(() => {});
  };

  const startDownload = (m: CatalogModel) => {
    setDownloadError(null);
    setDownload({ file: m.file, downloaded: 0, total: m.size_bytes, state: 'downloading', error: null });
    downloadModel(m.file)
      .then(() => {
        setDownload(null);
        refreshModels();
        // The user asked for this model — switch to it (hot-swaps, no restart).
        // Re-read settings first: they may have changed during the download.
        getSettings()
          .then((cur) => {
            const next = { ...cur, model_name: m.file };
            setSettings(next);
            return saveSettings(next);
          })
          .catch(() => {});
      })
      .catch((e) => {
        setDownload(null);
        if (String(e) !== 'cancelled') setDownloadError(String(e));
      });
  };

  const modelLabel = (file: string) => {
    if (file === BASE_MODEL) return 'Base (English) — bundled';
    const c = catalog.find((m) => m.file === file);
    return c ? c.label : file;
  };

  if (!settings || !paths) {
    return (
      <div className="max-w-[640px]">
        <SettingsHeader title="Advanced" />
        <p className="text-[13px] text-text-quaternary">Loading…</p>
      </div>
    );
  }

  return (
    <div className="max-w-[640px]">
      <SettingsHeader
        title="Advanced"
        subtitle="Speech model, injection mode, log level."
      />

      <SectionHeader>Speech model</SectionHeader>
      <Row
        name="Model"
        hint={
          engine?.loading
            ? `Loading ${modelLabel(engine.loading)}…`
            : engine?.last_error
              ? `Couldn't load that model — still using ${modelLabel(engine.model_name)}.`
              : engine && engine.model_name !== settings.model_name
                ? `Using ${modelLabel(engine.model_name)} (the selected model couldn't be loaded).`
                : 'Changes apply immediately.'
        }
      >
        <NativeSelect
          value={settings.model_name}
          disabled={!!engine?.loading}
          onChange={(v) => update({ model_name: String(v) })}
          options={(models.includes(settings.model_name)
            ? models
            : [settings.model_name, ...models]
          ).map((name) => ({ value: name, label: modelLabel(name) }))}
        />
      </Row>

      {catalog.map((m) => {
        const active = download?.file === m.file;
        const pct = active && download ? Math.min(100, (download.downloaded / Math.max(1, download.total)) * 100) : 0;
        const onGpu = !!engine?.gpu_available && !settings.force_cpu;
        const advice = onGpu
          ? 'Recommended for your GPU.'
          : 'Slow without a GPU: around 10 seconds per dictation even on a fast CPU (Base takes under 1 second).';
        return (
          <div key={m.file} className="py-5 border-b border-border-hairline">
            <div className="flex items-center justify-between gap-6">
              <div className="flex-1 min-w-0">
                <div className="text-[14px] text-text-primary font-medium tracking-[-0.1px] flex items-center gap-2">
                  {m.label}
                  {onGpu && !m.downloaded && <Pill kind="info">Recommended</Pill>}
                </div>
                <div className="text-[12px] text-text-tertiary tracking-[-0.1px]">
                  {m.description} {advice}
                </div>
              </div>
              <div className="flex-shrink-0 flex gap-2">
                {active ? (
                  <SecondaryButton onClick={() => cancelModelDownload().catch(() => {})}>
                    Cancel
                  </SecondaryButton>
                ) : m.downloaded ? (
                  <>
                    {settings.model_name !== m.file && (
                      <SecondaryButton onClick={() => update({ model_name: m.file })}>Use</SecondaryButton>
                    )}
                    <SecondaryButton
                      disabled={settings.model_name === m.file || engine?.model_name === m.file}
                      onClick={() =>
                        deleteModel(m.file)
                          .then(refreshModels)
                          .catch((e) => setDownloadError(String(e)))
                      }
                    >
                      Delete
                    </SecondaryButton>
                  </>
                ) : (
                  <SecondaryButton disabled={!!download} onClick={() => startDownload(m)}>
                    Download ({formatMB(m.size_bytes)})
                  </SecondaryButton>
                )}
              </div>
            </div>
            {active && download && (
              <div className="mt-3">
                <div className="h-[6px] rounded-full bg-bg-control overflow-hidden">
                  <div
                    className="h-full bg-[var(--toggle-on-bg)] transition-[width] duration-200"
                    style={{ width: `${pct}%` }}
                  />
                </div>
                <div className="mt-1 text-[11px] text-text-quaternary tabular-nums">
                  {download.state === 'verifying'
                    ? 'Verifying checksum…'
                    : `${formatMB(download.downloaded)} of ${formatMB(download.total)} · ${pct.toFixed(0)}%`}
                </div>
              </div>
            )}
          </div>
        );
      })}
      {downloadError && (
        <p className="text-[12px] text-text-secondary mt-2">
          <Pill kind="warn">Download</Pill> {downloadError}
        </p>
      )}

      <Row
        name="Accuracy"
        hint="Accurate uses beam search — better on jargon/accents, a little slower."
      >
        <Segmented
          value={settings.accuracy_mode ? 'accurate' : 'fast'}
          onChange={(v) => update({ accuracy_mode: v === 'accurate' })}
          options={[
            { value: 'fast', label: 'Fast' },
            { value: 'accurate', label: 'Accurate' },
          ]}
        />
      </Row>
      <Row
        name="Skip silence"
        hint="Removes pauses before transcribing (Silero VAD). Fewer phantom words and repeat loops on long takes."
      >
        <Toggle on={settings.vad_enabled} onChange={(v) => update({ vad_enabled: v })} />
      </Row>

      <Row name="Compute backend" hint={engine?.gpu_note ?? undefined}>
        <span className="text-[12px] text-text-primary font-medium max-w-[320px] block text-right truncate">
          {backendLabel(engine)}
        </span>
      </Row>
      <Row
        name="Use GPU"
        hint={
          engine?.gpu_available || settings.force_cpu
            ? 'Turn off to run on the CPU only (if the GPU misbehaves).'
            : engine?.gpu_pending
              ? 'Checking the GPU…'
              : 'No compatible GPU found — Murmr runs on the CPU.'
        }
      >
        <div className="flex items-center gap-2">
          {engine?.restart_required && (
            <SecondaryButton onClick={() => restartApp().catch(() => {})}>Restart</SecondaryButton>
          )}
          {!engine?.gpu_available && !engine?.gpu_pending && !settings.force_cpu && (
            <SecondaryButton onClick={() => retryGpuDetection().catch(() => {})}>
              Check again
            </SecondaryButton>
          )}
          <Toggle
            on={!settings.force_cpu}
            disabled={!engine?.gpu_available && !settings.force_cpu}
            onChange={(v) => update({ force_cpu: !v })}
          />
        </div>
      </Row>

      <SectionHeader>Injection</SectionHeader>
      <Row name="Injection mode" hint="Clipboard-paste is the default; per-keystroke is a fallback for paste-blocking apps.">
        <Segmented
          value={settings.injection_mode}
          onChange={(v) => update({ injection_mode: v })}
          options={[
            { value: 'clipboard', label: 'Clipboard' },
            { value: 'keystroke', label: 'Per-keystroke' },
          ]}
        />
      </Row>

      <SectionHeader>Logging</SectionHeader>
      <Row name="Log level">
        <NativeSelect
          value={settings.log_level}
          onChange={(v) => update({ log_level: String(v) })}
          options={LOG_LEVELS}
        />
      </Row>
      <Row name="Open log file" hint="Murmr's debug log (tail this if something goes wrong)">
        <SecondaryButton disabled>Open</SecondaryButton>
      </Row>
      <Row
        name="Open transcribe perf log"
        hint="One line per dictation: audio length, threads, full() time, total ms, real-time ratio"
      >
        <SecondaryButton onClick={() => openPerfLog().catch(() => {})}>
          Open
        </SecondaryButton>
      </Row>

      <SectionHeader>Files</SectionHeader>
      <Row name="App data folder" hint="Database, settings, models, logs">
        <SecondaryButton onClick={() => openAppDataFolder().catch(() => {})}>
          Open
        </SecondaryButton>
      </Row>
      <Row name="Database">
        <span className="text-[12px] text-text-tertiary tabular-nums max-w-[420px] block text-right truncate">
          {paths.db_path}
        </span>
      </Row>
      <Row name="Settings">
        <span className="text-[12px] text-text-tertiary tabular-nums max-w-[420px] block text-right truncate">
          {paths.settings_path}
        </span>
      </Row>

      <SectionHeader>Reset</SectionHeader>
      <Row
        name="Re-run first-run wizard"
        hint="Closes the main window and reopens the welcome / mic test / practice flow"
      >
        <SecondaryButton onClick={() => resetOnboarding().catch(() => {})}>
          Re-run
        </SecondaryButton>
      </Row>
      <Row name="Reset Murmr to defaults" hint="Wipes settings; transcriptions/dictionary stay unless you explicitly clear them">
        <SecondaryButton disabled>Reset</SecondaryButton>
      </Row>

      <p className="text-[11px] text-text-quaternary mt-6">
        <Pill kind="info">Note</Pill>{' '}
        Model, GPU and silence-skipping changes apply without a restart. Reset-to-defaults is still coming.
      </p>
    </div>
  );
}
