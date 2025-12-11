# Repository Guidelines

## Project Structure & Modules
- `src/main.rs` (`nexus` client) and `src/bin/server.rs` (`nexus-server`) are the entrypoints. Shared logic lives in `src/lib.rs`.
- Core modules: `src/client` (prompt/input/rendering), `src/channel` (PTY spawn + lifecycle), `src/protocol` (MessagePack/JSON types), `src/server` (Unix socket listener, sessions), and `src/config` (TOML loading).
- Tests live in `tests/` for integration coverage; artifacts land in `target/`. Additional notes and diagrams are under `docs/`; roadmap in `ROADMAP.md`.

## Build, Run, and Tooling
- `cargo build` — debug build; use before pushing to catch compile breaks.
- `cargo run --bin nexus` — start the client; `cargo run --bin nexus-server` — start the server.
- `cargo fmt` — format Rust sources; required before opening a PR.
- `cargo clippy --all-targets --all-features` — lints; fix or gate with `#[allow(...)]` plus a short comment.
- `cargo test` — run integration + unit tests; prefer this over per-file commands.
- Release build: `cargo build --release` (outputs to `target/release/`).

## Coding Style & Naming Conventions
- Rust 2021 edition; default `rustfmt` settings. Keep imports grouped (std, external, crate) and avoid unused `pub` exports.
- Use descriptive names for channels and message types (`ChannelManager`, `StatusUpdate`), and prefer small, single-purpose functions.
- Errors: wrap user-facing context with `anyhow::Context`; use `thiserror` for structured errors.
- Tracing: use `tracing::{info, warn, error, debug, trace}`; avoid `println!` outside tests/examples.

## Testing Guidelines
- Tests use `cargo test` with `proptest` for fuzz cases (see `tests/protocol_tests.rs`). Keep property tests deterministic with explicit seeds when debugging.
- Integration tests go in `tests/`; unit tests sit next to modules under `#[cfg(test)]`.
- Name tests after behavior, e.g., `handles_abrupt_disconnect` or `encodes_subscriptions`.
- Aim to cover channel lifecycle, protocol encoding, and server socket interactions when touching those areas.

## Commit & Pull Request Guidelines
- Commits follow Conventional Commit style observed in history (`feat: ...`, `fix: ...`, `chore: ...`); include a succinct scope when helpful, e.g., `feat(server): broadcast status`.
- Keep commits focused and buildable; avoid mixing refactors with behavior changes unless necessary.
- PRs should include: summary of changes, key test command outputs, linked issues/roadmap items, and screenshots or logs if user-facing behavior changes.
- Before submitting: run `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test`; note any expected failures with rationale.

## Configuration & Security Notes
- Default config loads from `~/.config/nexus/config.toml`; add new options behind sensible defaults to avoid breaking existing users.
- Socket paths and PTY handling live in `src/server`; guard against panic paths and prefer graceful shutdowns.
- Avoid logging secrets (env vars, tokens) in channel output or tracing spans.***
