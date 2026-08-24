---
id: "COMPLIANCE-MATRIX"
type: "compliance_matrix"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.3.0"
created: "2026-04-23"
updated: "2026-05-30"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from: ["SECURITY-MODEL", "PRIVACY-MODEL", "OBSERVABILITY-MODEL", "AUTH-MODEL", "KEY-MANAGEMENT"]
tags: ["architecture", "compliance", "soc2", "iso27001", "lgpd", "gdpr"]
---

# Compliance Matrix — SOC 2, ISO 27001, LGPD, GDPR

> **doc_status:** DRAFT
> **Versão:** 0.3.0
> **Última atualização:** 2026-05-30
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** — promoção a `doc_status: FROZEN` bloqueada até ≥ 2 reviewers nomeados conforme roles indicados (endereça F-09 audit Lote 3+4)
> **Supersedes:** —
> **Superseded By:** —

> **REALITY ANCHOR (2026-05-30):** This document was first written 2026-04-24 in
> aspirational mode before any production wiring existed. As of 2026-05-30 the
> data plane is LIVE and SHIPPABLE (verified by multi-model panel + live smoke —
> see `specs/_audits/2026-05-28-multimodel-prod-readiness-audit.md §9`). This
> v0.3.0 update reclassifies every control using the honest SOC 2 audit
> vocabulary:
>
> | Status label | Meaning in SOC 2 audit language |
> |---|---|
> | **IMPLEMENTED + EVIDENCED** | Control exists in code AND a verifiable artifact backs it (live probe, property-test run, signed report). An auditor can obtain evidence today. |
> | **IMPLEMENTED — needs evidence** | Control logic is in production code but no formal artifact exists yet (no screenshot, no drill log, no signed report). An auditor would need to wait for evidence collection. |
> | **DESIGNED — not implemented** | Spec and/or code skeleton exists; the control is NOT active on the live request path. An auditor would mark this "not yet in place." |
> | **DEFERRED** | Out of scope for current cycle; acknowledged, tracked, not claimed. |
>
> SOC 2 auditors distinguish **designed vs. implemented vs. operating**. A control
> that exists only in a spec is DESIGNED. A control in deployed code is
> IMPLEMENTED. A control with 6+ months of continuous evidence is OPERATING
> (required for Type II; not required for Type I "point-in-time design review").
>
> **Honest net readiness gap:** data plane just reached IMPLEMENTED state
> (2026-05-30). The earliest a Type I observation window can open is
> GA + 30 days (evidence collection baseline). Type I fieldwork target remains
> T+6 months per the roadmap. No control should be claimed as OPERATING until
> 2026-07-01 at the earliest.

> **Propósito:** fonte canônica (Nível 3) do mapeamento entre frameworks regulatórios/de auditoria e os controles internos do CoreLink (`CTRL-XXX` em `security_model.md` + `CTRL-PRIV-XXX` em `privacy_model.md`). Consumido por:
> - `specs/_templates/production_readiness_review.md §11` (verificação de controles em produção)
> - `specs/_templates/sprint_contract.md §16` (delta de compliance do sprint)
>
> Regra: todo novo control interno **DEVE** ter pelo menos 1 mapping para framework externo OU justificativa em ADR do por quê não aplica.

---

## Sumário

