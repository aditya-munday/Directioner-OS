#!/usr/bin/env python3
"""
Directioner OS — Logo/Asset Provenance Scanner (TODO 089)
Scans the container image or repo for files matching known third-party-logo
filename patterns. Unknown assets fail CI.

Does NOT store or compare Fedora logo files (Rule B12 compliant).
Uses filename pattern rules only.

Usage:
  python3 scripts/asset_provenance_scan.py --root /path/to/image/root
  python3 scripts/asset_provenance_scan.py --root . --manifest design/asset-manifest.json

Exit codes:
  0 — all assets accounted for
  1 — unknown assets found
  2 — usage error
"""

import argparse
import json
import os
import re
import sys
from pathlib import Path


# Forbidden filename patterns — match known third-party logo filename conventions
# Rule: do not store comparison files; use NAME PATTERNS only
FORBIDDEN_PATTERNS = [
    r"(?i)fedora[_\-]?(logo|icon|mark|badge|spin|wordmark|symbol)",
    r"(?i)redhat[_\-]?(logo|icon|mark)",
    r"(?i)rhel[_\-]?(logo|icon|mark)",
    r"(?i)centos[_\-]?(logo|icon|mark)",
    r"(?i)\.fedoraproject\.org",
    r"(?i)freedom_forever",        # known Fedora wallpaper filename
    r"(?i)f\d{2}_background",      # f42_background etc.
    r"(?i)fedora_infinity",
]

# Asset extensions to scan
ASSET_EXTENSIONS = {
    ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico",
    ".bmp", ".tiff", ".webp", ".xpm",
    ".ttf", ".otf", ".woff", ".woff2",
}

# Paths to always skip
SKIP_PATHS = [
    r".*\.git/.*",
    r".*/target/.*",
    r".*/vendor/.*",
    r".*rebrand_lint\.py$",
    r".*asset_provenance_scan\.py$",
    r".*/docs/.*",    # docs may reference names in text — file content not checked here
]


def is_skip_path(rel: str) -> bool:
    return any(re.match(pat, rel.replace("\\", "/")) for pat in SKIP_PATHS)


def is_forbidden_filename(name: str) -> bool:
    return any(re.search(pat, name) for pat in FORBIDDEN_PATTERNS)


def load_allowlist(manifest_path: str | None) -> set[str]:
    if manifest_path is None:
        return set()
    manifest = Path(manifest_path)
    if not manifest.exists():
        return set()
    with open(manifest) as f:
        data = json.load(f)
    return {entry["file"] for entry in data.get("assets", [])}


def scan(root: Path, allowlist: set[str]) -> list[dict]:
    violations = []
    for file_path in sorted(root.rglob("*")):
        if not file_path.is_file():
            continue
        if file_path.suffix.lower() not in ASSET_EXTENSIONS:
            continue
        rel = str(file_path.relative_to(root)).replace("\\", "/")
        if is_skip_path(rel):
            continue
        if rel in allowlist:
            continue
        if is_forbidden_filename(file_path.name):
            violations.append({
                "file": rel,
                "filename": file_path.name,
                "reason": "matches forbidden third-party logo filename pattern",
            })
    return violations


def main() -> int:
    parser = argparse.ArgumentParser(description="Directioner OS asset provenance scanner")
    parser.add_argument("--root", required=True, help="Root directory to scan")
    parser.add_argument("--manifest", help="Path to asset allowlist manifest JSON")
    parser.add_argument("--json", action="store_true", help="Output as JSON")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    if not root.is_dir():
        print(f"ERROR: {root} is not a directory", file=sys.stderr)
        return 2

    allowlist = load_allowlist(args.manifest)
    violations = scan(root, allowlist)

    if args.json:
        print(json.dumps({"violations": violations, "count": len(violations)}, indent=2))
    else:
        for v in violations:
            print(f"VIOLATION: {v['file']}  ({v['reason']})")
        print(f"\n--- Asset Provenance Scan ---")
        print(f"Total violations: {len(violations)}")
        print("STATUS:", "FAIL" if violations else "PASS")

    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
