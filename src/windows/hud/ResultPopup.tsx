interface Props {
  text: string;
  onDismiss: () => void;
}

export default function ResultPopup({ text, onDismiss }: Props) {
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      onDismiss();
    } catch (e) {
      console.error('clipboard copy failed', e);
    }
  };

  return (
    <div
      className="flex items-center gap-3"
      style={{
        background: '#1f1f1c',
        borderRadius: 14,
        padding: '14px 16px',
        border: '0.5px solid rgba(255,255,255,0.06)',
        boxShadow: '0 6px 28px rgba(0,0,0,0.22)',
        maxWidth: 480,
      }}
    >
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
      <button
        onClick={copy}
        style={{
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
        }}
      >
        Copy
      </button>
    </div>
  );
}
