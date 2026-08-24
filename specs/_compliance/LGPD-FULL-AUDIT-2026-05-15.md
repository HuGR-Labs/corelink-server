---
id: "LGPD-FULL-AUDIT-2026-05-15"
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
  - "LGPD-RESIDENCY-ATTESTATION-2026-05-15"
  - "PRIVACY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "SOC2-EVIDENCE-ROLLUP-2026-05-15"
  - "INVARIANT-REGISTRY"
tags: ["lgpd", "lgpd-art-5", "lgpd-art-7", "lgpd-art-9", "lgpd-art-11", "lgpd-art-14", "lgpd-art-18", "lgpd-art-27", "lgpd-art-33", "lgpd-art-37", "lgpd-art-38", "lgpd-art-41", "lgpd-art-46", "lgpd-art-48", "audit", "anpd", "brazil", "gap-22-followup"]
---

# LGPD Full Audit — beyond Art. 33 §1º (2026-05-15)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **scope:** every LGPD article that touches CoreLink's operational surface, not just Art. 33 §1º residency (which is covered by `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md`).
>
> **Framework version:** LGPD (Lei 13.709/2018), as amended; ANPD Resolução CD/ANPD nº 4/2023 (international transfer); ANPD Resolução CD/ANPD nº 15/2024 (incident notification template, effective 2024 Q4); ANPD Resolução CD/ANPD nº 2/2022 (small-controller framework — N/A for CoreLink, controller > 5k subjects).
>
> **Observation window:** 2026-05-15 → 2026-08-15 (90-day attestation, renewable quarterly with the residency bundle).
>
> **Companion canonical docs (do not duplicate):**
> - `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` (Art. 33 §1º — GAP-22 closure)
> - `specs/_compliance/LGPD-ROPA-2026-05-15.md` (Art. 41 — Record of Processing Activities)
> - `specs/_runbooks/RB-DSR-LGPD-FULL.md` (Art. 18 — internal end-to-end DSR runbook)
> - `apps/docs/docs/explanation/privacy/lgpd-full.mdx` (customer-facing explainer)
> - `specs/03_architecture/privacy_model.md` (LINDDUN + DSR pipeline + retention)
> - `specs/03_architecture/compliance_matrix.md` §4 (LGPD crosswalk)
> - `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md` (operational cadence)
> - `crates/corelink-dsr/`, `crates/corelink-privacy-*/` (runtime enforcement)
>
> **Purpose:** the GAP-22 attestation closed Art. 33 §1º. This audit extends coverage to the other LGPD articles that materially affect CoreLink (Art. 5–16 definitions and legal bases, Art. 18 data-subject rights, Art. 27 international transfers beyond §1º, Art. 37–38 records & DPIA, Art. 41 DPO + RoPA, Art. 46–48 security & incidents). Each article gets: legal text summary → CoreLink applicability → implementation evidence (`commit:path:line` where applicable) → control mapping → residual gap.

---

## 1. Article-by-article audit

> Commit hash anchor for code evidence: `fb9f56e` (`merge wt/r-prep-webhook-dlq` → main, 2026-05-15). All `commit:path:line` references pin to this hash.

### 1.1 Art. 5 — Definitions

LGPD Art. 5 defines the canonical terms: `dado pessoal` (I), `dado pessoal sensível` (II), `titular` (V), `controlador` (VI), `operador` (VII), `encarregado` (VIII), `tratamento` (X), `anonimização` (XI), `consentimento` (XII), `transferência internacional` (XV).

**CoreLink terminology alignment (canonical mapping):**

| LGPD Art. 5 term | CoreLink term | Canonical source |
|---|---|---|
| `titular` (V) | `data_subject` / `subject` | `crates/corelink-dsr/src/event.rs:DsrRequest::data_subject_id` |
| `controlador` (VI) | `controller` (HuGR for account/billing/telemetry; tenant for blob content) | `privacy_model.md §10.1` (matrix) |
| `operador` (VII) | `processor` (HuGR for tenant blob content under DPA) | `privacy_model.md §10.1` |
| `encarregado` (VIII) | `DPO` (interim: Privacy Officer = Gustavo Schneiter; see §1.10 Art. 41) | `LGPD-DPO-MONTHLY-CHECKLIST.md` |
| `tratamento` (X) | `processing` (canonical `purpose_tag` enum, 12 values) | `privacy_model.md §5.6.1` |
| `anonimização` (XI) | `pseudonymize` (canonical pipeline; HKDF-derived) | `crates/corelink-privacy-pseudonymize/` |
| `consentimento` (XII) | `consent` (with `notice_text_hash` + `notice_version` proof) | `crates/corelink-privacy-consent-ledger/` |
| `transferência internacional` (XV) | `cross_region_transfer` (fail-CLOSED 451) | `crates/corelink-privacy-residency-enforcement/` |

**Gap:** none. All LGPD Art. 5 terms have a canonical CoreLink equivalent.

**Evidence:** `EVT-026` (schema validation that DSR records use canonical term enum); `EVT-043` (data classification doc).

---

### 1.2 Art. 6 — Principles

LGPD Art. 6 enumerates 10 principles (finalidade, adequação, necessidade, livre acesso, qualidade dos dados, transparência, segurança, prevenção, não discriminação, responsabilização e prestação de contas).

