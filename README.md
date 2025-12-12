# nexus

A channel-based terminal manager with a unified prompt interface.

## What is nexus?

Unlike traditional terminal multiplexers (tmux, screen) that split your screen into panes, **nexus** provides a single unified prompt with multiple background channels. Think of it as IRC/Slack for your terminal sessions.

```
┌─────────────────────────────────────────────────────────────┐
│ [#build: ✓ done] [#tests: 47/100] [#server: listening:3000] │  ← Status bar
├─────────────────────────────────────────────────────────────┤
│ #build   │ Build complete in 2.3s                           │
│ #tests   │ PASS src/auth.test.ts                            │
│ #server  │ GET /api/users 200 12ms                          │  ← Unified output
│ #tests   │ PASS src/user.test.ts                            │
│ #build   │ Watching for changes...                          │
├─────────────────────────────────────────────────────────────┤
│ @server > _                                                 │  ← Single prompt
└─────────────────────────────────────────────────────────────┘
```

## Key Concepts

| Concept | Description |
|---------|-------------|
| **Channel** | A named background process/shell session |
| **Prompt** | Single input line, routed to the active channel |
| **Status Bar** | Real-time monitoring of all channel states |
| **Subscribe** | Choose which channels' output you see |
| **Publish** | Send input to the active (or specified) channel |

## Why not tmux?

| tmux | nexus |
|------|-------|
| Visual pane splits | Single unified view |
| Switch windows to see output | All subscribed output interleaves |
| Screen real-estate management | Attention/notification management |
| One shell visible at a time | All channels visible, one receives input |

## Installation

```bash
# From source
git clone https://github.com/yourusername/nexus.git
cd nexus
cargo build --release

# Copy both binaries to your bin directory
cp target/release/nexus ~/.local/bin/
cp target/release/nexus-server ~/.local/bin/

# Or use a single command
cp target/release/{nexus,nexus-server} ~/.local/bin/
```

**Note**: Both `nexus` (client) and `nexus-server` binaries are required. The client spawns the server automatically when you start a session.

## Quick Start

```bash
# Start nexus
nexus

# Create channels
:new build           # Create channel named "build"
:new tests           # Create channel named "tests"
:new server          # Create channel named "server"

# Switch active channel (receives your input)
@build               # Switch to build channel
npm run build        # This runs in the build channel

@server              # Switch to server channel
npm run dev          # This runs in the server channel

# Subscribe/unsubscribe to output
:sub build tests     # See output from build and tests
:unsub tests         # Stop seeing tests output
:sub *               # See all channel output

# Send command to specific channel without switching
@tests: npm test     # Run in tests channel, stay on current

# View channel status
:status              # Show all channels and their states
:status build        # Show detailed status for build channel
```

## Commands

### Prompt Commands

| Command | Description |
|---------|-------------|
| `@<channel>` | Switch active channel |
| `@<channel>: <cmd>` | Run command in channel without switching |
| `:<command>` | Run nexus control command |

### Control Commands

| Command | Description |
|---------|-------------|
| `:new <name> [cmd]` | Create new channel (optionally with command) |
| `:kill <name>` | Terminate channel |
| `:sub <channels...>` | Subscribe to channel output |
| `:unsub <channels...>` | Unsubscribe from channel output |
| `:status [channel]` | Show channel status |
| `:list` | List all channels |
| `:clear` | Clear output buffer |
| `:quit` | Exit nexus |

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+N` | Cycle to next channel |
| `Ctrl+P` | Cycle to previous channel |
| `Ctrl+L` | Clear screen |
| `Ctrl+D` | Send EOF to active channel |
| `Ctrl+C` | Send SIGINT to active channel |
| `Ctrl+\` | Exit nexus |

## Configuration

Configuration file: `~/.config/nexus/config.toml`

```toml
[general]
default_shell = "/bin/zsh"
history_limit = 10000

[appearance]
status_bar_position = "top"  # top | bottom
show_timestamps = true
channel_colors = true

[channels.default]
subscribed = true

[keybindings]
next_channel = "ctrl+n"
prev_channel = "ctrl+p"
```

## Architecture

```
┌─────────────────────────────────────────┐
│              nexus (client)             │
│  ┌─────────┐ ┌─────────┐ ┌───────────┐  │
│  │ Input   │ │ Output  │ │ Status    │  │
│  │ Handler │ │ Renderer│ │ Bar       │  │
│  └────┬────┘ └────▲────┘ └─────▲─────┘  │
│       │           │            │        │
│       └───────────┼────────────┘        │
│                   │                     │
└───────────────────┼─────────────────────┘
                    │ Unix Socket
┌───────────────────┼─────────────────────┐
│           nexus-server                  │
│  ┌────────────────┴───────────────┐     │
│  │        Channel Manager         │     │
│  └──┬─────────┬─────────┬─────────┘     │
│     │         │         │               │
│  ┌──▼──┐   ┌──▼──┐   ┌──▼──┐           │
│  │ PTY │   │ PTY │   │ PTY │  Channels │
│  │#build│  │#test│   │#srv │           │
│  └─────┘   └─────┘   └─────┘           │
└─────────────────────────────────────────┘
```

## Development Status

See [ROADMAP.md](./ROADMAP.md) for development phases and progress.

## License

MIT
