# Nexus Development Guidelines

## Testing Workflow

Before creating any PR, you MUST verify changes by:

1. **Kill stale processes**: `pkill -9 -f nexus-server; rm -rf /var/folders/*/*/T/nexus/*.sock`
2. **Build**: `cargo build --release`
3. **Install**: `cp target/release/{nexus,nexus-server} ~/.local/bin/`
4. **Run and test**: Actually launch `nexus` and verify functionality works
5. **Capture output**: Use `script` or redirect to capture terminal output for analysis

## Common Test Scenarios

```bash
# Basic channel creation and output
nexus
:new shell
ls -la
echo "test output"
:quit

# Multiple channels
nexus
:new shell1
:new shell2
@shell1
ls
@shell2
pwd
:quit
```

## Architecture Notes

- `nexus` is the client binary
- `nexus-server` is the server binary (auto-spawned by client)
- Communication via Unix sockets in `/var/folders/*/*/T/nexus/` (macOS) or `/tmp/nexus/` (Linux)
- PTY channels run shell commands
- Client subscribes to channels to receive output

## Key Files

- `src/client/mod.rs` - Client main loop, message handling
- `src/client/renderer.rs` - Terminal UI rendering
- `src/server/listener.rs` - Server message handling, subscriptions
- `src/channel/pty_handler.rs` - PTY spawn and I/O

## Debugging Tips

- Check server logs: Server outputs to stderr with tracing
- Verify subscriptions: Use `:subs` command
- Check channel status: Use `:status` command
