// GhostWire Client - Key Store Module
// Manages ephemeral encryption keys with automatic rotation

use crate::crypto::{
    EphemeralKeypair, IdentityKeypair, compute_safety_number, decode_verifying_key,
    derive_session_keys, encode_public_key, encode_verifying_key, fingerprint_public_key,
    generate_ephemeral_keypair, generate_identity_keypair, kdf_rk, ratchet_chain_key,
    sign_message, verify_signature,
};
use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use hkdf::Hkdf;
use sha2::Sha256;
use std::collections::{HashMap, HashSet, VecDeque};
use x25519_dalek::{PublicKey, StaticSecret};

/// Key rotation interval (24 hours)
const KEY_ROTATION_INTERVAL: i64 = 24 * 60 * 60;

/// Maximum age for a session key before it's considered stale
const MAX_SESSION_AGE: i64 = 48 * 60 * 60;

/// Peer session information
/// Maximum number of nonces to track per peer for replay protection
const MAX_NONCE_HISTORY: usize = 10_000;

/// Maximum number of peers whose first-seen (TOFU) key we retain. Bounds memory
/// for a long-running client in a busy channel; the oldest entry is evicted when
/// the cap is reached (a re-seen evicted peer simply re-establishes TOFU).
const MAX_KNOWN_PEER_KEYS: usize = 10_000;

/// Result of comparing an incoming peer public key against the first-seen
/// (trust-on-first-use) value we recorded for that username.
///
/// Note: GhostWire's KEY_EXCHANGE carries the rotating X25519 *ephemeral* key,
/// not a persistent identity key, so a legitimate 24-hour rotation also shows up
/// as `Changed`. Callers therefore treat a change to a *verified* peer as a
/// serious warning (verification is invalidated) but a change to an unverified
/// peer as routine (likely rotation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyChange {
    /// First key we've ever seen for this peer — recorded as the TOFU baseline.
    FirstSeen,
    /// Same key as the recorded baseline — nothing changed.
    Unchanged,
    /// Key differs from the recorded baseline. The baseline is updated to the
    /// new key after this is returned.
    Changed {
        /// Whether the peer had been marked verified before this change.
        was_verified: bool,
        /// Short fingerprint of the previously recorded key.
        old_fingerprint: String,
        /// Short fingerprint of the new key.
        new_fingerprint: String,
    },
}

pub struct PeerSession {
    pub their_public_key: PublicKey,
    pub created_at: DateTime<Utc>,
    pub last_message_at: DateTime<Utc>,
    pub verified: bool,
    /// Current send chain key (symmetric ratchet, per-message FS)
    pub send_chain: [u8; 32],
    /// Current receive chain key (symmetric ratchet, per-message FS)
    pub recv_chain: [u8; 32],
    /// Number of messages sent in current chain
    pub send_counter: u64,
    /// Number of messages received in current chain
    pub recv_counter: u64,
    /// Nonces we've already seen from this peer (replay protection)
    seen_nonces: HashSet<[u8; 12]>,
    /// FIFO queue to evict oldest nonces when capacity is reached
    nonce_order: VecDeque<[u8; 12]>,

    // ── DH Double Ratchet state (v0.7.0) ──────────────────────────────────
    /// Root key for the DH ratchet; advanced by `kdf_rk` on each DH step.
    pub root_key: [u8; 32],
    /// Our current DH ratchet secret. Used to compute the DH output when the
    /// peer's ratchet key changes, then replaced with a fresh keypair.
    pub ratchet_secret: StaticSecret,
    /// Public half of `ratchet_secret`; included as `ratchet_key` on every
    /// outgoing encrypted DM so the peer can detect a ratchet rotation.
    pub ratchet_public: PublicKey,
    /// Last known DH ratchet public key from the peer. When an incoming DM
    /// carries a different value, `perform_recv_dh_ratchet` is triggered.
    pub their_ratchet_public: PublicKey,
}

impl PeerSession {
    /// Derive the next send message key without mutating state.
    ///
    /// Returns `(msg_key, new_chain)`. Pass `new_chain` to [`commit_send`] after
    /// a successful encrypted send to advance the ratchet in a single HKDF pass.
    pub fn derive_send_key(&self) -> ([u8; 32], [u8; 32]) {
        let (new_chain, msg_key) = ratchet_chain_key(&self.send_chain);
        (msg_key, new_chain)
    }

    /// Commit one step on the send chain using the `new_chain` returned by
    /// [`derive_send_key`]. Avoids a redundant HKDF computation.
    pub fn commit_send(&mut self, new_chain: [u8; 32]) {
        self.send_chain = new_chain;
        self.send_counter += 1;
    }

    /// Derive the next receive message key without mutating state.
    ///
    /// Returns `(msg_key, new_chain)`. Pass `new_chain` to [`commit_recv`] after
    /// a successful decryption to advance the ratchet in a single HKDF pass.
    pub fn derive_recv_key(&self) -> ([u8; 32], [u8; 32]) {
        let (new_chain, msg_key) = ratchet_chain_key(&self.recv_chain);
        (msg_key, new_chain)
    }

    /// Commit one step on the receive chain using the `new_chain` returned by
    /// [`derive_recv_key`]. Avoids a redundant HKDF computation.
    pub fn commit_recv(&mut self, new_chain: [u8; 32]) {
        self.recv_chain = new_chain;
        self.recv_counter += 1;
    }

    /// Check if a nonce has been seen before without mutating state.
    pub fn nonce_seen(&self, nonce: &[u8; 12]) -> bool {
        self.seen_nonces.contains(nonce)
    }

    /// Record a nonce after successful decryption.
    pub fn record_nonce(&mut self, nonce: &[u8; 12]) {
        if self.seen_nonces.contains(nonce) {
            return;
        }
        // Evict oldest if at capacity
        if self.seen_nonces.len() >= MAX_NONCE_HISTORY
            && let Some(oldest) = self.nonce_order.pop_front()
        {
            self.seen_nonces.remove(&oldest);
        }
        self.seen_nonces.insert(*nonce);
        self.nonce_order.push_back(*nonce);
    }

