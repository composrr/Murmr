import { useEffect, useMemo, useState } from 'react';
import {
  acceptDictionarySuggestion,
  createDictionaryEntry,
  deleteDictionaryEntry,
  dismissDictionarySuggestion,
  listDictionary,
  listDictionarySuggestions,
  listenSuggestionsChanged,
  updateDictionaryEntry,
  type DictionaryEntry,
  type DictionarySuggestion,
  type DictionaryType,
} from '../../../lib/ipc';
import { Toggle } from './settings-ui';

type Filter = 'all' | DictionaryType;

const TYPE_LABEL: Record<DictionaryType, string> = {
  word: 'Word',
  replacement: 'Replacement',
  snippet: 'Snippet',
};

const TYPE_HINT: Record<DictionaryType, string> = {
  word: 'Recognize as proper noun',
  replacement: 'Replace on dictation',
  snippet: 'Expand on dictation',
};

export default function Dictionary() {
  const [entries, setEntries] = useState<DictionaryEntry[]>([]);
  const [filter, setFilter] = useState<Filter>('all');
  const [search, setSearch] = useState('');
  const [editing, setEditing] = useState<DictionaryEntry | 'new' | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [suggestions, setSuggestions] = useState<DictionarySuggestion[]>([]);

  const refresh = () =>
    listDictionary()
      .then(setEntries)
      .catch((e) => setError(String(e)));

  const refreshSuggestions = () =>
    listDictionarySuggestions()
      .then(setSuggestions)
      .catch((e) => setError(String(e)));

  useEffect(() => {
    refresh();
    refreshSuggestions();
    // New corrections (HUD edit, History fix) arrive while this page is open.
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    listenSuggestionsChanged(() => {
      refreshSuggestions();
      refresh();
    }).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  const acceptSuggestion = async (s: DictionarySuggestion) => {
    try {
      await acceptDictionarySuggestion(s.id);
      setSuggestions((prev) => prev.filter((x) => x.id !== s.id));
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const dismissSuggestion = async (s: DictionarySuggestion) => {
    try {
      await dismissDictionarySuggestion(s.id);
      setSuggestions((prev) => prev.filter((x) => x.id !== s.id));
    } catch (e) {
      setError(String(e));
    }
  };

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return entries.filter((e) => {
      if (filter !== 'all' && e.type !== filter) return false;
      if (!q) return true;
      const hay =
        e.trigger.toLowerCase() +
        ' ' +
        (e.expansion ?? '').toLowerCase() +
        ' ' +
        (e.description ?? '').toLowerCase();
      return hay.includes(q);
    });
  }, [entries, filter, search]);

  const counts = useMemo(() => {
    const c = { all: entries.length, word: 0, replacement: 0, snippet: 0 };
    for (const e of entries) c[e.type]++;
    return c;
  }, [entries]);

  return (
    <div className="max-w-[860px]">
      <div className="flex items-end justify-between gap-6 mb-7 fade-up">
        <div>
          <h1 className="page-title">Dictionary</h1>
          <p className="text-[13px] text-text-secondary mt-2 leading-[1.6] max-w-[520px]">
            Teach Murmr the words, names and shortcuts you use most. Words help it hear you;
            replacements and snippets are applied to every dictation.
          </p>
        </div>
        <button className="btn-primary flex-none" onClick={() => setEditing('new')}>
          <PlusIcon /> Add entry
        </button>
      </div>

      {suggestions.length > 0 && (
        <Suggested
          items={suggestions}
          onAccept={acceptSuggestion}
          onDismiss={dismissSuggestion}
        />
      )}

      <div className="flex items-center gap-3 mb-3">
        <div className="inline-flex gap-[2px] rounded-full p-[3px] bg-bg-control border border-border-hairline">
        <Chip active={filter === 'all'} onClick={() => setFilter('all')} count={counts.all} label="All" />
        <Chip active={filter === 'word'} onClick={() => setFilter('word')} count={counts.word} label="Words" />
        <Chip
          active={filter === 'replacement'}
          onClick={() => setFilter('replacement')}
          count={counts.replacement}
          label="Replacements"
        />
        <Chip
          active={filter === 'snippet'}
          onClick={() => setFilter('snippet')}
          count={counts.snippet}
          label="Snippets"
        />
        </div>
        <label className="relative ml-auto">
          <svg
            width="13"
            height="13"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.9"
            strokeLinecap="round"
            className="absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none text-text-tertiary"
            aria-hidden="true"
          >
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            className="input-glass !h-[34px] !rounded-full w-[200px] !pl-8 !text-[12.5px]"
            placeholder="Search"
            aria-label="Search dictionary"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </label>
      </div>

      {error && (
        <div className="text-[12px] text-danger py-2 px-3 rounded-[10px] bg-danger/10 border border-danger/30 mb-3">
          {error}
        </div>
      )}

      {filtered.length === 0 ? (
        <EmptyState onAdd={() => setEditing('new')} />
      ) : (
        <div className="glass px-2 py-1">
          {filtered.map((entry) => (
            <Row
              key={entry.id}
              entry={entry}
              onEdit={() => setEditing(entry)}
              onToggle={async (enabled) => {
                setEntries((prev) => prev.map((x) => (x.id === entry.id ? { ...x, enabled } : x)));
                try {
                  await updateDictionaryEntry({
                    id: entry.id,
                    entry_type: entry.type,
                    trigger: entry.trigger,
                    expansion: entry.expansion,
                    description: entry.description,
                    is_regex: entry.is_regex,
                    enabled,
                  });
                } catch (e) {
                  setError(String(e));
                  refresh();
                }
              }}
              onDelete={async () => {
                try {
                  await deleteDictionaryEntry(entry.id);
                  refresh();
                } catch (e) {
                  setError(String(e));
                }
              }}
            />
          ))}
        </div>
      )}

      {editing && (
        <EditorSheet
          initial={editing === 'new' ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            refresh();
          }}
        />
      )}
    </div>
  );
}

function Suggested({
  items,
  onAccept,
  onDismiss,
}: {
  items: DictionarySuggestion[];
  onAccept: (s: DictionarySuggestion) => void;
  onDismiss: (s: DictionarySuggestion) => void;
}) {
  return (
    <section className="glass overflow-hidden mb-6 px-5 pt-4 pb-1 fade-up">
      <div className="card-glow" aria-hidden="true" />
      <div className="relative flex items-start justify-between gap-4 mb-1">
        <div>
          <h2 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary m-0 flex items-center gap-2">
            <span className="w-[7px] h-[7px] rounded-full bg-accent shadow-[0_0_10px_rgb(var(--accent-rgb)/0.8)]" />
            Suggested from your fixes
            <span className="num text-[12px] font-medium text-text-tertiary">{items.length}</span>
          </h2>
          <p className="text-[12px] text-text-tertiary mt-1">
            Nothing is added until you say so.
          </p>
        </div>
      </div>
      {items.map((s) => (
        <div
          key={s.id}
          className="relative flex items-center gap-4 py-3 border-b border-border-hairline last:border-b-0"
          title={s.example ? `You wrote: “${s.example}”` : undefined}
        >
          <TypeBadge type={s.type} />
          <span className="text-[14px] text-text-primary flex-1 m-0 min-w-0 truncate">
            {s.type === 'word' ? (
              s.trigger
            ) : (
              <>
                {s.trigger}
                <span className="text-text-quaternary mx-2">→</span>
                <span className="text-accent">{s.expansion}</span>
              </>
            )}
          </span>
          {s.count > 1 && (
            <span className="num text-[11px] text-text-tertiary whitespace-nowrap">
              Fixed {s.count}×
            </span>
          )}
          <button onClick={() => onDismiss(s)} className="btn-ghost !h-[28px] !text-[12px]">
            Dismiss
          </button>
          <button onClick={() => onAccept(s)} className="btn-primary !h-[28px] !text-[12px] !px-[14px]">
            Add
          </button>
        </div>
      ))}
    </section>
  );
}

function Chip({
  active,
  onClick,
  count,
  label,
}: {
  active: boolean;
  onClick: () => void;
  count?: number;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      className={
        'rounded-full text-[12px] h-[28px] px-[13px] transition-colors ' +
        (active ? 'text-text-primary font-medium' : 'bg-transparent text-text-tertiary hover:text-text-primary')
      }
      style={
        active
          ? { background: 'var(--nav-active)', boxShadow: 'var(--glass-highlight), 0 0 0 1px var(--glass-border)' }
          : undefined
      }
    >
      {label}
      {typeof count === 'number' && (
        <span className="num text-text-quaternary ml-[5px]">
          {count}
        </span>
      )}
    </button>
  );
}

function Row({
  entry,
  onEdit,
  onToggle,
  onDelete,
}: {
  entry: DictionaryEntry;
  onEdit: () => void;
  onToggle: (enabled: boolean) => void;
  onDelete: () => void;
}) {
  return (
    <div
      className={
        'group flex items-center gap-4 px-3 py-3 border-b border-border-hairline last:border-b-0 cursor-pointer rounded-[12px] hover:bg-bg-row transition-colors ' +
        (entry.enabled ? '' : 'opacity-55')
      }
      onClick={onEdit}
    >
      <TypeBadge type={entry.type} />
      <span className="text-[14px] text-text-primary flex-1 min-w-0 truncate">
        {entry.type === 'word' ? (
          entry.trigger
        ) : (
          <>
            {entry.trigger}
            <span className="text-text-quaternary mx-2">→</span>
            <span className="text-text-secondary">{entry.expansion}</span>
          </>
        )}
      </span>
      <span className="text-[12px] text-text-tertiary truncate max-w-[220px]">
        {entry.description ?? TYPE_HINT[entry.type]}
      </span>
      <button
        onClick={(e) => {
          e.stopPropagation();
          onDelete();
        }}
        className="opacity-0 group-hover:opacity-100 focus:opacity-100 transition-opacity w-[30px] h-[30px] grid place-items-center rounded-[9px] text-text-tertiary hover:text-danger hover:bg-danger/10"
        title="Delete"
        aria-label={`Delete ${entry.trigger}`}
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <polyline points="3 6 5 6 21 6" />
          <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
          <path d="M10 11v6" />
          <path d="M14 11v6" />
          <path d="M9 6V4a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2" />
        </svg>
      </button>
      <span onClick={(e) => e.stopPropagation()} title={entry.enabled ? 'On' : 'Off'}>
        <Toggle on={entry.enabled} onChange={onToggle} />
      </span>
    </div>
  );
}

function TypeBadge({ type }: { type: DictionaryType }) {
  return (
    <span className="w-[96px] flex-none">
      <span className="inline-flex items-center h-[22px] px-[9px] rounded-full text-[10.5px] font-semibold uppercase tracking-[0.06em] text-text-secondary bg-bg-control border border-border-hairline">
        {TYPE_LABEL[type]}
      </span>
    </span>
  );
}

function PlusIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" aria-hidden="true">
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

function EmptyState({ onAdd }: { onAdd: () => void }) {
  return (
    <div className="glass text-center px-6 py-12">
      <p className="text-[16px] font-semibold text-text-primary mb-2">
        Nothing here yet.
      </p>
      <p className="text-[12.5px] text-text-tertiary mb-5">
        Add proper nouns Whisper trips on, or shortcuts you'd like expanded automatically.
      </p>
      <button onClick={onAdd} className="btn-primary">
        Add your first entry
      </button>
    </div>
  );
}

// ---------- Editor sheet (modal) ----------

function EditorSheet({
  initial,
  onClose,
  onSaved,
}: {
  initial: DictionaryEntry | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [type, setType] = useState<DictionaryType>(initial?.type ?? 'word');
  const [trigger, setTrigger] = useState(initial?.trigger ?? '');
  const [expansion, setExpansion] = useState(initial?.expansion ?? '');
  const [description, setDescription] = useState(initial?.description ?? '');
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const isNew = initial === null;
  const needsExpansion = type !== 'word';

  const save = async () => {
    if (!trigger.trim()) {
      setError('A trigger is required.');
      return;
    }
    if (needsExpansion && !expansion.trim()) {
      setError('Expansion is required for replacements and snippets.');
      return;
    }
    setSaving(true);
    setError(null);
    try {
      if (isNew) {
        await createDictionaryEntry({
          entry_type: type,
          trigger: trigger.trim(),
          expansion: needsExpansion ? expansion.trim() : null,
          description: description.trim() || null,
        });
      } else {
        await updateDictionaryEntry({
          id: initial!.id,
          entry_type: type,
          trigger: trigger.trim(),
          expansion: needsExpansion ? expansion.trim() : null,
          description: description.trim() || null,
          enabled: initial!.enabled,
        });
      }
      onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className="fixed inset-0 grid place-items-center z-50 bg-black/50 backdrop-blur-[6px]"
      onClick={onClose}
    >
      <div
        className="glass-strong w-[460px] max-w-[92vw] p-6 fade-up"
        style={{ background: 'var(--bg-popover)', boxShadow: 'var(--glass-highlight), 0 30px 80px rgba(0,0,0,0.5)' }}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        <h2 className="text-[20px] font-semibold tracking-[-0.5px] text-text-primary mb-5">
          {isNew ? 'Add entry' : 'Edit entry'}
        </h2>

        <div className="mb-4">
          <div className="eyebrow mb-2">Type</div>
          <div className="flex gap-[2px] rounded-full p-[3px] bg-bg-control border border-border-hairline">
            {(['word', 'replacement', 'snippet'] as DictionaryType[]).map((t) => (
              <button
                key={t}
                onClick={() => setType(t)}
                className={
                  'flex-1 text-[12px] h-[28px] rounded-full capitalize transition-colors ' +
                  (type === t ? 'text-text-primary font-medium' : 'text-text-tertiary hover:text-text-primary')
                }
                style={
                  type === t
                    ? { background: 'var(--nav-active)', boxShadow: 'var(--glass-highlight), 0 0 0 1px var(--glass-border)' }
                    : undefined
                }
              >
                {t}
              </button>
            ))}
          </div>
        </div>

        <Field label={type === 'word' ? 'Word' : 'Trigger'}>
          <input
            value={trigger}
            onChange={(e) => setTrigger(e.target.value)}
            placeholder={type === 'word' ? 'e.g. Murmr' : 'e.g. btw'}
            className="input-glass w-full"
            autoFocus
          />
        </Field>

        {needsExpansion && (
          <Field label={type === 'snippet' ? 'Expands to' : 'Becomes'}>
            <input
              value={expansion ?? ''}
              onChange={(e) => setExpansion(e.target.value)}
              placeholder={
                type === 'snippet' ? 'e.g. Best, Jon — Sent via Murmr' : 'e.g. by the way'
              }
              className="input-glass w-full"
            />
          </Field>
        )}

        <Field label="Description (optional)">
          <input
            value={description ?? ''}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="What this entry is for"
            className="input-glass w-full"
          />
        </Field>

        {error && (
          <div className="text-[12px] text-danger py-2 px-3 rounded-[10px] bg-danger/10 border border-danger/30 mb-3">
            {error}
          </div>
        )}

        <div className="flex justify-end gap-2 mt-2">
          <button onClick={onClose} className="btn-ghost">
            Cancel
          </button>
          <button onClick={save} disabled={saving} className="btn-primary">
            {saving ? 'Saving…' : isNew ? 'Save' : 'Update'}
          </button>
        </div>
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="mb-3">
      <div className="eyebrow mb-[6px]">{label}</div>
      {children}
    </div>
  );
}
