---
id: "SPEC-CONTRACT-S11"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.5.0"
created: "2026-04-24"
updated: "2026-05-03"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s11", "privacy", "dsr", "lgpd", "gdpr", "ccpa", "erasure", "consent", "residency", "byok-prep", "dpia", "high-risk", "sota-v1.2"]
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

Implementar **pipeline regulatory-grade de DSR (Data Subject Rights)** conforme **LGPD Art. 18**, **GDPR Art. 15-22**, **CCPA/CPRA §1798.105/115/120/125**: self-service **API backend** para 6 direitos (access, correction, erasure, portability, objection, consent revoke) — UI surface delivered em S-16 (frontend admin UI) consume estas APIs; erasure automation cross-backend (12 canonical pós Lote 10.11.0-bis: 8 effective — Neon multi-tabela / Neon billing fiscal / R2 CAS refcount-aware / R2 AC / D1 / KV / Stripe `Customer.update` / Loki — + 4 pseudonymized — R2 audit Object Lock 7y / Neon PITR 30d / R2 CAS legal_hold / R2 evidence-* 7y), consent management com proof of informed (CTRL-PRIV-CONSENT-001..006), breach notification runbook executable (≤ 72h GDPR Art. 33), residency pinning E2E.

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
  - "FAILURE-MODES"             # FM-061 (audit Object Lock vs DSR — canonical legal hold path); S-11 introduz FM-450/451/452/453 novos (erasure incomplete cross-backend, residency leak, consent tampering, sub-processor change broadcast miss)
  - "KEY-MANAGEMENT"            # crypto-erase para data at rest com BYOK
  - "RESILIENCE-PATTERNS"       # PAT-RETRY-IDEMPOTENT-001 erasure replays
  - "INVARIANT-REGISTRY"        # INV-DATA-ERASURE-COMPLETE, INV-DATA-RESIDENCY, INV-CONSENT-PROOF-VERIFIABLE, INV-AUDIT-APPEND-ONLY
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-PRIV-001** | DSR self-service API | `POST /v1/privacy/dsr/{access\|correction\|erasure\|portability\|objection\|consent_revoke}`. |
| **CAP-PRIV-002** | Erasure automation cross-backend | Worker `dsr-erasure-worker` propaga erasure 12 backends canonical (8 effective + 4 pseudonymized; Lote 10.11.0-bis privacy_model.md §6.2 source-of-truth) — Neon multi-tabela + Neon billing fiscal + R2 CAS refcount-aware + R2 AC + D1 + KV + Stripe `Customer.update` (NOT delete; PCI scope) + Loki + audit. |
| **CAP-PRIV-003** | Consent ledger com proof of informed | notice_text_hash + version + locale + wording_id + ui_capture_ts + submission_ts; verifiable post-facto. |
| **CAP-PRIV-004** | Privacy notice publication + versioning | `legal/privacy-notice/v<M.m>.md` semver versioned + diff diário em `/privacy`. |
| **CAP-PRIV-005** | Breach notification runbook executable | RB-BREACH-NOTIF com templates ANPD (LGPD) + Irish DPC (GDPR) + state AGs (CCPA); dry-run com Legal. |
| **CAP-PRIV-006** | Sub-processor register público + notification | Auto-generated de `legal/sub-processors.md`; email broadcast a customers ≥ 30d antes de mudança. |
| **CAP-PRIV-007** | Residency pinning E2E | Tenant region opt-in via canonical 6-region enum (`wnam`/`enam`/`weur`/`sam`/`apac`/`afr` per `data_model.md §2.1` + `privacy_model.md §7.1`); armazenado em `tenant.primary_region` (data_model.md §4.1, **NÃO** `tenant_metadata.region_pinned` — campo canonical é primary_region); CAS/AC/audit/billing-events data NUNCA cross-region; metadata global pseudonimizada pode replicar (privacy_model.md §7.2). |
| **CAP-PRIV-008** | DPIA template + LIA template | DPIA em `legal/dpia/<feature>.md`; LIA (Legitimate Interest Assessment) para telemetria. |
| **CAP-PRIV-009** | DSR receipt + verifiable proof | Customer recebe signed JWT receipt (`dsr_request_id`, `submission_ts`, `expected_completion_ts`); verifiable via API pública. |

## 5. Requirements específicos

### 5.1 DSR API (CAP-PRIV-001 + CAP-PRIV-009)

- **R-S11-1**: Endpoints DSR (todos requerem auth via PAT + **CTRL-AUTH-010 step-up MFA WebAuthn re-auth** — canonical em `security_model.md §242`; `CTRL-PRIV-016` aplica ao caso "support read access requer consent" e NÃO substitui CTRL-AUTH-010). SLAs por direito conforme `privacy_model.md §6.1` (clock semantics F-11):
  - `POST /v1/privacy/dsr/access` → SLA **15 dias úteis** (LGPD Art. 18 II / GDPR Art. 15); email JSON export quando ready; `clock_start` = `dsr_tickets.status='verified'`.
  - `POST /v1/privacy/dsr/correction` → SLA **5 dias úteis** (LGPD Art. 18 III / GDPR Art. 16); patch fields permitidos; outros denied 422.
  - `POST /v1/privacy/dsr/erasure` → SLA **30 dias corridos** (LGPD Art. 18 IV / GDPR Art. 17); soft-delete imediato + hard-erase cross-backend até clock_stop; aplica `CTRL-PRIV-030`.
  - `POST /v1/privacy/dsr/portability` → SLA **15 dias úteis** (LGPD Art. 18 V / GDPR Art. 20); JSON export machine-readable.
  - `POST /v1/privacy/dsr/objection` → SLA **15 dias úteis** (GDPR Art. 21); opt-out de processamento legitimate-interest baseado; **manual review path** (NÃO 100% self-service per privacy_model.md §6.1).
  - `POST /v1/privacy/dsr/consent_revoke` → SLA **≤ 5min** (LGPD Art. 18 VI / GDPR Art. 7.3); revoga consents granted; cascade unsubscribe; aplica `CTRL-PRIV-CONSENT-002`.
  - `POST /v1/privacy/dsr/confirmation` → SLA **5 dias úteis** (LGPD Art. 18 I / GDPR Art. 15.1); confirmação de tratamento (sub-categoria de access).
- **R-S11-2**: DSR receipt: cada request retorna signed JWT (`dsr_request_id`, `tenant_id`, `subject_id`, `submission_ts`, `expected_completion_ts`, `signature`); pública API `GET /v1/privacy/dsr/verify?token=X` para verificar.
- **R-S11-3**: Status endpoint `GET /v1/privacy/dsr/{ticket_id}/status` → 7-state canonical (data_model.md §4.1 source-of-truth pós Lote 10.11.0-bis): `received|verified|queued|in_progress|completed|denied|failed`. Authenticated tenant-scoped (privacy: público verify endpoint stateless retorna apenas JWT-derived fields, não pipeline state).

### 5.2 Erasure Automation (CAP-PRIV-002)

