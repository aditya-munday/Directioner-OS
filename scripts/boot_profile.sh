#!/usr/bin/env bash
# Boot profiling tool for Directioner-OS (TODO 43)
# Captures systemd-analyze blame & critical-chain, tracks artifacts,
# extracts top 5 slow units, compares with baseline, and flags regressions.
#
# Usage: ./scripts/boot_profile.sh [--baseline <file>] [--artifact-dir <dir>] [--mock] [--threshold-pct <15>]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

BASELINE=""
ARTIFACT_DIR="${REPO_ROOT}/target/boot_profiles"
MOCK=false
THRESHOLD_PCT=15

while [[ $# -gt 0 ]]; do
    case "$1" in
        --baseline)
            BASELINE="$2"
            shift 2
            ;;
        --artifact-dir)
            ARTIFACT_DIR="$2"
            shift 2
            ;;
        --mock)
            MOCK=true
            shift
            ;;
        --threshold-pct)
            THRESHOLD_PCT="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

mkdir -p "$ARTIFACT_DIR"
CURRENT_PROFILE="$ARTIFACT_DIR/boot_profile_$(date +%s).json"
BLAME_RAW="$ARTIFACT_DIR/systemd_blame.txt"
CHAIN_RAW="$ARTIFACT_DIR/critical_chain.txt"

echo "==> Directioner-OS Boot Profiling (Threshold: +${THRESHOLD_PCT}%)"
echo "    NOTE: QEMU TCG execution is substantially slower than KVM/bare-metal."
echo "    Compare ONLY like-with-like (TCG with TCG, KVM with KVM)."

if [[ "$MOCK" == "true" ]] || ! command -v systemd-analyze >/dev/null 2>&1; then
    cat << 'EOF' > "$BLAME_RAW"
1.850s systemd-udev-settle.service
1.210s NetworkManager-wait-online.service
0.950s dracut-initqueue.service
0.620s ai-capabilityd.service
0.410s systemd-journal-flush.service
0.220s polkit.service
0.110s dbus-broker.service
EOF
    cat << 'EOF' > "$CHAIN_RAW"
graphical.target @4.150s
└─multi-user.target @4.150s
  └─ai-capabilityd.service @3.530s +620ms
    └─basic.target @3.520s
      └─sockets.target @3.520s
        └─systemd-journald.socket @3.510s
EOF
else
    systemd-analyze blame > "$BLAME_RAW" 2>&1 || true
    systemd-analyze critical-chain > "$CHAIN_RAW" 2>&1 || true
fi

# Parse top 5 slow units
python3 - << PYEOF
import json
import re
import sys

blame_path = "${BLAME_RAW}"
profile_path = "${CURRENT_PROFILE}"
baseline_path = "${BASELINE}"
threshold = float("${THRESHOLD_PCT}")

units = []
with open(blame_path, "r", encoding="utf-8") as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        # Format: 1.850s unit.service or 850ms unit.service
        m = re.match(r"^([\d\.]+)(s|ms|min)\s+(.+)$", line)
        if m:
            val = float(m.group(1))
            unit = m.group(2)
            name = m.group(3)
            time_sec = val if unit == 's' else (val / 1000.0 if unit == 'ms' else val * 60.0)
            units.append({"unit": name, "duration_secs": round(time_sec, 3)})

units.sort(key=lambda x: x["duration_secs"], reverse=True)
top5 = units[:5]
total_top5_secs = round(sum(u["duration_secs"] for u in top5), 3)

data = {
    "top5": top5,
    "total_top5_secs": total_top5_secs,
    "environment": "tcg_or_emulated",
}

with open(profile_path, "w", encoding="utf-8") as f:
    json.dump(data, f, indent=2)

print("Top 5 Slowest Units Identified:")
for idx, u in enumerate(top5, 1):
    print(f"  {idx}. {u['unit']} ({u['duration_secs']}s)")
print(f"Total Top 5 Duration: {total_top5_secs}s")

if baseline_path and baseline_path != "":
    try:
        with open(baseline_path, "r", encoding="utf-8") as bf:
            bdata = json.load(bf)
        b_total = bdata.get("total_top5_secs", total_top5_secs)
        pct_delta = round(((total_top5_secs - b_total) / b_total) * 100.0, 2)
        print(f"Comparison with Baseline ({baseline_path}):")
        print(f"  Baseline: {b_total}s | Current: {total_top5_secs}s | Delta: {pct_delta:+}%")
        if pct_delta > threshold:
            print(f"REGRESSION WARNING: Boot slowdown exceeds threshold (+{threshold}%)!", file=sys.stderr)
            sys.exit(2)
    except Exception as e:
        print(f"Warning: unable to compare baseline: {e}")

PYEOF

echo "==> Profile successfully generated: $CURRENT_PROFILE"
exit 0
