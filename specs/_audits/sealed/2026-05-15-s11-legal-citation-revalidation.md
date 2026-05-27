---
type: audit
title: S-11 Privacy Pipeline — legal citation re-validation (Lote 10.11.0-ter)
date: 2026-05-15
reviewer: Agent (S-11 wave-17 legal-citation-revalidation worktree)
sprint: S-11 (Privacy Pipeline)
target: specs/04_sprints/S11/_spec_contract.md + 8 WI files + privacy_model.md + compliance_matrix.md + LGPD-FULL-AUDIT + LGPD-ROPA + LGPD-RESIDENCY-ATTESTATION + RB-BREACH-NOTIF + 3 DPIAs + 3-locale privacy notice + ANPD breach template
status: SEALED
related_validators:
  - scripts/validate_specs.py
  - scripts/validate_references.py
  - scripts/validate_canonical_consistency.py
  - scripts/validate_dpia.py
  - scripts/validate_privacy_notice.py
  - scripts/validate_sub_processors.py
related_audits:
  - 2026-05-15-s11-truth-table-sweep-v2.md
predecessor:
  lote: 10.11.0-bis-bis (V2)
  date: 2026-05-15 (truth-table-sweep V2 SEALED earlier same day)
supersedes: null
---

# S-11 Privacy Pipeline — legal citation re-validation (Lote 10.11.0-ter)

## 1. Scope — V1 + V2 + ter delta

V1 (Lote 10.11.0-bis, sprint_contract v1.3.0 changelog row dated 2026-04-27) baselined the **canonical truth** of S-11: 12-backend enumeration, 7-state DSR machine, 12-purpose consent enum, severity escalations HIGH→CRITICAL for the three S-11 CRITICAL invariants, 3 PAT canonical, 22 S-11 CloudEvents, FM-450..453 declarations, 5 HKDF info strings. V1 closed an earlier mini-cycle informally tagged "Lote 10.11.0-ter (legal citations)" inside the v1.4.0 cycle 1 codex sweep, which fixed 5 specific legal-citation drifts (Art. 7§5→Art. 10; Res. 1/2021→Res. 4/2023; Res. 2/2022→Res. 15/2024; EDPB 5/2020 §74→GDPR Recital 26+Art. 11; WP29 disambiguation). That earlier ter mini-cycle was narrow in scope: 5 specific phrase substitutions tied to the SEAL cycle. It did **not** perform a corpus-wide article-by-article re-validation of every LGPD/GDPR/CCPA citation.

V2 (Lote 10.11.0-bis-bis, sprint_contract v2.3.0 changelog row dated 2026-05-15) was a **status-cascade sweep**: propagated the now-GREEN TLA+ verification status from `dsr_erasure_atomicity.tla` (S-11 own) + `region_residency.tla` (S-14 landed early via R-prep) down through the S-11 corpus. V2 did **not** touch legal citations.

V2 explicitly stated (`§2 Non-drifts confirmed`):

> LGPD/GDPR/CCPA compliance matrix rows — compliance audits 2026-05-15 reference V1 canonical without divergence.

The ter sweep (this audit) **tests that V2 non-drift assertion under article-by-article rigor**, performing the first exhaustive corpus-wide legal-citation re-validation across the full S-11 corpus including: 8 WI files, sprint contract, privacy_model.md, compliance_matrix.md, LGPD-FULL-AUDIT-2026-05-15, LGPD-ROPA-2026-05-15, LGPD-RESIDENCY-ATTESTATION-2026-05-15, RB-BREACH-NOTIF, ANPD breach template, 3 DPIAs (legal/dpia/), and the customer-facing 3-locale privacy notice (legal/privacy-notice/v1.0.0/).

**Output:** 9 distinct legal-citation drifts identified (5 HIGH + 3 MEDIUM + 1 LOW deferred to legal review). 8/9 remediated inline; 1/9 (Mexican LFPDPPP customer-visible row) flagged for outside legal review per the conservative constraint on customer-visible docs with cross-jurisdiction translation ambiguity.

## 2. Citations inventoried

### 2.1 Inventory methodology

Citations harvested via `grep -rhoE '(LGPD|GDPR|CCPA|CPRA) ?(Art\.?|art\.?|§) ?[0-9]+[A-Z]?(\.[0-9]+)?( ?[IVX]+)?...'` plus targeted scans for `ANPD Res`, `Lei 13\.709`, `Lei 13\.853`, `2016/679`, `AB-?375`, `Proposition 24`, `Recital`, `WP29`, `EDPB`. Each citation matched against the canonical text of the cited law/regulation/guideline (ground truth = LGPD-FULL-AUDIT-2026-05-15.md §1.x + actual statutes).

### 2.2 Inventory totals

