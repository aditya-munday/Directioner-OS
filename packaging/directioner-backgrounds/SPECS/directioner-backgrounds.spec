Name:           directioner-backgrounds
Version:        42.0.0
Release:        1%{?dist}
Summary:        Directioner OS default wallpapers and backgrounds

License:        CC-BY-SA-4.0
URL:            https://github.com/aditya-munday/Directioner-OS
BuildArch:      noarch

Provides:       desktop-backgrounds-basic
Obsoletes:      f42-backgrounds-base < 42
Conflicts:      f42-backgrounds-base
Vendor:         Directioner OS Project

%description
Default wallpapers and background images for Directioner OS.
Includes light and dark variants at common resolutions,
GNOME background XML metadata, and KDE plasma config.

All artwork is original Directioner OS artwork.
No Fedora artwork or trademarks are included.

License: CC-BY-SA-4.0
Provenance: See /usr/share/directioner/backgrounds/MANIFEST.json

%install
mkdir -p %{buildroot}/usr/share/backgrounds/directioner
mkdir -p %{buildroot}/usr/share/gnome-background-properties
mkdir -p %{buildroot}/usr/share/directioner/backgrounds
mkdir -p %{buildroot}/usr/share/wallpapers/Directioner

# Dark wallpaper (placeholder — SVG-rendered gradient)
install -m 644 %{_sourcedir}/directioner-dark.svg \
    %{buildroot}/usr/share/backgrounds/directioner/directioner-dark.svg
install -m 644 %{_sourcedir}/directioner-light.svg \
    %{buildroot}/usr/share/backgrounds/directioner/directioner-light.svg

# GNOME background properties XML
install -m 644 %{_sourcedir}/directioner-backgrounds.xml \
    %{buildroot}/usr/share/gnome-background-properties/directioner.xml

# Provenance manifest
install -m 644 %{_sourcedir}/MANIFEST.json \
    %{buildroot}/usr/share/directioner/backgrounds/MANIFEST.json

%files
/usr/share/backgrounds/directioner/
/usr/share/gnome-background-properties/directioner.xml
/usr/share/directioner/backgrounds/MANIFEST.json

%changelog
* 2026-10-18 Directioner OS Project <noreply@directioneros.example> - 42.0.0-1
- Initial Directioner OS backgrounds package (TODO 059)
