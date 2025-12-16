use crate::client::app::{App, ViewMode};
use chrono::{DateTime, Local};
use ratatui::{
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, List, ListItem, Paragraph},
    Frame,
};
use regex::Regex;
use std::sync::LazyLock;

static ANSI_ESCAPE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b\[[0-9;?]*[a-zA-Z~]|\x1b\][^\x07]*(?:\x07|\x1b\\)|\x1b[a-zA-Z]").unwrap()
});

pub fn strip_ansi_codes(s: &str) -> String {
    ANSI_ESCAPE_RE.replace_all(s, "").to_string()
}

use std::time::Instant;

fn draw_notifications(f: &mut Frame, app: &App) {
    let now = Instant::now();
    let notifications: Vec<_> = app.notifications.iter()
        .filter(|n| now.duration_since(n.timestamp) < n.duration)
        .collect();
    
    if notifications.is_empty() {
        return;
    }

    let max_width = f.area().width.saturating_sub(4); // 2 padding on each side
    let mut current_y = f.area().height.saturating_sub(1) - 1; // Start above input area, going up

    for notification in notifications.iter().rev() { // Display newest on bottom
        let message = format!(" {}", notification.message);
        let message_len = message.chars().count();
        let display_message = if message_len > max_width as usize {
            message.chars().take(max_width as usize - 3).collect::<String>() + "..."
        } else {
            message
        };
        
        let width = display_message.chars().count() as u16 + 2; // +2 for padding
        let x = f.area().width.saturating_sub(width).saturating_sub(1); // Right aligned
        let y = current_y;

        let area = Rect::new(x, y, width, 1);
        let p = Paragraph::new(display_message)
            .style(Style::default().bg(Color::DarkGray).fg(Color::White))
            .block(Block::new().borders(ratatui::widgets::Borders::NONE));
        f.render_widget(p, area);
        current_y = current_y.saturating_sub(1);
        if current_y == 0 { // Don't draw over status bar
            break;
        }
    }
}

const HELP_TEXT: &[&str] = &[
    "nexus - channel-based terminal multiplexer",
    "",
    "Commands:",
    "  :new <name> [cmd]   Create a new channel (optionally with a command)",
    "  :kill <name>        Kill a channel",
    "  :list               List all channels",
    "  :status [name]      Show channel status",
    "  :sub <ch> [ch...]   Subscribe to channel output (:sub * for all)",
    "  :unsub <ch>         Unsubscribe from channel",
    "  :subs               Show current subscriptions",
    "  :view [channel|all] Toggle or set view mode",
    "  :clear              Clear the output area",
    "  :timestamps         Toggle timestamp display (:ts)",
    "  :quit               Exit nexus",
    "",
    "Channel switching:",
    "  #<name>             Switch to channel by name",
    "  #<name> <cmd>       Send command to channel without switching",
    "  Alt+1-9             Quick switch to channel by number",
    "  Ctrl+Left/Right     Switch to previous/next channel",
    "",
    "Scrolling:",
    "  Page Up/Down        Scroll output by page",
    "  Ctrl+U/B            Scroll up/down half page",
    "  Home/End            Jump to top/bottom of output",
    "  Tab                 Complete command/channel",
    "",
    "Line editing:",
    "  Left/Right          Move cursor within input",
    "  Home/End            Jump to start/end of input (Ctrl+A/E)",
    "  Up/Down             Navigate command history",
    "  Ctrl+W              Delete word backward",
    "  Ctrl+U/K            Delete to start/end of line",
    "",
    "Keyboard shortcuts:",
    "  Ctrl+C              Cancel current input / send interrupt to channel",
    "  Ctrl+D              Send EOF to channel",
    "  Ctrl+\\              Exit nexus immediately",
    "",
    "Mouse:",
    "  Click channel       Switch to clicked channel in status bar",
    "  Scroll wheel        Scroll output up/down",
];