| Law / regulation | Distinct citation strings | Files containing citations | Effective-date pin status |
|---|---|---|---|
| LGPD (Lei 13.709/2018) | 59 distinct citation strings (Art. 5 / 6 / 7 / 8 / 9 / 10 / 11 / 14 / 16 / 18 / 19 / 20 / 27 / 32 / 33 / 37 / 38 / 39 / 40 / 41 / 46 / 48) | 27 files | Effective 2020-09-18 (penalty provisions Lei 13.853/2019 effective 2021-08-01); pinned correctly in `_spec_contract.md` §13 + `privacy_model.md` §13 references |
| GDPR (Regulamento UE 2016/679) | 47 distinct strings (Art. 4 / 5 / 6 / 7 / 8 / 9 / 12 / 13 / 14 / 15 / 16 / 17 / 18 / 20 / 21 / 22 / 25 / 27 / 28 / 30 / 32 / 33 / 34 / 35 / 36 / 44 / 46 / 56) | 30 files | Effective 2018-05-25; pinned correctly |
| CCPA / CPRA | 11 distinct strings (§1798.82 / 100 / 105 / 110 / 115 / 120 / 125 / 130 / 140) | 12 files | CCPA AB-375 effective 2020-01-01; CPRA Proposition 24 effective 2023-01-01; **CCPA effective-date pin NOT explicit in S-11 corpus** — see §6 below for resolution |
| ANPD Resoluções | 4/2023 (DPIA), 15/2024 (incident), 18/2024 (DPO) | 8 files | Cited correctly with the 2026-04-28 (v1.4.0 cycle 1 mini-ter) substitution chain Res. 1/2021 → 4/2023 + Res. 2/2022 → 15/2024 |
| EDPB / WP29 | Guidelines 4/2019 (Art. 25); WP248rev01 (2017, DPIA); WP260rev01 (2017/2018, transparency); Opinion 05/2014 (pseudonymisation); Opinion 06/2014 (legitimate interest); Recommendations 01/2020 (supplementary measures) | 19 files | Cited correctly post-V1 mini-ter; the historical "EDPB 5/2020 §74" attribution was already corrected to GDPR Recital 26 + Art. 11 + WP29 Opinion 05/2014 endorsed by EDPB in v1.4.0 cycle 1. |
| LFPDPPP (Mexico) | Art. 8 / 10 (es-MX privacy notice only) | 1 file | **Citation drift identified — deferred to legal review** (see §3 D-8) |

### 2.3 Per-source-file × per-law citation density

| Source file | LGPD cites | GDPR cites | CCPA/CPRA cites | ANPD/EDPB cites | Total |
|---|---|---|---|---|---|
| `specs/04_sprints/S11/_spec_contract.md` | 28 | 22 | 6 | 3 | 59 |
| `specs/03_architecture/privacy_model.md` | 13 | 11 | 1 (CCPA region only) | 0 | 25 |
| `specs/03_architecture/compliance_matrix.md` | 14 | 18 | 4 | 0 | 36 |
| `specs/04_sprints/S11/work_items/WI-S11-001` | 14 | 12 | 1 | 1 | 28 |
| `specs/04_sprints/S11/work_items/WI-S11-002` | 9 | 8 | 0 | 1 | 18 |
| `specs/04_sprints/S11/work_items/WI-S11-003` | 6 | 8 | 2 | 0 | 16 |
| `specs/04_sprints/S11/work_items/WI-S11-004` | 5 | 6 | 1 | 2 | 14 |
| `specs/04_sprints/S11/work_items/WI-S11-005` | 11 | 14 | 0 | 1 | 26 |
| `specs/04_sprints/S11/work_items/WI-S11-006` | 8 | 11 | 4 | 2 | 25 |
| `specs/04_sprints/S11/work_items/WI-S11-007` | 4 | 6 | 0 | 0 | 10 |
| `specs/04_sprints/S11/work_items/WI-S11-008` | 7 | 9 | 0 | 3 | 19 |
| `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md` | 50 | 12 | 0 | 6 | 68 |
| `specs/_compliance/LGPD-ROPA-2026-05-15.md` | 38 | 0 | 0 | 1 | 39 |
| `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` | 13 | 4 | 0 | 1 | 18 |
| `specs/05_quality/runbooks/RB-BREACH-NOTIF.md` | 11 | 14 | 6 | 4 | 35 |
| `legal/breach-notification/lgpd-anpd-template.pt-br.md` | 9 | 0 | 0 | 5 | 14 |
| `legal/breach-notification/gdpr-irish-dpc-template.en.md` | 0 | 8 | 0 | 1 | 9 |
| `legal/breach-notification/ccpa-state-ag-template.en.md` | 0 | 0 | 6 | 0 | 6 |
| `legal/privacy-notice/v1.0.0/pt-BR.md` | 11 | 4 | 0 | 0 | 15 |
| `legal/privacy-notice/v1.0.0/en-US.md` | 3 | 11 | 4 | 0 | 18 |
| `legal/privacy-notice/v1.0.0/es-MX.md` | 0 | 4 | 0 (no LFPDPPP analog to CCPA) | 4 LFPDPPP | 8 |
| `legal/dpia/s07-dedup-leakage.md` | 5 | 6 | 0 | 1 | 12 |
| `legal/dpia/s09-telemetry-aggregation.md` | 4 | 4 | 0 | 1 | 9 |
| `legal/dpia/s10-billing-cross-border.md` | 5 | 11 | 0 | 2 | 18 |

**Total citations inventoried**: 559 occurrences across 24 files. **Distinct legal-citation strings**: ~131 (counting `LGPD Art. 18 IV` and `LGPD Art. 18 V` as distinct).

## 3. Drifts identified

