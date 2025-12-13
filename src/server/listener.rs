//! Unix socket listener and server main loop

use super::connection::{
    client_writer_task, create_error_message, create_welcome_message, parse_client_message,
    read_message, ClientConnection,
};
use super::session::Session;
use crate::{
    channel::{ChannelManager, ChannelManagerEvent},
    protocol::{ChannelEvent, ClientMessage, ServerMessage, PROTOCOL_VERSION},
};
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

/// Server state shared across connections
struct ServerState {
    session: Session,
    clients: HashMap<Uuid, ClientConnection>,
    channel_manager: ChannelManager,
}

/// Unix socket server listener
pub struct ServerListener {
    socket_path: PathBuf,
    session_name: String,
}

impl ServerListener {
    /// Create a new server listener
    pub fn new(session_name: String, socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            session_name,
        }
    }

    /// Check if socket already exists (another server running)
    pub fn socket_exists(&self) -> bool {
        self.socket_path.exists()
    }

    /// Get the socket path
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Run the server
    pub async fn run(&self, mut shutdown_rx: mpsc::Receiver<()>) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove stale socket if it exists
        if self.socket_path.exists() {
            // Try to connect to check if it's alive
            match UnixStream::connect(&self.socket_path).await {
                Ok(_) => {
                    return Err(anyhow!(
                        "Server already running for session '{}'",
                        self.session_name
                    ));
                }
                Err(_) => {
                    // Stale socket, remove it
                    tracing::info!("Removing stale socket: {:?}", self.socket_path);
                    std::fs::remove_file(&self.socket_path)?;
                }
            }
        }

        // Create Unix socket listener
        let listener = UnixListener::bind(&self.socket_path)?;
        tracing::info!("Server listening on {:?}", self.socket_path);

        // Channel for manager -> server communication
        let (event_tx, mut event_rx) = mpsc::channel::<ChannelManagerEvent>(256);

        // Initialize server state
        let state = Arc::new(RwLock::new(ServerState {
            session: Session::new(self.session_name.clone(), self.socket_path.clone()),
            clients: HashMap::new(),
            channel_manager: ChannelManager::new(event_tx),
        }));

        // Spawn the event handler task
        let event_state = Arc::clone(&state);
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                handle_channel_event(event, &event_state).await;
            }
            tracing::info!("Channel manager event loop finished");
        });

        // Main server loop
        loop {
            tokio::select! {
                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    tracing::info!("Shutdown signal received");
                    break;
                }

                // Accept new connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            let state = Arc::clone(&state);
                            tokio::spawn(async move {
                                if let Err(e) = handle_client(stream, state).await {
                                    tracing::error!("Client error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("Failed to accept connection: {}", e);
                        }
                    }
                }
            }
        }

        // Cleanup
        self.cleanup().await;

        Ok(())
    }

    /// Clean up server resources
    async fn cleanup(&self) {
        tracing::info!("Cleaning up server resources");

        // Remove socket file
        if self.socket_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.socket_path) {
                tracing::error!("Failed to remove socket file: {}", e);
            }
        }
    }
}

