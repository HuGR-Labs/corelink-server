---
id: "WI-S19-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-009"]
parent: "S-19"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "PRIVACY-MODEL"
  - "SECURITY-MODEL"
  - "DATA-MODEL"
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
tags: ["wi", "s19", "onboarding", "dpa-click-through", "6-field-consent", "ctrl-priv-consent", "jwt-receipt", "evt-049", "3-locales", "legal-review", "lote-10-16-locale-cookie", "high-risk"]
---

# WI-S19-002 — DPA Click-Through 6-Field Consent Capture (CTRL-PRIV-CONSENT-001..006 Reflection per S-11/S-16 WI-S16-003 Pattern Pós Lote 10.16) + Render DPA Template Versioned `legal/dpa/v1.0.0.{en-US,pt-BR,es-419}.md` 3 Locales Legal Local-Reviewed (NÃO Automated Translation; Mandatory Legal Sign-off Per Locale Em `specs/_audits/2026-XX-XX-legal-review-dpa-v1.md`) + UI Scroll-Gate Anti-Dismiss UX (Button Disabled Until User Scrolls to Bottom of DPA Full Text) + 6-Field Consent Payload (`notice_text_hash` sha256 Calculado Frontend Web Crypto API + `notice_version` Semver from mdx Frontmatter + `locale` from Rendered `corelink_locale` Cookie Lote 10.16 Canonical NÃO Accept-Language Header + `wording_id` Deterministic UUID v7 from notice_version + `ui_capture_ts` Browser Date.now() ms Epoch + `submission_ts` Server-Assigned ts) + EVT-049 Evidence Captured Imutável em R2 Audit Bucket Object Lock 7y Retention + Signed JWT Receipt HS256 Per-Region Signing Key Separate Key_Id Index Per data_model.md §4.1 (NÃO Confundir com PAT Format S-03 Hybrid HMAC + Argon2id) + Reuse S-11 Consent Ledger D1 + Verify Endpoint R-S11-9 Pattern + INV-CONSENT-PROOF-VERIFIABLE CRITICAL §3.12 Reforced

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-19](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S19-002 |
| Título | DPA click-through 6-field consent capture + JWT receipt **ES256** (ECDSA P-256 asymmetric) per-region key — Lote 10.19 codex P1 canonical fix; ES256 asymmetric (ECDSA P-256; Lote 10.19 codex P1 canonical fix) era incoherent com offline verify via public-key endpoint (codex P1 finding); ES256 asymmetric enables offline verify (private key signs em backend; public key endpoint `/v1/dpa/verify-key/{region}.pub` serves verifying key for client/auditor) + 3 locales Legal local-reviewed + EVT-049 evidence 7y retention + scroll-gate anti-dismiss UX |
| Sprint | S-19 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-009 (DPA é customer-facing legal contract; bug = LGPD/GDPR/CCPA breach + contract challenge legitimate) |

## 1. Intent

DPA click-through é o **single most legally-load-bearing UX** em CoreLink — sem proof-of-informed evidence-grade auditável post-facto, GDPR Art. 7 + LGPD Art. 8 + GDPR Art. 28 (Processor obligations) ficam unfulfilled (consent record sem proof de "informed" + DPA não é signed contract). Este WI implementa: render DPA template versioned mdx em 3 locales (en-US/pt-BR/es-419) Legal local-reviewed (NÃO automated translation; mandatory Legal sign-off per locale), UI scroll-gate anti-dismiss UX (button disabled until user scrolls to bottom of full text), 6-field consent payload capture (CTRL-PRIV-CONSENT-001..006 reflection per S-16 WI-S16-003 pattern pós Lote 10.16; locale from rendered `corelink_locale` cookie NÃO Accept-Language header), EVT-049 evidence captured imutável em R2 audit bucket Object Lock 7y retention, signed JWT receipt HS256 per-region signing key separate key_id index per data_model.md §4.1, reuso S-11 consent ledger D1 + verify endpoint R-S11-9 pattern, e reforça **INV-CONSENT-PROOF-VERIFIABLE** (CRITICAL §3.12 herdada S-11) via DPA verify endpoint round-trip integrity ≥ 99.9%.

```rust
// File: crates/corelink-onboarding-dpa/src/dpa_capture.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;

#[async_trait]
pub trait DpaCapture: Send + Sync {
    /// POST /api/onboarding/dpa-accept — capture DPA acceptance com 6-field consent.
    /// Reuse S-11 consent ledger pattern + verify endpoint integration.
    /// Returns signed JWT receipt HS256 per-region signing key.
    async fn accept_dpa(
        &self,
        tenant_ctx: &TenantCtx,
        proof: ConsentProofPayload,                      // 6-field canonical (CTRL-PRIV-CONSENT-001..006)
        rendered_locale: Bcp47Locale,                    // from corelink_locale cookie pós Lote 10.16
        region: Region,                                  // tenant.primary_region for JWT signing key
    ) -> Result<DpaAcceptanceReceipt, DpaCaptureError>;

    /// **POST /api/onboarding/dpa-verify** (body `{receipt_jwt}`) — público stateless verify (Lote 10.19 codex P1 canonical fix; prior GET ?receipt_jwt=X violava CTRL-CRED-001 "zero secrets em URL"; receipt JWT em request body NÃO em URL params).
    /// Reuses S-11 verify endpoint pattern; returns match status.
    async fn verify_dpa_acceptance(
        &self,
        receipt_jwt: SignedJwt,
    ) -> Result<DpaVerifyResponse, DpaCaptureError>;
}

/// 6-field canonical proof payload per privacy_model.md §5.6 L255 + S-11 R-S11-7.
/// Pós Lote 10.16: locale from rendered cookie NÃO Accept-Language header.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ConsentProofPayload {
    pub notice_text_hash: NoticeTextHash,                // SHA-256 hex64 do DPA full text
    pub notice_version: SemverVersion,                   // semver "1.0.0" major bump force re-acceptance
    pub locale: LocaleBcp47,                             // from corelink_locale cookie (Lote 10.16)
    pub wording_id: WordingId,                           // deterministic UUID v7 from notice_version
    pub ui_capture_ts: DateTime<Utc>,                    // browser Date.now() ms epoch
    pub submission_ts: DateTime<Utc>,                    // server-assigned ts (HuGR clock authoritative)
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

DPA (Data Processing Agreement) é o **legal foundation contract** entre CoreLink (Processor/Operador) e customer (Controller/Controlador) per GDPR Art. 28 + LGPD Art. 39. Sem DPA signed evidence-grade, qualquer customer pode contestar legitimately em court que não consentiu com data processing terms (Schrems II + LGPD enforcement risk; breach notification mandatory per LGPD Art. 48 + GDPR Art. 33). Spec contract S-19 §5.2 R-S19-4 + R-S19-5 + R-S19-6 estabelecem 3 hard requirements: (a) DPA template em `legal/dpa/v1.0.0.md` + click-through implementation com 6-field consent payload + EVT-049 evidence + JWT receipt; (b) DPA versioning semver discipline (WI-S19-003 owns); (c) DPA available em 3 locales Legal local review per locale. Este WI entrega (a) + (c); WI-S19-003 entrega (b).

**Risk justification HIGH_RISK**:

- **FF-HR-009**: DPA click-through é customer-facing legal contract; bug = legal exposure + contract challenge legitimate em court; retroactive consent invalidation se Legal review missed material change.
- **Reversibility**: bug detected post-deploy = catastrophic (existing customers retroactively re-prompted; reputation damage; compliance audit failure SOC 2 + ISO 27001).
- **Blast radius**: 100% novos signups + (em re-acceptance flow WI-S19-003) 100% existing tenants.

**Bugs catastróficos que este WI deve catch**:

- **Locale mismatch** (Lote 10.16 violation): user renders DPA em pt-BR via cookie + Accept-Language=en-US; payload locale=en-US (drift); CTRL-PRIV-CONSENT-005 violation; consent rendered em pt-BR mas recorded como en-US = proof-of-informed broken; legal challenge legitimate.
- **Hash mismatch**: notice_text_hash calculado em frontend ≠ backend re-compute (encoding issue ou whitespace drift); CTRL-PRIV-CONSENT-001 violation; backend rejects mas legal evidence ambíguo.
- **Auto-submit dark pattern**: button enabled antes de user scroll; LGPD Art. 8º + GDPR Art. 7§2 invalid consent.
- **Pre-checked consent checkbox**: GDPR Art. 7§2 explicitly invalid.
- **JWT receipt key confusion**: DPA receipt JWT signing key compartilha key_id com PAT format S-03 hybrid HMAC + Argon2id; data_model.md §4.1 violation; key rotation breaks PAT auth ou DPA verify.
- **Legal review missed material change**: DPA template patch update applied mas Legal review skipped; material change shipped sem re-acceptance broadcast; retroactive consent invalidation.
- **Translation legal terms incorrect**: pt-BR ou es-419 translation legal terms incorrect (e.g., "controller" mistranslated); LGPD enforcement risk; native speaker review skipped.

**Atacante adversarial scenarios validated**:

- **Locale forge**: pentester sets `corelink_locale=en-US` cookie mas submits payload locale=pt-BR; verify backend rejects via cookie match (Lote 10.16 canonical).
- **Hash tamper**: pentester modifies DPA text mid-transit; verify backend re-compute mismatch + rejects.
- **JWT receipt forge**: pentester captures JWT + tries forge new with different fields; verify HS256 signature reject + key_id index match.
- **Replay JWT receipt**: pentester replays JWT 1y later; verify endpoint accepts (intentional; receipt is verifiable post-facto for legal evidence).
- **Pre-check attempt**: pentester injects pre-checked checkbox state via DOM; verify rendering test rejects.

**Mitigation**: locale match canonical from rendered `corelink_locale` cookie (Lote 10.16); backend re-compute hash + reject mismatch; click capture event-bound (no auto-submit); button disabled until scroll to bottom; pre-check rejected em rendering test; JWT receipt HS256 separate signing key per data_model.md §4.1; Legal review on every PR touching DPA + quarterly audit; native speaker review per locale + Legal local review per locale (mandatory before publish).

## 3. Customer Impact & Journey

**Persona 1 — Customer prospect signing up (DPA acceptance)**:
- Render DPA full text em locale ativa (cookie corelink_locale set by Next.js middleware).
- Scroll-gate: button "I have read and consent to the DPA v1.0.0" disabled until user scrolls to bottom.
- Click capture event-bound (no auto-submit dark patterns; LGPD Art. 8º + GDPR Art. 7§2 alignment).
- Confirmation modal: "Your DPA acceptance was recorded. Receipt ID: <jwt>. You can verify anytime at /api/onboarding/dpa-verify."
- Email confirmation com JWT receipt link via SES.

**Persona 2 — Privacy Officer cliente / Compliance auditor**:
- 6-field consent payload audit trail forensic-grade (notice_text_hash + notice_version + locale + wording_id + ui_capture_ts + submission_ts).
- EVT-049 evidence retention 7y em R2 audit bucket Object Lock.
- DPA verify endpoint público stateless (titular pode auditar self-service sem auth).
- Compliance Matrix: GDPR Art. 7 + Art. 28 + LGPD Art. 8º + Art. 39 + CCPA §1798.140(v) + EDPB SCCs alignment.

**Persona 3 — Legal Counsel reviewing DPA template**:
- Legal review on every PR touching DPA (CI gate flag `legal-review-required`); quarterly audit.
- 3 locales (en-US + pt-BR + es-419) Legal local review per locale (NÃO automated translation; native speaker review + legal terms accuracy verification).
- Output: `specs/_audits/2026-XX-XX-legal-review-dpa-v1.md` com sign-off Legal Counsel per locale.

**SLA addendum**:
- DPA acceptance evidence verifiable post-facto (consent verify endpoint S-11 retorna match) ≥ 99.9% round-trip integrity.
- JWT receipt verifiable offline via public-key endpoint (separate per-region signing key).
- 3 locales Legal local-reviewed (mandatory; no waiver).

## 4. Capability Mapping

- **CAP-ONBOARD-002** (DPA click-through) — IMPLEMENTA primary; 6-field consent + JWT receipt + EVT-049 evidence.
- Trace: `_spec_contract.md §5.2 R-S19-4 + R-S19-6` + `privacy_model.md §5.6 CTRL-PRIV-CONSENT-001..006` + `S-11 R-S11-7 6-field schema canonical + R-S11-9 verify endpoint` + `S-16 WI-S16-003 consent UI 6-field pattern pós Lote 10.16` + `data_model.md §4.1 JWT signing key separate key_id` + `invariant_registry.md §3.12 INV-CONSENT-PROOF-VERIFIABLE CRITICAL`.

## 5. Tipo

Feature WI; HIGH_RISK; FF-HR-009; legal-load-bearing.

## 6. Escopo

### 6.1 In-scope

1. **DPA template versioned em 3 locales**:
   - `legal/dpa/v1.0.0.en-US.md` + `legal/dpa/v1.0.0.pt-BR.md` + `legal/dpa/v1.0.0.es-419.md`.
   - mdx with frontmatter exposing `notice_version` (semver) + `locale` + `effective_date` + `legal_basis`.
   - Native speaker review per locale (deferred S-16 WI-S16-006 i18n full).
   - Legal local review per locale mandatory (NÃO automated translation): `specs/_audits/2026-XX-XX-legal-review-dpa-v1.md` com sign-off Legal Counsel per locale.
   - CI gate `legal-review-required` flag triggered on every PR touching `legal/dpa/`.

2. **DPA click-through UI** em `apps/web/src/app/onboarding/dpa/`:
   - Render mdx full text via @next/mdx (consume from S-16 frontend).
   - Scroll-gate: button "I have read and consent to the DPA v<semver>" disabled until user scrolls to bottom (scroll position tracking via IntersectionObserver).
   - Click capture event-bound (no auto-submit; explicit user action required; property test 100 random interactions verifies).
   - Pre-checked checkbox rendering test rejects (LGPD Art. 8º + GDPR Art. 7§2 alignment).

3. **6-field consent payload capture** (CTRL-PRIV-CONSENT-001..006 reflection per S-11 R-S11-7 + S-16 WI-S16-003 pattern):
   - **Field 1 — `notice_text_hash`**: sha256(rendered_dpa_text) via Web Crypto API `crypto.subtle.digest('SHA-256', encoder.encode(text))`; computed frontend; backend re-computes + match.
   - **Field 2 — `notice_version`**: semver string from mdx frontmatter (e.g., "1.0.0").
   - **Field 3 — `locale` (Lote 10.16 canonical)**: from rendered `corelink_locale` cookie (set by Next.js middleware); fallback `document.documentElement.lang`. **NÃO** `navigator.language` / `Accept-Language`. Backend valida match contra cookie at request time; mismatch = 422 + audit `corelink.onboarding.dpa_locale_mismatch`.
   - **Field 4 — `wording_id`**: deterministic UUID v7 from `notice_version` hash (stable across users; A/B test deferred pós-GA).
   - **Field 5 — `ui_capture_ts`**: browser `Date.now()` ms epoch (captured at click moment).
   - **Field 6 — `submission_ts`**: server-assigned ts em backend POST `/api/onboarding/dpa-accept`.
   - Backend payload schema validated via zod; rejects malformed.

4. **EVT-049 evidence captured imutável**:
   - Full 6-field payload + screenshot evidence (S-16 WI-S16-003 pattern reuse: html2canvas client-side primary + headless Chrome server-side fallback) uploaded to R2 audit bucket.
   - R2 Object Lock 7y retention (S-09 alignment + S-11 ≥ 5y legal evidence).
   - Privacy: screenshot redacts user IP server-side post-process.

5. **Signed JWT receipt **ES256** (ECDSA P-256 asymmetric) per-region key — Lote 10.19 codex P1 canonical fix; ES256 asymmetric (ECDSA P-256; Lote 10.19 codex P1 canonical fix) era incoherent com offline verify via public-key endpoint (codex P1 finding); ES256 asymmetric enables offline verify (private key signs em backend; public key endpoint `/v1/dpa/verify-key/{region}.pub` serves verifying key for client/auditor)**:
   - Backend signs JWT receipt with **per-region signing key** separate `key_id` index per `data_model.md §4.1` (NÃO confundir com PAT format S-03 hybrid HMAC + Argon2id; separate key family).
   - JWT claims: `sub=tenant_id` + `iat` + `exp` (10y) + `dpa_version` + `dpa_hash` + `locale` + `wording_id` + `key_id` + `region`.
   - Receipt downloadable + emailed via SES (S-11 receipt pattern reuse).
   - Public-key endpoint `GET /v1/public/keys/dpa-receipt/{region}.pub` for offline verify.

6. **Reuse S-11 consent ledger D1 + verify endpoint R-S11-9**:
   - DPA acceptance row inserted em `consent_ledger` D1 com `purpose=DataProcessingAgreement` enum extended (S-11 alignment).
   - Verify endpoint `POST /api/onboarding/dpa-verify` (body `{receipt_jwt: "<jwt>"}`; Lote 10.19 codex P1 canonical fix — receipt JWT em body NÃO query param para zero secrets em URL CTRL-CRED-001) reuses S-11 R-S11-9 pattern; returns match status `{tenant_id, dpa_version, dpa_hash, locale, wording_id, ui_capture_ts, submission_ts, signature_valid: bool, key_id, region}`.
   - INV-CONSENT-PROOF-VERIFIABLE CRITICAL §3.12 reforced via round-trip integrity ≥ 99.9%.

7. **Confirmation modal + email**:
   - Modal "Your DPA acceptance was recorded. Receipt ID: <jwt>. You can verify anytime at /api/onboarding/dpa-verify."
   - Email confirmation via SES with JWT receipt link.

8. **Locale mismatch alert**:
   - Métrica `corelink_onboarding_consent_locale_mismatch_total{plan}` counter; alert > 5/dia indicates cookie drift (Lote 10.16 violation).

### 6.2 Out-of-scope (deferred)

- DPA versioning + re-acceptance flow (WI-S19-003).
- Tier selection + Stripe Checkout (WI-S19-004; activation depends on DPA accepted).
- Enterprise inquiry form (WI-S19-005; separate flow).
- Conversion funnel instrumentation (WI-S19-006).
- A/B test infrastructure (Optimizely/equiv. — pós-GA Q1).
- Customer-managed DPA per customer (single template at GA; enterprise custom é S-14 manual Legal).

## 7. Anti-Scope

- Locale from Accept-Language header (Lote 10.16 violation).
- Auto-submit DPA (LGPD Art. 8º + GDPR Art. 7§2 violation).
- Pre-checked consent checkbox (GDPR Art. 7§2 invalid).
- DPA template automated translation (Legal local review mandatory).
- JWT receipt signing key shared com PAT format S-03 (data_model.md §4.1 violation).
- Skip Legal review on PR touching DPA (compliance gap).
- Skip native speaker review per locale.
- Hash compute em backend only (frontend should compute first; backend re-computes for match).
- DPA verify endpoint behind auth (must be público stateless for legal verifiability).
- EVT-049 retention < 7y (compliance gap LGPD Art. 16).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: DPA click-through 6-field consent capture + JWT receipt + 3 locales

  Scenario: 6-field payload captured + backend persisted
    Given user navigates /onboarding/dpa com cookie corelink_locale=pt-BR
    When user reads DPA + scrolls to bottom + clicks "I consent"
    Then payload contains notice_text_hash + notice_version + locale=pt-BR + wording_id + ui_capture_ts
    And backend POST /api/onboarding/dpa-accept accepts
    And submission_ts assigned server-side
    And consent_id returned + JWT receipt signed

  Scenario: notice_text_hash sha256 computed frontend + backend re-computes match
    Given DPA mdx v1.0.0 rendered
    When hash computed Web Crypto API
    Then hash matches sha256(dpa_rendered_text)
    And backend re-computes + matches CTRL-PRIV-CONSENT-001 enforcement

  Scenario: Locale match canonical cookie corelink_locale (Lote 10.16)
    Given cookie corelink_locale=pt-BR (set by middleware after user clicked locale switcher)
    When consent submitted with locale=pt-BR
    Then backend validates match
    When locale=en-US sent (mismatch — Accept-Language drift)
    Then backend rejects 422 com error "locale_mismatch"
    And audit emit "corelink.onboarding.dpa_locale_mismatch"

  Scenario: Scroll-gate anti-dismiss UX
    Given user navigates /onboarding/dpa
    When DPA rendered
    Then "I consent" button disabled
    When user scrolls to bottom
    Then button enabled
    And click event-bound capture (no auto-submit)

  Scenario: Pre-checked consent rejected (LGPD Art. 8º + GDPR Art. 7§2)
    Given consent checkbox state pre-checked (DOM injection attempt)
    When form rendered
    Then unchecked default enforced
    And submit disabled until user explicitly checks

  Scenario: JWT receipt **ES256** (ECDSA P-256 asymmetric) per-region key — Lote 10.19 codex P1 canonical fix; ES256 asymmetric (ECDSA P-256; Lote 10.19 codex P1 canonical fix) era incoherent com offline verify via public-key endpoint (codex P1 finding); ES256 asymmetric enables offline verify (private key signs em backend; public key endpoint `/v1/dpa/verify-key/{region}.pub` serves verifying key for client/auditor) separate key_id
    Given DPA accepted successful
    When JWT receipt generated
    Then signing key separate from PAT format S-03 (per data_model.md §4.1)
    And key_id index per region
    And public-key endpoint GET /v1/public/keys/dpa-receipt/{region}.pub returns matching pub key

  Scenario: EVT-049 evidence captured + 7y retention
    Given DPA accepted
    When EVT-049 evidence emitted
    Then 6-field payload + screenshot persisted em R2 audit bucket
    And Object Lock 7y retention enforced

  Scenario: Verify endpoint round-trip integrity (S-11 R-S11-9 reuse)
    Given DPA receipt JWT issued
    When POST /api/onboarding/dpa-verify body {receipt_jwt: X} (Lote 10.19 codex P1)
    Then 6-field payload retrieved + matches captured fields
    And signature_valid=true
    And INV-CONSENT-PROOF-VERIFIABLE round-trip integrity ≥ 99.9%

  Scenario: 3 locales Legal local-reviewed
    Given DPA template v1.0.0 in en-US + pt-BR + es-419
    When Legal review per locale completed
    Then specs/_audits/2026-XX-XX-legal-review-dpa-v1.md committed com sign-off per locale
    And native speaker review per locale completed

  Scenario: Legal review CI gate on PR touching DPA
    Given PR modifies legal/dpa/v1.0.0.en-US.md
    When CI runs
    Then legal-review-required flag triggered
    And merge blocked until Legal Counsel sign-off

  Scenario: JWT receipt forge attempt rejected
    Given pentester captures JWT + tries forge new with different fields
    When verify endpoint validates
    Then signature reject (HS256 fails)
    And audit emit "corelink.onboarding.dpa_jwt_forge_attempt"

  Scenario: Hash tamper attempt rejected
    Given pentester modifies DPA text mid-transit
    When backend re-compute hash
    Then mismatch detected + 422 rejection
    And audit emit "corelink.onboarding.dpa_hash_mismatch"
```

## 9. Design Decisions

### 9.1 Why locale from cookie corelink_locale (Lote 10.16 canonical)

- Accept-Language drift: user em pt-BR mas browser Accept-Language=en-US (multi-locale users common); cookie reflects active rendered locale após user switch via footer.
- S-16 WI-S16-001 sets cookie via Next.js middleware; S-16 WI-S16-003 consent capture uses cookie; S-19 DPA capture aligns.
- Backend validates match contra cookie at request time (NÃO Accept-Language header); mismatch = 422.

### 9.2 Why JWT receipt **ES256** (ECDSA P-256 asymmetric) per-region key — Lote 10.19 codex P1 canonical fix; ES256 asymmetric (ECDSA P-256; Lote 10.19 codex P1 canonical fix) era incoherent com offline verify via public-key endpoint (codex P1 finding); ES256 asymmetric enables offline verify (private key signs em backend; public key endpoint `/v1/dpa/verify-key/{region}.pub` serves verifying key for client/auditor) separate key_id (data_model.md §4.1)

- DPA receipt JWT signing key MUST be separate from PAT format S-03 hybrid HMAC + Argon2id (different key family + rotation cadence).
- Per-region signing key for INV-DATA-RESIDENCY alignment (S-14); receipt verifiable em region of origin.
- key_id index permits rotation without breaking historical receipts (overlap canonical).

### 9.3 Why scroll-gate anti-dismiss UX

- LGPD Art. 8º + GDPR Art. 7§2: consent must be unambiguous + freely given + specific + informed.
- Auto-submit ou button enabled antes de scroll = invalid consent (UX dark pattern).
- Scroll-gate: explicit user action; auditable; aligned with industry standard (Stripe Atlas + Auth0 + Linear).

### 9.4 Why 3 locales (en-US + pt-BR + es-419) Legal local-reviewed

- en-US: primary market US/EU prospect.
- pt-BR: Brazilian market (LGPD enforcement risk; native speaker mandatory).
- es-419: LATAM market (broader Spanish dialect coverage).
- Automated translation NOT acceptable: legal terms accuracy critical (e.g., "controller" vs "controlador" vs "responsável pelo tratamento" — LGPD specific terminology).
- Legal local review per locale mandatory + native speaker review.

### 9.5 Why reuse S-11 consent ledger pattern (não novo schema)

- S-11 consent ledger D1 schema + verify endpoint R-S11-9 already canonical (Lote 10.11.0-bis canonical pós refactor).
- DPA é consent type with `purpose=DataProcessingAgreement` enum extended.
- Avoid duplication; INV-CONSENT-PROOF-VERIFIABLE CRITICAL canonical foundation.

### 9.6 Why deterministic UUID v7 wording_id (não A/B test variant)

- A/B test infrastructure deferred pós-GA (Optimizely/equiv).
- Deterministic UUID from notice_version hash = stable across users; analytics-friendly.
- Future A/B test: feature flag `feature_flag_dpa_ab_test` integration.

### 9.7 ADR potencial?

- Não. Patterns reused (S-11 consent ledger + S-16 consent capture pós Lote 10.16 + JWT HS256 per-region key data_model.md §4.1). No novel architecture decision.

## 10. Completeness Criteria SOTA

- [ ] **10.s19.002.1** DPA template v1.0.0 em 3 locales Legal local-reviewed (EVT-044).
- [ ] **10.s19.002.2** 6-field payload captured + backend persisted (EVT-049) — CTRL-PRIV-CONSENT-001..006 reflection.
- [ ] **10.s19.002.3** Locale match cookie corelink_locale (Lote 10.16 canonical) enforced backend (EVT-049).
- [ ] **10.s19.002.4** Scroll-gate anti-dismiss UX enforced (button disabled until scroll to bottom).
- [ ] **10.s19.002.5** EVT-049 evidence imutável R2 Object Lock 7y retention (EVT-049).
- [ ] **10.s19.002.6** JWT receipt **ES256** (ECDSA P-256 asymmetric) per-region key — Lote 10.19 codex P1 canonical fix; ES256 asymmetric (ECDSA P-256; Lote 10.19 codex P1 canonical fix) era incoherent com offline verify via public-key endpoint (codex P1 finding); ES256 asymmetric enables offline verify (private key signs em backend; public key endpoint `/v1/dpa/verify-key/{region}.pub` serves verifying key for client/auditor) separate key_id per data_model.md §4.1 (EVT-024).
- [ ] **10.s19.002.7** Verify endpoint round-trip integrity ≥ 99.9% (S-11 R-S11-9 reuse) — INV-CONSENT-PROOF-VERIFIABLE reforced.
- [ ] **10.s19.002.8** Legal review CI gate on PR touching DPA + quarterly audit.
- [ ] **10.s19.002.9** Native speaker review per locale + Legal local review per locale.
- [ ] **10.s19.002.10** Confirmation modal + JWT receipt + emailed via SES.
- [ ] **10.s19.002.11** Métricas Prometheus snake_case (locale_mismatch + acceptance_total + verify_round_trip).

## 11. DoD

- [ ] DPA click-through tested staging em 3 locales.
- [ ] Legal review committed per locale.
- [ ] 6-field payload schema validated zod.
- [ ] Scroll-gate anti-dismiss UX property test 100 random interactions.
- [ ] EVT-049 evidence retention 7y enforced.
- [ ] JWT receipt verify endpoint round-trip ≥ 99.9%.
- [ ] Tests: unit (hash + UUID + JWT signing) + integration (E2E DPA flow → verify) + 4+ negative scenarios.
- [ ] INV-CONSENT-PROOF-VERIFIABLE reforced.

## 12. Invariants Validated

- **CTRL-PRIV-CONSENT-001..006** reflection (UI capture per S-16 WI-S16-003; backend S-11 enforce).
- **CTRL-PRIV-CONSENT-005** (locale match) enforced backend; UI reflects `corelink_locale` cookie.
- **INV-CONSENT-PROOF-VERIFIABLE** (CRITICAL — registry §3.12 herdada S-11): DPA verify endpoint round-trip integrity ≥ 99.9%.
- **CTRL-PRIV-001** (zero PII em client-side logs) reforced via safeLog wrapper.
- Não introduz INV nova (consumer + reuso S-11 pattern; per spec contract §8 INV-ONBOARD-DPA-FIRST owned by WI-S19-004).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| DPA template en-US | `legal/dpa/v1.0.0.en-US.md` | Markdown+frontmatter |
| DPA template pt-BR | `legal/dpa/v1.0.0.pt-BR.md` | Markdown+frontmatter |
| DPA template es-419 | `legal/dpa/v1.0.0.es-419.md` | Markdown+frontmatter |
| Legal review report | `specs/_audits/2026-XX-XX-legal-review-dpa-v1.md` | Markdown |
| DPA capture crate | `crates/corelink-onboarding-dpa/` | Rust |
| DPA UI page | `apps/web/src/app/onboarding/dpa/page.tsx` | TypeScript |
| 6-field capture lib | `apps/web/src/app/onboarding/dpa/capture-flow.tsx` | TypeScript |
| JWT receipt signing | `crates/corelink-onboarding-dpa/src/jwt_receipt.rs` | Rust |
| Verify endpoint | `crates/corelink-onboarding-dpa/src/verify.rs` | Rust |
| Public-key endpoint | `crates/corelink-onboarding-dpa/src/public_keys.rs` | Rust |

## 14. Quality Standards SOTA

- **14.s19.002.1** 6-field payload schema validated zod backend.
- **14.s19.002.2** Test coverage ≥ 90% (capture + JWT signing + verify).
- **14.s19.002.3** SAST: html2canvas + uuid + jsonwebtoken-js zero CVEs HIGH/CRITICAL.
- **14.s19.002.4** Plain-language Flesch-Kincaid ≤ 8 per locale CI gate (S-16 alignment).
- **14.s19.002.5** EVT-049 retention 7y R2 Object Lock.
- **14.s19.002.6** JWT receipt round-trip integrity ≥ 99.9%.
- **14.s19.002.7** Legal review CI gate on PR touching DPA.
- **14.s19.002.8** Native speaker review per locale + Legal local review per locale (mandatory).

## 15. Chaos Experiments

1. **Locale forge attempt**: pentester sets cookie en-US + payload pt-BR; verify rejected.
2. **Hash tamper attempt**: pentester modifies DPA text mid-transit; verify rejected.
3. **JWT receipt forge attempt**: pentester captures + forges; verify rejected.
4. **Scroll-gate bypass attempt**: pentester injects DOM event; verify scroll-gate enforced.
5. **Pre-check injection attempt**: pentester injects pre-checked checkbox state; verify unchecked default enforced.
6. **Replay JWT receipt 1y later**: verify accepted (intentional; receipt verifiable post-facto).
7. **Public-key endpoint 404 region**: verify graceful error + fallback path.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (sprint S-19 §14):

- [ ] All Gherkin green.
- [ ] 6-field payload + JWT receipt + 3 locales Legal local-reviewed.
- [ ] EVT-049 retention 7y R2 Object Lock.
- [ ] Verify endpoint round-trip ≥ 99.9%.
- [ ] INV-CONSENT-PROOF-VERIFIABLE reforced.
- [ ] Métricas DASH-ONBOARDING emitting.
- [ ] Cost regression gate: DPA capture overhead ≤ 200ms p99.
- [ ] 11 sign-offs canonical documented.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | DPA template v1.0.0 em 3 locales + frontmatter | 4h |
| ST-002 | Legal local review per locale + native speaker | 6h |
| ST-003 | DPA UI scroll-gate + 6-field capture | 4h |
| ST-004 | Backend POST /api/onboarding/dpa-accept handler | 3h |
| ST-005 | JWT receipt **ES256** (ECDSA P-256 asymmetric) per-region key — Lote 10.19 codex P1 canonical fix; ES256 asymmetric (ECDSA P-256; Lote 10.19 codex P1 canonical fix) era incoherent com offline verify via public-key endpoint (codex P1 finding); ES256 asymmetric enables offline verify (private key signs em backend; public key endpoint `/v1/dpa/verify-key/{region}.pub` serves verifying key for client/auditor) signing | 2h |
| ST-006 | EVT-049 evidence R2 Object Lock 7y | 2h |
| ST-007 | Verify endpoint reuse S-11 R-S11-9 pattern | 2h |

**Total Optimistic**: ~23h. **PERT** (O=14h, M=22h, P=36h per spec contract §12): **23.0h**.

## 18. Dependencies

### Hard blockers
- WI-S19-001 SEALED (signup orchestration + tenant atomicity foundation).
- S-11 SEALED (consent ledger D1 + verify endpoint R-S11-9 canonical).
- S-16 WI-S16-003 SEALED (consent UI 6-field pattern pós Lote 10.16).
- Legal Counsel availability for 3 locales review (~2 weeks lead).

### Soft blockers
- S-09 SEALED (R2 Object Lock 7y retention pattern).
- S-13 SEALED (admin plane secret rotation JWT signing key per-region).

### Outbound
- WI-S19-003 (DPA versioning depends on this WI semver baseline).
- WI-S19-004 (Stripe activation requires DPA signed first; INV-ONBOARD-DPA-FIRST).
- WI-S19-006 (closing PRR + funnel instrumentation).

## 19. Effort PERT

O: 14h, M: 22h, P: 36h → PERT **23.0h** (per spec contract §12; DPA capture + 3 locales + Legal review).

## 20. Time-boxing

**26h hard limit owner**. Legal review **6h dedicated** (engagement com Legal Counsel externo). Se exceder: split em sub-WI (capture flow vs Legal review).

## 21. Observability

Métricas Prometheus snake_case underscored (cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado; NUNCA per-tenant labels):

- `corelink_onboarding_dpa_acceptance_total{locale, version, plan}` (counter).
- `corelink_onboarding_dpa_locale_mismatch_total{plan}` (counter; alert > 5/dia).
- `corelink_onboarding_dpa_hash_mismatch_total{plan}` (counter; alert > 0).
- `corelink_onboarding_dpa_jwt_receipt_signed_total{region, plan}` (counter).
- `corelink_onboarding_dpa_verify_total{outcome, plan}` (outcome ∈ ok|sig_invalid|key_not_found|round_trip_mismatch).
- `corelink_onboarding_dpa_verify_round_trip_integrity_ratio` (gauge ≥ 99.9% target).
- `corelink_onboarding_dpa_jwt_forge_attempt_total{plan}` (alert > 0; SEV-2).
- `corelink_onboarding_dpa_evt_049_evidence_total{outcome, plan}` (outcome ∈ ok|client_fallback|server_fallback).

Dashboard DASH-ONBOARDING painel "DPA Click-Through" (5-panel: acceptance rate per locale + locale mismatch + hash mismatch + verify round-trip integrity + JWT forge attempts).

## 22. Cost Analysis

- Legal review engagement externo: ~$5-10k per locale × 3 = $15-30k one-time + quarterly review ~$2k × 4 = $8k/yr.
- R2 Object Lock 7y retention: ~$0.015/GB/mês × estimated 50GB/yr = $0.75/mês.
- SES emails (DPA receipts): ~$0.10 / 1000 emails = ~$5/mês.
- Total: ~$25/mês incremental + $15-30k Legal one-time.

## 23. API Contract

- `POST /api/onboarding/dpa-accept` — capture DPA acceptance com 6-field consent; returns 200 (success + JWT receipt) | 422 (locale_mismatch | hash_mismatch | malformed) | 401 (auth invalid).
- `POST /api/onboarding/dpa-verify` body `{receipt_jwt}` — público stateless verify (Lote 10.19 codex P1 canonical fix; receipt JWT body NÃO URL params per CTRL-CRED-001 zero secrets em URL); returns 200 (match) | 401 (signature invalid) | 404 (key_not_found).
- `GET /v1/public/keys/dpa-receipt/{region}.pub` — public-key endpoint for offline verify; returns 200 (PEM) | 404.

## 24. Post-mortem Hooks

- Locale mismatch sustained > 7d → CTRL-PRIV-CONSENT-005 review + cookie middleware fix.
- Hash mismatch detected em prod → SEV-2 post-mortem + integrity review.
- JWT receipt forge attempt detected → SEV-2 + Security incident.
- Pre-checked consent shipped em prod → CRITICAL post-mortem + Privacy + Legal.
- Legal review missed PR touching DPA → CRITICAL post-mortem + Legal + retroactive re-acceptance broadcast.
- Translation legal terms incorrect detected → SEV-2 post-mortem + Legal local review re-cycle.

## 25. Rollback / Recovery

DPA capture regression detected → revert via CF Pages rollback; existing acceptances retained em D1; verify endpoint continues to work.

## 26. Security & Privacy

**STRIDE delta** (vs S-16 WI-S16-003 baseline):
- **Spoofing**: locale match cookie corelink_locale enforced; user authenticated via Clerk.
- **Tampering**: notice_text_hash sha256 backend re-computes; JWT receipt HS256 signature reject mismatch.
- **Repudiation**: 6-field payload + EVT-049 evidence + JWT receipt + R2 Object Lock 7y forensic trail.
- **Information disclosure**: PAT NUNCA em DPA payload; safeLog allowlist; screenshot redacts user IP server-side.
- **DoS**: DPA accept rate-limited (≤ 5/min per user via Cloudflare).
- **Elevation of privilege**: DPA acceptance is opt-in only; no privilege escalation possible.

**LINDDUN delta**:
- **Linkability**: wording_id deterministic UUID v7 (no PII); locale (cardinality bounded).
- **Identifiability**: payload references tenant_id + user_id_hash sha256; screenshot redacts IP.
- **Non-repudiation**: 6-field payload + JWT receipt + EVT-049 + audit chain S-09.
- **Detectability**: locale mismatch alerted; hash mismatch alerted; JWT forge alerted.
- **Disclosure**: DPA text public per locale; user PII never em payload (only references).
- **Unawareness**: notice plain-language Flesch-Kincaid ≤ 8 + native speaker review.
- **Non-compliance**: GDPR Art. 7 + Art. 28 + LGPD Art. 8º + Art. 39 + CCPA §1798.140(v) + EDPB SCCs satisfied.

## 27. Knowledge Transfer

- Tech talk (1h): "CoreLink DPA Click-Through — 6-Field Consent + JWT Receipt + 3 Locales".
- Doc `docs/internal/dpa-click-through.md` — overview + Legal mapping + Lote 10.16 canonical.
- Onboarding test (3 questions): 6-field payload + locale match + JWT receipt key separation rationale.

## 28. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Architect (Privacy + Legal SME specialization) | _TBD_ | _pending_ |
| 4 | Privacy Officer | _TBD_ | _pending_ |
| 5 | Legal Counsel | _TBD_ | _pending_ |
| 6 | Engineer (S-19 lead) | _TBD_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ |
| 9 | SRE Lead | _TBD_ | _pending_ |
| 10 | Compliance Officer | _TBD_ | _pending_ |
| 11 | Sales lead | _TBD_ | _pending_ |

> HIGH_RISK lane (per framework §33.5.4.3 + ADR-0034 solo-tier waiver): 11 canonical sign-offs.

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S19-002 (cycle 12.S19.0; DPA click-through 6-field + JWT receipt + 3 locales Legal local review pós Lote 10.16 canonical). |

## 30. Anti-patterns evitados

- Locale from Accept-Language header (Lote 10.16 violation).
- Auto-submit DPA (LGPD Art. 8º + GDPR Art. 7§2 violation).
- Pre-checked consent checkbox (GDPR Art. 7§2 invalid).
- Automated translation (Legal local review mandatory).
- JWT receipt signing key shared com PAT format S-03 (data_model.md §4.1 violation).
- Skip Legal review on PR touching DPA.
- Skip native speaker review per locale.
- DPA verify endpoint behind auth (must be público stateless).
- EVT-049 retention < 7y.

---

**Fim WI-S19-002.**
