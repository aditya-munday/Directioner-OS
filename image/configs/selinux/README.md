# SELinux policy for the AI Engineer Capability Layer.
#
# The Capability Layer's capability tokens are enforced logically in Rust, but a
# compromised daemon must NOT be able to exceed its scope at the kernel/LSM level.
# That second line of defense is a dedicated SELinux domain, ai_capabilityd_t,
# that can only:
#   - read the type enforcement policy and its own runtime directories,
#   - execute narrowly-scoped helper transitions (ai_pkg_t, ai_svc_t, ai_file_t)
#     for package/service/file operations,
#   - write to the audit log domain (ai_audit_log_t) — append only.
#
# TODO(Phase 0): drop in a real .te module built against the Fedora policy.
# Reference skeleton expected layout:
#   ai_capabilityd.te  -> type ai_capabilityd_t; domain_trans for adapters
#   ai_capabilityd.fc -> file_contexts for /run/ai-capabilityd, /var/lib/ai-engineer
#
# Keep this module GNU-style .te (systemd-context-free) so it compiles with
# `make -f /usr/share/selinux/devel/Makefile ai_capabilityd.pp`.