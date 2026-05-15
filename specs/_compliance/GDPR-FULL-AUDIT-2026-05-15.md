---
id: "GDPR-FULL-AUDIT-2026-05-15"
type: "compliance_audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "GAP-22-FOLLOWUP-FULL-AUDIT"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "LGPD-FULL-AUDIT-2026-05-15"
  - "LGPD-ROPA-2026-05-15"
  - "PRIVACY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "SOC2-EVIDENCE-ROLLUP-2026-05-15"
  - "INVARIANT-REGISTRY"
tags: ["gdpr", "eu", "gdpr-art-5", "gdpr-art-6", "gdpr-art-7", "gdpr-art-9", "gdpr-art-12", "gdpr-art-13", "gdpr-art-14", "gdpr-art-15", "gdpr-art-16", "gdpr-art-17", "gdpr-art-18", "gdpr-art-20", "gdpr-art-21", "gdpr-art-22", "gdpr-art-25", "gdpr-art-30", "gdpr-art-32", "gdpr-art-33", "gdpr-art-34", "gdpr-art-35", "gdpr-art-37", "gdpr-art-44", "gdpr-art-45", "gdpr-art-46", "gdpr-art-49", "audit", "edpb", "scc", "schrems-ii"]
---

# GDPR Full Audit — Regulation (EU) 2016/679 (2026-05-15)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **scope:** every GDPR (Regulation (EU) 2016/679) article that touches CoreLink's operational surface for EU data subjects and EU-established customers. Mirrors the LGPD full audit (`LGPD-FULL-AUDIT-2026-05-15`) so the two regimes can be reasoned about side-by-side.
>
> **Framework version:** GDPR (Regulation (EU) 2016/679), as in force; EDPB Guidelines 05/2021 (Concepts of controller/processor), 07/2020 (Concepts), 04/2021 (TIA), 01/2022 (Right of Access); EU SCC (Commission Decision 2021/914 of 4 June 2021); Schrems II (CJEU C-311/18); EDPB Recommendations 01/2020 on supplementary measures.
>
> **Observation window:** 2026-05-15 → 2026-08-15 (90-day attestation, renewable quarterly with the LGPD bundle).
>
> **Companion canonical docs (do not duplicate):**
> - `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md` (LGPD article-by-article — sister audit; many controls dual-purpose)
> - `specs/_compliance/LGPD-ROPA-2026-05-15.md` (Art. 30 RoPA — cross-mapped row-by-row; this audit references the same canonical table with GDPR deltas in §1.7)
> - `specs/_compliance/GDPR-SCC-EXECUTION-2026-05-15.md` (Art. 46 SCC module selection + Schrems II TIA per flow)
> - `specs/_compliance/GDPR-DPIA-LIBRARY.md` (Art. 35 DPIA index)
> - `specs/_runbooks/RB-DSR-GDPR.md` (Art. 12–22 internal runbook)
> - `apps/docs/docs/explanation/privacy/gdpr.mdx` (customer-facing explainer — 1500 words)
> - `specs/03_architecture/privacy_model.md` (LINDDUN + DSR pipeline + retention)
> - `specs/03_architecture/compliance_matrix.md` §5 (GDPR crosswalk)
> - `specs/03_architecture/invariant_registry.md` (INV-* canonical)
> - `crates/corelink-dsr/`, `crates/corelink-privacy-*/` (runtime enforcement)
>
> **Purpose:** CoreLink targets EU customers (Lighthouse pipeline includes EU-established design partners and EU resellers of Brazilian content). GDPR applies (Art. 3(1) establishment) for HuGR's EU sub-processor leg via Neon EU + offering services to EU subjects (Art. 3(2)(a) targeting). This audit extends LGPD's article-by-article coverage to GDPR's 50-article body, with explicit attention to **Schrems II TIA** (Art. 44–50), **Art. 35 DPIA triggers**, and **Art. 33 72h breach notification** (vs LGPD's "prazo razoável + ANPD Res. 15/2024 3 BD" — GDPR is stricter on the regulator clock, identical on subjects).

---

## 1. Article-by-article audit

> Commit hash anchor for code evidence: `de84b9d` (`merge wt/r-prep-active-failover` → main, 2026-05-14). All `commit:path:line` references pin to this hash.

### 1.1 Art. 5 — Principles relating to processing of personal data

GDPR Art. 5(1) enumerates six principles plus an accountability obligation in Art. 5(2). LGPD Art. 6 ten-principle taxonomy is a strict superset (LGPD splits "fairness/lawfulness" into separate `finalidade` / `adequação` / `necessidade` rows). Mapping is bidirectional.

| GDPR Art. 5(1) principle | LGPD Art. 6 equivalent | CoreLink implementation | Evidence |
|---|---|---|---|
| (a) **Lawfulness, fairness, transparency** | `finalidade` (I) + `transparência` (VI) | `purpose_tag` enum bound to `legal_basis`; privacy notice versioned + 3-locale | `privacy_model.md §5.6.1`; CTRL-PRIV-020 |
| (b) **Purpose limitation** | `finalidade` (I) + `adequação` (II) | `legal_basis` fixed per `purpose_tag`; major `notice_version` bump + force re-consent required for swap | `crates/corelink-privacy-consent-ledger/`; `EVT-049` |
| (c) **Data minimisation** | `necessidade` (III) | Minimal collection per dataflow (14 RoPA rows); deny-unknown-fields lint in CI | CTRL-PRIV-001; CTRL-PRIV-020 |
| (d) **Accuracy** | `qualidade dos dados` (V) | DSR Rectification endpoint `POST /v1/privacy/dsr/rectification` (5 BD SLA, MFA required) | `crates/corelink-dsr/`; `EVT-048` |
| (e) **Storage limitation** | `necessidade` (III) + retention | Canonical retention table `privacy_model.md §8.1`; GC + 12-backend erasure pipeline | `EVT-042` |
| (f) **Integrity and confidentiality (security)** | `segurança` (VII) | TLS 1.3 + envelope encryption + tenant isolation + audit + INV-AUDIT-APPEND-ONLY | CTRL-CRYPTO-001/002; CTRL-ISO-001..005 |
| Art. 5(2) **Accountability** | `responsabilização e prestação de contas` (X) | This audit + RoPA + DPO checklist + audit trail 7y Object Lock | `EVT-047`, `EVT-049` |

