# Nexus Ratatui TUI - Quick Start Guide

**Last Updated:** 2025-12-16

## Current Status

The `feature/ratatui-refactor` branch contains an **in-progress** ratatui implementation with compilation errors that need to be fixed before development can continue.

### What Works
- ✅ Basic project structure with separated concerns
- ✅ Event loop with tokio::select!
- ✅ Server message handling
- ✅ Basic widget usage (Block, Paragraph, List)

### What's Broken
- ❌ Several compilation errors
- ❌ Incomplete `draw_input()` function
- ❌ Missing imports and type mismatches
- ❌ Unreachable code warnings

## Immediate Next Steps (for contributors)

### Step 1: Fix Compilation Errors

The branch needs these fixes to compile:

#### 1. Fix `app.rs` - Add missing Rect import
```rust
// At top of src/client/app.rs
use ratatui::prelude::Rect;
```

#### 2. Fix `ui.rs` - Block borrow issues
Find all instances of:
```rust
f.render_widget(block, area);
let inner_area = block.inner(area);  // ❌ Error: use of moved value
```

Change to:
```rust
f.render_widget(&block, area);
let inner_area = block.inner(area);  // ✅ OK: borrow instead of move
```

#### 3. Fix `mod.rs` - Loop never returns
Around line 796, change:
```rust
    }  // End of tokio::select!
}  // ❌ End of loop - unreachable Ok(())

Ok(())
}
```

To:
```rust
    }  // End of tokio::select!

    // Add break condition
    if should_exit {
        break;
    }
}  // End of loop

Ok(())
}
```

#### 4. Fix `completion.rs` - Match arm types
Around line 65, change:
```rust
match *cmd {
    "kill" => ...,
    "sub" => ...,
```

To:
```rust
match **cmd {  // Double deref
    "kill" => ...,
    "sub" => ...,
```

#### 5. Fix `mod.rs` - Add missing is_subscribed field
Around line 527, change:
```rust
app.channels.push(ChannelInfo {
    name: name.clone(),
    running: true,
    has_new_output: false,
    exit_code: None,
});
```

To:
```rust
app.channels.push(ChannelInfo {
    name: name.clone(),
    running: true,
    has_new_output: false,
    exit_code: None,
    is_subscribed: false,  // ✅ Add this field
});
```

### Step 2: Build and Test

```bash
# From project root
git checkout feature/ratatui-refactor

# Apply the fixes above

# Build
cargo build --release

# Run tests
cargo test

# Try running
pkill -9 nexus-server 2>/dev/null
rm -rf /tmp/nexus/*.sock
./target/release/nexus new test
```

### Step 3: Start Development

Once compilation works, pick a task from the [TUI Modernization Plan](./TUI_MODERNIZATION_PLAN.md):

**Beginner-Friendly Tasks:**
- Implement theme system (Phase 2.5)
- Add line numbers to output (Phase 1.5)
- Create welcome screen improvements (Phase 1.5)
- Add keyboard shortcut hints to status line (Phase 1.3)

**Intermediate Tasks:**
- Implement tab bar with click support (Phase 1.3)
- Add collapsible sidebar (Phase 1.4)
- Create command palette UI (Phase 2.1)
- Implement progress indicators (Phase 2.3)

**Advanced Tasks:**
- Split pane system (Phase 2.2)
- Search and filter (Phase 3.5)
- Session management (Phase 3.2)
- Performance optimization (Phase 3.7)

## Project Structure

```
src/client/
├── mod.rs          # Main event loop, server communication
├── app.rs          # Application state (App struct)
├── ui.rs           # Ratatui rendering (draw functions)
├── commands.rs     # Command handling (:new, :kill, etc.)
├── completion.rs   # Tab completion logic
└── input.rs        # Input parsing
```

### Key Files Explained

