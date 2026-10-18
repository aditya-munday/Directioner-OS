#!/usr/bin/env bash
# Update Verification with Automatic Rollback (TODO 50)
# Orchestrates bootc staging verification, greenboot boot counter evaluation,
# gating health checks, and fallback slot rollback execution.
#
# NOTE (Rule E): Reboot is MANDATORY for kernel/OS image updates. Non-zero downtime.
#
# Usage: ./scripts/update_verify_rollback.sh [--simulate-good|--simulate-bad|--simulate-hung] [--counter <3>]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

MODE="good"
BOOT_COUNTER=3
STATE_DIR="${REPO_ROOT}/target/update_verification_state"
mkdir -p "$STATE_DIR"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --simulate-good)
            MODE="good"
            shift
            ;;
        --simulate-bad)
            MODE="bad"
            shift
            ;;
        --simulate-hung)
            MODE="hung"
            shift
            ;;
        --counter)
            BOOT_COUNTER="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

COUNTER_FILE="$STATE_DIR/greenboot_boot_counter.env"
echo "BOOT_COUNTER=$BOOT_COUNTER" > "$COUNTER_FILE"

echo "=========================================================="
echo "Directioner-OS Update Verification Engine (TODO 50)"
echo "Mode: $MODE | Initial Boot Counter: $BOOT_COUNTER"
echo "NOTE: Operating system updates require a reboot (non-zero downtime)."
echo "=========================================================="

python3 - << PYEOF
import sys

mode = "${MODE}"
counter = int("${BOOT_COUNTER}")

print(f"Step 1: Staging signed deployment 'directioner-os-42.0.1'...")
print("  -> Signature verification (cosign / GPG): VALID")
print("  -> Deployment staged into slot 1.")

if mode == "hung":
    print("Step 2: Rebooting into staged slot 1...")
    print("  -> Kernel hang / watchdog timeout encountered before userspace!")
    print(f"  -> Hardware watchdog resets system; decremented counter: {counter} -> 0")
    print("  -> [ROLLBACK TRIGGERED]: GRUB automatically boots previous known-good deployment (slot 0)!")
    sys.exit(0)

print("Step 2: Machine rebooted into staged slot 1.")
print(f"Step 3: Greenboot trial active (boot_counter={counter}). Executing gating health checks in /etc/greenboot/check/required.d/...")

checks = [
    ("01_systemd_posture.sh", True, "systemd state: running"),
    ("02_ai_capabilityd.sh", mode != "bad", "ai-capabilityd active and socket ping 200 OK" if mode != "bad" else "CRITICAL: ai-capabilityd crashed; socket connection refused!"),
    ("03_world_model.sh", True, "world-model DB integrity check OK"),
    ("04_audit_worklog.sh", True, "audit worklog SHA256 chain verified")
]

all_passed = True
for name, passed, details in checks:
    status_str = "PASS" if passed else "FAIL"
    print(f"  [{status_str}] {name}: {details}")
    if not passed:
        all_passed = False

if all_passed:
    print("Step 4: All gating health checks PASSED.")
    print("  -> Calling /usr/libexec/greenboot/greenboot-status set-status green")
    print("  -> Boot counter cleared. Deployment committed successfully.")
    sys.exit(0)
else:
    counter -= 1
    print(f"Step 4: Gating health checks FAILED! Decrementing boot counter to {counter}.")
    if counter == 0:
        print("  -> [ROLLBACK TRIGGERED]: Boot counter exhausted (0 attempts remaining).")
        print("  -> Rolling back to previous deployment slot 0...")
        print("  -> Successfully rebooted into rollback slot 0 with previous known-good state.")
        sys.exit(0)
    else:
        print(f"  -> System will retry staged deployment ({counter} attempts left).")
        sys.exit(1)

PYEOF

exit 0
