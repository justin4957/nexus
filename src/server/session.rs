//! Session management - tracks session state and metadata

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Unique session identifier
    pub id: Uuid,

    /// Human-readable session name
    pub name: String,

    /// When the session was created
    pub created_at: DateTime<Utc>,

    /// Socket path for this session
    pub socket_path: PathBuf,

    /// Number of connected clients
    pub client_count: usize,

    /// Number of active channels
    pub channel_count: usize,
}

/// Active session state
pub struct Session {
    /// Session metadata
    pub info: SessionInfo,

    /// Connected client IDs
    client_ids: Vec<Uuid>,
}

impl Session {
    /// Create a new session
    pub fn new(name: String, socket_path: PathBuf) -> Self {
        Self {
            info: SessionInfo {
                id: Uuid::new_v4(),
                name,
                created_at: Utc::now(),
                socket_path,
                client_count: 0,
                channel_count: 0,
            },
            client_ids: Vec::new(),
        }
    }

    /// Register a new client connection
    pub fn add_client(&mut self, client_id: Uuid) {
        self.client_ids.push(client_id);
        self.info.client_count = self.client_ids.len();
    }

    /// Remove a client connection
    pub fn remove_client(&mut self, client_id: &Uuid) {
        self.client_ids.retain(|id| id != client_id);
        self.info.client_count = self.client_ids.len();
    }

    /// Get session name
    pub fn name(&self) -> &str {
        &self.info.name
    }

    /// Get session ID
    pub fn id(&self) -> Uuid {
        self.info.id
    }

    /// Check if any clients are connected
    pub fn has_clients(&self) -> bool {
        !self.client_ids.is_empty()
    }

    /// Get connected client IDs
    pub fn client_ids(&self) -> &[Uuid] {
        &self.client_ids
    }
}
