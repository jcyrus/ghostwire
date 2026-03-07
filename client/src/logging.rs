// GhostWire Client - Logging System
// Sets up tracing with file appender and console output

use std::path::PathBuf;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize the logging system
///
/// Logs are written to:
/// - ~/.config/ghostwire/logs/ghostwire.log (rotating daily)
///
/// Log level can be controlled via RUST_LOG environment variable:
/// - RUST_LOG=debug ghostwire
/// - RUST_LOG=info ghostwire
/// - RUST_LOG=ghostwire=debug ghostwire
pub fn init_logging() -> anyhow::Result<()> {
    // Get log directory
    let log_dir = get_log_dir()?;

    // Create log directory if it doesn't exist
    std::fs::create_dir_all(&log_dir)?;

    // Create daily rotating file appender
    let file_appender = RollingFileAppender::new(Rotation::DAILY, log_dir.clone(), "ghostwire.log");

    // Create the file layer
    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false) // No ANSI colors in log files
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true);

    // Create env filter
    // Default to "info" level, but allow override via RUST_LOG
    let env_filter = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info"))?;

    // Build and initialize the subscriber
    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .try_init()?;

    tracing::info!("GhostWire client logging initialized");
    tracing::info!("Logs directory: {}", log_dir.display());

    Ok(())
}

/// Get the log directory path
fn get_log_dir() -> anyhow::Result<PathBuf> {
    // Use directories crate to get cross-platform config directory
    let config_dir = directories::ProjectDirs::from("com", "jcyrus", "ghostwire")
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    let log_dir = config_dir.config_dir().join("logs");
    Ok(log_dir)
}

/// Clean up old log files
/// Keeps only logs from the last `retention_days` days
#[allow(dead_code)]
pub fn cleanup_old_logs(retention_days: u32) -> anyhow::Result<()> {
    let log_dir = get_log_dir()?;

    if !log_dir.exists() {
        return Ok(());
    }

    let cutoff_time = std::time::SystemTime::now()
        - std::time::Duration::from_secs(retention_days as u64 * 24 * 60 * 60);

    let entries = std::fs::read_dir(&log_dir)?;
    let mut deleted_count = 0;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if modified < cutoff_time && std::fs::remove_file(&path).is_ok() {
                        deleted_count += 1;
                        tracing::debug!("Deleted old log file: {}", path.display());
                    }
                }
            }
        }
    }

    if deleted_count > 0 {
        tracing::info!("Cleaned up {} old log files", deleted_count);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_log_dir() {
        let log_dir = get_log_dir().unwrap();
        assert!(log_dir.to_string_lossy().contains("ghostwire"));
        assert!(log_dir.to_string_lossy().contains("logs"));
    }
}
