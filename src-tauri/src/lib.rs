pub mod backlog;
pub mod lunahost;
pub mod pipeline;
pub mod process;
pub mod store;
pub mod websocket;

use crate::lunahost::host_manager::{DashboardSnapshot, HostManager};
use crate::pipeline::cleaner::clean_sentence;
use crate::process::window_list::{ProcessEntry, list_windowed_processes};
use crate::store::config::AppConfig;
use crate::websocket::exstatic_server::ExStaticSentencePayload;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    AppHandle, Manager, State, WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

static TRAY_READY: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn get_dashboard_snapshot(mgr: State<'_, Arc<HostManager>>) -> DashboardSnapshot {
    mgr.snapshot()
}

#[tauri::command]
fn get_config(mgr: State<'_, Arc<HostManager>>) -> AppConfig {
    mgr.config.read().clone()
}

#[tauri::command]
fn save_config(mgr: State<'_, Arc<HostManager>>, config: AppConfig) -> Result<AppConfig, String> {
    mgr.update_config(config).map_err(|e| e.to_string())?;
    Ok(mgr.config.read().clone())
}

#[tauri::command]
async fn list_processes(mgr: State<'_, Arc<HostManager>>) -> Result<Vec<ProcessEntry>, String> {
    let saved = mgr.config.read().combined_auto_attach_exes();
    tokio::task::spawn_blocking(move || list_windowed_processes(&saved))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn attach_process(
    mgr: State<'_, Arc<HostManager>>,
    pid: u32,
) -> Result<DashboardSnapshot, String> {
    let mgr_clone = Arc::clone(&mgr);
    tokio::task::spawn_blocking(move || {
        mgr_clone.attach_process(pid).map_err(|e| e.to_string())?;
        Ok(mgr_clone.snapshot())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn detach_process(mgr: State<'_, Arc<HostManager>>, pid: u32) -> DashboardSnapshot {
    mgr.detach_process(pid);
    mgr.snapshot()
}

#[tauri::command]
fn detach_all(mgr: State<'_, Arc<HostManager>>) -> DashboardSnapshot {
    mgr.detach_all();
    mgr.snapshot()
}

#[tauri::command]
fn select_thread(
    mgr: State<'_, Arc<HostManager>>,
    key: Option<String>,
    remember: bool,
) -> DashboardSnapshot {
    mgr.select_thread(key, remember);
    mgr.snapshot()
}

#[tauri::command]
fn insert_hook_code(
    mgr: State<'_, Arc<HostManager>>,
    hookcode: String,
) -> Result<DashboardSnapshot, String> {
    mgr.insert_hook_code(&hookcode)
        .map_err(|e| e.to_string())?;
    Ok(mgr.snapshot())
}

#[tauri::command]
fn insert_pc_hooks(mgr: State<'_, Arc<HostManager>>, which: i32) -> DashboardSnapshot {
    mgr.insert_pc_hooks(which);
    mgr.snapshot()
}

#[tauri::command]
fn remove_hook(mgr: State<'_, Arc<HostManager>>, pid: u32, addr: u64) -> DashboardSnapshot {
    mgr.remove_hook(pid, addr);
    mgr.snapshot()
}

#[tauri::command]
fn preview_clean_sentence(
    mgr: State<'_, Arc<HostManager>>,
    raw: String,
    process_path: Option<String>,
) -> String {
    let cfg = mgr.config.read();
    clean_sentence(&raw, process_path.as_deref(), &cfg)
}

#[tauri::command]
fn broadcast_test_sentence(
    mgr: State<'_, Arc<HostManager>>,
    sentence: String,
) -> Result<usize, String> {
    let snap = mgr.snapshot();
    let (pid, path) = snap
        .attached_processes
        .first()
        .map(|p| (p.pid, p.exe_path.clone()))
        .unwrap_or((0, "TestGame.exe".to_string()));
    let payload = ExStaticSentencePayload {
        process_id: pid,
        process_path: path,
        sentence,
    };
    Ok(mgr.ws_server.broadcast_sentence(&payload))
}

#[tauri::command]
fn copy_to_clipboard(text: String) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
fn import_legacy_config_file(
    mgr: State<'_, Arc<HostManager>>,
    path: String,
    content: Option<String>,
) -> Result<AppConfig, String> {
    let p = if let Some(raw_text) = content {
        let ext = if path.to_lowercase().ends_with(".json") {
            "json"
        } else {
            "txt"
        };
        let tmp = std::env::temp_dir().join(format!("kiroku_import_tmp.{}", ext));
        std::fs::write(&tmp, raw_text).map_err(|e| e.to_string())?;
        tmp
    } else {
        PathBuf::from(&path)
    };
    let mut cfg = mgr.config.read().clone();
    if path.to_lowercase().ends_with(".json") {
        cfg.import_legacy_config64(&p).map_err(|e| e.to_string())?;
    } else {
        cfg.import_legacy_regex_filters(&p)
            .map_err(|e| e.to_string())?;
    }
    mgr.update_config(cfg).map_err(|e| e.to_string())?;
    Ok(mgr.config.read().clone())
}

#[tauri::command]
fn minimize_to_tray_zero_ram(app: AppHandle) -> Result<(), String> {
    if !TRAY_READY.load(Ordering::Relaxed) {
        return Err("System tray icon is not initialized.".to_string());
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.destroy();
    }
    Ok(())
}

fn show_or_recreate_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    if let Some(win_cfg) = app.config().app.windows.first() {
        if let Ok(builder) = tauri::WebviewWindowBuilder::from_config(app, win_cfg) {
            if let Ok(w) = builder.build() {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
    }
}

fn setup_tray(app: &AppHandle) -> anyhow::Result<()> {
    let show_i = MenuItem::with_id(app, "show", "Open Kiroku", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit Kiroku", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Kiroku (記録) — Text Hook Bridge")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                show_or_recreate_main_window(app);
            }
            "quit" => {
                if let Some(mgr) = app.try_state::<Arc<HostManager>>() {
                    mgr.detach_all();
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                show_or_recreate_main_window(app);
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let _ = builder.build(app)?;
    TRAY_READY.store(true, Ordering::Relaxed);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let config_path = app_data_dir.join("kiroku_config.json");

            let mgr = HostManager::init(handle.clone(), config_path)?;
            let start_min_cfg = mgr.config.read().start_minimized;
            app.manage(mgr);

            if let Err(e) = setup_tray(&handle) {
                log::warn!("Tray icon initialization failed: {}", e);
            }

            let launched_minimized = std::env::args().any(|a| a == "--minimized") || start_min_cfg;
            if !launched_minimized || !TRAY_READY.load(Ordering::Relaxed) {
                show_or_recreate_main_window(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                let app = window.app_handle();
                if let Some(mgr) = app.try_state::<Arc<HostManager>>() {
                    let minimize_to_tray = mgr.config.read().minimize_to_tray;
                    if minimize_to_tray && TRAY_READY.load(Ordering::Relaxed) {
                        // Allow the WebView window to close & destroy itself (Zero-RAM Tray Mode).
                        // `RunEvent::ExitRequested` below prevents the Rust process from exiting.
                    } else {
                        mgr.detach_all();
                        app.exit(0);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard_snapshot,
            get_config,
            save_config,
            list_processes,
            attach_process,
            detach_process,
            detach_all,
            select_thread,
            insert_hook_code,
            insert_pc_hooks,
            remove_hook,
            preview_clean_sentence,
            broadcast_test_sentence,
            copy_to_clipboard,
            import_legacy_config_file,
            minimize_to_tray_zero_ram,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app_handle, event| {
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
            // When `code.is_none()`, the last WebView window was destroyed to enter Zero-RAM Tray Mode.
            // Keep the native Rust process and tray icon alive until `app.exit(0)` is explicitly called.
            if code.is_none() && TRAY_READY.load(Ordering::Relaxed) {
                api.prevent_exit();
            }
        }
    });
}
