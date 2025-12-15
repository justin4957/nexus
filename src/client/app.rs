use chrono::{DateTime, Local};
use ratatui::style::Color;
use std::collections::{HashMap, HashSet};
// actually we should define it here or in a types module. Let's redefine it here and update mod.rs to use this one.

pub struct ChannelInfo {
    pub name: String,
    pub running: bool,
    pub has_new_output: bool,
    pub exit_code: Option<i32>,
}

impl ChannelInfo {
    pub fn status_indicator(&self) -> &'static str {
        if !self.running {
            if let Some(code) = self.exit_code {
                if code == 0 {
                    return ": ✓";
                } else {
                    return ": ✗";
                }
            }
            return ": stopped";
        }
        if self.has_new_output {
            return "*";
        }
        ""
    }
}

#[derive(Clone)]
pub struct BufferedLine {
    pub content: String,
    pub timestamp: DateTime<Local>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    ActiveChannel,
    AllChannels,
}

/// Input line editor with cursor position tracking
pub struct LineEditor {
    pub buffer: String,
    pub cursor: usize,
}

impl LineEditor {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
        }
    }

    pub fn content(&self) -> &str {
        &self.buffer
    }

    pub fn cursor_position(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn insert(&mut self, c: char) {
        if self.cursor > self.buffer.len() {
            self.cursor = self.buffer.len();
        }
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor > 0 {
            let prev_cursor = self.buffer[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.buffer.remove(prev_cursor);
            self.cursor = prev_cursor;
            true
        } else {
            false
        }
    }

    pub fn delete(&mut self) -> bool {
        if self.cursor < self.buffer.len() {
            self.buffer.remove(self.cursor);
            true
        } else {
            false
        }
    }

    pub fn move_left(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor = self.buffer[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            true
        } else {
            false
        }
    }

    pub fn move_right(&mut self) -> bool {
        if self.cursor < self.buffer.len() {
            self.cursor = self.buffer[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.buffer.len());
            true
        } else {
            false
        }
    }

    pub fn move_home(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor = 0;
            true
        } else {
            false
        }
    }

    pub fn move_end(&mut self) -> bool {
        if self.cursor < self.buffer.len() {
            self.cursor = self.buffer.len();
            true
        } else {
            false
        }
    }

    pub fn delete_word_backward(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let before_cursor = &self.buffer[..self.cursor];
        let trimmed_end = before_cursor.trim_end();
        let word_start = trimmed_end
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);

        self.buffer.drain(word_start..self.cursor);
        self.cursor = word_start;
        true
    }

    pub fn delete_to_end(&mut self) -> bool {
        if self.cursor < self.buffer.len() {
            self.buffer.truncate(self.cursor);
            true
        } else {
            false
        }
    }

    pub fn delete_to_start(&mut self) -> bool {
        if self.cursor > 0 {
            self.buffer.drain(..self.cursor);
            self.cursor = 0;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    pub fn set(&mut self, content: &str) {
        self.buffer = content.to_string();
        self.cursor = self.buffer.len();
    }

    pub fn take(&mut self) -> String {
        let content = std::mem::take(&mut self.buffer);
        self.cursor = 0;
        content
    }
}

pub struct App {
    pub channels: Vec<ChannelInfo>,
    pub active_channel: Option<String>,
    pub subscriptions: Vec<String>,
    pub line_editor: LineEditor,
    pub channel_buffers: HashMap<String, Vec<BufferedLine>>,
    pub interleaved_buffer: Vec<(String, BufferedLine)>,
    pub scroll_offsets: HashMap<String, usize>,
    pub view_mode: ViewMode,
    pub show_timestamps: bool,
    pub show_welcome: bool,
    pub show_channel_numbers: bool,
    pub max_buffer_lines: usize,
    pub channel_colors: HashMap<String, Color>,
    pub completions: Option<Vec<String>>,
}

impl App {
    pub fn new() -> Self {
        Self {
            channels: Vec::new(),
            active_channel: None,
            subscriptions: Vec::new(),
            line_editor: LineEditor::new(),
            channel_buffers: HashMap::new(),
            interleaved_buffer: Vec::new(),
            scroll_offsets: HashMap::new(),
            view_mode: ViewMode::ActiveChannel,
            show_timestamps: false,
            show_welcome: true,
            show_channel_numbers: true,
            max_buffer_lines: 10000,
            channel_colors: HashMap::new(),
            completions: None,
        }
    }

    pub fn add_output(&mut self, channel: String, text: String) {
        self.show_welcome = false;

        let buffered_line = BufferedLine {
            content: text,
            timestamp: Local::now(),
        };

        let buffer = self.channel_buffers.entry(channel.clone()).or_default();
        buffer.push(buffered_line.clone());
        if buffer.len() > self.max_buffer_lines {
            let excess = buffer.len() - self.max_buffer_lines;
            buffer.drain(0..excess);
        }

        self.interleaved_buffer
            .push((channel.clone(), buffered_line));
        if self.interleaved_buffer.len() > self.max_buffer_lines {
            let excess = self.interleaved_buffer.len() - self.max_buffer_lines;
            self.interleaved_buffer.drain(0..excess);
        }

        // Auto-scroll to bottom if not scrolled up
        if !self.is_scrolled(Some(&channel)) {
            self.scroll_to_bottom(Some(&channel));
        }
    }

    pub fn is_scrolled(&self, channel: Option<&str>) -> bool {
        channel
            .and_then(|ch| self.scroll_offsets.get(ch))
            .map(|&o| o > 0)
            .unwrap_or(false)
    }

    pub fn scroll_up(&mut self, lines: usize) {
        let _target = match self.view_mode {
            ViewMode::ActiveChannel => self.active_channel.as_deref(),
            ViewMode::AllChannels => Some("__interleaved__"), // Use a special key or handle logic differently
        };

        // For now, only scroll active channel
        if let Some(ch) = self.active_channel.as_deref() {
            let buffer_len = self.channel_buffers.get(ch).map(|b| b.len()).unwrap_or(0);
            // approximate visible rows - exact value available in draw, but logic needs it here.
            // We can store viewport height in App or just clamp to buffer len.
            // Clamping to buffer len is safe.
            let offset = self.scroll_offsets.entry(ch.to_string()).or_insert(0);
            *offset = (*offset + lines).min(buffer_len.saturating_sub(1));
        }
    }

    pub fn scroll_down(&mut self, lines: usize) {
        if let Some(ch) = self.active_channel.as_deref() {
            let offset = self.scroll_offsets.entry(ch.to_string()).or_insert(0);
            *offset = offset.saturating_sub(lines);
        }
    }

    pub fn scroll_to_bottom(&mut self, channel: Option<&str>) {
        if let Some(ch) = channel {
            self.scroll_offsets.insert(ch.to_string(), 0);
        }
    }

    pub fn get_channel_color(&mut self, channel: &str) -> Color {
        if let Some(c) = self.channel_colors.get(channel) {
            return *c;
        }

        // Simple color rotation
        let colors = [
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::Yellow,
            Color::Green,
            Color::Red,
        ];

        let used: HashSet<_> = self.channel_colors.values().copied().collect();
        let color = *colors
            .iter()
            .find(|c| !used.contains(c))
            .unwrap_or(&colors[self.channel_colors.len() % colors.len()]);

        self.channel_colors.insert(channel.to_string(), color);
        color
    }

    pub fn next_channel(&mut self) {
        if self.channels.is_empty() {
            return;
        }
        if let Some(curr) = &self.active_channel {
            if let Some(idx) = self.channels.iter().position(|c| &c.name == curr) {
                let next = (idx + 1) % self.channels.len();
                self.active_channel = Some(self.channels[next].name.clone());
            }
        } else {
            self.active_channel = Some(self.channels[0].name.clone());
        }
    }

    pub fn prev_channel(&mut self) {
        if self.channels.is_empty() {
            return;
        }
        if let Some(curr) = &self.active_channel {
            if let Some(idx) = self.channels.iter().position(|c| &c.name == curr) {
                let prev = if idx == 0 {
                    self.channels.len() - 1
                } else {
                    idx - 1
                };
                self.active_channel = Some(self.channels[prev].name.clone());
            }
        } else {
            self.active_channel = Some(self.channels[0].name.clone());
        }
    }
}
