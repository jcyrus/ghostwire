# Feature Implementation Status

## Overview

GhostWire now includes the released v0.4.0 security-story work, in addition to the earlier v0.2.0 and v0.3.0 foundations.

### ✅ v0.4.0 Features

1. **Safety Number Verification UI** - `/verify` and `/confirm` workflow for manual trust establishment
2. **Self-Destruct Command** - `/expire <seconds> <message>` with TTL propagation across the wire
3. **Automatic Key Rotation Trigger** - Periodic checks activate ephemeral key rotation and re-broadcast
4. **Replay Protection** - Nonce tracking rejects replayed DM ciphertext
5. **Per-Message Ratchets** - Send/receive chain ratchets derive unique message keys
6. **Group Message Encryption** - Sender-key distribution and encrypted `group:*` messages

### ✅ v0.2.0 Features (December 2025)

1. **CLI & Help System** - Version flag, enhanced help with examples
2. **Auto-Reconnect** - Exponential backoff reconnection (1s → 16s)
3. **Real-time Latency** - Ping/pong timestamp tracking with RTT display
4. **Typing Indicators** - Throttled typing status with 3s timeout
5. **Configuration System** - TOML config file in ~/.config/ghostwire/
6. **Logging System** - Daily rotating logs with RUST_LOG support
7. **Timestamp Formats** - 24h, 12h, DateTime, Relative ("2m ago")
8. **Enhanced Scrolling** - Line-based scroll, PageUp/Down, word wrapping
9. **Performance Metrics** - FPS and memory usage tracking
10. **Error Recovery** - Categorized errors with troubleshooting hints
11. **Scroll Indicators** - Position counter, "↓ X more", scroll bar

### ✅ v0.1.x Features

1. **Message Timestamps** - Messages display actual server timestamps
2. **User Activity Tracking** - Track when users last sent messages
3. **Last Seen Display** - Show how long ago offline users were active
4. **Connection Uptime** - Real-time tracking of connection duration
5. **Re-authentication Support** - Infrastructure for reconnection scenarios
6. **Idle Status** - Three-state presence (Online/Idle/Offline)

---

## 1. Message Timestamps

### What Changed

Previously, the `timestamp` field in `NetworkEvent::Message` was ignored. Now it's used to display accurate message times.

### Implementation

**[`client/src/main.rs`](client/src/main.rs)**

```rust
NetworkEvent::Message { sender, content, timestamp } => {
    // Convert Unix timestamp to DateTime
    let datetime = chrono::DateTime::from_timestamp(timestamp, 0)
        .unwrap_or_else(|| Utc::now());

    // Create message with actual timestamp
    let mut msg = ChatMessage::new(sender.clone(), content, false);
    msg.timestamp = datetime;

    app.add_message(msg);
}
```

### User Impact

- Messages show the exact time they were sent (from server)
- Timestamps are consistent across all clients
- Fallback to local time if server timestamp is invalid

---

## 2. User Activity Tracking

### What Changed

The `last_seen` field in `User` struct is now actively updated when users send messages.

### Implementation

**[`client/src/app.rs`](client/src/app.rs)**

```rust
/// Update a user's last_seen timestamp
pub fn update_user_activity(&mut self, username: &str) {
    if let Some(user) = self.users.iter_mut().find(|u| u.username == username) {
        user.last_seen = Utc::now();
        user.is_online = true;
    }
}
```

**Called when:**

- User sends a message
- User joins the chat
- Any activity is detected

---

## 3. Last Seen Display

### What Changed

The user roster now shows when offline users were last active.

### Implementation

**[`client/src/ui.rs`](client/src/ui.rs)**

```rust
let last_seen_text = if !user.is_online {
    let duration = Utc::now().signed_duration_since(user.last_seen);
    let mins = duration.num_minutes();
    let hours = duration.num_hours();
    let days = duration.num_days();

    if days > 0 {
        format!(" ({}d ago)", days)
    } else if hours > 0 {
        format!(" ({}h ago)", hours)
    } else if mins > 0 {
        format!(" ({}m ago)", mins)
    } else {
        " (just now)".to_string()
    }
} else {
    String::new()
};
```

### User Impact

**Before:**

```
● alice
○ bob
```

**After:**

```
● alice
○ bob (5m ago)
```

Shows:

- `(just now)` - Less than 1 minute
- `(Xm ago)` - Minutes
- `(Xh ago)` - Hours
- `(Xd ago)` - Days

---

## 4. Connection Uptime Tracking

### What Changed

The telemetry panel now shows accurate connection uptime that increments in real-time.

### Implementation

**[`client/src/main.rs`](client/src/main.rs)**

```rust
// Track uptime
let mut last_uptime_update = Instant::now();

loop {
    // ... render UI ...

    // Update uptime every second
    if last_uptime_update.elapsed() >= Duration::from_secs(1) {
        app.increment_uptime(1);
        last_uptime_update = Instant::now();
    }
}
```

**[`client/src/app.rs`](client/src/app.rs)**

```rust
/// Increment connection uptime (call this periodically)
pub fn increment_uptime(&mut self, seconds: u64) {
    self.telemetry.connection_uptime += seconds;
}
```

