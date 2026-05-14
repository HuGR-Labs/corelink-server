---
id: "LIGHTHOUSE-SLA-ATTESTATION-TEMPLATE"
type: "governance"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S20-004"
tags: ["lighthouse", "s20", "sla", "attestation", "template", "30d", "governance"]
---

# 30-Day SLA Attestation — Template (WI-S20-004)

> **Use:** Customer signs this form at D+40..D+45 (per `lighthouse-customer-program.md` §3) to attest the SLA claim was met sustained 30 days.
> **Storage:** Filed as `specs/_audits/2026-XX-XX-lighthouse-customer-{slot}-attestation.md` with `audit_status: ACTIVE`.
> **Library binding:** This template's *machine-readable* counterpart is the `SlaSample` record in `corelink-lighthouse-tracker`. Daily samples accumulate over the 30d window and the final aggregate becomes the values below.

---

## 1. Customer profile (filled by CoreLink Customer Success, customer reviews + approves)

| Field | Value |
|---|---|
| **Customer slot id** (internal) | `LH-<SLOT>` |
| **Customer name** | _Acme Corp_ |
| **Tier** | _Team_ / _Enterprise BYOK_ |
| **Vertical / industry** | _financial services / OSS / healthtech / ..._ |
| **Use case** | _Bazel RBE cache backend_ / _Buck2 CAS_ / _ML checkpoint store_ |
| **Pre-CoreLink baseline** | _S3 + bazel-remote-cache (self-hosted)_ |
| **Region(s) primary** | _wnam / enam / weur / sam_ |
| **Migration completion date** | _D+10 (2026-XX-XX)_ |
| **Observation window** | _D+10 to D+40 (30 days fixed; per OBSERVATION_WINDOW_SECS canonical)_ |
| **Attestation date** | _D+40..D+45 (2026-XX-XX)_ |

---

## 2. SLA claim measured (30-day actuals)

The values below MUST be drawn from the production observability stack (Prometheus `corelink_*` metrics) and cross-checked against the `lighthouse_sla_samples` D1 table (migration `0042`).

| SLO | Target | Actual (30d sustained) | Met? | Source |
|---|---|---|---|---|
| **SLO-AVAIL-CAS-PUT** | ≥ 99.9% | _99.9X%_ | _yes / no_ | `availability:cas_put_sli_30d` |
| **SLO-AVAIL-CAS-GET** | ≥ 99.9% | _99.9X%_ | _yes / no_ | `availability:cas_get_sli_30d` |
| **SLO-LAT-CAS-GET p99** | < 300 ms | _XXX ms_ | _yes / no_ | `histogram_quantile(0.99, ...)` over 30d |
| **SLO-FRESH-BILLING** | < 0.1% drift / 24h | _0.0X%_ | _yes / no_ | `corelink_billing_reconcile_drift_ratio` p99 over 30d |
| **SLO-FRESH-DSR-ERASURE** | ≤ 30 days | _N/A unless DSR triggered_ | _yes / N/A_ | `corelink_dsr_erasure_latency_hours_bucket` |

### 2.1 Cache hit ratio (informational; not an SLO at GA)

| Metric | Value |
|---|---|
| Cache hit ratio (30d avg) | _XX.X%_ |
| Cache hit ratio p99 daily | _XX.X%_ |
| GB stored at end of window | _X XXX GiB_ |

### 2.2 Enterprise BYOK additional fields (Enterprise BYOK tier ONLY)

| SLO / Health metric | Target | Actual | Met? |
|---|---|---|---|
| **SLO-BYOK-KILL-SWITCH** | ≤ 5 min p99 | _X min_ | _yes / no_ |
| **BYOK provider primary** | (declared) | _AWS KMS / GCP KMS / Azure KV / Vault_ | n/a |
| **BYOK matrix-test result** | 4/4 verde | _4/4 / 3/4 / ..._ | _yes / no_ |
| **BYOK key health at attestation time** | true | _true / false_ | _yes / no_ |
| **Erasure attestation Ed25519 verifiable** | yes (if DSR triggered) | _yes / no / N/A_ | _yes / N/A_ |

---

## 3. Operational observations during window

### 3.1 Incidents encountered

| Date | Severity | Description | Resolution time | Customer impact |
|---|---|---|---|---|
| _YYYY-MM-DD_ | _SEV-X_ | _short summary_ | _Xm / Xh_ | _none / minor / major_ |

(Add rows as needed. Zero rows is acceptable — "no incidents during observation window".)

### 3.2 DSR (data subject requests) tested

| Date | DSR type | Within SLO? | Erasure attestation signed? |
|---|---|---|---|
| _YYYY-MM-DD_ | _erasure / portability / access_ | _yes / no_ | _yes / no / N/A_ |

(N/A if no DSR was triggered during the window — common for early lighthouse customers.)

### 3.3 Customer Success engagements

| Week | Date | Topic | Notes |
|---|---|---|---|
| W1 | _YYYY-MM-DD_ | _kick-off check-in_ | _short summary_ |
| W2 | _YYYY-MM-DD_ | _bug triage_ | _short summary_ |
| W3 | _YYYY-MM-DD_ | _UX iteration_ | _short summary_ |
| W4 | _YYYY-MM-DD_ | _attestation prep_ | _short summary_ |

---

## 4. Aggregate attestation

By signing below, the customer attests:

- [ ] The SLA claim values in §2 reflect their production observation over the full 30-day window.
- [ ] All incidents encountered (§3.1) have been disclosed; no incidents have been omitted.
- [ ] Any DSR triggered (§3.2) was resolved within the canonical SLO and the erasure attestation (if applicable) was Ed25519-signed and verifiable.
- [ ] (Enterprise BYOK only) The BYOK key health (§2.2) was true at attestation time and the kill-switch chaos drill was run weekly with results within SLO.
- [ ] The customer authorizes CoreLink to reference this attestation in the GA Evidence Gate D+60 PRR pack (EVT-018) and (sanitized for Enterprise BYOK) in case-study materials.

---

## 5. Signatures

### 5.1 Customer

| Field | Value |
|---|---|
| Name | _Jane Doe_ |
| Role | _CTO / VP Engineering / Tech Lead / OSS Maintainer_ |
| Email | _jane@acme.example_ |
| Signature | _signed (DocuSign envelope id)_ |
| Date | _2026-XX-XX_ |

### 5.2 CoreLink

| Field | Value |
|---|---|
| Customer Success | _name + sign date_ |
| Engineer S-20 lead | _name + sign date_ |
| Privacy Officer (DPA-related sub-modes) | _name + sign date_ |
| Legal Counsel | _name + sign date_ |

---

## 6. Cross-references

- `specs/_lighthouse/lighthouse-customer-program.md` — program framework.
- `crates/corelink-lighthouse-tracker/src/lib.rs` — `LighthouseCustomer::transition_to(Attested)` gate logic; `SlaSample` daily sample shape.
- `migrations/d1/0042_lighthouse_customers.sql` — `lighthouse_sla_samples` evidence table.
- `specs/_runbooks/RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` — incident playbook (P0 priority).

---

**Fim LIGHTHOUSE-SLA-ATTESTATION-TEMPLATE.**
