# ZeroDrop Server - Relay Architecture

## 🏗️ Overview

The ZeroDrop server is a **near-dumb relay** - it forwards WebSocket messages without ever decrypting their content; all message security is client-side. As of v0.6 it does the minimum parsing required to **route direct messages to a single recipient** (unicast) instead of broadcasting every DM to all clients. To do this it keeps an in-memory `username → connection` index, so the relay sees DM routing metadata (sender/recipient usernames) but never plaintext. Group, global, typing, and key-exchange traffic — and any DM whose recipient is offline or unknown — still fall back to broadcast.

---

## Architecture

### Thread-Safe State Management

```rust
pub struct RelayState {
    registry: Arc<RwLock<Registry>>,
    next_client_id: Arc<AtomicUsize>,
    max_clients: usize,        // connection cap (DoS guard)
    channel_capacity: usize,   // per-client outbound buffer
}

struct Registry {
    clients: HashMap<ClientId, ClientHandle>, // id -> connection
    names: HashMap<String, ClientId>,         // username -> id (DM routing)
}

struct ClientHandle {
    tx: mpsc::Sender<Arc<str>>,  // bounded per-client outbound channel
    username: Option<String>,    // learned from AUTH
}
```

**Key Design Decisions:**

- `Arc<RwLock<Registry>>` - one lock guards both the client table and the
  `username → id` index, avoiding any lock-ordering hazard.
- `mpsc::channel` (**bounded**, capacity 256) - a slow/stalled reader is
  evicted via `try_send` instead of buffering without limit (DoS guard, v0.6).
- `next_client_id: AtomicUsize` - lock-free ID allocation.
- `names` index - lets `relay()` unicast a DM to its recipient in O(1);
  anything not routable falls back to `broadcast()`.

### Message Flow

`relay()` inspects each inbound frame and chooses unicast vs. broadcast:

```
                         ┌──────────────────────────────┐
 inbound text frame ───► │ parse minimal Envelope (serde)│
                         └──────────────┬───────────────┘
                                        │
                 AUTH? ─── yes ──► learn username → id, then broadcast
                                        │ no
            recipient set AND           │
            channel not "group:" AND ── yes ──► unicast to that client
            recipient is connected?     │
                                        │ no / unknown / offline
                                        ▼
                                   broadcast to all (except sender)
```

- **DM** (`dm:alice:bob`, `recipient:"bob"`) → unicast to bob only.
- **Group / global / typing / key-exchange**, or a DM to an offline/unknown
  name → broadcast (preserves pre-v0.6 delivery).
- **Unparseable** frame → broadcast (old dumb-relay fallback).

**Important:** The server does NOT echo messages back to the sender.

---

## Module Breakdown

### [`relay.rs`](server/src/relay.rs) - Core Logic

#### `RelayState`

Manages all connected clients, the username index, and message routing.

**Methods:**

- `new()` / `with_limits()` - Create state (latter sets caps for tests)
- `try_register_client()` - Add a client if under `max_clients`; returns ID + receiver
- `set_username()` - Index a client by its AUTH username
- `unregister_client()` - Remove a client and release its username
- `relay()` - Parse one frame and dispatch: unicast DM or broadcast
- `unicast()` - Send to a single client (evicts on full/closed buffer)
- `broadcast()` - Send to all clients except sender (evicts dead clients)
- `client_count()` - Get current connection count

#### `handle_websocket()`

Main WebSocket handler - spawns two tasks per connection:

**Send Task:**

```rust
tokio::spawn(async move {
    while let Some(msg) = broadcast_rx.recv().await {
        ws_tx.send(Message::Text(msg)).await?;
    }
});
```

**Receive Task:**

```rust
tokio::spawn(async move {
    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(Message::Text(text)) => {
                // Oversized frames are dropped (see MAX_MESSAGE_BYTES).
                // relay() parses the frame and decides unicast vs. broadcast.
                state.relay(client_id, text.to_string()).await;
            }
            // ... handle other message types
        }
    }
});
```

**Cleanup:**
Uses `tokio::select!` to wait for either task to finish, then aborts the other and unregisters the client.

### [`main.rs`](server/src/main.rs) - Entry Point

#### Endpoints

| Route     | Method | Purpose                            |
| --------- | ------ | ---------------------------------- |
| `/`       | GET    | HTML status page with client count |
| `/health` | GET    | Simple health check                |
| `/ws`     | GET    | WebSocket upgrade endpoint         |

#### Shuttle Integration

```rust
#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
    // Initialize tracing
    tracing_subscriber::fmt()...

    // Create shared state
    let state = RelayState::new();

    // Build router
    let router = Router::new()
        .route("/", get(root))
        .route("/health", get(health_check))
        .route("/ws", get(ws_handler))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    Ok(router.into())
}
```

#### Local Development

The `local_main()` function provides a non-Shuttle entry point for local testing:

```bash
cargo run --bin zerodrop-local --release
# Listens on 0.0.0.0:8080 by default
```

---

## Running the Server

### Local Development

```bash
# Start server
cargo run --bin zerodrop-local

# Or release mode
cargo build --bin zerodrop-local --release
./target/release/zerodrop-local
```

Server will listen on `http://0.0.0.0:8080`

