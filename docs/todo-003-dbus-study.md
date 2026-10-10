# TODO 003 – D-Bus Interface Study

**Gate:** RESEARCH-FIRST  
**Status:** Memo complete, awaiting Aditya's approval before any code  
**Date:** 2026-10-10  
**Author:** Antigravity agent  
**Depends on:** TODO 001 (wire limits complete), TODO 002 (subprocess timeouts complete)

---

## 3.1  What D-Bus would add over the Unix socket

The current IPC is a newline-framed JSON protocol over a Unix domain socket
(`/run/ai-capabilityd/ai-capabilityd.sock`, permissions 0600, authenticated
with `SO_PEERCRED`).

### What D-Bus offers that the Unix socket today does not

| Feature | Current Unix socket | D-Bus |
|---|---|---|
| **Introspection** | No schema exposed at runtime | Exposes XML introspection (`org.freedesktop.DBus.Introspectable`) that tools (`gdbus`, `busctl`, `d-spy`) can query live |
| **Activation on demand** | Daemon must be pre-started | `dbus-daemon`/`systemd --user` can socket-activate the service on first call |
| **Method call matching** | Hand-written dispatch in `handler.rs` | Declarative object/interface/method namespace; language bindings auto-generate dispatch stubs |
| **Signal / broadcast** | Requires a second channel or polling | Native publish/subscribe signals; well-suited for `WorkLogEntry` and `KillSwitchState` events |
| **Policy files** | Application-level only | `/etc/dbus-1/system.d/` policies restrict _which_ bus names a UID may own or call; kernel-enforced |
| **SELinux integration** | Current: `SO_PEERCRED` UID check only | D-Bus sends the peer's SELinux label; policy can restrict at the label level |
| **Tool interoperability** | None without custom client | `busctl call`, `gdbus`, `dbus-send` work out-of-the-box for debugging |
| **Named errors** | JSON `{"status":"error","message":…}` | `org.freedesktop.DBus.Error.*` names; structured, introspectable |

**Summary of gains:** introspection + signals + richer policy integration + standard tooling.

---

## 3.2  What D-Bus would cost

### Attack surface
- Adds `dbus-daemon` (or `dbus-broker`) as a trusted intermediary. Any bug in
  the broker is in the trust chain.
- Policy file misconfigurations can inadvertently grant access to unanticipated
  callers.
- `SO_PEERCRED` on the Unix socket identifies the caller with _kernel_ certainty;
  D-Bus `GetConnectionCredentials` depends on the broker relaying it — one
  more hop to audit.
- D-Bus allows third-party applications to call the service by name on the
  system bus without needing to know the socket path, widening the exposure
  surface.

### SELinux complexity
- Every method call needs an AVC rule of the form
  `allow <caller_t> <service_t>:<dbus_class> send_msg;`.
- The `dbus_t` domain must be added as a transitively trusted intermediary.
- Currently the daemon uses `SO_PEERCRED` uid-allowlist enforcement (lines 40–80
  of `server.rs`), which requires zero SELinux policy changes. D-Bus requires
  new policy to be written, audited, and tested on each SELinux policy update.

### New dependency
- `zbus` (pure Rust) or `dbus-rs` (C FFI to `libdbus`).
  - `zbus` adds ~12 crates; no C FFI; supports async with Tokio.
  - `dbus-rs` adds `libdbus-1-dev` as a build dependency; links a C library at
    runtime.
- The broker itself (`dbus-daemon` / `dbus-broker`) must be running for the
  service to start. If D-Bus dies, ai-capabilityd becomes unreachable.

### Integration with rpm-ostree live layering
- rpm-ostree uses `org.projectatomic.rpmostree1` on the system bus.
- Having two D-Bus services in the same codebase (our service and our rpm-ostree
  client proxy) adds complexity and may create circular activation dependencies.

---

## 3.3  zbus vs raw socket for this codebase

