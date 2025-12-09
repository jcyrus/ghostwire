# GhostWire - Quick Start Guide

## 🚀 Running GhostWire

### Prerequisites

- Rust 1.70+ installed
- Terminal with TrueColor support

---

## Local Testing (2 Clients + Server)

### Step 1: Start the Server

Open a terminal and run:

```bash
cd .
cargo run --bin ghostwire-local
```

You should see:

```
INFO ghostwire_server: 🚀 Starting GhostWire Relay Server (Local Mode)
INFO ghostwire_server: 👻 GhostWire Relay listening on http://0.0.0.0:8080
INFO ghostwire_server: 📡 WebSocket endpoint: ws://0.0.0.0:8080/ws
```

### Step 2: Start Client #1 (Alice)

Open a **new terminal** and run:

```bash
cd .
cargo run -p ghostwire-client alice ws://localhost:8080/ws
```

You should see the TUI with:

- Green "● CONNECTED" status
- Welcome message: "Welcome to GhostWire, alice!"

### Step 3: Start Client #2 (Bob)

Open **another terminal** and run:

```bash
cd .
cargo run -p ghostwire-client bob ws://localhost:8080/ws
```

You should see the TUI with:

- Green "● CONNECTED" status
- Welcome message: "Welcome to GhostWire, bob!"
- System message: "alice joined the chat"

---

## Using the Client

### Keyboard Controls

**Normal Mode (Default):**

- `i` or `Enter` - Enter edit mode (start typing)
- `q` or `Esc` - Quit application
- `j` or `↓` - Scroll down (one line)
- `k` or `↑` - Scroll up (one line)
- `PageDown` - Scroll down (page)
- `PageUp` - Scroll up (page)
- `G` - Jump to bottom (latest messages)
- `g` - Jump to top (oldest messages)
- `h/l` or `←/→` - Navigate channels
- `Tab` - Activate selected channel
- `#` - Jump to global channel
- `d` - Create DM with selected user
- `J/K` - Select user (for DM creation)

**Edit Mode (Typing):**

- `Esc` - Exit edit mode (back to normal)
- `Enter` - Send message
- `Backspace` - Delete character
- `←` / `→` - Move cursor
- Any character - Type

### Sending Messages

1. Press `i` to enter edit mode (border turns yellow)
2. Type your message
3. Press `Enter` to send
4. Message appears in both clients!

---

## Expected Behavior

### When Alice Sends a Message

**Alice's Screen:**

```
[12:34:56] alice: Hello, Bob!
```

**Bob's Screen:**

```
[12:34:56] alice: Hello, Bob!
```

**Server Logs:**

```
DEBUG ghostwire_server::relay: Client 0 sent: 123 bytes
```

### When Bob Replies

**Bob's Screen:**

```
[12:34:56] alice: Hello, Bob!
[12:35:01] bob: Hi Alice!
```

**Alice's Screen:**

```
[12:34:56] alice: Hello, Bob!
[12:35:01] bob: Hi Alice!
```

---

## Troubleshooting

### "Connection refused"

- Make sure the server is running first
- Check that you're using `ws://localhost:8080/ws` (not `http://`)

### "Address already in use"

- Another process is using port 8080
- Kill it: `lsof -i :8080` then `kill -9 <PID>`
- Or change the port in `server/src/main.rs`

### Messages not appearing

- Check server logs for errors
- Verify both clients show "● CONNECTED"
- Try restarting both clients

### UI looks broken

- Make sure your terminal supports TrueColor
- Try resizing the terminal window
- Use a modern terminal (iTerm2, Alacritty, etc.)

---

## Deployment to Shuttle

### Deploy the Server

```bash
cd server
cargo shuttle deploy
```

You'll get a URL like: `https://ghostwire-xxxxx.shuttleapp.rs`

### Connect Clients to Deployed Server

```bash
# Replace with your actual Shuttle URL
cargo run -p ghostwire-client alice wss://ghostwire-xxxxx.shuttleapp.rs/ws
```

**Note:** Use `wss://` (secure WebSocket) for Shuttle deployments.

---

## Building Release Binaries

### Client

```bash
cargo build -p ghostwire-client --release
./target/release/ghostwire alice ws://localhost:8080/ws
```

### Server

```bash
cargo build -p ghostwire-server --release
./target/release/ghostwire-server
```

---

## Next Steps

- Read [`docs/CLIENT.md`](docs/CLIENT.md) for detailed client architecture
- Read [`docs/SERVER.md`](docs/SERVER.md) for server deployment guide
- Customize the UI colors in `client/src/ui.rs`
- Add encryption (future feature)

---

## Quick Reference

| Command                                      | Description                |
| -------------------------------------------- | -------------------------- |
| `cargo run --bin ghostwire-local`            | Start server locally       |
| `cargo run -p ghostwire-client <name>`       | Start client with username |
| `cargo run -p ghostwire-client <name> <url>` | Connect to custom server   |
| `cargo build --release`                      | Build optimized binaries   |
| `cd server && cargo shuttle deploy`          | Deploy to Shuttle          |

---

**Enjoy your secure, hacker-themed chat! 👻**
