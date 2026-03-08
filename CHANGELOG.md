# Changelog

All notable changes to GhostWire will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.1] - 2026-03-08

### Fixed

- Release workflow now generates SHA256 sidecars with `Get-FileHash`, which works on Windows runners.
- Removed failing Linux ARM64 release target from the matrix to keep release publishing and distribution notifications reliable.

## [0.4.0] - 2026-03-08

### Added

- Safety-number verification commands with `/verify <username>` and `/confirm <username>`.
- Self-destruct message sending via `/expire <seconds> <message>`.
- Sender-key based group encryption for `group:*` channels.

### Changed

- Activated periodic 24-hour key rotation and automatic key re-broadcast.
- Hardened encrypted DM handling with per-message ratchets and replay protection.
- Refreshed release documentation to reflect the v0.4.0 security story shipping on `main`.

## [0.3.0] - 2025-12-09

### Added

- **🔒 End-to-End Encryption (E2EE)**
  - **Cryptographic Stack**:
    - X25519 (ECDH) for key exchange
    - ChaCha20-Poly1305 (AEAD) for message encryption
    - Ed25519 for identity keys (signatures)
    - HKDF-SHA256 for key derivation
    - SHA-256 for safety number fingerprints

  - **Key Management**:
    - Automatic key exchange on connect
    - Ephemeral keys (in-memory only, never persisted)
    - 24-hour automatic key rotation (infrastructure ready)
    - Per-peer session management with timeout cleanup
    - Implementation: `client/src/crypto.rs`, `client/src/keystore.rs`

  - **Message Encryption**:
    - Transparent encryption for DM messages
    - Automatic decryption on receive
    - Fallback to plaintext if no session exists
    - 🔒 lock icon displayed on encrypted messages
    - Implementation: `client/src/network.rs`

  - **Protocol Changes** ⚠️ **BREAKING**:
    - Added `encrypted` boolean field to `WireMessage`
    - Added `recipient` optional field for routing
    - Added `KEY_EXCHANGE` message type
    - Server acts as dumb relay (cannot read encrypted content)
    - Implementation: `client/src/app.rs`

- **Security Audit Logging**
  - Comprehensive logging of all security events
  - Log location: `~/.config/ghostwire/security_audit.log`
  - Events logged:
    - Session establishment/clearing
    - Message encryption/decryption
    - Decryption failures
    - Key rotations
    - Security warnings
  - Implementation: `client/src/security_audit.rs`

- **Self-Destructing Messages**
  - Message expiry with TTL (time-to-live)
  - Automatic cleanup every 5 seconds
  - Secure deletion with memory zeroing (`zeroize` crate)
  - Infrastructure: `ChatMessage::with_expiry()` constructor
  - Implementation: `client/src/app.rs`

- **Documentation**
  - Comprehensive security model documentation: `docs/SECURITY.md`
  - Threat model analysis
  - Best practices for users and developers
  - Known limitations and future roadmap
  - Cryptographic standards compliance

### Changed

- **Message Protocol**: Messages now include encryption metadata
- **Network Layer**: Integrated KeyStore for encryption operations
- **UI**: Messages display encryption status with lock icon
- **App State**: Messages track encryption status and expiry
- **Dependencies**: Added 8 new cryptographic crates

### Security Notes

⚠️ **Important**: This is the initial E2EE release. While using industry-standard cryptography:

- Third-party security audit pending (planned for v1.0.0)
- Only DM messages are encrypted (group encryption in v0.4.0)
- Trust-on-first-use (TOFU) model - no manual key verification UI yet
- Metadata (who talks to whom, when) visible to server
- No replay protection beyond timestamps

See `docs/SECURITY.md` for complete details.

### Compatibility

⚠️ **BREAKING CHANGES**:

- Wire protocol modified (v0.2.0 clients incompatible with v0.3.0 server)
- New required fields in `WireMessage` struct
- Recommendation: Upgrade all clients and server simultaneously

## [0.2.0] - 2025-12-05

### Added

