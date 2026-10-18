#!/usr/bin/env python3
"""
Directioner OS — brand.toml validator / generator (TODO 092)
Validates brand.toml against the required schema and rejects bad configs.

Usage:
  python3 scripts/validate_brand_toml.py               # validate brand.toml
  python3 scripts/validate_brand_toml.py --file path   # validate a specific file

Exit codes:
  0 — valid
  1 — validation error
"""

import argparse
import sys
import os

try:
    import tomllib
except ImportError:
    try:
        import tomli as tomllib  # type: ignore
    except ImportError:
        print("ERROR: tomllib (Python 3.11+) or tomli required", file=sys.stderr)
        sys.exit(1)

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

REQUIRED_FIELDS = {
    "brand": ["name", "short_name", "id", "vendor", "tagline"],
    "version": ["major", "minor", "patch", "channel", "version_string", "fedora_base_version"],
    "urls": ["homepage", "documentation", "support", "bug_report"],
    "theme": ["primary_color", "accent_color", "background_dark", "background_light"],
}

VALID_CHANNELS = {"stable", "beta", "dev"}


def validate(brand: dict) -> list[str]:
    errors = []

    # Required sections and fields
    for section, fields in REQUIRED_FIELDS.items():
        if section not in brand:
            errors.append(f"Missing section: [{section}]")
            continue
        for field in fields:
            if field not in brand[section]:
                errors.append(f"Missing field: [{section}].{field}")

    # Channel validation
    channel = brand.get("version", {}).get("channel", "")
    if channel and channel not in VALID_CHANNELS:
        errors.append(f"Invalid channel '{channel}'. Must be one of: {VALID_CHANNELS}")

    # ID must be lowercase alphanumeric + hyphens
    brand_id = brand.get("brand", {}).get("id", "")
    if brand_id and not all(c.isalnum() or c == "-" for c in brand_id):
        errors.append(f"brand.id must be lowercase alphanumeric+hyphens, got: {brand_id!r}")

    # Color fields must be #RRGGBB
    import re
    hex_re = re.compile(r"^#[0-9a-fA-F]{6}$")
    for color_field in ["primary_color", "accent_color", "background_dark", "background_light",
                        "text_dark", "text_light"]:
        val = brand.get("theme", {}).get(color_field, "")
        if val and not hex_re.match(val):
            errors.append(f"theme.{color_field} must be #RRGGBB format, got: {val!r}")

    # URLs must start with https://
    for url_field, url_val in brand.get("urls", {}).items():
        if url_val and not url_val.startswith("https://"):
            errors.append(f"urls.{url_field} must start with https://, got: {url_val!r}")

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description="Directioner OS brand.toml validator")
    parser.add_argument("pos_file", nargs="?", default=None,
                        help="Optional positional path to brand.toml")
    parser.add_argument("--file", default=None,
                        help="Path to brand.toml (default: repo root brand.toml)")
    args = parser.parse_args()

    path = args.file or args.pos_file or os.path.join(REPO_ROOT, "brand.toml")
    if not os.path.exists(path):
        print(f"ERROR: File not found: {path}", file=sys.stderr)
        return 1

    with open(path, "rb") as f:
        try:
            brand = tomllib.load(f)
        except Exception as e:
            print(f"ERROR: TOML parse error in {path}: {e}", file=sys.stderr)
            return 1

    errors = validate(brand)
    if errors:
        print(f"FAIL: {path} has {len(errors)} validation error(s):")
        for err in errors:
            print(f"  - {err}")
        return 1

    print(f"OK: {path} is valid")
    return 0


if __name__ == "__main__":
    sys.exit(main())
