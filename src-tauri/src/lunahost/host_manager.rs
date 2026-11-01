use crate::lunahost::ffi::{
    LunaHostLibrary, ThreadParam, cstr_ptr_to_string, wide_ptr_to_string,
};
use crate::lunahost::injector::inject_lunahook_dll;
use crate::pipeline::{
    cleaner::clean_sentence,
    scorer::{is_high_quality_dialogue_candidate, score_thread_text},
};
use crate::process::window_list::{
    get_icon_cached, list_windowed_processes_fast, query_process_info, write_luna_game_bridge_json,
};
use crate::store::config::{AppConfig, SavedHookContext, same_game_path};
use crate::websocket::{
    exstatic_server::{ExStaticBroadcastServer, ExStaticSentencePayload},
    knife_ingest::KnifeIngestClient,
};
use anyhow::{Context, Result, bail};
use log::{info, warn};
use once_cell::sync::Lazy;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

static HOST_EVENT_TX: Lazy<RwLock<Option<mpsc::UnboundedSender<HostEvent>>>> =
    Lazy::new(|| RwLock::new(None));

fn send_host_event(ev: HostEvent) {
    if let Some(tx) = HOST_EVENT_TX.read().as_ref() {
        let _ = tx.send(ev);
    }
}

#[derive(Debug, Clone)]
pub enum HostEvent {
    ProcessConnected(u32),
    ProcessDisconnected(u32),
    ThreadCreated {
        hookcode: String,
        hookname: String,
        tp: ThreadParam,
        is_embeddable: bool,
    },
    ThreadDestroyed {
        tp: ThreadParam,
    },
    TextOutput {
        hookcode: String,
        hookname: String,
        tp: ThreadParam,
        text: String,
    },
    HostInfo {
        info_type: i32,
        log: String,
    },
    KnifeSentence(String),
}

unsafe extern "C" fn cb_process_connect(pid: u32) {
    send_host_event(HostEvent::ProcessConnected(pid));
}

unsafe extern "C" fn cb_process_disconnect(pid: u32) {
    send_host_event(HostEvent::ProcessDisconnected(pid));
}

unsafe extern "C" fn cb_thread_create(
    hookcode: *const u16,
    hookname: *const i8,
    tp: ThreadParam,
    is_embeddable: bool,
) {
    let hc = unsafe { wide_ptr_to_string(hookcode) };
    let hn = unsafe { cstr_ptr_to_string(hookname) };
    send_host_event(HostEvent::ThreadCreated {
        hookcode: hc,
        hookname: hn,
        tp,
        is_embeddable,
    });
}

unsafe extern "C" fn cb_thread_destroy(
    _hookcode: *const u16,
    _hookname: *const i8,
    tp: ThreadParam,
) {
    send_host_event(HostEvent::ThreadDestroyed { tp });
}

unsafe extern "C" fn cb_output(
    hookcode: *const u16,
    hookname: *const i8,
    tp: ThreadParam,
    text: *const u16,
) {
    let hc = unsafe { wide_ptr_to_string(hookcode) };
    let hn = unsafe { cstr_ptr_to_string(hookname) };
    let txt = unsafe { wide_ptr_to_string(text) };
    send_host_event(HostEvent::TextOutput {
        hookcode: hc,
        hookname: hn,
        tp,
        text: txt,
    });
}

unsafe extern "C" fn cb_host_info(info_type: i32, log: *const u16) {
    let msg = unsafe { wide_ptr_to_string(log) };
    send_host_event(HostEvent::HostInfo {
        info_type,
        log: msg,
    });
}

