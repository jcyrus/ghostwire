// GhostWire Client - Application State
// This module manages the core application state and business logic

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Maximum number of messages to keep in memory
const MAX_MESSAGES: usize = 1000;

/// Maximum number of users to display
const MAX_USERS: usize = 100;

/// Message types for the GhostWire protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageType {
    #[serde(rename = "MSG")]
    Message,
    #[serde(rename = "AUTH")]
    Auth,
    #[serde(rename = "SYS")]
    System,
    #[serde(rename = "TYPING")]
    Typing,
    /// Key exchange message for E2EE (v0.3.0)
    #[serde(rename = "KEY_EXCHANGE")]
    KeyExchange,
}

/// Metadata for each message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMeta {
    pub sender: String,
    pub timestamp: i64,
}

/// Wire protocol message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireMessage {
    #[serde(rename = "type")]
    pub msg_type: MessageType,
    /// Payload: plaintext for AUTH/SYS, base64 encrypted for MSG, public key for KEY_EXCHANGE
    pub payload: String,
    /// Channel ID: "global", "dm:user1:user2", or "group:name"
    #[serde(default = "default_channel")]
    pub channel: String,
    pub meta: MessageMeta,
    /// For TYPING messages: true = typing, false = stopped typing
    #[serde(default)]
    pub is_typing: bool,
    /// Encryption status (v0.3.0): true if payload is encrypted
    #[serde(default)]
    pub encrypted: bool,
    /// Recipient for encrypted messages (username or "all" for broadcast)
    #[serde(default)]
    pub recipient: Option<String>,
}

/// Default channel is global for backward compatibility
fn default_channel() -> String {
    "global".to_string()
}

/// Message severity for system messages (errors, warnings, info)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSeverity {
    Info,
    Warning,
    Error,
}

/// Internal chat message representation
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub sender: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub is_system: bool,
    pub severity: Option<MessageSeverity>,
}

impl ChatMessage {
    pub fn new(sender: String, content: String, is_system: bool) -> Self {
        Self {
            sender,
            content,
            timestamp: Utc::now(),
            is_system,
            severity: None,
        }
    }

    pub fn system(content: String) -> Self {
        Self {
            sender: "SYSTEM".to_string(),
            content,
            timestamp: Utc::now(),
            is_system: true,
            severity: Some(MessageSeverity::Info),
        }
    }
    
    pub fn system_with_severity(content: String, severity: MessageSeverity) -> Self {
        Self {
            sender: "SYSTEM".to_string(),
            content,
            timestamp: Utc::now(),
            is_system: true,
            severity: Some(severity),
        }
    }
}

/// User in the roster
#[derive(Debug, Clone)]
pub struct User {
    pub username: String,
    pub is_online: bool,
    pub last_seen: DateTime<Utc>,
}

impl User {
    pub fn new(username: String) -> Self {
        Self {
            username,
            is_online: true,
            last_seen: Utc::now(),
        }
    }
    
    /// Check if user is idle (no activity for more than 5 minutes)
    pub fn is_idle(&self) -> bool {
        if !self.is_online {
            return false; // Offline users aren't considered idle
        }
        
        let idle_threshold = chrono::Duration::minutes(5);
        let now = Utc::now();
        let time_since_activity = now.signed_duration_since(self.last_seen);
        
        time_since_activity > idle_threshold
    }
}

/// Channel type variants
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelType {
    /// Global channel - all users
    Global,
    /// Direct message with another user
    DirectMessage { other_user: String },
    /// Group channel with multiple users
    #[allow(dead_code)]
    Group { name: String, members: Vec<String> },
}

/// A chat channel
#[derive(Debug, Clone)]
pub struct Channel {
    /// Unique channel ID
    pub id: String,
    /// Type of channel
    pub channel_type: ChannelType,
    /// Messages in this channel
    pub messages: VecDeque<ChatMessage>,
    /// Number of unread messages
    pub unread_count: usize,
    /// Users currently typing in this channel (username -> last typing timestamp)
    pub typing_users: std::collections::HashMap<String, std::time::Instant>,
}

impl Channel {
    /// Create a new global channel
    pub fn global() -> Self {
        Self {
            id: "global".to_string(),
            channel_type: ChannelType::Global,
            messages: VecDeque::with_capacity(MAX_MESSAGES),
            unread_count: 0,
            typing_users: std::collections::HashMap::new(),
        }
    }
    