    /// Execute a DH ratchet step on receiving a DM whose `ratchet_key` differs
    /// from `their_ratchet_public`.
    ///
    /// Step 1 — derive new recv chain (ECDH with peer's new ratchet key).
    /// Step 2 — generate fresh local keypair; derive new send chain so future
    ///          outgoing messages carry post-compromise-secured state.
    /// Both chains and counters are committed atomically at the end.
    pub fn perform_recv_dh_ratchet(&mut self, their_new_ratchet: PublicKey) {
        // Step 1: advance recv chain using our current ratchet secret + peer's new key.
        let dh = self.ratchet_secret.diffie_hellman(&their_new_ratchet);
        let (root1, recv_chain) = kdf_rk(&self.root_key, dh.as_bytes());

        // Step 2: generate a fresh ratchet keypair; advance send chain with it.
        let new_kp = generate_ephemeral_keypair();
        let dh2 = new_kp.secret.diffie_hellman(&their_new_ratchet);
        let (root2, send_chain) = kdf_rk(&root1, dh2.as_bytes());

        self.root_key = root2;
        self.recv_chain = recv_chain;
        self.recv_counter = 0;
        self.send_chain = send_chain;
        self.send_counter = 0;
        self.their_ratchet_public = their_new_ratchet;
        self.ratchet_secret = new_kp.secret;
        self.ratchet_public = new_kp.public;
    }
}

/// Sender key state for group encryption (v0.4.0).
/// Each group member distributes a sender key; all other members
/// store it to decrypt messages from that sender.
pub struct SenderKeyState {
    /// The symmetric sender key (ChaCha20-Poly1305)
    pub key: [u8; 32],
    /// Chain key for ratcheting the sender key forward
    pub chain_key: [u8; 32],
    /// Message counter (monotonic)
    pub counter: u64,
    /// DH ratchet public key (v0.7.0). Included in every outgoing group message
    /// and in the SENDER_KEY distribution payload. Recipients compare it against
    /// the stored value to detect when the sender has run `/groupkey`.
    pub ratchet_public: PublicKey,
    /// DH ratchet secret (v0.7.0). Present only on our own sender key (`generate`);
    /// set to a zero sentinel on received distributions (`from_distribution`).
    /// Stored for future group DH ratchet steps; not yet used in this version.
    #[allow(dead_code)]
    pub ratchet_secret: StaticSecret,
}

impl SenderKeyState {
    /// Generate a fresh sender key for ourselves in a group.
    pub fn generate() -> Self {
        let mut key = [0u8; 32];
        let mut chain_key = [0u8; 32];
        rand::fill(&mut key);
        rand::fill(&mut chain_key);
        let ratchet_kp = generate_ephemeral_keypair();
        Self {
            key,
            chain_key,
            counter: 0,
            ratchet_public: ratchet_kp.public,
            ratchet_secret: ratchet_kp.secret,
        }
    }

    /// Create from a received distribution message.
    /// `ratchet_public` is the sender's current DH ratchet public key (v0.7.0).
    /// Pass `PublicKey::from([0u8; 32])` when decoding a v0.6 64-byte payload.
    pub fn from_distribution(key: [u8; 32], chain_key: [u8; 32], ratchet_public: PublicKey) -> Self {
        Self {
            key,
            chain_key,
            counter: 0,
            ratchet_public,
            ratchet_secret: StaticSecret::from([0u8; 32]),
        }
    }

    /// Derive the next per-message key without mutating state.
    ///
    /// Returns `(msg_key, new_chain)`. Pass `new_chain` to [`commit`] after
    /// successful encrypt/decrypt to advance the ratchet without a second HKDF pass.
    pub fn derive_message_key(&self) -> ([u8; 32], [u8; 32]) {
        let (new_chain, msg_key) = ratchet_chain_key(&self.chain_key);
        (msg_key, new_chain)
    }

    /// Commit one ratchet step using the `new_chain` returned by
    /// [`derive_message_key`]. Avoids a redundant HKDF computation.
    pub fn commit(&mut self, new_chain: [u8; 32]) {
        self.chain_key = new_chain;
        self.counter += 1;
    }
}

/// In-memory key store (ephemeral, cleared on exit)
pub struct KeyStore {
    /// Our long-term identity keypair (Ed25519)
    pub identity: IdentityKeypair,

    /// Our current ephemeral keypair (X25519)
    pub ephemeral: EphemeralKeypair,

    /// When our ephemeral key was created
    ephemeral_created_at: DateTime<Utc>,

    /// Our DH ratchet init keypair (v0.7.0). Advertised in KEY_EXCHANGE and
    /// used to bootstrap DH Double Ratchet state in `establish_session`.
    /// Regenerated alongside `ephemeral` on every 24h key rotation.
    pub ratchet_init_keypair: EphemeralKeypair,

    /// Active sessions with peers (username -> session)
    sessions: HashMap<String, PeerSession>,

    /// Pending key exchanges (username -> their public key)
    pending_exchanges: HashMap<String, PublicKey>,

    /// Pending peer DH ratchet init keys (username -> their init ratchet pub).
    /// Populated from KEY_EXCHANGE messages; consumed by `establish_session`.
    pending_ratchet_keys: HashMap<String, PublicKey>,

    /// Our sender keys for groups we belong to (group_id -> SenderKeyState)
    our_sender_keys: HashMap<String, SenderKeyState>,

    /// Sender keys from other members (group_id -> (username -> SenderKeyState))
    group_sender_keys: HashMap<String, HashMap<String, SenderKeyState>>,

    /// First-seen (TOFU) public key per peer username, used to detect key
    /// changes. Deliberately NOT cleared by `clear_all_sessions`/rotation: our
    /// own rotation clears sessions but peers' keys are unchanged, so the record
    /// must persist to avoid false "key changed" alarms on re-establishment.
    known_peer_keys: HashMap<String, PublicKey>,

    /// FIFO insertion order of `known_peer_keys` usernames, used to evict the
    /// oldest entry once `MAX_KNOWN_PEER_KEYS` is reached.
    known_peer_order: VecDeque<String>,

