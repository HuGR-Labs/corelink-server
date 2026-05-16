---
id: "AUDIT-2026-05-16-LFPDPPP-MX-LEGAL-REVIEW-PACKAGE"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "S-11"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: ["Privacy Officer", "MX Attorney (TBD — to engage)"]
supersedes: null
superseded_by: null
parent_wi: "WI-S11-004"
wave: "wave-23"
inv: []
references:
  - "specs/04_sprints/S11/work_items/WI-S11-004-privacy-notice-versioning-3-locales-diff-publication.md"
  - "specs/_audits/2026-05-15-s11-legal-citation-revalidation.md"
  - "specs/_audits/2026-05-15-s11-round-2-validation.md"
  - "specs/_audits/2026-05-15-customer-breach-notification-templates.md"
  - "specs/_audits/2026-05-15-debt-register.md"
  - "legal/privacy-notice/v1.0.0/es-MX.md"
  - "legal/privacy-notice/v1.0.0/metadata.yaml"
  - "legal/privacy-notice/REVIEW_PROCESS.md"
  - "legal/sub-processors.md"
  - "legal/legal-externo-engagement-contract.md"
  - "docs/customer-comm/breach-notification/v1.0.0/es/audit-chain-integrity-incident.md"
  - "docs/customer-comm/breach-notification/v1.0.0/es/dsr-pipeline-temporary-degradation.md"
  - "docs/legal/lfpdppp-mx-engagement-letter-template.md"
tags: ["audit", "s11", "wave-23", "lfpdppp", "mexico", "legal-review-package", "mx-attorney", "arco", "engagement-prep"]
---

# LFPDPPP MX Legal Review Package — Engineering-Side Artifacts for Attorney Review

> **Status:** Engineering-side preparation complete. Attorney engagement itself is **user-bound** (Gustavo Schneiter retains the MX attorney; no agent can execute the retention). This wave-23 package consolidates every artifact a Mexican attorney needs to render a written opinion on CoreLink's LFPDPPP compliance posture. The attorney review itself is **DEFERRED to wave-23-DEFER → wave-26 (review absorption pre-GA)** per `2026-05-15-debt-register.md DEBT-025` (added by this wave).

---

## 0. Executive summary

CoreLink ships an es-MX privacy notice (`legal/privacy-notice/v1.0.0/es-MX.md`) + 2 es breach-notification templates (`docs/customer-comm/breach-notification/v1.0.0/es/{audit-chain-integrity-incident,dsr-pipeline-temporary-degradation}.md`) anchored on LFPDPPP statutory citations. All three artifacts carry `legal_review_status: PENDING_MX_ATTORNEY` (or equivalent `mx_attorney: TBD` in metadata) flagged in front-matter / metadata.yaml. The wave-17-ter audit (`2026-05-15-s11-legal-citation-revalidation.md §3 D-8`) identified one LFPDPPP citation-mapping drift that was **conservatively deferred** to outside MX legal review rather than self-resolved.

This package consolidates §1-§8 for a single self-contained handoff to a Mexican attorney. The deliverable from the attorney is a **written opinion** on the §1-§7 questions, estimated at **8-12 attorney hours**. The package does not engage; it pre-packages every reference the attorney needs so the engagement can start at the substantive review (not at orientation).

**Pending user action:** engage a Mexican attorney with `docs/legal/lfpdppp-mx-engagement-letter-template.md` + this audit as the scoping packet.

---

## §1 — Statutory citations inventory (every LFPDPPP Art. reference in our codebase)

The following table enumerates every LFPDPPP citation that ships in customer-visible or audit-bound artifacts. Citations in internal-only audit documents (e.g. this file, the legal-citation-revalidation audit, debt-register) are excluded — only the artifacts a regulator or customer could surface in a PPD (procedimiento de protección de derechos) before INAI are inventoried.

