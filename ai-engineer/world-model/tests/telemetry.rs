//! Integration and schema validation tests for World Model system telemetry.
//!
//! Validates read-only ingestion, synchronization, and querying of disk mounts,
//! thermal sensor readings, and network interface states.

use tempfile::NamedTempFile;
use world_model::db;
use world_model::model::{DiskSnapshot, NetIfaceSnapshot, ThermalSnapshot};
use world_model::query;
use world_model::sources;
use world_model::sync;

#[test]
fn test_world_model_telemetry_sync_and_query_roundtrip() {
    let tmp = NamedTempFile::new().unwrap();
    let conn = db::open(tmp.path()).unwrap();

    // 1. Sync Disks
    let disks = vec![
        DiskSnapshot {
            mount_point: "/".into(),
            device: "/dev/nvme0n1p3".into(),
            fs_type: "xfs".into(),
            total_bytes: 512_000_000_000,
            used_bytes: 128_000_000_000,
            avail_bytes: 384_000_000_000,
            use_percent: 25.0,
        },
        DiskSnapshot {
            mount_point: "/var".into(),
            device: "/dev/nvme0n1p4".into(),
            fs_type: "xfs".into(),
            total_bytes: 256_000_000_000,
            used_bytes: 64_000_000_000,
            avail_bytes: 192_000_000_000,
            use_percent: 25.0,
        },
    ];

    let disk_count = sync::sync_disks(&conn, &disks).unwrap();
    assert_eq!(disk_count, 2);

    let queried_disks = query::system_disks(&conn).unwrap();
    assert_eq!(queried_disks.len(), 2);
    assert_eq!(queried_disks[0].mount_point, "/");
    assert_eq!(queried_disks[0].fs_type, "xfs");
    assert_eq!(queried_disks[1].mount_point, "/var");

    // 2. Sync Thermals
    let thermals = vec![
        ThermalSnapshot {
            zone: "x86_pkg_temp".into(),
            temp_celsius: 48.5,
            trip_type: "critical_100C".into(),
        },
        ThermalSnapshot {
            zone: "acpitz".into(),
            temp_celsius: 42.0,
            trip_type: "passive_95C".into(),
        },
    ];

    let thermal_count = sync::sync_thermals(&conn, &thermals).unwrap();
    assert_eq!(thermal_count, 2);

    let queried_thermals = query::system_thermals(&conn).unwrap();
    assert_eq!(queried_thermals.len(), 2);
    assert_eq!(queried_thermals[0].zone, "acpitz");
    assert_eq!(queried_thermals[1].zone, "x86_pkg_temp");
    assert_eq!(queried_thermals[1].temp_celsius, 48.5);

    // 3. Sync Network Interfaces
    let ifaces = vec![
        NetIfaceSnapshot {
            name: "eth0".into(),
            oper_state: "up".into(),
            mac_address: "52:54:00:12:34:56".into(),
            carrier: true,
        },
        NetIfaceSnapshot {
            name: "wlan0".into(),
            oper_state: "dormant".into(),
            mac_address: "52:54:00:12:34:57".into(),
            carrier: false,
        },
    ];

    let iface_count = sync::sync_net_ifaces(&conn, &ifaces).unwrap();
    assert_eq!(iface_count, 2);

    let queried_ifaces = query::system_net_ifaces(&conn).unwrap();
    assert_eq!(queried_ifaces.len(), 2);
    assert_eq!(queried_ifaces[0].name, "eth0");
    assert!(queried_ifaces[0].carrier);
    assert_eq!(queried_ifaces[1].name, "wlan0");
    assert!(!queried_ifaces[1].carrier);

    // 4. Verify GraphStats aggregate includes telemetry
    let stats = query::graph_stats(&conn).unwrap();
    assert_eq!(stats.disks, 2);
    assert_eq!(stats.thermals, 2);
    assert_eq!(stats.net_ifaces, 2);
}

#[test]
fn test_sources_parsers_for_telemetry() {
    let raw_mounts = r#"
/dev/nvme0n1p3 / xfs 512000000 128000000 384000000
/dev/nvme0n1p4 /var ext4 256000000 64000000 192000000
"#;
    let disks = sources::parse_mounts_protocol(raw_mounts);
    assert_eq!(disks.len(), 2);
    assert_eq!(disks[0].mount_point, "/");
    assert_eq!(disks[0].use_percent, 25.0);

    let thermals =
        sources::parse_thermals_data(&[("zone0", 45.0, "passive"), ("zone1", 55.5, "critical")]);
    assert_eq!(thermals.len(), 2);
    assert_eq!(thermals[1].temp_celsius, 55.5);

    let ifaces = sources::parse_net_ifaces_data(&[("eth0", "up", "aa:bb:cc:dd:ee:ff", true)]);
    assert_eq!(ifaces.len(), 1);
    assert_eq!(ifaces[0].name, "eth0");
    assert!(ifaces[0].carrier);
}