    /// Usernames the user has verified (safety number confirmed). Tracked
    /// separately from `PeerSession.verified` so it survives `clear_all_sessions`
    /// / our own 24h rotation — otherwise a previously-verified peer whose key
    /// later changes would be reported as unverified and the loud warning
    /// suppressed. Cleared for a peer when that peer's key changes (the new key
    /// is not verified).
    verified_peers: HashSet<String>,
}

impl KeyStore {
    /// Create a new key store with fresh keys
    pub fn new() -> Self {
        let identity = generate_identity_keypair();

        // Lightweight startup self-check to validate identity key machinery.
        let probe = b"ghostwire-identity-selfcheck";
        let signature = sign_message(probe, &identity.signing_key);
        if let Err(e) = verify_signature(probe, &signature, &identity.verifying_key) {
            tracing::warn!("Identity signature self-check failed: {}", e);
        }
        let encoded_vk = encode_verifying_key(&identity.verifying_key);
        if let Ok(decoded_vk) = decode_verifying_key(&encoded_vk) {
            let _ = compute_safety_number(&identity.verifying_key, &decoded_vk);
        }

        Self {
            identity,
            ephemeral: generate_ephemeral_keypair(),
            ephemeral_created_at: Utc::now(),
            ratchet_init_keypair: generate_ephemeral_keypair(),
            sessions: HashMap::new(),
            pending_exchanges: HashMap::new(),
            pending_ratchet_keys: HashMap::new(),
            our_sender_keys: HashMap::new(),
            group_sender_keys: HashMap::new(),
            known_peer_keys: HashMap::new(),
            known_peer_order: VecDeque::new(),
            verified_peers: HashSet::new(),
        }
    }

    /// Return a stable identity fingerprint for diagnostics.
    pub fn get_identity_fingerprint(&self) -> String {
        compute_safety_number(&self.identity.verifying_key, &self.identity.verifying_key)
    }

    /// Get our current ephemeral public key (base64 encoded)
    pub fn get_our_public_key(&self) -> String {
        encode_public_key(&self.ephemeral.public)
    }

    /// Base64-encoded DH ratchet init public key for inclusion in KEY_EXCHANGE.
    pub fn get_our_ratchet_init_public_key(&self) -> String {
        encode_public_key(&self.ratchet_init_keypair.public)
    }

    /// Store the peer's decoded DH ratchet init public key for the upcoming
    /// `establish_session` call.
    pub fn store_peer_ratchet_key_decoded(&mut self, username: &str, key: PublicKey) {
        self.pending_ratchet_keys.insert(username.to_string(), key);
    }

    /// Take (remove and return) the pending DH ratchet init key for a peer.
    fn take_peer_ratchet_key(&mut self, username: &str) -> Option<PublicKey> {
        self.pending_ratchet_keys.remove(username)
    }

    /// Check if our ephemeral key needs rotation
    pub fn needs_rotation(&self) -> bool {
        let age = Utc::now() - self.ephemeral_created_at;
        age.num_seconds() > KEY_ROTATION_INTERVAL
    }

    /// Rotate our ephemeral keypair (forward secrecy)
    pub fn rotate_ephemeral_key(&mut self) {
        tracing::info!("Rotating ephemeral keypair for forward secrecy");
        self.ephemeral = generate_ephemeral_keypair();
        self.ephemeral_created_at = Utc::now();
        self.ratchet_init_keypair = generate_ephemeral_keypair();

        // Clear all sessions - they need to re-establish with new key
        self.clear_all_sessions();

        // Clear group sender-key state so peers can safely re-distribute
        // after pairwise sessions are re-established.
        self.our_sender_keys.clear();
        self.group_sender_keys.clear();
    }

    /// Store a peer's public key from a base64 key-exchange payload.
    ///
    /// Test-only convenience: production code decodes once in the KEY_EXCHANGE
    /// handler and calls [`store_peer_public_key_decoded`] directly.
    #[cfg(test)]
    pub fn store_peer_public_key(&mut self, username: &str, public_key_b64: &str) -> Result<()> {
        let public_key = crate::crypto::decode_public_key(public_key_b64)?;
        self.store_peer_public_key_decoded(username, public_key);
        Ok(())
    }

    /// Store an already-decoded peer public key. Lets callers that have decoded
    /// the key for a prior step (e.g. the TOFU check in `track_peer_key`) avoid a
    /// second base64 decode.
    pub fn store_peer_public_key_decoded(&mut self, username: &str, public_key: PublicKey) {
        self.pending_exchanges
            .insert(username.to_string(), public_key);
    }

