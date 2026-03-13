#!/usr/bin/env bash
# e2e-test.sh — E2E tests for rmux using the tmux-based test harness.
#
# Usage:
#   bash scripts/e2e-test.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=test-harness.sh
source "${SCRIPT_DIR}/test-harness.sh"

# --- Test framework -----------------------------------------------------------

TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

run_test() {
    local name="$1"
    TESTS_RUN=$((TESTS_RUN + 1))
    echo "--- TEST: ${name} ---"
    if "${name}"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        echo "--- PASS: ${name} ---"
        echo
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        echo "--- FAIL: ${name} ---"
        echo
    fi
    # Always stop between tests for isolation
    harness_stop 2>/dev/null || true
}

# --- Smoke tests --------------------------------------------------------------

test_session_starts() {
    harness_start
    harness_assert '\[0\]' "Status bar should show session name [0]"
    harness_assert '0:.*\*' "Status bar should show window 0 as active (*)"
}

test_shell_command() {
    harness_start
    harness_send "echo RMUX_TEST_OUTPUT" Enter
    harness_wait_for "RMUX_TEST_OUTPUT" 5
    harness_assert "RMUX_TEST_OUTPUT" "Shell command output should appear on screen"
}

test_new_window() {
    harness_start
    # C-b c creates a new window in rmux
    harness_prefix c
    harness_wait_for '1:.*\*' 5
    harness_assert '1:.*\*' "Status bar should show window 1 as active after C-b c"
}

test_window_switching() {
    harness_start
    # Create a second window
    harness_prefix c
    harness_wait_for '1:.*\*' 5

    # Switch back to window 0
    harness_prefix 0
    harness_wait_for '0:.*\*' 5
    harness_assert '0:.*\*' "Window 0 should be active after C-b 0"

    # Switch to window 1
    harness_prefix 1
    harness_wait_for '1:.*\*' 5
    harness_assert '1:.*\*' "Window 1 should be active after C-b 1"
}

test_detach() {
    harness_start
    # C-b d detaches from rmux
    harness_prefix d
    # After detach, rmux exits and tmux shows the shell (or our wrapper message)
    harness_wait_for '\[rmux exited\]' 5
    harness_assert '\[rmux exited\]' "Should see exit message after C-b d detach"
}

# --- Tier 1: Core full-stack --------------------------------------------------

test_split_vertical() {
    harness_start
    # C-b " splits vertically (top/bottom) — draws horizontal border ─
    harness_prefix '"'
    sleep 0.5
    harness_assert '2 panes' "Status bar should show (2 panes) after split"
    harness_assert '─' "Horizontal border should be visible after vertical split"
}

test_split_horizontal() {
    harness_start
    # C-b % splits horizontally (left/right) — draws vertical border │
    harness_prefix %
    sleep 0.5
    harness_assert '2 panes' "Status bar should show (2 panes) after split"
    harness_assert '│' "Vertical border should be visible after horizontal split"
}

test_pane_navigation() {
    harness_start
    # Split to get two panes
    harness_prefix '"'
    sleep 0.5
    harness_assert '2 panes' "Should have 2 panes after split"

    # Run a unique marker in the second (now active) pane
    harness_send "echo PANE_TWO_MARKER" Enter
    harness_wait_for "PANE_TWO_MARKER" 5

    # C-b o cycles to next pane (back to first)
    harness_prefix o
    sleep 0.3

    # Run a different marker in the first pane
    harness_send "echo PANE_ONE_MARKER" Enter
    harness_wait_for "PANE_ONE_MARKER" 5
    harness_assert "PANE_ONE_MARKER" "First pane should show PANE_ONE_MARKER after C-b o"
}

test_copy_mode() {
    harness_start
    # C-b [ enters copy mode
    harness_prefix '['
    harness_wait_for 'Copy mode' 5
    harness_assert 'Copy mode' "Status bar should show [Copy mode after C-b ["

    # C-g exits copy mode in emacs mode (the default)
    harness_send C-g
    sleep 0.5

    # Poll for copy mode to disappear (may need a redraw tick)
    local elapsed=0
    while (( $(echo "${elapsed} < 3" | bc -l) )); do
        if ! harness_capture 2>/dev/null | grep -qE 'Copy mode'; then
            break
        fi
        sleep 0.2
        elapsed=$(echo "${elapsed} + 0.2" | bc -l)
    done

    harness_assert_not 'Copy mode' "Copy mode indicator should disappear after C-g"
}

