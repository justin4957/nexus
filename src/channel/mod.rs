//! Channel management - PTY spawning, I/O handling, lifecycle

mod manager;
mod pty_handler;

pub use manager::ChannelManager;
pub use pty_handler::PtyChannel;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for creating a new channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// Channel name (must be unique within session)
    pub name: String,

    /// Command to run (defaults to user's shell)
    pub command: Option<String>,

    /// Working directory (defaults to current dir)
    pub working_dir: Option<PathBuf>,

    /// Environment variables to set
    pub env: Option<Vec<(String, String)>>,

    /// Initial terminal size
    pub size: Option<(u16, u16)>,
}

impl ChannelConfig {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: None,
            working_dir: None,
            env: None,
            size: None,
        }
    }

    pub fn with_command(mut self, cmd: impl Into<String>) -> Self {
        self.command = Some(cmd.into());
        self
    }

    pub fn with_working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }
}

/// Channel state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelState {
    /// Channel is starting up
    Starting,
    /// Channel is running
    Running,
    /// Channel process exited
    Exited(Option<i32>),
    /// Channel was killed
    Killed,
}

impl ChannelState {
    pub fn is_alive(&self) -> bool {
        matches!(self, ChannelState::Starting | ChannelState::Running)
    }
}
