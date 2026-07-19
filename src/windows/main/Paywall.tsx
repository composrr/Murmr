import { useState } from 'react';
import { setLicenseKey, type LicenseStatus } from '../../lib/ipc';

/**
 * Shown in place of the main UI when no valid license is present. The user
 * can paste a key, see immediate feedback, and either get into the app
 * (valid → onLicensed callback fires) or click through to a buy page.
 */
export default function Paywall({
  status,
  onLicensed,
  buyUrl = 'https://murmr.app/buy', // placeholder until the marketing site exists
}: {
  status: LicenseStatus;
  onLicensed: (next: LicenseStatus) => void;
  buyUrl?: string;
}) {
  const [input, setInput] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [lastError, setLastError] = useState<string | null>(null);

  const submit = async () => {
    setSubmitting(true);
    setLastError(null);
    try {
      const next = await setLicenseKey(input.trim());
      if (next.kind === 'valid') {
        onLicensed(next);
      } else {
        setLastError(humanReadable(next));
      }
    } catch (e) {
      setLastError(String(e));
    } finally {
      setSubmitting(false);
    }
  };

  const headerCopy = headerFor(status);

  return (
    <div className="min-h-screen flex items-center justify-center bg-bg-base px-6">
      <div className="max-w-[520px] w-full">
        <div className="text-center mb-8">
          <h1 className="font-serif text-[34px] tracking-[-0.6px] text-text-primary mb-3">
            {headerCopy.title}
          </h1>
          <p className="text-[14px] text-text-tertiary leading-[1.55]">
            {headerCopy.subtitle}
          </p>
        </div>

        <div className="bg-bg-row border border-border-hairline rounded-[14px] p-6 mb-4">
          <label className="block text-[12px] uppercase tracking-[0.06em] text-text-tertiary mb-2">
            License key
          </label>
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && input.trim() && !submitting) submit();
            }}
            placeholder="paste the license key from your purchase email"
            spellCheck={false}
            autoComplete="off"
            className="w-full bg-bg-control border border-border-control rounded-[8px] px-3 py-[10px] text-[12px] font-mono text-text-primary placeholder:text-text-quaternary"
            disabled={submitting}
          />
          {lastError && (
            <div className="mt-3 text-[12px] text-[#c14a2b] dark:text-[#e87a5e] leading-[1.5]">
              {lastError}
            </div>
          )}

          <div className="flex gap-2 mt-4">
            <button
              onClick={submit}
              disabled={submitting || !input.trim()}
              className="flex-1 bg-[#1f1f1c] text-[#fafaf9] dark:bg-[#d4d4cf] dark:text-[#1f1f1c] text-[13px] font-medium rounded-[8px] py-2.5 disabled:opacity-50"
            >
              {submitting ? 'Checking…' : 'Activate'}
            </button>
            <a
              href={buyUrl}
              target="_blank"
              rel="noreferrer"
              className="text-[13px] text-text-secondary hover:text-text-primary px-4 py-2.5"
            >
              Buy a key →
            </a>
          </div>
        </div>

        <p className="text-center text-[11px] text-text-quaternary leading-[1.55]">
          Murmr runs entirely on your machine — your audio never leaves it. The
          license key just confirms you've paid.
        </p>
      </div>
    </div>
  );
}

function headerFor(status: LicenseStatus): { title: string; subtitle: string } {
  switch (status.kind) {
    case 'missing':
      return {
        title: 'Activate Murmr',
        subtitle: 'Paste the license key you received with your purchase. One key, this machine.',
      };
    case 'malformed':
      return {
        title: "That key doesn't look right",
        subtitle: 'Try pasting it again — it should be one long line, no spaces.',
      };
    case 'bad-signature':
      return {
        title: 'Key not recognized',
        subtitle: "We couldn't verify that key against this build of Murmr. Make sure you're on the version it was issued for.",
      };
    case 'expired':
      return {
        title: 'License expired',
        subtitle: `Your license for ${status.email} expired on ${prettyDate(status.expired_at)}. Renew to keep using Murmr.`,
      };
    case 'valid':
      // Shouldn't normally render the paywall when valid, but cover the
      // edge case anyway.
      return {
        title: "You're all set",
        subtitle: `Activated as ${status.email}.`,
      };
  }
}

function humanReadable(status: LicenseStatus): string {
  switch (status.kind) {
    case 'malformed':
      return `Key isn't formatted correctly: ${status.reason}`;
    case 'bad-signature':
      return "We couldn't verify that key. Double-check you copied the whole thing, and that you're on the right Murmr build.";
    case 'expired':
      return `That key expired on ${prettyDate(status.expired_at)}.`;
    case 'missing':
      return 'Paste a key first.';
    case 'valid':
      return '';
  }
}

function prettyDate(iso: string): string {
  try {
    return new Date(iso).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return iso;
  }
}