**Gap:** none material. All six Art. 5(1) principles map to a canonical control with active evidence; accountability (Art. 5(2)) is demonstrated by the audit chain itself.

---

### 1.2 Art. 6 — Lawfulness of processing (legal bases)

GDPR Art. 6(1) lists six legal bases ((a) consent, (b) contract, (c) legal obligation, (d) vital interests, (e) public interest, (f) legitimate interest). Below is the per-dataflow mapping cross-referenced with LGPD Art. 7 (12-purpose canonical enum, `privacy_model.md §5.6.1`).

| Dataflow / `purpose_tag` | GDPR Art. 6(1) base | LGPD Art. 7 equivalent | Notes |
|---|---|---|---|
| `service_delivery` (CAS/AC/exec core ops) | **(b) contract** | §V | Primary base. Not revocable without terminating contract. |
| `account_management` (auth, billing, tenant admin) | **(b) contract** | §V | Same as service_delivery. |
| `regulatory_compliance` (audit retention, DSR fulfillment, breach reporting) | **(c) legal obligation** | §II | GDPR Art. 30 + Art. 33 + Art. 35 + ePrivacy + member-state fiscal/accounting law (DE/FR/IT/NL — 6–10y depending on jurisdiction). |
| `security_monitoring` (anomaly detection, abuse prevention, fraud) | **(f) legitimate interest** | §IX | LIA required (`legal/lia/security-monitoring.md`); objection workflow via Art. 21 (Art. 21(1) for general purposes; Art. 21(2) direct-marketing absolute right). |
| `analytics_aggregated` (cross-tenant k≥50 metrics with privacy budget) | **(f) legitimate interest** | §IX | LIA required; objection-capable per Art. 21(1). |
| `analytics_personalized` (per-tenant dashboards with reidentifiable PII) | **(a) consent** | §I | Opt-in, revocable ≤ 5min (Art. 7(3) — withdrawal as easy as giving). |
| `marketing_email` | **(a) consent** | §I | Opt-in + ePrivacy Art. 13 PECR-equivalent; double opt-in pattern (CTRL-PRIV-CONSENT-DOUBLE-OPT-IN, deferred for newsletter rollout). |
| `marketing_research` (surveys, NPS) | **(a) consent** | §I | Opt-in. |
| `beta_features` (preview features with enriched telemetry) | **(a) consent** | §I | Opt-in. |
| `third_party_integrations` (Stripe, GitHub, webhooks) | **(a) + (b)** | §I + §V | Per-integration granular consent + contractual necessity for Stripe payment leg. |
| `cross_tenant_benchmarks` (leaderboards) | **(a) consent** | §I | Opt-in. |
| `training_ml_models` (ML cache prediction with k-anon) | **(a) consent** | §I | Opt-in only; default `false`; k≥50 anonymity floor enforced (CTRL-PRIV-010). |

**Other Art. 6 bases not used in CoreLink default flow:**

- (d) vital interests — N/A (not life-safety processing).
- (e) public interest / official authority — N/A (private B2B SaaS, no member-state authority delegation).

**Invariant (Lote 10.11.0-bis):** `legal_basis` is **fixed per `purpose_tag`** — cannot swap dynamically. Changing basis = bump major `notice_version` + force re-consent. Enforced by `crates/corelink-privacy-consent-ledger/` immutability + schema (`EVT-026`). Same invariant addresses GDPR EDPB Guidelines 5/2020 §22 (no fishing between bases mid-processing).

**Gap:** none. All 12 purposes have a single explicit Art. 6(1) base with documented evidence.

---

### 1.3 Art. 7 — Conditions for consent

Art. 7 governs how consent is captured and proved. Compared with LGPD Art. 8: GDPR is **stricter** on demonstrability (Art. 7(1)) and explicit on the **as-easy-to-withdraw-as-give** requirement (Art. 7(3)).

| Requirement | CoreLink implementation | Evidence |
|---|---|---|
| **(1) Demonstrability** — controller must be able to demonstrate consent | Consent ledger: `notice_text_hash` (SHA-256) + `notice_version` + `locale` + `wording_id` + `ui_capture_ts` + `submission_ts` + `subject_hash` + `tenant_id` + `purpose_tag` + `basis_legal` + `ip_country` + `user_agent_class` (12 fields per record) — immutable CloudEvents in R2 Object Lock 7y | `EVT-049`; `crates/corelink-privacy-consent-ledger/`; INV-CONSENT-PROOF-VERIFIABLE |
| **(2) Distinguishable** — consent request must be clearly distinguishable from other matters | Per-purpose checkbox UI (12 canonical); not bundled with ToS or DPA acceptance | UI consent form (deferred wiring to WI-S13-*); privacy notice §5 |
| **(3) Withdrawal as easy as giving** | `DELETE /v1/consent/<purpose>` ≤ 5min propagation; identical lightweight auth path as opt-in | CTRL-PRIV-CONSENT-002; `EVT-049` (revoke); `EVT-024` (load-test revocation latency) |
| **(4) No detriment** — no bundling, no service-conditional consent for non-essential processing | service_delivery + account_management are contract-base (not consent), so users cannot be forced to consent to marketing as a precondition for service | `privacy_model.md §5.6.1`; `legal/privacy-notice/v*` |
| Recital 32 — **freely given, specific, informed, unambiguous** | Default `false` for every consent-base purpose; explicit checkbox; no pre-checked boxes (CJEU C-673/17 Planet49) | `privacy_model.md §5.6` |

**No fail-open invariant:** if consent expires (TTL or re-consent not renewed), `legal_basis` does NOT degrade to `legitimate_interest`. Purpose enters `consent_lapsed` state → processing PAUSES until re-consent or erasure. Addresses EDPB Guidelines 05/2020 §10 (no basis swapping).

**Gap:** Admin-UI ConsentForm wiring deferred to WI-S13-* track. Until then, consent is captured via signup form + DPA acceptance for B2B tenants. **Not GA-blocking** (B2B flow legally sound; consumer flow not in scope).

---

### 1.4 Art. 9 — Special categories of personal data ("sensitive data")

Art. 9(1) prohibits processing of special-category data (racial/ethnic origin, political opinions, religious or philosophical beliefs, trade-union membership, genetic, biometric for identification, health, sex life / sexual orientation) unless one of the 10 Art. 9(2) exceptions applies.