1. [Estado atual e roadmap de certificações](#1-estado-atual-e-roadmap-de-certificações)
2. [SOC 2 Type II — Trust Services Criteria](#2-soc-2-type-ii--trust-services-criteria)
3. [ISO/IEC 27001:2022](#3-isoiec-270012022)
4. [LGPD (Brasil)](#4-lgpd-brasil)
5. [GDPR (UE)](#5-gdpr-ue)
6. [Outras regulamentações relevantes](#6-outras-regulamentações-relevantes)
7. [Sub-processor compliance posture](#7-sub-processor-compliance-posture)
8. [Evidence collection strategy](#8-evidence-collection-strategy)
9. [Gap analysis](#9-gap-analysis)
10. [Gap to SOC 2 Type I readiness — honest checklist](#10-gap-to-soc-2-type-i-readiness--honest-checklist)
11. [Referências](#11-referências)

---

## 1. Estado atual e roadmap de certificações

> **v0.3.0 honest update:** states below reflect 2026-05-30 reality after the
> multi-model production audit and live smoke verification. "Compliance by design"
> is now upgraded to "Controls IMPLEMENTED; evidence accumulation begins" for the
> in-scope frameworks. Certification timeline starts from the GA date (2026-05-30
> is treated as Day 0).

| Framework              | Escopo                                | Estado (2026-05-30)     | Target                                       |
|------------------------|----------------------------------------|-------------------------|-----------------------------------------------|
| SOC 2 Type I           | Security + Availability + Confidentiality | **Controls IMPLEMENTED; 0-day operating window.** Type I requires design review only — observation window can open now. Internal readiness 83.7% / Drata 96.4% (snapshot 2026-05-14; see `specs/_audits/sealed/2026-05-14-soc2-readiness-score.md`). 33 open GAPs; 1 was blocking-GA (GAP-02 BYOK FIPS attestation — remediation in-flight per `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md`). | Type I fieldwork: T+6m (target 2026-11-30). Must close GAP-02 before fieldwork. |
| SOC 2 Type II          | Acima + Privacy + Processing Integrity | **Not yet started.** Requires 6+ months of continuous operating evidence post Type I. | 12–18 meses após Type I clean opinion. |
| ISO/IEC 27001:2022      | ISMS — escopo CoreLink CAS/AC          | **Crosswalk only.** Full 93-control SoA at `specs/_compliance/ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15.md`; 91% SOC 2 overlap; 7 ISO-unique gaps. No ISMS established. | Phased roadmap to Q1-2027 per `specs/_compliance/ISO27001-ROADMAP.md`. |
| LGPD                   | Brasil (operações sam region)          | **Controls IMPLEMENTED.** DSR (crates/corelink-dsr), consent ledger, erasure pipeline (12 backends), residency pinning (Art. 33 §1º), DPO appointed (interim). Full article-by-article audit: `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md`. | Operating; evidence accumulation begins D+0. |
| GDPR                   | UE (operações weur region)             | **Controls IMPLEMENTED.** Same surface as LGPD + SCC 2021/914 module mapping, Art. 35 DPIA library. `specs/_compliance/GDPR-FULL-AUDIT-2026-05-15.md`. | Operating; evidence accumulation begins D+0. |
| HIPAA                  | Saúde US (opcional, plano enterprise)  | **Conditional / not activated.** BYOK available; 6+ year audit log retention in design. No BAA signed. | Sob demanda cliente (BAA + audit). |
| PCI-DSS v4.0 SAQ-A      | Card-not-present merchant, all CHD outsourced to Stripe | **Compliant (self-attested).** SAQ-A signed 2026-05-15. | Next renewal 2027-05-15. See `specs/_compliance/PCI-DSS-SAQ-A-2026-05-15.md`. |
| FedRAMP                | US gov                                  | **Not-in-scope.** Informational crosswalk only (~85% Moderate baseline covered via SOC 2 + ISO 27001). | Sob demanda + sponsorship. See `specs/_compliance/FEDRAMP-NOT-IN-SCOPE-RATIONALE.md`. |

> **Princípio:** construir os controles **agora**, certificar quando houver demanda. Controles sem certificação ainda valem (clientes pedem SIG ou CAIQ para diligence).

---

## 2. SOC 2 Type II — Trust Services Criteria

### 2.1 Escopo (TSC 2017, rev. 2022)

Planejamento: **Security + Availability + Confidentiality + Privacy + Processing Integrity**.

> **v0.3.0 honest update:** each row now carries an honest implementation status
> using the four-tier vocabulary defined in the REALITY ANCHOR above. Readiness
> score source: `specs/_audits/sealed/2026-05-14-soc2-readiness-score.md`
> (83.7% internal / 96.4% Drata as of 2026-05-14). Live verification:
> `specs/_audits/2026-05-28-multimodel-prod-readiness-audit.md §9`
> (SHIPPABLE verdict 2026-05-30).

### 2.2 Mapping — Common Criteria (CC)

| TSC Criterion | Requisito resumido | CTRLs internos | Crate / artifact | Status (2026-05-30) |
|---|---|---|---|---|
| CC1.1 | Commitment to integrity/ethics | Code of Conduct, policies | `legal/`, `README.md §contributing` | **IMPLEMENTED — needs evidence** (owner-attested; no third-party review yet) |
| CC1.2..1.5 | Governance, authority, competence | Org chart, RACI, advisor pool | `specs/_governance/`; `legal/legal-externo-engagement-contract.md` | **IMPLEMENTED — needs evidence** (GAP-04 advisor pool TBD; GAP-05 competence attestations not yet collected) |
| CC2.1..2.3 | Information + communication (policies, ethics) | Public policies; incident disclosure | `apps/docs/`, status page | **IMPLEMENTED — needs evidence** (policies public; no 6-month operating log yet) |
| CC3.1..3.4 | Risk assessment | `failure_modes.md`; annual risk review | `specs/_governance/`, `specs/_audits/` | **IMPLEMENTED — needs evidence** (GAP-07 closed via Drata; risk review cadence not yet operating for 6m) |
| CC4.1..4.2 | Monitoring activities | Observability stack | `crates/corelink-audit-chain/src/lib.rs` (BLAKE3 Merkle chain, 10k property tests); Grafana Cloud | **IMPLEMENTED + EVIDENCED** (audit chain live; Drata continuous monitoring 96.4% green) |
| CC5.1..5.3 | Control activities | All CTRL-* catalogs | `specs/03_architecture/security_model.md` | **IMPLEMENTED + EVIDENCED** (100% in readiness score; all CTRL-* catalog present) |
| CC6.1 | Logical access / authentication | CTRL-AUTH-001, -004, -007, -010 | `crates/corelink-auth/`, `crates/corelink-pat/` (Argon2id two-stage PAT); live probe 2026-05-30: `GET /v1/users/me` HTTP 200 with fresh PAT | **IMPLEMENTED + EVIDENCED** (live-verified; ⚠️ P1-4: `crates/corelink-pat/src/verify.rs:54` token_id compare is non-constant-time — fix tracked; GAP-02 BYOK FIPS attestation in-flight) |
| CC6.2 | Authorization | CTRL-AUTHZ-001, -002 | `worker/src/`, `crates/corelink-worker/src/middleware/auth.rs` | **IMPLEMENTED + EVIDENCED** (live cross-tenant probe returned 403 "tenant mismatch" 2026-05-30) |
| CC6.3 | Access provisioning | `auth_model.md §5`; quarterly review | `specs/03_architecture/auth_model.md` | **IMPLEMENTED — needs evidence** (GAP-01: quarterly access review not yet run; first review due T+1m) |
| CC6.6 | Logical/physical boundaries | Trust boundaries §3 security_model | `crates/tenant-path/src/lib.rs` (HMAC-SHA256 prefix derivation); `crates/corelink-worker/src/middleware/` | **IMPLEMENTED + EVIDENCED** (HMAC tenant isolation 5-layer defense confirmed by Haiku-10 panel + live 403 cross-tenant probe) |
| CC6.7 | Transmission / in-flight | CTRL-CRYPTO-001 | TLS termination at Cloudflare edge, floor 1.2 since 2026-07-19 (ADR-0072); no plaintext path | **IMPLEMENTED — evidence stale** (the cited SSL Labs A+ scan predates the floor change; a re-scan is owed before this row is cited again) |
| CC6.8 | Malicious code / unauthorized software | CTRL-SUPPLY-001..005 | `deny.toml` strict allowlist; digest-pinned base images; non-root container; `semgrep.yml` | **IMPLEMENTED + EVIDENCED** (Haiku-container panel: supply chain EXCELLENT; deny.toml enforced in CI) |
| CC7.1..7.5 | Detection + incident response | Observability + runbooks + incident template | `crates/corelink-audit-chain/src/lib.rs`; `specs/_compliance/IR-TABLETOP-PLAYBOOK.md`; `specs/_runbooks/` | **IMPLEMENTED — needs evidence** (audit chain live; incident runbooks exist; no real incidents run through them yet; GAP-12 IR procedure evidence gap) |
| CC8.1 | Change management | PR review, progressive rollout, dual-approval | GitHub PR + branch protection; `scripts/` deploy runbooks | **IMPLEMENTED + EVIDENCED** (every commit gated by PR; branch protection enforced) |
| CC9.1 | Risk mitigation | Compensating controls via `waiver.md` | `specs/_governance/`, `specs/_audits/waiver*` | **IMPLEMENTED — needs evidence** (compensating control framework exists; no waiver countersignatures collected yet) |
| CC9.2 | Vendor management | `§7 sub-processors` + `VENDOR-RISK-REGISTER.md` | `specs/_compliance/VENDOR-RISK-REGISTER.md` (19 vendors + DD files); `scripts/subprocessor-change-notify.py` | **IMPLEMENTED + EVIDENCED** (19 vendors logged; DD files exist; quarterly review runbook written; 30-day notify clock automated) |

### 2.3 Mapping — Availability (A series)

| TSC Criterion | Requisito resumido | CTRLs internos | Crate / artifact | Status (2026-05-30) |
|---|---|---|---|---|
| A1.1 | Identify/monitor availability | SLOs `slo_catalog.md` + alerts | `specs/03_architecture/slo_catalog.md`; Grafana Cloud; BetterStack uptime probes | **IMPLEMENTED + EVIDENCED** (SLOs defined; Grafana dashboards live; BetterStack smoke passed 2026-05-30) |
| A1.2 | Recovery + contingency | Runbooks + chaos tests | `specs/_compliance/BCP-DR-DRILL-CADENCE.md` (14 drills documented); `specs/_runbooks/` | **IMPLEMENTED — needs evidence** (DR runbooks written; 14 drill cadence documented; GAP-15: cold restore drill not yet executed against production data) |
| A1.3 | Recovery infrastructure testing | DR drill semestral | `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` | **DESIGNED — not implemented** (spec written; production cold-restore drill not yet executed; GAP-15 open) |

### 2.4 Mapping — Confidentiality (C series)

| TSC Criterion | Requisito resumido | CTRLs internos | Crate / artifact | Status (2026-05-30) |
|---|---|---|---|---|
| C1.1 | Identify + classify confidential | `privacy_model.md §2`; CTRL-CRYPTO-002 | `crates/corelink-byok/src/lib.rs` (BYOK umbrella: AWS/GCP/Azure/Vault all 4 providers, compile-time mutual exclusion); `crates/corelink-erasure-attestation/src/lib.rs` (Ed25519/FIPS 186-5 signed erasure receipts) | **IMPLEMENTED — needs evidence** (⚠️ GAP-02: BYOK FIPS attestation letters in-flight per `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md`; ⚠️ P1-1: Azure/GCP/Vault BYOK use non-canonical AAD serialization — `serde_json` not `serde_jcs` — potential DEK unrecoverability, tracked as data-loss risk pre-GA+30) |
| C1.2 | Disposal | CTRL-CRYPTO-002, CTRL-ISO-001..005 | `crates/corelink-privacy-erasure-worker/src/lib.rs` (12-backend erasure pipeline); `crates/corelink-erasure-attestation/src/lib.rs` (signed attestation, 7-year R2 retention); `crates/corelink-dsr/src/lib.rs` (erasure orchestration) | **IMPLEMENTED + EVIDENCED** (12-backend erasure pipeline exists with Ed25519-signed completion report; property-tested; DSR end-to-end runbook `specs/_runbooks/RB-DSR-LGPD-FULL.md`) |

### 2.5 Mapping — Processing Integrity (PI series)

| TSC Criterion | Requisito resumido | CTRLs internos | Crate / artifact | Status (2026-05-30) |
|---|---|---|---|---|
| PI1.1 | Inputs complete/valid | CTRL-INPUT-001..004 | `crates/corelink-worker/src/middleware/`; input validation on PUT/GET paths | **IMPLEMENTED + EVIDENCED** (live probe: `PUT /v1/cas/<tenant>/<sha256>` with 16-byte blob returned HTTP 201, body echoes hash — 2026-05-30) |
| PI1.2 | Processing complete/accurate | CTRL-CAS-001, -002 | `crates/corelink-cas/`; BLAKE3 content-addressing guarantees bit-exact retrieval | **IMPLEMENTED + EVIDENCED** (live probe: `GET /v1/cas/<tenant>/<sha256>` body matched stored payload byte-for-byte — 2026-05-30) |
| PI1.3 | Outputs accurate | CTRL-AC-001, -002; PAT-RECONCILE-001 | `crates/corelink-ac/`; live `PUT /v1/ac/<tenant>/<key>` returned HTTP 201 | **IMPLEMENTED + EVIDENCED** (live probe passes; audit export endpoint returned 404 in probe — P2 follow-up) |
| PI1.4 | Authorized parties only | CTRL-AUTHZ-001; tenant isolation | `crates/tenant-path/src/lib.rs`; `crates/corelink-worker/src/middleware/auth.rs` | **IMPLEMENTED + EVIDENCED** (cross-tenant probe: HTTP 403 "tenant mismatch" live 2026-05-30) |
| PI1.5 | Storage completeness | Audit chain + R2 Object Lock | `crates/corelink-audit-chain/src/chain.rs` (BLAKE3 Merkle); R2 Object Lock Governance 7-year | **IMPLEMENTED — needs evidence** (⚠️ P1-3: `crates/corelink-audit-chain/src/verifier.rs:162` chain-break compare uses `!=` non-constant-time — timing oracle; tracked; fix required before Type I fieldwork) |

### 2.6 Mapping — Privacy (P series)

| TSC Criterion | Requisito resumido | CTRLs internos | Crate / artifact | Status (2026-05-30) |
|---|---|---|---|---|
| P1.1 | Privacy notice | `/privacy` page; versioned | `apps/docs/docs/trust/` public privacy pages | **IMPLEMENTED + EVIDENCED** (pages live; versioned) |
| P2.1 | Consent | CTRL-PRIV-CONSENT-001..004 | `crates/corelink-privacy/src/consent/` (absorbed from corelink-privacy-consent-ledger) | **IMPLEMENTED — needs evidence** (consent ledger in code; no production consent event log exists yet — operating window 0 days) |
| P3.1..3.2 | Collection limited to purpose | CTRL-PRIV-003 | `crates/corelink-privacy/src/` (purpose_tag enforcement) | **IMPLEMENTED — needs evidence** (design enforced in code; evidence log starts D+0) |
| P4.1..4.3 | Use, retention, disposal | `privacy_model.md §8` | `crates/corelink-privacy/src/erasure/` (12-backend pipeline); R2 retention policies | **IMPLEMENTED + EVIDENCED** (erasure pipeline exists; R2 Object Lock 7y configured; retention schedules documented) |
| P5.1..5.2 | Access + correction (DSR) | CTRL-PRIV-022 DSR self-service | `crates/corelink-dsr/src/lib.rs` (6 DSR rights: access/portability/rectification/erasure/restriction/objection; 10k property-tested); `specs/_runbooks/RB-DSR-LGPD-FULL.md` | **IMPLEMENTED + EVIDENCED** (DSR orchestrator has 10k property tests against all 6 rights; end-to-end runbook complete; Art. 18 I-IX mapped) |
| P6.1..6.7 | Disclosure + notification | DPA + breach runbook | `crates/corelink-privacy/src/breach/` (GDPR Art. 33 72h timer); `specs/_runbooks/RB-BREACH-NOTIF.md`; `legal/dpa/` | **IMPLEMENTED — needs evidence** (breach emit crate + runbook exist; no real breach events to point to; 72h timer validated by code review only) |
| P7.1 | Data quality | Reconciliation | `crates/corelink-audit-chain/` (BLAKE3 chain integrity); CAS content-addressing | **IMPLEMENTED + EVIDENCED** (BLAKE3 content-addressing is a cryptographic data-quality guarantee; live GET matched stored bytes 2026-05-30) |
| P8.1 | Monitoring + enforcement | Privacy Officer role + quarterly review | `specs/_compliance/DPO-APPOINTMENT-2026-05-15.md`; interim DPO appointed (Gustavo Schneiter); `DPO-RESPONSIBILITIES-MATRIX.md` | **IMPLEMENTED — needs evidence** (interim DPO formally designated per LGPD Art. 41; GAP-01 quarterly review cadence not yet operating) |

---

## 3. ISO/IEC 27001:2022

### 3.1 Escopo

Statement of Applicability (SoA) cobre os 93 controles Anexo A:2022. Mapping compacto (full SoA em `_audits/iso27001-soa.csv`).

> **Preliminary crosswalk (2026-05-15):** the full 93-control mapping (Annex A → SOC 2 → CTRL-* → evidence → status) lives at `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md` (98.9% in-scope coverage; 91% SOC 2 overlap). ISO-unique gaps (7 active + 1 informational): `specs/_compliance/ISO27001-GAP-ANALYSIS.md`. Phased roadmap to Q1-2027 certification: `specs/_compliance/ISO27001-ROADMAP.md`. Customer-facing one-pager: `apps/docs/docs/trust/iso27001.mdx`.

### 3.2 Temas críticos (seleção)

| Anexo A (2022) | Tema                                      | CTRLs internos                          | Notas                                 |
|----------------|-------------------------------------------|-----------------------------------------|----------------------------------------|
| A.5 Org. controls | Policies, roles, confidentiality, threat intel | Policies públicas; Security Lead    | Aplicável 100%                         |
| A.6 People     | Screening, training, NDA, sanctions         | HR process                              | Aplicável                              |
| A.7 Physical   | Site access, equipment                      | Cloudflare data centers (herda)          | Shared responsibility — evidence do sub-processor |
| A.8 Technological | Authentication, access, crypto, vuln mgmt, backup, logging, config mgmt, testing, software dev | CTRL-AUTH-*, CTRL-CRYPTO-*, CTRL-AUDIT-*, CTRL-SUPPLY-* | Core deste doc      |

### 3.3 Controles específicos relevantes

| Controle Anexo A | Implementação CoreLink | Evidence |
|------------------|------------------------|----------|
| A.8.2 (Privileged access) | Just-in-time admin; MFA mandatório | CTRL-AUTH-010 |
| A.8.9 (Config mgmt)       | Terraform + drift detection        | PAT-DRIFT-DETECTION-001 |
| A.8.24 (Cryptography)     | Algoritmos e key mgmt              | §7 security_model |
| A.8.25 (Secure dev)       | Code review + SAST                 | CTRL-INPUT-002, CTRL-SUPPLY-005 |
| A.8.28 (Secure coding)    | Rust + lints + fuzz                | EVT-008   |
| A.8.29 (Security testing) | Pentest anual                       | EVT-025 |
| A.8.30 (Outsourced dev)   | N/A (in-house)                      | — |

---

## 4. LGPD (Brasil)

> **Full article-by-article audit (canonical, do not duplicate):** `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md` (19 articles audited — Art. 5–48); `specs/_compliance/LGPD-ROPA-2026-05-15.md` (14-row RoPA — Art. 37 + 41); `specs/_runbooks/RB-DSR-LGPD-FULL.md` (Art. 18 end-to-end); `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` (Art. 33 §1º — GAP-22).

### 4.1 Bases principais

| Art.   | Requisito                                             | Como atendemos                            | Evidence                         |
|--------|-------------------------------------------------------|--------------------------------------------|----------------------------------|
| Art. 6 | Finalidade, adequação, necessidade                    | CTRL-PRIV-003 (purpose_tag)                | EVT-026            |
| Art. 7 | Base legal                                             | `privacy_model.md §9`                      | EVT-044                 |
| Art. 11 | Dado sensível — não coletamos default                | §2 privacy_model                            | EVT-026            |
| Art. 15 | Término do tratamento                                  | §8 privacy_model (retention)                | EVT-042                 |
| Art. 17 | Titular tem direito                                    | §6 privacy_model (DSRs)                     | EVT-048 (DSR_EVIDENCE)     |
| Art. 18 I–IX | Direitos específicos (9 rights + §II amendment objection) | §6 privacy_model + `crates/corelink-dsr/` (10k property test); end-to-end runbook `RB-DSR-LGPD-FULL.md`; article-by-article mapping `LGPD-FULL-AUDIT-2026-05-15.md §1.8` | EVT-048 (DSR_EVIDENCE) + EVT-049 (consent revoke) + EVT-042 (erasure pipeline 12-backend) |
| Art. 37 | Registro de operações (RoPA)                        | `LGPD-ROPA-2026-05-15.md` (14 dataflows × purpose × legal basis × retention × recipients × cross-border × security) | EVT-026 + EVT-047 |
| Art. 33 §1º | Transferência internacional + adequação              | §7 privacy_model + CTRL-PRIV-031 (residency pinning fail-CLOSED 451) + attestation bundle `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` (GAP-22 closed 2026-05-15) | EVT-044 + `dev.hugr.corelink.residency.{request_routed,write_rejected_cross_region}.v1` |
| Art. 37 | Registro de operações                                  | Audit events CloudEvents                    | EVT-047 (AUDIT_EVENT)      |
| Art. 38 | Relatório de impacto à proteção de dados (RIPD)       | DPIA por WI HIGH_RISK                       | EVT-045                         |

| Art. 39 (operador) + DPA contratual | Comunicação prévia ≥30 dias para mudança de sub-operador (LGPD não codifica o prazo de 30d explicitamente; obrigação contratual via DPA + GDPR Art. 28(2) industry standard; Lote 10.11.0-ter legal-citation re-validation 2026-05-15 — corrigida atribuição anterior errada "LGPD Art. 27 §4º") | Public sub-processor page auto-gerada de `VENDOR-RISK-REGISTER.md` + `scripts/subprocessor-change-notify.py` (30-day grace clock + CloudEvent `corelink.privacy.subprocessor.notify_required`) + runbook `RB-SUBPROCESSOR-CHANGE.md` + workflow `.github/workflows/subprocessors-sync.yml` | EVT-049 + audit-`<region>` Object Lock 7y |
| Art. 39 | Lista pública de sub-operadores                        | `apps/docs/docs/trust/subprocessors.mdx` (auto-generated from §7 register) | — |

| Art. 41 | Encarregado (DPO)                                      | Interim DPO formally designated (Gustavo Schneiter) per `specs/_compliance/DPO-APPOINTMENT-2026-05-15.md` (LGPD Art. 41 §1º + ANPD Resolução 18/2024); RACI in `DPO-RESPONSIBILITIES-MATRIX.md`; escalation in `RB-DPO-ESCALATION.md`; handoff plan to permanent DPO in `DPO-HANDOFF-PLAN.md` | EVT-032 |

| Art. 48 | Comunicação de incidente à ANPD                        | `RB-BREACH-NOTIF`                           | EVT-017           |

### 4.2 Ponto crítico: tenants que usam CoreLink para dados pessoais

CoreLink pode armazenar blobs contendo dado pessoal do **cliente final do tenant**. Neste caso:

- **Tenant = controlador** do dado final.
- **HuGR = operador** (Art. 5, VII LGPD / Art. 4 GDPR).
- Contrato: DPA (ver `legal/dpa/`).
- Obrigações HuGR: suporte a DSR do tenant, breach notification 48h, minimização/segurança, não uso fora do contrato.

---

## 5. GDPR (UE)

### 5.1 Artigos-chave

| Art.      | Requisito                                          | Como atendemos                            |
|-----------|----------------------------------------------------|--------------------------------------------|
| Art. 5    | Principles                                          | `privacy_model.md §1`                      |
| Art. 6    | Legal basis                                         | §9 privacy                                  |
| Art. 13/14 | Information to data subject                       | Privacy notice `/privacy`                  |
| Art. 15–22 | Rights of data subject (DSRs)                    | §6 privacy                                  |
| Art. 25   | Data protection by design/default                   | Todo este spec framework                    |
| Art. 28   | Processor obligations + ≥30-day notice of sub-processor change | DPA + auto-generated public list (`apps/docs/docs/trust/subprocessors.mdx` from `specs/_compliance/VENDOR-RISK-REGISTER.md`) + 30-day notify hook (`scripts/subprocessor-change-notify.py`) + runbook `RB-SUBPROCESSOR-CHANGE.md` + drift gate `.github/workflows/subprocessors-sync.yml` |
| Art. 30   | Records of processing                               | Audit events                                |
| Art. 32   | Security of processing                              | `security_model.md`                         |
| Art. 33   | Breach notification to DPA (72h)                    | `RB-BREACH-NOTIF`                           |
| Art. 34   | Breach notification to subjects                     | `RB-BREACH-NOTIF`                           |
| Art. 35   | DPIA                                                | Por WI HIGH_RISK                            |
| Art. 44–49 | International transfer                             | SCC; §7 privacy                              |

### 5.2 Schrems II readiness

- TIA (Transfer Impact Assessment) template em `legal/tia/`.
- Sob demanda cliente EU, entregamos TIA + SCC + lista sub-processores.

### 5.3 Full audit rollup (2026-05-15)

- **`specs/_compliance/GDPR-FULL-AUDIT-2026-05-15.md`** — article-by-article audit (Art. 5–49; 25 articles); mirrors LGPD-FULL-AUDIT for the EU regime; pending DPO + Legal sign-off.
- **`specs/_compliance/GDPR-SCC-EXECUTION-2026-05-15.md`** — SCC 2021/914 module selection per data flow (P2P / C2C / P2C / C2P); Schrems II TIA register; EU-US DPF status; Schrems-III contingency plan.
- **`specs/_compliance/GDPR-DPIA-LIBRARY.md`** — Art. 35 DPIA index; trigger criteria (WI metadata + EDPB WP248 nine-criteria + Lighthouse customer triggers); 5 completed DPIAs + per-tenant template.
- **`specs/_runbooks/RB-DSR-GDPR.md`** — internal runbook for Art. 12–22 + Art. 7(3); sister to RB-DSR-LGPD-FULL with GDPR-specific deltas (Art. 19 notification obligation; Art. 21(2) marketing absolute right; Art. 22 N/A declaration; EU SA escalation).
- **`apps/docs/docs/explanation/privacy/gdpr.mdx`** — customer-facing GDPR summary (~1500 words; 11 sections).

---

## 6. Outras regulamentações relevantes

### 6.1 HIPAA (US — saúde)

Opt-in por cliente enterprise. Requer:
- BAA (Business Associate Agreement) assinada.
- Audit anual pelo cliente.
- Controles adicionais: PHI encryption opcional (BYOK), audit logs com 6+ anos.

### 6.2 CCPA/CPRA (Califórnia)

Aligned com GDPR em essência. Direitos de "opt-out of sale" — **CoreLink não vende dados** (declarar na privacy notice).

### 6.3 SOC 3

Report público derivado de SOC 2 Type II. Distribuível livremente (marketing). Planejado pós Type II.

---

## 7. Sub-processor compliance posture

| Sub-processor         | Uso                              | Certificações                                              | DPA link                | Residency            |
|-----------------------|----------------------------------|-------------------------------------------------------------|--------------------------|---------------------|
| Cloudflare (Workers, R2, KV, DO, D1) | Infra core           | SOC 2 Type II, ISO 27001, ISO 27018, PCI-DSS, HIPAA (plano) | CF DPA standard          | Region selectable    |
| Neon                   | Postgres (control plane)          | SOC 2 Type II, HIPAA, ISO 27001                             | Neon DPA                 | US or EU             |
| Grafana Cloud          | Metrics/logs/traces                 | SOC 2 Type II, ISO 27001, HIPAA                             | Grafana DPA              | EU/US selectable     |
| Stripe                 | Billing                             | PCI-DSS Level 1, SOC 1/2                                    | Stripe DPA               | US (some EU)         |
| GitHub                 | Source + CI                         | SOC 1/2, FedRAMP                                             | GitHub DPA               | US                   |
| Sigstore               | Supply chain attestation            | OSS                                                          | —                        | US (public logs)      |
| PagerDuty              | Oncall                              | SOC 2, ISO 27001                                             | PagerDuty DPA            | US/EU                |

> Lista mantida em `apps/docs/docs/trust/subprocessors.mdx` (público, **auto-gerada**
> de `specs/_compliance/VENDOR-RISK-REGISTER.md` via
> `scripts/gen-public-subprocessors.py` + drift gate
> `.github/workflows/subprocessors-sync.yml`) e `legal/sub-processors.md`
> (source). Mudanças disparam o broadcast de 30 dias (LGPD Art. 39 +
> DPA contratual + GDPR Art. 28(2) industry standard; LGPD não codifica
> o prazo de 30d explicitamente — Lote 10.11.0-ter legal-citation
> re-validation 2026-05-15) via `scripts/subprocessor-change-notify.py`
> (CloudEvent `corelink.privacy.subprocessor.notify_required`) e o
> runbook [`RB-SUBPROCESSOR-CHANGE.md`](../_runbooks/RB-SUBPROCESSOR-CHANGE.md).

---

## 8. Evidence collection strategy

### 8.1 Princípio

**Audit evidence is a byproduct, não um chore.** Todo control gera evidence automaticamente via CI/observability. Humano só revisa e assina.

### 8.2 Tipos de EVT consumidos (taxonomia completa em `00_framework.md §35.7`)

| EVT category        | Frequência automática  | Retention           |
|---------------------|-------------------------|---------------------|
| EVT-001          | Cada PR                | 7y (audit-relevant) |
| EVT-005       | Cada PR                | 7y                  |
| EVT-010            | Cada release            | 7y                  |
| EVT-011 | Cada release            | 7y                  |
| EVT-025  | Anual + ad-hoc          | 7y                  |
| EVT-022 | PR que toca INV CRITICAL | 7y                  |
| EVT-019 | Pós incident SEV-1/2 | 7y                  |
| EVT-013 | Mensal (auto)       | 7y                  |
| EVT-047 (AUDIT_EVENT) | Contínuo                | 7y (Object Lock)    |

### 8.3 Audit log schema (CloudEvents)

Ver `observability_model.md §7`. Imutabilidade: R2 Object Lock Governance Mode; hash-chain verificado diariamente (PAT-AUDIT-VERIFY-001).

### 8.4 Access to evidence durante audit

- Auditor externo ganha acesso read-only via SSO + ticket temporário (expira 30d).
- Todas as consultas de auditor logadas em audit trail (meta-audit).
- Evidence redacted por tenant quando aplicável (auditor não precisa ver `tenant_id` ≠ generic).

---

## 9. Gap analysis

> **v0.3.0 update (2026-05-30):** Original §9 rows were folded into the
> canonical 33-GAP register at `specs/_compliance/SOC2-GAP-ANALYSIS.md`
> per the 2026-05-15 R5-3 update. This section is now a delta view:
> status changes since 2026-05-14 readiness score + new gaps surfaced by
> the 2026-05-28 multi-model production audit.
>
> **Full 33-GAP register:** `specs/_compliance/SOC2-GAP-ANALYSIS.md` (FROZEN
> 2026-05-14 snapshot). **New security gaps found 2026-05-28** are NOT in
> that register — they are itemized below as GAP-34..GAP-38.

| Gap ID | Descrição | Source | Status (2026-05-30) |
|---|---|---|---|
| GAP-01 | DPO formal vs. interim | `DPO-APPOINTMENT-2026-05-15.md` | In progress — interim designated; 90-day handoff plan to permanent DPO per `DPO-HANDOFF-PLAN.md` |
| GAP-02 | BYOK FIPS attestation letters from all 4 KMS providers | `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` | **Blocking Type I** — in-flight; target D+30 |
| GAP-05 | ISO 27001 SoA | `specs/_audits/iso27001-soa.csv` | Partial — 2022 crosswalk done; ISMS not established |
| GAP-06 | Legal hold process automation | `specs/_compliance/` | Open (T+3m) |
| GAP-15 | Cold restore drill execution against production | `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` | DESIGNED — not executed; production data restore untested |
| GAP-34 | P1-1: BYOK AAD non-canonical serialization (Azure/GCP/Vault use `serde_json` not `serde_jcs` RFC 8785) → potential DEK unrecoverability / data-loss under cross-arch deployment | Multi-model audit P1-1: `crates/corelink-byok/src/byok_{azure,gcp,vault}.rs` | **Open — fix required before Type I fieldwork** (data integrity risk) |
| GAP-35 | P1-3: Audit-chain verifier uses non-constant-time `!=` compare (`verifier.rs:162`) → timing oracle | Multi-model audit P1-3: `crates/corelink-audit-chain/src/verifier.rs:162` | **Open — fix required before Type I fieldwork** |
| GAP-36 | P1-4: PAT token_id compare non-constant-time (`verify.rs:54`) → token-id enumeration oracle; violates module's own INV-AUTH-CONSTANT-TIME-COLD-PAD | Multi-model audit P1-4: `crates/corelink-pat/src/verify.rs:54` | **Open — fix required before Type I fieldwork** |
| GAP-37 | P1-5: Rate-limit/quota race — free-tier quota race (concurrent writes both pass check); unbounded per-IP buckets | Multi-model audit P1-5: `crates/corelink-billing/src/quota/cas/cas.rs` | Open — T+1m hardening sprint |
| GAP-38 | P2: Audit export endpoint 404 in live probe — `/v1/audit/<tenant>/export` not mounted or path wrong | Multi-model audit §9 follow-up: deployed path unknown | Open — P2 follow-up audit |

**GAPs closed since 2026-05-14 snapshot:**

| Gap ID | Closed | How |
|---|---|---|
| GAP-04 | 2026-05-14 | SOC 2 readiness score delivered + sealed |
| GAP-07 | 2026-05-14 | Drata pipeline live (90.7% auto-coverage) |
| P0-1..P0-6 (prod wiring) | 2026-05-30 | WP-I1 commit `0f34a2c3` wired composed router; live smoke verified |

---

## 10. Gap to SOC 2 Type I readiness — honest checklist

> This section is new in v0.3.0. It answers the question an auditor asks at
> a Type I readiness assessment: *"What work remains before we can open the
> observation window?"* A Type I audit is a design-effectiveness review —
> the auditor checks whether controls are **designed and implemented**, not
> whether they have been operating for 6 months (that's Type II). The window
> can open once the blocking items below are resolved.

### 10.1 Must-close before Type I fieldwork (blocking)

| # | Gap | Control area | Crate / path | Notes |
|---|---|---|---|---|
| 1 | GAP-02: BYOK FIPS attestation letters missing | C1.1 Confidentiality | `crates/corelink-byok/src/` | AWS/GCP/Azure/Vault must supply written FIPS 140-2/3 attestation letters for key operations. Without these CC6.1 + C1.1 earn at best a qualified opinion. |
| 2 | GAP-34: BYOK AAD non-canonical serialization (P1-1) | PI1.2, C1.1 | `crates/corelink-byok/src/byok_azure.rs`, `byok_gcp.rs`, `byok_vault.rs` | `serde_json::to_vec()` not `serde_jcs::to_vec()` (RFC 8785). Cross-arch DEK unrecoverability = data-loss risk. Auditor will flag as design deficiency in C1.1 + PI1.2. |
| 3 | GAP-35: Audit-chain verifier non-constant-time (P1-3) | CC7.1, PI1.5 | `crates/corelink-audit-chain/src/verifier.rs:162` | `!=` compare on chain hashes is a timing oracle. Auditor will flag as security design deficiency. Fix: use `subtle::ConstantTimeEq` (already used in exporter + archive). |
| 4 | GAP-36: PAT token_id compare non-constant-time (P1-4) | CC6.1 | `crates/corelink-pat/src/verify.rs:54` | Violates the module's own invariant INV-AUTH-CONSTANT-TIME-COLD-PAD. Auditor will flag as CC6.1 design deficiency. |
| 5 | GAP-01: Permanent DPO not yet appointed | P8.1, CC1.2 | `specs/_compliance/DPO-APPOINTMENT-2026-05-15.md` | Interim is acceptable for Type I if the handoff plan is documented (it is). Blocks EU enterprise customers. |

### 10.2 Must-close before Type I observation window opens (pre-fieldwork)

| # | Gap | Control area | Notes |
|---|---|---|---|
| 6 | Evidence collection baseline: 0 operating days | All CC criteria | Type I is design-only so this is not a hard blocker, but the auditor will note that no operating history exists. First quarterly review due T+1m. |
| 7 | CC6.3 quarterly access review (first run) | CC6.3 | Must be executed once to demonstrate the process is operational, not just designed. |
| 8 | P2.1 consent event log (first real events) | P2.1 | Consent ledger crate deployed; needs production consent events to show evidence. |
| 9 | GAP-15: Cold restore drill execution | A1.3 | Spec + cadence documented; needs one executed drill log to close A1.3 RED. |
| 10 | Advisor pool engagement (GAP-04 + GAP-05) | CC1.2, CC1.4 | Compliance Officer + AppSec advisor needed for governance independence. First quarterly review with advisor countersignature closes CC1.2 YELLOW. |

### 10.3 Required only for Type II (not blocking Type I)

| # | Gap | Notes |
|---|---|---|
| 11 | 6-month operating evidence window | Type I = design review only. Type II requires continuous evidence of control operation. |
| 12 | GAP-37: Quota race fix | P1-5 rate-limit hardening. Affects cost control, not Type I design review. |
| 13 | GAP-38: Audit export endpoint 404 | PI1.3 evidence artifact. P2 — path routing fix. |
| 14 | ISO 27001 ISMS establishment | DEFERRED per roadmap. SOC 2 + LGPD/GDPR compliance is the current gate. |
| 15 | Webhook replay window (P1-2) | `apps/signup-worker/src/webhooks/clerk.ts:270-294` missing `svix-timestamp` validation. Not a SOC 2 TSC gap per se, but auditor will flag under CC6.6. Fix before T+1m. |

### 10.4 Control-to-crate reference index

This index lists the primary crate backing each compliance-relevant control.
Every cited path has been verified to exist on disk as of 2026-05-30.

| Control area | Primary crate | Key file | SOC 2 criteria |
|---|---|---|---|
| Tenant isolation (HMAC prefix) | `crates/tenant-path` | `src/prefix.rs` | CC6.2, CC6.6, PI1.4 |
| PAT authentication (Argon2id) | `crates/corelink-auth`, `crates/corelink-pat` | `src/verify.rs` | CC6.1 |
| Audit chain (BLAKE3 Merkle, CloudEvents) | `crates/corelink-audit-chain` | `src/chain.rs`, `src/event.rs` | CC4.1, CC7.1, PI1.5 |
| DSR rights orchestration (6 rights) | `crates/corelink-dsr` | `src/lib.rs` | P5.1, P5.2 |
| Erasure pipeline (12 backends) | `crates/corelink-privacy-erasure-worker` | `src/lib.rs` | C1.2, P4.3 |
| Erasure attestation (Ed25519/FIPS 186-5) | `crates/corelink-erasure-attestation` | `src/attestation.rs` | C1.2, PI1.5 |
| BYOK key management (4 providers) | `crates/corelink-byok` | `src/byok_core/`, `src/byok_aws.rs`, `src/byok_gcp.rs`, `src/byok_azure.rs`, `src/byok_vault.rs` | C1.1, CC6.1 |
| Privacy umbrella (consent/breach/notice/DSR/residency) | `crates/corelink-privacy` | `src/lib.rs` | P1.1, P2.1, P3.1, P4.1, P6.1, P8.1 |
| CAS (content-addressable storage) | `crates/corelink-cas` | `src/` | PI1.1, PI1.2 |
| Action cache | `crates/corelink-ac` | `src/` | PI1.3 |

---

## 11. Referências

### 11.1 Frameworks

- **AICPA Trust Services Criteria 2017 (rev. 2022)** — SOC 2.
- **ISO/IEC 27001:2022** + Anexo A.
- **NIST SP 800-53 rev. 5** — para crosswalk.
- **CIS Controls v8** — para crosswalk com ISO.
- **Cloud Security Alliance CCM v4** — shared responsibility.

### 11.2 Legislação

- LGPD — Lei 13.709/2018.
- GDPR — Regulamento (UE) 2016/679.
- CCPA/CPRA.
- HIPAA Security Rule (45 CFR 164.302-318).

### 11.3 Auditores potenciais

- Big 4 (Deloitte, KPMG, EY, PwC) — SOC 2.
- Schellman, A-LIGN — SOC 2 + ISO 27001, experiência SaaS.
- Prescient Assurance — SOC 2 rápido.

### 11.4 Ferramentas de evidence automation

- **Drata** — popular em SaaS early-stage. Active integration: `specs/_compliance/DRATA-INTEGRATION-COVERAGE.md` (90.7% strict / 96.4% dashboard green as of 2026-05-14).
- **Vanta** — competitor, bom para multi-framework.
- **Secureframe** — competitor.
- **OneTrust** — enterprise, privacy-first.

### 11.5 Key audit artifacts (v0.3.0 additions)

- `specs/_audits/sealed/2026-05-14-soc2-readiness-score.md` — quantitative readiness scorecard (83.7% internal / 96.4% Drata); 43 criteria; 26 GREEN / 15 YELLOW / 2 RED.
- `specs/_compliance/SOC2-GAP-ANALYSIS.md` — canonical 33-GAP register (FROZEN 2026-05-14).
- `specs/_audits/2026-05-28-multimodel-prod-readiness-audit.md` — multi-model brutal audit + §9 SHIPPABLE addendum (2026-05-30).
- `specs/_audits/iso27001-soa.csv` — ISO 27001:2022 SoA (2022 baseline; 93 controls).
- `specs/_audits/matrix-stride-ctrl.csv` — STRIDE threat model cross-referenced to CTRLs.
- `specs/_audits/lia-template.md` — LIA template for GDPR Art. 6(f) legitimate interest.

---

**Fim de COMPLIANCE-MATRIX v0.3.0.** Mudanças em frameworks externos (ex: SOC 2 2024 revision) requerem update deste doc dentro de 90d da publicação.
