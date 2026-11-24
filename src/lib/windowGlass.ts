/// Turns on see-through window glass for the main window.
///
/// The OS draws the material (tauri.conf.json `windowEffects`: Mica on
/// Windows 11, vibrancy on macOS). We only make the page ground translucent
/// when we are sure the OS material is there; otherwise a transparent window
/// would show the raw desktop with no blur.
///
/// - macOS: vibrancy is always available (macOS 13+ is the minimum).
/// - Windows: Mica needs Windows 11 (platformVersion major >= 13).
export async function enableNativeGlass(): Promise<void> {
  if (!('__TAURI_INTERNALS__' in window)) return;
  const ua = navigator.userAgent;
  let ok = false;
  if (/Mac/i.test(ua)) {
    ok = true;
  } else if (/Windows/i.test(ua)) {
    try {
      const uaData = (navigator as Navigator & {
        userAgentData?: {
          getHighEntropyValues(hints: string[]): Promise<{ platformVersion?: string }>;
        };
      }).userAgentData;
      const v = await uaData?.getHighEntropyValues(['platformVersion']);
      const major = parseInt((v?.platformVersion ?? '0').split('.')[0], 10);
      ok = major >= 13;
    } catch {
      ok = false;
    }
  }
  if (ok) {
    document.documentElement.classList.add('native-glass');
  } else {
    // No OS glass: make sure the window is painted solid.
    document.documentElement.classList.remove('native-glass');
  }
}
