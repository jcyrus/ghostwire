# GhostWire Client - Architecture & Usage Guide

## 🏗️ Architecture Overview

The GhostWire client implements a **critical async/sync split pattern** to ensure the UI remains responsive at 60fps while handling network I/O asynchronously.

### Thread Model

```
┌─────────────────────────────────────────────────────────────┐
│                        MAIN THREAD                          │
│                      (Synchronous)                          │
│                                                             │
│  ┌──────────────┐      ┌──────────────┐                   │
│  │   Terminal   │──────│   Ratatui    │                   │
│  │    Events    │      │   Renderer   │                   │
│  └──────────────┘      └──────────────┘                   │
│         │                      │                            │
│         └──────────┬───────────┘                           │
│                    │                                        │
│              ┌─────▼─────┐                                 │
│              │    App    │                                 │
│              │   State   │                                 │
│              └─────┬─────┘                                 │
│                    │                                        │
│         ┌──────────┴──────────┐                           │
│         │                     │                            │
│    ┌────▼────┐          ┌────▼────┐                       │
│    │ event_rx│          │command_tx│                       │
│    └────┬────┘          └────┬────┘                       │
└─────────┼──────────────────────┼──────────────────────────┘
          │                      │
          │  mpsc channels       │
          │                      │
┌─────────┼──────────────────────┼──────────────────────────┐
│    ┌────▼────┐          ┌────▼────┐                       │
│    │ event_tx│          │command_rx│                       │
│    └────┬────┘          └────┬────┘                       │
│         │                     │                            │
│         └──────────┬──────────┘                           │
│                    │                                        │
│              ┌─────▼─────┐                                 │
│              │ WebSocket │                                 │
│              │   Task    │                                 │
│              └───────────┘                                 │
│                                                             │
│                   NETWORK THREAD                            │
│                  (Asynchronous)                            │
│                  tokio::spawn                              │
└─────────────────────────────────────────────────────────────┘
```

### Module Breakdown

#### [`app.rs`](/client/src/app.rs) - Application State

**Purpose:** Core business logic and state management

**Key Components:**

- `WireMessage` - JSON protocol message structure
- `ChatMessage` - Internal message representation
- `User` - User roster entry
- `Telemetry` - Network statistics
- `App` - Main application state

**State Management:**

- Message history (VecDeque, max 1000)
- User roster (Vec, max 100)
- Input buffer with cursor position
- Scroll position tracking
- Connection status

#### [`ui.rs`](/client/src/ui.rs) - Ratatui Rendering

**Purpose:** All UI rendering logic using Ratatui

**Layout:**

```
┌─────────────┬──────────────────────────┬─────────────┐
│   Users     │      GhostWire           │  Telemetry  │
│   (20%)     │      ● CONNECTED         │   (20%)     │
│             │                          │             │
│ ● alice     │ [12:34:56] alice: hi     │ ┌─────────┐ │
│ ● bob       │ [12:35:01] bob: hello    │ │ Uptime  │ │
│ ○ charlie   │ [12:35:15] ⚠ System msg  │ └─────────┘ │
│             │                          │             │
│             │                          │ ┌─────────┐ │
│             │                          │ │ Latency │ │
│             │                          │ └─────────┘ │
│             │                          │             │
│             │                          │ ┌─────────┐ │
│             ├──────────────────────────┤ │  Stats  │ │
│             │ [EDIT] Type message...   │ └─────────┘ │
└─────────────┴──────────────────────────┴─────────────┘
```

**Color Scheme:**

- Primary: Green (`Color::Green`)
- Background: Black (`Color::Black`)
- Alerts: Red (`Color::Red`)
- User messages: Cyan/Yellow
- Borders: Rounded (`BorderType::Rounded`)

#### [`network.rs`](/client/src/network.rs) - WebSocket Layer

**Purpose:** Async network communication

**Key Features:**

- Runs in separate `tokio::spawn` task
- WebSocket client using `tokio-tungstenite`
- Graceful error handling (no `.unwrap()`)
- Automatic reconnection support (future)

**Message Flow:**

```
UI Thread                Network Thread
    │                         │
    │  NetworkCommand         │
    ├────────────────────────>│
    │  (SendMessage)          │
    │                         │
    │                    ┌────▼────┐
    │                    │ Encode  │
    │                    │  JSON   │
    │                    └────┬────┘
    │                         │
    │                    ┌────▼────┐
    │                    │  Send   │
    │                    │   WS    │
    │                    └─────────┘
```

#### [`main.rs`](/client/src/main.rs) - Entry Point

**Purpose:** Orchestrates the async/sync split

**Responsibilities:**

1. Parse CLI arguments
2. Create mpsc channels
3. Spawn network task
4. Initialize terminal
5. Run UI event loop
6. Handle cleanup

---

