# GhostWire Security Model

**Version**: v0.4.0
**Last Updated**: March 8, 2026
**Status**: Released security model

---

## Overview

GhostWire implements **end-to-end encryption (E2EE)** using modern cryptographic primitives to ensure that only the intended recipients can read messages. The server acts as a "dumb relay" and cannot decrypt message content.

### Security Goals

1. **Confidentiality**: Only sender and recipient can read messages
2. **Forward Secrecy**: Compromised keys don't reveal past messages
3. **Authentication**: Verify the identity of communication partners
4. **Ephemeral**: No persistent storage of messages or keys
5. **Auditability**: Security events are logged for review

---

## Cryptographic Stack

### Primitives

| Component                | Algorithm         | Purpose                                       |
| ------------------------ | ----------------- | --------------------------------------------- |
| **Key Exchange**         | X25519 (ECDH)     | Derive shared secrets between peers           |
| **Identity Keys**        | Ed25519           | Long-term identity for signatures             |
| **Symmetric Encryption** | ChaCha20-Poly1305 | AEAD encryption for messages                  |
| **Key Derivation**       | HKDF-SHA256       | Derive encryption/MAC keys from shared secret |
| **Fingerprints**         | SHA-256           | Safety numbers for identity verification      |

### Why These Algorithms?

- **X25519**: Industry standard (Signal, WireGuard), fast, no patents
- **ChaCha20-Poly1305**: Faster than AES on most platforms, constant-time
- **Ed25519**: Fast signatures, widely audited
- **HKDF**: Standard key derivation function (RFC 5869)

---

## Encryption Protocol

### 1. Key Exchange Flow

```
Alice                                Bob
  |                                   |
  |-- KEY_EXCHANGE(Alice_PubKey) --> |
  |                                   |
  |<-- KEY_EXCHANGE(Bob_PubKey) ----|
  |                                   |
  |--- Both derive shared secret via ECDH ---|
  |                                   |
  |--- HKDF: shared_secret -> [enc_key, mac_key, chain_key] ---|
  |                                   |
  |===== Encrypted channel ready =====|
```

### 2. Message Encryption

**Before sending**:

1. Check if session exists with recipient
2. If yes: Encrypt payload with ChaCha20-Poly1305
3. Mark message as `encrypted=true`
4. Server relays opaque ciphertext

**On receiving**:

1. Check if message is marked `encrypted=true`
2. If yes: Decrypt with session key
3. Display plaintext to user
4. If no session: Show "[No encryption session]"

### 3. Session Keys

Each peer-to-peer session has:

```rust
SessionKeys {
   encryption_key: [u8; 32],
   mac_key: [u8; 32],
   chain_key: [u8; 32],
}
```

Keys are derived via:

```
shared_secret = ECDH(our_secret, their_public)
HKDF(shared_secret) -> encryption_key || mac_key || chain_key
```

---

## Security Features

### ✅ Implemented on `main`

#### End-to-End Encryption

- **Status**: ✅ Functional for direct messages and group channels
- **Scope**: DMs use pairwise session ratchets; `group:*` channels use sender keys
- **Key Size**: 256-bit
- **Algorithm**: X25519 + ChaCha20-Poly1305

#### Ephemeral Key Storage

- **Status**: ✅ All keys in memory only
- **Persistence**: None - keys cleared on exit
- **Rotation**: 24-hour automatic rotation with active trigger checks and re-broadcast

#### Security Audit Logging

- **Status**: ✅ Logs all security events
- **Location**: `~/.config/ghostwire/security_audit.log`
- **Events Logged**:
  - Session establishment
  - Message encryption/decryption
  - Decryption failures
  - Replay detection
  - Key rotation
  - Identity verification
  - Security warnings

#### Self-Destructing Messages

- **Status**: ✅ Active via `/expire <seconds> <message>`
- **Usage**: TTL is transmitted on the wire and enforced by clients
- **Cleanup**: Every 5 seconds
- **Secure Deletion**: Overwrites content with zeros (zeroize)

#### Safety Number Verification

- **Status**: ✅ `/verify <username>` and `/confirm <username>` implemented
- **Display**: Safety number shown to user for out-of-band comparison
- **State**: Verified peers are tracked in session state and highlighted in the roster

#### Replay Protection

- **Status**: ✅ Nonce-based replay protection for encrypted direct messages
- **Method**: Track previously seen 96-bit ChaCha20-Poly1305 nonces per peer session
- **Response**: Drop replayed ciphertext and write an audit log entry

#### Group Message Encryption

- **Status**: ✅ Sender-key encryption for `group:*` channels
- **Distribution**: Manual `/groupkey` support plus automatic first-send bootstrap
- **Ratchet**: Sender keys advance per message via a symmetric chain ratchet

