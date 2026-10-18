#!/usr/bin/env python3
"""
Directioner OS — Brand Token Generator (TODO 078, 092)
Reads brand.toml + design/tokens.json and emits:
  - GRUB/BLS color variables     → build/grub-colors.cfg
  - Plymouth color script inject → build/plymouth-colors.inc
  - GTK CSS variables            → build/gtk-tokens.css
  - Qt/Kvantum color map         → build/qt-tokens.conf
  - Shell environment variables  → build/brand-env.sh

Output is deterministic (sorted keys). Run with `--check` to verify
diff-free regeneration (used in CI — TODO 078 verification).

Usage:
  python3 scripts/generate_brand_tokens.py
  python3 scripts/generate_brand_tokens.py --check   # fail if output changed
"""

import json
import sys
import argparse
import hashlib
import os
try:
    import tomllib
except ImportError:
    try:
        import tomli as tomllib  # type: ignore
    except ImportError:
        print("ERROR: tomllib (Python 3.11+) or tomli package required", file=sys.stderr)
        sys.exit(1)

REPO_ROOT  = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BRAND_TOML = os.path.join(REPO_ROOT, "brand.toml")
TOKENS_JSON = os.path.join(REPO_ROOT, "design", "tokens.json")
BUILD_DIR  = os.path.join(REPO_ROOT, "build", "brand")


def load_brand() -> dict:
    with open(BRAND_TOML, "rb") as f:
        return tomllib.load(f)


def load_tokens() -> dict:
    with open(TOKENS_JSON) as f:
        return json.load(f)


def hex_to_rgb_floats(hex_color: str) -> tuple[float, float, float]:
    """Convert #RRGGBB to (r, g, b) in [0,1] range."""
    h = hex_color.lstrip("#")
    if len(h) != 6:
        raise ValueError(f"Expected 6-char hex color, got: {hex_color!r}")
    r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
    return r / 255.0, g / 255.0, b / 255.0


def generate_grub_colors(brand: dict) -> str:
    theme = brand["theme"]
    primary = theme["primary_color"]
    bg_dark = theme["background_dark"]
    lines = [
        "# Directioner OS GRUB color variables (auto-generated — do not edit)",
        f"# Source: brand.toml  primary={primary}  bg={bg_dark}",
        f"set color_normal=white/black",
        f"set color_highlight=cyan/black",
        f"set menu_color_normal=white/black",
        f"set menu_color_highlight=cyan/black",
        f"set DIRECTIONER_PRIMARY=\"{primary}\"",
        f"set DIRECTIONER_BG=\"{bg_dark}\"",
        f"set DIRECTIONER_VERSION=\"{brand['version']['version_string']}\"",
        f"set DIRECTIONER_CODENAME=\"{brand['version']['codename']}\"",
    ]
    return "\n".join(lines) + "\n"


def generate_plymouth_colors(brand: dict) -> str:
    theme = brand["theme"]
    primary = theme["primary_color"]
    r, g, b = hex_to_rgb_floats(primary)
    bg_r, bg_g, bg_b = hex_to_rgb_floats(theme["background_dark"])
    lines = [
        "/* Directioner OS Plymouth color injection (auto-generated) */",
        f"/* primary: {primary} */",
        f"BRAND_R := {r:.4f};",
        f"BRAND_G := {g:.4f};",
        f"BRAND_B := {b:.4f};",
        f"BG_R    := {bg_r:.4f};",
        f"BG_G    := {bg_g:.4f};",
        f"BG_B    := {bg_b:.4f};",
    ]
    return "\n".join(lines) + "\n"


def generate_gtk_css(tokens: dict) -> str:
    colors = tokens.get("color", {})
    lines = [
        "/* Directioner OS GTK CSS tokens (auto-generated — do not edit) */",
        ":root {",
    ]
    for name, value in sorted(colors.items()):
        if isinstance(value, str):
            lines.append(f"  --dos-color-{name}: {value};")
        elif isinstance(value, dict):
            for variant, hex_val in sorted(value.items()):
                lines.append(f"  --dos-color-{name}-{variant}: {hex_val};")
    # Spacing
    spacing = tokens.get("spacing", {})
    for name, value in sorted(spacing.items()):
        lines.append(f"  --dos-spacing-{name}: {value};")
    # Typography
    typo = tokens.get("typography", {})
    for key, value in sorted(typo.items()):
        if isinstance(value, str):
            lines.append(f"  --dos-type-{key}: {value};")
    lines.append("}")
    return "\n".join(lines) + "\n"


