# Nexus TUI Modernization Plan

**Date:** 2025-12-16
**Author:** Claude Code
**Status:** Draft Proposal

---

## Executive Summary

This document outlines a comprehensive plan to modernize the Nexus TUI using ratatui, enhance developer experience, and create a more semantically rich and visually pleasing interface. The plan is divided into phases with clear milestones and measurable outcomes.

## Table of Contents

1. [Current State Analysis](#current-state-analysis)
2. [Design Principles](#design-principles)
3. [Phase 1: Foundation & Core UX](#phase-1-foundation--core-ux)
4. [Phase 2: Advanced Features](#phase-2-advanced-features)
5. [Phase 3: Polish & Productivity](#phase-3-polish--productivity)
6. [Technical Architecture](#technical-architecture)
7. [Implementation Timeline](#implementation-timeline)

---

## Current State Analysis

### Existing Ratatui Implementation (feature/ratatui-refactor branch)

**Strengths:**
- ✅ Basic ratatui integration started
- ✅ Separation of concerns (mod.rs, ui.rs, app.rs)
- ✅ Event loop with tokio::select!
- ✅ Per-channel buffering and scrolling
- ✅ Color-coded channels
- ✅ Tab completion framework

**Gaps:**
- ❌ Incomplete implementation (compilation errors)
- ❌ Limited widget usage (only Block, Paragraph, List)
- ❌ No interactive widgets (buttons, tabs, forms)
- ❌ Basic layout (simple vertical split)
- ❌ No visual feedback for loading/progress
- ❌ Limited error presentation
- ❌ No themes or customization
- ❌ No mouse interaction beyond basic scrolling

### User Pain Points

From analyzing the codebase and issues:

1. **Discoverability**: Features buried in `:help`, no visual menu
2. **Context Switching**: Hard to track multiple channels simultaneously
3. **Visual Hierarchy**: Everything looks the same (status bar = output = input)
4. **Feedback**: Silent failures, no loading indicators
5. **Navigation**: Keyboard-only, no visual tabs/panels
6. **Information Density**: Underutilized screen real estate
7. **Customization**: No themes, fixed colors
8. **Accessibility**: No screen reader support, poor contrast options

---

## Design Principles

### 1. **Progressive Disclosure**
- Essential info always visible
- Advanced features discoverable but not overwhelming
- Contextual help and hints

### 2. **Visual Hierarchy**
- Clear distinction between sections using borders, colors, spacing
- Important information stands out (errors, warnings, active channel)
- Semantic color usage (red = error, yellow = warning, green = success)

### 3. **Developer-First UX**
- Fast keyboard navigation
- Smart defaults that "just work"
- Powerful CLI for automation
- Extensible through config and hooks

### 4. **Responsive & Adaptive**
- Works on 80x24 terminals and 4K displays
- Graceful degradation on small screens
- Adaptive layouts based on content

### 5. **Delightful Interactions**
- Smooth transitions
- Visual feedback for all actions
- Helpful error messages with suggestions
- Undo/redo for critical actions

---

## Phase 1: Foundation & Core UX

**Goal:** Fix compilation issues, establish solid ratatui foundation, and deliver immediate UX improvements

**Duration:** 2-3 weeks

### 1.1 Fix Compilation & Core Stability

**Tasks:**
- [x] Fix Rect import in app.rs
- [x] Complete `draw_input()` function
- [x] Fix block borrow issues (use `&block` instead of `block`)
- [x] Add missing `is_subscribed` field to ChannelInfo
- [x] Fix match arm type mismatches in completion.rs
- [x] Remove unreachable code in mod.rs (move Ok(()) inside loop with break conditions)
- [ ] Run full test suite and ensure all tests pass
- [ ] Update CI to test ratatui branch

**Deliverables:**
- ✅ Clean compilation with zero warnings
- ✅ All existing tests pass
- ✅ Basic TUI renders without crashes

### 1.2 Enhanced Layout System

**Current Layout:**
```
┌─────────────────────────────────────┐
│ [channel] [1:#shell] [2:#build]    │  ← Status bar
├─────────────────────────────────────┤
│                                     │
│  Output Area (simple list)          │
│                                     │
│                                     │
├─────────────────────────────────────┤
│ #shell ❯ input_                     │  ← Input
└─────────────────────────────────────┘
```

**Proposed Layout (Enhanced):**
```
┌──────────────────────────────────────────────────────────┐
│ ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓  │
│ ┃ Nexus │ 1:shell ✓ │ 2:build ⚙ │ 3:test ✗ │ :help ┃  │ ← Tab Bar (interactive)
│ ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛  │
├──────────────────────────────────────────────────────────┤
│ ┌──────────────────────────┬───────────────────────────┐ │
│ │ Output                   │ Channels   (Collapsible)  │ │
│ │                          │ ┌─────────────────────┐   │ │
│ │ [12:34:56] Starting...   │ │ • shell   (12 lines)│   │ │
│ │ [12:34:57] Building      │ │ ⚙ build   (45 lines)│   │ │
│ │ [12:34:58] Success!      │ │ ✗ test    (8 lines) │   │ │
│ │                          │ └─────────────────────┘   │ │
│ │                          │                          │ │
│ │                          │ Quick Actions            │ │
│ │ ↑ SCROLLED (45%)         │ ┌─────────────────────┐   │ │
│ └──────────────────────────┘ │ ⏵ New Channel       │   │ │
│                              │ ⏸ Pause All         │   │ │
│                              │ 🗑 Clear Output      │   │ │
│                              └─────────────────────┘   │ │
│                                                        │ │
├──────────────────────────────┴───────────────────────────┤
│ #shell ❯ cargo build_                                   │ ← Input with autocomplete
│ ┌─────────────────────────────────┐                     │
│ │ cargo build                     │ ← Completion popup  │
│ │ cargo check                     │                     │
│ │ cargo test                      │                     │
│ └─────────────────────────────────┘                     │
├──────────────────────────────────────────────────────────┤
│ Press Ctrl+P for command palette | ? for help | Ctrl+\ quit │ ← Status line
└──────────────────────────────────────────────────────────┘
```

**Implementation:**
```rust
use ratatui::layout::{Constraint, Direction, Layout, Flex};

fn create_layout(f: &Frame) -> (Rect, Rect, Rect, Rect, Rect) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // Tab bar
            Constraint::Min(10),        // Content area
            Constraint::Length(3),      // Input
            Constraint::Length(1),      // Status line
        ])
        .split(f.area());

    // Split content area into output and sidebar
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(70), // Output
            Constraint::Percentage(30), // Sidebar (collapsible)
        ])
        .split(main_chunks[1]);

    (
        main_chunks[0], // tab_bar
        content_chunks[0], // output
        content_chunks[1], // sidebar
        main_chunks[2], // input
        main_chunks[3], // status_line
    )
}
```

### 1.3 Interactive Tab Bar

**Features:**
- Click to switch channels
- Visual indicators: ✓ (success), ⚙ (running), ✗ (failed), 💤 (idle)
- Activity badges (new output count)
- Close buttons (×)
- Add channel button (+)

**Implementation:**
```rust
use ratatui::widgets::{Tabs, BorderType};

fn draw_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = app.channels.iter().enumerate().map(|(i, ch)| {
        let icon = match (ch.running, ch.exit_code) {
            (true, _) => "⚙",
            (false, Some(0)) => "✓",
            (false, Some(_)) => "✗",
            (false, None) => "💤",
        };

        let badge = if ch.has_new_output {
            format!(" ({})", ch.unread_count)
        } else {
            String::new()
        };

        let style = if Some(&ch.name) == app.active_channel.as_ref() {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if ch.has_new_output {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        };

        Line::from(vec![
            Span::raw(format!("{}:", i + 1)),
            Span::styled(format!("{} {}{}", icon, ch.name, badge), style),
        ])
    }).collect();

    let tabs = Tabs::new(titles)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .title("Channels"))
        .select(app.active_channel_index())
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .divider(Span::raw(" │ "));

    f.render_widget(tabs, area);
}
```

### 1.4 Sidebar Panel (Collapsible)

**Features:**
- Channel list with status icons
- Line count per channel
- Quick action buttons
- Toggle visibility with F2

**Widgets:**
- `List` for channels
- `Block` with custom borders
- `Paragraph` for action buttons

### 1.5 Enhanced Output Display

**Features:**
- Line numbers (toggle with :linenumbers)
- Syntax highlighting for common patterns:
  - Errors: red background
  - Warnings: yellow text
  - URLs: blue underline
  - File paths: cyan
  - Numbers: green
- Search/filter (Ctrl+F)
- Bookmarks (Ctrl+B)

**Implementation:**
```rust
use ratatui::text::{Line, Span};
use ratatui::style::{Style, Color, Modifier};

fn format_output_line(content: &str, line_num: usize, show_line_nums: bool) -> Line {
    let mut spans = Vec::new();

    // Line number
    if show_line_nums {
        spans.push(Span::styled(
            format!("{:4} │ ", line_num),
            Style::default().fg(Color::DarkGray),
        ));
    }

    // Semantic highlighting
    if content.contains("error") || content.contains("ERROR") {
        spans.push(Span::styled(
            content,
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    } else if content.contains("warning") || content.contains("WARNING") {
        spans.push(Span::styled(
            content,
            Style::default().fg(Color::Yellow),
        ));
    } else if content.contains("http://") || content.contains("https://") {
        // Split and highlight URLs
        // ... URL parsing logic
        spans.push(Span::styled(
            content,
            Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
        ));
    } else {
        spans.push(Span::raw(content));
    }

    Line::from(spans)
}
```

### 1.6 Smart Input Bar

**Features:**
- Real-time validation (red border for invalid commands)
- Inline hints (grayed-out completion)
- Command history with search (Ctrl+R)
- Multi-line support for long commands

**Visual States:**
```
Normal:     #shell ❯ ls -la_
Invalid:    #shell ❯ :invalidcommand_  (red border)
Hint:       #shell ❯ ca_rgo build       (grayed completion)
Multi-line: #shell ❯ echo "line 1
            >       line 2"_
```

---

## Phase 2: Advanced Features

**Goal:** Add productivity-boosting features and advanced interactions

**Duration:** 3-4 weeks

### 2.1 Command Palette (Ctrl+P)

Similar to VS Code's command palette.

**Features:**
- Fuzzy search
- Recent commands
- Grouped by category
- Keyboard shortcuts shown
- Quick channel switcher

**Implementation:**
```rust
use ratatui::widgets::{List, ListItem, Block, Borders};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

struct CommandPalette {
    query: String,
    commands: Vec<Command>,
    filtered: Vec<(usize, Command)>, // (score, command)
    selected: usize,
}

struct Command {
    name: String,
    description: String,
    category: String,
    shortcut: Option<String>,
    action: Box<dyn Fn(&mut App)>,
}

fn draw_command_palette(f: &mut Frame, palette: &CommandPalette, area: Rect) {
    // Center popup
    let popup_area = centered_rect(60, 50, area);

    // Clear background
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title("Command Palette")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(block, popup_area);

    let inner = block.inner(popup_area);

    // Split into search box and results
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
        ])
        .split(inner);

    // Search input
    let search = Paragraph::new(format!("> {}", palette.query))
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(search, chunks[0]);

    // Results list
    let items: Vec<ListItem> = palette.filtered.iter().map(|(_, cmd)| {
        let shortcut = cmd.shortcut.as_ref()
            .map(|s| format!(" [{}]", s))
            .unwrap_or_default();

        ListItem::new(Line::from(vec![
            Span::styled(&cmd.name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw(shortcut),
            Span::raw(" - "),
            Span::styled(&cmd.description, Style::default().fg(Color::DarkGray)),
        ]))
    }).collect();

    let list = List::new(items)
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, chunks[1], &mut ListState::default().with_selected(Some(palette.selected)));
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
```

### 2.2 Split Panes

Allow viewing multiple channels simultaneously.

**Features:**
- Horizontal/vertical splits
- Resize with mouse or keyboard
- Each pane has independent scroll
- Maximize/minimize panes
- Saved layouts

**Implementation:**
```rust
enum PaneLayout {
    Single(String),  // channel_name
    Horizontal(Box<PaneLayout>, Box<PaneLayout>, u16), // left, right, split_ratio
    Vertical(Box<PaneLayout>, Box<PaneLayout>, u16),   // top, bottom, split_ratio
}

impl PaneLayout {
    fn render(&self, f: &mut Frame, app: &App, area: Rect) {
        match self {
            PaneLayout::Single(channel) => {
                draw_channel_output(f, app, channel, area);
            }
            PaneLayout::Horizontal(left, right, ratio) => {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Ratio(*ratio as u32, 100),
                        Constraint::Ratio((100 - ratio) as u32, 100),
                    ])
                    .split(area);

                left.render(f, app, chunks[0]);
                right.render(f, app, chunks[1]);
            }
            PaneLayout::Vertical(top, bottom, ratio) => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Ratio(*ratio as u32, 100),
                        Constraint::Ratio((100 - ratio) as u32, 100),
                    ])
                    .split(area);

                top.render(f, app, chunks[0]);
                bottom.render(f, app, chunks[1]);
            }
        }
    }
}
```

**Commands:**
- `:split` - Split current pane horizontally
- `:vsplit` - Split current pane vertically
- `:close` - Close current pane
- `:only` - Close all other panes
- Ctrl+W then arrow keys - Navigate panes

### 2.3 Progress Indicators & Loading States

**Features:**
- Spinner for running commands
- Progress bars for long operations
- Estimated time remaining
- Cancel button

**Implementation:**
```rust
use ratatui::widgets::{Gauge, LineGauge};

struct ProgressTracker {
    channel: String,
    message: String,
    progress: f64, // 0.0 to 1.0
    spinner_state: usize,
    start_time: Instant,
}

fn draw_progress(f: &mut Frame, tracker: &ProgressTracker, area: Rect) {
    let elapsed = tracker.start_time.elapsed();
    let eta = if tracker.progress > 0.0 {
        let total_time = elapsed.as_secs_f64() / tracker.progress;
        Duration::from_secs_f64(total_time - elapsed.as_secs_f64())
    } else {
        Duration::from_secs(0)
    };

    let spinner_frames = vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner = spinner_frames[tracker.spinner_state % spinner_frames.len()];

    let label = format!(
        "{} {} - ETA: {}s",
        spinner,
        tracker.message,
        eta.as_secs()
    );

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Progress"))
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Black))
        .percent((tracker.progress * 100.0) as u16)
        .label(label);

    f.render_widget(gauge, area);
}
```

### 2.4 Rich Error Display

**Features:**
- Error overlay with context
- Stacktraces with syntax highlighting
- Copy to clipboard button
- Link to documentation
- Suggested fixes

**Implementation:**
```rust
struct ErrorDisplay {
    title: String,
    message: String,
    details: Option<String>,
    suggestions: Vec<String>,
    docs_url: Option<String>,
}

fn draw_error_popup(f: &mut Frame, error: &ErrorDisplay, area: Rect) {
    let popup_area = centered_rect(70, 60, area);
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled("Error", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(Color::Red));
    f.render_widget(&block, popup_area);

    let inner = block.inner(popup_area);

    let mut text = vec![
        Line::from(Span::styled(&error.title, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(&error.message),
    ];

    if let Some(details) = &error.details {
        text.push(Line::from(""));
        text.push(Line::from(Span::styled("Details:", Style::default().add_modifier(Modifier::UNDERLINED))));
        text.push(Line::from(Span::styled(details, Style::default().fg(Color::DarkGray))));
    }

    if !error.suggestions.is_empty() {
        text.push(Line::from(""));
        text.push(Line::from(Span::styled("Suggestions:", Style::default().fg(Color::Yellow).add_modifier(Modifier::UNDERLINED))));
        for (i, suggestion) in error.suggestions.iter().enumerate() {
            text.push(Line::from(format!("  {}. {}", i + 1, suggestion)));
        }
    }

    if let Some(url) = &error.docs_url {
        text.push(Line::from(""));
        text.push(Line::from(vec![
            Span::raw("Documentation: "),
            Span::styled(url, Style::default().fg(Color::Blue).add_modifier(Modifier::UNDERLINED)),
        ]));
    }

    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, inner);
}
```

### 2.5 Themes & Customization

**Built-in Themes:**
- `dark` (default)
- `light`
- `dracula`
- `gruvbox`
- `nord`
- `solarized-dark`
- `solarized-light`
- `monokai`

**Configuration:**
```toml
# ~/.config/nexus/config.toml
[appearance]
theme = "dracula"
line_numbers = true
show_sidebar = true
sidebar_width = 30

[appearance.custom_theme]
background = "#282a36"
foreground = "#f8f8f2"
accent = "#bd93f9"
success = "#50fa7b"
warning = "#f1fa8c"
error = "#ff5555"
```

**Implementation:**
```rust
#[derive(Clone)]
struct Theme {
    background: Color,
    foreground: Color,
    accent: Color,
    success: Color,
    warning: Color,
    error: Color,
    border: Color,
    highlight: Color,
}

impl Theme {
    fn dracula() -> Self {
        Self {
            background: Color::Rgb(40, 42, 54),
            foreground: Color::Rgb(248, 248, 242),
            accent: Color::Rgb(189, 147, 249),
            success: Color::Rgb(80, 250, 123),
            warning: Color::Rgb(241, 250, 140),
            error: Color::Rgb(255, 85, 85),
            border: Color::Rgb(68, 71, 90),
            highlight: Color::Rgb(98, 114, 164),
        }
    }

    // ... other theme constructors
}

fn apply_theme(app: &mut App, theme: Theme) {
    app.theme = theme;
    // All UI elements reference app.theme for colors
}
```

---

## Phase 3: Polish & Productivity

**Goal:** Fine-tune UX, add power-user features, optimize performance

**Duration:** 2-3 weeks

### 3.1 Smart Suggestions & Context-Aware Help

**Features:**
- Inline documentation (hover over commands)
- Common error suggestions
- Workflow tips based on usage patterns
- Onboarding tour for new users

### 3.2 Session Management

**Features:**
- Save/restore sessions
- Named workspace presets
- Auto-save on crash
- Session templates

```toml
# ~/.config/nexus/sessions/web-dev.toml
name = "web-dev"

[[channels]]
name = "server"
command = "npm run dev"
working_dir = "~/projects/myapp"

[[channels]]
name = "tests"
command = "npm test -- --watch"
working_dir = "~/projects/myapp"

[[channels]]
name = "db"
command = "docker-compose up postgres"
working_dir = "~/projects/myapp"

[layout]
type = "horizontal"
left = "server"
right = "tests"
```

Commands:
- `:session save web-dev` - Save current state
- `:session load web-dev` - Restore session
- `:session list` - Show saved sessions
- `nexus --session web-dev` - Start with session

### 3.3 Output Processing & Analysis

**Features:**
- Word wrap vs horizontal scroll (configurable)
- Code block detection and syntax highlighting
- Table rendering for structured output
- JSON/XML pretty-printing
- Diff visualization

### 3.4 Clipboard Integration

**Features:**
- Copy selection (visual mode like vim)
- Copy entire channel output
- Copy last command
- Paste from system clipboard

### 3.5 Search & Filter

**Features:**
- Full-text search across all channels (Ctrl+F)
- Regex support
- Filter by level (errors only, warnings, etc.)
- Save search queries
- Highlight matches

### 3.6 Notifications & Alerts

**Features:**
- Desktop notifications (OS-level)
- Sound alerts (configurable)
- Notification rules (notify on error in "build" channel)
- Do Not Disturb mode

### 3.7 Performance Optimizations

**Targets:**
- Render at 60 FPS
- Handle 100,000+ lines per channel
- Sub-10ms input latency
- Efficient scrolling with huge buffers

**Techniques:**
- Virtual scrolling (only render visible lines)
- Lazy line parsing
- Incremental rendering
- Smart redraw (only changed regions)
- Background thread for ANSI parsing

---

## Technical Architecture

### Widget Component Library

Build reusable components on top of ratatui:

```
nexus::ui::widgets::
├── TabBar
├── CommandPalette
├── Notification
├── ErrorPopup
├── ProgressIndicator
├── ChannelList
├── OutputViewer
├── InputBar
├── StatusLine
├── Sidebar
└── SplitPane
```

Each widget:
- Owns its state
- Exposes event handlers
- Implements Draw trait
- Can be styled with Theme

### State Management

Use a proper state management pattern:

```rust
pub struct AppState {
    // Core state
    pub channels: Vec<Channel>,
    pub active_channel: Option<String>,
    pub layout: PaneLayout,

    // UI state
    pub command_palette: Option<CommandPaletteState>,
    pub error_popup: Option<ErrorDisplay>,
    pub notifications: Vec<Notification>,
    pub theme: Theme,

    // User preferences
    pub config: Config,
    pub key_bindings: KeyBindings,
}

// Reducer pattern for state updates
impl AppState {
    fn reduce(&mut self, action: Action) {
        match action {
            Action::SwitchChannel(name) => {
                self.active_channel = Some(name);
                self.clear_channel_notifications(&name);
            }
            Action::ShowCommandPalette => {
                self.command_palette = Some(CommandPaletteState::new());
            }
            Action::AddOutput { channel, line } => {
                if let Some(ch) = self.channels.iter_mut().find(|c| c.name == channel) {
                    ch.buffer.push(line);
                    if ch.name != self.active_channel {
                        ch.unread_count += 1;
                    }
                }
            }
            // ... more actions
        }
    }
}
```

### Event System

Centralized event handling with priorities:

```rust
enum AppEvent {
    // Input events
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),

    // Server events
    ServerMessage(ServerMessage),

    // Timer events
    Tick,
    NotificationExpired(usize),

    // UI events
    TabClicked(usize),
    PaneResized { pane_id: usize, new_size: Rect },
}

struct EventLoop {
    priority_queue: Vec<AppEvent>, // High-priority events
    normal_queue: Vec<AppEvent>,   // Normal events
}

impl EventLoop {
    async fn process(&mut self, app: &mut AppState) {
        // Process high-priority first (user input)
        while let Some(event) = self.priority_queue.pop() {
            self.handle_event(event, app);
        }

        // Then normal events (server messages)
        while let Some(event) = self.normal_queue.pop() {
            self.handle_event(event, app);
        }
    }
}
```

### Plugin System (Future)

Allow users to extend functionality:

```rust
trait Plugin {
    fn name(&self) -> &str;
    fn on_channel_output(&mut self, channel: &str, line: &str);
    fn on_command(&mut self, cmd: &str) -> Option<String>; // Can intercept commands
    fn widgets(&self) -> Vec<Box<dyn Widget>>; // Can add UI elements
}

// Example plugin
struct NotifyOnErrorPlugin {
    config: PluginConfig,
}

impl Plugin for NotifyOnErrorPlugin {
    fn on_channel_output(&mut self, channel: &str, line: &str) {
        if line.contains("error") || line.contains("ERROR") {
            send_desktop_notification(&format!("Error in {}", channel), line);
        }
    }
}
```

---

## Implementation Timeline

### Phase 1: Foundation (Weeks 1-3)

| Week | Tasks | Deliverables |
|------|-------|--------------|
| 1 | Fix compilation, core stability | ✅ Clean build, tests pass |
| 2 | Enhanced layout, tab bar, sidebar | 🎨 New UI structure |
| 3 | Output formatting, smart input | 🎯 Semantic highlighting, validation |

### Phase 2: Advanced Features (Weeks 4-7)

| Week | Tasks | Deliverables |
|------|-------|--------------|
| 4 | Command palette, fuzzy search | ⌨️ Quick command access |
| 5 | Split panes, multi-channel view | 🖥️ Simultaneous channel viewing |
| 6 | Progress indicators, error display | 🔄 Visual feedback |
| 7 | Themes & customization | 🎨 Multiple theme options |

### Phase 3: Polish (Weeks 8-10)

| Week | Tasks | Deliverables |
|------|-------|--------------|
| 8 | Session management, clipboard | 💾 Save/restore workflows |
| 9 | Search, filter, notifications | 🔍 Advanced output navigation |
| 10 | Performance optimization, docs | ⚡ Buttery smooth UX |

**Total Duration:** ~10-12 weeks

---

## Success Metrics

### Quantitative

- **Render Performance**: 60 FPS on 4K displays
- **Input Latency**: <10ms keystroke to screen update
- **Memory Usage**: <50MB for 10 channels with 100K lines each
- **Startup Time**: <500ms cold start
- **Test Coverage**: >80% line coverage

### Qualitative

- **Discoverability**: New users can find features without reading docs
- **Efficiency**: Common tasks require fewer keystrokes than tmux/screen
- **Visual Appeal**: Modern, polished look comparable to Warp/Hyper terminals
- **Reliability**: Zero crashes in 100 hours of continuous use
- **Accessibility**: Works with screen readers, high contrast modes

---

## Risks & Mitigation

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Ratatui API changes | High | Low | Pin version, follow changelog |
| Performance degradation | High | Medium | Profile regularly, benchmarks |
| Complexity creep | Medium | High | Strict scope for each phase, say no |
| Accessibility issues | Medium | Medium | Test with screen readers, contrast checker |
| Breaking config changes | Low | High | Versioned config, migration tool |

---

## Open Questions

1. **Backwards Compatibility**: Support old renderer? Migration path?
2. **Platform Differences**: How much platform-specific code (Windows vs *nix)?
3. **Telemetry**: Collect anonymous usage stats for feature prioritization?
4. **Cloud Features**: Sync sessions/config across machines?
5. **Mobile**: TUI for SSH sessions on tablets?

---

## Next Steps

1. **Review & Feedback**: Get community input on this plan
2. **Prototype**: Build POC for command palette + enhanced layout
3. **User Testing**: Test with 5-10 power users
4. **Refinement**: Adjust plan based on feedback
5. **Kick-off Phase 1**: Start implementation!

---

## References

- [Ratatui Docs](https://ratatui.rs/)
- [Ratatui Examples](https://github.com/ratatui/ratatui/tree/main/examples)
- [TUI Design Patterns](https://charm.sh/blog/tui-design-patterns/)
- [Terminal Trove](https://terminaltrove.com/) - TUI app showcase

---

**Document Version:** 1.0
**Last Updated:** 2025-12-16
**Status:** Draft - Awaiting Approval