#### `app.rs` - Application State
Holds all app state:
- `channels: Vec<ChannelInfo>` - List of channels
- `active_channel: Option<String>` - Currently focused channel
- `channel_buffers: HashMap<String, Vec<BufferedLine>>` - Output per channel
- `scroll_offsets: HashMap<String, usize>` - Scroll position per channel
- `line_editor: LineEditor` - Input line state
- `view_mode: ViewMode` - ActiveChannel or AllChannels
- `notifications: Vec<Notification>` - Transient notifications
- `theme: Theme` - Color scheme (TODO)

Methods:
- `add_output(channel, line)` - Add line to channel buffer
- `get_channel_color(name)` - Assign/retrieve channel color
- `add_notification(msg, duration)` - Show notification

#### `ui.rs` - Rendering
Contains all draw functions:
- `draw(f, app)` - Main entry point, creates layout
- `draw_status_bar(f, app, area)` - Top bar with channel tabs
- `draw_output(f, app, area)` - Output area with scrolling
- `draw_input(f, app, area)` - Bottom input prompt
- `draw_notifications(f, app)` - Notification overlays (TODO)
- `draw_help_popup(f, app)` - Help modal (TODO)

#### `mod.rs` - Event Loop
The core client logic:

```rust
loop {
    // Filter expired notifications
    app.notifications.retain(...)

    // Render UI
    terminal.draw(|f| ui::draw(f, &mut app))?;

    // Handle events
    tokio::select! {
        Some(msg) = server_rx.recv() => {
            // Handle ServerMessage
        }
        Some(event) = input_rx.recv() => {
            // Handle keyboard/mouse/resize
        }
    }
}
```

## Coding Guidelines

### 1. Separation of Concerns
- **app.rs**: State only, no rendering logic
- **ui.rs**: Rendering only, no business logic
- **mod.rs**: Event handling and coordination

### 2. Ratatui Best Practices

**DO:**
```rust
// Borrow blocks, don't move them
f.render_widget(&block, area);
let inner = block.inner(area);

// Use Layout for complex layouts
let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
        Constraint::Length(3),
        Constraint::Min(0),
    ])
    .split(area);

// Style composition
let base_style = Style::default().fg(Color::White);
let bold_style = base_style.add_modifier(Modifier::BOLD);
```

**DON'T:**
```rust
// Don't move widgets
f.render_widget(block, area);  // ❌ block is moved
let inner = block.inner(area);  // ❌ Error!

// Don't hardcode positions
queue!(stdout, MoveTo(10, 5));  // ❌ Breaks on resize

// Don't mix crossterm and ratatui
// Use ratatui's Frame API, not direct crossterm calls in draw functions
```

### 3. State Updates
All state changes should go through the `App` struct:

```rust
// ✅ GOOD
app.add_output("shell".to_string(), "Hello".to_string());

// ❌ BAD
app.channel_buffers.get_mut("shell").unwrap().push(line);
```

### 4. Error Handling
```rust
// Use Result and ? operator
fn draw_complex_widget(f: &mut Frame, app: &App, area: Rect) -> Result<()> {
    // ... drawing code
    Ok(())
}

// Gracefully handle errors
if let Some(channel) = app.channels.get(index) {
    // Use channel
} else {
    // Show error notification
    app.add_notification("Channel not found".to_string(), Duration::from_secs(3));
}
```

## Testing Strategy

