//! Integration tests for Multi-Node QEMU virtual network harness & read-only federation (TODO 41).
//!
//! Tests:
//! 1. 41.1 Harness verification (booting 2-3 nodes via socket virtual network abstraction).
//! 2. 41.2 Federation read-only queries between nodes (no remote execution, local queries only - F7).
//! 3. 41.3 Failure isolation: one node failing (degraded or corrupt) while other nodes stay healthy.
//! 4. 41.4 Serial log collection and artifact storage verification.
//! 5. 41.5 CI runtime budget compliance (< 500ms execution).

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use chrono::Utc;
use control_plane::federation::{FederationPeerClient, FederationPublisher};
use control_plane::health_model::{
    HealthEvidence, HealthStatus, SubsystemHealth, SystemHealthReport,
};

fn make_health_report(status: HealthStatus, note: &str) -> SystemHealthReport {
    SystemHealthReport {
        evaluated_at: Utc::now().to_rfc3339(),
        composite_score: match status {
            HealthStatus::Healthy => Some(100),
            HealthStatus::Degraded => Some(60),
            HealthStatus::Critical => Some(20),
            HealthStatus::Unknown => None,
        },
        overall_status: status,
        summary: note.to_string(),
        subsystems: vec![
            SubsystemHealth {
                name: "boot".into(),
                score: Some(100),
                status: HealthStatus::Healthy,
                summary: "boot clean".into(),
                evidence: vec![],
            },
            SubsystemHealth {
                name: "control_plane".into(),
                score: Some(95),
                status,
                summary: note.to_string(),
                evidence: vec![HealthEvidence {
                    timestamp: Utc::now().to_rfc3339(),
                    source: "multinode_test".into(),
                    entity: "node".into(),
                    description: note.to_string(),
                    severity: match status {
                        HealthStatus::Healthy => "info".into(),
                        HealthStatus::Degraded => "warning".into(),
                        HealthStatus::Critical => "error".into(),
                        HealthStatus::Unknown => "unknown".into(),
                    },
                }],
            },
        ],
    }
}

