# GhostWire Roadmap

**Vision**: A secure, ephemeral, terminal-based communication platform with zero-trust architecture and end-to-end encryption.

**Current Version**: v0.4.0
**Last Updated**: 2026-03-08

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

### v0.3.0 - Security Foundation (E2EE)

**Released**: December 2025 🎉

#### End-to-End Encryption

- [x] **X25519 Key Exchange** - ECDH for deriving shared secrets ✅
- [x] **ChaCha20-Poly1305 Encryption** - AEAD for message confidentiality ✅
- [x] **Automatic Key Distribution** - Public keys broadcast on connect ✅
- [x] **Session Management** - Per-peer ephemeral sessions ✅
- [x] **Transparent Encryption** - DMs encrypted automatically ✅

#### Zero-Trust Architecture

- [x] **Client-Side Encryption** - All encryption in client ✅
- [x] **Server Blindness** - Server sees only encrypted blobs ✅
- [x] **No User Database** - Server stores nothing ✅
- [x] **Ephemeral Keys** - In-memory only, never persisted ✅
- [x] **24-Hour Key Rotation** - Infrastructure ready (`needs_rotation()`, `rotate_ephemeral_key()`) — trigger activation in v0.4.0 ⚠️

#### Security Features

- [x] **Self-Destructing Messages** - TTL-based expiry with secure deletion (infrastructure ready, user command in v0.4.0) ⚠️
- [x] **Secure Deletion** - Memory zeroing with zeroize crate ✅
- [x] **Audit Logs** - Comprehensive security event logging (9 event types) ✅
- [x] **Security Indicators** - 🔒 icon for encrypted messages ✅
- [x] **Safety Numbers** - SHA-256 fingerprints computed; verification UI in v0.4.0 ⚠️

#### Documentation

- [x] **Security Model** - Complete threat analysis in `docs/SECURITY.md` ✅
- [x] **Breaking Changes** - Protocol v0.3.0 incompatible with v0.2.0 ✅

**See**: `CHANGELOG.md` for details, `docs/SECURITY.md` for security model

---

### v0.4.0 - Complete the Security Story

**Released**: March 2026

_Finishes what v0.3.0 started — every feature below has infrastructure already in place._

### Complete v0.3.0 Infrastructure

- [x] **Safety Number Verification UI** - `/verify <username>` displays safety numbers and `/confirm <username>` marks trust
- [x] **Self-Destruct UI Command** - `/expire <seconds> <message>` sends TTL-tagged messages and pairs with the cleanup loop
- [x] **Key Rotation Trigger** - Periodic 24h rotation checks now activate `rotate_ephemeral_key()` and re-broadcast fresh keys

### Encryption Hardening

- [x] **Double Ratchet Algorithm** - Per-message forward secrecy via send/receive chain ratchets derived from the HKDF chain key
- [x] **Replay Protection** - Nonce tracking rejects replayed DM ciphertext and writes audit log events
- [x] **Group Message Encryption** - Sender-key-based group E2EE implemented for `group:*` channels with auto-bootstrap distribution

---

## 💬 Rich Messaging & UI/UX Evolution (v0.5.0)

**Theme**: Expressive Terminal Communication

### Message Formatting

- [ ] **Code Blocks** - Syntax-highlighted code snippets in chat
- [ ] **Markdown Support** - Bold, italic, inline code, block quotes
- [ ] **Action Commands** - Support for the classic IRC `/me` command with unique italicized visual rendering
- [ ] **Reactions** - Emoji reactions to messages
- [ ] **Message Editing** - Edit a sent message within a time window
- [ ] **Message Threading** - Reply to a specific message (UUIDs already on `ChatMessage`)

### UI/UX

- [ ] **Procedural User Colors** - Generate unique, consistent colors for usernames based on their cryptographic identity
- [ ] **Command Palette** - A dedicated input mode for `/` commands to separate them from standard chat
- [ ] **Focus Mode** - A keyboard toggle (`F10`) to collapse the telemetry sidebar for a wider chat view

### Discovery & History

- [ ] **Search** - Full-text search over local message history
- [ ] **Pinned Messages** - Pin important messages in a channel
- [ ] **Jump to Date** - Navigate history by timestamp

---

## 👥 Groups & the IRC Era (v0.6.0)

**Theme**: Multi-User Collaboration Meets Classic IRC

### Group Channels

- [ ] **Classic IRC Routing** - Implement `/join #channel` and `/invite @user` workflows
- [ ] **Named Group Channels** - Multi-user group chats beyond global
- [ ] **Decentralized Channel Operators** - Implement the `@` status for channel creators, with local `/kick` and `/ban` capabilities (enforced client-side via ignore lists)
- [ ] **Group Invites** - Shareable invite links for groups

### Peer-to-Peer

- [ ] **Direct Client-to-Client (DCC)** - P2P encrypted file transfers that bypass the relay server's standard message broadcast

### Enhanced Presence

