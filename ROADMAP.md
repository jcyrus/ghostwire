# GhostWire Roadmap

**Vision**: A secure, ephemeral, terminal-based communication platform with zero-trust architecture and end-to-end encryption.

**Current Version**: v0.6.0
**Last Updated**: 2026-05-31

---

## ✅ Completed Releases

### v0.2.0 - Developer Experience & Core Utilities

**Released**: December 2025

- [x] Version flag, help command, auto-reconnect
- [x] Connection quality indicator, typing indicators
- [x] Message timestamps, scroll indicators, unread counts
- [x] Configuration file, logging system, error recovery
- [x] Performance metrics (FPS, memory)
- [x] Complete documentation

---

### v0.3.0 - Security Foundation (E2EE)

**Released**: December 2025

#### End-to-End Encryption

- [x] **X25519 Key Exchange** — ECDH for deriving shared secrets
- [x] **ChaCha20-Poly1305 Encryption** — AEAD for message confidentiality
- [x] **Automatic Key Distribution** — Public keys broadcast on connect
- [x] **Session Management** — Per-peer ephemeral sessions
- [x] **Transparent Encryption** — DMs encrypted automatically

#### Zero-Trust Architecture

- [x] **Client-Side Encryption** — All encryption happens in the client
- [x] **Server Blindness** — Server sees only encrypted blobs
- [x] **No User Database** — Server stores nothing
- [x] **Ephemeral Keys** — In-memory only, never persisted
- [x] **24-Hour Key Rotation** — Trigger activation in v0.4.0

#### Security Features

- [x] **Self-Destructing Messages** — TTL-based expiry infrastructure (UI in v0.4.0)
- [x] **Secure Deletion** — Memory zeroing with `zeroize` crate
- [x] **Audit Logs** — Comprehensive security event logging (9 event types)
- [x] **Security Indicators** — 🔒 icon for encrypted messages
- [x] **Safety Numbers** — SHA-256 fingerprints computed; verification UI in v0.4.0

---

### v0.4.0 - Complete the Security Story

**Released**: March 2026

#### Finishing v0.3.0 Infrastructure

- [x] **Safety Number Verification UI** — `/verify <username>` and `/confirm <username>`
- [x] **Self-Destruct UI Command** — `/expire <seconds> <message>` with TTL cleanup loop
- [x] **Key Rotation Trigger** — Periodic 24h checks now activate `rotate_ephemeral_key()` and re-broadcast

#### Encryption Hardening

- [x] **Symmetric Chain Ratchet** — Each DM session advances send/receive chains via HKDF, giving a unique message key per message. _Note: this is a symmetric ratchet, not the full DH Double Ratchet — sessions do not self-heal after key compromise (addressed in v0.7.0)._
- [x] **Replay Protection** — Per-session nonce tracking rejects replayed DM ciphertext; writes audit log events
- [x] **Group Message Encryption** — Sender-key-based E2EE for `group:*` channels with auto-bootstrap distribution

---

### v0.5.0 - Rich Messaging & UI/UX Evolution

**Released**: March 2026

#### Message Formatting

- [x] **Markdown Support** — Bold, italic, inline code, block quotes, fenced code blocks with syntax highlighting
- [x] **Action Commands** — `/me <action>` with italic magenta rendering
- [x] **Emoji Reactions** — `/react <emoji>` with aggregated per-message counts and quick-react `r` shortcut

#### UI/UX

- [x] **Procedural User Colors** — Deterministic username colors derived from peer X25519 public keys
- [x] **Command Palette** — Dedicated `InputMode::Command` with inline hints and cyan border
- [x] **Focus Mode** — `F10` toggles the telemetry sidebar for a wider chat view

#### Bug Fixes

- [x] **Unicode-safe input handling** — Cursor movement, insertion, and backspace are now char-boundary-aware; emoji in `/react` and chat input no longer panic

---

### v0.5.1 - Reliability Fixes

**Released**: March 2026

