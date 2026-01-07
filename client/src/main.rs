// GhostWire Client - Main Entry Point
// This implements the CRITICAL async/sync split architecture:
// - Main thread: Runs the Ratatui UI loop (synchronous)
// - Network thread: Runs the WebSocket task (asynchronous via tokio::spawn)
// - Communication: mpsc unbounded channels

mod app;
mod config;
mod crypto;
mod errors;
mod keystore;
mod logging;
mod network;
mod security_audit;
mod ui;

use app::{App, ChatMessage, InputMode, User};
use chrono::Utc;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use network::{NetworkCommand, NetworkEvent};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::io;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Default server URL (can be overridden via CLI args)
const DEFAULT_SERVER_URL: &str = "wss://ghostwire.fly.dev/ws";

/// GhostWire - Ephemeral terminal chat client
#[derive(Parser)]
#[command(name = "ghostwire")]
#[command(author, version, about, long_about = None)]
#[command(after_help = "EXAMPLES:\n    ghostwire                          # Random username, default server\n    ghostwire alice                    # Custom username\n    ghostwire alice ws://localhost:8080/ws  # Custom server\n\nKEYBOARD SHORTCUTS:\n    Esc           Switch between chat and input modes\n    Tab           Switch channels\n    j/k ↓/↑       Scroll down/up (one line)\n    PgDn/PgUp     Scroll down/up (page)\n    G             Jump to bottom (latest)\n    g             Jump to top (oldest)\n    Ctrl+C        Quit")]
struct Cli {
    /// Username for the chat session (default: random ghost_XXXXXXXX)
    #[arg(value_name = "USERNAME")]
    username: Option<String>,

    /// WebSocket server URL
    #[arg(value_name = "SERVER_URL", default_value = DEFAULT_SERVER_URL)]
    server_url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging system
    if let Err(e) = logging::init_logging() {
        eprintln!("Warning: Could not initialize logging: {}", e);
    }
    
    // Load configuration from file (or create default)
    let config = config::load_config().unwrap_or_else(|e| {
        eprintln!("Warning: Could not load config: {}. Using defaults.", e);
        tracing::warn!("Could not load config: {}. Using defaults.", e);
        config::GhostWireConfig::default()
    });
    
    tracing::info!("GhostWire client starting");
    tracing::info!("Server URL: {}", config.default_server_url);
    
    // Parse command line arguments using clap
    let cli = Cli::parse();
    
    let username = cli.username.unwrap_or_else(|| {
        // Generate a random username if none provided
        format!("ghost_{}", &uuid::Uuid::new_v4().to_string()[..8])
    });
    
    // CLI args override config file
    let server_url = if cli.server_url != DEFAULT_SERVER_URL {
        cli.server_url
    } else {
        config.default_server_url.clone()
    };

    // Create the application state with configuration
    let mut app = App::new(username.clone());
    app.timestamp_format = config.timestamp_format.clone();

    // Create channels for communication between UI and network task
    // event_rx: UI receives events from network
    // command_tx: UI sends commands to network
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<NetworkEvent>();
    let (command_tx, command_rx) = mpsc::unbounded_channel::<NetworkCommand>();

    // Spawn the network task in a separate async runtime
    // This is the CRITICAL async/sync split!
    tracing::debug!("Spawning network task for user: {}", username);
    let network_handle = tokio::spawn(network::network_task(
        server_url.clone(),
        username.clone(),
        event_tx,
        command_rx,
    ));

    // Setup terminal for TUI
    tracing::debug!("Initializing terminal UI");
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main UI loop (synchronous, runs on main thread)
    tracing::info!("Starting main UI loop");
    let result = run_ui_loop(&mut terminal, &mut app, &mut event_rx, &command_tx);

    // Cleanup: Restore terminal
    tracing::debug!("Cleaning up terminal");
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Shutdown network task
    tracing::debug!("Shutting down network task");
    let _ = command_tx.send(NetworkCommand::Disconnect);
    let _ = network_handle.await;

    // Print any errors
    if let Err(err) = result {
        tracing::error!("Application error: {:?}", err);
        eprintln!("Error: {:?}", err);
    }

    tracing::info!("GhostWire client shutdown complete");
    Ok(())
}