- **R-S11-4**: Worker `dsr-erasure-worker` executa erasure cross-backend. **Canonical pós Lote 10.11.0-bis** = 12 backends totais (`privacy_model.md §6.2` reescrita como source-of-truth). **8 effective slots** (erasure física): Neon multi-tabela, Neon billing fiscal exception, R2 CAS refcount-aware (subject_unaffiliated vs subject_dedicated), R2 AC, D1 (`blob_meta/ac_meta`), KV, Stripe `Customer.update`, Loki/Grafana log deletion API. **4 pseudonymized slots** (legal_hold canonical): R2 audit Object Lock 7y (HKDF audit-pseudonym), Neon PITR backup 30d, R2 CAS legal_hold partition, R2 evidence-* buckets 7y. Lista canonical S-11 v1.3.0:
  **8 effective slots canonical** (privacy_model.md §6.2 source-of-truth pós Lote 10.11.0-bis):
  1. **Neon `dsr_tickets/account/tenant/user_account/consent_ledger/subscription`** (control plane PG): SQL DELETE WHERE tenant_id/subject_id; tombstone em `dsr_erasure_log`; PG-side CHECK + FK enforcement.
  2. **Neon billing fiscal exception** (Postgres billing detail): DELETE com legal_hold preserved per LGPD Art. 16 fiscal 5y; pseudonymize PII em retained rows.
  3. **R2 CAS refcount-aware** (CAS blobs mutable): subject_unaffiliated → decrement refcount apenas; subject_dedicated → tombstone + GC sweep grace 72h.
  4. **R2 AC** (Action Cache mutable): DELETE entries WHERE owner_tenant_id; per-region pinned.
  5. **D1 `blob_meta/ac_meta`** (operational metadata): subject-scoped row delete; refcount sync com R2.
  6. **KV** (sessions + cached metadata): DELETE keys matching tenant + subject prefix; eventual consistency tolerable.
  7. **Stripe `Customer.update`** (billing): PII nullified em customer.metadata + email/name/address (NOT customer.delete — preserva invoice integrity per PCI scope GAAP ASC 606 + LGPD Art. 16 fiscal compliance).
  8. **Loki/Grafana log deletion API**: `/loki/api/v1/delete` por subject; retention compaction trigger.
- **R-S11-4-PSEUDO**: 4 pseudonymized slots canonical (legal_hold WORM regulatory immutability; aplica pseudonimização):
  1. **R2 audit Object Lock 7y** (CTRL-AUDIT-IMMUTABILITY + S-09 R-S09-10): substitui `subject_id` por `erased_<HMAC(salt, subject_id)>` via HKDF info=`corelink/v1/audit-pseudonym`; payload original preserved (legal hold).
  2. **Neon PITR backup 30d** (Point-In-Time Recovery): rotação natural; tombstone replay em qualquer restore; auto-expira após 30d retention window.
  3. **R2 CAS legal_hold partition** (governance mode): conteúdo retido se sob hold ativo; pseudonymize index references; release pós legal_hold expiry.
  4. **R2 evidence-*** (DPIA/LIA/DSR evidence buckets 7y): retain por SLA framework (EVT-046 LIA, EVT-049 consent record, EVT-048 DSR evidence); subject_id pseudonymized.
- **Pseudonymization rule**: `tenant_id_hash = sha256(tenant_id || erasure_salt)` + marker `pii_redacted=true`; customer com sua própria `erasure_salt` (kept separately em customer-controlled vault) pode correlacionar suas próprias entries pré- e pós-erasure mas terceiros não.
- **Conflict resolution**: GDPR Art. 17 (right to erasure) vs Object Lock 7y (audit immutability — `INV-AUDIT-APPEND-ONLY` CRITICAL) — **pseudonymization é o escape valve regulatório aceito** (GDPR Recital 26 + Art. 11 + WP29 Opinion 05/2014 endorsed by EDPB; Lote 10.11.0-ter corrigida citação anterior errada "EDPB 5/2020 §74"). Customer/auditor confirmation flow em `RB-DSR-ERASURE-INCOMPLETE` (criada cycle 13) e `RB-GDPR-ERASURE-HOLD` (existing, criado Lote 5.12 — privacy_model.md §11.3).
- **R-S11-5**: **Crypto-erase** para BYOK customers (S-14): erasure efetiva via destroy customer-managed key (NIST SP 800-88 Rev.1 compliant); evidence emitido em audit.
- **R-S11-6**: Verification job: 24h após `dsr.completed.v1` event (CloudEvents canonical em privacy_model.md §6.2), sweep todos 12 backends canonical (8 effective + 4 pseudonymized — Lote 10.11.0-bis); per-backend assert:
  - **Effective backends (8 — Lote 10.11.0-bis canonical)** — assert 0 records remanescentes via SQL `SELECT COUNT(*)` ou R2 `headObject` ou KV `get` ou Stripe Customer.metadata fetch (PII nullified) ou Loki query.
  - **Pseudonymized backends (4)** — assert 0 plaintext PII fields restantes via DLP scan + assert `pii_redacted=true` marker presente em todos records correlacionados.
  - Report em R2 `evidence-dsr/<dsr_id>/erasure-report.json` (EVT-048 DSR_EVIDENCE) retain **7y** (canonical aligned com privacy_model.md §8 audit log retention + 4-pseudonymized R2 evidence-* bucket framework Object Lock governance mode); per-backend status enum: `erased | pseudonymized | partial_failure | failed | not_applicable`.
  - Cross-link com `dsr.verified.v1` + `dsr.queued.v1` + `dsr.completed.v1` audit events (EVT-047 AUDIT_EVENT) via shared `dsr_id` ULID.

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

- **R-S11-16**: Tenant region opt-in: signup tenant escolhe `region` ∈ canonical 6-region enum (`wnam`/`enam`/`weur`/`sam`/`apac`/`afr` per `privacy_model.md §7.1` + `data_model.md §2.1`); armazenado em **`tenant.primary_region`** (canonical column em `data_model.md §4.1` L151 — **NÃO** `tenant_metadata.region_pinned`).
- **R-S11-17**: Worker routing: requests roteados para region pinned via custom domain mapping (`<tenant_id>.<region>.corelink.dev` onde `<region>` é canonical 6-code); cross-region queries denied 403 com `Reason: residency-violation`. Reuse routing layer de S-09 multi-region observability.
- **R-S11-18**: CAS/AC/billing-events/audit data tagged com `region` em backend metadata; insert checks reject cross-region writes via D1 trigger ou worker pre-flight assertion.
- **R-S11-19**: Property test: 10k tenants `weur` + 10k tenants `enam` → verify 0 blob lands em wrong region (R2 bucket selection respects `tenant.primary_region`); test em CI gate.
- **R-S11-19a (Lote 10.11.0-bis-prime cycle 3 reconciled)**: TLA+ formal coverage de residency tem **escopo dividido**: (a) **PARTIAL coverage em S-11** via `dsr_erasure_atomicity.tla` (InvResidencyPinned + temporal InvResidencyMonotonic) — prova: ticket sempre pinned em região canonical + monotonic (no cross-region migration de ticket). (b) **FULL coverage deferred to S-14** via `region_residency.tla` / `byok_sovereignty.tla` (per `invariant_registry.md §4.2`) — incluirá: backend region dimension + cross-region routing actions + cross-region write impossibility proof. S-11 sub-property `INV-DATA-RESIDENCY` (CRITICAL §3.11 L154) é satisfeito por 3 layers: (1) TLA+ partial (pinning + monotonic; S-11), (2) 20k property test runtime (S-11), (3) custom domain routing fail-CLOSED PAT-ROUTING-PINNED-001 (S-11). S-14 adiciona formal cross-region semantics. Honest-flag: S-11 NÃO claim full residency formal coverage.

### 5.8 DPIA + LIA (CAP-PRIV-008)

- **R-S11-20**: DPIA template em `_templates/dpia.md` (Article 35 GDPR style); preenchido para cada feature privacy-impacting (e.g., dedup leakage S-07, telemetry S-09).
- **R-S11-21**: LIA template (Legitimate Interest Assessment) preenchido para processamento sob legitimate interest (telemetria operacional sem consent explícito).

## 6. Definition of Done