- [x] **Session bootstrap recovery** — Re-sends targeted key exchange when a peer joins late, preventing one-way encrypted DM failures
- [x] **DM recipient parsing** — Correctly resolves the peer from `dm:user1:user2` channel IDs instead of treating the prefix as a username
- [x] **Status page routing** — Derives the public WebSocket endpoint from forwarded request headers for hosted deployments

---

### v0.5.2 - Concurrency & Protocol Hardening

**Released**: May 2026

- [x] **Mutex poisoning recovery** — Heartbeat and audit logger paths recover gracefully rather than panicking
- [x] **DM channel canonicalization** — `dm:bob:alice` is normalized to `dm:alice:bob` at insertion, preventing duplicate channels and session mismatches
- [x] **`Arc<str>` broadcast** — Single allocation per outbound frame regardless of connected client count
- [x] **Lock correctness** — Replaced bare mutexes with `Arc<RwLock<_>>` where concurrent reads were safe

---

### v0.6.0 - Relay Hardening & TOFU Key Detection

**Released**: May 2026

#### Server

- [x] **Relay DoS guards** — Bounded per-client outbound channel (256 messages), `try_send` eviction of slow/stalled readers, 10,000-client connection cap, 64 KiB inbound frame limit
- [x] **DM unicast routing** — Relay parses a minimal `Envelope` and unicasts DMs to the named recipient (O(N) → O(1) bandwidth); unknown/offline/group/unparseable frames fall back to broadcast
- [x] **Username index hardening** — `set_username` guards against orphan entries on evicted clients, stale entries on re-AUTH name changes, and name hijacking by live impostors

#### Client

- [x] **TOFU peer key-change detection** — `KeyStore` remembers the first-seen public key per peer; a verified peer's key change triggers a loud in-app warning and resets the verified status; unverified key changes are audit-logged quietly

---

## 🔐 v0.7.0 — Post-Compromise Security

**Theme**: Close the remaining cryptographic gap — full DH Double Ratchet

The symmetric chain ratchet (v0.4.0) gives unique per-message keys but no *post-compromise security*: a leaked chain key exposes all subsequent messages until the next 24-hour rotation. This release adds the missing Diffie-Hellman ratchet step so sessions self-heal after a key compromise, matching the Signal security model for DMs.

### Cryptography

- [ ] **DH Double Ratchet for DMs** — Add an X25519 DH ratchet step to the existing symmetric chain so the root key evolves per message, giving post-compromise security without breaking the existing key exchange flow
- [ ] **Group forward secrecy** — Ratchet group sender keys per message (`group:*` channels); currently sender keys are static for the session lifetime

### Server

- [ ] **Per-IP rate limiting** — `tower_governor` middleware; cap connection attempts and AUTH floods per source IP, handling `X-Forwarded-For` / `Fly-Client-IP` correctly behind Shuttle's proxy _(deferred from v0.6.0)_

### Reliability

- [ ] **Relay-level delivery receipts** — Relay acknowledges frame delivery to sender so the client can distinguish "sent" from "delivered to recipient's channel" (no server-side message storage; purely in-flight state)

---

## 👥 v0.8.0 — Groups, Collaboration & Operations

**Theme**: Multi-user collaboration, IRC-style channels, and self-hosting

### Group Channels

- [ ] **IRC-style routing** — `/join #channel`, `/leave #channel`, `/invite @user` workflows
- [ ] **Named group channels** — Multi-user group chats beyond global
- [ ] **Decentralized channel operators** — `@` status for channel creators with local `/kick` and `/ban` (enforced via client-side ignore lists)
- [ ] **Group invites** — Shareable invite links

### Messaging

- [ ] **Message editing** — Edit a sent message within a time window
- [ ] **Message threading** — Reply to a specific message (UUIDs already on `ChatMessage`)
- [ ] **Search** — Full-text search over local message history

### Enhanced Presence