#### Secure Deletion

- **Status**: ✅ Uses `zeroize` crate
- **Method**: Overwrites sensitive data before deallocation
- **Applies to**: Message content, encryption keys

---

## Threat Model

### What GhostWire Protects Against

✅ **Passive Network Eavesdropping**

- Encrypted payload prevents reading message content
- Server cannot decrypt messages

✅ **Compromised Server**

- Server only sees encrypted blobs
- No user database or message storage

✅ **Man-in-the-Middle (MITM)** _(partial)_

- Key exchange is authenticated
- Manual safety number verification is available via `/verify` and `/confirm`

✅ **Replay Attacks**

- Nonce history detects replayed DM ciphertext
- Replays are rejected before decryption

✅ **Memory Forensics** _(partial)_

- Zeroize clears sensitive data
- ⚠️ **Limitation**: OS may have swapped pages

---

### What GhostWire Does NOT Protect Against

❌ **Compromised Client Device**

- Malware on your computer can read messages
- Keyloggers can capture input

❌ **Compromised Binary**

- If GhostWire itself is backdoored
- **Mitigation**: Verify checksums, build from source

❌ **Metadata Leakage**

- Server sees: Who talks to whom, when, message sizes
- **v0.9.0**: Sealed sender, message padding, metadata minimization

❌ **Traffic Analysis**

- Timing attacks, frequency analysis
- **v0.9.0**: Tor integration, uniform message padding

✅ **Group Message Encryption**

- Sender-key-based E2EE for `group:*` channels is implemented on `main`
- Global broadcast remains plaintext by design

---

## Key Management

### Lifecycle

1. **Generation**: On client startup
   - Identity keypair (Ed25519): Long-term
   - Ephemeral keypair (X25519): Session-specific

2. **Storage**: In-memory only
   - `KeyStore` struct in RAM
   - Never written to disk

3. **Rotation**: Every 24 hours (automatic)
   - New ephemeral key generated
   - Old sessions cleared
   - Peers re-exchange keys

4. **Deletion**: On exit
   - Memory zeroed with `zeroize`
   - Sessions destroyed

### Session Management

**Per-Peer Sessions**:

```rust
PeerSession {
   their_public_key: PublicKey,
   session_keys: SessionKeys,
   created_at: DateTime,
   last_message_at: DateTime,
   verified: bool,
   send_chain: [u8; 32],
   recv_chain: [u8; 32],
}
```

**Stale Session Cleanup**:

- Sessions older than 48 hours are removed
- Triggered on: New messages, periodic checks

---

## Identity Verification

### Safety Numbers

**Goal**: Verify you're talking to the right person (prevent MITM)

**How it works**:

```rust
safety_number = SHA256(your_public_key || their_public_key)
```

**Usage**:

1. Alice and Bob both see same safety number
2. Compare out-of-band (phone call, in person)
3. If match: Mark peer as "verified"
4. Client shows 🔒 (verified) vs 🔓 (unverified)

**Command** (planned):

```bash
ghostwire --verify alice
# Shows: Safety Number: 1234 5678 9012 3456
```

---

## Forward Secrecy

### Current Status: ⚠️ Partial

**Implemented**:

- Ephemeral X25519 keys (not long-term)
- Key rotation every 24 hours
- Keys never stored on disk

**Not Yet Implemented**:

- **Double Ratchet** (like Signal)
- Per-message keys
- Automatic ratcheting on each message

### Roadmap: v0.4.0

Implement **Double Ratchet Algorithm**:

```
Chain Key ----> Message Key 1
        |-----> Message Key 2
        |-----> Message Key 3
        ...
```

Each message uses a different key derived from the chain key.

---

## Security Audit

### Audit Log Location

```
~/.config/ghostwire/security_audit.log
```

### Sample Audit Entry

```
[2025-12-09T10:30:45Z] E2EE_SESSION_ESTABLISHED: peer=alice, fingerprint=Ym9iX3B1YmxpY19rZXk=
[2025-12-09T10:30:50Z] MESSAGE_ENCRYPTED: recipient=alice, msg_id=550e8400-e29b-41d4-a716-446655440000
[2025-12-09T10:30:51Z] MESSAGE_DECRYPTED: sender=alice, msg_id=6ba7b810-9dad-11d1-80b4-00c04fd430c8
```

### Event Types

- `E2EE_SESSION_ESTABLISHED`
- `KEY_ROTATED`
- `MESSAGE_ENCRYPTED`
- `MESSAGE_DECRYPTED`
- `DECRYPTION_FAILED`
- `IDENTITY_VERIFIED` (future)
- `SESSION_CLEARED`
- `MESSAGE_SELF_DESTRUCTED`
- `SECURE_DELETION`
- `SECURITY_WARNING`

