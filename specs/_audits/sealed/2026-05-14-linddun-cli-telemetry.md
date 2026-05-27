---
id: "LINDDUN-CLI-TELEMETRY-2026-05-14"
type: "privacy_review"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
wi: "WI-S15-005"
tags: ["linddun", "privacy", "telemetry", "cli", "opt-in", "gdpr", "lgpd", "s15"]
---

# LINDDUN Privacy Review — CoreLink CLI Telemetry Opt-In

**Review date:** 2026-05-14  
**Scope:** `crates/corelink-cli` telemetry subsystem (`src/telemetry.rs`, `src/config.rs`)  
**WI:** WI-S15-005  
**Reviewer:** Gustavo Schneiter (Privacy Officer role folded in Product/DevX per STANDARD lane §28)

---

## 1. System Description

CoreLink CLI emits anonymized usage telemetry **only if explicitly enabled** by the user via
`corelink config set telemetry on` (persistent flag in `~/.corelink/config.toml`).

**Payload** (anonymized; no PII):

```json
{
  "cli_version": "0.1.0",
  "os": "linux-x86_64",
  "subcommand": "ls",
  "outcome": "ok",
  "duration_ms": 42,
  "anonymized_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**What is NEVER collected:** `tenant_id`, blob digests, PAT, file paths, IP address (server-side
scrubbed at ingestion), user identity.

**Endpoint:** `https://telemetry.corelink.humangr.com/v1/events` (separate domain from data plane
`corelink.humangr.com` per §9.4 — customers can firewall independently).

**Retention:** 90 days (server-side aggregation; raw events deleted after aggregation at 7d).

---

## 2. LINDDUN Threat Analysis

### L — Linkability

| Dimension | Assessment |
|---|---|
| Threat | `anonymized_id` (UUID v4) could be linked across sessions to build a usage profile. |
| Controls | UUID v4 is random (not derived from any user/tenant identifier). Rotatable via `corelink config rotate telemetry-id`. No server-side mapping to user/tenant. Separate domain prevents cross-origin correlation. |
| Residual risk | **LOW** — rotation capability + no PII linkage satisfies reasonable unlinkability. |
| GDPR/LGPD | GDPR Recital 26 (pseudonymisation); LGPD Art. 12 (anonymized data). |

### I — Identifiability

| Dimension | Assessment |
|---|---|
| Threat | Payload could identify a natural person or tenant. |
| Controls | Payload contains only: cli_version, os-arch slug, subcommand (enum), outcome (ok/err), duration_ms, anonymized_id (UUID v4). No username, email, tenant_id, PAT, hostname, file paths. IP address is scrubbed server-side at ingestion (X-Forwarded-For not stored). |
| Residual risk | **LOW** — payload is structurally anonymized; re-identification risk is negligible. |
| GDPR/LGPD | GDPR Art. 4(1) (personal data definition — not applicable); LGPD Art. 5 (personal data definition — not applicable). |

### N — Non-repudiation

| Dimension | Assessment |
|---|---|
| Threat | User cannot prove they did NOT enable telemetry (non-repudiation of consent). |
| Controls | Opt-in is explicit: user runs `corelink config set telemetry on`. Config file persisted at `~/.corelink/config.toml` with `telemetry = true`. Audit trail is local (config file timestamp). |
| Residual risk | **LOW** — opt-in logged locally; user can inspect `corelink config list` at any time. |
| GDPR/LGPD | GDPR Art. 7 (consent conditions — explicit opt-in satisfied); LGPD Art. 8 (consent). |

### D — Detectability

| Dimension | Assessment |
|---|---|
| Threat | Telemetry traffic is detectable by network monitoring tools. |
| Controls | Separate domain `telemetry.corelink.humangr.com` makes the traffic identifiable and blockable. This is a feature (transparency) not a threat in the privacy context. Customers can block via firewall without impacting data plane. |
| Residual risk | **LOW** — detectability is acceptable and transparent; aligns with privacy-by-design (GDPR Art. 25). |
| GDPR/LGPD | GDPR Recital 39 (transparency principle). |

### D — Disclosure of information

