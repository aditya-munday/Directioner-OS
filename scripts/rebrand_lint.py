#!/usr/bin/env python3
"""
Directioner OS — Rebrand Lint (TODO 087)
Scans a built container image or a directory tree for "Fedora" strings
that are NOT in the legal-notice allowlist.

Usage:
  # Scan a mounted image root (e.g., via podman export or buildah mount)
  python3 scripts/rebrand_lint.py --root /path/to/image/root

  # Scan the repository source tree (for CI checks before build)
  python3 scripts/rebrand_lint.py --root .

  # Output counts only (for CI artifact)
  python3 scripts/rebrand_lint.py --root . --counts-only

Exit codes:
  0 — no violations (all Fedora strings are allowlisted)
  1 — violations found (CI FAIL)
  2 — usage / configuration error
"""

import argparse
import json
import os
import re
import sys
from pathlib import Path

# ── Allowlist ─────────────────────────────────────────────────────────────────
# Paths or path patterns where "Fedora" strings are EXPECTED and LEGAL.
# Add to this list with a comment explaining why each entry is allowed.
ALLOWLIST_PATH_PATTERNS = [
    # Upstream RPM metadata — we cannot change these (TODO 052)
    r".*\.rpm$",
    r".*/rpm-gpg/RPM-GPG-KEY-fedora.*",
    # Legal notices in package licenses
    r".*/licenses/.*",
    r".*/copyright$",
    r".*/COPYING.*",
    r".*/LICENSE.*",
    # Upstream kernel packages — dist tag fcNN is expected (TODO 052)
    r".*/lib/modules/.*\.fc[0-9]+.*",
    r".*/boot/vmlinuz-.*\.fc[0-9]+.*",
    r".*/boot/System\.map-.*\.fc[0-9]+.*",
    # SELinux policy — upstream policy names (TODO 052)
    r".*/selinux/.*",
    # Git repository metadata (repo source, not image)
    r".*\.git/.*",
    # Build artifacts and third-party source tarballs
    r".*/vendor/.*",
    r".*target/.*",
    r".*build/.*",
    # Our own rebrand documentation explicitly cites Fedora (legal context)
    r".*/docs/.*",
    r"^docs/.*",
    r".*/ARCHITECTURE\.md$",
    r".*brand\.toml$",            # id_like=["fedora"] is intentional
    # This script itself + other lint/scan scripts (discuss Fedora in comments)
    r".*rebrand_lint\.py$",
    r".*asset_provenance_scan\.py$",
    r".*rebrand_grub\.sh$",
    r".*validate_brand_toml\.py$",
    r".*generate_brand_tokens\.py$",
    # CI workflows reference upstream (natural)
    r".*\.github/.*",
    # The plan and context files
    r".*DIRECTIONER_OS_ULTIMATE_PLAN.*",
    r".*DIRECTIONER_OS_CONTEXT.*",
    # Top-level docs: README, ARCHITECTURE, CONTRIBUTING (reference upstream)
    r".*README\.md$",
    r".*CONTRIBUTING\.md$",
    r".*ARCHITECTURE\.md$",
    # VMware forensics — raw system output from Fedora-based VM (historical data)
    r".*vmware-forensics/.*",
    r"^vmware-forensics/.*",
    # RPM spec files for our packages — may reference Fedora in Obsoletes/comments
    r".*/SPECS/.*\.spec$",
    r"^packaging/.*/SPECS/.*\.spec$",
    r"^packaging/.*\.spec$",
    r"^packaging/.*\.json$",
    r"^packaging/.*\.jsonc$",
    # Containerfile — FROM line references Fedora base image (required)
    r".*Containerfile$",
    r".*/image/configs/.*",
    # Rust test files that discuss Fedora behavior in comments
    r".*/tests/.*\.rs$",
    r".*/src/hwcheck\.rs$",
    r".*/src/localization\.rs$",
    r".*/benchmark-suite/.*",
    r".*/capability-layer/.*",
    r".*/world-model/.*",
    # Brand identity module discusses ID_LIKE=fedora (intentional)
    r".*/src/brand_identity\.rs$",
]

