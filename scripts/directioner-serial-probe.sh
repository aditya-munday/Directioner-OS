#!/usr/bin/env bash
# Hardware probe for serial console arbitration (TODO 35.3)
# Validates whether a target TTY device is responsive to termios inquiry before agetty launch.
# Exits 0 if the serial port is responsive and active.
# Exits 1 if the port is phantom, unconfigured, or returns EINVAL/ENODEV (skips systemd launch).

set -euo pipefail

DEV="${1:-/dev/ttyS0}"

if [[ ! -c "$DEV" ]]; then
    # Device node does not exist or is not a character device
    exit 1
fi

# Attempt stty query on the target device
# In VMware with disconnected UART, stty fails with return code != 0 (e.g. Inappropriate ioctl or Invalid argument)
if stty -F "$DEV" speed >/dev/null 2>&1; then
    exit 0
else
    exit 1
fi