/// Handle a single client connection
async fn handle_client(stream: UnixStream, state: Arc<RwLock<ServerState>>) -> Result<()> {
    let (mut reader, writer) = stream.into_split();

    // Create message channel for this client
    let (tx, rx) = mpsc::channel::<ServerMessage>(256);
    let mut client = ClientConnection::new(tx);
    let client_id = client.id();

    tracing::info!("Client connected: {}", client_id);

    // New clients subscribe to the active channel (if any) by default to avoid overwhelming output.
    let (session_id, initial_channels) = {
        let state_guard = state.read().await;
        let initial = state_guard
            .channel_manager
            .active_channel()
            .map(|name| vec![name.to_string()])
            .unwrap_or_default();
        (state_guard.session.id(), initial)
    };
    client.subscribe(&initial_channels);

    // Add client to state
    {
        let mut state_guard = state.write().await;
        state_guard.session.add_client(client_id);
        state_guard.clients.insert(client_id, client);
    }

    // Spawn writer task
    let writer_handle = tokio::spawn(client_writer_task(writer, rx));

    // Send welcome message
    {
        let state = state.read().await;
        if let Some(client) = state.clients.get(&client_id) {
            client.send(create_welcome_message(session_id)).await?;
        }
    }

    // Read and process messages
    loop {
        match read_message(&mut reader).await {
            Ok(Some(bytes)) => match parse_client_message(&bytes) {
                Ok(msg) => {
                    let response = process_message(msg, client_id, &state).await;
                    if let Some(response) = response {
                        let state = state.read().await;
                        if let Some(client) = state.clients.get(&client_id) {
                            if let Err(e) = client.send(response).await {
                                tracing::error!("Failed to send response: {}", e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to parse message: {}", e);
                    let state = state.read().await;
                    if let Some(client) = state.clients.get(&client_id) {
                        let _ = client
                            .send(create_error_message(format!("Invalid message: {}", e)))
                            .await;
                    }
                }
            },
            Ok(None) => {
                // Client disconnected
                tracing::info!("Client disconnected: {}", client_id);
                break;
            }
            Err(e) => {
                tracing::error!("Error reading from client: {}", e);
                break;
            }
        }
    }

    // Cleanup client
    {
        let mut state = state.write().await;
        state.session.remove_client(&client_id);
        state.clients.remove(&client_id);
    }

    // Wait for writer task to finish
    writer_handle.abort();

    tracing::info!("Client handler finished: {}", client_id);

    Ok(())
}

/// Process a client message and return optional response
async fn process_message(
    msg: ClientMessage,
    client_id: Uuid,
    state: &Arc<RwLock<ServerState>>,
) -> Option<ServerMessage> {
    match msg {
        ClientMessage::Hello { protocol_version } => {
            if protocol_version != PROTOCOL_VERSION {
                return Some(create_error_message(format!(
                    "Protocol version mismatch: expected {}, got {}",
                    PROTOCOL_VERSION, protocol_version
                )));
            }
            // Already sent welcome, just acknowledge
            Some(ServerMessage::Ack {
                for_command: "Hello".to_string(),
            })
        }

        ClientMessage::CreateChannel {
            name,
            command,
            working_dir,
        } => {
            let mut state_guard = state.write().await;
            let config = crate::channel::ChannelConfig {
                name: name.clone(),
                command,
                working_dir: working_dir.map(std::path::PathBuf::from),
                env: None,
                size: None, // TODO: Get from client
            };
            match state_guard.channel_manager.create_channel(config).await {
                Ok(()) => {
                    let created_event =
                        ServerMessage::Event(ChannelEvent::Created { name: name.clone() });
                    drop(state_guard); // Release write lock before broadcasting
                    broadcast_to_clients(created_event, state).await;
                    Some(ServerMessage::Ack {
                        for_command: "CreateChannel".to_string(),
                    })
                }
                Err(e) => Some(create_error_message(format!(
                    "Failed to create channel: {}",
                    e
                ))),
            }
        }

        ClientMessage::KillChannel { name } => {
            let mut state_guard = state.write().await;
            match state_guard.channel_manager.kill_channel(&name).await {
                Ok(()) => Some(ServerMessage::Ack {
                    for_command: "KillChannel".to_string(),
                }),
                Err(e) => Some(create_error_message(format!(
                    "Failed to kill channel: {}",
                    e
                ))),
            }
        }

        ClientMessage::ListChannels => {
            let state_guard = state.read().await;
            let client = state_guard.clients.get(&client_id).unwrap();
            let infos = state_guard
                .channel_manager
                .list_channels_info()
                .into_iter()
                .map(|info| crate::protocol::ChannelInfo {
                    is_subscribed: client.is_subscribed(&info.name),
                    is_active: info.is_active,
                    name: info.name,
                    running: info.running,
                })
                .collect();
            Some(ServerMessage::ChannelList { channels: infos })
        }

        ClientMessage::GetStatus { channel } => {
            let state_guard = state.read().await;
            let statuses = state_guard
                .channel_manager
                .list_channel_status()
                .into_iter()
                .filter(|status| channel.as_ref().map(|c| &status.name == c).unwrap_or(true))
                .map(|status| crate::protocol::ChannelStatus {
                    name: status.name,
                    pid: status.pid,
                    running: status.running,
                    exit_code: status.exit_code,
                    working_dir: status.working_dir,
                    command: status.command,
                    created_at: 0,
                    output_lines: status.output_lines,
                })
                .collect();
            Some(ServerMessage::Status { channels: statuses })
        }

        ClientMessage::Detach => {
            tracing::info!("Client {} requested detach", client_id);
            // Client will disconnect after receiving ack
            Some(ServerMessage::Ack {
                for_command: "Detach".to_string(),
            })
        }

        ClientMessage::Shutdown => {
            tracing::info!("Client {} requested shutdown", client_id);
            // TODO: Trigger server shutdown
            Some(ServerMessage::Ack {
                for_command: "Shutdown".to_string(),
            })
        }

        ClientMessage::Subscribe { channels } => {
            let mut state_guard = state.write().await;
            let known_channels: HashSet<_> = state_guard
                .channel_manager
                .list_channels()
                .into_iter()
                .collect();

            let mut target_channels = Vec::new();
            if channels.iter().any(|c| c == "*") {
                target_channels.extend(known_channels.iter().cloned());
            } else {
                for channel in channels {
                    if known_channels.contains(&channel) {
                        target_channels.push(channel);
                    } else {
                        tracing::warn!(
                            "Client {} attempted to subscribe to unknown channel '{}'",
                            client_id,
                            channel
                        );
                    }
                }
            }

            if let Some(client) = state_guard.clients.get_mut(&client_id) {
                client.subscribe(&target_channels);
                let subs = client.get_subscriptions();
                Some(ServerMessage::Event(ChannelEvent::SubscriptionChanged {
                    subscribed: subs,
                }))
            } else {
                Some(create_error_message("Client not found".to_string()))
            }
        }

        ClientMessage::Unsubscribe { channels } => {
            let mut state_guard = state.write().await;
            if let Some(client) = state_guard.clients.get_mut(&client_id) {
                client.unsubscribe(&channels);

                let subs = client.get_subscriptions();
                Some(ServerMessage::Event(ChannelEvent::SubscriptionChanged {
                    subscribed: subs,
                }))
            } else {
                Some(create_error_message("Client not found".to_string()))
            }
        }

        // These will be implemented in Phase 2
        ClientMessage::Input { .. }
        | ClientMessage::InputTo { .. }
        | ClientMessage::SwitchChannel { .. }
        | ClientMessage::Resize { .. } => Some(create_error_message(
            "Channel operations not yet implemented".to_string(),
        )),
    }
}

/// Broadcasts a server message to all connected clients.
async fn broadcast_to_clients(msg: ServerMessage, state: &Arc<RwLock<ServerState>>) {
    let state = state.read().await;
    for client in state.clients.values() {
        if let Err(e) = client.send(msg.clone()).await {
            tracing::warn!(
                "Failed to broadcast message to client {}: {}",
                client.id(),
                e
            );
        }
    }
}

/// Handles events coming from the ChannelManager.
async fn handle_channel_event(event: ChannelManagerEvent, state: &Arc<RwLock<ServerState>>) {
    match event {
        ChannelManagerEvent::Output { channel_name, data } => {
            let msg = ServerMessage::Output {
                channel: channel_name.clone(),
                data,
                timestamp: chrono::Utc::now().timestamp_millis(),
            };

            let state_read = state.read().await;
            // TODO: Maintain a subscription index to avoid scanning all clients on every output event.
            for client in state_read.clients.values() {
                if client.is_subscribed(&channel_name) {
                    if let Err(e) = client.send(msg.clone()).await {
                        tracing::warn!("Failed to send output to client {}: {}", client.id(), e);
                    }
                }
            }
        }
        ChannelManagerEvent::StateChanged {
            channel_name,
            state: channel_state,
        } => {
            let mut subscription_updates = Vec::new();
            if matches!(
                channel_state,
                crate::channel::ChannelState::Killed | crate::channel::ChannelState::Exited(_)
            ) {
                let mut state_guard = state.write().await;
                for (client_id, client) in state_guard.clients.iter_mut() {
                    if client.is_subscribed(&channel_name) {
                        client.unsubscribe(std::slice::from_ref(&channel_name));
                        subscription_updates.push((*client_id, client.get_subscriptions()));
                    }
                }
            }

            if !subscription_updates.is_empty() {
                let state_read = state.read().await;
                for (client_id, subs) in subscription_updates {
                    if let Some(client) = state_read.clients.get(&client_id) {
                        if let Err(e) = client
                            .send(ServerMessage::Event(ChannelEvent::SubscriptionChanged {
                                subscribed: subs.clone(),
                            }))
                            .await
                        {
                            tracing::warn!(
                                "Failed to notify client {} of subscription update: {}",
                                client_id,
                                e
                            );
                        }
                    }
                }
            }

            let server_event = match channel_state {
                // We broadcast Created events from the message handler to get an Ack.
                crate::channel::ChannelState::Running => None,
                crate::channel::ChannelState::Exited(code) => Some(ChannelEvent::Exited {
                    name: channel_name,
                    exit_code: code,
                }),
                crate::channel::ChannelState::Killed => {
                    Some(ChannelEvent::Killed { name: channel_name })
                }
                crate::channel::ChannelState::Starting => None,
            };
            if let Some(event) = server_event {
                broadcast_to_clients(ServerMessage::Event(event), state).await;
            }
        }
    }
}