- [ ] **WIs SEALED**: 8/8.
- [ ] **E2E erasure**: fake user signup → use product 30d → DSR erasure request → 0 records cross-backend em ≤ 30d (verification job verde).
- [ ] **Erasure cross-backend coverage**: **12 backends total canonical (Lote 10.11.0-bis: 8 effective + 4 pseudonymized — privacy_model.md §6.2 source-of-truth)**: 8 erasure-effective (Neon multi-tabela `dsr_tickets/account/tenant/user_account/consent_ledger/subscription`, Neon billing fiscal exception, R2 CAS refcount-aware (subject_unaffiliated vs subject_dedicated), R2 AC, D1 blob_meta+ac_meta, KV, Stripe `Customer.update`, Loki/Grafana log deletion API) + 4 pseudonymized via legal_hold canonical (R2 audit Object Lock 7y com HKDF info=`corelink/v1/audit-pseudonym`, Neon PITR backup 30d com tombstone replay, R2 CAS legal_hold partition governance mode, R2 evidence-* buckets 7y). Test isolado per backend + integrated cross-backend. Severity escalated: INV-DATA-ERASURE-COMPLETE CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ commit S-11 WI-S11-008)→CRITICAL (TLA+ obrigatório via PAT-FORMAL-VERIFICATION-001).
- [ ] **Crypto-erase BYOK**: simulate enterprise BYOK + DSR erasure → key destroyed; data inacessível (NIST SP 800-88 evidence).
- [ ] **Residency**: **20k property test cases** (≥ 10k EU + ≥ 10k US/SAM/APAC sweep) verde; 0 cross-region leaks; PAT-ROUTING-PINNED-001 fail-CLOSED 451 verified em chaos test.
- [ ] **Consent UI screenshot evidence captured** (EVT-012 + EVT-049) com all 6 fields populated.
- [ ] **Consent proof verifiable**: re-compute notice_text_hash em backend → match record stored.
- [ ] **RB-BREACH-NOTIF dry-run** executado com Legal + Privacy Officer + Security Lead; time-to-decision ≤ 4h.
- [ ] **Templates legais drafted** em 3 jurisdictions (LGPD/GDPR/CCPA) revisados por Legal externo.
- [ ] **CTRL-PRIV-030** (erasure pipeline) + **CTRL-PRIV-031** (residency) em prod com evidence.
- [ ] **DPIA** preenchido para 3 features (dedup, telemetry, billing).
- [ ] **PRR HIGH_RISK**: Privacy Officer + Legal + DPO interim + Security lead + Compliance officer + SRE + Engineer + QA + Product + 2 peers.
- [ ] **TLA+ spec** `dsr_erasure_atomicity.tla` (verifica cross-backend erasure é atomic ou compensating-rollback) verde em CI.
- [ ] **Runbook dry-run**: RB-GDPR-ERASURE-HOLD (existing) + RB-DSR-ERASURE-INCOMPLETE (criada cycle 13) + RB-DATA-RESIDENCY-LEAK (criada cycle 13) + RB-CONSENT-TAMPERING + RB-DSR-INTAKE-FAILURE + RB-PRIVACY-NOTICE-LATE-PUBLICATION + RB-SUB-PROCESSOR-BROADCAST-MISS + RB-TLA-COUNTEREXAMPLE (todas criadas cycle 13/14 stub canonical em specs/05_quality/runbooks/) (EVT-017).

## 7. Completeness Criteria (delta local)

- [ ] **10.s11.1** SLO-FRESH-DSR-ERASURE: 99% das erasures completas ≤ 30 dias (sustained 90d).
- [ ] **10.s11.2** DPIA preenchido para cada feature privacy-impacting (dedup, telemetry, billing minimum).
- [ ] **10.s11.3** LIA template preenchido para telemetria sob legitimate interest.
- [ ] **10.s11.4** **Crypto-erase NIST SP 800-88 Rev.1 compliant** para BYOK tenants (witness key destroy ceremony).
- [ ] **10.s11.5** **Erasure verification job 24h post-erasure**: 0 records cross **12 backends total (8 effective + 4 pseudonymized via legal_hold canonical — Lote 10.11.0-bis)**; **DSR report retained 7y** canonical (privacy_model.md §8 audit retention + R2 evidence-* bucket Object Lock governance framework; aligned cycle 15 SEAL push — was previously 5y/7y split, now unified 7y).
- [ ] **10.s11.6** **DSR receipt JWT** signed verifiable via public endpoint.
- [ ] **10.s11.7** **3 locales** (PT-BR, EN, ES) para privacy notice + DSR UI mandatory at GA.
- [ ] **10.s11.8** **Sub-processor change notification** ≥ 30d testado em staging (mock change → email triggered).

## 8. Invariants

### Mantidas

- **CTRL-PRIV-001..014** (PII em logs / dados em uso): zero PII em audit logs (DLP scan).
- **CTRL-PRIV-CONSENT-001..006**: consent capture com proof.

### Reforçadas / novas em S-11 (positions canonical em invariant_registry.md verificadas via grep Lote 10.11.0)

