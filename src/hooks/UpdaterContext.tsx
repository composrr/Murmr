import { createContext, useContext, type ReactNode } from 'react';
import { useUpdater } from './useUpdater';

/**
 * Single shared updater instance for the whole main window.
 *
 * Originally each component (App for the banner, General for the Settings
 * button) called `useUpdater()` independently — meaning two separate state
 * machines, two separate `Update` handle refs. That broke the obvious flow
 * "user clicks Check Now in Settings → banner appears at the top": Settings'
 * instance saw the new version, but the banner's instance still held its
 * stale `up-to-date` state from the auto-check at app start.
 *
 * Fix: hoist the hook into App, expose it via context, and have every other
 * consumer read from the context. Now any check anywhere updates everyone.
 */
export type UpdaterApi = ReturnType<typeof useUpdater>;

const UpdaterContext = createContext<UpdaterApi | null>(null);

export function UpdaterProvider({ children }: { children: ReactNode }) {
  const updater = useUpdater(true);
  return (
    <UpdaterContext.Provider value={updater}>{children}</UpdaterContext.Provider>
  );
}

export function useSharedUpdater(): UpdaterApi {
  const ctx = useContext(UpdaterContext);
  if (!ctx) {
    throw new Error('useSharedUpdater must be called inside <UpdaterProvider>');
  }
  return ctx;
}
