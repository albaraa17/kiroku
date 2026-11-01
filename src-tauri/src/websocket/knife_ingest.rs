use futures_util::StreamExt;
use log::{info, warn};
use parking_lot::RwLock;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub struct KnifeIngestClient {
    enabled: AtomicBool,
    connected: AtomicBool,
    forbidden_port: AtomicU16,
    target_url: RwLock<String>,
    reconnect_notify: Notify,
}

impl KnifeIngestClient {
    pub fn new(
        initial_enabled: bool,
        initial_url: String,
        forbidden_broadcast_port: u16,
        sentence_tx: mpsc::UnboundedSender<String>,
    ) -> Arc<Self> {
        let client = Arc::new(Self {
            enabled: AtomicBool::new(initial_enabled),
            connected: AtomicBool::new(false),
            forbidden_port: AtomicU16::new(forbidden_broadcast_port),
            target_url: RwLock::new(Self::normalize_ws_url(&initial_url)),
            reconnect_notify: Notify::new(),
        });
        client.spawn_worker(sentence_tx);
        client
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    pub fn current_url(&self) -> String {
        self.target_url.read().clone()
    }

    pub fn set_forbidden_port(&self, port: u16) {
        self.forbidden_port.store(port, Ordering::Relaxed);
    }

    pub fn update_config(&self, enabled: bool, url: &str) {
        let norm = Self::normalize_ws_url(url);
        let was_enabled = self.enabled.swap(enabled, Ordering::Relaxed);
        let mut guard = self.target_url.write();
        let url_changed = *guard != norm;
        *guard = norm;
        drop(guard);

        if was_enabled != enabled || url_changed {
            self.reconnect_notify.notify_one();
        }
    }

    /// Reads `knife.ini` next to `knife.exe` if present to extract `text-websocket = HOST:PORT`
    pub fn detect_ws_url_from_knife_exe(exe_path: &str) -> Option<String> {
        let path = Path::new(exe_path);
        let parent = path.parent()?;
        let ini_path = parent.join("knife.ini");
        if !ini_path.exists() {
            return None;
        }
        let content = fs::read_to_string(ini_path).ok()?;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(';') || trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                if k.trim() == "text-websocket" {
                    let val = v.trim();
                    if !val.is_empty() {
                        return Some(Self::normalize_ws_url(val));
                    }
                }
            }
        }
        None
    }

    pub fn normalize_ws_url(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return "ws://127.0.0.1:6677".to_string();
        }
        if let Some(rest) = trimmed.strip_prefix("wss://") {
            format!("ws://{}", rest)
        } else if trimmed.starts_with("ws://") {
            trimmed.to_string()
        } else {
            format!("ws://{}", trimmed)
        }
    }

    fn is_self_broadcast_url(&self, url: &str) -> bool {
        let port = self.forbidden_port.load(Ordering::Relaxed);
        let suffix = format!(":{}", port);
        (url.contains("127.0.0.1") || url.contains("localhost")) && url.ends_with(&suffix)
    }

    fn spawn_worker(self: &Arc<Self>, sentence_tx: mpsc::UnboundedSender<String>) {
        let client = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            loop {
                if !client.enabled.load(Ordering::Relaxed) {
                    client.connected.store(false, Ordering::Relaxed);
                    client.reconnect_notify.notified().await;
                    continue;
                }

                let url = client.target_url.read().clone();
                if client.is_self_broadcast_url(&url) {
                    warn!(
                        "Knife WS URL ({}) matches exSTATic broadcast port; skipping to prevent loop.",
                        url
                    );
                    client.connected.store(false, Ordering::Relaxed);
                    client.reconnect_notify.notified().await;
                    continue;
                }

                let connect_fut =
                    tokio::time::timeout(std::time::Duration::from_secs(4), connect_async(&url));

                match connect_fut.await {
                    Ok(Ok((ws_stream, _))) => {
                        info!("Connected to Knife/External WebSocket at {}", url);
                        client.connected.store(true, Ordering::Relaxed);
                        let (_, mut read) = ws_stream.split();

                        loop {
                            tokio::select! {
                                _ = client.reconnect_notify.notified() => {
                                    break;
                                }
                                msg = read.next() => {
                                    match msg {
                                        Some(Ok(Message::Text(text))) => {
                                            let s = text.trim();
                                            if !s.is_empty() && s.len() <= 16_384 {
                                                let _ = sentence_tx.send(s.to_string());
                                            }
                                        }
                                        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => {
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        client.connected.store(false, Ordering::Relaxed);
                    }
                    _ => {
                        client.connected.store(false, Ordering::Relaxed);
                    }
                }

                tokio::select! {
                    _ = client.reconnect_notify.notified() => {}
                    _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {}
                }
            }
        });
    }
}
