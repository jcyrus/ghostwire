// GhostWire Client - Configuration Management
// Handles loading and saving user preferences from ~/.config/ghostwire/config.toml

use crate::app::TimestampFormat;
use serde::{Deserialize, Serialize};

/// User configuration for GhostWire client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhostWireConfig {
    /// Default server URL to connect to
    #[serde(default = "default_server_url")]
    pub default_server_url: String,
    
    /// Auto-reconnect settings
    #[serde(default)]
    pub auto_reconnect: AutoReconnectConfig,
    
    /// Timestamp display format
    #[serde(default)]
    pub timestamp_format: TimestampFormat,
    
    /// Enable sending typing indicators
    #[serde(default = "default_true")]
    pub send_typing_indicators: bool,
    
    /// Log retention in days
    #[serde(default = "default_log_retention_days")]
    pub log_retention_days: u32,
}

/// Auto-reconnect configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoReconnectConfig {
    /// Enable auto-reconnect
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// Maximum number of reconnection attempts (0 = unlimited)
    #[serde(default = "default_max_reconnect_attempts")]
    pub max_attempts: u32,
    
    /// Initial backoff delay in seconds
    #[serde(default = "default_initial_backoff")]
    pub initial_backoff_secs: u64,
    
    /// Maximum backoff delay in seconds
    #[serde(default = "default_max_backoff")]
    pub max_backoff_secs: u64,
}

impl Default for AutoReconnectConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 10,
            initial_backoff_secs: 1,
            max_backoff_secs: 16,
        }
    }
}

impl Default for GhostWireConfig {
    fn default() -> Self {
        Self {
            default_server_url: default_server_url(),
            auto_reconnect: AutoReconnectConfig::default(),
            timestamp_format: TimestampFormat::default(),
            send_typing_indicators: true,
            log_retention_days: 7,
        }
    }
}

// Default value functions for serde
fn default_server_url() -> String {
    "wss://ghostwire.fly.dev/ws".to_string()
}

fn default_true() -> bool {
    true
}

fn default_log_retention_days() -> u32 {
    7
}

fn default_max_reconnect_attempts() -> u32 {
    10
}

fn default_initial_backoff() -> u64 {
    1
}

fn default_max_backoff() -> u64 {
    16
}

/// Load configuration from disk, or create default if not exists
pub fn load_config() -> Result<GhostWireConfig, confy::ConfyError> {
    confy::load("ghostwire", "config")
}

/// Save configuration to disk
#[allow(dead_code)]
pub fn save_config(config: &GhostWireConfig) -> Result<(), confy::ConfyError> {
    confy::store("ghostwire", "config", config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GhostWireConfig::default();
        assert_eq!(config.default_server_url, "wss://ghostwire.fly.dev/ws");
        assert!(config.auto_reconnect.enabled);
        assert_eq!(config.auto_reconnect.max_attempts, 10);
        assert!(config.send_typing_indicators);
        assert_eq!(config.log_retention_days, 7);
    }

    #[test]
    fn test_serialization() {
        let config = GhostWireConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("default_server_url"));
        assert!(toml_str.contains("auto_reconnect"));
        assert!(toml_str.contains("timestamp_format"));
    }
}
