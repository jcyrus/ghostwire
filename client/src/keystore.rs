// GhostWire Client - Key Store Module
// Manages ephemeral encryption keys with automatic rotation

use crate::crypto::{
    decode_public_key, derive_session_keys, encode_public_key, generate_ephemeral_keypair,
    EphemeralKeypair, SessionKeys,
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use x25519_dalek::PublicKey;

/// Peer session information
pub struct PeerSession {
    pub session_keys: SessionKeys,
    pub last_message_at: DateTime<Utc>,
}

/// In-memory key store (ephemeral, cleared on exit)
pub struct KeyStore {
    /// Our current ephemeral keypair (X25519)
    pub ephemeral: EphemeralKeypair,
    
    /// Active sessions with peers (username -> session)
    sessions: HashMap<String, PeerSession>,
    
    /// Pending key exchanges (username -> their public key)
    pending_exchanges: HashMap<String, PublicKey>,
}

impl KeyStore {
    /// Create a new key store with fresh keys
    pub fn new() -> Self {
        Self {
            ephemeral: generate_ephemeral_keypair(),
            sessions: HashMap::new(),
            pending_exchanges: HashMap::new(),
        }
    }
    
    /// Get our current ephemeral public key (base64 encoded)
    pub fn get_our_public_key(&self) -> String {
        encode_public_key(&self.ephemeral.public)
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
        
        let session = PeerSession {
            session_keys,
            last_message_at: Utc::now(),
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
