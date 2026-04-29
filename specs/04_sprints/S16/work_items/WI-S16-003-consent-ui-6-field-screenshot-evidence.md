---
id: "WI-S16-003"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "STANDARD"
parent: "S-16"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "PRIVACY-MODEL"
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s16", "ui", "consent", "ctrl-priv-consent", "lgpd", "gdpr", "evt-012", "screenshot-evidence", "standard"]
---

# WI-S16-003 — Consent Management UI Captura 6-field Proof of Informed (CTRL-PRIV-CONSENT-001..006 Reflection conforme S-11 R-S11-7 Schema): `notice_text_hash = sha256(notice_text)` Calculado em Frontend via Web Crypto API + `notice_version` Semver from mdx Frontmatter + `locale` from **rendered active locale** (cookie `corelink_locale` set by Next.js middleware; Lote 10.16 codex P0 canonical fix — não `navigator.language`/`Accept-Language` header) (CTRL-PRIV-CONSENT-005 Enforcement em Backend Valida Match) + `wording_id` UUID v7 per A/B Test Variant (Deterministic per `notice_version` se No A/B Test) + `ui_capture_ts` Browser `Date.now()` ms Epoch + `submission_ts` Server-assigned ts em Backend; **Screenshot Evidence EVT-012 Automática per Consent Capture for Legal Evidence Retention**: DOM-to-image Client-side Primary (html2canvas ou Equivalent) + Server-side Fallback via Headless Chrome Render Proof se Browser API Falhar (R-S11-X Risco Mitigation); Render Notice Versioned `legal/privacy-notice/v<M.m>.md` via @next/mdx; Plain-language Notice (Flesch-Kincaid Grade ≤ 8 Verified em CI per Locale + Native Speaker Review per Locale em WI-S16-006); Click Capture Event-bound (No Auto-submit; Explicit User Action Required); Consent Revoke Flow Integrated em DSR Form (WI-S16-004) per LGPD Art. 18 IX + GDPR Art. 7§3

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-16](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S16-003 |
| Título | Consent management UI captura 6-field proof of informed + screenshot evidence EVT-012. |
| Sprint | S-16 |
| Lane | STANDARD |
| Forcing factors | none (UI é capture frontend; CTRL-PRIV-CONSENT enforcement backend S-11; locale match validation backend) |

## 1. Intent

Entregar **consent management UI compliance-grade** com captura full payload conforme S-11 R-S11-7 schema (CTRL-PRIV-CONSENT-001..006 reflection): 6-field proof of informed + screenshot evidence EVT-012 automática per consent capture for legal evidence retention.

```typescript
// File: apps/web/src/app/consent/capture-flow.tsx
async function captureConsent(noticeText: string, noticeVersion: string) {
  const noticeHash = await sha256(noticeText); // Web Crypto API
  const wordingId = abTestVariantId() ?? deterministicUuid(noticeVersion);
  // Lote 10.16 codex P0 fix: locale comes from RENDERED active locale (cookie `corelink_locale` set by middleware), NOT navigator.language/Accept-Language header. Otherwise consent-rendered-in-pt-BR gets recorded as Accept-Language=en-US = proof-of-informed broken.
  const locale = getCookie('corelink_locale') || document.documentElement.lang || 'en-US';
  const uiCaptureTs = Date.now();

  // Screenshot evidence EVT-012 (client-side primary)
  let screenshotBlob: Blob | null = null;
  try {
    screenshotBlob = await html2canvas(document.body).then(c => new Promise(r => c.toBlob(r, 'image/png')));
  } catch (e) {
    // Server-side fallback (headless Chrome) triggered post-submit
    safeLog('warn', 'screenshot_client_failed', { error_code: 'EVT_012_CLIENT_FALLBACK' });
  }

  const payload = { notice_text_hash: noticeHash, notice_version: noticeVersion, locale, wording_id: wordingId, ui_capture_ts: uiCaptureTs };
  const formData = new FormData();
  formData.append('payload', JSON.stringify(payload));
  if (screenshotBlob) formData.append('screenshot', screenshotBlob);

  const res = await fetch('/api/consent', { method: 'POST', body: formData });
  return res.json(); // { consent_id, submission_ts, screenshot_url }
}
```

## 2. Narrative

Consent capture é o frente compliance-critical: LGPD Art. 8º (consentimento) + GDPR Art. 7 (conditions for consent) exigem **proof of informed** auditável. CoreLink S-16 entrega 6-field payload + screenshot evidence (zero competitors). Backend S-11 (consent ledger D1 + verify endpoint R-S11-9) valida match locale + hash + version + wording_id; UI é capture frontend que mantém CTRL-PRIV-CONSENT-005 (**locale match com rendered locale cookie**, não Accept-Language header — Lote 10.16 codex P0 canonical fix; cookie reflects active rendered locale após user switch via footer).

Este WI implementa: render notice mdx versioned, calcular hash sha256 frontend, capturar locale + wording_id + timestamps, screenshot evidence client-side primary + server-side fallback, click capture event-bound (no auto-submit), consent revoke flow integrated DSR.

**Risk justification STANDARD lane**:
- UI é capture frontend; CTRL-PRIV-CONSENT enforcement reside backend S-11.
- Screenshot evidence EVT-012 já é S-11 dependency (verify endpoint R-S11-9 valida).
- Locale match canonical CTRL-PRIV-CONSENT-005 backend valida (UI reflects).
- Não introduz cripto-load-bearing controle novel.

## 3. Customer Impact & Journey

**Persona 1 — End-user (consent giver)**:
- Render notice plain-language Flesch-Kincaid ≤ 8 + native speaker review per locale.
- Click capture event-bound (explicit user action; no auto-submit dark patterns).
- Confirmation displayed após submit ("Your consent was recorded; you can revoke anytime em /settings/dsr").

**Persona 2 — Privacy Officer cliente / Compliance auditor**:
- 6-field payload audit trail forensic-grade (`notice_text_hash` + `notice_version` + `locale` + `wording_id` + `ui_capture_ts` + `submission_ts`).
- Screenshot evidence EVT-012 retained for legal evidence retention.
- Backend S-11 verify endpoint R-S11-9 confirms backend match.

**Persona 3 — A/B test analyst (future post-GA)**:
- `wording_id` UUID per variant tracks consent rate per wording.
- Consent revoke rate per variant trackable.

## 4. Capability Mapping

- **CAP-UI-004** (Consent management UI) — IMPLEMENTA primary; 6-field payload + screenshot EVT-012.
- Trace: `_spec_contract.md §4 + §5.3 (R-S16-6)` + `privacy_model.md` (CTRL-PRIV-CONSENT-001..006 + CTRL-PRIV-001) + `S-11` (consent ledger D1 + verify endpoint R-S11-9 + R-S11-7 schema).

## 5. Tipo

Feature WI; STANDARD lane; consent capture compliance-critical.

## 6. Escopo

### 6.1 In-scope

1. **Consent capture flow** em `apps/web/src/app/consent/`:
   - Route `/consent` (rendered ao first-time user antes de any data processing).
   - Render mdx `legal/privacy-notice/v<M.m>.md` (latest semver) via @next/mdx.
   - Frontmatter exposes `notice_version` (semver) + `locale_supported` array.
   - Plain-language Flesch-Kincaid grade ≤ 8 verified em CI per locale (using `text-statistics` ou equivalent).

2. **6-field payload capture** (CTRL-PRIV-CONSENT-001..006 reflection conforme S-11 R-S11-7):
   - **Field 1 — `notice_text_hash`**: sha256(rendered_notice_text) via Web Crypto API `crypto.subtle.digest('SHA-256', encoder.encode(text))`; computed em frontend; envia ao backend.
   - **Field 2 — `notice_version`**: semver string from mdx frontmatter (e.g., "1.2.0").
   - **Field 3 — `locale` (Lote 10.16 codex P0 canonical fix)**: from **rendered active locale** — cookie `corelink_locale` (set by Next.js middleware during locale switch) primary; `document.documentElement.lang` fallback (server-rendered HTML lang attribute). **NÃO** `navigator.language` / `Accept-Language` (rejeitado: usuário pode renderizar consent em pt-BR via cookie switcher mesmo com browser Accept-Language=en-US; backend deve validar match contra rendered locale, não header). CTRL-PRIV-CONSENT-005 enforcement em backend valida `consent.locale` matches **rendered locale cookie at request time** (not `Accept-Language` header).
   - **Field 4 — `wording_id`**: UUID v7 per A/B test variant; if no A/B test running, deterministic UUID derived from `notice_version` (stable across users).
   - **Field 5 — `ui_capture_ts`**: browser `Date.now()` ms epoch (captured at click moment).
   - **Field 6 — `submission_ts`**: server-assigned ts em backend (POST `/api/consent` handler).
   - Backend POST `/api/consent` payload schema validated via zod ou equivalent; rejects malformed.

3. **Screenshot evidence EVT-012 automática** per consent capture (for legal evidence retention):
   - **Client-side primary**: `html2canvas` (or `dom-to-image-more`) renders DOM to canvas → blob `image/png`.
   - **Server-side fallback** se client API falhar (R-S11-X risco mitigation): backend triggers headless Chrome render proof via Playwright/Puppeteer; uses URL + auth context to re-render page server-side; capture screenshot blob.
   - Blob uploaded to R2 audit bucket (S-09 alignment); `screenshot_url` returned in response.
   - Privacy considerations: screenshot includes notice text + user IP redacted (server-side post-process); retention policy per S-11 (≥ 5 years legal evidence).

4. **Click capture event-bound** (no auto-submit; explicit user action required):
   - Button "I have read and consent to the privacy notice v<M.m>" disabled by default.
   - Button enabled only após user scrolls to end of notice (scroll position tracking).
   - Click handler is synchronous capture (no debounce; no auto-submit dark patterns).
   - Property test: 100 random page interactions verify no consent submit without click.

5. **Confirmation display + receipt** após submit:
   - Modal "Your consent was recorded. Consent ID: <consent_id>. You can revoke anytime em /settings/dsr."
   - JWT receipt downloadable (HS256 signed em backend; reuses S-11 receipt pattern).
   - Email confirmation via SES (S-11 alignment).

6. **Consent revoke flow** integrated em DSR form (WI-S16-004):
   - Per LGPD Art. 18 IX + GDPR Art. 7§3 (consent revoke right).
   - Cross-link em `/consent` page: "To revoke prior consent, use DSR form at /settings/dsr".
   - On revoke, backend S-11 marks consent as revoked + emits audit event S-09.

7. **Locale-aware rendering**:
   - Notice mdx per locale (`legal/privacy-notice/v1.2.0.{en-US,pt-BR,es-419}.md`).
   - Locale detection via `corelink_locale` cookie set by Next.js middleware (footer switcher updates cookie + `document.documentElement.lang` attribute consistently); fallback `Accept-Language` header apenas em first-visit antes de cookie set.
   - **CTRL-PRIV-CONSENT-005 enforcement (Lote 10.16 codex P0 canonical fix)**: backend valida `consent.locale` field matches **rendered locale cookie** (`corelink_locale`) at request time — NÃO `Accept-Language` header (header pode diverge da locale ativamente renderizada se usuário fez switch via cookie). Mismatch = 422 + audit emit `corelink.consent.locale_mismatch`.

8. **`wording_id` UUID generator**:
   - Library: `uuid` v9+ with `v7()` (time-ordered).
   - A/B test variant integration: if `feature_flag_consent_ab_test` enabled, picks variant from cookie; else deterministic UUID from `notice_version` hash.
   - Stable across re-renders (idempotent per session).

### 6.2 Out-of-scope (deferred)

- DSR self-service form (WI-S16-004; consent revoke flow integrated there).
- Audit log viewer (WI-S16-005; consent capture events queryable there).
- Component library + a11y full (WI-S16-006).
- E2E tests + closing PRR (WI-S16-007).
- A/B test infrastructure (Optimizely/equiv. — pós-GA).

## 7. Anti-Scope

- Auto-submit consent (dark pattern; explicit click required).
- Consent capture sem 6-field payload (CTRL-PRIV-CONSENT-001..006 violation).
- Screenshot evidence EVT-012 missing (S-11 R-S11-9 verify falha; compliance gap).
- Locale mismatch tolerated (CTRL-PRIV-CONSENT-005 violation).
- Consent revoke sem audit event (S-09 alignment).
- Notice text dense legalese (Flesch-Kincaid > 8; UX abandonment).
- Pre-checked consent checkbox (LGPD Art. 8º + GDPR Art. 7§2 violation).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Consent management UI 6-field capture + EVT-012 screenshot evidence

  Scenario: 6-field payload captured + backend persisted
    Given user navigates /consent com Accept-Language: pt-BR
    When user reads notice + clicks "I consent"
    Then payload contains notice_text_hash + notice_version + locale=pt-BR + wording_id + ui_capture_ts
    And backend POST /api/consent accepts payload
    And submission_ts assigned server-side
    And consent_id returned

  Scenario: notice_text_hash sha256 computed frontend
    Given notice mdx v1.2.0 rendered
    When hash computed via Web Crypto API
    Then hash matches sha256(notice_rendered_text)
    And backend re-computes + matches CTRL-PRIV-CONSENT-001 enforcement

  Scenario: Locale matches rendered cookie CTRL-PRIV-CONSENT-005 (Lote 10.16 codex P0 fix)
    Given corelink_locale cookie = pt-BR (set by middleware after user clicked locale switcher)
    When consent submitted with locale=pt-BR
    Then backend validates match
    When locale=en-US sent (mismatch)
    Then backend rejects com error "locale_mismatch"

  Scenario: wording_id UUID v7 per variant
    Given feature_flag_consent_ab_test enabled + variant A
    When consent submitted
    Then wording_id matches variant A UUID
    Given feature flag disabled
    Then wording_id deterministic from notice_version hash

  Scenario: Screenshot evidence EVT-012 client-side primary
    Given user submits consent
    When html2canvas captures DOM
    Then blob image/png uploaded to backend
    And screenshot_url returned
    And persisted em R2 audit bucket

  Scenario: Screenshot evidence server-side fallback
    Given client html2canvas fails (browser quirk)
    When backend receives consent payload sem screenshot
    Then headless Chrome triggered via backend
    And server-side screenshot rendered + persisted
    And screenshot_url returned

  Scenario: Click capture event-bound (no auto-submit)
    Given user navigates /consent
    When notice rendered
    Then "I consent" button disabled until user scrolls to end
    And button click is synchronous capture (no debounce)
    And no auto-submit em page load

  Scenario: Plain-language notice Flesch-Kincaid ≤ 8 per locale
    Given notice mdx pt-BR rendered
    When Flesch-Kincaid grade computed em CI
    Then grade ≤ 8
    And native speaker review per locale (em WI-S16-006)

  Scenario: Confirmation display + JWT receipt + emailed
    Given consent submitted successful
    Then modal "Your consent was recorded. Consent ID: <id>" displayed
    And JWT receipt downloadable
    And email confirmation sent via SES

  Scenario: Consent revoke flow cross-linked DSR
    Given user reads /consent
    Then cross-link "To revoke prior consent, use DSR form at /settings/dsr"
    When user navigates /settings/dsr (WI-S16-004)
    Then DSR form direito #6 consent_revoke available

  Scenario: Backend verify endpoint R-S11-9 confirms 6-field
    Given consent_id persisted
    When backend verify endpoint queried
    Then 6-field payload retrieved + matches captured fields
    And screenshot_url retrievable for audit

  Scenario: Pre-checked consent rejected (LGPD Art. 8º + GDPR Art. 7§2)
    Given consent checkbox state pre-checked
    When form rendered
    Then unchecked default (explicit opt-in required)
    And submit disabled until user explicitly checks
```

## 9. Design Decisions

### 9.1 Why 6-field payload (não 3 ou 9)

- 3-field (text+version+ts) insufficient for forensic legal evidence.
- 6-field covers: integrity (hash) + version + locale (LGPD Art. 8º + GDPR Art. 7) + variant tracking (A/B) + dual timestamps (UI capture + server submission).
- 9-field scope creep (e.g., GeoIP + browser fingerprint) = privacy concerns + cardinality.

### 9.2 Why Web Crypto API (não JS lib like `crypto-js`)

- Native browser API; no bundle bloat.
- Audited browser implementation (vs lib supply chain risk).

### 9.3 Why html2canvas client-side primary + server-side fallback

- Client-side primary: cheap; works ≥ 95% browsers.
- Server-side fallback: handles edge cases (browser quirks, CSP nonce mismatch, dom mutations during capture).
- Compliance baseline: screenshot evidence sempre captured (R-S11-X risco).

### 9.4 Why click capture event-bound (não checkbox + form submit)

- LGPD Art. 8º + GDPR Art. 7§2: consent must be unambiguous + freely given + specific.
- Pre-checked checkboxes explicitly invalid per GDPR.
- Click event-bound: explicit user action; auditable.

### 9.5 Why mdx versioned (não plain markdown)

- mdx allows JSX components em notice (e.g., `<DPALink />`, `<ReadMoreButton />`).
- Frontmatter versioning (semver) standard convention.
- @next/mdx native support.

### 9.6 Why wording_id UUID v7 (não v4)

- v7 time-ordered; database index-friendly.
- A/B test analytics easier (chronological grouping).

### 9.7 ADR potencial?

- Não. Patterns reused (Web Crypto + html2canvas + mdx + UUID v7 standard). No novel architecture decision.

## 10. Completeness Criteria

- [ ] **10.s16.003.1** 6-field payload captured + backend persisted via POST /api/consent (EVT-049).
- [ ] **10.s16.003.2** notice_text_hash sha256 computed frontend Web Crypto API + backend re-computes + match.
- [ ] **10.s16.003.3** Locale match CTRL-PRIV-CONSENT-005 enforced em backend (EVT-049).
- [ ] **10.s16.003.4** wording_id UUID v7 per A/B variant; deterministic se no A/B test.
- [ ] **10.s16.003.5** Screenshot evidence EVT-012 client-side primary + server-side fallback (EVT-012).
- [ ] **10.s16.003.6** Click capture event-bound (no auto-submit; property test passes).
- [ ] **10.s16.003.7** Plain-language notice Flesch-Kincaid grade ≤ 8 per locale verified em CI.
- [ ] **10.s16.003.8** Confirmation modal + JWT receipt + emailed via SES.
- [ ] **10.s16.003.9** Consent revoke flow cross-linked DSR form (WI-S16-004).
- [ ] **10.s16.003.10** Backend verify endpoint R-S11-9 confirms 6-field forwards correctly.

## 11. DoD

- [ ] Consent capture flow tested staging.
- [ ] 6-field payload schema validated zod.
- [ ] Screenshot evidence client-side + server-side fallback tested.
- [ ] Plain-language Flesch-Kincaid CI gate verde per locale.
- [ ] Consent revoke flow cross-linked DSR.
- [ ] Tests: unit (hash compute + UUID gen + screenshot blob handling) + integration (E2E /consent → backend persist → verify endpoint round-trip) + 4+ negative scenarios.

## 12. Invariants Validated

- **CTRL-PRIV-CONSENT-001..006** reflection (UI captura 6-field; backend S-11 enforce).
- **CTRL-PRIV-CONSENT-005** (locale match) enforced em backend; UI reflects.
- **CTRL-PRIV-001** (zero PII em client-side logs) reforced via safeLog wrapper.
- **Não introduz INVs novas** (UI é consumer; per spec contract §8).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Consent capture page | `apps/web/src/app/consent/page.tsx` | TypeScript |
| Consent capture flow logic | `apps/web/src/app/consent/capture-flow.tsx` | TypeScript |
| Consent API route | `apps/web/src/app/api/consent/route.ts` | TypeScript |
| Screenshot capture lib | `apps/web/src/lib/screenshot-evidence.ts` | TypeScript |
| Hash compute lib | `apps/web/src/lib/sha256.ts` | TypeScript |
| Notice mdx (per locale) | `legal/privacy-notice/v1.2.0.{en-US,pt-BR,es-419}.md` | Markdown+JSX |
| Flesch-Kincaid CI script | `scripts/check_readability.py` | Python |
| Server-side fallback worker | `apps/web/src/app/api/consent/screenshot-fallback/route.ts` | TypeScript |

## 14. Quality Standards

- **14.s16.003.1** 6-field payload schema validated zod em backend.
- **14.s16.003.2** Test coverage ≥ 85% (hash + UUID + screenshot + capture flow).
- **14.s16.003.3** SAST: html2canvas + uuid lib zero CVEs HIGH/CRITICAL.
- **14.s16.003.4** Plain-language Flesch-Kincaid ≤ 8 per locale CI gate.
- **14.s16.003.5** Screenshot evidence EVT-012 sustained ≥ 99% capture rate (client + server fallback).
- **14.s16.003.6** Backend verify endpoint R-S11-9 round-trip integrity ≥ 99.9%.

## 15. Test Plan

### Unit tests (≥ 85% coverage)
- Hash compute: sha256 matches reference em multiple inputs.
- UUID v7 gen: time-ordered + valid format.
- Screenshot lib: html2canvas wrapper handles errors; server-side fallback triggered.
- Capture flow: 6-field payload assembled + sent to backend.

### Integration tests (E2E staging)
- Full /consent flow: render notice → user reads → click → backend persist → verify endpoint round-trip.
- Locale match: pt-BR + en-US + es-419 each validated.
- Screenshot client-side primary tested em Chrome/Firefox/Safari.
- Server-side fallback tested with simulated client-side failure.

### Negative scenarios (≥ 4)
1. **Locale mismatch (Lote 10.16 codex P0 canonical)**: rendered locale cookie corelink_locale=pt-BR + payload locale=en-US → backend rejects "locale_mismatch" (NOT Accept-Language header — header pode diverge if user switched locale via cookie).
2. **Hash mismatch**: malformed notice text → backend re-compute mismatch → rejects.
3. **Pre-checked consent**: rendered checkbox state must default unchecked; pre-check rejected em rendering test.
4. **Screenshot client fail**: simulated html2canvas throw → server-side fallback triggered + persisted.
5. **Auto-submit attempt**: page load without click → no consent submit (property test).
6. **Notice version mismatch**: client sends `notice_version=1.0.0` while latest is `1.2.0` → backend warning + audit event.

### Cross-browser
- Smoke test em Chrome + Firefox + Safari + Edge para html2canvas behavior; full matrix em WI-S16-007.

## 16. Failure Modes

- **FM-150** (transient API): consent submit retry com exponential backoff (3 retries default); progress indicator.
- **FM-160** (auth invalid): clear error UI; redirect to Clerk sign-in flow.
- **EVT-012 client failure**: server-side fallback headless Chrome render proof.

## 17. Controls

- **CTRL-PRIV-CONSENT-001..006** reflection (UI capture; backend S-11 enforce).
- **CTRL-PRIV-CONSENT-005** (locale match) enforced backend; UI reflects.
- **CTRL-PRIV-001** (zero PII em client logs) reforced via safeLog wrapper.

## 18. Resilience Patterns

- Retry transient errors (FM-150): exponential backoff em consent submit.
- Server-side fallback screenshot evidence (EVT-012 sustained ≥ 99%).
- Optimistic UI confirmation com rollback em backend reject.

## 19. Observability

UI métricas Prometheus snake_case:
- `corelink_admin_ui_consent_capture_total{outcome, locale}` counter (outcome ∈ ok|fail|abandoned).
- `corelink_admin_ui_consent_screenshot_total{source}` counter (source ∈ client|server_fallback).
- `corelink_admin_ui_consent_locale_mismatch_total` counter (alert > 5/dia).
- `corelink_admin_ui_consent_revoke_total` counter (consume DSR form route).

Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (NUNCA per-tenant labels).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: locale match CTRL-PRIV-CONSENT-005 backend enforce; user authenticated via Clerk.
- **Tampering**: notice_text_hash sha256 backend re-computes; rejects mismatch.
- **Repudiation**: 6-field payload + screenshot evidence EVT-012 + JWT receipt forensic-grade trail.
- **Information disclosure**: PAT NUNCA em consent payload; safeLog allowlist; screenshot redacts user IP server-side.
- **DoS**: consent submit rate-limited (≤ 10/min per user; via Cloudflare).
- **Elevation of privilege**: consent revoke flow cross-linked DSR (WI-S16-004); user-initiated only.

**LINDDUN delta**:
- **Linkability**: wording_id UUID v7 (no PII linkable); locale (cardinality bounded).
- **Identifiability**: consent payload references user_id_hash (sha256 server-side); screenshot redacts IP.
- **Non-repudiation**: 6-field payload + screenshot + JWT receipt + audit event S-09 emitted.
- **Detectability**: consent locale mismatch alerted (> 5/dia indicates Accept-Language drift).
- **Disclosure**: notice text public; user PII never em payload (only references).
- **Unawareness**: notice plain-language Flesch-Kincaid ≤ 8 + native speaker review.
- **Non-compliance**: LGPD Art. 8º (consent unambiguous) + GDPR Art. 7 (conditions for consent) + GDPR Art. 25 (data protection by design); LINDDUN review committed em `specs/_audits/2026-XX-XX-linddun-consent-ui.md`.

## 21. Dependencies

### Hard blockers
- WI-S16-001 SEALED (skeleton + Clerk + CSP + i18n).
- S-11 SEALED (privacy backend + consent ledger D1 + verify endpoint R-S11-9).
- S-09 SEALED (audit events R2 bucket for screenshot evidence persistence).

### Soft blockers
- Native speaker review per locale (deferred to WI-S16-006 i18n full).

### Outbound
- WI-S16-004 (DSR form integrates consent revoke flow).
- WI-S16-005 (audit log viewer queries consent capture events).
- WI-S16-006 (component library reuses notice rendering patterns).
- WI-S16-007 (E2E consent flow + closing PRR).

## 22. Effort PERT

O: 14h, M: 22h, P: 36h → PERT **23.0h** (per spec contract §12; consent capture + screenshot evidence + Flesch-Kincaid CI + integration backend).

## 23. Cost Analysis

- Screenshot R2 storage: ~$0.015/GB/mês × estimated 100GB/yr = $1.5/mês.
- Headless Chrome server-side fallback: ~$0.02/render × 1k fallbacks/mês = $20/mês.
- SES emails (consent receipts): ~$0.10 / 1000 emails.
- Total: ~$25/mês incremental.

## 24. Post-mortem Hooks

- Consent screenshot evidence missed (em audit period) → post-mortem + privacy gap fix.
- Locale mismatch sustained > 7d → CTRL-PRIV-CONSENT-005 review + UI fix.
- Hash mismatch detected em backend → SEV-2 post-mortem + integrity review.
- Consent capture abandonment > 50% → UX research + iterate notice readability.
- Pre-checked consent shipped em prod → CRITICAL post-mortem + Privacy + Legal.

## 25. Rollback / Recovery

Consent capture regression detected → revert via CF Pages rollback; backend S-11 retains existing consents (no data loss).

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | html2canvas browser quirks | M | M | LOW | M | LOW | Client-side primary + server-side fallback headless Chrome render |
| R-002 | Locale mismatch CTRL-PRIV-CONSENT-005 | M | L | MEDIUM | M | LOW | Backend enforce match; UI reflects Accept-Language |
| R-003 | wording_id UUID collision | L | L | LOW | L | LOW | UUID v7 time-ordered; collision probability negligible |
| R-004 | Flesch-Kincaid ≤ 8 unmaintainable per locale | M | L | LOW | L | LOW | CI gate; native speaker review iterative |
| R-005 | Auto-submit dark pattern accidentally introduced | L | L | HIGH | L | LOW | Property test 100 random interactions; click capture event-bound |
| R-006 | Screenshot evidence retention compliance gap | L | M | HIGH | M | LOW | R2 audit bucket retention ≥ 5 years per S-11 |

## 27. Knowledge Transfer

- Tech talk (1h): "CoreLink Consent UI — 6-field Capture + EVT-012 Screenshot Evidence".
- Doc `docs/internal/consent-capture-flow.md` — overview + LGPD/GDPR mapping.
- Onboarding test (3 questions): 6-field payload + locale match + screenshot fallback rationale.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Frontend Lead | _TBD; emphatic — consent capture flow + screenshot evidence + hash compute_ | _pending_ |
| 4 | QA | _TBD; emphatic — E2E consent flow + Flesch-Kincaid CI + locale matrix tests_ | _pending_ |
| 5 | Product | Gustavo Schneiter | _pending_ |
| 6 | Designer/a11y advisor | _TBD; emphatic — notice readability + click capture UX + plain-language verification_ | _pending_ |
| 7 | Privacy officer | _TBD; emphatic — CTRL-PRIV-CONSENT-001..006 reflection + LINDDUN review + LGPD/GDPR compliance_ | _pending_ |

> STANDARD lane (per framework §33.5.4): 5-8 canonical sign-offs; 7 typical. Privacy officer canonical em S-16-003 (consent capture compliance-critical).

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S16-003 (cycle 12.S16.0; consent management UI 6-field capture + EVT-012 screenshot evidence). |

## 30. Anti-patterns evitados

- Auto-submit consent (dark pattern; LGPD Art. 8º + GDPR Art. 7 violation).
- Pre-checked consent checkbox (GDPR Art. 7§2 invalid).
- Consent capture sem 6-field payload (CTRL-PRIV-CONSENT-001..006 violation).
- Screenshot evidence EVT-012 missing (compliance gap).
- Locale mismatch tolerated (CTRL-PRIV-CONSENT-005 violation).
- Notice text dense legalese (UX abandonment + compliance opaque).
- Skip server-side fallback screenshot (EVT-012 sustained < 99%).
- Consent revoke sem audit event (S-09 alignment gap).
- PII em consent payload (only user_id_hash references).

---

**Fim WI-S16-003.**
