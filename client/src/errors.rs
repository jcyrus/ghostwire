// GhostWire Client - Error Handling and Recovery
// Provides user-friendly error messages with troubleshooting hints

#![allow(dead_code)]

use std::fmt;

/// Error severity levels for UI display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Informational - no action needed
    Info,
    /// Warning - may cause issues but not critical
    Warning,
    /// Error - functionality impaired but recoverable
    Error,
    /// Critical - application cannot continue
    Critical,
}

/// Categorized error types for better user communication
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorType {
    /// Connection-related errors
    Connection(ConnectionError),
    /// Authentication failures
    Authentication(AuthError),
    /// Network communication errors
    Network(NetworkError),
    /// Configuration errors
    Configuration(ConfigError),
    /// Terminal/UI errors
    Terminal(TerminalError),
    /// File system errors
    FileSystem(FileSystemError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionError {
    CannotConnect,
    Timeout,
    Disconnected,
    Refused,
    HostNotFound,
    InvalidUrl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    InvalidCredentials,
    Timeout,
    ServerRejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkError {
    SendFailed,
    ReceiveFailed,
    MessageTooLarge,
    ProtocolError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    CannotRead,
    InvalidFormat,
    MissingField,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalError {
    InitFailed,
    RenderFailed,
    InputFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSystemError {
    CannotCreateDir,
    CannotWriteFile,
    PermissionDenied,
}

/// User-friendly error message with troubleshooting hints
#[allow(dead_code)]
pub struct UserError {
    pub severity: ErrorSeverity,
    #[allow(dead_code)]
    pub error_type: ErrorType,
    pub message: String,
    pub hint: Option<String>,
    #[allow(dead_code)]
    pub recoverable: bool,
}

impl UserError {
    /// Create a new user error
    pub fn new(severity: ErrorSeverity, error_type: ErrorType, message: String) -> Self {
        let (hint, recoverable) = Self::get_hint_and_recovery(&error_type);
        
        Self {
            severity,
            error_type,
            message,
            hint,
            recoverable,
        }
    }
    
    /// Get troubleshooting hint and recovery status for an error type
    fn get_hint_and_recovery(error_type: &ErrorType) -> (Option<String>, bool) {
        match error_type {
            ErrorType::Connection(ConnectionError::CannotConnect) => (
                Some("Check your internet connection and server URL. Try: ghostwire --help".to_string()),
                true
            ),
            ErrorType::Connection(ConnectionError::Timeout) => (
                Some("Server is not responding. Check if the server is running and try again.".to_string()),
                true
            ),
            ErrorType::Connection(ConnectionError::Disconnected) => (
                Some("Connection lost. Auto-reconnect will attempt to restore connection.".to_string()),
                true
            ),
            ErrorType::Connection(ConnectionError::Refused) => (
                Some("Server refused connection. Verify the server address and port.".to_string()),
                true
            ),
            ErrorType::Connection(ConnectionError::HostNotFound) => (
                Some("Cannot resolve hostname. Check DNS settings or use IP address.".to_string()),
                true
            ),
            ErrorType::Connection(ConnectionError::InvalidUrl) => (
                Some("Invalid WebSocket URL. Format: ws://host:port/path or wss://host:port/path".to_string()),
                false
            ),
            
            ErrorType::Authentication(AuthError::InvalidCredentials) => (
                Some("Username may be invalid. Try a different username.".to_string()),
                true
            ),
            ErrorType::Authentication(AuthError::Timeout) => (
                Some("Authentication timed out. Check network connection and retry.".to_string()),
                true
            ),
            ErrorType::Authentication(AuthError::ServerRejected) => (
                Some("Server rejected authentication. Contact server administrator.".to_string()),
                false
            ),
            
            ErrorType::Network(NetworkError::SendFailed) => (
                Some("Cannot send message. Check connection status.".to_string()),
                true
            ),
            ErrorType::Network(NetworkError::ReceiveFailed) => (
                Some("Cannot receive messages. Connection may be unstable.".to_string()),
                true
            ),
            ErrorType::Network(NetworkError::MessageTooLarge) => (
                Some("Message exceeds size limit. Try sending a shorter message.".to_string()),
                false
            ),
            ErrorType::Network(NetworkError::ProtocolError) => (
                Some("Protocol error. You may need to update your client.".to_string()),
                false
            ),
            
            ErrorType::Configuration(ConfigError::CannotRead) => (
                Some("Cannot read config file. It will be created with defaults.".to_string()),
                true
            ),
            ErrorType::Configuration(ConfigError::InvalidFormat) => (
                Some("Config file has invalid format. Delete ~/.config/ghostwire/config.toml to reset.".to_string()),
                true
            ),
            ErrorType::Configuration(ConfigError::MissingField) => (
                Some("Config file is missing required fields. Using defaults.".to_string()),
                true
            ),
            
            ErrorType::Terminal(TerminalError::InitFailed) => (
                Some("Cannot initialize terminal. Ensure you're running in a compatible terminal emulator.".to_string()),
                false
            ),
            ErrorType::Terminal(TerminalError::RenderFailed) => (
                Some("Cannot render UI. Try resizing your terminal window.".to_string()),
                true
            ),
            ErrorType::Terminal(TerminalError::InputFailed) => (
                Some("Cannot read keyboard input. Terminal may not be in raw mode.".to_string()),
                false
            ),
            
            ErrorType::FileSystem(FileSystemError::CannotCreateDir) => (
                Some("Cannot create directory. Check file permissions.".to_string()),
                true
            ),
            ErrorType::FileSystem(FileSystemError::CannotWriteFile) => (
                Some("Cannot write to file. Check disk space and permissions.".to_string()),
                true
            ),
            ErrorType::FileSystem(FileSystemError::PermissionDenied) => (
                Some("Permission denied. Try running with appropriate permissions.".to_string()),
                false
            ),
        }
    }
    
    /// Format error for display in UI
    pub fn format_for_ui(&self) -> String {
        let severity_symbol = match self.severity {
            ErrorSeverity::Info => "ℹ",
            ErrorSeverity::Warning => "⚠",
            ErrorSeverity::Error => "✖",
            ErrorSeverity::Critical => "⛔",
        };
        
        let mut output = format!("{} {}", severity_symbol, self.message);
        
        if let Some(hint) = &self.hint {
            output.push_str(&format!("\n💡 {}", hint));
        }
        
        output
    }
    
    /// Get color for UI based on severity
    #[allow(dead_code)]
    pub fn get_color(&self) -> ratatui::style::Color {
        match self.severity {
            ErrorSeverity::Info => ratatui::style::Color::Cyan,
            ErrorSeverity::Warning => ratatui::style::Color::Yellow,
            ErrorSeverity::Error => ratatui::style::Color::Red,
            ErrorSeverity::Critical => ratatui::style::Color::Magenta,
        }
    }
}

impl fmt::Display for UserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_for_ui())
    }
}

/// Parse technical error messages into user-friendly errors
pub fn parse_error(error_msg: &str) -> UserError {
    let error_lower = error_msg.to_lowercase();
    
    // Connection errors
    if error_lower.contains("connection refused") {
        return UserError::new(
            ErrorSeverity::Error,
            ErrorType::Connection(ConnectionError::Refused),
            "Cannot connect to server - connection refused".to_string()
        );
    }
    
    if error_lower.contains("timed out") || error_lower.contains("timeout") {
        return UserError::new(
            ErrorSeverity::Warning,
            ErrorType::Connection(ConnectionError::Timeout),
            "Connection timed out".to_string()
        );
    }
    
    if error_lower.contains("dns") || error_lower.contains("name or service not known") 
        || error_lower.contains("nodename nor servname provided") {
        return UserError::new(
            ErrorSeverity::Error,
            ErrorType::Connection(ConnectionError::HostNotFound),
            "Cannot resolve hostname".to_string()
        );
    }
    
    if error_lower.contains("invalid") && (error_lower.contains("url") || error_lower.contains("uri")) {
        return UserError::new(
            ErrorSeverity::Error,
            ErrorType::Connection(ConnectionError::InvalidUrl),
            "Invalid server URL format".to_string()
        );
    }
    
    if error_lower.contains("failed to connect") || error_lower.contains("connection failed") {
        return UserError::new(
            ErrorSeverity::Error,
            ErrorType::Connection(ConnectionError::CannotConnect),
            "Failed to connect to server".to_string()
        );
    }
    
    // Network errors
    if error_lower.contains("failed to send") || error_lower.contains("send error") {
        return UserError::new(
            ErrorSeverity::Warning,
            ErrorType::Network(NetworkError::SendFailed),
            "Failed to send message".to_string()
        );
    }
    
    if error_lower.contains("failed to receive") || error_lower.contains("receive error") {
        return UserError::new(
            ErrorSeverity::Warning,
            ErrorType::Network(NetworkError::ReceiveFailed),
            "Failed to receive message".to_string()
        );
    }
    
    // Config errors
    if error_lower.contains("config") && error_lower.contains("load") {
        return UserError::new(
            ErrorSeverity::Warning,
            ErrorType::Configuration(ConfigError::CannotRead),
            "Cannot load configuration file".to_string()
        );
    }
    
    // File system errors
    if error_lower.contains("permission denied") || error_lower.contains("access denied") {
        return UserError::new(
            ErrorSeverity::Error,
            ErrorType::FileSystem(FileSystemError::PermissionDenied),
            "Permission denied".to_string()
        );
    }
    
    // Generic fallback
    UserError::new(
        ErrorSeverity::Error,
        ErrorType::Network(NetworkError::ProtocolError),
        format!("Error: {}", error_msg)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_connection_refused() {
        let error = parse_error("Connection refused");
        assert_eq!(error.severity, ErrorSeverity::Error);
        assert!(matches!(error.error_type, ErrorType::Connection(ConnectionError::Refused)));
    }

    #[test]
    fn test_parse_timeout() {
        let error = parse_error("Connection timed out");
        assert_eq!(error.severity, ErrorSeverity::Warning);
        assert!(matches!(error.error_type, ErrorType::Connection(ConnectionError::Timeout)));
    }

    #[test]
    fn test_format_for_ui() {
        let error = UserError::new(
            ErrorSeverity::Error,
            ErrorType::Connection(ConnectionError::CannotConnect),
            "Test error".to_string()
        );
        let formatted = error.format_for_ui();
        assert!(formatted.contains("✖"));
        assert!(formatted.contains("Test error"));
        assert!(formatted.contains("💡"));
    }
}
