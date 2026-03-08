// GhostWire Client - Cryptography Module
// Implements E2EE using X25519 (ECDH) + ChaCha20-Poly1305 (AEAD)

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chacha20poly1305::aead::rand_core::RngCore;
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;

/// Identity keypair (Ed25519) - Long-term identity for verification
#[derive(Clone)]
pub struct IdentityKeypair {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

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

/// Generate a new identity keypair (Ed25519)
pub fn generate_identity_keypair() -> IdentityKeypair {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    IdentityKeypair {
        signing_key,
        verifying_key,
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
    info: &[u8], // Context info (e.g., "GhostWire v0.4.0")
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

/// Sign a message with Ed25519 identity key
pub fn sign_message(message: &[u8], signing_key: &SigningKey) -> Signature {
    signing_key.sign(message)
}

/// Verify a signature with Ed25519 public key
pub fn verify_signature(
    message: &[u8],
    signature: &Signature,
    verifying_key: &VerifyingKey,
) -> Result<()> {
    verifying_key
        .verify(message, signature)
        .map_err(|e| anyhow!("Signature verification failed: {}", e))
}

/// Compute safety number (fingerprint) for identity verification
/// Returns a hex string for manual comparison between users
pub fn compute_safety_number(our_identity: &VerifyingKey, their_identity: &VerifyingKey) -> String {
    use sha2::Digest;

    // Canonicalize key order so both peers compute the same value.
    let our_bytes = our_identity.as_bytes();
    let their_bytes = their_identity.as_bytes();

    let mut hasher = Sha256::new();
    if our_bytes <= their_bytes {
        hasher.update(our_bytes);
        hasher.update(their_bytes);
    } else {
        hasher.update(their_bytes);
        hasher.update(our_bytes);
    }
    let hash = hasher.finalize();

    // Take first 64 bits (16 hex chars) for display
    hex::encode(&hash[..8])
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

/// Encode Ed25519 verifying key to base64
pub fn encode_verifying_key(key: &VerifyingKey) -> String {
    BASE64.encode(key.as_bytes())
}

/// Decode Ed25519 verifying key from base64
pub fn decode_verifying_key(encoded: &str) -> Result<VerifyingKey> {
    let bytes = BASE64
        .decode(encoded)
        .map_err(|e| anyhow!("Invalid verifying key encoding: {}", e))?;

    VerifyingKey::from_bytes(
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow!("Invalid verifying key length"))?,
    )
    .map_err(|e| anyhow!("Invalid verifying key: {}", e))
}

/// Symmetric ratchet step: advance a chain key and derive a message key.
/// Returns (new_chain_key, message_key).
pub fn ratchet_chain_key(chain_key: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let hkdf = Hkdf::<Sha256>::new(None, chain_key);

    let mut new_chain = [0u8; 32];
    let mut msg_key = [0u8; 32];

    // Derive next chain key
    hkdf.expand(b"chain", &mut new_chain)
        .expect("HKDF expand for chain key");
    // Derive message key
    hkdf.expand(b"message", &mut msg_key)
        .expect("HKDF expand for message key");

    (new_chain, msg_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let identity = generate_identity_keypair();
        let ephemeral = generate_ephemeral_keypair();

        // Keys should be non-zero
        assert_ne!(identity.verifying_key.as_bytes(), &[0u8; 32]);
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
        let alice_keys = derive_session_keys(&alice.secret, &bob.public, b"test").unwrap();

        let bob_keys = derive_session_keys(&bob.secret, &alice.public, b"test").unwrap();

        assert_eq!(alice_keys.encryption_key, bob_keys.encryption_key);
    }

    #[test]
    fn test_signature() {
        let identity = generate_identity_keypair();
        let message = b"Test message";

        let signature = sign_message(message, &identity.signing_key);
        verify_signature(message, &signature, &identity.verifying_key).unwrap();
    }

    #[test]
    fn test_public_key_encoding() {
        let keypair = generate_ephemeral_keypair();
        let encoded = encode_public_key(&keypair.public);
        let decoded = decode_public_key(&encoded).unwrap();

        assert_eq!(keypair.public.as_bytes(), decoded.as_bytes());
    }
}