/// Main UI event loop - runs synchronously on the main thread
fn run_ui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    event_rx: &mut mpsc::UnboundedReceiver<NetworkEvent>,
    command_tx: &mpsc::UnboundedSender<NetworkCommand>,
) -> anyhow::Result<()> {
    // Track uptime
    let mut last_uptime_update = Instant::now();
    
    // Initialize sysinfo for memory tracking
    let mut system = sysinfo::System::new_all();
    let pid = sysinfo::get_current_pid().expect("Failed to get current PID");
    let mut last_memory_update = Instant::now();
    
    // Track message cleanup for self-destruct (v0.3.0)
    let mut last_cleanup = Instant::now();
    
    loop {
        // Update frame timing for FPS calculation
        app.update_frame_time();
        
        // Cleanup expired messages every 5 seconds (v0.3.0)
        if last_cleanup.elapsed() >= Duration::from_secs(5) {
            app.cleanup_expired_messages();
            last_cleanup = Instant::now();
        }
        
        // Update memory usage every 500ms (don't need it every frame)
        if last_memory_update.elapsed() >= Duration::from_millis(500) {
            system.refresh_process(pid);
            app.update_memory_usage(&system, pid);
            last_memory_update = Instant::now();
        }
        
        // Render the UI
        terminal.draw(|f| ui::render(f, app))?;

        // Check for network events (non-blocking)
        while let Ok(event) = event_rx.try_recv() {
            handle_network_event(app, event);
        }

        // Check for terminal events (blocking with timeout)
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                handle_key_event(app, key.code, key.modifiers, command_tx)?;
            }
        }

        // Update uptime every second
        if last_uptime_update.elapsed() >= Duration::from_secs(1) {
            app.increment_uptime(1);
            app.update_network_activity();
            app.cleanup_typing_indicators();
            last_uptime_update = Instant::now();
        }
        
        // Check if we should quit
        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Handle keyboard events
fn handle_key_event(
    app: &mut App,
    key: KeyCode,
    _modifiers: KeyModifiers,
    command_tx: &mpsc::UnboundedSender<NetworkCommand>,
) -> anyhow::Result<()> {
    match app.input_mode {
        InputMode::Normal => {
            match key {
                // Quit
                KeyCode::Char('q') | KeyCode::Esc => {
                    app.quit();
                }
                // Enter edit mode
                KeyCode::Char('i') | KeyCode::Enter => {
                    app.enter_edit_mode();
                }
                // Scroll chat
                KeyCode::Char('j') | KeyCode::Down => {
                    app.scroll_down();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    app.scroll_up();
                }
                // Page scrolling (scroll by ~20 lines)
                KeyCode::PageDown => {
                    app.scroll_position = app.scroll_position.saturating_sub(20);
                }
                KeyCode::PageUp => {
                    app.scroll_position = app.scroll_position.saturating_add(20);
                }
                // Scroll to bottom
                KeyCode::Char('G') => {
                    app.scroll_to_bottom();
                }
                // Scroll to top
                KeyCode::Char('g') => {
                    app.scroll_to_top();
                }
                
                // Channel navigation
                KeyCode::Char('h') | KeyCode::Left => app.select_previous_channel(),
                KeyCode::Char('l') | KeyCode::Right => app.select_next_channel(),
                KeyCode::Tab => app.activate_selected_channel(),
                KeyCode::Char('#') => app.switch_channel("global".to_string()),
                
                // Create DM with selected user
                KeyCode::Char('d') => {
                    if !app.users.is_empty() {
                        if let Some(user) = app.users.get(app.selected_user) {
                            app.open_dm(user.username.clone());
                        }
                    }
                }
                
                // User selection (for DM creation)
                KeyCode::Char('J') => app.select_next_user(),
                KeyCode::Char('K') => app.select_previous_user(),
                
                _ => {}
            }
        }
        InputMode::Editing => {
            match key {
                // Exit edit mode
                KeyCode::Esc => {
                    app.exit_edit_mode();
                }
                // Send message
                KeyCode::Enter => {
                    let input = app.take_input();
                    if !input.is_empty() {
                        let channel_id = app.active_channel.clone();
                        
                        // Send to network task
                        let _ = command_tx.send(NetworkCommand::SendMessage {
                            content: input.clone(),
                            channel_id: channel_id.clone(),
                        });
                        
                        // Add to local chat immediately (optimistic update)
                        app.add_message(ChatMessage::new(
                            app.username.clone(),
                            input,
                            false,
                        ));
                        
                        // Update telemetry
                        app.telemetry.messages_sent += 1;
                        
                        // Stop typing indicator when sending
                        let _ = command_tx.send(NetworkCommand::SendTypingStatus {
                            channel_id: app.active_channel.clone(),
                            is_typing: false,
                        });
                    }
                    app.exit_edit_mode();
                }
                // Character input
                KeyCode::Char(c) => {
                    app.input_char(c);
                    
                    // Send typing indicator (throttled to 1 per second)
                    if app.should_send_typing_indicator() {
                        let _ = command_tx.send(NetworkCommand::SendTypingStatus {
                            channel_id: app.active_channel.clone(),
                            is_typing: true,
                        });
                        app.mark_typing_sent();
                    }
                }
                // Backspace
                KeyCode::Backspace => {
                    app.input_backspace();
                    
                    // Send typing indicator if not empty
                    if !app.input.is_empty() && app.should_send_typing_indicator() {
                        let _ = command_tx.send(NetworkCommand::SendTypingStatus {
                            channel_id: app.active_channel.clone(),
                            is_typing: true,
                        });
                        app.mark_typing_sent();
                    }
                }
                // Cursor movement
                KeyCode::Left => {
                    app.input_cursor_left();
                }
                KeyCode::Right => {
                    app.input_cursor_right();
                }
                _ => {}
            }
        }
    }

    Ok(())
}

