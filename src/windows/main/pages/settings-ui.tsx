/// Shared primitives for settings pages: title, glass rows, toggle,
/// segmented control. Consecutive <Row>s join into one glass card on their
/// own (see `.s-row` in settings-ui.css), so pages stay a plain list of rows.

import type { ReactNode } from 'react';
import './settings-ui.css';

export function SettingsHeader({
  title,
  subtitle,
}: {
  title: string;
  subtitle?: ReactNode;
}) {
  return (
    <header className="mb-8 fade-up">
      <h1 className="page-title">{title}</h1>
      {subtitle && (
        <p className="text-[14px] text-text-secondary mt-2 leading-[1.5]">{subtitle}</p>
      )}
    </header>
  );
}

export function SectionHeader({ children }: { children: ReactNode }) {
  return <h2 className="eyebrow mt-9 mb-3 px-1">{children}</h2>;
}

export function Row({
  name,
  hint,
  children,
}: {
  name: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="s-row flex items-center justify-between gap-6">
      <div className="flex-1 min-w-0">
        <div className="text-[13.5px] text-text-primary font-medium tracking-[-0.15px]">
          {name}
        </div>
        {hint && (
          <div className="text-[12px] leading-[1.5] text-text-tertiary mt-[3px]">{hint}</div>
        )}
      </div>
      <div className="flex-shrink-0">{children}</div>
    </div>
  );
}

/// A glass card for content that is not a list of rows.
export function Card({
  children,
  className = '',
  glow = false,
}: {
  children: ReactNode;
  className?: string;
  glow?: boolean;
}) {
  return (
    <div className={`glass overflow-hidden ${className}`}>
      {glow && <div className="card-glow" aria-hidden="true" />}
      <div className="relative">{children}</div>
    </div>
  );
}

export function Toggle({
  on,
  onChange,
  disabled,
  accent = false,
}: {
  on: boolean;
  onChange?: (value: boolean) => void;
  disabled?: boolean;
  /** Use the accent colour when on (reserved for the one feature that should pop). */
  accent?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      onClick={() => !disabled && onChange?.(!on)}
      disabled={disabled}
      className={
        'w-[40px] h-[24px] rounded-full relative transition-colors duration-200 ' +
        (disabled ? ' opacity-50 cursor-not-allowed' : '')
      }
      style={{
        background: on ? (accent ? 'var(--accent)' : 'var(--toggle-on-bg)') : 'var(--toggle-off-bg)',
        boxShadow: on && accent ? '0 0 16px rgb(var(--accent-rgb) / 0.45)' : 'inset 0 0 0 1px var(--border-hairline)',
      }}
    >
      <span
        className="absolute top-[3px] left-[3px] w-[18px] h-[18px] rounded-full transition-transform duration-200 ease-[cubic-bezier(0.3,0.7,0.3,1)]"
        style={{
          transform: on ? 'translateX(16px)' : 'none',
          background: on ? (accent ? 'var(--accent-ink)' : 'var(--toggle-on-thumb)') : 'var(--toggle-off-thumb)',
          boxShadow: '0 1px 3px rgba(0,0,0,0.3)',
        }}
      />
    </button>
  );
}

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  disabled,
}: {
  options: Array<{ value: T; label: string }>;
  value: T;
  onChange: (v: T) => void;
  disabled?: boolean;
}) {
  return (
    <div className="inline-flex gap-[2px] rounded-full p-[3px] bg-bg-control border border-border-hairline">
      {options.map((opt) => {
        const active = value === opt.value;
        return (
          <button
            key={opt.value}
            onClick={() => !disabled && onChange(opt.value)}
            disabled={disabled}
            className={
              'text-[12px] h-[26px] px-[12px] rounded-full transition-colors duration-150 ' +
              (active
                ? 'text-text-primary font-medium'
                : 'bg-transparent text-text-tertiary hover:text-text-primary')
            }
            style={
              active
                ? {
                    background: 'var(--nav-active)',
                    boxShadow: 'var(--glass-highlight), 0 0 0 1px var(--glass-border)',
                  }
                : undefined
            }
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}

export function NativeSelect<T extends string | number>({
  value,
  onChange,
  options,
  disabled,
}: {
  value: T;
  onChange: (v: T) => void;
  options: Array<{ value: T; label: string }>;
  disabled?: boolean;
}) {
  return (
    <div className="relative inline-flex">
      <select
        value={String(value)}
        disabled={disabled}
        onChange={(e) => {
          const raw = e.target.value;
          const matched = options.find((o) => String(o.value) === raw);
          if (matched) onChange(matched.value);
        }}
        className={
          'appearance-none text-[13px] h-[34px] pl-3 pr-8 rounded-[10px] border border-border-control bg-bg-control text-text-primary outline-none hover:border-border-strong focus:border-accent/60 transition-colors' +
          (disabled ? ' opacity-50 cursor-not-allowed' : ' cursor-pointer')
        }
      >
        {options.map((o) => (
          <option key={String(o.value)} value={String(o.value)}>
            {o.label}
          </option>
        ))}
      </select>
      <svg
        className="pointer-events-none absolute right-[10px] top-1/2 -translate-y-1/2 text-text-tertiary"
        width="12"
        height="12"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <path d="M6 9l6 6 6-6" />
      </svg>
    </div>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd
      className="inline-flex items-center h-[26px] px-[10px] rounded-[8px] text-[12px] font-medium text-text-primary"
      style={{
        background: 'var(--glass-bg-strong)',
        border: '1px solid var(--border-control)',
        boxShadow: 'var(--glass-highlight), 0 1px 0 rgba(0,0,0,0.25)',
        fontFamily: 'inherit',
      }}
    >
      {children}
    </kbd>
  );
}

export function SecondaryButton({
  children,
  onClick,
  disabled,
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <button type="button" onClick={onClick} disabled={disabled} className="btn-ghost">
      {children}
    </button>
  );
}

export function Pill({ children, kind }: { children: ReactNode; kind?: 'info' | 'warn' }) {
  const cls =
    kind === 'warn'
      ? 'bg-danger/10 text-danger border-danger/30'
      : 'bg-bg-control text-text-tertiary border-border-hairline';
  return (
    <span
      className={`inline-flex items-center text-[10px] uppercase tracking-[0.07em] font-semibold px-[8px] h-[20px] rounded-full border ${cls}`}
    >
      {children}
    </span>
  );
}
