#!/bin/bash
set -euo pipefail

cat > /usr/local/bin/directioner-boot-watchdog << 'EOS'
#!/bin/bash
set -euo pipefail
mkdir -p /var/lib/directioner
COUNT_FILE="/var/lib/directioner/boot_counter"

if grep -q "directioner.milestone=fault_test" /proc/cmdline; then
    COUNT=1
    if [ -f "$COUNT_FILE" ]; then
        COUNT=$(($(cat "$COUNT_FILE") + 1))
    fi
    echo "$COUNT" > "$COUNT_FILE"
    echo "[Directioner-OS Watchdog] Boot milestone failure detected on attempt $COUNT/3"

    if [ "$COUNT" -lt 3 ]; then
        echo "[Directioner-OS Watchdog] Rebooting for retry ($COUNT/3)..."
        sleep 2
        systemctl reboot
    else
        echo "[Directioner-OS Watchdog] Retries exhausted (3/3). Triggering Tier 1 autonomous rollback..."
        rm -f "$COUNT_FILE"
        bootc rollback || ostree admin switch 1 || true
        sleep 2
        systemctl reboot
    fi
else
    rm -f "$COUNT_FILE"
    echo "[Directioner-OS Watchdog] Boot milestone verified healthy."
fi
EOS
chmod 755 /usr/local/bin/directioner-boot-watchdog

cat > /etc/systemd/system/directioner-boot-watchdog.service << 'EOS'
[Unit]
Description=Directioner-OS Tier 1 Boot Milestone Watchdog
After=network.target

[Service]
Type=oneshot
ExecStart=/usr/local/bin/directioner-boot-watchdog
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOS

systemctl daemon-reload
systemctl enable directioner-boot-watchdog.service
