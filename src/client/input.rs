//! Input handling - parse user input and commands

use anyhow::Result;

/// Parsed user input
#[derive(Debug, Clone)]
pub enum ParsedInput {
    /// Regular input to send to active channel
    Text(String),

    /// Switch active channel: @channelname
    SwitchChannel(String),

    /// Send to specific channel: @channel: command
    SendToChannel { channel: String, command: String },

    /// Control command: :command args
    ControlCommand { command: String, args: Vec<String> },
}

/// Parse a line of user input
pub fn parse_input(line: &str) -> Result<ParsedInput> {
    let line = line.trim();

    // Control command: :command
    if let Some(rest) = line.strip_prefix(':') {
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        let command = parts[0].to_string();
        let args = parts
            .get(1)
            .map(|s| s.split_whitespace().map(String::from).collect())
            .unwrap_or_default();

        return Ok(ParsedInput::ControlCommand { command, args });
    }

    // Channel targeting: @channel or @channel: command
    if let Some(rest) = line.strip_prefix('@') {
        if let Some(colon_idx) = rest.find(':') {
            let channel = rest[..colon_idx].trim().to_string();
            let command = rest[colon_idx + 1..].trim().to_string();
            return Ok(ParsedInput::SendToChannel { channel, command });
        } else {
            // Just @channel means switch to that channel
            let channel = rest.split_whitespace().next().unwrap_or(rest).to_string();
            return Ok(ParsedInput::SwitchChannel(channel));
        }
    }

    // Regular text
    Ok(ParsedInput::Text(line.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_regular_text() {
        let result = parse_input("echo hello").unwrap();
        assert!(matches!(result, ParsedInput::Text(s) if s == "echo hello"));
    }

    #[test]
    fn test_parse_switch_channel() {
        let result = parse_input("@build").unwrap();
        assert!(matches!(result, ParsedInput::SwitchChannel(s) if s == "build"));
    }

    #[test]
    fn test_parse_send_to_channel() {
        let result = parse_input("@build: npm run build").unwrap();
        assert!(matches!(
            result,
            ParsedInput::SendToChannel { channel, command }
            if channel == "build" && command == "npm run build"
        ));
    }

    #[test]
    fn test_parse_control_command() {
        let result = parse_input(":new myserver").unwrap();
        assert!(matches!(
            result,
            ParsedInput::ControlCommand { command, args }
            if command == "new" && args == vec!["myserver"]
        ));
    }

    #[test]
    fn test_parse_control_command_no_args() {
        let result = parse_input(":list").unwrap();
        assert!(matches!(
            result,
            ParsedInput::ControlCommand { command, args }
            if command == "list" && args.is_empty()
        ));
    }
}
