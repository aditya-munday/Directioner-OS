Name:           directioner-plymouth
Version:        42.0.0
Release:        1%{?dist}
Summary:        Directioner OS Plymouth boot splash theme

License:        GPL-2.0-or-later
URL:            https://github.com/aditya-munday/Directioner-OS
BuildArch:      noarch

Requires:       plymouth
Requires:       directioner-logos
Vendor:         Directioner OS Project

%description
Plymouth boot splash theme for Directioner OS.
Provides a branded boot splash with progress indicator and text fallback.
LUKS passphrase prompt rendering is preserved.

%install
mkdir -p %{buildroot}/usr/share/plymouth/themes/directioner

install -m 644 %{_sourcedir}/directioner.plymouth \
    %{buildroot}/usr/share/plymouth/themes/directioner/directioner.plymouth
install -m 644 %{_sourcedir}/directioner.script \
    %{buildroot}/usr/share/plymouth/themes/directioner/directioner.script
install -m 644 %{_sourcedir}/directioner-logo.png \
    %{buildroot}/usr/share/plymouth/themes/directioner/directioner-logo.png

%post
if [ $1 -eq 1 ]; then
    plymouth-set-default-theme directioner --rebuild-initrd 2>/dev/null || true
fi

%files
/usr/share/plymouth/themes/directioner/

%changelog
* 2026-10-18 Directioner OS Project <noreply@directioneros.example> - 42.0.0-1
- Initial Plymouth theme (TODO 057)
