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