- **CLI Improvements**
  - Version flag: `ghostwire --version` displays current version
  - Enhanced help: `ghostwire --help` with usage examples and keyboard shortcuts
  - Implementation: `client/src/main.rs` using clap 4.4

- **Network Enhancements**
  - Auto-reconnect with exponential backoff (1s → 16s, max 10 attempts)
  - Real-time latency tracking with ping/pong timestamps
  - Connection quality indicator in UI showing RTT in milliseconds
  - Typing indicators with throttling (1/sec) and 3s timeout
  - Implementation: `client/src/network.rs`

- **Configuration System**
  - Configuration file: `~/.config/ghostwire/config.toml`
  - Settings: default server URL, auto-reconnect, timestamp format, typing indicators, log retention
  - Uses confy crate for TOML management
  - Implementation: `client/src/config.rs`

- **Logging System**
  - Daily rotating logs to `~/.config/ghostwire/logs/`
  - RUST_LOG environment variable support
  - File retention management
  - Implementation: `client/src/logging.rs`

- **Enhanced UI Features**
  - Configurable timestamp formats: 24h, 12h, DateTime, Relative ("2m ago")
  - Scroll indicators: position counter, "↓ X more" badge, visual scroll bar
  - Typing indicators: Shows "username is typing..." below input
  - Enhanced scrolling: PageUp/PageDown, 'g' for top, 'G' for bottom
  - Word wrapping for long messages with proper indentation
  - Performance metrics: FPS and memory usage in right sidebar
  - Implementation: `client/src/ui.rs`, `client/src/app.rs`

- **Error Recovery**
  - Categorized errors: Connection, Auth, Network, Config, Terminal, FileSystem
  - User-friendly error messages with troubleshooting hints
  - Severity-based color coding (Info/Warning/Error) with icons (ℹ/⚠/✖)
  - Implementation: `client/src/errors.rs`

### Fixed

- **Scroll Behavior**
  - Fixed messages not appearing after chat fills terminal height
  - Proper line-based scrolling with wrapped message support
  - Auto-scroll only when at/near bottom (within 5 lines)
  - Scroll offset calculation correctly handles visible window
  - Implementation: `client/src/ui.rs`, `client/src/app.rs`

### Changed

- Scroll controls: j/k now scroll by 3 lines for better responsiveness
- PageUp/PageDown scroll by 20 lines
- Updated all documentation with new keyboard shortcuts

## [0.1.2] - 2025-12-04

### Fixed

