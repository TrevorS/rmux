# AGENTS.md - rmux Code Quality Standards

## Project Overview

rmux is a high-performance, wire-compatible, drop-in replacement for tmux written in Rust.
Every change must maintain correctness, performance, and code hygiene simultaneously.

## Code Quality Rules

### Mandatory Lints
- All crates: `#![deny(clippy::all, clippy::pedantic)]`
- All crates except rmux-terminal and rmux-protocol: `#![forbid(unsafe_code)]`
- Zero compiler warnings. Treat warnings as errors.

### Unsafe Code Policy
- `unsafe` is permitted ONLY in these modules:
  - `rmux-protocol/src/codec.rs` (socket fd passing via SCM_RIGHTS)
  - `rmux-terminal/src/pty.rs` (PTY operations via forkpty/openpty FFI)
  - `rmux-core/src/grid/cell.rs` (only if zero-copy packed cell access requires it)
- Every `unsafe` block MUST have a `// SAFETY:` comment explaining the invariant.
- Prefer safe alternatives. Only use unsafe when there is no safe way to achieve
  the required performance or functionality.

### Error Handling
- Library code: NEVER use `.unwrap()` or `.expect()`. Use `Result<T, E>` with
  per-crate error enums via `thiserror`.
- Tests and benchmarks: `.unwrap()` is acceptable.
- Server event loop: catch errors at command boundaries; never panic.

### Performance
- Benchmarks are mandatory for all hot-path code (grid operations, input parsing,
  screen rendering, format expansion).
- Use `criterion` with real-world data, not synthetic microbenchmarks.
- Never add benchmark-specific code paths.
- All benchmarks must include correctness assertions.
- Memory usage matters as much as CPU time; measure both.

### Testing
- Every public function needs at least one test.
- Use `proptest` for property-based testing of data structures.
- Integration tests verify behavior against real tmux.
- Snapshot tests capture screen state after command sequences.

### Style
- Run `cargo fmt` before every commit.
- No trailing whitespace, no tabs (4-space indent).
- Document all public APIs with doc comments.
- Keep functions under 50 lines where possible; extract helpers for clarity.
- Prefer explicit types over `impl Trait` in public APIs.

### Architecture
- `rmux-core` has ZERO I/O dependencies. Pure data structures and algorithms.
- `rmux-protocol` is shared between client and server. Minimal dependencies.
- Trait boundaries at module edges for testability.
- No global mutable state. Pass state explicitly.

### Git Hygiene
- Atomic commits: one logical change per commit.
- Commit message format: `component: description` (e.g., `core/grid: implement scroll_up`)
- All commits must pass `cargo build && cargo test && cargo clippy`.

### Wire Compatibility
- The tmux protocol (imsg-based, PROTOCOL_VERSION = 8) must be exactly replicated.
- A real tmux client must connect to rmux server and vice versa.
- Config file syntax must be identical to tmux.
- CLI flags and behavior must match tmux exactly.
