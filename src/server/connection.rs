//! Client connection handling

use crate::protocol::{
    deserialize, frame_message, serialize, ClientMessage, ServerMessage, PROTOCOL_VERSION,
};
use anyhow::{anyhow, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Represents a connected client
pub struct ClientConnection {
    /// Unique client identifier
    id: Uuid,

    /// Channel to send messages to this client
    sender: mpsc::Sender<ServerMessage>,
}

impl ClientConnection {
    /// Create a new client connection
    pub fn new(sender: mpsc::Sender<ServerMessage>) -> Self {
        Self {
            id: Uuid::new_v4(),
            sender,
        }
    }

    /// Get client ID
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Send a message to the client
    pub async fn send(&self, msg: ServerMessage) -> Result<()> {
        self.sender
            .send(msg)
            .await
            .map_err(|_| anyhow!("Failed to send message to client"))
    }
}

/// Read a length-prefixed message from a stream
pub async fn read_message<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<Option<Vec<u8>>> {
    let mut len_bytes = [0u8; 4];

    match reader.read_exact(&mut len_bytes).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }

    let len = u32::from_be_bytes(len_bytes) as usize;

    // Sanity check on message size (max 16MB)
    if len > 16 * 1024 * 1024 {
        return Err(anyhow!("Message too large: {} bytes", len));
    }

    let mut buffer = vec![0u8; len];
    reader.read_exact(&mut buffer).await?;

    Ok(Some(buffer))
}

/// Write a length-prefixed message to a stream
pub async fn write_message<W: AsyncWriteExt + Unpin>(writer: &mut W, payload: &[u8]) -> Result<()> {
    let framed = frame_message(payload);
    writer.write_all(&framed).await?;
    writer.flush().await?;
    Ok(())
}

/// Task to write outgoing messages to the client
pub async fn client_writer_task(
    mut writer: OwnedWriteHalf,
    mut receiver: mpsc::Receiver<ServerMessage>,
) {
    while let Some(msg) = receiver.recv().await {
        match serialize(&msg) {
            Ok(payload) => {
                if let Err(e) = write_message(&mut writer, &payload).await {
                    tracing::error!("Failed to write message to client: {}", e);
                    break;
                }
            }
            Err(e) => {
                tracing::error!("Failed to serialize message: {}", e);
            }
        }
    }

    tracing::debug!("Client writer task finished");
}

/// Parse a client message from bytes
pub fn parse_client_message(bytes: &[u8]) -> Result<ClientMessage> {
    deserialize(bytes)
}

/// Create a welcome message for a new client
pub fn create_welcome_message(session_id: Uuid) -> ServerMessage {
    ServerMessage::Welcome {
        session_id,
        protocol_version: PROTOCOL_VERSION,
    }
}

/// Create an error message
pub fn create_error_message(message: String) -> ServerMessage {
    ServerMessage::Error { message }
}