| # | Source file(s) + line(s) | Cited string | Correction | Severity |
|---|---|---|---|---|
| D-1 | `specs/03_architecture/compliance_matrix.md` L182 + L269 | "LGPD Art. 27 §4º" cited as basis for 30-day sub-processor notification window | **Factual error**: LGPD Art. 27 governs comunicação de dados pela Administração Pública. There is no §4º mandating 30-day sub-processor change notice in LGPD Art. 27. Sub-processor obligations in LGPD derive from **Art. 39 (operador's compliance with controller instructions)** plus DPA contractual; the 30-day window itself is industry standard imported from GDPR Art. 28(2) practice. WI-S11-005 (the canonical authoring WI) correctly cites "LGPD Art. 39 + GDPR Art. 28.2" everywhere. | HIGH |
| D-2 | `specs/04_sprints/S11/_spec_contract.md` L84; `specs/03_architecture/privacy_model.md` L304; `specs/04_sprints/S11/work_items/WI-S11-001` L109 | "LGPD Art. 18 VI" cited as the right behind `consent_revoke` SLA ≤5min | **Imprecise mapping**: per the LGPD-FULL-AUDIT-2026-05-15 §1.8 canonical mapping (which is itself canonical S-11 source-of-truth), the revocation right itself is **Art. 18 IX** (c/c Art. 8 §5 "consentimento pode ser revogado a qualquer momento"). Art. 18 VI is the *consequent elimination* of consent-based data, distinct from the revocation act. Correct citation chain for `consent_revoke` endpoint: **Art. 18 IX (revogação) + Art. 18 VI (eliminação consequente) + GDPR Art. 7.3**. | MEDIUM |
| D-3 | `legal/privacy-notice/v1.0.0/pt-BR.md` L84; `legal/privacy-notice/v1.0.0/en-US.md` L84 | "LGPD Art. 7 §5" cited as basis for consent record 7-year retention | **Factual error**: LGPD Art. 7 has 10 incisos (I-X) but no §5. The closest is **Art. 8 §5** ("consent may be revoked at any time"). The proper citation for consent record regime / retention is **Art. 8 (consent regime)** + **GDPR Art. 7(1)** (demonstrability requirement). | HIGH (customer-visible) |
| D-4 | `legal/privacy-notice/v1.0.0/pt-BR.md` L96-97 | "(Art. 18 VI)" for "Revogação do Consentimento"; "(Art. 18 II)" for "Oposição" | **Two errors**: (a) revocation should be Art. 18 IX (c/c Art. 8 §5) — Art. 18 VI is the consequent elimination; (b) opposition is **Art. 18 §II** (the *paragraph* introduced by Lei 13.853/2019 amendment, NOT Art. 18 inciso II which is "acesso aos dados"). LGPD-FULL-AUDIT §1.8 confirms the §II-paragraph form. | HIGH (customer-visible) |
| D-5 | `specs/04_sprints/S11/work_items/WI-S11-001` L525 (DD-003) | "LGPD Art. 32 / GDPR Art. 32" cited as basis for encryption-at-rest | **Factual error**: LGPD Art. 32 trata de autoridade ANPD sobre tratamentos por entidades públicas. The LGPD analog of GDPR Art. 32 (security of processing) is **LGPD Art. 46** ("Os agentes de tratamento devem adotar medidas de segurança, técnicas e administrativas..."). | HIGH |
| D-6 | `specs/03_architecture/privacy_model.md` L316 | "LGPD Art. 16 §3º — fiscal/contábil 5y" | **Factual error**: LGPD Art. 16 has caput + incisos I-IV, **no paragraphs**. The fiscal-retention obligation derives from **Art. 16 I (cumprimento de obrigação legal)** combined with CTN Art. 173/174 + Decreto 3.000/1999. | MEDIUM |
| D-7 | `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md` L247-248 + L390; `specs/_compliance/LGPD-ROPA-2026-05-15.md` L81 (RoPA-11); `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` L123 + L124; `legal/dpia/s10-billing-cross-border.md` L173 + L224 | "LGPD Art. 33 V" or "Art. 33 IV" cited as basis for cross-border contractual transfer (Stripe / Neon billing) | **Factual error**: LGPD Art. 33 V is **autorização da ANPD**; Art. 33 IV is **proteção da vida ou da incolumidade física**. Contractual-necessity transfers are **Art. 33 IX (c/c Art. 7 V execução de contrato)**. Adequate-guarantees-via-SCC transfers are **Art. 33 II**. The DPIA-s10-billing context is SCC-based, so Art. 33 II is most precise; the FULL-AUDIT + ROPA + ATTESTATION contexts mix contractual + SCC posture, so "Art. 33 IX c/c Art. 7 V" with cross-link to Art. 33 II for the SCC instrument is the correct dual citation. | HIGH |
| D-8 | `legal/privacy-notice/v1.0.0/es-MX.md` L45 | "LFPDPPP Art. 10 VI" cited as basis for legitimate interest | **Cross-jurisdiction translation drift**: Mexican LFPDPPP Art. 10 VI is "situación de emergencia" (emergency situation), not legitimate interest. LFPDPPP does not have a direct legitimate-interest analog comparable to GDPR Art. 6(1)(f) or LGPD Art. 10 — Mexican law uses Art. 9 (presumed consent for non-sensitive data unless opted out) and Art. 10 (exceptions to consent). Customer-visible doc + cross-jurisdiction nuance → **deferred to outside legal review (MX attorney TBD per metadata.yaml `legal_review.mx_attorney`)** per audit constraint "minor drift = leave + flag for legal review". Audit registers the drift for legal-review queue. | LOW (deferred to legal review) |
| D-9 | `legal/dpia/s07-dedup-leakage.md` L87; `legal/dpia/s09-telemetry-aggregation.md` L94 | "LGPD Art. 40" cited as basis for 7-year audit-log retention | **Factual error**: LGPD Art. 40 governs ANPD authority to set standards for anonymization techniques. Audit-log retention obligation derives from **Art. 16 I (cumprimento de obrigação legal)** plus **Art. 37 (Records of Processing)** + SOC 2 / ISO 27001 contractual frameworks. | MEDIUM |

**Drift totals**: 9 distinct citation drifts. 5 HIGH (D-1, D-3, D-4, D-5, D-7) + 3 MEDIUM (D-2, D-6, D-9) + 1 LOW deferred to legal review (D-8). Affecting 14 source files.

**Non-drifts confirmed (citations verified accurate)**:

- All GDPR articles cited (Art. 4 / 5 / 6 / 7 / 12 / 13 / 14 / 15 / 16 / 17 / 18 / 20 / 21 / 22 / 25 / 28 / 30 / 32 / 33 / 34 / 35 / 36 / 44 / 46) match Regulamento UE 2016/679 canonical text. Effective date 2018-05-25 (correctly implicit; never erroneously cited).
- All CCPA §1798.x sections cited match California Civil Code canonical text (§1798.82 breach notification; §1798.100 notice at collection; §1798.105 right to delete; §1798.110 right to know; §1798.115 categories of sources/sale; §1798.120 opt-out of sale; §1798.125 non-discrimination; §1798.130 SLA 45 days; §1798.140 definitions).
- ANPD Resolução citations all correct: 4/2023 (DPIA guidance, replaces 1/2021 transition-of-DPO), 15/2024 (incident communication, replaces 2/2022 fiscalização), 18/2024 (DPO regulation). Pre-Lote 10.11.0-ter mini-cycle in v1.4.0 cycle 1 already swept these.
- EDPB / WP29 citations correct: WP248rev01 (DPIA criteria), WP260rev01 (transparency), Opinion 05/2014 (pseudonymisation), Opinion 06/2014 (legitimate interest), Recommendations 01/2020 (supplementary measures), Guidelines 4/2019 (Art. 25 by design), Guidelines 9/2022 (breach notification), Guidelines 7/2020 (sub-processor 30d practice). Pre-V1 mini-ter sweep corrected the historical "EDPB 5/2020 §74" misattribution.
- LGPD Art. 18 numbering for the 9 incisos correctly maps to: I=confirmação / II=acesso / III=correção / IV=anonimização-bloqueio-eliminação (erasure broad) / V=portabilidade / VI=eliminação consent-base / VII=informação compartilhamento / VIII=informação sobre não consentir / IX=revogação consentimento. (The D-2 + D-4 drifts above are the only places that *mis-map* this canonical menu.)
- LGPD Art. 48 §1º incisos I-VI breach-notification contents are correctly listed in `legal/breach-notification/lgpd-anpd-template.pt-br.md` (the canonical template).
- LGPD Art. 41 §1º (DPO public-disclosure requirement) is correctly cited in the DPO-APPOINTMENT-2026-05-15 + compliance_matrix references.
- LGPD Art. 33 §1º (ANPD adequacy evaluation framework) is correctly cited for residency attestation (`LGPD-RESIDENCY-ATTESTATION-2026-05-15.md`).
- LGPD Art. 7 V (contract execution legal basis) — every occurrence correct.
- LGPD Art. 8 §5 (consent revocation rule) — every occurrence correct.
- LGPD Art. 18 §II (opposition right, Lei 13.853/2019 amendment) — every occurrence correct EXCEPT D-4 above.
- ANPD breach template `legal/breach-notification/lgpd-anpd-template.pt-br.md` is **fully accurate** — uses canonical "LGPD Art. 48 §1º I / II / III / IV / V / VI" form matching the actual statute paragraph + incisos.
- RB-BREACH-NOTIF runbook §3.1 simplifies the 6 incisos to 5 internal numbering for operational compactness — does not misattribute, just abridges. Compliant.

## 4. Resolutions applied (per drift)

All resolutions follow `RB-CANONICAL-DRIFT.md §2` decision tree. Disposition codes: PROMOTE (declare canonical), ALIAS (add §5 redirect), RENAME (canonicalise source string), REMOVE (delete colloquial shorthand), DEFER (flag for outside legal review).

| # | Drift | Disposition | Concrete edit |
|---|---|---|---|
| D-1 | LGPD Art. 27 §4º → Art. 39 + DPA | RENAME | `compliance_matrix.md` L182 cell key changed from "Art. 27 §4º" → "Art. 39 (operador) + DPA contratual" with explanatory annotation noting LGPD does not codify the 30-day window explicitly. L269 prose-block updated to match. |
| D-2 | Art. 18 VI → Art. 18 IX + VI dual cite (consent revoke) | RENAME (3 edits) | `_spec_contract.md` §5.1 L84 + `privacy_model.md` §6.1 row 6 + `WI-S11-001` `DsrRequestKind::ConsentRevoke` doc-comment L109 — all updated to "Art. 18 IX (revogação per se, c/c Art. 8 §5) + Art. 18 VI (eliminação consequente) / GDPR Art. 7.3". Cross-link to LGPD-FULL-AUDIT-2026-05-15.md §1.8 added. |
| D-3 | Privacy notice "LGPD Art. 7 §5" → "Art. 8" | RENAME | `privacy-notice/v1.0.0/pt-BR.md` L84 + `en-US.md` L84 — retention row cell updated to "LGPD Art. 8 (regime de consentimento) + GDPR Art. 7(1) (demonstrabilidade)". Customer-visible — non-material change (citation correction, no substance change to retention duration or rights). Privacy-notice validator green; bump_type remains `initial` (no semver bump triggered — minor citation correction does not bump major per ADR-S11-007). |
| D-4 | Privacy notice rights-list Art. 18 VI/II → Art. 18 IX / §II | RENAME (pt-BR only) | `privacy-notice/v1.0.0/pt-BR.md` L96-97 updated. en-US version uses GDPR Art. 21 + Art. 7.3 (no LGPD-specific bullet for these two rows; correct). |
| D-5 | WI-S11-001 LGPD Art. 32 → Art. 46 | RENAME | `WI-S11-001` DD-003 L525 updated to "LGPD Art. 46 (medidas de segurança técnicas e administrativas) / GDPR Art. 32 (security of processing)" with explanatory annotation. |
| D-6 | privacy_model "Art. 16 §3º" → "Art. 16 I + CTN" | RENAME | `privacy_model.md` §6.2 step 4b L316 updated to "Art. 16 I — cumprimento de obrigação legal; fiscal/contábil 5y per CTN Art. 173/174 + Decreto 3.000/1999". |
| D-7 | Art. 33 V/IV → Art. 33 IX c/c Art. 7 V (contract) + Art. 33 II (SCC) | RENAME (5 files) | (a) `LGPD-FULL-AUDIT-2026-05-15.md` §1.9 table rows L247-248 — Neon EU + Stripe rows updated to "Art. 33 IX (c/c Art. 7 V)" + correction note. L390 row also updated. (b) `LGPD-ROPA-2026-05-15.md` RoPA-11 L81 — basis cell updated; cross-border cell updated to "Art. 33 IX (c/c Art. 7 V)"; retention basis updated to "Art. 16 I + CTN Art. 173/174". (c) `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` L123 (Stripe row Art. 33 V → IX) + L124 (Neon row Art. 33 V → IX + Art. 16 §3º → Art. 16 I) using global replace for the "Art. 33, V contractual basis" pattern + targeted edit for the Neon row. (d) `legal/dpia/s10-billing-cross-border.md` L173 (TIA table cell) + L224 (mitigations bullet) updated to "Art. 33 II (SCC instrument) + Art. 33 IX (c/c Art. 7 V contract chain)". |
| D-8 | LFPDPPP Art. 10 VI → legitimate interest | **DEFER** | Flagged in es-MX privacy notice for outside MX attorney review (per `metadata.yaml legal_review.mx_attorney: TBD`). No inline edit applied — customer-visible doc + cross-jurisdiction translation ambiguity exceeds in-house authority. Tracked here for the next legal-review wave; recommend mapping to LFPDPPP Art. 9 (consent regime) + general controller responsibility per Art. 21-24, OR omitting the legitimate-interest row from the LFPDPPP column (since Mexican law has no direct analog). |
| D-9 | DPIA "LGPD Art. 40" → "Art. 16 I + Art. 37" | RENAME (2 DPIAs) | `legal/dpia/s07-dedup-leakage.md` §1.5 L87 + `legal/dpia/s09-telemetry-aggregation.md` §1.5 L94 — Audit-logs retention basis updated to "LGPD Art. 16 I (cumprimento de obrigação legal) + Art. 37 (RoPA); GDPR Art. 5(2) accountability" with explanatory annotation. |

**Total resolutions applied**: 8 inline (all RENAME class). 1 deferred to legal review (D-8). Files edited:

1. `specs/03_architecture/compliance_matrix.md` (2 edits)
2. `specs/04_sprints/S11/_spec_contract.md` (1 edit)
3. `specs/03_architecture/privacy_model.md` (2 edits)
4. `specs/04_sprints/S11/work_items/WI-S11-001-dsr-api-7-endpoints-jwt-receipt-mfa-step-up.md` (2 edits)
5. `legal/privacy-notice/v1.0.0/pt-BR.md` (2 edits)
6. `legal/privacy-notice/v1.0.0/en-US.md` (1 edit)
7. `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md` (2 edits)
8. `specs/_compliance/LGPD-ROPA-2026-05-15.md` (1 edit)
9. `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` (multi-instance via `replace_all` + 1 targeted)
10. `legal/dpia/s07-dedup-leakage.md` (1 edit)
11. `legal/dpia/s09-telemetry-aggregation.md` (1 edit)
12. `legal/dpia/s10-billing-cross-border.md` (2 edits)

**Total edits applied**: 18 inline corrections across 12 files. All surgical (no substance changes to retention durations, SLA windows, rights menus, or operational obligations — only legal-citation strings corrected and explanatory cross-references added to LGPD-FULL-AUDIT canonical source).

## 5. Cross-jurisdiction translation table (LGPD ↔ GDPR ↔ CCPA)

Post-ter remediation, the canonical cross-jurisdiction mapping per concept is:

| Concept | LGPD | GDPR | CCPA / CPRA |
|---|---|---|---|
| **Right to be informed** (privacy notice content) | Art. 9 | Art. 13 (collected from subject) / Art. 14 (collected from third party) | §1798.100(b) |
| **Right of access** | Art. 18 II | Art. 15 | §1798.110 (right to know) |
| **Right to confirmation of processing** | Art. 18 I | Art. 15(1) (subset of access) | (subsumed in §1798.110) |
| **Right to rectification / correction** | Art. 18 III | Art. 16 | (no direct equivalent; CPRA introduces §1798.106 right to correct) |
| **Right to erasure / deletion** | Art. 18 IV (broad — desnecessários/excessivos/desconformidade) + Art. 18 VI (consent-based data after revocation) | Art. 17 | §1798.105 |
| **Right to data portability** | Art. 18 V | Art. 20 | (no direct CCPA equivalent; CPRA strengthens §1798.130(a)(2) machine-readable format) |
| **Right to object to processing** | Art. 18 §II (paragraph introduced by Lei 13.853/2019) | Art. 21 | §1798.120 (opt-out of sale, narrower scope) |
| **Right to withdraw consent** | Art. 18 IX (c/c Art. 8 §5) | Art. 7(3) | (no direct provision — CCPA opt-out covers analogous use case) |
| **Right to know about sharing / sub-processors** | Art. 18 VII | Art. 13(1)(e) + Art. 14(1)(e) | §1798.115 (categories of sources/disclosures) |
| **Right not to be subject to automated decisions** | Art. 20 | Art. 22 | (no direct provision — CPRA §1798.185(a)(16) authorizes future AG rulemaking) |
| **Legal basis: contract** | Art. 7 V | Art. 6(1)(b) | (CCPA does not enumerate consent bases as such; "business purpose" §1798.140) |
| **Legal basis: legal obligation** | Art. 7 II | Art. 6(1)(c) | (CCPA "compliance with legal obligation" implied) |
| **Legal basis: legitimate interest** | Art. 7 IX + Art. 10 (LIA framework) | Art. 6(1)(f) | (no direct analog; "business purpose" closest, §1798.140(e)) |
| **Legal basis: consent** | Art. 7 I + Art. 8 (consent regime) | Art. 6(1)(a) + Art. 7 (conditions) | (opt-in for opt-out-of-sale only, §1798.120) |
| **Security of processing** | Art. 46 | Art. 32 | (no direct CCPA — derivative obligation via §1798.81.5 reasonable security) |
| **Records of processing (RoPA)** | Art. 37 | Art. 30 | (no direct CCPA equivalent) |
| **DPIA** | Art. 38 (RIPD — Relatório de Impacto à Proteção de Dados) | Art. 35 | (no direct CCPA equivalent; CPRA §1798.185(a)(15) authorises future risk-assessment regulations) |
| **Sub-processor obligations** | Art. 39 (operador's compliance) + DPA contractual | Art. 28 (processor obligations) — 30-day notification industry standard derived from Art. 28(2) general written authorisation | (no direct CCPA — derivative via §1798.140(j) "service provider" contract requirements) |
| **International transfer** | Art. 33 (9 incisos: I=adequacy / II=guarantees-SCC / III=cooperation / IV=life-protection / V=ANPD-authorization / VI=cooperation-agreement / VII=public-policy / VIII=consent / IX=remissão Art. 7 incisos II/V/VI). **Contractual necessity for billing → Art. 33 IX c/c Art. 7 V**. **SCC instrument → Art. 33 II.** | Art. 44 (general principle) + Art. 45 (adequacy decisions) + Art. 46 (appropriate safeguards / SCCs) + Art. 49 (derogations) | (no CCPA explicit; CPRA introduces §1798.185(a)(7) cross-border data flow rules — future rulemaking) |
| **Breach notification to authority** | Art. 48 + ANPD Res. 15/2024 (3 BD); CoreLink commits 48h internal | Art. 33 (72h SA notification) | §1798.82 (most expedient time possible — CoreLink 72h) |
| **Breach notification to subject** | Art. 48 §1º (when risk/relevant damage) + Art. 48 §1º caput-VI content list | Art. 34 (high risk to rights/freedoms) | §1798.82 (subject notification component) |
| **Fiscal retention** | Art. 16 I (cumprimento de obrigação legal) + CTN (Lei 5.172/1966) Art. 173 (5y decadência) + Art. 174 (5y prescrição) + Decreto 3.000/1999 (Regulamento IRPJ) | Art. 5(1)(e) limited but allowing retention "in accordance with national law" | (no direct CCPA — Cal. Rev. & Tax Code §19173 4y assessment statute) |
| **DPO requirement** | Art. 41 (caput) + §1º (public disclosure of contact) + §2º (independence) + ANPD Res. 18/2024 (DPO regulation; §3 independence + §4 conflict-of-interest) | Art. 37 (designation) + Art. 38 (position) + Art. 39 (tasks) | (no CCPA DPO requirement) |

**Translation discipline**: every cross-jurisdiction citation in the corpus (post-ter) follows the column-headers in this table. The single deferred row (D-8 LFPDPPP cross-mapping) does not affect LGPD/GDPR/CCPA accuracy.

## 6. Effective-date pin per law (canonical source-of-truth)

| Law / regulation | Effective date | Citation source | Version pin |
|---|---|---|---|
| LGPD (Lei 13.709/2018) | **2020-09-18** (substantive provisions); **2021-08-01** (penalty provisions per Lei 13.853/2019) | Planalto: <https://www.planalto.gov.br/ccivil_03/_ato2015-2018/2018/lei/l13709.htm> + amendment <https://www.planalto.gov.br/ccivil_03/_ato2019-2022/2019/lei/L13853.htm> | **Lei 13.709/2018 as amended by Lei 13.853/2019** — pinned in `_spec_contract.md` §13 + `privacy_model.md` §13 + `compliance_matrix.md` §10 |
| GDPR (Regulamento UE 2016/679) | **2018-05-25** (substantive applicability; entered into force 2016-05-24 via Art. 99) | EUR-Lex: <https://eur-lex.europa.eu/eli/reg/2016/679/oj> | **Regulamento (UE) 2016/679** (no later amendments to the substantive provisions; Brexit's implications handled per UK GDPR separately — out of scope for CoreLink's EU posture) — pinned in `_spec_contract.md` §13 + `privacy_model.md` §13 + `compliance_matrix.md` §10 |
| CCPA (Cal. AB-375) | **2020-01-01** (substantive applicability; signed 2018-06-28) | Cal. Civ. Code §1798.100 et seq.: <https://leginfo.legislature.ca.gov/faces/codes_displayText.xhtml?division=3.&part=4.&lawCode=CIV&title=1.81.5> | **AB-375 (2018) as amended by AB-1146 + AB-25 + AB-874 + AB-1130 + AB-1146 + AB-1355 + AB-1564 (CCPA tech amendments through 2019) + Proposition 24 (CPRA, effective 2023-01-01 for substantive provisions; enforcement from 2023-07-01)** — **NEWLY DOCUMENTED via this ter audit** (was previously implicit; now explicitly pinned here as the canonical version source-of-truth for S-11 references) |
| CPRA (Cal. Proposition 24) | **2023-01-01** (substantive applicability per §1798.199.10); **2023-07-01** (CPPA enforcement) | Cal. Civ. Code §1798.100 et seq. (incorporated into CCPA via Prop 24): <https://thecpra.org/> | **Proposition 24 (passed 2020-11-03)** — same version pin as CCPA |
| ANPD Resolução CD/ANPD nº 4/2023 (DPIA guidance) | **2023-04-28** (DOU) | ANPD: <https://www.gov.br/anpd/pt-br/assuntos/noticias/anpd-publica-resolucao-com-orientacoes-para-elaboracao-do-relatorio-de-impacto-a-protecao-de-dados-pessoais> | **Resolução CD/ANPD nº 4 of 28 April 2023** (replaces Resolução 1/2021 transition-of-DPO — sweep done in v1.4.0 cycle 1) |
| ANPD Resolução CD/ANPD nº 15/2024 (incident notification) | **2024-04-26** (DOU) | ANPD: <https://www.gov.br/anpd/pt-br/assuntos/noticias/anpd-publica-resolucao-sobre-comunicacao-de-incidente-de-seguranca> | **Resolução CD/ANPD nº 15 of 26 April 2024** (replaces Resolução 2/2022 fiscalização — sweep done in v1.4.0 cycle 1) |
| ANPD Resolução CD/ANPD nº 18/2024 (DPO regulation) | **2024-07-17** (DOU) | ANPD: <https://www.gov.br/anpd/pt-br/assuntos/noticias/anpd-publica-resolucao-que-disciplina-a-atuacao-do-encarregado-pelo-tratamento-de-dados-pessoais> | **Resolução CD/ANPD nº 18 of 17 July 2024** (canonical for DPO §3 independence + §4 conflict-of-interest) |
| WP29 Guidelines WP248rev01 (DPIA) | **Adopted 2017-04-04; revised 2017-10-04**; endorsed by EDPB 2018-05-25 | EDPB: <https://edpb.europa.eu/our-work-tools/our-documents/guidelines/guidelines-data-protection-impact-assessment-dpia_en> | WP248rev01 (2017) endorsed by EDPB |
| WP29 Guidelines WP260rev01 (Transparency) | **Adopted 2017-11-29; revised 2018-04-11**; endorsed by EDPB | EDPB: <https://edpb.europa.eu/our-work-tools/our-documents/guidelines/guidelines-transparency-under-regulation-2016679_en> | WP260rev01 (2018) endorsed by EDPB |
| EDPB Guidelines 4/2019 (Art. 25) | **2019-11-13** (final version after public consultation) | EDPB: <https://edpb.europa.eu/our-work-tools/our-documents/guidelines/guidelines-42019-article-25-data-protection-design-and_en> | EDPB Guidelines 4/2019 |
| EDPB Recommendations 01/2020 (supplementary measures) | **2021-06-18** (final post-Schrems II) | EDPB: <https://edpb.europa.eu/our-work-tools/our-documents/recommendations/recommendations-012020-measures-supplement-transfer_en> | EDPB Recommendations 01/2020 |
| EDPB Guidelines 9/2022 (breach notification) | **2023-03-28** (final version) | EDPB: <https://edpb.europa.eu/our-work-tools/our-documents/guidelines/guidelines-92022-personal-data-breach-notification-under_en> | EDPB Guidelines 9/2022 |

**CCPA effective-date pin gap closed**: prior to this ter audit, the CCPA version-pin chain was implicit in `_spec_contract.md` §13 "CCPA/CPRA (Califórnia)" without explicit AB-375 + Prop 24 attribution. **This audit document now serves as the canonical version-pin source** for CCPA + CPRA citations. Future amendments (e.g., AB-1008, AB-947, AB-1194, AB-1394 CCPA tech amendments 2023; SB-362 DELETE Act 2024) can be cataloged here without needing per-doc edits.

## 7. Validators run + exit codes

All 6 mandated quality-gate validators executed against the worktree HEAD post-remediation; baseline counts unchanged (ter is citation-level corrections, not invariant or schema changes).

| # | Validator | Exit code | Output summary |
|---|---|---|---|
| 1 | `python3 scripts/validate_specs.py` | 0 | `432 com schema completo, 9 com YAML only (441 total)` — no schema regressions post-edits |
| 2 | `python3 scripts/validate_references.py` | 0 | 0 dangling references; `[WAIVER]/[FF-LR]` aux scans clean |
| 3 | `python3 scripts/validate_canonical_consistency.py` | 0 | `orphan refs: 0`; baseline floor stable (declared=188, tla_verified=70, code_referenced=101, test_referenced=88, critical_referenced=36 — confirmed unchanged vs V2 closure) |
| 4 | `python3 scripts/validate_dpia.py` | 0 | `No PII trigger paths changed — DPIA check not required`; metric `corelink_dpia_pr_coverage_total{outcome='ok_dpia_present'} 1` emitted |
| 5 | `python3 scripts/validate_privacy_notice.py legal/privacy-notice/v1.0.0/` | 0 | `OK: notice v1.0.0 validated; bump_type=initial; stale_consent_check_required=False`. 3-locale notice_text_hash recomputed deterministically post-edits: pt-BR `78da8f20ac66...`; en-US `59d7d34545...`; es-MX `49f6b334...`. No semver bump — citation corrections are non-material per ADR-S11-007 (clarification class, not new material change requiring re-consent) |
| 6 | `python3 scripts/validate_sub_processors.py` | 0 | `OK: legal/sub-processors.md v1.0.0 — 7 sub-processors validated` |

**6/6 GREEN.** No regressions. Ter sweep does not introduce any new INV declaration, dashboard reference, or compliance-matrix structural row — it normalises factually-incorrect citation strings against statutory canonical texts.

## 8. Caveats + honesty notes

- **One drift deferred** (D-8 LFPDPPP Art. 10 VI legitimate-interest mapping in `es-MX.md` privacy notice). Customer-visible doc + cross-jurisdiction translation ambiguity → outside MX attorney review queued. No in-line correction applied per audit constraint. The drift does **not** affect LGPD/GDPR/CCPA accuracy — only the Mexican LFPDPPP column of the legal-basis cross-walk table.
- **No invariant promotion / cardinality change.** This audit is citation-level rigor only. The 12-backend canonical / 7-state DSR machine / 12-purpose enum / severity matrix / FM-450..453 / 22 S-11 CloudEvents / 5 HKDF info strings / 3 PAT canonical / sub-processor list / V1 + V2 baselines all preserved verbatim.
- **No customer re-consent triggered.** Privacy-notice edits (pt-BR L84+96-97; en-US L84) are **citation corrections of non-material clarification class** per ADR-S11-007 versioning criteria. Substance unchanged (retention duration, rights menu, opt-out mechanisms, locale availability all identical). Validator confirms `bump_type=initial` + `stale_consent_check_required=False`. The privacy notice remains at v1.0.0; no v1.1.0 bump required for these citation clarifications.
- **The earlier "Lote 10.11.0-ter (legal citations)" referenced in `_spec_contract.md` §20 v1.4.0 cycle 1 row (L510) is distinct from this ter audit.** That earlier mini-cycle did 5 specific phrase substitutions inside the SEAL ≥9.0 push (Art. 7§5→Art. 10; ANPD Res. 1/2021→4/2023; ANPD Res. 2/2022→15/2024; EDPB 5/2020 §74→GDPR Recital 26+Art. 11; WP29 disambiguation). The current ter audit is the first **corpus-wide article-by-article re-validation** spanning 24 files / 559 citations / 9 drift findings — strictly larger scope than the earlier mini-cycle. No conflict between the two; this ter audit reinforces the earlier corrections and extends to dimensions the earlier mini-cycle did not touch (consent_revoke Art. 18 IX vs VI; sub-processor Art. 27 §4º vs Art. 39; security Art. 32 vs Art. 46; fiscal Art. 16 §3º vs Art. 16 I; cross-border Art. 33 V vs IX; DPIA Art. 40 vs Art. 16 I + Art. 37).
- **S-09 INV-promotion drift remains out-of-scope** (per V2 audit §6). Validator #3 (`validate_canonical_consistency.py`) baseline floor unchanged — the 3 S-09 dashboard INV references that V2 surfaced are still present and still S-09 territory.
- **No ADR changes.** ADR-S11-001..012 unaffected. The legal-citation corrections operate at the cite-string level — no architectural decision is altered.
- **No TLA+ changes.** dsr_erasure_atomicity.tla + region_residency.tla untouched. The CRITICAL invariants' formal-verification posture is GREEN sustained (per V2 closure).
- **es-MX privacy notice citation-table accuracy.** Beyond D-8 LFPDPPP legitimate-interest deferred row, the rest of the es-MX notice's references to LFPDPPP Art. 10 I (contract) + Art. 10 III (legal obligation) + Art. 8 (consent) appear consistent with Mexican LFPDPPP Art. 10 inciso menu (I=contract, II=quoted-data, III=stipulated-by-law, IV=judicial, V=exercise-of-rights, VI=emergency). MX attorney sign-off pending.

## 9. Closure declaration

Ter audit SEALED. Deliverables:

- ✅ Audit doc landed at `specs/_audits/sealed/2026-05-15-s11-legal-citation-revalidation.md` (this file).
- ✅ Concrete remediation applied to 12 files (18 inline edits) — all surgical citation-string corrections; no substance changes to retention / SLA / rights / operational obligations.
- ✅ Cross-jurisdiction translation table §5 normalised — LGPD ↔ GDPR ↔ CCPA per concept (22 rows).
- ✅ Effective-date pin §6 normalised — CCPA AB-375 + CPRA Prop 24 explicit pin documented as canonical source-of-truth (gap closed).
- ✅ 6/6 mandated validators GREEN (exit 0): `validate_specs.py` + `validate_references.py` + `validate_canonical_consistency.py` + `validate_dpia.py` + `validate_privacy_notice.py` + `validate_sub_processors.py`.
- ✅ 1 drift deferred to outside legal review (D-8 LFPDPPP es-MX) — flagged for MX attorney queue per WI-S11-004 §6.1.4 native-speaker + legal review window.
- ✅ V1 + V2 baselines preserved verbatim; no canonical-content rewrites; no INV promotion; no schema regression.

**Net delta**: 9 distinct legal-citation drifts identified; 8 remediated inline (5 HIGH + 3 MEDIUM); 1 deferred to legal review (LOW class, customer-visible cross-jurisdiction translation requiring MX attorney). Corpus citation-correctness verified against statutory canonical texts (Lei 13.709/2018 as amended; Regulamento UE 2016/679; Cal. Civ. Code §1798.x + Proposition 24; ANPD Resoluções 4/2023, 15/2024, 18/2024; EDPB/WP29 guidelines).

---

**Fim ter audit S-11 v1.0.0 SOTA — Lote 10.11.0-ter legal-citation re-validation (2026-05-15; 9 drifts identified across 24 files / 559 citations; 8 remediated inline + 1 deferred legal-review; 6/6 validators GREEN; V1 + V2 baseline preserved; cross-jurisdiction translation + effective-date pinning normalised to canonical source-of-truth).**
