# rmux / tmux Parity Tracker

Comprehensive audit of rmux implementation status vs tmux next-3.7.
Last updated: 2026-03-16.

Legend: `[x]` = implemented, `[ ]` = missing, `[~]` = partial/wrong default, `[!]` = bug

---

## 1. Commands

### Session Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `new-session` | `-d -s -x -y -n -c -A` | `-D -e -E -F -f -P -X`, shell cmd | Functional |
| `kill-session` | `-t -a` | `-C` | Functional |
| `list-sessions` / `ls` | (none) | `-F` | Functional |
| `has-session` | `-t` | — | Complete |
| `rename-session` | `-t` | — | Complete |
| `switch-client` | `-l -n -p -t` | `-c -E -F -O -r -T -Z` | Functional |

### Client Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `attach-session` | `-d -t` | `-c -E -f -r -x` | Functional |
| `detach-client` | `-a` | `-E -s -t -P` | Functional |
| `refresh-client` | (none) | all (`-A -B -c -C -D -f -l -L -r -R -S -t -U`) | Stub (full redraw) |
| `suspend-client` | `-t` | — | Functional |

### Window Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `new-window` | `-a -b -d -k -n -S -t -c` | `-e -F -P`, shell cmd | Functional |
| `kill-window` | `-a -t` | — | Functional |
| `select-window` | `-l -n -p -T -t` | — | Functional |
| `next-window` | `-a -t` | — | Functional |
| `previous-window` | `-a -t` | — | Functional |
| `last-window` | `-t` | — | Complete |
| `rename-window` | `-t` | — | Complete |
| `list-windows` | `-a -t` | `-F -f` | Functional |
| `find-window` | `-t` | `-C -N -r -T -Z` | Basic matching |
| `swap-window` | `-d -s -t` | — | Functional |
| `move-window` | `-d -k -r -s -t` | `-a -b` | Functional |
| `rotate-window` | `-t -D -U` | — | Functional |
| `select-layout` | `-E -n -p -t layout-name` | `-o` | Functional |
| `next-layout` / `previous-layout` | (none) | — | Functional |
| `respawn-window` | `-k -t` | shell cmd | Functional |
| `link-window` | — | all | Stub (always errors) |
| `unlink-window` | — | all | Stub (always errors) |

### Pane Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `split-window` | `-h -v -d -l -p -t -c` | `-b -e -f -F -I -P -Z` | Functional |
| `select-pane` | `-D -d -e -L -l -M -m -R -T -U -Z -t` | `-g -P` | Functional |
| `kill-pane` | `-a -t` | — | Functional |
| `list-panes` | `-a -s -t` | `-F -f` | Functional |
| `capture-pane` | `-b -e -E -J -p -q -S -t` | `-a -C -M -N -P -T` | Functional |
| `resize-pane` | `-D -L -R -U -x -y -Z` | `-M -T` | Functional |
| `swap-pane` | `-D -s -U -t -Z` | `-d` | Functional |
| `break-pane` | `-d -t` | `-a -b -F -n -P -s` | Functional |
| `join-pane` | `-d -h -s -t -v` | `-b -f -l -p` | Functional |
| `last-pane` | `-t -Z` | `-d -e` | Functional |
| `respawn-pane` | `-k -t` | shell cmd | Functional |

### Server / Control Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `send-keys` | `-H -l -N -R -t -X` | `-c -F -K -M` | Functional |
| `bind-key` | `-n -N -r -T` | — | Complete |
| `unbind-key` | `-a -n -q -T` | — | Functional |
| `source-file` | `-F -q`, glob, multiple paths | `-n -t -v` | Functional |
| `run-shell` | `-b` (positional) | `-C -d -E -s -t -c` | Functional (sync + background) |
| `command-prompt` | `-I -p` (template) | `-1 -b -e -F -i -k -l -N -T` | Functional (rename works) |
| `if-shell` | `-b -F -t` | — | Complete |
| `confirm-before` | `-p -t` | `-b -c -y` | Functional (y/n prompt via command-prompt) |
| `send-prefix` | `-2 -t` | — | Complete |
| `clear-history` | `-H -t` | — | Functional |
| `wait-for` | (none) | `-L -S -U` | No-op stub |

### Option Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `set-option` / `set` | `-a -F -g -o -q -t -u -w` | `-p -s -U` | Well-implemented |
| `show-options` / `show` | `-g -q -w -t` | `-A -H -p -s -v` | Functional |
| `set-window-option` / `setw` | delegates to set-option `-w` | same gaps | Functional |
| `show-window-options` | delegates to show-options `-w` | same gaps | Functional |