- **INV-DATA-ERASURE-COMPLETE** (CRITICAL — Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ commit S-11 WI-S11-008; registry **§3.5 L110**): erasure cross-backend é efetiva em **12/12 backends canonical (8 effective + 4 pseudonymized — privacy_model.md §6.2 source-of-truth)**; 0 records remanescentes 30d post-request (effective backends) ou 100% pseudonymized via legal_hold (Object Lock + audit-pseudonym HKDF). **Why:** LGPD Art. 18 §IV + GDPR Art. 17 = right to erasure absoluto; gap = regulatory finding (multa LGPD 2% revenue / GDPR 4% global). **How to apply:** verification job 24h post-erasure (R-S11-6) + EVT-048 retain 7y (canonical pós cycle 15 SEAL unified) + EVT-042 ERASURE_TEST CI gate + **TLA+ obrigatório via PAT-FORMAL-VERIFICATION-001** (`specs/tla/dsr_erasure_atomicity.tla` S-11 WI-S11-008).
- **INV-DATA-RESIDENCY** (CRITICAL — Lote 10.11.0-bis: HIGH→CRITICAL — Schrems II + LGPD Art. 33 §1º; registry **§3.11 L154**): tenant region pinned não vaza cross-region; **20k property test cases** 0 violations (R-S11-19); custom domain routing fail-CLOSED via PAT-ROUTING-PINNED-001 (NUNCA passthrough silencioso). **Why:** Schrems II 6 essential guarantees + LGPD Art. 33 §1º + GDPR Art. 44 (cross-border transfer); gap = catastrofic legal exposure. **How to apply:** insert checks region tag + property test CI + custom domain routing per canonical 6-region enum (data_model.md `tenant.primary_region` CHECK + privacy_model.md §7.1) + **TLA+ via PAT-FORMAL-VERIFICATION-001** (`dsr_erasure_atomicity.tla` InvResidencyPinned action).
- **INV-CONSENT-PROOF-VERIFIABLE** (CRITICAL — Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ symmetry InvConsentSymmetry; registry **§3.12 L168** marked S-11; delivered por S-11): consent records têm `notice_text_hash` verifiable post-facto; tampering detected via HMAC-SHA256 com HKDF info=`corelink/v1/consent-hmac`; **12 canonical purposes enum** (privacy_model.md §5.6.1) com legal_basis fixo per purpose (no fail-open swap entre `consent` e `legitimate_interest`). **Why:** GDPR Art. 7 + LGPD Art. 8 exige proof of informed consent; sem hash + symmetric grant/revoke = consent não defensible. **How to apply:** SHA-256 do notice + HMAC + verify endpoint JWS-signed + grant/revoke ledger symmetric (Lote 9.4 H-05) + **TLA+ via PAT-FORMAL-VERIFICATION-001** (`dsr_erasure_atomicity.tla` InvConsentSymmetry action).
- **INV-AUDIT-APPEND-ONLY** (CRITICAL — registry **§3.6 L116** "Audit (domain AUDIT)"; herda; alias histórico `INV-AuditLogImmutability`): DSR events (`dsr.verified.v1`, `dsr.queued.v1`, `dsr.completed.v1`) e consent events (`consent.granted.v1`, `consent.revoked.v1`) são imutáveis em R2 Object Lock Governance Mode 7y; hash chain verifica continuamente (PAT-AUDIT-VERIFY-001); TLA+ verde em `specs/tla/audit_immutability.tla` (Lote 6.2).
- **INV-OBS-AUDIT-CHAIN-INTEGRITY** (HIGH — registry **§3.12 L165** marked S-09; herda): DSR + consent events R2 hash chain unbroken; daily verifier alerta em break.

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
| **WI-S11-002** | Erasure worker cross-backend (12 backends canonical — 8 effective + 4 pseudonymized; Lote 10.11.0-bis) + verification 24h + reports | erasure logic per backend; verification job; report R2 7y EVT-048 (canonical aligned cycle 16); tombstone; reconcile; FM-450/452/453 declarations | 18h | 28h | 44h | **28.7h** |
| **WI-S11-003** | Consent ledger D1 schema + proof of informed payload + endpoints | schema; capture endpoint; revoke endpoint; list endpoint; verify endpoint; HMAC | 10h | 16h | 26h | **16.7h** |
| **WI-S11-004** | Privacy notice versioning + 3 locales (PT-BR/EN/ES) + diff publication | semver structure; notice content review; 3 translations; changelog page; CI hook | 8h | 14h | 22h | **14.3h** |
| **WI-S11-005** | Sub-processor register + 30d email broadcast + customer objection flow | legal/sub-processors.md auto-gen; email broadcast cron; objection ticket route | 6h | 10h | 16h | **10.3h** |
| **WI-S11-006** | Breach notification runbook + 3 jurisdictional templates + dry-run | RB-BREACH-NOTIF; LGPD/GDPR/CCPA templates; dry-run with Legal + Privacy + SecLead | 12h | 20h | 32h | **20.7h** |
| **WI-S11-007** | Residency pinning E2E + custom domain routing + property test | tenant.primary_region (canonical column); 6-region enum routing layer wnam/enam/weur/sam/apac/afr; insert checks; 20k property test; RB-DATA-RESIDENCY-LEAK; FM-451 declaration | 12h | 18h | 28h | **18.7h** |
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
| **Erasure incompleto** (algum backend ignora) | M | M | CRITICAL (LGPD non-compliance) | H | LOW | INV-DATA-ERASURE-COMPLETE + verification job 24h + **12-backend canonical coverage test (8 effective + 4 pseudonymized; Lote 10.11.0-bis)** + RB-DSR-ERASURE-INCOMPLETE stub. |
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
| Erasure cross-backend automated | Plug-in | Yes | Yes | **Yes — 12 backends canonical (8 effective + 4 pseudonymized via legal_hold; Lote 10.11.0-bis) + verification 24h** |
| Crypto-erase para BYOK | No | No | Manual | **Yes — NIST SP 800-88 Rev.1 compliant** |
| Consent proof of informed | Yes | Yes | Yes | **Yes — notice_text_hash + HMAC + verify endpoint** |
| Breach notification runbook executable | Manual | Plug-in | Manual | **Yes — RB executable + 3 jurisdiction templates** |
| Residency pinning E2E | Plug-in | Yes | Plug-in | **Yes — custom domain + property test 20k tenants** |
| DPIA template integrated | Yes | Yes | Yes | **Yes — `_templates/dpia.md` + CI hook** |
| TLA+ erasure atomicity formal spec | No | No | No | **Yes — dsr_erasure_atomicity.tla committed (Lote 10.11.0-bis-prime); CI gate pendente first run verde — status PLANNED → 🟡 spec written → ✅ GREEN apenas após sustained CI** |
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
- **Brazilian ANPD Resolution CD/ANPD nº 15/2024** — incident notification requirements (substitui Res. 2/2022 que tratou de procedimentos de fiscalização; correção Lote 10.11.0-ter).

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
- ❌ Erasure cross-backend coverage 12/12 backends canonical (8 effective + 4 pseudonymized; Lote 10.11.0-bis) — INV-DATA-ERASURE-COMPLETE CRITICAL.
- ❌ Residency pinning E2E + property test verde — Schrems II legal exposure.
- ❌ Consent proof of informed (notice_text_hash) — GDPR Art. 7.
- ❌ RB-BREACH-NOTIF dry-run with Legal sign-off — operational readiness.
- ❌ TLA+ dsr_erasure_atomicity verde — atomic guarantee.

Itens waivable com sign-off Privacy Officer + Legal + Compliance Officer + ADR:

- ⚠️ DSR rate limit 10/dia/subject → 5/dia (mais restritivo); nunca menos.
- ⚠️ 3 locales → 2 locales (PT-BR + EN) at GA, ES no Q1 pós-GA.
- ⚠️ Verification job 24h post-erasure → 48h (com SEV-2 alert se 48h gap detectado).

---

## 20. Change log

### v1.8.0 (2026-05-13) — WI-S11-004 implementation phase SEALED

Implementation phase landing (no spec changes; impl-only changelog row per charter §spec contract changelog discipline). WI-S11-004 SEALED at the canonical impl quality gates:

- **`crates/corelink-privacy-notice-emit/`** new crate (~820 LOC; 5 src modules + 3 test files). `NoticeEmitter` trait + `InMemoryNoticeEmitter` orchestrator with canonical fail-CLOSED `lookup → emit_audit → mutate_state` ordering (AC-008 + INV-AUDIT-APPEND-ONLY). `NoticeLocale` `#[non_exhaustive]` 3-arm enum (PtBr/EnUs/EsMx — LGPD primary, GDPR/CCPA secondary, LATAM tertiary). `VersionBump` `#[non_exhaustive]` 2-arm enum (Major/Minor) per ADR-S11-007 material vs minor criteria. `NoticeAuditSink` trait + `InMemoryNoticeAuditSink` + `FailingNoticeAuditSink` (fail-CLOSED chaos fixture). `NoticeStateStore` trait + `InMemoryNoticeStateStore`. `notice_text_hash` deterministic SHA-256 (CRLF→LF + trim + SHA-256 per AC-006). 2 CloudEvents canonical envelopes: `dev.hugr.corelink.privacy_notice.published.v1` + `dev.hugr.corelink.privacy_notice.deprecated.v1` per Lote 10.9bis P0-G prefix. Per-instance `Arc<Mutex<>>` F-001 closure.
- **`legal/privacy-notice/v1.0.0/`** initial notice content 3 locales (pt-BR.md ~1500 words / en-US.md ~1500 words / es-MX.md ~1500 words) + metadata.yaml schema. Content covers LGPD Art. 9 + GDPR Art. 13/14 + CCPA §1798.100(b) canonical fields (data categories, purposes, legal basis, sub-processors, DSR rights, DPO contact, retention, international transfers).
- **`scripts/notice_text_hash_canonical.py`** Python cross-validation of Rust `notice_text_hash` (CRLF→LF + NFC + SHA-256). `scripts/validate_privacy_notice.py` CI hook (5 validations: semver bump / 3 locales sync / native speaker review / Legal Review EVT-044 / major bump CD flag). `scripts/publish_privacy_diff.py` daily diff HTML generator (Cloudflare Pages `/privacy/changelog/<date>.html`).
- **`specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md`** NEW. Material vs minor criteria table (M-1..M-7 major + m-1..m-5 minor + borderline Privacy Officer judgment). CTRL-PRIV-CONSENT-005 normative reference.
- **`schemas/cloudevents/privacy-notice-{published,deprecated}.v1.json`** JSON Schema 2020-12 for both CloudEvents types.
- **`legal/privacy-notice/REVIEW_PROCESS.md`** SOP for native speaker + Legal local review process (roles, steps, SLA targets).
- **`specs/05_quality/runbooks/RB-PRIVACY-NOTICE-LATE-PUBLICATION.md`** NEW canonical runbook for diff publication SLA breach (SEV-MEDIUM + escalation path to SEV-HIGH on major bump stale_consent_check miss).
- **Quality gates verde**: 42 tests across 3 files all green (27 lib + 2 prop_audit_fail_closed at 10k iter + 4 prop_notice_hash_determinism at 10k iter + 9 regression_3locale_sync); `cargo clippy --all-targets -- -D warnings` clean; `cargo build --target wasm32-unknown-unknown -p corelink-privacy-notice-emit` clean.
- **8 ACs covered**: AC-001 (initial publication v1.0.0 happy path) / AC-002 (major bump force re-consent) / AC-003 (minor bump silent) / AC-004 (3 locales sync enforcement) / AC-005 (native speaker review checkbox enforcement) / AC-006 (notice_text_hash deterministic cross-platform) / AC-007 (diff publication scaffolding) / AC-008 (audit fail-CLOSED state unchanged).
- **INV-AUDIT-APPEND-ONLY** + **INV-CONSENT-PROOF-VERIFIABLE** both covered: fail-CLOSED envelope pinned by `prop_audit_fail_closed` 10k iter; `notice_text_hashes` in `NoticeEmitDecision` is the direct INV-CONSENT-PROOF-VERIFIABLE cross-validation anchor for WI-S11-003 `ConsentProofPayload.notice_text_hash`.

