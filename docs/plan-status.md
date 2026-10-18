# docs/plan-status.md
# in-memory model, no real OS integration

This file records the honest per-task state of every item in
DIRECTIONER_OS_ULTIMATE_PLAN.txt. States are:

- NOT-STARTED      — zero code exists for this task
- SCAFFOLD         — in-memory / struct-only code exists; no real OS binding
- OWNER-AUTH-PENDING — task explicitly requires owner sign-off before any code
- DESIGN-PENDING   — task is gated on an approved design doc
- WAITING-ON-REPOS — task requires a sibling repo (CLI, Aria, backend, website) not yet present

No task in this file has a status of DONE unless it has:
  1. a passing test visible in CI
  2. a merged PR
  3. no OWNER-AUTH or DESIGN gate outstanding

---

## Domain 1 — Foundation (TODOs 001-010)

| # | Title | Status |
|---|-------|--------|
| 001 | Repo + CI skeleton | SCAFFOLD |
| 002 | Cargo workspace | SCAFFOLD |
| 003 | Fedora-bootc base image | SCAFFOLD |
| 004 | GitHub Actions build pipeline | SCAFFOLD |
| 005 | Secret scanning baseline | NOT-STARTED |
| 006 | Dependabot config | NOT-STARTED |
| 007 | CODEOWNERS file | NOT-STARTED |
| 008 | Branch protection rules | NOT-STARTED |
| 009 | Developer certificate of origin | NOT-STARTED |
| 010 | License headers | NOT-STARTED |

## Domain 2 — Auth / Account (TODOs 011-020)

| # | Title | Status |
|---|-------|--------|
| 011 | accountd crate skeleton | SCAFFOLD |
| 012 | AccountState machine | SCAFFOLD |
| 013 | Owner-auth device binding | OWNER-AUTH-PENDING |
| 014 | OAuth device-flow client | SCAFFOLD |
| 015 | Keyring / Secret Service token storage | OWNER-AUTH-PENDING |
| 016 | Entitlements verifier | SCAFFOLD |
| 017 | Offline grace period logic | SCAFFOLD |
| 018 | Device registration list | SCAFFOLD |
| 019 | Token refresh scheduler | SCAFFOLD |
| 020 | Account event log | SCAFFOLD |

## Domain 3 — Incident Response (TODOs 021-030)

| # | Title | Status |
|---|-------|--------|
| 021 | Chaos runner skeleton | SCAFFOLD |
| 022 | Incident classifier | SCAFFOLD |
| 023 | Self-healing action table | SCAFFOLD |
| 024 | Audit log writer | SCAFFOLD |
| 025 | Watchdog heartbeat | SCAFFOLD |
| 026 | Recovery runbooks | NOT-STARTED |
| 027 | PagerDuty / alert bridge | WAITING-ON-REPOS |
| 028 | Incident severity tiers | SCAFFOLD |
| 029 | Post-mortem template | NOT-STARTED |
| 030 | On-call rotation config | NOT-STARTED |

## Domain 4 — Image / Bootc (TODOs 031-040)

| # | Title | Status |
|---|-------|--------|
| 031 | Containerfile skeleton | SCAFFOLD |
| 032 | bootc image build | SCAFFOLD |
| 033 | OCI layer layout | SCAFFOLD |
| 034 | A/B slot controller | SCAFFOLD |
| 035 | Boot-attempt watchdog | SCAFFOLD — NOTE: must use greenboot, not custom |
| 036 | Update signature verify | SCAFFOLD |
| 037 | Image signing (cosign) | NOT-STARTED |
| 038 | SBOM generation | NOT-STARTED |
| 039 | Quay.io push pipeline | SCAFFOLD |
| 040 | Rollback smoke test | SCAFFOLD |

## Domain 5 — CI/CD / QA (TODOs 041-050)

| # | Title | Status |
|---|-------|--------|
| 041 | GitHub Actions matrix | SCAFFOLD |
| 042 | Cargo test gate | SCAFFOLD |
| 043 | Integration test harness | SCAFFOLD |
| 044 | Code coverage report | NOT-STARTED |
| 045 | Telemetry pipeline | SCAFFOLD |
| 046 | Update verification | SCAFFOLD |
| 047 | Rollback trigger | SCAFFOLD |
| 048 | Performance benchmark | NOT-STARTED |
| 049 | Regression report | NOT-STARTED |
| 050 | Release changelog bot | NOT-STARTED |

## Domain 6 — Rebrand / Identity (TODOs 051-100)

| # | Title | Status |
|---|-------|--------|
| 051-060 | Brand tokens, fonts, wallpapers | SCAFFOLD |
| 061-070 | GNOME Shell theme overrides | DESIGN-PENDING |
| 071-080 | Flatpak portal overrides | NOT-STARTED |
| 081-090 | SELinux policy modules | SCAFFOLD |
| 091-100 | Polkit policies | SCAFFOLD |

## Domain 7 — Hardware / Power (TODOs 101-120)

| # | Title | Status |
|---|-------|--------|
| 101-120 | hardware_power.rs — thermal, profiles, sleep | SCAFFOLD — IN-MEMORY MODEL; no sysfs |

## Domain 8 — Security / Trust (TODOs 121-140)

| # | Title | Status |
|---|-------|--------|
| 108 | Vetted crypto library integration | OWNER-AUTH-PENDING |
| 121-140 | security_trust.rs — TPM, PCR, Merkle | SCAFFOLD — IN-MEMORY MODEL; no TPM/kernel calls |
| 170 | Kernel keyring token store | OWNER-AUTH-PENDING |

## Domain 9 — Release / Installer (TODOs 141-160)

| # | Title | Status |
|---|-------|--------|
| 141-160 | release_installer.rs — A/B slots, updates | SCAFFOLD — IN-MEMORY MODEL; no bootc integration |

## Domain 10 — Desktop Experience (TODOs 161-180)

| # | Title | Status |
|---|-------|--------|
| 161-180 | desktop_experience.rs — themes, Aria glow, workspaces | SCAFFOLD — IN-MEMORY MODEL |

## Domain 11 — Intent / Aria (TODOs 181-200)

| # | Title | Status |
|---|-------|--------|
| 181-200 | intent_broker.rs — NLU dispatch | SCAFFOLD — draft/preview only; no execution path |

## Domain 12 — System Actions (TODOs 201-220)

| # | Title | Status |
|---|-------|--------|
| 201-220 | system_actions.rs — alarms, a11y, settings | SCAFFOLD — IN-MEMORY MODEL |

## Domain 13 — QA Conformance (TODOs 221-240)

| # | Title | Status |
|---|-------|--------|
| 221-240 | qa_conformance.rs — 12-vector runner | SCAFFOLD |

## Domains 14-16 (TODOs 241-500)

| Range | Status |
|-------|--------|
| 241-500 | NOT-STARTED — no real code exists for these task IDs |

---

## OWNER-AUTH-PENDING items (must not be touched until owner signs off)

- 013 — Owner-auth device binding
- 015 — Keyring / Secret Service token storage
- 108 — Vetted crypto library integration
- 170 — Kernel keyring token store
- 230 — (see plan)
- 310 — (see plan)
- 497 — (see plan)

## WAITING-ON-REPOS items

Any task that requires the CLI, Aria, backend, or website repos.
Those repos are **not present** at `~/projects/`. Zero bindings can be built.

---

_Last updated: 2026-10-18. Updated by CLEANUP-1._
