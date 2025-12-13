//! Command handling for client control commands (prefixed with `:`)

use crate::client::renderer::{ChannelStatusInfo, Renderer, ViewMode};
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
    renderer: &mut Renderer,
    msg_tx: &Sender<ClientMessage>,
    channels: &[ChannelStatusInfo],
    active_channel: &mut Option<String>,
    subscriptions: &[String],
    input_buffer: &str,
) -> Result<CommandResult> {
    let mut stdout = std::io::stdout();
    let active = active_channel.as_deref();

    match command {
        "new" => {
            if args.is_empty() {
                renderer.draw_output_line(
                    &mut stdout,
                    "SYSTEM",
                    "Usage: :new <name> [command]",
                    active,
                )?;
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
                renderer.draw_output_line(&mut stdout, "SYSTEM", "Usage: :kill <name>", active)?;
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
                renderer.draw_output_line(
                    &mut stdout,
                    "SYSTEM",
                    "Usage: :sub <channel1> [channel2...] or :sub * for all",
                    active,
                )?;
                renderer.draw_output_line(
                    &mut stdout,
                    "SYSTEM",
                    &format!(
                        "Current subscriptions: {}",
                        if subscriptions.is_empty() {
                            "none".to_string()
                        } else {
                            subscriptions.join(", ")
                        }
                    ),
                    active,
                )?;
            } else {
                msg_tx
                    .send(ClientMessage::Subscribe { channels: args })
                    .await?;
            }
        }
        "unsub" | "unsubscribe" => {
            if args.is_empty() {
                renderer.draw_output_line(
                    &mut stdout,
                    "SYSTEM",
                    "Usage: :unsub <channel1> [channel2...]",
                    active,
                )?;
                renderer.draw_output_line(
                    &mut stdout,
                    "SYSTEM",
                    &format!(
                        "Current subscriptions: {}",
                        if subscriptions.is_empty() {
                            "none".to_string()
                        } else {
                            subscriptions.join(", ")
                        }
                    ),
                    active,
                )?;
            } else {
                msg_tx
                    .send(ClientMessage::Unsubscribe { channels: args })
                    .await?;
            }
        }
        "subs" | "subscriptions" => {
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                &format!(
                    "Current subscriptions: {}",
                    if subscriptions.is_empty() {
                        "none".to_string()
                    } else {
                        subscriptions.join(", ")
                    }
                ),
                active,
            )?;
        }
        "clear" => {
            renderer.clear_output_buffer(None);
            Renderer::clear(&mut stdout)?;
            renderer.draw_full_ui(&mut stdout, channels, active, input_buffer)?;
        }
        "view" => {
            // Toggle or set view mode
            if args.is_empty() {
                renderer.toggle_view_mode();
                let mode_name = match renderer.view_mode() {
                    ViewMode::ActiveChannel => "channel (clean output)",
                    ViewMode::AllChannels => "all (interleaved with prefixes)",
                };
                renderer.draw_output_line(
                    &mut stdout,
                    "SYSTEM",
                    &format!("View mode: {}", mode_name),
                    active,
                )?;
            } else {
                match args[0].as_str() {
                    "channel" | "active" => renderer.set_view_mode(ViewMode::ActiveChannel),
                    "all" | "interleaved" => renderer.set_view_mode(ViewMode::AllChannels),
                    _ => {
                        renderer.draw_output_line(
                            &mut stdout,
                            "SYSTEM",
                            "Usage: :view [channel|all]",
                            active,
                        )?;
                        return Ok(CommandResult::Continue);
                    }
                }
            }
            renderer.redraw_output_area(&mut stdout, active)?;
        }
        "help" | "?" => {
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "nexus - channel-based terminal multiplexer",
                active,
            )?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "", active)?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "Commands:", active)?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :new <name> [cmd]   Create a new channel (optionally with a command)",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :kill <name>        Kill a channel",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :list               List all channels",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :status [name]      Show channel status",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :sub <ch> [ch...]   Subscribe to channel output (:sub * for all)",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :unsub <ch>         Unsubscribe from channel",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :subs               Show current subscriptions",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :view [channel|all] Toggle or set view mode",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :clear              Clear the output area",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :quit               Exit nexus",
                active,
            )?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "", active)?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "Channel switching:", active)?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  #<name>             Switch to channel by name",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  #<name> <cmd>       Send command to channel without switching",
                active,
            )?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "", active)?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "Scrolling:", active)?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  Page Up/Down        Scroll output by page",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  Ctrl+U/B            Scroll up/down half page",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  Home/End            Jump to top/bottom of output",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  Tab                 Toggle view mode (channel/all)",
                active,
            )?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "", active)?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "Keyboard shortcuts:", active)?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  Ctrl+C              Cancel current input / send interrupt to channel",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  Ctrl+D              Send EOF to channel",
                active,
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  Ctrl+\\              Exit nexus immediately",
                active,
            )?;
        }
        "quit" | "exit" => return Ok(CommandResult::Exit),
        _ => {
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                &format!("Unknown command: {}", command),
                active,
            )?;
        }
    }

    Ok(CommandResult::Continue)
}