- [ ] **Custom status** — Set a custom status message
- [ ] **Do Not Disturb** — Suppress notifications
- [ ] **Away auto-detection** — Set away status after configurable inactivity period

### Peer-to-Peer

- [ ] **DCC file transfers** — P2P encrypted file transfers that bypass the relay entirely

### Operations & Self-Hosting

- [ ] **Prometheus metrics** — Relay health: connection count, message throughput, eviction rate
- [ ] **Graceful shutdown** — Drain in-flight messages before relay exits
- [ ] **Docker support** — Official multi-arch images with a Docker Compose one-command stack
- [ ] **On-premise guide** — Self-hosted deployment hardening checklist

---

## 🔏 v0.9.0 — Advanced Privacy

**Theme**: Minimize what the server knows

_Addresses the metadata-visibility gap documented in `docs/user/SECURITY.md`._

- [ ] **Sealed sender** — Hide sender identity from the relay (Signal-style `SealedSender`)
- [ ] **Message padding** — Uniform ciphertext sizes to prevent traffic analysis
- [ ] **Metadata minimization** — Reduce server-visible routing metadata beyond current routing requirements
- [ ] **Tor integration** — Optional onion routing for transport-layer anonymity
- [ ] **Session resumption** — Reconnect without full key re-exchange
- [ ] **Multi-device identity** — Share identity keypair across multiple terminals
- [ ] **Offline message queuing** — TTL-bounded encrypted store-and-forward _(deliberate server-knowledge trade-off, like the v0.6 DM routing trade-off; opt-in for self-hosters)_

---

## 🌟 v1.0.0 — Production Hardening

**Theme**: Audited, documented, production-ready

### Security

- [ ] **Third-party cryptographic audit** — Professional review of crypto implementation
- [ ] **Penetration testing** — Offensive security engagement
- [ ] **Bug bounty program** — Reward responsible disclosure
- [ ] **Reproducible builds** — Verifiable, deterministic binaries

### Quality

- [ ] **Load testing** — Benchmark relay under concurrent connections
- [ ] **Fuzzing** — `cargo-fuzz` on message parsing and crypto paths
- [ ] **Long-term soak testing** — Multi-day stability runs

### Documentation & Community

- [ ] **Complete API documentation** — Full developer reference for bot/library authors
- [ ] **Migration guides** — Upgrade paths for all prior versions
- [ ] **Best practices** — Security hardening guide for self-hosters

### Web Access

- [ ] **WASM web client** — Browser-based client compiled from the same Rust codebase

---

## 🔭 Future Exploration (Post-1.0)

_Research-stage ideas aligned with the project's privacy-first identity. No commitments._

- **Noise Protocol Framework** — Replace raw X25519 with the full Noise handshake (WireGuard, Signal)
- **Post-Quantum Cryptography** — Hybrid classical + ML-KEM (Kyber) for quantum resistance
- **Federated Relay Network** — Interoperable self-hosted relays (Matrix-style)
- **Zero-Knowledge Identity** — Prove group membership without revealing username
- **Desktop Notifications** — OS-level alerts for mentions (macOS/Linux, opt-in)

---

## 📊 Success Metrics

### Technical Metrics

- **Uptime**: 99.9% relay uptime
- **Latency**: <100ms message delivery (P95)
- **Security**: Zero plaintext stored server-side

### Community Metrics

- **Contributors**: 50+ contributors by v1.0.0
- **Stars**: 5K+ GitHub stars
- **Forks**: 500+ forks

---

## 🤝 How to Contribute

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines on:

- Picking tasks from this roadmap
- Proposing new features
- Submitting pull requests
- Reporting bugs

---

## 📝 Notes

- **Priorities may shift** based on user feedback and security concerns
- **Security features take precedence** over convenience features
- **Breaking changes** will be clearly communicated with migration guides
- **Anything contradicting the ephemeral/zero-trust model** will not be added to core without explicit documentation of the trade-off

---

**Last Updated**: 2026-05-31
**Maintained By**: @jcyrus
**License**: MIT
