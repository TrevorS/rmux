# tmux Parity Checklist

Tracks rmux feature completeness relative to tmux 3.4. Updated 2026-03-10.

Legend: ✅ = implemented, 🔧 = partial/stub, ❌ = missing

---

## Commands

### Sessions
- [x] `new-session` — create session with `-d`, `-s`, `-x`, `-y`
- [x] `kill-session` — destroy session by name
- [x] `has-session` — check if session exists
- [x] `list-sessions` / `ls` — list all sessions
- [x] `rename-session` — rename a session

### Clients
- [x] `attach-session` / `attach` — attach to a session
- [x] `detach-client` / `detach` — detach current client
- [x] `switch-client` / `switchc` — switch to another session
- [x] `refresh-client` / `refresh` — force redraw
- [ ] `suspend-client` / `suspendc` — stub (no SIGTSTP sent)
- [x] `list-clients` — list connected clients

### Windows
- [x] `new-window` — create window with `-d`, `-n`
- [x] `kill-window` — destroy window
- [x] `select-window` — switch to window by index
- [x] `next-window` / `next` — go to next window
- [x] `previous-window` / `prev` — go to previous window
- [x] `last-window` — go to last active window
- [x] `rename-window` — rename a window
- [x] `list-windows` — list windows in a session
- [x] `find-window` / `findw` — search for windows by name
- [x] `swap-window` / `swapw` — swap two windows
- [x] `move-window` / `movew` — move window between sessions
- [x] `rotate-window` / `rotatew` — rotate pane positions
- [x] `respawn-window` / `respawnw` — respawn dead window
- [ ] `link-window` / `linkw` — stub (needs shared ownership model)
- [ ] `unlink-window` / `unlinkw` — stub (needs shared ownership model)

### Panes
- [x] `split-window` — split horizontally (`-h`) or vertically (`-v`)
- [x] `select-pane` — select by direction (`-U/-D/-L/-R`) or target
- [x] `kill-pane` — destroy a pane
- [x] `list-panes` — list panes in a window
- [x] `resize-pane` / `resizep` — resize by direction or absolute
- [x] `swap-pane` / `swapp` — swap two panes
- [x] `break-pane` / `breakp` — move pane to its own window
- [x] `join-pane` / `joinp` — move pane into another window
- [x] `move-pane` / `movep` — alias for join-pane
- [x] `last-pane` / `lastp` — select previous active pane
- [x] `capture-pane` / `capturep` — capture pane contents as text
- [x] `respawn-pane` / `respawnp` — respawn dead pane
- [x] `pipe-pane` / `pipep` — pipe PTY output to shell command
- [x] `clear-history` / `clearhist` — clear pane scrollback history

### Layouts
- [x] `select-layout` / `selectl` — set layout (even-horizontal, even-vertical, main-horizontal, main-vertical, tiled)
- [x] `next-layout` / `nextl` — cycle to next layout
- [x] `previous-layout` / `prevl` — cycle to previous layout

### Key Bindings
- [x] `bind-key` / `bind` — add key binding with `-T` table, `-n` root
- [x] `unbind-key` / `unbind` — remove key binding
- [x] `list-keys` — list all key bindings
- [x] `send-keys` — send keys to a pane (`-l` literal)
- [x] `send-prefix` — send prefix key to pane

### Options
- [x] `set-option` / `set` — set option with `-g`, `-w`, `-s` scopes
- [x] `show-options` / `show` — display options
- [x] `set-window-option` / `setw` — set window option
- [x] `show-window-options` / `showw` — display window options

### Copy Mode & Paste
- [x] `copy-mode` — enter copy mode (`-u` for page up)
- [x] `paste-buffer` / `pasteb` — paste from buffer
- [x] `list-buffers` / `lsb` — list paste buffers
- [x] `show-buffer` / `showb` — show buffer contents
- [x] `set-buffer` / `setb` — set buffer contents
- [x] `delete-buffer` / `deleteb` — delete a buffer
- [x] `save-buffer` / `saveb` — save buffer to file
- [x] `load-buffer` / `loadb` — load file into buffer
- [ ] `choose-buffer` — interactive buffer picker (falls back to list-buffers)

