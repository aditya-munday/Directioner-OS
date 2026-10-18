Name:           directioner-logos
Version:        42.0.0
Release:        1%{?dist}
Summary:        Directioner OS logo and icon assets

License:        CC-BY-SA-4.0
URL:            https://github.com/aditya-munday/Directioner-OS
BuildArch:      noarch

Provides:       system-logos = %{version}-%{release}
Obsoletes:      fedora-logos < 42
Conflicts:      fedora-logos
Vendor:         Directioner OS Project

%description
Logo and icon assets for Directioner OS.
Includes SVG master logo, PNG rasterizations at standard icon sizes,
favicon, and ASCII art logo for TTY/fastfetch.

NOTE: All assets in this package are original Directioner OS artwork.
No Fedora artwork or trademarks are included.

%install
mkdir -p %{buildroot}/usr/share/pixmaps
mkdir -p %{buildroot}/usr/share/directioner/logos
mkdir -p %{buildroot}/usr/share/icons/hicolor/{16x16,32x32,48x48,64x64,128x128,256x256}/apps
mkdir -p %{buildroot}/etc/directioner

# Install SVG placeholder (final asset provided by Aditya — TODO 058)
install -m 644 %{_sourcedir}/directioner-logo.svg \
    %{buildroot}/usr/share/pixmaps/directioner-logo.svg
install -m 644 %{_sourcedir}/directioner-logo.svg \
    %{buildroot}/usr/share/directioner/logos/directioner-logo.svg

# Install ASCII logo for fastfetch/TTY
install -m 644 %{_sourcedir}/directioner-ascii.txt \
    %{buildroot}/usr/share/directioner/logos/directioner-ascii.txt

%files
/usr/share/pixmaps/directioner-logo.svg
/usr/share/directioner/logos/
/usr/share/icons/hicolor/*/apps/

%changelog
* 2026-10-18 Directioner OS Project <noreply@directioneros.example> - 42.0.0-1
- Initial Directioner OS logos package (TODO 058)
