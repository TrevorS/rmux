# rmux / tmux Parity Tracker

Comprehensive audit of rmux implementation status vs tmux next-3.7.
Last updated: 2026-03-16.

Legend: `[x]` = implemented, `[ ]` = missing, `[~]` = partial/wrong default, `[!]` = bug

---

## 1. Commands

### Session Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `new-session` | `-A -c -d -D -e -E -F -f -n -P -s -X -x -y`, shell cmd | — | Complete |
| `kill-session` | `-a -C -t` | — | Complete |
| `list-sessions` / `ls` | `-F -f` | — | Functional (`-F` format and `-f` filter parsed but not applied) |
| `has-session` | `-t` | — | Complete |
| `rename-session` | `-t` | — | Complete |
| `switch-client` | `-c -E -F -l -n -O -p -r -t -T -Z` | — | Complete |

### Client Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `attach-session` | `-c -d -E -f -r -t -x` | — | Complete |
| `detach-client` | `-a -E -P -s -t` | — | Complete |
| `refresh-client` | `-A -B -c -C -D -f -l -L -r -R -S -t -U` | — | Complete |
| `suspend-client` | `-t` | — | Complete |

### Window Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `new-window` | `-a -b -c -d -e -F -k -n -P -S -t`, shell cmd | — | Complete |
| `kill-window` | `-a -t` | — | Complete |
| `select-window` | `-l -n -p -T -t` | — | Complete |
| `next-window` | `-a -t` | — | Complete |
| `previous-window` | `-a -t` | — | Complete |
| `last-window` | `-t` | — | Complete |
| `rename-window` | `-t` | — | Complete |
| `list-windows` | `-a -F -f -t` | — | Complete |
| `find-window` | `-C -N -r -t -T -Z` | — | Functional (`-C/-N/-T` scope flags parsed but search is name-only) |
| `swap-window` | `-d -s -t` | — | Complete |
| `move-window` | `-a -b -d -k -r -s -t` | — | Complete |
| `rotate-window` | `-D -t -U` | — | Complete |
| `select-layout` | `-E -n -o -p -t layout-name` | — | Complete |
| `next-layout` / `previous-layout` | `-t` | — | Complete |
| `respawn-window` | `-k -t`, shell cmd | — | Complete |
| `link-window` | `-d -k -s -t` | — | Functional (copies window, no shared ownership) |
| `unlink-window` | `-k -t` | — | Functional (kills window, no shared ownership) |

### Pane Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `split-window` | `-b -c -d -e -f -F -h -I -l -p -P -t -v -Z`, shell cmd | — | Complete |
| `select-pane` | `-D -d -e -g -L -l -M -m -P -R -T -t -U -Z` | — | Complete |
| `kill-pane` | `-a -t` | — | Complete |
| `list-panes` | `-a -F -f -s -t` | — | Complete |
| `capture-pane` | `-a -b -C -e -E -J -M -N -p -P -q -S -t -T` | — | Complete |
| `resize-pane` | `-D -L -M -R -T -U -x -y -Z` | — | Complete |
| `swap-pane` | `-d -D -s -t -U -Z` | — | Complete |
| `break-pane` | `-a -b -d -F -n -P -s -t` | — | Complete |
| `join-pane` | `-b -d -f -h -l -p -s -t -v` | — | Complete |
| `last-pane` | `-d -e -t -Z` | — | Complete |
| `respawn-pane` | `-k -t`, shell cmd | — | Complete |

### Server / Control Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `send-keys` | `-c -F -H -K -l -M -N -R -t -X` | — | Complete |
| `bind-key` | `-n -N -r -T` | — | Complete |
| `unbind-key` | `-a -n -q -T` | — | Complete |
| `source-file` | `-F -n -q -t -v`, glob, multiple paths | — | Complete |
| `run-shell` | `-b -C -c -d -E -s -t` | — | Complete |
| `command-prompt` | `-1 -b -e -F -I -i -k -l -N -p -T` | — | Complete |
| `if-shell` | `-b -F -t` | — | Complete |
| `confirm-before` | `-b -c -p -t -y` | — | Complete |
| `send-prefix` | `-2 -t` | — | Complete |
| `clear-history` | `-H -t` | — | Complete |
| `wait-for` | `-L -S -U` | — | Functional (lock/signal/unlock; blocking wait is no-op) |

