import { THEMES } from './constants';
import type { ThemeSettings } from './types';

export function applyTheme(settings: ThemeSettings): void {
  if (typeof document === 'undefined') return;
  const theme = THEMES.find((t) => t.id === settings.theme) || THEMES[0];

  const primaryColor = settings.use_custom_color
    ? settings.accent_color
    : theme.primary;

  document.documentElement.style.setProperty('--primary', primaryColor);
  document.documentElement.style.setProperty('--secondary', primaryColor);
  document.documentElement.style.setProperty(
    '--main-background',
    theme.background,
  );
  document.documentElement.style.setProperty('--accent', theme.accent);
  document.documentElement.style.setProperty('--main-text', '#ffffff');
  document.documentElement.style.setProperty('--secondary-text', '#9ca3af');

  document.documentElement.setAttribute('data-theme', settings.theme);
}
