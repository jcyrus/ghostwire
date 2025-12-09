// GhostWire Client - UI Components
// This module handles all Ratatui rendering logic

use crate::app::{App, InputMode};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Gauge, List, ListItem, Paragraph,
    },
    Frame,
};

/// Main UI render function
pub fn render(f: &mut Frame, app: &App) {
    // Create the main layout: Left sidebar | Middle chat | Right sidebar
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Left: Channels
            Constraint::Percentage(60), // Middle: Chat
            Constraint::Percentage(20), // Right: Telemetry
        ])
        .split(f.size());

    // Render each section
    render_channel_list(f, app, chunks[0]);
    render_chat_area(f, app, chunks[1]);
    render_telemetry(f, app, chunks[2]);
}

/// Render the channel list (left sidebar)
fn render_channel_list(f: &mut Frame, app: &App, area: Rect) {
    // Split into channels (top) and users (bottom)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(60), // Channels
            Constraint::Percentage(40), // Users
        ])
        .split(area);
    
    // Render channels
    render_channels(f, app, chunks[0]);
    
    // Render users
    render_users(f, app, chunks[1]);
}

/// Render channels section
fn render_channels(f: &mut Frame, app: &App, area: Rect) {
    // Get sorted channel list
    let channel_ids = app.get_channel_list();
    
    // Create channel list items
    let channels: Vec<ListItem> = channel_ids
        .iter()
        .map(|channel_id| {
            if let Some(channel) = app.channels.get(channel_id) {
                let display_name = channel.display_name();
                
                // Add unread count if any
                let content = if channel.unread_count > 0 {
                    format!("{} ({})", display_name, channel.unread_count)
                } else {
                    display_name
                };
                
                // Highlight active channel
                let style = if channel_id == &app.active_channel {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else if channel.unread_count > 0 {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Green)
                };
                
                ListItem::new(content).style(style)
            } else {
                ListItem::new("???").style(Style::default().fg(Color::Red))
            }
        })
        .collect();

    let title = format!(" Channels ({}) ", app.channels.len());
    let channel_list = List::new(channels)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green)),
        )
        .style(Style::default().fg(Color::Green));

    f.render_widget(channel_list, area);
}

/// Render users section
fn render_users(f: &mut Frame, app: &App, area: Rect) {
    use chrono::Utc;
    
    // Create user list items
    let users: Vec<ListItem> = app
        .users
        .iter()
        .enumerate()
        .map(|(i, user)| {
            // Determine user status: online, idle, or offline
            let (status_icon, status_color) = if !user.is_online {
                ("○", Color::DarkGray) // Offline
            } else if user.is_idle() {
                ("◐", Color::Yellow) // Idle (half-circle)
            } else {
                ("●", Color::Green) // Online and active
            };
            
            // Calculate time since last seen for offline/idle users
            let last_seen_text = if !user.is_online {
                let duration = Utc::now().signed_duration_since(user.last_seen);
                let mins = duration.num_minutes();
                let hours = duration.num_hours();
                let days = duration.num_days();
                
                if days > 0 {
                    format!(" ({}d)", days)
                } else if hours > 0 {
                    format!(" ({}h)", hours)
                } else if mins > 0 {
                    format!(" ({}m)", mins)
                } else {
                    "".to_string()
                }
            } else if user.is_idle() {
                // Show idle time for idle users
                let duration = Utc::now().signed_duration_since(user.last_seen);
                let mins = duration.num_minutes();
                format!(" (idle {}m)", mins)
            } else {
                String::new()
            };
            
            let content = format!("{} {}{}", status_icon, user.username, last_seen_text);
            
            let style = if i == app.selected_user {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(status_color)
            };
            
            ListItem::new(content).style(style)
        })
        .collect();

    let title = format!(" Users ({}) [J/K to select, d for DM] ", app.users.len());
    let users_list = List::new(users)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().fg(Color::Green));

    f.render_widget(users_list, area);
}

/// Render the chat area (middle section)
fn render_chat_area(f: &mut Frame, app: &App, area: Rect) {
    // Split chat area into messages and input with typing indicator
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),      // Chat messages (at least 5 lines)
            Constraint::Length(3),   // Input box
            Constraint::Length(1),   // Typing indicator
        ])
        .split(area);

    render_messages(f, app, chunks[0]);
    render_input_box(f, app, chunks[1]);
    render_typing_indicator(f, app, chunks[2]);
}

