# Directioner-OS Integration Ledger (U003)

Generated: 2026-10-20 | Agent: Antigravity IDE

---

## 1. Repository Map

| Folder | Remote | Role | Framework / Stack |
| :--- | :--- | :--- | :--- |
| `Directioner-OS` | `github.com/aditya-munday/Directioner-OS` | **OS** — This repo. bootc image, Rust control plane, CI. | Rust (cargo 1.97.1), Fedora 42 bootc, GitHub Actions |
| `Beyonders-Backend` | `github.com/aditya-munday/Beyonders-Backend` | **Backend** — shared identity, AI, data, task, entitlement layer | **Fastify 5** (`fastify ^5.2.1`) + **Zod** (`zod ^3.24.1`) + TypeScript. Not Express. |
| `Beyonders-Frontend` | `github.com/aditya-munday/Beyonders-Frontend` | **Website** — Beyonders marketing / landing site | Next.js (App Router) + Tailwind CSS |
| `WakeWord` | *(no git remote; local folder only)* | **Voice samples only** — 19 `.wav` files for Aria wake word training | Python `.venv`; no Aria client code present |

### File citations for backend framework determination

```
Beyonders-Backend/package.json:
  "fastify": "^5.2.1",          # confirmed Fastify 5
  "zod": "^3.24.1",             # confirmed Zod

Beyonders-Backend/src/main.ts:
  import Fastify from 'fastify'; # line 1

Beyonders-Backend/src/http/app.ts:
  import Fastify, { type FastifyBaseLogger, type FastifyInstance } from 'fastify';
  import cors from '@fastify/cors';
  import helmet from '@fastify/helmet';
  import rateLimit from '@fastify/rate-limit';
  import sensible from '@fastify/sensible';
  import websocket from '@fastify/websocket';

Beyonders-Backend/src/contracts/common.ts:
  import { z } from 'zod';  # Zod used for validation contracts
Beyonders-Backend/src/config/env.ts:
  import { z } from 'zod';  # env validation via Zod
```

**Conclusion:** Backend is **Fastify 5 + Zod** (not Express). This affects U236 (OS backend client); the OS client must target Fastify 5 JSON schemas / OpenAPI output, not Express middleware types.

---

## 2. Absent Repositories (WAITING-ON-REPOS)

| Component | Expected Folder | Status |
| :--- | :--- | :--- |
| **Directioner-CLI** | `Directioner-CLI/` | **ABSENT** — repo not handed over |
| **Aria desktop client** | `Aria/` or similar | **ABSENT** — only wake word `.wav` samples in `WakeWord/` |

Both are blocked: `WAITING-ON-REPOS`. The OS integration surfaces (U256–U270 for CLI, U271–U285 for Aria) are built against mocks and contracts in `Section N` until handover.

---

## 3. Repo Privacy Check

```bash
$ gh repo view --json isPrivate
{
  "isPrivate": false
}
```

**WARNING (B4):** `Directioner-OS` is **PUBLIC**. CI artifacts are already using `actions/upload-artifact` with explicit `retention-days: 7` and no public release/package publishing. However, no sensitive private build outputs (VMDK, ISO, screenshots) should accumulate. All artifact uploads in `desktop-image.yml` are upload-only (no public release). This is acceptable under B4.

---

## 4. Secrets Scan

Scanned `Directioner-OS`, `Beyonders-Backend`, `Beyonders-Frontend`, `WakeWord` for:
- `API_KEY`, `SECRET`, `TOKEN`, `PRIVATE_KEY`, `BEARER`, `PASSWORD` patterns with non-placeholder values
- `-----BEGIN * PRIVATE KEY-----` patterns

**Findings in `Directioner-OS`:**

- `vm-test/ci_key` — committed SSH private key (RSA 452 bytes). This is the CI test key documented in `vm-test/README.md` and used exclusively in the headless smoke test. It has no associated production access. Noted as test-only per D7 boundary. Should be rotated to a CI secret at U014 hardening pass.
- No third-party API keys, bearer tokens, or production credentials found in working tree.

**Findings in `Beyonders-Backend`:**

- `src/config/env.ts` — references env var names (`DATABASE_URL`, `JWT_SECRET`, etc.) but contains **no literal values**, only Zod schema definitions. Clean.
- `.env.example` — placeholder values only, clearly labeled. Clean.

**Findings in `Beyonders-Frontend`, `WakeWord`:** None.

---

## 5. Backend Architecture Notes for U236

The OS backend client (`directioner-cloudd`, `directioner-accountd`) must target:
- **Auth:** JWT via `jose ^5.10.0` library; device-code flow TBD (M11.Q1 open question)
- **Wire format:** JSON REST; WebSocket via `@fastify/websocket ^11.3.4` for voice/stream
- **Validation:** Zod schemas in `src/contracts/`; OS should align request/response shapes to those schemas
- **OpenAPI:** `scripts/generate-openapi.ts` generates spec; OS can import it for client generation
- **Auth flow question Q2:** Voice transport (WebSocket confirmed available in backend); REST vs WebSocket for STT/TTS TBD

---

## 6. V4 Task Cross-Reference

| V4 Task | Backend Dependency | Notes |
| :--- | :--- | :--- |
| U236 | M1.1 auth endpoint | Fastify 5 + Zod; device-code flow pending Q1 answer |
| U237 | M4.1 AI proxy | WebSocket endpoint; wire format TBD (Q2) |
| U238 | M3.1 voice | WebSocket + chunked audio; Q3 open |
| U239 | M4.1–M4.5 storage | Blob API; content-addressed per M4.1 |
| U240 | M6.1 entitlements | Read-only endpoint with signed expiry blob |
