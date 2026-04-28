import { useEffect, useMemo, useState } from 'react';
import {
  deleteTranscription,
  getSettings,
  listenTranscriptionSaved,
  recentTranscriptions,
  reinsertText,
  searchTranscriptions,
  userInfo,
  type Transcription,
} from '../../../lib/ipc';
import { dateGroup, formatTime } from '../../../lib/format';

const DEBOUNCE_MS = 120;

type Toast = { kind: 'ok' | 'err'; message: string } | null;

export default function Home() {
  const [items, setItems] = useState<Transcription[]>([]);
  const [query, setQuery] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<Toast>(null);

  useEffect(() => {
    const id = setTimeout(() => setDebouncedQuery(query), DEBOUNCE_MS);
    return () => clearTimeout(id);
  }, [query]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    const load = async () => {
      try {
        const data = debouncedQuery
          ? await searchTranscriptions(debouncedQuery, 250)
          : await recentTranscriptions(250);
        if (!cancelled) {
          setItems(data);
          setError(null);
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    load();
    return () => {
      cancelled = true;
    };
  }, [debouncedQuery]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listenTranscriptionSaved(() => {
      (debouncedQuery
        ? searchTranscriptions(debouncedQuery, 250)
        : recentTranscriptions(250)
      )
        .then(setItems)
        .catch(() => {});
    }).then((u) => (unlisten = u));
    return () => {
      if (unlisten) unlisten();
    };
  }, [debouncedQuery]);

  const grouped = useMemo(() => groupByDate(items), [items]);
  const [name, setName] = useState<string | null>(null);
  useEffect(() => {
    // Prefer the user-supplied display name from onboarding/settings; fall
    // back to the OS user only when nothing's been set yet.
    (async () => {
      try {
        const s = await getSettings();
        if (s.display_name && s.display_name.trim()) {
          setName(s.display_name.trim());
          return;
        }
      } catch {}
      try {
        const u = await userInfo();
        setName(u.display_name);
      } catch {}
    })();
  }, []);
  const greeting = name ? `Welcome back, ${name}` : 'Welcome back';

  const showToast = (t: Toast) => {
    setToast(t);
    if (t) setTimeout(() => setToast(null), 1700);
  };

  const onCopy = async (entry: Transcription) => {
    try {
      await navigator.clipboard.writeText(entry.text);
      showToast({ kind: 'ok', message: 'Copied to clipboard' });
    } catch (e) {
      showToast({ kind: 'err', message: `Copy failed: ${e}` });
    }
  };

  const onReinsert = async (entry: Transcription) => {
    try {
      await reinsertText(entry.text);
      showToast({ kind: 'ok', message: 'Pasted into focused field' });
    } catch (e) {
      showToast({ kind: 'err', message: `Re-insert failed: ${e}` });
    }
  };

  const onDelete = async (entry: Transcription) => {
    try {
      await deleteTranscription(entry.id);
      setItems((prev) => prev.filter((i) => i.id !== entry.id));
    } catch (e) {
      showToast({ kind: 'err', message: `Delete failed: ${e}` });
    }
  };

  return (
    <div className="max-w-[760px] mx-auto">
      <h1 className="font-serif text-[30px] tracking-[-0.4px] text-text-primary mb-2">
        {greeting}
      </h1>
      <p className="text-[13px] text-text-quaternary mb-7">
        Tap or hold{' '}
        <kbd className="inline-block bg-bg-control border border-border-control rounded-[6px] px-[7px] py-[2px] text-[11px] font-medium text-text-primary align-[1px]">
          Right Ctrl
        </kbd>{' '}
        anywhere to dictate.{' '}
        <kbd className="inline-block bg-bg-control border border-border-control rounded-[6px] px-[7px] py-[2px] text-[11px] font-medium text-text-primary align-[1px]">
          Shift + Right Ctrl
        </kbd>{' '}
        re-pastes the most recent.
      </p>

      <div className="relative mb-6">
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="var(--text-quaternary)"
          strokeWidth="1.8"
          strokeLinecap="round"
          strokeLinejoin="round"
          className="absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none"
        >
          <circle cx="11" cy="11" r="8" />
          <line x1="21" y1="21" x2="16.65" y2="16.65" />
        </svg>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search transcriptions"
          className="w-full pl-9 pr-3 py-[9px] rounded-[8px] border border-border-control bg-bg-row text-[13px] text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-text-quaternary"
        />
      </div>

      {error && (
        <div className="text-[12px] text-[#e85d4a] py-2 px-3 rounded-[8px] bg-bg-row border border-border-hairline mb-4">
          {error}
        </div>
      )}

      {loading && items.length === 0 ? (
        <p className="text-[13px] text-text-quaternary">Loading…</p>
      ) : items.length === 0 ? (
        <EmptyState />
      ) : (
        grouped.map(([label, group]) => (
          <section key={label}>
            <div className="text-[11px] font-medium uppercase tracking-[0.8px] text-text-quaternary mt-6 mb-3 first:mt-1">
              {label}
            </div>
            {group.map((entry) => (
              <Row
                key={entry.id}
                entry={entry}
                onCopy={() => onCopy(entry)}
                onReinsert={() => onReinsert(entry)}
                onDelete={() => onDelete(entry)}
              />
            ))}
          </section>
        ))
      )}

      {toast && (
        <div
          className={
            'fixed bottom-6 left-1/2 -translate-x-1/2 px-4 py-2 rounded-full text-[12px] font-medium shadow-window border ' +
            (toast.kind === 'ok'
              ? 'bg-bg-content border-border-control text-text-primary'
              : 'bg-bg-content border-[#e85d4a] text-[#e85d4a]')
          }
        >
          {toast.message}
        </div>
      )}
    </div>
  );
}

function Row({
  entry,
  onCopy,
  onReinsert,
  onDelete,
}: {
  entry: Transcription;
  onCopy: () => void;
  onReinsert: () => void;
  onDelete: () => void;
}) {
  return (
    <article className="group flex items-start gap-5 py-4 border-b border-border-hairline last:border-b-0 relative">
      <span className="text-[12px] text-text-quaternary min-w-[64px] pt-[3px] tabular-nums">
        {formatTime(entry.created_at)}
      </span>
      <p className="font-serif text-[14px] leading-[1.6] text-text-primary flex-1 m-0">
        {entry.text}
      </p>
      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
        <ActionButton title="Copy" onClick={onCopy}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
          </svg>
        </ActionButton>
        <ActionButton title="Re-insert" onClick={onReinsert}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <path d="M3 12h13" />
            <path d="M11 7l5 5-5 5" />
            <path d="M21 4v16" />
          </svg>
        </ActionButton>
        <ActionButton title="Delete" onClick={onDelete}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="3 6 5 6 21 6" />
            <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
            <path d="M10 11v6" />
            <path d="M14 11v6" />
            <path d="M9 6V4a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2" />
          </svg>
        </ActionButton>
      </div>
    </article>
  );
}

function ActionButton({
  title,
  onClick,
  children,
}: {
  title: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      title={title}
      onClick={onClick}
      className="w-7 h-7 grid place-items-center rounded-md text-text-tertiary hover:text-text-primary hover:bg-bg-control transition-colors"
    >
      {children}
    </button>
  );
}

function EmptyState() {
  return (
    <div className="mt-12 text-center">
      <p className="font-serif text-[18px] text-text-secondary mb-2">
        Your transcriptions will appear here.
      </p>
      <p className="text-[12px] text-text-quaternary">
        Press Right Ctrl in any text field to give Murmr a try.
      </p>
    </div>
  );
}

function groupByDate(items: Transcription[]): Array<[string, Transcription[]]> {
  const out: Array<[string, Transcription[]]> = [];
  let currentLabel: string | null = null;
  for (const item of items) {
    const label = dateGroup(item.created_at);
    if (label !== currentLabel) {
      out.push([label, [item]]);
      currentLabel = label;
    } else {
      out[out.length - 1][1].push(item);
    }
  }
  return out;
}
