#!/bin/bash
# Directioner OS — Shell login banner (TODO 064)
# Minimal: shows image id and account state. No marketing claims (Rule E).
# Only runs for interactive login shells.

if [[ $- == *i* ]] && shopt -q login_shell 2>/dev/null; then
    # Read image metadata from os-release
    _DOS_IMAGE_ID=""
    _DOS_BUILD_ID=""
    if [[ -f /etc/os-release ]]; then
        # shellcheck disable=SC1091
        source /etc/os-release 2>/dev/null
        _DOS_IMAGE_ID="${IMAGE_ID:-unknown}"
        _DOS_BUILD_ID="${BUILD_ID:-unknown}"
    fi

    # Account state (optional — dos-account CLI if present)
    _DOS_ACCOUNT_STATE=""
    if command -v dos-account >/dev/null 2>&1; then
        _DOS_ACCOUNT_STATE=$(dos-account status --format=brief 2>/dev/null || echo "not signed in")
    fi

    # Print concise banner
    echo ""
    echo "  Directioner OS 42.0.0 — The AI-Native Operating System"
    echo "  ─────────────────────────────────────────────────────────"
    [[ -n "$_DOS_IMAGE_ID" ]]       && echo "  Image   : $_DOS_IMAGE_ID"
    [[ -n "$_DOS_BUILD_ID" ]]       && echo "  Build   : $_DOS_BUILD_ID"
    [[ -n "$_DOS_ACCOUNT_STATE" ]]  && echo "  Account : $_DOS_ACCOUNT_STATE"
    echo ""
fi
