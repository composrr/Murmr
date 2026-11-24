import { useEffect, useMemo, useState } from 'react';
import { Link } from 'react-router-dom';
import {
  deleteTranscription,
  getSettings,
  listenTranscriptionSaved,
  recentTranscriptions,
  reinsertText,
  searchTranscriptions,
  updateTranscriptionText,
  usageSummary,
  userInfo,
  type Transcription,
  type UsageSummary,
} from '../../../lib/ipc';
import { dateGroup, formatTime } from '../../../lib/format';
import { displayName } from './HotkeyCapture';

const DEBOUNCE_MS = 120;

type Toast = { kind: 'ok' | 'err'; message: string } | null;

export default function Home() {
  const [items, setItems] = useState<Transcription[]>([]);
  const [query, setQuery] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<Toast>(null);
  // Live hotkey labels — re-fetched each mount so changing them in Settings
  // and clicking back to Home shows the new value.
  const [dictationKey, setDictationKey] = useState('ControlRight');
  const [repeatKey, setRepeatKey] = useState('');

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

  // Glance tiles: today's words, overall pace, streak.
  const [usage, setUsage] = useState<UsageSummary | null>(null);
  useEffect(() => {
    const load = () => usageSummary().then(setUsage).catch(() => {});
    load();
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    listenTranscriptionSaved(load).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);
  const todayWords = useMemo(() => {
    const start = new Date();
    start.setHours(0, 0, 0, 0);
    return items
      .filter((i) => i.created_at >= start.getTime())
      .reduce((sum, i) => sum + i.word_count, 0);
  }, [items]);
  const [name, setName] = useState<string | null>(null);
  useEffect(() => {
    // Prefer the user-supplied display name from onboarding/settings; fall
    // back to the OS user only when nothing's been set yet.
    (async () => {
      try {
        const s = await getSettings();
        // Hotkey labels live alongside name — single getSettings call.
        if (s.dictation_hotkey) setDictationKey(s.dictation_hotkey);
        if (s.repeat_hotkey !== undefined) setRepeatKey(s.repeat_hotkey);
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
  const greeting = `${timeGreeting()}${name ? `, ${name}` : ''}`;

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

  // History fix: correct a past transcript. Murmr learns dictionary
  // suggestions from the change (same as the edit-last bubble).
  const [editingId, setEditingId] = useState<number | null>(null);

  const onSaveFix = async (entry: Transcription, text: string) => {
    const next = text.trim();
    if (!next || next === entry.text) {
      setEditingId(null);
      return;
    }
    try {
      const learned = await updateTranscriptionText(entry.id, next);
      setItems((prev) => prev.map((i) => (i.id === entry.id ? { ...i, text: next } : i)));
      setEditingId(null);
      const n = learned.length;
      showToast({
        kind: 'ok',
        message:
          n === 0
            ? 'Saved'
            : `Saved · ${n} dictionary suggestion${n === 1 ? '' : 's'} added to Dictionary`,
      });
    } catch (e) {
      showToast({ kind: 'err', message: `Save failed: ${e}` });
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

  const t = usage?.totals;
  const pace = t && t.total_speech_ms > 0 ? Math.round((t.total_words * 60_000) / t.total_speech_ms) : null;

  return (
    <div className="max-w-[820px]">
      <h1 className="page-title fade-up">{greeting}</h1>
      <p className="text-[13px] text-text-secondary mt-2 leading-[2]">
        Tap or hold <KeyCap>{displayName(dictationKey)}</KeyCap> anywhere to dictate.
        {repeatKey && (
          <>
            {' '}
            <KeyCap>{displayName(repeatKey)}</KeyCap> pastes the last one again.
          </>
        )}
      </p>

      <div className="grid grid-cols-3 gap-3 mt-6 fade-up">
        <Glance label="Today" value={todayWords.toLocaleString()} unit={todayWords === 1 ? 'word' : 'words'} />
        <Glance label="Pace" value={pace !== null ? String(pace) : '—'} unit="wpm" />
        <Link
          to="/insights"
          className="glass !rounded-[14px] px-[18px] py-[14px] block group hover:!border-border-control transition-colors"
        >
          <div className="eyebrow">Streak</div>
          <div className="flex items-baseline justify-between mt-1">
            <span className="num text-[22px] font-semibold tracking-[-0.6px] text-text-primary">
              {t ? t.current_streak : '—'}{' '}
              <span className="text-[12px] text-text-tertiary font-medium tracking-normal">
                {t?.current_streak === 1 ? 'day' : 'days'}
              </span>
            </span>
            <span className="text-[12px] font-medium text-accent transition-transform group-hover:translate-x-[2px]">
              See insights →
            </span>
          </div>
        </Link>
      </div>

      <label className="relative block mt-[18px]">
        <svg
          width="15"
          height="15"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.8"
          strokeLinecap="round"
          strokeLinejoin="round"
          className="absolute left-[14px] top-1/2 -translate-y-1/2 pointer-events-none text-text-tertiary"
          aria-hidden="true"
        >
          <circle cx="11" cy="11" r="8" />
          <line x1="21" y1="21" x2="16.65" y2="16.65" />
        </svg>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search everything you've said"
          aria-label="Search transcriptions"
          className="input-glass w-full !h-[42px] !rounded-[12px] !pl-10"
        />
      </label>

      {error && (
        <div className="text-[12px] text-danger py-2 px-3 rounded-[10px] bg-danger/10 border border-danger/30 mt-4">
          {error}
        </div>
      )}

      {loading && items.length === 0 ? (
        <p className="text-[13px] text-text-quaternary mt-6">Loading…</p>
      ) : items.length === 0 ? (
        <EmptyState dictationKey={dictationKey} searching={!!debouncedQuery} />
      ) : (
        grouped.map(([label, group]) => (
          <section key={label}>
            <div className="eyebrow mt-[22px] mb-[10px]">{label}</div>
            <div className="glass px-[18px] py-1">
              {group.map((entry) => (
                <Row
                  key={entry.id}
                  entry={entry}
                  editing={editingId === entry.id}
                  onCopy={() => onCopy(entry)}
                  onReinsert={() => onReinsert(entry)}
                  onDelete={() => onDelete(entry)}
                  onFix={() => setEditingId(entry.id)}
                  onCancelFix={() => setEditingId(null)}
                  onSaveFix={(text) => onSaveFix(entry, text)}
                />
              ))}
            </div>
          </section>
        ))
      )}

      {toast && (
        <div
          role="status"
          className={
            'fixed bottom-6 left-1/2 -translate-x-1/2 z-50 px-4 h-[36px] flex items-center rounded-full text-[12px] font-medium border fade-up ' +
            (toast.kind === 'ok' ? 'border-border-control text-text-primary' : 'border-danger/40 text-danger')
          }
          style={{
            background: 'var(--bg-popover)',
            boxShadow: '0 12px 32px rgba(0,0,0,0.4), var(--glass-highlight)',
          }}
        >
          {toast.message}
        </div>
      )}
    </div>
  );
}

function timeGreeting(): string {
  const h = new Date().getHours();
  if (h < 5) return 'Good evening';
  if (h < 12) return 'Good morning';
  if (h < 18) return 'Good afternoon';
  return 'Good evening';
}

function KeyCap({ children }: { children: React.ReactNode }) {
  return (
    <kbd
      className="inline-flex items-center h-[22px] px-[8px] mx-[2px] rounded-[7px] text-[11.5px] font-medium text-text-primary align-middle"
      style={{
        background: 'var(--glass-bg-strong)',
        border: '1px solid var(--border-control)',
        boxShadow: 'var(--glass-highlight), 0 1px 0 rgba(0,0,0,0.3)',
        fontFamily: 'inherit',
      }}
    >
      {children}
    </kbd>
  );
}

function Glance({ label, value, unit }: { label: string; value: string; unit: string }) {
  return (
    <div className="glass !rounded-[14px] px-[18px] py-[14px]">
      <div className="eyebrow">{label}</div>
      <div className="num text-[22px] font-semibold tracking-[-0.6px] mt-1 text-text-primary">
        {value}{' '}
        <span className="text-[12px] text-text-tertiary font-medium tracking-normal">{unit}</span>
      </div>
    </div>
  );
}

function Row({
  entry,
  editing,
  onCopy,
  onReinsert,
  onDelete,
  onFix,
  onCancelFix,
  onSaveFix,
}: {
  entry: Transcription;
  editing: boolean;
  onCopy: () => void;
  onReinsert: () => void;
  onDelete: () => void;
  onFix: () => void;
  onCancelFix: () => void;
  onSaveFix: (text: string) => void;
}) {
  if (editing) {
    return <FixRow entry={entry} onCancel={onCancelFix} onSave={onSaveFix} />;
  }
  return (
    <article className="group flex items-start gap-[18px] py-[14px] border-b border-border-hairline last:border-b-0 relative">
      <div className="w-[70px] flex-none pt-[2px]">
        <div className="num text-[12px] text-text-secondary">{formatTime(entry.created_at)}</div>
        {entry.target_app && (
          <div className="text-[11px] text-text-quaternary mt-[3px] truncate" title={entry.target_app}>
            {entry.target_app.replace(/\.(exe|app)$/i, '')}
          </div>
        )}
      </div>
      <p className="text-[14px] leading-[1.6] text-text-primary flex-1 m-0 select-text">
        {entry.text}
      </p>
      <div className="flex items-center gap-[6px] opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity">
        <ActionButton title="Fix a misheard word" onClick={onFix}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 20h9" />
            <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" />
          </svg>
        </ActionButton>
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

function FixRow({
  entry,
  onCancel,
  onSave,
}: {
  entry: Transcription;
  onCancel: () => void;
  onSave: (text: string) => void;
}) {
  const [draft, setDraft] = useState(entry.text);
  return (
    <article className="flex items-start gap-[18px] py-[14px] border-b border-border-hairline last:border-b-0">
      <span className="num text-[12px] text-text-secondary w-[70px] flex-none pt-[9px]">
        {formatTime(entry.created_at)}
      </span>
      <div className="flex-1 flex flex-col gap-2">
        <textarea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          autoFocus
          rows={Math.min(6, Math.max(2, Math.ceil(entry.text.length / 80)))}
          spellCheck={false}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              onSave(draft);
            } else if (e.key === 'Escape') {
              e.preventDefault();
              onCancel();
            }
          }}
          className="w-full text-[14px] leading-[1.6] text-text-primary px-3 py-2 rounded-[10px] border border-accent/50 bg-bg-content resize-y outline-none shadow-[0_0_0_3px_rgb(var(--accent-rgb)/0.12)]"
        />
        <div className="flex items-center gap-2">
          <span className="text-[11px] text-text-quaternary mr-auto">
            Fix misheard names or terms and Murmr will suggest dictionary entries.
          </span>
          <button onClick={onCancel} className="btn-ghost !h-[30px] !text-[12px]">
            Cancel
          </button>
          <button onClick={() => onSave(draft)} className="btn-primary !h-[30px] !text-[12px]">
            Save
          </button>
        </div>
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
      aria-label={title}
      className="w-[30px] h-[30px] grid place-items-center rounded-[9px] text-text-secondary border border-border-hairline bg-bg-control hover:text-text-primary hover:bg-bg-selected hover:border-border-control transition-colors"
    >
      {children}
    </button>
  );
}

function EmptyState({ dictationKey, searching }: { dictationKey: string; searching: boolean }) {
  return (
    <div className="glass mt-6 px-6 py-12 text-center">
      <p className="text-[16px] font-semibold text-text-primary mb-2">
        {searching ? 'Nothing matches that search.' : 'Your dictations will show up here.'}
      </p>
      <p className="text-[12.5px] text-text-tertiary">
        {searching
          ? 'Try a different word.'
          : `Press ${displayName(dictationKey)} in any text field to give Murmr a try.`}
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