fn draw_help_popup(f: &mut Frame, app: &mut App) {
    if !app.show_help {
        return;
    }

    let size = f.area();
    let width = size.width.saturating_sub(10).min(100);
    let height = size.height.saturating_sub(10).min(HELP_TEXT.len() as u16 + 2);
    let area = Rect::new(
        (size.width - width) / 2,
        (size.height - height) / 2,
        width,
        height,
    );

    let block = Block::default()
        .title("Help (Press ? or :help to close)")
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .style(Style::default().bg(Color::Black));

    let inner_area = block.inner(area);
    f.render_widget(block, area);
    let lines: Vec<ListItem> = HELP_TEXT
        .iter()
        .skip(app.help_scroll)
        .map(|&s| ListItem::new(Text::raw(s)))
        .collect();

    let list = List::new(lines)
        .block(Block::default())
        .style(Style::default().fg(Color::White));
    f.render_widget(list, inner_area);
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let mut constraints = vec![
        Constraint::Length(1), // Status bar
        Constraint::Length(1), // Separator
        Constraint::Min(0),    // Output
        Constraint::Length(1), // Separator
    ];

    if app.completions.is_some() {
        constraints.push(Constraint::Length(1)); // Completions
    }
    constraints.push(Constraint::Length(1)); // Input

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(f.area());

    // Status Bar
    draw_status_bar(f, app, chunks[0]);

    // Top Separator
    let _sep = Block::default().style(Style::default().fg(Color::DarkGray));
    f.render_widget(Span::raw("─".repeat(chunks[1].width as usize)), chunks[1]);

    // Output
    draw_output(f, app, chunks[2]);

    // Bottom Separator
    f.render_widget(Span::raw("─".repeat(chunks[3].width as usize)), chunks[3]);

    // Completions and Input
    if let Some(completions) = &app.completions {
        // Render completions
        let comp_text = format!("Completions: {}", completions.join("  "));
        let p = Paragraph::new(Span::styled(comp_text, Style::default().fg(Color::Yellow)));
        f.render_widget(p, chunks[4]);

        draw_input(f, app, chunks[5]);
    } else {
        draw_input(f, app, chunks[4]);
    }

    draw_notifications(f, app);
    draw_help_popup(f, app);
}

fn draw_status_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(ratatui::widgets::Borders::TOP | ratatui::widgets::Borders::LEFT | ratatui::widgets::Borders::RIGHT)
        .title("Channels")
        .border_style(Style::default().fg(Color::DarkGray));

    let inner_area = block.inner(area); // Get area inside borders
    f.render_widget(block, area);
    let mut spans = Vec::new();

    // Clear previous channel rects
    app.status_bar_channel_rects.clear();

    // Mode indicator
    let mode_str = match app.view_mode {
        ViewMode::ActiveChannel => "[channel]",
        ViewMode::AllChannels => "[all]",
    };
    spans.push(Span::styled(mode_str, Style::default().fg(Color::DarkGray)));
    spans.push(Span::raw(" "));

    let mut current_x = inner_area.x + 1; // Start after block border and mode indicator

    // Channels
    for (i, channel) in app.channels.iter().enumerate() {
        let is_active = app.active_channel.as_deref() == Some(&channel.name);

        let mut style = Style::default();
        if is_active {
            style = style.fg(Color::Green).add_modifier(Modifier::BOLD);
        } else if channel.has_new_output {
            style = style.fg(Color::Yellow);
        } else if !channel.running {
            if channel.exit_code == Some(0) {
                style = style.fg(Color::Green);
            } else {
                style = style.fg(Color::Red);
            }
        } else {
            style = style.fg(Color::DarkGray);
        }

        let prefix = if app.show_channel_numbers && i < 9 {
            format!(
                "[{}:#{}{}{}]", // Added {} for subscription status
                i + 1,
                channel.name,
                if channel.is_subscribed { "+" } else { "" },
                channel.status_indicator()
            )
        } else {
            format!("[#{}{}{}]", channel.name, if channel.is_subscribed { "+" } else { "" }, channel.status_indicator())
        };

        let span_width = prefix.chars().count() as u16;
        let channel_rect = Rect::new(current_x, inner_area.y, span_width, 1);
        app.status_bar_channel_rects.insert(channel.name.clone(), channel_rect);
        current_x += span_width + 1; // +1 for space between segments

        spans.push(Span::styled(prefix, style));
        spans.push(Span::raw(" "));
    }

    // Scroll indicator
    if app.is_scrolled(app.active_channel.as_deref()) {
        spans.push(Span::styled(
            " ↑ SCROLLED",
            Style::default().fg(Color::Yellow),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), inner_area);
}