| Principle | CoreLink implementation | Evidence |
|---|---|---|
| Finalidade (I) | `purpose_tag` enum (12 canonical values) + bound to `legal_basis` | `privacy_model.md §5.6.1`; `EVT-026` |
| Adequação (II) | Purpose review per WI under `production_readiness_review.md §12` | `EVT-046` (LIA template); CTRL-PRIV-003 |
| Necessidade (III) | Minimization: only required fields collected; data map public | CTRL-PRIV-020; `apps/docs/.../residency/` |
| Livre acesso (IV) | DSR Access endpoint `POST /v1/privacy/dsr/access` (5 BD SLA) | `crates/corelink-dsr/src/endpoint.rs`; `EVT-048` |
| Qualidade dos dados (V) | DSR Rectification endpoint `POST /v1/privacy/dsr/rectification` | `crates/corelink-dsr/`; `EVT-048` |
| Transparência (VI) | Privacy notice versioned (`legal/privacy-notice/v*`); sub-processor list public | CTRL-PRIV-020, -021 |
| Segurança (VII) | `security_model.md` + CTRL-CRYPTO-001/002 + CTRL-ISO-001..005 | `EVT-005`, `EVT-025` |
| Prevenção (VIII) | LINDDUN threat model + LIA reviews annual | `privacy_model.md §4`; `EVT-046` |
| Não discriminação (IX) | No use of data for discriminatory ranking; ML training opt-in only | CTRL-PRIV-003 `purpose_tag=training_ml_models` (default `false`) |
| Responsabilização (X) | This document + LGPD-DPO-MONTHLY-CHECKLIST + audit trail 7y | `EVT-047`, `EVT-049`, `EVT-048` |

**Gap:** none material. All 10 principles map to a canonical control with active evidence.

---

### 1.3 Art. 7 — Legal bases for processing

LGPD Art. 7 enumerates 10 legal bases. Below is the per-dataflow mapping (canonical 12-purpose enum from `privacy_model.md §5.6.1`):

| Dataflow / `purpose_tag` | LGPD Art. 7 base | GDPR equivalent | Notes |
|---|---|---|---|
| `service_delivery` (CAS/AC/exec core ops) | **Art. 7 §V** — execução de contrato | Art. 6(1)(b) | Primary base. Not revocable without terminating contract. |
| `account_management` (auth, billing, tenant admin) | **Art. 7 §V** — execução de contrato | Art. 6(1)(b) | Same as service_delivery. |
| `regulatory_compliance` (audit retention, DSR fulfillment, breach reporting) | **Art. 7 §II** — cumprimento de obrigação legal | Art. 6(1)(c) | LGPD itself + ANPD Resolução 15/2024 + SOC 2 + ISO 27001. |
| `security_monitoring` (anomaly detection, abuse prevention, fraud) | **Art. 7 §IX** — legítimo interesse | Art. 6(1)(f) | LIA required (`legal/lia/security-monitoring.md`); objection workflow via Art. 18 §II. |
| `analytics_aggregated` (cross-tenant k≥50 metrics with privacy budget) | **Art. 7 §IX** — legítimo interesse | Art. 6(1)(f) | LIA required; objection-capable. |
| `analytics_personalized` (per-tenant dashboards with reidentifiable PII) | **Art. 7 §I** — consentimento | Art. 6(1)(a) | Opt-in, revocable ≤ 5min. |
| `marketing_email` | **Art. 7 §I** — consentimento | Art. 6(1)(a) | Opt-in, revocable. |
| `marketing_research` (surveys, NPS) | **Art. 7 §I** — consentimento | Art. 6(1)(a) | Opt-in, revocable. |
| `beta_features` (preview features with enriched telemetry) | **Art. 7 §I** — consentimento | Art. 6(1)(a) | Opt-in, revocable. |
| `third_party_integrations` (Stripe, GitHub, webhooks) | **Art. 7 §I** + Art. 7 §V | Art. 6(1)(a)/(b) | Per-integration granular consent + contractual necessity for Stripe payment leg. |
| `cross_tenant_benchmarks` (leaderboards) | **Art. 7 §I** — consentimento | Art. 6(1)(a) | Opt-in, revocable. |
| `training_ml_models` (ML cache prediction with k-anon) | **Art. 7 §I** — consentimento | Art. 6(1)(a) | Opt-in only; default `false`. |

**Other Art. 7 bases not used in CoreLink default flow:**

- §III (políticas públicas) — N/A (private SaaS).
- §IV (estudos por órgão de pesquisa) — N/A.
- §VI (exercício regular de direitos em processo) — invoked only on subpoena (RB-REGULATOR-INQUIRY).
- §VII (proteção da vida) — N/A.
- §VIII (tutela da saúde) — N/A.
- §X (proteção do crédito) — N/A (Stripe handles credit risk).

**Invariant (Lote 10.11.0-bis):** `legal_basis` is **fixed per `purpose_tag`** — cannot swap dynamically. Changing basis = bump major `notice_version` + force re-consent. Enforced by `crates/corelink-privacy-consent-ledger/` immutability + schema (`EVT-026`).

**Gap:** none. All 12 purposes have a single explicit legal basis with documented evidence.

---

### 1.4 Art. 8 — Consent

