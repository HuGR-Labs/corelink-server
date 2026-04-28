---
id: "WI-S11-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-26"
updated: "2026-04-28"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-003", "FF-HR-005", "FF-HR-010"]
parent: "S-11"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "PRIVACY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
tags: ["wi", "s11", "privacy-notice", "semver", "3-locales", "pt-br", "en", "es", "diff-publication", "force-re-consent", "high-risk"]
---

# WI-S11-004 — Privacy Notice Versioning Semver + 3 Locales (PT-BR Primary LGPD + EN GDPR/CCPA + ES LATAM) + Diff Publication `/privacy/changelog` + CI Hook Force Re-Consent on Major Bump (CTRL-PRIV-CONSENT-005) + Native Speaker + Legal Local Review (`legal/privacy-notice/v<M.m>.md`; semver versioning; major bump = material change → force re-consent via WI-S11-003 stale_consent flag; minor bump = clarification/typo/contact update sem re-consent; diff diário publicado em `/privacy/changelog` rendered via static HTML; CI hook valida (a) semver bump em qualquer mudança; (b) 3 locales sync (PT-BR/EN/ES) — major bump em primary triggers re-translation obrigatória; (c) Legal Review evidence EVT-044 pre-merge; CTRL-PRIV-CONSENT-005 + P1.1 SOC 2 + GDPR Art. 13/14 + LGPD Art. 9 alignment; emit fail-CLOSED audit `dev.hugr.corelink.privacy_notice.{published,deprecated}.v1` 2 CloudEvents canonical; notice_text_hash SHA-256 deterministic per locale × version inputs WI-S11-003 ConsentProofPayload validation)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-11](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S11-004 |
| Título | Privacy notice versioning + 3 locales canonical (PT-BR LGPD primary, EN GDPR/CCPA, ES LATAM) per privacy_model.md §6.1 + sprint contract §5.4 R-S11-11; semver `v<M.m>.md`; major bump force re-consent via WI-S11-003 stale_consent flag; minor bump silent; diff diário em `/privacy/changelog`; CI hook valida sync 3 locales + Legal Review EVT-044 pre-merge; CTRL-PRIV-CONSENT-005 alignment; 2 CloudEvents canonical `dev.hugr.corelink.privacy_notice.{published,deprecated}.v1` per Lote 10.9bis P0-G prefix |
| Sprint | S-11 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-003 (PII regulatory), FF-HR-005 (CTRL-PRIV-CONSENT-005), FF-HR-010 (1ª regulatory full impl) |

## 1. Intent

Privacy notice é o **regulatory consent gateway** — sem notice publicado, atualizado, e em locale do titular, **GDPR Art. 13/14** + **LGPD Art. 9** ficam unfulfilled. Notice precisa ser (a) versionado semver para detectar material changes; (b) traduzido em 3 locales canonical (PT-BR LGPD primary porque cliente-âncora é HuGR Brasil; EN GDPR/CCPA secondary mercados US/EU; ES LATAM tertiary expansion); (c) auditável via diff publication para regulator inspection; (d) integrated com WI-S11-003 consent ledger via notice_text_hash deterministic (mesmo input → mesmo hash → cross-DB validation).

CTRL-PRIV-CONSENT-005 alignment: **major bump = material change** (novo data category, novo sub-processor, novo purpose) → force re-consent dos affected subjects (WI-S11-003 nightly stale_consent_check job). **Minor bump = clarification/typo/contact update** → no re-consent. Distinção semver é **legal definition of "material" per GDPR EDPB Guidelines + LGPD ANPD guidance**.

