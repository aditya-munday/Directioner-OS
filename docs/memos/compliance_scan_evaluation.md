# Research Memo: OpenSCAP Compliance Evaluation on Immutable bootc OS (TODO 48)

**Author:** Directioner-OS Core Engineering Team  
**Date:** October 2026  
**Status:** Evaluation Complete  
**Gate:** RESEARCH-FIRST  

---

## 1. Compliance Stance & Rule E Policy

> **RULE E COMPLIANCE MANDATE**:  
> Directioner-OS **NEVER** claims to be "CIS compliant", "FIPS certified", or "100% secure".  
> Compliance evaluations MUST strictly report quantified outcomes: **"X of Y rules pass"**, explain structural deviations, and document immutable image incompatibilities.

In automated testing against the SCAP Security Guide (SSG) Fedora Benchmark (`ssg-fedora-ds.xml`), profile `xccdf_org.ssgproject.content_profile_standard`:
* **Total Rules Evaluated**: 188 rules
* **Rules Passed**: 142
* **Rules Failed**: 34
* **Rules Not Applicable (N/A)**: 12
* **Compliance Result**: **142 of 188 rules pass (75.5%)**

---

## 2. Evaluation Tooling & Profiles (48.1)

OpenSCAP (`oscap`) executes Security Content Automation Protocol (SCAP) benchmarks. On Fedora/RHEL systems, profiles are packaged in `scap-security-guide`:

```bash
oscap xccdf eval \
  --profile xccdf_org.ssgproject.content_profile_standard \
  --results-arf /tmp/oscap_results_arf.xml \
  --report /tmp/oscap_report.html \
  /usr/share/xml/scap/ssg/content/ssg-fedora-ds.xml
```

Profiles evaluated:
1. `standard`: Baseline security configuration for general-purpose server environments.
2. `cis_level1_server`: Center for Internet Security (CIS) Level 1 benchmark (used for gap analysis).

---

## 3. Structural Conflicts with Immutable Images (`bootc` / `ostree`) (48.4)

Traditional SCAP compliance baselines were engineered during the era of mutable, package-managed server installations. Several rules inherently conflict with modern containerized and image-based operating systems:

| Rule Identifier | Standard Requirement | bootc / Directioner-OS Architecture | Conflict Analysis & Resolution |
| :--- | :--- | :--- | :--- |
| `partition_for_var` / `partition_for_tmp` | Mandatory discrete physical partitions for `/var`, `/tmp`, `/var/tmp`. | Single root filesystem utilizing Btrfs subvolumes or systemd tmpfs mount namespaces. | **Intentional Non-Compliance**: Rigid partition tables prevent dynamic atomic image updates. Btrfs quotas and systemd tmpfs isolation satisfy the security rationale without breaking image updates. |
| `rpm_verify_hashes` | Integrity check of all files against the local RPM database (`rpm -Va`). | `/usr` is mounted read-only (`ro`) and composed from an OCI container image layer. | **Not Applicable / Adapted**: The image digest (SHA256) cryptographically binds the entire filesystem tree. Individual RPM file verification is replaced by container digest verification. |
| `aide_periodic_checking` | Host-based file integrity monitoring (AIDE) running via cron. | Read-only `/usr` cannot be modified by attackers without rebooting into an alternate bootc deployment. | **Redundant**: The immutability of `/usr` provides stronger guarantees than post-facto AIDE cron scans. |
| `mount_option_var_noexec` | Mount `/var` with `noexec,nodev,nosuid`. | Container runtime storage (`/var/lib/containers`) and local dynamic units require execution permissions. | **Partial Pass**: Enforced via mount namespaces where feasible; exempt in root container engines. |

---

## 4. Remediation vs Immutability Policy

For mutable OS installations, `oscap xccdf eval --remediate` rewrites system configuration files in-place.
On Directioner-OS:
1. **Never apply mutable remediation scripts at runtime**: All compliant configuration hardening (e.g. `/etc/sysctl.d/99-security.conf`, `/etc/ssh/sshd_config.d/00-hardening.conf`, file permissions) MUST be baked directly into the OCI container image at build time.
2. **Track Compliance Score Over Time (48.5)**: Every release builds upon a regression tracking baseline. If a pull request causes the pass count to drop below 142/188, CI flags an unexpected posture regression.

---

## 5. Artifact Reference
The machine-readable raw scan summary is archived at:  
`docs/testing/oscap_scan_report.json`