### Option Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `set-option` / `set` | `-a -F -g -o -p -q -s -t -u -U -w` | — | Complete |
| `show-options` / `show` | `-A -g -H -p -q -s -t -v -w` | — | Complete |
| `set-window-option` / `setw` | delegates to set-option `-w` | — | Complete |
| `show-window-options` | delegates to show-options `-w` | — | Complete |

### Display Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `display-message` | `-a -b -c -d -F -l -p -v` | — | Complete |
| `list-keys` | `-1 -a -N -P -T` | — | Complete |
| `display-panes` | `-b -d -t` | — | Functional (text output, not overlay) |
| `clock-mode` | `-t` | — | Complete |
| `choose-tree` | `-F -f -G -K -N -O -r -s -t -w -Z` | — | Complete |
| `choose-buffer` / `choose-client` | `-F -f -G -K -N -O -r -t -Z` | — | Complete |
| `display-menu` | `-b -c -H -O -s -S -t -T -x -y` | — | Complete |
| `display-popup` | `-B -c -C -d -e -E -h -K -s -S -T -t -w -x -y` | — | Complete |
| `pipe-pane` | `-I -o -t` | — | Functional (`-o` toggle parsed but not applied) |
| `resize-window` | `-A -D -L -R -U -t -x -y` | — | Complete |

### Environment Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `set-environment` | `-F -g -h -r -t -u` | — | Complete |
| `show-environment` | `-g -h -s -t` | — | Complete |

### Paste Buffer Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `copy-mode` | `-d -e -H -M -q -S -s -t -u` | — | Complete |
| `paste-buffer` | `-b -d -p -r -s -t` | — | Complete |
| `set-buffer` | `-a -b -n -t -w` | — | Complete |
| `delete-buffer` | `-b` | — | Complete |
| `save-buffer` / `load-buffer` | `-a -b -t -w` | — | Complete |
| `show-buffer` | `-b` | — | Complete |
| `list-buffers` | `-F -f` | — | Complete |

### Stubs / No-ops

| Command | Status |
|---|---|
| `lock-server` / `lock-session` / `lock-client` | No-op |
| `server-access` | No-op |

---

## 2. Options

### Server Options

| Option | rmux | Default Correct? | Notes |
|---|---|---|---|
| `backspace` | [x] | Yes (`""`) | |
| `buffer-limit` | [x] | Yes (50) | |
| `command-alias` | [x] | Yes (6 standard aliases) | Array type |
| `copy-command` | [x] | Yes (`""`) | |
| `default-client-command` | [x] | Yes (`""`) | |
| `default-terminal` | [x] | Yes (`"screen"`, matches `TMUX_TERM`) | |
| `editor` | [x] | Yes (`""`) | |
| `escape-time` | [x] | Yes (10) | |
| `exit-empty` | [x] | Yes | |
| `exit-unattached` | [x] | Yes | |
| `extended-keys` | [x] | Yes (`"off"`) | Choice: off/on/always |
| `focus-events` | [x] | Yes | |
| `history-file` | [x] | Yes (`""`) | |
| `history-limit` | — | Moved to session scope (correct) | |
| `input-buffer-size` | [x] | Yes (1048576) | |
| `message-limit` | [x] | Yes (1000) | |
| `prefix-timeout` | [x] | Yes (0) | |
| `prompt-history-limit` | [x] | Yes (100) | |
| `set-clipboard` | [x] | Yes (`"external"`) | |
| `terminal-features` | [x] | Yes (xterm*, screen*) | Array type |
| `terminal-overrides` | [x] | Yes (`"linux*:AX@"`) | Array type |
| `user-keys` | [x] | Yes (`[]`) | Array type |

### Session Options