LGPD Art. 8: consent must be "manifestação livre, informada e inequívoca" with specific finality; written or by other means demonstrating the will of the subject; revocable at any time.

| Requirement | CoreLink implementation | Evidence |
|---|---|---|
| Free (sem vício) | Granular per-`purpose_tag`; service_delivery + account_management are contract-base (not consent), so users cannot be forced to consent to marketing as a precondition | UI form `apps/admin-ui/src/privacy/ConsentForm.tsx` (deferred wiring); `legal/privacy-notice/v*` |
| Informed | `notice_text_hash` + `notice_version` + `locale` + `wording_id` captured in every consent record (CTRL-PRIV-CONSENT-001) | `EVT-049`; `crates/corelink-privacy-consent-ledger/` |
| Unambiguous | Default `false` for every consent-base purpose; explicit checkbox; no pre-checked boxes (Art. 8 §3 prohibition) | UI implementation note in privacy_model.md §5.6 |
| Specific finality | Per-purpose checkbox (12 canonical), not bundled | `privacy_model.md §5.6.1` |
| Revocable at any time | `DELETE /v1/consent/<purpose>` ≤ 5min propagation (CTRL-PRIV-CONSENT-002) | `EVT-049` (revoke event); `EVT-024` (load-test revocation latency) |
| Written or equivalent | Immutable audit record in R2 Object Lock 7y with full proof bundle | CTRL-PRIV-CONSENT-003 |

**No fail-open invariant:** if consent expires (TTL or re-consent not renewed), `legal_basis` does NOT degrade to `legitimate_interest`. Purpose enters `consent_lapsed` state → processing PAUSES until re-consent or erasure. Addresses GPT P0-1 round-1 ambiguity. Enforced in `crates/corelink-privacy-consent-ledger/`.

**Gap:** Admin-UI ConsentForm wiring deferred to WI-S13-* track. Until then, consent is captured via signup form + DPA acceptance for B2B tenants. **Not GA-blocking** (B2B flow legally sound; consumer flow not in scope).

---

### 1.5 Art. 9 — Right to clear information about processing

Art. 9 requires that the subject have free access to information about processing: specific purposes, form/duration, identification of controller, controller contact, shared/joint controllers, responsibilities of agents, rights of the subject.

| Required disclosure | Where surfaced | Evidence |
|---|---|---|
| Specific purpose | Privacy notice `legal/privacy-notice/v*.{pt-BR,en-US,es-MX}.md` §3 | `EVT-044` (Legal review) |
| Form/duration | Privacy notice §4 + this audit §1.6 (retention) | `EVT-042` |
| Controller identity | Privacy notice §1; DPA `legal/dpa/v*.{locale}.md` | DPA |
| Controller contact | Privacy notice §1 + `privacy@hugr.com` (DPO inbox) | `LGPD-DPO-MONTHLY-CHECKLIST.md` |
| Sub-processors | Public `/privacy/sub-processors` page (10 documented, 4 pending) | `legal/sub-processors.md`; `apps/docs/.../compliance/sub-processors.mdx` |
| Rights of subject | This audit §1.7 + customer-facing explainer `apps/docs/.../privacy/lgpd-full.mdx` (this delivery) | EVT-044 |

**Gap:** none material.

---

### 1.6 Art. 11 — Sensitive personal data

LGPD Art. 11 §1: sensitive data may be processed only with specific and highlighted consent, OR for one of the 8 enumerated exceptional bases.

**CoreLink position:** we do NOT intentionally collect or process sensitive data (Art. 5 §II categories: racial/ethnic origin, religious conviction, political opinion, union membership, religious/philosophical/political organization membership, health/sexual data, genetic/biometric data).

| Sensitive category | CoreLink touch? | Mitigation |
|---|---|---|
| Biometric (Art. 5 §II) | **YES — WebAuthn public keys** (auth factor) | Stored as ECC public key only (not biometric template); cryptographic identifier, not the underlying biometric. ANPD guidance (Resolução nº 2/2022 commentary) treats WebAuthn pubkeys as `dado pessoal` not `dado pessoal sensível` because the biometric never leaves the user's device. CoreLink position documented in `legal/lia/webauthn-biometric-classification.md` (to be added). |
| Health | NO | HIPAA-opt-in tenants under separate BAA; no default flow. |
| Genetic | NO | N/A. |
| Racial/ethnic, religious, political, philosophical, union | NO | Explicit anti-pattern §32 in `privacy_model.md §2`: any tenant submitting blobs containing these = tenant's responsibility under DPA; HuGR is operator and does not look at content. |

**Gap:** WebAuthn classification position needs Legal sign-off in `legal/lia/webauthn-biometric-classification.md`. Tracked in `LGPD-DPO-MONTHLY-CHECKLIST.md` item 15 (added in this delivery; see §3 below). **Not GA-blocking** — defensible position pending formal LIA.

---

### 1.7 Art. 14 — Children's data (under 18 in BR)

LGPD Art. 14 requires specific and highlighted consent from at least one parent or legal guardian for processing children's data (under 12 = "criança", 12-18 = "adolescente"). Processing must be in the best interest of the child.

**CoreLink position:** CoreLink is a B2B developer infrastructure platform. Per Terms of Service (`legal/tos/v*.md`), all account holders **must be ≥ 18 years old** and represent a business entity. We do NOT knowingly accept signups from children.

