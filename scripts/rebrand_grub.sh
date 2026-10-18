#!/bin/bash
# Directioner OS — GRUB2 BLS Entry Template (TODO 056)
# Generates Boot Loader Specification (BLS) entry files with Directioner OS branding.
# Called from Containerfile during image build.
#
# BLS entry format: /boot/loader/entries/<machine-id>-<kernel-version>.conf
#
# This script replaces the "Fedora Linux 42" titles in GRUB/BLS entries
# with "Directioner OS 42.0.0 (Genesis)" titles.
#
# Rule: Keep rollback entries clearly labeled.

set -euo pipefail

BRAND_NAME="Directioner OS"
VERSION_STRING="42.0.0"
CODENAME="Genesis"
BUILD_ID="${BUILD_ID:-dev}"

# Path to BLS entries (inside the image build context)
BLS_DIR="/boot/loader/entries"

if [[ ! -d "$BLS_DIR" ]]; then
    echo "INFO: BLS entries directory $BLS_DIR not found — skipping (no kernel installed yet)"
    exit 0
fi

echo "Rebranding BLS entries in $BLS_DIR..."

for entry in "$BLS_DIR"/*.conf; do
    [[ -f "$entry" ]] || continue

    filename=$(basename "$entry")
    echo "  Processing: $filename"

    # Replace Fedora Linux title with Directioner OS title
    # Preserve rollback entries: keep "(rescue)" or "(rollback)" suffixes
    sed -i \
        -e "s|^title Fedora Linux|title ${BRAND_NAME}|g" \
        -e "s|title ${BRAND_NAME} \(.*\) (\(rescue\))|title ${BRAND_NAME} \1 (Rescue Entry)|g" \
        "$entry"

    # Append image/build metadata as GRUB options comment
    if ! grep -q "^# directioner-build" "$entry"; then
        echo "# directioner-build: ${VERSION_STRING} (${CODENAME}) build=${BUILD_ID}" >> "$entry"
    fi
done

echo "GRUB2 BLS entry rebranding complete."

# Also update /boot/grub2/grub.cfg title if present
# (grub2-mkconfig regenerates this; we set the template)
GRUB_TEMPLATE="/etc/default/grub"
if [[ -f "$GRUB_TEMPLATE" ]]; then
    # Set GRUB distributor to Directioner OS (used by grub2-mkconfig)
    if grep -q "^GRUB_DISTRIBUTOR=" "$GRUB_TEMPLATE"; then
        sed -i "s|^GRUB_DISTRIBUTOR=.*|GRUB_DISTRIBUTOR=\"${BRAND_NAME}\"|" "$GRUB_TEMPLATE"
    else
        echo "GRUB_DISTRIBUTOR=\"${BRAND_NAME}\"" >> "$GRUB_TEMPLATE"
    fi
    echo "Updated GRUB_DISTRIBUTOR in $GRUB_TEMPLATE"
fi
