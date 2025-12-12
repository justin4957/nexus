//! Protocol definitions for client-server communication
//!
//! Uses MessagePack for efficient binary serialization.

mod message;

pub use message::{ChannelEvent, ChannelInfo, ChannelStatus, ClientMessage, ServerMessage};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Protocol version for compatibility checking
pub const PROTOCOL_VERSION: u32 = 1;

/// Protocol-specific errors
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Protocol version mismatch: client={client}, server={server}")]
    VersionMismatch { client: u32, server: u32 },

    #[error("Invalid frame: {0}")]
    InvalidFrame(String),

    #[error("Malformed message: {0}")]
    MalformedMessage(String),

    #[error("Message too large: {size} bytes (max: {max})")]
    MessageTooLarge { size: u32, max: u32 },
}

/// Maximum message size to prevent DoS attacks (10 MB)
pub const MAX_MESSAGE_SIZE: u32 = 10 * 1024 * 1024;

/// Serialize a message to MessagePack bytes
pub fn serialize<T: Serialize>(msg: &T) -> Result<Vec<u8>> {
    Ok(rmp_serde::to_vec(msg)?)
}

/// Deserialize a message from MessagePack bytes
pub fn deserialize<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T> {
    rmp_serde::from_slice(bytes).map_err(|e| {
        anyhow!(ProtocolError::MalformedMessage(format!(
            "Failed to deserialize: {}",
            e
        )))
    })
}

/// Frame a message with length prefix for streaming
///
/// Frame format: [4-byte length BE][payload]
pub fn frame_message(payload: &[u8]) -> Vec<u8> {
    let len = payload.len() as u32;
    let mut framed = Vec::with_capacity(4 + payload.len());
    framed.extend_from_slice(&len.to_be_bytes());
    framed.extend_from_slice(payload);
    framed
}

/// Unframe a message from a byte buffer
///
/// Returns (payload, remaining_bytes) on success, or None if not enough data
pub fn unframe_message(buffer: &[u8]) -> Result<Option<(Vec<u8>, &[u8])>> {
    // Need at least 4 bytes for length prefix
    if buffer.len() < 4 {
        return Ok(None);
    }

    // Read length prefix (big-endian u32)
    let length_bytes: [u8; 4] = buffer[0..4]
        .try_into()
        .map_err(|_| anyhow!(ProtocolError::InvalidFrame("Invalid length prefix".into())))?;
    let message_length = u32::from_be_bytes(length_bytes);

    // Check message size limit
    if message_length > MAX_MESSAGE_SIZE {
        bail!(ProtocolError::MessageTooLarge {
            size: message_length,
            max: MAX_MESSAGE_SIZE
        });
    }

    // Check if we have the complete message
    let total_length = 4 + message_length as usize;
    if buffer.len() < total_length {
        return Ok(None);
    }

    // Extract payload and remaining bytes
    let payload = buffer[4..total_length].to_vec();
    let remaining = &buffer[total_length..];

    Ok(Some((payload, remaining)))
}

/// Check if client and server protocol versions are compatible
pub fn check_version_compatibility(client_version: u32, server_version: u32) -> Result<()> {
    if client_version != server_version {
        bail!(ProtocolError::VersionMismatch {
            client: client_version,
            server: server_version
        });
    }
    Ok(())
}

/// Serialize and frame a message in one operation
pub fn serialize_and_frame<T: Serialize>(msg: &T) -> Result<Vec<u8>> {
    let payload = serialize(msg)?;
    Ok(frame_message(&payload))
}

/// Unframe and deserialize a message in one operation
///
/// Note: This requires T to deserialize from owned data (DeserializeOwned)
pub fn unframe_and_deserialize<T>(buffer: &[u8]) -> Result<Option<(T, usize)>>
where
    T: for<'de> Deserialize<'de>,
{
    match unframe_message(buffer)? {
        Some((payload, remaining)) => {
            let msg = deserialize(&payload)?;
            let consumed = buffer.len() - remaining.len();
            Ok(Some((msg, consumed)))
        }
        None => Ok(None),
    }
}
