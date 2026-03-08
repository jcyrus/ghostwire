// GhostWire Client - Network Layer
// This module handles WebSocket communication in a separate async task

use crate::app::{MessageMeta, MessageType, WireMessage};
use crate::crypto::{decrypt_message, encrypt_message};
use crate::keystore::KeyStore;
use crate::security_audit::{SecurityAuditLogger, SecurityEvent};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
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
        audit_logger.lock().unwrap().log_path()
    );

    let identity_fingerprint = {
        let store = keystore.lock().unwrap();
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
        };

        if let Ok(json) = serde_json::to_string(&auth_msg) {
            if let Err(e) = write.send(Message::Text(json.into())).await {
                let _ = event_tx.send(NetworkEvent::Error {
                    message: format!("Failed to authenticate: {}", e),
                });
                return;
            }
        }

        // Send key exchange message to announce our public key (v0.3.0)
        let our_public_key = {
            let store = keystore.lock().unwrap();
            store.get_our_public_key()
        };
        let key_exchange_msg = WireMessage {
            msg_type: MessageType::KeyExchange,
            payload: our_public_key.clone(),
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
        };

        if let Ok(json) = serde_json::to_string(&key_exchange_msg) {
            if let Err(e) = write.send(Message::Text(json.into())).await {
                tracing::warn!("Failed to send key exchange: {}", e);
            }
        }

        // Heartbeat interval - send ping every 30 seconds to keep connection alive
        let mut heartbeat = interval(Duration::from_secs(30));
        heartbeat.tick().await; // First tick completes immediately

        // Track ping timestamps for latency measurement
        let ping_timestamps: Arc<Mutex<HashMap<Vec<u8>, Instant>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let mut ping_counter: u64 = 0;

        // Main network loop
        loop {
            tokio::select! {
                // Heartbeat - send ping to keep connection alive
                _ = heartbeat.tick() => {
                    // Create a unique ping payload with counter
                    ping_counter += 1;
                    let ping_data = ping_counter.to_le_bytes().to_vec();

                    // Store timestamp before sending
                    if let Ok(mut timestamps) = ping_timestamps.lock() {
                        timestamps.insert(ping_data.clone(), Instant::now());
                    }

                    if let Err(e) = write.send(Message::Ping(ping_data.into())).await {
                        let _ = event_tx.send(NetworkEvent::Error {
                            message: format!("Failed to send heartbeat: {}", e),
                        });
                        break;
                    }

                    let active_sessions = {
                        let mut store = keystore.lock().unwrap();
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
                            // Server responded to our ping - calculate round-trip time
                            if let Ok(mut timestamps) = ping_timestamps.lock() {
                                if let Some(sent_time) = timestamps.remove(data.as_ref()) {
                                    let rtt = sent_time.elapsed();
                                    let latency_ms = rtt.as_millis() as u64;
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
                            let mut pending_dm_commit: Option<String> = None;
                            let mut pending_group_commit: Option<String> = None;

                            // Determine recipient from channel_id (dm:user1:user2)
                            let recipient = if channel_id.starts_with("dm:") {
                                let parts: Vec<&str> = channel_id.split(':').collect();
                                parts.iter()
                                    .find(|&&u| u != username)
                                    .map(|&u| u.to_string())
                            } else {
                                None
                            };

                            // Encrypt direct messages with pairwise session keys.
                            // Encrypt group messages with per-group sender keys.
                            let (payload, encrypted) = if let Some(ref recip) = recipient {
                                let mut store = keystore.lock().unwrap();
                                if store.has_session(recip) {
                                    match store.get_session(recip) {
                                        Ok(session) => {
                                            let msg_key = session.derive_send_key();
                                            match encrypt_message(&content, &msg_key) {
                                                Ok(encrypted_payload) => {
                                                    tracing::debug!("Encrypted message to {}", recip);
                                                    audit_logger.lock().unwrap().log(SecurityEvent::MessageEncrypted {
                                                        recipient: recip.clone(),
                                                        message_id: uuid::Uuid::new_v4().to_string(),
                                                    });
                                                    pending_dm_commit = Some(recip.clone());
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
                                let store = keystore.lock().unwrap();
                                if let Some(msg_key) = store.derive_group_send_key(&channel_id) {
                                    match encrypt_message(&content, &msg_key) {
                                        Ok(encrypted_payload) => {
                                            tracing::debug!("Encrypted group message in {}", channel_id);
                                            audit_logger.lock().unwrap().log(SecurityEvent::MessageEncrypted {
                                                recipient: channel_id.clone(),
                                                message_id: uuid::Uuid::new_v4().to_string(),
                                            });
                                            pending_group_commit = Some(channel_id.clone());
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
                            };

                            if let Ok(json) = serde_json::to_string(&msg) {
                                if let Err(e) = write.send(Message::Text(json.into())).await {
                                    let _ = event_tx.send(NetworkEvent::Error {
                                        message: format!("Failed to send message: {}", e),
                                    });
                                } else if encrypted {
                                    if let Some(recipient_user) = pending_dm_commit {
                                        let mut store = keystore.lock().unwrap();
                                        if let Ok(session) = store.get_session(&recipient_user) {
                                            session.commit_send();
                                        }
                                    }

                                    if let Some(group_id) = pending_group_commit {
                                        let mut store = keystore.lock().unwrap();
                                        let _ = store.commit_group_send(&group_id);
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
                            };

                            if let Ok(json) = serde_json::to_string(&msg) {
                                if let Err(e) = write.send(Message::Text(json.into())).await {
                                    tracing::debug!("Failed to send typing status: {}", e);
                                }
                            }
                        }
                        NetworkCommand::VerifyPeer { username: peer_username } => {
                            let mut store = keystore.lock().unwrap();
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
                                audit_logger.lock().unwrap().log(SecurityEvent::IdentityVerificationFailed {
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
                            let mut store = keystore.lock().unwrap();
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
                                audit_logger.lock().unwrap().log(SecurityEvent::IdentityVerified {
                                    peer: peer_username.clone(),
                                    safety_number: session_fingerprint
                                        .unwrap_or_else(|| "session fingerprint unavailable".to_string()),
                                });
                                let _ = event_tx.send(NetworkEvent::PeerVerified {
                                    username: peer_username,
                                });
                            } else {
                                drop(store);
                                audit_logger.lock().unwrap().log(SecurityEvent::IdentityVerificationFailed {
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
                                let store = keystore.lock().unwrap();
                                store.needs_rotation()
                            };

                            if needs_rotation {
                                let peers_to_rebootstrap = {
                                    let store = keystore.lock().unwrap();
                                    store.active_sessions()
                                };

                                {
                                    let mut store = keystore.lock().unwrap();
                                    store.rotate_ephemeral_key();
                                }

                                // Audit log the rotation
                                audit_logger.lock().unwrap().log(SecurityEvent::KeyRotated {
                                    reason: format!(
                                        "24-hour automatic rotation; {} sessions reset and re-bootstrap initiated",
                                        peers_to_rebootstrap.len()
                                    ),
                                });

                                // Re-broadcast new public key
                                let public_key = {
                                    let store = keystore.lock().unwrap();
                                    store.get_our_public_key()
                                };

                                let msg = WireMessage {
                                    msg_type: MessageType::KeyExchange,
                                    payload: public_key.clone(),
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
                                };

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
                            let (key, chain_key) = {
                                let mut store = keystore.lock().unwrap();
                                store.get_or_create_sender_key(&group_id)
                            };
                            // Encode key + chain_key together as payload
                            let mut payload_bytes = Vec::with_capacity(64);
                            payload_bytes.extend_from_slice(&key);
                            payload_bytes.extend_from_slice(&chain_key);
                            let payload = BASE64.encode(&payload_bytes);

                            // Send sender key to each member via the relay
                            for member in &members {
                                let encrypted_payload = {
                                    let mut store = keystore.lock().unwrap();
                                    if let Ok(session) = store.get_session(member) {
                                        let msg_key = session.derive_send_key();
                                        match encrypt_message(&payload, &msg_key) {
                                            Ok(payload) => Some(payload),
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

                                let Some(encrypted_payload) = encrypted_payload else {
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
                                };
                                if let Ok(json) = serde_json::to_string(&msg) {
                                    if write.send(Message::Text(json.into())).await.is_ok() {
                                        let mut store = keystore.lock().unwrap();
                                        if let Ok(session) = store.get_session(member) {
                                            session.commit_send();
                                        }
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
                            };

                            if let Ok(json) = serde_json::to_string(&msg) {
                                if let Err(e) = write.send(Message::Text(json.into())).await {
                                    let _ = event_tx.send(NetworkEvent::Error {
                                        message: format!("Failed to send reaction: {}", e),
                                    });
                                }
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
            if let (Some(target_id), Some(emoji)) =
                (msg.reaction_to.clone(), msg.reaction_emoji.clone())
            {
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
                let mut store = keystore.lock().unwrap();
                if msg.channel.starts_with("group:") {
                    if store.has_sender_key(&msg.channel, &msg.meta.sender) {
                        let Some(msg_key) =
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
                                let _ = store.commit_group_recv(&msg.channel, &msg.meta.sender);
                                (plaintext, message_id)
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to decrypt group message from {}: {}",
                                    msg.meta.sender,
                                    e
                                );
                                audit_logger
                                    .lock()
                                    .unwrap()
                                    .log(SecurityEvent::DecryptionFailed {
                                        sender: msg.meta.sender.clone(),
                                        reason: e.to_string(),
                                    });
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
                    // Replay protection: extract nonce and check for duplicates
                    let mut extracted_nonce: Option<[u8; 12]> = None;
                    let decrypted = {
                        if let Ok(payload_bytes) = BASE64.decode(&msg.payload) {
                            if payload_bytes.len() >= 12 {
                                let mut nonce = [0u8; 12];
                                nonce.copy_from_slice(&payload_bytes[..12]);
                                if session.nonce_seen(&nonce) {
                                    tracing::warn!(
                                        "Replay attack detected from {}",
                                        msg.meta.sender
                                    );
                                    audit_logger.lock().unwrap().log(
                                        SecurityEvent::ReplayDetected {
                                            sender: msg.meta.sender.clone(),
                                            nonce: hex::encode(nonce),
                                        },
                                    );
                                    return;
                                }
                                extracted_nonce = Some(nonce);
                            }
                        }

                        let msg_key = session.derive_recv_key();
                        decrypt_message(&msg.payload, &msg_key)
                    };

                    match decrypted {
                        Ok(plaintext) => {
                            if let Some(nonce) = extracted_nonce {
                                session.record_nonce(&nonce);
                            }
                            session.commit_recv();
                            tracing::debug!("Decrypted message from {}", msg.meta.sender);
                            store.touch_session(&msg.meta.sender);

                            // Audit log
                            audit_logger
                                .lock()
                                .unwrap()
                                .log(SecurityEvent::MessageDecrypted {
                                    sender: msg.meta.sender.clone(),
                                    message_id: message_id.clone(),
                                });

                            (plaintext, message_id)
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to decrypt message from {}: {}",
                                msg.meta.sender,
                                e
                            );

                            // Audit log failure
                            audit_logger
                                .lock()
                                .unwrap()
                                .log(SecurityEvent::DecryptionFailed {
                                    sender: msg.meta.sender.clone(),
                                    reason: e.to_string(),
                                });

                            (format!("[Decryption failed: {}]", e), message_id)
                        }
                    }
                } else {
                    tracing::warn!("No session for encrypted message from {}", msg.meta.sender);

                    // Audit log
                    audit_logger
                        .lock()
                        .unwrap()
                        .log(SecurityEvent::SecurityWarning {
                            message: format!(
                                "Received encrypted message from {} without session",
                                msg.meta.sender
                            ),
                        });

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
            if let Some(recipient) = &msg.recipient {
                if recipient != local_username {
                    return;
                }
            }

            // Store peer's public key and establish session
            let their_username = msg.meta.sender.clone();
            let their_public_key = msg.payload.clone();

            let mut store = keystore.lock().unwrap();

            // Store their public key
            if let Err(e) = store.store_peer_public_key(&their_username, &their_public_key) {
                tracing::error!("Failed to store public key from {}: {}", their_username, e);
                return;
            }

            // Establish encrypted session
            if let Err(e) = store.establish_session(&their_username) {
                tracing::error!("Failed to establish session with {}: {}", their_username, e);
                return;
            }

            tracing::info!("✓ Established E2EE session with {}", their_username);

            // Audit log session establishment
            audit_logger
                .lock()
                .unwrap()
                .log(SecurityEvent::SessionEstablished {
                    peer: their_username.clone(),
                    public_key_fingerprint: their_public_key[..16].to_string(), // First 16 chars as fingerprint
                });

            // Notify UI layer
            let _ = event_tx.send(NetworkEvent::KeyExchangeReceived {
                username: their_username,
                public_key_b64: their_public_key,
            });
        }
        MessageType::SenderKey => {
            if msg.recipient.as_deref() != Some(local_username) {
                return;
            }

            if !msg.channel.starts_with("group:") {
                return;
            }

            // Receive a sender key distribution for group encryption
            let payload_b64 = if msg.encrypted {
                let mut store = keystore.lock().unwrap();
                let Ok(session) = store.get_session(&msg.meta.sender) else {
                    tracing::warn!(
                        "Ignoring encrypted sender key from {} without active session",
                        msg.meta.sender
                    );
                    return;
                };

                let msg_key = session.derive_recv_key();
                match decrypt_message(&msg.payload, &msg_key) {
                    Ok(plaintext) => {
                        session.commit_recv();
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

            if let Ok(payload_bytes) = BASE64.decode(payload_b64) {
                if payload_bytes.len() == 64 {
                    let mut key = [0u8; 32];
                    let mut chain_key = [0u8; 32];
                    key.copy_from_slice(&payload_bytes[..32]);
                    chain_key.copy_from_slice(&payload_bytes[32..]);

                    let mut store = keystore.lock().unwrap();
                    store.store_sender_key(&msg.channel, &msg.meta.sender, key, chain_key);
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

    let hex_str = hex::encode(&hash[..15]);
    hex_str
        .as_bytes()
        .chunks(5)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ")
}