test_command_prompt_cancel() {
    harness_start
    # C-b : opens command prompt
    harness_prefix ':'
    sleep 0.3

    # The last line should show the : prompt (replaces status bar)
    harness_assert ':' "Command prompt ':' should appear on screen"

    # Escape cancels the prompt and returns to the status bar
    harness_send Escape
    sleep 0.3

    harness_wait_for '\[0\]' 5
    harness_assert '\[0\]' "Status bar should return after prompt is cancelled"
}

test_command_prompt_execute() {
    harness_start
    # C-b : opens command prompt, then type a command and press Enter
    harness_prefix ':'
    sleep 0.3
    harness_assert ':' "Command prompt ':' should appear"

    # Type "new-window" and press Enter to create a new window
    harness_send "new-window" Enter
    harness_wait_for '1:.*\*' 5
    harness_assert '1:.*\*' "new-window via command prompt should create window 1"
}

test_command_prompt_backspace() {
    harness_start
    harness_prefix ':'
    sleep 0.3

    # Type "xxx", backspace 3 times, then type the real command
    harness_send "xxx" BSpace BSpace BSpace "new-window" Enter
    harness_wait_for '1:.*\*' 5
    harness_assert '1:.*\*' "Backspace in prompt should erase chars before executing"
}

test_non_attached_list_sessions() {
    harness_start
    # Use harness_rmux to run a non-attached command
    local output
    output=$(harness_rmux list-sessions 2>&1) || true
    echo "list-sessions output: ${output}"

    # Format: "0: 1 windows (attached)"
    if ! echo "${output}" | grep -qE '0:.*windows'; then
        _harness_fail "list-sessions should show '0: N windows'"
        return 1
    fi
}

test_non_attached_send_keys() {
    harness_start
    # Send keys to the running session's pane via non-attached client
    harness_rmux send-keys -t 0:0 "echo REMOTE_CMD" Enter
    harness_wait_for "REMOTE_CMD" 5
    harness_assert "REMOTE_CMD" "send-keys should deliver keystrokes to the pane"
}

test_reattach_after_detach() {
    harness_start
    # Detach
    harness_prefix d
    harness_wait_for '\[rmux exited\]' 5
    harness_assert '\[rmux exited\]' "Should see exit message after detach"

    # Compute the TMPDIR that rmux needs to find the test socket
    local rmux_tmpdir
    rmux_tmpdir="$(dirname "$(dirname "${HARNESS_RMUX_SOCKET}")")"

    # Reattach by running rmux again inside the tmux pane
    harness_send "TMPDIR=${rmux_tmpdir} PATH=${HARNESS_BINARY_DIR}:\${PATH} rmux" Enter
    harness_wait_for '\[0\]' 10
    harness_assert '\[0\]' "Status bar should reappear after reattach"
}

# --- Pane lifecycle -----------------------------------------------------------

test_pane_exit_closes_split() {
    harness_start
    # Split vertically
    harness_prefix '"'
    sleep 0.5
    harness_assert '2 panes' "Should have 2 panes after split"
    harness_assert '─' "Border should be visible after split"

    # Exit the active (bottom) pane's shell
    harness_send "exit" Enter
    sleep 1

    # Pane should be gone: no border, no "(2 panes)" indicator
    harness_wait_for '\[0\]' 5
    harness_assert_not '2 panes' "Pane count should disappear after exit"
    harness_assert_not '─' "Border should disappear after pane exits"
}

test_pane_exit_preserves_remaining() {
    harness_start
    # Put a marker in the first pane
    harness_send "export PS1='PANE1> '" Enter
    harness_wait_for 'PANE1>' 5

    # Split and exit the new pane immediately
    harness_prefix '"'
    sleep 0.5
    harness_assert '2 panes' "Should have 2 panes"
    harness_send "exit" Enter
    sleep 1

    # The original pane should still be functional
    harness_wait_for 'PANE1>' 5
    harness_send "echo STILL_ALIVE" Enter
    harness_wait_for "STILL_ALIVE" 5
    harness_assert "STILL_ALIVE" "Original pane should still work after sibling exits"
}

