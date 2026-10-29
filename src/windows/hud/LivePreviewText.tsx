/// Live transcript preview shown under the recording pill. Two lines tall at
/// most; when the text is longer, the newest words stay visible and older
/// lines scroll off the top. Stable words (two decodes agree) are drawn at
/// full strength, the still-changing tail dimmer. This text is a preview only
/// — what gets pasted is the full-accuracy pass that runs after you stop.

interface Props {
  stable: string;
  tentative: string;
  /** Recording has stopped and the final pass is running: dim everything. */
  finishing?: boolean;
}

export default function LivePreviewText({ stable, tentative, finishing }: Props) {
  if (!stable && !tentative) return null;
  return (
    <div
      style={{
        width: 516,
        maxWidth: 'calc(100vw - 24px)',
        background: '#1f1f1c',
        border: '0.5px solid rgba(255,255,255,0.06)',
        boxShadow: '0 6px 28px rgba(0,0,0,0.22)',
        borderRadius: 12,
        padding: '9px 14px',
        boxSizing: 'border-box',
      }}
    >
      <div
        style={{
          maxHeight: 38,
          overflow: 'hidden',
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'flex-end',
          fontSize: 13,
          lineHeight: '19px',
          letterSpacing: '-0.1px',
          opacity: finishing ? 0.6 : 1,
          transition: 'opacity 150ms linear',
        }}
      >
        <p style={{ margin: 0, wordBreak: 'break-word' }}>
          {stable && <span style={{ color: '#d4d4cf' }}>{stable}</span>}
          {stable && tentative ? ' ' : null}
          {tentative && <span style={{ color: 'rgba(212,212,207,0.5)' }}>{tentative}</span>}
        </p>
      </div>
    </div>
  );
}
