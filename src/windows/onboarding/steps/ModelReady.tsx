import StepFrame from './StepFrame';
import type { StepProps } from '../App';

export default function ModelReady(props: StepProps) {
  return (
    <StepFrame
      {...props}
      title="Speech model"
      subtitle="Murmr ships with the model already on disk — no download, no internet, no API key."
    >
      <div className="rounded-[12px] bg-bg-row border border-border-hairline p-5 max-w-[560px]">
        <div className="flex items-center gap-3 mb-2">
          <span className="w-[18px] h-[18px] rounded-full bg-[#1f1f1c] dark:bg-[#d4d4cf] grid place-items-center text-[#fafaf9] dark:text-[#1f1f1c] text-[10px] font-bold">
            ✓
          </span>
          <span className="font-serif text-[18px] text-text-primary">base.en</span>
          <span className="text-[11px] uppercase tracking-[0.6px] text-text-quaternary font-medium">
            ready · 147 MB
          </span>
        </div>
        <div className="text-[12px] text-text-tertiary leading-[1.55]">
          Whisper's English-only base model is the right balance of speed and accuracy on a modern
          CPU. Most dictation finishes in 1–3 seconds.
        </div>
      </div>

      <div className="mt-5 grid grid-cols-2 gap-3 max-w-[560px]">
        <Alt
          name="tiny.en"
          size="~75 MB"
          tradeoff="Faster but noticeably less accurate. Good on slow machines."
        />
        <Alt
          name="small.en"
          size="~466 MB"
          tradeoff="More accurate, especially on accents. Slower transcribe time."
        />
      </div>

      <p className="text-[11px] text-text-quaternary mt-5 max-w-[560px] leading-[1.55]">
        Switching models lands in Settings → Advanced (Phase 9 wiring). For now, base.en is what
        Murmr uses for every transcription.
      </p>
    </StepFrame>
  );
}

function Alt({ name, size, tradeoff }: { name: string; size: string; tradeoff: string }) {
  return (
    <div className="rounded-[10px] border border-border-hairline p-4 opacity-70">
      <div className="flex items-baseline justify-between mb-1.5">
        <span className="font-serif text-[16px] text-text-primary">{name}</span>
        <span className="text-[11px] text-text-quaternary tabular-nums">{size}</span>
      </div>
      <div className="text-[12px] text-text-tertiary leading-[1.5]">{tradeoff}</div>
    </div>
  );
}
