import StepFrame from './StepFrame';
import type { StepProps } from '../App';

export default function Permissions(props: StepProps) {
  return (
    <StepFrame
      {...props}
      title="A few permissions"
      subtitle="Murmr never sends audio or text off your machine. Everything runs locally."
    >
      <ul className="space-y-3">
        <Item
          ok
          title="Local-only Whisper"
          body="The 147 MB base.en model is bundled with the app. No internet required, ever."
        />
        <Item
          ok
          title="Microphone access"
          body="Windows asks for permission the first time Murmr listens. You can revoke any time in Settings → Privacy → Microphone."
        />
        <Item
          warn
          title="Type-into-other-apps permission"
          body="On Windows that's automatic. (On macOS we'd ask for Accessibility access here.)"
        />
        <Item
          ok
          title="Saved locally"
          body={`Transcriptions and settings live at %APPDATA%\\app.murmr.desktop\\.`}
        />
      </ul>
    </StepFrame>
  );
}

function Item({
  warn,
  title,
  body,
}: {
  ok?: boolean;
  warn?: boolean;
  title: string;
  body: string;
}) {
  return (
    <li className="flex items-start gap-3 py-2">
      <span
        className={
          'mt-[3px] w-[16px] h-[16px] rounded-full grid place-items-center flex-shrink-0 ' +
          (warn ? 'bg-bg-control text-text-secondary' : 'bg-[#1f1f1c] text-[#fafaf9]')
        }
      >
        {warn ? (
          <svg width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><line x1="12" y1="8" x2="12" y2="13" /><line x1="12" y1="17" x2="12.01" y2="17" /></svg>
        ) : (
          <svg width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="4" strokeLinecap="round" strokeLinejoin="round"><polyline points="20 6 9 17 4 12" /></svg>
        )}
      </span>
      <div>
        <div className="text-[13px] text-text-primary font-medium">{title}</div>
        <div className="text-[12px] text-text-tertiary leading-[1.55]">{body}</div>
      </div>
    </li>
  );
}
