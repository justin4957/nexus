//! Client - user-facing terminal interface

mod commands;
mod input;
mod renderer;

use crate::client::commands::{handle_control_command, CommandResult};
use crate::client::input::{parse_input, ParsedInput};
use crate::client::renderer::{ChannelStatusInfo, Renderer};
use crate::config::Config;
use crate::protocol::{ChannelEvent, ClientMessage, ServerMessage};
use crate::server::connection::{read_message, write_message};
use anyhow::{anyhow, Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::time::sleep;

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

    // Setup UI
    let mut renderer = Renderer::new()?;
    Renderer::enter_raw_mode()?;

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
    let mut input_buffer = String::new();
    let mut channels: Vec<ChannelStatusInfo> = Vec::new();
    let mut active_channel: Option<String> = None;
    let mut subscriptions: Vec<String> = Vec::new();
    let mut should_exit = false;

    // Redraw initially
    Renderer::clear(&mut std::io::stdout())?;
    renderer.draw_status_bar(&mut std::io::stdout(), &channels, active_channel.as_deref())?;
    renderer.draw_prompt(
        &mut std::io::stdout(),
        active_channel.as_deref(),
        &input_buffer,
    )?;

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
                                )?;
                            }
                        }
                    }
                    ServerMessage::Output { channel, data, .. } => {
                        // Mark channel as having output
                        if let Some(c) = channels.iter_mut().find(|c| c.name == channel) {
                            if Some(channel.as_str()) != active_channel.as_deref() {
                                c.has_new_output = true;
                            }
                        }

                        // Display if it's the active channel
                        // TODO: Implement background output handling (maybe just status indicator)
                        // For now, only print if active or maybe we print everything with channel prefix?
                        // Renderer::draw_output_line prints with channel name.

                        let text = String::from_utf8_lossy(&data);

                        for line in text.lines() {
                             renderer.draw_output_line(&mut std::io::stdout(), &channel, line)?;
                        }
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
                                active_channel = Some(name);
                                // Clear has_new_output for this channel
                                if let Some(active) = active_channel.as_ref() {
                                    if let Some(c) = channels.iter_mut().find(|c| c.name == *active) {
                                        c.has_new_output = false;
                                    }
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
                                )?;
                            }
                        }
                    }
                    ServerMessage::Error { message } => {
                         renderer.draw_output_line(
                             &mut std::io::stdout(),
                             "SYSTEM",
                             &format!("Error: {}", message)
                         )?;
                    }
                    _ => {} // Ignore other server messages for now
                }

                renderer.draw_status_bar(&mut std::io::stdout(), &channels, active_channel.as_deref())?;
                renderer.draw_prompt(&mut std::io::stdout(), active_channel.as_deref(), &input_buffer)?;
            }

            // Handle user input
            Some(event) = input_rx.recv() => {
                if let Event::Key(key) = event {
                    match key.code {
                        KeyCode::Char(c) => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                match c {
                                    'c' => {
                                        if input_buffer.is_empty() {
                                            // Send ETX
                                            msg_tx.send(ClientMessage::Input { data: vec![3] }).await?;
                                        } else {
                                            input_buffer.clear();
                                        }
                                    }
                                    '\\' => {
                                        should_exit = true;
                                    }
                                    'd' => {
                                         if input_buffer.is_empty() {
                                            // Send EOT
                                            msg_tx.send(ClientMessage::Input { data: vec![4] }).await?;
                                         }
                                    }
                                    _ => {} // Ignore other control chars
                                }
                            } else {
                                input_buffer.push(c);
                            }
                        }
                        KeyCode::Backspace => {
                            input_buffer.pop();
                        }
                        KeyCode::Enter => {
                            match parse_input(&input_buffer) {
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
                                        &input_buffer,
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
                            input_buffer.clear();
                        }
                        _ => {} // Ignore other key events
                    }
                    renderer.draw_prompt(&mut std::io::stdout(), active_channel.as_deref(), &input_buffer)?;
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
