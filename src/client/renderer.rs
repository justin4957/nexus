//! Output rendering - status bar, channel output, prompt

use crossterm::{
    cursor, execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::io::{self, Write};

/// Terminal renderer for nexus client
pub struct Renderer {
    /// Terminal size (cols, rows)
    size: (u16, u16),

    /// Status bar height
    status_bar_height: u16,

    /// Prompt height
    prompt_height: u16,

    /// Whether to show timestamps
    show_timestamps: bool,
}

impl Renderer {
    /// Create a new renderer
    pub fn new() -> io::Result<Self> {
        let size = terminal::size()?;

        Ok(Self {
            size,
            status_bar_height: 1,
            prompt_height: 1,
            show_timestamps: false,
        })
    }

    /// Update terminal size
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.size = (cols, rows);
    }

    /// Height available for output
    pub fn output_height(&self) -> u16 {
        self.size
            .1
            .saturating_sub(self.status_bar_height + self.prompt_height)
    }

    /// Draw the status bar
    pub fn draw_status_bar(
        &self,
        stdout: &mut impl Write,
        channels: &[ChannelStatusInfo],
        active_channel: Option<&str>,
    ) -> io::Result<()> {
        queue!(stdout, cursor::MoveTo(0, 0))?;
        queue!(stdout, terminal::Clear(ClearType::CurrentLine))?;

        for (i, channel) in channels.iter().enumerate() {
            if i > 0 {
                queue!(stdout, Print(" "))?;
            }

            let color = if Some(channel.name.as_str()) == active_channel {
                Color::Green
            } else if channel.has_new_output {
                Color::Yellow
            } else {
                Color::DarkGrey
            };

            queue!(stdout, SetForegroundColor(color))?;
            queue!(
                stdout,
                Print(format!("[#{}{}]", channel.name, channel.status_indicator()))
            )?;
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
        let prompt_row = self.size.1.saturating_sub(1);
        queue!(stdout, cursor::MoveTo(0, prompt_row))?;
        queue!(stdout, terminal::Clear(ClearType::CurrentLine))?;

        let channel_display = active_channel.unwrap_or("none");
        queue!(stdout, SetForegroundColor(Color::Cyan))?;
        queue!(stdout, Print(format!("@{}", channel_display)))?;
        queue!(stdout, ResetColor)?;
        queue!(stdout, Print(format!(" > {}", input)))?;

        stdout.flush()
    }

    /// Draw a channel output line
    pub fn draw_output_line(
        &self,
        stdout: &mut impl Write,
        channel_name: &str,
        line: &str,
        channel_color: Color,
    ) -> io::Result<()> {
        queue!(stdout, SetForegroundColor(channel_color))?;
        queue!(stdout, Print(format!("#{:<8}", channel_name)))?;
        queue!(stdout, ResetColor)?;
        queue!(stdout, Print(" │ "))?;
        queue!(stdout, Print(line))?;
        queue!(stdout, Print("\n"))?;

        stdout.flush()
    }

    /// Enter raw mode for terminal
    pub fn enter_raw_mode() -> io::Result<()> {
        terminal::enable_raw_mode()
    }

    /// Exit raw mode
    pub fn exit_raw_mode() -> io::Result<()> {
        terminal::disable_raw_mode()
    }

    /// Clear the screen
    pub fn clear(stdout: &mut impl Write) -> io::Result<()> {
        execute!(
            stdout,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            size: (80, 24),
            status_bar_height: 1,
            prompt_height: 1,
            show_timestamps: false,
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