### Display & Info
- [x] `display-message` / `display` — show/expand format string
- [x] `list-commands` / `lscm` — list all commands
- [x] `display-panes` / `displayp` — show pane info (text, not overlay)
- [x] `clock-mode` — display ASCII clock
- [ ] `show-messages` / `showmsgs` — stub (returns empty)
- [ ] `show-prompt-history` — stub (returns empty)
- [ ] `clear-prompt-history` — stub (no-op)

### Interactive UI (require client-side overlay rendering)
- [ ] `choose-tree` — interactive session/window tree (falls back to list-sessions)
- [ ] `choose-client` — interactive client picker (falls back to list-clients)
- [ ] `display-menu` / `menu` — popup menu (no-op)
- [ ] `display-popup` / `popup` — popup window (no-op)
- [ ] `customize-mode` — options browser (no-op)

### Server & Config
- [x] `kill-server` — shutdown server
- [x] `start-server` / `start` — no-op (server already running)
- [x] `source-file` / `source` — load config file
- [x] `run-shell` / `run` — execute shell command
- [x] `command-prompt` — enter command prompt mode
- [x] `if-shell` / `if` — conditional command execution
- [x] `confirm-before` / `confirm` — executes directly (no interactive y/n)
- [ ] `wait-for` / `wait` — stub (no channel management)
- [ ] `server-access` — stub (no ACL)
- [ ] `lock-server` / `lock` — stub (no-op)
- [ ] `lock-session` / `locks` — stub (no-op)
- [ ] `lock-client` / `lockc` — stub (no-op)
- [ ] `resize-window` / `resizew` — stub (no-op, sizing is automatic)

### Hooks & Environment
- [x] `set-hook` — register hook commands
- [x] `show-hooks` — list registered hooks
- [x] `set-environment` / `setenv` — set session env var
- [x] `show-environment` / `showenv` — show env vars

---

## Terminal Emulation (VT100/xterm)

### CSI Sequences
- [x] CUU/CUD/CUF/CUB (`A/B/C/D`) — cursor movement
- [x] CUP/HVP (`H/f`) — cursor position (with DECOM support)
- [x] ED (`J`) — erase in display (modes 0,1,2,3)
- [x] EL (`K`) — erase in line (modes 0,1,2)
- [x] CNL/CPL (`E/F`) — cursor next/prev line
- [x] CHA (`G`) — cursor horizontal absolute
- [x] IL/DL (`L/M`) — insert/delete lines
- [x] ICH/DCH (`@/P`) — insert/delete characters
- [x] ECH (`X`) — erase characters
- [x] SU/SD (`S/T`) — scroll up/down
- [x] HPA (`\``) — horizontal position absolute
- [x] VPA (`d`) — vertical position absolute
- [x] REP (`b`) — repeat character
- [x] CBT (`Z`) — cursor backward tab
- [x] TBC (`g`) — tab clear
- [x] SGR (`m`) — full color/attribute support (256-color, RGB, bold, italic, underline, etc.)
- [x] DECSTBM (`r`) — set scroll region
- [x] SM/RM (`h/l`) — set/reset mode
- [x] DECSCUSR (`q SP`) — cursor style (block, underline, bar, blinking variants)
- [x] DA (`c`) — primary device attributes (VT220)
- [x] DA2 (`> c`) — secondary device attributes
- [x] DSR/CPR (`n`) — device status / cursor position report
- [ ] DECSC/DECRC via CSI — (handled via ESC 7/8 instead)
- [ ] DECSCA — selective character erase attribute
- [x] DECSTR — soft terminal reset (CSI ! p)

