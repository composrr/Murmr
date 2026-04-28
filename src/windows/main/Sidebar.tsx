import { NavLink } from 'react-router-dom';
import type { ComponentType, SVGProps } from 'react';

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

const TOP: NavItem[] = [
  { to: '/', label: 'Home', Icon: HomeIcon },
  { to: '/insights', label: 'Insights', Icon: InsightsIcon },
  { to: '/dictionary', label: 'Dictionary', Icon: DictionaryIcon },
];

const BOTTOM: NavItem[] = [
  { to: '/general', label: 'General', Icon: GeneralIcon },
  { to: '/microphone', label: 'Microphone', Icon: MicIcon },
  { to: '/hotkeys', label: 'Hotkeys', Icon: HotkeysIcon },
  { to: '/preferences', label: 'Preferences', Icon: PreferencesIcon },
  { to: '/advanced', label: 'Advanced', Icon: AdvancedIcon },
];

export default function Sidebar({ version }: { version: string }) {
  return (
    <aside
      className="w-[200px] bg-bg-chrome border-r border-border-hairline flex flex-col px-[10px] py-[18px] gap-[2px]"
    >
      {TOP.map((item) => (
        <NavItemLink key={item.to} item={item} />
      ))}
      <div className="h-[0.5px] bg-border-hairline mx-2 my-3" />
      {BOTTOM.map((item) => (
        <NavItemLink key={item.to} item={item} />
      ))}
      <div className="mt-auto px-3 pt-2 pb-1 text-[11px] text-text-quaternary">
        Murmr v{version}
      </div>
    </aside>
  );
}

function NavItemLink({ item }: { item: NavItem }) {
  return (
    <NavLink
      to={item.to}
      end={item.to === '/'}
      className={({ isActive }) =>
        'flex items-center gap-[10px] px-3 py-2 rounded-[8px] text-[13px] transition-colors ' +
        (isActive
          ? 'bg-bg-selected text-text-primary font-medium'
          : 'text-text-secondary hover:text-text-primary')
      }
    >
      <item.Icon />
      <span>{item.label}</span>
    </NavLink>
  );
}