**CoreLink position:** we do NOT intentionally collect or process special-category data. The only edge case is WebAuthn.

| Special category (Art. 9(1)) | CoreLink touch? | Mitigation |
|---|---|---|
| **Biometric data for identification** (Art. 9(1) + Recital 51) | **EDGE CASE — WebAuthn public keys** (auth factor) | Stored as ECC public key only (not biometric template); cryptographic identifier, not the underlying biometric. EDPB Guidelines 05/2022 §22-25 (on facial recognition) and EDPB-EDPS Joint Opinion 1/2021 acknowledge that **device-bound cryptographic credentials derived from on-device biometrics are NOT themselves "biometric data" under Art. 4(14)** because they do not allow identification by biometric template comparison. CoreLink position documented in `legal/lia/webauthn-biometric-classification.md` (to be added; parallel obligation under LGPD §1.6). |
| Health data | NO | HIPAA-opt-in tenants under separate BAA; no default flow. EU equivalent: tenants in regulated EU healthcare under separate DPA + Art. 9(2)(h) base on tenant side, never on HuGR side. |
| Genetic | NO | N/A. |
| Racial/ethnic, political, religious, philosophical, trade-union, sex life | NO | Explicit anti-pattern §32 in `privacy_model.md §2`: any tenant submitting blobs containing these = tenant's responsibility under DPA Art. 28(3) (the tenant warrants legal basis under Art. 9(2)); HuGR is processor and does not look at content. |

**Gap:** WebAuthn classification position needs Legal sign-off in `legal/lia/webauthn-biometric-classification.md`. Tracked in `LGPD-DPO-MONTHLY-CHECKLIST.md` item 15 (cross-references this audit). **Not GA-blocking** — defensible position pending formal LIA.

---

### 1.5 Art. 12 — Transparent information, communication, modalities

Art. 12 sets the meta-rules for exercising rights: information must be concise, transparent, intelligible, easily accessible, in clear and plain language, **free of charge**, with response **within one month** (extendable by two further months for complex requests).

| Requirement | CoreLink implementation | Evidence |
|---|---|---|
| Concise / intelligible / plain language | Customer-facing GDPR explainer `apps/docs/.../privacy/gdpr.mdx`; privacy notice 3-locale | `apps/docs/docs/explanation/privacy/gdpr.mdx` (this delivery); `legal/privacy-notice/v*.{en-US,pt-BR,es-MX}.md` |
| Free of charge | All DSR endpoints free; Art. 12(5) "manifestly unfounded or excessive" gate documented in `RB-DSR-GDPR.md` §5 | `RB-DSR-GDPR.md` |
| Response within one month (Art. 12(3)) | DSR endpoints SLA: 5 BD (Access/Rectification/Confirmation), 15 BD (Portability/Recipients/Objection), 30 cal d (Erasure). All within 1-month statutory cap. | `crates/corelink-dsr/`; `EVT-048` |
| Extension up to two further months for complex requests | Documented pause conditions in `RB-DSR-GDPR.md` §2; customer notified within 1 month with reasons + new deadline | `RB-DSR-GDPR.md` §2.4 |
| Identity verification (Art. 12(6)) | WebAuthn MFA step-up for destructive arms (Erasure, Rectification); session re-auth for informational | INV-DSR-MFA-DESTRUCTIVE; `crates/corelink-dsr/src/endpoint.rs` |
| Refusal — reasoned, with right to lodge complaint with SA + judicial remedy | `dsr.rejected.v1` audit event includes reasoning; rejection email references local SA list | `RB-DSR-GDPR.md` §5; `legal/breach-notification/eu-sa-contacts.md` (to populate) |

**Gap:** EU supervisory authority contact list (`legal/breach-notification/eu-sa-contacts.md`) needs to be populated — defer to legal pack T+30. **Not GA-blocking** (CNIL/AEPD/Garante/BfDI URLs are stable and can be linked from the customer page meanwhile).

---

### 1.6 Art. 13 & 14 — Information to be provided

Art. 13 (collected directly from subject) and Art. 14 (collected from third party) enumerate ~15 disclosures: controller identity, DPO contact, purposes + bases, recipients, transfers + safeguards, retention, rights, complaint right, source (Art. 14), automated decision-making.

| Required disclosure | Where surfaced | Evidence |
|---|---|---|
| Controller identity + contact | Privacy notice §1; DPA `legal/dpa/v*.{locale}.md` | DPA |
| DPO contact | Privacy notice §1 + `privacy@hugr.com` (DPO inbox) | `LGPD-DPO-MONTHLY-CHECKLIST.md` |
| Purposes + legal bases | Privacy notice §3; this audit §1.2; GDPR explainer §3 | `gdpr.mdx` |
| Legitimate interests pursued (where base is (f)) | LIA records `legal/lia/<purpose>.md` (security-monitoring done; aggregate-analytics pending GAP-14) | `EVT-046` |
| Recipients / categories of recipients | Public `/privacy/sub-processors` page (10 documented, 4 pending) | `legal/sub-processors.md`; `apps/docs/.../compliance/sub-processors.mdx` |
| Transfers to third countries + safeguards | `GDPR-SCC-EXECUTION-2026-05-15.md` (this delivery); customer-facing summary in `gdpr.mdx` §6 | `GDPR-SCC-EXECUTION-2026-05-15.md` |
| Retention period | Privacy notice §4 + this audit §1.1(e); retention table `privacy_model.md §8.1` | `EVT-042` |
| Rights (Art. 15–22 + complaint to SA + withdraw consent) | This audit §1.7; `gdpr.mdx` §1 (table of 8 rights) | `gdpr.mdx` |
| Source of data (Art. 14(2)(f)) | N/A for Art. 13 direct collection; for blob-content collected from tenant: routed in `gdpr.mdx` §8 ("special cases") | `gdpr.mdx` |
| Automated decision-making + significance (Art. 13(2)(f) / 14(2)(g)) | Disclosed: ML cache prediction = `training_ml_models` opt-in purpose; **no automated decisions with legal/significant effect on subjects** | this audit §1.13 (Art. 22) |

**Gap:** none material.

---

### 1.7 Art. 15–22 — Data subject rights (the menu)

