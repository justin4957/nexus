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
use std::io::{self, Write};

use std::collections::{HashMap, HashSet};

use crate::config::StatusBarPosition;

const CHANNEL_COLORS: [Color; 6] = [
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::Yellow,
    Color::Green,
    Color::Red,
];

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

    /// Current line in the output area (for tracking where to print next)
    output_line: u16,
}

impl Renderer {
    /// Create a new renderer with default settings
    pub fn new() -> io::Result<Self> {
        Self::with_position(StatusBarPosition::Top)
    }

    /// Create a new renderer with specified status bar position
    pub fn with_position(position: StatusBarPosition) -> io::Result<Self> {
        let size = terminal::size()?;

        // Output starts at line 2 (after status bar and separator)
        let output_start = 2;

        Ok(Self {
            size,
            status_bar_height: 1,
            prompt_height: 1,
            show_timestamps: false,
            channel_colors: HashMap::new(),
            status_bar_position: position,
            output_line: output_start,
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

    /// Get the maximum row for output (line before separator)
    fn max_output_row(&self) -> u16 {
        // Output area ends at row n-3 (separator is at n-2, prompt at n-1)
        self.size.1.saturating_sub(3)
    }

    /// Draw a channel output line with proper scrolling
    pub fn draw_output_line(
        &mut self,
        stdout: &mut impl Write,
        channel_name: &str,
        line: &str,
    ) -> io::Result<()> {
        let max_row = self.max_output_row();

        // If we've reached the bottom of output area, scroll up
        if self.output_line > max_row {
            // Scroll the output area up by moving content
            // We'll redraw from scratch for simplicity - move everything up
            self.output_line = max_row;
        }

        // Move to current output line
        queue!(stdout, cursor::MoveTo(0, self.output_line))?;
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
        let display_line = if line.len() > max_line_len && max_line_len > 0 {
            &line[..max_line_len]
        } else {
            line
        };
        queue!(stdout, Print(display_line))?;

        stdout.flush()?;

        // Move to next line for next output
        self.output_line = self.output_line.saturating_add(1);

        // If we've filled up to the separator, keep at max row (will overwrite)
        if self.output_line > max_row {
            self.output_line = max_row;
        }

        Ok(())
    }

    /// Reset output line position (e.g., after clear)
    pub fn reset_output_position(&mut self) {
        self.output_line = 2; // Start after status bar and separator
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
        // Reset output position
        self.reset_output_position();

        // Draw status bar
        self.draw_status_bar(stdout, channels, active_channel)?;

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
            output_line: 2,
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
