import { NavLink } from 'react-router-dom';
import { useEffect, useState, type ComponentType, type SVGProps } from 'react';
import {
  engineStatus,
  isMac,
  listenEngineStatus,
  modelCatalog,
  type CatalogModel,
  type EngineStatus,
} from '../../lib/ipc';
import { useDictationStatus } from '../../hooks/useDictationStatus';

interface NavItem {
  to: string;
  label: string;
  Icon: ComponentType<SVGProps<SVGSVGElement>>;
}

const ICON_PROPS: SVGProps<SVGSVGElement> = {
  width: 15,
  height: 15,
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 1.7,
  strokeLinecap: 'round',
  strokeLinejoin: 'round',
};

const HomeIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...ICON_PROPS} {...p}>
    <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    <polyline points="9 22 9 12 15 12 15 22" />
  </svg>
);
const InsightsIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...ICON_PROPS} {...p}>
    <line x1="6" y1="20" x2="6" y2="10" />
    <line x1="12" y1="20" x2="12" y2="4" />
    <line x1="18" y1="20" x2="18" y2="14" />
  </svg>
);
const DictionaryIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...ICON_PROPS} {...p}>
    <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
    <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
  </svg>
);
const GeneralIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...ICON_PROPS} {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3 2" />
  </svg>
);
const MicIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...ICON_PROPS} {...p}>
    <rect x="9" y="3" width="6" height="12" rx="3" />
    <path d="M5 11a7 7 0 0 0 14 0" />
    <line x1="12" y1="18" x2="12" y2="22" />
  </svg>
);
const HotkeysIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...ICON_PROPS} {...p}>
    <rect x="3" y="6" width="18" height="12" rx="2" />
    <path d="M7 10h.01M11 10h.01M15 10h.01M7 14h10" />
  </svg>
);
const PreferencesIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...ICON_PROPS} {...p}>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 1v3M12 20v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M1 12h3M20 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1" />
  </svg>
);
const AdvancedIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...ICON_PROPS} {...p}>
    <polyline points="16 18 22 12 16 6" />
    <polyline points="8 6 2 12 8 18" />
  </svg>
);
const PermissionsIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...ICON_PROPS} {...p}>
    <path d="M12 3l7 3v5c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6z" />
    <path d="M9.5 12l1.8 1.8L15 10" />
  </svg>
);

const TOP: NavItem[] = [
  { to: '/', label: 'Home', Icon: HomeIcon },
  { to: '/insights', label: 'Insights', Icon: InsightsIcon },
  { to: '/dictionary', label: 'Dictionary', Icon: DictionaryIcon },
];

const BOTTOM: NavItem[] = [
  { to: '/general', label: 'General', Icon: GeneralIcon },
  { to: '/microphone', label: 'Microphone', Icon: MicIcon },
  { to: '/hotkeys', label: 'Hotkeys', Icon: HotkeysIcon },
  // macOS-only: the three TCC permissions Murmr needs. Windows/Linux don't
  // gate these, so the panel (and this entry) only appear on macOS.
  ...(isMac() ? [{ to: '/permissions', label: 'Permissions', Icon: PermissionsIcon }] : []),
  { to: '/preferences', label: 'Preferences', Icon: PreferencesIcon },
  { to: '/advanced', label: 'Advanced', Icon: AdvancedIcon },
];

export default function Sidebar({ version }: { version: string }) {
  return (
    <aside
      className="relative w-[224px] flex-shrink-0 flex flex-col px-3 pt-5 pb-4 gap-[2px] border-r border-border-hairline"
      style={{
        background: 'var(--bg-chrome)',
        backdropFilter: 'blur(40px) saturate(150%)',
        WebkitBackdropFilter: 'blur(40px) saturate(150%)',
      }}
    >
      <div className="flex items-center gap-[10px] px-3 pb-5">
        <BrandMark />
        <span className="text-[15px] font-semibold tracking-[-0.3px] text-text-primary">Murmr</span>
      </div>
      {TOP.map((item) => (
        <NavItemLink key={item.to} item={item} />
      ))}
      <div className="eyebrow px-3 pt-6 pb-2 !text-[10.5px] !text-text-quaternary">Settings</div>
      {BOTTOM.map((item) => (
        <NavItemLink key={item.to} item={item} />
      ))}
      <div className="mt-auto flex flex-col gap-[10px]">
        <EngineCard />
        <div className="px-3 text-[11px] text-text-quaternary num">Murmr v{version}</div>
      </div>
    </aside>
  );
}