/// Handle network events from the async task
fn handle_network_event(app: &mut App, event: NetworkEvent) {
    match event {
        NetworkEvent::Connected => {
            tracing::info!("Connected to server");
            app.set_connected(true);
        }
        NetworkEvent::Disconnected => {
            tracing::warn!("Disconnected from server");
            app.set_connected(false);
        }
        NetworkEvent::Message { sender, content, timestamp, channel_id, encrypted } => {
            tracing::debug!("Received message from {} in channel {} (encrypted: {})", sender, channel_id, encrypted);
            // Convert Unix timestamp to DateTime
            let datetime = chrono::DateTime::from_timestamp(timestamp, 0)
                .unwrap_or_else(Utc::now);
            
            // Create message with actual timestamp and encryption status
            let mut msg = ChatMessage::with_encryption(sender.clone(), content, encrypted);
            msg.timestamp = datetime;
            
            // Add user to roster if not already there (for user discovery)
            if !app.users.iter().any(|u| u.username == sender) && sender != app.username {
                app.add_user(User::new(sender.clone()));
            }
            
            // Route to the correct channel
            app.add_message_to_channel(&channel_id, msg);
            app.telemetry.messages_received += 1;
            
            // Update user activity
            app.update_user_activity(&sender);
        }
        NetworkEvent::UserJoined { username } => {
            app.add_user(User::new(username));
        }
        NetworkEvent::UserLeft { username } => {
            app.remove_user(&username);
        }
        NetworkEvent::SystemMessage { content } => {
            app.add_message(ChatMessage::system(content));
        }
        NetworkEvent::Error { message } => {
            // Parse error and create user-friendly message
            let user_error = errors::parse_error(&message);
            tracing::warn!("Error: {} (severity: {:?})", message, user_error.severity);
            
            // Map error severity to message severity
            let msg_severity = match user_error.severity {
                errors::ErrorSeverity::Info => app::MessageSeverity::Info,
                errors::ErrorSeverity::Warning => app::MessageSeverity::Warning,
                errors::ErrorSeverity::Error | errors::ErrorSeverity::Critical => app::MessageSeverity::Error,
            };
            
            // Add formatted error message to chat
            app.add_message(ChatMessage::system_with_severity(
                user_error.format_for_ui(),
                msg_severity
            ));
        }
        NetworkEvent::LatencyUpdate { latency_ms } => {
            app.update_latency(latency_ms);
        }
        NetworkEvent::Reconnecting { attempt, max_attempts } => {
            tracing::info!("Reconnecting: attempt {}/{}", attempt, max_attempts);
            app.add_message(ChatMessage::system(
                format!("Reconnecting... (attempt {}/{})", attempt, max_attempts)
            ));
        }
        NetworkEvent::TypingStatus { username, channel_id, is_typing } => {
            tracing::debug!("User {} typing status: {} in channel {}", username, is_typing, channel_id);
            app.set_user_typing(&channel_id, &username, is_typing);
        }
        NetworkEvent::KeyExchangeReceived { username } => {
            tracing::info!("✓ Key exchange complete with {}", username);
            app.add_message(ChatMessage::system_with_severity(
                format!("🔒 Secure session established with {}", username),
                app::MessageSeverity::Info,
            ));
        }
    }
}
