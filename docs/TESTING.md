# Testing Strategy for nexus

## Overview

Testing a terminal multiplexer requires multiple approaches due to the interaction between:
- PTY management (OS-level)
- Client-server IPC
- Terminal rendering
- User input handling

## Test Categories

### 1. Unit Tests

Located in each module as `#[cfg(test)]` blocks.

**Focus areas:**
- Input parsing (`client/input.rs`)
- Protocol serialization (`protocol/`)
- Configuration parsing (`config/`)
- Channel state transitions

**Run:**
```bash
cargo test
```

### 2. Integration Tests

Located in `tests/` directory.

**Focus areas:**
- Client-server communication
- Channel lifecycle (create, kill, restart)
- Multi-client scenarios
- Session persistence

**Run:**
```bash
cargo test --test '*'
```

### 3. Property-Based Tests

Using `proptest` crate for fuzzing-style tests.

**Focus areas:**
- Protocol encoding/decoding roundtrips
- Input parsing edge cases
- Buffer handling with arbitrary data

**Example:**
```rust
proptest! {
    #[test]
    fn protocol_roundtrip(msg: ClientMessage) {
        let encoded = serialize(&msg)?;
        let decoded: ClientMessage = deserialize(&encoded)?;
        assert_eq!(msg, decoded);
    }
}
```

### 4. Manual Testing

Some scenarios require manual testing:

**Status bar rendering:**
```bash
# Start nexus, create multiple channels
nexus
:new build
:new tests
:new server
# Verify status bar shows all three
```

**Channel switching:**
```bash
@build
echo "in build"
@server
echo "in server"
# Verify output shows correct channel prefixes
```

**Subscription model:**
```bash
:new noisy  # Create channel that produces lots of output
:unsub noisy  # Unsubscribe
@noisy: yes  # Run infinite output
# Verify main view stays clean
:sub noisy  # Subscribe again
# Verify output now appears
```

### 5. Control Command Testing

Test the control commands (prefixed with `:`) implemented in Phase 2.4:

**Channel creation (`:new`):**
```bash
# Start nexus
cargo run -- new test-session

# Create channel with default shell
:new shell
# Expected: Creates channel named "shell" with default shell

# Create channel with specific command
:new build cargo build
# Expected: Creates channel running "cargo build"

# Verify channels exist
:list
# Expected: Shows "shell" and "build" channels
```

**Channel listing (`:list`):**
```bash
:list
# Expected: Shows all channels with their running status
```

**Channel status (`:status`):**
```bash
# Show all channel statuses
:status
# Expected: Shows status for all channels (name, pid, running state, exit code)

# Show specific channel status
:status shell
# Expected: Shows status only for "shell" channel
```

**Subscription management (`:sub`, `:unsub`, `:subs`):**
```bash
# Subscribe to specific channels
:sub shell build
# Expected: "Subscriptions updated: shell, build"

# Check current subscriptions
:subs
# Expected: "Current subscriptions: shell, build"

# Subscribe to all channels
:sub *
# Expected: Subscribes to all existing channels

# Unsubscribe from channels
:unsub shell
# Expected: "Subscriptions updated: build"

# Usage help when no args
:sub
# Expected: Shows usage and current subscriptions
```

**Channel termination (`:kill`):**
```bash
:kill build
# Expected: Terminates "build" channel
:list
# Expected: Shows "build" as stopped or removed
```

**Screen clear (`:clear`):**
```bash
:clear
# Expected: Clears output buffer, redraws status bar and prompt
```

**Exit (`:quit` or `:exit`):**
```bash
:quit
# Expected: Exits nexus client (Ctrl+\ also works)
```

**Error handling:**
```bash
# Invalid command
:invalid
# Expected: "Unknown command: invalid"

# Missing required argument
:new
# Expected: "Usage: :new <name> [command]"

:kill
# Expected: "Usage: :kill <name>"
```

### 6. Status Bar Testing

Test the status bar features implemented in Phase 3.1:

**Status indicators:**
```bash
# Start nexus and create channels
cargo run -- new test-session

# Create a channel and verify status bar shows it
:new shell
# Expected: Status bar shows [#shell] in green (active)

# Create more channels
:new build cargo build
:new tests cargo test
# Expected: Status bar shows [#shell] [#build] [#tests]

# When a process completes successfully
# Expected: Shows [#build: ✓] in dark green

# When a process exits with error
:new fail false
# Expected: Shows [#fail: ✗] in dark red

# When channel has new output (not active)
# Expected: Shows [#channel*] in yellow
```

**Configurable position (top/bottom):**
```bash
# Create config file
mkdir -p ~/.config/nexus
cat > ~/.config/nexus/config.toml << 'EOF'
[appearance]
status_bar_position = "bottom"
EOF

# Start nexus
cargo run -- new test-session
# Expected: Status bar appears at bottom (above prompt)

# Test with top position
cat > ~/.config/nexus/config.toml << 'EOF'
[appearance]
status_bar_position = "top"
EOF
# Expected: Status bar appears at top of terminal
```

**Truncation with many channels:**
```bash
# Create many channels to test truncation
:new ch1
:new ch2
:new ch3
:new ch4
:new ch5
:new ch6
:new ch7
:new ch8
:new ch9
:new ch10

# Expected: Status bar shows as many channels as fit
# followed by "..." indicator when truncated
# Example: [#ch1] [#ch2] [#ch3] [#ch4] ...
```

**Color coding:**
```bash
# Verify color coding
:new active
@active
# Expected: [#active] is green (active channel)

:new background
@active
# Run something in background channel that produces output
@background: echo "hello"
# Expected: [#background*] is yellow (has new output)

# Channel that stopped
:new stopped
@stopped
exit
# Expected: [#stopped: ✓] is dark green (exited 0)
```

## Test Utilities

### Mock PTY

For testing without actual PTY:

```rust
struct MockPty {
    input_buffer: Vec<u8>,
    output_queue: VecDeque<Vec<u8>>,
}

impl MockPty {
    fn queue_output(&mut self, data: &[u8]) {
        self.output_queue.push_back(data.to_vec());
    }
}
```

### Test Server

Lightweight server for integration tests:

```rust
async fn spawn_test_server() -> (PathBuf, JoinHandle<()>) {
    let socket_path = temp_socket_path();
    let handle = tokio::spawn(async move {
        // Run server with test configuration
    });
    (socket_path, handle)
}
```

## CI Configuration

GitHub Actions workflow runs:

1. `cargo fmt --check` - Formatting
2. `cargo clippy -- -D warnings` - Linting
3. `cargo test` - All tests
4. `cargo build --release` - Release build

## Coverage

Generate coverage report:

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
open tarpaulin-report.html
```

## Performance Testing

For high-output scenarios:

```bash
# Generate high-volume output
@test: yes | head -n 100000

# Measure rendering performance
time nexus replay test_session.log
```

## Known Testing Challenges

1. **Terminal state** - Tests may leave terminal in raw mode on failure
   - Solution: Use `std::panic::catch_unwind` with cleanup

2. **Socket cleanup** - Failed tests may leave sockets
   - Solution: Use unique socket paths with `tempfile`

3. **PTY availability** - CI environments may lack PTY
   - Solution: Mock PTY interface, skip PTY tests in CI if needed

4. **Timing** - Async tests may have race conditions
   - Solution: Use proper synchronization, avoid `sleep`-based waits