```rust
// File: scripts/build_privacy_notice.py (Python — não Rust; static site builder + CI orchestrator)

# Notice canonical structure:
#   legal/privacy-notice/
#   ├─ v1.0.0/
#   │  ├─ pt-BR.md              # primary LGPD source
#   │  ├─ en-US.md              # GDPR/CCPA
#   │  ├─ es-MX.md              # LATAM expansion
#   │  └─ metadata.yaml         # version + changelog + diff_summary + legal_review_evt-044_path
#   ├─ v1.1.0/                  # minor bump example
#   │  └─ ...
#   ├─ v2.0.0/                  # major bump example
#   │  └─ ...
#   └─ changelog/
#      ├─ daily-diffs/<date>.html  # rendered via mistune + diff library
#      └─ index.html               # latest + history

# CI script invariants enforced:
#   1. Semver bump valid (parsed via semver library)
#   2. 3 locales sync (PT-BR ⇒ EN translated ⇒ ES translated; native speaker review checkmark per locale)
#   3. Legal review EVT-044 path em metadata.yaml exists em R2 evidence-legal/ (size > 0; signed PDF)
#   4. notice_text_hash deterministic: SHA-256 do conteúdo MD após canonicalization (CRLF → LF; trim trailing whitespace; UTF-8 NFC normalization)
#   5. Major bump triggers WI-S11-003 stale_consent_check flag in CD pipeline
#   6. Diff diário published em /privacy/changelog static page (Cloudflare Pages)

# Outputs:
#   - /privacy/v<M.m>/<locale>.html (rendered notice)
#   - /privacy/changelog/<date>.html (daily diff)
#   - notice_text_hash registered em R2 audit-`<region>` via 2 CloudEvents:
#     `dev.hugr.corelink.privacy_notice.published.v1` (new version)
#     `dev.hugr.corelink.privacy_notice.deprecated.v1` (old version superseded)
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + customer cost protection + service protection discipline justification)

### 2.1 Contexto

GDPR Art. 13/14 + LGPD Art. 9 exigem privacy notice publicado em locale do titular, descrevendo (a) categorias de dado coletadas; (b) finalidades; (c) basis legal; (d) sub-processadores; (e) direitos do titular; (f) contato DPO. EDPB Guidelines (WP29 Guidelines on Transparency WP260rev01 endorsed by EDPB) + ANPD Res. CD/ANPD nº 15/2024 (incident communication; substitui Res. 2/2022 que tratou de fiscalização) + Privacy by Design (LGPD Art. 6 §X) esclarecem: **mudança material exige re-consent** dos titulares afetados. Distinguir "material" de "clarification" é judgment call legal — solution: **semver convention** que codifica intenção do publisher.

CoreLink target customers: BR (cliente-âncora HuGR; mercado primário sam region) + US/EU (GDPR/CCPA secondary; regiões enam/weur) + LATAM (tertiary; sam region overlap). Native speaker review obrigatório por locale (typo em legal text = defensibility issue; "I agree" tradução literal pode ter implicação legal diferente em PT vs EN). Legal local review obrigatório (e.g., advogado brasileiro revisa PT-BR; advogado UE revisa EN; advogado MX revisa ES).

### 2.2 Abordagem

Notice em git: `legal/privacy-notice/v<M.m>.md` 3 locales per version (PT-BR primary, EN translated, ES translated). Metadata YAML carrega changelog + Legal Review EVT-044 evidence path + native speaker review checkbox per locale. CI hook valida (a) semver bump em qualquer mudança; (b) 3 locales sync; (c) Legal Review presente; (d) notice_text_hash deterministic generation. Major bump CD step triggers WI-S11-003 stale_consent_check job (sets stale_consent=true para affected subjects; UI prompts re-consent within 30d grace + 60d cap = 90d total).

Diff diário publicado em `/privacy/changelog/<date>.html` (Cloudflare Pages static site). 2 CloudEvents canonical em audit-`<region>` Object Lock 7y registram published + deprecated states. notice_text_hash SHA-256 deterministic (CRLF → LF; trim trailing; UTF-8 NFC) é input para WI-S11-003 ConsentProofPayload — cross-validation regulator: re-compute hash do notice em git → match com consent_ledger.notice_text_hash.

### 2.3 Valor entregue

- **GDPR Art. 13/14 + LGPD Art. 9 alignment absoluto**: notice publicado + traduzido + versionado.
- **CTRL-PRIV-CONSENT-005 satisfação**: notice versioning + retranslation enforcement + force re-consent on major bump.
- **WI-S11-003 integration**: notice_text_hash deterministic generation = consent ledger 6-field proof input.
- **Regulator audit-grade**: diff diário público + Legal Review evidence + native speaker review per locale.
- **3 locales mandatório at GA** (sprint contract §6 DoD + §10.s11.7): PT-BR/EN/ES native speaker reviewed.

### 2.4 Principais riscos & trade-offs

- **Major vs minor judgment call**: e.g., "we now use Stripe v2 API instead of v1" — material? Criteria documentated em ADR-S11-007: novo data category OR novo sub-processor OR novo purpose ⇒ major; clarity update OR typo OR contact update ⇒ minor. Borderline cases → Privacy Officer sign-off (default minor unless certain).
- **3 locales force re-consent friction**: major bump aciona re-consent em 100% dos titulares — UX hit. **Mitigation**: 30d grace + 60d cap = 90d total; após = legitimate_interest fallback ou cascade unsubscribe (Privacy Officer decision per LIA EVT-046).
- **Native speaker review delay**: legal local review pode adicionar 7-14d ao timeline. **Mitigation**: pre-stage 3 locales review em parallel; build pipeline blocks merge até all 3 verified.
- **notice_text_hash determinism cross-platform**: line endings (CRLF Windows vs LF Linux) + Unicode normalization (NFC vs NFD) podem causar hash drift. **Mitigation**: canonical normalization step em CI (Python + Rust integration test fixture verifies same hash em both implementations).

## 3. Customer Impact & Journey

### 3.1 Personas afetadas

- **Privacy-aware data subject**: lê notice em locale de tenant.primary_region; receives email "notice atualizada" em major bump.
- **Tenant admin**: configura tenant.locale_default per business preference (override de tenant.primary_region default).
- **Privacy Officer interno HuGR**: revisa changelog mensalmente; aciona Legal Review per major bump.
- **External auditor (ANPD, Irish DPC, SOC 2)**: verifica diff publication + Legal Review trail + version semver discipline.

### 3.2 Customer journey (touchpoints)

1. Privacy Officer drafts new notice version em git branch `notice/v2.0.0`.
2. Native speaker review per locale (PT-BR/EN/ES) → metadata.yaml checkbox per locale.
3. Legal local review per locale (advogado per market) → EVT-044 PDF em R2 evidence-legal/.
4. PR merge → CI hook valida semver + 3 locales sync + Legal Review presence + notice_text_hash deterministic.
5. CD pipeline: deploy notice em `/privacy/v2.0.0/<locale>.html` Cloudflare Pages.
6. Major bump: CD triggers WI-S11-003 stale_consent_check job → titulares affected receive email "Privacy notice updated; please re-consent within 30d grace".
7. Diff publicado em `/privacy/changelog/<date>.html`.
8. 2 CloudEvents emitidos: `privacy_notice.published.v1` + (se major bump) `privacy_notice.deprecated.v1` para versão anterior.

### 3.3 Jornadas (User Journeys) afetadas

- **Signup** (S-13): consent UI carrega current notice em tenant.primary_region locale.
- **Email transactional**: footer link `/privacy/v<latest>/<locale>.html` per email enviado.
- **Settings**: link "Privacy notice" em current version + history.

### 3.4 Métricas de customer-visible

- **Notice version drift detection**: `corelink_privacy_notice_version_published_total{version,locale}` Prom counter.
- **Re-consent completion rate**: ≥ 80% within 30d grace (sustained 90d).
- **Diff publication freshness**: ≤ 24h post-merge p99.
- **Legal Review SLA**: ≤ 14d native speaker + Legal local per locale (HIGH_RISK lane buffer).

### 3.5 Comunicação ao customer

Email transactional em locale do titular: (1) notice update notification (major bump only); (2) re-consent prompt (major bump within 30d grace); (3) deprecation notice (90d post-major bump se titular não re-consent → fallback flow). Templates em 3 locales mandatory.

### 3.6 Mitigação de fricção

- **30d grace + 60d cap**: titular tem tempo para re-consent sem service interruption.
- **Diff publication transparency**: titular pode review changelog antes de re-consenting (informed decision GDPR Art. 7 alignment).
- **Locale tenant override**: tenant pode forçar locale específico (e.g., enterprise BR usa EN para legal team Anglo).

### Anti-pattern ❌

❌ Notice em 1 locale only (LGPD Art. 9 violation); ❌ Major bump silent (CTRL-PRIV-CONSENT-005 violation); ❌ Diff publication interno only (regulator transparency gap); ❌ notice_text_hash não-deterministic (cross-validation regulator falha); ❌ Native speaker review skipped (defensibility weak); ❌ Legal Review post-merge (compliance gap).

## 4. Capability Mapping / Trace

| Camada | ID | Item |
|---|---|---|
| Sprint contract | R-S11-10 + R-S11-11 | Privacy notice semver + 3 locales |
| CAPs | CAP-PRIV-004 | Privacy notice publication + versioning |
| Invariantes | INV-CONSENT-PROOF-VERIFIABLE (CRITICAL — Lote 10.11.0-bis §3.12 L168; foundation — notice_text_hash input) + INV-AUDIT-APPEND-ONLY (CRITICAL §3.6 L116) |
| Controles | CTRL-PRIV-CONSENT-005 (notice versioning + retranslation enforcement) + CTRL-PRIV-022 (DSR self-service surface — links from notice) |
| Padrões | PAT-DRIFT-DETECTION-001 (CI hook detects locale sync drift) |
| Failure modes | **FM-453 (declarado em WI-S11-005)** sub-processor change broadcast miss correlated com notice update |
| Métricas | `corelink_privacy_notice_version_published_total{version,locale}` + `corelink_privacy_notice_diff_publication_lag_seconds` + `corelink_privacy_notice_re_consent_completion_total{version,locale,outcome}` |
| Eventos | 2 CloudEvents canonical: privacy_notice.published/deprecated |
| Evidence | EVT-044 (LEGAL_REVIEW per locale 7y) + EVT-043 (DATA_CLASSIFICATION_DOC; notice_version field) + EVT-001 (CI logs) |

## 5. Tipo e Classificação

### 5.1 Tipo primário

`feature/regulatory-baseline` — implementação primária privacy notice publication.

### 5.2 Prioridade

**P0** — sprint contract §6 DoD bloqueante (3 locales mandatory at GA).

### 5.3 Blast radius

**Cross-tenant + cross-region** (notice é global; locale é per-titular). Bug em translation pode causar cross-locale defensibility issue.

### 5.4 Reversibilidade

**Reversível** via git revert + republish; mas re-consent already triggered não pode ser undone (titulares já notificados; cascade unsubscribe pode ter executado). Decisão documented em DPA: notice versioning é monotonic (no down-versioning).

### 5.5 Experiment? (A/B test, feature flag experiment)

Não — regulatory feature, sem A/B (multi-arm = compliance risk per privacy_model.md anti-pattern).

### 5.6 Compliance triggers

- LGPD Art. 9 (informações ao titular).
- GDPR Art. 13 (information to be provided where personal data are collected from the data subject), Art. 14 (where data not from the data subject).
- CCPA/CPRA §1798.100(b) (privacy policy).
- SOC 2 P1.1 (Privacy notice).
- ISO/IEC 27701 §6.10.2.1 (Identifying lawful basis).

## 6. Escopo

### 6.1 Em escopo (exaustivo)

1. Diretório `legal/privacy-notice/` com 1 inicial version `v1.0.0/` contendo 3 locales markdown (PT-BR/EN/ES) + metadata.yaml.
2. Initial notice content (~3000 words per locale) cobrindo: data categories, purposes, basis legal, sub-processors, DSR rights, DPO contact.
3. Native speaker review process documentation em `legal/privacy-notice/REVIEW_PROCESS.md`.
4. Legal local review process: advogado BR (PT-BR), UE (EN), MX (ES) — contracts to be signed pre-GA (anti-scope: actual contracts; this WI documents process).
5. CI hook script `scripts/validate_privacy_notice.py`:
   - (a) Valida semver bump em qualquer mudança (parse via `semver` library);
   - (b) Verifica 3 locales sync (PT-BR ⇒ EN ⇒ ES; commits relevantes em todos 3);
   - (c) Verifica Legal Review evidence file em metadata.yaml exists em R2 evidence-legal/;
   - (d) Verifica notice_text_hash deterministic (CRLF→LF + trim + UTF-8 NFC + SHA-256);
   - (e) Major bump → flag em CD pipeline para trigger WI-S11-003 stale_consent_check.
6. Diff publication script `scripts/publish_privacy_diff.py`: gera HTML diff via `python-diff` library; deploy em Cloudflare Pages `/privacy/changelog/<date>.html`.
7. 2 CloudEvents emitter: `privacy_notice.published.v1` + `privacy_notice.deprecated.v1` em audit-`<region>` Object Lock 7y; emit fail-CLOSED.
8. ADR-S11-007 (NEW): Material vs minor change criteria (data category / sub-processor / purpose vs clarity / typo / contact update).
9. Email transactional templates em 3 locales (PT-BR/EN/ES) para: (a) notice update notification major bump; (b) re-consent prompt 30d grace; (c) deprecation 90d post-major bump.
10. WI-S11-003 integration: stale_consent_check job consume `privacy_notice.deprecated.v1` event → flag affected consents.
11. Static site builder configuration: Cloudflare Pages `/privacy/v<M.m>/<locale>.html` rendering via `mistune` markdown library.

#### 6.1.5 ADR-S11-007 material vs minor criteria (excerpt)

| Change type | Bump | Examples |
|---|---|---|
| Novo data category coletada | Major | "Adicionamos coleta de geolocation IP via consent" |
| Novo sub-processor adicionado | Major | "Adicionamos Sentry para error tracking" |
| Novo purpose para coletar/processar | Major | "Usaremos analytics data para AI training" |
| Mudança em retenção (period extension) | Major | "Audit log retenção 7y → 10y" |
| Mudança em DSR SLA (extension) | Major | "Erasure SLA 30d → 45d" |
| Cross-border transfer change (e.g., new region) | Major | "Adicionamos enam region; data pode trafegar EU→US sob SCC" |
| Clarification de existing wording | Minor | "Esclarecemos que 'analytics' inclui error tracking" |
| Typo / grammar fix | Minor | "Corrigimos 'colectamos' → 'coletamos' PT-BR" |
| Contact update (DPO email change) | Minor | "DPO email dpo@hugr.dev → privacy@hugr.dev" |
| Add additional locale (não primary) | Minor | "Adicionamos PT-PT (não substitui PT-BR primary)" |
| Borderline | Privacy Officer judgment + ADR | Default minor unless certain (defensible posture) |

### 6.2 Componentes C4 afetados

- **Cloudflare Pages** (`/privacy/v<M.m>/`, `/privacy/changelog/`).
- **R2 audit-`<region>`**: 2 canonical CloudEvents types Object Lock 7y.
- **R2 evidence-legal/**: Legal Review PDFs per locale.
- **Cloudflare Email** (S-13 inheritance): 3 templates per locale × 3 notification types = 9 templates total.
- **CI/CD pipeline**: validate_privacy_notice.py + publish_privacy_diff.py invocations.
- **Worker WI-S11-003 consumer** (consent ledger): consome stale_consent_check trigger.

### 6.3 Arquivos do repositório

```
legal/privacy-notice/
├─ v1.0.0/
│  ├─ pt-BR.md                                # primary LGPD ~3000 words
│  ├─ en-US.md                                # GDPR/CCPA translation
│  ├─ es-MX.md                                # LATAM translation
│  └─ metadata.yaml                            # changelog, legal_review_evt-044_path, native_speaker_review per locale
├─ REVIEW_PROCESS.md                           # native speaker + Legal local review SOP
├─ TEMPLATES/                                   # email templates 3 locales × 3 types = 9 .mjml files
└─ changelog/                                   # auto-gen daily diffs

