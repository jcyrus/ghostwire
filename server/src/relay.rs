// GhostWire Server - WebSocket Relay
//
// Historically this was a pure "dumb broadcast" relay. As of v0.6 it does the
// minimum parsing needed to route direct messages to a single recipient
// instead of fanning every DM out to all clients (O(N) -> O(1) bandwidth).
//
// Privacy note: routing means the relay now learns *recipient* and *sender*
// usernames for DMs (the social graph), a deliberate trade-off documented in
// docs/user/SECURITY.md. Message *content* remains opaque ciphertext — the
// server still cannot read it. Anything it cannot confidently route (group,
// global, typing, key-exchange broadcasts, or an offline/unknown recipient)
// falls back to the original broadcast behavior.

use axum::extract::ws::{Message, WebSocket};
use futures::{stream::StreamExt, SinkExt};
use serde::Deserialize;
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
/// once this fills, send paths evict the client instead of queueing forever.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Largest inbound text frame we accept (bytes). Chat ciphertext is tiny; this
/// is a generous ceiling that lets us drop and log oversized frames rather than
/// relaying them.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// Minimal view of the wire message needed for routing. All fields are optional
/// / defaulted so a malformed or unexpected payload never fails to parse — it
/// just falls back to broadcast. Mirrors the client's `WireMessage`
/// (client/src/app.rs); only the routing-relevant fields are read here.
#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(rename = "type", default)]
    msg_type: String,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    recipient: Option<String>,
    #[serde(default)]
    meta: EnvelopeMeta,
}

#[derive(Debug, Default, Deserialize)]
struct EnvelopeMeta {
    #[serde(default)]
    sender: String,
}

/// Per-connection state held in the registry.
struct ClientHandle {
    tx: mpsc::Sender<Arc<str>>,
    /// Username learned from this client's AUTH message, if any.
    username: Option<String>,
}

/// All shared, lock-protected relay state. Keeping the client table and the
/// username index behind a single lock avoids any lock-ordering hazard.
#[derive(Default)]
struct Registry {
    /// client id -> connection handle
    clients: HashMap<ClientId, ClientHandle>,
    /// username -> client id (for O(1) DM routing)
    names: HashMap<String, ClientId>,
}

/// Shared state for the relay server
#[derive(Clone)]
pub struct RelayState {
    registry: Arc<RwLock<Registry>>,
    /// Monotonic counter for unique client IDs.
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
            registry: Arc::new(RwLock::new(Registry::default())),
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

        let mut reg = self.registry.write().await;
        if reg.clients.len() >= self.max_clients {
            warn!(
                "Connection rejected: at capacity ({} clients)",
                self.max_clients
            );
            return None;
        }

        let id = self.next_id();
        reg.clients.insert(
            id,
            ClientHandle {
                tx,
                username: None,
            },
        );
        info!("Client {} connected. Total clients: {}", id, reg.clients.len());