**Endpoints:**

- WebSocket: `ws://localhost:8080/ws`
- Status Page: `http://localhost:8080`
- Health Check: `http://localhost:8080/health`

### Shuttle Deployment

```bash
cd server
cargo shuttle deploy

# Output will show your deployment URL
# e.g., https://zerodrop-XXXXX.shuttleapp.rs
```

**WebSocket URL:** Replace `https://` with `wss://`

```
wss://zerodrop-XXXXX.shuttleapp.rs/ws
```

---

## Testing End-to-End

### Terminal 1: Start Server

```bash
cargo run --bin zerodrop-local
```

### Terminal 2: Client (Alice)

```bash
cargo run -p zerodrop alice ws://localhost:8080/ws
```

### Terminal 3: Client (Bob)

```bash
cargo run -p zerodrop bob ws://localhost:8080/ws
```

**Expected Behavior:**

1. Both clients connect and see "Connected" status
2. Alice types a message → Bob sees it
3. Bob types a message → Alice sees it
4. Messages are NOT echoed back to sender

---

## Logging

The server uses `tracing` for structured logging:

```bash
# Default (info level)
cargo run --bin zerodrop-local

# Debug level
RUST_LOG=zerodrop_server=debug cargo run --bin zerodrop-local

# Trace level (very verbose)
RUST_LOG=zerodrop_server=trace,tower_http=trace cargo run --bin zerodrop-local
```

**Log Events:**

- Client connections/disconnections
- Message broadcasts
- WebSocket errors
- HTTP requests (via tower_http)

---

## Performance Characteristics

| Metric             | Value                         |
| ------------------ | ----------------------------- |
| Concurrent Clients | Limited by system resources   |
| Message Latency    | <10ms typical (local network) |
| Memory per Client  | ~10KB (channel + state)       |
| CPU Usage          | Minimal (async I/O)           |
| Network            | Non-blocking async            |

---

## Security Model

### What the Server Knows

- Number of connected clients
- Client IDs (internal, not exposed)
- Message sizes (bytes)
- **DM routing metadata**: an in-memory `username → connection` map (learned
  from AUTH) plus the sender/recipient/channel of each routed message. Cleared
  on disconnect; never persisted.

### What the Server Does NOT Know

- Message content (treats payloads as opaque strings; cannot decrypt)
- Message history (no storage, in-memory only)
- Any cryptographic key material

**Philosophy:** The server is a near-dumb pipe — it routes traffic (and since v0.6 unicasts DMs by recipient) but can never read message content.

---

## Error Handling

### Connection Failures

```rust
// Failed sends are logged and client is removed
if let Err(e) = tx.send(msg.content.clone()) {
    warn!("Failed to send to client {}: {}", client_id, e);
    failed_clients.push(client_id);
}
```

### WebSocket Errors

```rust
Err(e) => {
    error!("WebSocket error for client {}: {}", client_id, e);
    break; // Exit receive loop
}
```

**Graceful Degradation:** Failed clients are automatically removed from the roster.

---

## Customization

### Change Port (Local)

Edit `main.rs`:

```rust
let addr = SocketAddr::from(([0, 0, 0, 0], 3000)); // Change 8080 to 3000
```

### Add CORS

Already configured via `tower_http`, but you can customize:

```rust
use tower_http::cors::CorsLayer;

let router = Router::new()
    // ... routes
    .layer(CorsLayer::permissive());
```

### Add Rate Limiting

Use `tower_governor` or similar:

```rust
use tower_governor::{GovernorLayer, GovernorConfigBuilder};

let governor_conf = Box::new(
    GovernorConfigBuilder::default()
        .per_second(10)
        .burst_size(20)
        .finish()
        .unwrap(),
);

let router = Router::new()
    // ... routes
    .layer(GovernorLayer { config: governor_conf });
```

---

## Troubleshooting

### "Address already in use"

```bash
# Find process using port 8080
lsof -i :8080

# Kill it
kill -9 <PID>
```

### Clients can't connect

- Check firewall rules
- Verify WebSocket URL (ws:// not http://)
- Check server logs for errors

### Messages not broadcasting

- Check server logs for client count
- Verify both clients are connected
- Check for WebSocket errors in client

---

## Next Steps

### Production Deployment

1. **Use Shuttle.rs** - Free tier, zero config
2. **Add TLS** - Shuttle provides HTTPS/WSS automatically
3. **Monitor** - Use Shuttle logs or add external monitoring

### Future Enhancements

- **Authentication** - Add token-based auth
- **Rate Limiting** - Prevent spam
- **Message Persistence** - Store history (optional)
- **Rooms/Channels** - Multiple chat rooms
- **Presence** - Track online/offline status

---

## Files Summary

| File                            | Purpose                                       |
| ------------------------------- | --------------------------------------------- |
| [relay.rs](server/src/relay.rs) | WebSocket relay: registry, routing, DoS caps  |
| [main.rs](server/src/main.rs)   | Axum/Shuttle entry point                       |
| [local.rs](server/src/local.rs) | Local (non-Shuttle) dev entry point           |

The relay is intentionally small and dependency-light; all message security is client-side.

---

## Ready for Deployment 🚀

The server is production-ready and can be deployed to Shuttle.rs with a single command.
