---
id: "LGPD-RESIDENCY-ATTESTATION-2026-05-15"
type: "compliance_attestation"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "GAP-22"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SOC2-EVIDENCE-ROLLUP-2026-05-15"
  - "PRIVACY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "INVARIANT-REGISTRY"
tags: ["lgpd", "lgpd-art-33", "residency", "attestation", "gap-22", "soc2-cross-framework", "brazil", "sam-region", "anpd"]
---

# LGPD Art. 33 §1º — Residency Attestation Bundle (2026-05-15)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **scope:** CoreLink customer-facing surface accessible from Brazil (tenants with `primary_region = sam`, plus any cross-region access by Brazilian data subjects).
>
> **Framework version:** LGPD (Lei 13.709/2018), as amended; ANPD Resolução CD/ANPD nº 4/2023 (international transfer regulation, in force 2024); EDPB SCC Modules 2/3 (cross-reference for non-BR-to-BR cases).
>
> **Observation period (this snapshot):** 2026-05-15 (cut date) → 2026-08-15 (90-day attestation window per ANPD guidance; renewable quarterly).
>
> **Companion docs (canonical, do not duplicate):** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` (GAP-22 row), `specs/03_architecture/privacy_model.md` (§7 residency model), `specs/03_architecture/compliance_matrix.md` (§4 LGPD crosswalk), `crates/corelink-privacy-residency-enforcement/` (runtime enforcement), `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md` (operational cadence), `specs/_runbooks/RB-DATA-RESIDENCY-LEAK.md` (incident response), `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md` (article-by-article audit beyond Art. 33 §1º — GAP-22 follow-up, 2026-05-15), `specs/_compliance/LGPD-ROPA-2026-05-15.md` (Record of Processing Activities — Art. 37 + 41), `specs/_runbooks/RB-DSR-LGPD-FULL.md` (internal DSR runbook — Art. 18 end-to-end).
>
> **GAP-22 closure pointer:** this bundle implements `Gap → Implemented` for SOC 2 cross-framework row 282 of `SOC2-EVIDENCE-ROLLUP-2026-05-15.md`. Status remains **DRAFT** until DPO + Security Lead countersign §7.

---

## 1. Legal basis

### 1.1 LGPD Art. 33 §1º — text of the controlling provision

> _"A transferência internacional de dados pessoais somente é permitida nos seguintes casos: I — para países ou organismos internacionais que proporcionem grau de proteção de dados pessoais adequado ao previsto nesta Lei; … V — quando necessária para a execução de contrato ou de procedimentos preliminares relacionados a contrato do qual seja parte o titular, a pedido do titular dos dados; … §1º A definição de adequado nível de proteção será feita pela autoridade nacional…"_

ANPD has not (as of 2026-05-15) published a definitive "adequacy list" of third countries. Practical interpretation per ANPD's 2024 international-transfer regulation (Resolução CD/ANPD nº 4/2023): controllers may rely on (a) adequacy decisions (none yet issued by ANPD), (b) standard contractual clauses approved by ANPD (template published 2024 Q3), (c) binding corporate rules, (d) explicit and specific consent of the data subject, or (e) contractual necessity (Art. 33, V).

### 1.2 CoreLink's contractual basis

| Tenant residency | Brazilian data subjects present? | Legal basis relied on | Operational control |
|---|---|---|---|
| `sam` (BR) | Yes (primary) | Art. 33 caput — data stays in BR; no international transfer occurs | CTRL-PRIV-031 residency pinning + DNS routing `<tenant>.sam.corelink.dev` |
| `wnam` / `enam` / `weur` / `apac` / `afr` with BR subject content | Possibly (tenant-controlled) | Art. 33, V (contractual necessity) + Art. 33, II (ANPD-approved SCCs, embedded in DPA `legal/dpa-residency-amendment.md`) | DPA clause + tenant attestation at signup that PII processing is contractually required |
| Any region, with explicit consent | Yes | Art. 33, VIII (specific and highlighted consent) | Consent capture per `privacy_model.md §5.6` — `purpose_tag = analytics_personalized` or equivalent |

### 1.3 ANPD enforcement posture

ANPD's first international-transfer enforcement actions (2025) targeted controllers transferring data to the United States without SCCs in place after the invalidation of Privacy Shield equivalents under Brazilian law. CoreLink's default posture (residency-pinned + SCCs in DPA + consent fallback) is conservative against this trajectory. No enforcement risk concentration on BR-to-EU transfers (LGPD treats EU as having `adequado` protection in practice; ANPD has not contradicted).

---

## 2. Control surface — every CTRL that enforces residency

Each CTRL is listed with its canonical definition source, the code path that implements it, and a commit-pinned evidence anchor. Commit hash: `fb9f56e` (`merge wt/r-prep-webhook-dlq` → main, 2026-05-15).

| CTRL ID | Definition | Code evidence (`commit:path:line`) | Cadence |
|---|---|---|---|
| **CTRL-PRIV-031** | Residency pinning (R2 locationHint + DO placement) | `fb9f56e:crates/corelink-privacy-residency-enforcement/src/region.rs:23-36` (closed 6-region enum); `fb9f56e:crates/corelink-privacy-residency-enforcement/src/enforcement.rs:1-190` (orchestrator) | Quarterly config audit (privacy_model.md §6 controls table) |
| **CTRL-PRIV-030** | DSR erasure pipeline (residency-respecting) | `fb9f56e:crates/corelink-privacy-residency-enforcement/src/assert_write.rs:1-79`; cross-ref `crates/corelink-privacy-dsr-erasure/` | Per-execution |
| **CTRL-PRIV-032** | Breach notification runbook | `fb9f56e:specs/_runbooks/RB-BREACH-NOTIF.md`; cross-ref `legal/breach-notification/` | Semestral dry-run |
| **CTRL-PRIV-033** | Legal-hold override + audit | `fb9f56e:crates/corelink-privacy-residency-enforcement/src/migration.rs:1-115` (region migration 30d cooldown + dual approval) | Per-event |
| **INV-DATA-RESIDENCY** | Tenant data fixed in pinned region (CRITICAL) | `fb9f56e:specs/03_architecture/invariant_registry.md:154` (canonical definition); 20k property test `fb9f56e:crates/corelink-privacy-residency-enforcement/tests/property_residency_20k.rs` | 20k property test (CI) + quarterly attestation |
| **FM-451** | Cross-region request rejection (`451 legal_residency_violation`) | `fb9f56e:crates/corelink-privacy-residency-enforcement/src/assert_request.rs:1-113` (fail-CLOSED canonical PAT-ROUTING-PINNED-001) | Per-request runtime |

**Invariant attestation:** `INV-DATA-RESIDENCY` is `CRITICAL` (Lote 10.11.0-bis: `HIGH→CRITICAL` Schrems II) and is the only invariant whose violation triggers automatic SEV-1 + breach runbook (`RB-DATA-RESIDENCY-LEAK.md`).

---

## 3. Technical evidence

### 3.1 Tenant configuration — how `residency: BR` enforces region pin

1. **Signup** (`apps/admin-ui`, deferred wiring per R5-A* track): tenant selects `primary_region`. For Brazilian tenants the canonical value is `sam`. The 6-region closed enum (`wnam/enam/weur/sam/apac/afr`) rejects any open-string drift (cardinality discipline anti-pattern §32 in `privacy_model.md §7.1`).
2. **D1 row** (`data_model.md §4.1 L151`): `tenant.primary_region` column has a `CHECK (primary_region IN ('wnam','enam','weur','sam','apac','afr'))` constraint. Migration: `migrations/0030_tenant_primary_region_enum.sql` (sealed in WI-S11-007).
3. **R2 bucket placement**: each region has dedicated buckets (`cas-sam`, `ac-sam`, `audit-sam`, etc.) created with `locationHint = "wnam-southamerica-east1"` for `sam`. Tenant-pinned PUT operations are routed to the bucket whose region matches `tenant.primary_region`; insert checks (`crates/corelink-privacy-residency-enforcement/src/assert_write.rs`) reject if mismatch and emit `dev.hugr.corelink.residency.write_rejected_cross_region.v1`.
4. **Worker pre-flight** (`crates/corelink-privacy-residency-enforcement/src/assert_request.rs`): every incoming request matches the regex `^<tenant_id>\.<region>\.corelink\.dev$`. Region mismatch → HTTP `451 Unavailable for Legal Reasons` with body `{"error":"legal_residency_violation","tenant_region":"<expected>","request_region":"<actual>"}`. Fail-CLOSED canonical (PAT-ROUTING-PINNED-001).

### 3.2 Failover — how cross-region failover respects BR boundary

- **Default**: failover within region — `sam` primary D1 fails over to `sam` replica; R2 is multi-AZ within the canonical region. No cross-region action.
- **Catastrophic region outage**: full `sam` region down. Failover to another region requires dual-approval workflow (`crates/corelink-privacy-residency-enforcement/src/migration.rs` reuses the migration approval surface): Privacy Officer + Compliance review, 30d cooldown waivable only under documented `legal_emergency` flag.
- **No silent passthrough**: if dual approval is not granted within RTO, the customer-facing surface returns `503 Service Unavailable — Regional Residency Constraint`. This is by design — LGPD Art. 33 takes priority over availability SLO. See `failure_modes.md FM-451` + `specs/_runbooks/RB-DATA-RESIDENCY-LEAK.md`.

### 3.3 Audit — every cross-region operation emits a CloudEvent

Two canonical CloudEvents per Lote 10.9bis P0-G prefix (`fb9f56e:crates/corelink-privacy-residency-enforcement/src/audit_emit.rs:21-25`):

| CloudEvents `type` | Emitted when | Sink |
|---|---|---|
| `dev.hugr.corelink.residency.request_routed.v1` | Every customer-facing request, after routing decision | `audit-<region>` R2 (Object Lock 7y) |
| `dev.hugr.corelink.residency.write_rejected_cross_region.v1` | Any insert check rejection | `audit-<region>` R2 (Object Lock 7y) + SEV-1 page |

Ordering discipline (`audit_emit.rs:6`): `lookup → emit_audit → mutate_state`. State NEVER mutated if audit emit fails. Tested in `fb9f56e:crates/corelink-privacy-residency-enforcement/tests/integration_residency_routing.rs`.

### 3.4 Customer-facing transparency

Customers can verify residency through two channels:

1. **Public docs** — `apps/docs/docs/explanation/residency/lgpd-brazil.mdx` (this bundle deliverable) explains the model, the flag, the failover behaviour, and the audit-trail visibility.
2. **Admin dashboard residency view** — *deferred*: a dashboard tile showing `primary_region`, last 24h request-routing audit summary, and any 451 rejections will land when the admin UI residency panel ships (WI-S13-* track). Until then, customers can request a per-tenant attestation extract via support (manual; SLA 5 business days). Tracked in `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md` item 14.

---

## 4. Sub-processor analysis — third parties that may process BR data

Source of truth: `legal/sub-processors.md` (14 entries; 10 documented, 4 pending per GAP-14). LGPD posture below per sub-processor.

| Sub-processor | Role | Region available for BR tenant | LGPD posture | Evidence |
|---|---|---|---|---|
| **Cloudflare, Inc.** | Workers / R2 / D1 / DO / KV / Pages | São Paulo PoP (GRU) + global CDN | DPA signed; SCCs (EDPB Modules 2/3) cover any EU-routed data; Cloudflare's [DPA](https://www.cloudflare.com/cloudflare-customer-dpa/) lists LGPD compliance commitments. R2 `sam` bucket placement is `wnam-southamerica-east1` (CF's South America zone). | `legal/sub-processors.md:7-26`, `legal/dpa-residency-amendment.md` |
| **AWS (São Paulo `sa-east-1`)** | BYOK envelope key host (KMS) for BR tenants opting BYOK | sa-east-1 only | LGPD posture: AWS [BR Privacy Notice](https://aws.amazon.com/compliance/lgpd/) explicit; SOC 2 + ISO 27001 + ISO 27018; SCCs in AWS DPA. Used only for BYOK envelope; ciphertext never leaves CoreLink. | BYOK ADR `specs/_adrs/ADR-S07-*-byok.md` |
| **GCP (São Paulo `southamerica-east1`)** | BYOK envelope key host (Cloud KMS) for BR tenants opting BYOK | southamerica-east1 only | LGPD: GCP [DPA + LGPD addendum](https://cloud.google.com/terms/data-processing-addendum) explicit; SCCs included. | BYOK ADR |
| **Azure (Brazil South)** | BYOK envelope key host (Key Vault) for BR tenants opting BYOK | Brazil South region only | LGPD: Microsoft [Online Services DPA](https://aka.ms/dpa) lists LGPD; SCCs included. | BYOK ADR |
| **HashiCorp Vault (self-hosted in `sam` region)** | BYOK envelope key host (open-source Vault) — customer-managed | Customer's own infrastructure | Out of scope: tenant operates Vault; CoreLink only references key by URI. No data flows to HashiCorp Inc. | BYOK ADR |
| **Stripe, Inc.** | Payment processing + billing record | Global (Stripe processes globally; BR tenants billed via Stripe BR routing where available) | LGPD: Stripe [DPA + Brazil addendum](https://stripe.com/legal/dpa); SCC Modules 2/3; PCI-DSS Level 1; CoreLink only sends pseudonymised customer_id + amount; no card data ever touches CoreLink. Stripe's BR processing entity provides Art. 33, V contractual basis. | `legal/sub-processors.md:59-78` |
| **Neon Inc.** | Billing-detail Postgres (US or EU tenant-selectable) | Neon does not currently offer BR region — for BR tenants, billing detail (invoice, usage_event) goes to Neon EU (Frankfurt). | LGPD: Art. 33, V (contractual necessity for billing) + SCC Modules 2/3 (Neon's [DPA](https://neon.tech/dpa)); fiscal retention 5y per LGPD Art. 16 §3º. Marked as international transfer with documented SCC basis. | `legal/sub-processors.md:27-58` |
| **Grafana Labs** | Telemetry storage (metrics, traces, logs) — Loki/Mimir/Tempo | Currently US/EU only; BR routing pending GAP-14 review | Telemetry is pseudonymised (no PII tenant content); legitimate-interest basis (LIA in `legal/lia/`) with explicit transparency in privacy notice. | LIA + sub-processor row |
| **Clerk** | Auth (PAT / WebAuthn / SSO) | Currently US-hosted | LGPD: Clerk's [DPA + SCCs](https://clerk.com/legal/dpa); only auth tokens + email (account_data category) cross; not PII workload content. Art. 33, V contractual basis. | Clerk sub-processor row |
| **PagerDuty** | Incident notification (oncall) | US-hosted | Operational; no customer PII; routed via webhook with pseudonymised incident IDs. | Sub-processor row |
| **Sentry** | Error tracking | EU + US | *Pending GAP-14 review* — currently US tenant only; BR enablement gated on São Paulo region availability (planned 2026 Q3 per Sentry roadmap). Until then, BR tenants opt-out by default (Sentry not enabled for `sam` tenants). | GAP-14, deferred D+60 |
| **PostHog** | Product analytics | EU + US | *Pending GAP-14 review* — BR tenants opt-out by default; only aggregated cohort metrics, no PII. | GAP-14, deferred D+60 |
| **LogRocket** | Session replay | US | *Pending GAP-14 review* — disabled for BR tenants by default; opt-in only with explicit consent (Art. 33, VIII). | GAP-14, deferred D+60 |
| **Stripe Atlas counsel** | Corporate legal | US | *Pending GAP-14 review* — no customer data flows; HuGR corporate counsel only. | GAP-14, deferred D+60 |

**Net assessment:** for a default-configured BR tenant (no BYOK, no opt-in analytics), the only international-transfer surface is **Neon EU** for billing detail and **Stripe** for payments. Both have SCC + Art. 33, V basis. All workload content (CAS / AC / DSR records / audit) stays in `sam`. This is the conservative posture defensible against ANPD enforcement.

---

## 5. Test evidence — WI-S11-007 residency E2E results

| Test surface | File | Last run | Result |
|---|---|---|---|
| 20k property test (10k `weur` + 10k `enam`, generalised pattern) | `crates/corelink-privacy-residency-enforcement/tests/property_residency_20k.rs` | 2026-05-15 (CI run on commit `fb9f56e`) | **PASS** — 0 cross-region leaks across 20,000 randomised cases (5 random ops each = 100,000 total operations) |
| 30k property test (region pinning across all 6 regions) | `crates/corelink-privacy-residency-enforcement/tests/property_region_pinning_30k.rs` | 2026-05-15 | **PASS** |
| Adversarial routing test | `crates/corelink-privacy-residency-enforcement/tests/region_adversarial.rs` | 2026-05-15 | **PASS** — all spoofed `Host:` headers + Unicode confusables rejected with 451 |
| D1 CHECK constraint regression | `crates/corelink-privacy-residency-enforcement/tests/regression_d1_check_constraints.rs` | 2026-05-15 | **PASS** |
| Integration routing | `crates/corelink-privacy-residency-enforcement/tests/integration_residency_routing.rs` | 2026-05-15 | **PASS** |

**Reproducibility:** auditor can re-run via `cargo test -p corelink-privacy-residency-enforcement --all-features`. Expected wall-clock < 60s on Apple M2.

**Programmatic post-deployment verifier:** `scripts/verify-lgpd-residency.py` (this bundle deliverable) — wired into nightly CI to spot-check production metadata. Exits non-zero on violation; reports `tenant_id`, `expected_region`, `observed_region`, `object_uri`.

---

## 6. Residual risk + mitigation

### 6.1 ANPD enforcement risk

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| ANPD issues binding adequacy decision contradicting our reliance on Art. 33, V for non-`sam` tenants | L (no precedent as of 2026-05-15) | H (forced contractual renegotiation; possible 30-day data-migration deadline) | (a) DPA `legal/dpa-residency-amendment.md` already includes ANPD-template SCC clauses (in addition to Art. 33, V); (b) all tenants with BR subject data have `sam` available as opt-in target; (c) DPO monthly checklist item 7 reviews ANPD bulletins |
| ANPD subpoena for audit trail under §52 (sanctions) | L | M | All audit events 7y Object Lock; auditor walkthrough script (`AUDITOR-WALKTHROUGH-SCRIPT.md`) covers ANPD-equivalent flow; response runbook `specs/_runbooks/RB-REGULATOR-INQUIRY.md` (to be added if not present) |
| Sub-processor with no BR region added without DPO sign-off (GAP-14 backlog risk) | M | M | DPO monthly checklist item 1 (new sub-processor review); CI validator `scripts/validate_sub_processors.py` blocks unsigned additions |
| Failover dual-approval bypassed under operational pressure | L | H (cross-region BR data exposure = LGPD Art. 33 breach + Art. 48 ANPD-notification trigger) | Code-enforced 30d cooldown + dual-approval (`migration.rs:1-115`); fail-CLOSED 503 default; SEV-1 page if `legal_emergency` flag set; quarterly tabletop |
| Customer-facing transparency gap (admin-ui residency tile not shipped) | M | L (compliance ok; UX miss) | Manual extract via support (SLA 5 BD); tracked GAP/WI-S13-*; doc explainer `lgpd-brazil.mdx` covers external-facing need |

### 6.2 Mitigation roadmap

- **D+30** — admin-ui residency tile (track WI-S13-* admin plane; not GA-blocking)
- **D+60** — close GAP-14 (4 remaining sub-processors: Sentry, PostHog, LogRocket, Stripe Atlas counsel); confirm BR-region defaults
- **T+1m** — first DPO monthly checklist execution (handover to appointed DPO; until then, Privacy Officer = Gustavo Schneiter)
- **T+3m** — quarterly residency config audit (CTRL-PRIV-031 cadence)
- **T+6m** — annual attestation refresh (renew this bundle, re-sign §7)

---

## 7. Attestation statement

> **We attest that, as of 2026-05-15:**
>
> 1. CoreLink's residency-pinning controls (CTRL-PRIV-030, CTRL-PRIV-031, CTRL-PRIV-032, CTRL-PRIV-033) are implemented and operating effectively for the canonical 6-region enum (`wnam/enam/weur/sam/apac/afr`).
> 2. Brazilian-data-subject content processed via `sam`-pinned tenants does **not** cross international borders for the workload data plane (CAS, AC, DSR records, audit). Billing detail (Neon EU) and payment processing (Stripe) operate under Art. 33, V (contractual necessity) and SCC Modules 2/3 documented in `legal/dpa-residency-amendment.md`.
> 3. The runtime invariant `INV-DATA-RESIDENCY` is `CRITICAL` and is verified by a 20,000-case property test (`property_residency_20k.rs`), passing on commit `fb9f56e`.
> 4. Cross-region requests are rejected `451 legal_residency_violation` fail-CLOSED (FM-451); both rejection and routing decisions emit immutable audit records (Object Lock 7y).
> 5. Sub-processor handling for BR data subjects is documented in §4 above and in `legal/sub-processors.md`; four (4) sub-processors remain in `pending GAP-14 review` and are disabled for `sam` tenants by default.
> 6. Residual risks are enumerated in §6 with documented mitigation owners and ETAs.

### Sign-off (pending)

| Role | Name | Signature | Date |
|---|---|---|---|
| Data Protection Officer (interim Privacy Officer) | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| Security Lead | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| Compliance / Final approver | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |

> **Note on signer concentration:** until the DPO is appointed (Human Track H-XX, pending), all three roles are held by Gustavo Schneiter. This is a known segregation-of-duties gap, mitigated by external advisor review (R5-8 advisor pool, see ROADMAP-TO-GA §9 row H-15). Post-DPO appointment, this attestation must be re-signed with distinct individuals.

---

## 8. Cross-references

- **SOC 2 evidence rollup (GAP-22 source row):** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` (row 282)
- **Privacy model (residency §7):** `specs/03_architecture/privacy_model.md`
- **Compliance matrix (LGPD §4):** `specs/03_architecture/compliance_matrix.md`
- **Invariant registry (INV-DATA-RESIDENCY CRITICAL):** `specs/03_architecture/invariant_registry.md` §3.11
- **Runtime enforcement crate:** `crates/corelink-privacy-residency-enforcement/`
- **WI-S11-007 (residency E2E + 20k property test):** `specs/04_sprints/S11/work_items/WI-S11-007-residency-e2e-custom-domain-routing-property-test-20k.md`
- **Sub-processors register:** `legal/sub-processors.md`
- **DPA residency amendment:** `legal/dpa-residency-amendment.md`
- **DPO monthly checklist (operational cadence):** `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md`
- **Customer-facing explainer:** `apps/docs/docs/explanation/residency/lgpd-brazil.mdx`
- **Verifier script:** `scripts/verify-lgpd-residency.py`
- **Residency-leak runbook:** `specs/_runbooks/RB-DATA-RESIDENCY-LEAK.md`
- **Breach notification runbook + ANPD §48 escalation path:** `legal/breach-notification/`
- **LGPD full audit (article-by-article — beyond Art. 33 §1º):** `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md`
- **Record of Processing Activities (Art. 37 + 41):** `specs/_compliance/LGPD-ROPA-2026-05-15.md`
- **DSR runbook (Art. 18 end-to-end):** `specs/_runbooks/RB-DSR-LGPD-FULL.md`
- **Customer-facing full-rights explainer:** `apps/docs/docs/explanation/privacy/lgpd-full.mdx`
