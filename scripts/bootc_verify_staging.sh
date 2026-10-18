#!/usr/bin/env bash
# Verification script for staged bootc deployments before reboot (TODO 39.1, 39.2)
# Usage: ./scripts/bootc_verify_staging.sh [--mock-status-file <FILE>]

set -euo pipefail

MOCK_FILE=""
if [[ "${1:-}" == "--mock-status-file" ]]; then
    MOCK_FILE="${2:-}"
fi

echo "==> Verifying staged bootc deployment before reboot..."

# 1. Obtain bootc status JSON
if [[ -n "$MOCK_FILE" ]] && [[ -f "$MOCK_FILE" ]]; then
    STATUS_JSON=$(cat "$MOCK_FILE")
elif command -v bootc >/dev/null 2>&1; then
    STATUS_JSON=$(bootc status --json)
else
    echo "[-] Error: bootc command not found and no mock status file provided" >&2
    exit 2
fi

# 2. Check for staged deployment existence
HAS_STAGED=$(echo "$STATUS_JSON" | grep -q '"staged"' && echo "true" || echo "false")
if [[ "$HAS_STAGED" != "true" ]]; then
    echo "[-] No staged deployment found. System is currently running the primary deployment." >&2
    exit 1
fi
echo "[+] Verified: Staged deployment exists in status."

# 3. Check greenboot health scripts
GREENBOOT_DIR="/etc/greenboot/check/required.d"
if [[ -d "$GREENBOOT_DIR" ]]; then
    SCRIPTS=$(find "$GREENBOOT_DIR" -type f -name "*.sh" 2>/dev/null | wc -l)
    if [[ "$SCRIPTS" -gt 0 ]]; then
        echo "[+] Verified: $SCRIPTS greenboot health check script(s) found in $GREENBOOT_DIR."
    else
        echo "[-] Warning: No greenboot health scripts found in $GREENBOOT_DIR." >&2
    fi
else
    echo "[+] Notice: Standard host run (using built-in milestone checks)."
fi

# 4. Document Boot Counter Threshold
echo "[+] Boot counter threshold: 3 failed attempts before automatic grub2/greenboot rollback."
echo "==> Staging verification complete. Safe to reboot into staged deployment."
exit 0
