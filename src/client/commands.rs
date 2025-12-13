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