## 🎮 Keyboard Controls

### Normal Mode (Default)

| Key            | Action                           |
| -------------- | -------------------------------- |
| `i` or `Enter` | Enter edit mode                  |
| `q` or `Esc`   | Quit application                 |
| `j` or `↓`     | Scroll down (one line)           |
| `k` or `↑`     | Scroll up (one line)             |
| `PageDown`     | Scroll down (page)               |
| `PageUp`       | Scroll up (page)                 |
| `G`            | Jump to bottom (latest messages) |
| `g`            | Jump to top (oldest messages)    |
| `h` or `←`     | Navigate to previous channel     |
| `l` or `→`     | Navigate to next channel         |
| `Tab`          | Activate selected channel        |
| `#`            | Jump to global channel           |
| `d`            | Create DM with selected user     |
| `J`            | Select next user                 |
| `K`            | Select previous user             |

### Edit Mode (Typing)

| Key         | Action           |
| ----------- | ---------------- |
| `Esc`       | Exit edit mode   |
| `Enter`     | Send message     |
| `Backspace` | Delete character |
| `←` / `→`   | Move cursor      |
| Any char    | Type character   |

---

## 🚀 Usage

### Running the Client

```bash
# With default username (random ghost_XXXXXXXX)
cargo run -p ghostwire-client

# With custom username
cargo run -p ghostwire-client alice

# With custom username and server URL
cargo run -p ghostwire-client alice ws://example.com:8080/ws
```

### Building Release Binary

```bash
cargo build -p ghostwire-client --release

# Binary location
./target/release/ghostwire
```

### Running Release Binary

```bash
# Default
./target/release/ghostwire

# With username
./target/release/ghostwire alice

# With username and server
./target/release/ghostwire alice ws://localhost:8080/ws
```

---

## 📡 Protocol

All messages use JSON over WebSocket:

```json
{
  "type": "MSG" | "AUTH" | "SYS",
  "payload": "message content",
  "meta": {
    "sender": "username",
    "timestamp": 1234567890
  }
}
```

### Message Types

**MSG** - Regular chat message

```json
{
  "type": "MSG",
  "payload": "Hello, world!",
  "meta": {
    "sender": "alice",
    "timestamp": 1733234567
  }
}
```

**AUTH** - Authentication

```json
{
  "type": "AUTH",
  "payload": "alice",
  "meta": {
    "sender": "alice",
    "timestamp": 1733234567
  }
}
```

**SYS** - System message

```json
{
  "type": "SYS",
  "payload": "alice joined",
  "meta": {
    "sender": "SYSTEM",
    "timestamp": 1733234567
  }
}
```

---

## 🔧 Error Handling

The client follows strict error handling rules:

### ✅ Correct (No Crashes)

```rust
// Network errors are handled gracefully
if let Err(e) = write.send(Message::Text(json)).await {
    let _ = event_tx.send(NetworkEvent::Error {
        message: format!("Failed to send: {}", e),
    });
}
```

### ❌ Incorrect (Will Crash)

```rust
// NEVER use .unwrap() in network code
write.send(Message::Text(json)).await.unwrap();
```

**Philosophy:** The UI must never crash due to network issues. All network errors are converted to `NetworkEvent::Error` and displayed as system messages.

---

## 🎨 Customization

### Changing Colors

Edit [`ui.rs`](/client/src/ui.rs):

```rust
// Change primary color from Green to Cyan
Style::default().fg(Color::Cyan)

// Change alert color from Red to Magenta
Style::default().fg(Color::Magenta)
```

### Adjusting Layout

Edit [`ui.rs`](GhostWire/client/src/ui.rs) `render()` function:

```rust
// Current: 20% | 60% | 20%
Constraint::Percentage(20), // Left
Constraint::Percentage(60), // Middle
Constraint::Percentage(20), // Right

// Example: 15% | 70% | 15%
Constraint::Percentage(15),
Constraint::Percentage(70),
Constraint::Percentage(15),
```

---

## 🐛 Known Limitations

1. **No Reconnection:** Client doesn't auto-reconnect on disconnect (future feature)
2. **No Encryption:** Messages are sent in plaintext (client-side encryption planned)
3. **No Persistence:** Message history is lost on restart
4. **No User Authentication:** Anyone can join with any username

---

## 📊 Performance

- **Target:** 60fps UI rendering
- **Message Capacity:** 1000 messages in memory
- **User Capacity:** 100 users in roster
- **Network:** Non-blocking async I/O
- **Memory:** ~5MB typical usage

---

## 🔜 Next Steps

To complete GhostWire, you need to:

1. **Implement the Server** - Create the relay server in `server/src/`
2. **Test End-to-End** - Run client + server together
3. **Add Encryption** - Implement client-side E2E encryption
4. **Deploy** - Deploy server to Shuttle.rs