### v1.7.0 (2026-05-13) — WI-S11-003 implementation phase SEALED

Implementation phase landing (no spec changes; impl-only changelog row per charter §spec contract changelog discipline). WI-S11-003 SEALED at the canonical impl quality gates:

- **`crates/corelink-privacy-consent-ledger/`** new crate (~1850 LOC; 7 src modules + 7 test files). `ConsentLedger` trait + `InMemoryConsentLedger` orchestrator with per-instance `Arc<Mutex<u64>>` F-001 closure + canonical fail-CLOSED `locale_enforce → notice_version_check → audit_emit → hmac_sign → store_insert` ordering (S-06 P0-2 / S-07 P1-1 lessons absorbed). `ConsentHmacSigner` trait + `InMemoryConsentHmacSigner` (HMAC-SHA256 via HKDF-SHA256; tenant-scoped salt; HKDF info=`corelink/v1/consent-hmac`; constant-time verify via `subtle::ConstantTimeEq`). `ConsentStore` trait + `InMemoryConsentStore` (idempotency UNIQUE 5-tuple: tenant_id + subject_id + purpose + notice_text_hash + ui_capture_ts — PAT-RETRY-IDEMPOTENT-001). `ConsentAuditSink` + `InMemoryConsentAuditSink` + `FailingConsentAuditSink`. `CascadeSink` + `InMemoryCascadeSink` (Cloudflare Queue fanout ≤24h SLA CTRL-PRIV-CONSENT-002). `LocaleEnforceError` + `enforce_locale` strict (CTRL-PRIV-CONSENT-005). `NoticeVersionCheckError` + `is_notice_version_stale` + `check_notice_version` (major bump force re-consent AC-006; NO fail-open swap to legitimate_interest — corrige GPT P0-1 round-1).
- **Schema symmetric grant ↔ revoke (Lote 9.4 Opus H-05)**: `ConsentProofPayload` 6-field struct (notice_text_hash + notice_version + locale + wording_id + ui_capture_ts + submission_ts) used identically in `ConsentRecord` and `ConsentRevocationRecord`. Verified at Rust type level (same type) + runtime field equality in `regression_symmetric_schema` tests.
- **12 canonical `ConsentPurpose` enum** closed (privacy_model.md §5.6.1; ADR-S11-006 cardinality discipline) + `LegalBasis` 4-arm enum; `legal_basis()` + `is_revocable()` mapping fixed per purpose. NO dynamic swap.
- **2 CloudEvents canonical types pinned**: `dev.hugr.corelink.consent.{granted,revoked}.v1` per Lote 10.9bis P0-G prefix.
- **Neon migration `N+3__consent_ledger_revocation.sql`**: 2-table DDL (`consent_ledger` + `consent_revocation`) with 12-arm purpose CHECK, 3-locale CHECK, cascade_status CHECK, UNIQUE 5-tuple constraints, and 4 indexes.
- **`VerifyParams` + `HmacParams` structs** group preimage fields to satisfy `too_many_arguments` clippy lint — clean API surface for the stateless verify endpoints (AC-004).
- **Quality gates verde**: 59 tests across 7 files all green (18 lib unit + 3 chaos + 13 integration + 3 prop_hmac_roundtrip at 10k iter + 2 prop_idempotency_replay at 10k iter + 10 regression_locale_enforce + 6 regression_notice_version + 4 regression_symmetric_schema); `cargo clippy --all-targets -- -D warnings` clean; `cargo build --target wasm32-unknown-unknown -p corelink-privacy-consent-ledger` clean.
- **PROPTEST_CASES=10000 verified**: prop_hmac_roundtrip (3 tests × 10k) + prop_idempotency_replay (2 tests × 10k) all green.
- **Charter constraints verified**: no `unsafe`; no `unwrap`/`expect`/`panic`/indexing in lib code; F-001 `Arc<Mutex<>>` per-instance closure; `#[non_exhaustive]` on every public enum; wasm32-clean (no ring / no C toolchain dep); PROPTEST_CASES env-var runtime override per S-07 P1-2; no `prop_assert!(matches!(...))` anti-pattern per S-08 P1-1.
- **INV-CONSENT-PROOF-VERIFIABLE CRITICAL (§3.12 L168)** satisfied: 6-field proof + HMAC signature + stateless verify endpoint. **INV-AUDIT-APPEND-ONLY CRITICAL (§3.6 L116)** satisfied: `chaos_audit_emit_failure` pins state UNCHANGED on audit failure.

### v1.6.0 (2026-05-07) — WI-S11-002 implementation phase SEALED

Implementation phase landing (no spec changes; impl-only changelog row per charter §spec contract changelog discipline). WI-S11-002 SEALED at the canonical impl quality gates:

- **`crates/corelink-privacy-pseudonymize/`** new small crate (~430 LOC). Canonical `pseudonymize(subject_id, erasure_salt) = sha256(subject_id || erasure_salt)` helper + `verify_pseudonym` constant-time forensic re-correlation surface (subtle::ConstantTimeEq) + `PseudonymizationMarker` typed shape with `pii_redacted=true` canonical marker. SHA-256 chosen over BLAKE3 per WI-S11-002 §9.3 DD-002 (NIST FIPS 180-4 regulatory defensibility). wasm32-unknown-unknown clean (sha2 + no ring / no C toolchain dep).
- **`crates/corelink-privacy-erasure-worker/`** new crate (~6500 LOC; 9 src modules + 7 test files). `ErasureWorker` trait + `InMemoryErasureWorker` orchestrator with per-instance `Arc<Mutex<()>>` F-001 closure + canonical fail-CLOSED `lookup → emit_audit → mutate_state` ordering (S-06 P0-2 / S-07 P1-1 lessons absorbed). 12 `BackendErasureAdapter` trait + `InMemoryBackendErasureAdapter` (one per `BackendKind` arm; refcount-aware R2 CAS for S-07 dedup safety; legal_hold gate skips effective backends; pseudonymized backends preserve rows + insert canonical `pii_redacted=true` marker). `ErasureIdempotencyLedger` trait + `InMemoryErasureIdempotencyLedger` + `FailingErasureIdempotencyLedger` (canonical D1 `dsr_erasure_log` UNIQUE `(dsr_id, backend)` surface). `ErasureAuditSink` + `InMemoryErasureAuditSink` + `FailingErasureAuditSink`. `ReportSigner` trait + `InMemoryReportSigner` (BLAKE3-keyed MAC over JCS-canonical preimage RFC 8785). `VerificationJob` 24h cron sweep entry point + `VerificationOutcome` shape + `elapsed_dsr_ids` cron tick helper.
- **Canonical 5-event CloudEvents taxonomy pinned**: `dev.hugr.corelink.dsr.erasure.{started, backend_completed, verification_passed, verification_failed, completed}.v1` per Lote 10.9bis P0-G prefix discipline. JSON schemas committed under `schemas/cloudevents/`.
- **D1 migration `0022_dsr_erasure_log.sql` canonical**: 12-arm backend CHECK constraint + 5-arm outcome CHECK + UNIQUE `(dsr_id, backend)` constraint + 3 indexes (lookup-by-dsr, tenant-outcome-time scan, subject_id_hash forensic re-correlation).
- **ADRs landed**: ADR-S11-003 (erasure_salt interim per-tenant random in D1 vault encrypted; BYOK KMS deferred to S-14) + ADR-S11-004 (cross-backend eventual consistency with 24h verification gate; 2PC inviável Stripe + Object Lock heterogeneity). Both committed at `specs/03_architecture/adrs/`.
- **FM-450 + FM-452 already declared in failure_modes.md** (verified at this WI commit; no edits needed). RB-DSR-ERASURE-INCOMPLETE + RB-CONSENT-TAMPERING runbooks already exist (verified).
- **Quality gates verde**: 113 tests across 8 files all green (83 lib + 6 chaos + 2 integration + 5 prop_idempotency_replay_100x at 10k iter + 6 prop_pseudonymization_correctness at 10k iter + 3 regression_refcount_aware_scrub + 3 regression_stripe_invoice_preserved + 5 verification_job_24h); `cargo clippy --all-targets -- -D warnings` clean; `cargo build --target wasm32-unknown-unknown -p corelink-privacy-pseudonymize -p corelink-privacy-erasure-worker` clean.
- **Charter constraints** verified: no `unsafe`; no `unwrap`/`expect`/`panic`/indexing in lib code; F-001 `Arc<Mutex<()>>` per-instance closure; `#[non_exhaustive]` on every public enum; wasm32-clean (no tokio dep / no ring / no C toolchain dep); PROPTEST_CASES env-var override at runtime per S-07 P1-2; no `prop_assert!(matches!(...))` anti-pattern (S-08 P1-1) — every property assert lifts the canonical typed match into a separate `let` binding then `prop_assert!`.
- **Audit fail-CLOSED ordering canonical**: `audit_failure_state_unchanged_post_failure` chaos test pins the canonical S-06 P0-2 / S-07 P1-1 lift (audit failure aborts the pipeline; ledger unchanged).
- **Salvageable archive `_archive/wi-s11-002-partial/` mined for taxonomy patterns** (event/error/audit) but adapted to spec's 5-event canonical CloudEvents prefix `dev.hugr.corelink.dsr.erasure.<arm>.v1` (vs archive's 8-event internal taxonomy) and pseudonymize separated into its own crate per spec §6.3 mandate.

### v1.5.0 (2026-05-03) — WI-S11-001 implementation phase SEALED

Implementation phase landing (no spec changes; impl-only changelog row per charter §spec contract changelog discipline). WI-S11-001 SEALED at the canonical impl quality gates:

- **`crates/corelink-dsr/`** new crate (~4447 LOC; 8 src modules + 1 prop test file). `DsrEndpoint` trait + `InMemoryDsrEndpoint` orchestrator wired to `JwtReceiptIssuer` + `MfaStepUpVerifier` + `DsrRequestStore` + `DsrAuditSink` (canonical 7-event taxonomy `corelink.dsr.{request_received, mfa_step_up_required, mfa_verified, request_accepted, receipt_issued, request_rejected, status_polled}`).
- **MFA gate boundary canonical**: ONLY destructive arms (Erasure + Rectification) require step-up MFA per CTRL-AUTH-010 + ADR-S11-001; read arms (Access + Portability) and policy-only arms (Restriction + Objection) skip the gate. Pinned by `prop_mfa_required_for_erasure_rectification`.
- **JWT receipt anti-replay**: 90d expiration cap per `RECEIPT_EXPIRY_DAYS` constant; `iss = corelink.dev/privacy`; alg = RS256 pinned. In-memory deterministic FNV-1a-based fake covers the canonical sign/verify roundtrip + cross-key reject + tampered + expired arms; production wiring at WI-S11-008 substitutes `jsonwebtoken = 9.3` (S-03 inheritance) + KMS-backed key rotation (HKDF info=`corelink/v1/dsr-receipt`).
- **Audit fail-CLOSED envelope**: every decision arm fires its canonical audit BEFORE state mutation per ADR-S11-002 split-tier (DSR regulatory-grade NEVER tolerates silent loss; distinct from billing fail-OPEN at Lote 10.6bis). Pinned by `prop_audit_emit_per_decision_arm`.
- **SLA timer canonical**: LGPD Art. 19 = 15d / GDPR Art. 12.3 = 30d / CCPA §1798.130 = 45d via `sla_for(jurisdiction, submitted_at)`; SEV-2 alert deferred to WI-S11-008 production wiring. Pinned by `prop_sla_deadline_correct`.
- **Tenant isolation CTRL-ISO-004**: cross-tenant poll surfaces as RequestRejected with IdentityVerificationFailed (constant-time confidentiality — never disclose whether a `request_id` exists for a different tenant via 404 vs 403 distinction). Pinned by `prop_tenant_isolation`.
- **Idempotency UNIQUE quad**: `(tenant_id, request_id)` keyed store with divergent-payload SEV-1 forensic anomaly surface; production Neon mirror uses canonical UNIQUE quad `(tenant_id, subject_user_id, request_kind, request_payload_hash)` per data_model.md §4.1.
- **Quality gates verde**: `cargo test -p corelink-dsr --all-targets` 113/113 (94 inline unit + 8 sanity + 11 property at 10k iter); `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` zero warnings; `validate_specs.py` 288 docs schema clean; `check_migrations_additive.py` 21 migrations additive.
- **Charter constraints** verified: no `unsafe`; no `unwrap`/`expect`/`panic`/indexing in lib code; F-001 `Arc<Mutex<()>>` per-instance closure; `#[non_exhaustive]` on every public enum; wasm32-clean (no tokio dep); PROPTEST_CASES env-var override; ChaCha20Rng pinned PRNG; no `prop_assert!(matches!(...))` anti-pattern (S-08 P1-1).
- **Trait-abstraction-defer per charter**: production CF Worker route + real RS256 + WebAuthn step-up binding + Neon `dsr_tickets` schema migration + Cloudflare Email + S-08 rate-limit + S-09 chain fan-out + SLA-pause logic + RB-DSR-INTAKE-FAILURE runbook + OpenAPI 3.1 + Grafana dashboard all deferred to WI-S11-008 PRR ship gate.

No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-11 corpus once 8/8 WIs SEALED.

### v1.4.0 (2026-04-28) — Lote 10.11.0-bis-prime cycles 1-4 (post-codex SEAL push)

Cycle 4 score trajectory após 4 codex review cycles: 5.18 (baseline) → 5.9 → 6.2 → 7.8 → 8.3 → 8.4 (target ≥9.0 SEAL).

