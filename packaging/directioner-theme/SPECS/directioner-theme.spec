Name:           directioner-theme
Version:        42.0.0
Release:        1%{?dist}
Summary:        Directioner OS GTK and GNOME Shell themes

License:        MIT
URL:            https://github.com/aditya-munday/Directioner-OS
BuildArch:      noarch
Vendor:         Directioner OS Project

Source0:        directioner-theme-%{version}.tar.gz

%description
Official GTK 3, GTK 4, and GNOME Shell themes for Directioner OS.
Provides Directioner-Dark and Directioner-Light variants built with
calibrated color tokens, glassmorphic styling, and high contrast accents.

%install
mkdir -p %{buildroot}/usr/share/themes/Directioner-Dark/gtk-3.0
mkdir -p %{buildroot}/usr/share/themes/Directioner-Dark/gtk-4.0
mkdir -p %{buildroot}/usr/share/themes/Directioner-Dark/gnome-shell
mkdir -p %{buildroot}/usr/share/themes/Directioner-Light/gtk-3.0
mkdir -p %{buildroot}/usr/share/themes/Directioner-Light/gtk-4.0
mkdir -p %{buildroot}/usr/share/themes/Directioner-Light/gnome-shell

cp -a %{_sourcedir}/directioner-dark/* %{buildroot}/usr/share/themes/Directioner-Dark/
cp -a %{_sourcedir}/directioner-light/* %{buildroot}/usr/share/themes/Directioner-Light/

%files
/usr/share/themes/Directioner-Dark
/usr/share/themes/Directioner-Light

%changelog
* 2026-10-18 Directioner OS Project <noreply@directioneros.example> - 42.0.0-1
- Initial Directioner OS theme package