    /// Create a new DM channel
    pub fn dm(current_user: &str, other_user: String) -> Self {
        // Sort usernames alphabetically for consistent channel IDs
        let (user1, user2) = if current_user < other_user.as_str() {
            (current_user, other_user.as_str())
        } else {
            (other_user.as_str(), current_user)
        };
        
        Self {
            id: format!("dm:{}:{}", user1, user2),
            channel_type: ChannelType::DirectMessage { other_user },
            messages: VecDeque::with_capacity(MAX_MESSAGES),
            unread_count: 0,
            typing_users: std::collections::HashMap::new(),
        }
    }
    
    /// Create a new group channel    
    /// Create a group channel (reserved for future use)
    #[allow(dead_code)]
    pub fn group(name: String, members: Vec<String>) -> Self {
        Self {
            id: format!("group:{}", name),
            channel_type: ChannelType::Group { name: name.clone(), members },
            messages: VecDeque::with_capacity(MAX_MESSAGES),
            unread_count: 0,
            typing_users: std::collections::HashMap::new(),
        }
    }
    
    /// Add a message to this channel
    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push_back(message);
        
        // Keep only the last MAX_MESSAGES
        if self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
        }
    }
    
    /// Get display name for this channel
    pub fn display_name(&self) -> String {
        match &self.channel_type {
            ChannelType::Global => "# global".to_string(),
            ChannelType::DirectMessage { other_user } => format!("@ {}", other_user),
            ChannelType::Group { name, .. } => format!("# {}", name),
        }
    }
}

/// Telemetry data for monitoring
#[derive(Debug, Clone)]
pub struct Telemetry {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub connection_uptime: u64, // seconds
    pub latency_ms: u64,
    /// Network activity history (messages per second over last 60 seconds)
    pub network_activity: Vec<u64>,
    /// Current frames per second
    pub fps: f64,
    /// Memory usage in bytes
    pub memory_usage: u64,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            connection_uptime: 0,
            latency_ms: 0,
            network_activity: vec![0; 60], // 60 seconds of history
            fps: 0.0,
            memory_usage: 0,
        }
    }
}

/// UI input mode
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,   // Navigation mode
    Editing,  // Typing a message
}

/// Timestamp display format
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimestampFormat {
    /// 24-hour format: 14:30:45
    Time24h,
    /// 12-hour format: 2:30:45 PM
    Time12h,
    /// Date and time: 2025-12-05 14:30:45
    DateTime,
    /// Relative time: "2 minutes ago"
    Relative,
}

impl TimestampFormat {
    /// Format a timestamp according to this format
    pub fn format(&self, timestamp: &DateTime<Utc>) -> String {
        match self {
            TimestampFormat::Time24h => timestamp.format("%H:%M:%S").to_string(),
            TimestampFormat::Time12h => timestamp.format("%I:%M:%S %p").to_string(),
            TimestampFormat::DateTime => timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
            TimestampFormat::Relative => {
                let now = Utc::now();
                let duration = now.signed_duration_since(*timestamp);
                
                if duration.num_seconds() < 60 {
                    "just now".to_string()
                } else if duration.num_minutes() < 60 {
                    format!("{}m ago", duration.num_minutes())
                } else if duration.num_hours() < 24 {
                    format!("{}h ago", duration.num_hours())
                } else {
                    format!("{}d ago", duration.num_days())
                }
            }
        }
    }
}

impl Default for TimestampFormat {
    fn default() -> Self {
        TimestampFormat::Time24h
    }
}

/// Main application state
pub struct App {
    /// Current username
    pub username: String,
    
    /// All channels (keyed by channel ID)
    pub channels: std::collections::HashMap<String, Channel>,
    
    /// Currently active channel ID
    pub active_channel: String,
    
    /// Selected channel index in sidebar
    pub selected_channel: usize,
    
    /// Current input buffer
    pub input: String,
    
    /// Input cursor position
    pub input_cursor: usize,
    
    /// Current input mode
    pub input_mode: InputMode,
    
    /// User roster (all known users)
    pub users: Vec<User>,
    
    /// Selected user index in roster (for creating DMs)
    pub selected_user: usize,
    
    /// Chat scroll position (for active channel)
    pub scroll_position: usize,
    
    /// Telemetry data
    pub telemetry: Telemetry,
    
    /// Connection status
    pub is_connected: bool,
    
    /// Should quit the application
    pub should_quit: bool,
    
    /// Timestamp display format
    pub timestamp_format: TimestampFormat,
    
