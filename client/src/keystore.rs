// GhostWire Client - Key Store Module
// Manages ephemeral encryption keys with automatic rotation

use crate::crypto::{
    compute_safety_number, decode_public_key, decode_verifying_key, derive_session_keys,
    encode_public_key, encode_verifying_key, generate_ephemeral_keypair, generate_identity_keypair,
    ratchet_chain_key, sign_message, verify_signature, EphemeralKeypair, IdentityKeypair,
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;
use std::collections::{HashMap, HashSet, VecDeque};
use x25519_dalek::PublicKey;

/// Key rotation interval (24 hours)
const KEY_ROTATION_INTERVAL: i64 = 24 * 60 * 60;

/// Maximum age for a session key before it's considered stale
const MAX_SESSION_AGE: i64 = 48 * 60 * 60;

/// Peer session information
/// Maximum number of nonces to track per peer for replay protection
const MAX_NONCE_HISTORY: usize = 10_000;

pub struct PeerSession {
    pub their_public_key: PublicKey,
    pub created_at: DateTime<Utc>,
    pub last_message_at: DateTime<Utc>,
    pub verified: bool,
    /// Current send chain key (ratcheted per message)
    pub send_chain: [u8; 32],
    /// Current receive chain key (ratcheted per message)
    pub recv_chain: [u8; 32],
    /// Number of messages sent in current chain
    pub send_counter: u64,
    /// Number of messages received in current chain
    pub recv_counter: u64,
    /// Nonces we've already seen from this peer (replay protection)
    seen_nonces: HashSet<[u8; 12]>,
    /// FIFO queue to evict oldest nonces when capacity is reached
    nonce_order: VecDeque<[u8; 12]>,
}

impl PeerSession {
    /// Derive the next send message key without mutating state.
    pub fn derive_send_key(&self) -> [u8; 32] {
        let (_, msg_key) = ratchet_chain_key(&self.send_chain);
        msg_key
    }

    /// Commit one step on the send chain after a successful encrypted send.
    pub fn commit_send(&mut self) {
        let (new_chain, _) = ratchet_chain_key(&self.send_chain);
        self.send_chain = new_chain;
        self.send_counter += 1;
    }

    /// Derive the next receive message key without mutating state.
    pub fn derive_recv_key(&self) -> [u8; 32] {
        let (_, msg_key) = ratchet_chain_key(&self.recv_chain);
        msg_key
    }

    /// Commit one step on the receive chain after a successful decrypt.
    pub fn commit_recv(&mut self) {
        let (new_chain, _) = ratchet_chain_key(&self.recv_chain);
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
        if self.seen_nonces.len() >= MAX_NONCE_HISTORY {
            if let Some(oldest) = self.nonce_order.pop_front() {
                self.seen_nonces.remove(&oldest);
            }
        }
        self.seen_nonces.insert(*nonce);
        self.nonce_order.push_back(*nonce);
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
}

impl SenderKeyState {
    /// Generate a fresh sender key for ourselves in a group.
    pub fn generate() -> Self {
        let mut key = [0u8; 32];
        let mut chain_key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        OsRng.fill_bytes(&mut chain_key);
        Self {
            key,
            chain_key,
            counter: 0,
        }
    }

    /// Create from a received distribution message.
    pub fn from_distribution(key: [u8; 32], chain_key: [u8; 32]) -> Self {
        Self {
            key,
            chain_key,
            counter: 0,
        }
    }

    /// Derive the next per-message key without mutating state.
    pub fn derive_message_key(&self) -> [u8; 32] {
        let (_, msg_key) = ratchet_chain_key(&self.chain_key);
        msg_key
    }

    /// Commit one ratchet step after successful encrypt/decrypt.
    pub fn commit(&mut self) {
        let (new_chain, _) = ratchet_chain_key(&self.chain_key);
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

    /// Active sessions with peers (username -> session)
    sessions: HashMap<String, PeerSession>,

    /// Pending key exchanges (username -> their public key)
    pending_exchanges: HashMap<String, PublicKey>,

    /// Our sender keys for groups we belong to (group_id -> SenderKeyState)
    our_sender_keys: HashMap<String, SenderKeyState>,

    /// Sender keys from other members (group_id -> (username -> SenderKeyState))
    group_sender_keys: HashMap<String, HashMap<String, SenderKeyState>>,
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
            sessions: HashMap::new(),
            pending_exchanges: HashMap::new(),
            our_sender_keys: HashMap::new(),
            group_sender_keys: HashMap::new(),
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

        // Clear all sessions - they need to re-establish with new key
        self.clear_all_sessions();
    }

    /// Store a peer's public key from key exchange message
    pub fn store_peer_public_key(&mut self, username: &str, public_key_b64: &str) -> Result<()> {
        let public_key = decode_public_key(public_key_b64)?;
        self.pending_exchanges
            .insert(username.to_string(), public_key);
        Ok(())
    }

    /// Establish a session with a peer (perform ECDH)
    pub fn establish_session(&mut self, username: &str) -> Result<()> {
        // Get their public key from pending exchanges
        let their_public = self
            .pending_exchanges
            .get(username)
            .ok_or_else(|| anyhow!("No public key for peer: {}", username))?;

        // Derive session keys
        let session_keys =
            derive_session_keys(&self.ephemeral.secret, their_public, b"GhostWire v0.3.0")?;

        // Derive send/recv chains with role differentiation.
        // The peer with the lexicographically smaller public key uses
        // (chain_key, "send"/"recv") ordering; the other uses the reverse.
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

        let now = Utc::now();
        let session = PeerSession {
            their_public_key: *their_public,
            created_at: now,
            last_message_at: now,
            verified: false,
            send_chain,
            recv_chain,
            send_counter: 0,
            recv_counter: 0,
            seen_nonces: HashSet::new(),
            nonce_order: VecDeque::new(),
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
    pub fn derive_group_send_key(&self, group_id: &str) -> Option<[u8; 32]> {
        self.our_sender_keys
            .get(group_id)
            .map(|state| state.derive_message_key())
    }

    /// Commit one step on the group-send chain.
    pub fn commit_group_send(&mut self, group_id: &str) -> bool {
        if let Some(state) = self.our_sender_keys.get_mut(group_id) {
            state.commit();
            true
        } else {
            false
        }
    }

    /// Store a sender key received from another group member.
    pub fn store_sender_key(
        &mut self,
        group_id: &str,
        sender: &str,
        key: [u8; 32],
        chain_key: [u8; 32],
    ) {
        let group = self
            .group_sender_keys
            .entry(group_id.to_string())
            .or_default();

        if let Some(existing) = group.get(sender) {
            // Ignore stale/duplicate distributions to avoid resetting an active receive chain.
            if existing.key == key && existing.chain_key == chain_key {
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
            SenderKeyState::from_distribution(key, chain_key),
        );
    }

    /// Derive the next group-receive key without mutating state.
    pub fn derive_group_recv_key(&self, group_id: &str, sender: &str) -> Option<[u8; 32]> {
        self.group_sender_keys
            .get(group_id)
            .and_then(|group| group.get(sender))
            .map(|state| state.derive_message_key())
    }

    /// Commit one step on the group-receive chain.
    pub fn commit_group_recv(&mut self, group_id: &str, sender: &str) -> bool {
        self.group_sender_keys
            .get_mut(group_id)
            .and_then(|group| group.get_mut(sender))
            .map(|state| {
                state.commit();
            })
            .is_some()
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

        assert_eq!(
            alice_session.derive_send_key(),
            bob_session.derive_recv_key()
        );
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

        let alice_send_1 = alice_session.derive_send_key();
        alice_session.commit_send();
        let alice_send_2 = alice_session.derive_send_key();
        alice_session.commit_send();

        let bob_recv_1 = bob_session.derive_recv_key();
        bob_session.commit_recv();
        let bob_recv_2 = bob_session.derive_recv_key();
        bob_session.commit_recv();

        assert_eq!(alice_session.send_counter, 2);
        assert_eq!(bob_session.recv_counter, 2);
        assert_eq!(alice_send_1, bob_recv_1);
        assert_eq!(alice_send_2, bob_recv_2);
        assert_ne!(alice_send_1, alice_send_2);
        assert_ne!(bob_recv_1, bob_recv_2);
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