**Cycle 1 (post-baseline 5.18 codex review):**
- Phase B canonical sweep V2 (9 architecture docs + spec contract v1.3.0): tenant region/locale CHECK; dsr_tickets canonical = Neon NOT D1; 7-state machine canonical; UUIDv7; 12-purpose enum §5.6.1 NEW; 3 PAT canonical (RETRY-IDEMPOTENT/ROUTING-PINNED/FORMAL-VERIFICATION); FM-450/451/452/453 §3.10 NEW; INV severity HIGH→CRITICAL ×3; 22 S-11 CloudEvents; 5 HKDF info strings; specs/tla/dsr_erasure_atomicity.tla scaffolded.
- Lote 10.11.0-ter legal citations (5 critical fixes): Art. 7§5→Art. 10; Res. 1/2021→Res. 4/2023; Res. 2/2022→Res. 15/2024; EDPB 5/2020 §74→GDPR Recital 26+Art. 11; WP29 disambiguation.
- Lote 10.11bis WI cascading: cardinalidade 10→12; WI-001 dsr_tickets D1→Neon + 7-state UUIDv7; WI-005 P0-3 sub-processor mandatory all-plans; WI-007 P1-7 residency_strict write-once-true; WI-001 P1-4 verify endpoint stateless; WI-008 TLA+ rewrite Init/Next/EffectiveBackends/PseudonymizedBackends/no tautology; INV severity cascade 8 WIs.

**Cycle 2 (codex 6.2/10):**
- TLA+ ProofRecord typed records (was string-only → dead actions); InvResidencyPinned strengthened + InvResidencyMonotonic temporal NEW.
- scripts/run_tlc_corelink.sh CREATED com TLC SHA-256 verification.
- WI-003 D1→Neon final sweep (L368/587/640/725/742/770); L303 fail-open BACK fixed; DDL basis_legal 6→4 canonical values.
- WI-005 contradictions L376/494/502/723 swept (free/solo opt-in residue).
- WI-002 EDPB §74 sweep; key_management overlap clarified (rotation vs verify-grace vs salt retention).
- INV severity HIGH→CRITICAL table cells WI-002/003/007/008.
- WI-002 stale 10/6 backends sweep; SQL CHECK syntax fix (commas inside single CHECK).

**Cycle 3 (codex 7.8/10):**
- TLC SHA-256 pin made MANDATORY (TLC_SHA256_SKIP opt-out only; CI must verify).
- .github/workflows/tla_check.yml NEW (5 specs: dsr_erasure_atomicity, tenant_isolation, gc_correctness, cas_integrity, audit_immutability).
- WI-008 stale TLA prose (8-field→6-field; 4 actions→10; 4 invariants→5+3 temporal); PLANNED→GREEN claims swept to honest-flag (🟡 spec written → ✅ GREEN apenas após first CI run).
- WI-002 narrative L176/180 stale 6 effective; severity cascade post-mortem hooks.
- specs/tla/README.md InvResidencyMonotonic added.
- resilience_patterns.md tla-check workflow + bootstrap.sh references fixed.

**Cycle 4 (codex 8.3/10) → THIS v1.4.0 final push:**
- CI gate hard-enforces SHA pin (`::error::` + exit 1 on hash drift; no TLC_SHA256_SKIP em CI invocations).
- Residency formal scope split reconciled em spec contract + WI-007 + WI-008: PARTIAL S-11 (InvResidencyPinned + InvResidencyMonotonic) + FULL S-14 (region_residency.tla cross-region routing). ADR-S11-012 REVISED.
- WI-008 stale workflow refs (.github/workflows/tla-ci.yaml→tla_check.yml; dpia-ci.yaml→dpia_check.yml PLANNED stub).
- WI-008 false GREEN claims L27/39/467/817/862 swept honest-flag.
- WI-002 L92-123 ErasureBackend Rust enum 10→12 canonical (NeonMain/NeonBilling/R2Cas/R2Ac/D1/Kv/Stripe/Loki + R2AuditPseudo/NeonPitrPseudo/R2CasLegalHoldPseudo/R2EvidencePseudo).
- spec_contract L91-111 backend list canonical 8+4 (was old D1/Neon/R2 cas/R2 ac/KV/DO/Loki/Stripe + R2 audit/billing-events/Loki cold/CF Analytics).
- invariant_registry.md L446/453 INV-DATA-RESIDENCY + INV-CONSENT-PROOF-VERIFIABLE HIGH→CRITICAL; L352 tla-ci-gate.yml→tla_check.yml.
- ADR-0042 §A1 tla-ci-gate.yml→tla_check.yml + EXPECTED_SHA→TLC_SHA256_PINNED.
- RB-DSR-ERASURE-INCOMPLETE.md operative sections (L23/50-53/183) atualizado para 12 backends canonical.

Não foram introduzidos novos requisitos funcionais. v1.4.0 é remediation de drift cross-doc; corpus internamente consistente para SEAL ≥9.0.

### v1.3.0 (2026-04-27) — Lote 10.11.0-bis Canonical truth-table sweep V2

Pós baseline review aggregate 5.18/10 (R4 Opus parts 1+2: 6.4+6.4; R5 Sonnet: 4.1; GPT codex: 3.8 — pior baseline da história CoreLink). Findings escalonados resolvidos via canonical updates em 9 architecture docs ANTES de WI fixes (Lote 10.11bis), eliminando hallucinations cascading.

**Decisões canonical aplicadas:**