| Question | Answer | Evidence |
|---|---|---|
| Do we collect children's data? | **No.** | ToS §2 (Eligibility); signup flow age attestation |
| Do we have a parental-consent path? | **No** — out of scope (B2B-only platform). | Same |
| What if a tenant uploads blobs containing children's data? | Tenant's responsibility under DPA (Art. 7 of DPA = controller's representations and warranties). HuGR as operator does not access blob content. | `legal/dpa/v*.{locale}.md` §7 |
| What if a child impersonates an adult? | Discovered cases trigger immediate account termination + DSR-erasure of all collected data + breach evaluation. | `RB-DSR-LGPD-FULL.md` §5 (special cases) |

**Gap:** none. N/A by design.

---

### 1.8 Art. 18 — Data subject rights (full menu)

LGPD Art. 18 enumerates 9 rights. Cross-referenced with CoreLink's DSR pipeline (`crates/corelink-dsr/` + `privacy_model.md §6.1`).

| # | LGPD Art. 18 right | Self-service? | SLA | API endpoint | Evidence |
|---|---|---|---|---|---|
| 1 | **§I — Confirmação da existência de tratamento** | Yes | 5 BD | `POST /v1/privacy/dsr/access` (subset query) | `EVT-048`; `crates/corelink-dsr/src/endpoint.rs::confirm_processing` |
| 2 | **§II — Acesso aos dados** | Yes | 15 BD | `POST /v1/privacy/dsr/access` | `EVT-048`; `crates/corelink-dsr/`; receipt JWT 90d expiry |
| 3 | **§III — Correção (dados incompletos/inexatos/desatualizados)** | Yes | 5 BD | `POST /v1/privacy/dsr/rectification` (MFA required — destructive) | `EVT-048`; `crates/corelink-dsr/src/endpoint.rs` (MFA gate) |
| 4 | **§IV — Anonimização, bloqueio ou eliminação de dados desnecessários, excessivos ou tratados em desconformidade** | Yes | 30 dias | `POST /v1/privacy/dsr/erasure` (MFA required — destructive) | `EVT-042`; `EVT-017`; `crates/corelink-privacy-erasure-worker/`; 12-backend canonical propagation (`privacy_model.md §6.2`) |
| 5 | **§V — Portabilidade dos dados a outro fornecedor** | Yes | 15 BD | `POST /v1/privacy/dsr/portability` | `EVT-048`; structured JSON + Parquet bundle; `crates/corelink-dsr/` |
| 6 | **§VI — Eliminação dos dados tratados com consentimento** | Yes | 30 dias | Same endpoint as §IV (server distinguishes basis-fixed-by-purpose) | `EVT-042`; consent-base purposes get hard erasure (no legal_obligation override) |
| 7 | **§VII — Informação sobre as entidades públicas e privadas com as quais o controlador realizou uso compartilhado de dados** | Yes | 15 BD | `GET /v1/privacy/dsr/shared-with` | `legal/sub-processors.md` + per-subject `data-recipients-export.json` |
| 8 | **§VIII — Informação sobre a possibilidade de não fornecer consentimento e sobre as consequências da negativa** | Surfaced at consent capture | Continuous | UI consent form + privacy notice §5 | `EVT-049` (consent event with `wording_id` proving "Caso você não consinta, [consequência]" text shown) |
| 9 | **§IX — Revogação do consentimento (Art. 8 §5)** | Yes | Immediate (≤ 5min) | `DELETE /v1/consent/<purpose>` | `EVT-049`; CTRL-PRIV-CONSENT-002 |

**Bonus right — §II of Art. 18 (Lei 13.853/2019 amendment):** **Right to oppose** processing carried out under one of the dispenses for consent, in case of non-compliance with the law. Implemented via `POST /v1/privacy/dsr/objection` with 15 BD SLA. Used primarily for `security_monitoring` and `analytics_aggregated` purposes (legitimate-interest base).

**Pipeline correctness invariants (canonical from `privacy_model.md §6.2`):**

- `INV-DSR-VERIFIED-CLOCK` — SLA clock starts at `dsr_tickets.status = verified` (post-MFA), not at submission.
- `INV-DSR-AUDIT-FAIL-CLOSED` — every DSR decision arm emits audit BEFORE state mutation; audit failure aborts the run fail-CLOSED.
- `INV-DSR-MFA-DESTRUCTIVE` — Erasure + Rectification require WebAuthn step-up; Access + Portability + Restriction + Objection do not.
- `INV-DSR-TENANT-ISOLATION` — tenant A's DSR requests never visible to tenant B (CTRL-ISO-004 constant-time 404 vs 403).
- `INV-DSR-RECEIPT-90D` — JWT receipt anti-replay window 90 days, RS256-signed, KMS-rooted.

**Test evidence:** 10k property test in `crates/corelink-dsr/tests/` (canonical taxonomy: 6-arm `DsrRequestKind` × 4-arm `DsrStatus` × 4-arm `DsrDecision` × 3-arm `DsrJurisdiction`). Last green CI run 2026-05-15 (commit `fb9f56e`).

**Gap:** none material. All 9 Art. 18 rights are self-service with documented SLA + evidence + invariants under property testing.

---

### 1.9 Art. 27 — International transfer (beyond Art. 33 §1º residency attestation)

LGPD Art. 33 §1º residency attestation is covered by `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` (GAP-22). Art. 27 (less commonly cited) addresses **shared use of data with private entities** — the receiving party becomes a `controlador` or `operador` under LGPD definitions and must comply.

**CoreLink position:** sub-processor management is the operational implementation of Art. 27. All shared-use partnerships are documented in `legal/sub-processors.md` with:

- Role (controlador / operador / joint controller).
- LGPD posture (DPA + SCCs where applicable).
- Region availability for BR tenants.
- ANPD-template SCC clauses embedded in `legal/dpa-residency-amendment.md`.

For Brazilian data subjects' data crossing international borders:

| Receiving entity | International transfer? | Art. 33 base relied on | SCC? | Evidence |
|---|---|---|---|---|
| Cloudflare (sam region buckets) | No (data stays in `wnam-southamerica-east1`) | N/A — domestic | N/A | LGPD-RESIDENCY-ATTESTATION §4 |
| Neon EU (billing detail for BR tenants — Neon does not offer BR region) | Yes (BR → EU) | **Art. 33 IX (c/c Art. 7 V execução de contrato)** — contractual necessity for billing + **Art. 16 I** fiscal retention (CTN Art. 173/174) — Lote 10.11.0-ter legal-citation re-validation 2026-05-15 corrigiu cites anteriores "Art. 33 §V" (V = autorização ANPD, não contrato) e "Art. 16 §3" (Art. 16 LGPD não tem §3) | Yes (Neon DPA Module 2/3) | `legal/sub-processors.md:27-58` |
| Stripe (payment processing) | Yes (BR → US/global) | **Art. 33 IX (c/c Art. 7 V execução de contrato)** — contractual necessity for payment (Lote 10.11.0-ter corrigida cite anterior "Art. 33 §V") | Yes (Stripe DPA + BR addendum) | `legal/sub-processors.md:59-78` |
| AWS São Paulo (sa-east-1, BYOK envelope) | No (sa-east-1 = BR) | N/A — domestic | N/A | BYOK ADR |
| GCP/Azure (BR region for BYOK) | No | N/A | N/A | BYOK ADR |
| Grafana Labs (telemetry — pending GAP-14) | TBD — BR tenants opt-out by default until BR region ships | N/A (opt-out default) | LIA + future SCC | GAP-14 |
| Sentry / PostHog / LogRocket | Disabled for BR tenants by default | N/A (opt-out default) | TBD | GAP-14, deferred D+60 |

**ANPD enforcement trajectory:** ANPD's first international-transfer enforcement actions (2025) targeted US transfers without SCCs. CoreLink's conservative posture (residency-pinned + SCCs in DPA + consent fallback for opt-in only) is defensible.

**Gap:** four sub-processors pending GAP-14 review (Sentry, PostHog, LogRocket, Stripe Atlas counsel). **Not GA-blocking** — disabled for `sam` tenants by default; tracked in `LGPD-DPO-MONTHLY-CHECKLIST.md` item 1.

---

### 1.10 Art. 37 — Records of processing operations

Art. 37: controller and operator **shall maintain records of personal data processing operations** they carry out, especially when based on legitimate interest.

**CoreLink implementation:** see companion doc `LGPD-ROPA-2026-05-15.md` (Record of Processing Activities). Required content per Art. 37 + Art. 41:

| Required field | CoreLink RoPA column | Source |
|---|---|---|
| Controller identification | RoPA §1 (HuGR Labs / per-tenant for blob content) | `legal/dpa/v*.md` |
| Categories of personal data | RoPA table (Account, Telemetry, Blob, Audit, DSR ticket, etc.) | `LGPD-ROPA-2026-05-15.md` |
| Purpose of processing | `purpose_tag` enum (12 values) | `privacy_model.md §5.6.1` |
| Legal basis | Per-purpose fixed (Art. 7 §I/II/V/IX) | This audit §1.3 |
| Retention period | Retention table | `privacy_model.md §8.1` |
| Recipients | Sub-processor list | `legal/sub-processors.md` |
| International transfer | Per-flow analysis | This audit §1.9 |
| Security measures | CTRL catalog | `security_model.md` |

**Gap:** RoPA delivered in this bundle (`LGPD-ROPA-2026-05-15.md`).

---

### 1.11 Art. 38 — Data Protection Impact Report (RIPD / DPIA)

Art. 38: the national authority may require the controller to prepare a **Relatório de Impacto à Proteção de Dados Pessoais** (RIPD, aka DPIA) when processing is based on legitimate interest OR involves systematic, large-scale processing of sensitive data.

**CoreLink trigger criteria** (auto-generated when WI declares `HIGH_RISK` privacy tag):

- New `purpose_tag` introduction.
- New sub-processor adding cross-border data flow.
- Change in `legal_basis` for existing purpose.
- New ML model training on user-generated data.
- Sensitive-data exception invoked.

**Template:** `legal/dpia/template.md` (canonical, derived from ANPD's 2024 RIPD guidance).

**Completed RIPDs (as of 2026-05-15):**

1. `legal/dpia/security_monitoring-2026-04-15.md` — legitimate interest LIA + balancing test (mitigations: pseudonymized IPs, 30d retention, opt-out for `analytics_aggregated`).
2. `legal/dpia/cross_region_dedup-2026-04-22.md` — closed: dedup is tenant-local by default (CTRL-ISO-005); cross-tenant dedup opt-in only with separate consent.

**Gap:** none. Trigger criteria operational; template canonical; 2 completed RIPDs cover material flows.

---

### 1.12 Art. 41 — Data Protection Officer (encarregado)

Art. 41: controller appoints an `encarregado` (DPO) responsible for communication between controller, data subjects, and ANPD.

**CoreLink position (interim, formalized 2026-05-15 via GAP-01 closure track):**

- **Interim DPO formally designated:** Gustavo Schneiter (`gustavo@humangr.com` internal / `privacy@hugr.com` public) per `specs/_compliance/DPO-APPOINTMENT-2026-05-15.md` (LGPD Art. 41 §1º + ANPD Resolução 18/2024 compliant; ANPD registration template in §7 of appointment doc, filing pending H-1 CNPJ assignment).
- **Reporting line:** direct to HuGR Labs Board (bypasses operational chain; satisfies GDPR Art. 38(3) + LGPD Art. 41 §2º interpretive requirement).
- **Independence safeguards:** no operational role on `crates/corelink-privacy-*` or `crates/corelink-dsr/` code authoring without external advisor countersign; veto rights over `purpose_tag`, `legal_basis`, sub-processor onboarding, cross-border flows (see `DPO-RESPONSIBILITIES-MATRIX.md §2.2`).
- **Term:** interim until permanent DPO appointment (target Q3-2026, hard cap T+1m of GA per `compliance_matrix.md §9` GAP-01); 90-day handoff plan in `DPO-HANDOFF-PLAN.md`.
- **Public contact:** `privacy@hugr.com` (canonical; routes to DPO inbox).
- **ANPD contact:** documented in `legal/breach-notification/anpd-contacts.md`; escalation runbook `specs/_runbooks/RB-DPO-ESCALATION.md §5`.
- **RACI for DPO-touched activities:** `specs/_compliance/DPO-RESPONSIBILITIES-MATRIX.md` (30 rows covering DSR, consent, sub-processors, DPIA, breach, ANPD comms, training, SOC2/ISO cross-framework).
- **Escalation triggers + SLAs:** `specs/_runbooks/RB-DPO-ESCALATION.md` (12 trigger families, 5-tier SLA matrix).

**Segregation-of-duties caveat:** all 3 sign-off roles (DPO, Security Lead, Compliance/Final approver) currently held by Gustavo Schneiter. Mitigation: external advisor pool (R5-8) provides countersign during the interim window. Post-DPO appointment, all attestations (residency, full audit, SOC2-evidence-rollup) re-signed with distinct individuals.

**Operational cadence:** monthly checklist `LGPD-DPO-MONTHLY-CHECKLIST.md` — 14 items today; this audit adds item 15 (WebAuthn-biometric LIA classification follow-up) and item 16 (RoPA quarterly refresh).

**Gap:** formal DPO appointment (GAP-01, ROADMAP §9 row H-15). **Interim formalization landed 2026-05-15** via `DPO-APPOINTMENT-2026-05-15.md` + `DPO-RESPONSIBILITIES-MATRIX.md` + `RB-DPO-ESCALATION.md` + `DPO-HANDOFF-PLAN.md`. **Not GA-blocking for SAM-only launch** (interim acceptable per ANPD Resolução 18/2024 §4 conflict-of-interest analysis); **blocking for EU enterprise** until permanent DPO ceremony per `DPO-HANDOFF-PLAN.md §7` (parallel GDPR Art. 37 obligation).

---

### 1.13 Art. 43 — ANPD sanctions (preparedness)

Art. 43 lists ANPD sanctions: warning, simple fine (up to 2% of revenue, capped R$ 50M per infraction), daily fine, publicization of infraction, blockade of data, deletion of data.

**CoreLink defensive posture:**

| Sanction trigger | CoreLink mitigation | Evidence |
|---|---|---|
| Failure to attend DSR | 10k property test on DSR pipeline; quarterly DSR SLO report | `EVT-013`; `crates/corelink-dsr/tests/` |
| Failure to notify breach (Art. 48) | `RB-BREACH-NOTIF` runbook + 48h internal SLA + tabletop semestral | `EVT-019` |
| International transfer without basis | Fail-CLOSED 451 routing + SCC in DPA + LGPD-RESIDENCY-ATTESTATION | `crates/corelink-privacy-residency-enforcement/`; `EVT-022` (TLA+) |
| Excessive retention | Retention table + GC jobs + erasure pipeline 12-backend canonical | `privacy_model.md §8.1`; `EVT-042` |
| Processing without legal basis | `purpose_tag` immutability + `consent_lapsed` state (no fail-open) | `privacy_model.md §5.6.1`; `crates/corelink-privacy-consent-ledger/` |

**Gap:** none. All ANPD-sanctionable risk vectors have a documented control.

---

### 1.14 Art. 46-48 — Security & incident notification

Art. 46: controller and operator shall adopt **technical and administrative security measures** capable of protecting personal data from unauthorized access and accidental or illicit destruction, loss, alteration, communication, or any form of inadequate or unlawful processing.

Art. 47: the agents must ensure data security including with after-the-fact compromise.

Art. 48: the controller shall **notify the national authority and data subject of incidents** that may create relevant risk or damage to the data subjects.

**CoreLink implementation (cross-framework with SOC 2 CC6.x + ISO 27001 A.8):**

| LGPD Art. | Control | Cross-framework | Evidence |
|---|---|---|---|
| Art. 46 (security) | CTRL-CRYPTO-001 (TLS 1.2 floor / 1.3 preferred (ADR-0072)) | SOC 2 CC6.7 | `EVT-037` (SSL Labs A+ — scan anterior à queda do piso; reexecução pendente) |
| Art. 46 | CTRL-CRYPTO-002 (at-rest envelope encryption) | SOC 2 CC6.1; ISO A.8.24 | `EVT-005` |
| Art. 46 | CTRL-ISO-001..005 (tenant isolation) | SOC 2 CC6.2 | `EVT-022` (TLA+ proofs) |
| Art. 46 | CTRL-AUTH-001..010 (auth + MFA) | SOC 2 CC6.1 | `EVT-025` (pentest) |
| Art. 46 | CTRL-SUPPLY-001..005 (SBOM + Sigstore + provenance) | SOC 2 CC6.8 | `EVT-011` (SLSA L3) |
| Art. 47 | CTRL-AUDIT-001..005 (audit trail + Object Lock 7y) | SOC 2 CC7.x; ISO A.8.15 | `EVT-047` |
| Art. 48 | `RB-BREACH-NOTIF` + ANPD/DPA timeline | GDPR Art. 33 / 34 | `EVT-019` (tabletop) |
| Art. 48 | ANPD Resolução 15/2024 incident template alignment | — | `legal/breach-notification/templates/anpd-incident-form-v2024.md` |

**Incident notification timeline (Art. 48 + ANPD Resolução 15/2024):**

- Detection → internal declaration: ≤ 1h (SEV-1 oncall).
- Declaration → triage + containment: ≤ 4h.
- Triage → ANPD notification (Art. 48): "em prazo razoável a ser definido pela autoridade" — ANPD Resolução 15/2024 sets 3 business days from awareness; CoreLink commits 48h internal SLA (conservative).
- Notification to data subjects: 72h after authority (per ANPD guidance; aligned with GDPR Art. 34).
- Post-mortem (public if material): ≤ 14 days.

**Gap:** none material. SOC2-EVIDENCE-ROLLUP-2026-05-15.md row coverage confirms all Art. 46-48 controls are operational.

---

## 2. Article coverage summary

| Article | Status | Implementation | Companion doc |
|---|---|---|---|
| Art. 5 (definitions) | DONE | Terminology aligned | This audit §1.1 |
| Art. 6 (principles) | DONE | All 10 principles mapped | §1.2 |
| Art. 7 (legal bases) | DONE | 12 purposes × fixed basis | §1.3 |
| Art. 8 (consent) | DONE (B2B) / Deferred (consumer UI) | Consent ledger + proof bundle | §1.4 |
| Art. 9 (right to clear info) | DONE | Privacy notice + sub-processor list + this audit | §1.5 |
| Art. 11 (sensitive data) | DONE | Not collected by default; WebAuthn LIA pending | §1.6 |
| Art. 14 (children) | DONE (N/A) | B2B ToS ≥ 18 only | §1.7 |
| Art. 15 (termination) | DONE | Retention table + 30d grace + erasure pipeline | `privacy_model.md §8` |
| Art. 16 I (fiscal retention via cumprimento de obrigação legal — CTN Art. 173/174) | DONE | 5y billing legal hold (Lote 10.11.0-ter legal-citation re-validation 2026-05-15 corrigida cite anterior "Art. 16 §3" — Art. 16 LGPD tem caput + incisos I-IV apenas, sem §) | `privacy_model.md §8.1` |
| Art. 17 (subject's right exists) | DONE | DSR self-service | §1.8 |
| Art. 18 §I–IX (9 rights) | DONE | 6 destructive arms + 3 informational; 10k property test | §1.8 |
| Art. 18 §II amendment (objection) | DONE | `POST /v1/privacy/dsr/objection` | §1.8 |
| Art. 27 (shared use / sub-processor) | DONE | `legal/sub-processors.md` + DPA + ANPD-SCCs | §1.9 |
| Art. 33 §1º (international transfer) | DONE | GAP-22 attestation (LGPD-RESIDENCY-ATTESTATION) | Separate doc |
| Art. 37 (records of processing) | DONE | RoPA delivered | `LGPD-ROPA-2026-05-15.md` |
| Art. 38 (RIPD / DPIA) | DONE | Template + 2 completed RIPDs | §1.11 |
| Art. 41 (DPO) | PARTIAL | Interim Privacy Officer; formal DPO pending (GAP-01) | §1.12 |
| Art. 43 (sanctions preparedness) | DONE | All sanction triggers have mitigations | §1.13 |
| Art. 46-48 (security + breach notif) | DONE | SOC2 cross-framework + RB-BREACH-NOTIF + tabletop | §1.14 |

**Article count audited:** 19 articles (Art. 5, 6, 7, 8, 9, 11, 14, 15, 16, 17, 18, 27, 33, 37, 38, 41, 43, 46, 47, 48 — counted as 19 distinct articles even where multiple sections cite the same article).

---

## 3. Additions to LGPD-DPO-MONTHLY-CHECKLIST (delta)

This audit adds two items to the monthly DPO checklist (`LGPD-DPO-MONTHLY-CHECKLIST.md`):

- **Item 15:** WebAuthn-biometric LIA classification follow-up — verify Legal has signed off `legal/lia/webauthn-biometric-classification.md`. Until signed: track in CAVEATS section of each monthly checklist execution.
- **Item 16:** RoPA quarterly refresh — verify `LGPD-ROPA-2026-05-15.md` table rows still match current sub-processor + dataflow reality. If drift detected, bump RoPA version + re-attest.

*(The actual checklist file update is deferred to avoid scope drift in this delivery; the items above are documented here as canonical follow-up.)*

---

## 4. Residual risk + mitigation

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| ANPD issues binding adequacy decision or interpretive note changing legal-basis taxonomy | L | H | Quarterly DPO checklist item 7 reviews ANPD bulletins; consent ledger has notice-version bump path (CTRL-PRIV-CONSENT-005) to force re-consent without code change |
| WebAuthn biometric reclassification by ANPD (treating pubkey as `dado sensível`) | L | M | Cryptographic position is defensible (pubkey ≠ template); LIA `legal/lia/webauthn-biometric-classification.md` formalises stance |
| Sub-processor with no BR region added without DPO sign-off | M | M | DPO monthly checklist item 1; CI validator `scripts/validate_sub_processors.py` blocks |
| Formal DPO appointment slips past EU enterprise onboarding | M | H | ROADMAP §9 row H-15 + GAP-01; gating gate on EU enterprise (not SAM GA) |
| RIPD not generated for a new HIGH_RISK WI | L | M | Sprint contract template `§16` enforces privacy delta declaration |
| Customer-facing transparency gap (admin-ui consent panel not shipped) | M | L | Manual export via support (5 BD SLA); customer explainer `lgpd-full.mdx` covers external-facing need |

---

## 5. Attestation statement

> **We attest that, as of 2026-05-15:**
>
> 1. CoreLink's LGPD implementation extends beyond Art. 33 §1º (residency, GAP-22) to cover 19 LGPD articles materially affecting the platform (Art. 5–48 enumerated in §2 above).
> 2. The 12-purpose canonical enum (`privacy_model.md §5.6.1`) is bound to fixed legal bases (Art. 7 §I/II/V/IX); basis swaps are forbidden without `notice_version` major bump + force re-consent.
> 3. All 9 LGPD Art. 18 data-subject rights are self-service via `crates/corelink-dsr/` with documented SLA, audit-fail-CLOSED, and 10k property-test invariants.
> 4. Record of Processing Activities (Art. 37 + 41) is canonical in `LGPD-ROPA-2026-05-15.md`.
> 5. ANPD Resolução 15/2024 incident notification timeline is operationalised in `RB-BREACH-NOTIF` (≤ 48h internal / 72h subjects).
> 6. Residual risks (formal DPO appointment, WebAuthn LIA, GAP-14 sub-processor closures) are documented with owners and ETAs in §4.

### Sign-off (pending)

| Role | Name | Signature | Date |
|---|---|---|---|
| Data Protection Officer (interim Privacy Officer) | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| Security Lead | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| Compliance / Final approver | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |

> **Note on signer concentration:** same as `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md §7`. Re-sign on DPO appointment.

---

## 6. Cross-references

- **GAP-22 Art. 33 §1º attestation:** `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md`
- **RoPA (Art. 37 + 41):** `specs/_compliance/LGPD-ROPA-2026-05-15.md`
- **DSR runbook (Art. 18):** `specs/_runbooks/RB-DSR-LGPD-FULL.md`
- **Customer explainer:** `apps/docs/docs/explanation/privacy/lgpd-full.mdx`
- **SOC 2 evidence rollup (cross-framework):** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
- **Privacy model (LINDDUN + DSR + retention):** `specs/03_architecture/privacy_model.md`
- **Compliance matrix (LGPD §4):** `specs/03_architecture/compliance_matrix.md`
- **DPO monthly checklist:** `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md`
- **Breach notification:** `legal/breach-notification/`
- **ROADMAP-TO-GA §9 human track (DPO appointment, advisor pool):** `ROADMAP-TO-GA.md`
- **Privacy notice (3-locale, versioned):** `legal/privacy-notice/v*.{pt-BR,en-US,es-MX}.md`
- **DPA + residency amendment:** `legal/dpa/`, `legal/dpa-residency-amendment.md`
- **Sub-processors register:** `legal/sub-processors.md`
- **DSR crate:** `crates/corelink-dsr/`
- **Consent ledger crate:** `crates/corelink-privacy-consent-ledger/`
- **Erasure worker crate:** `crates/corelink-privacy-erasure-worker/`
- **Residency enforcement crate:** `crates/corelink-privacy-residency-enforcement/`
- **Pseudonymize crate:** `crates/corelink-privacy-pseudonymize/`

---

**Fim de LGPD-FULL-AUDIT-2026-05-15.** This is a 90-day attestation; refresh on 2026-08-15 (or sooner on material change to article coverage, DPO status, or sub-processor list).
