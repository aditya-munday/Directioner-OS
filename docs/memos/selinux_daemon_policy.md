# Research Memo: Confined SELinux Policy Architecture for `ai-capabilityd`

**Author**: Directioner-OS Security Team  
**Date**: 2026-10-18  
**Status**: Proposal & Research Evaluation (Gate: RESEARCH-FIRST)  
**Security Invariant B7**: NEVER switch SELinux to Permissive to "make it work".

---

## 1. Executive Summary & Problem Definition

In Fedora 42 bootc, standard systemd background daemons without custom policy run in `unconfined_service_t` or `init_t`. While DAC (user `root`) permissions exist, running unconfined leaves the entire kernel attack surface open to exploitation if an input parsing or memory safety flaw were to manifest.

This memo defines:
1. The transition of `ai-capabilityd` from `init_t` to a strictly confined domain (`ai_capabilityd_t`).
2. Catalog of expected AVC denials based on observed runtime capability requirements.
3. Reference Type Enforcement (`ai_capabilityd.te`) and File Context (`ai_capabilityd.fc`) definitions with line-by-line manual justification.
4. Strict enforcement of Invariant B7: Enforcing mode must remain active during all testing, with audit2allow output reviewed manually.

---

## 2. Domain Transition & Labeling Model

### 2.1 Type Definitions
- Binary: `ai_capabilityd_exec_t` (`/usr/bin/ai-capabilityd`)
- Domain: `ai_capabilityd_t`
- Socket File: `ai_capabilityd_socket_t` (`/run/ai-capabilityd/capability.sock`)
- Log & Data: `ai_capabilityd_var_lib_t` (`/var/lib/ai-engineer(/.*)?`)
- Configuration: `ai_capabilityd_etc_t` (`/etc/ai-engineer(/.*)?`)

### 2.2 Systemd Transition Rule
When `init_t` (systemd PID 1) executes a file labeled `ai_capabilityd_exec_t`, SELinux automatically transitions the process context into `ai_capabilityd_t`:
```text
domain_auto_trans(init_t, ai_capabilityd_exec_t, ai_capabilityd_t)
```

---

## 3. Observed Runtime Requirements & AVC Analysis

When executing under standard Enforcing mode, `ai-capabilityd` performs the following operations:

| Subsystem | Target Object & Class | Required Permissions | Justification |
|:---|:---|:---|:---|
| Local Socket | `/run/ai-capabilityd/capability.sock` (`sock_file`) | `create`, `bind`, `listen`, `read`, `write`, `unlink` | IPC socket for command dispatch from Reviewer / CLI. |
| Unix Stream | `unconfined_t` / `user_t` (`unix_stream_socket`) | `connectto`, `read`, `write`, `getattr` | Client peer credential inspection (`SO_PEERCRED`). |
| Systemd D-Bus | `systemd_unit_file_t` (`service`) | `status`, `start`, `stop`, `reload` | Service lifecycle operations routed via D-Bus / systemd. |
| Scoped Files | `etc_t` (`file`, `dir`) | `open`, `read`, `write`, `getattr` | Configuration inspection and rollback file adjustments. |
| Kernel Audit | `systemd_journal_t` (`dir`, `file`) | `read`, `open`, `getattr` | Work log and journal anomaly diagnostic scanning. |
| Process Signals | `self` (`process`) | `setrlimit`, `signal` | Process limits and internal thread coordination. |

---

## 4. Line-by-Line Type Enforcement Specification (`ai_capabilityd.te`)

```selinux
policy_module(ai_capabilityd, 1.0.0)

########################################
# Declarations
########################################

type ai_capabilityd_t;
type ai_capabilityd_exec_t;
init_daemon_domain(ai_capabilityd_t, ai_capabilityd_exec_t)

type ai_capabilityd_socket_t;
files_type(ai_capabilityd_socket_t)

type ai_capabilityd_etc_t;
files_config_file(ai_capabilityd_etc_t)

type ai_capabilityd_var_lib_t;
files_type(ai_capabilityd_var_lib_t)

########################################
# Local Policy & Permissions
########################################

# Allow self operations
allow ai_capabilityd_t self:process { fork signal setrlimit };
allow ai_capabilityd_t self:unix_stream_socket { create_stream_socket_perms connectto };

# Manage daemon runtime directory & IPC socket under /run/ai-capabilityd
allow ai_capabilityd_t ai_capabilityd_socket_t:dir { create_dir_perms rw_dir_perms };
allow ai_capabilityd_t ai_capabilityd_socket_t:sock_file { create_file_perms rw_file_perms unlink };

# Read-only configuration under /etc/ai-engineer
allow ai_capabilityd_t ai_capabilityd_etc_t:dir list_dir_perms;
allow ai_capabilityd_t ai_capabilityd_etc_t:file read_file_perms;

# Manage work log and state persistence under /var/lib/ai-engineer
allow ai_capabilityd_t ai_capabilityd_var_lib_t:dir create_dir_perms;
allow ai_capabilityd_t ai_capabilityd_var_lib_t:file create_file_perms;

# D-Bus communication with systemd PID 1 for unit lifecycle control
systemd_dbus_chat_systemd(ai_capabilityd_t)

# Read-only access to /proc and /sys for hardware checks and PSI metrics
kernel_read_system_state(ai_capabilityd_t)
dev_read_sysfs(ai_capabilityd_t)

# Reading journal logs for anomaly and diagnosis detection
logging_read_all_logs(ai_capabilityd_t)
```

---

## 5. File Contexts Specification (`ai_capabilityd.fc`)

```text
/usr/bin/ai-capabilityd           -- gen_context(system_u:object_r:ai_capabilityd_exec_t,s0)
/run/ai-capabilityd(/.*)?            gen_context(system_u:object_r:ai_capabilityd_socket_t,s0)
/etc/ai-engineer(/.*)?               gen_context(system_u:object_r:ai_capabilityd_etc_t,s0)
/var/lib/ai-engineer(/.*)?           gen_context(system_u:object_r:ai_capabilityd_var_lib_t,s0)
/var/log/ai-engineer(/.*)?           gen_context(system_u:object_r:ai_capabilityd_var_lib_t,s0)
```

---

## 6. Audit & Validation Methodology (B7 Invariant)

1. **Strictly Enforcing**: The test image builds with `SELINUX=enforcing` in `/etc/selinux/config`.
2. **AVC Inspection Protocol**:
   ```bash
   ausearch -m avc -ts recent | audit2allow -v
   ```
3. **Audit2allow Review Mandate**:
   - Zero blanket `allow *` or wildcard rules.
   - Any proposed allow rule must correspond to a verified capability in `CapabilityKind`.
   - Never grant `capability:sys_admin`, `capability:sys_rawio`, or `file:execute` on arbitrary paths.
4. **QEMU Verification Test Plan**:
   - Boot VM with `ai_capabilityd.pp` installed.
   - Run `ps -eZ | grep ai-capabilityd` $\to$ confirm domain is `system_u:system_r:ai_capabilityd_t:s0`.
   - Run integration smoke test suite through socket $\to$ ensure zero AVC denial spikes in `audit.log`.
