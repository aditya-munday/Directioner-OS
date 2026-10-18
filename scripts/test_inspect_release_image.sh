#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSPECT_BIN="$SCRIPT_DIR/inspect_release_image.sh"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "==> Testing inspect_release_image.sh..."

# 1. Clean rootfs test
CLEAN_ROOT="$TMPDIR/clean_rootfs"
mkdir -p "$CLEAN_ROOT/etc/sudoers.d" "$CLEAN_ROOT/usr/lib/systemd/system" "$CLEAN_ROOT/run"
echo "root:x:0:0:root:/root:/bin/bash" > "$CLEAN_ROOT/etc/passwd"
echo "# Empty sudoers.d" > "$CLEAN_ROOT/etc/sudoers.d/README"
echo "ExecStart=/usr/bin/ai-capabilityd" > "$CLEAN_ROOT/usr/lib/systemd/system/ai-capabilityd.service"

if "$INSPECT_BIN" "$CLEAN_ROOT" >/dev/null 2>&1; then
    echo "[+] PASS: Clean rootfs verified cleanly"
else
    echo "[-] FAIL: Clean rootfs should have passed" >&2
    exit 1
fi

# 2. Dirty rootfs test with 'ci' user and NOPASSWD
DIRTY_ROOT="$TMPDIR/dirty_rootfs"
mkdir -p "$DIRTY_ROOT/etc/sudoers.d" "$DIRTY_ROOT/usr/lib/systemd/system" "$DIRTY_ROOT/run"
echo "ci:x:1000:1000::/home/ci:/bin/bash" > "$DIRTY_ROOT/etc/passwd"
echo "%wheel ALL=(ALL) NOPASSWD: ALL" > "$DIRTY_ROOT/etc/sudoers.d/wheel"

if "$INSPECT_BIN" "$DIRTY_ROOT" >/dev/null 2>&1; then
    echo "[-] FAIL: Dirty rootfs should have failed validation" >&2
    exit 1
else
    echo "[+] PASS: Dirty rootfs correctly rejected with non-zero exit code"
fi

echo "==> All inspect_release_image.sh tests passed."
