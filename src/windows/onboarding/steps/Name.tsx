import { useEffect, useRef, useState } from 'react';
import StepFrame from './StepFrame';
import type { StepProps } from '../App';
import { getSettings, saveSettings, userInfo } from '../../../lib/ipc';

export default function Name(props: StepProps) {
  const [name, setName] = useState('');
  const [loaded, setLoaded] = useState(false);
  const ref = useRef<HTMLInputElement>(null);

  // Pre-fill with whatever's already in settings, falling back to the OS
  // user's first name. Don't block the user — they can clear it freely.
  useEffect(() => {
    (async () => {
      try {
        const s = await getSettings();
        if (s.display_name) {
          setName(s.display_name);
          setLoaded(true);
          return;
        }
      } catch {}
      try {
        const u = await userInfo();
        setName(u.display_name);
      } catch {}
      setLoaded(true);
    })();
  }, []);

  useEffect(() => {
    if (loaded) ref.current?.focus();
  }, [loaded]);

  const onContinue = async () => {
    try {
      const s = await getSettings();
      await saveSettings({ ...s, display_name: name.trim() });
    } catch (e) {
      console.error('save name failed', e);
    }
    props.next();
  };

  return (
    <StepFrame
      {...props}
      title="What should I call you?"
      subtitle="Murmr greets you on the home screen. You can change this later in Settings."
      onPrimary={onContinue}
    >
      <div className="max-w-[440px]">
        <label className="text-[11px] uppercase tracking-[0.6px] text-text-quaternary font-medium block mb-2">
          Display name
        </label>
        <input
          ref={ref}
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') onContinue();
          }}
          placeholder="Jon"
          maxLength={32}
          className="w-full text-[16px] font-serif text-text-primary border border-border-control rounded-[10px] px-4 py-[10px] bg-bg-row placeholder:text-text-quaternary placeholder:font-sans placeholder:text-[14px] focus:outline-none focus:border-text-quaternary"
        />
        <p className="text-[12px] text-text-tertiary mt-3 leading-[1.55]">
          Will show as{' '}
          <span className="font-serif text-text-primary">
            "Welcome back, {name.trim() || 'friend'}"
          </span>{' '}
          on the home screen.
        </p>
      </div>
    </StepFrame>
  );
}
