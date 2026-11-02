use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Notify};
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::Message;

/// Exact JSON payload expected by exSTATic / TextractorSender clients
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExStaticSentencePayload {
    pub process_id: u32,
    pub process_path: String,
    pub sentence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientFormatMode {
    FollowDefault,
    ExStaticJson,
    PlainText,
}

pub struct ExStaticBroadcastServer {
    tx: broadcast::Sender<ExStaticSentencePayload>,
    desired_port: AtomicU16,
    bound_port: AtomicU16,
    default_plain_text: AtomicBool,
    client_count: Arc<AtomicUsize>,
    last_payload: Arc<RwLock<Option<ExStaticSentencePayload>>>,
    rebind_notify: Arc<Notify>,
}

impl ExStaticBroadcastServer {
    pub fn new(initial_port: u16, initial_plain_text: bool) -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        let server = Arc::new(Self {
            tx,
            desired_port: AtomicU16::new(initial_port),
            bound_port: AtomicU16::new(initial_port),
            default_plain_text: AtomicBool::new(initial_plain_text),
            client_count: Arc::new(AtomicUsize::new(0)),
            last_payload: Arc::new(RwLock::new(None)),
            rebind_notify: Arc::new(Notify::new()),
        });
        server.spawn_listener();
        server
    }

    pub fn port(&self) -> u16 {
        self.bound_port.load(Ordering::Relaxed)
    }

    pub fn client_count(&self) -> usize {
        self.client_count.load(Ordering::Relaxed)
    }

    pub fn set_plain_text(&self, plain_text: bool) {
        self.default_plain_text.store(plain_text, Ordering::Relaxed);
    }

    pub fn set_port(&self, new_port: u16) {
        if new_port < 1024 {
            return;
        }
        let old = self.desired_port.swap(new_port, Ordering::Relaxed);
        if old != new_port {
            self.rebind_notify.notify_one();
        }
    }

    pub fn broadcast_sentence(&self, payload: &ExStaticSentencePayload) -> usize {
        *self.last_payload.write() = Some(payload.clone());
        self.tx.send(payload.clone()).unwrap_or(0)
    }

    fn format_for_client(&self, payload: &ExStaticSentencePayload, mode: ClientFormatMode) -> Option<String> {
        let use_plain = match mode {
            ClientFormatMode::PlainText => true,
            ClientFormatMode::ExStaticJson => false,
            ClientFormatMode::FollowDefault => self.default_plain_text.load(Ordering::Relaxed),
        };
        if use_plain {
            Some(payload.sentence.clone())
        } else {
            serde_json::to_string(payload).ok()
        }
    }

    fn spawn_listener(self: &Arc<Self>) {
        let server = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            loop {
                let port = server.desired_port.load(Ordering::Relaxed);
                let addr = format!("127.0.0.1:{}", port);
                let listener = match TcpListener::bind(&addr).await {
                    Ok(l) => {
                        server.bound_port.store(port, Ordering::Relaxed);
                        info!("exSTATic WebSocket server listening on ws://{}", addr);
                        l
                    }
                    Err(e) => {
                        warn!(
                            "Failed to bind exSTATic WebSocket server on {}: {}. Retrying in 3s...",
                            addr, e
                        );
                        tokio::select! {
                            _ = server.rebind_notify.notified() => continue,
                            _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => continue,
                        }
                    }
                };

                loop {
                    tokio::select! {
                        _ = server.rebind_notify.notified() => {
                            info!("Rebinding exSTATic WebSocket server to new port...");
                            break;
                        }
                        accept_res = listener.accept() => {
                            match accept_res {
                                Ok((stream, peer)) => {
                                    info!("New exSTATic client connected from {}", peer);
                                    let srv = Arc::clone(&server);
                                    tauri::async_runtime::spawn(async move {
                                        srv.handle_client(stream).await;
                                    });
                                }
                                Err(e) => {
                                    error!("Error accepting WebSocket connection: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    async fn handle_client(&self, stream: TcpStream) {
        let mut mode = ClientFormatMode::FollowDefault;
        let ws_stream = match accept_hdr_async(stream, |req: &Request, response: Response| {
            let path = req.uri().path().to_ascii_lowercase();
            if path.contains("raw") || path.contains("text") {
                mode = ClientFormatMode::PlainText;
            } else if path.contains("exstatic") || path.contains("json") {
                mode = ClientFormatMode::ExStaticJson;
            }
            Ok(response)
        })
        .await
        {
            Ok(ws) => ws,
            Err(e) => {
                warn!("WebSocket handshake failed: {}", e);
                return;
            }
        };

        self.client_count.fetch_add(1, Ordering::Relaxed);
        let (mut write, mut read) = ws_stream.split();
        let mut rx = self.tx.subscribe();

        // Send most recent sentence to newly connected client if available
        let initial_payload = self.last_payload.read().clone();
        if let Some(payload) = initial_payload {
            if let Some(msg) = self.format_for_client(&payload, mode) {
                if write.send(Message::Text(msg)).await.is_err() {
                    self.client_count.fetch_sub(1, Ordering::Relaxed);
                    return;
                }
            }
        }

        loop {
            tokio::select! {
                Ok(payload) = rx.recv() => {
                    if let Some(msg) = self.format_for_client(&payload, mode) {
                        if write.send(Message::Text(msg)).await.is_err() {
                            break;
                        }
                    }
                }
                incoming = read.next() => {
                    match incoming {
                        Some(Ok(Message::Ping(payload))) => {
                            let _ = write.send(Message::Pong(payload)).await;
                        }
                        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => {
                            break;
                        }
                        // Broadcast server is strictly read-only for connected browser clients
                        _ => {}
                    }
                }
            }
        }

        self.client_count.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exstatic_payload_serialization() {
        let payload = ExStaticSentencePayload {
            process_id: 4242,
            process_path: "D:\\VNs\\Summer Pockets\\SiglusEngine.exe".to_string(),
            sentence: "「眩しさだけは、忘れなかった。」".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"process_id\":4242"));
        assert!(json.contains("\"process_path\":\"D:\\\\VNs\\\\Summer Pockets\\\\SiglusEngine.exe\""));
        assert!(json.contains("\"sentence\":\"「眩しさだけは、忘れなかった。」\""));
    }
}