| Dimension | Assessment |
|---|---|
| Threat | Telemetry channel could disclose sensitive operational or business data. |
| Controls | Payload is strictly bounded to 6 fields (see §1). No blob content, no tenant business data, no PAT, no file paths. Enforced via: (a) static type system (Rust struct with only the 6 fields), (b) property test 0 PII across 10k iterations. HTTPS enforced (TLS 1.3 minimum). |
| Residual risk | **LOW** — structural bound on payload + test verification. |
| GDPR/LGPD | GDPR Art. 5(1)(c) (data minimisation). |

### U — Unawareness

| Dimension | Assessment |
|---|---|
| Threat | Users may be unaware that telemetry is collected. |
| Controls | Telemetry is **default off** — no data collected without explicit opt-in. Discoverable via `corelink config list` (always shows `telemetry: off/on`). Privacy policy published at `docs/cli/telemetry.md`. `corelink --help` references telemetry opt-in. |
| Residual risk | **LOW** — opt-in default-off is the strongest unawareness control (GDPR Art. 25 data protection by design). |
| GDPR/LGPD | GDPR Art. 25 (data protection by design and by default); LGPD Art. 6 X (transparency). |

### N — Non-compliance

| Dimension | Assessment |
|---|---|
| Threat | Telemetry design violates applicable privacy regulations. |
| Controls | GDPR Art. 25 compliant: default-off (data protection by design). GDPR Art. 7 compliant: explicit opt-in consent. GDPR Art. 5(1)(c): data minimisation enforced structurally. LGPD Art. 8: consent-based collection. LGPD Art. 6 X: transparency via privacy policy. No cross-border transfer concerns (telemetry.corelink.humangr.com regional endpoint per tenant residency model — S-14 alignment). |
| Residual risk | **LOW** — design is compliant with GDPR + LGPD at point of review. |
| GDPR/LGPD | GDPR Art. 25, Art. 7, Art. 5; LGPD Art. 8, Art. 6. |

---

## 3. Risk Summary

| LINDDUN Dimension | Risk | Mitigations | Residual |
|---|---|---|---|
| Linkability | LOW | UUID v4 rotatable; no tenant linkage | LOW |
| Identifiability | LOW | Structural payload bound; no PII fields | LOW |
| Non-repudiation | LOW | Explicit opt-in; local config audit | LOW |
| Detectability | LOW | Transparent; separate domain; blockable | LOW |
| Disclosure | LOW | 6-field struct; HTTPS; property test | LOW |
| Unawareness | LOW | Default-off; config list; privacy doc | LOW |
| Non-compliance | LOW | GDPR Art. 25 + 7 + 5; LGPD Art. 8 + 6 | LOW |

**Overall risk: LOW across all 7 LINDDUN dimensions.**

---

## 4. Invariants Ratified

- **INV-TELEMETRY-DEFAULT-OFF**: `Config::default().telemetry == false` — enforced by type default + property test.
- **INV-PAYLOAD-NO-PII**: Payload struct has no PII fields — enforced by Rust type system + `prop_payload_never_contains_pii` (10k iterations).
- **INV-EMIT-GUARD**: `emit_if_enabled(false, _)` is always a no-op — enforced by code path + test.

---

## 5. Controls Cross-Reference

| Control | Mechanism | Test |
|---|---|---|
| CTRL-CRED-001 | PAT never in telemetry payload | `serialised_payload_no_forbidden_fields` |
| CTRL-AUDIT-002 | Opt-in config change auditable via config file | Config roundtrip tests |
| INV-OBS-CARDINALITY-BUDGET | Telemetry labels bounded (no per-tenant labels) | Payload struct review |

---

## 6. Sign-off

| Role | Name | Status | Date |
|---|---|---|---|
| Privacy Officer (folded in Product/DevX per STANDARD lane) | Gustavo Schneiter | APPROVED | 2026-05-14 |
| Engineer (WI-S15-005 implementer) | Gustavo Schneiter | APPROVED | 2026-05-14 |

---

*End of LINDDUN review — WI-S15-005. All 7 dimensions: LOW. Design ratified.*