unsafe extern "C" fn cb_hook_insert(_pid: u32, _address: u64, _hookcode: *const u16) {}
unsafe extern "C" fn cb_embed(_text: *const u16, _tp: ThreadParam) {}
unsafe extern "C" fn cb_i18n(_text: *const u16) -> *mut u16 {
    std::ptr::null_mut()
}
unsafe extern "C" fn cb_emu_info(_id: *const u16, _title: *const u16, _version: *const u16) {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachedProcess {
    pub pid: u32,
    pub name: String,
    pub exe_path: String,
    pub is_64bit: bool,
    pub is_knife: bool,
    pub icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentenceEntry {
    pub id: u64,
    pub timestamp: String,
    pub raw: String,
    pub cleaned: String,
    pub char_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookThreadView {
    pub key: String,
    pub tp: ThreadParam,
    pub hookcode: String,
    pub name: String,
    pub is_embeddable: bool,
    pub is_saved_match: bool,
    pub is_knife: bool,
    pub score: u8,
    pub latest_raw: String,
    pub latest_cleaned: String,
    pub total_sentences: u64,
    pub total_chars: u64,
    pub history: Vec<SentenceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSnapshot {
    pub attached_processes: Vec<AttachedProcess>,
    pub threads: Vec<HookThreadView>,
    pub selected_thread_key: Option<String>,
    pub ws_port: u16,
    pub ws_clients: usize,
    pub knife_connected: bool,
    pub knife_url: String,
    pub console_logs: Vec<String>,
    pub total_session_chars: u64,
    pub total_session_sentences: u64,
}

pub struct HostManager {
    app_handle: AppHandle,
    resources_dir: PathBuf,
    config_path: PathBuf,
    pub config: RwLock<AppConfig>,
    lib: Option<LunaHostLibrary>,
    pub ws_server: Arc<ExStaticBroadcastServer>,
    pub knife_client: Arc<KnifeIngestClient>,
    attached: RwLock<HashMap<u32, AttachedProcess>>,
    user_detached_pids: RwLock<HashSet<u32>>,
    threads: RwLock<HashMap<String, HookThreadView>>,
    selected_thread_key: RwLock<Option<String>>,
    console_logs: RwLock<Vec<String>>,
    knife_pid_cache: Mutex<Option<(Instant, u32, String)>>,
    sentence_counter: std::sync::atomic::AtomicU64,
    session_chars: std::sync::atomic::AtomicU64,
    session_sentences: std::sync::atomic::AtomicU64,
    manual_or_saved_locked: std::sync::atomic::AtomicBool,
}

impl HostManager {
    pub fn init(app_handle: AppHandle, config_path: PathBuf) -> Result<Arc<Self>> {
        let resources_dir = AppConfig::resolve_resources_dir();
        let defaults_dir = resources_dir.join("defaults");
        let config = AppConfig::load_or_init(
            &config_path,
            if defaults_dir.exists() {
                Some(defaults_dir.as_path())
            } else {
                None
            },
        );
        if config.start_on_boot {
            config.sync_windows_autostart();
        }

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        *HOST_EVENT_TX.write() = Some(event_tx.clone());

        let ws_server = ExStaticBroadcastServer::new(config.ws_broadcast_port);

        let (knife_tx, mut knife_rx) = mpsc::unbounded_channel::<String>();
        let knife_client = KnifeIngestClient::new(
            config.knife_ws_enabled,
            config.knife_ws_url.clone(),
            config.ws_broadcast_port,
            knife_tx,
        );

        let tx_knife = event_tx.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(line) = knife_rx.recv().await {
                let _ = tx_knife.send(HostEvent::KnifeSentence(line));
            }
        });

        let dll_path = resources_dir.join("lunahook/LunaHost64.dll");
        let lib = match LunaHostLibrary::load(&dll_path) {
            Ok(l) => {
                l.start(
                    cb_process_connect,
                    cb_process_disconnect,
                    cb_thread_create,
                    cb_thread_destroy,
                    cb_output,
                    cb_host_info,
                    cb_hook_insert,
                    cb_embed,
                    cb_i18n,
                    cb_emu_info,
                );
                l.apply_settings(
                    config.flush_delay,
                    config.default_codepage,
                    config.max_buffer_size,
                    config.max_history_size,
                    config.enable_pc_hooks,
                );
                info!("Loaded LunaHost64.dll from {}", dll_path.display());
                Some(l)
            }
            Err(e) => {
                warn!("Could not load LunaHost64.dll: {:?}", e);
                None
            }
        };

        let manager = Arc::new(Self {
            app_handle,
            resources_dir,
            config_path,
            config: RwLock::new(config),
            lib,
            ws_server,
            knife_client,
            attached: RwLock::new(HashMap::new()),
            user_detached_pids: RwLock::new(HashSet::new()),
            threads: RwLock::new(HashMap::new()),
            selected_thread_key: RwLock::new(None),
            console_logs: RwLock::new(vec!["Kiroku initialized.".to_string()]),
            knife_pid_cache: Mutex::new(None),
            sentence_counter: std::sync::atomic::AtomicU64::new(1),
            session_chars: std::sync::atomic::AtomicU64::new(0),
            session_sentences: std::sync::atomic::AtomicU64::new(0),
            manual_or_saved_locked: std::sync::atomic::AtomicBool::new(false),
        });

        manager.spawn_event_loop(event_rx);
        manager.spawn_auto_attach_watcher();

        Ok(manager)
    }

    pub fn snapshot(&self) -> DashboardSnapshot {
        let hide_garbage = self.config.read().hide_garbage_threads;
        let selected = self.selected_thread_key.read().clone();

        let mut threads: Vec<HookThreadView> = self
            .threads
            .read()
            .values()
            .filter(|t| {
                if Some(&t.key) == selected.as_ref() || t.is_saved_match || t.is_knife {
                    return true;
                }
                if hide_garbage && t.total_sentences > 0 && t.score < 20 {
                    return false;
                }
                true
            })
            .map(|t| {
                // Only clone full sentence history for the currently selected thread to keep IPC lightweight
                let is_sel = Some(&t.key) == selected.as_ref();
                HookThreadView {
                    key: t.key.clone(),
                    tp: t.tp,
                    hookcode: t.hookcode.clone(),
                    name: t.name.clone(),
                    is_embeddable: t.is_embeddable,
                    is_saved_match: t.is_saved_match,
                    is_knife: t.is_knife,
                    score: t.score,
                    latest_raw: t.latest_raw.clone(),
                    latest_cleaned: t.latest_cleaned.clone(),
                    total_sentences: t.total_sentences,
                    total_chars: t.total_chars,
                    history: if is_sel {
                        t.history.clone()
                    } else {
                        Vec::new()
                    },
                }
            })
            .collect();

        // Sort threads:
        // 1. Saved hook match or currently selected
        // 2. Knife WebSocket thread
        // 3. Score descending, then total_sentences descending
        threads.sort_by(|a, b| {
            let a_sel = Some(&a.key) == selected.as_ref();
            let b_sel = Some(&b.key) == selected.as_ref();
            b.is_saved_match
                .cmp(&a.is_saved_match)
                .then_with(|| b_sel.cmp(&a_sel))
                .then_with(|| b.is_knife.cmp(&a.is_knife))
                .then_with(|| b.score.cmp(&a.score))
                .then_with(|| b.total_sentences.cmp(&a.total_sentences))
                .then_with(|| a.key.cmp(&b.key))
        });

        let mut attached_processes: Vec<AttachedProcess> =
            self.attached.read().values().cloned().collect();
        attached_processes.sort_by_key(|p| p.pid);

        DashboardSnapshot {
            attached_processes,
            threads,
            selected_thread_key: selected,
            ws_port: self.ws_server.port(),
            ws_clients: self.ws_server.client_count(),
            knife_connected: self.knife_client.is_connected(),
            knife_url: self.knife_client.current_url(),
            console_logs: self.console_logs.read().clone(),
            total_session_chars: self
                .session_chars
                .load(std::sync::atomic::Ordering::Relaxed),
            total_session_sentences: self
                .session_sentences
                .load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    pub fn push_log(&self, msg: impl Into<String>) {
        let mut logs = self.console_logs.write();
        logs.push(msg.into());
        if logs.len() > 150 {
            let excess = logs.len() - 150;
            logs.drain(0..excess);
        }
    }

    pub fn emit_state(&self) {
        let _ = self.app_handle.emit("dashboard_updated", self.snapshot());
    }

    pub fn attach_process(&self, pid: u32) -> Result<()> {
        self.user_detached_pids.write().remove(&pid);

        let (exe_path, is_64bit) =
            query_process_info(pid).context("Could not query process path for PID")?;
        let name = Path::new(&exe_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Game.exe")
            .to_string();
        let is_knife = name.eq_ignore_ascii_case("knife.exe");
        let icon_data_url = get_icon_cached(&exe_path);

        if is_knife {
            self.attached.write().insert(
                pid,
                AttachedProcess {
                    pid,
                    name: name.clone(),
                    exe_path: exe_path.clone(),
                    is_64bit,
                    is_knife: true,
                    icon_data_url,
                },
            );
            let mut cfg = self.config.write();
            if !cfg
                .auto_attach_exes
                .iter()
                .any(|e| same_game_path(e, &exe_path))
            {
                cfg.auto_attach_exes.push(exe_path.clone());
            }
            cfg.knife_ws_enabled = true;
            let url_to_use = if cfg.knife_auto_read_ini {
                KnifeIngestClient::detect_ws_url_from_knife_exe(&exe_path)
                    .unwrap_or_else(|| cfg.knife_ws_url.clone())
            } else {
                cfg.knife_ws_url.clone()
            };
            cfg.knife_ws_url = url_to_use.clone();
            let _ = cfg.save(&self.config_path);
            drop(cfg);

            self.knife_client.update_config(true, &url_to_use);
            self.push_log(format!(
                "Attached to knife.exe (PID {}) — listening via {}",
                pid, url_to_use
            ));
            self.ensure_knife_thread_created(pid);
            self.sync_game_bridge_file();
            self.emit_state();
            return Ok(());
        }

        let Some(lib) = &self.lib else {
            bail!("LunaHost64.dll is not loaded; cannot attach to native game process.");
        };

        // Connect pipe and inject BEFORE adding to `attached` so failed injection leaves no ghost state
        lib.connect_process(pid);
        if lib.check_if_need_inject(pid) {
            let lunahook_dir = self.resources_dir.join("lunahook");
            if let Err(e) = inject_lunahook_dll(&lunahook_dir, pid, is_64bit) {
                lib.detach_process(pid);
                return Err(e);
            }
        }

        self.attached.write().insert(
            pid,
            AttachedProcess {
                pid,
                name: name.clone(),
                exe_path: exe_path.clone(),
                is_64bit,
                is_knife: false,
                icon_data_url,
            },
        );

        // Save in auto_attach_exes list and apply per-game settings if any
        {
            let mut cfg = self.config.write();
            if !cfg
                .auto_attach_exes
                .iter()
                .any(|e| same_game_path(e, &exe_path))
            {
                cfg.auto_attach_exes.push(exe_path.clone());
                let _ = cfg.save(&self.config_path);
            }

            let (fd, cp) =
                if let Some(saved) = Self::find_saved_hook_in_map(&cfg.saved_hooks, &exe_path) {
                    (
                        saved.custom_flush_delay.unwrap_or(cfg.flush_delay),
                        saved.custom_codepage.unwrap_or(cfg.default_codepage),
                    )
                } else {
                    (cfg.flush_delay, cfg.default_codepage)
                };
            lib.apply_settings(
                fd,
                cp,
                cfg.max_buffer_size,
                cfg.max_history_size,
                cfg.enable_pc_hooks,
            );
        }

        self.push_log(format!(
            "Attached to {} (PID {}, {})",
            name,
            pid,
            if is_64bit { "x64" } else { "x86" }
        ));
        self.manual_or_saved_locked
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.sync_game_bridge_file();
        self.emit_state();
        Ok(())
    }

    pub fn detach_process(&self, pid: u32) {
        self.user_detached_pids.write().insert(pid);
        let removed_proc = self.attached.write().remove(&pid);
        let was_knife = removed_proc.as_ref().map(|p| p.is_knife).unwrap_or(false);

        if let Some(proc_info) = &removed_proc {
            if !proc_info.is_knife {
                if let Some(lib) = &self.lib {
                    lib.detach_process(pid);
                }
            }
            self.push_log(format!("Detached from {} (PID {})", proc_info.name, pid));
        }

        let mut threads = self.threads.write();
        threads.retain(|_, t| {
            if was_knife && t.is_knife {
                return false;
            }
            t.tp.process_id != pid
        });
        let sel_still_exists = self
            .selected_thread_key
            .read()
            .as_ref()
            .map(|k| threads.contains_key(k))
            .unwrap_or(false);
        drop(threads);

        if !sel_still_exists {
            *self.selected_thread_key.write() = None;
            self.manual_or_saved_locked
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }

        // Restore global Luna_Settings if no processes remain
        if self.attached.read().is_empty() {
            if let Some(lib) = &self.lib {
                let cfg = self.config.read();
                lib.apply_settings(
                    cfg.flush_delay,
                    cfg.default_codepage,
                    cfg.max_buffer_size,
                    cfg.max_history_size,
                    cfg.enable_pc_hooks,
                );
            }
        }

        self.sync_game_bridge_file();
        self.emit_state();
    }

    pub fn detach_all(&self) {
        let pids: Vec<u32> = self.attached.read().keys().copied().collect();
        for pid in pids {
            self.detach_process(pid);
        }
        self.threads.write().clear();
        *self.selected_thread_key.write() = None;
        self.sync_game_bridge_file();
        self.emit_state();
    }

    fn sync_game_bridge_file(&self) {
        let attached = self.attached.read();
        if let Some(proc_info) = attached
            .values()
            .find(|p| !p.is_knife)
            .or_else(|| attached.values().next())
        {
            write_luna_game_bridge_json(proc_info.pid, &proc_info.name, "");
        } else {
            write_luna_game_bridge_json(0, "", "");
        }
    }

    pub fn select_thread(&self, key: Option<String>, remember_for_exe: bool) {
        let mut sel_guard = self.selected_thread_key.write();
        let mut threads = self.threads.write();

        // Validate key exists if Some
        if let Some(ref k) = key {
            if !threads.contains_key(k) {
                return;
            }
        }

        if let Some(prev_key) = sel_guard.as_ref() {
            if Some(prev_key) != key.as_ref() {
                if let Some(prev_thread) = threads.get(prev_key) {
                    if !prev_thread.is_knife {
                        if let Some(lib) = &self.lib {
                            lib.sync_thread(prev_thread.tp, false);
                        }
                    }
                }
            }
        }

        *sel_guard = key.clone();
        drop(sel_guard);

        if let Some(k) = key {
            if let Some(info) = threads
                .get(&k)
                .map(|t| (t.tp, t.hookcode.clone(), t.name.clone(), t.is_knife))
            {
                let (tp, hookcode, name, is_knife) = info;
                if !is_knife {
                    if let Some(lib) = &self.lib {
                        lib.sync_thread(tp, true);
                    }
                }
                if remember_for_exe && !is_knife {
                    self.manual_or_saved_locked
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    if let Some(proc_info) = self.attached.read().get(&tp.process_id) {
                        let exe_path = proc_info.exe_path.clone();
                        let mut cfg = self.config.write();
                        let existing_overrides = cfg
                            .saved_hooks
                            .get(&exe_path)
                            .map(|s| (s.custom_flush_delay, s.custom_codepage))
                            .unwrap_or((None, None));
                        cfg.saved_hooks.insert(
                            exe_path.clone(),
                            SavedHookContext {
                                hookcode,
                                ctx1: tp.ctx,
                                ctx2: tp.ctx2,
                                name,
                                custom_flush_delay: existing_overrides.0,
                                custom_codepage: existing_overrides.1,
                            },
                        );
                        let _ = cfg.save(&self.config_path);
                        for t in threads.values_mut() {
                            if t.tp.process_id == tp.process_id {
                                t.is_saved_match = false;
                            }
                        }
                        if let Some(t) = threads.get_mut(&k) {
                            t.is_saved_match = true;
                        }
                    }
                }
            }
        }
        drop(threads);

        self.emit_state();
    }

    pub fn insert_hook_code(&self, hookcode: &str) -> Result<()> {
        let hc = hookcode.trim();
        if hc.is_empty() {
            return Ok(());
        }
        let pids: Vec<u32> = self
            .attached
            .read()
            .values()
            .filter(|p| !p.is_knife)
            .map(|p| p.pid)
            .collect();
        if let Some(lib) = &self.lib {
            for pid in &pids {
                lib.insert_hook_code(*pid, hc);
            }
        }
        self.push_log(format!("Inserted user hook code: {}", hc));
        self.emit_state();
        Ok(())
    }

    pub fn insert_pc_hooks(&self, which: i32) {
        let pids: Vec<u32> = self
            .attached
            .read()
            .values()
            .filter(|p| !p.is_knife)
            .map(|p| p.pid)
            .collect();
        if let Some(lib) = &self.lib {
            for pid in pids {
                lib.insert_pc_hooks(pid, which);
            }
        }
        self.push_log(format!("Inserted PC hooks (mode {})", which));
        self.emit_state();
    }

    pub fn remove_hook(&self, pid: u32, addr: u64) {
        if let Some(lib) = &self.lib {
            lib.remove_hook(pid, addr);
        }
        let mut threads = self.threads.write();
        threads.retain(|_, t| !(t.tp.process_id == pid && t.tp.addr == addr));
        let sel_exists = self
            .selected_thread_key
            .read()
            .as_ref()
            .map(|k| threads.contains_key(k))
            .unwrap_or(false);
        drop(threads);
        if !sel_exists {
            *self.selected_thread_key.write() = None;
        }
        self.emit_state();
    }

    pub fn update_config(&self, mut new_cfg: AppConfig) -> Result<()> {
        new_cfg.sanitize();
        if let Some(lib) = &self.lib {
            lib.apply_settings(
                new_cfg.flush_delay,
                new_cfg.default_codepage,
                new_cfg.max_buffer_size,
                new_cfg.max_history_size,
                new_cfg.enable_pc_hooks,
            );
        }
        self.ws_server.set_port(new_cfg.ws_broadcast_port);
        self.knife_client
            .set_forbidden_port(new_cfg.ws_broadcast_port);
        self.knife_client
            .update_config(new_cfg.knife_ws_enabled, &new_cfg.knife_ws_url);
        new_cfg.sync_windows_autostart();
        new_cfg.save(&self.config_path)?;
        *self.config.write() = new_cfg;
        self.emit_state();
        Ok(())
    }

    fn find_saved_hook_in_map<'a>(
        saved_hooks: &'a HashMap<String, SavedHookContext>,
        exe_path: &str,
    ) -> Option<&'a SavedHookContext> {
        if let Some(exact) = saved_hooks.get(exe_path) {
            return Some(exact);
        }
        // Deterministic fallback using `same_game_path` (matches parent_folder + exe_name, never bare SiglusEngine.exe)
        let mut matches: Vec<(&String, &SavedHookContext)> = saved_hooks
            .iter()
            .filter(|(k, _)| same_game_path(k, exe_path))
            .collect();
        matches.sort_by(|(ka, _), (kb, _)| ka.cmp(kb));
        matches.first().map(|(_, v)| *v)
    }

    fn ensure_knife_thread_created(&self, pid: u32) {
        let key = "knife:ws".to_string();
        let mut threads = self.threads.write();
        threads.entry(key.clone()).or_insert_with(|| HookThreadView {
            key: key.clone(),
            tp: ThreadParam {
                process_id: pid,
                addr: 0,
                ctx: 0,
                ctx2: 0,
            },
            hookcode: "WS@knife".to_string(),
            name: "Knife WebSocket".to_string(),
            is_embeddable: false,
            is_saved_match: true,
            is_knife: true,
            score: 100,
            latest_raw: String::new(),
            latest_cleaned: String::new(),
            total_sentences: 0,
            total_chars: 0,
            history: Vec::new(),
        });
        drop(threads);
        if self.selected_thread_key.read().is_none() {
            self.select_thread(Some(key), false);
        }
    }

    fn spawn_event_loop(self: &Arc<Self>, mut rx: mpsc::UnboundedReceiver<HostEvent>) {
        let mgr = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            while let Some(ev) = rx.recv().await {
                mgr.handle_host_event(ev);
            }
        });
    }

    fn handle_host_event(&self, ev: HostEvent) {
        match ev {
            HostEvent::ProcessConnected(pid) => {
                if !self.attached.read().contains_key(&pid) {
                    if let Some((exe_path, is_64bit)) = query_process_info(pid) {
                        let name = Path::new(&exe_path)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Game.exe")
                            .to_string();
                        let icon_data_url = get_icon_cached(&exe_path);
                        self.attached.write().insert(
                            pid,
                            AttachedProcess {
                                pid,
                                name,
                                exe_path: exe_path.clone(),
                                is_64bit,
                                is_knife: false,
                                icon_data_url,
                            },
                        );
                    }
                }
                // Check if saved hook is a UserHook and auto-insert it
                if let Some(proc_info) = self.attached.read().get(&pid).cloned() {
                    let cfg = self.config.read();
                    if let Some(saved) =
                        Self::find_saved_hook_in_map(&cfg.saved_hooks, &proc_info.exe_path)
                    {
                        if saved.name.starts_with("UserHook")
                            || saved.hookcode.starts_with("/H")
                            || saved.hookcode.starts_with('H')
                        {
                            if let Some(lib) = &self.lib {
                                lib.insert_hook_code(pid, &saved.hookcode);
                            }
                        }
                    }
                }
                self.sync_game_bridge_file();
                self.emit_state();
            }
            HostEvent::ProcessDisconnected(pid) => {
                self.attached.write().remove(&pid);
                let mut threads = self.threads.write();
                threads.retain(|_, t| t.tp.process_id != pid);
                let sel_exists = self
                    .selected_thread_key
                    .read()
                    .as_ref()
                    .map(|k| threads.contains_key(k))
                    .unwrap_or(false);
                drop(threads);
                if !sel_exists {
                    *self.selected_thread_key.write() = None;
                }
                self.push_log(format!("Process {} disconnected.", pid));
                self.sync_game_bridge_file();
                self.emit_state();
            }
            HostEvent::ThreadCreated {
                hookcode,
                hookname,
                tp,
                is_embeddable,
            } => {
                if tp.process_id == 0 {
                    return;
                }
                let key = tp.thread_key();
                let is_saved = self.check_matches_saved(&tp, &hookcode);
                let mut threads = self.threads.write();
                threads.entry(key.clone()).or_insert_with(|| HookThreadView {
                    key: key.clone(),
                    tp,
                    hookcode,
                    name: hookname,
                    is_embeddable,
                    is_saved_match: is_saved,
                    is_knife: false,
                    score: if is_saved { 100 } else { 50 },
                    latest_raw: String::new(),
                    latest_cleaned: String::new(),
                    total_sentences: 0,
                    total_chars: 0,
                    history: Vec::new(),
                });
                drop(threads);

                if is_saved {
                    self.select_thread(Some(key), false);
                } else {
                    self.emit_state();
                }
            }
            HostEvent::ThreadDestroyed { tp } => {
                let key = tp.thread_key();
                self.threads.write().remove(&key);
                if self.selected_thread_key.read().as_ref() == Some(&key) {
                    *self.selected_thread_key.write() = None;
                }
                self.emit_state();
            }
            HostEvent::TextOutput {
                hookcode,
                hookname,
                tp,
                text,
            } => {
                if tp.process_id == 0 || text.trim().is_empty() {
                    return;
                }
                let key = tp.thread_key();
                let proc_path = self
                    .attached
                    .read()
                    .get(&tp.process_id)
                    .map(|p| p.exe_path.clone());
                let cfg = self.config.read().clone();

                let cleaned = clean_sentence(&text, proc_path.as_deref(), &cfg);
                if cleaned.is_empty() {
                    return;
                }
                let score = score_thread_text(&cleaned);
                let is_saved = self.check_matches_saved(&tp, &hookcode);

                let mut should_auto_select = false;
                {
                    let mut threads = self.threads.write();
                    let entry = threads.entry(key.clone()).or_insert_with(|| HookThreadView {
                        key: key.clone(),
                        tp,
                        hookcode: hookcode.clone(),
                        name: hookname.clone(),
                        is_embeddable: false,
                        is_saved_match: is_saved,
                        is_knife: false,
                        score,
                        latest_raw: String::new(),
                        latest_cleaned: String::new(),
                        total_sentences: 0,
                        total_chars: 0,
                        history: Vec::new(),
                    });

                    if is_saved && !entry.is_saved_match {
                        entry.is_saved_match = true;
                    }
                    // Smooth rolling score so single short lines don't swing thread ranking
                    entry.score = if entry.is_saved_match {
                        100
                    } else if entry.total_sentences == 0 {
                        score
                    } else {
                        ((entry.score as u16 * 2 + score as u16) / 3) as u8
                    };
                    entry.latest_raw = text.clone();
                    entry.latest_cleaned = cleaned.clone();
                    entry.total_sentences += 1;
                    let char_len = cleaned.chars().filter(|c| !c.is_whitespace()).count();
                    entry.total_chars += char_len as u64;

                    let id = self
                        .sentence_counter
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
                    entry.history.push(SentenceEntry {
                        id,
                        timestamp,
                        raw: text,
                        cleaned: cleaned.clone(),
                        char_count: char_len,
                    });
                    if entry.history.len() > 80 {
                        let excess = entry.history.len() - 80;
                        entry.history.drain(0..excess);
                    }

                    if is_saved {
                        self.manual_or_saved_locked
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        if self.selected_thread_key.read().as_ref() != Some(&key) {
                            should_auto_select = true;
                        }
                    } else if cfg.auto_select_best_hook
                        && !self
                            .manual_or_saved_locked
                            .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        let entry_score = entry.score;
                        if is_high_quality_dialogue_candidate(&cleaned, entry_score) {
                            let cur_sel = self.selected_thread_key.read().clone();
                            match cur_sel {
                                None => {
                                    should_auto_select = true;
                                }
                                Some(ref cur_key) if cur_key != &key => {
                                    let cur_score =
                                        threads.get(cur_key).map(|t| t.score).unwrap_or(0);
                                    if entry_score >= cur_score.saturating_add(6) {
                                        should_auto_select = true;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }

                if should_auto_select {
                    self.select_thread(Some(key.clone()), false);
                }

                let is_selected = self.selected_thread_key.read().as_ref() == Some(&key);
                if is_selected {
                    self.dispatch_selected_sentence(
                        tp.process_id,
                        proc_path.unwrap_or_default(),
                        &cleaned,
                        &cfg,
                    );
                }

                self.emit_state();
            }
            HostEvent::HostInfo { info_type: _, log } => {
                if !log.trim().is_empty() {
                    self.push_log(log);
                    self.emit_state();
                }
            }
            HostEvent::KnifeSentence(raw_text) => {
                let (pid, proc_path) = self.resolve_knife_process();
                if pid != 0 && self.user_detached_pids.read().contains(&pid) {
                    return;
                }
                self.ensure_knife_thread_created(pid);
                let cfg = self.config.read().clone();
                let cleaned = clean_sentence(&raw_text, Some(&proc_path), &cfg);
                if cleaned.is_empty() {
                    return;
                }

                let key = "knife:ws".to_string();
                let char_len = cleaned.chars().filter(|c| !c.is_whitespace()).count();
                {
                    let mut threads = self.threads.write();
                    if let Some(entry) = threads.get_mut(&key) {
                        entry.tp.process_id = pid;
                        entry.latest_raw = raw_text.clone();
                        entry.latest_cleaned = cleaned.clone();
                        entry.total_sentences += 1;
                        entry.total_chars += char_len as u64;
                        let id = self
                            .sentence_counter
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
                        entry.history.push(SentenceEntry {
                            id,
                            timestamp,
                            raw: raw_text,
                            cleaned: cleaned.clone(),
                            char_count: char_len,
                        });
                        if entry.history.len() > 80 {
                            let excess = entry.history.len() - 80;
                            entry.history.drain(0..excess);
                        }
                    }
                }

                if self.selected_thread_key.read().as_ref() == Some(&key) {
                    self.dispatch_selected_sentence(pid, proc_path, &cleaned, &cfg);
                }
                self.emit_state();
            }
        }
    }

    fn dispatch_selected_sentence(
        &self,
        process_id: u32,
        process_path: String,
        sentence: &str,
        cfg: &AppConfig,
    ) {
        let char_count = sentence.chars().filter(|c| !c.is_whitespace()).count() as u64;
        self.session_chars
            .fetch_add(char_count, std::sync::atomic::Ordering::Relaxed);
        self.session_sentences
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        if cfg.ws_broadcast_enabled {
            let payload = ExStaticSentencePayload {
                process_id,
                process_path,
                sentence: sentence.to_string(),
            };
            self.ws_server.broadcast_sentence(&payload);
        }

        if cfg.to_clipboard {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                let _ = cb.set_text(sentence.to_string());
            }
        }
    }

    fn check_matches_saved(&self, tp: &ThreadParam, hookcode: &str) -> bool {
        let Some(proc_info) = self.attached.read().get(&tp.process_id).cloned() else {
            return false;
        };
        let cfg = self.config.read();
        let Some(saved) = Self::find_saved_hook_in_map(&cfg.saved_hooks, &proc_info.exe_path)
        else {
            return false;
        };
        ((saved.ctx1 & 0xffff) == (tp.ctx & 0xffff))
            && (saved.ctx2 == tp.ctx2)
            && saved.hookcode.eq_ignore_ascii_case(hookcode)
    }

    fn resolve_knife_process(&self) -> (u32, String) {
        if let Some(p) = self.attached.read().values().find(|p| p.is_knife) {
            return (p.pid, p.exe_path.clone());
        }
        let mut cache = self.knife_pid_cache.lock();
        if let Some((ts, pid, ref path)) = *cache {
            if ts.elapsed() < Duration::from_secs(3) {
                return (pid, path.clone());
            }
        }
        for p in list_windowed_processes_fast(&[]) {
            if p.is_knife {
                *cache = Some((Instant::now(), p.pid, p.exe_path.clone()));
                return (p.pid, p.exe_path);
            }
        }
        *cache = Some((Instant::now(), 0, "knife.exe".to_string()));
        (0, "knife.exe".to_string())
    }

    fn spawn_auto_attach_watcher(self: &Arc<Self>) {
        let mgr = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let cfg = mgr.config.read().clone();
                if !cfg.auto_attach || !mgr.attached.read().is_empty() {
                    continue;
                }
                let procs = list_windowed_processes_fast(&cfg.auto_attach_exes);
                let live_pids: HashSet<u32> = procs.iter().map(|p| p.pid).collect();
                mgr.user_detached_pids
                    .write()
                    .retain(|pid| live_pids.contains(pid));

                for proc_entry in procs {
                    if mgr.user_detached_pids.read().contains(&proc_entry.pid) {
                        continue;
                    }
                    if proc_entry.is_knife && cfg.knife_ws_enabled {
                        let _ = mgr.attach_process(proc_entry.pid);
                        break;
                    }
                    if !proc_entry.is_saved {
                        continue;
                    }
                    if cfg.auto_attach_saved_only
                        && Self::find_saved_hook_in_map(&cfg.saved_hooks, &proc_entry.exe_path)
                            .is_none()
                    {
                        continue;
                    }
                    let _ = mgr.attach_process(proc_entry.pid);
                    break;
                }
            }
        });
    }
}
