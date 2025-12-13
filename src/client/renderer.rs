//! Output rendering - status bar, channel output, prompt
//!
//! The rendering approach uses a simple model:
//! - Status bar at line 0 (top)
//! - Output area from line 2 to n-2 (scrolling region)
//! - Prompt at line n-1 (bottom)
//!
//! Output is printed at the current cursor position within the output area,
//! and after each output we redraw the prompt to keep it at the bottom.

use crossterm::{
    cursor, execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use regex::Regex;
use std::io::{self, Write};
use std::sync::LazyLock;

use std::collections::{HashMap, HashSet};

/// Regex to match ANSI escape sequences (colors, cursor movement, etc.)
static ANSI_ESCAPE_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Matches:
    // - CSI sequences: ESC [ ... (letter or ~)
    // - OSC sequences: ESC ] ... (BEL or ESC \)
    // - Simple escape sequences: ESC (letter)
    Regex::new(r"\x1b\[[0-9;?]*[a-zA-Z~]|\x1b\][^\x07]*(?:\x07|\x1b\\)|\x1b[a-zA-Z]").unwrap()
});

/// Strip ANSI escape sequences from a string
pub fn strip_ansi_codes(s: &str) -> String {
    ANSI_ESCAPE_RE.replace_all(s, "").to_string()
}

use crate::config::StatusBarPosition;

const CHANNEL_COLORS: [Color; 6] = [
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::Yellow,
    Color::Green,
    Color::Red,
];

/// Welcome tips shown on first launch
const WELCOME_TIPS: &[&str] = &[
    "Welcome to nexus - channel-based terminal multiplexer",
    "",
    "Quick start:",
    "  :new <name> [cmd]  Create a new channel (optionally with a command)",
    "  #<name>            Switch to channel by name",
    "  :list              List all channels",
    "  :sub *             Subscribe to all channel output",
    "  :clear             Clear the output area",
    "  :quit              Exit nexus (Ctrl+\\ also works)",
    "",
    "Scrolling: Page Up/Down, Ctrl+U/D to scroll channel output",
    "",
    "Type a command to get started, or :new shell to create a shell channel.",
];

/// View mode for output display
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// Show only active channel's output (clean, no prefix)
    ActiveChannel,
    /// Show all subscribed channels interleaved (with channel prefix)
    AllChannels,
}

/// Terminal renderer for nexus client
pub struct Renderer {
    /// Terminal size (cols, rows)
    size: (u16, u16),

    /// Status bar height
    status_bar_height: u16,

    /// Prompt height
    prompt_height: u16,

    /// Whether to show timestamps
    #[allow(dead_code)]
    show_timestamps: bool,

    /// Map of channel names to colors
    channel_colors: HashMap<String, Color>,

    /// Status bar position (top or bottom)
    status_bar_position: StatusBarPosition,

    /// Per-channel output buffers
    channel_buffers: HashMap<String, Vec<String>>,

    /// Scroll offset per channel (0 = at bottom/most recent)
    scroll_offsets: HashMap<String, usize>,

    /// Maximum lines to keep in buffer per channel
    max_buffer_lines: usize,

    /// Whether to show welcome tips (hidden once output appears)
    show_welcome: bool,

    /// Current view mode
    view_mode: ViewMode,

    /// Buffer for interleaved "all channels" view
    interleaved_buffer: Vec<(String, String)>, // (channel, content)
}

impl Renderer {
    /// Create a new renderer with default settings
    pub fn new() -> io::Result<Self> {
        Self::with_position(StatusBarPosition::Top)
    }

    /// Create a new renderer with specified status bar position
    pub fn with_position(position: StatusBarPosition) -> io::Result<Self> {
        let size = terminal::size()?;

        Ok(Self {
            size,
            status_bar_height: 1,
            prompt_height: 1,
            show_timestamps: false,
            channel_colors: HashMap::new(),
            status_bar_position: position,
            channel_buffers: HashMap::new(),
            scroll_offsets: HashMap::new(),
            max_buffer_lines: 10000,
            show_welcome: true,
            view_mode: ViewMode::ActiveChannel,
            interleaved_buffer: Vec::new(),
        })
    }