fn draw_output(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .title("Output")
        .border_style(Style::default().fg(Color::DarkGray));

    let inner_area = block.inner(area);
    f.render_widget(block, area);
    let mut list_items: Vec<ListItem> = Vec::new();
            let height = inner_area.height as usize;
    
            let format_line = |content: &str, timestamp: DateTime<Local>, show_ts: bool| -> String {
                if show_ts {
                    format!("[{}] {}", timestamp.format("%H:%M:%S"), content)
                } else {
                    content.to_string()
                }
            };
    
            if app.view_mode == ViewMode::ActiveChannel {
                if let Some(ch) = app.active_channel.clone() {
                    if let Some(buffer) = app.channel_buffers.get(&ch) {
                         let scroll_offset = app.scroll_offsets.get(&ch).copied().unwrap_or(0);
                         let end_index = buffer.len().saturating_sub(scroll_offset);
                         let start_index = end_index.saturating_sub(height);
    
                         for line in &buffer[start_index..end_index] {
                             let content = format_line(&line.content, line.timestamp, app.show_timestamps);
                             list_items.push(ListItem::new(Text::raw(strip_ansi_codes(&content))));
                         }            }
                }
            } else {
                // ViewMode::AllChannels
                let buffer = &app.interleaved_buffer;
                let scroll_offset = 0; // TODO: interleaved scroll
                let end_index = buffer.len().saturating_sub(scroll_offset);
                let start_index = end_index.saturating_sub(height);
    
                // Fix slice range
                let start = start_index.min(buffer.len());
                let end = end_index.min(buffer.len());
    
                let visible_items: Vec<(String, String, DateTime<Local>)> = buffer[start..end]
                    .iter()
                    .map(|(n, l)| (n.clone(), l.content.clone(), l.timestamp))
                    .collect();
    
                for (ch_name, content_str, timestamp) in visible_items {
                    let content = format_line(&content_str, timestamp, app.show_timestamps);
                    let color = app.get_channel_color(&ch_name);
                    
                    let text = Text::raw(strip_ansi_codes(&content));
                    for mut line_content in text.lines {
                        line_content.spans.insert(
                            0,
                            Span::styled(format!("#{:<8} │ ", ch_name), Style::default().fg(color)),
                        );
                        list_items.push(ListItem::new(line_content));
                    }
                }
            }
    
            if list_items.is_empty() && app.show_welcome {
                let welcome_text = [
                    "Welcome to nexus - channel-based terminal multiplexer",
                    "",
                    "Quick start:",
                    "  :new <name> [cmd]  Create a new channel",
                    "  #<name>            Switch to channel",
                    "  :list              List channels",
                    "  :quit              Exit",
                ];
                let p = Paragraph::new(Text::from(welcome_text.join("\n")))
                    .style(Style::default().fg(Color::DarkGray))
                    .block(Block::default());
                f.render_widget(p, inner_area); // Use inner_area here
            } else {
                f.render_widget(List::new(list_items), inner_area); // Use inner_area here
            }
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
        let block = Block::default()
            .borders(ratatui::widgets::Borders::TOP | ratatui::widgets::Borders::LEFT | ratatui::widgets::Borders::RIGHT)
            .title("Input")
            .border_style(Style::default().fg(Color::DarkGray));

        let inner_area = block.inner(area);
        f.render_widget(block, area);
        let channel_name = app.active_channel.as_deref().unwrap_or("none");

        let prompt_text = format!("#{} ❯ {}", channel_name, app.line_editor.content());
        let cursor_pos = 1 + channel_name.len() + 3 + app.line_editor.cursor_position(); // # + name + " ❯ " + position

        let paragraph = Paragraph::new(prompt_text)
            .style(Style::default().fg(Color::White));
        f.render_widget(paragraph, inner_area);

        // Set cursor position
        if let Some(row) = inner_area.y.checked_add(0) {
            if let Some(col) = inner_area.x.checked_add(cursor_pos as u16) {
                f.set_cursor_position(Position::new(col, row));
            }
        }
}