    /// Establish a session with a peer (perform ECDH + optional DH ratchet bootstrap).
    pub fn establish_session(&mut self, username: &str) -> Result<()> {
        // Copy their public key out to avoid holding a borrow into pending_exchanges
        // while also mutably accessing other fields below.
        let their_public = *self
            .pending_exchanges
            .get(username)
            .ok_or_else(|| anyhow!("No public key for peer: {}", username))?;

        // Base ECDH session keys (v0.6 path).
        let session_keys =
            derive_session_keys(&self.ephemeral.secret, &their_public, b"GhostWire v0.4.0")?;

        // Role-differentiated send/recv chains (lex ordering of ephemeral public keys).
        let our_pub = self.ephemeral.public.as_bytes();
        let their_pub = their_public.as_bytes();
        let (send_label, recv_label) = if our_pub < their_pub {
            (b"send" as &[u8], b"recv" as &[u8])
        } else {
            (b"recv" as &[u8], b"send" as &[u8])
        };

        let hkdf = Hkdf::<Sha256>::new(None, &session_keys.chain_key);
        let mut send_chain = [0u8; 32];
        let mut recv_chain = [0u8; 32];
        hkdf.expand(send_label, &mut send_chain)
            .expect("HKDF expand for send chain");
        hkdf.expand(recv_label, &mut recv_chain)
            .expect("HKDF expand for recv chain");

        // DH Double Ratchet bootstrap (v0.7): if the peer advertised an init
        // ratchet key (carried in KEY_EXCHANGE.ratchet_key), perform one DH step
        // to derive a shared root key and tighten the initial chain keys.
        //
        // If the peer's init ratchet key is absent (v0.6 client), skip the DH
        // step entirely and fall back to symmetric-only ratchet.
        let (root_key, ratchet_secret, ratchet_public, their_ratchet_public) =
            if let Some(their_init_ratchet) = self.take_peer_ratchet_key(username) {
                let dh_init = self
                    .ratchet_init_keypair
                    .secret
                    .diffie_hellman(&their_init_ratchet);
                let (root_key, init_chain) =
                    kdf_rk(&session_keys.chain_key, dh_init.as_bytes());

                // Re-derive role-differentiated chains from init_chain so the DH
                // output folds into the initial send/recv keys.
                let our_rp = self.ratchet_init_keypair.public.as_bytes();
                let their_rp = their_init_ratchet.as_bytes();
                let (s_lbl, r_lbl): (&[u8], &[u8]) = if our_rp < their_rp {
                    (b"send", b"recv")
                } else {
                    (b"recv", b"send")
                };
                let rp_hkdf = Hkdf::<Sha256>::new(None, &init_chain);
                rp_hkdf.expand(s_lbl, &mut send_chain).expect("HKDF");
                rp_hkdf.expand(r_lbl, &mut recv_chain).expect("HKDF");

                // Clone the init ratchet secret into the session so `perform_recv_dh_ratchet`
                // can use it when the peer sends with a rotated ratchet key.
                // (StaticSecret implements Clone in x25519-dalek 2.0 with static_secrets feature)
                let ratchet_secret = self.ratchet_init_keypair.secret.clone();
                let ratchet_public = self.ratchet_init_keypair.public;
                (root_key, ratchet_secret, ratchet_public, their_init_ratchet)
            } else {
                // v0.6 fallback: no DH ratchet. Generate a placeholder keypair so
                // outbound DMs always carry a valid ratchet_key field.
                let fallback = generate_ephemeral_keypair();
                (session_keys.chain_key, fallback.secret, fallback.public, their_public)
            };

        let now = Utc::now();
        let session = PeerSession {
            their_public_key: their_public,
            created_at: now,
            last_message_at: now,
            verified: false,
            send_chain,
            recv_chain,
            send_counter: 0,
            recv_counter: 0,
            seen_nonces: HashSet::new(),
            nonce_order: VecDeque::new(),
            root_key,
            ratchet_secret,
            ratchet_public,
            their_ratchet_public,
        };

        self.sessions.insert(username.to_string(), session);
        self.pending_exchanges.remove(username);

        tracing::info!("Established encrypted session with {}", username);
        Ok(())
    }

    /// Get session keys for encrypting/decrypting messages with a peer
    pub fn get_session(&mut self, username: &str) -> Result<&mut PeerSession> {
        self.sessions
            .get_mut(username)
            .ok_or_else(|| anyhow!("No session with peer: {}", username))
    }

    /// Check if we have an active session with a peer
    pub fn has_session(&self, username: &str) -> bool {
        self.sessions.contains_key(username)
    }

    /// Mark a peer's identity as verified (safety number confirmed)
    pub fn verify_peer(&mut self, username: &str) -> Result<()> {
        let session = self
            .sessions
            .get_mut(username)
            .ok_or_else(|| anyhow!("No session with peer: {}", username))?;

        session.verified = true;
        // Record verification persistently so it survives session clears / our
        // own ephemeral rotation (see `verified_peers`).
        self.verified_peers.insert(username.to_string());
        tracing::info!("Verified identity of peer: {}", username);
        Ok(())
    }

    /// Check if a peer's identity has been verified
    pub fn is_verified(&self, username: &str) -> bool {
        self.sessions
            .get(username)
            .map(|s| s.verified)
            .unwrap_or(false)
    }

    /// Compare an incoming (already-decoded) peer public key against the
    /// first-seen (TOFU) value and record/update the baseline. Call this on
    /// KEY_EXCHANGE *before* [`establish_session`] replaces the session, so
    /// `was_verified` reflects the prior session's state.
    ///
    /// Returns [`KeyChange`] describing whether this is the first key for the
    /// peer, the same key, or a different key (which invalidates any prior
    /// verification — the new session re-established afterwards starts unverified).
    pub fn track_peer_key(&mut self, username: &str, new_key: &PublicKey) -> KeyChange {
        match self.known_peer_keys.get(username) {
            None => {
                self.insert_known_key(username, *new_key);
                KeyChange::FirstSeen
            }
            Some(existing) if existing.as_bytes() == new_key.as_bytes() => KeyChange::Unchanged,
            Some(existing) => {
                let old_fingerprint = fingerprint_public_key(existing);
                let new_fingerprint = fingerprint_public_key(new_key);
                // Read persistent verification (survives session clears), captured
                // before we invalidate it below.
                let was_verified = self.verified_peers.contains(username);

                // The new key invalidates any prior verification.
                self.verified_peers.remove(username);

                // Adopt the new key as the baseline so we warn once per change,
                // not on every subsequent message from the new key. The username
                // is already tracked in `known_peer_order`, so order is unchanged.
                self.known_peer_keys.insert(username.to_string(), *new_key);

                KeyChange::Changed {
                    was_verified,
                    old_fingerprint,
                    new_fingerprint,
                }
            }
        }
    }

    /// Insert a brand-new peer's TOFU key, evicting the oldest entry first if the
    /// map is at `MAX_KNOWN_PEER_KEYS`.
    fn insert_known_key(&mut self, username: &str, key: PublicKey) {
        if self.known_peer_keys.len() >= MAX_KNOWN_PEER_KEYS
            && let Some(oldest) = self.known_peer_order.pop_front()
        {
            self.known_peer_keys.remove(&oldest);
        }
        self.known_peer_keys.insert(username.to_string(), key);
        self.known_peer_order.push_back(username.to_string());
    }

    /// Clean up stale sessions
    pub fn cleanup_stale_sessions(&mut self) {
        let now = Utc::now();
        let threshold = Duration::seconds(MAX_SESSION_AGE);

        self.sessions.retain(|username, session| {
            let age = now - session.created_at;
            if age > threshold {
                tracing::info!("Removing stale session with {}", username);
                false
            } else {
                true
            }
        });
    }