### Unit Tests
Test individual components:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_output() {
        let mut app = App::new();
        app.add_output("test".to_string(), "Hello".to_string());

        let buffer = app.channel_buffers.get("test").unwrap();
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer[0].content, "Hello");
    }

    #[test]
    fn test_channel_color_assignment() {
        let mut app = App::new();
        let color1 = app.get_channel_color("shell");
        let color2 = app.get_channel_color("build");

        assert_ne!(color1, color2); // Different channels get different colors
        assert_eq!(app.get_channel_color("shell"), color1); // Consistent assignment
    }
}
```

### Integration Tests
Test full UI rendering:

```rust
#[test]
fn test_full_ui_render() {
    let mut app = App::new();
    app.channels.push(ChannelInfo {
        name: "test".to_string(),
        running: true,
        has_new_output: false,
        exit_code: None,
        is_subscribed: false,
    });
    app.active_channel = Some("test".to_string());

    // Create mock terminal
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    // Render should not panic
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();

    // Verify layout (check backend buffer for expected content)
    let buffer = terminal.backend().buffer();
    assert!(buffer_contains(buffer, "test")); // Channel name appears
}
```

### Manual Testing Checklist

After making changes, manually test:

- [ ] Basic rendering (no panics)
- [ ] Channel switching (Alt+1-9, Ctrl+Left/Right)
- [ ] Scrolling (Page Up/Down, Ctrl+U/B)
- [ ] Input editing (typing, backspace, Ctrl+W, Ctrl+U/K)
- [ ] Command history (Up/Down arrows)
- [ ] Tab completion
- [ ] Terminal resize
- [ ] Mouse scrolling (if enabled)
- [ ] Notifications appear and disappear
- [ ] Help popup (if implemented)
- [ ] Theme switching (if implemented)

## Debugging Tips

### 1. Enable Logging
```rust
// In mod.rs
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();

// Then use throughout code
tracing::debug!("Switching to channel: {}", channel_name);
tracing::error!("Failed to parse input: {:?}", error);
```

View logs:
```bash
RUST_LOG=debug ./target/release/nexus new test 2>nexus.log
# In another terminal:
tail -f nexus.log
```

### 2. Inspect Terminal State
```rust
// In ui.rs, add debug rendering
f.render_widget(
    Paragraph::new(format!("DEBUG: size={:?}, scroll={:?}", f.area(), app.scroll_offsets)),
    Rect::new(0, 0, 50, 1)
);
```

### 3. Use ratatui-inspector
```toml
# Add to Cargo.toml [dev-dependencies]
ratatui-inspector = "0.1"
```

```rust
// Wrap terminal creation
let backend = CrosstermBackend::new(std::io::stdout());
let terminal = Terminal::new(backend)?;
let terminal = ratatui_inspector::wrap(terminal);
```

Access inspector at `http://localhost:8080` to see real-time terminal buffer.

### 4. Common Pitfalls

**Cursor Disappears:**
```rust
// Make sure to set cursor position in draw_input
f.set_cursor_position(Position::new(col, row));
```

**Flickering:**
```rust
// Use double buffering (ratatui does this by default)
// Don't call terminal.draw() multiple times per loop iteration
```

**Layout Broken on Resize:**
```rust
// Listen for Event::Resize and update app state
Event::Resize(cols, rows) => {
    // Ratatui handles this automatically
    // Just ensure constraints are responsive (use Percentage, Ratio, Min, not Length)
}
```

**Colors Wrong:**
```rust
// Don't forget to reset colors after styling
Line::from(vec![
    Span::styled("Error", Style::default().fg(Color::Red)),
    Span::raw(" "),  // ✅ This will be default color
]);
```

## Resources

- **Ratatui Docs**: https://ratatui.rs/
- **Examples**: https://github.com/ratatui/ratatui/tree/main/examples
- **Awesome Ratatui**: https://github.com/ratatui/awesome-ratatui
- **Discord**: https://discord.gg/pMCEU9hNEj (ratatui channel)

## Getting Help

1. **Read the Docs**: Check the [TUI Modernization Plan](./TUI_MODERNIZATION_PLAN.md)
2. **Check Examples**: See ratatui examples for similar widgets
3. **Ask in Issues**: Create a GitHub issue with `[ratatui]` prefix
4. **Discord**: Join the ratatui Discord (link above)

## Contributing

1. **Pick a Task**: Choose from the modernization plan
2. **Create Branch**: `git checkout -b feature/my-feature`
3. **Develop**: Write code, add tests, update docs
4. **Test**: Run tests, manual testing checklist
5. **PR**: Create pull request with screenshots/videos of UI changes
6. **Review**: Address feedback, iterate

---

**Happy Hacking!** 🚀
