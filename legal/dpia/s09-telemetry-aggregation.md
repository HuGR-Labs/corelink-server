---
id: "DPIA-S09-001"
type: "dpia"
doc_status: "DRAFT"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
feature: "S-09 Telemetry Aggregation"
sprint: "S-09"
owner: "Gustavo Schneiter"
privacy_officer: "Gustavo Schneiter (interim)"
dpo: "TBD"
tags: ["dpia", "s09", "telemetry", "legitimate-interest", "pseudonymization", "gdpr-art-35", "lgpd-art-38"]
evidence_event: "EVT-045"
evidence_retain: "7y"
evidence_path: "evidence-legal/dpia-s09-telemetry-aggregation.md"
related_lia: "legal/lia/s09-telemetry-aggregation.md"
---

# DPIA: S-09 Telemetry Aggregation — Legitimate Interest Processing

> **GDPR Art. 35 / LGPD Art. 38 (RIPD) Data Protection Impact Assessment**
> Template: `specs/_templates/dpia.md` v1.0.0 — WI-S11-008
> **Related LIA:** `legal/lia/s09-telemetry-aggregation.md` (mandatory companion for legitimate interest basis)

---

## Metadata

| Field | Value |
|---|---|
| DPIA ID | DPIA-S09-001 |
| Feature / Sprint | S-09 telemetry aggregation pipeline (usage metrics, error rates, latency histograms) |
| Data Controller | HuGR Labs Ltda (LGPD) / HuGR Labs Ltd (GDPR) |
| Encarregado / DPO | TBD (interim: Gustavo Schneiter) |
| Privacy Officer | Gustavo Schneiter (interim) |
| Processing basis | legitimate_interest (GDPR Art. 6(1)(f) / LGPD Art. 10) — operational telemetry for service reliability, performance monitoring, abuse detection, and security. See LIA companion document. |
| Date initiated | 2026-05-13 |
| Target review date | 2026-06-13 |
| Legal frameworks | GDPR Art. 35 + LGPD Art. 38 + WP29 WP248rev01 (2017) endorsed by EDPB + ANPD Res. CD/ANPD nº 4/2023 + WP29 Opinion 06/2014 endorsed by EDPB (legitimate interest) |
| EVT evidence path | `evidence-legal/dpia-s09-telemetry-aggregation.md` (R2 retain 7y per EVT-045) |

---

## Section 1 — Description of Processing

### 1.1 Processing purpose(s)

S-09 processes operational telemetry data (API latency, error rates, usage counters, cache hit/miss ratios, rate-limit events) for three purposes:

1. **Service reliability monitoring:** detecting anomalies, performance degradations, and outages before they impact SLA commitments.
2. **Capacity planning:** aggregated usage metrics drive infrastructure scaling decisions (no per-user granularity required for capacity).
3. **Abuse / security detection (S-08 integration):** rate-limit events and unusual access patterns are analyzed to detect API abuse, credential stuffing, and tenant isolation violations.

Purpose enum (privacy_model.md §5.6.1): `service_delivery` (reliability/capacity) + `legitimate_interest` (abuse detection).

### 1.2 Data subjects affected

- **Registered users** whose API activity generates telemetry: every CoreLink user (API calls, blob uploads, DSR submissions, consent events).
- **Tenant administrators:** aggregate tenant-level metrics.
- **Scale:** all active users — potentially millions at GA scale.
- **Geographic spread:** global (multi-region).

### 1.3 Personal data categories processed

| Data category | Classification | Backend(s) | Retention | Notes |
|---|---|---|---|---|
| Pseudonymized subject_id (FNV-1a hash of tenant_id + subject_id) | Pseudonym | Loki, CF Analytics Engine | 90 days aggregate; 7y audit | Not directly identifying; keyed pseudonym |
| IP address (truncated to /24 for IPv4, /48 for IPv6) | Pseudonym | Loki | 30 days | Jittered + truncated per CTRL-PRIV-010 |
| API endpoint + HTTP method + status code | Ordinary | CF Analytics Engine, Loki | 90 days | Not PII on its own |
| Latency histograms (p50/p95/p99) | Ordinary | CF Analytics Engine | 90 days | Aggregate; no per-request |
| Rate-limit events (subject_id pseudonymized + endpoint + timestamp bucket) | Pseudonym | Loki, KV | 30 days | Bucket=15min granularity |
| Abuse detection signals (pattern score — no raw content) | Ordinary | Internal (S-08 worker) | 7 days | Ephemeral; no PII |
| Error traces (pseudonymized tenant + request ID) | Pseudonym | Loki | 14 days | No PII in trace body |