    /// Update last message time for a peer session
    pub fn touch_session(&mut self, username: &str) {
        if let Some(session) = self.sessions.get_mut(username) {
            session.last_message_at = Utc::now();
        }
    }

    /// Get all active session usernames
    pub fn active_sessions(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    /// Clear all sessions (emergency)
    pub fn clear_all_sessions(&mut self) {
        tracing::warn!("Clearing all encryption sessions");
        self.sessions.clear();
        self.pending_exchanges.clear();
        self.pending_ratchet_keys.clear();
    }

    /// Get or create our sender key for a group. Returns (key, chain_key) for distribution.
    pub fn get_or_create_sender_key(&mut self, group_id: &str) -> ([u8; 32], [u8; 32]) {
        let state = self
            .our_sender_keys
            .entry(group_id.to_string())
            .or_insert_with(SenderKeyState::generate);
        (state.key, state.chain_key)
    }

    /// Derive the next group-send key without mutating state.
    ///
    /// Returns `Some((msg_key, new_chain))`. Pass `new_chain` to
    /// [`commit_group_send`] after a successful encrypted send.
    pub fn derive_group_send_key(&self, group_id: &str) -> Option<([u8; 32], [u8; 32])> {
        self.our_sender_keys
            .get(group_id)
            .map(|state| state.derive_message_key())
    }

    /// Commit one step on the group-send chain using the `new_chain` returned
    /// by [`derive_group_send_key`].
    pub fn commit_group_send(&mut self, group_id: &str, new_chain: [u8; 32]) -> bool {
        if let Some(state) = self.our_sender_keys.get_mut(group_id) {
            state.commit(new_chain);
            true
        } else {
            false
        }
    }

    /// Store a sender key received from another group member.
    /// `ratchet_public` is the sender's current DH ratchet pub (v0.7.0); pass
    /// `PublicKey::from([0u8;32])` for v0.6 64-byte distributions.
    pub fn store_sender_key(
        &mut self,
        group_id: &str,
        sender: &str,
        key: [u8; 32],
        chain_key: [u8; 32],
        ratchet_public: PublicKey,
    ) {
        let group = self
            .group_sender_keys
            .entry(group_id.to_string())
            .or_default();

        if let Some(existing) = group.get(sender) {
            // Ignore stale/duplicate distributions (same key, chain, and ratchet pub).
            if existing.key == key
                && existing.chain_key == chain_key
                && existing.ratchet_public.as_bytes() == ratchet_public.as_bytes()
            {
                return;
            }
            if existing.counter > 0 {
                tracing::warn!(
                    "Ignoring sender-key reset for {} in {} because receive chain already advanced",
                    sender,
                    group_id
                );
                return;
            }
        }

        group.insert(
            sender.to_string(),
            SenderKeyState::from_distribution(key, chain_key, ratchet_public),
        );
    }

    /// Return the DH ratchet public key for our own sender key in `group_id`.
    pub fn get_group_send_ratchet_public(&self, group_id: &str) -> Option<PublicKey> {
        self.our_sender_keys.get(group_id).map(|s| s.ratchet_public)
    }

    /// Return the stored ratchet public key for a peer's sender key in `group_id`.
    pub fn get_group_sender_ratchet_public(&self, group_id: &str, sender: &str) -> Option<PublicKey> {
        self.group_sender_keys
            .get(group_id)
            .and_then(|g| g.get(sender))
            .map(|s| s.ratchet_public)
    }

    /// Remove a peer's sender key for `group_id` (called on rotation detection).
    pub fn remove_sender_key(&mut self, group_id: &str, sender: &str) {
        if let Some(group) = self.group_sender_keys.get_mut(group_id) {
            group.remove(sender);
        }
    }

    /// Derive the next group-receive key without mutating state.
    ///
    /// Returns `Some((msg_key, new_chain))`. Pass `new_chain` to
    /// [`commit_group_recv`] after a successful decryption.
    pub fn derive_group_recv_key(
        &self,
        group_id: &str,
        sender: &str,
    ) -> Option<([u8; 32], [u8; 32])> {
        self.group_sender_keys
            .get(group_id)
            .and_then(|group| group.get(sender))
            .map(|state| state.derive_message_key())
    }

    /// Commit one step on the group-receive chain using the `new_chain` returned
    /// by [`derive_group_recv_key`].
    pub fn commit_group_recv(&mut self, group_id: &str, sender: &str, new_chain: [u8; 32]) -> bool {
        self.group_sender_keys
            .get_mut(group_id)
            .and_then(|group| group.get_mut(sender))
            .map(|state| {
                state.commit(new_chain);
            })
            .is_some()
    }

    /// Return the number of active peer sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Check if we have a sender key from a specific member in a group.
    pub fn has_sender_key(&self, group_id: &str, sender: &str) -> bool {
        self.group_sender_keys
            .get(group_id)
            .map(|group| group.contains_key(sender))
            .unwrap_or(false)
    }
}

impl Default for KeyStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keystore_creation() {
        let store = KeyStore::new();
        assert!(!store.get_our_public_key().is_empty());
        assert!(!store.has_session("alice"));
    }

    #[test]
    fn test_key_rotation() {
        let mut store = KeyStore::new();
        let old_key = store.get_our_public_key();

        store.rotate_ephemeral_key();
        let new_key = store.get_our_public_key();

        assert_ne!(old_key, new_key);
    }

    #[test]
    fn test_key_rotation_clears_group_sender_state() {
        let mut store = KeyStore::new();
        let group_id = "group:ops";
        let sender = "alice";

        let _ = store.get_or_create_sender_key(group_id);
        assert!(store.derive_group_send_key(group_id).is_some());

        store.store_sender_key(group_id, sender, [1u8; 32], [2u8; 32], PublicKey::from([0u8; 32]));
        assert!(store.has_sender_key(group_id, sender));

        store.rotate_ephemeral_key();

        assert!(store.derive_group_send_key(group_id).is_none());
        assert!(!store.has_sender_key(group_id, sender));
    }

