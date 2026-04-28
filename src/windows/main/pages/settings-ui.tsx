/// Shared primitives for settings pages: title, hairline-row, toggle,
/// segmented control. Keeping them tight so each page reads as a list of
/// rows rather than a wall of Tailwind classes.

import type { ReactNode } from 'react';

export function SettingsHeader({
  title,
  subtitle,
}: {
  title: string;
  subtitle?: ReactNode;
}) {
  return (
    <>
      <h1 className="font-serif text-[30px] tracking-[-0.4px] text-text-primary mb-1">
        {title}
      </h1>
      {subtitle && (
        <p className="text-[13px] text-text-quaternary mb-7">{subtitle}</p>
      )}
    </>
  );
}

export function SectionHeader({ children }: { children: ReactNode }) {
  return (
    <h2 className="font-serif text-[22px] tracking-[-0.3px] text-text-primary mt-8 mb-3">
      {children}
    </h2>
  );
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
    <div className="flex items-center justify-between gap-6 py-5 border-b border-border-hairline last:border-b-0">
      <div className="flex-1 min-w-0">
        <div className="text-[14px] text-text-primary font-medium tracking-[-0.1px]">
          {name}
        </div>
        {hint && (
          <div className="text-[12px] text-text-tertiary tracking-[-0.1px]">
            {hint}
          </div>
        )}
      </div>
      <div className="flex-shrink-0">{children}</div>
    </div>
  );
}

export function Toggle({
  on,
  onChange,
  disabled,
}: {
  on: boolean;
  onChange?: (value: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={() => !disabled && onChange?.(!on)}
      disabled={disabled}
      className={
        'w-[38px] h-[22px] rounded-full relative transition-colors ' +
        (on ? 'bg-[var(--toggle-on-bg)]' : 'bg-[var(--toggle-off-bg)]') +
        (disabled ? ' opacity-60 cursor-not-allowed' : '')
      }
    >
      <span
        className={
          'absolute top-[2px] w-[18px] h-[18px] rounded-full transition-all ' +
          (on
            ? 'right-[2px] bg-[var(--toggle-on-thumb)]'
            : 'left-[2px] bg-white')
        }
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
    <div className="flex gap-[2px] bg-bg-control rounded-control p-[2px]">
      {options.map((opt) => (
        <button
          key={opt.value}
          onClick={() => !disabled && onChange(opt.value)}
          disabled={disabled}
          className={
            'text-[12px] px-[11px] py-[4px] rounded-[5px] transition-colors ' +
            (value === opt.value
              ? 'bg-bg-content text-text-primary font-medium shadow-[0_1px_2px_rgba(0,0,0,0.04)]'
              : 'bg-transparent text-text-tertiary hover:text-text-secondary')
          }
        >
          {opt.label}
        </button>
      ))}
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
    <select
      value={String(value)}
      disabled={disabled}
      onChange={(e) => {
        const raw = e.target.value;
        const matched = options.find((o) => String(o.value) === raw);
        if (matched) onChange(matched.value);
      }}
      className={
        'text-[13px] border border-border-control rounded-[7px] px-3 py-[6px] bg-bg-content text-text-primary' +
        (disabled ? ' opacity-60 cursor-not-allowed' : '')
      }
    >
      {options.map((o) => (
        <option key={String(o.value)} value={String(o.value)}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="inline-block bg-bg-control border border-border-control rounded-[7px] px-3 py-[5px] text-[12px] font-medium text-text-primary shadow-[0_1px_0_rgba(0,0,0,0.04)]">
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
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="text-[12px] px-[14px] py-[6px] rounded-[8px] border border-border-control bg-bg-content text-text-primary font-medium hover:bg-bg-control disabled:opacity-50"
    >
      {children}
    </button>
  );
}

export function Pill({ children, kind }: { children: ReactNode; kind?: 'info' | 'warn' }) {
  const cls =
    kind === 'warn'
      ? 'bg-[#3a302a]/10 text-text-secondary border-[#bcb5a4]'
      : 'bg-bg-row text-text-tertiary border-border-hairline';
  return (
    <span className={`text-[10px] uppercase tracking-[0.6px] font-medium px-[7px] py-[2px] rounded-[6px] border ${cls}`}>
      {children}
    </span>
  );
}
