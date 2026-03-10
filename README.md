# rmux

A drop-in replacement for [tmux](https://github.com/tmux/tmux) written in Rust.

Wire-compatible with tmux's imsg protocol (v8) — real tmux clients can connect to an rmux server and vice versa. CLI flags, key bindings, and config syntax match tmux.

## Install

```bash
cargo install --path crates/rmux-client
cargo install --path crates/rmux-server
```

## Usage

```bash
rmux                        # Start a new session (auto-starts server)
rmux new-session -s work    # Named session
rmux attach                 # Reattach to existing session
rmux ls                     # List sessions
```

Key bindings work like tmux: `Ctrl-b` is the prefix key.

| Keys | Action |
|------|--------|
| `Ctrl-b c` | New window |
| `Ctrl-b n` / `p` | Next / previous window |
| `Ctrl-b 0-9` | Select window by number |
| `Ctrl-b d` | Detach |
| `Ctrl-b "` / `%` | Split pane horizontal / vertical |
| `Ctrl-b o` | Cycle panes |
| `Ctrl-b x` | Kill pane |
| `Ctrl-b :` | Command prompt |
| `Ctrl-b [` | Copy mode |
| `Ctrl-b ]` | Paste buffer |

## Build

```bash
cargo build                                        # Debug build
cargo build --release                              # Release build
cargo test                                         # Run all tests
cargo clippy --all-targets --all-features          # Lint
cargo bench -p rmux-core                           # Benchmarks
make check                                         # Format + lint + test
make e2e                                           # E2E tests (requires tmux)
make fuzz                                          # Fuzz all targets (requires nightly)
```

## Architecture

```
rmux-core       Pure data structures (Grid, Screen, Style, Layout, Options)
rmux-terminal   VT100 parser, PTY operations, escape sequence generation
rmux-protocol   imsg wire protocol, message encode/decode
rmux-server     Async event loop, session/window/pane management
rmux-client     CLI, server auto-start, attached mode I/O
```

Client and server communicate over a Unix domain socket at `$TMPDIR/rmux-$UID/default`.

## Testing

Unit and integration tests run via `cargo test`. Command integration tests use a `MockCommandServer` to verify behavior without I/O. Property-based tests use `proptest` for data structure invariants.

E2E tests (`make e2e`) launch rmux inside a real tmux session and exercise the full stack — pane splitting, copy mode, window management, non-attached client commands, detach/reattach, and more.

Fuzz targets (`make fuzz`) cover all parsers and data ingestion points.

## License

ISC
