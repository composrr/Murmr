export default function Thinking() {
  return (
    <div
      className="flex items-center gap-[11px] rounded-full"
      style={{
        background: '#1f1f1c',
        padding: '11px 22px',
        border: '0.5px solid rgba(255,255,255,0.06)',
        boxShadow: '0 6px 28px rgba(0,0,0,0.22)',
      }}
    >
      <div className="flex items-center gap-[4px]">
        <span style={dotStyle(0)} />
        <span style={dotStyle(1)} />
        <span style={dotStyle(2)} />
      </div>
      <span style={{ color: 'rgba(212,212,207,0.75)', fontSize: 13 }}>transcribing</span>

      {/* Inline keyframes so the HUD entry point doesn't need a separate stylesheet. */}
      <style>{`
        @keyframes hudPulse {
          0%, 80%, 100% { opacity: 0.4; transform: scale(1); }
          40% { opacity: 0.95; transform: scale(1.15); }
        }
      `}</style>
    </div>
  );
}

function dotStyle(index: number): React.CSSProperties {
  return {
    width: 6,
    height: 6,
    borderRadius: '50%',
    background: '#d4d4cf',
    display: 'inline-block',
    animation: `hudPulse 1.05s ease-in-out ${index * 0.16}s infinite`,
  };
}
