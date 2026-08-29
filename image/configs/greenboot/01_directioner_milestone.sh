#!/bin/bash
set -euo pipefail

echo "[Greenboot] Evaluating Directioner-OS milestone required services..."

# Bounded poll: up to 10 seconds defense-in-depth safety net
active=0
for i in $(seq 1 10); do
    if systemctl is-active --quiet directioner-milestone.service; then
        active=1
        break
    fi
    sleep 1
done

if [ "$active" -ne 1 ]; then
    echo "[Greenboot] CRITICAL: directioner-milestone.service is NOT active after 10s bounded timeout" >&2
    systemctl status directioner-milestone.service --no-pager || true

    # Manage GRUB boot_counter retry cycles (3 -> 2 -> 1 -> rollback)
    COUNTER=$(grub2-editenv /boot/grub2/grubenv list 2>/dev/null | grep '^boot_counter=' | cut -d= -f2 || true)
    COUNTER=${COUNTER:-1}
    echo "[Greenboot] Current boot_counter: $COUNTER"
    if [ "$COUNTER" -gt 1 ]; then
        NEXT_COUNT=$((COUNTER - 1))
        echo "[Greenboot] Decrementing boot_counter to $NEXT_COUNT and rebooting for retry ($NEXT_COUNT remaining)..."
        grub2-editenv /boot/grub2/grubenv set boot_counter=$NEXT_COUNT
        sleep 2
        systemctl reboot
        exit 1
    else
        echo "[Greenboot] Boot retries exhausted (counter=$COUNTER). Initiating Tier 1 ostree automatic rollback to pinned good deployment..."
        grub2-editenv /boot/grub2/grubenv unset boot_counter 2>/dev/null || true
        grub2-editenv /boot/grub2/grubenv set boot_success=0 2>/dev/null || true
        bootc rollback || ostree admin switch 1 || true
        sleep 2
        systemctl reboot
        exit 1
    fi
fi

# Healthy boot: clear counter, mark boot success
grub2-editenv /boot/grub2/grubenv unset boot_counter 2>/dev/null || true
grub2-editenv /boot/grub2/grubenv set boot_success=1 2>/dev/null || true
echo "[Greenboot] Directioner-OS milestone verification passed."
exit 0
