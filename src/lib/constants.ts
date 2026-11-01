import type { Theme, ThemeSettings } from '$lib/types';

export const THEMES: Theme[] = [
  {
    id: 'default',
    name: 'Default',
    primary: '#bb9af7',
    background: '#1b1b1b',
    accent: '#2a2a2a',
  },
  {
    id: 'tokyo-night',
    name: 'Tokyo Night',
    primary: '#bb9af7',
    background: '#1a1b26',
    accent: '#24283b',
  },
  {
    id: 'sakura',
    name: 'Sakura Pink',
    primary: '#f978b6',
    background: '#1f1d2e',
    accent: '#2d2a3e',
  },
  {
    id: 'ocean-blue',
    name: 'Ocean Blue',
    primary: '#7aa2f7',
    background: '#192330',
    accent: '#24283b',
  },
  {
    id: 'forest-green',
    name: 'Forest Green',
    primary: '#9ece6a',
    background: '#1e2030',
    accent: '#282e44',
  },
];

export const COLOR_SWATCHES = [
  '#3b82f6', // Blue
  '#bb9af7', // Purple
  '#f978b6', // Pink
  '#9ece6a', // Green
  '#7aa2f7', // Light blue
  '#e0af68', // Orange
  '#f7768e', // Red
  '#ffae00', // Yellow
];

export const DEFAULT_THEME_SETTINGS: ThemeSettings = {
  theme: 'default',
  accent_color: '#bb9af7',
  use_custom_color: false,
};

export const CODEPAGE_OPTIONS = [
  { value: 932, label: '932 — Shift-JIS (Japanese Default)' },
  { value: 65001, label: '65001 — UTF-8' },
  { value: 1200, label: '1200 — UTF-16LE' },
  { value: 936, label: '936 — GBK (Simplified Chinese)' },
  { value: 950, label: '950 — Big5 (Traditional Chinese)' },
  { value: 949, label: '949 — EUC-KR (Korean)' },
  { value: 1252, label: '1252 — Western European (Latin)' },
];
