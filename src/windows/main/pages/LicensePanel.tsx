import { useEffect, useState } from 'react';
import { getLicenseStatus, setLicenseKey, type LicenseStatus } from '../../../lib/ipc';
import { LICENSE_ENFORCED } from '../../../lib/license';
import { Pill, Row, SecondaryButton, SectionHeader } from './settings-ui';

// Settings panel for entering / viewing a license key. Works regardless of
// whether the gate is on (LICENSE_ENFORCED) — while Murmr is free it just
// lets you confirm the licensing mechanism end-to-end.

function statusLabel(s: LicenseStatus | null): { text: string; kind: 'info' | 'warn' } {
  if (!s) return { text: 'Checking', kind: 'info' };
  switch (s.kind) {
    case 'valid':
      return { text: 'Valid', kind: 'info' };
    case 'missing':
      return { text: 'No key', kind: 'info' };
    case 'expired':
      return { text: 'Expired', kind: 'warn' };
    case 'bad-signature':
    case 'malformed':
      return { text: 'Invalid', kind: 'warn' };
  }
  return { text: 'Unknown', kind: 'warn' };
}

function statusDetail(s: LicenseStatus | null, fallback: string): string {
  if (!s) return fallback;
  switch (s.kind) {
    case 'valid':
      return s.email
        ? `Licensed to ${s.email}${s.expires_at ? ` · expires ${s.expires_at.slice(0, 10)}` : ''}`
        : 'License active.';
    case 'missing':
      return fallback;
    case 'expired':
      return `Key for ${s.email} expired ${s.expired_at.slice(0, 10)}.`;
    case 'bad-signature':
      return "This key's signature doesn't match this build.";
    case 'malformed':
      return s.reason;
  }
  return fallback;
}

export default function LicensePanel() {
  const [status, setStatus] = useState<LicenseStatus | null>(null);
  const [input, setInput] = useState('');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    getLicenseStatus()
      .then(setStatus)
      .catch((e) => setStatus({ kind: 'malformed', reason: String(e) }));
  }, []);

  const apply = async () => {
    setSaving(true);
    try {
      setStatus(await setLicenseKey(input.trim()));
    } catch (e) {
      setStatus({ kind: 'malformed', reason: String(e) });
    } finally {
      setSaving(false);
    }
  };

  const fallbackHint = LICENSE_ENFORCED
    ? 'Enter your license key to unlock Murmr.'
    : 'Murmr is free right now — no key required. This panel is here for the future.';
  const label = statusLabel(status);

  return (
    <>
      <SectionHeader>License</SectionHeader>
      <Row name="License key" hint={statusDetail(status, fallbackHint)}>
        <div className="flex items-center gap-2">
          <Pill kind={label.kind}>{label.text}</Pill>
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="paste key…"
            spellCheck={false}
            className="text-[12px] font-mono border border-border-control rounded-[7px] px-3 py-[6px] bg-bg-content text-text-primary w-[240px]"
          />
          <SecondaryButton onClick={apply} disabled={saving || !input.trim()}>
            {saving ? 'Applying…' : 'Apply'}
          </SecondaryButton>
        </div>
      </Row>
    </>
  );
}
