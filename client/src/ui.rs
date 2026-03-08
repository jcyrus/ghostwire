// GhostWire Client - UI Components
// This module handles all Ratatui rendering logic

use crate::app::{App, InputMode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame,
};
use std::hash::{Hash, Hasher};

/// Main UI render function
pub fn render(f: &mut Frame, app: &App) {
    // Create the main layout: Left sidebar | Middle chat | Right sidebar
    let constraints = if app.show_telemetry {
        [
            Constraint::Percentage(20), // Left: Channels
            Constraint::Percentage(60), // Middle: Chat
            Constraint::Percentage(20), // Right: Telemetry
        ]
    } else {
        [
            Constraint::Percentage(20), // Left: Channels
            Constraint::Percentage(80), // Middle: Chat (expanded)
            Constraint::Length(0),      // Right: Telemetry hidden
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(f.area());

    // Render each section
    render_channel_list(f, app, chunks[0]);
    render_chat_area(f, app, chunks[1]);
    if app.show_telemetry {
        render_telemetry(f, app, chunks[2]);
    }
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

            let verified_badge = if user.verified { " ✓" } else { "" };
            let content = format!(
                "{} {}{}{}",
                status_icon, user.username, verified_badge, last_seen_text
            );

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
    // Split chat area into messages and a single footer so the bordered input
    // panel aligns with the sidebars at the bottom edge.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // Chat messages (at least 5 lines)
            Constraint::Length(4), // Input footer with inline status/hints
        ])
        .split(area);

    render_messages(f, app, chunks[0]);
    render_input_box(f, app, chunks[1]);
    render_typing_indicator(f, app, chunks[1]);
}

