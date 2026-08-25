# Phase 0 — Foundation Checklist

Status tracking against the build guide (`AI_Native_Linux_OS_HowTo_Build_Guide.docx`, §3).

Legend: `[x]` done and verified · `[~]` scaffolded, needs CI/VM to verify · `[ ]` not started

## 0.1 Bootable custom image
- [~] `image/Containerfile` — FROM `quay.io/fedora/fedora-bootc:42`, adds neovim/tmux/htop + custom motd
- [~] `image/build.sh` — wraps podman/docker build + bootc-image-builder → qcow2
- [~] `.github/workflows/build-test.yml` — build image, emit qcow2, headless VM smoke test (checks motd on serial console)
- **Done when**: fresh VM boots and every added package/config is present.
- **Status**: image build runs in CI (needs podman/KVM — not runnable on this WSL2 dev box without sudo).

## 0.2 Capability Layer skeleton (Rust `ai-capabilityd`)
- [x] Unix-socket API, JSON-lines framing, `api_version` check
- [x] Typed capabilities: `ping`, `file.read`, `file.write(scope)`, `package.install`, `service.status`, `service.restart`
- [x] Path-scope enforcement: writes confined to `--work-root`; `..` traversal denied (not sanitized)
- [x] Approval gate: `package.install` / `service.restart` refuse when `approved=false` (in production config)
- [x] Dry-run by default: returns projected `dnf5`/`systemctl` commands, never executes
- [x] Protected-unit list (ssh.service, systemd-udevd.service) refuses restart
- [x] Kill-switch checked on *every* request, from the lock file (independent of daemon state)
- [x] Tests: 6 unit (handler) + 4 integration (over a real Unix socket, incl. kill-switch mid-flight)
- **Done when**: a `package.install` call goes through this service and a log entry is created. ✔ (verified live)

## 0.3 Audit log
- [x] `AuditLog` — append-only JSONL, fsync per line, sequence numbers survive restart
- [x] Written by the daemon itself; callers cannot skip logging
- [x] `ai-worklog` CLI: `tail [--lines N]`, `show <seq>`, `today`, `search <term>`
- [x] Content redaction: file-write bodies are recorded as `content_bytes_redacted: true`
- **Done when**: `ai-worklog tail` prints every action taken. ✔ (verified live)

## 0.4 Kill-switch
- [x] `ai-killswitch engage|release|status [--lock-file PATH]`; `engage --stop-service` also stops `ai-*` units via systemctl
- [x] Lock file is the single source of truth; daemon re-reads on every request
- [x] Cross-process test: one instance engages, an independent instance sees it immediately
- [x] `image/configs/systemd/ai-killswitch.service` — oneshot unit + CLI, bindable to key combo / recovery menu
- **Done when**: engaging mid-flight stops all AI activity within one second. ✔ (verified live: engaged → next request denied)

## Also scaffolding (Phases 1+)
- [~] `image/configs/systemd/ai-capabilityd.service` (noexec hardening, RuntimeDirectory, ProtectSystem)
- [~] SELinux + polkit policy placeholders (`image/configs/`)
- [ ] Phase 1.1 world-model, 1.2 planner/implementer, 1.3 dry-run UI, 1.4 worklog widget, 1.5 blast-radius rules, 1.6 benchmark suite

## Verification commands (dev mode, no root)
```sh
cd ai-engineer
cargo test --workspace

./target/debug/ai-capabilityd \
  --socket /tmp/ai-engineer/cap.sock --audit-log /tmp/ai-engineer/worklog.jsonl \
  --lock-file /tmp/ai-engineer/kill.lock --work-root /tmp/ai-engineer/scope
./target/debug/ai-killswitch engage --lock-file /tmp/ai-engineer/kill.lock
./target/debug/ai-worklog --log /tmp/ai-engineer/worklog.jsonl tail --lines 10
```