    /// Last time we sent a typing indicator
    pub last_typing_sent: Option<std::time::Instant>,
    
    /// Frame timing for FPS calculation (stores last 10 frame times)
    pub frame_times: VecDeque<std::time::Duration>,
    
    /// Last frame render time
    pub last_frame_time: Option<std::time::Instant>,
}

impl App {
    /// Create a new application instance
    pub fn new(username: String) -> Self {
        // Create global channel
        let mut global_channel = Channel::global();
        global_channel.add_message(ChatMessage::system(
            format!("Welcome to GhostWire, {}!", username)
        ));
        
        // Initialize channels map
        let mut channels = std::collections::HashMap::new();
        channels.insert("global".to_string(), global_channel);
        
        Self {
            username,
            channels,
            active_channel: "global".to_string(),
            selected_channel: 0,
            input: String::new(),
            input_cursor: 0,
            input_mode: InputMode::Normal,
            users: Vec::with_capacity(MAX_USERS),
            selected_user: 0,
            scroll_position: 0,
            telemetry: Telemetry::default(),
            is_connected: false,
            should_quit: false,
            timestamp_format: TimestampFormat::default(),
            last_typing_sent: None,
            frame_times: VecDeque::with_capacity(10),
            last_frame_time: None,
        }
    }
    
    /// Add a message to the active channel
    pub fn add_message(&mut self, message: ChatMessage) {
        if let Some(channel) = self.channels.get_mut(&self.active_channel) {
            channel.add_message(message);
            
            // Auto-scroll to bottom only if already at/near bottom (within 5 messages)
            if self.scroll_position <= 5 {
                self.scroll_to_bottom();
            }
        }
    }
    
    /// Add a message to a specific channel
    pub fn add_message_to_channel(&mut self, channel_id: &str, message: ChatMessage) {
        // Auto-create DM channel if it doesn't exist
        if channel_id.starts_with("dm:") && !self.channels.contains_key(channel_id) {
            // Extract the other user's name from the channel ID
            // Format: "dm:user1:user2"
            let parts: Vec<&str> = channel_id.split(':').collect();
            if parts.len() == 3 {
                let other_user = if parts[1] == self.username {
                    parts[2].to_string()
                } else {
                    parts[1].to_string()
                };
                
                let channel = Channel::dm(&self.username, other_user);
                self.channels.insert(channel_id.to_string(), channel);
            }
        }
        
        if let Some(channel) = self.channels.get_mut(channel_id) {
            channel.add_message(message);
            
            // Increment unread count if not active channel
            if channel_id != self.active_channel {
                channel.unread_count += 1;
            } else {
                // Auto-scroll to bottom only if already at/near bottom (within 5 messages)
                if self.scroll_position <= 5 {
                    self.scroll_to_bottom();
                }
            }
        }
    }
    
    /// Add a user to the roster
    pub fn add_user(&mut self, user: User) {
        // Don't add yourself
        if user.username == self.username {
            return;
        }
        
        // Check if user already exists
        if !self.users.iter().any(|u| u.username == user.username) {
            self.users.push(user.clone());
            self.add_message(ChatMessage::system(
                format!("{} joined the chat", user.username)
            ));
        }
    }
    
    /// Remove a user from the roster
    pub fn remove_user(&mut self, username: &str) {
        if let Some(pos) = self.users.iter().position(|u| u.username == username) {
            self.users.remove(pos);
            self.add_message(ChatMessage::system(
                format!("{} left the chat", username)
            ));
            
            // Adjust selected user if necessary
            if self.selected_user >= self.users.len() && self.selected_user > 0 {
                self.selected_user = self.users.len() - 1;
            }
        }
    }
    
    /// Update a user's last_seen timestamp
    pub fn update_user_activity(&mut self, username: &str) {
        if let Some(user) = self.users.iter_mut().find(|u| u.username == username) {
            user.last_seen = Utc::now();
            user.is_online = true;
        }
    }
    
    /// Mark a user as offline (for future presence tracking)
    #[allow(dead_code)]
    pub fn mark_user_offline(&mut self, username: &str) {
        if let Some(user) = self.users.iter_mut().find(|u| u.username == username) {
            user.is_online = false;
        }
    }
    
    /// Enter editing mode
    pub fn enter_edit_mode(&mut self) {
        self.input_mode = InputMode::Editing;
        self.input_cursor = self.input.len();
    }
    
