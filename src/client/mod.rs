//! Client - user-facing terminal interface

mod commands;
mod completion;
mod input;
mod renderer;

use crate::client::commands::{handle_control_command, CommandResult};
use crate::client::input::{parse_input, ParsedInput};
use crate::client::renderer::{ChannelStatusInfo, Renderer};
use crate::config::Config;
use crate::protocol::{ChannelEvent, ClientMessage, ServerMessage};
use crate::server::connection::{read_message, write_message};
use anyhow::{anyhow, Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::time::sleep;

/// Command history for input recall
struct CommandHistory {
    /// History entries (oldest first)
    entries: Vec<String>,
    /// Current position in history (None = not browsing history)
    position: Option<usize>,
    /// Maximum entries to keep
    max_entries: usize,
    /// Saved current input when browsing history
    saved_input: String,
}

impl CommandHistory {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            position: None,
            max_entries,
            saved_input: String::new(),
        }
    }

    /// Add a command to history (only if non-empty and different from last)
    fn add(&mut self, command: &str) {
        if command.is_empty() {
            return;
        }
        // Don't add duplicates of the last entry
        if self.entries.last().map(|s| s.as_str()) == Some(command) {
            return;
        }
        self.entries.push(command.to_string());
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.position = None;
        self.saved_input.clear();
    }

    /// Move up in history (older), returning the command to display
    fn up(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        let new_pos = match self.position {
            None => {
                // Save current input before browsing
                self.saved_input = current_input.to_string();
                self.entries.len().saturating_sub(1)
            }
            Some(0) => 0, // Already at oldest
            Some(pos) => pos - 1,
        };

        self.position = Some(new_pos);
        self.entries.get(new_pos).map(|s| s.as_str())
    }

    /// Move down in history (newer), returning the command to display
    fn down(&mut self) -> Option<&str> {
        match self.position {
            None => None,
            Some(pos) => {
                if pos + 1 >= self.entries.len() {
                    // Return to current input
                    self.position = None;
                    Some(self.saved_input.as_str())
                } else {
                    self.position = Some(pos + 1);
                    self.entries.get(pos + 1).map(|s| s.as_str())
                }
            }
        }
    }

    /// Reset history browsing state
    fn reset_position(&mut self) {
        self.position = None;
        self.saved_input.clear();
    }
}

/// Input line editor with cursor position tracking
struct LineEditor {
    /// The input buffer
    buffer: String,
    /// Cursor position (byte index)
    cursor: usize,
}