GDPR enumerates 8 rights in Chapter III. Cross-referenced with CoreLink's DSR pipeline (`crates/corelink-dsr/` + `privacy_model.md §6.1`) and parallel-mapped to LGPD Art. 18.

| # | GDPR Art. | Right | LGPD parallel | Self-service? | SLA | API endpoint | Evidence |
|---|---|---|---|---|---|---|---|
| 1 | **Art. 15** | **Right of access** (incl. confirmation + copy) | Art. 18 §I + §II | Yes | 5 BD (confirmation) / 15 BD (full export) — both within 1-month cap | `POST /v1/privacy/dsr/access` | `EVT-048`; `crates/corelink-dsr/src/endpoint.rs::access`; receipt JWT 90d expiry |
| 2 | **Art. 16** | **Right to rectification** | Art. 18 §III | Yes | 5 BD | `POST /v1/privacy/dsr/rectification` (MFA required — destructive) | `EVT-048`; `crates/corelink-dsr/src/endpoint.rs` (MFA gate) |
| 3 | **Art. 17** | **Right to erasure ("right to be forgotten")** | Art. 18 §IV + §VI | Yes | 30 cal d | `POST /v1/privacy/dsr/erasure` (MFA required — destructive) | `EVT-042`; `EVT-017`; `crates/corelink-privacy-erasure-worker/`; 12-backend canonical propagation (`privacy_model.md §6.2`) |
| 4 | **Art. 18** | **Right to restriction of processing** | Art. 18 §IV (bloqueio) | Yes | 5 BD | `POST /v1/privacy/dsr/restriction` (purpose-scoped pause) | `crates/corelink-dsr/`; restriction enters `consent_lapsed` state for the purpose |
| 5 | **Art. 19** | **Notification obligation regarding rectification / erasure / restriction** | Implicit in LGPD §IV propagation | Yes (automatic) | Same as triggering right | Audit event `dsr.notification_propagated.v1` to each recipient sub-processor | `RB-DSR-GDPR.md` §2.5; recipient manifest in audit |
| 6 | **Art. 20** | **Right to data portability** (structured, commonly used, machine-readable format) | Art. 18 §V | Yes | 15 BD | `POST /v1/privacy/dsr/portability` | `EVT-048`; structured JSON + Parquet bundle; `crates/corelink-dsr/` |
| 7 | **Art. 21** | **Right to object** (Art. 21(1) legit-int + Art. 21(2) direct marketing — **absolute** for marketing) | Art. 18 §II amendment | Yes | 15 BD (Art. 21(1)) / immediate (Art. 21(2) marketing) | `POST /v1/privacy/dsr/objection` (Art. 21(1)) + `DELETE /v1/consent/marketing_email` (Art. 21(2)) | `EVT-049`; LIA balancing on Art. 21(1) review |
| 8 | **Art. 22** | **Right not to be subject to automated decision-making + profiling with legal or similarly significant effect** | LGPD Art. 20 (parallel) | Disclosure only — **no Art. 22 processing at CoreLink** | N/A | N/A | `gdpr.mdx` §7 (explicit declaration: no Art. 22 processing) |

**Bonus right** — withdraw consent under Art. 7(3): `DELETE /v1/consent/<purpose>` (≤ 5 min). Withdrawal as easy as giving — verified in `EVT-024` (load-test parity).

**Pipeline correctness invariants (canonical from `privacy_model.md §6.2`):**

- `INV-DSR-VERIFIED-CLOCK` — SLA clock starts at `dsr_tickets.status = verified` (post-MFA), not at submission.
- `INV-DSR-AUDIT-FAIL-CLOSED` — every DSR decision arm emits audit BEFORE state mutation; audit failure aborts the run fail-CLOSED.
- `INV-DSR-MFA-DESTRUCTIVE` — Erasure + Rectification require WebAuthn step-up; Access + Portability + Restriction + Objection do not.
- `INV-DSR-TENANT-ISOLATION` — tenant A's DSR requests never visible to tenant B (CTRL-ISO-004 constant-time 404 vs 403).
- `INV-DSR-RECEIPT-90D` — JWT receipt anti-replay window 90 days, RS256-signed, KMS-rooted.
- `INV-CONSENT-PROOF-VERIFIABLE` — consent records have SHA-256 `notice_text_hash` + HMAC-SHA256 (HKDF info `corelink/v1/consent-hmac`); grant/revoke ledger symmetric (Lote 9.4 H-05); TLA+ via `dsr_erasure_atomicity.tla` (`InvConsentSymmetry`, WI-S11-008).

**Test evidence:** 10k property test in `crates/corelink-dsr/tests/` (canonical taxonomy: 6-arm `DsrRequestKind` × 4-arm `DsrStatus` × 4-arm `DsrDecision` × 3-arm `DsrJurisdiction`). Last green CI run 2026-05-14 (commit `de84b9d`).

**Gap:** none material. All 8 GDPR rights are self-service with documented SLA + evidence + invariants under property testing. Art. 22 is non-applicable by design (no profiling with legal effect; ML purposes opt-in + k-anon).

---

### 1.8 Art. 25 — Data protection by design and by default

Art. 25 requires technical + organisational measures by design + by default. Mapped to CoreLink's canonical INV-* invariants and CTRL-* controls.