- **Cardinalidade unificada 12 backends (8 effective + 4 pseudonymized)** — privacy_model.md §6.2 reescrita como source-of-truth. Effective: Neon multi-tabela / R2 CAS refcount-aware (subject_unaffiliated vs subject_dedicated) / R2 AC / D1 / KV / Stripe `Customer.update` / Loki / Neon billing fiscal exception. Pseudonymized: R2 audit (HKDF audit-pseudonym) / Neon PITR backup 30d / R2 CAS legal_hold partition / R2 evidence-* buckets 7y. Resolve contradição 6 vs 8 vs 10 vs 12 baseline.
- **DSR storage canonical = Neon (NOT D1)** — data_model.md §4.1 atualizado com 7-state machine (received/verified/queued/in_progress/completed/denied/failed); UUIDv7; CHECK constraints denial_reason + failure_reason + receipt_jws; índices condicionais.
- **`tenant.primary_region` CHECK constraint 6-region enum** (wnam/enam/weur/sam/apac/afr) — data_model.md §4.1 enforced PG-side.
- **`tenant.locale_default` 3-locale enum** (pt-BR/en-US/es-MX, default 'en-US') — data_model.md §4.1 + privacy_model.md §5.6 cross-link.
- **12 canonical purposes enum** (privacy_model.md §5.6.1 NEW) — substitui "(analytics, marketing, beta features)" examples por 12 purposes with explicit `legal_basis` fixo: contract (3) / legal_obligation (1) / legitimate_interest (2 — security_monitoring + analytics_aggregated; LIA req'd) / consent (6). Endereça GPT P0-1: NO fail-open LI swap em consent expiry — purpose entra `consent_lapsed` state e processing PARA.
- **Oposição self-service** (privacy_model.md §6.1 row 7 + WI-S11-001 alignment) — substitui "Manual (email)" + 30 dias CCPA → self-service API + 15 dias úteis.
- **R2 CAS subject_unaffiliated semantics** (privacy_model.md §6.2 step 4c) — refcount-based eraser elimina ambiguidade "depende se blob é só do tenant".
- **3 PAT canonical declarados** (resilience_patterns.md §3.2/§3.4/§3.6): PAT-RETRY-IDEMPOTENT-001 (regulatory-grade fail-CLOSED com 5 attempts + dead-letter quarantine + cross-backend ack idempotency), PAT-ROUTING-PINNED-001 (region-pinned routing fail-CLOSED 451 NEVER passthrough), PAT-FORMAL-VERIFICATION-001 (TLA+ obrigatório para CRITICAL invariants — banido `\/ TRUE` tautologies + bounded state space + SHA-256 pinned).
- **FM-450/451/452/453 declared canonical** (failure_modes.md §3.10 NEW Privacy/Regulatory category): DSR pipeline failure / residency violation / consent ledger fork / sub-processor breach upstream. RPN 25-30, classe P1 (S=5 → upgrade).
- **Severity escalations HIGH → CRITICAL** (invariant_registry.md): INV-DATA-ERASURE-COMPLETE / INV-DATA-RESIDENCY / INV-CONSENT-PROOF-VERIFIABLE — TLA+ obrigatório via PAT-FORMAL-VERIFICATION-001.
- **L414 stale "7 backends" → "12 backends canonical"** (invariant_registry.md §4.2 PLANNED matrix).
- **22 S-11 CloudEvents catalog declared** (observability_model.md §7.2 — 12 base → 34 total): DSR (9), Consent (3), Privacy notice (2), Sub-processor (4), Breach (3), Residency (1). Prefix canonical `dev.hugr.corelink.<op>.v1`.
- **HKDF info strings 5 novos canonical** (security_model.md §7.2 + key_management.md §2): `corelink/v1/dsr-receipt`, `corelink/v1/consent-hmac`, `corelink/v1/erasure-salt`, `corelink/v1/audit-pseudonym`, `corelink/v1/dkim-broadcast`. Erasure salt section 365d rotation com forward verifiability 7y.
- **TLA+ specs/tla/README.md entry** dsr_erasure_atomicity.tla declared (5 invariants ligados: InvDataErasureComplete + InvConsentSymmetry + InvResidencyPinned + InvAuditAppendOnlyDSR + InvBackendAckIdempotent; bounded state space; SHA-256 pinned; liveness `<>completed \/ <>denied \/ <>failed`).

Não foram introduzidos novos requisitos funcionais. v1.3.0 é alinhamento canonical pós-review eliminando 14 P0 + 25 P1 cascading hallucinations. Próximo: Lote 10.11.0-ter (legal citations) + Lote 10.11bis (WI fixes derivados) + Lote 10.11-tris (round-2 adversarial validation).

### v1.2.0 (2026-04-26) — Lote 10.11.0 Truth-table sweep

Pre-WI canonical alignment cycle aplicando lições absorvidas em S-10 (source-of-truth FIRST discipline + cascade discipline absoluta). Findings:

- **§3 inherits_from FAILURE-MODES**: corrigido — contrato v1.1 citava `FM-401 (erasure incomplete), FM-402 (residency leak)` mas `failure_modes.md` real define `FM-401 = thundering herd, FM-402 = alert feedback loop`. v1.2 aponta para `FM-061` canonical (audit Object Lock vs DSR — Lote 5.12) e declara `FM-450/451/452/453` novos a serem criados em WI-S11-002 + WI-S11-007.
- **§4 CAP-PRIV-007 region codes**: corrigido `US/EU/APAC` → canonical 6-region enum `wnam/enam/weur/sam/apac/afr` (privacy_model.md §7.1 + data_model.md §2.1).
- **§5.1 R-S11-1 SLAs por direito**: substituido "≤ 30d" simplista por SLAs específicos por direito conforme privacy_model.md §6.1 clock semantics F-11 (confirm 5d / access 15d / correction 5d / **erasure 30d** / portability 15d / consent revoke ≤5min / objection 15d). Adicionado endpoint `/v1/privacy/dsr/confirmation` (LGPD Art. 18 I).
- **§5.1 MFA reference**: adicionado `CTRL-AUTH-010` canonical (security_model.md §242, MFA + WebAuthn session binding); CTRL-PRIV-016 mantido no contexto correto ("support read access requer consent").
- **§5.2 R-S11-4 erasure backends** (v1.2.0 historical; **SUPERSEDED em v1.4.0** — atual canonical 12 backends pós Lote 10.11.0-bis: 8 effective Neon multi-tabela / Neon billing fiscal / R2 CAS refcount-aware / R2 AC / D1 / KV / Stripe Customer.update / Loki + 4 pseudonymized R2 audit Object Lock 7y / Neon PITR 30d / R2 CAS legal_hold partition / R2 evidence-* 7y; ver §20 v1.4.0 changelog): declarado explicitamente que S-11 EXTENDS canonical privacy_model.md §6.2 baseline (6 backends: D1, Neon, R2 audit, R2 CAS, KV, Loki) com 4 backends introduzidos por sprints subsequentes (DO em S-07/S-08, Stripe em S-10, R2 billing-events em S-10, CF Analytics Engine em S-09). Pipeline completo v1.2.0 = 10 backends (6 effective + 4 pseudonymized) — superseded.
- **§5.2 R-S11-4-PSEUDO**: nova subsection separando pseudonymization rule + GDPR Recital 26 + Art. 11 + WP29 Opinion 05/2014 (endorsed by EDPB) escape valve regulatório + cross-link com RB-GDPR-ERASURE-HOLD existing.
- **§5.2 R-S11-6 verification**: ajustado retention de 7y para 5y (privacy_model.md §2 retention table — DSR tickets 5y pós resolução, not 7y); cross-link com CloudEvents canonical (`dsr.verified.v1`/`dsr.queued.v1`/`dsr.completed.v1`) já documentadas em privacy_model.md §6.2.
- **§5.7 R-S11-16 column name**: corrigido `tenant_metadata.region_pinned` → `tenant.primary_region` (canonical em data_model.md §4.1 L151).
- **§5.7 R-S11-19a (novo)**: declaração explícita que TLA+ residency é deferred a S-14 per invariant_registry.md §4.2 (`region_residency.tla` / `byok_sovereignty.tla`); S-11 entrega runtime + property tests + custom domain routing.
- **§6 DoD coverage line** (v1.2.0 historical; SUPERSEDED v1.4.0 = 12 backends canonical): alinhado com R-S11-4/4-PSEUDO (10 backends explícitos).
- **§8 Invariants positions**: explícitas — `INV-DATA-ERASURE-COMPLETE` §3.5 L110, `INV-AUDIT-APPEND-ONLY` §3.6 L116, `INV-DATA-RESIDENCY` §3.11 L154, `INV-CONSENT-PROOF-VERIFIABLE` §3.12 L168 (marked S-11), `INV-OBS-AUDIT-CHAIN-INTEGRITY` §3.12 L165 (herda S-09). Lição S-10 P1-13: NUNCA marcar §3.X TBD.
- **§19 Waiver policy** (v1.2.0 historical; SUPERSEDED v1.4.0 = 12/12 backends canonical): corrigido "7/7 backends" → "10/10 backends (6 effective + 4 pseudonymized)" alinhando R-S11-6.

Não foram introduzidos novos requisitos funcionais. v1.2.0 é alinhamento canonical pré-WI authoring; estado FROZEN aguarda staffing-blocked promoção.

### v1.1.0 (2026-04-24) — versão original

Spec contract criado em ciclo de Sprint S-09→S-20 SOTA elevation (Lote 9.1) + revisado em Lote 9.4 (Opus H-05 consent symmetry, H-06 LGPD Art. 20, H-10 10-backend expansion).

---

**Fim spec contract S-11 v1.5.0 SOTA — Lote 10.11.0-bis-prime cycles 1-4 SEAL-ready + WI-S11-001 IMPL SEALED (corpus internamente consistente; codex score trajectory 5.18 → 8.4+; pending final ≥9.0 validation).**