/// Render chat messages
fn render_messages(f: &mut Frame, app: &App, area: Rect) {
    // Calculate available width for message content (subtract borders and some padding)
    let available_width = area.width.saturating_sub(4) as usize;
    
    // Get ALL messages from active channel (don't skip/take here - let List handle it)
    let messages: Vec<ListItem> = if let Some(channel) = app.channels.get(&app.active_channel) {
        channel.messages
            .iter()
            .flat_map(|msg| {
                let timestamp = app.timestamp_format.format(&msg.timestamp);
                
                if msg.is_system {
                    // System messages with color-coded severity
                    use crate::app::MessageSeverity;
                    
                    let (color, symbol) = match msg.severity {
                        Some(MessageSeverity::Info) => (Color::Cyan, "ℹ"),
                        Some(MessageSeverity::Warning) => (Color::Yellow, "⚠"),
                        Some(MessageSeverity::Error) => (Color::Red, "✖"),
                        None => (Color::Red, "⚠"), // Default fallback
                    };
                    
                    let prefix = format!("[{}] {} ", timestamp, symbol);
                    let prefix_len = prefix.chars().count();
                    let content_width = available_width.saturating_sub(prefix_len);
                    
                    // Wrap system message content
                    let wrapped_lines = wrap_message_content(&msg.content, content_width);
                    
                    wrapped_lines.into_iter().enumerate().map(|(i, line)| {
                        if i == 0 {
                            // First line with prefix
                            ListItem::new(Line::from(vec![
                                Span::styled(prefix.clone(), Style::default().fg(Color::DarkGray)),
                                Span::styled(line, Style::default().fg(color).add_modifier(Modifier::BOLD)),
                            ]))
                        } else {
                            // Continuation lines with indent
                            let indent = " ".repeat(prefix_len);
                            ListItem::new(Line::from(vec![
                                Span::raw(indent),
                                Span::styled(line, Style::default().fg(color).add_modifier(Modifier::BOLD)),
                            ]))
                        }
                    }).collect::<Vec<_>>()
                } else {
                    // Regular messages
                    let sender_style = if msg.sender == app.username {
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    };
                    
                    // Add lock icon for encrypted messages (v0.3.0)
                    let lock_icon = if msg.encrypted { "🔒 " } else { "" };
                    let prefix = format!("[{}] {}{}: ", timestamp, lock_icon, msg.sender);
                    let prefix_len = prefix.chars().count();
                    let content_width = available_width.saturating_sub(prefix_len);
                    
                    // Wrap message content
                    let wrapped_lines = wrap_message_content(&msg.content, content_width);
                    
                    wrapped_lines.into_iter().enumerate().map(|(i, line)| {
                        if i == 0 {
                            // First line with full prefix
                            ListItem::new(Line::from(vec![
                                Span::styled(
                                    format!("[{}] ", timestamp),
                                    Style::default().fg(Color::DarkGray),
                                ),
                                Span::styled(format!("{}: ", msg.sender), sender_style),
                                Span::styled(line, Style::default().fg(Color::White)),
                            ]))
                        } else {
                            // Continuation lines with indent
                            let indent = " ".repeat(prefix_len);
                            ListItem::new(Line::from(vec![
                                Span::raw(indent),
                                Span::styled(line, Style::default().fg(Color::White)),
                            ]))
                        }
                    }).collect::<Vec<_>>()
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let connection_status = if app.is_connected {
        Span::styled(" ● CONNECTED ", Style::default().fg(Color::Green))
    } else {
        Span::styled(" ○ DISCONNECTED ", Style::default().fg(Color::Red))
    };
    
    // Get active channel display name
    let channel_name = app.channels.get(&app.active_channel)
        .map(|ch| ch.display_name())
        .unwrap_or_else(|| "Unknown".to_string());
    
    // Calculate scroll position info
    let total_messages = app.get_total_messages();
    let messages_below = app.get_messages_below();
    
    // Build title with scroll position indicator
    let mut title_spans = vec![
        Span::raw(" "),
        Span::styled(channel_name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        connection_status,
    ];
    
    // Add scroll position indicator if there are messages
    if total_messages > 0 {
        let position_text = if app.scroll_position == 0 {
            format!(" [Latest] ")
        } else {
            format!(" [↑{}] ", app.scroll_position)
        };
        title_spans.push(Span::styled(
            position_text,
            Style::default().fg(Color::DarkGray)
        ));
    }
    
    let title = Line::from(title_spans);

    // Calculate scroll offset: when scroll_position=0, we want to show the bottom
    // The List widget scrolls from top, so we need to calculate the offset
    let total_items = messages.len();
    let visible_lines = area.height.saturating_sub(2) as usize; // Subtract borders
    
    let scroll_offset = if total_items > visible_lines {
        // Calculate how far from the top we should scroll
        // When scroll_position=0 (at bottom), offset should be (total - visible)
        // When scroll_position increases, offset decreases
        let max_offset = total_items.saturating_sub(visible_lines);
        let desired_offset = max_offset.saturating_sub(app.scroll_position);
        // Clamp to valid range [0, max_offset]
        desired_offset.min(max_offset)
    } else {
        0
    };

    // Slice messages to show only the visible window starting from scroll_offset
    let visible_messages: Vec<ListItem> = if total_items > visible_lines {
        messages.into_iter()
            .skip(scroll_offset)
            .take(visible_lines)
            .collect()
    } else {
        messages
    };
    
    let messages_list = List::new(visible_messages)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green)),
        )
        .style(Style::default().fg(Color::Green));

    f.render_widget(messages_list, area);
    
    // Render scroll bar if there are enough items to scroll
    if total_items > visible_lines {
        render_scroll_bar(f, area, app.scroll_position, total_items, visible_lines);
    }
    
    // Show "more messages below" indicator if not at bottom
    if messages_below > 0 {
        let indicator_text = format!(" ↓ {} more ", messages_below);
        let indicator = Paragraph::new(indicator_text)
            .style(Style::default()
                .fg(Color::Yellow)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD))
            .alignment(ratatui::layout::Alignment::Center);
        
        // Position at bottom of messages area (just above border)
        let indicator_area = ratatui::layout::Rect {
            x: area.x + area.width / 4,
            y: area.y + area.height - 2,
            width: area.width / 2,
            height: 1,
        };
        
        f.render_widget(indicator, indicator_area);
    }
}

/// Render input box only
fn render_input_box(f: &mut Frame, app: &App, area: Rect) {
    let input_style = match app.input_mode {
        InputMode::Normal => Style::default().fg(Color::Green),
        InputMode::Editing => Style::default().fg(Color::Yellow),
    };

    let mode_indicator = match app.input_mode {
        InputMode::Normal => " [NORMAL] ",
        InputMode::Editing => " [EDIT] ",
    };

    // Wrap text if it's too long
    let available_width = area.width.saturating_sub(4) as usize; // Subtract borders and padding
    let input_text = wrap_text(&app.input, available_width);

    let input = Paragraph::new(input_text)
        .style(input_style)
        .block(
            Block::default()
                .title(mode_indicator)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(input_style),
        )
        .wrap(ratatui::widgets::Wrap { trim: false });

    f.render_widget(input, area);

    // Show cursor in edit mode
    if app.input_mode == InputMode::Editing {
        // Calculate cursor position with wrapping
        let lines_before = app.input[..app.input_cursor.min(app.input.len())]
            .chars()
            .filter(|&c| c == '\n')
            .count();
        let current_line_start = app.input[..app.input_cursor.min(app.input.len())]
            .rfind('\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let col_in_line = app.input_cursor.saturating_sub(current_line_start);
        
        f.set_cursor(
            area.x + (col_in_line % available_width) as u16 + 1,
            area.y + (lines_before + col_in_line / available_width) as u16 + 1,
        );
    }
}

/// Render typing indicator
fn render_typing_indicator(f: &mut Frame, app: &App, area: Rect) {
    let typing_users = app.get_typing_users();
    if !typing_users.is_empty() {
        let typing_text = if typing_users.len() == 1 {
            format!("{} is typing...", typing_users[0])
        } else if typing_users.len() == 2 {
            format!("{} and {} are typing...", typing_users[0], typing_users[1])
        } else {
            format!("{} and {} others are typing...", typing_users[0], typing_users.len() - 1)
        };
        
        let typing_indicator = Paragraph::new(typing_text)
            .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC));
        
        f.render_widget(typing_indicator, area);
    }
}

