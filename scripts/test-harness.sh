#!/usr/bin/env bash
# test-harness.sh — Reusable shell library for E2E testing rmux inside tmux.
#
# Usage:
#   source scripts/test-harness.sh
#   harness_build
#   harness_start
#   harness_send "echo hello"
#   harness_send Enter
#   harness_wait_for "hello" 5
#   harness_capture
#   harness_stop

set -euo pipefail

# --- Configuration -----------------------------------------------------------

HARNESS_PID=$$
HARNESS_TMUX_SESSION="rmux-test-${HARNESS_PID}"
HARNESS_SOCKET_DIR="/tmp/rmux-test-${HARNESS_PID}"
HARNESS_SOCKET_NAME="default"
HARNESS_POLL_INTERVAL=0.2
HARNESS_DEFAULT_TIMEOUT=5
HARNESS_PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS_BINARY_DIR="${HARNESS_PROJECT_ROOT}/target/debug"
HARNESS_RMUX_SOCKET=""
HARNESS_STARTED=0

# --- Internal helpers ---------------------------------------------------------

_harness_log() {
    echo "[harness] $*" >&2
}

_harness_fail() {
    echo "[harness] FAIL: $*" >&2
    echo "[harness] Screen contents:" >&2
    harness_capture >&2 || true
    return 1
}

# --- Public API ---------------------------------------------------------------

harness_build() {
    _harness_log "Building rmux..."
    (cd "${HARNESS_PROJECT_ROOT}" && cargo build 2>&1) || {
        _harness_fail "cargo build failed"
        return 1
    }
    _harness_log "Build complete."
}

harness_start() {
    if [[ "${HARNESS_STARTED}" -eq 1 ]]; then
        _harness_log "Harness already started, stopping first..."
        harness_stop
    fi

    # Verify binaries exist
    if [[ ! -x "${HARNESS_BINARY_DIR}/rmux" ]]; then
        _harness_fail "rmux binary not found at ${HARNESS_BINARY_DIR}/rmux — run harness_build first"
        return 1
    fi

    # Create isolated socket directory
    mkdir -p "${HARNESS_SOCKET_DIR}"

    # Set TMPDIR so rmux uses our isolated socket path.
    # rmux resolves: $TMPDIR/rmux-$UID/$socket_name
    # We point TMPDIR at a directory where rmux-$UID/ maps to our test dir.
    local uid
    uid=$(id -u)
    local rmux_parent="${HARNESS_SOCKET_DIR}/rmux-parent"
    mkdir -p "${rmux_parent}/rmux-${uid}"
    local rmux_tmpdir="${rmux_parent}"
    HARNESS_RMUX_SOCKET="${rmux_parent}/rmux-${uid}/${HARNESS_SOCKET_NAME}"

    _harness_log "Starting tmux session '${HARNESS_TMUX_SESSION}'..."

    # Launch tmux with C-q as prefix (so C-b passes through to rmux)
    tmux new-session -d -s "${HARNESS_TMUX_SESSION}" -x 120 -y 40 \
        "TMPDIR=${rmux_tmpdir} PATH=${HARNESS_BINARY_DIR}:\${PATH} rmux; echo '[rmux exited]'; sleep 86400"

    # Remap tmux prefix to C-q so C-b passes through to rmux
    tmux set-option -t "${HARNESS_TMUX_SESSION}" prefix C-q
    tmux unbind-key C-b

    HARNESS_STARTED=1

    # Wait for rmux status bar to appear
    if ! harness_wait_for '\[0\]' "${HARNESS_DEFAULT_TIMEOUT}"; then
        _harness_fail "rmux did not start — status bar not found"
        harness_stop
        return 1
    fi

    _harness_log "rmux is running."
}

harness_send() {
    if [[ "${HARNESS_STARTED}" -eq 0 ]]; then
        _harness_fail "harness not started"
        return 1
    fi
    tmux send-keys -t "${HARNESS_TMUX_SESSION}" "$@"
}

# Send a prefix key combo (C-b + key) with a brief gap.
# Convenience wrapper to avoid repeating the C-b + sleep + key pattern.
harness_prefix() {
    harness_send C-b
    sleep 0.1
    harness_send "$@"
}

harness_rmux() {
    if [[ -z "${HARNESS_RMUX_SOCKET}" ]]; then
        _harness_fail "harness not started — no socket path"
        return 1
    fi
    "${HARNESS_BINARY_DIR}/rmux" -S "${HARNESS_RMUX_SOCKET}" "$@"
}

harness_capture() {
    if [[ "${HARNESS_STARTED}" -eq 0 ]]; then
        _harness_fail "harness not started"
        return 1
    fi
    tmux capture-pane -t "${HARNESS_TMUX_SESSION}" -p
}

harness_wait_for() {
    local pattern="$1"
    local timeout="${2:-${HARNESS_DEFAULT_TIMEOUT}}"

    if [[ "${HARNESS_STARTED}" -eq 0 ]]; then
        _harness_fail "harness not started"
        return 1
    fi

    local elapsed=0
    while (( $(echo "${elapsed} < ${timeout}" | bc -l) )); do
        if harness_capture 2>/dev/null | grep -qE "${pattern}"; then
            return 0
        fi
        sleep "${HARNESS_POLL_INTERVAL}"
        elapsed=$(echo "${elapsed} + ${HARNESS_POLL_INTERVAL}" | bc -l)
    done

    return 1
}

harness_assert() {
    local pattern="$1"
    local msg="${2:-Pattern '${pattern}' not found on screen}"

    if ! harness_capture | grep -qE "${pattern}"; then
        _harness_fail "${msg}"
        return 1
    fi
}

harness_assert_not() {
    local pattern="$1"
    local msg="${2:-Pattern '${pattern}' should NOT be on screen}"

    if harness_capture | grep -qE "${pattern}"; then
        _harness_fail "${msg}"
        return 1
    fi
}

harness_stop() {
    _harness_log "Stopping harness..."

    # Kill the tmux session (which kills rmux inside it)
    tmux kill-session -t "${HARNESS_TMUX_SESSION}" 2>/dev/null || true

    # Clean up any lingering rmux-server processes using our socket
    local uid
    uid=$(id -u)
    local socket="${HARNESS_SOCKET_DIR}/rmux-parent/rmux-${uid}/${HARNESS_SOCKET_NAME}"
    if [[ -S "${socket}" ]]; then
        # Find and kill any process listening on the socket
        fuser "${socket}" 2>/dev/null | xargs -r kill 2>/dev/null || true
    fi

    # Clean up socket directory
    rm -rf "${HARNESS_SOCKET_DIR}"

    HARNESS_RMUX_SOCKET=""
    HARNESS_STARTED=0
    _harness_log "Stopped."
}

# Register cleanup on exit
trap harness_stop EXIT INT TERM
