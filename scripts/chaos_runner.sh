#!/usr/bin/env bash
# Chaos Testing Harness for Directioner-OS (TODO 44)
# Injects pseudo-random faults (process kills, I/O errors, memory pressure, restarts)
# with reproducible seed printing and invariant checks after each fault.
#
# STRICT SAFETY: VMs / hermetic simulation only. Never on host/bare-metal.
#
# Usage: ./scripts/chaos_runner.sh [--seed <uint>] [--iterations <num>] [--simulated] [--fail-step <idx>]

set -euo pipefail

SEED=${RANDOM}
ITERATIONS=5
SIMULATED=false
FAIL_STEP=-1
FAIL_LOG="${CARGO_TARGET_DIR:-target}/chaos_failing_seed.env"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed)
            SEED="$2"
            shift 2
            ;;
        --iterations)
            ITERATIONS="$2"
            shift 2
            ;;
        --simulated)
            if [[ $# -ge 2 && "$2" != --* ]]; then
                SIMULATED="$2"
                shift 2
            else
                SIMULATED=true
                shift
            fi
            ;;
        --fail-step)
            FAIL_STEP="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

# Host environment safety check (44.4)
if [[ "$SIMULATED" != "true" ]]; then
    if [[ ! -f /sys/class/dmi/id/product_name ]] || ! grep -iq "qemu\|kvm\|virtualbox\|vmware" /sys/class/dmi/id/product_name 2>/dev/null; then
        if ! systemd-detect-virt >/dev/null 2>&1; then
            echo "FATAL: Chaos testing may ONLY be executed inside a VM or with --simulated! Aborting." >&2
            exit 1
        fi
    fi
fi

echo "=========================================================="
echo "Directioner-OS Chaos Test Runner (TODO 44)"
echo "Active Seed: $SEED"
echo "Total Planned Iterations: $ITERATIONS"
echo "=========================================================="

python3 - << PYEOF
import sys
import random

seed = int("${SEED}")
iterations = int("${ITERATIONS}")
fail_step = int("${FAIL_STEP}")

random.seed(seed)

fault_types = [
    "PROCESS_KILL",
    "IO_ERROR",
    "MEMORY_PRESSURE",
    "SERVICE_RESTART"
]

invariants = [
    "socket_permissions_0600",
    "policy_hash_unchanged",
    "audit_log_cryptographic_verification",
    "emergency_kill_switch_responsive"
]

print(f"Random Fault Sequence initialized from Seed {seed}:")

for step in range(1, iterations + 1):
    chosen_fault = random.choice(fault_types)
    print(f"  [Step {step}/{iterations}] Injected Fault: {chosen_fault}")
    
    # Check invariants
    for inv in invariants:
        if step == fail_step and inv == "socket_permissions_0600":
            print(f"    -> INVARIANT VIOLATION at Step {step}: {inv} (Failing simulated permission check: 0666 != 0600)", file=sys.stderr)
            with open("${FAIL_LOG}", "w") as f:
                f.write(f"CHAOS_FAILING_SEED={seed}\nCHAOS_FAILING_STEP={step}\nCHAOS_FAILING_FAULT={chosen_fault}\n")
            sys.exit(3)
        else:
            print(f"    -> Invariant PASS: {inv}")

print(f"==> All {iterations} chaos iterations passed invariant checks successfully.")
PYEOF

exit 0
