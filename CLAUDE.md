# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is rmux?

A high-performance, wire-compatible, drop-in replacement for tmux written in Rust. A real tmux client must be able to connect to rmux server and vice versa. CLI flags, config syntax, and the imsg wire protocol (v8) must match tmux exactly.

## Build & Validate

```bash
cargo build                              # Build all crates
cargo test                               # Run all tests
cargo test -p rmux-core                  # Test a single crate
cargo test -p rmux-server -- test_name   # Run a single test
cargo fmt --check                        # Check formatting
cargo clippy --all-targets --all-features  # Lint (zero warnings required)
cargo bench -p rmux-core                 # Run benchmarks for a crate
make check                               # Format + lint + test (pre-commit)
make e2e                                 # Run E2E tests (requires tmux)
make fuzz                                # Run all fuzz targets briefly (requires nightly)
make coverage                            # HTML coverage report (requires cargo-llvm-cov)

# Run a single fuzz target (requires nightly)
cargo +nightly fuzz run fuzz_input_parser   # from project root
```

**Pre-commit checklist:** `make check` (or `cargo fmt && cargo clippy --all-targets --all-features && cargo test`)

**CI:** GitHub Actions runs fmt/clippy/test on push to `master` and on PRs (`.github/workflows/ci.yml`).

## Workspace Crates

```
rmux-core       Pure data structures, no I/O. Grid, Screen, Style, Layout, Options, Key.
rmux-terminal   VT100 parser, PTY ops (forkpty/openpty), escape sequence generation, terminal diffing.
rmux-protocol   imsg-compatible wire protocol. Message encode/decode, fd passing via SCM_RIGHTS.
rmux-server     Tokio single-threaded async event loop. Sessions → Windows → Panes hierarchy.
rmux-client     CLI parsing (tmux-compatible flags), server auto-start, attached mode I/O loop.
```

**Dependency direction:** core ← terminal ← protocol ← server/client (core has zero I/O deps).

## Architecture

**Client-server over Unix domain sockets.** Socket path: `$TMPDIR/rmux-$UID/default` (uses `getuid()`, not PID). Client connects, sends identification sequence (term type, cwd, env), then sends a command (argc/argv). Server responds with Ready (attach) or Exit. In attached mode, bidirectional InputData/OutputData messages flow between client stdin/stdout and server PTY fds.

**Server event loop** (`server.rs`) — single-threaded tokio `select!` over four event sources:
1. New client connections (UnixListener accept)
2. PTY output from panes (mpsc channel, one reader task per pane)
3. Client messages (mpsc channel, one reader task per client)
4. Periodic redraw tick (16ms / ~60fps)

**Command dispatch** — Commands register as `CommandEntry` structs in `command/builtins/mod.rs` with name, handler fn, usage. Handlers receive `(&[String], &mut dyn CommandServer)` and return `CommandResult`. The `CommandServer` trait abstracts all server state mutations so commands are decoupled from the `Server` struct. Command lookup supports exact match and unambiguous prefix matching (tmux-compatible).

**Pane lifecycle** — PTY reader tasks send an empty-vec EOF sentinel when the shell exits. Cascades: removes pane → removes window if last pane → removes session and detaches clients if last window.

**Render pipeline:** PTY fd → raw bytes → `InputParser` (VT100 state machine) → `Screen` operations → `Grid` updated → `render_window()` via `TermWriter` → OutputData message → client stdout. Status line and window list use format expansion (`format.rs`) with configurable templates.

**Grid cell optimization:** Compact 8-byte cell for ASCII + 256-color (common case), extended cell for non-ASCII/RGB/hyperlinks. Mirrors tmux's `grid_cell_entry`/`grid_extd_entry` split. Attrs are split across `attrs` (low byte) and `attrs_hi` (high byte) to preserve all 16 bits in compact storage.

**Options hierarchy:** Server → Session → Window → Pane, each level inherits/overrides parent. `Options` is a HashMap with typed getters (`get_string`, `get_number`, `get_flag`).

