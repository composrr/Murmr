import { useEffect, useState } from 'react';
import { listenStatus, type DictationStatus } from '../lib/ipc';

const INITIAL: DictationStatus = { kind: 'idle' };

export function useDictationStatus() {
  const [status, setStatus] = useState<DictationStatus>(INITIAL);
  const [lastInjected, setLastInjected] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    listenStatus((s) => {
      if (cancelled) return;
      setStatus(s);
      if (s.kind === 'injected') setLastInjected(s.text);
    }).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  return { status, lastInjected };
}
