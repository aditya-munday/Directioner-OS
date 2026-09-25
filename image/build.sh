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
    rm -rf output bin
fi

# Stage AI Engineer control plane release binaries into bin/
mkdir -p bin
if [[ "$CLEAN" == "true" ]] || [[ ! -f "../ai-engineer/target/release/ai-capabilityd" ]]; then
    echo "==> Building AI Engineer release workspace..."
    (cd ../ai-engineer && cargo build --release --workspace)
fi
echo "==> Staging release binaries into image/bin/..."
for b in ai-worldmodel ai-benchmark ai-capabilityd ai-worklog ai-killswitch ai-engineer ai-health ai-cockpit ai-hwcheck; do
    if [[ -f "../ai-engineer/target/release/$b" ]]; then
        cp "../ai-engineer/target/release/$b" bin/
    fi
done

echo "==> building $IMAGE_TAG"
"$ENGINE" build -t "$IMAGE_TAG" -f Containerfile .

if [[ "$QCW2_FLAG" != "true" ]]; then
    echo "==> done. Use '--qcow2' to also emit a bootable disk image."
    exit 0
fi

# bootc-image-builder runs as a container (guide §1.4). It needs
# privileges to assemble the disk image.
mkdir -p output

# If config.toml does not exist, auto-generate one with the operator's SSH public key
CONFIG_ARG=()
CONFIG_MOUNT=()
if [[ ! -f "config.toml" ]]; then
    echo "==> Checking for SSH public key to inject into config.toml..."
    USER_KEY=""
    if [[ -n "${SSH_PUBKEY:-}" ]]; then
        USER_KEY="$SSH_PUBKEY"
    elif [[ -f "${HOME}/.ssh/id_ed25519.pub" ]]; then
        USER_KEY="$(cat "${HOME}/.ssh/id_ed25519.pub")"
    elif [[ -f "${HOME}/.ssh/id_rsa.pub" ]]; then
        USER_KEY="$(cat "${HOME}/.ssh/id_rsa.pub")"
    elif [[ -f "../vm-test/ci_key.pub" ]]; then
        USER_KEY="$(cat "../vm-test/ci_key.pub")"
    fi

    if [[ -n "$USER_KEY" ]]; then
        cat > config.toml <<EOF
[[customizations.user]]
name = "directioner"
key = "${USER_KEY}"
groups = ["wheel"]
EOF
        echo "==> Injected user 'directioner' with SSH key into config.toml"
    else
        echo "==> warning: No SSH public key found; image will build with default bootc user config." >&2
    fi
fi

if [[ -f "config.toml" ]]; then
    CONFIG_MOUNT=(-v "$(pwd)/config.toml":/config.toml:ro)
    CONFIG_ARG=(--config /config.toml)
fi

RUN_PREFIX=""
if "$ENGINE" info --format '{{.Host.Security.Rootless}}' 2>/dev/null; then
    RUN_PREFIX="sudo"
fi

$RUN_PREFIX "$ENGINE" run --rm --privileged \
    --security-opt label=type:unconfined_t \
    -v "$(pwd)/output":/output \
    "${CONFIG_MOUNT[@]}" \
    -v /var/lib/containers/storage:/var/lib/containers/storage \
    quay.io/centos-bootc/bootc-image-builder:latest \
    --type qcow2 --rootfs xfs "${CONFIG_ARG[@]}" "$IMAGE_TAG"

echo "==> Done. Boot the result in a VM:"
echo "    qemu-system-x86_64 -machine accel=kvm -cpu host -m 4096 -smp 4 \\"
echo "      -drive file=output/qcow2/disk.qcow2,format=qcow2,if=virtio \\"
echo "      -netdev user,id=n0,hostfwd=tcp:127.0.0.1:2222-:22 -device virtio-net-pci,netdev=n0 \\"
echo "      -display none -serial mon:stdio"
echo ""
echo "    Connect via SSH from another terminal:"
echo "    ssh -p 2222 directioner@127.0.0.1"