| Artifact | Path | LFPDPPP citation | Use site |
|---|---|---|---|
| Privacy notice (es-MX) | `legal/privacy-notice/v1.0.0/es-MX.md` | **Art. 8** | §4 "Consentimiento (analítica)" base legal row |
| Privacy notice (es-MX) | `legal/privacy-notice/v1.0.0/es-MX.md` | **Art. 10 I** | §4 base legal "Ejecución contractual" |
| Privacy notice (es-MX) | `legal/privacy-notice/v1.0.0/es-MX.md` | **Art. 10 III** | §4 base legal "Obligación legal" |
| Privacy notice (es-MX) | `legal/privacy-notice/v1.0.0/es-MX.md` | **Art. 10 VI** | §4 base legal "Interés legítimo" — *see §7 Q3 below; LFPDPPP Art. 10 VI is "emergency" exception, not generic legitimate-interest analog of GDPR Art. 6(1)(f)* |
| Privacy notice (es-MX) | `legal/privacy-notice/v1.0.0/es-MX.md` | **Arts. 22-36 (ARCO)** | §8 "Derechos del Titular (LFPDPPP Arts. 22-36 / ARCO)" header + body |
| Breach template (audit-chain) | `docs/customer-comm/breach-notification/v1.0.0/es/audit-chain-integrity-incident.md` | **Art. 20-21** | front-matter `legal_basis`, §opening paragraph, §3 status table, §closing references |
| Breach template (audit-chain) | `docs/customer-comm/breach-notification/v1.0.0/es/audit-chain-integrity-incident.md` | **Arts. 22-26 (ARCO subset)** | §4 customer rights body |
| Breach template (DSR-degradation) | `docs/customer-comm/breach-notification/v1.0.0/es/dsr-pipeline-temporary-degradation.md` | **Art. 20-21** | front-matter `legal_basis`, §opening paragraph |
| Breach template (DSR-degradation) | `docs/customer-comm/breach-notification/v1.0.0/es/dsr-pipeline-temporary-degradation.md` | **Arts. 22-26** | §1 "Naturaleza" row, §4 customer rights body |
| Breach template (DSR-degradation) | `docs/customer-comm/breach-notification/v1.0.0/es/dsr-pipeline-temporary-degradation.md` | **Art. 32** | §4 "plazo de respuesta 20 días" computation reference |
| Breach template (DSR-degradation) | `docs/customer-comm/breach-notification/v1.0.0/es/dsr-pipeline-temporary-degradation.md` | **Capítulo VII (PPD ante INAI)** | §5 escalation reference |

**Totals:** 3 customer-visible artifacts (1 privacy notice + 2 breach templates); 11 citation sites covering Arts. **8, 10, 20, 21, 22-26, 32, 36** + Capítulo VII (PPD).

**Out-of-scope:** Reglamento de la LFPDPPP (Diario Oficial 2011-12-21), INAI Lineamientos de Aviso de Privacidad (2013-01-17), and Mexican Constitution Art. 16 §2 (right to data protection) are *not* directly cited in any customer-visible artifact at v1.0.0. Attorney may opine on whether any of these need additional citation in §4 of the privacy notice for ARCO defensibility.

---

## §2 — Breach notification templates (es locale; statutory anchor LFPDPPP Art. 20-21)

Two es breach-notification templates ship under `docs/customer-comm/breach-notification/v1.0.0/es/`:

1. **`audit-chain-integrity-incident.md`** — fired on `corelink.audit.export_verify_failed.v1` (SEV-0 / SEV-1) and `corelink.audit.export_integrity_failed.v1`; jurisdiction anchor MX.
2. **`dsr-pipeline-temporary-degradation.md`** — fired on `corelink.privacy.statuspage_publish_failed.v1` and `corelink.privacy.dsr_sla_breach.v1` (SEV-1, loss-of-availability ≥ 24h); jurisdiction anchor MX.

Both templates carry:

- `legal_basis: "LFPDPPP Art. 20-21 ... cláusula primaria Art. 21 (análisis de causas + medidas correctivas tras vulneración); concordancia: GDPR Art. 34 / LGPD Art. 48"`
- `legal_review_status: PENDING_MX_ATTORNEY`
- `native_speaker_reviewed: true`
- `legal_review_caveat: "LFPDPPP Art. 21 anchor confirmed by user task spec wave-18; distinct from LFPDPPP Art. 10 VI emergency-exception (deferred legal review per wave-17-ter audit D-8). Outside MX attorney still to sign off pre-GA per WI-S11-004 §6.1.4."`

