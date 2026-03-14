# tmux Config System Parity

Tracks rmux configuration system completeness relative to tmux next-3.7.
Features required by the user's tmux.conf + catppuccin theme are marked with **[CONF]**.

Legend: `[x]` = implemented, `[ ]` = missing, `[~]` = partial

---

## 1. Config File Parsing

### Basic Syntax
- [x] Comment lines (`# ...`)
- [x] Inline comments after commands
- [x] Double-quoted strings with escape sequences (`\"`, `\\`, `\n`, `\t`)
- [x] Single-quoted strings (literal, no expansion)
- [x] Empty quoted strings preserved (`""`, `''`)
- [x] Semicolon command separator (`;`)
- [x] Escaped semicolons (`\;` in bind multi-commands) **[CONF]**
- [ ] Line continuation (backslash at end of line) **[CONF]**
- [ ] Tilde expansion (`~` → `$HOME`, `~user` → home dir) **[CONF]**

### Conditional Directives
- [ ] `%if <expression>` — conditional block **[CONF]**
- [ ] `%elif <expression>` — else-if branch **[CONF]**
- [ ] `%else` — else branch **[CONF]**
- [ ] `%endif` — end conditional **[CONF]**
- [ ] `%hidden NAME=VALUE` — hidden environment variable **[CONF]**

### Variable Interpolation
- [ ] `${VAR}` — environment variable substitution in values **[CONF]**
- [ ] `${VAR}` in option names (catppuccin uses `@catppuccin_${MODULE_NAME}_color`)

### Backslash Escapes (in tokens)
- [x] `\\` → backslash
- [x] `\"` → double quote (in double-quoted strings)
- [ ] `\a`, `\b`, `\e`, `\f`, `\r`, `\s`, `\v` — control characters
- [ ] `\uNNNN` — Unicode (UCS-2)
- [ ] `\UNNNNNNNN` — Unicode (UCS-4)
- [ ] `\NNN` — octal escape

---

## 2. set-option / set Command

### Scope Flags
- [x] `-g` — global (server-level) scope **[CONF]**
- [x] `-w` — window scope (also via `setw` alias)
- [x] `-s` — server scope
- [ ] `-p` — pane scope
- [x] `-t target` — target session/window

### Behavior Flags
- [x] `-o` — only set if not already set (fail if exists) **[CONF]**
- [x] `-q` — quiet mode, suppress all errors **[CONF]**
- [x] `-u` — unset option (revert to parent/default) **[CONF]**
- [ ] `-U` — unset in all panes (with `-w`)
- [x] `-a` — append to existing string value **[CONF]**
- [x] `-F` — format-expand value before setting **[CONF]**

### Combined Flag Support
- [x] `-gF`, `-ag`, `-sg` etc. parsed via `has_flag` **[CONF]**
- [x] `-ogq` — combined only/quiet/global **[CONF]**
- [x] `-ogqF` — combined only/quiet/global/format **[CONF]**
- [x] `-agF` — combined append/global/format **[CONF]**
- [x] `-wgF` — combined window/global/format **[CONF]**
- [x] `-ug` — combined unset/global **[CONF]**

### User Options
- [x] `@name` prefix stored as string options **[CONF]**
- [x] User options accessible in format strings as `#{@name}` **[CONF]**

### Style Aliases
- [x] `status-bg X` → `status-style bg=X`
- [x] `status-fg X` → `status-style fg=X`

---

## 3. source-file / source Command

- [x] Basic file loading and command execution **[CONF]**
- [x] Error reporting (non-fatal, continues on error)
- [ ] `-F` flag — format-expand the file path **[CONF]**
- [ ] `-q` flag — suppress "file not found" errors
- [ ] `-n` flag — parse only, don't execute
- [ ] `-v` flag — verbose, show each command
- [ ] Glob patterns (`source-file ~/.config/tmux/conf.d/*.conf`)
- [ ] Depth limiting (tmux: 50 levels max)
- [ ] `current_file` variable set during source (for `#{d:current_file}`) **[CONF]**

---

## 4. Format String Expansion

### Variable References
- [x] `#{variable_name}` — lookup from format context
- [x] Short aliases: `#S`, `#W`, `#I`, `#T`, `#F`, `#D`, `#H`, `#h`, `#P`
- [ ] `#{@user_option}` — user option lookup **[CONF]**
- [x] `##` — literal `#`

### Modifiers
- [x] `#{E:expr}` — double expansion (expand value, then expand result) **[CONF]**
- [x] `#{T:expr}` — strftime expansion
- [x] `#{l:text}` — literal (no expansion)
- [ ] `#{d:variable}` — dirname (directory of path) **[CONF]**
- [ ] `#{b:variable}` — basename (filename of path)
- [ ] `#{q:expr}` — shell quoting
- [ ] `#{n:expr}` — string length
- [ ] `#{w:expr}` — display width
- [ ] `#{a:expr}` — ASCII code to character
- [ ] `#{c:expr}` — color name to RGB
- [ ] `#{!expr}` — logical NOT
- [ ] `#{||:a,b}` — logical OR
- [ ] `#{&&:a,b}` — logical AND
- [ ] `#{R:N:expr}` — repeat N times

