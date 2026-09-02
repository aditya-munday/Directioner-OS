# Directioner-OS — Proton & Wine Application Compatibility Architecture

**Status**: Research Document & Architectural Specification (Phase 4 Compatibility Layer)  
**Target Environment**: Directioner-OS on Fedora bootc (`x86_64`)  
**Scope**: Windows Application Compatibility Profiles, Proton/Wine Runtime Scaffolding, and World Model Schema.

---

## 1. Executive Summary & Problem Statement

Directioner-OS aims to provide seamless execution of specialized Windows desktop applications and games on top of an immutable container-native Linux base. Achieving reliable execution of Windows software requires managing complex runtime matrices:
1. **Runner Selection**: Proton (Valve upstream), Proton-GE (GloriousEggroll community build), Wine-Staging, or custom Wine prefixes.
2. **Graphics Translation**: Direct3D 9/10/11 to Vulkan via DXVK, Direct3D 12 to Vulkan via VKD3D-Proton, and OpenGL fallback.
3. **Environment Overrides & Tweaks**: Frame rate sync, FSR upscaling (`WINE_FULLSCREEN_FSR`), audio backend switches (`PULSE_LATENCY_MSEC`), and memory overrides.
4. **DLL Overrides & Runtimes**: Specific MSVC CRT runtime libraries, DirectX redistributables, or Media Foundation workarounds.

To maintain strict isolation and user sovereignty, Directioner-OS uses a **read-only compatibility profile store** integrated into the AI Engineer's **World Model**. This allows the resident control plane to look up curated, deterministic runtime configurations for target applications without granting arbitrary write or mutation capabilities to background processes.

---

## 2. Compatibility Profile Structure & Tiering

Directioner-OS adopts the standard community compatibility rating tiers:

| Tier | Status Description | Typical User Experience |
| :--- | :--- | :--- |
| **Platinum** | Runs out-of-the-box with standard upstream Proton/Wine without manual tweaks. | Native parity, zero configuration required. |
| **Gold** | Runs flawlessly after applying documented tweaks (e.g. specific runner version or env flag). | High stability with automated profile application. |
| **Silver** | Minor graphical glitches or non-critical features unavailable, but core app is fully usable. | Usable with caveats. |
| **Bronze** | Frequent crashes, performance degradation, or missing multiplayer/anticheat support. | Degraded functionality. |
| **Borked** | Fails to start or crashes immediately on launch. | Unsupported without architectural fixes. |

---

## 3. World Model Data Schema

Compatibility profiles are modeled as immutable records in SQLite, following the exact read-only telemetry pattern used for system disks, thermals, and network interfaces:

```sql
CREATE TABLE IF NOT EXISTS compatibility_profiles (
    app_id TEXT PRIMARY KEY,
    app_title TEXT NOT NULL,
    runner_version TEXT NOT NULL,
    tier TEXT NOT NULL,
    dxvk_version TEXT,
    vkd3d_version TEXT,
    env_overrides TEXT,
    dll_overrides TEXT,
    notes TEXT,
    synced_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_compat_app_title ON compatibility_profiles(app_title);
```

### Field Definitions

- `app_id`: Normalized application identifier (e.g., `steam:730`, `epic:fortnite`, `standalone:affinity_photo`).
- `app_title`: Human-readable application title (e.g., `"Affinity Photo 2"`, `"Counter-Strike 2"`).
- `runner_version`: Target Wine/Proton runner binary tag (e.g., `"Proton-9.0"`, `"GE-Proton8-26"`).
- `tier`: Compatibility rating string (`"Platinum"`, `"Gold"`, `"Silver"`, `"Bronze"`, `"Borked"`).
- `dxvk_version`: DXVK translation layer version (e.g., `"2.4"`, `"2.3.1"`).
- `vkd3d_version`: VKD3D-Proton translation layer version (e.g., `"2.13"`).
- `env_overrides`: JSON array of environment variable overrides (e.g., `["WINE_FULLSCREEN_FSR=1", "DXVK_HUD=fps"]`).
- `dll_overrides`: JSON object mapping DLL names to override behavior (e.g., `{"d3d11": "native,builtin"}`).
- `notes`: Diagnostic notes and workarounds for the application.
- `synced_at`: RFC3339 timestamp when the profile was ingested into the World Model.

---

## 4. Integration with Directioner-OS Control Plane

1. **Read-Only Ingestion**: The World Model ingests compatibility manifests from `/usr/share/directioner/compat.json` or local cache during `ai-worldmodel sync`.
2. **Querying Profiles**: The Planner and Cockpit query profile recommendations via `world_model::query::compatibility_profile(conn, app_id)`.
3. **Containerized Prefixes**: Wine/Proton runs within isolated Podman/Bubblewrap rootless containers located under `/var/home/<user>/.local/share/wineprefixes`, ensuring the immutable `/usr` root filesystem remains pristine.