| Option | rmux | Default Correct? | Notes |
|---|---|---|---|
| `activity-action` | [x] | Yes | |
| `assume-paste-time` | [x] | Yes (1) | |
| `base-index` | [x] | Yes (0) | |
| `bell-action` | [x] | Yes | |
| `default-command` | [x] | Yes | |
| `default-shell` | [x] | Yes | |
| `default-size` | [x] | Yes | |
| `destroy-unattached` | [x] | Yes (`"off"`) | Choice: off/on/keep-last/keep-group |
| `detach-on-destroy` | [x] | Yes | |
| `display-panes-active-colour` | [x] | Yes (`"red"`) | |
| `display-panes-colour` | [x] | Yes (`"blue"`) | |
| `display-panes-time` | [x] | Yes (1000) | |
| `display-time` | [x] | Yes (750) | |
| `focus-follows-mouse` | [x] | Yes | |
| `key-table` | [x] | Yes | |
| `lock-after-time` | [x] | Yes (0) | |
| `lock-command` | [x] | Yes | |
| `message-command-style` | [x] | Yes | |
| `message-style` | [x] | Yes | |
| `mouse` | [x] | Yes | |
| `prefix` | [x] | Yes (`C-b`) | |
| `prefix2` | [x] | Yes (`""` = no prefix2) | |
| `renumber-windows` | [x] | Yes | |
| `repeat-time` | [x] | Yes (500) | |
| `set-titles` | [x] | Yes | |
| `set-titles-string` | [x] | Yes | |
| `silence-action` | [x] | Yes (`"other"`) | |
| `status` | [x] | Yes | |
| `status-format` | [x] | Yes (`[]`) | Array, dynamically computed |
| `status-interval` | [x] | Yes (15) | |
| `status-justify` | [x] | Yes | |
| `status-keys` | [x] | Yes | |
| `status-left` | [x] | Yes | |
| `status-left-length` | [x] | Yes (10) | |
| `status-left-style` | [x] | Yes | |
| `status-position` | [x] | Yes | |
| `status-right` | [x] | Yes (includes `#{?window_bigger,...}`) | |
| `status-right-length` | [x] | Yes (40) | |
| `status-right-style` | [x] | Yes | |
| `status-style` | [x] | Yes | |
| `update-environment` | [x] | Yes (8 vars) | Array type |
| `visual-activity` | [x] | Yes | |
| `visual-bell` | [x] | Yes | |
| `visual-silence` | [x] | Yes | |
| `word-separators` | [x] | Yes (full punctuation set) | |

### Window Options

| Option | rmux | Default Correct? | Notes |
|---|---|---|---|
| `aggressive-resize` | [x] | Yes | |
| `allow-passthrough` | [x] | Yes (`"off"`) | Choice: off/on/all |
| `allow-rename` | [x] | Yes (`false`) | |
| `alternate-screen` | [x] | Yes | |
| `automatic-rename` | [x] | Yes | |
| `automatic-rename-format` | [x] | Yes (conditional format) | |
| `clock-mode-colour` | [x] | Yes (`"blue"`) | |
| `clock-mode-style` | [x] | Yes (24) | |
| `copy-mode-current-match-style` | [x] | Yes | |
| `copy-mode-mark-style` | [x] | Yes | |
| `copy-mode-match-style` | [x] | Yes | |
| `fill-character` | [x] | Yes (`""`) | |
| `main-pane-height` | [x] | Yes (`"24"`) | String type, supports `%` |
| `main-pane-width` | [x] | Yes (`"80"`) | String type, supports `%` |
| `mode-keys` | [x] | Yes | |
| `mode-style` | [x] | Yes (`"bg=yellow,fg=black"`) | |
| `monitor-activity` | [x] | Yes | |
| `monitor-bell` | [x] | Yes | |
| `monitor-silence` | [x] | Yes (0) | |
| `pane-active-border-style` | [x] | Yes (conditional format) | |
| `pane-base-index` | [x] | Yes (0) | |
| `pane-border-format` | [x] | Yes | |
| `pane-border-lines` | [x] | Yes (`"single"`) | Choice: single/double/heavy/simple/number |
| `pane-border-status` | [x] | Yes | |
| `pane-border-style` | [x] | Yes | |
| `popup-border-lines` | [x] | Yes (`"single"`) | |
| `popup-border-style` | [x] | Yes (`"default"`) | |
| `popup-style` | [x] | Yes (`"default"`) | |
| `remain-on-exit` | [x] | Yes (`"off"`) | Choice: off/on/failed |
| `scroll-on-clear` | [x] | Yes | |
| `synchronize-panes` | [x] | Yes | |
| `window-active-style` | [x] | Yes | |
| `window-size` | [x] | Yes (`"latest"`) | Choice: largest/smallest/manual/latest |
| `window-status-activity-style` | [x] | Yes | |
| `window-status-bell-style` | [x] | Yes | |
| `window-status-current-format` | [~] | Simplified — uses `#I:#W#F` instead of conditional `#{?window_flags,...}` | |
| `window-status-current-style` | [x] | Yes | |
| `window-status-format` | [~] | Same simplified format | |
| `window-status-last-style` | [x] | Yes | |
| `window-status-separator` | [x] | Yes | |
| `window-status-style` | [x] | Yes | |
| `window-style` | [x] | Yes | |
| `wrap-search` | [x] | Yes | |
| `xterm-keys` | [x] | Yes (deprecated) | |

