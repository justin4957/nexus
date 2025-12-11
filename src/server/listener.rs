//! Unix socket listener and server main loop

use super::connection::{
    client_writer_task, create_error_message, create_welcome_message, parse_client_message,
    read_message, ClientConnection,
};
use super::session::Session;
use crate::protocol::{ClientMessage, ServerMessage, PROTOCOL_VERSION};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

/// Server state shared across connections
struct ServerState {
    session: Session,
    clients: HashMap<Uuid, ClientConnection>,
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

        // Initialize server state
        let state = Arc::new(RwLock::new(ServerState {
            session: Session::new(self.session_name.clone(), self.socket_path.clone()),
            clients: HashMap::new(),
        }));

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
    let client = ClientConnection::new(tx);
    let client_id = client.id();

    tracing::info!("Client connected: {}", client_id);

    // Get session ID for welcome message
    let session_id = {
        let mut state = state.write().await;
        state.session.add_client(client_id);
        state.clients.insert(client_id, client);
        state.session.id()
    };

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
    _state: &Arc<RwLock<ServerState>>,
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

        ClientMessage::ListChannels => {
            // TODO: Return actual channel list when channel manager is implemented
            Some(ServerMessage::ChannelList { channels: vec![] })
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

        // These will be implemented in Phase 2
        ClientMessage::Input { .. }
        | ClientMessage::InputTo { .. }
        | ClientMessage::CreateChannel { .. }
        | ClientMessage::KillChannel { .. }
        | ClientMessage::SwitchChannel { .. }
        | ClientMessage::Subscribe { .. }
        | ClientMessage::Unsubscribe { .. }
        | ClientMessage::Resize { .. } => Some(create_error_message(
            "Channel operations not yet implemented".to_string(),
        )),
    }
}