**No special-category data** processed in telemetry pipeline.

### 1.4 Data flows

1. CF Worker intercepts API request → extracts: tenant_id, pseudonymized subject_id, endpoint, latency, status, IP (truncated).
2. Metrics emitted via CF Analytics Engine (aggregate) + structured log to Loki (pseudonymized per-request at 1% sample rate for latency distribution).
3. Abuse detection worker (S-08) consumes rate-limit events from KV; runs sliding-window anomaly detection; no raw logs retained.
4. Aggregated dashboards in Grafana (no per-user drill-down available in standard tier).
5. No cross-border transfer: Loki runs in tenant's primary region (weur for EU tenants); CF Analytics Engine aggregates globally but stores only aggregates (not raw PII).

### 1.5 Retention periods

| Data | Retention | Legal basis |
|---|---|---|
| Raw pseudonymized logs (Loki) | 90 days | Operational necessity (legitimate interest) |
| Truncated IP | 30 days | Minimum for abuse detection; shorter retention insufficient for meaningful pattern analysis |
| Aggregate metrics (CF Analytics Engine) | 90 days | Operational necessity |
| Abuse detection ephemeral state (KV) | 7 days | Minimum window for pattern detection |
| Audit chain (EVT-022 + EVT-048) | 7 years | LGPD Art. 40; GDPR accountability principle |

### 1.6 WP29 WP248rev01 high-risk criteria checklist

| Criterion | Met? | Justification |
|---|---|---|
| 1. Evaluation/scoring (profiling) | PARTIAL | Abuse detection scores subjects on behavioral patterns; limited scope |
| 2. Automated decision-making with legal effect | NO | Rate-limit is operational (no legal/significant effect on subject) |
| 3. Systematic monitoring | YES | All API activity monitored systematically |
| 4. Sensitive data (special categories) | NO | No special-category data in telemetry |
| 5. Large-scale processing | YES | All active users at global scale |
| 6. Matching or combining datasets | NO | Pseudonymized data not combined with external datasets |
| 7. Data on vulnerable subjects | NO | Not targeting vulnerable groups |
| 8. Innovative use or new technology | YES | Real-time pseudonymization + streaming aggregation pipeline |
| 9. Data transfer outside EU/EEA or BR | PARTIAL | CF Analytics Engine global aggregation; aggregates only (not raw PII) |

**Criteria count: 3–4/9 — DPIA mandatory** (systematic monitoring + large-scale + innovative technology; partial profiling/cross-border).

---

## Section 2 — Necessity and Proportionality Assessment

### 2.1 Legal basis

**Primary basis: legitimate interest** (GDPR Art. 6(1)(f) / LGPD Art. 10). CoreLink's legitimate interest in operational reliability, service improvement, and security is well-established per WP29 Opinion 06/2014 endorsed by EDPB (IT security as legitimate interest). See companion LIA document (`legal/lia/s09-telemetry-aggregation.md`) for full ICO three-part test.

**LGPD Art. 10 requirements:** (a) purposes are legitimate (service reliability + security); (b) processing is necessary (see §2.2); (c) data subject rights respected via pseudonymization + opt-out mechanism.

### 2.2 Necessity test

Telemetry processing is necessary for:
- **SLA compliance:** without latency monitoring, SLA breaches cannot be detected or attributed.
- **Security:** without rate-limit events, credential stuffing and abuse cannot be detected.
- **Capacity planning:** without usage metrics, infrastructure cannot be right-sized.

