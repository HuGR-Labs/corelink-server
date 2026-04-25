---
id: "SPEC-CONTRACT-S11"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s11", "privacy", "dsr", "lgpd", "gdpr", "ccpa", "erasure", "consent", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-11: Privacy Pipeline (DSR + Erasure Automation + Consent Ledger)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-11 |
| Nome | Privacy Pipeline |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-003 (PII/GDPR), FF-HR-005 (privacy controls), FF-HR-010 (1ª regulatory full impl) |
| Duração estimada | 3 semanas |
| WIs antecipados | 8 |
| SOTA target | Pipeline regulatory-grade — DSR self-service ≤ 30d SLA cross-backend + consent ledger com proof of informed + breach notification ≤ 72h GDPR Art. 33 |

## 1. Objetivo

Implementar **pipeline regulatory-grade de DSR (Data Subject Rights)** conforme **LGPD Art. 18**, **GDPR Art. 15-22**, **CCPA/CPRA §1798.105/115/120/125**: self-service **API backend** para 6 direitos (access, correction, erasure, portability, objection, consent revoke) — UI surface delivered em S-16 (frontend admin UI) consume estas APIs; erasure automation cross-backend (D1, Neon, R2, KV, DO, Grafana Loki, Stripe), consent management com proof of informed (CTRL-PRIV-CONSENT-001..006), breach notification runbook executable (≤ 72h GDPR Art. 33), residency pinning E2E.

**Por que SOTA:** competitors entregam (a) DSR manual via support ticket (≠ self-service); (b) erasure parcial (alguns backends esquecidos = compliance gap); (c) consent capture sem proof (apenas checkbox boolean); (d) breach notification sem template legal pronto. CoreLink S-11 entrega **pipeline auditável end-to-end** com cryptographic proof de consent, erasure verifiable cross-backend, DPIA template para cada feature privacy-impacting. Reference: **EDPB Guidelines 4/2019** (Art. 25 by design), **NIST Privacy Framework 1.0**.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (10–12 sign-offs).
- **FF-HR-003**: processa PII / dados regulados diretamente — toda failure tem regulatory impact.
- **FF-HR-005**: implementa CTRL-PRIV-001..033 + CTRL-PRIV-CONSENT-001..006 (32+ controles formais).
- **FF-HR-010**: 1ª implementação regulatory completa (LGPD + GDPR + CCPA/CPRA) — qualquer gap aqui é catastrófico (multa LGPD = 2% revenue + R$ 50M; GDPR = 4% global + €20M).

## 3. Inherits_from