The wave-18 audit `2026-05-15-customer-breach-notification-templates.md` documents the design rationale for the Art. 20-21 jointly + Art. 21 primary anchor choice. The wave-23 attorney review must confirm or correct this anchor framing (see §7 Q1 below).

**Native speaker register** (wave-18 audit §2.1): es-MX register; canonical terms `titular`, `vulneración`, `responsable`, `encargado`, `ARCO` per LFPDPPP statutory prose; explicitly distinct from Brazilian `controlador/operador` and Iberian-Spanish lexicon variants.

**Detection-to-notification SLA** (wave-18 audit §2.1 jurisdictional comparison table): LFPDPPP Art. 20 caput says *"de forma inmediata"* — CoreLink internal commitment is ≤ 72h (parity with GDPR Art. 34). Attorney to confirm 72h is *"inmediata"* under prevailing INAI interpretation.

---

## §3 — Privacy notice (es-MX; WI-S11-004)

The es-MX privacy notice at `legal/privacy-notice/v1.0.0/es-MX.md` (v1.0.0; published 2026-05-13) is the LGPD-primary → GDPR-secondary → LFPDPPP-tertiary cascade described in `WI-S11-004 §1` (PT-BR LGPD primary + EN GDPR/CCPA + ES LATAM tertiary). The es locale anchors the LATAM expansion market and is **the canonical LFPDPPP-compliant data subject notification surface** for any Mexican tenant onboarded to CoreLink.

Sections covered (13 sections + DPO contact):

1. Identidad y datos del responsable (§1)
2. Categorías de datos personales (§2 — 5 categories)
3. Finalidades del tratamiento (§3 — 4 purposes)
4. Base legal (§4 — Mexican LFPDPPP × GDPR cross-mapping table)
5. Sub-procesadores (§5 — 4 entities: Cloudflare, Stripe, Neon, Grafana Labs; ≥30d notice on change)
6. Transferencias internacionales (§6 — SAM/WNAM/ENAM/WEUR; SCCs + adequacy)
7. Retención (§7 — 5 categories)
8. Derechos ARCO + portabilidad + revocación (§8 — Arts. 22-36)
9. Medidas de seguridad (§9)
10. Cookies (§10)
11. Menores (§11 — B2B; no minors)
12. Cambios (§12 — semver bump per WI-S11-004)
13. Contacto INAI (§13)

**Metadata** (`legal/privacy-notice/v1.0.0/metadata.yaml`):

```yaml
locales:
  es-MX:
    file: "es-MX.md"
    native_speaker_reviewed: true
    legal_review_evt-044_path: "evidence-legal/v1.0.0-es-MX.pdf"
    notice_text_hash: "[computed by CI hook — placeholder]"
legal_review:
  status: "pending_pre_ga"
  mx_attorney: "[TBD — contratar pré-GA conforme WI-S11-004 §6.1.4]"
```

Attorney must (a) verify Art. 8 / Art. 10 I, III, VI base-legal mapping in §4, (b) confirm Arts. 22-36 ARCO header in §8 is statutorily complete (we cover A, R, C, O + portabilidad + revocación — portabilidad is not strictly in LFPDPPP ARCO but is GDPR Art. 20; attorney to confirm Mexican defensibility of bundling), (c) sign EVT-044 PDF for upload to R2 `evidence-legal/v1.0.0-es-MX.pdf`, (d) confirm INAI contact details in §13.

---

## §4 — DSR erasure flow (LFPDPPP Art. 22-26 ARCO rights compliance)

ARCO (Acceso, Rectificación, Cancelación, Oposición) rights are implemented in CoreLink via the WI-S11-001 / WI-S11-002 DSR pipeline:

