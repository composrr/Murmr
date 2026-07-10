import { useEffect, useRef, useState } from 'react';
import { hideHudWindow, insertEdited, reinsertText } from '../../lib/ipc';

interface Props {
  text: string;
  /** Editable mode (edit-last hotkey): show a textarea + Insert button. */
  editable?: boolean;
  onDismiss: () => void;
}

const PILL_STYLE: React.CSSProperties = {
  background: '#1f1f1c',
  borderRadius: 14,
  padding: '14px 16px',
  border: '0.5px solid rgba(255,255,255,0.06)',
  boxShadow: '0 6px 28px rgba(0,0,0,0.22)',
  maxWidth: 480,
};

const BUTTON_STYLE: React.CSSProperties = {
  background: '#3a3a35',
  border: '0.5px solid rgba(255,255,255,0.08)',
  color: '#d4d4cf',
  fontSize: 12,
  padding: '6px 12px',
  borderRadius: 8,
  fontWeight: 500,
  whiteSpace: 'nowrap',
  fontFamily: 'inherit',
  cursor: 'pointer',
};

export default function ResultPopup({ text, editable, onDismiss }: Props) {
  const [draft, setDraft] = useState(text);
  const areaRef = useRef<HTMLTextAreaElement>(null);

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
      onDismiss();
    } catch (e) {
      console.error('clipboard copy failed', e);
    }
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
      <div className="flex flex-col gap-2" style={PILL_STYLE}>
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
          style={{
            background: 'rgba(0,0,0,0.25)',
            border: '0.5px solid rgba(255,255,255,0.12)',
            borderRadius: 8,
            color: '#e6e3dc',
            fontSize: 14,
            lineHeight: 1.4,
            padding: '8px 10px',
            resize: 'none',
            outline: 'none',
            fontFamily: 'inherit',
            width: 440,
          }}
        />
        <div className="flex items-center justify-end gap-2">
          <button onClick={cancelEdit} style={{ ...BUTTON_STYLE, background: 'transparent' }}>
            Cancel
          </button>
          <button onClick={copy} style={BUTTON_STYLE}>
            Copy
          </button>
          <button
            onClick={insert}
            style={{ ...BUTTON_STYLE, background: '#4a5a3f', color: '#eaf0e2' }}
          >
            Insert
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-3" style={PILL_STYLE}>
      <span
        className="font-serif"
        style={{
          color: '#d4d4cf',
          fontSize: 14,
          lineHeight: 1.4,
          flex: 1,
          fontStyle: 'italic',
        }}
      >
        {text}
      </span>
      <button onClick={copy} style={BUTTON_STYLE}>
        Copy
      </button>
    </div>
  );
}