/// Render telemetry (right sidebar)
fn render_telemetry(f: &mut Frame, app: &App, area: Rect) {
    // Split telemetry area into sections
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // Connection uptime
            Constraint::Length(3),   // Latency
            Constraint::Length(4),   // Performance (FPS + Memory)
            Constraint::Length(7),   // Statistics (expanded)
            Constraint::Min(3),      // Network activity chart
            Constraint::Length(3),   // Server time
        ])
        .split(area);

    // Connection uptime
    let uptime_hours = app.telemetry.connection_uptime / 3600;
    let uptime_mins = (app.telemetry.connection_uptime % 3600) / 60;
    let uptime_secs = app.telemetry.connection_uptime % 60;
    
    let uptime = Paragraph::new(format!(
        "{}h {}m {}s",
        uptime_hours, uptime_mins, uptime_secs
    ))
    .style(Style::default().fg(Color::Green))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .title(" Uptime ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green)),
    );
    f.render_widget(uptime, chunks[0]);

    // Latency gauge
    let latency_percent = (app.telemetry.latency_ms.min(500) as f64 / 500.0 * 100.0) as u16;
    let latency_color = if app.telemetry.latency_ms < 50 {
        Color::Green
    } else if app.telemetry.latency_ms < 150 {
        Color::Yellow
    } else {
        Color::Red
    };

    let latency = Gauge::default()
        .block(
            Block::default()
                .title(format!(" Latency: {}ms ", app.telemetry.latency_ms))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green)),
        )
        .gauge_style(Style::default().fg(latency_color))
        .percent(latency_percent);
    f.render_widget(latency, chunks[1]);

    // Performance metrics (FPS and Memory)
    let memory_mb = app.telemetry.memory_usage as f64 / 1024.0 / 1024.0;
    let fps_color = if app.telemetry.fps >= 30.0 {
        Color::Green
    } else if app.telemetry.fps >= 15.0 {
        Color::Yellow
    } else {
        Color::Red
    };
    
    let performance_text = format!(
        "FPS: {:.1}\nMem: {:.1} MB",
        app.telemetry.fps,
        memory_mb
    );
    
    let performance = Paragraph::new(performance_text)
        .style(Style::default().fg(fps_color))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" Performance ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green)),
        );
    f.render_widget(performance, chunks[2]);

    // Expanded statistics
    let active_channel_name = app.channels.get(&app.active_channel)
        .map(|ch| ch.display_name())
        .unwrap_or_else(|| "Unknown".to_string());
    
    let stats_text = format!(
        "↑ Sent: {}\n↓ Recv: {}\n📊 Bytes: {} / {}\n📺 Channel: {}\n👥 Users: {} | Channels: {}",
        app.telemetry.messages_sent,
        app.telemetry.messages_received,
        format_bytes(app.telemetry.bytes_sent),
        format_bytes(app.telemetry.bytes_received),
        active_channel_name,
        app.users.len(),
        app.channels.len(),
    );
    
    let stats = Paragraph::new(stats_text)
        .style(Style::default().fg(Color::Green))
        .block(
            Block::default()
                .title(" Statistics ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green)),
        );
    f.render_widget(stats, chunks[3]);

    // Compact network activity chart using Braille sparkline
    let activity_data: Vec<u64> = app.telemetry.network_activity.clone();
    let max_activity = *activity_data.iter().max().unwrap_or(&1).max(&1);
    
    // Take last 30 data points for sparkline
    let recent_data: Vec<u64> = activity_data
        .iter()
        .rev()
        .take(30)
        .rev()
        .copied()
        .collect();
    
    let sparkline_text = create_sparkline(&recent_data, max_activity);
    let title = format!(" Activity (max: {}/s) ", max_activity);
    
    let activity_chart = Paragraph::new(sparkline_text)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green)),
        )
        .style(Style::default().fg(Color::Green))
        .alignment(Alignment::Left);
    
    f.render_widget(activity_chart, chunks[4]);
    
    // Server time
    use chrono::Utc;
    let now = Utc::now();
    let time_str = now.format("%H:%M:%S UTC").to_string();
    
    let time_widget = Paragraph::new(time_str)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" Server Time ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green)),
        );
    f.render_widget(time_widget, chunks[5]);
}

