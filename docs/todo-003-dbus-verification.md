# TODO 003 – D-Bus Study: Verification Evidence

**Date:** 2026-10-10  
**Gate:** RESEARCH-FIRST  
**Purpose:** Show that the memo (todo-003-dbus-study.md) cites real sources and no code was written.

## Evidence

### Claim verification

| Claim | Source |
|---|---|
| D-Bus introspection via `org.freedesktop.DBus.Introspectable` | https://dbus.freedesktop.org/doc/dbus-specification.html §8.1 |
| `GetConnectionCredentials` returns PID/UID/SELinux label | https://dbus.freedesktop.org/doc/dbus-specification.html §7.2 |
| zbus 4.x is the canonical pure-Rust D-Bus binding | https://dbus2.github.io/zbus/ |
| dbus-broker is the default broker on Fedora 42 | `dnf info dbus-broker` on Fedora 42 base image |
| `SO_PEERCRED` at `accept(2)` time is kernel-direct, no broker relay | `man 7 socket` "SCM_CREDENTIALS" / `man 2 getsockopt` |
| rpm-ostree exposes `org.projectatomic.rpmostree1` on system bus | https://github.com/coreos/rpm-ostree/blob/main/src/daemon/rpmostree-dbus-util.cxx |
| Directioner-OS current UID allowlist is in server.rs | `git grep SO_PEERCRED ai-engineer/capability-layer/src/server.rs` |

### No code written
```
$ git diff main..plan/003-dbus-study-1 --name-only
docs/todo-003-dbus-study.md
docs/todo-003-dbus-verification.md
```
Both files are documentation only. Zero `.rs` files changed.

### Current peer-cred enforcement site (evidence)
```
$ grep -n "SO_PEERCRED\|peer_cred\|peercred\|allowlist\|uid" \
    ai-engineer/capability-layer/src/server.rs | head -25
27:    pub allowed_uids: Vec<u32>,
37:    /// Parses a comma-separated string of UIDs and appends unique parsed values to `uids`.
38:    pub fn parse_uids_into(raw: &str, uids: &mut Vec<u32>) {
40:            if let Ok(uid) = part.trim().parse::<u32>()
41:                && !uids.contains(&uid)
43:                uids.push(uid);
51:        let current_uid = rustix::process::getuid().as_raw();
52:        let mut uids = vec![current_uid];
53:        if current_uid != 0 {
54:            uids.push(0);
56:        if let Ok(env_uids) = std::env::var("DIRECTIONER_ALLOWED_UIDS") {
57:            Self::parse_uids_into(&env_uids, &mut uids);
60:            allowed_uids: uids,
236:    // AUDIT-CAP-02: Enforce caller UID verification via SO_PEERCRED
237:    let peer_uid = match rustix::net::sockopt::socket_peercred(&stream) {
238:        Ok(ucred) => ucred.uid.as_raw(),
240:            eprintln!("failed to query SO_PEERCRED on incoming connection: {e}");
243:                reason: format!("unable to verify caller identity via SO_PEERCRED: {e}"),
255:    if !config.allowed_uids.contains(&peer_uid) {
257:            "rejecting connection: caller UID {peer_uid} not in allowed_uids {:?}",
258:            config.allowed_uids
263:                "caller UID {peer_uid} is not authorized to connect to ai-capabilityd (allowed: {:?})",
264:                config.allowed_uids
```
Confirms: UID verification happens at line 237 via `socket_peercred()` — kernel-direct, no broker relay.

## Recommendation status
Recommendation in §3.5 is: **Do nothing** — keep Unix socket.  
Gate cleared: memo complete, no code changes.  
**Next step: Aditya approves or rejects. If approved, record in plan-status.md and move to TODO 004.**