/// Render chat messages
fn render_messages(f: &mut Frame, app: &App, area: Rect) {
    // Calculate available width for message content (subtract borders and some padding)
    let available_width = area.width.saturating_sub(4) as usize;

    // Get ALL messages from active channel (don't skip/take here - let List handle it)
    let messages: Vec<ListItem> = if let Some(channel) = app.channels.get(&app.active_channel) {
        channel
            .messages
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

                    wrapped_lines
                        .into_iter()
                        .enumerate()
                        .map(|(i, line)| {
                            if i == 0 {
                                // First line with prefix
                                ListItem::new(Line::from(vec![
                                    Span::styled(
                                        prefix.clone(),
                                        Style::default().fg(Color::DarkGray),
                                    ),
                                    Span::styled(
                                        line,
                                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                                    ),
                                ]))
                            } else {
                                // Continuation lines with indent
                                let indent = " ".repeat(prefix_len);
                                ListItem::new(Line::from(vec![
                                    Span::raw(indent),
                                    Span::styled(
                                        line,
                                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                                    ),
                                ]))
                            }
                        })
                        .collect::<Vec<_>>()
                } else {
                    // Action messages (/me)
                    if msg.is_action {
                        let prefix = format!("[{}] * {} ", timestamp, msg.sender);
                        let prefix_len = prefix.chars().count();
                        let content_width = available_width.saturating_sub(prefix_len);
                        let wrapped_lines = wrap_message_content(&msg.content, content_width);
                        let action_style = Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::ITALIC);

                        let mut items = wrapped_lines
                            .into_iter()
                            .enumerate()
                            .map(|(i, line)| {
                                if i == 0 {
                                    ListItem::new(Line::from(vec![
                                        Span::styled(
                                            format!("[{}] ", timestamp),
                                            Style::default().fg(Color::DarkGray),
                                        ),
                                        Span::styled(format!("* {} ", msg.sender), action_style),
                                        Span::styled(line, action_style),
                                    ]))
                                } else {
                                    let indent = " ".repeat(prefix_len);
                                    ListItem::new(Line::from(vec![
                                        Span::raw(indent),
                                        Span::styled(line, action_style),
                                    ]))
                                }
                            })
                            .collect::<Vec<_>>();

                        if !msg.reactions.is_empty() {
                            let reaction_text = format_reaction_summary(msg);
                            let indent = " ".repeat(prefix_len);
                            items.push(ListItem::new(Line::from(vec![
                                Span::raw(indent),
                                Span::styled(reaction_text, Style::default().fg(Color::Cyan)),
                            ])));
                        }

                        items
                    } else {
                        // Regular messages
                        let sender_style = if msg.sender == app.username {
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            let sender_color = app
                                .peer_public_key(&msg.sender)
                                .map(|bytes| username_color_from_key(&bytes))
                                .unwrap_or_else(|| username_color_from_username(&msg.sender));

                            Style::default()
                                .fg(sender_color)
                                .add_modifier(Modifier::BOLD)
                        };

                        // Add lock icon for encrypted messages (v0.3.0)
                        let lock_icon = if msg.encrypted { "🔒 " } else { "" };
                        let prefix = format!("[{}] {}{}: ", timestamp, lock_icon, msg.sender);
                        let prefix_len = prefix.chars().count();
                        let content_width = available_width.saturating_sub(prefix_len);

                        // Render markdown/code blocks and then wrap into display lines.
                        let rendered_lines =
                            render_message_content_lines(&msg.content, content_width);

                        let mut items = rendered_lines
                            .into_iter()
                            .enumerate()
                            .map(|(i, spans)| {
                                if i == 0 {
                                    // First line with full prefix
                                    let mut full_spans = vec![
                                        Span::styled(
                                            format!("[{}] ", timestamp),
                                            Style::default().fg(Color::DarkGray),
                                        ),
                                        Span::styled(format!("{}: ", msg.sender), sender_style),
                                    ];
                                    full_spans.extend(spans);

                                    ListItem::new(Line::from(full_spans))
                                } else {
                                    // Continuation lines with indent
                                    let indent = " ".repeat(prefix_len);
                                    let mut full_spans = vec![Span::raw(indent)];
                                    full_spans.extend(spans);

                                    ListItem::new(Line::from(full_spans))
                                }
                            })
                            .collect::<Vec<_>>();

                        if !msg.reactions.is_empty() {
                            let reaction_text = format_reaction_summary(msg);
                            let indent = " ".repeat(prefix_len);
                            items.push(ListItem::new(Line::from(vec![
                                Span::raw(indent),
                                Span::styled(reaction_text, Style::default().fg(Color::Cyan)),
                            ])));
                        }

                        items
                    }
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
    let channel_name = app
        .channels
        .get(&app.active_channel)
        .map(|ch| ch.display_name())
        .unwrap_or_else(|| "Unknown".to_string());

    // Calculate scroll position info
    let total_messages = app.get_total_messages();
    let encrypted_messages = app.count_encrypted_messages();
    let messages_below = app.get_messages_below();

    // Build title with scroll position indicator
    let mut title_spans = vec![
        Span::raw(" "),
        Span::styled(
            channel_name,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        connection_status,
    ];

    // Add scroll position indicator if there are messages
    if total_messages > 0 {
        let position_text = if app.scroll_position == 0 {
            " [Latest] ".to_string()
        } else {
            format!(" [↑{}] ", app.scroll_position)
        };
        title_spans.push(Span::styled(
            position_text,
            Style::default().fg(Color::DarkGray),
        ));

        let encryption_text = format!(" [🔒{}] ", encrypted_messages);
        title_spans.push(Span::styled(
            encryption_text,
            Style::default().fg(Color::Yellow),
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
    // When there are fewer messages than visible lines, pad the top with empty
    // items so messages stick to the bottom of the panel.
    let visible_messages: Vec<ListItem> = if total_items > visible_lines {
        messages
            .into_iter()
            .skip(scroll_offset)
            .take(visible_lines)
            .collect()
    } else {
        let padding = visible_lines.saturating_sub(total_items);
        let mut padded: Vec<ListItem> = (0..padding).map(|_| ListItem::new("")).collect();
        padded.extend(messages);
        padded
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
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
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

/// Render message text with lightweight markdown and fenced code block support.
fn render_message_content_lines(text: &str, max_width: usize) -> Vec<Vec<Span<'static>>> {
    if max_width == 0 {
        return vec![vec![Span::styled(
            text.to_string(),
            Style::default().fg(Color::White),
        )]];
    }

    let code_style = Style::default().fg(Color::Green).bg(Color::Rgb(30, 30, 30));
    let quote_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::ITALIC);

    let mut lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut in_code_block = false;

    for raw_line in text.split('\n') {
        let trimmed = raw_line.trim_start();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            for wrapped in wrap_message_content(raw_line, max_width) {
                lines.push(vec![Span::styled(wrapped, code_style)]);
            }
            continue;
        }

        if trimmed.starts_with('>') {
            let quoted = trimmed.strip_prefix('>').unwrap_or(trimmed).trim_start();
            for wrapped in wrap_message_content(quoted, max_width.saturating_sub(2)) {
                lines.push(vec![Span::styled(format!("│ {}", wrapped), quote_style)]);
            }
            continue;
        }

        for wrapped in wrap_message_content(raw_line, max_width) {
            lines.push(parse_inline_markdown(&wrapped));
        }
    }

    if lines.is_empty() {
        lines.push(vec![Span::styled(
            String::new(),
            Style::default().fg(Color::White),
        )]);
    }

    lines
}

/// Parse a subset of markdown inline syntax into styled spans.
/// Supported: **bold**, *italic* / _italic_, and `inline code`.
fn parse_inline_markdown(input: &str) -> Vec<Span<'static>> {
    let chars: Vec<char> = input.chars().collect();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;

    let flush_plain = |plain: &mut String, spans: &mut Vec<Span<'static>>| {
        if !plain.is_empty() {
            spans.push(Span::styled(
                std::mem::take(plain),
                Style::default().fg(Color::White),
            ));
        }
    };

    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(end) = find_closing_double_star(&chars, i + 2) {
                flush_plain(&mut plain, &mut spans);
                let bold_text: String = chars[i + 2..end].iter().collect();
                spans.push(Span::styled(
                    bold_text,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ));
                i = end + 2;
                continue;
            }
        }

        if chars[i] == '`' {
            if let Some(end) = find_closing_char(&chars, i + 1, '`') {
                flush_plain(&mut plain, &mut spans);
                let code_text: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(
                    code_text,
                    Style::default().fg(Color::Green).bg(Color::DarkGray),
                ));
                i = end + 1;
                continue;
            }
        }

        if chars[i] == '*' || chars[i] == '_' {
            let marker = chars[i];
            if let Some(end) = find_closing_char(&chars, i + 1, marker) {
                flush_plain(&mut plain, &mut spans);
                let italic_text: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(
                    italic_text,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::ITALIC),
                ));
                i = end + 1;
                continue;
            }
        }

        plain.push(chars[i]);
        i += 1;
    }

    flush_plain(&mut plain, &mut spans);

    if spans.is_empty() {
        spans.push(Span::styled(
            String::new(),
            Style::default().fg(Color::White),
        ));
    }

    spans
}

