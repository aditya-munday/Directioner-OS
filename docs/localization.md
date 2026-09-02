# Directioner-OS — User-Facing Localization Architecture & Scaffolding

**Status**: Architecture & Implementation Specification (Phase 4 Internationalization & Scaffolding)  
**Target Scope**: CLI Messages, Operator Alerts, Reviewer Findings, and Hardware Diagnostic Telemetry  
**Default Locale**: `en-US` (English - United States)

---

## 1. Executive Summary

Directioner-OS is designed for global developers and system administrators. To ensure clear communication during automated operations, self-healing cycles, and diagnostic reviews without hardcoding English strings into business logic, Directioner-OS incorporates a structured localization catalog architecture (`control_plane::localization::MessageCatalog`).

This document details:
1. The message catalog format and key hierarchy.
2. Parameter interpolation conventions.
3. How new translation catalogs (e.g. `es-ES`, `de-DE`, `ja-JP`, `zh-CN`) are added and verified.

---

## 2. Message Key Hierarchies & Namespaces

Translation keys follow dot-delimited hierarchical namespaces:

| Namespace | Category Description | Example Keys |
| :--- | :--- | :--- |
| `cli.banner.*` | Top-level tool identification and header banners | `cli.banner.ai_engineer`, `cli.banner.cockpit` |
| `alert.*` | System-level notifications, rollback warnings, killswitch halts | `alert.rollback_failure`, `alert.killswitch_engaged` |
| `reviewer.verdict.*` | Automated safety and invariant evaluation decisions | `reviewer.verdict.approved`, `reviewer.verdict.rejected` |
| `hwcheck.*` | Hardware driver compatibility notices and mitigations | `hwcheck.wifi.broadcom_warning`, `hwcheck.sleep.apst_warning` |
| `conversational.*` | Operator conversational preview and safety boundaries | `conversational.shell.draft_notice`, `conversational.shell.no_execution` |
| `compat.*` | Wine/Proton application compatibility profile lookups | `compat.profile.found`, `compat.profile.missing` |

---

## 3. Parameter Interpolation Conventions

Dynamic variables in message strings use named curly-brace syntax: `{variable_name}`.

### Example

Template string (`en-US`):
```json
"hwcheck.wifi.broadcom_warning": "Broadcom WiFi adapter [{slot}] requires broadcom-wl kmod. Layer into Containerfile via RPM Fusion."
```

Code lookup and interpolation:
```rust
let catalog = MessageCatalog::default_en_us();
let message = catalog.format("hwcheck.wifi.broadcom_warning", &[("slot", "0000:03:00.0")]);
```

Formatted output:
```text
Broadcom WiFi adapter [0000:03:00.0] requires broadcom-wl kmod. Layer into Containerfile via RPM Fusion.
```

---

## 4. Multi-Locale Catalog Registration & Storage

Catalogs are stored as JSON files under `locales/<locale_tag>.json` or embedded at build-time.

### Example Catalog Structure (`locales/es-ES.json`):

```json
{
  "locale": "es-ES",
  "messages": {
    "cli.banner.ai_engineer": "Línea de Control de IA de Directioner-OS",
    "alert.rollback_failure": "🚨 ALERTA CRÍTICA DEL SISTEMA: FALLO EN REVERSIÓN DETECTADO",
    "alert.action_required": "ACCIÓN REQUERIDA: El estado del sistema puede ser inconsistente. Inspeccione inmediatamente.",
    "alert.killswitch_engaged": "KillSwitch activado: Evaluación y envíos detenidos.",
    "reviewer.verdict.approved": "✔ Plan Aprobado: No se detectaron violaciones de seguridad.",
    "reviewer.verdict.rejected": "✘ Plan Rechazado: Violación fatal de invariante de seguridad.",
    "hwcheck.status.supported": "Todos los subsistemas de hardware coinciden con configuraciones conocidas de Fedora bootc."
  }
}
```

---

## 5. Verification & Testing

Every registered catalog is tested against the `en-US` reference key-set to ensure 100% key coverage without missing translation identifiers.