| ARCO right | LFPDPPP Art. | CoreLink endpoint | SLA (LFPDPPP Art. 32) | Implementation |
|---|---|---|---|---|
| **A**cceso | Art. 22 | `POST /v1/privacy/dsr/access` | 20 días hábiles | WI-S11-001 §3 — JWT receipt + MFA step-up |
| **R**ectificación | Art. 24 | `POST /v1/privacy/dsr/rectification` | 15 días hábiles | WI-S11-001 §3 |
| **C**ancelación | Art. 25 | `POST /v1/privacy/dsr/erasure` | 15 días hábiles | WI-S11-002 (erasure worker; 10 backends; 24h verification reports) |
| **O**posición | Art. 27 | `POST /v1/privacy/dsr/objection` | 20 días hábiles | WI-S11-001 §3 |
| Portabilidad (GDPR Art. 20 analog) | (not LFPDPPP; bundled per privacy notice §8) | `POST /v1/privacy/dsr/portability` | 20 días hábiles | WI-S11-001 §3 — JSON structured output |
| Revocación del consentimiento | Art. 8 (revocability) | `POST /v1/privacy/dsr/consent-revoke` | Immediate (audit + WI-S11-003 stale_consent flag) | WI-S11-003 §6 — symmetric revoke |

**Cancelación implementation** (`crates/corelink-privacy-erasure`, WI-S11-002): 10-backend matrix (Neon control plane + R2 audit-region + R2 audit-secondary + KV consent ledger + DO state + D1 dsr-tickets + Stripe customer + Grafana metrics + Loki logs + status page) with 24h verification reports surfaced to `dsr_tickets.verification_evidence` column. Erasure is **fail-CLOSED**: if any backend verification fails, the ticket stays open until manual remediation (no silent partial erasure).

**LFPDPPP Art. 26 retention exception**: where data is legally required to be retained (e.g. fiscal records 5y, audit records 7y per Mexican Código Fiscal de la Federación Art. 30), the erasure response surfaces the retention basis to the titular. Attorney to confirm wording in §7 of the privacy notice (retention table) is sufficient under LFPDPPP Art. 26 carve-out doctrine.

**Cross-tenant isolation**: `INV-LGPD-AUTO-SUSPEND-FORBIDDEN` (registry §3.12) ensures erasure of tenant A's data never auto-suspends tenant B's service; verification is per-tenant.

---

## §5 — Data residency MX (Region::Iad fallback acceptable per LFPDPPP — no MX data residency required)

**LFPDPPP residency posture**: Unlike LGPD (which carries Art. 33 §1 cross-border transfer requirements) or GDPR (Art. 44-50 international transfer chapter), the LFPDPPP **does not impose an in-country data residency requirement** for Mexican titulares. Cross-border transfers are governed by LFPDPPP Art. 36 (Encargado disclosure — see §6 below) and Arts. 66-70 of the Reglamento (notice of cross-border transfer + safeguard equivalence).

**CoreLink posture for MX tenants**:

- CoreLink ships 4 regions (`sam` Brazil; `enam` US east; `wnam` US west; `weur` EU). There is no MX region.
- MX tenants are pinned (via `tenant.primary_region`) to either `enam` (US east — Region::Iad / N. Virginia for R2) or `sam` (Brazil — São Paulo for R2). The recommended default for MX tenants is `enam` (lowest latency from MX City).
- The privacy notice §6 (Transferencias internacionales) explicitly discloses: "Brasil (SAM): región primaria; Estados Unidos (WNAM/ENAM): Cloudflare, Stripe, Neon; Unión Europea (WEUR): Cloudflare EU, Grafana Labs. Las transferencias internacionales están amparadas por cláusulas contractuales estándar (SCCs) y mecanismos de adecuación reconocidos."
- Region::Iad fallback acceptable because LFPDPPP does not require in-country processing; the cross-border notice in §6 of the privacy notice + the LFPDPPP Art. 36 sub-processor disclosure in §5 of the privacy notice + the DPA Amendment Template (`legal/dpa-residency-amendment.md`) cover the statutory hooks.

**Attorney to confirm**:

