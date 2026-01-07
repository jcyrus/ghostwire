// GhostWire Client - Security Audit Logging
// Logs security-relevant events for audit trail (v0.3.0)

use chrono::Utc;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

/// Security event types
#[derive(Debug, Clone)]
pub enum SecurityEvent {
    /// E2EE session established with peer
    SessionEstablished { peer: String, public_key_fingerprint: String },
    
    /// Message decrypted successfully
    MessageDecrypted { sender: String, message_id: String },
    
    /// Decryption failed
    DecryptionFailed { sender: String, reason: String },
    
    /// Security warning
    SecurityWarning { message: String },
}

/// Security audit logger
pub struct SecurityAuditLogger {
    log_path: PathBuf,
    enabled: bool,
}

impl SecurityAuditLogger {
    /// Create a new security audit logger
    pub fn new(config_dir: &std::path::Path) -> Self {
        let log_path = config_dir.join("security_audit.log");
        
        Self {
            log_path,
            enabled: true,
        }
    }
    
    /// Log a security event
    pub fn log(&self, event: SecurityEvent) {
        if !self.enabled {
            return;
        }
        
        let timestamp = Utc::now().to_rfc3339();
        let event_str = format_event(&event);
        let log_line = format!("[{}] {}\n", timestamp, event_str);
        
        // Also log to tracing
        tracing::info!("SECURITY_AUDIT: {}", event_str);
        
        // Write to audit log file
        if let Err(e) = self.write_to_file(&log_line) {
            tracing::error!("Failed to write security audit log: {}", e);
        }
    }
    
    /// Write log entry to file
    fn write_to_file(&self, entry: &str) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;
        
        file.write_all(entry.as_bytes())?;
        file.flush()?;
        
        Ok(())
    }
    
    /// Disable audit logging (for testing or privacy)
    #[allow(dead_code)]
    pub fn disable(&mut self) {
        self.enabled = false;
    }
    
    /// Enable audit logging
    #[allow(dead_code)]
    pub fn enable(&mut self) {
        self.enabled = true;
    }
    
    /// Get path to audit log file
    pub fn log_path(&self) -> &std::path::Path {
        &self.log_path
    }
}

/// Format security event as human-readable string
fn format_event(event: &SecurityEvent) -> String {
    match event {
        SecurityEvent::SessionEstablished { peer, public_key_fingerprint } => {
            format!("E2EE_SESSION_ESTABLISHED: peer={}, fingerprint={}", peer, public_key_fingerprint)
        }
        SecurityEvent::MessageDecrypted { sender, message_id } => {
            format!("MESSAGE_DECRYPTED: sender={}, msg_id={}", sender, message_id)
        }
        SecurityEvent::DecryptionFailed { sender, reason } => {
            format!("DECRYPTION_FAILED: sender={}, reason={}", sender, reason)
        }
        SecurityEvent::SecurityWarning { message } => {
            format!("SECURITY_WARNING: {}", message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    
    #[test]
    fn test_format_event() {
        let event = SecurityEvent::SessionEstablished {
            peer: "alice".to_string(),
            public_key_fingerprint: "abcd1234".to_string(),
        };
        
        let formatted = format_event(&event);
        assert!(formatted.contains("E2EE_SESSION_ESTABLISHED"));
        assert!(formatted.contains("alice"));
        assert!(formatted.contains("abcd1234"));
    }
    
    #[test]
    fn test_logger_creation() {
        let temp_dir = std::env::temp_dir().join("ghostwire_test");
        fs::create_dir_all(&temp_dir).unwrap();
        
        let logger = SecurityAuditLogger::new(&temp_dir);
        assert!(logger.enabled);
        assert_eq!(logger.log_path(), temp_dir.join("security_audit.log"));
        
        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
