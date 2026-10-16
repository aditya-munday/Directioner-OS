//! Directioner-OS Capability Layer (Phase 0.2 / 0.3 / 0.4).
//!
//! The typed, audited, kill-switchable system API that every AI Engineer role
//! must call instead of shelling out directly. The phase-0 skeleton exposes a
//! handful of capabilities:
//!
//! - [`CapabilityKind::FileRead`] / [`CapabilityKind::FileWrite`] — scoped file I/O
//! - [`CapabilityKind::PackageInstall`] — approval-gated package installation
//! - [`CapabilityKind::ServiceStatus`] / [`CapabilityKind::ServiceRestart`] — systemd
//! - [`CapabilityKind::Ping`] — health check
//!
//! Every call is written to an append-only [`audit::AuditLog`] *before* it is
//! acted on, and refused outright when the [`killswitch::KillSwitch`] is engaged.
//!
//! The top-level flow is:
//!
//! 1. Daemon (`ai-capabilityd`) listens on a Unix socket.
//! 2. Each request is a single JSON line (newline-delimited framing).
//! 3. [`handler::Handler::execute`] validates scope + gates, executes through the
//!    typed `dnf5`/`systemctl`/file adapters, writes an audit row, and replies.

pub mod audit;
pub mod handler;
pub mod killswitch;
pub mod notify;
pub mod scope;
pub mod scrub;
pub mod server;

use std::fmt;

/// Maximum number of bytes a single [`CapabilityKind::FileRead`] will return.
pub const MAX_READ_BYTES: usize = 1024 * 1024;

/// Current protocol version of the capability request envelope.
pub const API_VERSION: u32 = 1;

/// A typed, capability-scoped request from an agent role.
///
/// `token` is the short-lived capability token granted for this specific call;
/// `requester` names the agent role, and `id` correlates plan → execution →
/// verification across the Work Log.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct CapabilityRequest {
    /// Protocol version; must equal [`API_VERSION`].
    pub api_version: u32,
    /// Caller-supplied correlation id (opaque string).
    pub id: String,
    /// Agent role or user making the call.
    pub requester: String,
    /// Short-lived capability token scoped to this plan.
    pub token: String,
    /// Whether this capability call is part of a compensating rollback.
    #[serde(default)]
    pub is_rollback: bool,
    /// The typed action to perform, with its arguments.
    pub capability: CapabilityKind,
}

/// The typed capability surface. Serialized with a `capability` tag so the wire
/// format reads like `{"capability":"package_install","package":"nginx",...}`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(tag = "capability", rename_all = "snake_case")]
pub enum CapabilityKind {
    /// Read a file inside the work scope.
    FileRead {
        /// Absolute path to read.
        path: String,
    },
    /// Write (create or append to) a file inside the work scope.
    FileWrite {
        /// Absolute path to write.
        path: String,
        /// Content to write or append.
        content: String,
        /// If true, append; otherwise truncate-then-write.
        append: bool,
    },
    /// Delete (unlink) a file inside the work scope.
    FileDelete {
        /// Absolute path to delete.
        path: String,
    },
    /// Install a package. Requires `approved == true` in production (approval gate).
    PackageInstall {
        /// Package name (or name-version spec) to install.
        package: String,
        /// Whether the human approval gate has cleared this specific plan step.
        approved: bool,
    },
    /// Remove a package. Requires `approved == true` in production (approval gate).
    PackageRemove {
        /// Package name to remove.
        package: String,
        /// Whether the human approval gate has cleared this specific plan step.
        approved: bool,
    },
    /// Report the state of a systemd unit.
    ServiceStatus {
        /// Unit name, e.g. `nginx.service`.
        unit: String,
    },
    /// Restart a systemd unit. Requires approval and is refused for protected units.
    ServiceRestart {
        /// Unit name, e.g. `nginx.service`.
        unit: String,
        /// Whether the human approval gate has cleared this specific plan step.
        approved: bool,
    },
    /// Stop a systemd unit. Requires approval and is refused for protected units.
    ServiceStop {
        /// Unit name, e.g. `nginx.service`.
        unit: String,
        /// Whether the human approval gate has cleared this specific plan step.
        approved: bool,
    },
    /// Pin an OSTree/bootc deployment slot to prevent garbage collection before changes (Phase 3.0.1).
    BootSlotPin {
        /// Slot/index to pin (0 is current active deployment).
        slot: u32,
        /// Approval gate status.
        approved: bool,
    },
    /// Set a runtime and/or persistent sysctl kernel parameter (Phase 3.0.4).
    KernelParamSet {
        /// Parameter key in sysctl dotted format (e.g. "net.ipv4.ip_forward", "vm.swappiness", "fs.file-max").
        key: String,
        /// Parameter value to apply.
        value: String,
        /// If true, also writes to `/etc/sysctl.d/99-directioner-tuning.conf` to persist across reboots.
        persistent: bool,
        /// Approval gate status.
        approved: bool,
    },
    /// Load a kernel module dynamically into the running kernel (Phase 3.0.4).
    KernelModuleLoad {
        /// Module name (e.g. "br_netfilter", "overlay", "dummy", "wireguard").
        module: String,
        /// Optional module arguments passed to `modprobe`.
        args: Vec<String>,
        /// Approval gate status.
        approved: bool,
    },
    /// Unload a dynamically loaded kernel module (Phase 3.0.4).
    KernelModuleUnload {
        /// Module name to unload via `modprobe -r` (refused if in-use by kernel subsystems).
        module: String,
        /// Approval gate status.
        approved: bool,
    },
    /// Tier 1 emergency rollback to last known good deployment.
    BootcRollback,
    /// Liveness probe; always returns OK when the kill-switch is off.
    Ping,
}