---

## Best Practices

### For Users

1. **Verify Safety Numbers** (when available)
   - Compare with your contact out-of-band
   - Watch for warnings if keys change

2. **Use Self-Destruct for Sensitive Messages** (future)

   ```
   /expire 300  (5 minutes)
   This message will self-destruct...
   ```

3. **Check Encryption Status**
   - Look for 🔒 icon in chat
   - Encrypted messages show lock symbol

4. **Review Audit Logs**
   - Check `security_audit.log` for suspicious activity
   - Watch for decryption failures

### For Developers

1. **Never Log Decrypted Content**
   - Only log metadata, not plaintext

2. **Use Secure Deletion**

   ```rust
   msg.secure_delete();  // Zeroizes content
   ```

3. **Validate Public Keys**
   - Check key length (32 bytes for X25519)
   - Verify base64 encoding

4. **Handle Decryption Failures Gracefully**
   - Don't crash on bad ciphertext
   - Log failure, show warning to user

---

## Known Limitations

### Historical v0.3.0 Limitations

1. **No Identity Verification UI**
   - This was true in v0.3.0 only
   - `main` now supports `/verify` and `/confirm`

2. **No Group Encryption**
   - Only 1-on-1 DMs are encrypted
   - Global channel is plaintext

3. **Metadata Exposed**
   - Server sees: who, when, message count
   - No traffic padding or cover traffic

4. **Replay Protection Absent in v0.3.0**
   - This was true in v0.3.0 only
   - `main` now uses per-session nonce tracking

5. **Trust On First Use (TOFU)**
   - First key exchange is unauthenticated
   - Vulnerable to active MITM

---

## Compliance & Standards

### Cryptographic Standards

- **FIPS 140-2**: ChaCha20-Poly1305 (approved alternative to AES)
- **RFC 7748**: X25519 and Ed25519 specification
- **RFC 5869**: HKDF key derivation
- **RFC 8439**: ChaCha20-Poly1305 AEAD

### Dependencies Audit

| Crate              | Version | Audited | Notes                   |
| ------------------ | ------- | ------- | ----------------------- |
| `x25519-dalek`     | 2.0     | ✅      | Widely used, audited    |
| `ed25519-dalek`    | 2.1     | ✅      | Widely used, audited    |
| `chacha20poly1305` | 0.10    | ✅      | RustCrypto project      |
| `sha2`             | 0.10    | ✅      | RustCrypto project      |
| `hkdf`             | 0.12    | ✅      | RustCrypto project      |
| `zeroize`          | 1.7     | ✅      | Memory clearing utility |

All cryptographic crates from **RustCrypto** organization, which has ongoing security audits.

---

## Reporting Security Issues

**Please DO NOT open public GitHub issues for security vulnerabilities.**

**Contact**: security@ghostwire.dev (or DM @jcyrus)

We follow **responsible disclosure**:

1. Report privately
2. We investigate (target: 48 hours)
3. We issue a patch
4. Public disclosure after fix released

**Bug Bounty**: Not yet available (planned for v1.0.0)

---

## Future Roadmap

### v0.4.0 - Complete the Security Story

- [x] Safety number verification UI
- [x] Self-destruct UI command (`/expire <seconds>`)
- [x] Key rotation trigger activation
- [x] Per-message keys (Double Ratchet)
- [x] Replay protection (nonce tracking)
- [x] Group message encryption (sender keys)

### v0.9.0 - Advanced Privacy

- [ ] Sealed sender (hide sender identity from relay)
- [ ] Metadata minimization
- [ ] Message padding (uniform ciphertext sizes)
- [ ] Tor integration option
- [ ] Session resumption without key re-exchange
- [ ] Multi-device identity

### v1.0.0 - Production Hardening

- [ ] Third-party cryptographic audit
- [ ] Penetration testing
- [ ] Bug bounty program
- [ ] Reproducible builds
- [ ] Formal verification of crypto paths

---

## References

1. **Signal Protocol**: https://signal.org/docs/
2. **X25519**: RFC 7748 - Elliptic Curves for Security
3. **ChaCha20-Poly1305**: RFC 8439
4. **HKDF**: RFC 5869
5. **Double Ratchet**: https://signal.org/docs/specifications/doubleratchet/

---

**Disclaimer**: While GhostWire uses industry-standard cryptography, it has not yet undergone a third-party security audit. Use for sensitive communications at your own risk. A professional audit is planned for v1.0.0.
