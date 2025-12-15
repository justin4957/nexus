//! Client - user-facing terminal interface

mod app;
mod commands;
mod completion;
mod input;
mod ui;

use crate::client::app::{App, ChannelInfo, ViewMode};
use crate::client::commands::{handle_control_command, CommandResult};
use crate::client::input::{parse_input, ParsedInput};
use crate::config::Config;
use crate::protocol::{ChannelEvent, ClientMessage, ServerMessage};
use crate::server::connection::{read_message, write_message};
use anyhow::{anyhow, Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
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
    start_new_session(name).await
}

/// Handle scroll keys when input buffer is empty
fn handle_scroll_keys(key: &KeyEvent, app: &mut App) -> bool {
    let page_size = 20;

    if app.show_help { // If help is open, scroll help text
        match key.code {
            KeyCode::PageUp => {
                app.help_scroll = app.help_scroll.saturating_sub(page_size);
                true
            }
            KeyCode::PageDown => {
                app.help_scroll = app.help_scroll.saturating_add(page_size);
                true
            }
            KeyCode::Home => {
                app.help_scroll = 0;
                true
            }
            KeyCode::End => {
                app.help_scroll = usize::MAX; // Will be clamped by UI
                true
            }
            _ => false,
        }
    } else { // Normal scrolling
        match key.code {
            KeyCode::PageUp => {
                if app.view_mode == ViewMode::AllChannels {
                    app.scroll_interleaved_up(page_size);
                } else {
                    app.scroll_up(page_size);
                }
                true
            }
            KeyCode::PageDown => {
                if app.view_mode == ViewMode::AllChannels {
                    app.scroll_interleaved_down(page_size);
                } else {
                    app.scroll_down(page_size);
                }
                true
            }
            KeyCode::Home => {
                if app.view_mode == ViewMode::AllChannels {
                    app.interleaved_scroll = usize::MAX; // Will be clamped by UI
                } else {
                    let active = app.active_channel.clone();
                    app.scroll_to_bottom(active.as_deref());
                    if let Some(ch) = app.active_channel.clone() {
                        app.scroll_offsets.insert(ch, usize::MAX);
                        app.scroll_up(usize::MAX);
                    }
                }
                true
            }
            KeyCode::End => {
                if app.view_mode == ViewMode::AllChannels {
                    app.interleaved_scroll = 0;
                } else {
                    let active = app.active_channel.clone();
                    app.scroll_to_bottom(active.as_deref());
                }
                true
            }
            KeyCode::Tab => {
                if !app.line_editor.is_empty() {
                    let completions =
                        crate::client::completion::complete(app.line_editor.content(), &app);

                    if completions.len() == 1 {
                        app.line_editor.set(&completions[0]);
                        app.completions = None;
                    } else if !completions.is_empty() {
                        if let Some(prefix) = crate::client::completion::common_prefix(&completions) {
                            if prefix.len() > app.line_editor.content().len() {
                                app.line_editor.set(&prefix);
                            }
                        }
                        app.completions = Some(completions);
                    } else {
                        app.completions = None;
                    }
                } else {
                    app.view_mode = match app.view_mode {
                        ViewMode::ActiveChannel => ViewMode::AllChannels,
                        ViewMode::AllChannels => ViewMode::ActiveChannel,
                    };
                }
                true
            }
            _ => false,
        }
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

    // Setup Ratatui Terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Load config
    let config = Config::load()?;

    // Notification settings
    let notify_bell = config.notifications.bell;
    let notify_title = config.notifications.title_update;
    let notify_cooldown = std::time::Duration::from_secs(config.notifications.cooldown_seconds);
    let mut last_notification: HashMap<String, std::time::Instant> = HashMap::new();

    // Channels
    let (input_tx, mut input_rx) = mpsc::channel(100);
    let (server_tx, mut server_rx) = mpsc::channel(100);
    let (msg_tx, mut msg_rx) = mpsc::channel(100);

    // Input thread
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

    // Request channel list
    msg_tx.send(ClientMessage::ListChannels).await?;

    // App State
    let mut app = App::new();
    app.show_channel_numbers = config.appearance.show_channel_numbers;
    // app.line_wrap = config.appearance.line_wrap; // If we support line wrap toggle

    let mut history: HashMap<String, CommandHistory> = HashMap::new();
    let mut should_exit = false;
    let mut line_buffers: HashMap<String, String> = HashMap::new();

    // Send initial resize
    if let Ok(size) = terminal.size() {
        msg_tx
            .send(ClientMessage::Resize {
                cols: size.width,
                rows: size.height,
            })
            .await?;
    }

    loop {
        // Filter out expired notifications
        let now = std::time::Instant::now();
        app.notifications.retain(|n| now.duration_since(n.timestamp) < n.duration);

        // Draw UI
        terminal.draw(|f| ui::draw(f, &mut app))?;

        // Set title
        if notify_title {
            let title = if let Some(active) = &app.active_channel {
                format!("nexus: #{}", active)
            } else {
                "nexus".to_string()
            };
            let _ = execute!(std::io::stdout(), crossterm::terminal::SetTitle(title));
        }

        tokio::select! {
            Some(msg) = server_rx.recv() => {
                match msg {
                    ServerMessage::Welcome { .. } => {}, // Ignore
                    ServerMessage::Status { channels: status } => {
                        if status.is_empty() {
                            app.add_output("SYSTEM".to_string(), "No status available.".to_string());
                            app.add_notification("No status available.".to_string(), Duration::from_secs(3));
                        } else {
                            for s in status {
                                let msg_str = format!(
                                    "#{} {} pid={:?} exit={:?} cwd={} cmd={}",
                                    s.name,
                                    if s.running { "running" } else { "stopped" },
                                    s.pid,
                                    s.exit_code,
                                    s.working_dir,
                                    s.command
                                );
                                app.add_output("SYSTEM".to_string(), msg_str.clone());
                                app.add_notification(msg_str, Duration::from_secs(5));
                            }
                        }
                    },
                    ServerMessage::Output { channel, data, .. } => {
                        let is_background = Some(channel.as_str()) != app.active_channel.as_deref();
                        if let Some(c) = app.channels.iter_mut().find(|c| c.name == channel) {
                            if is_background {
                                c.has_new_output = true;

                                let now = std::time::Instant::now();
                                let should_notify = last_notification
                                    .get(&channel)
                                    .map(|&last| now.duration_since(last) >= notify_cooldown)
                                    .unwrap_or(true);

                                if should_notify {
                                    last_notification.insert(channel.clone(), now);
                                    if notify_bell {
                                        // Bell
                                        print!("\x07");
                                    }
                                    app.add_notification(format!("New output in #{}", channel), Duration::from_secs(3));
                                }
                            }
                        }

                        let text = String::from_utf8_lossy(&data);
                        if !text.is_empty() {
                            let buffer = line_buffers.entry(channel.clone()).or_default();
                            buffer.push_str(&text);

                            while let Some(newline_pos) = buffer.find('\n') {
                                let line = buffer[..newline_pos].to_string();
                                *buffer = buffer[newline_pos + 1..].to_string();
                                let clean_line = line.trim_end_matches('\r').to_string();
                                // We don't strip ANSI here, let UI handle it
                                app.add_output(channel.clone(), clean_line);
                            }
                        }
                    },
                    ServerMessage::ChannelList { channels: list } => {
                        let active_from_server = list.iter().find(|info| info.is_active).map(|info| info.name.clone());
                        app.subscriptions = list.iter().filter(|info| info.is_subscribed).map(|info| info.name.clone()).collect();

                        app.channels = list.into_iter().map(|info| ChannelInfo {
                            name: info.name,
                            running: info.running,
                            has_new_output: false,
                            exit_code: None,
                            is_subscribed: info.is_subscribed,
                        }).collect();

                        if let Some(active) = active_from_server {
                            app.active_channel = Some(active);
                        } else if app.active_channel.is_none() {
                             if let Some(c) = app.channels.first() {
                                 app.active_channel = Some(c.name.clone());
                             }
                        }
                    },
                    ServerMessage::Event(event) => {
                         match event {
                            ChannelEvent::Created { name } => {
                                app.channels.push(ChannelInfo {
                                    name: name.clone(),
                                    running: true,
                                    has_new_output: false,
                                    exit_code: None,
                                });
                                if app.active_channel.is_none() {
                                    app.active_channel = Some(name.clone()); // Clone to use here
                                }
                                app.add_notification(format!("Channel #{} created.", name), Duration::from_secs(3));
                            }
                            ChannelEvent::Exited { name, exit_code } => {
                                if let Some(c) = app.channels.iter_mut().find(|c| c.name == name) {
                                    c.running = false;
                                    c.exit_code = exit_code;
                                }
                                app.add_notification(format!("Channel #{} exited with code {:?}", name, exit_code), Duration::from_secs(3));
                            }
                            ChannelEvent::Killed { name } => {
                                if let Some(c) = app.channels.iter_mut().find(|c| c.name == name) {
                                    c.running = false;
                                    c.exit_code = None;
                                }
                                app.add_notification(format!("Channel #{} killed.", name), Duration::from_secs(3));
                            }
                            ChannelEvent::ActiveChanged { name } => {
                                app.active_channel = Some(name.clone());
                                if let Some(c) = app.channels.iter_mut().find(|c| c.name == name) {
                                    c.has_new_output = false;
                                }
                                let ch_name = Some(name.clone());
                                app.scroll_to_bottom(ch_name.as_deref());
                                app.add_notification(format!("Switched to #{}", name), Duration::from_secs(2));
                                app.last_switched_channel_time = Some(std::time::Instant::now());
                            }
                            ChannelEvent::SubscriptionChanged { subscribed } => {
                                app.subscriptions = subscribed;
                                let msg_str = format!(
                                    "Subscriptions updated: {}",
                                    if app.subscriptions.is_empty() { "none".to_string() } else { app.subscriptions.join(", ") }
                                );
                                app.add_output("SYSTEM".to_string(), msg_str.clone());
                                app.add_notification(msg_str, Duration::from_secs(3));
                            }
                        }
                    },
                    ServerMessage::Error { message } => {
                        app.add_output("SYSTEM".to_string(), format!("Error: {}", message));
                        app.add_notification(format!("Error: {}", message), Duration::from_secs(5));
                    },
                    _ => {} // Ignore other server messages
                }
            },

            Some(event) = input_rx.recv() => {
                match event {
                    Event::Resize(cols, rows) => {
                        msg_tx.send(ClientMessage::Resize { cols, rows }).await?;
                        terminal.autoresize()?;
                    },
                    Event::Mouse(mouse_event) => {
                        // TODO: Implement mouse clicking on channel tabs if possible
                        // For now we just ignore or maybe handle scrolling
                         match mouse_event.kind {
                            MouseEventKind::ScrollUp => {
                                app.scroll_up(3);
                            }
                            MouseEventKind::ScrollDown => {
                                app.scroll_down(3);
                            }
                            _ => {} // Ignore other mouse events
                        }
                    },
                    Event::Key(key) => {
                        if app.command_palette_active {
                            match key.code {
                                KeyCode::Char(c) => {
                                    app.command_palette_input.push(c);
                                    let all_commands = vec![
                                        "new", "kill", "list", "status", "sub", "unsub",
                                        "subs", "clear", "view", "timestamps", "help", "quit"
                                    ].into_iter().map(|s| s.to_string()).collect::<Vec<String>>();
                                    app.command_palette_suggestions = all_commands.into_iter()
                                        .filter(|cmd| cmd.starts_with(&app.command_palette_input))
                                        .collect();
                                }
                                KeyCode::Backspace => {
                                    app.command_palette_input.pop();
                                    let all_commands = vec![
                                        "new", "kill", "list", "status", "sub", "unsub",
                                        "subs", "clear", "view", "timestamps", "help", "quit"
                                    ].into_iter().map(|s| s.to_string()).collect::<Vec<String>>();
                                    app.command_palette_suggestions = all_commands.into_iter()
                                        .filter(|cmd| cmd.starts_with(&app.command_palette_input))
                                        .collect();
                                }
                                KeyCode::Enter => {
                                    if let Some(command) = app.command_palette_suggestions.first().cloned() {
                                        let parsed_input = parse_input(&format!(":{}", command));
                                        if let Ok(ParsedInput::ControlCommand { command, args }) = parsed_input {
                                            match handle_control_command(
                                                &command, args, &mut app, &msg_tx, &format!(":{}", command)
                                            ).await? {
                                                CommandResult::Exit => should_exit = true,
                                                CommandResult::Continue => {}
                                            }
                                        }
                                    }
                                    app.command_palette_active = false;
                                    app.command_palette_input.clear();
                                    app.command_palette_suggestions.clear();
                                }
                                KeyCode::Esc => {
                                    app.command_palette_active = false;
                                    app.command_palette_input.clear();
                                    app.command_palette_suggestions.clear();
                                }
                                _ => {}
                            }
                        } else if app.show_help { // If help is open, handle its input
                            match key.code {
                                KeyCode::PageUp => { app.help_scroll = app.help_scroll.saturating_sub(1); }
                                KeyCode::PageDown => { app.help_scroll = app.help_scroll.saturating_add(1); }
                                KeyCode::Esc | KeyCode::Char('?') => { app.show_help = false; }
                                _ => {}
                            }
                        } else { // Normal input handling
                            if app.line_editor.is_empty() && handle_scroll_keys(&key, &mut app) {
                                continue;
                            }

                            let channel_key = app.active_channel.clone().unwrap_or_default();

                            match key.code {
                                KeyCode::Char(c) => {
                                    app.completions = None;
                                    if key.modifiers.contains(KeyModifiers::ALT) {
                                        if (1..=9).contains(&c.to_digit(10).unwrap_or(0)) {
                                            let idx = (c.to_digit(10).unwrap_or(0) - 1) as usize;
                                            if let Some(channel) = app.channels.get(idx) {
                                                msg_tx.send(ClientMessage::SwitchChannel { name: channel.name.clone() }).await?;
                                                app.last_switched_channel_time = Some(std::time::Instant::now());
                                            }
                                        }
                                    } else if key.modifiers.contains(KeyModifiers::CONTROL) {
                                        match c {
                                            'p' => { // Ctrl+P to toggle command palette
                                                app.command_palette_active = !app.command_palette_active;
                                                if app.command_palette_active {
                                                    app.command_palette_input.clear();
                                                    app.command_palette_suggestions = crate::client::completion::complete("", &app);
                                                }
                                            }
                                            'c' => {
                                                if app.line_editor.is_empty() {
                                                    msg_tx.send(ClientMessage::Input { data: vec![3] }).await?;
                                                } else {
                                                    app.line_editor.clear();
                                                    if let Some(h) = history.get_mut(&channel_key) { h.reset_position(); }
                                                }
                                            }
                                            '\\' => should_exit = true,
                                            'd' => {
                                                if app.line_editor.is_empty() {
                                                     msg_tx.send(ClientMessage::Input { data: vec![4] }).await?;
                                                }
                                            },
                                            'a' => { app.line_editor.move_home(); },
                                            'e' => { app.line_editor.move_end(); },
                                            'w' => { app.line_editor.delete_word_backward(); },
                                            'k' => { app.line_editor.delete_to_end(); },
                                            'u' => {
                                                 if app.line_editor.is_empty() {
                                                     app.scroll_up(10);
                                                 } else {
                                                     app.line_editor.delete_to_start();
                                                 }
                                            },
                                            'b' => { app.scroll_down(10); },
                                            _ => {} // Ignore other control chars
                                        }
                                    } else {
                                        app.line_editor.insert(c);
                                        let active = app.active_channel.clone();
                                        app.scroll_to_bottom(active.as_deref());
                                        if let Some(h) = history.get_mut(&channel_key) { h.reset_position(); }
                                    }
                                }
                                KeyCode::Backspace => { app.line_editor.backspace(); },
                                KeyCode::Delete => { app.line_editor.delete(); },
                                KeyCode::Left => {
                                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                                        // Switch channel
                                        app.prev_channel();
                                        if let Some(ch) = &app.active_channel {
                                            msg_tx.send(ClientMessage::SwitchChannel { name: ch.clone() }).await?;
                                            app.last_switched_channel_time = Some(std::time::Instant::now());
                                        }
                                    } else {
                                        app.line_editor.move_left();
                                    }
                                },
                                KeyCode::Right => {
                                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                                        app.next_channel();
                                         if let Some(ch) = &app.active_channel {
                                            msg_tx.send(ClientMessage::SwitchChannel { name: ch.clone() }).await?;
                                            app.last_switched_channel_time = Some(std::time::Instant::now());
                                        }
                                    } else {
                                        app.line_editor.move_right();
                                    }
                                },
                                KeyCode::Up => {
                                    let h = history.entry(channel_key.clone()).or_insert_with(|| CommandHistory::new(1000));
                                    if let Some(cmd) = h.up(app.line_editor.content()) {
                                        app.line_editor.set(cmd);
                                    }
                                },
                                KeyCode::Down => {
                                    let h = history.entry(channel_key.clone()).or_insert_with(|| CommandHistory::new(1000));
                                    if let Some(cmd) = h.down() {
                                        app.line_editor.set(cmd);
                                    }
                                },
                                KeyCode::Enter => {
                                    let input_content = app.line_editor.take();
                                    if !input_content.is_empty() {
                                        history.entry(channel_key.clone()).or_insert_with(|| CommandHistory::new(1000)).add(&input_content);
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
                                                &mut app,
                                                &msg_tx,
                                                &input_content
                                            ).await? {
                                                CommandResult::Exit => should_exit = true,
                                                CommandResult::Continue => {} // Do nothing
                                            }
                                        }
                                        Err(_) => {} // Ignore parse errors for now
                                    }
                                },
                                _ => {} // Ignore other key events
                            }
                        }
                    },
                    _ => {} // Ignore other events
                }
            },

    Ok(())
}
