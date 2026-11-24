export default function Thinking({ label = 'Writing it out' }: { label?: string }) {
  return (
    <div className="hud-pill gap-[12px]">
      <div className="flex items-center gap-[4px]">
        <span style={dotStyle(0)} />
        <span style={dotStyle(1)} />
        <span style={dotStyle(2)} />
      </div>
      <span style={{ color: '#d4d4cf' }}>{label}</span>
    </div>
  );
}

function dotStyle(index: number): React.CSSProperties {
  return {
    width: 6,
    height: 6,
    borderRadius: '50%',
    background: '#f4f4f5',
    display: 'inline-block',
    animation: `hudPulse 1.05s ease-in-out ${index * 0.16}s infinite`,
  };
}
