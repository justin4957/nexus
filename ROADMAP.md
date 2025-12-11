# nexus Roadmap

## Overview

Development is organized into four phases, each building on the previous. The goal is to have a working prototype as early as possible, then iterate.

---

## Phase 1: Foundation (MVP)

**Goal:** Single channel working end-to-end

### Milestones

- [ ] **1.1 Server Core**
  - Basic server process that stays alive in background
  - Unix socket listener for client connections
  - Graceful shutdown handling

- [ ] **1.2 PTY Management**
  - Spawn a single PTY with shell
  - Read output from PTY
  - Write input to PTY
  - Handle SIGWINCH (terminal resize)

- [ ] **1.3 Client Core**
  - Connect to server via Unix socket
  - Raw terminal mode (capture all input)
  - Basic output display
  - Clean disconnect handling

- [ ] **1.4 Protocol v1**
  - Define message types (Input, Output, Control)
  - MessagePack serialization
  - Bidirectional communication

### Exit Criteria
```bash
$ nexus-server &
$ nexus
> echo "hello world"
hello world
> exit
```

---

## Phase 2: Multi-Channel

**Goal:** Multiple named channels with switching

### Milestones

- [ ] **2.1 Channel Manager**
  - Create/destroy channels by name
  - Track channel state (running, exited, etc.)
  - Route input to active channel

- [ ] **2.2 Channel Switching**
  - `@channel` syntax to switch active
  - `@channel: cmd` to send without switching
  - Maintain per-channel input history

- [ ] **2.3 Output Multiplexing**
  - Subscription model (which channels to display)
  - Channel-prefixed output lines
  - Color coding per channel

- [ ] **2.4 Control Commands**
  - `:new`, `:kill`, `:list`
  - `:sub`, `:unsub`
  - `:status`

### Exit Criteria
```bash
$ nexus
> :new build
> :new server
> @build: npm run build
> @server: npm run dev
> :sub build server
# See interleaved output from both
```

---

## Phase 3: Polish

**Goal:** Production-quality UX

### Milestones

- [ ] **3.1 Status Bar**
  - Real-time channel status display
  - Notification indicators (new output, errors)
  - Configurable position (top/bottom)

- [ ] **3.2 Configuration**
  - TOML config file parsing
  - Default shell, history limit
  - Keybinding customization
  - Channel-specific settings

- [ ] **3.3 Input Enhancements**
  - Line editing (readline-like)
  - Command history (up/down arrows)
  - Tab completion for channel names

- [ ] **3.4 Output Enhancements**
  - Scrollback buffer with search
  - Timestamps (optional)
  - Unicode support
  - Truecolor passthrough

### Exit Criteria
- Config file works
- Status bar shows live channel states
- Comfortable interactive experience

---

## Phase 4: Advanced

**Goal:** Power user features

### Milestones

- [ ] **4.1 Session Persistence**
  - Detach/reattach to running server
  - Session naming
  - List available sessions

- [ ] **4.2 Channel Templates**
  - Predefined channel configs
  - Auto-start channels on session create
  - Working directory per channel

- [ ] **4.3 Filters & Alerts**
  - Output filtering (grep-like)
  - Alert on pattern match
  - Desktop notifications (optional)

- [ ] **4.4 Scripting**
  - Startup scripts
  - Command aliases
  - Hook system (on channel exit, etc.)

### Exit Criteria
- Can detach and reattach
- Templates work for common workflows
- Alerts notify on important events

---

## Technical Debt & Quality

Ongoing throughout all phases:

- [ ] **Testing**
  - Unit tests for core modules
  - Integration tests for client-server
  - Property-based tests for protocol

- [ ] **Documentation**
  - Inline doc comments
  - Man page
  - TESTING.md with strategies

- [ ] **CI/CD**
  - GitHub Actions for tests
  - Release builds for Linux/macOS
  - Automated changelog

---

## Non-Goals (Explicit Exclusions)

To keep scope focused, these are **not** planned:

- GUI/graphical interface
- GPU-accelerated rendering
- Remote/network access (SSH handles this)
- Plugin system with external languages
- Windows support (initially)
- Mouse support (initially)
- Split pane layouts (use tmux for that)

---

## Success Metrics

1. **Usability:** Can run typical dev workflow (build + test + server) comfortably
2. **Reliability:** No crashes or lost output during normal use
3. **Performance:** <10ms latency for input echo, handles high-output channels
4. **Simplicity:** Core codebase under 5000 lines