    #[test]
    fn test_session_establishment() {
        let mut alice_store = KeyStore::new();
        let mut bob_store = KeyStore::new();

        // Exchange public keys
        let alice_pub = alice_store.get_our_public_key();
        let bob_pub = bob_store.get_our_public_key();

        alice_store.store_peer_public_key("bob", &bob_pub).unwrap();
        bob_store
            .store_peer_public_key("alice", &alice_pub)
            .unwrap();

        // Establish sessions
        alice_store.establish_session("bob").unwrap();
        bob_store.establish_session("alice").unwrap();

        assert!(alice_store.has_session("bob"));
        assert!(bob_store.has_session("alice"));

        // First derived send/recv keys should match across peers.
        let alice_session = alice_store.get_session("bob").unwrap();
        let bob_session = bob_store.get_session("alice").unwrap();

        let (alice_send, _) = alice_session.derive_send_key();
        let (bob_recv, _) = bob_session.derive_recv_key();
        assert_eq!(alice_send, bob_recv);
    }

    #[test]
    fn test_session_ratchet_progression() {
        let mut alice_store = KeyStore::new();
        let mut bob_store = KeyStore::new();

        let alice_pub = alice_store.get_our_public_key();
        let bob_pub = bob_store.get_our_public_key();

        alice_store.store_peer_public_key("bob", &bob_pub).unwrap();
        bob_store
            .store_peer_public_key("alice", &alice_pub)
            .unwrap();

        alice_store.establish_session("bob").unwrap();
        bob_store.establish_session("alice").unwrap();

        let alice_session = alice_store.get_session("bob").unwrap();
        let bob_session = bob_store.get_session("alice").unwrap();

        let (alice_send_1, new_chain) = alice_session.derive_send_key();
        alice_session.commit_send(new_chain);
        let (alice_send_2, new_chain) = alice_session.derive_send_key();
        alice_session.commit_send(new_chain);

        let (bob_recv_1, new_chain) = bob_session.derive_recv_key();
        bob_session.commit_recv(new_chain);
        let (bob_recv_2, new_chain) = bob_session.derive_recv_key();
        bob_session.commit_recv(new_chain);

        assert_eq!(alice_session.send_counter, 2);
        assert_eq!(bob_session.recv_counter, 2);
        assert_eq!(alice_send_1, bob_recv_1);
        assert_eq!(alice_send_2, bob_recv_2);
        assert_ne!(alice_send_1, alice_send_2);
        assert_ne!(bob_recv_1, bob_recv_2);
    }

    /// A fresh random X25519 public key, as a peer would send on KEY_EXCHANGE.
    /// Uses only an ephemeral keygen (no identity keypair / self-check) so the
    /// capacity test can create many cheaply.
    fn peer_pubkey() -> PublicKey {
        crate::crypto::generate_ephemeral_keypair().public
    }

    #[test]
    fn test_track_peer_key_first_seen_then_unchanged() {
        let mut store = KeyStore::new();
        let peer_pub = peer_pubkey();

        assert_eq!(store.track_peer_key("alice", &peer_pub), KeyChange::FirstSeen);
        assert_eq!(store.track_peer_key("alice", &peer_pub), KeyChange::Unchanged);
    }

    #[test]
    fn test_track_peer_key_detects_change_unverified() {
        let mut store = KeyStore::new();
        let first = peer_pubkey();
        let second = peer_pubkey();
        assert_ne!(first.as_bytes(), second.as_bytes());

        assert_eq!(store.track_peer_key("alice", &first), KeyChange::FirstSeen);

        match store.track_peer_key("alice", &second) {
            KeyChange::Changed {
                was_verified,
                old_fingerprint,
                new_fingerprint,
            } => {
                assert!(!was_verified, "no session ⇒ not verified");
                assert_ne!(old_fingerprint, new_fingerprint);
            }
            other => panic!("expected Changed, got {other:?}"),
        }

        // Baseline adopted the new key: re-seeing it is Unchanged.
        assert_eq!(store.track_peer_key("alice", &second), KeyChange::Unchanged);
    }

    #[test]
    fn test_track_peer_key_reports_prior_verification() {
        // Establish a verified session with bob, then see a different key.
        let mut alice = KeyStore::new();
        let bob_pub_b64 = KeyStore::new().get_our_public_key();
        let bob_pub = crate::crypto::decode_public_key(&bob_pub_b64).unwrap();

        alice.track_peer_key("bob", &bob_pub);
        alice.store_peer_public_key("bob", &bob_pub_b64).unwrap();
        alice.establish_session("bob").unwrap();
        alice.verify_peer("bob").unwrap();
        assert!(alice.is_verified("bob"));

        let attacker_pub = peer_pubkey();
        match alice.track_peer_key("bob", &attacker_pub) {
            KeyChange::Changed { was_verified, .. } => {
                assert!(was_verified, "must report that bob had been verified");
            }
            other => panic!("expected Changed, got {other:?}"),
        }
    }

    #[test]
    fn test_verification_survives_rotation_for_warning() {
        // Regression for PR #9 review: a peer verified before our own rotation
        // must still trigger the loud "verified peer" warning if their key later
        // changes, even though rotation cleared the live session.
        let mut alice = KeyStore::new();
        let bob_pub_b64 = KeyStore::new().get_our_public_key();
        let bob_pub = crate::crypto::decode_public_key(&bob_pub_b64).unwrap();

        alice.track_peer_key("bob", &bob_pub);
        alice.store_peer_public_key("bob", &bob_pub_b64).unwrap();
        alice.establish_session("bob").unwrap();
        alice.verify_peer("bob").unwrap();

        // Our 24h rotation clears sessions (and the live verified badge)…
        alice.rotate_ephemeral_key();
        assert!(!alice.is_verified("bob"), "rotation clears the live session badge");

        // …but a subsequent key change for bob must still be flagged as a
        // previously-verified peer (the warning must not be suppressed).
        let attacker_pub = peer_pubkey();
        match alice.track_peer_key("bob", &attacker_pub) {
            KeyChange::Changed { was_verified, .. } => {
                assert!(
                    was_verified,
                    "verification must persist across rotation for the warning decision"
                );
            }
            other => panic!("expected Changed, got {other:?}"),
        }

        // Adopting the changed key clears persistent verification.
        let third_pub = peer_pubkey();
        match alice.track_peer_key("bob", &third_pub) {
            KeyChange::Changed { was_verified, .. } => {
                assert!(!was_verified, "a changed key must drop prior verification");
            }
            other => panic!("expected Changed, got {other:?}"),
        }
    }

