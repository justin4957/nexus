//! Command handling for client control commands (prefixed with `:`)

use crate::client::renderer::{ChannelStatusInfo, Renderer};
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

    match command {
        "new" => {
            if args.is_empty() {
                renderer.draw_output_line(&mut stdout, "SYSTEM", "Usage: :new <name> [command]")?;
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
                renderer.draw_output_line(&mut stdout, "SYSTEM", "Usage: :kill <name>")?;
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
            )?;
        }
        "clear" => {
            renderer.clear_output_buffer();
            Renderer::clear(&mut stdout)?;
            renderer.draw_full_ui(
                &mut stdout,
                channels,
                active_channel.as_deref(),
                input_buffer,
            )?;
        }
        "help" | "?" => {
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "nexus - channel-based terminal multiplexer",
            )?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "")?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "Commands:")?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :new <name> [cmd]   Create a new channel (optionally with a command)",
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :kill <name>        Kill a channel",
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :list               List all channels",
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :status [name]      Show channel status",
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :sub <ch> [ch...]   Subscribe to channel output (:sub * for all)",
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :unsub <ch>         Unsubscribe from channel",
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :subs               Show current subscriptions",
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  :clear              Clear the output area",
            )?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "  :quit               Exit nexus")?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "")?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "Channel switching:")?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  #<name>             Switch to channel by name",
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  #<name> <cmd>       Send command to channel without switching",
            )?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "")?;
            renderer.draw_output_line(&mut stdout, "SYSTEM", "Keyboard shortcuts:")?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  Ctrl+C              Cancel current input / send interrupt to channel",
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  Ctrl+D              Send EOF to channel",
            )?;
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                "  Ctrl+\\              Exit nexus immediately",
            )?;
        }
        "quit" | "exit" => return Ok(CommandResult::Exit),
        _ => {
            renderer.draw_output_line(
                &mut stdout,
                "SYSTEM",
                &format!("Unknown command: {}", command),
            )?;
        }
    }

    Ok(CommandResult::Continue)
}