- [ ] **Custom Status** - Set a custom status message
- [ ] **Do Not Disturb** - Suppress all notifications
- [ ] **Away Auto-Detection** - Set away status after configurable inactivity period
- [ ] **Last Seen Privacy** - Control who can see your last-seen timestamp

---

## 🛠️ Developer Experience (v0.7.0)

**Theme**: Extensibility & Customization

### Integrations

- [ ] **Bot API** - Build bots and automations via message hooks
- [ ] **Webhooks** - Outgoing webhooks for external integrations
- [ ] **CLI Pipe Mode** - `echo "msg" | ghostwire user` for scripting
- [ ] **Bridge Protocols** - Bridge to Matrix or IRC
- [ ] **API Client Libraries** - Python and Go library bindings

### Customization

- [ ] **Theme System** - Built-in color schemes (cyberpunk, matrix, nord, gruvbox)
- [ ] **Custom Themes** - User-defined themes via `config.toml`
- [ ] **Keybinding Customization** - Vim, Emacs, or fully custom bindings
- [ ] **Layout Options** - Rearrange or resize panels

### Accessibility

- [ ] **Screen Reader Support** - Accessible output for screen readers
- [ ] **High Contrast Mode** - For visually impaired users
- [ ] **Color Blind Modes** - Alternative color palettes

---

## 🏗️ Infrastructure & Scale (v0.8.0)

**Theme**: Scalability & Reliability

### Server Improvements

- [ ] **Horizontal Scaling** - Multiple relay instances with shared state
- [ ] **Redis Integration** - Pub/sub for multi-server message fanout
- [ ] **Rate Limiting** - Per-connection message rate limits to prevent abuse
- [ ] **Graceful Degradation** - Failover handling for server restarts
- [ ] **Monitoring** - Prometheus metrics + Grafana dashboards

### Performance

- [ ] **Message Compression** - zstd compression to reduce bandwidth
- [ ] **Binary Protocol** - Replace JSON with MessagePack for lower overhead
- [ ] **Connection Pooling** - Reuse connections for multi-server setups

### Deployment

- [ ] **Docker Support** - Official multi-arch Docker images
- [ ] **Docker Compose** - One-command local stack for self-hosters
- [ ] **On-Premise Deployment** - Self-hosted guide and hardening checklist
- [ ] **Kubernetes Helm Chart** - Production-grade K8s deployment

---

## 🔏 Advanced Privacy (v0.9.0)

**Theme**: Minimize What the Server Knows

_Addresses the known metadata-visibility gap documented in `docs/SECURITY.md`._

- [ ] **Sealed Sender** - Hide sender identity from the relay (Signal-style `SealedSender`)
- [ ] **Metadata Minimization** - Reduce server-visible routing metadata
- [ ] **Message Padding** - Uniform ciphertext sizes to prevent traffic analysis
- [ ] **Tor Integration** - Optional onion routing for transport-layer anonymity
- [ ] **Session Resumption** - Reconnect without full key re-exchange
- [ ] **Multi-Device Identity** - Share identity keypair across multiple terminals
- [ ] **Offline Message Queuing** - Encrypted store-and-forward for offline recipients

---

## 🌟 v1.0.0 — Production Hardening

**Theme**: Audited, Documented, Production Ready

### Security

- [ ] **Third-Party Cryptographic Audit** - Professional review of crypto implementation
- [ ] **Penetration Testing** - Offensive security engagement
- [ ] **Bug Bounty Program** - Reward responsible disclosure
- [ ] **Reproducible Builds** - Verifiable, deterministic binaries

### Performance & Stability

- [ ] **Load Testing** - Benchmark relay under concurrent connections
- [ ] **Fuzzing** - cargo-fuzz on message parsing and crypto paths
- [ ] **Long-Term Soak Testing** - Multi-day stability runs

### Documentation & Community

- [ ] **Complete Documentation** - All features documented with examples
- [ ] **API Documentation** - Full developer reference for bot/library authors
- [ ] **Migration Guides** - Upgrade paths for all prior versions
- [ ] **Best Practices** - Security hardening guide for self-hosters

### Web Access (WebAssembly)

- [ ] **WASM Web Client** - Browser-based client compiled from the same Rust codebase; no Electron or separate GUI needed

---

## 🔭 Future Exploration (Post-1.0)

_Research-stage ideas aligned with the project's privacy-first identity. No commitments._

- **Noise Protocol Framework** - Replace raw X25519 with the full Noise handshake (used by WireGuard, Signal)
- **Post-Quantum Cryptography** - Hybrid classical + ML-KEM (Kyber) for quantum resistance
- **Federated Relay Network** - Interoperable self-hosted relays (Matrix-style)
- **Zero-Knowledge Identity** - Prove group membership without revealing username
- **File Sharing** - End-to-end encrypted file transfer (chunked, ephemeral)
- **Desktop Notifications** - OS-level alerts for mentions (macOS/Linux only, opt-in)

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
- **Anything contradicting the ephemeral/zero-trust model** will not be added to core

---

**Last Updated**: 2026-03-08
**Maintained By**: @jcyrus
**License**: MIT