### User Impact

The uptime counter in the telemetry panel now updates every second:

```
┌─────────┐
│ Uptime  │
│ 0h 5m 23s│
└─────────┘
```

---

## 5. Re-authentication Support

### What Changed

The `Authenticate` command is now fully implemented for reconnection scenarios.

### Implementation

**[`client/src/network.rs`](client/src/network.rs)**

```rust
NetworkCommand::Authenticate { username: new_username } => {
    let msg = WireMessage {
        msg_type: MessageType::Auth,
        payload: new_username.clone(),
        meta: MessageMeta {
            sender: new_username,
            timestamp: chrono::Utc::now().timestamp(),
        },
    };

    if let Ok(json) = serde_json::to_string(&msg) {
        if let Err(e) = write.send(Message::Text(json)).await {
            let _ = event_tx.send(NetworkEvent::Error {
                message: format!("Failed to authenticate: {}", e),
            });
        }
    }
}
```

### Use Cases

- **Reconnection:** Re-authenticate after network disconnect
- **Username Change:** Change username without restarting (future)
- **Session Resume:** Resume session with new credentials (future)

---

## Future-Ready Methods

---

## v0.4.0 Security Story

### Safety Number Verification

- `/verify <username>` computes a deterministic safety number from both peers' public keys
- `/confirm <username>` marks the peer as trusted
- Verified peers are shown with a `✓` badge in the user roster

### Self-Destructing Messages

- `/expire <seconds> <message>` sends a message with `ttl` metadata
- Local and remote clients render the message with a timer marker and clean it up after expiry

### Replay Protection

- Incoming encrypted DM payloads are base64-decoded
- The nonce prefix is checked against a bounded per-peer history
- Replays are dropped and logged to `security_audit.log`

### Per-Message Ratchets

- Each DM session now maintains send and receive chains
- Every encrypted send/receive advances its respective chain and derives a fresh message key

### Group Message Encryption

- `group:*` channels use sender-key encryption
- Sender keys can be distributed manually with `/groupkey ...`
- The network layer also auto-bootstraps sender-key distribution on first encrypted group send

The following methods are implemented but marked with `#[allow(dead_code)]` for future features:

### `mark_user_offline()`

**Purpose:** Mark users as offline when they disconnect

**Future Use:**

- Server sends disconnect notifications
- Presence tracking system
- Idle timeout detection

### `update_telemetry()`

**Purpose:** Batch update all telemetry data at once

**Future Use:**

- Periodic telemetry sync from server
- Network statistics aggregation
- Performance monitoring

### `update_latency()`

**Purpose:** Update network latency measurement

**Future Use:**

- Ping/pong implementation
- RTT (Round-Trip Time) tracking
- Network quality indicator

---

## Compiler Warnings: Before vs After

### Before (4 warnings)

```
warning: field `last_seen` is never read
warning: method `update_telemetry` is never used
warning: field `timestamp` is never read
warning: variant `Authenticate` is never constructed
```

### After (0 warnings in release mode)

```
Finished `release` profile [optimized] target(s) in 2.99s
```

All warnings resolved! Future-use methods are properly annotated.

---

## Testing the New Features

### 1. Test Message Timestamps

```bash
# Terminal 1: Server
cargo run --bin ghostwire-local

# Terminal 2: Alice
cargo run -p ghostwire-client alice ws://localhost:8080/ws

# Terminal 3: Bob
cargo run -p ghostwire-client bob ws://localhost:8080/ws
```

**Expected:** Messages show accurate timestamps like `[20:45:32]`

### 2. Test Last Seen

1. Alice sends a message
2. Alice quits (`q`)
3. Bob should see: `○ alice (just now)`
4. Wait 5 minutes
5. Bob should see: `○ alice (5m ago)`

### 3. Test Uptime

1. Connect a client
2. Watch the "Uptime" panel in telemetry
3. Should increment every second: `0h 0m 1s`, `0h 0m 2s`, etc.

---

## Code Quality Improvements

### Type Safety

- ✅ All fields now have purpose
- ✅ No unused code warnings
- ✅ Proper error handling throughout

### Performance

- ✅ Uptime tracking uses `Instant` (monotonic clock)
- ✅ Timestamp conversion with fallback
- ✅ Efficient user lookup with `find()`

### Maintainability

- ✅ Clear documentation for future methods
- ✅ Consistent naming conventions
- ✅ Modular implementation

---

## Summary

| Feature                | Status         | Impact                      |
| ---------------------- | -------------- | --------------------------- |
| Message Timestamps     | ✅ Implemented | Accurate time display       |
| User Activity Tracking | ✅ Implemented | Real-time presence          |
| Last Seen Display      | ✅ Implemented | Better UX for offline users |
| Connection Uptime      | ✅ Implemented | Live telemetry              |
| Re-authentication      | ✅ Implemented | Reconnection support        |
| Offline Marking        | 🔜 Future      | Presence system             |
| Latency Tracking       | 🔜 Future      | Ping/pong                   |
| Batch Telemetry        | 🔜 Future      | Performance monitoring      |

**Result:** All previously unused fields are now functional, with infrastructure in place for future enhancements!