    /// Update terminal size
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.size = (cols, rows);
    }

    /// Get terminal size
    pub fn terminal_size(&self) -> (u16, u16) {
        self.size
    }

    /// Get current view mode
    pub fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    /// Set view mode
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
    }

    /// Toggle between view modes
    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::ActiveChannel => ViewMode::AllChannels,
            ViewMode::AllChannels => ViewMode::ActiveChannel,
        };
    }

    /// Scroll up in the current channel
    pub fn scroll_up(&mut self, channel: Option<&str>, lines: usize) {
        if let Some(ch) = channel {
            let buffer_len = self.channel_buffers.get(ch).map(|b| b.len()).unwrap_or(0);
            let visible = self.visible_output_rows();
            let max_scroll = buffer_len.saturating_sub(visible);

            let offset = self.scroll_offsets.entry(ch.to_string()).or_insert(0);
            *offset = (*offset + lines).min(max_scroll);
        }
    }

    /// Scroll down in the current channel
    pub fn scroll_down(&mut self, channel: Option<&str>, lines: usize) {
        if let Some(ch) = channel {
            let offset = self.scroll_offsets.entry(ch.to_string()).or_insert(0);
            *offset = offset.saturating_sub(lines);
        }
    }

    /// Scroll to bottom (most recent) in the current channel
    pub fn scroll_to_bottom(&mut self, channel: Option<&str>) {
        if let Some(ch) = channel {
            self.scroll_offsets.insert(ch.to_string(), 0);
        }
    }

    /// Check if scrolled up from bottom
    pub fn is_scrolled(&self, channel: Option<&str>) -> bool {
        channel
            .and_then(|ch| self.scroll_offsets.get(ch))
            .map(|&o| o > 0)
            .unwrap_or(false)
    }

    /// Height available for output
    #[allow(dead_code)]
    pub fn output_height(&self) -> u16 {
        self.size
            .1
            .saturating_sub(self.status_bar_height + self.prompt_height)
    }

    /// Get the row position for the status bar based on configuration
    fn status_bar_row(&self) -> u16 {
        match self.status_bar_position {
            StatusBarPosition::Top => 0,
            StatusBarPosition::Bottom => self.size.1.saturating_sub(2), // Above prompt
        }
    }

    /// Get the row position for the prompt
    fn prompt_row(&self) -> u16 {
        self.size.1.saturating_sub(1)
    }

    /// Build the status bar content with truncation support
    fn build_status_bar_content(
        &self,
        channels: &[ChannelStatusInfo],
        active_channel: Option<&str>,
    ) -> Vec<(String, Color)> {
        let terminal_width = self.size.0 as usize;
        let mut segments: Vec<(String, Color)> = Vec::new();
        let mut total_width = 0;
        let ellipsis = " ...";
        let ellipsis_width = ellipsis.len();

        // Add view mode indicator
        let mode_indicator = match self.view_mode {
            ViewMode::ActiveChannel => "[channel]",
            ViewMode::AllChannels => "[all]",
        };
        segments.push((mode_indicator.to_string(), Color::DarkGrey));
        total_width += mode_indicator.len() + 1;

        for channel in channels {
            let color = if Some(channel.name.as_str()) == active_channel {
                Color::Green
            } else if channel.has_new_output {
                Color::Yellow
            } else if !channel.running {
                if channel.exit_code == Some(0) {
                    Color::DarkGreen
                } else if channel.exit_code.is_some() {
                    Color::DarkRed
                } else {
                    Color::DarkGrey
                }
            } else {
                Color::DarkGrey
            };

            let segment = format!("[#{}{}]", channel.name, channel.status_indicator());
            let segment_width = segment.len() + 1; // +1 for space separator

            // Check if adding this segment would exceed terminal width
            if total_width + segment_width > terminal_width.saturating_sub(ellipsis_width) {
                // Check if we have more channels to show
                let remaining = channels.len() - (segments.len() - 1); // -1 for mode indicator
                if remaining > 0 {
                    segments.push((ellipsis.to_string(), Color::DarkGrey));
                }
                break;
            }

            total_width += 1; // Space between segments
            total_width += segment.len();
            segments.push((segment, color));
        }

        segments
    }

    /// Draw the status bar
    pub fn draw_status_bar(
        &self,
        stdout: &mut impl Write,
        channels: &[ChannelStatusInfo],
        active_channel: Option<&str>,
    ) -> io::Result<()> {
        let status_row = self.status_bar_row();
        queue!(stdout, cursor::MoveTo(0, status_row))?;
        queue!(stdout, terminal::Clear(ClearType::CurrentLine))?;

        let segments = self.build_status_bar_content(channels, active_channel);

        for (i, (segment, color)) in segments.iter().enumerate() {
            if i > 0 && !segment.starts_with(" ...") {
                queue!(stdout, Print(" "))?;
            }

            queue!(stdout, SetForegroundColor(*color))?;
            queue!(stdout, Print(segment))?;
        }

        // Add scroll indicator if scrolled up
        if self.is_scrolled(active_channel) {
            queue!(stdout, SetForegroundColor(Color::Yellow))?;
            queue!(stdout, Print(" ↑scroll"))?;
        }

        queue!(stdout, ResetColor)?;
        stdout.flush()
    }

    /// Draw the prompt line with enhanced visuals and cursor positioning
    pub fn draw_prompt(
        &self,
        stdout: &mut impl Write,
        active_channel: Option<&str>,
        input: &str,
        cursor_pos: usize,
    ) -> io::Result<()> {
        let prompt_row = self.prompt_row();
        queue!(stdout, cursor::MoveTo(0, prompt_row))?;
        queue!(stdout, terminal::Clear(ClearType::CurrentLine))?;

        // Draw channel name with color
        let channel_display = active_channel.unwrap_or("none");
        queue!(stdout, SetForegroundColor(Color::Cyan))?;
        queue!(stdout, Print(format!("@{}", channel_display)))?;

        // Draw prompt arrow
        queue!(stdout, SetForegroundColor(Color::Green))?;
        queue!(stdout, Print(" ❯ "))?;
        queue!(stdout, ResetColor)?;

        // Calculate prompt prefix length for cursor positioning
        // "@channel ❯ " - the ❯ is 3 bytes in UTF-8
        let prefix_len = 1 + channel_display.len() + 4; // @ + channel + " ❯ "

        // Draw input or placeholder
        if input.is_empty() {
            queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
            queue!(stdout, Print("Type :help for commands"))?;
            queue!(stdout, ResetColor)?;
            // Position cursor at start of input area
            queue!(stdout, cursor::MoveTo(prefix_len as u16, prompt_row))?;
        } else {
            queue!(stdout, Print(input))?;
            // Calculate cursor column position
            // cursor_pos is a byte index, we need to count characters for display
            let chars_before_cursor = input[..cursor_pos].chars().count();
            let cursor_col = prefix_len + chars_before_cursor;
            queue!(stdout, cursor::MoveTo(cursor_col as u16, prompt_row))?;
        }

        stdout.flush()
    }

    /// Remove the cached color for a channel so the slot can be reused.
    pub fn clear_channel_color(&mut self, channel_name: &str) {
        self.channel_colors.remove(channel_name);
    }

    /// Get a color for a channel, assigning a new one if necessary
    fn get_channel_color(&mut self, channel_name: &str) -> Color {
        if let Some(color) = self.channel_colors.get(channel_name) {
            return *color;
        }

        let used_colors: HashSet<_> = self.channel_colors.values().copied().collect();
        let new_color = CHANNEL_COLORS
            .iter()
            .find(|c| !used_colors.contains(c))
            .copied()
            .unwrap_or(CHANNEL_COLORS[self.channel_colors.len() % CHANNEL_COLORS.len()]);

        self.channel_colors
            .insert(channel_name.to_string(), new_color);
        new_color
    }

    /// Get the number of visible output rows
    pub fn visible_output_rows(&self) -> usize {
        // Output area: from row 2 to row n-3 (inclusive)
        // Row 0: status bar, Row 1: separator, Row n-2: separator, Row n-1: prompt
        self.size.1.saturating_sub(4) as usize
    }

    /// Get the starting row for output area
    fn output_start_row(&self) -> u16 {
        2 // After status bar (row 0) and separator (row 1)
    }

    /// Draw a single output line at a specific row (clean mode - no prefix)
    fn draw_clean_line_at_row(
        &self,
        stdout: &mut impl Write,
        row: u16,
        content: &str,
    ) -> io::Result<()> {
        queue!(stdout, cursor::MoveTo(0, row))?;
        queue!(stdout, terminal::Clear(ClearType::CurrentLine))?;
        // Flush queued commands before writing raw content
        stdout.flush()?;

        // For content with ANSI codes, we need to truncate based on visible length
        let visible_len = Self::visible_len(content);
        let max_line_len = self.size.0 as usize;

        // Write content directly to preserve ANSI escape sequences
        if visible_len > max_line_len && max_line_len > 0 {
            let display_line = truncate_with_ansi(content, max_line_len);
            stdout.write_all(display_line.as_bytes())?;
        } else {
            stdout.write_all(content.as_bytes())?;
            // Reset colors after content to prevent color bleeding
            stdout.write_all(b"\x1b[0m")?;
        }

        Ok(())
    }

    /// Draw a single output line at a specific row (with channel prefix)
    fn draw_prefixed_line_at_row(
        &mut self,
        stdout: &mut impl Write,
        row: u16,
        channel_name: &str,
        content: &str,
    ) -> io::Result<()> {
        queue!(stdout, cursor::MoveTo(0, row))?;
        queue!(stdout, terminal::Clear(ClearType::CurrentLine))?;

        // Draw the channel name with color
        let color = if channel_name == "SYSTEM" {
            Color::Red
        } else {
            self.get_channel_color(channel_name)
        };
        queue!(stdout, SetForegroundColor(color))?;
        queue!(stdout, Print(format!("#{:<8}", channel_name)))?;
        queue!(stdout, ResetColor)?;
        queue!(stdout, Print(" │ "))?;
        // Flush queued commands before writing raw content
        stdout.flush()?;

        // Truncate line if it's too long (accounting for ANSI codes)
        let prefix_len = 12; // "#channel  │ "
        let max_line_len = (self.size.0 as usize).saturating_sub(prefix_len);
        let visible_len = Self::visible_len(content);

        // Write content directly to preserve ANSI escape sequences
        if visible_len > max_line_len && max_line_len > 0 {
            let display_line = truncate_with_ansi(content, max_line_len);
            stdout.write_all(display_line.as_bytes())?;
        } else {
            stdout.write_all(content.as_bytes())?;
            // Reset colors after content to prevent color bleeding
            stdout.write_all(b"\x1b[0m")?;
        }

        Ok(())
    }

    /// Draw welcome tips in the output area
    fn draw_welcome_tips(&self, stdout: &mut impl Write) -> io::Result<()> {
        let visible_rows = self.visible_output_rows();
        let start_row = self.output_start_row();

        // Center the welcome tips vertically if there's enough space
        let tips_height = WELCOME_TIPS.len();
        let vertical_offset = if visible_rows > tips_height {
            (visible_rows - tips_height) / 2
        } else {
            0
        };

        // Clear all output rows first
        for i in 0..visible_rows {
            let row = start_row + i as u16;
            queue!(stdout, cursor::MoveTo(0, row))?;
            queue!(stdout, terminal::Clear(ClearType::CurrentLine))?;
        }

        // Draw tips with color
        for (i, tip) in WELCOME_TIPS.iter().enumerate() {
            if i >= visible_rows {
                break;
            }
            let row = start_row + (vertical_offset + i) as u16;
            if row >= start_row + visible_rows as u16 {
                break;
            }

            queue!(stdout, cursor::MoveTo(2, row))?;

            // Color the header line differently
            if i == 0 {
                queue!(stdout, SetForegroundColor(Color::Cyan))?;
                queue!(stdout, Print(tip))?;
                queue!(stdout, ResetColor)?;
            } else if tip.starts_with("  :") || tip.starts_with("  #") || tip.starts_with("  [") {
                // Command hints - use green for commands
                let parts: Vec<&str> = tip.splitn(2, "  ").collect();
                if parts.len() == 2 {
                    queue!(stdout, SetForegroundColor(Color::Green))?;
                    queue!(stdout, Print(format!("  {}", parts[0].trim())))?;
                    queue!(stdout, ResetColor)?;
                    queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
                    queue!(stdout, Print(format!("  {}", parts[1])))?;
                    queue!(stdout, ResetColor)?;
                } else {
                    queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
                    queue!(stdout, Print(tip))?;
                    queue!(stdout, ResetColor)?;
                }
            } else {
                queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
                queue!(stdout, Print(tip))?;
                queue!(stdout, ResetColor)?;
            }
        }

        Ok(())
    }

    /// Redraw output area for active channel mode (clean, no prefix)
    fn redraw_channel_output(
        &self,
        stdout: &mut impl Write,
        channel: Option<&str>,
    ) -> io::Result<()> {
        let visible_rows = self.visible_output_rows();
        let start_row = self.output_start_row();

        let (lines, scroll_offset) = if let Some(ch) = channel {
            let buffer = self.channel_buffers.get(ch);
            let offset = self.scroll_offsets.get(ch).copied().unwrap_or(0);
            (buffer, offset)
        } else {
            (None, 0)
        };

        // Clear all output rows first
        for i in 0..visible_rows {
            let row = start_row + i as u16;
            queue!(stdout, cursor::MoveTo(0, row))?;
            queue!(stdout, terminal::Clear(ClearType::CurrentLine))?;
        }

        if let Some(buffer) = lines {
            let buffer_len = buffer.len();
            // Calculate start index accounting for scroll offset
            // scroll_offset 0 = bottom (most recent), higher = scrolled up
            let end_idx = buffer_len.saturating_sub(scroll_offset);
            let start_idx = end_idx.saturating_sub(visible_rows);

            let visible_lines: Vec<&str> = buffer[start_idx..end_idx]
                .iter()
                .map(|s| s.as_str())
                .collect();

            for (i, line) in visible_lines.iter().enumerate() {
                let row = start_row + i as u16;
                self.draw_clean_line_at_row(stdout, row, line)?;
            }
        }

        Ok(())
    }

    /// Redraw output area for all channels mode (interleaved with prefix)
    fn redraw_interleaved_output(&mut self, stdout: &mut impl Write) -> io::Result<()> {
        let visible_rows = self.visible_output_rows();
        let start_row = self.output_start_row();

        // Clear all output rows first
        for i in 0..visible_rows {
            let row = start_row + i as u16;
            queue!(stdout, cursor::MoveTo(0, row))?;
            queue!(stdout, terminal::Clear(ClearType::CurrentLine))?;
        }

        let buffer_len = self.interleaved_buffer.len();
        let skip_count = buffer_len.saturating_sub(visible_rows);

        let visible_lines: Vec<(String, String)> = self
            .interleaved_buffer
            .iter()
            .skip(skip_count)
            .cloned()
            .collect();

        for (i, (channel, content)) in visible_lines.iter().enumerate() {
            let row = start_row + i as u16;
            self.draw_prefixed_line_at_row(stdout, row, channel, content)?;
        }

        Ok(())
    }

    /// Redraw all visible output lines from the buffer
    pub fn redraw_output_area(
        &mut self,
        stdout: &mut impl Write,
        active_channel: Option<&str>,
    ) -> io::Result<()> {
        // Check if we have any output at all
        let has_output = !self.channel_buffers.is_empty()
            || !self.interleaved_buffer.is_empty()
            || active_channel
                .and_then(|ch| self.channel_buffers.get(ch))
                .map(|b| !b.is_empty())
                .unwrap_or(false);

        // Show welcome tips if no output and welcome is enabled
        if !has_output && self.show_welcome {
            return self.draw_welcome_tips(stdout);
        }

        match self.view_mode {
            ViewMode::ActiveChannel => self.redraw_channel_output(stdout, active_channel),
            ViewMode::AllChannels => self.redraw_interleaved_output(stdout),
        }
    }

    /// Draw a channel output line with proper scrolling
    pub fn draw_output_line(
        &mut self,
        stdout: &mut impl Write,
        channel_name: &str,
        line: &str,
        active_channel: Option<&str>,
    ) -> io::Result<()> {
        // Hide welcome tips once we have actual output
        self.show_welcome = false;

        // Add to channel-specific buffer
        let buffer = self
            .channel_buffers
            .entry(channel_name.to_string())
            .or_default();
        buffer.push(line.to_string());

        // Trim buffer if it exceeds max size
        if buffer.len() > self.max_buffer_lines {
            let excess = buffer.len() - self.max_buffer_lines;
            buffer.drain(0..excess);
        }

        // Also add to interleaved buffer for "all channels" view
        self.interleaved_buffer
            .push((channel_name.to_string(), line.to_string()));
        if self.interleaved_buffer.len() > self.max_buffer_lines {
            let excess = self.interleaved_buffer.len() - self.max_buffer_lines;
            self.interleaved_buffer.drain(0..excess);
        }

        // Auto-scroll to bottom when new output arrives (if not manually scrolled)
        if !self.is_scrolled(Some(channel_name)) {
            self.scroll_to_bottom(Some(channel_name));
        }

        // Redraw the output area
        self.redraw_output_area(stdout, active_channel)?;

        stdout.flush()
    }

    /// Clear the output buffer for a specific channel or all
    pub fn clear_output_buffer(&mut self, channel: Option<&str>) {
        match channel {
            Some(ch) => {
                self.channel_buffers.remove(ch);
                self.scroll_offsets.remove(ch);
            }
            None => {
                self.channel_buffers.clear();
                self.scroll_offsets.clear();
                self.interleaved_buffer.clear();
            }
        }
    }

    /// Enter raw mode for terminal
    pub fn enter_raw_mode() -> io::Result<()> {
        terminal::enable_raw_mode()
    }

    /// Exit raw mode
    pub fn exit_raw_mode() -> io::Result<()> {
        terminal::disable_raw_mode()
    }

    /// Clear the screen and set up the UI layout
    pub fn clear(stdout: &mut impl Write) -> io::Result<()> {
        execute!(
            stdout,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )
    }

    /// Draw a full UI refresh with separators
    pub fn draw_full_ui(
        &mut self,
        stdout: &mut impl Write,
        channels: &[ChannelStatusInfo],
        active_channel: Option<&str>,
        input: &str,
        cursor_pos: usize,
    ) -> io::Result<()> {
        // Draw status bar
        self.draw_status_bar(stdout, channels, active_channel)?;

        // Redraw output area from buffer
        self.redraw_output_area(stdout, active_channel)?;

        // Draw separator line below status bar (if top position)
        if matches!(self.status_bar_position, StatusBarPosition::Top) {
            queue!(stdout, cursor::MoveTo(0, 1))?;
            queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
            let separator = "─".repeat(self.size.0 as usize);
            queue!(stdout, Print(&separator))?;
            queue!(stdout, ResetColor)?;
        }

        // Draw separator line above prompt
        let separator_row = self.size.1.saturating_sub(2);
        queue!(stdout, cursor::MoveTo(0, separator_row))?;
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        let separator = "─".repeat(self.size.0 as usize);
        queue!(stdout, Print(&separator))?;
        queue!(stdout, ResetColor)?;

        // Draw prompt
        self.draw_prompt(stdout, active_channel, input, cursor_pos)?;

        stdout.flush()
    }

    /// Calculate the visible length of a string (excluding ANSI codes)
    pub fn visible_len(s: &str) -> usize {
        strip_ansi_codes(s).chars().count()
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            size: (80, 24),
            status_bar_height: 1,
            prompt_height: 1,
            show_timestamps: false,
            channel_colors: HashMap::new(),
            status_bar_position: StatusBarPosition::Top,
            channel_buffers: HashMap::new(),
            scroll_offsets: HashMap::new(),
            max_buffer_lines: 10000,
            show_welcome: true,
            view_mode: ViewMode::ActiveChannel,
            interleaved_buffer: Vec::new(),
        })
    }
}

/// Channel status for status bar display
pub struct ChannelStatusInfo {
    pub name: String,
    pub running: bool,
    pub has_new_output: bool,
    pub exit_code: Option<i32>,
}

impl ChannelStatusInfo {
    fn status_indicator(&self) -> &'static str {
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

/// Truncate a string with ANSI codes to a maximum visible length.
/// Preserves ANSI escape sequences while counting only visible characters.
fn truncate_with_ansi(s: &str, max_visible_len: usize) -> String {
    let mut result = String::new();
    let mut visible_count = 0;
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Start of ANSI escape sequence
            result.push(c);
            // Consume the entire escape sequence
            while let Some(&next) = chars.peek() {
                result.push(chars.next().unwrap());
                // CSI sequences end with a letter (A-Z, a-z) or ~
                if next.is_ascii_alphabetic() || next == '~' {
                    break;
                }
            }
        } else {
            // Regular visible character
            if visible_count >= max_visible_len {
                break;
            }
            result.push(c);
            visible_count += 1;
        }
    }

    // Add reset sequence to ensure colors don't bleed
    result.push_str("\x1b[0m");
    result
}
