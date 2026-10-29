import { useEffect, useState } from 'react';
import {
  llmKeyStatus,
  llmRemoveKey,
  llmSaveKey,
  llmTestKey,
  type LlmKeyStatus,
  type Settings,
} from '../../../lib/ipc';
import { Row, SecondaryButton, SectionHeader, Toggle } from './settings-ui';

type TestState =
  | { kind: 'idle' }
  | { kind: 'running' }
  | { kind: 'ok'; ms: number; sample: string }
  | { kind: 'error'; message: string };

/** "AI cleanup" settings: toggle + Anthropic API key (save / test / remove). */
export default function AiCleanupSection({
  settings,
  update,
}: {
  settings: Settings;
  update: (patch: Partial<Settings>) => void;
}) {
  const [status, setStatus] = useState<LlmKeyStatus | null>(null);
  const [draft, setDraft] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [test, setTest] = useState<TestState>({ kind: 'idle' });

  useEffect(() => {
    llmKeyStatus()
      .then(setStatus)
      .catch((e) => setError(String(e)));
  }, []);

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await llmSaveKey(draft));
      setDraft('');
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!window.confirm('Remove the saved Anthropic API key from this computer?')) return;
    setBusy(true);
    setError(null);
    try {
      await llmRemoveKey();
      setStatus({ has_key: false, hint: null });
      setTest({ kind: 'idle' });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const runTest = async () => {
    setTest({ kind: 'running' });
    try {
      const r = await llmTestKey(draft.trim() || undefined);
      setTest(
        r.ok
          ? { kind: 'ok', ms: r.latency_ms, sample: r.message }
          : { kind: 'error', message: r.message },
      );
    } catch (e) {
      setTest({ kind: 'error', message: String(e) });
    }
  };

  const hasKey = status?.has_key ?? false;
  const canTest = hasKey || draft.trim().length > 0;

  return (
    <>
      <SectionHeader>AI cleanup</SectionHeader>
      <Row
        name="AI cleanup"
        hint={
          settings.ai_cleanup && status && !hasKey
            ? 'Add an Anthropic API key below to turn this on. Until then dictations use the built-in rules.'
            : 'Polish punctuation, grammar, and filler with Claude Haiku after the built-in rules. Falls back to the rule-based text if it is slow or fails. Skipped in literal mode.'
        }
      >
        <Toggle on={settings.ai_cleanup} onChange={(v) => update({ ai_cleanup: v })} />
      </Row>
      <Row
        name="Anthropic API key"
        hint={
          hasKey
            ? `Saved in your system keychain (${status?.hint ?? '••••'}).`
            : 'Stored in your system keychain, never in settings.json.'
        }
      >
        <div className="flex items-center gap-2">
          {!hasKey && (
            <input
              type="password"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              placeholder="sk-ant-…"
              autoComplete="off"
              spellCheck={false}
              className="text-[13px] border border-border-control rounded-[7px] px-3 py-[6px] bg-bg-content text-text-primary w-[200px]"
            />
          )}
          {!hasKey && (
            <SecondaryButton onClick={save} disabled={busy || draft.trim().length === 0}>
              Save
            </SecondaryButton>
          )}
          <SecondaryButton onClick={runTest} disabled={!canTest || test.kind === 'running'}>
            {test.kind === 'running' ? 'Testing…' : 'Test'}
          </SecondaryButton>
          {hasKey && (
            <SecondaryButton onClick={remove} disabled={busy}>
              Remove
            </SecondaryButton>
          )}
        </div>
      </Row>
      {(test.kind === 'ok' || test.kind === 'error' || error) && (
        <div className="text-[12px] text-text-tertiary pb-4 -mt-2">
          {error && <div>Keychain error: {error}</div>}
          {test.kind === 'ok' && (
            <div>
              Works — {test.ms} ms. Sample: “{test.sample}”
            </div>
          )}
          {test.kind === 'error' && <div>Test failed: {test.message}</div>}
        </div>
      )}
      <p className="text-[11px] text-text-quaternary pb-2">
        When AI cleanup is on, the text of each dictation is sent to Anthropic. Audio never leaves
        your computer.
      </p>
    </>
  );
}
