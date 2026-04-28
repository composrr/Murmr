import { PrimaryButton, ProgressDots, type StepProps } from '../App';

const FEATURES = [
  {
    title: 'Talk, and it types',
    body: 'Press a key, speak naturally — your words appear in whatever app you\'re using.',
    Icon: WaveIcon,
  },
  {
    title: 'Cleans up as you go',
    body: 'Removes filler words, fixes punctuation, capitalizes sentences automatically.',
    Icon: SparkleIcon,
  },
  {
    title: 'Voice commands',
    body: 'Say "period", "comma", or "new line" and Murmr inserts the right character.',
    Icon: KeyIcon,
  },
  {
    title: 'Works everywhere',
    body: 'Notepad, Word, browsers, Slack, VS Code — anywhere you can type, you can talk.',
    Icon: WindowIcon,
  },
  {
    title: 'Knows your words',
    body: 'Teach it proper nouns, replacements, and shortcuts that expand into longer text.',
    Icon: BookIcon,
  },
  {
    title: 'Privacy first',
    body: 'Runs entirely on your computer. No internet, no API key, nothing leaves your machine.',
    Icon: ShieldIcon,
  },
];

export default function Welcome({ index, total, next }: StepProps) {
  return (
    <div className="flex-1 flex flex-col items-center px-12 pt-2 pb-8 min-h-0 overflow-y-auto">
      <div className="w-[64px] h-[64px] rounded-[14px] bg-[#1f1f1c] grid place-items-center mb-4">
        <svg viewBox="0 0 48 48" width="40" height="40" xmlns="http://www.w3.org/2000/svg">
          <g transform="translate(24, 24)">
            <rect x="-8" y="-17" width="16" height="22" rx="8" fill="#fafaf9" />
            <path
              d="M -13 4 Q -13 17 0 17 Q 13 17 13 4"
              fill="none"
              stroke="#fafaf9"
              strokeWidth="3"
              strokeLinecap="round"
            />
            <line x1="0" y1="17" x2="0" y2="24" stroke="#fafaf9" strokeWidth="3" strokeLinecap="round" />
          </g>
        </svg>
      </div>

      <h1 className="font-serif text-[30px] tracking-[-0.5px] text-text-primary m-0 mb-1.5 text-center">
        Welcome to Murmr
      </h1>
      <p className="text-[13.5px] text-text-tertiary leading-[1.55] text-center max-w-[440px] m-0 mb-6">
        Voice dictation that runs entirely on your machine. Speak. It types.
      </p>

      <div className="grid grid-cols-2 gap-x-6 gap-y-4 w-full max-w-[600px] mb-8">
        {FEATURES.map((f) => (
          <div key={f.title} className="flex items-start gap-3">
            <div className="w-[28px] h-[28px] rounded-[8px] bg-bg-control text-text-secondary grid place-items-center flex-shrink-0 mt-0.5">
              <f.Icon />
            </div>
            <div className="min-w-0">
              <div className="text-[13px] text-text-primary font-medium mb-0.5">{f.title}</div>
              <div className="text-[12px] text-text-tertiary leading-[1.45]">{f.body}</div>
            </div>
          </div>
        ))}
      </div>

      <PrimaryButton onClick={next}>Get started</PrimaryButton>

      <div className="mt-6">
        <ProgressDots index={index} total={total} />
      </div>
    </div>
  );
}

function iconProps() {
  return {
    width: 14,
    height: 14,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 1.8,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
  };
}

function ShieldIcon() {
  return (
    <svg {...iconProps()}>
      <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
    </svg>
  );
}
function KeyIcon() {
  return (
    <svg {...iconProps()}>
      <rect x="3" y="6" width="18" height="12" rx="2" />
      <path d="M7 10h.01M11 10h.01M15 10h.01M7 14h10" />
    </svg>
  );
}
function WindowIcon() {
  return (
    <svg {...iconProps()}>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <line x1="3" y1="9" x2="21" y2="9" />
    </svg>
  );
}
function WaveIcon() {
  return (
    <svg {...iconProps()}>
      <path d="M2 12h2l3-7 5 14 3-7 4 5h3" />
    </svg>
  );
}
function BookIcon() {
  return (
    <svg {...iconProps()}>
      <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
      <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
    </svg>
  );
}
function SparkleIcon() {
  return (
    <svg {...iconProps()}>
      <path d="M12 3v3M12 18v3M5 12H2M22 12h-3M6 6l2 2M16 16l2 2M6 18l2-2M16 8l2-2" />
    </svg>
  );
}
