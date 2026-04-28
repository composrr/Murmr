interface Props {
  title: string;
  caption?: string;
  comingIn?: string;
}

export default function PlaceholderPage({ title, caption, comingIn = 'Phase 6' }: Props) {
  return (
    <div className="max-w-[760px] mx-auto">
      <h1 className="font-serif text-[30px] tracking-[-0.4px] text-text-primary mb-2">
        {title}
      </h1>
      {caption && <p className="text-[13px] text-text-quaternary mb-7">{caption}</p>}
      <div className="mt-8 rounded-card border border-border-hairline bg-bg-row px-5 py-4">
        <div className="text-[11px] uppercase tracking-[0.6px] text-text-quaternary font-medium mb-1">
          Coming in {comingIn}
        </div>
        <p className="text-[13px] text-text-secondary leading-[1.55]">
          The full {title.toLowerCase()} experience lands in {comingIn}. The shell, sidebar, and routing
          are in place — content slides in next.
        </p>
      </div>
    </div>
  );
}
