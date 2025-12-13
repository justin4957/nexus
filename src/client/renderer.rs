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

/// A single line of output with its channel name
#[derive(Clone)]
struct OutputLine {
    channel: String,
    content: String,
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

    /// Buffer of output lines for scrolling
    output_buffer: Vec<OutputLine>,

    /// Maximum lines to keep in buffer (for memory management)
    max_buffer_lines: usize,
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
            output_buffer: Vec::new(),
            max_buffer_lines: 10000, // Keep up to 10k lines in memory
        })
    }

    /// Update terminal size
    #[allow(dead_code)]
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.size = (cols, rows);
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
                let remaining = channels.len() - segments.len();
                if remaining > 0 {
                    segments.push((ellipsis.to_string(), Color::DarkGrey));
                }
                break;
            }

            if !segments.is_empty() {
                total_width += 1; // Space between segments
            }
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

        queue!(stdout, ResetColor)?;
        stdout.flush()
    }

    /// Draw the prompt line
    pub fn draw_prompt(
        &self,
        stdout: &mut impl Write,
        active_channel: Option<&str>,
        input: &str,
    ) -> io::Result<()> {
        let prompt_row = self.prompt_row();
        queue!(stdout, cursor::MoveTo(0, prompt_row))?;
        queue!(stdout, terminal::Clear(ClearType::CurrentLine))?;

        let channel_display = active_channel.unwrap_or("none");
        queue!(stdout, SetForegroundColor(Color::Cyan))?;
        queue!(stdout, Print(format!("@{}", channel_display)))?;
        queue!(stdout, ResetColor)?;
        queue!(stdout, Print(format!(" > {}", input)))?;

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
    fn visible_output_rows(&self) -> usize {
        // Output area: from row 2 to row n-3 (inclusive)
        // Row 0: status bar, Row 1: separator, Row n-2: separator, Row n-1: prompt
        self.size.1.saturating_sub(4) as usize
    }

    /// Get the starting row for output area
    fn output_start_row(&self) -> u16 {
        2 // After status bar (row 0) and separator (row 1)
    }

    /// Draw a single output line at a specific row
    fn draw_line_at_row(
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

        // Truncate line if it's too long
        let prefix_len = 12; // "#channel  │ "
        let max_line_len = (self.size.0 as usize).saturating_sub(prefix_len);
        let display_line = if content.len() > max_line_len && max_line_len > 0 {
            &content[..max_line_len]
        } else {
            content
        };
        queue!(stdout, Print(display_line))?;

        Ok(())
    }

    /// Redraw all visible output lines from the buffer
    fn redraw_output_area(&mut self, stdout: &mut impl Write) -> io::Result<()> {
        let visible_rows = self.visible_output_rows();
        let start_row = self.output_start_row();

        // Calculate which lines from buffer to show (most recent ones)
        let buffer_len = self.output_buffer.len();
        let skip_count = buffer_len.saturating_sub(visible_rows);

        // Clone the visible lines to avoid borrow issues
        let visible_lines: Vec<OutputLine> = self
            .output_buffer
            .iter()
            .skip(skip_count)
            .cloned()
            .collect();

        // Clear and redraw each row in the output area
        for (i, line) in visible_lines.iter().enumerate() {
            let row = start_row + i as u16;
            self.draw_line_at_row(stdout, row, &line.channel, &line.content)?;
        }

        // Clear any remaining rows (if buffer has fewer lines than visible area)
        let lines_drawn = visible_lines.len();
        for i in lines_drawn..visible_rows {
            let row = start_row + i as u16;
            queue!(stdout, cursor::MoveTo(0, row))?;
            queue!(stdout, terminal::Clear(ClearType::CurrentLine))?;
        }

        Ok(())
    }

    /// Draw a channel output line with proper scrolling
    pub fn draw_output_line(
        &mut self,
        stdout: &mut impl Write,
        channel_name: &str,
        line: &str,
    ) -> io::Result<()> {
        // Add line to buffer
        self.output_buffer.push(OutputLine {
            channel: channel_name.to_string(),
            content: line.to_string(),
        });

        // Trim buffer if it exceeds max size
        if self.output_buffer.len() > self.max_buffer_lines {
            let excess = self.output_buffer.len() - self.max_buffer_lines;
            self.output_buffer.drain(0..excess);
        }

        // Redraw the entire output area with scrolling
        self.redraw_output_area(stdout)?;

        stdout.flush()
    }

    /// Clear the output buffer (e.g., for :clear command)
    pub fn clear_output_buffer(&mut self) {
        self.output_buffer.clear();
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
    ) -> io::Result<()> {
        // Draw status bar
        self.draw_status_bar(stdout, channels, active_channel)?;

        // Redraw output area from buffer
        self.redraw_output_area(stdout)?;

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
        self.draw_prompt(stdout, active_channel, input)?;

        stdout.flush()
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
            output_buffer: Vec::new(),
            max_buffer_lines: 10000,
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