### Conditionals
- [x] `#{?condition,true,false}` — ternary conditional **[CONF]**
- [ ] `#{?cond1,val1,cond2,val2,...,default}` — multi-branch conditional **[CONF]**

### Comparisons
- [x] `#{==:a,b}` — string equality **[CONF]**
- [x] `#{!=:a,b}` — string inequality **[CONF]**
- [x] `#{<:a,b}` — string less-than
- [x] `#{>:a,b}` — string greater-than
- [x] `#{<=:a,b}` — string less-or-equal
- [x] `#{>=:a,b}` — string greater-or-equal **[CONF]**
- [ ] `#{m:pattern,string}` — fnmatch/regex match
- [ ] `#{m/ri:pattern,string}` — regex match with flags

### Truncation & Padding
- [x] `#{=N:expr}` — truncate to N chars (positive=left, negative=right)
- [ ] `#{p:N:expr}` — pad to N chars
- [ ] `#{L:N:expr}` — left-truncate with marker

### Substitution
- [x] `#{s/pattern/replacement:expr}` — string substitution
- [ ] `#{e:op:args}` — expression evaluation (+,-,*,/,%)

### Loops
- [ ] `#{S:format}` — loop over sessions
- [ ] `#{W:format}` — loop over windows
- [ ] `#{P:format}` — loop over panes

### Inline Styles
- [x] `#[fg=color,bg=color,attrs]` — inline style changes

---

## 5. Format Variables

### Special Variables
- [ ] `current_file` — path of config file being sourced **[CONF]**
- [ ] `version` — tmux/rmux version string **[CONF]**
- [x] `host` / `host_short` — hostname
- [ ] `pid` — server process ID

### Client Variables
- [x] `client_name`, `client_tty`, `client_session`
- [x] `client_width`, `client_height`
- [x] `client_activity`
- [ ] `client_prefix` — whether prefix key is active **[CONF]**
- [ ] `client_control_mode`, `client_created`
- [ ] `client_flags`, `client_key_table`
- [ ] `client_pid`, `client_termname`, `client_utf8`

### Session Variables
- [x] `session_name`, `session_id`, `session_windows`
- [x] `session_attached`, `session_created`, `session_activity`
- [x] `session_alerts`
- [ ] `session_path`, `session_group`, `session_format`

### Window Variables
- [x] `window_name`, `window_index`, `window_id`, `window_active`
- [x] `window_flags`, `window_panes`, `window_layout`
- [ ] `window_activity`, `window_activity_flag`, `window_bell_flag`
- [ ] `window_last_flag`, `window_zoomed_flag` **[CONF]**
- [ ] `window_marked_flag`, `window_silence_flag`
- [ ] `window_bigger`, `window_cell_height`, `window_cell_width`

### Pane Variables
- [x] `pane_id`, `pane_index`, `pane_title`, `pane_active`
- [x] `pane_width`, `pane_height`, `pane_dead`
- [x] `pane_current_command`, `pane_current_path`, `pane_pid`
- [x] `pane_in_mode`, `pane_tty`, `pane_start_command`
- [ ] `pane_synchronized` **[CONF]**
- [ ] `pane_at_top`, `pane_at_bottom`, `pane_at_left`, `pane_at_right`

---

## 6. Option Scopes & Inheritance

### Scope Hierarchy
- [x] Server (global) options
- [x] Session options with parent inheritance from server **[CONF]**
- [x] Window options (default set)
- [ ] Window options inherit from session
- [ ] Pane options inherit from window

### Option Types
- [x] String options
- [x] Number options (auto-detected from value)
- [x] Flag/boolean options (`on`/`off`)
- [ ] Choice options (validated enum)
- [ ] Colour options (validated color)
- [ ] Array options (indexed with `option[N]`)

---

## 7. Plugin System Compatibility (TPM)

### Implemented
- [x] `run-shell` command (async, pauses config queue) **[CONF]**
- [x] tmux → rmux shim (symlink for plugin compatibility) **[CONF]**
- [x] `$TMUX` env var set for run-shell processes **[CONF]**
- [x] Config loading inside event loop (plugins can connect back) **[CONF]**
- [x] Exit-empty guard during config loading **[CONF]**
- [x] Version string compatibility (`rmux 3.6.0`, digits extractable) **[CONF]**

### Missing for Full Plugin Support
- [ ] `source` as `source-file` alias (prefix matching may cover this)
- [x] All `set-option` flags needed by catppuccin (`-ogqF`, `-agF`, `-wgF`, `-ug`)
- [ ] Conditional directives (`%if`/`%elif`/`%else`/`%endif`)
- [ ] Format expansion in `source-file -F` paths
- [ ] `#{d:current_file}` for relative path resolution in plugins
- [x] `#{@user_option}` format references
- [ ] `${VAR}` interpolation in option names

---

## 8. Catppuccin Theme — Required Features

The catppuccin/tmux plugin chain requires these specific features to load:

### Phase 1: catppuccin.tmux (bash script via run-shell)
Calls `tmux source <path>` twice. Requires:
- [x] `run-shell` executing bash scripts
- [x] tmux shim resolving to rmux
- [ ] `source` command lookup (prefix match to `source-file`)

