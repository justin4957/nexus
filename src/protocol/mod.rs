//! Protocol definitions for client-server communication
//!
//! Uses MessagePack for efficient binary serialization.

mod message;

pub use message::{ChannelEvent, ChannelStatus, ClientMessage, ServerMessage};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Protocol version for compatibility checking
pub const PROTOCOL_VERSION: u32 = 1;

/// Serialize a message to MessagePack bytes
pub fn serialize<T: Serialize>(msg: &T) -> Result<Vec<u8>> {
    Ok(rmp_serde::to_vec(msg)?)
}

/// Deserialize a message from MessagePack bytes
pub fn deserialize<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T> {
    Ok(rmp_serde::from_slice(bytes)?)
}

/// Frame a message with length prefix for streaming
pub fn frame_message(payload: &[u8]) -> Vec<u8> {
    let len = payload.len() as u32;
    let mut framed = Vec::with_capacity(4 + payload.len());
    framed.extend_from_slice(&len.to_be_bytes());
    framed.extend_from_slice(payload);
    framed
}