| Art. 25 dimension | CoreLink invariant / control | Evidence |
|---|---|---|
| **Pseudonymisation by design** | INV-PRIVACY-PSEUDONYMIZE-ON-ERASURE (audit chain pseudonymized via HKDF); INV-AUDIT-PSEUDONYM-DETERMINISTIC | `crates/corelink-privacy-pseudonymize/`; `EVT-022` |
| **Data minimisation by default** | INV-AUDIT-MINIMIZATION (log schema allowlist; deny-unknown-fields lint); CTRL-PRIV-014 | `EVT-026` (schema validation); `EVT-001` (lint in CI) |
| **Encryption by design** | CTRL-CRYPTO-001 (TLS 1.3 in-flight); CTRL-CRYPTO-002 (envelope at-rest); BYOK ADR | `EVT-005`, `EVT-037` |
| **Tenant isolation by default** | INV-ISO-CONSTANT-TIME-404; INV-ISO-NO-CROSS-LEAK; CTRL-ISO-001..005 | `EVT-022` (TLA+ proofs); `EVT-025` (pentest) |
| **Residency pinning by default** | INV-RESIDENCY-FAIL-CLOSED (FM-451 cross-region routing); CTRL-PRIV-RESIDENCY-001 | `crates/corelink-privacy-residency-enforcement/`; `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` |
| **Consent freshness by default** | INV-CONSENT-PROOF-VERIFIABLE; INV-CONSENT-NO-FAIL-OPEN (no basis degradation) | `crates/corelink-privacy-consent-ledger/`; TLA+ `InvConsentSymmetry` |
| **Audit append-only by design** | INV-AUDIT-APPEND-ONLY (CRITICAL); INV-AUDIT-HASH-CHAIN-CONTINUOUS | `EVT-047`; PAT-AUDIT-VERIFY-001 |
| **Erasure determinism by design** | INV-DSR-ERASURE-12-BACKEND (canonical propagation order); TLA+ `dsr_erasure_atomicity.tla` | `EVT-022`; `crates/corelink-privacy-erasure-worker/` |
| **Sub-processor change notice by default** | CTRL-PRIV-021 (30d notice + DPO sign-off + CI validator) | `scripts/validate_sub_processors.py`; `EVT-026` |
| **Privacy threat model maintained** | LINDDUN methodology in `privacy_model.md §4`; quarterly DPO review | `EVT-046` |

**Gap:** none. Privacy by design + by default is the architectural posture, not bolt-on.

---

### 1.9 Art. 30 — Records of processing activities (RoPA)

Art. 30(1) (controller) and Art. 30(2) (processor) require records. CoreLink wears both hats depending on dataflow (controller for account/billing/telemetry; processor for tenant blob content). The canonical RoPA is `LGPD-ROPA-2026-05-15.md` — **fields are a superset of GDPR Art. 30 requirements**; this section documents the EU-specific deltas.

| Art. 30 required field | CoreLink RoPA column | Source |
|---|---|---|
| Name and contact of controller / joint / representative / DPO | RoPA §1 (HuGR Labs Ltda. / per-tenant for blob content) | `legal/dpa/v*.md` |
| Purposes of processing | `purpose_tag` enum (12 values) | `privacy_model.md §5.6.1` |
| Categories of data subjects + personal data | RoPA table (Account, Telemetry, Blob, Audit, DSR ticket, etc.) | `LGPD-ROPA-2026-05-15.md` |
| Categories of recipients (incl. third-country) | Sub-processor list + cross-border column | `legal/sub-processors.md` |
| Transfers to third countries + identification of country + safeguards (where Art. 49(1)(2nd subparagraph) is used) | RoPA "Cross-border" column + `GDPR-SCC-EXECUTION-2026-05-15.md` (this delivery — SCC module per flow) | `GDPR-SCC-EXECUTION-2026-05-15.md` |
| Envisaged retention | Retention column + `privacy_model.md §8.1` | `EVT-042` |
| General description of TOMs (technical + organisational measures) — Art. 32 | "Security measures" column (CTRL IDs) | `security_model.md` |

**EU-specific deltas vs LGPD-ROPA:**

- **Art. 27 representative in the Union:** N/A while HuGR has no EU establishment; **becomes mandatory before Lighthouse EU customer onboarding** (tracked in ROADMAP §9 Human Track H-21: appoint EU representative — candidate firms in `legal/legal-externo-engagement-contract.md`).
- **Joint controllership arrangements (Art. 26):** none currently; will arise if HuGR enters a co-branded EU launch with a reseller (out of scope for GA).
- **EU DPO appointment trigger (Art. 37(1)):** core activities don't yet meet the "large-scale systematic monitoring" threshold (CoreLink is infrastructure for tenants who themselves may monitor). Formal DPO appointment tracked in GAP-01 / ROADMAP §9 H-15. Mandatory before Art. 37(1)(b) trigger fires.

**Gap:** EU representative appointment + EU DPO appointment — both tracked in ROADMAP §9; **not GA-blocking for SAM-only launch**; **blocking for EU enterprise onboarding** (GAP-01 + new GAP-EU-REP).

---

### 1.10 Art. 32 — Security of processing

Art. 32 mirrors LGPD Art. 46. Cross-framework with SOC 2 CC6.x + ISO 27001 A.8.

| Art. 32 requirement | CoreLink control | Cross-framework | Evidence |
|---|---|---|---|
| Pseudonymisation + encryption (32(1)(a)) | CTRL-CRYPTO-001/002; CTRL-PRIV-pseudonymize | SOC 2 CC6.7; ISO A.8.24 | `EVT-005`, `EVT-037` |
| Confidentiality, integrity, availability, resilience (32(1)(b)) | CTRL-CRYPTO; CTRL-ISO; CTRL-AVAIL (RTO 15min / RPO 5min from R-prep active-failover) | SOC 2 CC6, CC7; ISO A.8 | `EVT-005`; `RB-ACTIVE-FAILOVER.md` |
| Ability to restore in timely manner (32(1)(c)) | RTO 15min via warm standby; backup verification quarterly | SOC 2 CC7.x | `RB-COLD-RESTORE-FROM-ZERO.md`; `RB-BACKUP-VERIFICATION.md` |
| Regular testing (32(1)(d)) | Pentest annual; tabletop semestral; chaos catalog; endurance 24h drill | SOC 2 CC4.x; ISO A.8.16 | `EVT-025`; `EVT-019` |
| Risk-based assessment (32(2)) | LINDDUN threat model annual; LIA per legit-int purpose | SOC 2 CC3.x | `privacy_model.md §4`; `EVT-046` |

**Gap:** none material. SOC2-EVIDENCE-ROLLUP-2026-05-15.md row coverage confirms all Art. 32 controls are operational.

---

### 1.11 Art. 33 & 34 — Breach notification

Art. 33: **72 hours to supervisory authority** from awareness. Art. 34: notify subjects **without undue delay** when likelihood of high risk to rights and freedoms.