    /// Exit editing mode
    pub fn exit_edit_mode(&mut self) {
        self.input_mode = InputMode::Normal;
    }
    
    /// Add a character to the input buffer
    pub fn input_char(&mut self, c: char) {
        self.input.insert(self.input_cursor, c);
        self.input_cursor += 1;
    }
    
    /// Delete character before cursor
    pub fn input_backspace(&mut self) {
        if self.input_cursor > 0 {
            self.input.remove(self.input_cursor - 1);
            self.input_cursor -= 1;
        }
    }
    
    /// Move cursor left
    pub fn input_cursor_left(&mut self) {
        if self.input_cursor > 0 {
            self.input_cursor -= 1;
        }
    }
    
    /// Move cursor right
    pub fn input_cursor_right(&mut self) {
        if self.input_cursor < self.input.len() {
            self.input_cursor += 1;
        }
    }
    
    /// Get the current input and clear the buffer
    pub fn take_input(&mut self) -> String {
        let input = self.input.clone();
        self.input.clear();
        self.input_cursor = 0;
        input
    }
    
    /// Scroll chat up (away from bottom, into history)
    pub fn scroll_up(&mut self) {
        // Scroll up by 3 lines for better responsiveness
        self.scroll_position = self.scroll_position.saturating_add(3);
    }
    
    /// Scroll chat down (toward bottom, toward latest)
    pub fn scroll_down(&mut self) {
        // Scroll down by 3 lines
        self.scroll_position = self.scroll_position.saturating_sub(3);
    }
    