### Option Scopes & Inheritance

- [x] Server (global) scope
- [x] Session scope with parent inheritance from server
- [x] Window scope (default set)
- [ ] Window options inherit from session
- [ ] Pane options (entire `-p` scope)
- [x] String, Number, Flag/boolean types
- [ ] Choice options (validated enum)
- [ ] Array options (indexed with `option[N]`)

---

## 3. Format Expansion

### Variable References
- [x] `#{variable_name}` lookup
- [x] Short aliases: `#S`, `#W`, `#I`, `#T`, `#F`, `#D`, `#H`, `#h`, `#P`
- [x] `#{@user_option}` lookup
- [x] `##` literal `#`

### Modifiers
- [x] `#{E:expr}` double expansion
- [x] `#{T:expr}` strftime expansion
- [x] `#{l:text}` literal
- [x] `#{d:variable}` dirname
- [x] `#{b:variable}` basename
- [x] `#{s/pattern/replacement:expr}` substitution
- [x] `#{=N:expr}` truncation (positive=left, negative=right)
- [x] `#{q:expr}` shell quoting (single-quote with escaping)
- [x] `#{n:expr}` string length (character count)
- [x] `#{w:expr}` display width (CJK-aware)
- [x] `#{a:expr}` ASCII/Unicode code to character
- [x] `#{p:N:expr}` padding (positive=right-pad, negative=left-pad)
- [x] `#{!expr}` logical NOT
- [x] `#{||:a,b}` / `#{&&:a,b}` logical OR/AND
- [x] `#{e|op:a,b}` arithmetic (+,-,*,/,%)
- [x] `#{m:pattern,string}` fnmatch glob match
- [x] `#{m/r:pattern,string}` regex match (basic POSIX-like)