test_last_pane_exit_closes_window() {
    harness_start
    # Create a second window so the session survives
    harness_prefix c
    harness_wait_for '1:.*\*' 5

    # Exit the shell in window 1 — should switch back to window 0
    harness_send "exit" Enter
    sleep 1
    # Window 1 should be gone, window 0 should be active
    harness_wait_for '0:.*\*' 5
    harness_assert_not '1:' "Window 1 should be gone after its last pane exits"
}

# --- Status bar & auto-rename -------------------------------------------------

test_status_bar_strftime() {
    harness_start
    sleep 0.5
    # The default status-right includes %H:%M which should expand to HH:MM
    # Verify no literal %H or %M appears in the status bar
    harness_assert_not '%H' "Status bar should not show literal %H"
    harness_assert_not '%M' "Status bar should not show literal %M"
    harness_assert_not '%d' "Status bar should not show literal %d"
    harness_assert_not '%b' "Status bar should not show literal %b"
    harness_assert_not '%y' "Status bar should not show literal %y"
}

test_automatic_rename() {
    harness_start
    sleep 0.5
    # Run cat (blocks forever), window name should change to "cat"
    harness_send "cat" Enter
    sleep 1
    harness_assert '0:cat' "Window name should auto-rename to 'cat'"

    # Kill cat, name should revert to shell
    harness_send "" "C-c"
    sleep 1
    harness_assert_not '0:cat' "Window name should revert after cat exits"
}

test_automatic_rename_disabled() {
    harness_start
    harness_rmux set-option automatic-rename off
    sleep 0.3
    # Run cat, window name should NOT change
    harness_send "cat" Enter
    sleep 1
    harness_assert_not '0:cat' "Window name should not change when automatic-rename is off"
    harness_send "" "C-c"
    sleep 0.5
}

# --- Tier 2: Operations -------------------------------------------------------

test_rename_window() {
    harness_start
    harness_rmux rename-window -t 0:0 mywin
    sleep 0.3
    harness_wait_for '0:mywin' 5
    harness_assert '0:mywin\*' "Status bar should show renamed window '0:mywin*'"
}

test_pane_resize() {
    harness_start
    # Split vertically (top/bottom) to get a border we can track
    harness_prefix '"'
    sleep 0.5
    harness_assert '2 panes' "Should have 2 panes"

    # Resize upward — top pane has room to shrink
    local output
    output=$(harness_rmux resize-pane -t 0:0 -U 5 2>&1) || true

    if echo "${output}" | grep -qi 'error'; then
        _harness_fail "resize-pane should not error: ${output}"
        return 1
    fi

    sleep 0.3
    # Verify panes still exist after resize command
    harness_assert '2 panes' "Panes should still be intact after resize"
    harness_assert '─' "Border should still be visible after resize"
}

test_multiple_sessions() {
    harness_start
    # Create a second detached session
    harness_rmux new-session -d -s second
    sleep 0.3

    # Verify both sessions appear
    local output
    output=$(harness_rmux list-sessions 2>&1)
    echo "list-sessions output: ${output}"

    if ! echo "${output}" | grep -qE '0:.*windows'; then
        _harness_fail "First session '0' should appear in list-sessions"
        return 1
    fi
    if ! echo "${output}" | grep -qE 'second:.*windows'; then
        _harness_fail "Second session 'second' should appear in list-sessions"
        return 1
    fi
}

