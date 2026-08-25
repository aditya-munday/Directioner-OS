#!/usr/bin/env bash
# build.sh — wraps podman/docker build + bootc-image-builder (guide §1.3/§1.4)
#
# Usage:
#   ./build.sh                     # build container image only
#   ./build.sh --qcow2             # build image + emit a bootable qcow2 disk image
#   ./build.sh --clean             # remove prior build artifacts first
set -euo pipefail

cd "$(dirname "$0")"

IMAGE_TAG="${AI_OS_IMAGE_TAG:-localhost/ai-os:dev}"
ENGINE="${AI_OS_ENGINE:-}"
QCW2_FLAG="false"
CLEAN="false"

for arg in "$@"; do
    case "$arg" in
        --qcow2) QCW2_FLAG="true" ;;
        --clean) CLEAN="true" ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

# Prefer podman, fall back to docker (bootc tooling targets podman).
if [[ -z "$ENGINE" ]]; then
    if command -v podman >/dev/null 2>&1; then
        ENGINE=podman
    elif command -v docker >/dev/null 2>&1; then
        ENGINE=docker
    else
        echo "error: need podman or docker to build the image" >&2
        exit 1
    fi
fi
echo "==> using engine: $ENGINE"

if [[ "$CLEAN" == "true" ]]; then
    rm -rf output
fi

echo "==> building $IMAGE_TAG"
"$ENGINE" build -t "$IMAGE_TAG" -f Containerfile .

if [[ "$QCW2_FLAG" != "true" ]]; then
    echo "==> done. Use '--qcow2' to also emit a bootable disk image."
    exit 0
fi

# bootc-image-builder runs as a container (guide §1.4). It needs
# privileges to assemble the disk image.
mkdir -p output
if "$ENGINE" info --format '{{.Host.Security.Rootless}}' 2>/dev/null; then
    sudo "$ENGINE" run --rm --privileged \
        --security-opt label=type:unconfined_t \
        -v "$(pwd)/output":/output \
        -v /var/lib/containers/storage:/var/lib/containers/storage \
        quay.io/centos-bootc/bootc-image-builder:latest \
        --type qcow2 "$IMAGE_TAG"
else
    echo "==> container engine is rootful; running bootc-image-builder directly" >&2
    "$ENGINE" run --rm --privileged \
        --security-opt label=type:unconfined_t \
        -v "$(pwd)/output":/output \
        -v /var/lib/containers/storage:/var/lib/containers/storage \
        quay.io/centos-bootc/bootc-image-builder:latest \
        --type qcow2 "$IMAGE_TAG"
fi

echo "==> done. Boot the result in a VM:"
echo "    qemu-system-x86_64 -m 4096 -enable-kvm -drive file=output/qcow2/disk.qcow2,format=qcow2"