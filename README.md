# nexus

A channel-based terminal manager with a unified prompt interface.

## What is nexus?

Unlike traditional terminal multiplexers (tmux, screen) that split your screen into panes, **nexus** provides a single unified prompt with multiple background channels. Think of it as IRC/Slack for your terminal sessions.

```
┌─────────────────────────────────────────────────────────────┐
│ [#build: ✓] [#tests*] [#server] [#logs]                     │  ← Status bar
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

### Status Bar Indicators

| Indicator | Meaning |
|-----------|---------|
| `[#name]` | Running channel (grey) |
| `[#name]` (green) | Active channel (receives input) |
| `[#name*]` (yellow) | Channel has new unread output |
| `[#name: ✓]` (green) | Process exited successfully (code 0) |
| `[#name: ✗]` (red) | Process exited with error |
| `...` | More channels (truncated to fit terminal) |

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

### 1. Start nexus

```bash
nexus
```

You'll see an empty interface with a prompt:

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│                                                             │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ @none > _                                                   │
└─────────────────────────────────────────────────────────────┘
```

### 2. Create your first channel

```
:new shell
```

This creates a channel named "shell" running your default shell:

```
┌─────────────────────────────────────────────────────────────┐
│ [#shell]                                                    │
├─────────────────────────────────────────────────────────────┤
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ @shell > _                                                  │
└─────────────────────────────────────────────────────────────┘
```

### 3. Run commands

Type any command and it runs in the active channel:

```
ls -la
```

```
┌─────────────────────────────────────────────────────────────┐
│ [#shell]                                                    │
├─────────────────────────────────────────────────────────────┤
│ #shell   │ total 48                                         │
│ #shell   │ drwxr-xr-x  12 user  staff   384 Dec 13 10:00 .  │
│ #shell   │ -rw-r--r--   1 user  staff  1234 Dec 13 09:30 .. │
├─────────────────────────────────────────────────────────────┤
│ @shell > _                                                  │
└─────────────────────────────────────────────────────────────┘
```

### 4. Create more channels

```
:new build cargo watch -x build
:new server cargo run
```

```
┌─────────────────────────────────────────────────────────────┐
│ [#shell] [#build] [#server]                                 │
├─────────────────────────────────────────────────────────────┤
│ #build   │ Compiling myapp v0.1.0                           │
│ #server  │ Listening on 127.0.0.1:3000                      │
│ #build   │ Finished dev [unoptimized] target(s) in 2.34s    │
├─────────────────────────────────────────────────────────────┤
│ @server > _                                                 │
└─────────────────────────────────────────────────────────────┘
```

### 5. Switch between channels

```
@build              # Switch to build channel
@shell              # Switch to shell channel
```

### 6. Send command to specific channel without switching

```
@build: cargo test
```

This runs `cargo test` in the build channel but keeps you on the current channel.

### 7. Manage subscriptions

```
:sub build server   # Subscribe to build and server output
:unsub build        # Unsubscribe from build
:sub *              # Subscribe to all channels
:subs               # Show current subscriptions
```

### 8. View channel status

```
:status             # Show all channel statuses
:list               # List all channels
```

### 9. Clean up

```
:kill build         # Terminate the build channel
:clear              # Clear the screen
:quit               # Exit nexus (or Ctrl+\)
```

## Tutorial: Web Development Workflow

Here's a real-world example using nexus for web development:

### Setup your development environment

```bash
# Start nexus
nexus

# Create channels for different tasks
:new frontend npm run dev          # Frontend dev server
:new backend cargo watch -x run    # Backend server with auto-reload
:new tests cargo watch -x test     # Continuous test runner
:new git                           # Git operations
```

Your status bar now shows all channels:

```
[#frontend] [#backend] [#tests] [#git]
```

### Monitor all output while working

```bash
# Subscribe to see output from all channels
:sub *

# Work in the git channel
@git
git status
git add .
git commit -m "Add feature"
```

Output from all channels interleaves in your view:

```
#frontend │ Compiled successfully!
#backend  │ Listening on 127.0.0.1:8080
#tests    │ Running 42 tests... 42 passed
#git      │ On branch main
#git      │ nothing to commit, working tree clean
```

### Focus on specific channels

```bash
# Only see frontend and backend output
:unsub tests git

# Quick command to another channel without switching
@tests: cargo test auth
```

### Check channel status

```bash
:status
```

```
#frontend running  pid=12345 cwd=/app cmd=npm run dev
#backend  running  pid=12346 cwd=/app cmd=cargo watch -x run
#tests    running  pid=12347 cwd=/app cmd=cargo watch -x test
#git      running  pid=12348 cwd=/app cmd=/bin/zsh
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
