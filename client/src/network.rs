// GhostWire Client - Network Layer
// This module handles WebSocket communication in a separate async task

use crate::app::{MessageMeta, MessageType, WireMessage};
use crate::crypto::{decode_public_key, decrypt_message, encode_public_key, encrypt_message};
use x25519_dalek::PublicKey;
use crate::keystore::{KeyChange, KeyStore};
use crate::security_audit::{SecurityAuditLogger, SecurityEvent};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// Successfully connected to server
    Connected,

    /// Disconnected from server
    Disconnected,

    /// Received a chat message
    Message {
        sender: String,
        content: String,
        timestamp: i64,
        channel_id: String,
        encrypted: bool,  // v0.3.0: true if message was encrypted
        ttl: Option<i64>, // v0.4.0: TTL for self-destructing messages
        action: bool,     // v0.5.0: IRC-style /me action message
        message_id: String,
    },

    /// Reaction applied to a message (v0.5.0)
    Reaction {
        sender: String,
        channel_id: String,
        message_id: String,
        emoji: String,
    },

    /// User joined
    UserJoined { username: String },

    /// User left
    UserLeft { username: String },

    /// System message
    SystemMessage { content: String },

    /// Security alert — like a system message but surfaced as a loud Warning in
    /// the UI (used for verified-peer key changes). Carries the affected peer so
    /// the UI can also drop that peer's now-stale verified badge.
    SecurityAlert { username: String, content: String },

    /// Error occurred
    Error { message: String },

    /// Latency update (round-trip time in milliseconds)
    LatencyUpdate { latency_ms: u64 },

    /// Reconnecting to server
    Reconnecting { attempt: u32, max_attempts: u32 },

    /// User typing status changed
    TypingStatus {
        username: String,
        channel_id: String,
        is_typing: bool,
    },

    /// Key exchange received (v0.3.0 E2EE)
    KeyExchangeReceived {
        username: String,
        public_key_b64: String,
    },

    /// Safety number verification result (v0.4.0)
    VerificationResult {
        username: String,
        safety_number: String,
        already_verified: bool,
    },

    /// Verification failed (no session with peer)
    VerificationFailed { username: String, reason: String },

    /// Key rotation occurred (v0.4.0)
    KeyRotated,

    /// Peer identity confirmed as trusted (v0.4.0)
    PeerVerified { username: String },

    /// Sender key received for group encryption (v0.4.0)
    SenderKeyReceived { group_id: String, sender: String },

    /// Relay confirmed a DM reached the recipient's outbound channel (v0.7.0).
    MessageDelivered { message_id: String, recipient: String },

    /// A group message arrived with a different sender ratchet key than what was
    /// distributed, meaning the sender re-ran `/groupkey` (v0.7.0).
    GroupSenderKeyRotated { group_id: String, sender: String },
}

/// Messages sent from the UI to the network task
#[derive(Debug, Clone)]
pub enum NetworkCommand {
    /// Send a chat message to a specific channel
    SendMessage {
        content: String,
        channel_id: String,
        ttl: Option<i64>,
        action: bool,
        message_id: String,
    },

    /// Send a reaction for a specific message ID
    SendReaction {
        channel_id: String,
        message_id: String,
        emoji: String,
    },

    /// Send typing status
    SendTypingStatus { channel_id: String, is_typing: bool },

    /// Verify peer identity (v0.4.0)
    VerifyPeer { username: String },

    /// Confirm peer identity verification (v0.4.0)
    ConfirmVerification { username: String },

    /// Check and perform key rotation if needed (v0.4.0)
    CheckKeyRotation,

    /// Distribute our sender key to group members (v0.4.0)
    DistributeGroupKey {
        group_id: String,
        members: Vec<String>,
    },

    /// Disconnect from server
    Disconnect,
}