impl LineEditor {
    fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
        }
    }

    /// Get the current buffer content
    fn content(&self) -> &str {
        &self.buffer
    }

    /// Get cursor position
    fn cursor_position(&self) -> usize {
        self.cursor
    }

    /// Check if buffer is empty
    fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Insert a character at cursor position
    fn insert(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Delete character before cursor (backspace)
    fn backspace(&mut self) -> bool {
        if self.cursor > 0 {
            // Find the previous character boundary
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

    /// Delete character at cursor (delete key)
    fn delete(&mut self) -> bool {
        if self.cursor < self.buffer.len() {
            self.buffer.remove(self.cursor);
            true
        } else {
            false
        }
    }

    /// Move cursor left by one character
    fn move_left(&mut self) -> bool {
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

    /// Move cursor right by one character
    fn move_right(&mut self) -> bool {
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

    /// Move cursor to start of line
    fn move_home(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor = 0;
            true
        } else {
            false
        }
    }

    /// Move cursor to end of line
    fn move_end(&mut self) -> bool {
        if self.cursor < self.buffer.len() {
            self.cursor = self.buffer.len();
            true
        } else {
            false
        }
    }

    /// Delete word backward (Ctrl+W)
    fn delete_word_backward(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }

        // Find start of previous word
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

    /// Delete to end of line (Ctrl+K)
    fn delete_to_end(&mut self) -> bool {
        if self.cursor < self.buffer.len() {
            self.buffer.truncate(self.cursor);
            true
        } else {
            false
        }
    }

    /// Delete to start of line (Ctrl+U)
    fn delete_to_start(&mut self) -> bool {
        if self.cursor > 0 {
            self.buffer.drain(..self.cursor);
            self.cursor = 0;
            true
        } else {
            false
        }
    }

    /// Clear the buffer and reset cursor
    fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    /// Set the buffer content (e.g., from history)
    fn set(&mut self, content: &str) {
        self.buffer = content.to_string();
        self.cursor = self.buffer.len();
    }

    /// Take the buffer content and clear
    fn take(&mut self) -> String {
        let content = std::mem::take(&mut self.buffer);
        self.cursor = 0;
        content
    }
}

/// Start a new session (spawns server if needed)
pub async fn start_new_session(name: &str) -> Result<()> {
    tracing::info!("Starting new session: {}", name);

    let config = Config::load()?;
    let socket_path = config.socket_path(name);

    // Ensure runtime dir exists
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Try to connect
    let stream = match UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(_) => {
            // Spawn server
            println!("nexus: spawning server for session '{}'...", name);
            // Assuming nexus-server is in PATH or same dir
            // For development, we might want to try finding it
            let exe = std::env::current_exe()?
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join("nexus-server");

            let server_bin = if exe.exists() {
                exe.to_string_lossy().to_string()
            } else {
                "nexus-server".to_string()
            };

            Command::new(server_bin)
                .arg("--session")
                .arg(name)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("Failed to spawn nexus-server")?;

            // Wait for socket to appear
            let mut attempts = 0;
            loop {
                sleep(Duration::from_millis(100)).await;
                if let Ok(s) = UnixStream::connect(&socket_path).await {
                    break s;
                }
                attempts += 1;
                if attempts > 20 {
                    return Err(anyhow!("Timed out waiting for server to start"));
                }
            }
        }
    };

    run_client_loop(stream).await
}
/// Attach to an existing session
pub async fn attach_session(name: &str) -> Result<()> {
    tracing::info!("Attaching to session: {}", name);

    let config = Config::load()?;
    let socket_path = config.socket_path(name);

    if !socket_path.exists() {
        return Err(anyhow!("Session '{}' not found", name));
    }

    let stream = UnixStream::connect(&socket_path)
        .await
        .context("Failed to connect to session")?;

    run_client_loop(stream).await
}

/// List available sessions
pub async fn list_sessions() -> Result<()> {
    let config = Config::load()?;
    let runtime_dir = config.runtime_dir();

    if !runtime_dir.exists() {
        println!("No sessions found.");
        return Ok(());
    }

    let mut found = false;
    for entry in std::fs::read_dir(runtime_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("sock") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                println!("{}", stem);
                found = true;
            }
        }
    }

    if !found {
        println!("No sessions found.");
    }
    Ok(())
}

/// Kill a session
pub async fn kill_session(name: &str) -> Result<()> {
    tracing::info!("Killing session: {}", name);

    // Connect and send shutdown
    // We reuse logic but send a Shutdown message immediately
    let config = Config::load()?;
    let socket_path = config.socket_path(name);

    if !socket_path.exists() {
        return Err(anyhow!("Session '{}' not found", name));
    }

    let mut stream = UnixStream::connect(&socket_path).await?;

    // Handshake
    let hello = ClientMessage::Hello {
        protocol_version: 1,
    };
    write_message(&mut stream, &crate::protocol::serialize(&hello)?).await?;

    // Send Shutdown
    let shutdown = ClientMessage::Shutdown;
    write_message(&mut stream, &crate::protocol::serialize(&shutdown)?).await?;

    println!("Session '{}' killed.", name);
    Ok(())
}

/// Attach to session or create if doesn't exist
pub async fn attach_or_create(name: &str) -> Result<()> {
    // We can just use start_new_session, as it handles connection check
    start_new_session(name).await
}

