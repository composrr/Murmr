import { useCallback, useEffect, useRef, useState } from 'react';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export type UpdaterState =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'up-to-date'; checkedAt: number }
  | {
      kind: 'available';
      version: string;
      currentVersion: string;
      notes?: string;
    }
  | { kind: 'downloading'; downloaded: number; total: number | null }
  | { kind: 'ready' }
  /** A network/parse failure during a `check()` call. Kept distinct from
   * install errors so the UI can be quiet about it (auto-checks fire on
   * every launch and on a timer; one transient failure shouldn't slap a
   * red banner across the top of the window). */
  | { kind: 'check-failed'; message: string }
  /** An error during the actual download/install. The user is actively
   * waiting for this, so it warrants a visible error. */
  | { kind: 'error'; message: string };

/// Manages auto-update state. Background-checks once on mount (after a brief
/// delay so it doesn't block startup), and exposes `checkNow()` /
/// `installNow()` for explicit user actions.
///
/// The Tauri `Update` handle is held in a ref (NOT in state) so we never
/// race against React's setState batching when the user clicks Install.
/// The earlier version stored the handle inside the `available` state and
/// tried to peek at it via a setState callback after first transitioning
/// the state to `downloading` — by which point `prev.kind === 'downloading'`
/// and the handle was unreachable, causing the install click to silently
/// fall through to a fresh `check()`.
export function useUpdater(autoCheck = true) {
  const [state, setState] = useState<UpdaterState>({ kind: 'idle' });
  const checkedOnceRef = useRef(false);
  const updateHandleRef = useRef<Update | null>(null);

  const checkNow = useCallback(async () => {
    setState({ kind: 'checking' });
    try {
      console.log('[updater] check() …');
      const update = await check();
      if (update) {
        console.log(
          '[updater] available: v' +
            update.version +
            ' (currently v' +
            update.currentVersion +
            ')',
        );
        updateHandleRef.current = update;
        setState({
          kind: 'available',
          version: update.version,
          currentVersion: update.currentVersion,
          notes: update.body,
        });
      } else {
        console.log('[updater] up to date');
        updateHandleRef.current = null;
        setState({ kind: 'up-to-date', checkedAt: Date.now() });
      }
    } catch (e) {
      console.error('[updater] check failed:', e);
      setState({ kind: 'check-failed', message: String(e) });
    }
  }, []);

  const installNow = useCallback(async () => {
    try {
      // Prefer the cached handle from the most recent check; only re-check
      // if we don't have one (e.g. the user clicked Install on a stale UI
      // state after the handle got cleared).
      let upd = updateHandleRef.current;
      if (!upd) {
        console.log('[updater] no cached handle, re-checking…');
        const fresh = await check();
        if (!fresh) {
          console.log('[updater] no update on re-check');
          setState({ kind: 'up-to-date', checkedAt: Date.now() });
          return;
        }
        upd = fresh;
        updateHandleRef.current = fresh;
      }

      console.log('[updater] downloadAndInstall starting (v' + upd.version + ')');
      setState({ kind: 'downloading', downloaded: 0, total: null });

      let downloaded = 0;
      let total: number | null = null;
      await upd.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          total = event.data.contentLength ?? null;
          console.log('[updater] download started, ' + (total ?? '?') + ' bytes');
          setState({ kind: 'downloading', downloaded: 0, total });
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
          setState({ kind: 'downloading', downloaded, total });
        } else if (event.event === 'Finished') {
          console.log('[updater] download finished, installing');
          setState({ kind: 'ready' });
        }
      });

      // On Windows + Linux, downloadAndInstall replaces the binary and
      // requires a relaunch to pick up the new version. macOS replaces in
      // place and reopens automatically.
      console.log('[updater] relaunching…');
      await relaunch();
    } catch (e) {
      console.error('[updater] install failed:', e);
      setState({ kind: 'error', message: String(e) });
    }
  }, []);

  const dismiss = useCallback(() => {
    setState({ kind: 'idle' });
  }, []);

  useEffect(() => {
    if (!autoCheck || checkedOnceRef.current) return;
    checkedOnceRef.current = true;

    // First check: 4 sec after mount (so it doesn't fight model loading).
    const initial = setTimeout(() => {
      checkNow().catch(() => {});
    }, 4000);

    // Periodic re-check every 6 hours so a long-running session catches
    // updates without requiring a relaunch. Six hours is roughly a half-day
    // — frequent enough that you don't fall too far behind, infrequent
    // enough that we don't slam the GitHub Releases CDN with thousands of
    // installs every minute.
    const PERIODIC_MS = 6 * 60 * 60 * 1000;
    const periodic = setInterval(() => {
      checkNow().catch(() => {});
    }, PERIODIC_MS);

    return () => {
      clearTimeout(initial);
      clearInterval(periodic);
    };
  }, [autoCheck, checkNow]);

  return { state, checkNow, installNow, dismiss };
}
