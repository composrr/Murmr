import { useEffect, useRef, useState } from 'react';
import {
  acceptDictionarySuggestion,
  hideHudWindow,
  insertEdited,
  learnFromCorrection,
  previewCorrection,
  reinsertText,
  type LearnCandidate,
} from '../../lib/ipc';

interface Props {
  text: string;
  /** Editable mode (edit-last hotkey): show a textarea + Insert button. */
  editable?: boolean;
  onDismiss: () => void;
}

const PILL_STYLE: React.CSSProperties = {
  padding: '14px 16px',
  maxWidth: 480,
};

/** Most "Add to dictionary" chips shown at once. */
const MAX_CHIPS = 3;

const candidateKey = (c: { kind?: string; type?: string; trigger: string; expansion: string | null }) =>
  `${c.kind ?? c.type}|${c.trigger.toLowerCase()}|${(c.expansion ?? '').toLowerCase()}`;

/**
 * Learn from the correction the user just made, accepting the chips they
 * ticked. Runs after the text is inserted and never throws: learning must
 * not get in the way of the dictation.
 */
async function learnInBackground(original: string, edited: string, accept: Set<string>) {
  try {
    const touched = await learnFromCorrection(original, edited);
    for (const s of touched) {
      if (accept.has(candidateKey(s))) {
        await acceptDictionarySuggestion(s.id);
      }
    }
  } catch (e) {
    console.error('learning from correction failed', e);
  }
}

export default function ResultPopup({ text, editable, onDismiss }: Props) {
  const [draft, setDraft] = useState(text);
  const areaRef = useRef<HTMLTextAreaElement>(null);
  // "Add to dictionary" chips for the edit bubble, refreshed as the user types.
  const [candidates, setCandidates] = useState<LearnCandidate[]>([]);
  const [picked, setPicked] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!editable) return;
    let cancelled = false;
    const id = window.setTimeout(() => {
      previewCorrection(text, draft)
        .then((c) => {
          if (!cancelled) setCandidates(c.slice(0, MAX_CHIPS));
        })
        .catch(() => {
          if (!cancelled) setCandidates([]);
        });
    }, 250);
    return () => {
      cancelled = true;
      window.clearTimeout(id);
    };
  }, [editable, text, draft]);

  const togglePick = (c: LearnCandidate) =>
    setPicked((prev) => {
      const next = new Set(prev);
      const k = candidateKey(c);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });

  /** Only count picks that are still on screen (the draft may have changed). */
  const livePicks = () => new Set(candidates.map(candidateKey).filter((k) => picked.has(k)));

  // Focus + select the text when the editable bubble opens so the user can
  // immediately fix a word.
  useEffect(() => {
    if (editable && areaRef.current) {
      areaRef.current.focus();
      areaRef.current.select();
    }
  }, [editable]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(editable ? draft : text);
    } catch (e) {
      console.error('clipboard copy failed', e);
      return;
    }
    if (editable) {
      const out = draft.trim();
      if (out) void learnInBackground(text, out, livePicks());
      // Release focus back to the original app, same as Cancel.
      try {
        await hideHudWindow();
      } catch (e) {
        console.error('hide hud failed', e);
      }
    }
    onDismiss();
  };

  const insert = async () => {
    const out = draft.trim();
    if (!out) {
      await cancelEdit();
      return;
    }
    try {
      // Edit mode routes through insert_edited (restores focus to the
      // original app first); the debug/result path uses plain reinsert.
      await (editable ? insertEdited(out) : reinsertText(out));
    } catch (e) {
      console.error('re-insert failed', e);
    }
    // Learn after inserting so it can never delay or block the text.
    if (editable) void learnInBackground(text, out, livePicks());
    onDismiss();
  };

  const cancelEdit = async () => {
    if (editable) {
      try {
        await hideHudWindow();
      } catch (e) {
        console.error('hide hud failed', e);
      }
    }
    onDismiss();
  };

  if (editable) {
    return (
      <div className="hud-panel flex flex-col gap-2" style={PILL_STYLE}>
        <textarea
          ref={areaRef}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              void insert();
            } else if (e.key === 'Escape') {
              e.preventDefault();
              void cancelEdit();
            }
          }}
          rows={2}
          spellCheck={false}
          style={{
            background: 'rgba(0,0,0,0.3)',
            border: '1px solid rgba(124,156,255,0.45)',
            boxShadow: '0 0 0 3px rgba(124,156,255,0.12)',
            borderRadius: 10,
            color: '#f4f4f5',
            fontSize: 14,
            lineHeight: 1.4,
            padding: '8px 10px',
            resize: 'none',
            outline: 'none',
            fontFamily: 'inherit',
            width: 440,
          }}
        />
        {candidates.length > 0 && (
          <div className="flex items-center gap-[6px] flex-wrap" style={{ minHeight: 24 }}>
            <span style={{ color: '#8e8e96', fontSize: 11, marginRight: 2 }}>Add to dictionary:</span>
            {candidates.map((c) => {
              const on = picked.has(candidateKey(c));
              return (
                <button
                  key={candidateKey(c)}
                  onClick={() => togglePick(c)}
                  title={
                    c.kind === 'word'
                      ? 'Help Murmr recognise this word'
                      : `Always replace "${c.trigger}" with "${c.expansion}"`
                  }
                  aria-pressed={on}
                  className="hud-btn"
                  style={{
                    fontSize: 11,
                    height: 24,
                    padding: '0 9px',
                    background: on ? 'rgba(124,156,255,0.18)' : 'transparent',
                    color: on ? '#7c9cff' : '#d4d4d8',
                    borderColor: on ? 'rgba(124,156,255,0.45)' : 'rgba(255,255,255,0.16)',
                    maxWidth: 220,
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                  }}
                >
                  {on ? '✓ ' : '+ '}
                  {c.kind === 'word' ? c.trigger : `${c.trigger} → ${c.expansion}`}
                </button>
              );
            })}
          </div>
        )}
        <div className="flex items-center justify-end gap-2">
          <button onClick={cancelEdit} className="hud-btn hud-btn-ghost">
            Cancel
          </button>
          <button onClick={copy} className="hud-btn">
            Copy
          </button>
          <button onClick={insert} className="hud-btn hud-btn-primary">
            Insert
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="hud-panel flex items-center gap-3" style={PILL_STYLE}>
      <span
        style={{
          color: '#f4f4f5',
          fontSize: 14,
          lineHeight: 1.4,
          flex: 1,
        }}
      >
        {text}
      </span>
      <button onClick={copy} className="hud-btn">
        Copy
      </button>
    </div>
  );
}
