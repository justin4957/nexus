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
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

const MAX_BUFFERED_OUTPUTS: usize = 200;

#[derive(Clone)]
struct BufferedOutput {
    data: Vec<u8>,
    timestamp: i64,
}

/// Server state shared across connections
struct ServerState {
    session: Session,
    clients: HashMap<Uuid, ClientConnection>,
    channel_manager: ChannelManager,
    output_buffers: HashMap<String, VecDeque<BufferedOutput>>,
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
            output_buffers: HashMap::new(),
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

    if !initial_channels.is_empty() {
        send_buffered_output(client_id, &initial_channels, &state).await;
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
                    state_guard
                        .output_buffers
                        .entry(name.clone())
                        .or_insert_with(VecDeque::new);
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

        ClientMessage::GetStatus { channel: _ } => {
            // TODO: Return actual status when channel manager is implemented
            Some(ServerMessage::Status { channels: vec![] })
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
            let target_channels = {
                let state_guard = state.read().await;
                let known_channels: HashSet<_> = state_guard
                    .channel_manager
                    .list_channels()
                    .into_iter()
                    .collect();

                if channels.iter().any(|c| c == "*") {
                    known_channels.into_iter().collect::<Vec<_>>()
                } else {
                    channels
                        .into_iter()
                        .filter(|channel| {
                            if known_channels.contains(channel) {
                                true
                            } else {
                                tracing::warn!(
                                    "Client {} attempted to subscribe to unknown channel '{}'",
                                    client_id,
                                    channel
                                );
                                false
                            }
                        })
                        .collect()
                }
            };

            let response = {
                let mut state_guard = state.write().await;
                if let Some(client) = state_guard.clients.get_mut(&client_id) {
                    let newly_added = client.subscribe(&target_channels);
                    let subs = client.get_subscriptions();
                    drop(state_guard);

                    if !newly_added.is_empty() {
                        send_buffered_output(client_id, &newly_added, state).await;
                    }

                    Some(ServerMessage::Event(ChannelEvent::SubscriptionChanged {
                        subscribed: subs,
                    }))
                } else {
                    Some(create_error_message("Client not found".to_string()))
                }
            };

            response
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

/// Send buffered output for specified channels to the given client.
async fn send_buffered_output(
    client_id: Uuid,
    channels: &[String],
    state: &Arc<RwLock<ServerState>>,
) {
    let buffers: Vec<(String, Vec<BufferedOutput>)> = {
        let state_guard = state.read().await;
        channels
            .iter()
            .filter_map(|channel| {
                state_guard
                    .output_buffers
                    .get(channel)
                    .map(|buf| (channel.clone(), buf.iter().cloned().collect()))
            })
            .collect()
    };

    if buffers.is_empty() {
        return;
    }

    let state_read = state.read().await;
    if let Some(client) = state_read.clients.get(&client_id) {
        for (channel, entries) in buffers {
            for entry in entries {
                if let Err(e) = client
                    .send(ServerMessage::Output {
                        channel: channel.clone(),
                        data: entry.data.clone(),
                        timestamp: entry.timestamp,
                    })
                    .await
                {
                    tracing::warn!(
                        "Failed to send buffered output to client {}: {}",
                        client_id,
                        e
                    );
                }
            }
        }
    }
}

/// Handles events coming from the ChannelManager.
async fn handle_channel_event(event: ChannelManagerEvent, state: &Arc<RwLock<ServerState>>) {
    match event {
        ChannelManagerEvent::Output { channel_name, data } => {
            let timestamp = chrono::Utc::now().timestamp_millis();
            let mut recipients = Vec::new();
            {
                let mut state_guard = state.write().await;
                let buffer = state_guard
                    .output_buffers
                    .entry(channel_name.clone())
                    .or_insert_with(VecDeque::new);
                buffer.push_back(BufferedOutput {
                    data: data.clone(),
                    timestamp,
                });
                while buffer.len() > MAX_BUFFERED_OUTPUTS {
                    buffer.pop_front();
                }

                for (client_id, client) in state_guard.clients.iter() {
                    if client.is_subscribed(&channel_name) {
                        recipients.push(*client_id);
                    }
                }
            }

            // TODO: Maintain a subscription index to avoid scanning all clients on every output event.
            let msg = ServerMessage::Output {
                channel: channel_name.clone(),
                data,
                timestamp,
            };
            let state_read = state.read().await;
            for client_id in recipients {
                if let Some(client) = state_read.clients.get(&client_id) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::ChannelConfig;
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn sends_output_only_to_subscribers() {
        let temp_dir = tempdir().unwrap();
        let (event_tx, _event_rx) = mpsc::channel(8);
        let (client1_tx, mut client1_rx) = mpsc::channel(8);
        let (client2_tx, mut client2_rx) = mpsc::channel(8);

        let mut client1 = ClientConnection::new(client1_tx);
        let client1_id = client1.id();
        client1.subscribe(&["chan".to_string()]);

        let client2 = ClientConnection::new(client2_tx);
        let client2_id = client2.id();

        let state = Arc::new(RwLock::new(ServerState {
            session: Session::new("test".to_string(), temp_dir.path().join("sock")),
            clients: HashMap::from([(client1_id, client1), (client2_id, client2)]),
            channel_manager: ChannelManager::new(event_tx),
            output_buffers: HashMap::new(),
        }));

        handle_channel_event(
            ChannelManagerEvent::Output {
                channel_name: "chan".to_string(),
                data: b"hello".to_vec(),
            },
            &state,
        )
        .await;

        let msg = client1_rx
            .recv()
            .await
            .expect("client1 should receive output");
        match msg {
            ServerMessage::Output { channel, data, .. } => {
                assert_eq!(channel, "chan");
                assert_eq!(data, b"hello");
            }
            other => panic!("unexpected message for subscriber: {:?}", other),
        }

        assert!(
            client2_rx.try_recv().is_err(),
            "unsubscribe client should not receive output"
        );
    }

    #[tokio::test]
    async fn replays_buffer_on_subscribe() {
        let temp_dir = tempdir().unwrap();
        let (event_tx, _event_rx) = mpsc::channel(8);
        let (client_tx, mut client_rx) = mpsc::channel(16);
        let client = ClientConnection::new(client_tx);
        let client_id = client.id();

        let state = Arc::new(RwLock::new(ServerState {
            session: Session::new("test".to_string(), temp_dir.path().join("sock")),
            clients: HashMap::from([(client_id, client)]),
            channel_manager: ChannelManager::new(event_tx),
            output_buffers: HashMap::new(),
        }));

        {
            let mut guard = state.write().await;
            guard
                .channel_manager
                .create_channel(ChannelConfig::new("chan").with_command("/bin/echo"))
                .await
                .unwrap();
            guard
                .output_buffers
                .entry("chan".to_string())
                .or_insert_with(VecDeque::new);
        }

        handle_channel_event(
            ChannelManagerEvent::Output {
                channel_name: "chan".to_string(),
                data: b"missed".to_vec(),
            },
            &state,
        )
        .await;

        // Subscribe after output was produced
        let response = process_message(
            ClientMessage::Subscribe {
                channels: vec!["chan".to_string()],
            },
            client_id,
            &state,
        )
        .await
        .expect("subscribe should return response");

        let output_msg = client_rx.recv().await.expect("buffered output delivered");
        match output_msg {
            ServerMessage::Output { channel, data, .. } => {
                assert_eq!(channel, "chan");
                assert_eq!(data, b"missed");
            }
            other => panic!("unexpected message: {:?}", other),
        }

        match response {
            ServerMessage::Event(ChannelEvent::SubscriptionChanged { subscribed }) => {
                assert_eq!(subscribed, vec!["chan".to_string()]);
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }
}
