//! PTY handling - spawn and manage pseudo-terminal processes

use super::{manager::ChannelManagerEvent, ChannelConfig, ChannelState};
use anyhow::Result;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, RwLock},
};
use tokio::{
    sync::{mpsc, Mutex},
    task,
};

// Fields are deliberately kept for future server-side status reporting; suppress dead_code lint until wired.
#[allow(dead_code)]
/// A single PTY channel
pub struct PtyChannel {
    /// Channel name
    name: String,

    /// Current state
    state: Arc<RwLock<ChannelState>>,

    /// Working directory
    working_dir: PathBuf,

    /// Command being run
    command: String,

    /// Process ID (when running)
    pid: Option<u32>,

    /// Master PTY handle (for resize)
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,

    /// Writer to the PTY master
    writer: Arc<Mutex<Box<dyn Write + Send>>>,

    /// Child process killer handle
    killer: Option<Box<dyn ChildKiller + Send + Sync>>,

    /// Output stream receiver
    output_rx: Option<mpsc::Receiver<Vec<u8>>>,
}

impl PtyChannel {
    /// Spawn a new PTY channel
    pub async fn spawn(config: ChannelConfig) -> Result<Self> {
        Self::spawn_with_notifier(config, None).await
    }

    /// Spawn a new PTY channel with an optional event notifier.
    ///
    /// The `event_notifier` is used to send channel events (output data, state changes)
    /// to the `ChannelManager`. When provided, output is sent directly via the notifier
    /// instead of through the `output_rx` receiver to avoid duplicate sends.
    ///
    /// # Arguments
    /// * `config` - Channel configuration (name, command, working directory, etc.)
    /// * `event_notifier` - Optional sender for `ChannelManagerEvent`s
    pub async fn spawn_with_notifier(
        config: ChannelConfig,
        event_notifier: Option<mpsc::Sender<ChannelManagerEvent>>,
    ) -> Result<Self> {
        let working_dir = config
            .working_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

        let command = config
            .command
            .clone()
            .unwrap_or_else(|| std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()));

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(Self::pty_size_from_config(&config))?;

        let mut cmd = CommandBuilder::new(&command);
        cmd.cwd(&working_dir);

        // Set TERM for proper terminal emulation
        cmd.env("TERM", "xterm-256color");

        if let Some(env) = &config.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        let mut child = pair.slave.spawn_command(cmd)?;
        let pid = child.process_id();
        let killer = Some(child.clone_killer());
        let state = Arc::new(RwLock::new(ChannelState::Running));

        // Take reader and writer before wrapping master in Mutex to avoid potential deadlock
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let master = Arc::new(Mutex::new(pair.master));
        let writer = Arc::new(Mutex::new(writer));

        let (output_tx, output_rx) = mpsc::channel(64);
        let output_log_name = config.name.clone();
        let output_event_name = output_log_name.clone();
        let wait_log_name = config.name.clone();
        let wait_event_name = wait_log_name.clone();
        let state_for_wait = Arc::clone(&state);
        let notifier_for_output = event_notifier.clone();

