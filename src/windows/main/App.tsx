import { useEffect, useState } from 'react';
import { HashRouter, Navigate, Route, Routes } from 'react-router-dom';
import { getLicenseStatus, ping, type LicenseStatus } from '../../lib/ipc';
import { useTheme } from '../../hooks/useTheme';
import { UpdaterProvider, useSharedUpdater } from '../../hooks/UpdaterContext';
import Paywall from './Paywall';
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
  const [license, setLicense] = useState<LicenseStatus | null>(null);

  useEffect(() => {
    ping()
      .then((p) => setVersion(p.version))
      .catch(() => {});
    // License check is gating — don't show ANYTHING until we know.
    getLicenseStatus()
      .then(setLicense)
      .catch((e) => setLicense({ kind: 'malformed', reason: String(e) }));
  }, []);

  // Re-show the banner if a new version arrives after a previous dismissal.
  useEffect(() => {
    if (updateState.kind === 'available') setBannerDismissed(false);
  }, [updateState.kind]);

  const showBanner =
    !bannerDismissed &&
    (updateState.kind === 'available' ||
      updateState.kind === 'downloading' ||
      updateState.kind === 'ready' ||
      updateState.kind === 'error');

  // Initial license fetch hasn't returned yet — render nothing rather than
  // flash the main UI for a frame.
  if (license === null) {
    return <div className="h-screen bg-bg-window" />;
  }

  // No license / invalid → paywall instead of main UI. The paywall is
  // self-contained (no router, no sidebar) and calls onLicensed when the
  // user pastes a valid key.
  if (license.kind !== 'valid') {
    return <Paywall status={license} onLicensed={setLicense} />;
  }

  return (
    <HashRouter>
      <div className="h-screen flex flex-col bg-bg-window text-text-primary">
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
          <main className="flex-1 overflow-auto bg-bg-content px-10 py-9">
            <Routes>
              <Route path="/" element={<Home />} />
              <Route path="/insights" element={<Insights />} />
              <Route path="/dictionary" element={<Dictionary />} />
              <Route path="/general" element={<General />} />
              <Route path="/microphone" element={<Microphone />} />
              <Route path="/hotkeys" element={<Hotkeys />} />
              <Route path="/preferences" element={<Preferences />} />
              <Route path="/advanced" element={<Advanced />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Routes>
          </main>
        </div>
      </div>
    </HashRouter>
  );
}
