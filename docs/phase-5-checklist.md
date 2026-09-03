# Phase 5 — Ecosystem: Plugin SDK, Multi-Machine Federation & Public Security Audit Prep Checklist

Status tracking against master architecture specifications, Phase 5 scope, and security invariants ([`docs/multi-machine-federation.md`](file:///home/aditya_munday/projects/Directioner-OS/docs/multi-machine-federation.md), [`docs/security-self-audit.md`](file:///home/aditya_munday/projects/Directioner-OS/docs/security-self-audit.md)).

Legend: `[x]` done and verified · `[~]` scaffolded, in review · `[ ]` not started

---

## Phase 5 Scoped Milestones

### 5.1 Plugin SDK Architecture & Scaffolding (`control-plane::plugin`)
- [x] Strongly-typed plugin manifest format (`PluginManifest`, `PluginMetadata`, `PluginSecurity`, `PluginPermissions`)
- [x] Cryptographic manifest integrity verification (`compute_canonical_hash()` and `verify_integrity()`)
- [x] Bounded execution context (`PluginContext`):
  - Structurally encapsulates `CapabilityClient` and `Implementer` as private
  - Plugins receive only `&PluginContext`
  - All plan proposals routed through mandatory 5-stage policy and safety gate:
    1. Plugin capability allowlist verification (`allowed_capabilities`)
    2. PolicyEngine blast radius evaluation
    3. Plugin approval tier ceiling check (`max_approval_tier`)
    4. Reviewer safety invariant pass (detecting fatal violations like path traversal)
    5. Internal execution with verified approval token
- [x] Plugin lifecycle manager (`PluginRegistry`): registers and verifies plugins before admission
- [x] Reference production plugin (`WorldModelAuditPlugin`): read-only SQLite inspection of local package and systemd unit inventories
- [x] Comprehensive unit test suite (`test_plugin_manifest_hash_integrity_verification`, `test_plugin_context_rejects_unpermitted_capability`, `test_plugin_execution_read_only_world_model`, `test_plugin_registry_lifecycle`)
- **What this actually verifies**: A secure third-party extension framework that registers third-party skills through the exact same capability-token and approval-gate path as built-in capabilities with zero privileged bypass.

---

### 5.2 Governance Guard Extension (`control-plane/tests/governance_guard.rs`)
- [x] Extension of governance guard test suite asserting plugin isolation:
  - `test_plugin_sdk_cannot_bypass_policy_or_reviewer`
  - Asserts that submitting a plan violating Reviewer invariants (path traversal) is rejected with `PluginError::ReviewerRejected` even if the plugin has ExpertReview tier permissions and provides an approval token
  - Asserts that submitting a plan exceeding the plugin's `max_approval_tier` is rejected with `PluginError::TierExceeded`
  - Asserts that submitting un-allowlisted capabilities is rejected with `PluginError::CapabilityNotAllowlisted`
- [x] Permanent code-level invariants preserved: zero changes to `policy.rs`, `reviewer.rs`, `self_healing.rs`, `incident_response.rs`, `conversational.rs`, or `CapabilityKind` enum.
- **What this actually verifies**: Hard compiler- and test-enforced verification that plugins cannot circumvent the Directioner-OS safety architecture under any circumstances.

---

### 5.3 Multi-Machine Federation Architecture & Minimal Read-Only Prototype (`control-plane::federation`)
- [x] Architecture design document in `docs/multi-machine-federation.md`:
  - Decentralized peer model over trusted LAN
  - Strict zero remote execution invariant (no remote shell, no remote RPCs, no mutation handles)
  - Zero cloud dependency
- [x] Telemetry snapshot wire format (`FederatedHealthSnapshot`, `SignedFederatedSnapshot`, `FederatedResourceSummary`)
- [x] Publisher implementation (`FederationPublisher`): gathers `HealthModel` evaluations, generates deterministic SHA-256 payload digest, and signs with local node key
- [x] Peer client implementation (`FederationPeerClient`):
  - Ingests and verifies signatures against sender public key
  - Enforces enrollment key verification against trusted peer registry
  - Enforces freshness window and detects stale snapshots (>300s)
  - Manages isolated read-only peer cache (`peer_cache`)
- [x] Automated unit test suite (`test_federation_publisher_create_and_verify`, `test_federation_peer_client_ingest_and_query`, `test_federation_rejects_tampered_payload`, `test_federation_rejects_untrusted_peer_key`)
- **What this actually verifies**: Safe, decentralized, read-only multi-node health monitoring with cryptographic verification and zero remote execution attack surface.

---

### 5.4 Public Security Audit Preparation & Internal Self-Audit (`docs/security-self-audit.md`)
- [x] Comprehensive internal security self-audit covering:
  - Capability Layer (`ai-capabilityd`): IPC socket permissions, path traversal, symlink safety, killswitch latency, command escaping
  - Safety Layer (`control-plane`): PolicyEngine blast radius, Reviewer invariants, Tier 2 boundaries, self-healing circuit breakers
  - Plugin SDK: Structural isolation, capability allowlists, approval tier ceilings
  - Federation: Zero remote execution, payload digest verification, replay prevention
- [x] Honest identification and documentation of edge cases:
  - `AUDIT-CAP-01`: Unix socket creation permissions (umask / chmod)
  - `AUDIT-CAP-02`: Unix socket missing `SO_PEERCRED` caller UID verification
  - `AUDIT-CAP-03`: File write following pre-existing symlinks in scoped root without `O_NOFOLLOW`
  - `AUDIT-CAP-04`: Killswitch non-preemption of child subprocesses already in flight
- [x] Concrete remediation plan and hardening roadmap documented in `docs/security-self-audit.md`.
- **What this actually verifies**: Rigorous, transparent security audit posture establishing external readiness.

---

## Verification & Test Results
- `cargo test -p control-plane`: 58/58 unit tests passing
- `cargo test -p control-plane --test governance_guard`: 10/10 tests passing
- `cargo clippy --workspace --all-targets -- -D warnings -A missing-docs`: Passed cleanly (0 warnings)
- `cargo fmt --all -- --check`: Passed cleanly
- `ai-benchmark`: 32/32 assertions passing
- Restricted safety files diff: **Exact zero diff** across `policy.rs`, `reviewer.rs`, `self_healing.rs`, `incident_response.rs`, `conversational.rs`, `CapabilityKind`.
