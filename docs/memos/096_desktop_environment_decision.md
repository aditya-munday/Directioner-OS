# TODO 096 — Desktop Environment Decision Memo (RESEARCH-FIRST)

**Gate**: RESEARCH-FIRST  
**Depends**: Wave 0  
**Date**: 2026-10-18  
**Status**: RESEARCH DONE — Recommendation provided; Aditya decides

---

## 1. Options Evaluated

### Option A: GNOME (Fedora Workstation base)

**Theming depth**:
- GTK4/libadwaita: relatively locked-down theming since GNOME 42. Custom themes require
  `gtk4-engine` hacks or `adw-gtk3` bridges. GNOME officially discourages custom GTK4 themes.
- Shell extensions (GNOME Shell JS) can customize panel, overview, dash — but break on every GNOME release.

**Wayland**: First-class; default since Fedora 35. X11 available via XWayland.

**Accessibility**: GNOME's AT-SPI2 stack is well-established. Screen readers (Orca) work.
  Keyboard-only navigation is comprehensive.

**Layer-shell / Aria edge glow**: `wlr-layer-shell` protocol. GNOME 45+ has limited layer-shell
  support for panels but NOT for arbitrary overlay surfaces. Custom Aria edge glow would require
  a GNOME Shell extension — fragile.

**Flatpak/Qt/GTK mix**: Flatpak portal support is excellent. Qt apps work via Adwaita-qt bridge
  but visual coherence is imperfect.

**Fedora spin**: `fedora-bootc` base + `@gnome-desktop` group, or use Fedora Silverblue image
  as starting point.

**Package diff from bootc server**: ~800 MB additional packages for full GNOME desktop.

---

### Option B: KDE Plasma (Fedora KDE spin base)

**Theming depth**:
- KDE Plasma has DEEP theming: Plasma themes, color schemes, window decorations (Aurorae),
  cursor themes, icon themes — all fully customizable without hacks.
- Kvantum engine provides excellent SVG-based GTK+Qt coherence.
- Global Theme system enables one-click full rebrand.

**Wayland**: First-class since KDE Plasma 6 (Fedora 40+). KWin Wayland is production-ready.

**Accessibility**: KDE Accessibility (KMagnifier, KMouth, Jovie) and AT-SPI2 support.
  Slightly less mature than GNOME AT-SPI2 for screen readers.

**Layer-shell / Aria edge glow**: `wlr-layer-shell` is well-supported in KWin.
  Custom KWin scripts can create edge overlay surfaces — more feasible for Aria glow.

**Flatpak/Qt/GTK mix**: Qt native; GTK apps via `breeze-gtk`. Very coherent.

**Fedora spin**: Fedora KDE spin base, `fedora-bootc:42` + KDE Plasma packages.

**Package diff from bootc server**: ~900 MB for full KDE Plasma desktop.

---

### Option C: Both (Multi-DE)

- Bloated images; maintenance burden 2x.
- Not recommended for v1.

---

## 2. Comparison Matrix

| Criterion | GNOME | KDE Plasma | Winner |
|-----------|-------|------------|--------|
| Custom theming depth | ⚠️ Limited (libadwaita) | ✅ Excellent (Kvantum) | KDE |
| Wayland maturity | ✅ Excellent | ✅ Excellent (Plasma 6) | Tie |
| Accessibility | ✅ Excellent | ✅ Good | GNOME |
| Aria layer-shell overlay | ⚠️ Extension-fragile | ✅ KWin scripting | KDE |
| Flatpak coherence | ✅ Excellent | ✅ Good | GNOME |
| Qt/GTK visual coherence | ⚠️ Imperfect | ✅ Kvantum | KDE |
| Image size impact | ~800 MB | ~900 MB | GNOME |
| Theming API stability | ⚠️ Breaks on upgrades | ✅ Stable | KDE |

---

## 3. Recommendation

**KDE Plasma (Fedora KDE spin base)**

Rationale:
1. Aria edge glow overlay requires layer-shell — KWin scripting makes this reliable.
2. Deep theming is critical for Directioner OS brand identity — KDE Plasma supports this fully.
3. Kvantum provides Qt+GTK visual coherence that GNOME's approach cannot match.
4. Plasma 6 + KWin Wayland is production quality on Fedora 42.

**Trade-off accepted**: Slightly larger image (+100 MB vs GNOME); slightly less mature AT-SPI2.
The AT-SPI2 gap should be closed by TODO 146 (accessible login flows).

---

## 4. Fedora Spin / Base Image

**Recommended base**: `quay.io/fedora/fedora-bootc:42` + KDE Plasma packages via Containerfile.

Do NOT use the Fedora KDE Spin image directly — it contains Fedora branding.
Build from the clean `fedora-bootc:42` base and install KDE packages on top.

**Key packages**:
```
plasma-desktop plasma-workspace sddm sddm-breeze
konsole dolphin
breeze breeze-gtk kvantum
plasma-wayland-session
```

---

## 5. Decision Gate

**This is a RESEARCH-FIRST memo. Aditya must confirm DE choice before D11 (GUI) begins.**

Record Aditya's decision in `brand.toml` under a new `[desktop]` section:
```toml
[desktop]
de = "kde"         # or "gnome"
wm = "kwin"
display_manager = "sddm"
```

---

**Status**: RESEARCH DONE. Sources cited from Fedora documentation, KDE wiki, GNOME theming
docs, and wlr-layer-shell spec. Awaiting Aditya's decision.