    #[test]
    fn test_known_key_survives_session_clear() {
        // Our own rotation clears sessions but must not cause a false key-change
        // alarm when the same peer re-exchanges the same key.
        let mut store = KeyStore::new();
        let peer_pub = peer_pubkey();

        assert_eq!(store.track_peer_key("alice", &peer_pub), KeyChange::FirstSeen);
        store.clear_all_sessions();
        assert_eq!(
            store.track_peer_key("alice", &peer_pub),
            KeyChange::Unchanged,
            "TOFU record must persist across session clears"
        );
    }

    #[test]
    fn test_rotation_preserves_known_peer_keys() {
        // rotate_ephemeral_key() clears sessions; it must NOT wipe the TOFU
        // baseline, or every peer would look like a key change after rotation.
        let mut store = KeyStore::new();
        let peer_pub = peer_pubkey();

        assert_eq!(store.track_peer_key("alice", &peer_pub), KeyChange::FirstSeen);
        store.rotate_ephemeral_key();
        assert_eq!(
            store.track_peer_key("alice", &peer_pub),
            KeyChange::Unchanged,
            "rotation must preserve known_peer_keys"
        );
    }

    #[test]
    fn test_known_peer_keys_are_capped() {
        let mut store = KeyStore::new();
        // Insert one past capacity; the oldest entry must be evicted.
        for i in 0..(MAX_KNOWN_PEER_KEYS + 1) {
            let pk = peer_pubkey();
            store.track_peer_key(&format!("peer{i}"), &pk);
        }
        assert_eq!(store.known_peer_keys.len(), MAX_KNOWN_PEER_KEYS);
        assert_eq!(store.known_peer_keys.len(), store.known_peer_order.len());
        assert!(
            !store.known_peer_keys.contains_key("peer0"),
            "oldest peer should have been evicted"
        );
    }

    // ── Group sender-key ratchet tests (v0.7.0) ─────────────────────────────

    #[test]
    fn test_sender_key_state_generate_has_ratchet_public() {
        let state = SenderKeyState::generate();
        assert_ne!(state.ratchet_public.as_bytes(), &[0u8; 32], "ratchet_public must be non-zero");
    }

    #[test]
    fn test_store_sender_key_96_byte_payload() {
        let mut store = KeyStore::new();
        let rp = generate_ephemeral_keypair().public;
        store.store_sender_key("group:ops", "alice", [1u8; 32], [2u8; 32], rp);
        let stored = store.get_group_sender_ratchet_public("group:ops", "alice").unwrap();
        assert_eq!(stored.as_bytes(), rp.as_bytes());
    }

    #[test]
    fn test_store_sender_key_64_byte_fallback() {
        let mut store = KeyStore::new();
        // 64-byte v0.6 distribution: sentinel zero ratchet pub
        store.store_sender_key("group:ops", "bob", [3u8; 32], [4u8; 32], PublicKey::from([0u8; 32]));
        let stored = store.get_group_sender_ratchet_public("group:ops", "bob").unwrap();
        assert_eq!(stored.as_bytes(), &[0u8; 32], "sentinel must be zero");
    }

    #[test]
    fn test_group_sender_key_rotation_detected() {
        let mut store = KeyStore::new();
        let rp_a = generate_ephemeral_keypair().public;
        let rp_b = generate_ephemeral_keypair().public;
        assert_ne!(rp_a.as_bytes(), rp_b.as_bytes());

        store.store_sender_key("group:ops", "alice", [1u8; 32], [2u8; 32], rp_a);
        // Simulate /groupkey: new distribution with rp_b → rotation detected externally
        // (the rotation is detected in network.rs; here we just verify remove_sender_key works)
        store.remove_sender_key("group:ops", "alice");
        assert!(!store.has_sender_key("group:ops", "alice"));
    }

    #[test]
    fn test_group_sender_key_unchanged_no_rotation() {
        let mut store = KeyStore::new();
        let rp = generate_ephemeral_keypair().public;
        store.store_sender_key("group:ops", "alice", [1u8; 32], [2u8; 32], rp);
        let stored = store.get_group_sender_ratchet_public("group:ops", "alice").unwrap();
        // Same ratchet pub → no rotation
        assert_eq!(stored.as_bytes(), rp.as_bytes());
    }

    // ── DH Double Ratchet tests (v0.7.0) ────────────────────────────────────

    /// Exchange init ratchet keys between two keystores and verify that after
    /// `establish_session` the chains are properly mirrored.
    fn setup_ratchet_sessions(alice: &mut KeyStore, bob: &mut KeyStore) {
        let alice_pub = alice.get_our_public_key();
        let alice_rk = alice.get_our_ratchet_init_public_key();
        let bob_pub = bob.get_our_public_key();
        let bob_rk = bob.get_our_ratchet_init_public_key();

        alice.store_peer_public_key("bob", &bob_pub).unwrap();
        alice.store_peer_ratchet_key_decoded(
            "bob",
            crate::crypto::decode_public_key(&bob_rk).unwrap(),
        );

        bob.store_peer_public_key("alice", &alice_pub).unwrap();
        bob.store_peer_ratchet_key_decoded(
            "alice",
            crate::crypto::decode_public_key(&alice_rk).unwrap(),
        );

        alice.establish_session("bob").unwrap();
        bob.establish_session("alice").unwrap();
    }