/// Render a vertical scroll bar indicator
fn render_scroll_bar(f: &mut Frame, area: Rect, scroll_position: usize, total_items: usize, visible_items: usize) {
    if total_items == 0 || visible_items >= total_items {
        return; // No need for scroll bar
    }
    
    let bar_height = area.height.saturating_sub(2) as usize; // Subtract borders
    if bar_height == 0 {
        return;
    }
    
    // Calculate scroll bar position
    let scroll_percentage = if total_items > visible_items {
        let max_scroll = total_items.saturating_sub(visible_items);
        let current_scroll = scroll_position.min(max_scroll);
        1.0 - (current_scroll as f32 / max_scroll as f32)
    } else {
        1.0
    };
    
    let bar_position = ((bar_height as f32) * scroll_percentage) as usize;
    let bar_position = bar_position.min(bar_height.saturating_sub(1));
    
    // Render the scroll bar at the right edge of the area
    let scroll_bar_x = area.x + area.width - 2;
    
    for i in 0..bar_height {
        let y = area.y + 1 + i as u16;
        let symbol = if i == bar_position {
            "█"
        } else {
            "│"
        };
        
        let style = if i == bar_position {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        
        f.render_widget(
            Paragraph::new(symbol).style(style),
            Rect {
                x: scroll_bar_x,
                y,
                width: 1,
                height: 1,
            }
        );
    }
}

/// Format bytes into human-readable format
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

/// Wrap text to fit within a specific width
fn wrap_text(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return text.to_string();
    }
    
    let mut result = String::new();
    let mut current_line_len = 0;
    
    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        
        if current_line_len == 0 {
            // First word on the line
            result.push_str(word);
            current_line_len = word_len;
        } else if current_line_len + 1 + word_len <= max_width {
            // Word fits on current line
            result.push(' ');
            result.push_str(word);
            current_line_len += 1 + word_len;
        } else {
            // Word doesn't fit, start new line
            result.push('\n');
            result.push_str(word);
            current_line_len = word_len;
        }
    }
    
    result
}

