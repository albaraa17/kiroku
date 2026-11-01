export interface ThreadParam {
  process_id: number;
  addr: number;
  ctx: number;
  ctx2: number;
}

export interface SavedHookContext {
  hookcode: string;
  ctx1: number;
  ctx2: number;
  name: string;
  custom_flush_delay?: number | null;
  custom_codepage?: number | null;
}

export interface RegexFilterRule {
  id: string;
  pattern: string;
  process_path: string | null;
  enabled: boolean;
}

export interface RegexReplaceRule {
  id: string;
  pattern: string;
  replacement: string;
  ignore_case: boolean;
  process_path: string | null;
  enabled: boolean;
}

export interface StringReplaceRule {
  id: string;
  original: string;
  replacement: string;
  enabled: boolean;
}

export interface Theme {
  id: string;
  name: string;
  primary: string;
  background: string;
  accent: string;
}

export interface ThemeSettings {
  theme: string;
  accent_color: string;
  use_custom_color: boolean;
}

export interface AppConfig {
  flush_delay: number;
  default_codepage: number;
  max_buffer_size: number;
  max_history_size: number;
  enable_pc_hooks: boolean;
  filter_repetition: boolean;
  auto_attach: boolean;
  auto_attach_saved_only: boolean;

  ws_broadcast_enabled: boolean;
  ws_broadcast_port: number;
  to_clipboard: boolean;
  minimize_to_tray: boolean;

  knife_ws_enabled: boolean;
  knife_ws_url: string;
  knife_auto_read_ini: boolean;

  remove_repeated_chars: boolean;
  remove_repeated_phrases: boolean;
  strip_html_ruby_tags: boolean;
  remove_speaker_brackets: boolean;
  filter_chinese_segments: boolean;
  normalize_whitespace: boolean;
  hide_garbage_threads: boolean;

  regex_filters: RegexFilterRule[];
  regex_replacements: RegexReplaceRule[];
  string_replacements: StringReplaceRule[];

  auto_attach_exes: string[];
  saved_hooks: Record<string, SavedHookContext>;

  theme_settings: ThemeSettings;
}

export interface ProcessEntry {
  pid: number;
  name: string;
  title: string;
  exe_path: string;
  is_64bit: boolean;
  is_saved: boolean;
  is_knife: boolean;
  icon_data_url: string | null;
}

export interface AttachedProcess {
  pid: number;
  name: string;
  exe_path: string;
  is_64bit: boolean;
  is_knife: boolean;
  icon_data_url: string | null;
}

export interface SentenceEntry {
  id: number;
  timestamp: string;
  raw: string;
  cleaned: string;
  char_count: number;
}

export interface HookThreadView {
  key: string;
  tp: ThreadParam;
  hookcode: string;
  name: string;
  is_embeddable: boolean;
  is_saved_match: boolean;
  is_knife: boolean;
  score: number;
  latest_raw: string;
  latest_cleaned: string;
  total_sentences: number;
  total_chars: number;
  history: SentenceEntry[];
}

export interface DashboardSnapshot {
  attached_processes: AttachedProcess[];
  threads: HookThreadView[];
  selected_thread_key: string | null;
  ws_port: number;
  ws_clients: number;
  knife_connected: boolean;
  knife_url: string;
  console_logs: string[];
  total_session_chars: number;
  total_session_sentences: number;
}

export type NavTab = 'console' | 'processes' | 'cleanup' | 'settings';