    #[test]
    fn test_double_ratchet_session_establishment() {
        let mut alice = KeyStore::new();
        let mut bob = KeyStore::new();
        setup_ratchet_sessions(&mut alice, &mut bob);

        let a = alice.get_session("bob").unwrap();
        let b = bob.get_session("alice").unwrap();

        assert_ne!(a.send_chain, b.send_chain, "roles must be distinct");
        assert_eq!(a.send_chain, b.recv_chain, "alice send == bob recv");
        assert_eq!(a.recv_chain, b.send_chain, "alice recv == bob send");
    }

    #[test]
    fn test_perform_recv_dh_ratchet_resets_counters() {
        let mut alice = KeyStore::new();
        let mut bob = KeyStore::new();
        setup_ratchet_sessions(&mut alice, &mut bob);

        // Simulate bob advancing his send counter.
        {
            let s = bob.get_session("alice").unwrap();
            let (_, nc) = s.derive_send_key();
            s.commit_send(nc);
            assert_eq!(s.send_counter, 1);
        }

        // Bob receives a new ratchet key from alice → counters should reset.
        let new_alice_kp = generate_ephemeral_keypair();
        {
            let s = bob.get_session("alice").unwrap();
            s.perform_recv_dh_ratchet(new_alice_kp.public);
            assert_eq!(s.recv_counter, 0, "recv counter reset");
            assert_eq!(s.send_counter, 0, "send counter reset");
            assert_eq!(
                s.their_ratchet_public.as_bytes(),
                new_alice_kp.public.as_bytes(),
                "their_ratchet_public updated"
            );
        }
    }

    #[test]
    fn test_double_ratchet_encrypt_decrypt_roundtrip() {
        use crate::crypto::{decrypt_message, encrypt_message};

        let mut alice = KeyStore::new();
        let mut bob = KeyStore::new();
        setup_ratchet_sessions(&mut alice, &mut bob);

        let plaintext = "hello ratchet";

        // --- Message 1: Alice sends with INIT ratchet key (no DH step on Bob's side) ---
        let (ciphertext1, alice_ratchet_pub1) = {
            let s = alice.get_session("bob").unwrap();
            let rk_pub = s.ratchet_public;
            let (msg_key, nc) = s.derive_send_key();
            let ct = encrypt_message(plaintext, &msg_key).unwrap();
            s.commit_send(nc);
            (ct, rk_pub)
        };

        // Bob receives msg 1 — ratchet pub == their_ratchet_public → no DH step.
        {
            let s = bob.get_session("alice").unwrap();
            // No DH step (same pub)
            assert_eq!(s.their_ratchet_public.as_bytes(), alice_ratchet_pub1.as_bytes());
            let (msg_key, nc) = s.derive_recv_key();
            let got = decrypt_message(&ciphertext1, &msg_key).unwrap();
            s.commit_recv(nc);
            assert_eq!(got, plaintext);
        }

        // --- Message 2: Alice manually rotates her ratchet key ---
        let new_kp = generate_ephemeral_keypair();
        let alice_ratchet_pub2 = new_kp.public;
        let (ciphertext2, _) = {
            let s = alice.get_session("bob").unwrap();
            // Advance alice's send chain via DH with bob's current ratchet pub
            let dh = new_kp.secret.diffie_hellman(&s.their_ratchet_public);
            let (new_root, new_send) = kdf_rk(&s.root_key, dh.as_bytes());
            s.root_key = new_root;
            s.send_chain = new_send;
            s.send_counter = 0;
            s.ratchet_secret = new_kp.secret;
            s.ratchet_public = alice_ratchet_pub2;

            let (msg_key, nc) = s.derive_send_key();
            let ct = encrypt_message(plaintext, &msg_key).unwrap();
            s.commit_send(nc);
            (ct, alice_ratchet_pub2)
        };

        // Bob receives msg 2 — new ratchet pub → perform_recv_dh_ratchet.
        {
            let s = bob.get_session("alice").unwrap();
            assert_ne!(s.their_ratchet_public.as_bytes(), alice_ratchet_pub2.as_bytes());
            s.perform_recv_dh_ratchet(alice_ratchet_pub2);
            let (msg_key, nc) = s.derive_recv_key();
            let got = decrypt_message(&ciphertext2, &msg_key).unwrap();
            s.commit_recv(nc);
            assert_eq!(got, plaintext);
        }
    }

    #[test]
    fn test_no_ratchet_key_falls_back_to_symmetric() {
        // establish_session without a pending ratchet key: v0.6 path.
        let mut alice = KeyStore::new();
        let mut bob = KeyStore::new();

        let alice_pub = alice.get_our_public_key();
        let bob_pub = bob.get_our_public_key();

        alice.store_peer_public_key("bob", &bob_pub).unwrap();
        bob.store_peer_public_key("alice", &alice_pub).unwrap();
        // No store_peer_ratchet_key_decoded → v0.6 fallback path.
        alice.establish_session("bob").unwrap();
        bob.establish_session("alice").unwrap();

        // v0.6 symmetric chain must still work for encrypt/decrypt.
        use crate::crypto::{decrypt_message, encrypt_message};
        let (msg_key, nc) = alice.get_session("bob").unwrap().derive_send_key();
        alice.get_session("bob").unwrap().commit_send(nc);
        let ct = encrypt_message("hi", &msg_key).unwrap();

        let (msg_key_bob, nc_bob) = bob.get_session("alice").unwrap().derive_recv_key();
        bob.get_session("alice").unwrap().commit_recv(nc_bob);
        assert_eq!(decrypt_message(&ct, &msg_key_bob).unwrap(), "hi");
    }

    #[test]
    fn test_replay_nonce_detection() {
        let mut alice_store = KeyStore::new();
        let mut bob_store = KeyStore::new();

        let alice_pub = alice_store.get_our_public_key();
        let bob_pub = bob_store.get_our_public_key();

        alice_store.store_peer_public_key("bob", &bob_pub).unwrap();
        bob_store
            .store_peer_public_key("alice", &alice_pub)
            .unwrap();

        alice_store.establish_session("bob").unwrap();

        let session = alice_store.get_session("bob").unwrap();
        let nonce = [7u8; 12];

        assert!(!session.nonce_seen(&nonce));
        session.record_nonce(&nonce);
        assert!(session.nonce_seen(&nonce));
    }
}