**Less intrusive alternatives considered:**
- **Pure aggregation (no per-request log):** Would not support latency distribution analysis (p95/p99 require per-request data at sampling). Mitigation: 1% sampling rate applied.
- **Synthetic data only:** Cannot reflect real traffic patterns for abuse detection. Not sufficient.
- **Consent-based only:** Impractical for operational monitoring (100% opt-out would leave system unmonitorable). Consent not required for security/operational processing under LGPD Art. 10 / GDPR Rec. 47.

### 2.3 Proportionality assessment

- **Data minimization:** IP truncated to /24 (IPv4) / /48 (IPv6); subject_id pseudonymized via HKDF (not raw UUID); latency in buckets not raw nanoseconds.
- **Storage limitation:** 90-day rolling window; automatic deletion via Loki retention policy.
- **Purpose limitation:** telemetry not used for behavioral profiling for marketing; abuse detection is security-scoped.
- **Pseudonymization:** all per-subject fields pseudonymized per CTRL-PRIV-010 before emission.

### 2.4 Data protection by design measures

| Measure | Implementation |
|---|---|
| Subject pseudonymization (HKDF + FNV-1a) | CTRL-PRIV-010; keyed pseudonym prevents re-identification without key |
| IP truncation (jitter + /24 bucketing) | Implemented in CF Worker telemetry middleware |
| Sampling at 1% for per-request logs | CF Worker sampling middleware; reduces data volume 100x |
| 90-day auto-deletion (Loki retention) | Loki retention policy configured; not overridable by feature code |
| Aggregate-only default (CF Analytics Engine) | Raw events not stored; counters only |
| DSR erasure integration | pseudonymized_subject_id mapped to real subject_id via erasure worker key deletion; WI-S11-002 |
| No PII in trace bodies | Linting + PR check enforces no PII in structured log fields |
| Opt-out mechanism | Subject can opt out of telemetry beyond core operational necessity via privacy settings (WI-S11-004) |

### 2.5 Sub-processor obligations

| Sub-processor | Role | Location | DPA/SCC status |
|---|---|---|---|
| Cloudflare Inc. (CF Workers, CF Analytics Engine) | Compute + aggregation | US (global) | Cloudflare DPA + SCCs (Module 2) + BCRs |
| Grafana Labs | Dashboard rendering (no raw data retention) | US / EU | Grafana DPA + SCCs |

---

## Section 3 — Risk Identification

### 3.1 Risk register

| ID | Threat (LINDDUN) | Description | Likelihood | Severity | Risk Level | Mitigation |
|---|---|---|---|---|---|---|
| R-001 | Identifiability (I) | Re-identification of pseudonymized subject from combination of truncated IP + timestamp + endpoint pattern | LOW | MEDIUM | LOW | IP truncated + jittered; timestamps in 15-min buckets; no external dataset linkage |
| R-002 | Linkability (L) | Adversary with access to Loki queries subjects' API activity across sessions by correlating pseudonymous ID | LOW | MEDIUM | LOW | Loki access restricted to SRE role; no tenant-facing API for raw logs; audit log of Loki access |
| R-003 | Non-compliance (N) | Legitimate interest basis challenged by DPC/ANPD: telemetry held to be excessive without explicit consent | MEDIUM | HIGH | MEDIUM | LIA companion document (ICO 3-part test + WP29 8 criteria); strong opt-out mechanism; data minimization documented |
| R-004 | Disclosure (D) | Loki data breach exposes pseudonymized subject activity logs | LOW | MEDIUM | LOW | Loki hosted in tenant's primary region; encrypted at rest; access controls + API key rotation |
| R-005 | Unawareness (U) | Data subjects unaware telemetry is processed | LOW | MEDIUM | LOW | Privacy notice (WI-S11-004) explicitly discloses telemetry under legitimate interest basis; LIA available on request |
| R-006 | Linkability (L) | Incomplete erasure: DSR subject requests deletion but pseudonym key not rotated → old pseudonymized logs remain linkable | LOW | HIGH | MEDIUM | Erasure worker (WI-S11-002) deletes pseudonym key from KV; old logs become orphan pseudonyms (unlinked to subject). INV-DATA-ERASURE-COMPLETE TLA+ proof covers. |

---

## Section 4 — Mitigation Measures

### 4.1 Technical mitigations

