<div align="center">

![GhostWire Logo](docs/logo.jpg)

[![Status](https://img.shields.io/website?url=https%3A%2F%2Fghostwire-ardt.shuttle.app%2Fhealth&label=Relay%20Status&style=for-the-badge&color=success)](https://ghostwire-ardt.shuttle.app)

**The server knows nothing. The terminal is everything.**

[View Demo (Coming Soon)] | [Report Bug (Coming Soon)] | [Request Feature (Coming Soon)]

</div>

---

## 📡 Transmission Incoming

**GhostWire** is a secure, ephemeral TUI chat client for those who prefer keyboards over mouse clicks. Built with **Rust** and **Ratatui**, it combines the aesthetic of a cyberpunk system monitor with the privacy of a dead-drop.

### 📸 Visual Recon

![GhostWire Screenshot](docs/screenshot.jpg)

---

## ⚡ Core Systems

| Feature                        | Description                                                                                                 |
| :----------------------------- | :---------------------------------------------------------------------------------------------------------- |
| **👻 Ephemeral Relay**         | The relay server is a "dumb broadcast." It routes traffic without storing or processing it.                 |
| **🛡️ End-to-End Encryption**   | Direct messages are encrypted automatically with X25519 + ChaCha20-Poly1305 and per-message ratchets.       |
| **🔐 Verification & Ephemera** | Safety-number verification, self-destructing messages, replay protection, and key rotation are implemented. |
| **🖥️ High-Fidelity TUI**       | Built on `Ratatui`. Supports mouse capture, resizing, and custom themes.                                    |
| **🚀 Blazing Fast**            | Written in Async Rust (`Tokio`). Minimal footprint, maximum throughput.                                     |
| **🎨 Cyberpunk Aesthetics**    | Detailed telemetry, network activity charts, and real-time statistics.                                      |

---

## 💾 Initialize Uplink (Installation)

### macOS

#### 🍺 Homebrew (Recommended)

```bash
brew tap jcyrus/tap
brew install ghostwire
```

#### ⚡️ Quick Install Script

```bash
curl -sL https://ghost.jcyrus.com/install | bash
```

### Windows

#### 📦 Scoop (Recommended)

```powershell
scoop bucket add jcyrus https://github.com/jcyrus/scoop-bucket
scoop install ghostwire
```

#### ⚡️ Quick Install Script

```powershell
irm https://ghost.jcyrus.com/install.ps1 | iex
```

> **Note:** After installation, you may need to restart your terminal for the PATH changes to take effect. If `ghostwire` is not recognized, run:
>
> ```powershell
> $env:Path = [System.Environment]::GetEnvironmentVariable('Path','User')
> ```

### Linux

#### ⚡️ Quick Install Script

```bash
curl -sL https://ghost.jcyrus.com/install | bash
```

### 📦 Manual Installation

#### Prerequisites

- **Rust Toolchain:** 1.70+ (2021 Edition)
- **Terminal:** Support for TrueColor (NerdFonts recommended for icons)

#### Compile from Source

Clone the repository and build the binary:

```bash
git clone https://github.com/jcyrus/GhostWire.git
cd ghostwire

# Build the client only (The part you use)
cargo build --release -p ghostwire-client

```

### 🔄 Updating GhostWire

| Method                   | Update Command                                          |
| :----------------------- | :------------------------------------------------------ |
| **Homebrew (macOS)**     | `brew upgrade ghostwire`                                |
| **Scoop (Windows)**      | `scoop update ghostwire`                                |
| **Quick Install Script** | Re-run the installation command                         |
| **Manual (from source)** | `git pull && cargo build --release -p ghostwire-client` |

**Check your current version:**

```bash
ghostwire --version
```

---

## 🚀 Usage

After installation, connect to the public relay:

```bash
# Connect with your username (connects to wss://ghost.jcyrus.com/ws by default)
ghostwire your_username

# Or connect to a custom server
ghostwire your_username wss://your-server.com/ws

# For local development (requires local server running)
ghostwire your_username ws://localhost:8080/ws
```

### Controls

**Message Mode:**

- **`i` or `Enter`**: Enter message mode
- **`Esc`**: Exit message mode
- **`Enter`**: Send message

**Security Commands:**

- **`/verify <username>`**: Show a peer's safety number for out-of-band comparison
- **`/confirm <username>`**: Mark a peer as trusted after verifying the safety number
- **`/expire <seconds> <message>`**: Send a self-destructing message with TTL
- **`/groupkey <group> <user1,user2,...>`**: Manually distribute a sender key for a group channel

**Navigation:**

- **`Esc` or `q`**: Quit (in normal mode)
- **`h/l` or `←/→`**: Navigate channels
- **`Tab`**: Activate selected channel
- **`#`**: Jump to global channel
- **`d`**: Create DM with selected user
- **`J/K`**: Select user (for DM creation)

**Scrolling:**

- **`j/k` or `↓/↑`**: Scroll down/up (one line)
- **`PageDown/PageUp`**: Scroll down/up (page)
- **`G`**: Jump to bottom (latest messages)
- **`g`**: Jump to top (oldest messages)

---

## ☁️ Deployment (Host Your Own Relay)

Want to create a private network for your friends? Spin up the "Dumb Relay" in seconds.

### Option A: The "One-Click" (Shuttle.rs)

No config required. Perfect for free tier hosting.

```bash
cd server
cargo shuttle deploy
# Copy the URL provided (e.g., wss://ghostwire.shuttleapp.rs)
```

### Option B: Local / VPS

```bash
# For local development
cd server
cargo run --bin ghostwire-local
# Listens on 0.0.0.0:8080 by default
```

---

## 📂 Documentation

For detailed technical documentation, see the [`docs/`](docs/) directory:

- **[Client Architecture](docs/CLIENT.md)** - Async/sync split, module breakdown
- **[Server Architecture](docs/SERVER.md)** - Relay pattern, deployment
- **[Feature Details](docs/FEATURES.md)** - Implementation specifics
- **[Local Development](docs/LOCAL_DEV.md)** - Development setup

<details>
<summary><strong>🔎 Click to expand Technical Internals</strong></summary>

### The Stack

- **Client:** `Ratatui` (UI), `Tokio` (Async), `Tungstenite` (WebSockets)
- **Server:** `Axum` (Http), `Shuttle` (Infra)

### The Threading Model (Critical)

To ensure the UI never freezes at 60fps, we use a strict Actor-model separation:

1.  **Main Thread:** Synchronous. Handles drawing the UI and capturing keystrokes.
2.  **Network Task:** Asynchronous. Runs on `Tokio`. Handles the WebSocket stream.
3.  **Bridge:** `mpsc::unbounded_channel` passes messages between the two worlds.

### The Protocol (JSON)

```json
{
  "type": "MSG",
  "payload": "EncryptedBase64String...",
  "channel": "dm:alice:bob",
  "meta": {
    "sender": "User_Hash_ID",
    "timestamp": 171542100
  },
  "recipient": "bob",
  "ttl": 60
}
```

</details>

---

## 🤝 Contributing (Join the Network)

We welcome all hackers, cypherpunks, and Rustaceans.

1.  Fork the Project
2.  Create your Feature Branch (`git checkout -b feature/MatrixRain`)
3.  Commit your Changes (`git commit -m 'Add Matrix rain effect'`)
4.  Push to the Branch (`git push origin feature/MatrixRain`)
5.  Open a Pull Request

## 📄 License

Distributed under the MIT License. See `LICENSE` for more information.

---

<div align="center">
<sub>Built with 🦀 and ☕ by jCyrus</sub>
</div>
