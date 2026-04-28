import { useEffect, useMemo, useState } from 'react';
import {
  createDictionaryEntry,
  deleteDictionaryEntry,
  listDictionary,
  updateDictionaryEntry,
  type DictionaryEntry,
  type DictionaryType,
} from '../../../lib/ipc';

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

  const refresh = () =>
    listDictionary()
      .then(setEntries)
      .catch((e) => setError(String(e)));

  useEffect(() => {
    refresh();
  }, []);

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
    <div className="max-w-[840px] mx-auto">
      <div className="flex items-center justify-between mb-1.5">
        <h1 className="font-serif text-[30px] tracking-[-0.4px] text-text-primary m-0">
          Dictionary
        </h1>
        <button
          className="bg-[#1f1f1c] text-[#fafaf9] dark:bg-[#d4d4cf] dark:text-[#1f1f1c] text-[13px] font-medium rounded-full px-[18px] py-[8px]"
          onClick={() => setEditing('new')}
        >
          Add new
        </button>
      </div>
      <p className="text-[13px] text-text-quaternary mb-[22px] leading-[1.6]">
        Teach Murmr the words, names, and shortcuts you use most.{' '}
        <span className="text-text-tertiary">
          (Replacements + snippets fire after Phase 7's post-processing pipeline lands.)
        </span>
      </p>

      <div className="flex items-center gap-1 mb-[18px] pb-4 border-b border-border-hairline">
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
        <input
          className="ml-auto rounded-full border border-border-control bg-bg-row text-[12px] px-3 py-[6px] w-[140px]"
          placeholder="Search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>

      {error && (
        <div className="text-[12px] text-[#e85d4a] py-2 px-3 rounded-[8px] bg-bg-row border border-border-hairline mb-3">
          {error}
        </div>
      )}

      {filtered.length === 0 ? (
        <EmptyState onAdd={() => setEditing('new')} />
      ) : (
        filtered.map((entry) => (
          <Row
            key={entry.id}
            entry={entry}
            onEdit={() => setEditing(entry)}
            onDelete={async () => {
              try {
                await deleteDictionaryEntry(entry.id);
                refresh();
              } catch (e) {
                setError(String(e));
              }
            }}
          />
        ))
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
        'rounded-full text-[12px] px-[14px] py-[6px] ' +
        (active
          ? 'bg-[#1f1f1c] dark:bg-[#d4d4cf] text-[#fafaf9] dark:text-[#1f1f1c] font-medium'
          : 'bg-transparent text-text-secondary hover:text-text-primary')
      }
    >
      {label}
      {typeof count === 'number' && (
        <span className={(active ? 'opacity-70 ' : 'text-text-quaternary ') + 'ml-[4px]'}>
          {count}
        </span>
      )}
    </button>
  );
}

function Row({
  entry,
  onEdit,
  onDelete,
}: {
  entry: DictionaryEntry;
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <div
      className="group flex items-center gap-4 py-[14px] border-b border-border-hairline last:border-b-0 cursor-pointer"
      onClick={onEdit}
    >
      <span className="text-[9px] uppercase tracking-[0.6px] text-text-quaternary font-medium w-[92px]">
        {TYPE_LABEL[entry.type]}
      </span>
      <span className="font-serif text-[16px] text-text-primary flex-1 m-0">
        {entry.type === 'word' ? (
          entry.trigger
        ) : (
          <>
            {entry.trigger}
            <span className="text-text-quaternary mx-2">→</span>
            <span className={entry.type === 'snippet' ? 'text-text-secondary text-[14px]' : ''}>
              {entry.expansion}
            </span>
          </>
        )}
      </span>
      <span className="text-[12px] text-text-quaternary mr-2">
        {entry.description ?? TYPE_HINT[entry.type]}
      </span>
      <button
        onClick={(e) => {
          e.stopPropagation();
          onDelete();
        }}
        className="opacity-0 group-hover:opacity-100 transition-opacity w-7 h-7 grid place-items-center rounded-md text-text-tertiary hover:text-[#e85d4a] hover:bg-bg-control"
        title="Delete"
      >
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
          <polyline points="3 6 5 6 21 6" />
          <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
          <path d="M10 11v6" />
          <path d="M14 11v6" />
          <path d="M9 6V4a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2" />
        </svg>
      </button>
    </div>
  );
}

function EmptyState({ onAdd }: { onAdd: () => void }) {
  return (
    <div className="text-center mt-12">
      <p className="font-serif text-[18px] text-text-secondary mb-2">
        No entries yet.
      </p>
      <p className="text-[12px] text-text-quaternary mb-4">
        Add proper nouns Whisper trips on, or shortcuts you'd like expanded automatically.
      </p>
      <button
        onClick={onAdd}
        className="bg-[#1f1f1c] text-[#fafaf9] dark:bg-[#d4d4cf] dark:text-[#1f1f1c] text-[13px] font-medium rounded-full px-[18px] py-[8px]"
      >
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
      className="fixed inset-0 bg-black/30 grid place-items-center z-50"
      onClick={onClose}
    >
      <div
        className="bg-bg-window border border-border-hairline rounded-card w-[440px] max-w-[92vw] p-6 shadow-window"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="font-serif text-[22px] tracking-[-0.3px] text-text-primary mb-4">
          {isNew ? 'Add entry' : 'Edit entry'}
        </h2>

        <div className="mb-4">
          <div className="text-[11px] uppercase tracking-[0.6px] text-text-quaternary font-medium mb-2">
            Type
          </div>
          <div className="flex gap-[2px] bg-bg-control rounded-control p-[2px]">
            {(['word', 'replacement', 'snippet'] as DictionaryType[]).map((t) => (
              <button
                key={t}
                onClick={() => setType(t)}
                className={
                  'flex-1 text-[12px] py-[5px] rounded-[5px] capitalize ' +
                  (type === t
                    ? 'bg-bg-content text-text-primary font-medium shadow-[0_1px_2px_rgba(0,0,0,0.04)]'
                    : 'text-text-tertiary hover:text-text-secondary')
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
            className="w-full px-3 py-[8px] rounded-[7px] border border-border-control bg-bg-row text-[13px] text-text-primary"
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
              className="w-full px-3 py-[8px] rounded-[7px] border border-border-control bg-bg-row text-[13px] text-text-primary"
            />
          </Field>
        )}

        <Field label="Description (optional)">
          <input
            value={description ?? ''}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="What this entry is for"
            className="w-full px-3 py-[8px] rounded-[7px] border border-border-control bg-bg-row text-[13px] text-text-primary"
          />
        </Field>

        {error && (
          <div className="text-[12px] text-[#e85d4a] py-2 px-3 rounded-[8px] bg-bg-row border border-border-hairline mb-3">
            {error}
          </div>
        )}

        <div className="flex justify-end gap-2 mt-2">
          <button
            onClick={onClose}
            className="text-[13px] text-text-secondary px-3 py-[7px] rounded-full hover:text-text-primary"
          >
            Cancel
          </button>
          <button
            onClick={save}
            disabled={saving}
            className="bg-[#1f1f1c] text-[#fafaf9] dark:bg-[#d4d4cf] dark:text-[#1f1f1c] text-[13px] font-medium rounded-full px-[18px] py-[8px] disabled:opacity-50"
          >
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
      <div className="text-[11px] uppercase tracking-[0.6px] text-text-quaternary font-medium mb-1.5">
        {label}
      </div>
      {children}
    </div>
  );
}
