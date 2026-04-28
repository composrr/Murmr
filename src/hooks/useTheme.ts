import { useCallback, useEffect, useState } from 'react';
import { applyTheme, getStoredTheme, setStoredTheme, type Theme } from '../lib/theme';

export function useTheme() {
  const [theme, setThemeState] = useState<Theme>(() => getStoredTheme());

  useEffect(() => {
    applyTheme(theme);
    setStoredTheme(theme);
  }, [theme]);

  useEffect(() => {
    if (theme !== 'auto') return;
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = () => applyTheme('auto');
    media.addEventListener('change', handler);
    return () => media.removeEventListener('change', handler);
  }, [theme]);

  const setTheme = useCallback((next: Theme) => {
    setThemeState(next);
  }, []);

  return { theme, setTheme };
}