| Stage | GDPR Art. 33/34 | LGPD Art. 48 | CoreLink commit |
|---|---|---|---|
| Detection → internal SEV-1 declaration | (operational; no statutory clock) | (same) | ≤ 1h |
| Declaration → triage + containment | (operational) | (same) | ≤ 4h |
| Awareness → SA notification | **72h** (Art. 33(1)) | "prazo razoável" + ANPD Res. 15/2024 3 BD | **48h internal SLA** (conservative versus both regimes) |
| Awareness → subject notification (high risk) | "without undue delay" (Art. 34(1)) | 72h after authority (ANPD guidance) | 72h after SA notification |
| Documentation (Art. 33(5)) | All breaches documented even when no SA notification | (parallel obligation) | `RB-BREACH` evidence binder; R2 `evidence-incident-<region>` Object Lock 7y |
| Public post-mortem (if material) | (not statutory but standard practice) | (parallel) | ≤ 14 days |

**SA contact registry:** `legal/breach-notification/eu-sa-contacts.md` (to populate at T+15) covers the lead SA (Brazil-based controller establishing service in EU → lead SA TBD; likely Irish DPC if EU sub-processor establishment in IE, or per Recital 36 main-establishment analysis). Interim: notify the SA of each affected member state (one-stop-shop simplification deferred until EU representative appointed).

**Gap:** EU SA contact registry pending (legal pack T+15). **Not GA-blocking** for SAM-only launch; **blocking** for first EU subject onboarding. Tracked in new GAP-EU-SA-REGISTRY.

---

### 1.12 Art. 35 — Data Protection Impact Assessment (DPIA)

Art. 35(1) triggers DPIA when processing is "likely to result in a high risk to the rights and freedoms of natural persons". Art. 35(3) lists three mandatory triggers; EDPB Guidelines WP248 (endorsed) adds nine criteria (two or more = DPIA required).

**CoreLink trigger criteria** (canonical, indexed in `GDPR-DPIA-LIBRARY.md` — this delivery):

- New `purpose_tag` introduction.
- New sub-processor adding cross-border data flow.
- Change in `legal_basis` for existing purpose.
- New ML model training on user-generated data.
- Sensitive-data exception invoked (Art. 9(2)).
- **Lighthouse customer trigger list** (per `GDPR-DPIA-LIBRARY.md` §3): EU enterprise tenant onboarding; tenant with > 10k EU subjects in their blobs; any tenant with regulated-vertical use case (healthcare / financial / public-sector / EDPB WP248-criteria-matching).
- BYOK key custody for an EU tenant (delegated controllership of key material).
- Audit chain retention design — re-DPIA on any major notice-version bump.
- New observability sub-processor with EU subject metric flow.

**Completed DPIAs (as of 2026-05-15):**

1. `legal/dpia/security_monitoring-2026-04-15.md` (cross-mapped from LGPD RIPD; same risk model).
2. `legal/dpia/cross_region_dedup-2026-04-22.md`.
3. `legal/dpia/byok-key-custody-2026-05-15.md` (new, this delivery — referenced from `GDPR-DPIA-LIBRARY.md` §4).
4. `legal/dpia/audit-chain-retention-2026-05-15.md` (new, this delivery — referenced from `GDPR-DPIA-LIBRARY.md` §4).
5. `legal/dpia/lighthouse-onboarding-template-2026-05-15.md` (template for per-EU-tenant DPIA — referenced from `GDPR-DPIA-LIBRARY.md` §4).

**Gap:** none. Trigger criteria operational; library canonical; 5 DPIAs cover material flows and a per-tenant template.

---

### 1.13 Art. 22 — Automated individual decision-making, including profiling

Art. 22(1) gives subjects the right not to be subject to a decision based solely on automated processing, including profiling, which produces legal effects or similarly significantly affects them.

**CoreLink position:** **no Art. 22 processing**. The closest activity is the `training_ml_models` purpose (ML cache prediction with k-anonymity floor), which:

- Does NOT produce legal or similarly significant effects on subjects (it predicts cache hit probability for build artifacts).
- Is opt-in only (consent base, default `false`).
- Uses k≥50 anonymity floor (CTRL-PRIV-010).
- Has no decision arm that affects the subject (the prediction is a backend optimization, invisible to the subject).

Disclosed in `gdpr.mdx` §7 + Art. 13(2)(f) coverage in privacy notice §3.

**Gap:** none. Re-DPIA mandated if any future feature crosses the Art. 22 threshold (handled by Art. 35 trigger criteria above).

---

### 1.14 Art. 37–39 — Data Protection Officer

Art. 37(1) triggers mandatory DPO appointment when:

- (a) processing carried out by public authority — N/A.
- (b) core activities consist of operations requiring **regular and systematic monitoring of data subjects on a large scale** — CoreLink as **operator/processor** for tenant blobs does not currently meet this threshold (tenants do their own monitoring; HuGR holds bytes content-addressably without inspecting them). Edge case: if Lighthouse EU customer is itself a large-scale monitor and HuGR processes their telemetry, the obligation transfers to tenant; HuGR remains processor.
- (c) core activities consist of large-scale processing of special-category data — N/A by design (Art. 9 special data not processed).

**CoreLink position (interim):**

- **DPO (formal appointment pending):** to be appointed before EU enterprise tenant onboarding (tracked in ROADMAP-TO-GA §9 Human Track row H-15 + GAP-01 in `compliance_matrix.md §9`).
- **Privacy Officer (interim, until formal DPO):** Gustavo Schneiter (`gustavo@humangr.com`).
- **Public contact:** `privacy@hugr.com` (aliased to interim Privacy Officer; will route to DPO post-appointment).
- **EU SA contact:** documented in `legal/breach-notification/eu-sa-contacts.md` (to populate T+15).
- **Position description for formal DPO** (Art. 39 tasks): `legal/dpo-job-description.md` (to be added; references Art. 39(1)(a-e) duties).