fn find_closing_char(chars: &[char], start: usize, marker: char) -> Option<usize> {
    (start..chars.len()).find(|&idx| chars[idx] == marker)
}

fn find_closing_double_star(chars: &[char], start: usize) -> Option<usize> {
    let mut idx = start;
    while idx + 1 < chars.len() {
        if chars[idx] == '*' && chars[idx + 1] == '*' {
            return Some(idx);
        }
        idx += 1;
    }
    None
}

fn format_reaction_summary(msg: &crate::app::ChatMessage) -> String {
    let summary = msg.reaction_summary();
    summary
        .iter()
        .map(|(emoji, count)| format!("{} {}", emoji, count))
        .collect::<Vec<_>>()
        .join("  ")
}

/// Render input box only
fn render_input_box(f: &mut Frame, app: &App, area: Rect) {
    let input_style = match app.input_mode {
        InputMode::Normal => Style::default().fg(Color::Green),
        InputMode::Editing => Style::default().fg(Color::Yellow),
        InputMode::Command => Style::default().fg(Color::Cyan),
    };

    let mode_indicator = match app.input_mode {
        InputMode::Normal => {
            if app.show_telemetry {
                " [NORMAL | F10: Focus] "
            } else {
                " [NORMAL | F10: Telemetry] "
            }
        }
        InputMode::Editing => {
            if app.show_telemetry {
                " [EDIT | F10: Focus] "
            } else {
                " [EDIT | F10: Telemetry] "
            }
        }
        InputMode::Command => {
            if app.show_telemetry {
                " [COMMAND | F10: Focus] "
            } else {
                " [COMMAND | F10: Telemetry] "
            }
        }
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
    if app.input_mode == InputMode::Editing || app.input_mode == InputMode::Command {
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

        f.set_cursor_position((
            area.x + (col_in_line % available_width) as u16 + 1,
            area.y + (lines_before + col_in_line / available_width) as u16 + 1,
        ));
    }
}

/// Render typing indicator
fn render_typing_indicator(f: &mut Frame, app: &App, area: Rect) {
    if area.height < 3 || area.width < 3 {
        return;
    }

    let indicator_area = Rect {
        x: area.x + 1,
        y: area.y + area.height - 2,
        width: area.width.saturating_sub(2),
        height: 1,
    };

    if app.input_mode == InputMode::Command {
        let hint = command_hint_line(&app.input);
        let hint_widget = Paragraph::new(hint).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::ITALIC),
        );
        f.render_widget(hint_widget, indicator_area);
        return;
    }

    let typing_users = app.get_typing_users();
    if !typing_users.is_empty() {
        let typing_text = if typing_users.len() == 1 {
            format!("{} is typing...", typing_users[0])
        } else if typing_users.len() == 2 {
            format!("{} and {} are typing...", typing_users[0], typing_users[1])
        } else {
            format!(
                "{} and {} others are typing...",
                typing_users[0],
                typing_users.len() - 1
            )
        };

        let typing_indicator = Paragraph::new(typing_text).style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        );

        f.render_widget(typing_indicator, indicator_area);
    }
}

