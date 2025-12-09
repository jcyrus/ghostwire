// GhostWire Client - Key Store Module
// Manages ephemeral encryption keys with automatic rotation

use crate::crypto::{
    decode_public_key, derive_session_keys, encode_public_key, generate_ephemeral_keypair,
    generate_identity_keypair, EphemeralKeypair, IdentityKeypair, SessionKeys,
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use x25519_dalek::PublicKey;

/// Key rotation interval (24 hours)
const KEY_ROTATION_INTERVAL: i64 = 24 * 60 * 60;

/// Maximum age for a session key before it's considered stale
const MAX_SESSION_AGE: i64 = 48 * 60 * 60;

/// Peer session information
pub struct PeerSession {
    pub their_public_key: PublicKey,
    pub session_keys: SessionKeys,
    pub created_at: DateTime<Utc>,
    pub last_message_at: DateTime<Utc>,
    pub verified: bool, // True if identity has been verified
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
}

impl KeyStore {
    /// Create a new key store with fresh keys
    pub fn new() -> Self {
        Self {
            identity: generate_identity_keypair(),
            ephemeral: generate_ephemeral_keypair(),
            ephemeral_created_at: Utc::now(),
            sessions: HashMap::new(),
            pending_exchanges: HashMap::new(),
        }
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
        self.sessions.clear();
    }
    
    /// Store a peer's public key from key exchange message
    pub fn store_peer_public_key(&mut self, username: &str, public_key_b64: &str) -> Result<()> {
        let public_key = decode_public_key(public_key_b64)?;
        self.pending_exchanges.insert(username.to_string(), public_key);
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
        let session_keys = derive_session_keys(
            &self.ephemeral.secret,
            their_public,
            b"GhostWire v0.3.0",
        )?;
        
        let now = Utc::now();
        let session = PeerSession {
            their_public_key: *their_public,
            session_keys,
            created_at: now,
            last_message_at: now,
            verified: false,
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
        bob_store.store_peer_public_key("alice", &alice_pub).unwrap();
        
        // Establish sessions
        alice_store.establish_session("bob").unwrap();
        bob_store.establish_session("alice").unwrap();
        
        assert!(alice_store.has_session("bob"));
        assert!(bob_store.has_session("alice"));
        
        // Keys should match
        let alice_session = alice_store.get_session("bob").unwrap();
        let bob_session = bob_store.get_session("alice").unwrap();
        
        assert_eq!(
            alice_session.session_keys.encryption_key,
            bob_session.session_keys.encryption_key
        );
    }
}