**Key bindings** — Named tables: `prefix`, `root`, `copy-mode-vi`, `copy-mode-emacs`. Prefix key (Ctrl-b) enters the prefix table for the next keystroke.

**Copy mode** (`copymode.rs`) — vi and emacs key tables for cursor navigation, selection, search, and yank-to-paste-buffer.

**Screen notifications** — Side-channel events from escape sequences (OSC 52 clipboard, OSC 4/10/11 colors) that need server-level handling. The server drains notifications after processing PTY output.

**Hooks** (`hooks.rs`) — Hook names map to command lists. Events (after-new-session, after-new-window, etc.) trigger registered commands. Managed via `set-hook`/`show-hooks`.

**Paste buffers** (`paste.rs`) — FIFO stack of named buffers used by copy mode, OSC 52, and paste-buffer commands.

**Config** (`config.rs`) — tmux-compatible config file parser. Handles comments, line continuation, quoted strings. `source-file` loads config at runtime.

**Format expansion** (`format.rs`) — `#{variable}` and `#(shell-command)` syntax matching tmux. Supports conditionals, comparisons, and truncation. Used in status line templates and display-message.

**Pane navigation** (`navigate.rs`) — Directional pane selection (up/down/left/right) based on layout geometry, plus next/previous pane cycling.

## Important Patterns

**Attached vs non-attached clients.** Command errors must not disconnect attached clients (tmux shows errors in the status line). Only non-attached clients get `ErrorOutput` + `Exit` on command failure.

**Target resolution** (`-t` flag). Targets use tmux syntax: `session:window.pane`. Bare numbers are window indices (use current session), not session names. See `resolve_session`/`resolve_window_idx` in `command/builtins/window.rs`.

**Adding a new command.** Add a handler fn in the appropriate `command/builtins/*.rs` file, register a `CommandEntry` in `command/builtins/mod.rs`, add any needed methods to the `CommandServer` trait in `command/mod.rs`, implement on both `Server` (in `server.rs`) and `MockCommandServer` (in `command/test_helpers.rs`), then add tests in `phase4_tests.rs`, `phase5_tests.rs`, or `phase6_tests.rs`.

**Tests** are inline `#[cfg(test)]` modules. Command integration tests use `MockCommandServer` from `command/test_helpers.rs`. E2E tests (`make e2e`) launch rmux inside a real tmux session via `scripts/test-harness.sh` — use `harness_rmux` for non-attached commands and `harness_prefix` for keybind tests. Fuzz targets live in `fuzz/fuzz_targets/`.

**Style parsing** — `parse_style()` in `rmux-core` converts tmux-compatible style strings (`fg=red,bg=green,bold`) into `Style` structs. Used by options and rendering.

## Code Standards

- **Lints:** `#![deny(clippy::all, clippy::pedantic)]` in all crates. Zero warnings.
- **Unsafe:** `#![forbid(unsafe_code)]` in rmux-core. Other crates allow unsafe only where required (PTY FFI, SCM_RIGHTS). Every `unsafe` block needs a `// SAFETY:` comment.
- **Error handling:** Never `.unwrap()` or `.expect()` in library code. Use `thiserror` error enums. Server event loop catches errors at command boundaries — never panic. Tests/benchmarks may `.unwrap()`.
- **Testing:** `proptest` for property-based testing. Fuzz targets for all parsers and data ingestion points. Integration tests verify behavior against real tmux.
- **Benchmarks:** Mandatory for hot paths (grid, parsing, rendering, format expansion). Use criterion with real-world data.
- **Style:** Functions under 50 lines. Prefer explicit types over `impl Trait` in public APIs.
- **Architecture:** Trait boundaries at module edges for testability. No global mutable state — pass state explicitly.
- **Commits:** Format `component: description` (e.g., `core/grid: implement scroll_up`). One logical change per commit.