        Some((id, rx))
    }

    /// Unregister a client and drop any username it owned.
    async fn unregister_client(&self, id: ClientId) {
        let mut reg = self.registry.write().await;
        if let Some(handle) = reg.clients.remove(&id)
            && let Some(name) = handle.username
            // Only clear the name index if it still points at this client
            // (a reconnect under the same name may have re-claimed it).
            && reg.names.get(&name) == Some(&id)
        {
            reg.names.remove(&name);
        }
        info!("Client {} disconnected. Total clients: {}", id, reg.clients.len());
    }

    /// Associate a username (from AUTH) with a client id.
    ///
    /// Usernames are unauthenticated, so this is deliberately conservative to
    /// keep the index consistent with the live client table:
    /// - if the client is already gone (e.g. evicted for a full buffer) nothing
    ///   is inserted, so the index never points at a non-existent connection
    ///   (which would silently drop DMs instead of falling back to broadcast);
    /// - a name already held by a *different, still-connected* client is left
    ///   untouched and the claim ignored, so no connection can hijack another
    ///   user's DMs by re-AUTHing as them — those DMs just keep broadcasting;
    /// - if this client previously claimed a different name, that stale index
    ///   entry is dropped before the new one is recorded.
    async fn set_username(&self, id: ClientId, name: &str) {
        if name.is_empty() {
            return;
        }
        let mut reg = self.registry.write().await;

        // Client already gone: don't create an orphan index entry.
        if !reg.clients.contains_key(&id) {
            debug!("Ignoring AUTH for '{}': client {} already gone", name, id);
            return;
        }

        // Refuse to steal a name another live client already holds.
        if let Some(&owner) = reg.names.get(name)
            && owner != id
            && reg.clients.contains_key(&owner)
        {
            debug!("Client {} tried to claim in-use name '{}'; ignoring", id, name);
            return;
        }

        // Drop any previous name this client held so no stale entry lingers.
        let previous = reg
            .clients
            .get_mut(&id)
            .and_then(|handle| handle.username.replace(name.to_string()));
        if let Some(old) = previous
            && old != name
            && reg.names.get(&old) == Some(&id)
        {
            reg.names.remove(&old);
        }

        reg.names.insert(name.to_string(), id);
        debug!("Client {} registered as '{}'", id, name);
    }

    /// Parse and relay one inbound message: learn usernames from AUTH, unicast
    /// routable DMs, and broadcast everything else.
    async fn relay(&self, from: ClientId, content: String) {
        match serde_json::from_str::<Envelope>(&content) {
            Ok(env) => {
                // Learn the sender's username from its AUTH announcement.
                if env.msg_type == "AUTH" {
                    self.set_username(from, &env.meta.sender).await;
                }

                // Unicast only a true, routable DM: a named recipient that is
                // currently connected, on a non-group channel. Everything else
                // (group, global, typing, key-exchange broadcast, offline or
                // unknown recipient) falls through to broadcast.
                if let Some(recipient) = env.recipient.as_deref()
                    && !env.channel.starts_with("group:")
                    && let Some(target) = self.lookup(recipient).await
                {
                    self.unicast(target, content).await;
                    return;
                }

                self.broadcast(from, content).await;
            }
            // Unparseable payloads keep the old dumb-relay behavior.
            Err(_) => self.broadcast(from, content).await,
        }
    }

    /// Resolve a username to a connected client id.
    async fn lookup(&self, username: &str) -> Option<ClientId> {
        self.registry.read().await.names.get(username).copied()
    }

    /// Send a message to a single client. Evicts the client if its buffer is
    /// full (stalled reader) or closed (gone).
    async fn unicast(&self, target: ClientId, content: String) {
        let shared: Arc<str> = Arc::from(content.as_str());
        let failed = {
            let reg = self.registry.read().await;
            match reg.clients.get(&target) {
                Some(handle) => handle.tx.try_send(shared).is_err(),
                None => false,
            }
        };
        if failed {
            warn!("Dropping client {} (unicast send failed)", target);
            self.remove_clients(&[target]).await;
        }
    }

    /// Broadcast a message to all clients except the sender.
    /// Builds one `Arc<str>` and sends refcount increments — O(1) allocation
    /// regardless of connected client count.
    ///
    /// Uses non-blocking `try_send`: a client whose buffer is full (a slow or
    /// stalled reader) is evicted rather than allowed to apply backpressure to
    /// the whole relay or grow memory without bound.
    async fn broadcast(&self, from: ClientId, content: String) {
        let shared: Arc<str> = Arc::from(content.as_str());
        let mut failed_clients = Vec::new();

        {
            let reg = self.registry.read().await;
            for (&client_id, handle) in reg.clients.iter() {
                // Don't echo back to sender
                if client_id == from {
                    continue;
                }
                // try_send never awaits: Full (slow client) and Closed (gone)
                // both mean "drop this client".
                if handle.tx.try_send(Arc::clone(&shared)).is_err() {
                    failed_clients.push(client_id);
                }
            }
        }

        if !failed_clients.is_empty() {
            self.remove_clients(&failed_clients).await;
        }
    }

    /// Remove dead clients and any username index entries they owned.
    async fn remove_clients(&self, ids: &[ClientId]) {
        let mut reg = self.registry.write().await;
        for &id in ids {
            if let Some(handle) = reg.clients.remove(&id) {
                if let Some(name) = handle.username
                    && reg.names.get(&name) == Some(&id)
                {
                    reg.names.remove(&name);
                }
                debug!("Removed dead client {}", id);
            }
        }
    }

    /// Get the current number of connected clients
    pub async fn client_count(&self) -> usize {
        self.registry.read().await.clients.len()
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

                // Forward routed/broadcast messages.
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

                    // Parse-and-route. text is Utf8Bytes (axum 0.8); .to_string()
                    // is the required conversion. relay() decides unicast vs broadcast.
                    state_clone.relay(client_id, text.to_string()).await;
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

    /// Build an AUTH wire message for `user` (matches the client's format).
    fn auth_json(user: &str) -> String {
        format!(
            r#"{{"type":"AUTH","payload":"{user}","channel":"global","meta":{{"sender":"{user}","timestamp":0}},"action":false}}"#
        )
    }

    /// Build a DM message from `sender` to `recipient` on a dm: channel.
    fn dm_json(sender: &str, recipient: &str) -> String {
        format!(
            r#"{{"type":"MSG","payload":"ciphertext","channel":"dm:{sender}:{recipient}","meta":{{"sender":"{sender}","timestamp":0}},"encrypted":true,"recipient":"{recipient}","action":false}}"#
        )
    }

    #[tokio::test]
    async fn rejects_connections_beyond_capacity() {
        let state = RelayState::with_limits(2, 8);

        let first = state.try_register_client().await;
        let second = state.try_register_client().await;
        assert!(first.is_some());
        assert!(second.is_some());
        assert_eq!(state.client_count().await, 2);

        // Third connection is over capacity and must be rejected.
        assert!(state.try_register_client().await.is_none());
        assert_eq!(state.client_count().await, 2);

        drop((first, second));
    }

    #[tokio::test]
    async fn evicts_client_with_full_buffer() {
        let state = RelayState::with_limits(10, 2);

        // Slow client: registered but never drains its receiver.
        let (_slow_id, _slow_rx) = state.try_register_client().await.unwrap();
        let (sender_id, _sender_rx) = state.try_register_client().await.unwrap();
        assert_eq!(state.client_count().await, 2);

        // Broadcast more than the slow client's buffer (capacity 2) can hold.
        for i in 0..5 {
            state.broadcast(sender_id, format!("msg {i}")).await;
        }

        assert_eq!(state.client_count().await, 1);
    }

    #[tokio::test]
    async fn broadcast_delivers_to_other_clients() {
        let state = RelayState::with_limits(10, 8);

        let (sender_id, _sender_rx) = state.try_register_client().await.unwrap();
        let (_recipient_id, mut recipient_rx) = state.try_register_client().await.unwrap();

        state.broadcast(sender_id, "hello".to_string()).await;

        let received = recipient_rx.recv().await.expect("recipient should receive");
        assert_eq!(&*received, "hello");
        assert_eq!(state.client_count().await, 2);
    }

    #[tokio::test]
    async fn broadcast_does_not_echo_to_sender() {
        let state = RelayState::with_limits(10, 8);

        let (sender_id, mut sender_rx) = state.try_register_client().await.unwrap();
        state.broadcast(sender_id, "no echo".to_string()).await;

        assert!(sender_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn dm_is_unicast_only_to_recipient() {
        let state = RelayState::with_limits(10, 8);

        let (alice, mut alice_rx) = state.try_register_client().await.unwrap();
        let (_bob, mut bob_rx) = state.try_register_client().await.unwrap();
        let (_carol, mut carol_rx) = state.try_register_client().await.unwrap();

        // Everyone authenticates so the relay learns the name -> id map.
        state.relay(alice, auth_json("alice")).await;
        state.relay(_bob, auth_json("bob")).await;
        state.relay(_carol, auth_json("carol")).await;

        // Drain the AUTH broadcasts each client received.
        while bob_rx.try_recv().is_ok() {}
        while carol_rx.try_recv().is_ok() {}
        while alice_rx.try_recv().is_ok() {}

        // Alice DMs bob.
        state.relay(alice, dm_json("alice", "bob")).await;

        // Bob receives exactly the DM; carol receives nothing.
        let got = bob_rx.try_recv().expect("bob should get the DM");
        assert!(got.contains("\"recipient\":\"bob\""));
        assert!(carol_rx.try_recv().is_err(), "carol must not receive the DM");
        assert!(alice_rx.try_recv().is_err(), "sender must not get an echo");
    }

    #[tokio::test]
    async fn dm_to_unknown_recipient_falls_back_to_broadcast() {
        let state = RelayState::with_limits(10, 8);

        let (alice, _alice_rx) = state.try_register_client().await.unwrap();
        let (_bob, mut bob_rx) = state.try_register_client().await.unwrap();

        // Alice authenticates; bob does NOT, so "bob" is unknown to the relay.
        state.relay(alice, auth_json("alice")).await;
        while bob_rx.try_recv().is_ok() {}

        // DM to the (unregistered) name falls back to broadcast, so bob still
        // receives it — preserving pre-routing delivery behavior.
        state.relay(alice, dm_json("alice", "bob")).await;
        assert!(bob_rx.try_recv().is_ok(), "fallback broadcast should reach bob");
    }

    #[tokio::test]
    async fn group_message_is_broadcast_not_unicast() {
        let state = RelayState::with_limits(10, 8);

        let (alice, _alice_rx) = state.try_register_client().await.unwrap();
        let (_bob, mut bob_rx) = state.try_register_client().await.unwrap();
        let (_carol, mut carol_rx) = state.try_register_client().await.unwrap();

        state.relay(alice, auth_json("alice")).await;
        state.relay(_bob, auth_json("bob")).await;
        state.relay(_carol, auth_json("carol")).await;
        while bob_rx.try_recv().is_ok() {}
        while carol_rx.try_recv().is_ok() {}

        // Group message: recipient is set to the group id, channel is group:*.
        let group = r#"{"type":"MSG","payload":"ct","channel":"group:team","meta":{"sender":"alice","timestamp":0},"encrypted":true,"recipient":"group:team","action":false}"#;
        state.relay(alice, group.to_string()).await;

        // Both other members receive it (broadcast), not just one.
        assert!(bob_rx.try_recv().is_ok());
        assert!(carol_rx.try_recv().is_ok());
    }

    #[tokio::test]
    async fn username_is_cleared_on_disconnect() {
        let state = RelayState::with_limits(10, 8);

        let (alice, _alice_rx) = state.try_register_client().await.unwrap();
        state.relay(alice, auth_json("alice")).await;
        assert_eq!(state.lookup("alice").await, Some(alice));

        state.unregister_client(alice).await;
        assert_eq!(state.lookup("alice").await, None);
        assert_eq!(state.client_count().await, 0);
    }

    #[tokio::test]
    async fn reauth_under_new_name_drops_stale_index_entry() {
        let state = RelayState::with_limits(10, 8);

        let (alice, _alice_rx) = state.try_register_client().await.unwrap();
        state.relay(alice, auth_json("alice")).await;
        assert_eq!(state.lookup("alice").await, Some(alice));

        // Same connection re-AUTHs under a different name: the old entry must go.
        state.relay(alice, auth_json("alice2")).await;
        assert_eq!(state.lookup("alice").await, None, "stale name must be dropped");
        assert_eq!(state.lookup("alice2").await, Some(alice));
    }

    #[tokio::test]
    async fn cannot_hijack_a_live_clients_name() {
        let state = RelayState::with_limits(10, 8);

        let (alice, _alice_rx) = state.try_register_client().await.unwrap();
        let (mallory, _mallory_rx) = state.try_register_client().await.unwrap();

        state.relay(alice, auth_json("alice")).await;
        // Mallory tries to claim "alice" while alice is still connected.
        state.relay(mallory, auth_json("alice")).await;

        // The name still routes to the original owner, not the impostor.
        assert_eq!(state.lookup("alice").await, Some(alice));
    }

    #[tokio::test]
    async fn auth_for_absent_client_creates_no_orphan_entry() {
        let state = RelayState::with_limits(10, 8);

        // An id that was never registered (mimics a client evicted before its
        // AUTH was processed) must not leave a dangling name in the index.
        state.set_username(999, "ghost").await;
        assert_eq!(state.lookup("ghost").await, None);
    }
}