test_startup_config_loading() {
    # Write a config that sets a distinctive server option via the default path.
    # We place it at ~/.tmux.conf inside the harness so rmux finds it on startup.
    # To avoid clobbering the real ~/.tmux.conf, we use harness_start which sets
    # TMPDIR — but HOME is still the real home. Instead, we launch rmux with -f
    # by writing a wrapper script that passes the flag.
    local config_file="/tmp/rmux-e2e-startup-config-${HARNESS_PID}.conf"
    echo 'set-option -g history-limit 54321' > "${config_file}"

    # Create a wrapper script that launches rmux with -f
    local wrapper="/tmp/rmux-e2e-wrapper-${HARNESS_PID}.sh"
    cat > "${wrapper}" <<WRAPPER
#!/usr/bin/env bash
exec "\$(dirname "\$0")/rmux" -f "${config_file}" "\$@"
WRAPPER
    chmod +x "${wrapper}"
    # Symlink it into the binary dir so harness finds it
    cp "${wrapper}" "${HARNESS_BINARY_DIR}/rmux-with-config"

    # Use the harness, but override the rmux binary
    mkdir -p "${HARNESS_SOCKET_DIR}"
    local uid
    uid=$(id -u)
    local rmux_parent="${HARNESS_SOCKET_DIR}/rmux-parent"
    mkdir -p "${rmux_parent}/rmux-${uid}"
    local rmux_tmpdir="${rmux_parent}"
    HARNESS_RMUX_SOCKET="${rmux_parent}/rmux-${uid}/${HARNESS_SOCKET_NAME}"

    tmux new-session -d -s "${HARNESS_TMUX_SESSION}" -x 120 -y 40 \
        "TMPDIR=${rmux_tmpdir} PATH=${HARNESS_BINARY_DIR}:\${PATH} rmux-with-config; echo '[rmux exited]'; sleep 86400"
    tmux set-option -t "${HARNESS_TMUX_SESSION}" prefix C-q
    tmux unbind-key C-b 2>/dev/null || true
    HARNESS_STARTED=1

    if ! harness_wait_for '\[0\]' 10; then
        _harness_fail "rmux did not start"
        rm -f "${config_file}" "${wrapper}" "${HARNESS_BINARY_DIR}/rmux-with-config"
        return 1
    fi

    # Verify the -f config was applied at server level
    local output
    output=$(harness_rmux show-options -g history-limit 2>&1) || true
    echo "show-options output: ${output}"

    rm -f "${config_file}" "${wrapper}" "${HARNESS_BINARY_DIR}/rmux-with-config"

    if ! echo "${output}" | grep -q '54321'; then
        _harness_fail "startup config not applied: history-limit should be 54321, got: ${output}"
        return 1
    fi
}

test_source_file() {
    harness_start
    # Write a temp config file that sets an option
    local config_file="/tmp/rmux-e2e-config-${HARNESS_PID}.conf"
    echo 'set-option -g status-left "[CUSTOM] "' > "${config_file}"

    # Source the config
    harness_rmux source-file "${config_file}"
    sleep 0.3

    # Verify the option was set via show-options
    local output
    output=$(harness_rmux show-options -g status-left 2>&1) || true
    echo "show-options output: ${output}"

    if ! echo "${output}" | grep -q 'CUSTOM'; then
        _harness_fail "source-file should have set status-left option"
        rm -f "${config_file}"
        return 1
    fi

    rm -f "${config_file}"
}

# --- Tier 3: Advanced ---------------------------------------------------------

test_set_option() {
    harness_start
    # Set a server-level option
    harness_rmux set-option -g status-left "[TEST] "
    sleep 0.3

    # Verify with show-options
    local output
    output=$(harness_rmux show-options -g status-left 2>&1) || true
    echo "show-options output: ${output}"

    if ! echo "${output}" | grep -q 'TEST'; then
        _harness_fail "set-option should store the value (visible via show-options)"
        return 1
    fi
}

test_paste_buffer() {
    harness_start
    # Put some text in the paste buffer directly
    harness_rmux set-buffer "PASTE_TEST_DATA"
    sleep 0.2

    # Paste with C-b ]
    harness_prefix ']'
    harness_wait_for "PASTE_TEST_DATA" 5
    harness_assert "PASTE_TEST_DATA" "Pasted text should appear in the pane"
}

test_swap_window() {
    harness_start
    # Disable auto-rename at session level so manual names stick after swap
    harness_rmux set-option automatic-rename off
    sleep 0.3

    # Rename window 0 so we can track it
    harness_rmux rename-window -t 0:0 first
    harness_wait_for '0:first' 5

    # Create window 1
    harness_prefix c
    harness_wait_for '1:.*\*' 5
    harness_rmux rename-window -t 0:1 second
    harness_wait_for '1:second' 5

    # Swap windows 0 and 1 (use session-qualified targets for non-attached client)
    harness_rmux swap-window -s 0:0 -t 0:1
    sleep 0.5

    # After swap: window 0 should be "second", window 1 should be "first"
    harness_assert '0:second' "After swap, window 0 should have name 'second'"
    harness_assert '1:first' "After swap, window 1 should have name 'first'"
}

