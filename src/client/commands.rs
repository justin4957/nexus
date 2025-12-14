//! Command handling for client control commands (prefixed with `:`) 

use crate::client::app::{App, ViewMode};
use crate::protocol::ClientMessage;
use anyhow::Result;
use tokio::sync::mpsc::Sender;

pub enum CommandResult {
    Continue,
    Exit,
}

/// Handle a parsed control command and return whether to continue or exit.
#[allow(clippy::too_many_arguments)]
pub async fn handle_control_command(
    command: &str,
    args: Vec<String>,
    app: &mut App,
    msg_tx: &Sender<ClientMessage>,
    _input_buffer: &str,
) -> Result<CommandResult> {
    match command {
        "new" => {
            if args.is_empty() {
                app.add_output("SYSTEM".to_string(), "Usage: :new <name> [command]".to_string());
                return Ok(CommandResult::Continue);
            }
            let name = args[0].clone();
            let command = if args.len() > 1 {
                Some(args[1..].join(" "))
            } else {
                None
            };
            msg_tx
                .send(ClientMessage::CreateChannel {
                    name,
                    command,
                    working_dir: None,
                })
                .await?;
        }
        "kill" => {
            if args.len() != 1 {
                app.add_output("SYSTEM".to_string(), "Usage: :kill <name>".to_string());
                return Ok(CommandResult::Continue);
            }
            msg_tx
                .send(ClientMessage::KillChannel {
                    name: args[0].clone(),
                })
                .await?;
        }
        "list" => {
            msg_tx.send(ClientMessage::ListChannels).await?;
        }
        "status" => {
            let target = args.first().cloned();
            msg_tx
                .send(ClientMessage::GetStatus { channel: target })
                .await?;
        }
        "sub" | "subscribe" => {
            if args.is_empty() {
                app.add_output("SYSTEM".to_string(), "Usage: :sub <channel1> [channel2...] or :sub * for all".to_string());
                app.add_output("SYSTEM".to_string(), format!(
                    "Current subscriptions: {}",
                    if app.subscriptions.is_empty() {
                        "none".to_string()
                    } else {
                        app.subscriptions.join(", ")
                    }
                ));
            } else {
                msg_tx
                    .send(ClientMessage::Subscribe { channels: args })
                    .await?;
            }
        }
        "unsub" | "unsubscribe" => {
            if args.is_empty() {
                app.add_output("SYSTEM".to_string(), "Usage: :unsub <channel1> [channel2...]".to_string());
                app.add_output("SYSTEM".to_string(), format!(
                    "Current subscriptions: {}",
                    if app.subscriptions.is_empty() {
                        "none".to_string()
                    } else {
                        app.subscriptions.join(", ")
                    }
                ));
            } else {
                msg_tx
                    .send(ClientMessage::Unsubscribe { channels: args })
                    .await?;
            }
        }
        "subs" | "subscriptions" => {
            app.add_output("SYSTEM".to_string(), format!(
                "Current subscriptions: {}",
                if app.subscriptions.is_empty() {
                    "none".to_string()
                } else {
                    app.subscriptions.join(", ")
                }
            ));
        }
        "clear" => {
            // Clear buffers
            app.channel_buffers.clear();
            app.interleaved_buffer.clear();
            app.scroll_offsets.clear();
        }
        "view" => {
            // Toggle or set view mode
            if args.is_empty() {
                app.view_mode = match app.view_mode {
                    ViewMode::ActiveChannel => ViewMode::AllChannels,
                    ViewMode::AllChannels => ViewMode::ActiveChannel,
                };
                let mode_name = match app.view_mode {
                    ViewMode::ActiveChannel => "channel (clean output)",
                    ViewMode::AllChannels => "all (interleaved with prefixes)",
                };
                app.add_output("SYSTEM".to_string(), format!("View mode: {}", mode_name));
            } else {
                match args[0].as_str() {
                    "channel" | "active" => app.view_mode = ViewMode::ActiveChannel,
                    "all" | "interleaved" => app.view_mode = ViewMode::AllChannels,
                    _ => {
                        app.add_output("SYSTEM".to_string(), "Usage: :view [channel|all]".to_string());
                        return Ok(CommandResult::Continue);
                    }
                }
            }
        }
        "timestamps" | "ts" => {
            app.show_timestamps = !app.show_timestamps;
            let status = if app.show_timestamps {
                "enabled"
            } else {
                "disabled"
            };
            app.add_output("SYSTEM".to_string(), format!("Timestamps: {}", status));
        }
        "help" | "?" => {
            let help_lines = vec![
                "nexus - channel-based terminal multiplexer",
                "",
                "Commands:",
                "  :new <name> [cmd]   Create a new channel (optionally with a command)",
                "  :kill <name>        Kill a channel",
                "  :list               List all channels",
                "  :status [name]      Show channel status",
                "  :sub <ch> [ch...]   Subscribe to channel output (:sub * for all)",
                "  :unsub <ch>         Unsubscribe from channel",
                "  :subs               Show current subscriptions",
                "  :view [channel|all] Toggle or set view mode",
                "  :clear              Clear the output area",
                "  :timestamps         Toggle timestamp display (:ts)",
                "  :quit               Exit nexus",
                "",
                "Channel switching:",
                "  #<name>             Switch to channel by name",
                "  #<name> <cmd>       Send command to channel without switching",
                "  Alt+1-9             Quick switch to channel by number",
                "  Ctrl+Left/Right     Switch to previous/next channel",
                "",
                "Scrolling:",
                "  Page Up/Down        Scroll output by page",
                "  Ctrl+U/B            Scroll up/down half page",
                "  Home/End            Jump to top/bottom of output",
                "  Tab                 Complete command/channel",
                "",
                "Line editing:",
                "  Left/Right          Move cursor within input",
                "  Home/End            Jump to start/end of input (Ctrl+A/E)",
                "  Up/Down             Navigate command history",
                "  Ctrl+W              Delete word backward",
                "  Ctrl+U/K            Delete to start/end of line",
                "",
                "Keyboard shortcuts:",
                "  Ctrl+C              Cancel current input / send interrupt to channel",
                "  Ctrl+D              Send EOF to channel",
                "  Ctrl+\\              Exit nexus immediately",
                "",
                "Mouse:",
                "  Click channel       Switch to clicked channel in status bar",
                "  Scroll wheel        Scroll output up/down",
            ];
            
            for line in help_lines {
                app.add_output("SYSTEM".to_string(), line.to_string());
            }
        }
        "quit" | "exit" => return Ok(CommandResult::Exit),
        _ => {
            app.add_output("SYSTEM".to_string(), format!("Unknown command: {}", command));
        }
    }

    Ok(CommandResult::Continue)
}