/// Live engine state: ready / listening / writing, plus model + GPU/CPU.
function EngineCard() {
  const [engine, setEngine] = useState<EngineStatus | null>(null);
  const [catalog, setCatalog] = useState<CatalogModel[]>([]);
  const { status } = useDictationStatus();

  useEffect(() => {
    engineStatus().then(setEngine).catch(() => {});
    modelCatalog().then(setCatalog).catch(() => {});
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    listenEngineStatus(setEngine).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  if (!engine) return null;

  const busy =
    status.kind === 'recording' || status.kind === 'command-recording'
      ? { label: 'Listening', color: 'var(--danger)' }
      : status.kind === 'transcribing' || status.kind === 'command-thinking'
        ? { label: 'Writing it out', color: 'var(--accent)' }
        : null;
  const state = engine.loading
    ? { label: 'Loading model', color: 'var(--accent)' }
    : engine.last_error
      ? { label: 'Needs attention', color: 'var(--danger)' }
      : busy ?? { label: 'Ready', color: 'rgb(var(--good-rgb))' };

  const model =
    catalog.find((m) => m.file === engine.model_name)?.label ??
    engine.model_name.replace(/^ggml-/, '').replace(/\.bin$/, '');
  const backend = engine.backend.kind === 'CPU' ? 'CPU' : 'GPU';

  return (
    <div className="glass !rounded-[14px] p-3" title={engine.backend.device ?? undefined}>
      <div className="flex items-center gap-2 text-[12px] text-text-primary">
        <span
          className="w-[7px] h-[7px] rounded-full"
          style={{ background: state.color, boxShadow: `0 0 8px ${state.color}` }}
        />
        {state.label}
      </div>
      <div className="text-[11px] text-text-tertiary mt-1 truncate">
        {model} · {backend}
      </div>
    </div>
  );
}

/// Small waveform glyph in a glass tile, with the accent as the one pop.
function BrandMark() {
  return (
    <span
      className="w-[26px] h-[26px] rounded-[8px] flex items-center justify-center"
      style={{
        background: 'var(--glass-bg-strong)',
        border: '1px solid var(--border-control)',
        boxShadow: 'var(--glass-highlight)',
      }}
      aria-hidden="true"
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" strokeLinecap="round" strokeWidth="2.4">
        <line x1="5" y1="10" x2="5" y2="14" stroke="var(--text-secondary)" />
        <line x1="9.5" y1="6" x2="9.5" y2="18" stroke="var(--text-primary)" />
        <line x1="14" y1="8" x2="14" y2="16" stroke="var(--accent)" />
        <line x1="18.5" y1="10.5" x2="18.5" y2="13.5" stroke="var(--text-secondary)" />
      </svg>
    </span>
  );
}

function NavItemLink({ item }: { item: NavItem }) {
  return (
    <NavLink
      to={item.to}
      end={item.to === '/'}
      className={({ isActive }) =>
        'relative flex items-center gap-[10px] h-[34px] px-3 rounded-[10px] text-[13px] transition-colors duration-150 ' +
        (isActive
          ? 'text-text-primary font-medium'
          : 'text-text-secondary hover:text-text-primary hover:bg-bg-row')
      }
      style={({ isActive }) =>
        isActive
          ? {
              background: 'var(--nav-active)',
              boxShadow: 'var(--glass-highlight), 0 0 0 1px var(--glass-border)',
            }
          : undefined
      }
    >
      <item.Icon />
      <span>{item.label}</span>
    </NavLink>
  );
}