/// Wrap message content into multiple lines
fn wrap_message_content(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_len = 0;
    
    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        
        // If word itself is longer than max_width, break it up
        if word_len > max_width {
            if !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::new();
                current_len = 0;
            }
            
            // Break long word into chunks
            let chars: Vec<char> = word.chars().collect();
            for chunk in chars.chunks(max_width) {
                lines.push(chunk.iter().collect());
            }
            continue;
        }
        
        if current_len == 0 {
            // First word on the line
            current_line.push_str(word);
            current_len = word_len;
        } else if current_len + 1 + word_len <= max_width {
            // Word fits on current line
            current_line.push(' ');
            current_line.push_str(word);
            current_len += 1 + word_len;
        } else {
            // Word doesn't fit, start new line
            lines.push(current_line);
            current_line = word.to_string();
            current_len = word_len;
        }
    }
    
    // Don't forget the last line
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    
    // Return at least one line even if empty
    if lines.is_empty() {
        lines.push(String::new());
    }
    
    lines
}

/// Create a sparkline visualization using Braille characters
fn create_sparkline(data: &[u64], max_value: u64) -> String {
    // Braille characters for 9 levels (0-8)
    const BRAILLE_CHARS: [&str; 9] = [" ", "⡀", "⡄", "⡆", "⡇", "⣇", "⣧", "⣷", "⣿"];
    
    if data.is_empty() || max_value == 0 {
        return " ".repeat(30);
    }
    
    data.iter()
        .map(|&value| {
            // Normalize to 0-8 range
            let normalized = ((value as f64 / max_value as f64) * 8.0).round() as usize;
            let index = normalized.min(8);
            BRAILLE_CHARS[index]
        })
        .collect()
}
