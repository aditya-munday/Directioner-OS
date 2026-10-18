Name:           directioner-repos
Version:        42.0.0
Release:        1%{?dist}
Summary:        Directioner OS RPM repository configuration

License:        MIT
URL:            https://github.com/aditya-munday/Directioner-OS
BuildArch:      noarch

Vendor:         Directioner OS Project

%description
Repository configuration files for Directioner OS.
Configures the Directioner OS package repository and
preserves upstream Fedora repositories for base system packages.

NOTE: No real repository URLs or GPG keys are embedded in this package.
Repository URL and GPG key are read from /etc/directioner/platform.toml at runtime.
GPG keys must be provided by the Directioner OS signing infrastructure (OWNER-AUTH TODO 497).

%install
mkdir -p %{buildroot}/etc/yum.repos.d
mkdir -p %{buildroot}/etc/pki/rpm-gpg
mkdir -p %{buildroot}/etc/directioner

# Directioner OS main repo (URL sourced from platform.toml at runtime)
cat > %{buildroot}/etc/yum.repos.d/directioner.repo << 'EOF'
[directioner]
name=Directioner OS $releasever - $basearch
baseurl=@DIRECTIONER_REPO_URL@/$releasever/$basearch/
enabled=1
gpgcheck=1
gpgkey=file:///etc/pki/rpm-gpg/RPM-GPG-KEY-directioner
skip_if_unavailable=True
metadata_expire=6h
priority=10

[directioner-source]
name=Directioner OS $releasever - Source
baseurl=@DIRECTIONER_REPO_URL@/$releasever/source/
enabled=0
gpgcheck=1
gpgkey=file:///etc/pki/rpm-gpg/RPM-GPG-KEY-directioner
EOF

# GPG key placeholder — REPLACE with real key (OWNER-AUTH TODO 497)
cat > %{buildroot}/etc/pki/rpm-gpg/RPM-GPG-KEY-directioner << 'EOF'
# PLACEHOLDER — Replace with real Directioner OS GPG public key (OWNER-AUTH TODO 497)
# This file intentionally empty until the signing key is generated.
EOF

%files
%config(noreplace) /etc/yum.repos.d/directioner.repo
/etc/pki/rpm-gpg/RPM-GPG-KEY-directioner

%changelog
* 2026-10-18 Directioner OS Project <noreply@directioneros.example> - 42.0.0-1
- Initial Directioner OS repos package (TODO 067)
