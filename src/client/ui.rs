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
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();

    // Mode indicator
    let mode_str = match app.view_mode {
        ViewMode::ActiveChannel => "[channel]",
        ViewMode::AllChannels => "[all]",
    };
    spans.push(Span::styled(mode_str, Style::default().fg(Color::DarkGray)));
    spans.push(Span::raw(" "));

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
                "[{}:#{}{}]",
                i + 1,
                channel.name,
                channel.status_indicator()
            )
        } else {
            format!("[#{}{}]", channel.name, channel.status_indicator())
        };

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

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_output(f: &mut Frame, app: &mut App, area: Rect) {
    let mut list_items: Vec<ListItem> = Vec::new();
    let height = area.height as usize;

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
                }
            }
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
        f.render_widget(p, area);
    } else {
        f.render_widget(List::new(list_items), area);
    }
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let channel_name = app.active_channel.as_deref().unwrap_or("none");

    // Construct prompt: #channel ❯ input
    let prompt_prefix = format!("#{} ❯ ", channel_name);
    let prefix_len = prompt_prefix.chars().count();

    let input_content = app.line_editor.content();
    let _full_line = format!("{}{}", prompt_prefix, input_content);

    let p = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("#{}", channel_name),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(" ❯ ", Style::default().fg(Color::Green)),
        Span::raw(input_content),
    ]));

    f.render_widget(p, area);

    let cursor_char_idx = input_content[..app.line_editor.cursor_position()]
        .chars()
        .count();
    let cursor_x = area.x + prefix_len as u16 + cursor_char_idx as u16;
    f.set_cursor_position(Position::new(cursor_x, area.y));
}
