//! Configuration management

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub appearance: AppearanceConfig,
    pub keybindings: KeybindingsConfig,
}

/// General settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// Default shell for new channels
    pub default_shell: String,

    /// Maximum lines to keep in scrollback
    pub history_limit: usize,

    /// Socket directory
    pub runtime_dir: Option<PathBuf>,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            default_shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
            history_limit: 10000,
            runtime_dir: None,
        }
    }
}

/// Appearance settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    /// Status bar position
    pub status_bar_position: StatusBarPosition,

    /// Show timestamps in output
    pub show_timestamps: bool,

    /// Color-code channels
    pub channel_colors: bool,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            status_bar_position: StatusBarPosition::Top,
            show_timestamps: false,
            channel_colors: true,
        }
    }
}

/// Status bar position
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StatusBarPosition {
    #[default]
    Top,
    Bottom,
}

/// Keybinding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    pub next_channel: String,
    pub prev_channel: String,
    pub clear_screen: String,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            next_channel: "ctrl+n".to_string(),
            prev_channel: "ctrl+p".to_string(),
            clear_screen: "ctrl+l".to_string(),
        }
    }
}

impl Config {
    /// Load config from file, or return defaults if not found
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Get the config file path
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nexus")
            .join("config.toml")
    }

    /// Get the runtime directory for sockets
    pub fn runtime_dir(&self) -> PathBuf {
        self.general
            .runtime_dir
            .clone()
            .or_else(dirs::runtime_dir)
            .unwrap_or_else(std::env::temp_dir)
            .join("nexus")
    }

    /// Get socket path for a session
    pub fn socket_path(&self, session_name: &str) -> PathBuf {
        self.runtime_dir().join(format!("{}.sock", session_name))
    }
}
