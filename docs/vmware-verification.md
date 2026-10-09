# VMware Boot Verification — Directioner-OS

**Status:** Changes applied. Verification pending re-test in a live VMware guest.

## What This Documents

The forensic bundle `vmware-forensics/directioner-vmware-final-20260927T053641Z-evidence.zip`
revealed two issues on the Directioner-OS image when booted inside VMware Fusion/Workstation:

1. **Serial getty noise:** `serial-getty@ttyS0.service` activated unnecessarily under VMware,
   polluting the console with login prompts on the VMware virtual serial device.

2. **sshd.service protection gap:** The live capability daemon denied `ssh.service` (the
   Debian/Ubuntu unit name) but permitted `sshd.service`, `sshd`, and `ssh` (bare) because
   the protected-unit list used exact-string matching only.

## Changes Applied (branch `vmware-findings-20260928`)

### 1. Serial-getty drop-in (`image/configs/systemd/serial-getty@ttyS0.service.d/vmware.conf`)

```ini
[Unit]
ConditionVirtualization=!vmware
```

Installed into the image via `image/Containerfile`:

```dockerfile
COPY configs/systemd/serial-getty@ttyS0.service.d/ \
     /usr/lib/systemd/system/serial-getty@ttyS0.service.d/
```

When systemd detects `VirtualizationType=vmware`, this condition evaluates false and the unit
does not start. On bare metal and other hypervisors the unit behaves normally.

### 2. sshd.service / ssh normalization (`ai-engineer/capability-layer/src/handler.rs`)

`HandlerConfig::default()` now explicitly lists:

```rust
protected_units: vec![
    "ssh.service".into(),
    "sshd.service".into(),
    "systemd-udevd.service".into(),
    "dbus.service".into(),
    "firewalld.service".into(),
],
```

The `is_protected()` method normalizes names so that `ssh`, `sshd`, `ssh.service`, and
`sshd.service` all match the same protection. The same normalization is applied in
`policy.rs` and `reviewer.rs`.

Evidence from the forensic bundle (`30_dryrun_protected_policy_probes.txt`):

| Probe | Pre-fix result | Post-fix expected |
|---|---|---|
| `ssh.service` | denied ✅ | denied ✅ |
| `sshd.service` | **success (dry-run would_run)** ❌ | denied ✅ |
| `sshd` | **success (dry-run would_run)** ❌ | denied ✅ |
| `ssh` | **success (dry-run would_run)** ❌ | denied ✅ |

### 3. dbus.service alias note

On Fedora 42, `dbus.service` is an alias for `dbus-broker.service`. The forensic bundle does
not contain `systemctl show -p Id,Names dbus.service` output, so the real unit ID cannot be
confirmed from the bundle alone. `dbus.service` has been added to the protected lists under
the authorized name — if the real unit ID turns out to be `dbus-broker.service` only and
`dbus.service` is not resolved by the handler, an additional alias entry may be needed.
No change has been made beyond the explicit authorization.

## What Remains Unverified

- **Serial-getty suppression under VMware:** The drop-in is in place but has not been retested
  in a live VMware boot since the image was rebuilt. Verification requires re-running the image
  under VMware and confirming `serial-getty@ttyS0.service` is in `inactive (dead)` state.

- **dbus.service alias resolution:** Whether the live systemd on the image treats `dbus.service`
  as a resolvable alias for `dbus-broker.service` under the exact name used in the protected list.

- **firewalld.service on clean Fedora 42 bootc:** firewalld may or may not be active by default.
  The protected list entry does not depend on it being active — it prevents the daemon from
  issuing restart/stop even if firewalld is present.

## How to Verify

Boot the rebuilt image in VMware and run:

```bash
# 1. Serial getty (should be inactive)
systemctl is-active serial-getty@ttyS0.service
# Expected: inactive

# 2. sshd.service protection (dry-run daemon, no real mutation)
echo '{"api_version":1,"id":"verify-1","requester":"human","token":"tok","is_rollback":false,"capability":{"capability":"service_restart","unit":"sshd.service","approved":true}}' \
  | sudo socat - UNIX-CONNECT:/run/ai-capabilityd.sock
# Expected: {"status":"denied", ...}

# 3. Same for bare "sshd"
echo '{"api_version":1,"id":"verify-2","requester":"human","token":"tok","is_rollback":false,"capability":{"capability":"service_restart","unit":"sshd","approved":true}}' \
  | sudo socat - UNIX-CONNECT:/run/ai-capabilityd.sock
# Expected: {"status":"denied", ...}
```