# --- Overlay tests ------------------------------------------------------------

test_choose_tree_open_close() {
    harness_start
    # C-b s opens choose-tree (session chooser)
    harness_prefix s
    harness_wait_for 'choose-tree' 5
    harness_assert 'choose-tree' "choose-tree overlay should appear"

    # Press q to cancel
    harness_send q
    sleep 0.5

    # Status bar should return
    harness_wait_for '\[0\]' 5
    harness_assert '\[0\]' "Status bar should return after closing choose-tree"
}

test_choose_tree_shows_sessions() {
    harness_start
    # Create a second session so the tree has content
    harness_rmux new-session -d -s "second"
    sleep 0.3

    # Open choose-tree
    harness_prefix s
    harness_wait_for 'choose-tree' 5

    # Both sessions should be visible
    harness_assert '0:' "Default session should be visible in tree"
    harness_assert 'second' "Second session should be visible in tree"

    # Cancel
    harness_send Escape
    harness_wait_for '\[0\]' 5
}

test_choose_tree_navigate_and_select() {
    harness_start
    # Create a second session
    harness_rmux new-session -d -s "target"
    sleep 0.3

    # Open choose-tree
    harness_prefix s
    harness_wait_for 'choose-tree' 5

    # Collapse all sessions first (Left on each), then navigate to "target"
    # Simpler: use Left to collapse session 0, then j lands on "target"
    harness_send Left
    sleep 0.2
    harness_send j
    sleep 0.2
    harness_send Enter
    sleep 0.5

    # We should now be in the "target" session (visible in status bar)
    harness_wait_for 'target' 5
    harness_assert 'target' "Should have switched to target session"
}

test_display_menu_key_shortcut() {
    harness_start
    # Open a custom menu via command prompt
    harness_prefix ':'
    sleep 0.3
    harness_send "display-menu -T Test New c new-window Kill k kill-window" Enter
    harness_wait_for 'Test' 5
    harness_assert 'New' "Menu should show 'New' item"

    # Press 'c' to trigger New Window
    harness_send c
    sleep 0.5

    # Should have created a new window
    harness_wait_for '1:.*\*' 5
    harness_assert '1:.*\*' "New window should be created via menu key shortcut"
}

# --- Main ---------------------------------------------------------------------

echo "=== rmux E2E Test Suite ==="
echo

# Build first
harness_build

# Smoke tests
run_test test_session_starts
run_test test_shell_command
run_test test_new_window
run_test test_window_switching
run_test test_detach

# Tier 1: Core full-stack
run_test test_split_vertical
run_test test_split_horizontal
run_test test_pane_navigation
run_test test_copy_mode
run_test test_command_prompt_cancel
run_test test_command_prompt_execute
run_test test_command_prompt_backspace
run_test test_non_attached_list_sessions
run_test test_non_attached_send_keys
run_test test_reattach_after_detach

# Pane lifecycle
run_test test_pane_exit_closes_split
run_test test_pane_exit_preserves_remaining
run_test test_last_pane_exit_closes_window

# Status bar & auto-rename
run_test test_status_bar_strftime
run_test test_automatic_rename
run_test test_automatic_rename_disabled

# Tier 2: Operations
run_test test_rename_window
run_test test_pane_resize
run_test test_multiple_sessions
run_test test_startup_config_loading
run_test test_source_file

# Tier 3: Advanced
run_test test_set_option
run_test test_paste_buffer
run_test test_swap_window

# Overlay tests
run_test test_choose_tree_open_close
run_test test_choose_tree_shows_sessions
run_test test_choose_tree_navigate_and_select
run_test test_display_menu_key_shortcut

echo "=== Results: ${TESTS_PASSED}/${TESTS_RUN} passed, ${TESTS_FAILED} failed ==="

if [[ "${TESTS_FAILED}" -gt 0 ]]; then
    exit 1
fi
