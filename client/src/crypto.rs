// GhostWire Client - Cryptography Module
// Implements E2EE using X25519 (ECDH) + ChaCha20-Poly1305 (AEAD)

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce,
};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;

/// Ephemeral keypair (X25519) - Short-lived keys for ECDH key exchange
#[derive(Clone)]
pub struct EphemeralKeypair {
    pub secret: StaticSecret,
    pub public: PublicKey,
}

/// Session keys derived from ECDH
pub struct SessionKeys {
    pub encryption_key: [u8; 32],
    pub mac_key: [u8; 32],
    pub chain_key: [u8; 32], // For forward secrecy (Double Ratchet)
}

impl Drop for SessionKeys {
    fn drop(&mut self) {
        // Securely zero out keys on drop
        self.encryption_key.zeroize();
        self.mac_key.zeroize();
        self.chain_key.zeroize();
    }
}

/// Generate a new ephemeral keypair (X25519)
pub fn generate_ephemeral_keypair() -> EphemeralKeypair {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    
    EphemeralKeypair { secret, public }
}

/// Perform ECDH key exchange and derive session keys
pub fn derive_session_keys(
    our_secret: &StaticSecret,
    their_public: &PublicKey,
    info: &[u8], // Context info (e.g., "GhostWire v0.3.0")
) -> Result<SessionKeys> {
    // Perform ECDH
    let shared_secret = our_secret.diffie_hellman(their_public);
    
    // Derive keys using HKDF-SHA256
    let hkdf = Hkdf::<Sha256>::new(None, shared_secret.as_bytes());
    
    let mut encryption_key = [0u8; 32];
    let mut mac_key = [0u8; 32];
    let mut chain_key = [0u8; 32];
    
    hkdf.expand(b"encryption", &mut encryption_key)
        .map_err(|_| anyhow!("Failed to derive encryption key"))?;
    
    hkdf.expand(b"mac", &mut mac_key)
        .map_err(|_| anyhow!("Failed to derive MAC key"))?;
    
    hkdf.expand(info, &mut chain_key)
        .map_err(|_| anyhow!("Failed to derive chain key"))?;
    
    Ok(SessionKeys {
        encryption_key,
        mac_key,
        chain_key,
    })
}

/// Encrypt a message using ChaCha20-Poly1305
pub fn encrypt_message(plaintext: &str, key: &[u8; 32]) -> Result<String> {
    let cipher = ChaCha20Poly1305::new(key.into());
    
    // Generate random nonce (96 bits)
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    // Encrypt
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow!("Encryption failed: {}", e))?;
    
    // Prepend nonce to ciphertext (nonce is not secret)
    let mut payload = nonce_bytes.to_vec();
    payload.extend_from_slice(&ciphertext);
    
    // Base64 encode for wire transmission
    Ok(BASE64.encode(&payload))
}

/// Decrypt a message using ChaCha20-Poly1305
pub fn decrypt_message(ciphertext_b64: &str, key: &[u8; 32]) -> Result<String> {
    let cipher = ChaCha20Poly1305::new(key.into());
    
    // Base64 decode
    let payload = BASE64
        .decode(ciphertext_b64)
        .map_err(|e| anyhow!("Base64 decode failed: {}", e))?;
    
    // Extract nonce (first 12 bytes)
    if payload.len() < 12 {
        return Err(anyhow!("Invalid ciphertext: too short"));
    }
    
    let (nonce_bytes, ciphertext) = payload.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    
    // Decrypt
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow!("Decryption failed: {}", e))?;
    
    String::from_utf8(plaintext).map_err(|e| anyhow!("Invalid UTF-8: {}", e))
}

/// Encode public key to base64 for wire transmission
pub fn encode_public_key(public_key: &PublicKey) -> String {
    BASE64.encode(public_key.as_bytes())
}

/// Decode public key from base64
pub fn decode_public_key(encoded: &str) -> Result<PublicKey> {
    let bytes = BASE64
        .decode(encoded)
        .map_err(|e| anyhow!("Invalid public key encoding: {}", e))?;
    
    if bytes.len() != 32 {
        return Err(anyhow!("Invalid public key length"));
    }
    
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&bytes);
    
    Ok(PublicKey::from(key_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_keypair_generation() {
        let ephemeral = generate_ephemeral_keypair();
        
        // Keys should be non-zero
        assert_ne!(ephemeral.public.as_bytes(), &[0u8; 32]);
    }
    
    #[test]
    fn test_encryption_decryption() {
        let key = [42u8; 32];
        let plaintext = "Hello, GhostWire!";
        
        let ciphertext = encrypt_message(plaintext, &key).unwrap();
        let decrypted = decrypt_message(&ciphertext, &key).unwrap();
        
        assert_eq!(plaintext, decrypted);
    }
    
    #[test]
    fn test_key_exchange() {
        let alice = generate_ephemeral_keypair();
        let bob = generate_ephemeral_keypair();
        
        // Both parties derive same session keys
        let alice_keys = derive_session_keys(
            &alice.secret,
            &bob.public,
            b"test",
        ).unwrap();
        
        let bob_keys = derive_session_keys(
            &bob.secret,
            &alice.public,
            b"test",
        ).unwrap();
        
        assert_eq!(alice_keys.encryption_key, bob_keys.encryption_key);
    }
    
    #[test]
    fn test_public_key_encoding() {
        let keypair = generate_ephemeral_keypair();
        let encoded = encode_public_key(&keypair.public);
        let decoded = decode_public_key(&encoded).unwrap();
        
        assert_eq!(keypair.public.as_bytes(), decoded.as_bytes());
    }
}
