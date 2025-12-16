//! Tab completion for commands and channel names

/// Available control commands for completion
pub const COMMANDS: &[&str] = &[
    "new",
    "kill",
    "list",
    "status",
    "sub",
    "unsub",
    "subs",
    "clear",
    "view",
    "timestamps",
    "help",
    "quit",
    "exit",
];

use crate::client::app::App;

/// Complete a partial input string
/// Returns a list of possible completions
pub fn complete(input: &str, app: &App) -> Vec<String> {
    let input = input.trim();
    let channel_names: Vec<String> = app.channels.iter().map(|c| c.name.clone()).collect();

    // Command completion: :cmd
    if let Some(partial_cmd) = input.strip_prefix(':') {
        // Check if there's a space (completing an argument)
        if let Some(space_idx) = partial_cmd.find(' ') {
            let cmd = &partial_cmd[..space_idx];
            let arg_partial = partial_cmd[space_idx..].trim();

            // Commands that take channel names as arguments
            if matches!(cmd, "kill" | "sub" | "unsub") {
                return complete_channel_arg(input, arg_partial, app);
            }
            return vec![];
        }

        // Completing the command name itself
        return complete_command(partial_cmd, app);
    }

    // Channel completion: #channel
    if let Some(partial_channel) = input.strip_prefix('#') {
        return complete_channel(partial_channel, &channel_names);
    }

    vec![]
}

/// Complete a command name
fn complete_command(partial: &str, app: &App) -> Vec<String> {
    let partial_lower = partial.to_lowercase();
    COMMANDS
        .iter()
        .filter(|cmd| {
            cmd.starts_with(&partial_lower) &&
            // Context-aware filtering:
            // Only suggest 'kill' for running channels
            // Only suggest 'sub'/'unsub' for relevant channels
            match **cmd {
                "kill" => app.channels.iter().any(|ch| ch.running && app.active_channel.as_deref() != Some(&ch.name)),
                "sub" => app.channels.iter().any(|ch| !ch.is_subscribed),
                "unsub" => app.channels.iter().any(|ch| ch.is_subscribed),
                _ => true, // Other commands always suggested
            }
        })
        .map(|cmd| format!(":{}", cmd))
        .collect()
}

/// Complete a channel name
fn complete_channel(partial: &str, channel_names: &[String]) -> Vec<String> {
    let partial_lower = partial.to_lowercase();
    channel_names
        .iter()
        .filter(|name| name.to_lowercase().starts_with(&partial_lower))
        .map(|name| format!("#{}", name))
        .collect()
}

/// Complete a channel argument for a command
fn complete_channel_arg(
    full_input: &str,
    partial_arg: &str,
    app: &App,
) -> Vec<String> {
    let partial_lower = partial_arg.to_lowercase();
    let prefix = if let Some(space_idx) = full_input.find(' ') {
        &full_input[..=space_idx]
    } else {
        full_input
    };

    let cmd = full_input.strip_prefix(':').and_then(|s| s.split_whitespace().next()).unwrap_or("");

    let filtered_channels: Vec<String> = app.channels.iter()
        .filter(|ch| {
            ch.name.to_lowercase().starts_with(&partial_lower) &&
            match cmd {
                "kill" => ch.running,
                "sub" => !ch.is_subscribed,
                "unsub" => ch.is_subscribed,
                _ => true,
            }
        })
        .map(|ch| ch.name.clone())
        .collect();

    filtered_channels.into_iter()
        .map(|name| format!("{}{}", prefix, name))
        .collect()
}

/// Get the common prefix of all completions
pub fn common_prefix(completions: &[String]) -> Option<String> {
    if completions.is_empty() {
        return None;
    }
    if completions.len() == 1 {
        return Some(completions[0].clone());
    }

    let first = &completions[0];
    let mut prefix_len = first.len();

    for completion in &completions[1..] {
        let common_len = first
            .chars()
            .zip(completion.chars())
            .take_while(|(a, b)| a == b)
            .count();
        prefix_len = prefix_len.min(common_len);
    }

    if prefix_len > 0 {
        Some(first.chars().take(prefix_len).collect())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::app::ChannelInfo;

    fn create_test_app(channel_names: Vec<&str>) -> App {
        let mut app = App::new();
        for name in channel_names {
            app.channels.push(ChannelInfo {
                name: name.to_string(),
                running: true,
                has_new_output: false,
                exit_code: None,
                is_subscribed: false,
            });
        }
        app
    }

    #[test]
    fn test_complete_command() {
        let app = create_test_app(vec![]);
        let completions = complete(":ne", &app);
        assert_eq!(completions, vec![":new"]);
    }

    #[test]
    fn test_complete_command_multiple() {
        let app = create_test_app(vec!["shell"]);
        let completions = complete(":s", &app);
        assert!(completions.contains(&":status".to_string()));
        assert!(completions.contains(&":sub".to_string()));
        assert!(completions.contains(&":subs".to_string()));
    }

    #[test]
    fn test_complete_channel() {
        let app = create_test_app(vec!["shell", "build", "server"]);
        let completions = complete("#sh", &app);
        assert_eq!(completions, vec!["#shell"]);
    }

    #[test]
    fn test_complete_channel_arg() {
        let app = create_test_app(vec!["shell", "build"]);
        let completions = complete(":kill sh", &app);
        assert_eq!(completions, vec![":kill shell"]);
    }

    #[test]
    fn test_common_prefix() {
        let completions = vec![
            ":status".to_string(),
            ":sub".to_string(),
            ":subs".to_string(),
        ];
        assert_eq!(common_prefix(&completions), Some(":s".to_string()));
    }
}
