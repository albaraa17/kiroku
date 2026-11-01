use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static RULE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn next_rule_id(prefix: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let seq = RULE_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}", prefix, ts, seq)
}

/// Checks if two executable paths refer to the same game without colliding on generic
/// engine names like `SiglusEngine.exe` or `Game.exe`.
/// Matches if:
/// 1. Full normalized paths match case-insensitively, OR
/// 2. Both `(parent_folder_name, exe_file_name)` match case-insensitively (handles drive letter changes like D:\ -> E:\).
pub fn same_game_path(path_a: &str, path_b: &str) -> bool {
    let norm_a = path_a.trim().replace('/', "\\");
    let norm_b = path_b.trim().replace('/', "\\");
    if norm_a.is_empty() || norm_b.is_empty() {
        return false;
    }
    if norm_a.eq_ignore_ascii_case(&norm_b) {
        return true;
    }

    let pa = Path::new(&norm_a);
    let pb = Path::new(&norm_b);
    let Some(exe_a) = pa.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    let Some(exe_b) = pb.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if !exe_a.eq_ignore_ascii_case(exe_b) {
        return false;
    }

    let parent_a = pa
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str());
    let parent_b = pb
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str());

    match (parent_a, parent_b) {
        (Some(fa), Some(fb)) if !fa.is_empty() && !fb.is_empty() => fa.eq_ignore_ascii_case(fb),
        _ => false,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SavedHookContext {
    pub hookcode: String,
    pub ctx1: u64,
    pub ctx2: u64,
    pub name: String,
    pub custom_flush_delay: Option<i32>,
    pub custom_codepage: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegexFilterRule {
    pub id: String,
    pub pattern: String,
    /// If Some(exe_path), only applies when attached to that game
    pub process_path: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegexReplaceRule {
    pub id: String,
    pub pattern: String,
    pub replacement: String,
    #[serde(default)]
    pub ignore_case: bool,
    pub process_path: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StringReplaceRule {
    pub id: String,
    pub original: String,
    pub replacement: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeSettings {
    pub theme: String,
    pub accent_color: String,
    pub use_custom_color: bool,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            accent_color: "#bb9af7".to_string(),
            use_custom_color: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    // 1. Customizable LunaHost / Hook Engine Settings
    pub flush_delay: i32,
    pub default_codepage: i32,
    pub max_buffer_size: i32,
    pub max_history_size: i32,
    pub enable_pc_hooks: bool,
    pub filter_repetition: bool,
    pub auto_attach: bool,
    pub auto_attach_saved_only: bool,
    pub auto_select_best_hook: bool,

    // 2. Output & Bridge Settings
    pub ws_broadcast_enabled: bool,
    pub ws_broadcast_port: u16,
    pub to_clipboard: bool,
    pub minimize_to_tray: bool,
    pub start_on_boot: bool,
    pub start_minimized: bool,

    // 3. Knife & External WebSocket Input Settings
    pub knife_ws_enabled: bool,
    pub knife_ws_url: String,
    pub knife_auto_read_ini: bool,

    // 4. Text Cleanup Pipeline Settings
    pub remove_repeated_chars: bool,
    pub remove_repeated_phrases: bool,
    pub strip_html_ruby_tags: bool,
    pub remove_speaker_brackets: bool,
    pub filter_chinese_segments: bool,
    pub normalize_whitespace: bool,
    pub hide_garbage_threads: bool,

    pub regex_filters: Vec<RegexFilterRule>,
    pub regex_replacements: Vec<RegexReplaceRule>,
    pub string_replacements: Vec<StringReplaceRule>,

    // 5. Saved Games & Hook Contexts
    pub auto_attach_exes: Vec<String>,
    pub saved_hooks: HashMap<String, SavedHookContext>,

    // 6. Tadoku Appearance Settings
    pub theme_settings: ThemeSettings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            flush_delay: 100,
            default_codepage: 932,
            max_buffer_size: 3000,
            max_history_size: 10_000_000,
            enable_pc_hooks: true,
            filter_repetition: true,
            auto_attach: true,
            auto_attach_saved_only: true,
            auto_select_best_hook: true,

            ws_broadcast_enabled: true,
            ws_broadcast_port: 9001,
            to_clipboard: false,
            minimize_to_tray: true,
            start_on_boot: false,
            start_minimized: false,

            knife_ws_enabled: true,
            knife_ws_url: "ws://127.0.0.1:6677".to_string(),
            knife_auto_read_ini: true,

            remove_repeated_chars: true,
            remove_repeated_phrases: true,
            strip_html_ruby_tags: true,
            remove_speaker_brackets: true,
            filter_chinese_segments: true,
            normalize_whitespace: true,
            hide_garbage_threads: false,

            regex_filters: Vec::new(),
            regex_replacements: Vec::new(),
            string_replacements: Vec::new(),

            auto_attach_exes: Vec::new(),
            saved_hooks: HashMap::new(),

            theme_settings: ThemeSettings::default(),
        }
    }
}

#[derive(Deserialize)]
struct LegacyLunaHostConfig {
    #[serde(rename = "AutoAttach")]
    auto_attach: Option<bool>,
    #[serde(rename = "AutoAttach_SavedOnly")]
    auto_attach_saved_only: Option<bool>,
    #[serde(rename = "ToClipboard")]
    to_clipboard: Option<bool>,
    #[serde(rename = "defaultCodepage")]
    default_codepage: Option<i32>,
    #[serde(rename = "filterRepetition")]
    filter_repetition: Option<bool>,
    #[serde(rename = "flushDelay")]
    flush_delay: Option<i32>,
    #[serde(rename = "maxBufferSize")]
    max_buffer_size: Option<i32>,
    #[serde(rename = "maxHistorySize")]
    max_history_size: Option<i32>,
    #[serde(default)]
    autoattachexes: Vec<String>,
    #[serde(default)]
    savedhookcontext: HashMap<String, SavedHookContext>,
}

impl AppConfig {
    pub fn sanitize(&mut self) {
        self.flush_delay = self.flush_delay.clamp(0, 10_000);
        self.max_buffer_size = self.max_buffer_size.clamp(64, 500_000);
        self.max_history_size = self.max_history_size.clamp(1_000, 100_000_000);
        if self.ws_broadcast_port < 1024 {
            self.ws_broadcast_port = 9001;
        }
        if self.default_codepage <= 0 {
            self.default_codepage = 932;
        }
    }

    pub fn load_or_init(config_file: &Path, defaults_dir: Option<&Path>) -> Self {
        if config_file.exists() {
            if let Ok(content) = fs::read_to_string(config_file) {
                match serde_json::from_str::<AppConfig>(&content) {
                    Ok(mut cfg) => {
                        cfg.sanitize();
                        return cfg;
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to parse config file ({}), backing up to .bak: {}",
                            config_file.display(),
                            e
                        );
                        let bak_path = config_file.with_extension("json.bak");
                        let _ = fs::copy(config_file, bak_path);
                    }
                }
            }
        }

        let mut cfg = AppConfig::default();
        if let Some(dir) = defaults_dir {
            let legacy_json = dir.join("config64.json");
            if legacy_json.exists() {
                let _ = cfg.import_legacy_config64(&legacy_json);
            }
            let legacy_filters = dir.join("SavedRegexFilters.txt");
            if legacy_filters.exists() {
                let _ = cfg.import_legacy_regex_filters(&legacy_filters);
            }
        }
        cfg.sanitize();
        let _ = cfg.save(config_file);
        cfg
    }

    /// Atomic write via temporary file + rename so an interrupted write never corrupts the config.
    pub fn save(&self, config_file: &Path) -> Result<()> {
        if let Some(parent) = config_file.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        let tmp_file = config_file.with_extension("json.tmp");
        fs::write(&tmp_file, json)?;
        fs::rename(&tmp_file, config_file)?;
        Ok(())
    }

    pub fn import_legacy_config64(&mut self, path: &Path) -> Result<(usize, usize)> {
        let content = fs::read_to_string(path)?;
        let legacy: LegacyLunaHostConfig = serde_json::from_str(&content)?;

        if let Some(v) = legacy.auto_attach {
            self.auto_attach = v;
        }
        if let Some(v) = legacy.auto_attach_saved_only {
            self.auto_attach_saved_only = v;
        }
        if let Some(v) = legacy.to_clipboard {
            self.to_clipboard = v;
        }
        if let Some(v) = legacy.default_codepage {
            self.default_codepage = v;
        }
        if let Some(v) = legacy.filter_repetition {
            self.filter_repetition = v;
        }
        if let Some(v) = legacy.flush_delay {
            self.flush_delay = v;
        }
        if let Some(v) = legacy.max_buffer_size {
            self.max_buffer_size = v;
        }
        if let Some(v) = legacy.max_history_size {
            self.max_history_size = v;
        }

        let mut exes_lower: HashSet<String> = self
            .auto_attach_exes
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        let mut added_exes = 0;
        for exe in legacy.autoattachexes {
            let trimmed = exe.trim().to_string();
            if !trimmed.is_empty() && exes_lower.insert(trimmed.to_lowercase()) {
                self.auto_attach_exes.push(trimmed);
                added_exes += 1;
            }
        }

        let mut added_hooks = 0;
        for (exe, ctx) in legacy.savedhookcontext {
            let trimmed = exe.trim().to_string();
            self.saved_hooks.insert(trimmed.clone(), ctx);
            if exes_lower.insert(trimmed.to_lowercase()) {
                self.auto_attach_exes.push(trimmed);
            }
            added_hooks += 1;
        }

        self.auto_attach_exes
            .sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        self.sanitize();
        Ok((added_exes, added_hooks))
    }

    pub fn import_legacy_regex_filters(&mut self, path: &Path) -> Result<usize> {
        let bytes = fs::read(path)?;
        let text = if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
            let u16s: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u16s)
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let mut seen: HashSet<(Option<String>, String)> = self
            .regex_filters
            .iter()
            .map(|r| {
                (
                    r.process_path.as_ref().map(|p| p.to_lowercase()),
                    r.pattern.clone(),
                )
            })
            .collect();

        let mut added = 0;
        for line in text.lines() {
            if let Some(proc_idx) = line.find("|PROCESS|") {
                let rest = &line[proc_idx + 9..];
                if let Some(filter_idx) = rest.find("|FILTER|") {
                    let proc_str = rest[..filter_idx].trim();
                    let after_filter = &rest[filter_idx + 8..];
                    if let Some(end_idx) = after_filter.find("|END|") {
                        let pattern = after_filter[..end_idx].trim();
                        if pattern.is_empty() || pattern.contains('\u{FFFD}') {
                            continue;
                        }
                        if regex::Regex::new(pattern).is_err() {
                            continue;
                        }
                        let process_path = if proc_str.is_empty()
                            || proc_str.starts_with("Error getting name")
                        {
                            None
                        } else {
                            Some(proc_str.to_string())
                        };

                        let key = (
                            process_path.as_ref().map(|p| p.to_lowercase()),
                            pattern.to_string(),
                        );
                        if seen.insert(key) {
                            self.regex_filters.push(RegexFilterRule {
                                id: next_rule_id("filter"),
                                pattern: pattern.to_string(),
                                process_path,
                                enabled: true,
                            });
                            added += 1;
                        }
                    }
                }
            }
        }

        Ok(added)
    }

    /// Resolves the `resources/` directory, checking the executable's directory first
    /// (to prevent CWD DLL planting in installed builds), with dev fallback only in debug builds.
    pub fn resolve_resources_dir() -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let res = parent.join("resources");
                if res.join("lunahook/LunaHost64.dll").exists() {
                    return std::fs::canonicalize(&res).unwrap_or(res);
                }
            }
        }

        #[cfg(debug_assertions)]
        {
            let dev_candidates = [
                PathBuf::from("resources"),
                PathBuf::from("src-tauri/resources"),
            ];
            for c in dev_candidates {
                if c.join("lunahook/LunaHost64.dll").exists() {
                    return std::fs::canonicalize(&c).unwrap_or(c);
                }
            }
        }

        PathBuf::from("resources")
    }

    /// Registers or removes Kiroku in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
    pub fn sync_windows_autostart(&self) {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            let run_key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
            if self.start_on_boot {
                if let Ok(exe) = std::env::current_exe() {
                    let exe_str = exe.to_string_lossy();
                    let cmd_val = if self.start_minimized || self.minimize_to_tray {
                        format!("\"{}\" --minimized", exe_str)
                    } else {
                        format!("\"{}\"", exe_str)
                    };
                    let _ = std::process::Command::new("reg")
                        .args([
                            "add",
                            run_key,
                            "/v",
                            "Kiroku",
                            "/t",
                            "REG_SZ",
                            "/d",
                            &cmd_val,
                            "/f",
                        ])
                        .creation_flags(CREATE_NO_WINDOW)
                        .status();
                }
            } else {
                let _ = std::process::Command::new("reg")
                    .args(["delete", run_key, "/v", "Kiroku", "/f"])
                    .creation_flags(CREATE_NO_WINDOW)
                    .status();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lunahost::ffi::LunaHostLibrary;

    #[test]
    fn test_same_game_path_disambiguates_siglus_engine() {
        let sp = r"D:\VNs\Summer Pockets REFLECTION BLU\SiglusEngine.exe";
        let rw = r"E:\Anime\VNs\Key\Rewrite_PLUS\SiglusEngine.exe";
        let sp_moved = r"F:\Games\Summer Pockets REFLECTION BLU\SiglusEngine.exe";

        assert!(!same_game_path(sp, rw), "Different games sharing SiglusEngine.exe must NOT match");
        assert!(same_game_path(sp, sp_moved), "Same game folder on a different drive should match");
    }

    #[test]
    fn test_defaults_import_and_lunahost_dll_load() {
        let res_dir = AppConfig::resolve_resources_dir();
        let dll_path = res_dir.join("lunahook/LunaHost64.dll");
        assert!(dll_path.exists(), "LunaHost64.dll must exist in resources");

        let lib = LunaHostLibrary::load(&dll_path).expect("Failed to load LunaHost64.dll via FFI");
        lib.apply_settings(100, 932, 3000, 10_000_000, true);

        let defaults_dir = res_dir.join("defaults");
        let mut cfg = AppConfig::default();
        let (added_exes, added_hooks) = cfg
            .import_legacy_config64(&defaults_dir.join("config64.json"))
            .expect("Failed to import config64.json");
        assert!(added_exes >= 30, "Expected at least 30 auto-attach exes");
        assert!(added_hooks >= 10, "Expected at least 10 saved hook contexts");

        let added_filters = cfg
            .import_legacy_regex_filters(&defaults_dir.join("SavedRegexFilters.txt"))
            .expect("Failed to import SavedRegexFilters.txt");
        assert!(added_filters >= 1, "Expected imported regex filters");
    }
}
