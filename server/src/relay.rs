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
use tracing::{debug, info, warn};

/// Unique identifier for each connected client
pub type ClientId = usize;

/// Maximum number of simultaneously connected clients.
/// Caps file-descriptor and memory growth from a connection flood.
pub const DEFAULT_MAX_CLIENTS: usize = 10_000;

/// Per-client outbound buffer (messages). A bounded buffer turns a slow or
/// malicious reader into a *dropped client* rather than unbounded memory growth:
/// once this fills, broadcast() evicts the client instead of queueing forever.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Largest inbound text frame we accept (bytes). Chat ciphertext is tiny; this
/// is a generous ceiling that lets us drop and log oversized frames rather than
/// relaying them.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

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
    clients: Arc<RwLock<HashMap<ClientId, mpsc::Sender<Arc<str>>>>>,
    /// Monotonic counter for unique client IDs.
    /// AtomicUsize replaces Arc<RwLock<usize>> — a single fetch_add instruction
    /// vs. a full async lock acquisition.
    next_client_id: Arc<AtomicUsize>,
    /// Upper bound on concurrent clients (see [`DEFAULT_MAX_CLIENTS`]).
    max_clients: usize,
    /// Per-client outbound buffer size (see [`DEFAULT_CHANNEL_CAPACITY`]).
    channel_capacity: usize,
}

impl RelayState {
    /// Create a new relay state with production defaults.
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_MAX_CLIENTS, DEFAULT_CHANNEL_CAPACITY)
    }

    /// Create a relay state with explicit limits (used by tests).
    pub fn with_limits(max_clients: usize, channel_capacity: usize) -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            next_client_id: Arc::new(AtomicUsize::new(0)),
            max_clients,
            channel_capacity,
        }
    }

    /// Get the next available client ID (lock-free).
    fn next_id(&self) -> ClientId {
        self.next_client_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Register a new client and return their ID and receiver.
    /// Returns `None` if the server is at capacity, so the caller can reject the
    /// connection. The capacity check and insert happen under a single write
    /// lock so they are atomic.
    async fn try_register_client(&self) -> Option<(ClientId, mpsc::Receiver<Arc<str>>)> {
        let (tx, rx) = mpsc::channel(self.channel_capacity);

        let mut clients = self.clients.write().await;
        if clients.len() >= self.max_clients {
            warn!(
                "Connection rejected: at capacity ({} clients)",
                self.max_clients
            );
            return None;
        }

        let id = self.next_id();
        clients.insert(id, tx);
        info!("Client {} connected. Total clients: {}", id, clients.len());

        Some((id, rx))
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
    ///
    /// Uses non-blocking `try_send`: a client whose buffer is full (a slow or
    /// stalled reader) is evicted rather than allowed to apply backpressure to
    /// the whole relay or grow memory without bound.
    async fn broadcast(&self, msg: BroadcastMessage) {
        let shared: Arc<str> = Arc::from(msg.content.as_str());
        let clients = self.clients.read().await;
        let mut failed_clients = Vec::new();

        for (&client_id, tx) in clients.iter() {
            // Don't echo back to sender
            if client_id == msg.from {
                continue;
            }

            // Arc::clone is just an atomic refcount increment.
            // try_send never awaits: Full (slow client) and Closed (gone) both
            // mean "drop this client".
            if let Err(e) = tx.try_send(Arc::clone(&shared)) {
                let reason = match e {
                    mpsc::error::TrySendError::Full(_) => "full",
                    mpsc::error::TrySendError::Closed(_) => "closed",
                };
                warn!("Dropping client {} (channel {})", client_id, reason);
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

impl Default for RelayState {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle a WebSocket connection
pub async fn handle_websocket(socket: WebSocket, state: RelayState) {
    // Split the WebSocket into sender and receiver
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Register this client (rejecting if the relay is at capacity)
    let (client_id, mut broadcast_rx) = match state.try_register_client().await {
        Some(registration) => registration,
        None => {
            // Politely close: server is full.
            let _ = ws_tx.send(Message::Close(None)).await;
            return;
        }
    };

    // Spawn a task to forward broadcast messages to this client.
    // Also send periodic pings to keep the connection alive.
    let mut send_task = tokio::spawn(async move {
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(30));
        heartbeat.tick().await; // First tick completes immediately

        loop {
            tokio::select! {
                // Send heartbeat ping
                _ = heartbeat.tick() => {
                    if ws_tx.send(Message::Ping(vec![].into())).await.is_err() {
                        break; // Client disconnected
                    }
                }

                // Forward broadcast messages.
                // msg is Arc<str>; allocate the String here (unavoidable for WS send),
                // but the channel itself held only a refcount, not a full copy.
                Some(msg) = broadcast_rx.recv() => {
                    if ws_tx.send(Message::Text((*msg).to_owned().into())).await.is_err() {
                        break; // Client disconnected
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
                    // Drop and log oversized frames rather than relaying them.
                    if text.len() > MAX_MESSAGE_BYTES {
                        warn!(
                            "Client {} sent oversized message ({} bytes > {}), closing",
                            client_id,
                            text.len(),
                            MAX_MESSAGE_BYTES
                        );
                        break;
                    }

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
                    warn!("WebSocket error for client {}: {}", client_id, e);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_connections_beyond_capacity() {
        let state = RelayState::with_limits(2, 8);

        // Hold the receivers so the senders stay alive in the map.
        let first = state.try_register_client().await;
        let second = state.try_register_client().await;
        assert!(first.is_some());
        assert!(second.is_some());
        assert_eq!(state.client_count().await, 2);

        // Third connection is over capacity and must be rejected.
        assert!(state.try_register_client().await.is_none());
        assert_eq!(state.client_count().await, 2);

        // Keep guards alive until here.
        drop((first, second));
    }

    #[tokio::test]
    async fn evicts_client_with_full_buffer() {
        let state = RelayState::with_limits(10, 2);

        // Slow client: registered but never drains its receiver.
        let (_slow_id, _slow_rx) = state.try_register_client().await.unwrap();
        // A sender we can broadcast "from" so the slow client is a recipient.
        let (sender_id, _sender_rx) = state.try_register_client().await.unwrap();
        assert_eq!(state.client_count().await, 2);

        // Send more messages than the slow client's buffer (capacity 2) can hold.
        for i in 0..5 {
            state
                .broadcast(BroadcastMessage {
                    from: sender_id,
                    content: format!("msg {i}"),
                })
                .await;
        }

        // The slow client should have been evicted once its buffer filled.
        assert_eq!(state.client_count().await, 1);
    }

    #[tokio::test]
    async fn delivers_to_other_clients() {
        let state = RelayState::with_limits(10, 8);

        let (sender_id, _sender_rx) = state.try_register_client().await.unwrap();
        let (_recipient_id, mut recipient_rx) = state.try_register_client().await.unwrap();

        state
            .broadcast(BroadcastMessage {
                from: sender_id,
                content: "hello".to_string(),
            })
            .await;

        let received = recipient_rx.recv().await.expect("recipient should receive");
        assert_eq!(&*received, "hello");
        // Both clients remain connected.
        assert_eq!(state.client_count().await, 2);
    }

    #[tokio::test]
    async fn does_not_echo_to_sender() {
        let state = RelayState::with_limits(10, 8);

        let (sender_id, mut sender_rx) = state.try_register_client().await.unwrap();

        state
            .broadcast(BroadcastMessage {
                from: sender_id,
                content: "no echo".to_string(),
            })
            .await;

        // Sender must not receive its own message.
        assert!(sender_rx.try_recv().is_err());
    }
}
