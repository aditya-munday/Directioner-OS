Name:           directioner-config
Version:        42.0.0
Release:        1%{?dist}
Summary:        Directioner OS system configuration and shell defaults

License:        MIT
URL:            https://github.com/aditya-munday/Directioner-OS
BuildArch:      noarch

Requires:       directioner-release
Requires:       fastfetch
Vendor:         Directioner OS Project

%description
Default system configuration for Directioner OS.
Includes:
  - /etc/skel defaults (XDG dirs, bash prompt)
  - fastfetch config with Directioner branding
  - Shell profile.d banner (minimal, no marketing)
  - Firefox/browser policies.json
  - Default shortcut map (with reserved AI key)
  - Hostname scheme configuration
  - Locale/timezone defaults

%install
mkdir -p %{buildroot}/etc/skel/{.config,.local/share,.cache}
mkdir -p %{buildroot}/etc/profile.d
mkdir -p %{buildroot}/etc/fastfetch
mkdir -p %{buildroot}/usr/lib/firefox/distribution
mkdir -p %{buildroot}/etc/directioner
mkdir -p %{buildroot}/usr/share/directioner/config

# Bash prompt
install -m 644 %{_sourcedir}/directioner-prompt.sh \
    %{buildroot}/etc/profile.d/directioner-prompt.sh

# fastfetch config
install -m 644 %{_sourcedir}/fastfetch.jsonc \
    %{buildroot}/etc/fastfetch/config.jsonc

# Firefox policies
install -m 644 %{_sourcedir}/policies.json \
    %{buildroot}/usr/lib/firefox/distribution/policies.json

# Shell banner (minimal)
install -m 644 %{_sourcedir}/directioner-banner.sh \
    %{buildroot}/etc/profile.d/directioner-banner.sh

# Shortcut map documentation
install -m 644 %{_sourcedir}/shortcuts.toml \
    %{buildroot}/usr/share/directioner/config/shortcuts.toml

# Hostname config (used by OOBE / first-boot)
install -m 644 %{_sourcedir}/hostname-scheme.conf \
    %{buildroot}/etc/directioner/hostname-scheme.conf

%files
/etc/skel/.config/
/etc/skel/.local/
/etc/skel/.cache/
/etc/profile.d/directioner-prompt.sh
/etc/profile.d/directioner-banner.sh
/etc/fastfetch/config.jsonc
/usr/lib/firefox/distribution/policies.json
/usr/share/directioner/config/shortcuts.toml
/etc/directioner/hostname-scheme.conf

%changelog
* 2026-10-18 Directioner OS Project <noreply@directioneros.example> - 42.0.0-1
- Initial config package (TODO 063, 064, 069, 083, 084, 085, 086)
