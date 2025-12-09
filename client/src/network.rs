// GhostWire Client - Network Layer
// This module handles WebSocket communication in a separate async task

use crate::app::{MessageMeta, MessageType, WireMessage};
use crate::crypto::{decrypt_message, encrypt_message};
use crate::keystore::KeyStore;
use crate::security_audit::{SecurityAuditLogger, SecurityEvent};
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
        encrypted: bool, // v0.3.0: true if message was encrypted
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
        public_key: String,
    },
}

/// Messages sent from the UI to the network task
#[derive(Debug, Clone)]
pub enum NetworkCommand {
    /// Send a chat message to a specific channel
    SendMessage { content: String, channel_id: String },
    
    /// Authenticate with username (for reconnection scenarios)
    #[allow(dead_code)]
    Authenticate { username: String },
    
    /// Send typing status
    SendTypingStatus { channel_id: String, is_typing: bool },
    
    /// Send key exchange (v0.3.0 E2EE)
    SendKeyExchange { recipient: Option<String> },
    
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
        let config_dir = directories::ProjectDirs::from("com", "ghostwire", "GhostWire")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        SecurityAuditLogger::new(&config_dir)
    }));
    
    tracing::info!("Security audit logging enabled at {:?}", 
        audit_logger.lock().unwrap().log_path());
    
    // Send our public key on connect (broadcast for key exchange)
    let our_public_key = {
        let store = keystore.lock().unwrap();
        store.get_our_public_key()
    };
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
                initial_backoff_secs * 2u64.pow((attempt - 2) as u32),
                max_backoff_secs
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
                        message: "Max reconnection attempts reached. Please restart the client.".to_string(),
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
    };

    if let Ok(json) = serde_json::to_string(&auth_msg) {
        if let Err(e) = write.send(Message::Text(json)).await {
            let _ = event_tx.send(NetworkEvent::Error {
                message: format!("Failed to authenticate: {}", e),
            });
            return;
        }
    }
    
    // Send key exchange message to announce our public key (v0.3.0)
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
    };
    
    if let Ok(json) = serde_json::to_string(&key_exchange_msg) {
        if let Err(e) = write.send(Message::Text(json)).await {
            tracing::warn!("Failed to send key exchange: {}", e);
        }
    }

    // Heartbeat interval - send ping every 30 seconds to keep connection alive
    let mut heartbeat = interval(Duration::from_secs(30));
    heartbeat.tick().await; // First tick completes immediately

    // Track ping timestamps for latency measurement
    let ping_timestamps: Arc<Mutex<HashMap<Vec<u8>, Instant>>> = Arc::new(Mutex::new(HashMap::new()));
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
                
                if let Err(e) = write.send(Message::Ping(ping_data)).await {
                    let _ = event_tx.send(NetworkEvent::Error {
                        message: format!("Failed to send heartbeat: {}", e),
                    });
                    break;
                }
            }

            // Handle incoming messages from server
            Some(msg_result) = read.next() => {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        // Parse the wire message
                        if let Ok(wire_msg) = serde_json::from_str::<WireMessage>(&text) {
                            handle_wire_message(wire_msg, &event_tx, &keystore, &audit_logger);
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
                            if let Some(sent_time) = timestamps.remove(&data) {
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
                    NetworkCommand::SendMessage { content, channel_id } => {
                        // Determine recipient from channel_id (dm:user1:user2)
                        let recipient = if channel_id.starts_with("dm:") {
                            let parts: Vec<&str> = channel_id.split(':').collect();
                            parts.iter()
                                .find(|&&u| u != username)
                                .map(|&u| u.to_string())
                        } else {
                            None
                        };
                        
                        // Try to encrypt if we have a session with recipient
                        let (payload, encrypted) = if let Some(ref recip) = recipient {
                            let mut store = keystore.lock().unwrap();
                            if store.has_session(recip) {
                                match store.get_session(recip) {
                                    Ok(session) => {
                                        match encrypt_message(&content, &session.session_keys.encryption_key) {
                                            Ok(encrypted_payload) => {
                                                tracing::debug!("Encrypted message to {}", recip);
                                                (encrypted_payload, true)
                                            }
                                            Err(e) => {
                                                tracing::warn!("Encryption failed: {}, sending plaintext", e);
                                                (content.clone(), false)
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("Failed to get session: {}, sending plaintext", e);
                                        (content.clone(), false)
                                    }
                                }
                            } else {
                                tracing::debug!("No session with {}, sending plaintext", recip);
                                (content.clone(), false)
                            }
                        } else {
                            // Group/global messages are plaintext for now
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
                        };

                        if let Ok(json) = serde_json::to_string(&msg) {
                            if let Err(e) = write.send(Message::Text(json)).await {
                                let _ = event_tx.send(NetworkEvent::Error {
                                    message: format!("Failed to send message: {}", e),
                                });
                            }
                        }
                    }
                    NetworkCommand::Authenticate { username: new_username } => {
                        let msg = WireMessage {
                            msg_type: MessageType::Auth,
                            payload: new_username.clone(),
                            channel: "global".to_string(),
                            meta: MessageMeta {
                                sender: new_username,
                                timestamp: chrono::Utc::now().timestamp(),
                            },
                            is_typing: false,
                            encrypted: false,
                            recipient: None,
                        };

                        if let Ok(json) = serde_json::to_string(&msg) {
                            if let Err(e) = write.send(Message::Text(json)).await {
                                let _ = event_tx.send(NetworkEvent::Error {
                                    message: format!("Failed to authenticate: {}", e),
                                });
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
                        };

                        if let Ok(json) = serde_json::to_string(&msg) {
                            if let Err(e) = write.send(Message::Text(json)).await {
                                tracing::debug!("Failed to send typing status: {}", e);
                            }
                        }
                    }
                    NetworkCommand::SendKeyExchange { recipient } => {
                        let public_key = {
                            let store = keystore.lock().unwrap();
                            store.get_our_public_key()
                        };
                        
                        let msg = WireMessage {
                            msg_type: MessageType::KeyExchange,
                            payload: public_key,
                            channel: "global".to_string(),
                            meta: MessageMeta {
                                sender: username.clone(),
                                timestamp: chrono::Utc::now().timestamp(),
                            },
                            is_typing: false,
                            encrypted: false,
                            recipient,
                        };
                        
                        if let Ok(json) = serde_json::to_string(&msg) {
                            if let Err(e) = write.send(Message::Text(json)).await {
                                tracing::warn!("Failed to send key exchange: {}", e);
                            } else {
                                tracing::info!("Sent key exchange message");
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
) {
    match msg.msg_type {
        MessageType::Message => {
            // Decrypt message if it's encrypted
            let (content, message_id) = if msg.encrypted {
                let message_id = uuid::Uuid::new_v4().to_string();
                let mut store = keystore.lock().unwrap();
                if let Ok(session) = store.get_session(&msg.meta.sender) {
                    match decrypt_message(&msg.payload, &session.session_keys.encryption_key) {
                        Ok(plaintext) => {
                            tracing::debug!("Decrypted message from {}", msg.meta.sender);
                            session.last_message_at = chrono::Utc::now();
                            
                            // Audit log
                            audit_logger.lock().unwrap().log(SecurityEvent::MessageDecrypted {
                                sender: msg.meta.sender.clone(),
                                message_id: message_id.clone(),
                            });
                            
                            (plaintext, message_id)
                        }
                        Err(e) => {
                            tracing::error!("Failed to decrypt message from {}: {}", msg.meta.sender, e);
                            
                            // Audit log failure
                            audit_logger.lock().unwrap().log(SecurityEvent::DecryptionFailed {
                                sender: msg.meta.sender.clone(),
                                reason: e.to_string(),
                            });
                            
                            (format!("[Decryption failed: {}]", e), message_id)
                        }
                    }
                } else {
                    tracing::warn!("No session for encrypted message from {}", msg.meta.sender);
                    
                    // Audit log
                    audit_logger.lock().unwrap().log(SecurityEvent::SecurityWarning {
                        message: format!("Received encrypted message from {} without session", msg.meta.sender),
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
            audit_logger.lock().unwrap().log(SecurityEvent::SessionEstablished {
                peer: their_username.clone(),
                public_key_fingerprint: their_public_key[..16].to_string(), // First 16 chars as fingerprint
            });
            
            // Notify UI layer
            let _ = event_tx.send(NetworkEvent::KeyExchangeReceived {
                username: their_username,
                public_key: their_public_key,
            });
        }
    }
}
