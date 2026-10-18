Name:           directioner-release
Version:        42.0.0
Release:        1%{?dist}
Summary:        Directioner OS release files and os-release

License:        MIT
URL:            https://github.com/aditya-munday/Directioner-OS
BuildArch:      noarch

Provides:       system-release = %{version}-%{release}
Provides:       system-release-cpe
Obsoletes:      fedora-release < 42
Conflicts:      fedora-release
Vendor:         Directioner OS Project

%description
Core release identification files for Directioner OS.
Provides /etc/os-release, /etc/system-release, /etc/system-release-cpe,
/etc/issue, /etc/issue.net, and /etc/motd.

%install
mkdir -p %{buildroot}/etc
mkdir -p %{buildroot}/usr/lib/os-release.d

# os-release
cat > %{buildroot}/etc/os-release << 'EOF'
NAME="Directioner OS"
VERSION="42.0.0 (Genesis)"
ID=directioner
ID_LIKE=fedora
VERSION_ID=42
PLATFORM_ID="platform:dos42"
PRETTY_NAME="Directioner OS 42 (Genesis)"
ANSI_COLOR="0;34"
LOGO=directioner-logo
CPE_NAME="cpe:/o:directioneros:directioner:42"
DEFAULT_HOSTNAME="directioner"
HOME_URL="https://github.com/aditya-munday/Directioner-OS"
DOCUMENTATION_URL="https://github.com/aditya-munday/Directioner-OS#readme"
SUPPORT_URL="https://github.com/aditya-munday/Directioner-OS/issues"
BUG_REPORT_URL="https://github.com/aditya-munday/Directioner-OS/issues"
VARIANT_ID=server
VARIANT="Server Edition"
BUILD_ID=@BUILD_ID@
IMAGE_ID=@IMAGE_ID@
IMAGE_VERSION=@IMAGE_VERSION@
EOF

# system-release
echo "Directioner OS release 42.0.0 (Genesis)" > %{buildroot}/etc/system-release

# system-release-cpe
echo "cpe:/o:directioneros:directioner:42" > %{buildroot}/etc/system-release-cpe

# issue
cat > %{buildroot}/etc/issue << 'EOF'
 ____  _               _   _                      ___  ____
|  _ \(_)_ __ ___  ___| |_(_) ___  _ __   ___ _ _/ _ \/ ___|
| | | | | '__/ _ \/ __| __| |/ _ \| '_ \ / _ \ '__| | | \___ \
| |_| | | | |  __/ (__| |_| | (_) | | | |  __/ |  | |_| |___) |
|____/|_|_|  \___|\___|\__|_|\___/|_| |_|\___|_|   \___/|____/

  Directioner OS 42.0.0 (Genesis) — The AI-Native Operating System
  Kernel \r on \m (\l)

EOF

# issue.net (no escape sequences)
cat > %{buildroot}/etc/issue.net << 'EOF'
Directioner OS 42.0.0 (Genesis) — The AI-Native Operating System
EOF

# motd
cat > %{buildroot}/etc/motd << 'EOF'
Welcome to Directioner OS 42.0.0 (Genesis)
The AI-Native Operating System

  Image: @IMAGE_ID@ | Build: @BUILD_ID@

  docs: https://github.com/aditya-munday/Directioner-OS#readme
  bugs: https://github.com/aditya-munday/Directioner-OS/issues

EOF

%files
/etc/os-release
/etc/system-release
/etc/system-release-cpe
/etc/issue
/etc/issue.net
/etc/motd

%changelog
* 2026-10-18 Directioner OS Project <noreply@directioneros.example> - 42.0.0-1
- Initial Directioner OS release package (TODO 054, 055)