**R-003 (Legitimate interest challenge):**
- Full LIA documented at `legal/lia/s09-telemetry-aggregation.md` with ICO three-part test + WP29 Opinion 06/2014 endorsed by EDPB 8 criteria.
- Opt-out implemented: users can disable non-core telemetry via privacy settings (operational/security telemetry retained per legitimate interest override).
- Privacy notice (WI-S11-004) is transparent about processing.

**R-006 (Incomplete erasure of pseudonymized logs):**
- Pseudonym key deleted from KV on DSR erasure → all historical log entries become permanently unlinked from subject (forward-privacy).
- Erasure worker (WI-S11-002) `Loki` backend implements key deletion + confirmation.
- INV-DATA-ERASURE-COMPLETE (CRITICAL) covers Loki backend in 12-backend erasure model.
- TLA+ proof `dsr_erasure_atomicity.tla` validates atomicity.

### 4.2 Organizational mitigations

- Annual LIA review (Privacy Officer).
- DPIA re-assessment trigger: any change to telemetry schema adding new PII fields → CI hook detects.
- SRE access to Loki governed by role-based access (documented in runbook).

### 4.3 Contractual / legal mitigations

- Cloudflare DPA (Module 2 SCCs) covers CF Analytics Engine.
- Grafana DPA in place.
- Privacy notice published and accessible before service activation.

### 4.4 Residual risk after mitigation

| Risk ID | Residual Likelihood | Residual Severity | Residual Level | Accepted by |
|---|---|---|---|---|
| R-001 | LOW | LOW | LOW | Privacy Officer |
| R-002 | LOW | LOW | LOW | Privacy Officer |
| R-003 | LOW | MEDIUM | LOW | Privacy Officer (LIA companion + opt-out mitigates challenge risk) |
| R-004 | LOW | LOW | LOW | Privacy Officer |
| R-005 | LOW | LOW | LOW | Privacy Officer |
| R-006 | LOW | MEDIUM | LOW | Privacy Officer (key-deletion approach makes logs permanently unlinked post-erasure) |

---

## Section 5 — Residual Risk Assessment + Sign-off

### 5.1 Overall residual risk assessment

All risks reduced to LOW after mitigation. The most material risk (R-003: legitimate interest challenge) is mitigated by the companion LIA document, strong data minimization, and opt-out mechanism. **Overall residual risk: LOW.** Processing may commence.

### 5.2 Consultation (GDPR Art. 36 / LGPD Art. 38 §3)

Prior supervisory authority consultation is **not required** — residual risk is LOW. LIA companion document is available if ANPD/DPC request documentation.

### 5.3 Sign-off

| Role | Name | Date | Status |
|---|---|---|---|
| Privacy Officer | Gustavo Schneiter (interim) | 2026-05-13 | Pending final sign-off pre-PRR |
| DPO / Encarregado | TBD | TBD | Pending hire |
| Legal (external) | TBD | TBD | Pending engagement |
| Compliance | TBD | TBD | Pending |

> **EVT-045:** Upload to `evidence-legal/dpia-s09-telemetry-aggregation.md` (R2 retain 7y) upon first sign-off.
> **EVT-046 (LIA):** `evidence-legal/lia-s09-telemetry-aggregation.md` (R2 retain 3y).

---

## Section 6 — Quarterly Review Schedule

| Review cycle | Target date | Trigger condition | Reviewer |
|---|---|---|---|
| Initial | 2026-06-13 | Pre-PRR | Privacy Officer |
| Q3 2026 | 2026-09-01 | Quarterly | Privacy Officer |
| Q4 2026 | 2026-12-01 | Quarterly | Privacy Officer |
| Q1 2027 | 2027-03-01 | Quarterly | Privacy Officer |
| Change-triggered | On any PR adding new telemetry field touching PII | CI hook detect | Privacy Officer |
| LIA annual review | 2027-05-13 | Annual | Privacy Officer |

### 6.1 DPIA changelog

| Version | Date | Author | Change summary |
|---|---|---|---|
| 1.0 | 2026-05-13 | Gustavo Schneiter | Initial DPIA — WI-S11-008 |
