# Directioner-OS configs

Layered configuration for the bootc image (`Containerfile` copies/bakes these in,
or they ship inside the Phase 0 RPMs). Configuration only — no Fedora source edits.

- `systemd/` — unit files for the AI Engineer services
- `selinux/` — SELinux policy modules enforcing the Capability Layer token model
- `polkit/` — polkit rules for user-initiated privileged operations

## systemd/ units (Phase 0)

| Unit | Purpose |
|------|---------|
| `ai-capabilityd.service` | The typed Capability Layer daemon (Phase 0.2) |
| `ai-killswitch.service`  | One-shot unit that engages the kill-switch and stops all `ai-*` units (Phase 0.4) |
| `ai-worklog.service`     | (optional) exposes the audit-log reader (`ai-worklog`) as a service |