# String patterns that ARE allowed even when found in non-allowlisted paths
ALLOWLIST_STRING_PATTERNS = [
    # id_like=fedora in os-release is intentional for dnf/rpm-ostree compat
    r'ID_LIKE\s*=\s*["\']?fedora["\']?',
    r"id_like\s*=\s*\[.*fedora.*\]",
    # Comments explicitly acknowledging Fedora base (honest disclosure)
    r"#.*fedora.*base",
    r"#.*based on fedora",
    r"#.*upstream fedora",
    r"#.*fedora linux",
    r"//.*fedora",
    r"/\*.*fedora.*\*/",
    # Package version dist tags in specfiles (expected)
    r"fedora_base_version",
    # Containerfile FROM line
    r"FROM.*fedora.*bootc",
]

# File extensions to skip entirely (binary files, images, etc.)
SKIP_EXTENSIONS = {
    ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico",
    ".ttf", ".otf", ".woff", ".woff2",
    ".so", ".a", ".o",
    ".gz", ".xz", ".bz2", ".zst", ".zip", ".tar",
    ".rpm", ".deb",
    ".pyc", ".pyo",
    ".bin", ".img", ".qcow2", ".vmdk",
}

FEDORA_RE = re.compile(r"\bfedora\b", re.IGNORECASE)


def is_allowlisted_path(path: str) -> bool:
    p = path.replace("\\", "/")
    return any(re.match(pat, p) for pat in ALLOWLIST_PATH_PATTERNS)


def is_allowlisted_string(line: str) -> bool:
    return any(re.search(pat, line, re.IGNORECASE) for pat in ALLOWLIST_STRING_PATTERNS)


def scan_file(file_path: Path, root: Path) -> list[dict]:
    """Return list of violations: {file, line_no, line}"""
    violations = []
    rel = str(file_path.relative_to(root))

    if is_allowlisted_path(rel):
        return []
    if file_path.suffix.lower() in SKIP_EXTENSIONS:
        return []

    try:
        text = file_path.read_text(errors="replace")
    except (OSError, PermissionError):
        return []

    for i, line in enumerate(text.splitlines(), 1):
        if FEDORA_RE.search(line):
            if not is_allowlisted_string(line):
                violations.append({
                    "file": rel,
                    "line": i,
                    "content": line.rstrip()[:200],  # truncate long lines
                })
    return violations


def scan_tree(root: Path) -> list[dict]:
    violations = []
    for file_path in sorted(root.rglob("*")):
        if file_path.is_file():
            violations.extend(scan_file(file_path, root))
    return violations


def main() -> int:
    parser = argparse.ArgumentParser(description="Directioner OS rebrand lint")
    parser.add_argument("--root", required=True, help="Root directory to scan")
    parser.add_argument("--counts-only", action="store_true",
                        help="Only print counts, not individual violations")
    parser.add_argument("--json", action="store_true",
                        help="Output violations as JSON")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    if not root.is_dir():
        print(f"ERROR: {root} is not a directory", file=sys.stderr)
        return 2

    print(f"Scanning {root} for unallowlisted 'Fedora' strings...", file=sys.stderr)
    violations = scan_tree(root)

    if args.json:
        print(json.dumps({"violations": violations, "count": len(violations)}, indent=2))
    elif args.counts_only:
        print(f"violations: {len(violations)}")
    else:
        if violations:
            for v in violations:
                print(f"{v['file']}:{v['line']}: {v['content']}")
        print(f"\n--- Rebrand Lint Report ---")
        print(f"Total violations: {len(violations)}")
        if violations:
            print("STATUS: FAIL — unallowlisted 'Fedora' strings found")
        else:
            print("STATUS: PASS — no unallowlisted 'Fedora' strings found")

    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
