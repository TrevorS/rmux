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
make coverage                            # HTML coverage report (requires cargo-llvm-cov)

# Fuzzing (requires nightly) — 8 targets in fuzz/fuzz_targets/
cd fuzz && cargo +nightly fuzz run fuzz_input_parser
```

**Pre-commit checklist:** `make check` (or `cargo fmt && cargo clippy --all-targets --all-features && cargo test`)

**CI:** GitHub Actions runs fmt/clippy/test + coverage on push to `master` and on PRs (`.github/workflows/ci.yml`).

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

**Pane lifecycle** — PTY reader tasks send an empty-vec EOF sentinel when the shell exits. `handle_pane_exit()` cascades: removes pane → removes window if last pane → removes session and detaches clients if last window.

**Render pipeline:** PTY fd → raw bytes → `InputParser` (VT100 state machine) → `Screen` operations → `Grid` updated → `render_window()` via `TermWriter` → OutputData message → client stdout. The renderer receives the full window list (for the status bar) and optional prompt state (for command prompt mode). Status line shows `[session] 0:bash* 1:vim 2:logs` with `*` marking active window.

**Grid cell optimization:** Compact 8-byte cell for ASCII + 256-color (common case), extended cell for non-ASCII/RGB/hyperlinks. Mirrors tmux's `grid_cell_entry`/`grid_extd_entry` split.

**Options hierarchy:** Server → Session → Window → Pane, each level inherits/overrides parent. `Options` is a HashMap with typed getters (`get_string`, `get_number`, `get_flag`).

**Key bindings** — Named tables: `prefix`, `root`, `copy-mode-vi`, `copy-mode-emacs`. Prefix key (Ctrl-b) enters the prefix table for the next keystroke. `KeyBindings::process_input()` returns `KeyAction::SendToPane(bytes)` or `KeyAction::Command(argv)`.

**Copy mode** (`copymode.rs`) — `CopyModeState` state machine with vi and emacs key tables. Supports cursor navigation, word/line/paragraph movement, jump-to-char (f/F/t/T/;/,), incremental search (/ and ?), visual selection (character, line, rectangle), and yank-to-paste-buffer. `dispatch_copy_mode_action()` maps action strings to state transitions, returning `CopyModeAction` (None, Cancel, Copy, Unhandled).

**Screen notifications** — Side-channel events from escape sequences that need server-level handling. `Screen::notifications` (a `VecDeque<Notification>`) captures events like OSC 52 (clipboard), OSC 4 (palette color), OSC 10/11 (fg/bg color). The server drains notifications after processing PTY output via `handle_screen_notification()`.

**Hooks** (`hooks.rs`) — `HookStore` maps hook names to command lists. `fire_hook()` executes registered commands when events occur (after-new-session, after-new-window, after-select-window, etc.). Managed via `set-hook`/`show-hooks` commands.

**Paste buffers** (`paste.rs`) — `PasteBufferStore` maintains a FIFO stack of named buffers. Used by copy mode yank, OSC 52 clipboard, `set-buffer`/`paste-buffer`/`save-buffer`/`load-buffer` commands.

**Config** (`config.rs`) — tmux-compatible config file parser. Handles comments (#), line continuation (\), quoted strings, and nested commands. `source-file` command loads config at runtime.

**Format expansion** (`format.rs`) — `#{variable}` and `#(shell-command)` syntax matching tmux. Used in status line format strings and display commands.

## Important Patterns

**Attached vs non-attached clients.** Command errors must not disconnect attached clients (tmux shows errors in the status line). Only non-attached clients get `ErrorOutput` + `Exit` on command failure.

**Target resolution** (`-t` flag). Targets use tmux syntax: `session:window.pane`. Bare numbers are window indices (use current session), not session names. The `resolve_session`/`resolve_window_idx` helpers in `command/builtins/window.rs` handle this.

**Adding a new command.** Add a handler fn in the appropriate `command/builtins/*.rs` file, register a `CommandEntry` in `command/builtins/mod.rs`, add any needed methods to the `CommandServer` trait in `command/mod.rs`, implement on both `Server` (in `server.rs`) and `MockCommandServer` (in `command/test_helpers.rs`), then add tests in `phase4_tests.rs` or `phase5_tests.rs`.

**Tests** are inline `#[cfg(test)]` modules. Command integration tests live in `command/phase4_tests.rs` and `phase5_tests.rs` using mock helpers from `command/test_helpers.rs`. E2E tests (`scripts/e2e-test.sh`) launch rmux inside a real tmux session via the harness (`scripts/test-harness.sh`) — use `harness_rmux` for non-attached commands and `harness_prefix` for keybind tests.

## Code Standards

- **Lints:** `#![deny(clippy::all, clippy::pedantic)]` in all crates. Zero warnings.
- **Unsafe:** `#![forbid(unsafe_code)]` in all crates except rmux-terminal and rmux-protocol. Permitted only in `protocol/codec.rs` (SCM_RIGHTS), `terminal/pty.rs` (FFI), and `core/grid/cell.rs` (if needed). Every `unsafe` block needs a `// SAFETY:` comment.
- **Error handling:** Never `.unwrap()` or `.expect()` in library code. Use `thiserror` error enums. Server event loop catches errors at command boundaries — never panic. Tests/benchmarks may `.unwrap()`.
- **Testing:** `proptest` for property-based testing of data structures. Integration tests verify behavior against real tmux. Snapshot tests capture screen state after command sequences.
- **Benchmarks:** Mandatory for hot paths (grid, parsing, rendering, format expansion). Use criterion with real-world data. No benchmark-specific code paths. Include correctness assertions.
- **Style:** Functions under 50 lines. Prefer explicit types over `impl Trait` in public APIs.
- **Architecture:** Trait boundaries at module edges for testability. No global mutable state — pass state explicitly.
- **Commits:** Format `component: description` (e.g., `core/grid: implement scroll_up`). One logical change per commit.
