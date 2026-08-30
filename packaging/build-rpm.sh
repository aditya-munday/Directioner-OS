#!/usr/bin/env bash
# Directioner-OS RPM Packaging Script
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

RPMBUILD_DIR="${ROOT_DIR}/packaging/rpmbuild"
mkdir -p "${RPMBUILD_DIR}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

VERSION="0.1.0"
PKG_NAME="directioner-ai-engineer"

echo "==> Preparing source archive for ${PKG_NAME}-${VERSION}..."
tar --exclude-vcs \
    --exclude="target" \
    --exclude="packaging/rpmbuild" \
    --transform="s,^,${PKG_NAME}-${VERSION}/," \
    -czf "${RPMBUILD_DIR}/SOURCES/${PKG_NAME}-${VERSION}.tar.gz" \
    -C "${ROOT_DIR}" .

cp "${SCRIPT_DIR}/rpm/${PKG_NAME}.spec" "${RPMBUILD_DIR}/SPECS/"

if command -v rpmbuild >/dev/null 2>&1; then
    echo "==> Building RPM package via rpmbuild..."
    rpmbuild --define "_topdir ${RPMBUILD_DIR}" -ba "${RPMBUILD_DIR}/SPECS/${PKG_NAME}.spec"
    echo "==> RPM build complete: ${RPMBUILD_DIR}/RPMS/"
else
    echo "==> rpmbuild is not available on host. The spec file and source archive are staged in ${RPMBUILD_DIR}."
fi