- That the §6 cross-border transfer disclosure is sufficient under Reglamento Art. 67 (cross-border transfer notice form) and Art. 68 (Encargado/Responsable contract obligations).
- That SCCs as the safeguard mechanism are recognized by INAI as a valid Art. 36 / Reglamento Art. 70 mechanism, or whether a separate Mexican-form data transfer addendum is required for tenants headquartered in MX.

---

## §6 — Sub-processor disclosure (LFPDPPP Art. 36 Encargado disclosure)

**LFPDPPP Art. 36**: "Cuando el responsable pretenda transferir los datos personales a terceros nacionales o extranjeros, distintos del encargado, deberá comunicar a éstos el aviso de privacidad y las finalidades a las que el titular sujetó su tratamiento."

CoreLink's sub-processor disclosure ships at `legal/sub-processors.md` (4 entities) and is mirrored in the es-MX privacy notice §5. The notification cadence is ≥30 days advance notice for any change (LFPDPPP does not specify a fixed window; ≥30d is the GDPR-EDPB norm, surplus to LFPDPPP Art. 36 minimum which is "comunicar a éstos el aviso de privacidad" — point-in-time disclosure rather than advance notice).

**Sub-processors** (mirrored in privacy notice §5):

| Encargado | Role | Region | Cert basis | DPA URL |
|---|---|---|---|---|
| Cloudflare, Inc. | Infraestructura (Workers / R2 / KV / DO / D1 / Pages / Email) | Multi-region (tenant-pinned) | SOC 2 Type II + ISO 27001 + ISO 27018 + PCI-DSS L1 + HIPAA | https://www.cloudflare.com/cloudflare-customer-dpa/ |
| Neon, Inc. | Postgres (control plane — `dsr_tickets`, `account`, `tenant`, `billing`) | US or EU (selectable) | SOC 2 Type II + ISO 27001 + HIPAA | https://neon.tech/dpa |
| Grafana Labs | Observabilidad (Mimir / Loki / Tempo / Grafana) | EU or US (selectable) | SOC 2 Type II + ISO 27001 | (DPA URL in `legal/sub-processors.md`) |
| Stripe, Inc. | Procesamiento de pagos | US/EU | PCI-DSS L1 + SOC 1 + SOC 2 + ISO 27001 | (DPA URL in `legal/sub-processors.md`) |

**Attorney to confirm**:

- That LFPDPPP Art. 36's "comunicar a éstos el aviso de privacidad" obligation is satisfied by the per-Encargado DPA URL + the cross-reference in privacy notice §5.
- That ≥30 days advance notice on change is *sufficient* under LFPDPPP Art. 36 (or whether INAI guidance mandates a specific minimum window).
- That a Mexican tenant cannot, under the Reglamento Art. 67 cross-border transfer regime, claim that the §5 disclosure is incomplete because the Cloudflare DPA is in English (and not Spanish). If so, recommend whether (a) translated DPA URLs must be linked, or (b) a Spanish-summary boilerplate per Encargado must accompany the §5 table.

---

## §7 — Open questions for attorney

Three substantive legal questions requiring attorney opinion. These are the questions a Mexican attorney needs to render written answers to before WI-S11-004 §6.1.4 (legal local review per locale) can be marked CLOSED.

### Q1 — Art. 21 standalone vs Art. 20-21 jointly per breach notification

**Background**: The wave-18 audit `2026-05-15-customer-breach-notification-templates.md §2.1` concluded that `legal_basis: "LFPDPPP Art. 20-21 ... cláusula primaria Art. 21"` is the correct framing because Art. 20 establishes the *occurrence-trigger* (vulneración that significantly affects derechos patrimoniales o morales) and Art. 21 establishes the *content + corrective-action requirement* (análisis de causas + medidas correctivas, preventivas y de mejora). Both templates (`audit-chain-integrity-incident.md`, `dsr-pipeline-temporary-degradation.md`) cite Art. 20-21 jointly with Art. 21 primary.

**Question to attorney**:

> Is the joint Art. 20-21 anchor with Art. 21 as primary correct, or should the templates anchor on Art. 21 alone (with Art. 20 cited only as occurrence-trigger context, not as joint legal basis)? Conversely, should Art. 20 be the primary anchor with Art. 21 cited only as content requirement? Provide the canonical INAI interpretation if one exists in the jurisprudence (Sala Superior INAI cases or *criterios* publicados).

### Q2 — Art. 10 VI emergency exception scope (deferred wave-17-ter D-8)

**Background**: The wave-17-ter audit identified that the es-MX privacy notice §4 maps "Interés legítimo" to **LFPDPPP Art. 10 VI** as a parallel of GDPR Art. 6(1)(f) legitimate-interest basis. However, LFPDPPP Art. 10 VI literal text is the **emergency exception** — *"Cuando exista una situación de emergencia que potencialmente pueda dañar a un individuo en su persona o en sus bienes"* — and is **not** a generic legitimate-interest analog. The wave-17-ter audit conservatively deferred this citation mapping to outside legal review (D-8 deferral).

**Question to attorney**:

> Is mapping CoreLink's "monitoreo de abuso, detección de fraude, seguridad de la plataforma" to LFPDPPP Art. 10 VI defensible, or must this purpose be re-anchored on a different LFPDPPP basis (Art. 10 V "para fines previstos en este artículo"? Art. 4 §"finalidades compatibles con la finalidad inicial"? Reglamento Art. 16 §2 secondary purposes?)? If Art. 10 VI is not defensible for these purposes, propose the correct citation and supply the corrected es-MX privacy notice §4 row text (Spanish, formal-legal register).

### Q3 — Cross-border transfer notice format (Reglamento Art. 67)

**Background**: CoreLink discloses cross-border transfers (SAM/ENAM/WNAM/WEUR) in privacy notice §6, citing "SCCs y mecanismos de adecuación reconocidos" generically. Reglamento Art. 67 specifies the form requirements of the cross-border notice: identity of recipient, country, finalidad, and a copy of the international agreement.

**Question to attorney**:

> Does CoreLink's §6 disclosure satisfy Reglamento Art. 67's form requirements, or must each cross-border transfer be itemized per-recipient (e.g. one row per Encargado × destination country)? If itemization is required, propose the corrected §6 table layout (Spanish). Also opine on whether the *"mecanismos de adecuación reconocidos"* language requires INAI to have formally recognized the adequacy of US/EU/Brazil — and if so, whether such recognition exists at the date of this engagement.

### (Optional) Q4 — INAI complaint mechanism wording in §13 of privacy notice

**Background**: §13 of the privacy notice currently says: `México: Instituto Nacional de Transparencia, Acceso a la Información y Protección de Datos Personales (INAI) — www.inai.org.mx`. The Capítulo VII PPD path (procedimiento de protección de derechos) is cited in the dsr-pipeline-temporary-degradation breach template but not in the privacy notice §13.

**Question to attorney**:

> Should §13 explicitly cite Capítulo VII LFPDPPP (PPD) + Capítulo VIII (procedimiento de verificación) as the formal complaint mechanism, or is the current INAI contact-only disclosure sufficient under LFPDPPP Art. 16 fracción VII (right to lodge a complaint)?

---

## §8 — Engagement scope (attorney deliverable; estimated 8-12 attorney hours)

### 8.1 Deliverable

A **written legal opinion** addressing §1-§7 of this package, with:

- §1 — confirmation that the statutory citations inventory is complete (or correction with missing citations).
- §2 — sign-off on the breach notification templates as LFPDPPP Art. 20-21 compliant, with redlines if any.
- §3 — sign-off on the es-MX privacy notice as LFPDPPP-compliant, with redlines if any. Output is the **signed EVT-044 PDF** uploaded to R2 `evidence-legal/v1.0.0-es-MX.pdf` per `legal/privacy-notice/REVIEW_PROCESS.md §2.3`.
- §4 — sign-off on the DSR erasure flow as ARCO-compliant (Arts. 22-26 + Art. 32 SLA + Art. 26 retention exception).
- §5 — confirmation that no MX data residency is required (Region::Iad fallback acceptable).
- §6 — confirmation that the sub-processor disclosure satisfies LFPDPPP Art. 36 (or correction with the missing element).
- §7 — written answers to Q1 (Art. 21 anchor), Q2 (Art. 10 VI scope; resolves wave-17-ter D-8), Q3 (Reglamento Art. 67 cross-border form), Q4 (Capítulo VII reference in §13; optional).