### Display Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `display-message` | `-a -c -d -l -p -v` | `-b -F` | Functional |
| `list-keys` | `-N -T` | `-1 -P -a` | Functional (filters by table) |
| `display-panes` | `-t` | `-b -d` | Text output, not overlay |
| `clock-mode` | `-t` | — | Functional (text output) |
| `choose-tree` | `-s` | `-F -f -G -K -N -O -r -t -w -Z` | Functional |
| `choose-buffer` / `choose-client` | (none) | all filter/format flags | Functional |
| `display-menu` | `-t -T -x -y` | `-b -c -H -O -s -S` | Functional |
| `display-popup` | `-B -C -E -h -T -t -w -x -y` | `-c -d -e -K -s -S` | Functional |
| `pipe-pane` | `-o -t` (`-o` parsed not checked) | `-I` | Partial |
| `resize-window` | `-A -D -L -R -U -t -x -y` | — | Functional |

### Environment Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `set-environment` | `-g -u -t` | `-F -h -r` | Functional |
| `show-environment` | `-g -t` | `-h -s` | Functional |

### Paste Buffer Commands

| Command | Flags Implemented | Flags Missing | Status |
|---|---|---|---|
| `copy-mode` | `-e -q -u` | `-d -H -M -S -s -t` | Functional |
| `paste-buffer` | `-b -d -p -r -s` | `-t` | Functional |
| `set-buffer` | `-a -b` | `-n -t -w` | Functional |
| `delete-buffer` | `-b` | — | Complete |
| `save-buffer` / `load-buffer` | `-a -b` | `-t -w` | Functional |
| `show-buffer` | `-b` | — | Complete |
| `list-buffers` | (none) | `-F -f` | Functional |

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
| `buffer-limit` | [x] | Yes (50) | |
| `default-terminal` | [x] | Yes (`"screen"`, matches `TMUX_TERM`) | |
| `escape-time` | [x] | Yes (10) | |
| `exit-empty` | [x] | Yes | |
| `exit-unattached` | [x] | Yes | |
| `focus-events` | [x] | Yes | |
| `history-limit` | — | Moved to session scope (correct) | |
| `message-limit` | [x] | Yes (1000) | |
| `prefix-timeout` | [x] | Yes (0) | |
| `set-clipboard` | [x] | Yes (`"external"`) | |
| `backspace` | [ ] | — | |
| `command-alias` | [ ] | — | Array of command aliases |
| `copy-command` | [ ] | — | |
| `default-client-command` | [ ] | — | |
| `editor` | [ ] | — | |
| `extended-keys` | [ ] | — | |
| `history-file` | [ ] | — | |
| `input-buffer-size` | [ ] | — | |
| `prompt-history-limit` | [ ] | — | |
| `terminal-features` | [ ] | — | Feature detection for terminals |
| `terminal-overrides` | [~] | Missing default `linux*:AX@` | |
| `user-keys` | [ ] | — | |

### Session Options

| Option | rmux | Default Correct? | Notes |
|---|---|---|---|
| `activity-action` | [x] | Yes | |
| `base-index` | [x] | Yes (0) | |
| `bell-action` | [x] | Yes | |
| `default-command` | [x] | Yes | |
| `default-shell` | [x] | Yes | |
| `default-size` | [x] | Yes | |
| `destroy-unattached` | [~] | Value OK but tmux has 4-choice enum (`off/on/keep-last/keep-group`) | |
| `detach-on-destroy` | [x] | Yes | |
| `display-time` | [x] | Yes (750) | |
| `key-table` | [x] | Yes | |
| `lock-after-time` | [x] | Yes (0) | |
| `lock-command` | [x] | Yes | |
| `message-command-style` | [x] | Yes | |
| `message-style` | [x] | Yes | |
| `mouse` | [x] | Yes | |
| `prefix` | [x] | Yes (`C-b`) | |
| `prefix2` | [~] | Stores `"None"` string instead of KEYC_NONE | |
| `renumber-windows` | [x] | Yes | |
| `repeat-time` | [x] | Yes (500) | |
| `set-titles` | [x] | Yes | |
| `set-titles-string` | [x] | Yes | |
| `silence-action` | [x] | Yes (`"other"`) | |
| `status` | [x] | Yes | |
| `status-interval` | [x] | Yes (15) | |
| `status-justify` | [x] | Yes | |
| `status-keys` | [x] | Yes | |
| `status-left` | [x] | Yes | |
| `status-left-length` | [x] | Yes (10) | |
| `status-left-style` | [x] | Yes | |
| `status-position` | [x] | Yes | |
| `status-right` | [~] | Missing `#{?window_bigger,...}` prefix | |
| `status-right-length` | [x] | Yes (40) | |
| `status-right-style` | [x] | Yes | |
| `status-style` | [x] | Yes | |
| `visual-activity` | [x] | Yes | |
| `visual-bell` | [x] | Yes | |
| `visual-silence` | [x] | Yes | |
| `word-separators` | [x] | Yes (full punctuation set) | |
| `assume-paste-time` | [ ] | — | |
| `display-panes-active-colour` | [ ] | — | |
| `display-panes-colour` | [ ] | — | |
| `display-panes-time` | [ ] | — | |
| `focus-follows-mouse` | [ ] | — | |
| `status-format` | [ ] | — | Complex array format |
| `update-environment` | [ ] | — | |

### Window Options