- **WebSocket Connection Stability**: Implemented bidirectional heartbeat mechanism to prevent connection timeouts
  - Implementation: `client/src/network.rs`, `server/src/relay.rs`
  - Impact: Connections now stay alive indefinitely during idle periods. Fixes "WebSocket protocol error: C" disconnections that occurred after 30-60 seconds of inactivity
  - Root Cause: Intermediate proxies and load balancers (including Shuttle's infrastructure) were timing out idle WebSocket connections due to lack of traffic
  - Solution: Both client and server now send ping frames every 30 seconds and properly respond to ping/pong messages, keeping the connection active even when users aren't chatting

- **Default Server URL**: Changed client default from `ws://localhost:8080/ws` to `wss://ghost.jcyrus.com/ws`
  - Implementation: `client/src/main.rs`
  - Impact: Users can now connect without specifying a server URL. Running `ghostwire username` now connects to production by default instead of failing with "Connection refused"
  - Root Cause: The hardcoded localhost default was intended for development but caused confusion for end users

### Added

- **User Idle Status**: Users now display idle status when inactive for more than 5 minutes
  - Implementation: `client/src/app.rs`, `client/src/ui.rs`
  - Impact: Three-state presence system provides better awareness of user activity
  - Visual Indicators:
    - `●` Green: Online and active (sent message within last 5 minutes)
    - `◐` Yellow: Idle (connected but no activity for 5+ minutes, shows "idle Xm")
    - `○` Gray: Offline (disconnected, shows time since last seen)
  - Helps users identify who is actively chatting versus who is just connected

- **Usage Documentation**: Added comprehensive usage section to README
  - Examples for connecting to default server, custom servers, and local development
  - Complete keyboard controls reference
  - Impact: Users now have clear instructions on how to use the client after installation

- **Windows Installer**: PowerShell installation script for Windows users
  - Implementation: `install.ps1` with automatic PATH configuration
  - Server route: `/install.ps1` redirects to raw GitHub script
  - Impact: Windows users can now install with one-liner: `irm https://ghost.jcyrus.com/install.ps1 | iex`
  - Automatically adds installation directory to user PATH
  - Includes instructions for refreshing PATH in current session

## [0.1.1] - 2025-12-03

### Added

- **One-Liner Installation**: New "Hacker" install command `curl -sL https://ghost.jcyrus.com/install | bash`.
- **Install Script**: Robust `install.sh` with OS (Linux/macOS) and Architecture (x64/arm64) detection.
- **Cross-Platform Builds**: GitHub Actions workflow to automatically build and release binaries for Linux, macOS, and Windows.
- **Server Redirect**: Added `/install` route to server to redirect to the raw install script.

### Changed

- **Shuttle Dependencies**: Updated `shuttle-runtime` and `shuttle-axum` to v0.50.0.
- **README**: Updated with new installation instructions and dynamic status badge.

## [0.1.0] - 2025-12-03

### Added

- **Multi-Channel System**: Support for global chat and direct messages (DMs)
  - Global channel (`# global`) for public conversations
  - Direct message channels (`@ username`) for private 1-on-1 chats
  - Auto-creation of DM channels when receiving messages
  - Channel switching with keyboard shortcuts (`h/l` + `Tab`)
  - Unread message count badges on channels
- **Enhanced Telemetry Panel**: Real-time statistics and monitoring
  - Dynamic network activity chart (last 60 seconds)
  - Connection uptime tracking
  - Latency gauge with color-coded status
  - Message throughput statistics
  - Active channel display
  - User and channel count
  - Server time display (UTC)
- **User Discovery**: Automatic user roster population
  - Users appear when they send messages
  - Online/offline status indicators
  - Last seen timestamps for offline users
  - User selection for DM creation (`J/K` keys)
- **Client Architecture**: Async/sync split design
  - Main thread handles UI rendering (60fps target)
  - Separate async task for WebSocket communication
  - `mpsc` channels for thread-safe message passing
- **Server Implementation**: Dumb relay pattern
  - WebSocket-based message broadcasting
  - Connection management and client tracking
  - Shuttle.rs deployment support
  - Local development mode
- **TUI Features**: Ratatui-based interface
  - Three-panel layout (channels, chat, telemetry)
  - Vim-style keyboard navigation
  - Message timestamps
  - System message support
  - Scrollable chat history
  - Input mode with cursor support

### Technical Details

- **Protocol**: JSON-based wire format with channel routing
- **Dependencies**: Tokio for async runtime, Ratatui for TUI, Axum for server
- **Workspace**: Monorepo structure with client and server packages
- **Build**: Rust 2021 edition, clippy-clean with strict warnings

### Documentation

- Comprehensive README with ASCII art logo
- Client architecture documentation (`CLIENT.md`)
- Server deployment guide (`SERVER.md`)
- Quick start guide (`QUICKSTART.md`)
- Channel system user guide (`CHANNELS.md`)
- Feature implementation details (`FEATURES.md`)

### Known Limitations

- No encryption (messages are plain JSON)
- No message persistence (ephemeral chat)
- No group channels yet (reserved for future)
- Server broadcasts all messages to all clients (no server-side filtering)

[Unreleased]: https://github.com/jcyrus/GhostWire/compare/v0.4.1...HEAD
[0.4.1]: https://github.com/jcyrus/GhostWire/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/jcyrus/GhostWire/compare/v0.3.2...v0.4.0
[0.1.2]: https://github.com/jcyrus/GhostWire/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/jcyrus/GhostWire/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/jcyrus/GhostWire/releases/tag/v0.1.0