#[test]
fn test_multinode_script_dry_run_and_artifact_collection() {
    let start = Instant::now();
    let root = env!("CARGO_MANIFEST_DIR");
    let script_path = Path::new(root).join("../../scripts/qemu_multinode.sh");

    assert!(
        script_path.exists(),
        "scripts/qemu_multinode.sh must exist at {:?}",
        script_path
    );

    let temp_log_dir = std::env::temp_dir().join("test_multinode_logs");
    let _ = fs::remove_dir_all(&temp_log_dir);

    let output = Command::new(&script_path)
        .arg("3")
        .arg("true")
        .arg(&temp_log_dir)
        .output()
        .expect("failed to execute qemu_multinode.sh");

    assert!(
        output.status.success(),
        "qemu_multinode.sh failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(temp_log_dir.exists(), "multinode log directory must exist");

    // Check serial logs for all 3 nodes
    for i in 1..=3 {
        let serial_log = temp_log_dir.join(format!("node-{}_serial.log", i));
        assert!(
            serial_log.exists(),
            "serial log {:?} must exist",
            serial_log
        );
        let contents = fs::read_to_string(&serial_log).unwrap();
        assert!(
            contents.contains(&format!("Node node-{} started successfully", i)),
            "log should confirm node start"
        );
    }

    let _ = fs::remove_dir_all(&temp_log_dir);

    assert!(
        start.elapsed().as_millis() < 500,
        "Multinode test must complete well under CI budget"
    );
}

#[test]
fn test_multinode_read_only_federation_queries() {
    let start = Instant::now();

    // Setup 3 nodes
    let node2_pub = FederationPublisher::new("node-2", "node2.cluster.local", "key-node-2");
    let node3_pub = FederationPublisher::new("node-3", "node3.cluster.local", "key-node-3");

    // Each node acts as a read-only consumer of its peers
    let mut node1_client = FederationPeerClient::new();
    node1_client.enroll_peer("node-2", "key-node-2");
    node1_client.enroll_peer("node-3", "key-node-3");

    // Generate signed reports
    let h2 = make_health_report(HealthStatus::Healthy, "node-2 nominal");
    let h3 = make_health_report(HealthStatus::Healthy, "node-3 nominal");

    let snap2 = node2_pub.create_signed_snapshot(&h2, 2, 4, 1100, 120);
    let snap3 = node3_pub.create_signed_snapshot(&h3, 1, 2, 950, 80);

    // Node 1 ingests snapshots
    assert!(node1_client.ingest(&snap2).is_ok());
    assert!(node1_client.ingest(&snap3).is_ok());

    assert_eq!(node1_client.peer_count(), 2);
    let p2 = node1_client.get_peer("node-2").expect("peer 2 found");
    assert_eq!(p2.snapshot.hostname, "node2.cluster.local");
    assert_eq!(p2.snapshot.overall_status, HealthStatus::Healthy);

    let p3 = node1_client.get_peer("node-3").expect("peer 3 found");
    assert_eq!(p3.snapshot.hostname, "node3.cluster.local");
    assert_eq!(p3.snapshot.overall_status, HealthStatus::Healthy);

    assert!(
        start.elapsed().as_millis() < 200,
        "Read-only federation query test must complete within budget"
    );
}

#[test]
fn test_multinode_node_failure_isolation() {
    // 41.3: One node failing while others stay healthy
    let node1_pub = FederationPublisher::new("node-1", "node1.cluster.local", "key-node-1");
    let node2_pub = FederationPublisher::new("node-2", "node2.cluster.local", "key-node-2");
    let node3_pub = FederationPublisher::new("node-3", "node3.cluster.local", "key-node-3");

    let mut cluster_monitor = FederationPeerClient::new();
    cluster_monitor.enroll_peer("node-1", "key-node-1");
    cluster_monitor.enroll_peer("node-2", "key-node-2");
    cluster_monitor.enroll_peer("node-3", "key-node-3");

    // Node 1 healthy, Node 3 healthy, Node 2 goes Critical / degraded
    let h1 = make_health_report(HealthStatus::Healthy, "node-1 operating normally");
    let h2 = make_health_report(HealthStatus::Critical, "node-2 disk partition full");
    let h3 = make_health_report(HealthStatus::Healthy, "node-3 operating normally");

    let snap1 = node1_pub.create_signed_snapshot(&h1, 2, 4, 1200, 130);
    let snap2 = node2_pub.create_signed_snapshot(&h2, 1, 2, 800, 70);
    let snap3 = node3_pub.create_signed_snapshot(&h3, 2, 4, 1150, 125);

    assert!(cluster_monitor.ingest(&snap1).is_ok());
    assert!(cluster_monitor.ingest(&snap2).is_ok());
    assert!(cluster_monitor.ingest(&snap3).is_ok());

    // Verify isolation: Node 2 is Critical, but Node 1 and Node 3 remain Healthy
    assert_eq!(
        cluster_monitor
            .get_peer("node-1")
            .unwrap()
            .snapshot
            .overall_status,
        HealthStatus::Healthy
    );
    assert_eq!(
        cluster_monitor
            .get_peer("node-2")
            .unwrap()
            .snapshot
            .overall_status,
        HealthStatus::Critical
    );
    assert_eq!(
        cluster_monitor
            .get_peer("node-3")
            .unwrap()
            .snapshot
            .overall_status,
        HealthStatus::Healthy
    );

    // Furthermore, an invalid/tampered snapshot from node-2 is rejected and does not corrupt cluster state
    let mut tampered_snap2 = snap2.clone();
    tampered_snap2.snapshot.hostname = "hacked.local".into();
    let err = cluster_monitor.ingest(&tampered_snap2);
    assert!(err.is_err(), "tampered snapshot must be rejected");

    // Node 1 & 3 state remains pristine
    assert_eq!(
        cluster_monitor
            .get_peer("node-1")
            .unwrap()
            .snapshot
            .overall_status,
        HealthStatus::Healthy
    );
    assert_eq!(
        cluster_monitor
            .get_peer("node-3")
            .unwrap()
            .snapshot
            .overall_status,
        HealthStatus::Healthy
    );
}