scripts/
├─ validate_privacy_notice.py                  # CI hook (a-d)
├─ publish_privacy_diff.py                     # daily diff page generator
└─ notice_text_hash_canonical.py                # CRLF→LF + trim + NFC + SHA-256 helper

specs/03_architecture/adrs/
└─ ADR-S11-007-privacy-notice-material-vs-minor.md  # NEW

crates/corelink-privacy-notice-emit/           # NEW small crate
├─ Cargo.toml
└─ src/
   └─ lib.rs                                   # 2 CloudEvents emitter (published + deprecated)

specs/05_quality/runbooks/RB-PRIVACY-NOTICE-LATE-PUBLICATION.md  # NEW (CTRL-PRIV-CONSENT-005 SLA miss)
```

### 6.4 Sistemas externos tocados

- **Cloudflare Pages** (static site rendering).
- **R2** (audit + evidence-legal + Pages assets).
- **Cloudflare Email** (transactional 9 templates).
- **CI/CD** (GitHub Actions + Cloudflare Wrangler).

## 7. Anti-Scope

- ❌ DSR API endpoints — entregue em WI-S11-001.
- ❌ Erasure worker — entregue em WI-S11-002.
- ❌ Consent ledger D1 schema — entregue em WI-S11-003.
- ❌ Sub-processor register — entregue em WI-S11-005 (referenced in notice content).
- ❌ Breach notification runbook — entregue em WI-S11-006 (referenced in notice).
- ❌ Residency pinning — entregue em WI-S11-007 (referenced in notice).
- ❌ DPIA + LIA + TLA+ — entregue em WI-S11-008.
- ❌ Notice content authoring (~3000 words × 3 locales) — DELIVERED em WI but content é Privacy Officer + Legal responsibility, not engineering.
- ❌ A/B test framework de wording — anti-scope total per privacy_model.md.
- ❌ HTML/CSS design custom — using Cloudflare Pages default theme.
- ❌ PT-PT, FR, DE locales — pós-GA (anti-scope explícito sprint contract §10).

### Anti-pattern ❌

❌ Notice em 1 locale only; ❌ Major bump silent; ❌ notice_text_hash não-deterministic; ❌ Native speaker review skipped; ❌ Legal Review post-merge; ❌ Down-versioning notice (regulatory rollback gap); ❌ HTML rendering com user input (XSS risk).

## 8. Acceptance Criteria (Gherkin) — 8 scenarios

### AC-001: Initial notice publication v1.0.0 happy path

```gherkin
Given diretório legal/privacy-notice/v1.0.0/ contém pt-BR.md, en-US.md, es-MX.md, metadata.yaml
And metadata.yaml tem legal_review_evt-044_path apontando para R2 evidence-legal/v1.0.0-{pt-BR,en-US,es-MX}.pdf
And native_speaker_review checkbox = true para 3 locales
When PR merge é executed
Then CI hook valida (a) semver = "1.0.0" (initial); (b) 3 locales presentes; (c) 3 PDFs Legal Review existem em R2 com size > 0; (d) notice_text_hash deterministic gerado per locale
And CD pipeline deploy /privacy/v1.0.0/{pt-BR,en-US,es-MX}.html em Cloudflare Pages
And `dev.hugr.corelink.privacy_notice.published.v1` emitido em audit-`<region>` com payload {version: "1.0.0", locales: [...], notice_text_hashes: {...}}
And NO `privacy_notice.deprecated.v1` (initial — nada a deprecar)
```

### AC-002: Major bump force re-consent flow

```gherkin
Given current published notice é v1.5.0 com 1000 titulares granted consents
When PR merge para v2.0.0/ contém adicionado novo sub-processor (Sentry) per ADR-S11-007 material criteria
Then CI hook detecta semver major bump (1→2)
And CD pipeline triggers WI-S11-003 stale_consent_check job
And consent_ledger 1000 rows são flagged stale_consent=true (NÃO deleted; audit preserved)
And email enviado em locale do tenant: "Privacy notice updated to v2.0.0; please re-consent within 30d"
And `dev.hugr.corelink.privacy_notice.published.v1` (new) + `privacy_notice.deprecated.v1` (v1.5.0) emitidos
And `corelink_privacy_notice_re_consent_pending_total{version='v2.0.0',locale}` Prom gauge populated
```

### AC-003: Minor bump silent (no re-consent)

```gherkin
Given current published notice é v1.5.0
When PR merge para v1.6.0/ contém apenas typo fix em pt-BR.md ("colectamos" → "coletamos") per ADR-S11-007 minor criteria
Then CI hook detecta semver minor bump (5→6)
And CD pipeline NÃO triggers stale_consent_check
And consent_ledger rows NÃO modified (no stale_consent flag)
And `privacy_notice.published.v1` emitido com payload.material_change=false
And NO email enviado (silent update)
And diff publicado em /privacy/changelog/<date>.html mostrando typo fix
```

### AC-004: 3 locales sync enforcement

```gherkin
Given PR atualiza apenas pt-BR.md em v2.0.0/ (esquece en-US.md + es-MX.md)
When CI hook valida sync 3 locales
Then validation falha com error: "3 locales sync violation: pt-BR.md modified but en-US.md + es-MX.md unchanged"
And PR merge é blocked
And erro ensina: "Per CTRL-PRIV-CONSENT-005, all 3 locales must be sync; major bump requires re-translation by native speaker + Legal local review"
```

### AC-005: Native speaker review checkbox enforcement

```gherkin
Given metadata.yaml tem native_speaker_review.es-MX = false
When CI hook valida native speaker review
Then validation falha com error: "es-MX native speaker review checkbox not checked"
And PR merge é blocked
And erro ensina: "Per WI-S11-004 §6.1.4, all 3 locales require native speaker review pre-merge; locale es-MX missing"
```

### AC-006: notice_text_hash deterministic cross-platform

```gherkin
Given pt-BR.md content em git com mixed line endings (CRLF + LF)
When notice_text_hash_canonical.py computa hash
Then steps: (1) CRLF → LF normalize; (2) trim trailing whitespace per line; (3) UTF-8 NFC normalize; (4) SHA-256 hex64 hash
And resultado deterministic across platforms (Windows + Linux + macOS) verifiable em CI
And property test: 100 random content samples × 3 platforms → 100% hash parity
And cross-validation regulator: re-compute hash do notice em git → match com consent_ledger.notice_text_hash em D1
```

### AC-007: Diff publication daily

```gherkin
Given v2.0.0 publicado ontem em ts T
When daily cron worker (S-09 inheritance) dispara `publish_privacy_diff.py`
Then diff HTML gerado entre v1.5.0 (previous) → v2.0.0 (current)
And page deployed em /privacy/changelog/<T+1>.html via Cloudflare Pages
And changelog index page atualizada com latest entry
And p99 publication lag (PR merge → diff page live) ≤ 24h via `corelink_privacy_notice_diff_publication_lag_seconds` histogram
```

### AC-008: Audit fail-CLOSED em emit failure

```gherkin
Given audit emit infrastructure (R2 audit-`<region>`) está temporariamente indisponível
When CD pipeline tenta deploy v2.0.0
Then `privacy_notice.published.v1` emit fails
And CD pipeline aborts deploy (transaction-like behavior; Cloudflare Pages NÃO atualizado)
And SEV-2 alert disparado
And rollback-safe: PR merge fica em "deploy_pending" status; manual intervention required pos-audit recovery
And distinct from WI-S10-001 billing fail-OPEN (split-tier discipline)
```

## 9. Design Decisions

### 9.1 Decisões locais

- **DD-001 Semver convention vs date-based versioning**: semver mais expressive (major = material; minor = clarity); date-based é simpler mas perde semantics. Decisão: semver per CTRL-PRIV-CONSENT-005 alignment.
- **DD-002 3 locales (não 4 ou 5)**: PT-BR/EN/ES cobrem 90% target market early-stage; PT-PT, FR, DE adicionados pós-GA conforme demand. Sprint contract §10 anti-scope explícito.
- **DD-003 Static site Cloudflare Pages vs dynamic Workers**: static é cacheable + zero-runtime cost; dynamic vantagem é per-tenant customization (e.g., enterprise BR usa EN). Decisão: static (regulatory page; tenant override via UI redirect S-13).
- **DD-004 Markdown source vs DOCX/PDF**: markdown enables git diff + auto-rendering; DOCX/PDF tradicional legal mas zero diff capability. Decisão: markdown per CTRL-PRIV-CONSENT-005 diff publication requirement.
- **DD-005 Native speaker + Legal local review serial vs parallel**: parallel reduces timeline 7-14d → 5-7d. Decisão: parallel; CI gate enforces both completed pre-merge.

### 9.2 Decisões que justificam ADR

- **ADR-S11-007 (NEW)**: Material vs minor change criteria (table §6.1.5). Rationale: regulatory legal definition + CTRL-PRIV-CONSENT-005 enforcement consistency. Alternative (Privacy Officer ad-hoc judgment) considered + rejected (consistency risk).

### 9.3 Trade-offs explícitos

| Trade-off | Opção A | Opção B | Decisão | Rationale |
|---|---|---|---|---|
| Versioning convention | Semver | Date-based | A | CTRL-PRIV-CONSENT-005 semantics |
| Locales count | 3 (PT-BR/EN/ES) | 6+ (incl. PT-PT/FR/DE) | A | Market coverage early-stage; anti-scope rest |
| Static vs dynamic | Cloudflare Pages | Workers dynamic | A | Cacheable + zero cost regulatory page |
| Source format | Markdown | DOCX/PDF | A | git diff + auto-rendering |
| Review process | Serial | Parallel | B | Timeline reduction 7-14d → 5-7d |
| Major bump grace | 30d | 60d | 30d + 60d cap | UX balance + CTRL-PRIV-CONSENT-005 SLA |

### Anti-pattern ❌

❌ Date-based versioning (semantics gap); ❌ 1 locale only; ❌ Dynamic per-tenant rendering (caching loss + complexity); ❌ DOCX source (diff impossible); ❌ Serial review (timeline 14d+); ❌ No grace period (UX violation).

## 10. Completeness Criteria SOTA

### 10.1 Code Completeness

- [ ] **C-1.1** `legal/privacy-notice/v1.0.0/` 3 locales markdown + metadata.yaml committed.
- [ ] **C-1.2** `scripts/validate_privacy_notice.py` CI hook implementado (5 validations a-e).
- [ ] **C-1.3** `scripts/publish_privacy_diff.py` daily diff publisher.
- [ ] **C-1.4** `scripts/notice_text_hash_canonical.py` deterministic helper (Python + Rust integration test parity).
- [ ] **C-1.5** Crate `corelink-privacy-notice-emit` (NEW; 2 CloudEvents emitter) compila WASM.
- [ ] **C-1.6** Cloudflare Pages config + GitHub Actions CD workflow.
- [ ] **C-1.7** ADR-S11-007 material vs minor criteria committed.

### 10.2 Test Completeness

- [ ] **T-2.1** Integration test full lifecycle (PR notice update → CI validate → CD deploy → audit emit → diff publish) verde.
- [ ] **T-2.2** Property test notice_text_hash determinism: 100 content samples × 3 platforms → 100% parity.
- [ ] **T-2.3** Regression test 3 locales sync enforcement (single-locale PR blocked).
- [ ] **T-2.4** Regression test native speaker review checkbox enforcement.
- [ ] **T-2.5** Regression test major bump force re-consent flow (WI-S11-003 integration).
- [ ] **T-2.6** Regression test minor bump silent (no re-consent triggered).
- [ ] **T-2.7** Chaos test audit emit failure → CD pipeline aborts.
- [ ] **T-2.8** Cross-locale defensibility test: re-compute hash from git → match D1 consent_ledger.

### 10.3 Documentation Completeness

- [ ] **D-3.1** `legal/privacy-notice/REVIEW_PROCESS.md` SOP native speaker + Legal local.
- [ ] **D-3.2** ADR-S11-007 material vs minor criteria.
- [ ] **D-3.3** `docs/dev/privacy-notice-publishing.md` CI/CD workflow.
- [ ] **D-3.4** `specs/05_quality/runbooks/RB-PRIVACY-NOTICE-LATE-PUBLICATION.md` (NEW canonical).

### 10.4 Observability Completeness

- [ ] **O-4.1** 3 Prom metrics: version_published_total, diff_publication_lag_seconds, re_consent_pending_total.
- [ ] **O-4.2** 1 dashboard `corelink-privacy-notice` em Grafana com 4 panels.
- [ ] **O-4.3** 2 CloudEvents canonical types em audit-`<region>` Object Lock 7y.

### 10.5 Security & Privacy Completeness

- [ ] **S-5.1** notice_text_hash deterministic (CRLF + NFC + trim + SHA-256).
- [ ] **S-5.2** Legal Review PDFs em R2 evidence-legal/ retain 7y.
- [ ] **S-5.3** Cloudflare Pages default content security policy (no XSS).
- [ ] **S-5.4** GDPR Art. 13/14 + LGPD Art. 9 alignment validated em Legal Review pre-merge.

### 10.6 SBOM Completeness

- [ ] **B-6.1** SBOM CycloneDX 1.5+ inclui `corelink-privacy-notice-emit` crate + Python script deps.

## 11. DoD

10.x checked + sign-off matrix §30 + 3 locales Legal Review evidence presente + chaos test audit fail-CLOSED verde.

## 12. Invariants Validated

| INV | Severity | Position canonical | Cobertura WI-S11-004 |
|---|---|---|---|
| **INV-CONSENT-PROOF-VERIFIABLE** (foundation) | CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL com TLA+ symmetry) | invariant_registry.md §3.12 L168 | notice_text_hash deterministic generation = consent ledger 6-field input; cross-validation regulator verifiable |
| **INV-AUDIT-APPEND-ONLY** | CRITICAL | invariant_registry.md §3.6 L116 | 2 CloudEvents canonical em audit-`<region>` Object Lock 7y; emit fail-CLOSED CD pipeline abort |

## 13. Artifacts Produced

- `legal/privacy-notice/v1.0.0/` (3 locales × ~3000 words + metadata).
- `scripts/validate_privacy_notice.py` + `publish_privacy_diff.py` + `notice_text_hash_canonical.py`.
- `crates/corelink-privacy-notice-emit/` (NEW; ~500 LoC).
- `legal/privacy-notice/REVIEW_PROCESS.md` SOP.
- `docs/dev/privacy-notice-publishing.md` CI/CD workflow.
- ADR-S11-007 material vs minor criteria.
- 2 CloudEvents schemas em `schemas/cloudevents/privacy-notice-{published,deprecated}.v1.json`.
- 9 email templates (3 locales × 3 notification types) em `legal/privacy-notice/TEMPLATES/`.
- `specs/05_quality/runbooks/RB-PRIVACY-NOTICE-LATE-PUBLICATION.md`.
- Grafana dashboard JSON.
- 3 Prom metrics + alerts.

## 14. Quality Standards SOTA

- **14.s11.4.1** Native speaker review per locale + Legal local review per locale (CI gate).
- **14.s11.4.2** notice_text_hash deterministic 100% cross-platform (property test).
- **14.s11.4.3** Major bump force re-consent flow integrated com WI-S11-003.
- **14.s11.4.4** Diff publication ≤ 24h p99 sustained.
- **14.s11.4.5** 3 locales sync enforcement CI gate (sprint contract §6 DoD + §10.s11.7).
- **14.s11.4.6** Legal Review evidence EVT-044 retention 7y per locale.
- **14.s11.4.7** ADR-S11-007 material vs minor criteria documented + Privacy Officer sign-off.
- **14.s11.4.8** INV §3.X positions canonical verified pre-merge (Lote 10.8bis P1-13).

## 15. Chaos Experiments (6)

1. R2 audit unavailable → CD pipeline aborts deploy (fail-CLOSED).
2. Cloudflare Pages deploy timeout → SEV-2 alert; manual reissue.
3. Native speaker review fake checkbox (CI parsing bypass) → integration test detects + blocks.
4. notice_text_hash drift cross-platform (Windows CRLF) → property test detects + fixes.
5. Major bump CD job triggers stale_consent_check em wrong tenant scope → property test 100k tenant pairs verifies isolation.
6. Email template missing locale → fallback to en-US default + SEV-3 alert.

## 16. PRR

PRR HIGH_RISK 12 sign-offs + chaos 30d + Privacy + Legal + Compliance mandatory emphatic.

## 17. Sub-tasks

| ID | Descrição | PERT |
|---|---|---|
| ST-001 | Initial notice content authoring 3 locales (Privacy Officer + Legal lead; eng support) | 4h |
| ST-002 | metadata.yaml schema + initial v1.0.0 commit | 1h |
| ST-003 | validate_privacy_notice.py CI hook | 2h |
| ST-004 | notice_text_hash_canonical.py + Python+Rust parity test | 1.5h |
| ST-005 | publish_privacy_diff.py daily worker | 1.5h |
| ST-006 | corelink-privacy-notice-emit crate (2 CloudEvents) | 1h |
| ST-007 | Cloudflare Pages config + GitHub Actions CD | 1.5h |
| ST-008 | ADR-S11-007 material vs minor criteria | 0.5h |
| ST-009 | REVIEW_PROCESS.md + RB-PRIVACY-NOTICE-LATE-PUBLICATION SOP | 0.8h |
| ST-010 | 9 email templates (3 locales × 3 types) | 0.5h |

**PERT total**: ~14.3h (alinha com sprint contract §12).

## 18. Dependencies

- **Hard**: spec contract S-11 v1.2.0 SEALED; S-09 SEALED (audit emit + Cloudflare cron); WI-S11-003 (consent ledger consume stale_consent_check trigger).
- **Soft**: WI-S11-005 (sub-processor register content referenced em notice); WI-S11-007 (residency E2E content referenced em notice).
- **External**: Privacy Officer + Legal local advogados (BR/UE/MX) — staffing pre-GA.

## 19. Effort PERT: ~14.3h. ## 20. Time-boxing: 20h hard limit (lane HIGH_RISK +40% buffer).

## 21. Observability

3 Prom metrics + 1 Grafana dashboard 4 panels + 2 CloudEvents canonical.

## 22. Cost Analysis

- **Cloudflare Pages**: free tier covers static site; ≈$0/month.
- **R2 evidence-legal/ Legal Review PDFs**: ~5MB/PDF × 3 locales × ~2 versions/year × 7y = ~210MB; ≈$0.005/year.
- **R2 audit-`<region>`**: 2 events/version; 7y Object Lock; ≈$0.001/version.
- **Cloudflare Email transactional**: 9 templates × N titulares × major bump rate; ≈$0.001/email × 1000 titulares = $1/major bump.
- **CI compute**: validate_privacy_notice.py + publish_diff ≤ 1min; ≈$0.0001/PR.
- **Total estimated**: ≤ $20/year (well under §14 budget).

## 23. API Contract

2 CloudEvents schemas em `schemas/cloudevents/privacy-notice-*.v1.json`. metadata.yaml schema em `schemas/yaml/privacy-notice-metadata.json`. 9 email template schemas em mjml format.

## 24. Post-mortem Hooks

| Trigger | Severity | Owner |
|---|---|---|
| INV-AUDIT-APPEND-ONLY violation (notice publish event NOT em R2 7y) | CRITICAL | Privacy Officer + SecLead |
| Major bump silent (re-consent flow miss) | HIGH | Privacy Officer + Architect |
| 3 locales out of sync (single locale PR merged) | HIGH | Privacy Officer + CI gate review |
| Native speaker review fake checkbox exploit | HIGH | SecLead + Privacy + CI hardening |
| notice_text_hash drift cross-platform | HIGH | Architect + cross-validation regulator concern |
| Diff publication > 24h delay | MEDIUM | SRE + Privacy |
| Material change classified as minor (ADR-S11-007 violation) | HIGH | Privacy Officer + Compliance + Legal escalation |
| Audit fail-OPEN regression (Lote 10.6bis) | CRITICAL | Architect + Privacy |

## 25. Rollback / Recovery

Git revert + republish via CI/CD; mas re-consent already triggered não pode ser undone (titulares já notificados; cascade unsubscribe pode ter executado downstream). Decisão documented em DPA + ADR-S11-007: notice versioning é monotonic (no down-versioning).

## 26. Security & Privacy

LINDDUN per privacy_model.md §4 + STRIDE per security_model.md §6:
- L(inkability): notice é público (no PII concern).
- I(dentifiability): notice content describes data flows; no titular identification.
- N(on-repudiation): R2 Object Lock 7y + signed Legal Review PDFs.
- D(etectability): diff publication transparente.
- D(isclosure): notice é intencionalmente público (transparency).
- U(nawareness): notice IS the awareness mechanism.
- N(on-compliance): **GDPR Art. 13/14 + LGPD Art. 9 + CCPA §1798.100(b) + SOC 2 P1.1 compliance** via versioning + 3 locales + Legal Review + diff publication.

## 27. Knowledge Transfer

Tech talk (45min): "S-11 Privacy Notice Versioning: semver + 3 locales + ADR-S11-007 material/minor + force re-consent integration WI-S11-003"; doc `docs/dev/privacy-notice-publishing.md`; onboarding test 4 questões: ADR-S11-007 material criteria (5 examples), 3 locales rationale (cliente-âncora HuGR BR + GDPR/CCPA + LATAM), notice_text_hash determinism (CRLF→LF + NFC + SHA-256), major bump cascade WI-S11-003 stale_consent_check.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Material change classified as minor (ADR-S11-007 violation) | M | M | HIGH | M | LOW | Privacy Officer dual-approval; CI gate ensures ADR-S11-007 referenced em PR description; quarterly audit |
| R-002 | 3 locales out of sync | M | L | HIGH | M | LOW | CI gate strict enforcement; native speaker review per locale checkbox |
| R-003 | notice_text_hash drift cross-platform | L | L | HIGH | L | LOW | canonical normalization + property test 100 samples × 3 platforms |
| R-004 | Native speaker review fake checkbox | L | M | HIGH | M | LOW | Legal Review PDF presence cross-check; Privacy Officer audit |
| R-005 | Diff publication > 24h delay | M | L | LOW | L | LOW | SLO `corelink_privacy_notice_diff_publication_lag_seconds` p99; SEV-3 alert |
| R-006 | Major bump silent (re-consent miss) | L | M | HIGH | M | LOW | CI hook flag em CD pipeline; integration test verifies stale_consent_check triggered |
| R-007 | Email locale fallback miss (titular receives en-US instead of pt-BR) | L | L | LOW | L | LOW | Fallback chain documented; tenant.locale_default override |
| R-008 | Cloudflare Pages deploy failure | L | L | MEDIUM | L | LOW | CD retry + manual reissue runbook |
| R-009 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | LOW | L | LOW | INV positions verified Lote 10.11.0 |
| R-010 | Audit fail-OPEN regression (Lote 10.6bis violation) | L | L | CRITICAL | L | LOW | ADR-S11-002 cross-WI |
| R-011 | Markdown XSS via user-injected content | L | L | MEDIUM | L | LOW | Cloudflare Pages sanitize; mistune library safe-mode |
| R-012 | Legal local review delay > 14d (timeline blocker) | M | M | LOW | L | LOW | Parallel review + Privacy Officer escalation |

## 29. Review Checkpoints

D+0 design (Architect; semver + 3 locales rationale); D+1 Privacy Officer (LGPD Art. 9 + EDPB material change); D+2 Legal (DPA reference + Legal local advogado contracts); D+3 Compliance (SOC 2 P1.1); D+4 Native speaker review per locale (parallel); D+5 code review; D+6 PRR.

## 30. Sign-off (HIGH_RISK 12)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ |
| 4 | Security Lead | _TBD; **mandatory** — Cloudflare Pages CSP + audit fail-CLOSED_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — property test cross-platform hash determinism_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 P1.1 + GDPR Art. 13/14 + LGPD Art. 9 + ADR-S11-007_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — material vs minor criteria + 3 locales sync_ |
| 11 | Architect | _TBD; **mandatory emphatic** — split-tier audit fail-CLOSED + INV §3.X verification_ |
| 12 | DPO interim | _TBD; **mandatory emphatic** — 3 locales mandatory at GA + Legal Review per locale_ |

(Legal sign-off via DPA reference + Legal local contracts at sprint level.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.1.0 | 2026-04-28 | Gustavo (Lote 10.11.0-bis + 10.11bis) | **Canonical fixes pós baseline review aggregate 5.18/10**: (a) **Locale enum canonical** = pt-BR/en-US/es-MX (data_model.md §4.1 tenant.locale_default CHECK constraint NEW + privacy_model.md §5.6.1 cross-link). (b) **ANPD Res. 2/2022 → Res. CD/ANPD nº 15/2024** (Regulamento sobre comunicação de incidente — substitui Res. 2/2022 que tratou de fiscalização) + **EDPB Guidelines → WP29 Guidelines on Transparency WP260rev01 endorsed by EDPB** disambiguation. (c) **CloudEvents canonical** `dev.hugr.corelink.privacy_notice.{published,translated}.v1` em observability_model.md §7.2 catalog 22-S11 events. |
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.11) | Criação WI-S11-004; HIGH_RISK; SOTA pós-S-10 SEALED. Privacy notice versioning regulatory consent gateway. Diretório `legal/privacy-notice/v<M.m>.md` 3 locales canonical (PT-BR LGPD primary cliente-âncora HuGR BR; EN GDPR/CCPA secondary; ES LATAM tertiary). Semver convention: major bump = material change → force re-consent via WI-S11-003 stale_consent_check; minor bump = clarification/typo/contact update silent. CI hook valida (a) semver bump valid; (b) 3 locales sync; (c) Legal Review EVT-044 evidence presente em R2; (d) notice_text_hash deterministic (CRLF→LF + trim + UTF-8 NFC + SHA-256); (e) major bump CD pipeline flag. ADR-S11-007 material vs minor criteria (5 categories major + 5 categories minor + Privacy Officer judgment borderline). 2 CloudEvents canonical `dev.hugr.corelink.privacy_notice.{published,deprecated}.v1` per Lote 10.9bis P0-G prefix em audit-`<region>` Object Lock 7y. Diff publication daily em /privacy/changelog/<date>.html via Cloudflare Pages. WI-S11-003 integration: notice_text_hash deterministic = consent ledger 6-field proof input cross-validation regulator. Legal Review PDFs per locale em R2 evidence-legal/ retain 7y. Native speaker review per locale (checkbox em metadata.yaml) + Legal local review per locale (advogado BR/UE/MX). 9 email templates (3 locales × 3 notification types: update notification + re-consent prompt + deprecation notice). 8 AC scenarios + 6 chaos + 12 risks + 8 post-mortem hooks. NEW corelink-privacy-notice-emit crate + scripts (validate + publish + hash canonical). NEW RB-PRIVACY-NOTICE-LATE-PUBLICATION runbook. **Lote 10.10 lessons absorbed**: source-of-truth FIRST INV positions; typed enum (locales canonical); sign-off cap 12; cascade discipline; split-tier audit fail-CLOSED; 3 locales mandatory at GA per sprint contract §10.s11.7. |

## 32. Anti-patterns evitados

- ❌ Date-based versioning (semantics gap); ❌ 1 locale only (LGPD Art. 9 violation); ❌ Major bump silent (CTRL-PRIV-CONSENT-005 violation); ❌ notice_text_hash não-deterministic (cross-validation regulator falha); ❌ Native speaker review skipped (defensibility weak); ❌ Legal Review post-merge (compliance gap); ❌ Down-versioning (regulatory rollback gap); ❌ Audit fail-OPEN (Lote 10.6bis split-tier violation); ❌ INV §3.X position TBD (Lote 10.8bis P1-13); ❌ HTML rendering com user input (XSS).

---