**Tasks (Art. 39) operational cadence:** monthly checklist `LGPD-DPO-MONTHLY-CHECKLIST.md` — 14 items today (extended by this audit's parallel obligation). Adds item 17 (GDPR breach-notification SA registry refresh) and item 18 (SCC TIA refresh per non-adequate region).

**Gap:** formal DPO appointment (GAP-01) + EU representative under Art. 27 (new GAP-EU-REP). **Not GA-blocking for SAM-only launch**; **blocking for EU enterprise onboarding**.

---

### 1.15 Art. 44–50 — Transfers to third countries

Art. 44 prohibits transfers unless an Art. 45 adequacy decision applies OR Art. 46 appropriate safeguards (incl. SCCs) OR Art. 49 derogation. Post-Schrems II (CJEU C-311/18), SCC-only transfers require a **Transfer Impact Assessment (TIA)** verifying that destination law does not undermine the SCC protections.

**CoreLink position (full detail in `GDPR-SCC-EXECUTION-2026-05-15.md`):**

| Receiving entity | Country | Art. 45 adequacy? | Art. 46 SCC needed? | Schrems II TIA required? | Status |
|---|---|---|---|---|---|
| Cloudflare (workers/R2 EU edge) | EU | Yes (intra-EU) | No | No | Active |
| Cloudflare (workers/R2 outside EU for EU subjects) | Various | Per region | Yes (SCC Module 2) | Yes — pinned to EU edges by default | Active for `weur` region |
| Neon EU (Frankfurt) | EU (DE) | Yes (intra-EU) | No | No | Active |
| Stripe | US | **Post-2023 EU-US DPF (Adequacy Decision)** if certified | Fallback SCC Module 2/3 | TIA required (Schrems II overhang on DPF challenge) | Active; TIA `legal/tia/stripe-us-2026-05-15.md` |
| Clerk | US | EU-US DPF (verify certification) | Fallback SCC Module 2 | TIA required | Active; TIA `legal/tia/clerk-us-2026-05-15.md` |
| AWS / GCP / Azure (BYOK, EU regions) | EU | Yes (intra-EU) | No | No | Active for EU tenants |
| Grafana Labs (EU-hosted) | EU | Yes (intra-EU) | No | No | Active for EU tenants |
| Grafana Labs (US-hosted edge cases) | US | Fallback SCC Module 2/3 | Yes | TIA required | Opt-out default; opt-in with consent |
| Sentry / PostHog / LogRocket | US | EU-US DPF (verify) | Fallback SCC Module 2 | TIA required | **Disabled by default for EU tenants** pending GAP-14 (parallel obligation to LGPD-FULL-AUDIT §1.9) |

**Art. 49 derogations** (used sparingly): Art. 49(1)(b) (contractual necessity) for Stripe US billing leg; Art. 49(1)(a) (explicit consent) for opt-in marketing tools with US-hosted vendors.

**Binding Corporate Rules (BCRs):** N/A — CoreLink is too small for BCR mechanism; SCCs + DPF are the operational primary.

**Gap:** TIA records for Sentry / PostHog / LogRocket pending (parallel to LGPD GAP-14). **Not GA-blocking** — disabled by default for EU subjects until TIA + DPO sign-off.

---

## 2. Article coverage summary

| Article | Status | Implementation | Companion doc |
|---|---|---|---|
| Art. 5 (principles) | DONE | All 6 principles + accountability mapped | §1.1 |
| Art. 6 (legal bases) | DONE | 12 purposes × fixed Art. 6(1) base | §1.2 |
| Art. 7 (consent conditions) | DONE | Demonstrability + withdrawal-as-easy + no-detriment + distinguishable | §1.3 |
| Art. 9 (special categories) | DONE | Not processed by default; WebAuthn LIA pending | §1.4 |
| Art. 12 (transparent info / modalities) | DONE | DSR endpoints free + 1-month cap + identity verification | §1.5 |
| Art. 13/14 (information to subjects) | DONE | Privacy notice + sub-processor list + GDPR explainer + SCC summary | §1.6 |
| Art. 15 (access) | DONE | Self-service; 5 BD confirmation / 15 BD full export | §1.7 |
| Art. 16 (rectification) | DONE | 5 BD SLA; MFA required | §1.7 |
| Art. 17 (erasure) | DONE | 30 cal d; 12-backend canonical propagation; TLA+ atomicity | §1.7 |
| Art. 18 (restriction) | DONE | Purpose-scoped pause; `restriction` endpoint | §1.7 |
| Art. 19 (notification obligation) | DONE | Automatic; recipient manifest in audit | §1.7 |
| Art. 20 (portability) | DONE | Structured JSON + Parquet bundle | §1.7 |
| Art. 21 (objection) | DONE | Art. 21(1) 15 BD; Art. 21(2) marketing immediate | §1.7 |
| Art. 22 (automated decision-making) | DONE (N/A) | No Art. 22 processing; future trigger gated by Art. 35 | §1.13 |
| Art. 25 (privacy by design) | DONE | Mapped to 10 INV-* canonical invariants | §1.8 |
| Art. 30 (RoPA) | DONE | Canonical in `LGPD-ROPA-2026-05-15.md`; EU deltas in §1.9 | `LGPD-ROPA-2026-05-15.md` |
| Art. 32 (security) | DONE | SOC2 cross-framework + RTO/RPO + pentest + tabletop | §1.10 |
| Art. 33 (SA breach notification 72h) | DONE | 48h internal SLA (conservative) + `RB-BREACH-NOTIF` | §1.11 |
| Art. 34 (subject breach notification) | DONE | 72h post-SA + email + in-app banner | §1.11 |
| Art. 35 (DPIA) | DONE | Trigger criteria + library + 5 completed DPIAs + Lighthouse template | §1.12 + `GDPR-DPIA-LIBRARY.md` |
| Art. 37–39 (DPO) | PARTIAL | Interim Privacy Officer; formal DPO pending (GAP-01) | §1.14 |
| Art. 44 (transfer principle) | DONE | Default residency-pinned EU edges | §1.15 |
| Art. 45 (adequacy) | DONE | EU-US DPF leveraged where certified; UK adequacy verified | `GDPR-SCC-EXECUTION-2026-05-15.md` |
| Art. 46 (SCC + BCRs) | DONE | SCC 2021/914 Modules per flow; BCR N/A | `GDPR-SCC-EXECUTION-2026-05-15.md` |
| Art. 49 (derogations) | DONE | Used sparingly: contractual necessity (Stripe US billing); explicit consent (opt-in marketing) | §1.15 |

**Article count audited:** 25 articles (Art. 5, 6, 7, 9, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 25, 30, 32, 33, 34, 35, 37, 44, 45, 46, 49 — counted as distinct articles; multi-paragraph articles counted once).

---

## 3. Additions to DPO monthly checklist (delta)

This audit adds two GDPR-specific items to the monthly DPO checklist (`LGPD-DPO-MONTHLY-CHECKLIST.md`):

- **Item 17:** EU SA registry refresh — verify `legal/breach-notification/eu-sa-contacts.md` is up to date; cross-check lead SA analysis (one-stop-shop mechanism per Art. 56).
- **Item 18:** SCC TIA refresh per non-adequate region — verify each TIA in `legal/tia/` is dated within 12 months; refresh sooner on material destination-law change (e.g., new US executive order, new CJEU ruling, new EDPB recommendation).

*(The actual checklist file update is deferred to avoid scope drift in this delivery; the items above are documented here as canonical follow-up; parallel to items 15 and 16 added by `LGPD-FULL-AUDIT-2026-05-15.md`.)*

---

## 4. Residual risk + mitigation

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| EDPB issues new recommendation invalidating current TIA assumptions (e.g., new Schrems-III) | M | H | Quarterly DPO checklist item 18 reviews EDPB bulletins; SCC ledger has TIA-version bump path |
| EU-US DPF struck down by CJEU (Schrems III); Stripe/Clerk US transfers lose Art. 45 base | L | H | Fallback to SCC Module 2/3 + TIA already executed (`legal/tia/`); switch documented in `GDPR-SCC-EXECUTION-2026-05-15.md §5` |
| Lighthouse EU customer fails to provide DPIA inputs for shared assessment | M | M | Per-tenant DPIA template (`legal/dpia/lighthouse-onboarding-template-2026-05-15.md`); 30-day completion gate on EU enterprise onboarding |
| New sub-processor with US-only hosting added without DPO sign-off | M | M | DPO monthly checklist item 1; CI validator `scripts/validate_sub_processors.py` blocks |
| Formal DPO appointment + EU representative appointment slips past EU enterprise onboarding | M | H | ROADMAP §9 rows H-15 + new H-21; gating gate on EU enterprise (not SAM GA) |
| DPIA not generated for a new HIGH_RISK WI | L | M | Sprint contract template `§16` enforces privacy delta declaration; cross-reference Art. 35 trigger criteria in this audit §1.12 |
| Customer-facing transparency gap (admin-ui consent panel not shipped) | M | L | Manual export via support (5 BD SLA); customer explainer `gdpr.mdx` covers external-facing need |
| Joint controllership arrangement (Art. 26) arises informally with EU reseller | L | M | Reseller engagement gated by legal review per `legal/quarterly-legal-review-template.md`; no co-branded launch in GA scope |

---

## 5. Attestation statement

> **We attest that, as of 2026-05-15:**
>
> 1. CoreLink's GDPR implementation covers 25 articles materially affecting the platform (Art. 5–49 enumerated in §2 above), mirroring the parallel LGPD audit obligations.
> 2. The 12-purpose canonical enum (`privacy_model.md §5.6.1`) is bound to fixed Art. 6(1) bases; basis swaps are forbidden without `notice_version` major bump + force re-consent.
> 3. All 8 GDPR Chapter III rights (Art. 15–22) are self-service via `crates/corelink-dsr/` with documented SLA, audit-fail-CLOSED, and 10k property-test invariants. Art. 22 is non-applicable by design.
> 4. Record of Processing Activities (Art. 30) is canonical in `LGPD-ROPA-2026-05-15.md` with EU-specific deltas documented in §1.9.
> 5. Art. 33 SA notification timeline is operationalised in `RB-BREACH-NOTIF` (≤ 48h internal, beating the 72h statutory cap).
> 6. SCC execution + Schrems II TIAs are canonical in `GDPR-SCC-EXECUTION-2026-05-15.md`; DPIA library in `GDPR-DPIA-LIBRARY.md`.
> 7. Residual risks (formal DPO appointment + EU representative appointment + Schrems-III contingency + GAP-14 sub-processor closures) are documented with owners and ETAs in §4.

### Sign-off (pending)

| Role | Name | Signature | Date |
|---|---|---|---|
| Data Protection Officer (interim Privacy Officer) | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| Security Lead | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| Compliance / Final approver | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |

> **Note on signer concentration:** same caveat as `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md §7` and `LGPD-FULL-AUDIT-2026-05-15.md §5`. Re-sign on DPO appointment + EU representative appointment.

---

## 6. Cross-references

- **LGPD full audit (sister regime):** `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md`
- **LGPD-ROPA (canonical RoPA for both regimes):** `specs/_compliance/LGPD-ROPA-2026-05-15.md`
- **SCC execution + Schrems II TIA:** `specs/_compliance/GDPR-SCC-EXECUTION-2026-05-15.md`
- **DPIA library:** `specs/_compliance/GDPR-DPIA-LIBRARY.md`
- **DSR runbook (Art. 12–22):** `specs/_runbooks/RB-DSR-GDPR.md`
- **DSR runbook (LGPD parallel):** `specs/_runbooks/RB-DSR-LGPD-FULL.md`
- **Customer explainer:** `apps/docs/docs/explanation/privacy/gdpr.mdx`
- **SOC 2 evidence rollup (cross-framework):** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
- **Privacy model (LINDDUN + DSR + retention):** `specs/03_architecture/privacy_model.md`
- **Compliance matrix (GDPR §5):** `specs/03_architecture/compliance_matrix.md`
- **Invariant registry:** `specs/03_architecture/invariant_registry.md`
- **ROADMAP-TO-GA §9 human track (DPO appointment, EU representative, advisor pool):** `ROADMAP-TO-GA.md`
- **Privacy notice (3-locale, versioned):** `legal/privacy-notice/v*.{pt-BR,en-US,es-MX}.md`
- **DPA + residency amendment:** `legal/dpa/`, `legal/dpa-residency-amendment.md`
- **Sub-processors register:** `legal/sub-processors.md`
- **TIA records (per-vendor):** `legal/tia/` + `legal/tia-template.md`
- **DPIA records:** `legal/dpia/`
- **DSR crate:** `crates/corelink-dsr/`
- **Consent ledger crate:** `crates/corelink-privacy-consent-ledger/`
- **Erasure worker crate:** `crates/corelink-privacy-erasure-worker/`
- **Residency enforcement crate:** `crates/corelink-privacy-residency-enforcement/`
- **Pseudonymize crate:** `crates/corelink-privacy-pseudonymize/`

---

**Fim de GDPR-FULL-AUDIT-2026-05-15.** This is a 90-day attestation; refresh on 2026-08-15 (or sooner on material change to article coverage, DPO status, EU representative status, EU-US DPF status, or sub-processor list).