### ESC Sequences
- [x] ESC 7 / ESC 8 — save/restore cursor (DECSC/DECRC)
- [x] ESC M — reverse index (scroll down at top)
- [x] ESC D — index (scroll up at bottom)
- [x] ESC E — next line
- [x] ESC c — full reset (RIS)
- [x] ESC H — set tab stop
- [x] ESC (0 / ESC (B — DEC line drawing charset / ASCII
- [x] ESC ) — G1 charset designation
- [x] SO/SI (0x0E/0x0F) — shift in/out for line drawing

### DEC Private Modes (DECSET/DECRST)
- [x] Mode 6 — DECOM (origin mode)
- [x] Mode 12/13 — cursor blink (acknowledged, no flag)
- [x] Mode 25 — cursor visible (DECTCEM)
- [x] Mode 47/1047 — alternate screen buffer
- [x] Mode 1049 — alternate screen with cursor save/restore
- [x] Mode 1000 — mouse standard mode
- [x] Mode 1002 — mouse button tracking
- [x] Mode 1003 — mouse any-event tracking
- [x] Mode 1004 — focus events
- [x] Mode 1006 — mouse SGR extended coordinates
- [x] Mode 2004 — bracketed paste mode
- [x] Mode 2026 — synchronized output (defers redraw)
- [x] Mode 7 — auto-wrap mode (DECAWM)
- [x] Mode 1 — cursor keys mode (DECCKM)
- [x] Mode 4 — insert/replace mode (IRM via SM/RM)
- [ ] Mode 1005 — mouse UTF-8 mode
- [ ] Mode 1015 — mouse urxvt mode
- [ ] Mode 1007 — alternate scroll mode
- [ ] Mode 2 — keyboard action mode (KAM)

### Standard Modes (SM/RM)
- [x] Mode 4 — IRM (insert/replace mode)

### OSC Sequences
- [x] OSC 0 — set window/icon title
- [x] OSC 1 — set icon name
- [x] OSC 2 — set window title
- [x] OSC 4 — set/query palette color
- [x] OSC 7 — set working directory (per-pane CWD)
- [x] OSC 8 — hyperlinks
- [x] OSC 10 — set/query foreground color
- [x] OSC 11 — set/query background color
- [x] OSC 52 — clipboard access (base64)
- [x] OSC 112 — reset cursor color
- [x] OSC 104 — reset palette color
- [x] OSC 110 — reset foreground color
- [x] OSC 111 — reset background color

### SGR Attributes
- [x] Bold, dim, italic, underline, blink, reverse, hidden, strikethrough
- [x] Double underline, curly underline, dotted underline, dashed underline
- [x] Overline
- [x] Underline color (SGR 58)
- [x] 256-color palette (SGR 38;5;N / 48;5;N)
- [x] 24-bit RGB color (SGR 38;2;R;G;B / 48;2;R;G;B)
- [x] Bright/high-intensity colors (SGR 90-97, 100-107)

---

## Options

### Server Options
- [x] `buffer-limit` — max paste buffers
- [x] `escape-time` — key escape delay (ms)
- [x] `exit-empty` — exit when no sessions
- [x] `exit-unattached` — exit when no clients
- [x] `focus-events` — pass focus events to apps
- [x] `history-limit` — scrollback lines
- [x] `set-clipboard` — clipboard integration mode
- [x] `terminal-overrides` — terminal capability overrides
- [x] `default-terminal` — TERM value
- [x] `message-limit` — message log size
- [x] `prefix-timeout` — prefix key timeout

### Session Options
- [x] `base-index` — starting window index
- [x] `default-shell` — shell path
- [x] `default-command` — default command
- [x] `prefix` / `prefix2` — prefix key(s)
- [x] `status` — show/hide status line
- [x] `status-left` / `status-right` — status line content
- [x] `status-style` — status line style
- [x] `status-position` — top/bottom
- [x] `status-justify` — left/centre/right window list alignment
- [x] `status-left-length` / `status-right-length` — max widths
- [x] `status-left-style` / `status-right-style` — section styles
- [x] `status-interval` — refresh interval (defined, not yet used in tick)
- [x] `status-keys` — emacs/vi mode for prompts
- [x] `mouse` — mouse support
- [x] `renumber-windows` — renumber on close
- [x] `automatic-rename` — auto-rename from process title
- [x] `display-time` — message display duration (defined, not timed)
- [x] `repeat-time` — key repeat window (defined, not timed)
- [x] `set-titles` / `set-titles-string` — xterm title updates
- [x] `message-style` / `message-command-style` — prompt styles (defined)
- [x] `destroy-unattached` — kill session when last client detaches (defined)
- [x] `detach-on-destroy` — behavior when session destroyed (defined)
- [x] `visual-activity` / `visual-bell` / `visual-silence` — notification styles (defined)
- [x] `word-separators` — word boundary chars for copy mode (defined)
- [x] `window-status-format` / `window-status-current-format` — per-window format
- [ ] `lock-after-time` — auto-lock timeout
- [ ] `lock-command` — lock screen command
- [ ] `default-size` — default window size
- [ ] `key-table` — default key table
- [ ] `silence-action` / `bell-action` / `activity-action` — alert actions

### Window Options
- [x] `mode-keys` — vi/emacs copy mode
- [x] `automatic-rename` — auto-rename
- [x] `aggressive-resize` — resize to smallest client
- [x] `allow-rename` — allow process to rename
- [x] `monitor-activity` — monitor for activity
- [x] `monitor-bell` / `monitor-silence` — bell/silence monitoring
- [x] `pane-border-style` / `pane-active-border-style` — border styling
- [x] `remain-on-exit` — keep dead panes
- [x] `alternate-screen` — allow alternate screen
- [x] `synchronize-panes` — send input to all panes
- [x] `wrap-search` — wrap copy mode search (defined)
- [x] `pane-base-index` — starting pane index (defined)
- [x] `main-pane-height` / `main-pane-width` — main pane dimensions (defined)
- [x] `window-status-style` / `window-status-current-style` — entry styling
- [x] `window-status-last-style` — last-active window style (defined)
- [x] `window-status-activity-style` / `window-status-bell-style` — alert styles (defined)
- [x] `window-status-separator` — separator between entries
- [x] `window-active-style` / `window-style` — pane background styles (defined)
- [x] `allow-passthrough` — allow passthrough sequences (defined)
- [x] `xterm-keys` — xterm-compatible key output (defined)
- [x] `copy-mode-match-style` / `copy-mode-current-match-style` / `copy-mode-mark-style` — search highlight styles (defined)

---

## Rendering

- [x] Status line with format expansion (`#{variable}`, `#S`, `#I`, `#W`, `#F`)
- [x] Status line position (top/bottom)
- [x] Status line on/off
- [x] Status line justification (left/centre/right)
- [x] Status line length truncation
- [x] Window entry styling (active/inactive)
- [x] Custom window-status-separator
- [x] Pane borders (vertical `│`, horizontal `─`)
- [x] Active pane border highlighting
- [x] Copy mode overlay with selection highlighting
- [x] Copy mode scroll offset display
- [x] Pane count display in status line
- [x] Multi-window status line
- [x] Command prompt mode (`:`, `/`, `?`)
- [x] Synchronized output (mode 2026 defers redraw)
- [x] Xterm title escape (OSC 2) via set-titles
- [x] Cursor style passthrough (block/underline/bar)
- [x] Status line style changes within format strings (`#[fg=red]`)
- [ ] Pane border status line (pane-border-status)
- [ ] Window flags beyond `*` (e.g., `-`, `#`, `!`, `Z`, `M`)
- [ ] Window activity/bell/silence indicators in status

---

## Copy Mode

- [x] Enter/exit copy mode
- [x] Vi and emacs key modes
- [x] Cursor movement (h/j/k/l, arrows, w/b/e, 0/$, ^)
- [x] Page up/down, half-page up/down
- [x] Scroll with offset tracking
- [x] Selection (v, space to start, enter to copy)
- [x] Search forward (`/`) and backward (`?`)
- [x] Search next/prev (`n`/`N`)
- [x] Copy to paste buffer
- [x] Selection rendering (reverse video)
- [x] History line access (scrollback)
- [ ] Rectangle selection
- [x] Jump to character (f/F/t/T)
- [ ] Go to line (`:` in copy mode)
- [ ] Mark and swap (`m`/`M-m`)
- [ ] Copy pipe (copy and pipe to command)
- [ ] Word selection (double-click equivalent)

---

## Key Bindings

### Prefix Table (Ctrl-b + key)
- [x] `c` — new-window
- [x] `n` — next-window
- [x] `p` — previous-window
- [x] `l` — last-window
- [x] `d` — detach-client
- [x] `"` — split-window vertical
- [x] `%` — split-window horizontal
- [x] `x` — kill-pane (via confirm-before)
- [x] `o` — select next pane
- [x] `Up/Down/Left/Right` — select pane by direction
- [x] `z` — zoom/unzoom pane
- [x] `[` — copy-mode
- [x] `]` — paste-buffer
- [x] `!` — break-pane
- [x] `;` — last-pane
- [x] `{` / `}` — swap-pane
- [x] `Space` — next-layout
- [x] `0-9` — select window by index
- [x] `:` — command-prompt
- [x] `&` — kill-window (via confirm-before)
- [x] `,` — rename-window prompt
- [x] `'` — window index prompt
- [x] `.` — move-window prompt
- [x] `?` — list-keys
- [x] `w` — choose-tree
- [x] `=` — choose-buffer
- [x] `~` — show-messages
- [x] `s` — choose-tree (sessions)
- [x] `$` — rename-session prompt
- [x] `#` — list-buffers
- [x] `t` — clock-mode
- [x] `q` — display-panes
- [x] `i` — display-message (window info)
- [x] `C-Up/Down/Left/Right` — resize-pane
- [x] `M-Up/Down/Left/Right` — resize-pane (x5)
- [x] `M-1..5` — select layout
- [x] `f` — find-window prompt
- [x] `D` — choose-client (detach)
- [x] `(` / `)` — switch-client prev/next
- [x] `r` — refresh-client
- [x] `C-o` — rotate-window
- [x] `M-o` — rotate-window reverse
- [x] `PageUp` — copy-mode + page up

---

## Wire Protocol (imsg v8)

- [x] Client → Server identification sequence (term type, CWD, env)
- [x] Command messages (argc/argv encoding)
- [x] Ready/Exit/Detach responses
- [x] InputData / OutputData bidirectional streaming
- [x] SCM_RIGHTS fd passing
- [x] Message framing with type codes matching tmux
- [ ] Full imsg header field compatibility (pid, flags)
- [ ] Control mode (`-C` flag)
- [ ] Window/pane change notifications

---

## Format Variables

### Implemented
- [x] `session_name`, `session_id`, `session_windows`, `session_attached`
- [x] `window_name`, `window_index`, `window_id`, `window_active`, `window_flags`, `window_panes`
- [x] `pane_id`, `pane_index`, `pane_title`, `pane_active`, `pane_width`, `pane_height`
- [x] `pane_current_command`, `pane_current_path` (OSC 7), `pane_pid`
- [x] `pane_in_mode`, `alternate_on`
- [x] `cursor_x`, `cursor_y`
- [x] `host`, `host_short`
- [x] `client_name`, `client_tty`
- [x] `pane_count`

### Format Features
- [x] `#{variable}` expansion
- [x] `#S`, `#I`, `#W`, `#F`, `#T`, `#P` shorthand aliases
- [x] Conditionals: `#{?test,true,false}`
- [x] Comparisons: `#{==:a,b}`, `#{!=:a,b}`
- [x] Truncation: `#{=N:var}` (positive=left, negative=right)
- [x] String operations: `#{s/pat/rep:var}` (substitution)
- [x] `#{l:literal}` — literal string
- [ ] `#{E:var}` — expand twice
- [ ] `#{T:var}` — strftime expansion
- [ ] Numeric comparisons: `#{<:a,b}`, `#{>:a,b}`

### Missing Variables
- [ ] `session_created`, `session_activity`, `session_alerts`
- [ ] `window_layout`, `window_bell_flag`, `window_activity_flag`
- [ ] `pane_tty`, `pane_start_command`, `pane_dead`
- [ ] `client_session`, `client_width`, `client_height`, `client_activity`
- [ ] `cursor_flag`, `insert_flag`, `keypad_flag`, `mouse_any_flag`

---

## Infrastructure

- [x] Unix domain socket transport
- [x] Single-threaded tokio event loop
- [x] PTY management (forkpty, resize, process tracking)
- [x] Pane exit cascading (pane → window → session cleanup)
- [x] Config file loading (`source-file`, `~/.tmux.conf`)
- [x] Unambiguous prefix command matching
- [x] Hooks system (set-hook/show-hooks)
- [x] Environment variable management
- [x] Mouse event handling (click, drag, scroll, SGR encoding)
- [x] Automatic window rename via OSC 0/2
- [x] Clipboard via OSC 52
- [x] ~60fps render tick
- [x] Fuzzing infrastructure (8 targets)
- [x] Property-based testing (proptest)
- [ ] Control mode (`tmux -C`)
- [ ] Socket session naming (`tmux -L name`)
- [ ] Multiple server socket support
- [ ] Terminal info database integration (terminfo/termcap)
- [ ] Activity/bell/silence monitoring and alerts
