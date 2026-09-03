# Multi-Machine Federation Architecture

**Status**: Research & Minimal Read-Only Prototype (Phase 5 Ecosystem)  
**Security Invariant**: Strictly Read-Only Telemetry Sharing Over Trusted LAN — Zero Remote Execution Authority

---

## 1. Executive Summary & Design Principles

Directioner-OS is designed as an autonomous, single-node operating system. However, in multi-workstation, homelab, or edge cluster environments, administrators require unified visibility into node health, daemon reliability, resource strain, and hardware status without surrendering sovereignty to an external cloud or central server.

Multi-Machine Federation defines a **fully decentralized, read-only telemetry gossip protocol** tailored specifically for Directioner-OS nodes on a trusted local network.

### Core Architectural Invariants
1. **Zero Remote Execution**: A federated packet cannot trigger shell commands, plan synthesis, service mutation, package installation, kernel manipulation, or capability execution on a remote machine. No execution sockets or remote RPC endpoints exist.
2. **Read-Only Telemetry Sharing**: Nodes broadcast cryptographically signed health snapshots (`SignedFederatedSnapshot`). Remote data is consumed strictly for cockpit aggregation, cross-node health matrices, and administrative dashboards.
3. **No Cloud Dependency**: Federation operates entirely over local link-local or LAN multicast/unicast UDP/TCP. No external SaaS, telemetry collection servers, or centralized coordination authorities are required.
4. **Cryptographic Identity & Authenticity**: Every node generates an ed25519 signing keypair (`key-<node_id>`). All telemetry snapshots are signed over a SHA-256 canonical digest of their normalized payload.
5. **Fail-Closed Verification**: Snapshots with invalid signatures, unknown public keys (in enrolled environments), future-skewed timestamps, or stale payloads (>300 seconds) are dropped immediately.

---

## 2. Threat Model & Security Analysis

Federation operates on local networks that may contain untrusted devices or compromised peers. The architecture explicitly models and defends against the following threat vectors:

| Threat Vector | Attack Scenario | Architectural Defense |
|---|---|---|
| **Remote Execution Injection** | Attacker crafts a packet attempting to run `systemctl stop` or write `/etc/sudoers`. | **Structural Impossibility**: `control_plane::federation` has zero capability handles, zero socket connection logic to `ai-capabilityd`, and exposes no mutation RPCs. |
| **Telemetry Tampering / MITM** | Attacker modifies reported health status or score in-flight. | **Cryptographic Signing**: Every snapshot includes `payload_digest` computed via SHA-256 over normalized JSON. Signatures are verified against the sender's public key before ingest. |
| **Peer Impersonation / Spoofing** | Rogue machine claims the node ID of a critical server. | **Enrolled Key Registry**: `FederationPeerClient` validates incoming `public_key_id` against a static local enrollment table (`trusted_peers`). |
| **Replay Attacks** | Attacker replays a previously signed "Healthy" snapshot during an outage. | **Timestamp Bounding**: Ingestion rejects snapshots with timestamps older than 300 seconds or more than 60 seconds into the future. Monotonic timestamp checks ensure updates advance forward. |
| **Denial of Service / Flooding** | Attacker floods node with oversized or malformed federation packets. | **Bounded Payloads & Memory Footprint**: Snapshots are limited to fixed schema structures; in-memory cache maintains only the single latest snapshot per enrolled peer (`HashMap<String, FederatedPeerStatus>`). |

---

## 3. Data Model & Wire Format

Federated health snapshots capture point-in-time state from the local `HealthModel` and `WorldModel` without leaking sensitive secrets, credentials, or file contents.

### 3.1 Unsigned Payload Schema (`FederatedHealthSnapshot`)

```json
{
  "node_id": "node-alpha-42",
  "hostname": "workstation-01.lan",
  "os_version": "Directioner-OS 42.0.0 (bootc:ostree)",
  "timestamp": "2026-09-03T03:15:00Z",
  "overall_status": "Healthy",
  "subsystems": {
    "boot": "Healthy",
    "services": "Healthy",
    "packages": "Healthy",
    "worklog": "Healthy"
  },
  "resources": {
    "disk_count": 3,
    "thermal_zone_count": 2,
    "package_count": 1420,
    "unit_count": 218
  }
}
```

### 3.2 Signed Envelope Schema (`SignedFederatedSnapshot`)

```json
{
  "snapshot": { ... },
  "public_key_id": "key-node-alpha-42",
  "signature": "3045022100e4b8...",
  "payload_digest": "a3f5c9e2b104..."
}
```

---

## 4. Component Implementation

The reference prototype is implemented in `ai-engineer/control-plane/src/federation.rs` and exposed through `control_plane::federation`:

### 4.1 Publisher (`FederationPublisher`)
- Periodically gathers local health evaluations from `HealthModel::evaluate_system()`.
- Counts active disk mounts, thermal zones, installed packages, and systemd units via SQLite World Model read queries.
- Computes deterministic SHA-256 payload digest.
- Signs payload digest using the node's local signing key.
- Emits `SignedFederatedSnapshot` for LAN broadcast.

### 4.2 Ingest Client (`FederationPeerClient`)
- Maintains local peer cache: `peer_cache: HashMap<String, FederatedPeerStatus>`.
- Ingests incoming snapshots:
  1. Validates `payload_digest` matches SHA-256 of inner snapshot.
  2. Verifies cryptographic signature using `public_key_id`.
  3. Verifies `public_key_id` against enrolled trusted peers if configured.
  4. Enforces time-window freshness checks.
  5. Updates peer cache with verified status.
- Exposes read-only queries:
  - `query_peer(node_id) -> Option<&FederatedPeerStatus>`
  - `query_all() -> Vec<&FederatedPeerStatus>`
  - `cluster_health_summary() -> ClusterSummary`

---

## 5. Verification & Testing

The federation prototype is covered by automated unit tests in `control-plane`:
- `test_federation_publisher_create_and_verify`: Verifies digest calculation and round-trip signature validation.
- `test_federation_peer_client_ingest_and_query`: Verifies peer cache updates, node lookup, and resource aggregation.
- `test_federation_rejects_tampered_payload`: Asserts that any byte modification to the payload causes signature verification failure (`PayloadDigestMismatch`).
- `test_federation_rejects_untrusted_peer_key`: Asserts that an unknown or spoofed signing key is rejected by enrolled clients (`UntrustedPeerKey`).