| Criterion | `zbus` (D-Bus) | Current Unix socket + JSON |
|---|---|---|
| **Lines of code** | Less dispatch boilerplate (macros generate interfaces) | More explicit dispatch (already written) |
| **Dependencies** | +12 crates (tokio-based async) | Already in tree |
| **Type safety** | D-Bus type system (signatures); compile-time via macros | serde + typed enums (already done) |
| **Streaming/async** | Native async; `tokio` integration | Current: synchronous loop with thread per connection (server.rs) |
| **Testing** | `zbus::connection::Builder` can use in-process bus | `std::os::unix::net::UnixListener` — easily mocked |
| **Introspection** | Free via `#[dbus_interface]` | Zero without extra work |
| **Audit surface** | zbus 4.x — actively maintained, peer-reviewed | Self-written framing — smaller but needs its own audit |
| **Signals** | `emit_signal!` macro; fan-out handled by broker | Not yet implemented; would need custom multicast |
| **Policy complexity** | D-Bus policy XML files | UID allowlist in `server.rs:40–80` |

**Verdict for this codebase today:** the Unix socket is simpler, has fewer
moving parts, and the current `SO_PEERCRED` enforcement is strictly equivalent
to D-Bus peer-credential checking for single-machine IPC. The main gap is the
lack of native signals — addressed when ai-cockpit polling is replaced (TODO 17).

---

## 3.4  SO_PEERCRED-equivalent identity on D-Bus

D-Bus provides caller identity via `org.freedesktop.DBus.GetConnectionCredentials`
(available since D-Bus 1.7). It returns:

```
ProcessID   → uint32  (same as SO_PEERCRED.pid)
UnixUserID  → uint32  (same as SO_PEERCRED.uid)
LinuxSecurityLabel → []byte  (SELinux label, when available)
```

The broker relays these from the kernel. The service obtains them with:

```rust
// zbus example
let creds = connection
    .call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "GetConnectionCredentials",
        &sender_name,
    )
    .await?;
```

**Key difference from `SO_PEERCRED`:** with `SO_PEERCRED` the kernel fills the
`ucred` struct directly at `accept(2)` time — there is no broker relay. With
D-Bus the credentials pass through the broker and the service trusts the broker
to relay them faithfully. In practice `dbus-broker` does this correctly, but
it is architecturally one additional trust step.

**Reference:** D-Bus spec §7.2, freedesktop.org/wiki/Software/dbus  
**Reference:** `systemd/src/shared/bus-get-credentials.c` for real-world usage  
**Reference:** zbus book §4 "Peer-to-peer connections" for SO_PEERCRED workaround  

---

## 3.5  Recommendation

### Decision: Do nothing for now (keep Unix socket)

**Rationale:**

1. **No missing capability today.** The Unix socket + SO_PEERCRED gives
   equivalent identity guarantees with fewer dependencies and no broker in the
   trust chain.

2. **Signals are not yet required.** When ai-cockpit needs push events (TODO 17),
   a lightweight SSE or a second Unix socket (notification channel) is sufficient
   and avoids the D-Bus dependency entirely.

3. **SELinux policy complexity is non-trivial.** Adding dbus_t to the trust
   chain requires new AVC rules on every policy update, which increases the
   maintenance burden for a security-sensitive daemon.

4. **D-Bus introspection is the most compelling feature.** If Aditya wants
   standard tooling (`busctl`, `gdbus`) for debugging or wants Aria/CLI to use
   standard D-Bus bindings rather than a custom TCP socket, reconsider at that
   point.

5. **Reconsider trigger:** if TODO 251–310 (Aria OS integration) chooses D-Bus
   as the Aria↔OS communication channel, migrate then — not before.

**Next step (if Aditya approves this memo):** No code changes. File the
RESEARCH-FIRST gate as cleared. Record in plan-status.md.

---

## References

- D-Bus specification: https://dbus.freedesktop.org/doc/dbus-specification.html
- zbus book: https://dbus2.github.io/zbus/
- dbus-broker project: https://github.com/bus1/dbus-broker
- systemd `sd-bus` API: https://www.freedesktop.org/software/systemd/man/sd_bus_get_peer_creds.html
- Linux `SO_PEERCRED`: `man 7 socket`, `man 2 getsockopt`
- Directioner-OS server.rs peer-cred enforcement: `ai-engineer/capability-layer/src/server.rs:40–80`