### 8.2 Effort estimate

| Activity | Hours |
|---|---|
| Read package (§1-§7) | 1.0-2.0 |
| Verify citations inventory + statutory cross-check | 1.0-2.0 |
| Q1 (Art. 21 anchor) — review breach templates + render opinion | 1.0-1.5 |
| Q2 (Art. 10 VI) — review privacy notice §4 + render opinion + propose corrected text | 2.0-3.0 |
| Q3 (Reglamento Art. 67) — review privacy notice §6 + render opinion + propose corrected table if needed | 1.5-2.5 |
| Q4 (Capítulo VII reference, optional) | 0.5-1.0 |
| Draft + sign EVT-044 PDFs (1 per artifact: 1 privacy notice + 2 breach templates = 3 PDFs) | 1.0-1.5 |
| **Total** | **8.0-13.5 hours** |

The package is designed so the attorney spends ≥90 % of their time on substantive opinion (Q1-Q4), not on orientation. The §1-§6 sections are factual prep; only §7 is substantive.

### 8.3 Retention budget

To be set per `docs/legal/lfpdppp-mx-engagement-letter-template.md §3` (Owner decision; not pre-set in this audit). Suggested budget range USD 2,500-4,500 at MX-attorney market rates for a 10-hour engagement with 2-week turnaround, plus 1-2 hours follow-up reserve for Q&A clarification.

### 8.4 Timeline

| Step | Owner | Target |
|---|---|---|
| Send engagement letter to candidate attorney | Gustavo Schneiter (user) | wave-23 close (T+0) |
| Sign retention contract | Gustavo Schneiter + attorney | T+7d |
| Deliver this package + privacy notice + breach templates | Gustavo Schneiter | T+7d |
| Attorney review (8-12h spread over) | Attorney | T+21d |
| Receive written opinion + 3 EVT-044 PDFs | Gustavo Schneiter | T+21d |
| Absorb opinion into spec corpus (privacy notice §4 / §6 corrections if needed; metadata.yaml `legal_review.mx_attorney` update; metadata `notice_text_hash` re-compute on any text change; DEBT-025 → CLOSED) | Orchestrator (wave-26) | T+28d (wave-26 absorption pre-GA) |

**DEBT-025** tracks the open state of this engagement; will be CLOSED in wave-26 when the attorney opinion is absorbed.

---

## §9 — Sign-off

**Engineering-side preparation**: COMPLETE (this document).

**Pending user action**: engage MX attorney with `docs/legal/lfpdppp-mx-engagement-letter-template.md` + this audit packet.

**No code changes** in this wave; doc-only.

**Quality gates** (per task charter):

- `python3 scripts/validate_specs.py` → PASS (zero spec regression)
- `python3 scripts/validate_references.py` → PASS

**Cross-references**:

- WI-S11-004 §6.1.4 (legal local review per locale; native-speaker + legal review SLA)
- DEBT-025 (debt-register §3 — target wave-23-DEFER for engagement, wave-26 for absorption)
- `2026-05-15-s11-legal-citation-revalidation.md §3 D-8` (original LFPDPPP Art. 10 VI deferral, now resolved by Q2 in this package)
- `2026-05-15-customer-breach-notification-templates.md §2.1` (Art. 21 anchor design rationale, now confirmed by Q1 in this package)
- `legal/privacy-notice/REVIEW_PROCESS.md §2.3` (EVT-044 PDF sign-off SOP)

---

**End wave-23 LFPDPPP MX legal review package** — 3 customer-visible artifacts (1 notice + 2 breach templates); 11 statutory citation sites; 4 open questions for attorney (Q1-Q4); 8-13h estimated engagement; DEBT-025 added (open) → wave-26 close target.
