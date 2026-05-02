// GhostWire Server - WebSocket Relay
// This module implements the "dumb relay" - it broadcasts messages without understanding them

use axum::extract::ws::{Message, WebSocket};
use futures::{stream::StreamExt, SinkExt};
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Unique identifier for each connected client
pub type ClientId = usize;

/// Message to be broadcast to clients
#[derive(Debug, Clone)]
pub struct BroadcastMessage {
    /// The client who sent this message (to avoid echo)
    pub from: ClientId,
    /// The raw message content (JSON string)
    pub content: String,
}

/// Shared state for the relay server
#[derive(Clone)]
pub struct RelayState {
    /// Map of client IDs to their broadcast channels.
    /// Uses `Arc<str>` so broadcast() creates a single refcounted copy instead
    /// of cloning the String once per recipient.
    clients: Arc<RwLock<HashMap<ClientId, mpsc::UnboundedSender<Arc<str>>>>>,
    /// Monotonic counter for unique client IDs.
    /// AtomicUsize replaces Arc<RwLock<usize>> — a single fetch_add instruction
    /// vs. a full async lock acquisition.
    next_client_id: Arc<AtomicUsize>,
}

impl RelayState {
    /// Create a new relay state
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            next_client_id: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Get the next available client ID (lock-free).
    fn next_id(&self) -> ClientId {
        self.next_client_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Register a new client and return their ID and receiver.
    /// Holds the write lock only once, reading `.len()` from the held guard.
    async fn register_client(&self) -> (ClientId, mpsc::UnboundedReceiver<Arc<str>>) {
        let id = self.next_id();
        let (tx, rx) = mpsc::unbounded_channel();

        let mut clients = self.clients.write().await;
        clients.insert(id, tx);
        info!("Client {} connected. Total clients: {}", id, clients.len());

        (id, rx)
    }

    /// Unregister a client.
    /// Holds the write lock only once, reading `.len()` from the held guard.
    async fn unregister_client(&self, id: ClientId) {
        let mut clients = self.clients.write().await;
        clients.remove(&id);
        info!("Client {} disconnected. Total clients: {}", id, clients.len());
    }

    /// Broadcast a message to all clients except the sender.
    /// Builds one `Arc<str>` and sends refcount increments — O(1) allocation
    /// regardless of connected client count.
    async fn broadcast(&self, msg: BroadcastMessage) {
        let shared: Arc<str> = Arc::from(msg.content.as_str());
        let clients = self.clients.read().await;
        let mut failed_clients = Vec::new();

        for (&client_id, tx) in clients.iter() {
            // Don't echo back to sender
            if client_id == msg.from {
                continue;
            }

            // Arc::clone is just an atomic refcount increment
            if let Err(e) = tx.send(Arc::clone(&shared)) {
                warn!("Failed to send to client {}: {}", client_id, e);
                failed_clients.push(client_id);
            }
        }

        // Clean up failed clients
        drop(clients);
        if !failed_clients.is_empty() {
            let mut clients = self.clients.write().await;
            for client_id in failed_clients {
                clients.remove(&client_id);
                debug!("Removed dead client {}", client_id);
            }
        }
    }

    /// Get the current number of connected clients
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }
}

/// Handle a WebSocket connection
pub async fn handle_websocket(socket: WebSocket, state: RelayState) {
    // Register this client
    let (client_id, mut broadcast_rx) = state.register_client().await;

    // Split the WebSocket into sender and receiver
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Spawn a task to forward broadcast messages to this client
    // Also send periodic pings to keep the connection alive
    let mut send_task = tokio::spawn(async move {
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(30));
        heartbeat.tick().await; // First tick completes immediately

        loop {
            tokio::select! {
                // Send heartbeat ping
                _ = heartbeat.tick() => {
                    if ws_tx.send(Message::Ping(vec![].into())).await.is_err() {
                        // Client disconnected
                        break;
                    }
                }

                // Forward broadcast messages.
                // msg is Arc<str>; allocate the String here (unavoidable for WS send),
                // but the channel itself held only a refcount, not a full copy.
                Some(msg) = broadcast_rx.recv() => {
                    if ws_tx.send(Message::Text((*msg).to_owned().into())).await.is_err() {
                        // Client disconnected
                        break;
                    }
                }

                // Channel closed
                else => break,
            }
        }
    });

    // Handle incoming messages from this client
    let state_clone = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(result) = ws_rx.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    debug!("Client {} sent: {} bytes", client_id, text.len());

                    // Broadcast to all other clients.
                    // text is Utf8Bytes (axum 0.8); .to_string() is the required conversion.
                    // The broadcast() itself avoids N-1 further clones via Arc<str>.
                    state_clone
                        .broadcast(BroadcastMessage {
                            from: client_id,
                            content: text.to_string(),
                        })
                        .await;
                }
                Ok(Message::Close(_)) => {
                    info!("Client {} sent close frame", client_id);
                    break;
                }
                Ok(Message::Ping(_data)) => {
                    debug!("Client {} sent ping", client_id);
                    // Pongs are handled automatically by axum
                }
                Ok(Message::Pong(_)) => {
                    debug!("Client {} sent pong", client_id);
                }
                Ok(Message::Binary(_)) => {
                    warn!("Client {} sent binary data (ignored)", client_id);
                }
                Err(e) => {
                    error!("WebSocket error for client {}: {}", client_id, e);
                    break;
                }
            }
        }
    });

    // Wait for either task to finish (disconnect)
    tokio::select! {
        _ = &mut send_task => {
            debug!("Send task finished for client {}", client_id);
            recv_task.abort();
        }
        _ = &mut recv_task => {
            debug!("Recv task finished for client {}", client_id);
            send_task.abort();
        }
    }

    // Unregister the client
    state.unregister_client(client_id).await;
}
