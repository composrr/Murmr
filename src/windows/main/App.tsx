import { useEffect, useState } from 'react';
import { HashRouter, Navigate, Route, Routes, useLocation } from 'react-router-dom';
import { ping } from '../../lib/ipc';
import { useTheme } from '../../hooks/useTheme';
import { UpdaterProvider, useSharedUpdater } from '../../hooks/UpdaterContext';
import ReleaseNotes from './ReleaseNotes';
import Sidebar from './Sidebar';
import UpdateBanner from './UpdateBanner';
import Home from './pages/Home';
import Insights from './pages/Insights';
import Dictionary from './pages/Dictionary';
import General from './pages/General';
import Microphone from './pages/Microphone';
import Hotkeys from './pages/Hotkeys';
import Preferences from './pages/Preferences';
import Advanced from './pages/Advanced';
import Permissions from './pages/Permissions';

export default function App() {
  // Wrap the actual app body in UpdaterProvider so the banner and Settings
  // page share one updater instance (single state machine, single Update
  // handle). Two separate `useUpdater()` calls deadlocked us before — a
  // check from Settings didn't update the banner.
  return (
    <UpdaterProvider>
      <AppBody />
    </UpdaterProvider>
  );
}

function AppBody() {
  useTheme();
  const [version, setVersion] = useState('0.1.0');
  const { state: updateState, installNow, dismiss } = useSharedUpdater();
  const [bannerDismissed, setBannerDismissed] = useState(false);
  // "What's new" modal — opened from the update banner or from any other
  // surface via the `murmr:open-release-notes` window event. Avoids
  // prop-drilling a callback through Sidebar / Settings / etc.
  const [showReleaseNotes, setShowReleaseNotes] = useState(false);
  useEffect(() => {
    const open = () => setShowReleaseNotes(true);
    window.addEventListener('murmr:open-release-notes', open);
    return () => window.removeEventListener('murmr:open-release-notes', open);
  }, []);

  useEffect(() => {
    ping()
      .then((p) => setVersion(p.version))
      .catch(() => {});
  }, []);

  // Re-show the banner if a new version arrives after a previous dismissal.
  useEffect(() => {
    if (updateState.kind === 'available') setBannerDismissed(false);
  }, [updateState.kind]);

  // System notification on update detected. Fires ONCE per version (tracked
  // in localStorage) so the user isn't pinged every 6h auto-check while the
  // banner sits there. Especially useful when the main window is hidden in
  // the tray — without this, the user has no idea an update's waiting.
  useEffect(() => {
    if (updateState.kind !== 'available') return;
    const targetVersion = updateState.version;
    if (!targetVersion) return;

    (async () => {
      try {
        const { isPermissionGranted, requestPermission, sendNotification } =
          await import('@tauri-apps/plugin-notification');

        const lastNotified = localStorage.getItem('murmr.lastNotifiedVersion');
        if (lastNotified === targetVersion) return; // already pinged for this version

        let granted = await isPermissionGranted();
        if (!granted) {
          const result = await requestPermission();
          granted = result === 'granted';
        }
        if (!granted) {
          // User denied OS notifications. Silently move on — banner is
          // still visible inside the main window.
          localStorage.setItem('murmr.lastNotifiedVersion', targetVersion);
          return;
        }

        sendNotification({
          title: 'Murmr update available',
          body: `v${targetVersion} is ready to install. Open Murmr to update.`,
        });
        localStorage.setItem('murmr.lastNotifiedVersion', targetVersion);
      } catch (e) {
        console.warn('[updater] toast failed:', e);
      }
    })();
  }, [updateState.kind, updateState.kind === 'available' ? updateState.version : null]);

  const showBanner =
    !bannerDismissed &&
    (updateState.kind === 'available' ||
      updateState.kind === 'downloading' ||
      updateState.kind === 'ready' ||
      updateState.kind === 'error');

  return (
    <HashRouter>
      <div className="h-screen flex flex-col bg-bg-window text-text-primary">
        {showReleaseNotes && (
          <ReleaseNotes onClose={() => setShowReleaseNotes(false)} />
        )}
        {showBanner && (
          <UpdateBanner
            state={updateState}
            onInstall={installNow}
            onDismiss={() => {
              setBannerDismissed(true);
              dismiss();
            }}
          />
        )}
        <div className="flex-1 min-h-0 flex">
          <Sidebar version={version} />
          <main className="relative flex-1 min-w-0 overflow-auto">
            <ScrollReset />
            <div className="page-glow" aria-hidden="true" />
            <div className="relative z-[1] px-10 pt-9 pb-12 max-w-[1080px] mx-auto">
            <Routes>
              <Route path="/" element={<Home />} />
              <Route path="/insights" element={<Insights />} />
              <Route path="/dictionary" element={<Dictionary />} />
              <Route path="/general" element={<General />} />
              <Route path="/microphone" element={<Microphone />} />
              <Route path="/hotkeys" element={<Hotkeys />} />
              <Route path="/permissions" element={<Permissions />} />
              <Route path="/preferences" element={<Preferences />} />
              <Route path="/advanced" element={<Advanced />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Routes>
            </div>
          </main>
        </div>
      </div>
    </HashRouter>
  );
}

/// Each page starts at the top (the scroll container is shared).
function ScrollReset() {
  const { pathname } = useLocation();
  useEffect(() => {
    document.querySelector('main')?.scrollTo({ top: 0 });
  }, [pathname]);
  return null;
}
