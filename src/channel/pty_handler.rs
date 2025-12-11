//! PTY handling - spawn and manage pseudo-terminal processes

use super::{ChannelConfig, ChannelState};
use anyhow::Result;
use std::path::PathBuf;

/// A single PTY channel
pub struct PtyChannel {
    /// Channel name
    name: String,

    /// Current state
    state: ChannelState,

    /// Working directory
    working_dir: PathBuf,

    /// Command being run
    command: String,

    /// Process ID (when running)
    pid: Option<u32>,
    // TODO: Add actual PTY handle from portable-pty
    // pty_pair: PtyPair,
    // child: Child,
}

impl PtyChannel {
    /// Spawn a new PTY channel
    pub async fn spawn(config: ChannelConfig) -> Result<Self> {
        let working_dir = config
            .working_dir
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

        let command = config
            .command
            .unwrap_or_else(|| std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()));

        // TODO: Actually spawn PTY using portable-pty
        // let pty_system = native_pty_system();
        // let pair = pty_system.openpty(PtySize { ... })?;
        // let child = pair.slave.spawn_command(CommandBuilder::new(&command))?;

        tracing::info!(
            "Spawning channel '{}' with command '{}' in '{}'",
            config.name,
            command,
            working_dir.display()
        );

        Ok(Self {
            name: config.name,
            state: ChannelState::Running,
            working_dir,
            command,
            pid: None, // TODO: Get from child process
        })
    }

    /// Write data to the PTY
    pub async fn write(&mut self, data: &[u8]) -> Result<()> {
        if !self.state.is_alive() {
            anyhow::bail!("Channel '{}' is not running", self.name);
        }

        // TODO: Write to PTY master
        tracing::debug!("Writing {} bytes to channel '{}'", data.len(), self.name);

        Ok(())
    }

    /// Resize the PTY
    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        // TODO: Resize PTY
        tracing::debug!("Resizing channel '{}' to {}x{}", self.name, cols, rows);
        Ok(())
    }

    /// Kill the channel process
    pub async fn kill(&mut self) -> Result<()> {
        // TODO: Kill child process
        self.state = ChannelState::Killed;
        tracing::info!("Killed channel '{}'", self.name);
        Ok(())
    }

    /// Get current state
    pub fn state(&self) -> ChannelState {
        self.state
    }

    /// Get process ID
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Get channel name
    pub fn name(&self) -> &str {
        &self.name
    }
}
