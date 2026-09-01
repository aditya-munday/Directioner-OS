//! Domain entities and in-memory data representations for the SQLite World Model.

/// Package row representing an installed or repository package.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PackageRow {
    /// Canonical package name (e.g. `glibc`).
    pub name: String,
    /// Epoch-Version-Release string (e.g. `2.38-4.fc40`).
    pub evr: String,
    /// Target architecture (e.g. `x86_64`).
    pub arch: String,
    /// Human-readable package description summary.
    pub summary: String,
}

/// Dependency edge pointing from a package to its required symbol/capability.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RequireEdge {
    /// Dependent package name.
    pub pkg: String,
    /// Required dependency symbol or package name.
    pub dep: String,
    /// Comparison flags (e.g. `>=`, `=`).
    pub flags: String,
    /// Required version constraint string.
    pub version: String,
}

/// Capability edge pointing from a package to the symbols/sonames it provides.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProvideEdge {
    /// Providing package name.
    pub pkg: String,
    /// Provided capability or soname (e.g. `libc.so.6()(64bit)`).
    pub provide: String,
    /// Version associated with the provided symbol.
    pub version: String,
}

/// Aggregate container for transactional package synchronization.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PackageData {
    /// List of packages in this sync dataset.
    pub packages: Vec<PackageRow>,
    /// List of require edges in this sync dataset.
    pub requires: Vec<RequireEdge>,
    /// List of provide edges in this sync dataset.
    pub provides: Vec<ProvideEdge>,
}

/// Snapshot of a systemd unit state at a point in time.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnitSnapshot {
    /// Canonical unit name (e.g. `nginx.service`).
    pub name: String,
    /// Unit load state (e.g. `loaded`, `not-found`).
    pub load_state: String,
    /// Unit active state (e.g. `active`, `failed`, `inactive`).
    pub active_state: String,
    /// Unit sub state (e.g. `running`, `dead`, `exited`).
    pub sub_state: String,
    /// Unit enablement status (e.g. `enabled`, `disabled`, `static`).
    pub enabled: String,
}

impl UnitSnapshot {
    /// Returns the unit state tuple for comparison.
    pub fn tuple(&self) -> (&str, &str, &str, &str) {
        (
            &self.load_state,
            &self.active_state,
            &self.sub_state,
            &self.enabled,
        )
    }
}

/// Change delta detected between consecutive systemd unit polls.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnitChange {
    /// Target unit name.
    pub unit: String,
    /// Change category (`added`, `removed`, `active`, `load`, `sub`, `enabled`).
    pub kind: String,
    /// Previous state value prior to transition.
    pub old_value: String,
    /// New state value after transition.
    pub new_value: String,
}

/// Read-only snapshot of a mounted filesystem / storage device.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiskSnapshot {
    /// Filesystem mount path (e.g. `/`, `/var`, `/sysroot`).
    pub mount_point: String,
    /// Block device or backing identifier (e.g. `/dev/nvme0n1p3`).
    pub device: String,
    /// Filesystem type (e.g. `xfs`, `btrfs`, `ext4`).
    pub fs_type: String,
    /// Total capacity in bytes.
    pub total_bytes: u64,
    /// Used storage in bytes.
    pub used_bytes: u64,
    /// Available capacity in bytes.
    pub avail_bytes: u64,
    /// Utilization percentage (0.0 - 100.0).
    pub use_percent: f64,
}

/// Read-only snapshot of a hardware thermal sensor zone.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ThermalSnapshot {
    /// Thermal zone identifier (e.g. `thermal_zone0`, `acpitz`, `x86_pkg_temp`).
    pub zone: String,
    /// Sensor temperature reading in degrees Celsius.
    pub temp_celsius: f64,
    /// Associated trip point or sensor type description.
    pub trip_type: String,
}

/// Read-only snapshot of a network interface operational state.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NetIfaceSnapshot {
    /// Interface name (e.g. `eth0`, `wlan0`, `lo`).
    pub name: String,
    /// Operational state (`up`, `down`, `dormant`, `unknown`).
    pub oper_state: String,
    /// Hardware MAC address.
    pub mac_address: String,
    /// Physical link carrier detection status.
    pub carrier: bool,
}