def generate_qt_conf(tokens: dict) -> str:
    colors = tokens.get("color", {})
    lines = [
        "# Directioner OS Qt/Kvantum color map (auto-generated)",
        "[Colors:Window]",
    ]
    # Flatten colors
    for name, value in sorted(colors.items()):
        if isinstance(value, str):
            lines.append(f"dos_color_{name}={value}")
        elif isinstance(value, dict):
            for variant, hex_val in sorted(value.items()):
                lines.append(f"dos_color_{name}_{variant}={hex_val}")
    return "\n".join(lines) + "\n"


def generate_brand_env(brand: dict, tokens: dict) -> str:
    theme = brand["theme"]
    ver = brand["version"]
    lines = [
        "# Directioner OS brand environment variables (auto-generated)",
        "# Source: build/brand/brand-env.sh",
        f"export DOS_BRAND_NAME=\"{brand['brand']['name']}\"",
        f"export DOS_BRAND_ID=\"{brand['brand']['id']}\"",
        f"export DOS_VERSION=\"{ver['version_string']}\"",
        f"export DOS_CODENAME=\"{ver['codename']}\"",
        f"export DOS_CHANNEL=\"{ver['channel']}\"",
        f"export DOS_PRIMARY_COLOR=\"{theme['primary_color']}\"",
        f"export DOS_ACCENT_COLOR=\"{theme['accent_color']}\"",
        f"export DOS_BG_DARK=\"{theme['background_dark']}\"",
        f"export DOS_BG_LIGHT=\"{theme['background_light']}\"",
    ]
    return "\n".join(lines) + "\n"


def write_or_check(path: str, content: str, check_mode: bool) -> bool:
    """Write file or verify it matches. Returns True if content changed."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    new_hash = hashlib.sha256(content.encode()).hexdigest()
    if os.path.exists(path):
        with open(path) as f:
            old_content = f.read()
        old_hash = hashlib.sha256(old_content.encode()).hexdigest()
        if old_hash == new_hash:
            return False  # no change
    if check_mode:
        return True  # changed but not writing
    with open(path, "w") as f:
        f.write(content)
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description="Directioner OS brand token generator")
    parser.add_argument("--check", action="store_true",
                        help="Check mode: fail with exit code 1 if any output would change")
    args = parser.parse_args()

    brand  = load_brand()
    tokens = load_tokens()

    outputs = [
        (os.path.join(BUILD_DIR, "grub-colors.cfg"),    generate_grub_colors(brand)),
        (os.path.join(BUILD_DIR, "plymouth-colors.inc"), generate_plymouth_colors(brand)),
        (os.path.join(BUILD_DIR, "gtk-tokens.css"),      generate_gtk_css(tokens)),
        (os.path.join(BUILD_DIR, "qt-tokens.conf"),      generate_qt_conf(tokens)),
        (os.path.join(BUILD_DIR, "brand-env.sh"),        generate_brand_env(brand, tokens)),
    ]

    changed = []
    for path, content in outputs:
        if write_or_check(path, content, args.check):
            changed.append(path)
        if not args.check:
            print(f"  wrote {os.path.relpath(path, REPO_ROOT)}")

    if args.check and changed:
        print("FAIL: Token outputs would change. Run generate_brand_tokens.py to update:",
              file=sys.stderr)
        for p in changed:
            print(f"  {os.path.relpath(p, REPO_ROOT)}", file=sys.stderr)
        return 1

    if args.check:
        print("OK: All token outputs are up to date.")
    else:
        print(f"Generated {len(outputs)} files in build/brand/")
    return 0


if __name__ == "__main__":
    sys.exit(main())
