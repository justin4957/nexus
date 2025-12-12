//! Channel manager - orchestrates multiple channels

use super::{ChannelConfig, ChannelState, PtyChannel};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use tokio::sync::mpsc;

/// Event emitted by channels
#[derive(Debug, Clone)]
pub enum ChannelManagerEvent {
    /// Output received from a channel
    Output { channel_name: String, data: Vec<u8> },
    /// Channel state changed
    StateChanged {
        channel_name: String,
        state: ChannelState,
    },
}

/// Manages all channels in a session
pub struct ChannelManager {
    /// All channels by name
    channels: HashMap<String, PtyChannel>,

    /// Currently active channel (receives input by default)
    active_channel: Option<String>,

    /// Channels the client is subscribed to
    subscribed_channels: Vec<String>,

    /// Event sender for notifying about channel events
    event_sender: mpsc::Sender<ChannelManagerEvent>,
}

impl ChannelManager {
    /// Create a new channel manager
    pub fn new(event_sender: mpsc::Sender<ChannelManagerEvent>) -> Self {
        Self {
            channels: HashMap::new(),
            active_channel: None,
            subscribed_channels: Vec::new(),
            event_sender,
        }
    }

    /// Create a new channel
    pub async fn create_channel(&mut self, config: ChannelConfig) -> Result<()> {
        if self.channels.contains_key(&config.name) {
            return Err(anyhow!("Channel '{}' already exists", config.name));
        }

        let channel_name = config.name.clone();

        // Spawn with notifier - output events go directly to event_sender
        let channel =
            PtyChannel::spawn_with_notifier(config, Some(self.event_sender.clone())).await?;

        // If this is the first channel, make it active and subscribed
        let is_first = self.channels.is_empty();

        self.channels.insert(channel_name.clone(), channel);

        if is_first {
            self.active_channel = Some(channel_name.clone());
            self.subscribed_channels.push(channel_name.clone());
        }

        let _ = self
            .event_sender
            .send(ChannelManagerEvent::StateChanged {
                channel_name,
                state: ChannelState::Running,
            })
            .await;

        Ok(())
    }

    /// Kill a channel
    pub async fn kill_channel(&mut self, name: &str) -> Result<()> {
        let channel = self
            .channels
            .get_mut(name)
            .ok_or_else(|| anyhow!("Channel '{}' not found", name))?;

        channel.kill().await?;

        // If this was the active channel, switch to another
        if self.active_channel.as_deref() == Some(name) {
            self.active_channel = self.channels.keys().find(|k| *k != name).cloned();
        }

        // Remove from subscriptions
        self.subscribed_channels.retain(|c| c != name);

        // Send state change event
        let _ = self
            .event_sender
            .send(ChannelManagerEvent::StateChanged {
                channel_name: name.to_string(),
                state: ChannelState::Killed,
            })
            .await;

        Ok(())
    }

    /// Switch active channel
    pub fn switch_active(&mut self, name: &str) -> Result<()> {
        if !self.channels.contains_key(name) {
            return Err(anyhow!("Channel '{}' not found", name));
        }
        self.active_channel = Some(name.to_string());
        Ok(())
    }

    /// Get active channel name
    pub fn active_channel(&self) -> Option<&str> {
        self.active_channel.as_deref()
    }

    /// Send input to active channel
    pub async fn send_input(&mut self, data: &[u8]) -> Result<()> {
        let active_name = self
            .active_channel
            .as_ref()
            .ok_or_else(|| anyhow!("No active channel"))?
            .clone();

        self.send_input_to(&active_name, data).await
    }

    /// Send input to specific channel
    pub async fn send_input_to(&mut self, channel_name: &str, data: &[u8]) -> Result<()> {
        let channel = self
            .channels
            .get_mut(channel_name)
            .ok_or_else(|| anyhow!("Channel '{}' not found", channel_name))?;

        channel.write(data).await
    }

    /// Subscribe to channels
    pub fn subscribe(&mut self, channel_names: &[String]) {
        for name in channel_names {
            if self.channels.contains_key(name) && !self.subscribed_channels.contains(name) {
                self.subscribed_channels.push(name.clone());
            }
        }
    }

    /// Unsubscribe from channels
    pub fn unsubscribe(&mut self, channel_names: &[String]) {
        self.subscribed_channels
            .retain(|c| !channel_names.contains(c));
    }

    /// Check if subscribed to a channel
    pub fn is_subscribed(&self, name: &str) -> bool {
        self.subscribed_channels.iter().any(|c| c == name)
    }

    /// List all channel names
    pub fn list_channels(&self) -> Vec<String> {
        self.channels.keys().cloned().collect()
    }

    /// List detailed info for all channels
    pub fn list_channels_info(&self) -> Vec<crate::protocol::ChannelInfo> {
        self.channels
            .values()
            .map(|c| crate::protocol::ChannelInfo {
                name: c.name().to_string(),
                running: c.state().is_alive(),
                is_active: self.active_channel() == Some(c.name()),
                is_subscribed: self.is_subscribed(c.name()),
            })
            .collect()
    }

    /// Resize all channels
    pub async fn resize_all(&mut self, cols: u16, rows: u16) -> Result<()> {
        for channel in self.channels.values_mut() {
            channel.resize(cols, rows).await?;
        }
        Ok(())
    }
}
