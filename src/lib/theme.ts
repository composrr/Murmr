export type Theme = 'light' | 'dark' | 'auto';

const STORAGE_KEY = 'murmr.theme';
// One-time switch: the liquid-glass redesign is dark first, so everyone
// starts in dark once. They can still pick Light or Auto afterwards.
const GLASS_DEFAULT_KEY = 'murmr.theme.glassDefault';

export function getStoredTheme(): Theme {
  try {
    if (!localStorage.getItem(GLASS_DEFAULT_KEY)) {
      localStorage.setItem(GLASS_DEFAULT_KEY, '1');
      localStorage.setItem(STORAGE_KEY, 'dark');
      return 'dark';
    }
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === 'light' || raw === 'dark' || raw === 'auto') return raw;
  } catch {
    /* storage blocked: fall through to the default */
  }
  return 'dark';
}

export function setStoredTheme(theme: Theme): void {
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    /* ignore */
  }
}

export function resolveTheme(theme: Theme): 'light' | 'dark' {
  if (theme === 'auto') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  return theme;
}

export function applyTheme(theme: Theme): void {
  const resolved = resolveTheme(theme);
  document.documentElement.setAttribute('data-theme', resolved);
  syncWindowTheme(resolved);
}

/// Match the native title bar and the OS glass (Mica / vibrancy) to the
/// app theme. No-op outside Tauri.
function syncWindowTheme(resolved: 'light' | 'dark') {
  if (!('__TAURI_INTERNALS__' in window)) return;
  import('@tauri-apps/api/window')
    .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(resolved))
    .catch(() => {});
}