fn command_hint_line(input: &str) -> String {
    const COMMANDS: [(&str, &str); 6] = [
        ("/verify", "/verify <username>"),
        ("/confirm", "/confirm <username>"),
        ("/groupkey", "/groupkey <group> <user1,user2,...>"),
        ("/expire", "/expire <seconds> <message>"),
        ("/me", "/me <action>"),
        ("/react", "/react <emoji> OR /react <message_id> <emoji>"),
    ];

    if input.is_empty() || input == "/" {
        return "Commands: /verify  /confirm  /groupkey  /expire  /me  /react".to_string();
    }

    let query = input.to_lowercase();
    if let Some((name, usage)) = COMMANDS
        .iter()
        .find(|(name, _)| name.starts_with(&query))
        .copied()
    {
        format!("{} -> {}", name, usage)
    } else {
        format!("Unknown command: {}", input)
    }
}

/// Render telemetry (right sidebar)
fn render_telemetry(f: &mut Frame, app: &App, area: Rect) {
    // Split into compact activity summary + commands reference
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Compact activity summary
            Constraint::Min(10),   // Commands reference
        ])
        .split(area);

    // --- Compact activity summary ---
    let uptime_hours = app.telemetry.connection_uptime / 3600;
    let uptime_mins = (app.telemetry.connection_uptime % 3600) / 60;
    let uptime_secs = app.telemetry.connection_uptime % 60;

    let latency_color = if app.telemetry.latency_ms < 50 {
        Color::Green
    } else if app.telemetry.latency_ms < 150 {
        Color::Yellow
    } else {
        Color::Red
    };

    use chrono::Utc;
    let now = Utc::now();

    let summary_text = vec![
        Line::from(vec![
            Span::styled(" ⏱ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}h{}m{}s", uptime_hours, uptime_mins, uptime_secs),
                Style::default().fg(Color::Green),
            ),
            Span::styled("  ⏎ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}ms", app.telemetry.latency_ms),
                Style::default().fg(latency_color),
            ),
        ]),
        Line::from(vec![
            Span::styled(" ↑ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(app.telemetry.bytes_sent),
                Style::default().fg(Color::Green),
            ),
            Span::styled("  ↓ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_bytes(app.telemetry.bytes_received),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled(" 📨 ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{}/{}",
                    app.telemetry.messages_sent, app.telemetry.messages_received
                ),
                Style::default().fg(Color::Green),
            ),
            Span::styled("  🕐 ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                now.format("%H:%M UTC").to_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ];

    let summary = Paragraph::new(summary_text).block(
        Block::default()
            .title(" Activity ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green)),
    );
    f.render_widget(summary, chunks[0]);

    // --- Commands reference ---
    let commands_text = vec![
        Line::from(Span::styled(
            "── Normal Mode ──",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled(" i/Enter ", Style::default().fg(Color::Cyan)),
            Span::styled("Edit mode", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" q/Esc   ", Style::default().fg(Color::Cyan)),
            Span::styled("Quit", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" j/k ↑↓  ", Style::default().fg(Color::Cyan)),
            Span::styled("Scroll chat", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" PgUp/Dn ", Style::default().fg(Color::Cyan)),
            Span::styled("Page scroll", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" g/G     ", Style::default().fg(Color::Cyan)),
            Span::styled("Top / Bottom", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" h/l ←→  ", Style::default().fg(Color::Cyan)),
            Span::styled("Prev/Next channel", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" Tab     ", Style::default().fg(Color::Cyan)),
            Span::styled("Activate channel", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" #       ", Style::default().fg(Color::Cyan)),
            Span::styled("Global channel", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" J/K     ", Style::default().fg(Color::Cyan)),
            Span::styled("Select user", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" d       ", Style::default().fg(Color::Cyan)),
            Span::styled("DM selected user", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" r       ", Style::default().fg(Color::Cyan)),
            Span::styled("Quick react", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(" F10     ", Style::default().fg(Color::Cyan)),
            Span::styled("Toggle focus", Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "── Commands ──",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled(" /me ", Style::default().fg(Color::Cyan)),
            Span::styled("<action>", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(" /react ", Style::default().fg(Color::Cyan)),
            Span::styled("<emoji>", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(" /verify ", Style::default().fg(Color::Cyan)),
            Span::styled("<user>", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(" /confirm ", Style::default().fg(Color::Cyan)),
            Span::styled("<user>", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(" /groupkey ", Style::default().fg(Color::Cyan)),
            Span::styled("<grp> <u,u>", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(" /expire ", Style::default().fg(Color::Cyan)),
            Span::styled("<secs> <msg>", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "── Markdown ──",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled(" **bold** ", Style::default().fg(Color::Cyan)),
            Span::styled(" *italic* ", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled(" `code`  ", Style::default().fg(Color::Cyan)),
            Span::styled(" > quote ", Style::default().fg(Color::Cyan)),
        ]),
    ];

    let commands = Paragraph::new(commands_text).block(
        Block::default()
            .title(" Reference ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green)),
    );
    f.render_widget(commands, chunks[1]);
}

/// Render a vertical scroll bar indicator
fn render_scroll_bar(
    f: &mut Frame,
    area: Rect,
    scroll_position: usize,
    total_items: usize,
    visible_items: usize,
) {
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
        let symbol = if i == bar_position { "█" } else { "│" };

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
            },
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

/// Derive a deterministic username color from the first 3 bytes of a public key.
fn username_color_from_key(public_key: &[u8; 32]) -> Color {
    let mut r = public_key[0];
    let mut g = public_key[1];
    let mut b = public_key[2];

    // Keep colors readable on dark backgrounds.
    let min_channel = 80;
    r = r.max(min_channel);
    g = g.max(min_channel);
    b = b.max(min_channel);

    Color::Rgb(r, g, b)
}

/// Fallback deterministic color when a peer key is unavailable.
fn username_color_from_username(username: &str) -> Color {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    username.hash(&mut hasher);
    let hash = hasher.finish().to_le_bytes();

    let r = hash[0].max(80);
    let g = hash[1].max(80);
    let b = hash[2].max(80);
    Color::Rgb(r, g, b)
}