| Option | rmux | Default Correct? | Notes |
|---|---|---|---|
| `aggressive-resize` | [x] | Yes | |
| `alternate-screen` | [x] | Yes | |
| `automatic-rename` | [x] | Yes | |
| `copy-mode-current-match-style` | [x] | Yes | |
| `copy-mode-mark-style` | [x] | Yes | |
| `copy-mode-match-style` | [x] | Yes | |
| `mode-keys` | [x] | Yes | |
| `monitor-activity` | [x] | Yes | |
| `monitor-bell` | [x] | Yes | |
| `monitor-silence` | [x] | Yes (0) | |
| `pane-base-index` | [x] | Yes (0) | |
| `pane-border-format` | [x] | Yes | |
| `pane-border-status` | [x] | Yes | |
| `pane-border-style` | [x] | Yes | |
| `synchronize-panes` | [x] | Yes | |
| `window-active-style` | [x] | Yes | |
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
| `allow-passthrough` | [~] | tmux has 3-choice enum (`off/on/all`), rmux uses flag | |
| `allow-rename` | [x] | Yes (`false`) | |
| `main-pane-height` | [~] | Value OK but tmux type is string (allows `%`) | |
| `main-pane-width` | [~] | Same issue | |
| `pane-active-border-style` | [~] | rmux: `"fg=green"`, tmux: conditional format | |
| `remain-on-exit` | [~] | tmux has 3-choice (`off/on/failed`) | |
| `automatic-rename-format` | [ ] | — | |
| `clock-mode-colour` | [ ] | — | |
| `clock-mode-style` | [ ] | — | |
| `fill-character` | [ ] | — | |
| `mode-style` | [ ] | — | |
| `pane-border-lines` | [ ] | — | |
| `popup-border-lines` | [ ] | — | |
| `popup-border-style` | [ ] | — | |
| `popup-style` | [ ] | — | |
| `scroll-on-clear` | [ ] | — | |
| `window-size` | [ ] | — | |

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
- [ ] `#{q:expr}` shell quoting
- [ ] `#{n:expr}` string length
- [ ] `#{w:expr}` display width
- [ ] `#{a:expr}` ASCII code to character
- [ ] `#{p:N:expr}` padding
- [ ] `#{!expr}` logical NOT
- [ ] `#{||:a,b}` / `#{&&:a,b}` logical OR/AND
- [ ] `#{e|op:a,b}` arithmetic (+,-,*,/,%)
- [ ] `#{m:pattern,string}` fnmatch/regex match

### Conditionals & Comparisons
- [x] `#{?condition,true,false}` ternary
- [x] `#{==:a,b}`, `#{!=:a,b}`, `#{<:a,b}`, `#{>:a,b}`, `#{<=:a,b}`, `#{>=:a,b}`
- [ ] Multi-branch `#{?c1,v1,c2,v2,...,default}`

### Loops
- [ ] `#{S:format}` sessions
- [ ] `#{W:format}` windows
- [ ] `#{P:format}` panes

### Inline Styles
- [x] `#[fg=color,bg=color,attrs]`

---

## 4. Format Variables

### Implemented

**Session:** `session_name`, `session_id`, `session_windows`, `session_attached`, `session_created`, `session_activity`, `session_alerts`

**Window:** `window_index`, `window_name`, `window_id`, `window_flags`, `window_active`, `window_panes`, `window_layout`

**Pane:** `pane_id`, `pane_index`, `pane_title`, `pane_width`, `pane_height`, `pane_active`, `pane_dead`, `pane_current_command`, `pane_current_path`, `pane_pid`, `pane_tty`, `pane_start_command`, `pane_in_mode`, `pane_synchronized`

**Client:** `client_name` (stub), `client_tty` (stub), `client_prefix`, `client_width`, `client_height`, `client_activity`, `client_session`

**Terminal state:** `cursor_x`, `cursor_y`, `cursor_flag`, `insert_flag`, `keypad_flag`, `alternate_on`

**System:** `host`, `host_short`, `version`, `current_file`

### Missing (commonly used)

| Variable | Why it matters |
|---|---|
| `pid` | Scripts check server PID |
| `socket_path` | Plugin path resolution |
| `session_path` | Status bar configs |
| `window_zoomed_flag` | Status bar zoom indicator |
| `window_last_flag` | Status bar last-window marker |
| `window_activity_flag` | Alert styling |
| `window_bell_flag` | Alert styling |
| `window_silence_flag` | Alert styling |
| `window_bigger` | Used in tmux's own default `status-right` |
| `pane_at_top/bottom/left/right` | Navigation config |
| `pane_dead_status` | Exit code display |
| `client_pid` | Plugin process management |
| `client_key_table` | Key table indicator in status |
| `client_termname` | Terminal detection |
| `history_size` / `history_limit` | Scrollback display |
| `buffer_name` / `buffer_size` | Paste buffer display |
| `mouse_x` / `mouse_y` | Mouse tracking |

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

### Missing for Full Plugin Support
- [ ] `run-shell -b` (background execution)
- [ ] `send-keys -X` (copy-mode command dispatch)
- [ ] `command-prompt -I/-p` (initial text, custom prompts)
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