/// Curated, closed allowlist of authorized kernel modules for Phase 3.0.4 Tier 2 remediation.
pub const ALLOWED_KERNEL_MODULES: &[&str] = &[
    // Container networking, bridging, and packet filtering
    "br_netfilter",
    "overlay",
    "dummy",
    "veth",
    "wireguard",
    "ip_tables",
    "iptable_filter",
    "nf_conntrack",
    "xt_nat",
    "ip6_tables",
    "ip6table_filter",
    "tcp_bbr",
    // Userspace I/O & virtualization primitives
    "tun",
    "tap",
    "loop",
    "fuse",
];

/// Prohibited security-critical kernel parameters (Fatal Reviewer rejection, non-approvable).
pub const PROHIBITED_KERNEL_PARAMS: &[&str] = &[
    "kernel.core_pattern",
    "kernel.modprobe",
    "kernel.hotplug",
    "vm.mmap_min_addr",
    "kernel.kptr_restrict",
    "kernel.dmesg_restrict",
    "kernel.unprivileged_bpf_disabled",
    "fs.protected_symlinks",
    "fs.protected_hardlinks",
    "fs.protected_fifos",
    "fs.protected_regular",
    "kernel.kexec_load_disabled",
    "kernel.modules_disabled",
];

/// The typed response envelope for every capability call.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CapabilityResponse {
    /// The capability executed successfully.
    Success {
        /// Echo of the caller's correlation id.
        request_id: String,
        /// Machine-readable outcome details.
        result: serde_json::Value,
        /// Sequence number of the audit row that recorded this call.
        audit_seq: u64,
    },
    /// The call was refused before it executed.
    Denied {
        /// Echo of the caller's correlation id.
        request_id: String,
        /// Human-readable reason (approval gate, scope, kill-switch, protected).
        reason: String,
        /// Sequence number of the audit row that recorded this call.
        audit_seq: u64,
    },
    /// The call was accepted but failed during execution.
    Error {
        /// Echo of the caller's correlation id.
        request_id: String,
        /// Human-readable failure message.
        message: String,
        /// Sequence number of the audit row that recorded this call.
        audit_seq: u64,
    },
}

/// Error type used inside the Capability Layer.
#[derive(Debug, Clone)]
pub enum CapError {
    /// The request was refused by policy before any side effect.
    Denied(String),
    /// The request was accepted but could not be completed.
    Internal(String),
}

impl fmt::Display for CapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CapError::Denied(reason) => write!(f, "denied: {reason}"),
            CapError::Internal(message) => write!(f, "error: {message}"),
        }
    }
}

impl std::error::Error for CapError {}

/// Attempt a capability call and report which `audit_seq` recorded it.
pub type Result<T> = std::result::Result<T, CapError>;
