%{!?_unitdir: %global _unitdir /usr/lib/systemd/system}
%{!?_sharedstatedir: %global _sharedstatedir /var/lib}
%{!?systemd_post: %global systemd_post() : }
%{!?systemd_preun: %global systemd_preun() : }
%{!?systemd_postun_with_restart: %global systemd_postun_with_restart() : }
%global debug_package %{nil}

Name:           directioner-ai-engineer
Version:        0.1.0
Release:        1%{?dist}
Summary:        Resident multi-agent AI Engineer and capability control plane for Directioner-OS

License:        Apache-2.0
URL:            https://github.com/aditya-munday/Directioner-OS
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  systemd-rpm-macros
BuildRequires:  sqlite-devel
BuildRequires:  openssl-devel

Requires:       systemd
Requires:       dnf5
Requires:       sqlite

%description
Directioner-OS resident multi-agent AI Engineer subsystem.
Provides capability-scoped execution, quantitative blast-radius policy enforcement,
multi-signal health monitoring, structured compiler diagnostic repair, and closed-loop
resilience under strict user sovereignty.

%prep
%autosetup

%build
cargo build --release --manifest-path ai-engineer/Cargo.toml

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_unitdir}
mkdir -p %{buildroot}%{_sysconfdir}/ai-engineer
mkdir -p %{buildroot}%{_sharedstatedir}/ai-engineer

# Install control plane binaries
install -m 0755 ai-engineer/target/release/ai-capabilityd %{buildroot}%{_bindir}/ai-capabilityd
install -m 0755 ai-engineer/target/release/ai-worklog %{buildroot}%{_bindir}/ai-worklog
install -m 0755 ai-engineer/target/release/ai-killswitch %{buildroot}%{_bindir}/ai-killswitch
install -m 0755 ai-engineer/target/release/ai-worldmodel %{buildroot}%{_bindir}/ai-worldmodel
install -m 0755 ai-engineer/target/release/ai-engineer %{buildroot}%{_bindir}/ai-engineer
install -m 0755 ai-engineer/target/release/ai-cockpit %{buildroot}%{_bindir}/ai-cockpit
install -m 0755 ai-engineer/target/release/ai-health %{buildroot}%{_bindir}/ai-health
install -m 0755 ai-engineer/target/release/ai-hwcheck %{buildroot}%{_bindir}/ai-hwcheck

# Install systemd unit files
install -m 0644 image/configs/systemd/ai-capabilityd.service %{buildroot}%{_unitdir}/ai-capabilityd.service
install -m 0644 image/configs/systemd/ai-killswitch.service %{buildroot}%{_unitdir}/ai-killswitch.service

# Install default configuration
install -m 0644 image/configs/ai-engineer/self-healing.toml %{buildroot}%{_sysconfdir}/ai-engineer/self-healing.toml
mkdir -p %{buildroot}%{_sysconfdir}/default
install -m 0644 image/configs/ai-engineer/ai-capabilityd.default %{buildroot}%{_sysconfdir}/default/ai-capabilityd

%post
%systemd_post ai-capabilityd.service

%preun
%systemd_preun ai-capabilityd.service

%postun
%systemd_postun_with_restart ai-capabilityd.service

%files
%{_bindir}/ai-capabilityd
%{_bindir}/ai-worklog
%{_bindir}/ai-killswitch
%{_bindir}/ai-worldmodel
%{_bindir}/ai-engineer
%{_bindir}/ai-cockpit
%{_bindir}/ai-health
%{_bindir}/ai-hwcheck
%{_unitdir}/ai-capabilityd.service
%{_unitdir}/ai-killswitch.service
%dir %{_sysconfdir}/ai-engineer
%config(noreplace) %{_sysconfdir}/ai-engineer/self-healing.toml
%config(noreplace) %{_sysconfdir}/default/ai-capabilityd
%dir %{_sharedstatedir}/ai-engineer

%changelog
* Sun Aug 30 2026 Directioner-OS Core Team <core@directioner-os.org> - 0.1.0-1
- Initial Directioner-OS AI Engineer subsystem release for Fedora 42 bootc.