fn dm_recipient_from_channel(channel_id: &str, local_username: &str) -> Option<String> {
    let mut parts = channel_id.split(':');

    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("dm"), Some(first_user), Some(second_user), None) => {
            if first_user == local_username && second_user != local_username {
                Some(second_user.to_string())
            } else if second_user == local_username && first_user != local_username {
                Some(first_user.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn build_key_exchange_message(
    username: &str,
    public_key_b64: &str,
    recipient: Option<String>,
    ratchet_key_b64: Option<String>,
) -> WireMessage {
    WireMessage {
        msg_type: MessageType::KeyExchange,
        payload: public_key_b64.to_string(),
        channel: "global".to_string(),
        meta: MessageMeta {
            sender: username.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        },
        is_typing: false,
        encrypted: false,
        recipient,
        ttl: None,
        action: false,
        message_id: None,
        reaction_to: None,
        reaction_emoji: None,
        ratchet_key: ratchet_key_b64,
    }
}

fn key_exchange_recipient_for_incoming(
    wire_msg: &WireMessage,
    local_username: &str,
) -> Option<String> {
    if matches!(wire_msg.msg_type, MessageType::Auth) && wire_msg.meta.sender != local_username {
        Some(wire_msg.meta.sender.clone())
    } else {
        None
    }
}

/// Network task that runs in a separate tokio runtime
/// This is the CRITICAL async/sync split - this task is async, UI is sync
pub async fn network_task(
    server_url: String,
    username: String,
    event_tx: mpsc::UnboundedSender<NetworkEvent>,
    mut command_rx: mpsc::UnboundedReceiver<NetworkCommand>,
) {
    // Initialize keystore for E2EE (v0.3.0)
    let keystore = Arc::new(Mutex::new(KeyStore::new()));

    // Initialize security audit logger
    let audit_logger = Arc::new(Mutex::new({
        let config_dir = directories::ProjectDirs::from("com", "jcyrus", "ghostwire")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        SecurityAuditLogger::new(&config_dir)
    }));

    tracing::info!(
        "Security audit logging enabled at {:?}",
        audit_logger
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .log_path()
    );

    let identity_fingerprint = {
        let store = keystore.lock().unwrap_or_else(|e| e.into_inner());
        store.get_identity_fingerprint()
    };
    tracing::info!("Identity fingerprint: {}", identity_fingerprint);

    // Auto-reconnect configuration
    let max_attempts = 10;
    let initial_backoff_secs = 1;
    let max_backoff_secs = 16;

    let mut attempt = 0;
    let mut should_reconnect = true;

    while should_reconnect {
        attempt += 1;

        if attempt > 1 {
            // Send reconnecting event
            let _ = event_tx.send(NetworkEvent::Reconnecting {
                attempt,
                max_attempts,
            });

            // Calculate exponential backoff delay
            let backoff_secs = std::cmp::min(
                initial_backoff_secs * 2u64.pow(attempt - 2),
                max_backoff_secs,
            );

            tracing::info!(
                "Reconnecting in {} seconds (attempt {}/{})",
                backoff_secs,
                attempt,
                max_attempts
            );

            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        }

        // Attempt to connect to the server
        let ws_stream = match connect_async(&server_url).await {
            Ok((stream, _)) => {
                tracing::info!("Successfully connected to server");
                let _ = event_tx.send(NetworkEvent::Connected);
                attempt = 0; // Reset attempt counter on successful connection
                stream
            }
            Err(e) => {
                tracing::error!("Failed to connect: {}", e);
                let _ = event_tx.send(NetworkEvent::Error {
                    message: format!("Failed to connect: {}", e),
                });

                // Check if should retry
                if attempt >= max_attempts {
                    tracing::error!("Max reconnection attempts reached");
                    let _ = event_tx.send(NetworkEvent::Error {
                        message: "Max reconnection attempts reached. Please restart the client."
                            .to_string(),
                    });
                    return;
                }

                continue;
            }
        };

        let (mut write, mut read) = ws_stream.split();

        // Send authentication message
        let auth_msg = WireMessage {
            msg_type: MessageType::Auth,
            payload: username.clone(),
            channel: "global".to_string(),
            meta: MessageMeta {
                sender: username.clone(),
                timestamp: chrono::Utc::now().timestamp(),
            },
            is_typing: false,
            encrypted: false,
            recipient: None,
            ttl: None,
            action: false,
            message_id: None,
            reaction_to: None,
            reaction_emoji: None,
            ratchet_key: None,
        };

        if let Ok(json) = serde_json::to_string(&auth_msg)
            && let Err(e) = write.send(Message::Text(json.into())).await
        {
            let _ = event_tx.send(NetworkEvent::Error {
                message: format!("Failed to authenticate: {}", e),
            });
            return;
        }

        // Send key exchange message to announce our public key (v0.3.0)
        let (our_public_key, our_ratchet_init_key) = {
            let store = keystore.lock().unwrap_or_else(|e| e.into_inner());
            (store.get_our_public_key(), store.get_our_ratchet_init_public_key())
        };
        let key_exchange_msg = build_key_exchange_message(
            &username,
            &our_public_key,
            None,
            Some(our_ratchet_init_key),
        );

        if let Ok(json) = serde_json::to_string(&key_exchange_msg)
            && let Err(e) = write.send(Message::Text(json.into())).await
        {
            tracing::warn!("Failed to send key exchange: {}", e);
        }

        // Heartbeat interval - send ping every 30 seconds to keep connection alive
        let mut heartbeat = interval(Duration::from_secs(30));
        heartbeat.tick().await; // First tick completes immediately

        // Track ping timestamps for latency measurement.
        // Key is the u64 counter value — avoids a Vec<u8> heap allocation per ping.
        let ping_timestamps: Arc<Mutex<HashMap<u64, Instant>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let mut ping_counter: u64 = 0;

        // Main network loop
        loop {
            tokio::select! {
                // Heartbeat - send ping to keep connection alive
                _ = heartbeat.tick() => {
                    // Create a unique ping payload with counter.
                    // The counter is the map key (u64, stack-only); the bytes are
                    // only serialised once for the wire message.
                    ping_counter += 1;
                    ping_timestamps
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(ping_counter, Instant::now());

                    if let Err(e) = write.send(Message::Ping(ping_counter.to_le_bytes().to_vec().into())).await {
                        let _ = event_tx.send(NetworkEvent::Error {
                            message: format!("Failed to send heartbeat: {}", e),
                        });
                        break;
                    }

                    let active_sessions = {
                        let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                        store.cleanup_stale_sessions();
                        store.active_sessions().len()
                    };
                    tracing::trace!("Active encrypted sessions: {}", active_sessions);
                }

                // Handle incoming messages from server
                Some(msg_result) = read.next() => {
                    match msg_result {
                        Ok(Message::Text(text)) => {
                            // Parse the wire message
                            if let Ok(wire_msg) = serde_json::from_str::<WireMessage>(&text) {
                                // Re-broadcast our key directly to newly connected peers.
                                // This fixes staggered connect ordering where a peer misses our
                                // initial broadcast key exchange sent before they joined.
                                if let Some(recipient) =
                                    key_exchange_recipient_for_incoming(&wire_msg, &username)
                                {
                                    let our_public_key = {
                                        let store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                        store.get_our_public_key()
                                    };

                                    let our_rk = {
                                        let store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                        store.get_our_ratchet_init_public_key()
                                    };
                                    let targeted_key_exchange = build_key_exchange_message(
                                        &username,
                                        &our_public_key,
                                        Some(recipient.clone()),
                                        Some(our_rk),
                                    );

                                    if let Ok(json) = serde_json::to_string(&targeted_key_exchange)
                                        && let Err(e) = write.send(Message::Text(json.into())).await
                                    {
                                        tracing::warn!(
                                            "Failed to send targeted key exchange to {}: {}",
                                            recipient,
                                            e
                                        );
                                    }
                                }

                                handle_wire_message(
                                    wire_msg,
                                    &event_tx,
                                    &keystore,
                                    &audit_logger,
                                    &username,
                                );
                            } else {
                                let _ = event_tx.send(NetworkEvent::Error {
                                    message: "Failed to parse message".to_string(),
                                });
                            }
                        }
                        Ok(Message::Ping(data)) => {
                            // Respond to server ping with pong
                            if let Err(e) = write.send(Message::Pong(data)).await {
                                let _ = event_tx.send(NetworkEvent::Error {
                                    message: format!("Failed to send pong: {}", e),
                                });
                                break;
                            }
                        }
                        Ok(Message::Pong(data)) => {
                            // Server responded to our ping - calculate round-trip time.
                            // Parse the u64 counter key from the 8-byte payload.
                            if data.len() == 8
                                && let Ok(key_bytes) = <[u8; 8]>::try_from(data.as_ref())
                            {
                                let key = u64::from_le_bytes(key_bytes);
                                let mut timestamps = ping_timestamps
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner());
                                if let Some(sent_time) = timestamps.remove(&key) {
                                    let latency_ms = sent_time.elapsed().as_millis() as u64;
                                    let _ = event_tx.send(NetworkEvent::LatencyUpdate { latency_ms });
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            let _ = event_tx.send(NetworkEvent::Disconnected);
                            break;
                        }
                        Err(e) => {
                            let _ = event_tx.send(NetworkEvent::Error {
                                message: format!("WebSocket error: {}", e),
                            });
                            break;
                        }
                        _ => {}
                    }
                }

                // Handle commands from UI
                Some(command) = command_rx.recv() => {
                    match command {
                        NetworkCommand::SendMessage {
                            content,
                            channel_id,
                            ttl,
                            action,
                            message_id,
                        } => {
                            // Carry the pre-computed new_chain alongside the recipient/group
                            // so commit_send/commit_group_send don't re-derive via HKDF.
                            let mut pending_dm_commit: Option<(String, [u8; 32])> = None;
                            let mut pending_group_commit: Option<(String, [u8; 32])> = None;
                            // Ratchet public keys to include in outgoing DM / group messages (v0.7.0).
                            let mut dm_ratchet_key: Option<String> = None;
                            let mut group_ratchet_key: Option<String> = None;

                            // Determine recipient from channel_id (dm:user1:user2)
                            let recipient = dm_recipient_from_channel(&channel_id, &username);

                            // Encrypt direct messages with pairwise session keys.
                            // Encrypt group messages with per-group sender keys.
                            let (payload, encrypted) = if let Some(ref recip) = recipient {
                                let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                if store.has_session(recip) {
                                    match store.get_session(recip) {
                                        Ok(session) => {
                                            let (msg_key, new_chain) = session.derive_send_key();
                                            // Only include ratchet_key for v0.7+ peers with DH ratchet active.
                                            if session.dh_ratchet_enabled {
                                                dm_ratchet_key = Some(encode_public_key(&session.ratchet_public));
                                            }
                                            match encrypt_message(&content, &msg_key) {
                                                Ok(encrypted_payload) => {
                                                    tracing::debug!("Encrypted message to {}", recip);
                                                    audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(SecurityEvent::MessageEncrypted {
                                                        recipient: recip.clone(),
                                                        message_id: uuid::Uuid::new_v4().to_string(),
                                                    });
                                                    pending_dm_commit = Some((recip.clone(), new_chain));
                                                    (encrypted_payload, true)
                                                }
                                                Err(e) => {
                                                    let _ = event_tx.send(NetworkEvent::Error {
                                                        message: format!(
                                                            "Encrypted DM to {} failed: {}",
                                                            recip, e
                                                        ),
                                                    });
                                                    continue;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let _ = event_tx.send(NetworkEvent::Error {
                                                message: format!(
                                                    "No usable DM session with {}: {}",
                                                    recip, e
                                                ),
                                            });
                                            continue;
                                        }
                                    }
                                } else {
                                    let _ = event_tx.send(NetworkEvent::Error {
                                        message: format!(
                                            "Cannot send encrypted DM to {}: no active session",
                                            recip
                                        ),
                                    });
                                    continue;
                                }
                            } else if channel_id.starts_with("group:") {
                                let store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                if let Some((msg_key, new_chain)) = store.derive_group_send_key(&channel_id) {
                                    // Capture ratchet pub for the group message header (v0.7.0).
                                    group_ratchet_key = store.get_group_send_ratchet_public(&channel_id)
                                        .map(|pk| encode_public_key(&pk));
                                    match encrypt_message(&content, &msg_key) {
                                        Ok(encrypted_payload) => {
                                            tracing::debug!("Encrypted group message in {}", channel_id);
                                            audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(SecurityEvent::MessageEncrypted {
                                                recipient: channel_id.clone(),
                                                message_id: uuid::Uuid::new_v4().to_string(),
                                            });
                                            pending_group_commit = Some((channel_id.clone(), new_chain));
                                            (encrypted_payload, true)
                                        }
                                        Err(e) => {
                                            let _ = event_tx.send(NetworkEvent::Error {
                                                message: format!(
                                                    "Encrypted group send in {} failed: {}",
                                                    channel_id, e
                                                ),
                                            });
                                            continue;
                                        }
                                    }
                                } else {
                                    let _ = event_tx.send(NetworkEvent::Error {
                                        message: format!(
                                            "Cannot send encrypted group message in {}: missing sender key (try /groupkey)",
                                            channel_id
                                        ),
                                    });
                                    continue;
                                }
                            } else {
                                // Global channel remains plaintext
                                (content.clone(), false)
                            };

                            let is_dm = recipient.is_some();
                            let msg = WireMessage {
                                msg_type: MessageType::Message,
                                payload,
                                channel: channel_id,
                                meta: MessageMeta {
                                    sender: username.clone(),
                                    timestamp: chrono::Utc::now().timestamp(),
                                },
                                is_typing: false,
                                encrypted,
                                recipient,
                                ttl,
                                action,
                                message_id: Some(message_id),
                                reaction_to: None,
                                reaction_emoji: None,
                                // Include ratchet pub on encrypted DMs and group messages (v0.7.0).
                                ratchet_key: match (encrypted, is_dm) {
                                    (true, true) => dm_ratchet_key,
                                    (true, false) => group_ratchet_key,
                                    _ => None,
                                },
                            };

                            if let Ok(json) = serde_json::to_string(&msg) {
                                if let Err(e) = write.send(Message::Text(json.into())).await {
                                    let _ = event_tx.send(NetworkEvent::Error {
                                        message: format!("Failed to send message: {}", e),
                                    });
                                } else if encrypted {
                                    if let Some((recipient_user, new_chain)) = pending_dm_commit {
                                        let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                        if let Ok(session) = store.get_session(&recipient_user) {
                                            session.commit_send(new_chain);
                                        }
                                    }

                                    if let Some((group_id, new_chain)) = pending_group_commit {
                                        let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                        let _ = store.commit_group_send(&group_id, new_chain);
                                    }
                                }
                            }
                        }
                        NetworkCommand::SendTypingStatus { channel_id, is_typing } => {
                            let msg = WireMessage {
                                msg_type: MessageType::Typing,
                                payload: String::new(),
                                channel: channel_id,
                                meta: MessageMeta {
                                    sender: username.clone(),
                                    timestamp: chrono::Utc::now().timestamp(),
                                },
                                is_typing,
                                encrypted: false,
                                recipient: None,
                                ttl: None,
                                action: false,
                                message_id: None,
                                reaction_to: None,
                                reaction_emoji: None,
                                ratchet_key: None,
                            };

                            if let Ok(json) = serde_json::to_string(&msg)
                                && let Err(e) = write.send(Message::Text(json.into())).await
                            {
                                tracing::debug!("Failed to send typing status: {}", e);
                            }
                        }
                        NetworkCommand::VerifyPeer { username: peer_username } => {
                            let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                            if store.has_session(&peer_username) {
                                // Compute a session fingerprint from ephemeral session keys.
                                let our_pub_bytes = *store.ephemeral.public.as_bytes();
                                let their_pub_bytes = if let Ok(session) = store.get_session(&peer_username) {
                                    *session.their_public_key.as_bytes()
                                } else {
                                    drop(store);
                                    let _ = event_tx.send(NetworkEvent::VerificationFailed {
                                        username: peer_username.clone(),
                                        reason: "Session lookup failed".to_string(),
                                    });
                                    continue;
                                };

                                let safety_number = format_session_fingerprint(our_pub_bytes, their_pub_bytes);

                                let already_verified = store.is_verified(&peer_username);
                                drop(store);

                                let _ = event_tx.send(NetworkEvent::VerificationResult {
                                    username: peer_username,
                                    safety_number,
                                    already_verified,
                                });
                            } else {
                                drop(store);
                                audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(SecurityEvent::IdentityVerificationFailed {
                                    peer: peer_username.clone(),
                                    reason: format!("No active session with {}", peer_username),
                                });
                                let _ = event_tx.send(NetworkEvent::VerificationFailed {
                                    username: peer_username.clone(),
                                    reason: format!("No active session with {}", peer_username),
                                });
                            }
                        }
                        NetworkCommand::ConfirmVerification { username: peer_username } => {
                            let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                            let session_fingerprint = {
                                let our_pub_bytes = *store.ephemeral.public.as_bytes();
                                if let Ok(session) = store.get_session(&peer_username) {
                                    Some(format_session_fingerprint(
                                        our_pub_bytes,
                                        *session.their_public_key.as_bytes(),
                                    ))
                                } else {
                                    None
                                }
                            };

                            if store.verify_peer(&peer_username).is_ok() {
                                drop(store);
                                audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(SecurityEvent::IdentityVerified {
                                    peer: peer_username.clone(),
                                    safety_number: session_fingerprint
                                        .unwrap_or_else(|| "session fingerprint unavailable".to_string()),
                                });
                                let _ = event_tx.send(NetworkEvent::PeerVerified {
                                    username: peer_username,
                                });
                            } else {
                                drop(store);
                                audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(SecurityEvent::IdentityVerificationFailed {
                                    peer: peer_username.clone(),
                                    reason: format!("No active session with {}", peer_username),
                                });
                                let _ = event_tx.send(NetworkEvent::VerificationFailed {
                                    username: peer_username.clone(),
                                    reason: format!("No active session with {}", peer_username),
                                });
                            }
                        }
                        NetworkCommand::CheckKeyRotation => {
                            let needs_rotation = {
                                let store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                store.needs_rotation()
                            };

                            if needs_rotation {
                                // Read session count before clearing, without cloning all keys.
                                let prior_session_count = {
                                    let store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                    store.session_count()
                                };

                                {
                                    let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                    store.rotate_ephemeral_key();
                                }

                                // Audit log the rotation
                                audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(SecurityEvent::KeyRotated {
                                    reason: format!(
                                        "24-hour automatic rotation; {} sessions reset and re-bootstrap initiated",
                                        prior_session_count
                                    ),
                                });

                                // Re-broadcast new public key + new ratchet init key
                                let (public_key, ratchet_init_key) = {
                                    let store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                    (store.get_our_public_key(), store.get_our_ratchet_init_public_key())
                                };

                                let msg = build_key_exchange_message(
                                    &username,
                                    &public_key,
                                    None,
                                    Some(ratchet_init_key),
                                );

                                if let Ok(json) = serde_json::to_string(&msg) {
                                    let _ = write.send(Message::Text(json.into())).await;
                                }

                                let _ = event_tx.send(NetworkEvent::KeyRotated);
                                let _ = event_tx.send(NetworkEvent::SystemMessage {
                                    content: "Encrypted conversations were reset after key rotation; sessions are re-establishing.".to_string(),
                                });
                            }
                        }
                        NetworkCommand::DistributeGroupKey { group_id, members } => {
                            let (key, chain_key, ratchet_pub_bytes) = {
                                let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                let (key, chain_key) = store.get_or_create_sender_key(&group_id);
                                let rp = store
                                    .get_group_send_ratchet_public(&group_id)
                                    .map(|pk| *pk.as_bytes())
                                    .unwrap_or([0u8; 32]);
                                (key, chain_key, rp)
                            };
                            // 96-byte payload: key(32) || chain_key(32) || ratchet_public(32).
                            // v0.6 receivers expect 64 bytes and will reject this distribution;
                            // that is the intended migration cutover (see v0.7 release notes).
                            let mut payload_bytes = Vec::with_capacity(96);
                            payload_bytes.extend_from_slice(&key);
                            payload_bytes.extend_from_slice(&chain_key);
                            payload_bytes.extend_from_slice(&ratchet_pub_bytes);
                            let payload = BASE64.encode(&payload_bytes);

                            // Send sender key to each member via the relay
                            for member in &members {
                                let encrypt_result = {
                                    let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                    if let Ok(session) = store.get_session(member) {
                                        let (msg_key, new_chain) = session.derive_send_key();
                                        match encrypt_message(&payload, &msg_key) {
                                            Ok(encrypted) => Some((encrypted, new_chain)),
                                            Err(e) => {
                                                tracing::warn!(
                                                    "Failed to encrypt sender key for {}: {}",
                                                    member,
                                                    e
                                                );
                                                None
                                            }
                                        }
                                    } else {
                                        tracing::warn!(
                                            "Cannot distribute sender key to {} without an active session",
                                            member
                                        );
                                        None
                                    }
                                };

                                let Some((encrypted_payload, new_chain)) = encrypt_result else {
                                    continue;
                                };

                                let msg = WireMessage {
                                    msg_type: MessageType::SenderKey,
                                    payload: encrypted_payload,
                                    channel: group_id.clone(),
                                    meta: MessageMeta {
                                        sender: username.clone(),
                                        timestamp: chrono::Utc::now().timestamp(),
                                    },
                                    is_typing: false,
                                    encrypted: true,
                                    recipient: Some(member.clone()),
                                    ttl: None,
                                    action: false,
                                    message_id: None,
                                    reaction_to: None,
                                    reaction_emoji: None,
                                    ratchet_key: None,
                                };
                                if let Ok(json) = serde_json::to_string(&msg)
                                    && write.send(Message::Text(json.into())).await.is_ok()
                                {
                                    let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                                    if let Ok(session) = store.get_session(member) {
                                        session.commit_send(new_chain);
                                    }
                                }
                            }
                            tracing::info!("Distributed sender key for group {} to {} members", group_id, members.len());
                        }
                        NetworkCommand::SendReaction {
                            channel_id,
                            message_id,
                            emoji,
                        } => {
                            let msg = WireMessage {
                                msg_type: MessageType::Message,
                                payload: String::new(),
                                channel: channel_id,
                                meta: MessageMeta {
                                    sender: username.clone(),
                                    timestamp: chrono::Utc::now().timestamp(),
                                },
                                is_typing: false,
                                encrypted: false,
                                recipient: None,
                                ttl: None,
                                action: false,
                                message_id: None,
                                reaction_to: Some(message_id),
                                reaction_emoji: Some(emoji),
                                ratchet_key: None,
                            };

                            if let Ok(json) = serde_json::to_string(&msg)
                                && let Err(e) = write.send(Message::Text(json.into())).await
                            {
                                let _ = event_tx.send(NetworkEvent::Error {
                                    message: format!("Failed to send reaction: {}", e),
                                });
                            }
                        }
                        NetworkCommand::Disconnect => {
                            tracing::info!("Received disconnect command");
                            let _ = write.send(Message::Close(None)).await;
                            should_reconnect = false;
                            break;
                        }
                    }
                }

                // If both channels are closed, exit
                else => {
                    should_reconnect = false;
                    break;
                }
            }
        }

        // Send disconnected event and loop back to reconnect if needed
        let _ = event_tx.send(NetworkEvent::Disconnected);

        if !should_reconnect {
            tracing::info!("Network task exiting (no reconnect)");
            break;
        }

        tracing::info!("Connection lost, will attempt to reconnect");
    }
}

/// Handle a wire message and convert it to a NetworkEvent
fn handle_wire_message(
    msg: WireMessage,
    event_tx: &mpsc::UnboundedSender<NetworkEvent>,
    keystore: &Arc<Mutex<KeyStore>>,
    audit_logger: &Arc<Mutex<SecurityAuditLogger>>,
    local_username: &str,
) {
    match msg.msg_type {
        MessageType::Message => {
            // msg is owned here — move the Option<String> fields directly (no clone)
            if let (Some(target_id), Some(emoji)) = (msg.reaction_to, msg.reaction_emoji) {
                let _ = event_tx.send(NetworkEvent::Reaction {
                    sender: msg.meta.sender,
                    channel_id: msg.channel,
                    message_id: target_id,
                    emoji,
                });
                return;
            }

            // Decrypt message if it's encrypted
            let (content, _message_id) = if msg.encrypted {
                let message_id = uuid::Uuid::new_v4().to_string();
                let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                if msg.channel.starts_with("group:") {
                    if store.has_sender_key(&msg.channel, &msg.meta.sender) {
                        // Sender key rotation detection (v0.7.0): check BEFORE deriving
                        // recv key / decrypting. When the sender re-runs /groupkey, the
                        // message is encrypted under the new distribution and decryption
                        // with the stale one fails — so the Ok branch is never reached.
                        // Detecting the ratchet key mismatch up front lets us surface the
                        // warning and skip the doomed decrypt attempt.
                        if let Some(rk_b64) = &msg.ratchet_key
                            && let Ok(received_rk) = decode_public_key(rk_b64)
                            && received_rk.as_bytes() != &[0u8; 32]
                            && let Some(stored_rk) = store.get_group_sender_ratchet_public(
                                &msg.channel,
                                &msg.meta.sender,
                            )
                            && stored_rk.as_bytes() != &[0u8; 32]
                            && stored_rk.as_bytes() != received_rk.as_bytes()
                        {
                            store.remove_sender_key(&msg.channel, &msg.meta.sender);
                            let _ = event_tx.send(NetworkEvent::GroupSenderKeyRotated {
                                group_id: msg.channel.clone(),
                                sender: msg.meta.sender.clone(),
                            });
                            return;
                        }

                        let Some((msg_key, new_chain)) =
                            store.derive_group_recv_key(&msg.channel, &msg.meta.sender)
                        else {
                            return;
                        };
                        match decrypt_message(&msg.payload, &msg_key) {
                            Ok(plaintext) => {
                                tracing::debug!(
                                    "Decrypted group message from {} in {}",
                                    msg.meta.sender,
                                    msg.channel
                                );
                                let _ = store.commit_group_recv(
                                    &msg.channel,
                                    &msg.meta.sender,
                                    new_chain,
                                );

                                (plaintext, message_id)
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to decrypt group message from {}: {}",
                                    msg.meta.sender,
                                    e
                                );
                                audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(
                                    SecurityEvent::DecryptionFailed {
                                        sender: msg.meta.sender.clone(),
                                        reason: e.to_string(),
                                    },
                                );
                                (format!("[Group decryption failed: {}]", e), message_id)
                            }
                        }
                    } else {
                        tracing::warn!(
                            "No sender key for {} in group {}",
                            msg.meta.sender,
                            msg.channel
                        );
                        (
                            "[No group sender key: ask peer to redistribute]".to_string(),
                            message_id,
                        )
                    }
                } else if let Ok(session) = store.get_session(&msg.meta.sender) {
                    // Decode once — used for both the nonce replay check and decryption.
                    // Avoids a second BASE64 decode pass inside decrypt_message (C-2).
                    let payload_bytes = match BASE64.decode(&msg.payload) {
                        Ok(b) => b,
                        Err(e) => {
                            tracing::error!(
                                "Failed to decode message from {}: {}",
                                msg.meta.sender,
                                e
                            );
                            return;
                        }
                    };

                    // Replay protection: extract nonce and check for duplicates
                    let mut extracted_nonce: Option<[u8; 12]> = None;
                    if payload_bytes.len() >= 12 {
                        let mut nonce = [0u8; 12];
                        nonce.copy_from_slice(&payload_bytes[..12]);
                        if session.nonce_seen(&nonce) {
                            tracing::warn!("Replay attack detected from {}", msg.meta.sender);
                            audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(
                                SecurityEvent::ReplayDetected {
                                    sender: msg.meta.sender.clone(),
                                    nonce: hex::encode(nonce),
                                },
                            );
                            return;
                        }
                        extracted_nonce = Some(nonce);
                    }

                    // DH ratchet step (v0.7.0): only for sessions where both peers
                    // exchanged init ratchet keys. Skipped for v0.6 fallback sessions.
                    if session.dh_ratchet_enabled
                        && let Some(rk_b64) = &msg.ratchet_key
                        && let Ok(their_new_rk) = decode_public_key(rk_b64)
                        && their_new_rk.as_bytes() != session.their_ratchet_public.as_bytes()
                    {
                        session.perform_recv_dh_ratchet(their_new_rk);
                    }

                    // Single HKDF pass: derive msg_key and carry new_chain (C-1).
                    let (msg_key, new_chain) = session.derive_recv_key();
                    // Decrypt from already-decoded bytes — no second allocation (C-2).
                    let decrypted = crate::crypto::decrypt_message_bytes(&payload_bytes, &msg_key);

                    match decrypted {
                        Ok(plaintext) => {
                            if let Some(nonce) = extracted_nonce {
                                session.record_nonce(&nonce);
                            }
                            session.commit_recv(new_chain);
                            tracing::debug!("Decrypted message from {}", msg.meta.sender);
                            store.touch_session(&msg.meta.sender);

                            // Audit log
                            audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(
                                SecurityEvent::MessageDecrypted {
                                    sender: msg.meta.sender.clone(),
                                    message_id: message_id.clone(),
                                },
                            );

                            (plaintext, message_id)
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to decrypt message from {}: {}",
                                msg.meta.sender,
                                e
                            );

                            // Audit log failure
                            audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(
                                SecurityEvent::DecryptionFailed {
                                    sender: msg.meta.sender.clone(),
                                    reason: e.to_string(),
                                },
                            );

                            (format!("[Decryption failed: {}]", e), message_id)
                        }
                    }
                } else {
                    tracing::warn!("No session for encrypted message from {}", msg.meta.sender);

                    // Audit log
                    audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(
                        SecurityEvent::SecurityWarning {
                            message: format!(
                                "Received encrypted message from {} without session",
                                msg.meta.sender
                            ),
                        },
                    );

                    ("[No encryption session]".to_string(), message_id)
                }
            } else {
                (msg.payload, uuid::Uuid::new_v4().to_string())
            };

            let _ = event_tx.send(NetworkEvent::Message {
                sender: msg.meta.sender,
                content,
                timestamp: msg.meta.timestamp,
                channel_id: msg.channel,
                encrypted: msg.encrypted,
                ttl: msg.ttl,
                action: msg.action,
                message_id: msg
                    .message_id
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            });
        }
        MessageType::System => {
            // Parse system messages for user join/leave
            if msg.payload.contains("joined") {
                let _ = event_tx.send(NetworkEvent::UserJoined {
                    username: msg.meta.sender,
                });
            } else if msg.payload.contains("left") {
                let _ = event_tx.send(NetworkEvent::UserLeft {
                    username: msg.meta.sender,
                });
            } else {
                let _ = event_tx.send(NetworkEvent::SystemMessage {
                    content: msg.payload,
                });
            }
        }
        MessageType::Auth => {
            // User authenticated - add them to roster
            let username = msg.meta.sender.clone();
            let _ = event_tx.send(NetworkEvent::UserJoined { username });
        }
        MessageType::Typing => {
            // User typing status changed
            let _ = event_tx.send(NetworkEvent::TypingStatus {
                username: msg.meta.sender,
                channel_id: msg.channel,
                is_typing: msg.is_typing,
            });
        }
        MessageType::KeyExchange => {
            // If a key exchange is targeted, only the intended recipient should process it.
            if let Some(recipient) = &msg.recipient
                && recipient != local_username
            {
                return;
            }

            // Store peer's public key and establish session
            let their_username = msg.meta.sender.clone();
            let their_public_key = msg.payload.clone();

            // Decode the peer key once; both the TOFU check and the pending-key
            // store reuse it (avoids decoding the same base64 twice).
            let their_pk = match decode_public_key(&their_public_key) {
                Ok(pk) => pk,
                Err(e) => {
                    tracing::error!("Failed to decode public key from {}: {}", their_username, e);
                    return;
                }
            };

            let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());

            // Detect a key change (TOFU) BEFORE the session is replaced, so the
            // verification state we read reflects the prior session.
            let key_change = store.track_peer_key(&their_username, &their_pk);

            // Record their public key for the handshake.
            store.store_peer_public_key_decoded(&their_username, their_pk);

            // If the KEY_EXCHANGE carries a DH ratchet init key (v0.7.0), store it
            // so `establish_session` can perform the DH bootstrap.
            if let Some(rk_b64) = &msg.ratchet_key {
                if let Ok(rk) = decode_public_key(rk_b64) {
                    store.store_peer_ratchet_key_decoded(&their_username, rk);
                } else {
                    tracing::warn!("Failed to decode ratchet_key from {}", their_username);
                }
            }

            // Establish encrypted session.
            //
            // Note: even when a *verified* peer's key changed, we re-establish
            // immediately so messaging keeps working, rather than blocking until
            // the user re-verifies (as Signal does). The new session starts
            // `verified == false`, which clears the verified badge, and the user
            // is warned below to re-run `/verify`. This is a deliberate
            // usability trade-off, not full Signal-style blocking.
            if let Err(e) = store.establish_session(&their_username) {
                tracing::error!("Failed to establish session with {}: {}", their_username, e);
                return;
            }

            tracing::info!("✓ Established E2EE session with {}", their_username);

            // Audit log session establishment
            audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(
                SecurityEvent::SessionEstablished {
                    peer: their_username.clone(),
                    public_key_fingerprint: their_public_key[..16].to_string(),
                },
            );

            // Surface key changes. A change to a *verified* peer is a serious
            // warning (verification no longer applies to the new key); a change
            // to an unverified peer is routine (likely the 24h ephemeral
            // rotation) and is audit-logged only — no UI spam.
            match &key_change {
                KeyChange::Changed {
                    was_verified: true,
                    old_fingerprint,
                    new_fingerprint,
                } => {
                    let message = format!(
                        "⚠️ SECURITY: {}'s key changed ({}… → {}…) — their previous identity was VERIFIED. \
                         This happens on key rotation, but if unexpected it may indicate impersonation. \
                         Re-verify with /verify {}.",
                        their_username, old_fingerprint, new_fingerprint, their_username
                    );
                    audit_logger
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .log(SecurityEvent::SecurityWarning {
                            message: message.clone(),
                        });
                    // Route through SecurityAlert so the UI renders it as a loud
                    // Warning AND drops the now-stale verified badge for this peer.
                    let _ = event_tx.send(NetworkEvent::SecurityAlert {
                        username: their_username.clone(),
                        content: message,
                    });
                }
                KeyChange::Changed {
                    was_verified: false,
                    old_fingerprint,
                    new_fingerprint,
                } => {
                    // Routine for an unverified peer (likely 24h rotation): audit only.
                    audit_logger.lock().unwrap_or_else(|e| e.into_inner()).log(
                        SecurityEvent::SecurityWarning {
                            message: format!(
                                "{}'s key changed ({}… → {}…); peer was not verified (likely key rotation).",
                                their_username, old_fingerprint, new_fingerprint
                            ),
                        },
                    );
                    tracing::info!(
                        "Peer {} key changed (unverified; likely rotation)",
                        their_username
                    );
                }
                KeyChange::FirstSeen | KeyChange::Unchanged => {}
            }

            // Notify UI layer
            let _ = event_tx.send(NetworkEvent::KeyExchangeReceived {
                username: their_username,
                public_key_b64: their_public_key,
            });
        }
        MessageType::Ack => {
            if let Some(mid) = msg.message_id {
                let recipient = msg.recipient.unwrap_or_default();
                let _ = event_tx.send(NetworkEvent::MessageDelivered {
                    message_id: mid,
                    recipient,
                });
            }
        }
        // Unrecognised type from a future protocol version — silently discard.
        MessageType::Unknown => {}
        MessageType::SenderKey => {
            if msg.recipient.as_deref() != Some(local_username) {
                return;
            }

            if !msg.channel.starts_with("group:") {
                return;
            }

            // Receive a sender key distribution for group encryption
            let payload_b64 = if msg.encrypted {
                let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                let Ok(session) = store.get_session(&msg.meta.sender) else {
                    tracing::warn!(
                        "Ignoring encrypted sender key from {} without active session",
                        msg.meta.sender
                    );
                    return;
                };

                let (msg_key, new_chain) = session.derive_recv_key();
                match decrypt_message(&msg.payload, &msg_key) {
                    Ok(plaintext) => {
                        session.commit_recv(new_chain);
                        plaintext
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Ignoring sender key from {} due to decrypt failure: {}",
                            msg.meta.sender,
                            e
                        );
                        return;
                    }
                }
            } else {
                tracing::warn!(
                    "Ignoring unencrypted sender key from {} for {}",
                    msg.meta.sender,
                    msg.channel
                );
                return;
            };

            // Parse 96-byte (v0.7: key||chain||ratchet_pub) or 64-byte (v0.6: key||chain)
            // payload. Anything else is silently discarded.
            if let Ok(payload_bytes) = BASE64.decode(payload_b64) {
                let parsed = match payload_bytes.len() {
                    96 => {
                        let mut key = [0u8; 32];
                        let mut chain_key = [0u8; 32];
                        let mut rp = [0u8; 32];
                        key.copy_from_slice(&payload_bytes[..32]);
                        chain_key.copy_from_slice(&payload_bytes[32..64]);
                        rp.copy_from_slice(&payload_bytes[64..96]);
                        Some((key, chain_key, PublicKey::from(rp)))
                    }
                    64 => {
                        let mut key = [0u8; 32];
                        let mut chain_key = [0u8; 32];
                        key.copy_from_slice(&payload_bytes[..32]);
                        chain_key.copy_from_slice(&payload_bytes[32..64]);
                        Some((key, chain_key, PublicKey::from([0u8; 32])))
                    }
                    n => {
                        tracing::warn!(
                            "Invalid sender key payload length {} from {} — expected 64 or 96",
                            n, msg.meta.sender
                        );
                        None
                    }
                };

                if let Some((key, chain_key, ratchet_public)) = parsed {
                    let mut store = keystore.lock().unwrap_or_else(|e| e.into_inner());
                    store.store_sender_key(&msg.channel, &msg.meta.sender, key, chain_key, ratchet_public);
                    drop(store);

                    tracing::info!(
                        "Received sender key from {} for group {}",
                        msg.meta.sender,
                        msg.channel
                    );

                    let _ = event_tx.send(NetworkEvent::SenderKeyReceived {
                        group_id: msg.channel,
                        sender: msg.meta.sender,
                    });
                }
            }
        }
    }
}

fn format_session_fingerprint(our_pub_bytes: [u8; 32], their_pub_bytes: [u8; 32]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    if our_pub_bytes < their_pub_bytes {
        hasher.update(our_pub_bytes);
        hasher.update(their_pub_bytes);
    } else {
        hasher.update(their_pub_bytes);
        hasher.update(our_pub_bytes);
    }
    let hash = hasher.finalize();

    // 30 hex chars grouped into 6 blocks of 5, separated by spaces (35 chars total).
    // Build directly into a pre-sized String — avoids the intermediate Vec<&str>.
    let hex_str = hex::encode(&hash[..15]);
    let mut result = String::with_capacity(35);
    for (i, chunk) in hex_str.as_bytes().chunks(5).enumerate() {
        if i > 0 {
            result.push(' ');
        }
        // SAFETY: hex::encode output is always ASCII
        result.push_str(unsafe { std::str::from_utf8_unchecked(chunk) });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{dm_recipient_from_channel, key_exchange_recipient_for_incoming};
    use crate::app::{MessageMeta, MessageType, WireMessage};

    fn mock_wire_message(msg_type: MessageType, sender: &str) -> WireMessage {
        WireMessage {
            msg_type,
            payload: String::new(),
            channel: "global".to_string(),
            meta: MessageMeta {
                sender: sender.to_string(),
                timestamp: 0,
            },
            is_typing: false,
            encrypted: false,
            recipient: None,
            ttl: None,
            action: false,
            message_id: None,
            reaction_to: None,
            reaction_emoji: None,
            ratchet_key: None,
        }
    }

    #[test]
    fn dm_recipient_skips_prefix_and_picks_other_user() {
        assert_eq!(
            dm_recipient_from_channel("dm:alice:bob", "alice"),
            Some("bob".to_string())
        );
        assert_eq!(
            dm_recipient_from_channel("dm:alice:bob", "bob"),
            Some("alice".to_string())
        );
    }

    #[test]
    fn dm_recipient_rejects_invalid_channel_shapes() {
        assert_eq!(dm_recipient_from_channel("global", "alice"), None);
        assert_eq!(dm_recipient_from_channel("dm:alice", "alice"), None);
        assert_eq!(dm_recipient_from_channel("dm:alice:alice", "alice"), None);
    }

    #[test]
    fn targeted_key_exchange_only_for_other_users_auth() {
        let local_username = "alice";

        assert_eq!(
            key_exchange_recipient_for_incoming(
                &mock_wire_message(MessageType::Auth, "alice"),
                local_username,
            ),
            None
        );

        assert_eq!(
            key_exchange_recipient_for_incoming(
                &mock_wire_message(MessageType::Auth, "bob"),
                local_username,
            ),
            Some("bob".to_string())
        );

        assert_eq!(
            key_exchange_recipient_for_incoming(
                &mock_wire_message(MessageType::Message, "bob"),
                local_username,
            ),
            None
        );
    }

    #[test]
    fn test_ratchet_key_wire_field_roundtrip() {
        let mut msg = mock_wire_message(MessageType::Message, "alice");
        msg.ratchet_key = Some("abc123".to_string());
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: WireMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.ratchet_key, Some("abc123".to_string()));

        // When None, field must be absent from JSON output.
        let mut msg2 = mock_wire_message(MessageType::Message, "alice");
        msg2.ratchet_key = None;
        let json2 = serde_json::to_string(&msg2).unwrap();
        assert!(!json2.contains("ratchet_key"), "field must be skipped when None");
    }

    #[test]
    fn test_unknown_message_type_is_ignored() {
        // MessageType::Unknown is the #[serde(other)] catch-all that silently
        // absorbs unrecognised type values at the match arm.  Verify the variant
        // is reachable and that a WireMessage carrying it round-trips correctly.
        let msg = mock_wire_message(MessageType::Unknown, "relay");
        assert!(matches!(msg.msg_type, MessageType::Unknown));
        // Serialise and re-parse (Unknown serialises to a sentinel JSON value
        // that serde maps back to Unknown via the catch-all arm).
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: WireMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded.msg_type, MessageType::Unknown));
    }

    #[test]
    fn staggered_join_sequence_triggers_rekey_for_late_peer() {
        let local_username = "alice";

        // Simulate a typical staggered timeline observed in production:
        // 1) Alice's own AUTH is observed
        // 2) Bob joins later with AUTH
        // 3) Bob sends normal messages
        let incoming = [
            mock_wire_message(MessageType::Auth, "alice"),
            mock_wire_message(MessageType::Auth, "bob"),
            mock_wire_message(MessageType::Message, "bob"),
        ];

        let recipients: Vec<String> = incoming
            .iter()
            .filter_map(|msg| key_exchange_recipient_for_incoming(msg, local_username))
            .collect();

        assert_eq!(recipients, vec!["bob".to_string()]);
    }
}
