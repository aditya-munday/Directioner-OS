#!/bin/bash
set -euo pipefail

echo "[Greenboot] Evaluating Directioner-OS milestone required services..."
if ! systemctl is-active --quiet directioner-milestone.service; then
    echo "[Greenboot] CRITICAL: directioner-milestone.service is NOT active" >&2
    systemctl status directioner-milestone.service --no-pager || true
    exit 1
fi

echo "[Greenboot] Directioner-OS milestone verification passed."
exit 0