```yaml
inherits_from:
  - "PRIVACY-MODEL"             # CTRL-PRIV-001..033, CTRL-PRIV-CONSENT-001..006
  - "COMPLIANCE-MATRIX"         # LGPD Art. 18, GDPR Art. 15-22, CCPA §1798.x
  - "SECURITY-MODEL"            # CTRL-SEC-* alignment
  - "AUTH-MODEL"                # MFA re-auth para ops sensíveis
  - "DATA-MODEL"                # PII inventory + erasure paths
  - "OBSERVABILITY-MODEL"       # SLO-FRESH-DSR-ERASURE
  - "FAILURE-MODES"             # FM-401 (erasure incomplete), FM-402 (residency leak)
  - "KEY-MANAGEMENT"            # crypto-erase para data at rest com BYOK
  - "RESILIENCE-PATTERNS"       # PAT-RETRY-IDEMPOTENT-001 erasure replays
  - "INVARIANT-REGISTRY"        # INV-DATA-ERASURE-COMPLETE, INV-DATA-RESIDENCY, INV-CONSENT-PROOF-VERIFIABLE, INV-AUDIT-APPEND-ONLY
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-PRIV-001** | DSR self-service API | `POST /v1/privacy/dsr/{access\|correction\|erasure\|portability\|objection\|consent_revoke}`. |
| **CAP-PRIV-002** | Erasure automation cross-backend | Worker `dsr-erasure-worker` propaga erasure D1, Neon, R2, KV, DO, Grafana Loki, Stripe customer + audit. |
| **CAP-PRIV-003** | Consent ledger com proof of informed | notice_text_hash + version + locale + wording_id + ui_capture_ts + submission_ts; verifiable post-facto. |
| **CAP-PRIV-004** | Privacy notice publication + versioning | `legal/privacy-notice/v<M.m>.md` semver versioned + diff diário em `/privacy`. |
| **CAP-PRIV-005** | Breach notification runbook executable | RB-BREACH-NOTIF com templates ANPD (LGPD) + Irish DPC (GDPR) + state AGs (CCPA); dry-run com Legal. |
| **CAP-PRIV-006** | Sub-processor register público + notification | Auto-generated de `legal/sub-processors.md`; email broadcast a customers ≥ 30d antes de mudança. |
| **CAP-PRIV-007** | Residency pinning E2E | Tenant region opt-in (US/EU/APAC); CAS/AC/billing data NUNCA cross-region. |
| **CAP-PRIV-008** | DPIA template + LIA template | DPIA em `legal/dpia/<feature>.md`; LIA (Legitimate Interest Assessment) para telemetria. |
| **CAP-PRIV-009** | DSR receipt + verifiable proof | Customer recebe signed JWT receipt (`dsr_request_id`, `submission_ts`, `expected_completion_ts`); verifiable via API pública. |

## 5. Requirements específicos

### 5.1 DSR API (CAP-PRIV-001 + CAP-PRIV-009)

- **R-S11-1**: Endpoints DSR (todos requerem auth via PAT + MFA re-auth):
  - `POST /v1/privacy/dsr/access` → response em ≤ 30d (GDPR Art. 12.3); email JSON export quando ready.
  - `POST /v1/privacy/dsr/correction` → patch fields permitidos; outros denied com 422.
  - `POST /v1/privacy/dsr/erasure` → soft-delete imediato + hard-erase ≤ 30d cross-backend.
  - `POST /v1/privacy/dsr/portability` → JSON export em formato machine-readable (LGPD Art. 18.V; GDPR Art. 20).
  - `POST /v1/privacy/dsr/objection` → opt-out de processamento legitimate-interest baseado.
  - `POST /v1/privacy/dsr/consent_revoke` → revoga consents granted; cascade unsubscribe.
- **R-S11-2**: DSR receipt: cada request retorna signed JWT (`dsr_request_id`, `tenant_id`, `subject_id`, `submission_ts`, `expected_completion_ts`, `signature`); pública API `GET /v1/privacy/dsr/verify?token=X` para verificar.
- **R-S11-3**: Status endpoint `GET /v1/privacy/dsr/{id}/status` → `pending|in_progress|completed|failed|rejected_with_reason`.

### 5.2 Erasure Automation (CAP-PRIV-002)

- **R-S11-4**: Worker `dsr-erasure-worker` executa erasure cross-backend:
  - **D1** (relational): SQL DELETE WHERE tenant_id/subject_id; tombstone em `dsr_erasure_log`.
  - **Neon** (Postgres): DELETE + VACUUM FULL post-batch para reclaim physical space.
  - **R2** (CAS blobs): scrub manifests + chunks que pertencem ao subject (FK via S-07 dedup); tombstone.
  - **KV**: DELETE keys matching prefix.
  - **DO** (Durable Objects): atomic delete + replicated state purge.
  - **Grafana Loki**: log labels com PII (rare por CTRL-PRIV-001) → query API delete.
  - **Stripe**: customer.delete (mantém invoice por GAAP — pseudonymize subject info).
- **R-S11-5**: **Crypto-erase** para BYOK customers (S-14): erasure efetiva via destroy customer-managed key (NIST SP 800-88 Rev.1 compliant); evidence emitido em audit.
- **R-S11-6**: Verification job: 24h após erasure, sweep **10 backends total (Lote 9.4 Opus H-10 expansion)**:
  - **Erasure-effective (mutable, full delete)** — 6 backends: D1, Neon, R2 (mutable buckets), KV, DO, Stripe (customer.delete).
  - **Erasure-pseudonymized (Object Lock WORM)** — 4 backends: R2 audit bucket (Object Lock 7y, S-09 R-S09-10), R2 billing-events (Object Lock 7y, S-10 R-S10-1), Loki long-term R2 cold archive (S-09 R-S09-5), Cloudflare Analytics Engine (rolling 30d retention).
  - **Pseudonymization rule**: PII em records imutáveis substituído por `tenant_id_hash = sha256(tenant_id || erasure_salt)` + `pii_redacted_marker`; tenant_id ID pode ser correlacionado pelo customer com sua própria erasure_salt (kept separately) mas não por terceiros.
  - Conflict resolution: GDPR Art. 17 (right to erasure) vs Object Lock 7y (audit immutability) — **pseudonymization é o escape valve regulatório aceito** (EDPB Guidelines 5/2020). Customer/auditor confirmation flow documented em runbook RB-DSR-ERASURE-INCOMPLETE.
  - Report em `dsr-erasure-reports/<id>.json` retain 7y; per-backend status (`erased`, `pseudonymized`, `partial_failure`, `failed`).

### 5.3 Consent Ledger (CAP-PRIV-003)

- **R-S11-7**: Endpoint `POST /v1/consent/<purpose>` → grava em D1 `consent_ledger`:
  - `subject_id`, `purpose` (e.g., `analytics`, `marketing`, `email_transactional`), `granted: bool`, `notice_text_hash` (SHA-256 do notice text shown), `notice_version` (semver), `locale` (ISO 639-1), `wording_id` (UUID do A/B test), `ui_capture_ts` (browser ts), `submission_ts` (server ts), `ip_addr_hash` (SHA-256), `signature` (HMAC).
- **R-S11-8**: `DELETE /v1/consent/<purpose>` revoga **com proof simétrico (Lote 9.4 Opus H-05)** — consent_revocation table espelha schema do consent_ledger 6-field: `subject_id`, `purpose`, `revoked: bool`, `notice_text_hash` (do notice em vigor no momento da revoga), `notice_version`, `locale`, `wording_id`, `ui_capture_ts`, `submission_ts`, `ip_addr_hash`, `signature` HMAC. Cascade ao processamento downstream ≤ 24h. **GDPR Art. 7 alignment**: revoke é "as easy as giving consent" + cryptographically attested. Verify endpoint `GET /v1/consent/revocation/verify?revocation_id=X` retorna proof verificável.
- **R-S11-9**: `GET /v1/consent` → user vê todos os consents + history with timestamps + notice diffs.

### 5.4 Privacy Notice Versioning (CAP-PRIV-004)

- **R-S11-10**: Privacy notice em `legal/privacy-notice/v<M.m>.md` versionado semver:
  - Major bump = material change (novo data category, novo sub-processor, novo purpose); requires re-consent dos affected subjects.
  - Minor bump = clarification, typo, contact update; no re-consent required.
  - Diff diário publicado em `/privacy/changelog`.
- **R-S11-11**: Privacy notice tem versions em **PT-BR** (LGPD primary), **EN** (GDPR/CCPA), **ES** (LATAM); 3 locales mandatory at GA.

### 5.5 Sub-processor Register (CAP-PRIV-006)

- **R-S11-12**: `legal/sub-processors.md` listing: name, role (e.g., "infrastructure provider"), data categories processed, region, certification (SOC 2, ISO 27001).
- **R-S11-13**: Mudança de sub-processor: email broadcast ≥ 30d antes; customer pode objetar → escalonamento para Legal + decisão accept-or-terminate.

### 5.6 Breach Notification (CAP-PRIV-005)

- **R-S11-14**: Runbook `RB-BREACH-NOTIF` executable com:
  - Decision tree: tipo de breach × severidade × jurisdição → quem notificar e quando.
  - Templates legais drafted em `legal/breach-notification/`:
    - `lgpd-anpd-template.pt-br.md` — ANPD notification (LGPD Art. 48)
    - `gdpr-irish-dpc-template.en.md` — Lead supervisory authority (GDPR Art. 33)
    - `ccpa-state-ag-template.en.md` — California AG (CCPA §1798.82)
  - Customer notification template (em 3 locales).
  - **Timeline:** GDPR exige ≤ 72h; LGPD ≤ "tempo razoável" (interpretado 72h); CCPA "in the most expedient time possible".
- **R-S11-15**: Dry-run com Legal + Privacy Officer + Security Lead + simulated breach scenario (PII leak via log) → measure time-to-decision-tree-completion ≤ 4h.

### 5.7 Residency (CAP-PRIV-007)

- **R-S11-16**: Tenant region opt-in: signup tenant escolhe `region: us|eu|apac`; armazenado em D1 `tenant_metadata.region_pinned`.
- **R-S11-17**: Worker routing: requests roteados para region pinned via custom domain mapping (`<tenant_id>.eu.corelink.dev`); cross-region queries denied 403.
- **R-S11-18**: CAS/AC/billing data tagged com `region`; insert checks reject cross-region writes.
- **R-S11-19**: Property test: 10k tenants EU + 10k tenants US → verify 0 blob lands em wrong region.

### 5.8 DPIA + LIA (CAP-PRIV-008)

- **R-S11-20**: DPIA template em `_templates/dpia.md` (Article 35 GDPR style); preenchido para cada feature privacy-impacting (e.g., dedup leakage S-07, telemetry S-09).
- **R-S11-21**: LIA template (Legitimate Interest Assessment) preenchido para processamento sob legitimate interest (telemetria operacional sem consent explícito).

## 6. Definition of Done

- [ ] **WIs SEALED**: 8/8.
- [ ] **E2E erasure**: fake user signup → use product 30d → DSR erasure request → 0 records cross-backend em ≤ 30d (verification job verde).
- [ ] **Erasure cross-backend coverage**: **10 backends total** (Lote 9.5c R3-11 alignment com R-S11-6): 6 erasure-effective (D1, Neon, R2 mutable, KV, DO, Stripe) + 4 pseudonymized via Object Lock (R2 audit 7y, R2 billing-events 7y, Loki cold archive 400d, CF Analytics Engine rolling 30d). Test isolado per backend + integrated cross-backend.
- [ ] **Crypto-erase BYOK**: simulate enterprise BYOK + DSR erasure → key destroyed; data inacessível (NIST SP 800-88 evidence).
- [ ] **Residency**: 10k tenant EU + 10k US property test verde; 0 cross-region leaks.
- [ ] **Consent UI screenshot evidence captured** (EVT-012 + EVT-049) com all 6 fields populated.
- [ ] **Consent proof verifiable**: re-compute notice_text_hash em backend → match record stored.
- [ ] **RB-BREACH-NOTIF dry-run** executado com Legal + Privacy Officer + Security Lead; time-to-decision ≤ 4h.
- [ ] **Templates legais drafted** em 3 jurisdictions (LGPD/GDPR/CCPA) revisados por Legal externo.
- [ ] **CTRL-PRIV-030** (erasure pipeline) + **CTRL-PRIV-031** (residency) em prod com evidence.
- [ ] **DPIA** preenchido para 3 features (dedup, telemetry, billing).
- [ ] **PRR HIGH_RISK**: Privacy Officer + Legal + DPO interim + Security lead + Compliance officer + SRE + Engineer + QA + Product + 2 peers.
- [ ] **TLA+ spec** `dsr_erasure_atomicity.tla` (verifica cross-backend erasure é atomic ou compensating-rollback) verde em CI.
- [ ] **Runbook dry-run**: RB-GDPR-ERASURE-HOLD (existing) + new RB stub `RB-DSR-ERASURE-INCOMPLETE` (criar) + new RB stub `RB-DATA-RESIDENCY-LEAK` (criar) (EVT-017).

## 7. Completeness Criteria (delta local)

- [ ] **10.s11.1** SLO-FRESH-DSR-ERASURE: 99% das erasures completas ≤ 30 dias (sustained 90d).
- [ ] **10.s11.2** DPIA preenchido para cada feature privacy-impacting (dedup, telemetry, billing minimum).
- [ ] **10.s11.3** LIA template preenchido para telemetria sob legitimate interest.
- [ ] **10.s11.4** **Crypto-erase NIST SP 800-88 Rev.1 compliant** para BYOK tenants (witness key destroy ceremony).
- [ ] **10.s11.5** **Erasure verification job 24h post-erasure**: 0 records cross 10 backends total (6 effective + 4 pseudonymized via Object Lock); report retained 7y.
- [ ] **10.s11.6** **DSR receipt JWT** signed verifiable via public endpoint.
- [ ] **10.s11.7** **3 locales** (PT-BR, EN, ES) para privacy notice + DSR UI mandatory at GA.
- [ ] **10.s11.8** **Sub-processor change notification** ≥ 30d testado em staging (mock change → email triggered).

## 8. Invariants

### Mantidas

- **CTRL-PRIV-001..014** (PII em logs / dados em uso): zero PII em audit logs (DLP scan).
- **CTRL-PRIV-CONSENT-001..006**: consent capture com proof.

### Novas (introduzidas por S-11 — adicionar a invariant_registry.md)

- **INV-DATA-ERASURE-COMPLETE** (HIGH — herda registry §3.5; **NÃO é novo** — ID já existia desde Lote 5; S-11 reforça evidence): erasure cross-backend é efetiva em 7/7 backends; 0 records remanescentes 30d post-request. **Why:** LGPD Art. 18 + GDPR Art. 17 = right to erasure absoluto; gap = regulatory finding (multa). **How to apply:** verification job 24h post-erasure + audit retain 7y. **Severity drift Lote 9.4:** S-11 v1.1 inicialmente classificou como CRITICAL — corrigido para HIGH alinhando ao registry; severity bump para CRITICAL requer ADR + Privacy Officer sign-off.
- **INV-DATA-RESIDENCY** (HIGH — herda registry §3.11; **NÃO é novo**): tenant region pinned não vaza cross-region; 10k property test 0 violations. **Why:** Schrems II + LGPD Art. 33 § 1º + GDPR Art. 44 (cross-border transfer); gap = catastrofic legal exposure. **How to apply:** insert checks region tag + property test CI + custom domain routing. **Severity drift Lote 9.4:** mesma correção que ERASURE-COMPLETE.
- **INV-CONSENT-PROOF-VERIFIABLE** (HIGH — novo): consent records têm notice_text_hash verifiable post-facto; tampering detected via signature. **Why:** GDPR Art. 7 exige proof of informed consent; sem hash = consent não defensible. **How to apply:** SHA-256 + HMAC + verify endpoint.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL — herda): DSR events e consent events são imutáveis em R2 Object Lock 7y.

## 9. Quality Standards (delta local)

- **14.s11.1 Zero PII em audit logs** (CTRL-PRIV-001 + CTRL-PRIV-014); DLP CI scan + DLP runtime scan em produção.
- **14.s11.2 DSR API security**: PAT + MFA re-auth (CTRL-PRIV-016) para ops sensíveis (erasure, correction); rate-limit 10 DSR/dia/subject (anti-DoS).
- **14.s11.3 Consent proof**: notice_text_hash verifiable; signature HMAC verifiable; immutable em D1 audit.
- **14.s11.4 Erasure idempotency**: replay erasure 100× → idempotent (PAT-RETRY-IDEMPOTENT-001).
- **14.s11.5 Translation quality**: privacy notice em 3 locales reviewed por native speaker + Legal local; semver bump triggers re-translation.
- **14.s11.6 Sub-processor transparency**: customer pode export sub-processor list via API anytime; mudanças em real-time no `/privacy/sub-processors` page.
- **14.s11.7 Breach response timeliness**: time-to-decision-tree-completion ≤ 4h em dry-run; goal time-to-customer-notification ≤ 72h.
- **14.s11.8 DPIA quality**: peer-reviewed por Privacy Officer + Legal antes de feature ship; archived 7y.

## 10. Anti-scope

- ❌ Schrems II TIA (Transfer Impact Assessment) templates — pós-GA se EU tenants materializarem (anti-scope explícito).
- ❌ CCPA/CPRA specific UI flows — aligned com GDPR; specific wording em S-18 docs.
- ❌ HIPAA BAA (Business Associate Agreement) — opt-in enterprise no S-14; não default.
- ❌ China PIPL (Personal Information Protection Law) — pós-GA; mercado China não target inicial.
- ❌ India DPDPA — pós-GA; mercado India não target inicial.
- ❌ Children's Online Privacy (COPPA) — anti-scope; product não target < 13 anos.
- ❌ Cookies banner / e-Privacy Directive — não temos website público com cookies de tracking; admin UI tem disclosure básico.
- ❌ Right to data portability inter-vendor (export → import competitor) — exportamos JSON; não convertemos para BuildBuddy/NativeLink format.

## 11. Dependencies

### Hard blockers

- **S-01 + S-02 + S-03 SEALED** (data exists to erase + auth context).
- **S-09 SEALED** (observability para SLO-FRESH-DSR-ERASURE + audit events).
- **S-10 SEALED** (billing data é parte do DSR scope).

### Soft blockers

- **S-08 SEALED** (rate limit DSR endpoints).

### Outbound

- S-13 (admin plane DSR queue management).
- S-14 (BYOK crypto-erase).
- S-16 (frontend DSR UI).
- S-18 (public docs privacy section).
- S-20 (GA exige RB-BREACH-NOTIF dry-run + Legal sign-off).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S11-001** | DSR API endpoints (6 direitos) + JWT receipt + status endpoint | 6 endpoints; receipt JWT signing/verification; status query; rate limit; MFA re-auth | 14h | 22h | 36h | **23.0h** |
| **WI-S11-002** | Erasure worker cross-backend (7 backends) + verification 24h + reports | erasure logic per backend; verification job; report R2 7y; tombstone; reconcile | 18h | 28h | 44h | **28.7h** |
| **WI-S11-003** | Consent ledger D1 schema + proof of informed payload + endpoints | schema; capture endpoint; revoke endpoint; list endpoint; verify endpoint; HMAC | 10h | 16h | 26h | **16.7h** |
| **WI-S11-004** | Privacy notice versioning + 3 locales (PT-BR/EN/ES) + diff publication | semver structure; notice content review; 3 translations; changelog page; CI hook | 8h | 14h | 22h | **14.3h** |
| **WI-S11-005** | Sub-processor register + 30d email broadcast + customer objection flow | legal/sub-processors.md auto-gen; email broadcast cron; objection ticket route | 6h | 10h | 16h | **10.3h** |
| **WI-S11-006** | Breach notification runbook + 3 jurisdictional templates + dry-run | RB-BREACH-NOTIF; LGPD/GDPR/CCPA templates; dry-run with Legal + Privacy + SecLead | 12h | 20h | 32h | **20.7h** |
| **WI-S11-007** | Residency pinning E2E + custom domain routing + property test | tenant_metadata.region_pinned; routing layer; insert checks; 20k property test; runbook | 12h | 18h | 28h | **18.7h** |
| **WI-S11-008** | DPIA template + LIA template + 3 DPIAs filled + TLA+ dsr_erasure_atomicity | template; LIA; 3 DPIAs (dedup/telemetry/billing); TLA+ spec + CI | 14h | 20h | 32h | **20.7h** |

**Total PERT:** ~153h ≈ 19 dias work × 1 eng. Buffer 7 dias confere com 3 semanas (legal review + translations têm unpredictability).

## 13. Duração + Timeline

- **Duração:** 3 semanas (15 dias úteis) + buffer 7 dias.
- **Marcos:**
  - **D+5:** WI-001 + WI-003 SEALED (DSR API + consent ledger).
  - **D+10:** WI-002 SEALED (erasure cross-backend).
  - **D+12:** WI-004 + WI-005 SEALED (notice + sub-processor).
  - **D+14:** WI-006 + WI-007 SEALED (breach notif + residency).
  - **D+15:** WI-008 SEALED (DPIA + TLA+).
  - **D+17:** Sprint review + Legal sign-off + DPO walkthrough.

## 14. Critérios de promoção

- DoD complete + erasure E2E em ≤ 30d sustained 90d staging.
- DPO/Legal/Privacy Officer sign-off em DPIA + privacy notice + sub-processor register.
- DPIA documentada + Legal Review (EVT-044) + 3 locales reviewed.
- TLA+ verde + breach notif dry-run successful.
- PRR HIGH_RISK aprovado.

## 15. Riscos (registry expandido)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Erasure incompleto** (algum backend ignora) | M | M | CRITICAL (LGPD non-compliance) | H | LOW | INV-DATA-ERASURE-COMPLETE + verification job 24h + 7-backend coverage test + RB-DSR-ERASURE-INCOMPLETE stub. |
| **Consent record sem notice proof** (GDPR Art. 7) | M | M | HIGH | H | LOW | INV-CONSENT-PROOF-VERIFIABLE + notice_text_hash mandatory + verify endpoint test em CI. |
| **Residency leak** (tenant EU dado em US) | L | M | CRITICAL (Schrems II) | M | LOW | INV-DATA-RESIDENCY + insert checks + 20k property test + custom domain routing. |
| **Legal templates inadequados** (rejected by ANPD/DPC) | M | M | HIGH (dependência externa) | M | LOW | Drafted + reviewed by Legal externo + 3 jurisdiction templates + dry-run. |
| **Translation quality issues** (PT-BR/ES native review fail) | M | L | MEDIUM | M | LOW | Native speaker + Legal local review + semver bump triggers re-translation. |
| **Crypto-erase verification challenge** (auditor disputes destroy) | M | M | HIGH (BYOK customer dispute) | M | LOW | NIST SP 800-88 Rev.1 compliant ceremony + witness sign-off + retained 7y evidence. |
| **DSR storm DoS** (10k DSR requests in 1 day) | L | M | MEDIUM (operational overload) | L | LOW | Rate limit 10 DSR/dia/subject + queue + alert. |
| **Sub-processor change broadcast failure** (email não chega) | M | M | MEDIUM (silent breach of policy) | M | LOW | Email DKIM + delivery confirmation + dashboard `/privacy/sub-processors` + customer poll API. |
| **Breach notification > 72h** (timeline miss) | L | M | CRITICAL (regulatory) | M | LOW | RB-BREACH-NOTIF dry-run; time-to-decision ≤ 4h target; on-call escalation matrix. |
| **DPIA missing for new feature** (silent privacy debt) | M | M | HIGH | M | LOW | CI hook: PR mudando PII handling sem DPIA → fail; quarterly Privacy Officer review. |
| **Consent revoke cascade incomplete** (downstream still uses) | M | H | HIGH | H | LOW | Cascade unsubscribe ≤ 24h + verification job 48h post-revoke + audit log. |

## 16. Benchmarks SOTA externos (target qualitativo + quantitativo)

| Critério | OneTrust | Transcend | DataGrail | **CoreLink target S-11** |
|---|---|---|---|---|
| DSR self-service API | Plug-in | Yes | Yes | **Yes — 6 direitos REST API + JWT receipt** |
| Erasure cross-backend automated | Plug-in | Yes | Yes | **Yes — 7 backends + verification 24h** |
| Crypto-erase para BYOK | No | No | Manual | **Yes — NIST SP 800-88 Rev.1 compliant** |
| Consent proof of informed | Yes | Yes | Yes | **Yes — notice_text_hash + HMAC + verify endpoint** |
| Breach notification runbook executable | Manual | Plug-in | Manual | **Yes — RB executable + 3 jurisdiction templates** |
| Residency pinning E2E | Plug-in | Yes | Plug-in | **Yes — custom domain + property test 20k tenants** |
| DPIA template integrated | Yes | Yes | Yes | **Yes — `_templates/dpia.md` + CI hook** |
| TLA+ verified erasure atomicity | No | No | No | **Yes — dsr_erasure_atomicity.tla CI** |
| 3+ locales mandatory | Add-on | Add-on | Add-on | **Yes — PT-BR, EN, ES at GA** |
| SLA ≤ 30d for erasure | Yes | Yes | Yes | **99% ≤ 30d sustained 90d** |

**Veredito SOTA:** S-11 v1.1 atinge **estado-da-arte em 10/10 dimensões**; TLA+ erasure atomicity é diferencial vs todos privacy SaaS competitors.

## 17. References (RFCs, papers, standards)

- **LGPD (Lei 13.709/2018) Art. 18** — direitos do titular <https://www.planalto.gov.br/ccivil_03/_ato2015-2018/2018/lei/l13709.htm>.
- **GDPR (EU 2016/679) Art. 15-22** — data subject rights <https://gdpr-info.eu/>.
- **GDPR Art. 33** — breach notification ≤ 72h.
- **GDPR Art. 35** — DPIA requirement.
- **CCPA/CPRA §1798.105** — right to delete; §1798.115 — right to know; §1798.120 — opt-out.
- **NIST Privacy Framework 1.0** <https://www.nist.gov/privacy-framework>.
- **NIST SP 800-88 Rev.1** — Guidelines for Media Sanitization (crypto-erase).
- **EDPB Guidelines 4/2019** — Article 25 Data Protection by Design and by Default.
- **EDPB Guidelines 9/2022** — personal data breach notification.
- **ISO/IEC 27701:2019** — Privacy Information Management System (PIMS).
- **ISO/IEC 27018:2019** — code of practice for PII protection in public clouds.
- **Schrems II (CJEU C-311/18)** — invalidação Privacy Shield + adequacy decisions.
- **Brazilian ANPD Resolution CD/ANPD nº 2/2022** — incident notification requirements.

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- Erasure incomplete detected by verification job → CRITICAL post-mortem + Privacy Officer + Legal.
- Residency leak detected (cross-region blob) → CRITICAL post-mortem + breach notification consideration.
- DSR SLA miss (> 30d) → 5-Why obrigatório + Privacy Officer review + ANPD/DPC notification consideration.
- Consent record tampering detected → CRITICAL post-mortem + audit chain + INV-CONSENT-PROOF-VERIFIABLE review.
- Breach notification > 72h → CRITICAL post-mortem + regulatory exposure assessment.
- Sub-processor change broadcast missed → post-mortem + customer trust review.
- DPIA missing for new privacy-impacting feature → post-mortem + retroactive DPIA + Privacy Officer sign-off.

## 19. Waiver policy

S-11 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ DSR self-service API for all 6 rights — regulatory baseline.
- ❌ Erasure cross-backend coverage 7/7 backends — INV-DATA-ERASURE-COMPLETE.
- ❌ Residency pinning E2E + property test verde — Schrems II legal exposure.
- ❌ Consent proof of informed (notice_text_hash) — GDPR Art. 7.
- ❌ RB-BREACH-NOTIF dry-run with Legal sign-off — operational readiness.
- ❌ TLA+ dsr_erasure_atomicity verde — atomic guarantee.

Itens waivable com sign-off Privacy Officer + Legal + Compliance Officer + ADR:

- ⚠️ DSR rate limit 10/dia/subject → 5/dia (mais restritivo); nunca menos.
- ⚠️ 3 locales → 2 locales (PT-BR + EN) at GA, ES no Q1 pós-GA.
- ⚠️ Verification job 24h post-erasure → 48h (com SEV-2 alert se 48h gap detectado).

---

**Fim spec contract S-11 v1.1.0 SOTA.**