        // Async output reader (runs in blocking thread)
        task::spawn_blocking(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        tracing::debug!("PTY EOF for channel '{}'", output_log_name);
                        break;
                    }
                    Ok(n) => {
                        let chunk = buf[..n].to_vec();

                        // Send via notifier if available, otherwise via output_tx
                        // This avoids duplicate sends when ChannelManager is listening
                        if let Some(notifier) = &notifier_for_output {
                            if notifier
                                .blocking_send(ChannelManagerEvent::Output {
                                    channel_name: output_event_name.clone(),
                                    data: chunk,
                                })
                                .is_err()
                            {
                                tracing::debug!(
                                    "Event notifier closed for channel '{}'",
                                    output_log_name
                                );
                                break;
                            }
                        } else if output_tx.blocking_send(chunk).is_err() {
                            tracing::debug!(
                                "Output channel closed for channel '{}'",
                                output_log_name
                            );
                            break;
                        }
                    }
                    Err(err) => {
                        tracing::warn!("Read error on channel '{}': {}", output_log_name, err);
                        break;
                    }
                }
            }
        });

        // Track child exit without blocking the async runtime
        task::spawn_blocking(move || match child.wait() {
            Ok(status) => {
                let code = Some(status.exit_code() as i32);
                if let Ok(mut guard) = state_for_wait.write() {
                    *guard = ChannelState::Exited(code);
                }
                tracing::info!("Channel '{}' exited with code {:?}", wait_log_name, code);
                if let Some(notifier) = event_notifier {
                    if notifier
                        .blocking_send(ChannelManagerEvent::StateChanged {
                            channel_name: wait_event_name.clone(),
                            state: ChannelState::Exited(code),
                        })
                        .is_err()
                    {
                        tracing::debug!(
                            "Event notifier closed when reporting exit for '{}'",
                            wait_log_name
                        );
                    }
                }
            }
            Err(err) => {
                tracing::warn!("Failed waiting on child '{}': {}", wait_log_name, err);
                if let Ok(mut guard) = state_for_wait.write() {
                    *guard = ChannelState::Exited(None);
                }
                if let Some(notifier) = event_notifier {
                    let _ = notifier.blocking_send(ChannelManagerEvent::StateChanged {
                        channel_name: wait_event_name,
                        state: ChannelState::Exited(None),
                    });
                }
            }
        });

        tracing::info!(
            "Spawned channel '{}' with command '{}' in '{}' (PID: {:?})",
            config.name,
            command,
            working_dir.display(),
            pid
        );

        Ok(Self {
            name: config.name,
            state,
            working_dir,
            command,
            pid,
            master,
            writer,
            killer,
            output_rx: Some(output_rx),
        })
    }

    fn pty_size_from_config(config: &ChannelConfig) -> PtySize {
        if let Some((cols, rows)) = config.size {
            PtySize {
                cols,
                rows,
                pixel_width: 0,
                pixel_height: 0,
            }
        } else {
            PtySize::default()
        }
    }

    /// Write data to the PTY
    pub async fn write(&mut self, data: &[u8]) -> Result<()> {
        if !self.state().is_alive() {
            anyhow::bail!("Channel '{}' is not running", self.name);
        }

        let writer = Arc::clone(&self.writer);
        let data = data.to_vec();

        task::spawn_blocking(move || -> Result<()> {
            let mut guard = writer.blocking_lock();
            guard.write_all(&data)?;
            guard.flush()?;
            Ok(())
        })
        .await??;

        Ok(())
    }

    /// Resize the PTY
    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        let master = Arc::clone(&self.master);
        task::spawn_blocking(move || -> Result<()> {
            let guard = master.blocking_lock();
            guard.resize(PtySize {
                cols,
                rows,
                pixel_width: 0,
                pixel_height: 0,
            })?;
            Ok(())
        })
        .await??;

        tracing::debug!("Resized channel '{}' to {}x{}", self.name, cols, rows);
        Ok(())
    }

    /// Kill the channel process
    pub async fn kill(&mut self) -> Result<()> {
        if let Some(mut killer) = self.killer.take() {
            task::spawn_blocking(move || killer.kill())
                .await?
                .map_err(anyhow::Error::from)?;
        }

        if let Ok(mut guard) = self.state.write() {
            *guard = ChannelState::Killed;
        }
        tracing::info!("Killed channel '{}'", self.name);
        Ok(())
    }

    /// Consume and return the output receiver for this channel.
    ///
    /// Note: When the channel was created with an event notifier, output is sent
    /// via the notifier and this receiver will not receive data.
    pub fn take_output_receiver(&mut self) -> Option<mpsc::Receiver<Vec<u8>>> {
        self.output_rx.take()
    }

    /// Get current state
    pub fn state(&self) -> ChannelState {
        self.state
            .read()
            .map(|s| *s)
            .unwrap_or(ChannelState::Exited(None))
    }

    /// Get process ID
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Get working directory path
    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    /// Get configured command
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Get channel name
    pub fn name(&self) -> &str {
        &self.name
    }
}
