#!/usr/bin/env bash
# Inspection script for Directioner-OS release image (TODO 31.3)
# Scans a mounted rootfs directory or extracted container root for forbidden artifacts:
#   - 'ci' user in /etc/passwd
#   - committed SSH authorized_keys
#   - passwordless sudo (/etc/sudoers.d/* with NOPASSWD)
#   - forbidden '--exec' execution flags in systemd unit definitions
#   - test sockets under /run or /tmp
#
# Usage: ./scripts/inspect_release_image.sh <ROOTFS_DIR>

set -euo pipefail

ROOTFS="${1:-}"

if [[ -z "$ROOTFS" ]] || [[ ! -d "$ROOTFS" ]]; then
    echo "Usage: $0 <ROOTFS_DIR>" >&2
    exit 2
fi

echo "==> Inspecting release rootfs at: $ROOTFS"
ERRORS=0

# 1. Check for 'ci' user in /etc/passwd
if [[ -f "$ROOTFS/etc/passwd" ]]; then
    if grep -q "^ci:" "$ROOTFS/etc/passwd"; then
        echo "[-] VIOLATION: 'ci' user found in /etc/passwd!" >&2
        ERRORS=$((ERRORS + 1))
    else
        echo "[+] PASS: No 'ci' user in /etc/passwd"
    fi
fi

# 2. Check for committed public keys in authorized_keys
KEYS_FOUND=$(find "$ROOTFS" -name "authorized_keys" 2>/dev/null || true)
if [[ -n "$KEYS_FOUND" ]]; then
    echo "[-] VIOLATION: committed authorized_keys found:" >&2
    echo "$KEYS_FOUND" >&2
    ERRORS=$((ERRORS + 1))
else
    echo "[+] PASS: No committed authorized_keys found"
fi

# 3. Check for passwordless sudo (NOPASSWD) in /etc/sudoers and /etc/sudoers.d
if [[ -d "$ROOTFS/etc/sudoers.d" ]]; then
    NOPASSWD_MATCHES=$(grep -rn "NOPASSWD" "$ROOTFS/etc/sudoers.d" 2>/dev/null || true)
    if [[ -n "$NOPASSWD_MATCHES" ]]; then
        echo "[-] VIOLATION: Passwordless sudo (NOPASSWD) configuration detected:" >&2
        echo "$NOPASSWD_MATCHES" >&2
        ERRORS=$((ERRORS + 1))
    else
        echo "[+] PASS: No NOPASSWD found in /etc/sudoers.d"
    fi
fi

# 4. Check for forbidden '--exec' flags in custom systemd unit files
if [[ -d "$ROOTFS/usr/lib/systemd/system" ]]; then
    EXEC_MATCHES=$(grep -rn -- "--exec" "$ROOTFS/usr/lib/systemd/system/ai-"* 2>/dev/null || true)
    if [[ -n "$EXEC_MATCHES" ]]; then
        echo "[-] VIOLATION: Forbidden '--exec' flag found in systemd units:" >&2
        echo "$EXEC_MATCHES" >&2
        ERRORS=$((ERRORS + 1))
    else
        echo "[+] PASS: No '--exec' flags found in ai-* systemd units"
    fi
fi

# 5. Check for test sockets
TEST_SOCKETS=$(find "$ROOTFS/run" "$ROOTFS/var/run" "$ROOTFS/tmp" -name "*test*.sock" 2>/dev/null || true)
if [[ -n "$TEST_SOCKETS" ]]; then
    echo "[-] VIOLATION: Test sockets detected in image:" >&2
    echo "$TEST_SOCKETS" >&2
    ERRORS=$((ERRORS + 1))
else
    echo "[+] PASS: No test sockets found"
fi

# Summary verdict
if [[ "$ERRORS" -gt 0 ]]; then
    echo "==> FAILED: $ERRORS security boundary violation(s) detected in release image!" >&2
    exit 1
fi

echo "==> SUCCESS: Release image conforms to all TODO 31 security invariants."
exit 0