### Phase 2: catppuccin_options_tmux.conf
Sets default options with `set -ogq @name "value"`. Requires:
- [x] `-o` flag (only set if not already set)
- [x] `-q` flag (suppress errors)
- [x] Combined `-ogq` flag parsing
- [x] `@name` user option storage

### Phase 3: catppuccin_mocha_tmux.conf (theme colors)
Sets theme colors with `set -ogq @thm_bg "#1e1e2e"`. Requires:
- [x] Same `-ogq` as Phase 2

### Phase 4: catppuccin_tmux.conf (main config)
Heavy use of advanced features. Requires:
- [ ] `source -F "#{d:current_file}/themes/..."` — format-expanded source path
- [ ] `#{d:current_file}` — directory of current config file
- [ ] `#{@catppuccin_flavor}` — user option in format strings
- [ ] `%if "#{==:#{@option},value}"` — conditional directives
- [ ] `%elif`, `%else`, `%endif`
- [ ] `%hidden VAR="value"` — hidden variables
- [x] `set -gF status-style "bg=#{@thm_mantle}"` — format expansion at set-time
- [x] `set -wgF` — window-global with format
- [x] `set -agF` — append-global with format
- [x] `set -ug` — unset option
- [x] `#{E:@option}` expanding user options (already implemented for known vars)
- [x] `#{?condition,...}` with `#{@user_option}` references
- [ ] `#{version}` variable (for `#{>=:#{version},3.4}`)

### Phase 5: status/session.conf (status module)
Builds the `@catppuccin_status_session` format string. Requires:
- [ ] `%hidden MODULE_NAME="session"` — hidden variable
- [ ] `${MODULE_NAME}` interpolation in option names
- [ ] `source -F "#{d:current_file}/../utils/..."` — relative path resolution
- [x] `set -ogq` / `set -gF` / `set -agF` as above
- [ ] `#{?client_prefix,...}` — client_prefix variable

### Phase 6: User's tmux.conf (post-TPM lines)
```
set -g status-left ""
set -gF status-right "#{E:@catppuccin_status_session}"
```
Requires:
- [x] `-F` flag expanding `#{E:@catppuccin_status_session}` at set-time
- [x] `#{E:@option}` double-expanding user option values
- [x] `#{@option}` in nested format strings

---

## 9. Implementation Priority

Ordered by what unblocks the most functionality:

### P0 — Core config flags (unblocks all plugins) ✅ DONE
1. ~~`set -q` flag (quiet, suppress errors)~~
2. ~~`set -o` flag (only if not set)~~
3. ~~`set -u` flag (unset option)~~
4. ~~`set -a` flag (append to string)~~
5. ~~`set -F` flag (format-expand value at set-time)~~
6. ~~Combined flags: `-ogq`, `-ogqF`, `-agF`, `-wgF`, `-ug`~~
7. ~~`#{@user_option}` in format expansion~~

### P1 — Config directives (unblocks catppuccin)
8. `%if` / `%elif` / `%else` / `%endif` conditional processing
9. `%hidden` variable assignment
10. `${VAR}` interpolation in option names and values

### P2 — Source file features (unblocks catppuccin path resolution)
11. `source -F` flag (format-expand file path)
12. `current_file` variable tracking during source
13. `#{d:path}` dirname modifier
14. Line continuation (backslash at end of line)

### P3 — Format variables (unblocks catppuccin conditionals)
15. `version` variable
16. `client_prefix` variable
17. `window_zoomed_flag`, `window_last_flag` etc.
18. `pane_synchronized` variable
19. Tilde expansion (`~` in paths)

### P4 — Advanced format features
20. `#{b:path}` basename modifier
21. `#{q:expr}` shell quoting
22. Multi-branch conditionals `#{?c1,v1,c2,v2,...,default}`
23. `#{m:pattern,string}` matching
24. Loop expansions (`#{S:}`, `#{W:}`, `#{P:}`)

---

## 10. Reference

### tmux Source Files
- `cfg.c` — config file loading, `current_file` tracking
- `cmd-parse.y` — parser with `%if`/`%hidden`, `${VAR}`, quoting rules
- `cmd-set-option.c` — set-option with all flags (-g/-s/-w/-p/-o/-q/-u/-U/-a/-F)
- `cmd-source-file.c` — source-file with -F/-n/-q/-v, glob, depth limit
- `format.c` — format string engine (5000+ lines), all modifiers and variables
- `options.c` / `options-table.c` — option types, scopes, inheritance

### rmux Source Files
- `crates/rmux-server/src/config.rs` — config parser
- `crates/rmux-server/src/command/builtins/options.rs` — set-option command
- `crates/rmux-server/src/command/builtins/server_cmds.rs` — source-file, bind-key
- `crates/rmux-server/src/command/mod.rs` — flag parsing, CommandServer trait
- `crates/rmux-server/src/format.rs` — format string expansion
- `crates/rmux-core/src/options.rs` — Options struct with parent inheritance
- `crates/rmux-server/src/server.rs` — option storage, config queue, event loop
