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

# Fuzzing (requires nightly)
cd fuzz && cargo +nightly fuzz run fuzz_input_parser
```

**Pre-commit checklist:** `cargo fmt && cargo clippy --all-targets --all-features && cargo test`

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

**Client-server over Unix domain sockets.** Client connects, sends identification sequence (term type, cwd, env), then sends a command (argc/argv). Server responds with Ready (attach) or Exit. In attached mode, bidirectional InputData/OutputData messages flow between client stdin/stdout and server PTY fds.

**Pane data flow:** PTY fd → raw bytes → `InputParser` (VT100 state machine) → `Screen` operations → `Grid` updated → `render_window()` via `TermWriter` → OutputData message → client stdout.

**Grid cell optimization:** Compact 8-byte cell for ASCII + 256-color (common case), extended cell for non-ASCII/RGB/hyperlinks. Mirrors tmux's `grid_cell_entry`/`grid_extd_entry` split.

**Options hierarchy:** Server → Session → Window → Pane, each level inherits/overrides parent.

## Code Standards (from AGENTS.md)

- **Lints:** `#![deny(clippy::all, clippy::pedantic)]` in all crates. Zero warnings.
- **Unsafe whitelist:** Only in `protocol/codec.rs` (SCM_RIGHTS), `terminal/pty.rs` (FFI), and `core/grid/cell.rs` (if needed). Every `unsafe` block needs a `// SAFETY:` comment.
- **Error handling:** Never `.unwrap()` in library code. Use `thiserror` error enums. Tests/benchmarks may `.unwrap()`.
- **Benchmarks:** Mandatory for hot paths (grid, parsing, rendering, format expansion). Use criterion with real-world data.
- **Commits:** Format `component: description` (e.g., `core/grid: implement scroll_up`). One logical change per commit.