    /// Scroll to bottom of chat (latest messages)
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_position = 0;
    }
    
    /// Scroll to top of chat (oldest messages)
    pub fn scroll_to_top(&mut self) {
        // Set to a very large number - the rendering will clamp it
        self.scroll_position = 100000;
    }
    
    /// Get list of channel IDs sorted for display
    pub fn get_channel_list(&self) -> Vec<String> {
        let mut channels: Vec<String> = self.channels.keys().cloned().collect();
        channels.sort_by(|a, b| {
            // Global first, then DMs alphabetically
            match (a.as_str(), b.as_str()) {
                ("global", _) => std::cmp::Ordering::Less,
                (_, "global") => std::cmp::Ordering::Greater,
                _ => a.cmp(b),
            }
        });
        channels
    }
    
    /// Switch to a different channel
    pub fn switch_channel(&mut self, channel_id: String) {
        if self.channels.contains_key(&channel_id) {
            self.active_channel = channel_id.clone();
            self.scroll_to_bottom();
            
            // Clear unread count
            if let Some(channel) = self.channels.get_mut(&channel_id) {
                channel.unread_count = 0;
            }
        }
    }
    
    /// Create or switch to a DM channel
    pub fn open_dm(&mut self, other_user: String) {
        let channel = Channel::dm(&self.username, other_user.clone());
        let channel_id = channel.id.clone();
        
        // Add channel if it doesn't exist
        if !self.channels.contains_key(&channel_id) {
            self.channels.insert(channel_id.clone(), channel);
        }
        
        // Switch to it
        self.switch_channel(channel_id);
    }
    
    /// Select previous channel
    pub fn select_previous_channel(&mut self) {
        if self.selected_channel > 0 {
            self.selected_channel -= 1;
        }
    }
    
    /// Select next channel
    pub fn select_next_channel(&mut self) {
        let channel_count = self.channels.len();
        if self.selected_channel < channel_count.saturating_sub(1) {
            self.selected_channel += 1;
        }
    }
    
    /// Switch to selected channel
    pub fn activate_selected_channel(&mut self) {
        let channels = self.get_channel_list();
        if let Some(channel_id) = channels.get(self.selected_channel) {
            self.switch_channel(channel_id.clone());
        }
    }
    
    /// Select previous user in roster
    pub fn select_previous_user(&mut self) {
        if self.selected_user > 0 {
            self.selected_user -= 1;
        }
    }
    
    /// Select next user in roster
    pub fn select_next_user(&mut self) {
        if self.selected_user < self.users.len().saturating_sub(1) {
            self.selected_user += 1;
        }
    }
    
    /// Update connection status
    pub fn set_connected(&mut self, connected: bool) {
        if connected != self.is_connected {
            self.is_connected = connected;
            let status = if connected { "Connected" } else { "Disconnected" };
            self.add_message(ChatMessage::system(status.to_string()));
        }
    }
    
    /// Update telemetry (for future batch updates)
    #[allow(dead_code)]
    pub fn update_telemetry(&mut self, telemetry: Telemetry) {
        self.telemetry = telemetry;
    }
    
    /// Increment connection uptime (call this periodically)
    pub fn increment_uptime(&mut self, seconds: u64) {
        self.telemetry.connection_uptime += seconds;
    }
    
    /// Update network activity history (call every second)
    pub fn update_network_activity(&mut self) {
        // Calculate messages in the last second
        let current_total = self.telemetry.messages_sent + self.telemetry.messages_received;
        
        // Shift history left and add new value
        self.telemetry.network_activity.rotate_left(1);
        if let Some(last) = self.telemetry.network_activity.last_mut() {
            // Store the delta (messages in last second)
            static mut LAST_TOTAL: u64 = 0;
            unsafe {
                *last = current_total.saturating_sub(LAST_TOTAL);
                LAST_TOTAL = current_total;
            }
        }
    }
    
    /// Update network latency (for future ping/pong implementation)
    #[allow(dead_code)]
    pub fn update_latency(&mut self, latency_ms: u64) {
        self.telemetry.latency_ms = latency_ms;
    }
    
    /// Check if we should send a typing indicator (throttle to max 1 per second)
    pub fn should_send_typing_indicator(&self) -> bool {
        if let Some(last_sent) = self.last_typing_sent {
            last_sent.elapsed() >= std::time::Duration::from_secs(1)
        } else {
            true
        }
    }
    
    /// Mark that we sent a typing indicator
    pub fn mark_typing_sent(&mut self) {
        self.last_typing_sent = Some(std::time::Instant::now());
    }
    
    /// Set a user's typing state in a channel
    pub fn set_user_typing(&mut self, channel_id: &str, username: &str, is_typing: bool) {
        if let Some(channel) = self.channels.get_mut(channel_id) {
            if is_typing {
                channel.typing_users.insert(username.to_string(), std::time::Instant::now());
            } else {
                channel.typing_users.remove(username);
            }
        }
    }
    
    /// Clean up stale typing indicators (older than 3 seconds)
    pub fn cleanup_typing_indicators(&mut self) {
        let timeout = std::time::Duration::from_secs(3);
        let now = std::time::Instant::now();
        
        for channel in self.channels.values_mut() {
            channel.typing_users.retain(|_, last_time| {
                now.duration_since(*last_time) < timeout
            });
        }
    }
    
    /// Get list of users typing in active channel (excluding self)
    pub fn get_typing_users(&self) -> Vec<String> {
        if let Some(channel) = self.channels.get(&self.active_channel) {
            channel.typing_users
                .keys()
                .filter(|user| *user != &self.username)
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }
    
    /// Get total number of messages in active channel
    pub fn get_total_messages(&self) -> usize {
        if let Some(channel) = self.channels.get(&self.active_channel) {
            channel.messages.len()
        } else {
            0
        }
    }
    
    /// Check if scrolled to bottom of active channel
    #[allow(dead_code)]
    pub fn is_at_bottom(&self) -> bool {
        self.scroll_position == 0
    }
    
    /// Get number of messages below current scroll position
    pub fn get_messages_below(&self) -> usize {
        self.scroll_position
    }
    
    /// Update frame timing and calculate FPS
    pub fn update_frame_time(&mut self) {
        let now = std::time::Instant::now();
        
        if let Some(last_time) = self.last_frame_time {
            let frame_duration = now.duration_since(last_time);
            
            // Add to frame times buffer
            self.frame_times.push_back(frame_duration);
            
            // Keep only last 10 frames for rolling average
            if self.frame_times.len() > 10 {
                self.frame_times.pop_front();
            }
            
            // Calculate average frame time and FPS
            if !self.frame_times.is_empty() {
                let total_time: std::time::Duration = self.frame_times.iter().sum();
                let avg_frame_time = total_time.as_secs_f64() / self.frame_times.len() as f64;
                
                // Calculate FPS (avoiding division by zero)
                if avg_frame_time > 0.0 {
                    self.telemetry.fps = 1.0 / avg_frame_time;
                }
            }
        }
        
        self.last_frame_time = Some(now);
    }
    
    /// Update memory usage using sysinfo
    pub fn update_memory_usage(&mut self, system: &sysinfo::System, pid: sysinfo::Pid) {
        if let Some(process) = system.process(pid) {
            self.telemetry.memory_usage = process.memory();
        }
    }
    
    /// Quit the application
    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}