/// Handle scroll keys when input buffer is empty
/// Returns true if a scroll key was handled
fn handle_scroll_keys(key: &KeyEvent, renderer: &mut Renderer, channel: Option<&str>) -> bool {
    match key.code {
        KeyCode::PageUp => {
            let page = renderer.visible_output_rows();
            renderer.scroll_up(channel, page);
            true
        }
        KeyCode::PageDown => {
            let page = renderer.visible_output_rows();
            renderer.scroll_down(channel, page);
            true
        }
        KeyCode::Home => {
            // Scroll to top (oldest)
            renderer.scroll_up(channel, usize::MAX);
            true
        }
        KeyCode::End => {
            // Scroll to bottom (most recent)
            renderer.scroll_to_bottom(channel);
            true
        }
        KeyCode::Tab => {
            // Toggle view mode
            renderer.toggle_view_mode();
            true
        }
        _ => false,
    }
}

/// Main client loop
async fn run_client_loop(stream: UnixStream) -> Result<()> {
    let (mut reader, mut writer) = stream.into_split();

    // 1. Handshake
    let hello = ClientMessage::Hello {
        protocol_version: 1,
    };
    write_message(&mut writer, &crate::protocol::serialize(&hello)?).await?;

    // Wait for Welcome (in the read loop, but we expect it first)
    // Actually, we'll just handle it in the loop

    // Load config for appearance settings
    let config = Config::load()?;

    // Setup UI with config-based status bar position
    let mut renderer = Renderer::with_position(config.appearance.status_bar_position)?;
    renderer.set_line_wrap(config.appearance.line_wrap);
    renderer.set_show_channel_numbers(config.appearance.show_channel_numbers);
    Renderer::enter_raw_mode()?;

    // Notification settings
    let notify_bell = config.notifications.bell;
    let notify_title = config.notifications.title_update;
    let notify_cooldown = std::time::Duration::from_secs(config.notifications.cooldown_seconds);
    let mut last_notification: HashMap<String, std::time::Instant> = HashMap::new();

    // Channels for communication
    let (input_tx, mut input_rx) = mpsc::channel(100);
    let (server_tx, mut server_rx) = mpsc::channel(100);
    let (msg_tx, mut msg_rx) = mpsc::channel(100); // Messages to send to server

    // Input task (blocking)
    std::thread::spawn(move || loop {
        if let Ok(event) = event::read() {
            if input_tx.blocking_send(event).is_err() {
                break;
            }
        }
    });

    // Server read task
    tokio::spawn(async move {
        loop {
            match read_message(&mut reader).await {
                Ok(Some(data)) => match crate::protocol::deserialize::<ServerMessage>(&data) {
                    Ok(msg) => {
                        if server_tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Err(_e) => {
                        tracing::error!("Failed to deserialize: {}", _e);
                    }
                },
                Ok(None) => break, // EOF
                Err(e) => {
                    tracing::error!("Connection error: {}", e);
                    break;
                }
            }
        }
    });

    // Request channel list initially
    msg_tx.send(ClientMessage::ListChannels).await?;

    // State
    let mut line_editor = LineEditor::new();
    let mut history: HashMap<String, CommandHistory> = HashMap::new();
    let mut channels: Vec<ChannelStatusInfo> = Vec::new();
    let mut active_channel: Option<String> = None;
    let mut subscriptions: Vec<String> = Vec::new();
    let mut should_exit = false;
    // Buffer for partial lines (output without trailing newline) per channel
    let mut line_buffers: HashMap<String, String> = HashMap::new();

    // Send initial resize to server
    let (initial_cols, initial_rows) = renderer.terminal_size();
    msg_tx
        .send(ClientMessage::Resize {
            cols: initial_cols,
            rows: initial_rows,
        })
        .await?;

    // Redraw initially with full UI layout
    Renderer::clear(&mut std::io::stdout())?;
    renderer.draw_full_ui(
        &mut std::io::stdout(),
        &channels,
        active_channel.as_deref(),
        line_editor.content(),
        line_editor.cursor_position(),
    )?;

    // Set initial terminal title if enabled
    if notify_title {
        let _ =
            Renderer::update_terminal_title(&mut std::io::stdout(), active_channel.as_deref(), &[]);
    }

    // Main loop
    loop {
        tokio::select! {
            // Handle server messages
            Some(msg) = server_rx.recv() => {
                match msg {
                    ServerMessage::Welcome { .. } => {
                        // Connected
                    }
                    ServerMessage::Status { channels: status } => {
                        if status.is_empty() {
                            renderer.draw_output_line(
                                &mut std::io::stdout(),
                                "SYSTEM",
                                "No status available.",
                                active_channel.as_deref(),
                            )?;
                        } else {
                            for s in status {
                                renderer.draw_output_line(
                                    &mut std::io::stdout(),
                                    "SYSTEM",
                                    &format!(
                                        "#{} {} pid={:?} exit={:?} cwd={} cmd={}",
                                        s.name,
                                        if s.running { "running" } else { "stopped" },
                                        s.pid,
                                        s.exit_code,
                                        s.working_dir,
                                        s.command
                                    ),
                                    active_channel.as_deref(),
                                )?;
                            }
                        }
                    }
                    ServerMessage::Output { channel, data, .. } => {
                        // Mark channel as having output and handle notifications
                        let is_background = Some(channel.as_str()) != active_channel.as_deref();
                        if let Some(c) = channels.iter_mut().find(|c| c.name == channel) {
                            if is_background {
                                c.has_new_output = true;

                                // Check notification cooldown
                                let now = std::time::Instant::now();
                                let should_notify = last_notification
                                    .get(&channel)
                                    .map(|&last| now.duration_since(last) >= notify_cooldown)
                                    .unwrap_or(true);

                                if should_notify {
                                    last_notification.insert(channel.clone(), now);

                                    // Ring terminal bell if enabled
                                    if notify_bell {
                                        let _ = Renderer::ring_bell(&mut std::io::stdout());
                                    }

                                    // Update terminal title if enabled
                                    if notify_title {
                                        let channels_with_output: Vec<&str> = channels
                                            .iter()
                                            .filter(|ch| ch.has_new_output)
                                            .map(|ch| ch.name.as_str())
                                            .collect();
                                        let _ = Renderer::update_terminal_title(
                                            &mut std::io::stdout(),
                                            active_channel.as_deref(),
                                            &channels_with_output,
                                        );
                                    }
                                }
                            }
                        }

                        // Convert to string preserving ANSI escape codes
                        let text = String::from_utf8_lossy(&data);

                        if text.is_empty() {
                            continue;
                        }

                        // Get or create the line buffer for this channel
                        let buffer = line_buffers.entry(channel.clone()).or_default();

                        // Append new text to buffer
                        buffer.push_str(&text);

                        // Process complete lines (those ending with \n)
                        // Keep any partial line (no trailing \n) in the buffer
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].to_string();
                            *buffer = buffer[newline_pos + 1..].to_string();

                            // Clean up the line (remove carriage returns)
                            let clean_line = line.trim_end_matches('\r');

                            // Skip empty lines and lines that are just whitespace
                            // Note: We check the visible content, not ANSI codes
                            let visible_content = renderer::strip_ansi_codes(clean_line);
                            if visible_content.trim().is_empty() {
                                continue;
                            }

                            renderer.draw_output_line(&mut std::io::stdout(), &channel, clean_line, active_channel.as_deref())?;
                        }

                        // Redraw status bar and prompt after output
                        renderer.draw_status_bar(&mut std::io::stdout(), &channels, active_channel.as_deref())?;
                        renderer.draw_prompt(&mut std::io::stdout(), active_channel.as_deref(), line_editor.content(), line_editor.cursor_position())?;
                    }
                    ServerMessage::ChannelList { channels: list } => {
                        let active_from_server = list
                            .iter()
                            .find(|info| info.is_active)
                            .map(|info| info.name.clone());
                        subscriptions = list
                            .iter()
                            .filter(|info| info.is_subscribed)
                            .map(|info| info.name.clone())
                            .collect();

                        channels = list
                            .into_iter()
                            .map(|info| ChannelStatusInfo {
                                name: info.name,
                                running: info.running,
                                has_new_output: false,
                                exit_code: None,
                            })
                            .collect();

                        if let Some(active) = active_from_server {
                            active_channel = Some(active);
                        } else if active_channel.is_none() {
                             if let Some(c) = channels.first() {
                                 active_channel = Some(c.name.clone());
                             }
                        }
                    }
                    ServerMessage::Event(event) => {
                        match event {
                            ChannelEvent::Created { name } => {
                                channels.push(ChannelStatusInfo {
                                    name: name.clone(),
                                    running: true,
                                    has_new_output: false,
                                    exit_code: None,
                                });
                                if active_channel.is_none() {
                                    active_channel = Some(name);
                                }
                            }
                            ChannelEvent::Exited { name, exit_code } => {
                                if let Some(c) = channels.iter_mut().find(|c| c.name == name) {
                                    c.running = false;
                                    c.exit_code = exit_code;
                                }
                                renderer.clear_channel_color(&name);
                            }
                            ChannelEvent::Killed { name } => {
                                if let Some(c) = channels.iter_mut().find(|c| c.name == name) {
                                    c.running = false;
                                    c.exit_code = None;
                                }
                                renderer.clear_channel_color(&name);
                            }
                            ChannelEvent::ActiveChanged { name } => {
                                active_channel = Some(name.clone());
                                // Clear has_new_output for this channel
                                if let Some(c) = channels.iter_mut().find(|c| c.name == name) {
                                    c.has_new_output = false;
                                }
                                // Redraw output for the new active channel
                                renderer.redraw_output_area(&mut std::io::stdout(), active_channel.as_deref())?;

                                // Update terminal title if enabled
                                if notify_title {
                                    let channels_with_output: Vec<&str> = channels
                                        .iter()
                                        .filter(|ch| ch.has_new_output)
                                        .map(|ch| ch.name.as_str())
                                        .collect();
                                    let _ = Renderer::update_terminal_title(
                                        &mut std::io::stdout(),
                                        active_channel.as_deref(),
                                        &channels_with_output,
                                    );
                                }
                            }
                            ChannelEvent::SubscriptionChanged { subscribed } => {
                                subscriptions = subscribed.clone();
                                renderer.draw_output_line(
                                    &mut std::io::stdout(),
                                    "SYSTEM",
                                    &format!(
                                        "Subscriptions updated: {}",
                                        if subscriptions.is_empty() {
                                            "none".to_string()
                                        } else {
                                            subscriptions.join(", ")
                                        }
                                    ),
                                    active_channel.as_deref(),
                                )?;
                            }
                        }
                    }
                    ServerMessage::Error { message } => {
                         renderer.draw_output_line(
                             &mut std::io::stdout(),
                             "SYSTEM",
                             &format!("Error: {}", message),
                             active_channel.as_deref(),
                         )?;
                    }
                    _ => {} // Ignore other server messages for now
                }

                renderer.draw_status_bar(&mut std::io::stdout(), &channels, active_channel.as_deref())?;
                renderer.draw_prompt(&mut std::io::stdout(), active_channel.as_deref(), line_editor.content(), line_editor.cursor_position())?;
            }

            // Handle user input
            Some(event) = input_rx.recv() => {
                match event {
                    // Handle terminal resize (Issue #31)
                    Event::Resize(cols, rows) => {
                        renderer.resize(cols, rows);
                        // Send resize to server for PTY resizing
                        msg_tx.send(ClientMessage::Resize { cols, rows }).await?;
                        // Redraw entire UI
                        Renderer::clear(&mut std::io::stdout())?;
                        renderer.draw_full_ui(
                            &mut std::io::stdout(),
                            &channels,
                            active_channel.as_deref(),
                            line_editor.content(),
                            line_editor.cursor_position(),
                        )?;
                        continue;
                    }
                    // Handle mouse events (Issue #34)
                    Event::Mouse(mouse_event) => {
                        match mouse_event.kind {
                            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                                // Check if click was on status bar
                                if renderer.is_status_bar_click(mouse_event.row) {
                                    // Check if we clicked on a channel
                                    if let Some(channel_name) =
                                        renderer.channel_at_position(mouse_event.column, &channels)
                                    {
                                        // Switch to the clicked channel
                                        msg_tx
                                            .send(ClientMessage::SwitchChannel {
                                                name: channel_name,
                                            })
                                            .await?;
                                    }
                                }
                            }
                            MouseEventKind::ScrollUp => {
                                // Scroll output up by 3 lines
                                renderer.scroll_up(active_channel.as_deref(), 3);
                                renderer.redraw_output_area(
                                    &mut std::io::stdout(),
                                    active_channel.as_deref(),
                                )?;
                                renderer.draw_status_bar(
                                    &mut std::io::stdout(),
                                    &channels,
                                    active_channel.as_deref(),
                                )?;
                                renderer.draw_prompt(
                                    &mut std::io::stdout(),
                                    active_channel.as_deref(),
                                    line_editor.content(),
                                    line_editor.cursor_position(),
                                )?;
                            }
                            MouseEventKind::ScrollDown => {
                                // Scroll output down by 3 lines
                                renderer.scroll_down(active_channel.as_deref(), 3);
                                renderer.redraw_output_area(
                                    &mut std::io::stdout(),
                                    active_channel.as_deref(),
                                )?;
                                renderer.draw_status_bar(
                                    &mut std::io::stdout(),
                                    &channels,
                                    active_channel.as_deref(),
                                )?;
                                renderer.draw_prompt(
                                    &mut std::io::stdout(),
                                    active_channel.as_deref(),
                                    line_editor.content(),
                                    line_editor.cursor_position(),
                                )?;
                            }
                            _ => {} // Ignore other mouse events
                        }
                        continue;
                    }
                    Event::Key(key) => {
                        // Handle scrolling and view mode when input is empty
                        if line_editor.is_empty()
                            && handle_scroll_keys(&key, &mut renderer, active_channel.as_deref())
                        {
                            renderer.redraw_output_area(
                                &mut std::io::stdout(),
                                active_channel.as_deref(),
                            )?;
                            renderer.draw_status_bar(
                                &mut std::io::stdout(),
                                &channels,
                                active_channel.as_deref(),
                            )?;
                            renderer.draw_prompt(
                                &mut std::io::stdout(),
                                active_channel.as_deref(),
                                line_editor.content(),
                                line_editor.cursor_position(),
                            )?;
                            continue;
                        }

                        // Get the current channel's history
                        let channel_key = active_channel.clone().unwrap_or_default();

                        match key.code {
                            KeyCode::Char(c) => {
                                // Alt+1-9 for quick channel switching
                                if key.modifiers.contains(KeyModifiers::ALT) {
                                    if let Some(digit) = c.to_digit(10) {
                                        if (1..=9).contains(&digit) {
                                            let idx = (digit - 1) as usize;
                                            if let Some(channel) = channels.get(idx) {
                                                msg_tx
                                                    .send(ClientMessage::SwitchChannel {
                                                        name: channel.name.clone(),
                                                    })
                                                    .await?;
                                            }
                                        }
                                    }
                                } else if key.modifiers.contains(KeyModifiers::CONTROL) {
                                    match c {
                                        'c' => {
                                            if line_editor.is_empty() {
                                                // Send ETX (interrupt)
                                                msg_tx.send(ClientMessage::Input { data: vec![3] }).await?;
                                            } else {
                                                line_editor.clear();
                                                // Reset history browsing when clearing
                                                if let Some(h) = history.get_mut(&channel_key) {
                                                    h.reset_position();
                                                }
                                            }
                                        }
                                        '\\' => {
                                            should_exit = true;
                                        }
                                        'd' => {
                                            if line_editor.is_empty() {
                                                // Send EOT
                                                msg_tx.send(ClientMessage::Input { data: vec![4] }).await?;
                                            }
                                        }
                                        'a' => {
                                            // Move to start of line (Ctrl+A)
                                            line_editor.move_home();
                                        }
                                        'e' => {
                                            // Move to end of line (Ctrl+E)
                                            line_editor.move_end();
                                        }
                                        'w' => {
                                            // Delete word backward (Ctrl+W)
                                            line_editor.delete_word_backward();
                                        }
                                        'k' => {
                                            // Delete to end of line (Ctrl+K)
                                            line_editor.delete_to_end();
                                        }
                                        'u' => {
                                            if line_editor.is_empty() {
                                                // Scroll up half page when input is empty
                                                let half_page = renderer.visible_output_rows() / 2;
                                                renderer.scroll_up(active_channel.as_deref(), half_page.max(1));
                                                renderer.redraw_output_area(&mut std::io::stdout(), active_channel.as_deref())?;
                                                renderer.draw_status_bar(&mut std::io::stdout(), &channels, active_channel.as_deref())?;
                                            } else {
                                                // Delete to start of line (Ctrl+U)
                                                line_editor.delete_to_start();
                                            }
                                        }
                                        'b' => {
                                            // Scroll down half page
                                            let half_page = renderer.visible_output_rows() / 2;
                                            renderer.scroll_down(active_channel.as_deref(), half_page.max(1));
                                            renderer.redraw_output_area(&mut std::io::stdout(), active_channel.as_deref())?;
                                            renderer.draw_status_bar(&mut std::io::stdout(), &channels, active_channel.as_deref())?;
                                        }
                                        _ => {} // Ignore other control chars
                                    }
                                } else {
                                    line_editor.insert(c);
                                    // Auto-scroll to bottom when user starts typing
                                    renderer.scroll_to_bottom(active_channel.as_deref());
                                    // Reset history browsing when typing new content
                                    if let Some(h) = history.get_mut(&channel_key) {
                                        h.reset_position();
                                    }
                                }
                            }
                            KeyCode::Backspace => {
                                line_editor.backspace();
                            }
                            KeyCode::Delete => {
                                line_editor.delete();
                            }
                            KeyCode::Left => {
                                if key.modifiers.contains(KeyModifiers::CONTROL) {
                                    // Ctrl+Left: switch to previous channel
                                    if let Some(current) = &active_channel {
                                        if let Some(idx) = channels.iter().position(|c| &c.name == current) {
                                            let prev_idx = if idx == 0 {
                                                channels.len().saturating_sub(1)
                                            } else {
                                                idx - 1
                                            };
                                            if let Some(prev_channel) = channels.get(prev_idx) {
                                                msg_tx
                                                    .send(ClientMessage::SwitchChannel {
                                                        name: prev_channel.name.clone(),
                                                    })
                                                    .await?;
                                            }
                                        }
                                    }
                                } else {
                                    line_editor.move_left();
                                }
                            }
                            KeyCode::Right => {
                                if key.modifiers.contains(KeyModifiers::CONTROL) {
                                    // Ctrl+Right: switch to next channel
                                    if let Some(current) = &active_channel {
                                        if let Some(idx) = channels.iter().position(|c| &c.name == current) {
                                            let next_idx = (idx + 1) % channels.len().max(1);
                                            if let Some(next_channel) = channels.get(next_idx) {
                                                msg_tx
                                                    .send(ClientMessage::SwitchChannel {
                                                        name: next_channel.name.clone(),
                                                    })
                                                    .await?;
                                            }
                                        }
                                    }
                                } else {
                                    line_editor.move_right();
                                }
                            }
                            KeyCode::Home => {
                                if !line_editor.is_empty() {
                                    line_editor.move_home();
                                }
                                // When empty, Home is handled by scroll_keys
                            }
                            KeyCode::End => {
                                if !line_editor.is_empty() {
                                    line_editor.move_end();
                                }
                                // When empty, End is handled by scroll_keys
                            }
                            KeyCode::Up => {
                                // Navigate history (older)
                                let channel_history = history
                                    .entry(channel_key.clone())
                                    .or_insert_with(|| CommandHistory::new(1000));
                                if let Some(cmd) = channel_history.up(line_editor.content()) {
                                    line_editor.set(cmd);
                                }
                            }
                            KeyCode::Down => {
                                // Navigate history (newer)
                                let channel_history = history
                                    .entry(channel_key.clone())
                                    .or_insert_with(|| CommandHistory::new(1000));
                                if let Some(cmd) = channel_history.down() {
                                    line_editor.set(cmd);
                                }
                            }
                            KeyCode::Tab => {
                                // Tab completion for commands and channel names
                                if !line_editor.is_empty() {
                                    let channel_names: Vec<String> =
                                        channels.iter().map(|c| c.name.clone()).collect();
                                    let completions =
                                        completion::complete(line_editor.content(), &channel_names);

                                    if completions.len() == 1 {
                                        // Single match - complete it
                                        line_editor.set(&completions[0]);
                                    } else if !completions.is_empty() {
                                        // Multiple matches - try to extend common prefix first
                                        let mut extended = false;
                                        if let Some(prefix) =
                                            completion::common_prefix(&completions)
                                        {
                                            if prefix.len() > line_editor.content().len() {
                                                line_editor.set(&prefix);
                                                extended = true;
                                            }
                                        }
                                        // Show completions if we couldn't extend
                                        if !extended {
                                            // Show completions directly on line above prompt
                                            renderer.show_completions(
                                                &mut std::io::stdout(),
                                                &completions,
                                            )?;
                                            // Redraw prompt to restore cursor position
                                            renderer.draw_prompt(
                                                &mut std::io::stdout(),
                                                active_channel.as_deref(),
                                                line_editor.content(),
                                                line_editor.cursor_position(),
                                            )?;
                                        }
                                    }
                                }
                                // When input is empty, Tab is handled by scroll_keys (view toggle)
                            }
                            KeyCode::Enter => {
                                let input_content = line_editor.take();

                                // Add to history before processing
                                if !input_content.is_empty() {
                                    let channel_history = history
                                        .entry(channel_key.clone())
                                        .or_insert_with(|| CommandHistory::new(1000));
                                    channel_history.add(&input_content);
                                }

                                match parse_input(&input_content) {
                                    Ok(ParsedInput::Text(text)) => {
                                        let mut data = text.into_bytes();
                                        data.push(b'\n');
                                        msg_tx.send(ClientMessage::Input { data }).await?;
                                    }
                                    Ok(ParsedInput::SwitchChannel(name)) => {
                                        msg_tx.send(ClientMessage::SwitchChannel { name }).await?;
                                    }
                                    Ok(ParsedInput::SendToChannel { channel, command }) => {
                                        msg_tx.send(ClientMessage::InputTo {
                                            channel,
                                            data: format!("{}\n", command).into_bytes()
                                        }).await?;
                                    }
                                    Ok(ParsedInput::ControlCommand { command, args }) => {
                                        match handle_control_command(
                                            &command,
                                            args,
                                            &mut renderer,
                                            &msg_tx,
                                            &channels,
                                            &mut active_channel,
                                            &subscriptions,
                                            &input_content,
                                        ).await? {
                                            CommandResult::Exit => should_exit = true,
                                            CommandResult::Continue => {}
                                        }
                                    }
                                    Err(_e) => {
                                        // Print error locally
                                        // We don't have a good way to print local error log yet in renderer
                                    }
                                }
                            }
                            _ => {} // Ignore other key events
                        }
                        renderer.draw_prompt(&mut std::io::stdout(), active_channel.as_deref(), line_editor.content(), line_editor.cursor_position())?;
                    }
                    _ => {} // Ignore other events (mouse, focus, paste)
                }
            }

            // Send messages to server
            Some(msg) = msg_rx.recv() => {
                let bytes = crate::protocol::serialize(&msg)?;
                if let Err(e) = write_message(&mut writer, &bytes).await {
                     tracing::error!("Failed to write to server: {}", e);
                     break;
                }
            }

            else => break, // All channels closed
        }

        if should_exit {
            break;
        }
    }

    Renderer::exit_raw_mode()?;
    Ok(())
}
