//! Message types for nexus protocol

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Messages sent from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Handshake with protocol version
    Hello { protocol_version: u32 },

    /// Send input to active channel
    Input { data: Vec<u8> },

    /// Send input to specific channel
    InputTo { channel: String, data: Vec<u8> },

    /// Create a new channel
    CreateChannel {
        name: String,
        command: Option<String>,
        working_dir: Option<String>,
    },

    /// Destroy a channel
    KillChannel { name: String },

    /// Switch active channel
    SwitchChannel { name: String },

    /// Subscribe to channel output
    Subscribe { channels: Vec<String> },

    /// Unsubscribe from channel output
    Unsubscribe { channels: Vec<String> },

    /// Request channel list
    ListChannels,

    /// Request channel status
    GetStatus { channel: Option<String> },

    /// Terminal resize event
    Resize { cols: u16, rows: u16 },

    /// Detach from session (server keeps running)
    Detach,

    /// Graceful shutdown request
    Shutdown,
}

/// Messages sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Handshake response
    Welcome {
        session_id: Uuid,
        protocol_version: u32,
    },

    /// Output from a channel
    Output {
        channel: String,
        data: Vec<u8>,
        timestamp: i64,
    },

    /// Channel event notification
    Event(ChannelEvent),

    /// Channel list response
    ChannelList { channels: Vec<ChannelInfo> },

    /// Status response
    Status { channels: Vec<ChannelStatus> },

    /// Error response
    Error { message: String },

    /// Acknowledgment (for commands that need confirmation)
    Ack { for_command: String },
}

/// Channel lifecycle events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelEvent {
    /// New channel created
    Created { name: String },

    /// Channel exited
    Exited {
        name: String,
        exit_code: Option<i32>,
    },

    /// Channel was killed
    Killed { name: String },

    /// Active channel changed
    ActiveChanged { name: String },

    /// Subscription changed
    SubscriptionChanged { subscribed: Vec<String> },
}

/// Basic channel info for list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub name: String,
    pub running: bool,
    pub is_active: bool,
    pub is_subscribed: bool,
}

/// Detailed channel status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStatus {
    pub name: String,
    pub pid: Option<u32>,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub working_dir: String,
    pub command: String,
    pub created_at: i64,
    pub output_lines: usize,
}
