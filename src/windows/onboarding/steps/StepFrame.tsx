import type { ReactNode } from 'react';
import { PrimaryButton, ProgressDots, SecondaryButton, type StepProps } from '../App';

interface FrameProps extends StepProps {
  title: string;
  subtitle?: ReactNode;
  children: ReactNode;
  primaryLabel?: string;
  primaryDisabled?: boolean;
  onPrimary?: () => void;
  hideBack?: boolean;
}

export default function StepFrame({
  index,
  total,
  next,
  back,
  title,
  subtitle,
  children,
  primaryLabel = 'Continue',
  primaryDisabled,
  onPrimary,
  hideBack,
}: FrameProps) {
  const handlePrimary = onPrimary ?? next;
  return (
    <div className="flex-1 flex flex-col px-12 pt-2 pb-8 min-h-0">
      <div className="flex-1 min-h-0 overflow-hidden flex flex-col">
        <h1 className="font-serif text-[26px] tracking-[-0.4px] text-text-primary m-0 mb-2">
          {title}
        </h1>
        {subtitle && (
          <p className="text-[13.5px] text-text-tertiary leading-[1.55] m-0 mb-5 max-w-[560px]">
            {subtitle}
          </p>
        )}
        <div className="flex-1 min-h-0 overflow-y-auto pr-1 -mr-1">{children}</div>
      </div>

      <div className="flex items-center justify-between mt-5 pt-4 border-t border-border-hairline flex-shrink-0">
        {hideBack ? <span /> : <SecondaryButton onClick={back}>Back</SecondaryButton>}
        <ProgressDots index={index} total={total} />
        <PrimaryButton onClick={handlePrimary} disabled={primaryDisabled}>
          {primaryLabel}
        </PrimaryButton>
      </div>
    </div>
  );
}