### Conditionals & Comparisons
- [x] `#{?condition,true,false}` ternary
- [x] `#{==:a,b}`, `#{!=:a,b}`, `#{<:a,b}`, `#{>:a,b}`, `#{<=:a,b}`, `#{>=:a,b}`
- [~] Multi-branch — works via nested conditionals (tmux doesn't support native multi-branch either)

### Loops
- [ ] `#{S:format}` sessions
- [ ] `#{W:format}` windows
- [ ] `#{P:format}` panes

### Inline Styles
- [x] `#[fg=color,bg=color,attrs]`

---

## 4. Format Variables

### Implemented

**Session:** `session_name`, `session_id`, `session_windows`, `session_attached`, `session_created`, `session_activity`, `session_alerts`, `session_path`

**Window:** `window_index`, `window_name`, `window_id`, `window_flags`, `window_active`, `window_panes`, `window_layout`, `window_zoomed_flag`, `window_last_flag`, `window_activity_flag`, `window_bell_flag`, `window_silence_flag`, `window_bigger`

**Pane:** `pane_id`, `pane_index`, `pane_title`, `pane_width`, `pane_height`, `pane_active`, `pane_dead`, `pane_dead_status`, `pane_current_command`, `pane_current_path`, `pane_pid`, `pane_tty`, `pane_start_command`, `pane_in_mode`, `pane_synchronized`, `pane_at_top`, `pane_at_bottom`, `pane_at_left`, `pane_at_right`

**Client:** `client_name`, `client_tty`, `client_prefix`, `client_width`, `client_height`, `client_activity`, `client_session`, `client_pid`, `client_key_table`, `client_termname`

**Terminal state:** `cursor_x`, `cursor_y`, `cursor_flag`, `insert_flag`, `keypad_flag`, `alternate_on`, `mouse_any_flag`

**Scrollback:** `history_size`, `history_limit`

**Paste buffers:** `buffer_name`, `buffer_size`

**System:** `host`, `host_short`, `version`, `pid`, `socket_path`, `current_file`

**Mouse:** `mouse_x`, `mouse_y`

### Missing (less common)

| Variable | Why it matters |
|---|---|
| `session_grouped` | Session group membership |
| `session_group` | Session group name |
| `window_linked` | Window linked to multiple sessions |
| `pane_pipe` | Whether pipe-pane is active |
| `pane_search_string` | Last search in copy mode |
| `client_control_mode` | Control mode indicator |
| `client_flags` | Client flag string |
| `server_sessions` | Total server session count |

---

## 5. Key Bindings

### Prefix Table — Missing Bindings

| Key | tmux command | Notes |
|---|---|---|
| `C-z` | `suspend-client` | |
| `-` | `delete-buffer` | |
| `/` | `command-prompt -kpkey {list-keys -1N}` | Key help |
| `C` | `customize-mode -Z` | |
| `E` | `select-layout -E` | Spread even |
| `L` | `switch-client -l` | Last client |
| `M` | `select-pane -M` | Clear mark |
| `m` | `select-pane -m` | Set mark |
| `M-6` | `select-layout main-horizontal-mirrored` | |
| `M-7` | `select-layout main-vertical-mirrored` | |
| `M-n` | `next-window -a` | Next with alert |
| `M-p` | `previous-window -a` | Previous with alert |
| `S-Up/Down/Left/Right` | `refresh-client -U/D/L/R 10` | Client scroll |
| `DC` (Delete) | `refresh-client -c` | Clear client |
| `<` | `display-menu` (window menu) | |
| `>` | `display-menu` (pane menu) | |

### Prefix Table — Behavior Differences

| Key | tmux | rmux | Issue |
|---|---|---|---|
| `&` | `confirm-before ... kill-window` | `kill-window` directly | No confirmation |
| `x` | `confirm-before ... kill-pane` | `kill-pane` directly | No confirmation |
| `Up/Down/Left/Right` | `select-pane` with `-r` (repeat) | `select-pane` without `-r` | Not repeatable |
| `s` | `choose-tree -Zs` | `choose-tree` | Missing `-Z` (zoom) |
| `w` | `choose-tree -Zw` | `choose-tree` | Missing `-Z`, same as `s` |
| `=` | `choose-buffer -Z` | `choose-buffer` | Missing `-Z` |
| `]` | `paste-buffer -p` | `paste-buffer` | Missing `-p` (bracket paste) |

### Root Table

tmux has extensive mouse bindings in the root table — **all missing from rmux**:
`MouseDown1Pane`, `MouseDrag1Pane`, `WheelUpPane`, `WheelDownPane`, `MouseDown2Pane`, `DoubleClick1Pane`, `TripleClick1Pane`, `MouseDrag1Border`, `MouseDown1Status`, `WheelUpStatus`, `WheelDownStatus`, `MouseDown3StatusLeft`, `MouseDown3Status`, `MouseDown3Pane`.

### Copy-mode-vi — Bugs

| Key | tmux | rmux | Bug |
|---|---|---|---|
| `Escape` | `clear-selection` | `cancel` | Should clear selection, not exit |
| `v` | `rectangle-toggle` | `begin-selection` | Wrong action entirely |
| `m` | (unbound) | `set-mark` | tmux uses `X` for set-mark |
| `Enter` | `copy-pipe-and-cancel` | `copy-selection-and-cancel` | No pipe support |

### Copy-mode-vi — Missing Bindings

`#`, `*`, `A`, `B`, `C-c`, `C-e`, `C-h`, `C-j`, `C-y`, `D`, `E`, `H`, `J`, `K`, `L`, `M`, `P`, `W`, `X`, `o`, `r`, `z`, `%`, `{`, `}`, `BSpace`, `M-x`, `C-Up`, `C-Down`, `1`-`9` (repeat count).

### Copy-mode-emacs — Bugs

| Key | tmux | rmux | Bug |
|---|---|---|---|
| `C-g` | `clear-selection` | `cancel` | Should clear, not exit |
| `M-f` | `next-word-end` | `next-word` | Wrong word movement |

### Copy-mode-emacs — Missing Bindings

`C-c`, `C-k`, `C-l`, `C-r`, `C-s`, `C-w`, `Space`, `,`, `;`, `F`, `N`, `P`, `R`, `T`, `X`, `f`, `g`, `n`, `q`, `r`, `t`, `Home`, `End`, `M-1`-`M-9`, `M-<`, `M->`, `M-R`, `M-l`, `M-m`, `M-r`, `M-x`, `M-{`, `M-}`, `M-Up`, `M-Down`, `C-Up`, `C-Down`, `C-M-b`, `C-M-f`.

---

## 6. Config File Parsing

### Implemented
- [x] Comments (`# ...`), inline comments
- [x] Double-quoted strings with escapes (`\"`, `\\`, `\n`, `\t`)
- [x] Single-quoted strings (literal)
- [x] Empty quoted strings (`""`, `''`)
- [x] Semicolon command separator (`;`)
- [x] Escaped semicolons (`\;` for bind multi-commands)
- [x] Line continuation (backslash at end of line)
- [x] Tilde expansion (`~` -> `$HOME`)
- [x] `%if` / `%elif` / `%else` / `%endif`
- [x] `%hidden NAME=VALUE`
- [x] `${VAR}` environment variable interpolation

### Missing
- [ ] `\a`, `\b`, `\e`, `\f`, `\r`, `\s`, `\v` control character escapes
- [ ] `\uNNNN` / `\UNNNNNNNN` Unicode escapes
- [ ] `\NNN` octal escapes
- [ ] `$VAR` (no-brace) interpolation
- [x] Glob patterns in `source-file`
- [ ] `source-file` depth limiting (tmux: 50 levels max)

---

## 7. Terminal Emulation

### SGR Attributes
- [x] Bold (1), Dim (2), Italic (3), Underline (4), Blink (5), Reverse (7), Hidden (8), Strikethrough (9)
- [x] Double underline (21)
- [x] Overline (53)
- [x] All turn-off codes (22-29, 55)
- [~] Curly/dotted/dashed underline — **defined in data model and emitted by TermWriter, but parser cannot decode colon sub-params** (`4:3`, `4:4`, `4:5`)
- [ ] Rapid blink (6) — silently dropped

### Colors
- [x] Standard 16 colors (30-37, 40-47, 90-97, 100-107)
- [x] 256-color palette (38;5;n, 48;5;n)
- [x] RGB truecolor (38;2;r;g;b, 48;2;r;g;b)
- [x] Underline color (58;5;n, 58;2;r;g;b, 59)
- [x] Default color reset (39, 49)

### CSI Sequences
- [x] Cursor movement: CUU/CUD/CUF/CUB/CNL/CPL/CHA/CUP/HVP/VPA/HPA
- [x] Erase: ED, EL, ECH
- [x] Insert/delete: ICH, DCH, IL, DL, REP
- [x] Scroll: SU, SD
- [x] Tabs: CBT, TBC
- [x] SGR (full)
- [x] Scroll region: DECSTBM
- [x] Mode set/reset: SM, RM, DECSET, DECRST
- [x] Cursor style: DECSCUSR
- [x] Cursor save/restore: ANSI `CSI s` / `CSI u`
- [x] Device queries: DA1, DA2, DSR
- [x] Soft reset: DECSTR

### OSC Sequences
- [x] OSC 0/1/2 — window/icon title
- [x] OSC 4 — palette color set/reset
- [x] OSC 7 — current working directory
- [x] OSC 8 — hyperlinks
- [x] OSC 10/11 — foreground/background color set
- [x] OSC 52 — clipboard
- [x] OSC 104/110/111/112 — color resets
- [ ] OSC 4/52 query responses (acknowledged, no reply sent)

### DCS Sequences
- [x] State machine fully implemented (DCS parsed correctly)
- [ ] DECRQSS — not processed
- [ ] Sixel graphics — not processed
- [ ] tmux passthrough — not processed

### DECSET/DECRST Private Modes
- [x] 1 (DECCKM), 6 (DECOM), 7 (DECAWM), 25 (DECTCEM)
- [x] 47/1047/1049 (alternate screen)
- [x] 1000/1002/1003 (mouse modes)
- [x] 1004 (focus reporting) — flag tracked but [!] server does not emit focus in/out sequences
- [x] 1005/1006/1015 (mouse encodings: UTF-8, SGR, urxvt)
- [x] 1007 (alternate scroll)
- [x] 2004 (bracketed paste)
- [x] 2026 (synchronized output)
- [x] Standard modes: 2 (KAM), 4 (IRM) — [!] IRM flag tracked but `write_cell()` doesn't check it

### Mouse Protocols
- [x] X10 Normal (`ESC[M` + 3 bytes)
- [x] SGR (`ESC[<Ps;Px;PyM/m`)
- [x] UTF-8 mode 1005
- [x] urxvt mode 1015
- [x] Button 1/2/3 press, release, drag, wheel
- [ ] Buttons 4-5 (extra buttons)

### ESC Sequences
- [x] DECSC/DECRC (save/restore cursor)
- [x] IND, NEL, RI (line feed, next line, reverse index)
- [x] HTS (tab stop set)
- [x] RIS (full reset)
- [x] G0/G1 charset designation and SO/SI shifting
- [x] DEC Special Graphics (line drawing) character translation

### Cursor Styles
- [x] All 7 shapes (0-6): blinking/steady block/underline/bar

### Other
- [x] Alternate screen buffer with cursor save/restore
- [x] Scroll regions (partial and full screen)
- [x] Scrollback history (configurable, disabled in alternate screen)
- [x] Wide character support (2-column cells with padding)
- [x] Compact cell storage (8-byte ASCII + 256-color fast path)
- [x] Differential rendering (only emits changes)
- [x] Synchronized output for flicker-free updates

---

## 8. Plugin Compatibility (TPM)

### Implemented
- [x] `run-shell` command (async, pauses config queue)
- [x] tmux -> rmux shim (symlink for plugin compatibility)
- [x] `$TMUX` env var set for run-shell processes
- [x] Config loading inside event loop (plugins can connect back)
- [x] Exit-empty guard during config loading
- [x] Version string compatibility (`rmux 3.6.0`)
- [x] `set -ogqF` / `-agF` / `-wgF` / `-ug` flag combos
- [x] `%if`/`%elif`/`%else`/`%endif` conditionals
- [x] `%hidden` variables + `${VAR}` interpolation
- [x] `source -F` with `#{d:current_file}` path resolution
- [x] `#{@user_option}` format references
- [x] `#{E:@option}` double expansion

### Plugin Support Checklist
- [x] `run-shell -b` (background execution)
- [x] `send-keys -X` (copy-mode command dispatch)
- [x] `command-prompt -I/-p` (initial text, custom prompts)
- [x] Glob patterns in `source-file`
- [ ] `$VAR` (no-brace) interpolation

---

## 9. Fixed Defaults (resolved)

These wrong defaults were fixed and now match tmux:

| Option | Was | Now | tmux |
|---|---|---|---|
| `escape-time` | 500ms | 10ms | 10ms |
| `word-separators` | `" "` | full punctuation set | `!"#$%&'()*+,-.:;<=>?@[\]^`{|}~` |
| `allow-rename` | `true` | `false` | `false` |
| `silence-action` | `"none"` | `"other"` | `"other"` |
| `history-limit` | server scope | session scope | session scope |

Note: `default-terminal` = `"screen"` is correct — tmux upstream (`TMUX_TERM` in `tmux.h`) also defaults to `"screen"`. Distros may override this at build time to `"tmux-256color"`.

---

## 10. Reference

### tmux Source Files
- `options-table.c` — all options with types, scopes, defaults
- `format.c` — format variable definitions and expansion engine
- `key-bindings.c` — default key binding tables
- `cmd-*.c` — individual command implementations with flag handling
- `cfg.c` — config file loading
- `cmd-parse.y` — parser with conditionals, interpolation, quoting

### rmux Source Files
- `crates/rmux-server/src/server.rs` — event loop, option defaults, format context
- `crates/rmux-server/src/config.rs` — config parser
- `crates/rmux-server/src/command/builtins/*.rs` — command handlers
- `crates/rmux-server/src/command/mod.rs` — flag parsing, CommandServer trait
- `crates/rmux-server/src/format.rs` — format expansion
- `crates/rmux-server/src/keybind.rs` — default key bindings
- `crates/rmux-server/src/render/mod.rs` — rendering pipeline
- `crates/rmux-core/src/options.rs` — Options struct
- `crates/rmux-core/src/style/` — Style, Attrs, Color
- `crates/rmux-terminal/src/input/parser.rs` — VT100 parser
- `crates/rmux-terminal/src/mouse.rs` — mouse protocol
- `crates/rmux-terminal/src/keys.rs` — key encoding
