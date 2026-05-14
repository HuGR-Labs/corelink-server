---
id: "WI-S11-005"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.2.0"
created: "2026-04-26"
updated: "2026-05-13"
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
  - "FAILURE-MODES"
tags: ["wi", "s11", "sub-processor", "register", "30d-broadcast", "objection-flow", "ctrl-priv-021", "fm-453", "high-risk"]
---

# WI-S11-005 — Sub-Processor Register Auto-Generated `legal/sub-processors.md` + Public Page `/privacy/sub-processors` + 30d Email Broadcast Cron Worker (CTRL-PRIV-021 + GDPR Art. 28.2 + LGPD Art. 39) + Customer Objection Flow → Legal Escalation Accept-or-Terminate + DKIM Email Authentication + Delivery Confirmation + FM-453 Broadcast Miss Declaration (`legal/sub-processors.md` source of truth com 7 canonical sub-processors compliance_matrix.md §7 inheritance — Cloudflare/Neon/Grafana Cloud/Stripe/GitHub/Sigstore/PagerDuty; auto-gen `/privacy/sub-processors` Cloudflare Pages static; mudança = email broadcast a customers ≥ 30d antes via Cloudflare Email transactional + DKIM signed + delivery confirmation; objection endpoint `POST /v1/privacy/sub-processor-objection` roteia ticket Privacy Officer + Legal review ≤14d → accept or terminate flow GDPR Art. 28.2; 2 CloudEvents canonical `dev.hugr.corelink.sub_processor.{published,changed,objection_filed}.v1` 3 types per Lote 10.9bis P0-G prefix em audit-`<region>` Object Lock 7y; FM-453 broadcast miss P1 S=4 declared em failure_modes.md NEW)

> **doc_status:** SEALED · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-11](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S11-005 |
| Título | Sub-processor register `legal/sub-processors.md` source-of-truth + auto-gen `/privacy/sub-processors` static page (Cloudflare Pages) + 30d email broadcast cron worker (CTRL-PRIV-021 + GDPR Art. 28.2 + LGPD Art. 39) + DKIM signed + delivery confirmation per recipient + objection endpoint POST /v1/privacy/sub-processor-objection → Privacy Officer + Legal escalation accept-or-terminate ≤14d (sprint contract §5.5 R-S11-12 + R-S11-13); 7 canonical sub-processors compliance_matrix.md §7 inheritance; 3 CloudEvents canonical `dev.hugr.corelink.sub_processor.{published,changed,objection_filed}.v1` per Lote 10.9bis P0-G prefix em audit-`<region>` Object Lock 7y; FM-453 (NEW) broadcast miss declaration |
| Sprint | S-11 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-003 (PII regulatory), FF-HR-005 (CTRL-PRIV-021), FF-HR-010 (1ª regulatory full impl) |

## 1. Intent

Sub-processor transparency é **regulatory baseline** — GDPR Art. 28.2 + LGPD Art. 39 exigem (a) lista pública de sub-processors com função/região/certificações; (b) notification ≥ 30d antes de mudança; (c) customer right to objection (accept-or-terminate clause). Empresas que entregam apenas via DPA static linha → falham porque (a) DPA inalterado mascara mudança silent; (b) sem broadcast 30d, customer perdido oportunidade; (c) sem objection flow, GDPR Art. 28.2.b violation.

CoreLink stack: 7 canonical sub-processors hoje (compliance_matrix.md §7 inheritance — Cloudflare, Neon, Grafana Cloud, Stripe, GitHub, Sigstore, PagerDuty). `legal/sub-processors.md` é source-of-truth (git versioned, semver) → auto-gen `/privacy/sub-processors` Cloudflare Pages public page. Mudança em arquivo dispara CD pipeline: 30d email broadcast a all customers via Cloudflare Email transactional com DKIM signed + delivery confirmation per recipient. Objection endpoint roteia ticket Privacy Officer + Legal escalation ≤14d (default minor objections accepted; major requires accept-or-terminate decision per DPA).

```yaml
# legal/sub-processors.md — YAML front-matter + markdown body

---
version: "1.0.0"                          # semver; bump on any change
last_updated: "2026-04-26"
notification_required: true               # if true, 30d broadcast triggered on next CD
sub_processors:
  - id: "cloudflare"
    name: "Cloudflare, Inc."
    role: "Infrastructure provider (Workers, R2, KV, DO, D1)"
    data_categories_processed: ["account_metadata", "blob_content", "audit_logs", "telemetry"]
    region: "Multi-region (tenant-pinned per `tenant.primary_region`)"
    certifications: ["SOC 2 Type II", "ISO 27001", "ISO 27018", "PCI-DSS Level 1", "HIPAA-compliant infra"]
    dpa_url: "https://www.cloudflare.com/cloudflare-customer-dpa/"
    primary_jurisdiction: "United States (EU offices)"
    contract_signed_at: "2026-04-23"

  - id: "neon"
    name: "Neon Inc."
    role: "Postgres database hosting (control plane)"
    data_categories_processed: ["billing_data", "account_data"]
    region: "US or EU (selectable per tenant)"
    certifications: ["SOC 2 Type II", "ISO 27001", "HIPAA-compliant"]
    dpa_url: "https://neon.tech/dpa"
    primary_jurisdiction: "United States"
    contract_signed_at: "2026-04-23"

  # ... (5 outros: Grafana Cloud, Stripe, GitHub, Sigstore, PagerDuty)
---

# CoreLink — Sub-Processors Register

CoreLink processa dados via os seguintes sub-processadores conforme GDPR Art. 28.2 + LGPD Art. 39.
Lista completa abaixo + última atualização em frontmatter.

## ⚠️ Right to Object

Per GDPR Art. 28.2.b + LGPD Art. 39, customers podem objetar a mudanças via:
- **Email**: privacy@hugr.dev
- **API**: `POST /v1/privacy/sub-processor-objection`
- **DPA escalation**: Privacy Officer + Legal review ≤ 14 dias úteis → accept-or-terminate decision.

[Tabela detalhada por sub-processor com role, region, certifications, DPA URL]

## Subscribed to update notifications?

Default: yes para **TODOS os 5 planos canonical** (free/solo/team/business/enterprise) — `sub_processor_notifications` purpose tem `legal_obligation` basis (privacy_model.md §5.6.1); NÃO é opt-out-able via consent_revoke. Lote 10.11.0-bis correção P0-3 round-1.
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + customer cost protection + service protection discipline justification)

### 2.1 Contexto

GDPR Art. 28.2 estabelece: "The processor shall not engage another processor without prior specific or general written authorisation of the controller. **In the case of general written authorisation, the processor shall inform the controller of any intended changes** concerning the addition or replacement of other processors, **thereby giving the controller the opportunity to object to such changes**." LGPD Art. 39 espelha. EDPB Guidelines 7/2020 §125 esclarece: 30d notification é industry standard.

CoreLink usa 7 sub-processors hoje (compliance_matrix.md §7 inheritance). Mudança esperada em S-12+ conforme product expand: e.g., adicionar Sentry para error tracking, ou substituir Cloudflare Email por SendGrid. Cada change → broadcast obrigatório. Sem automation, processo manual + susceptível a miss = regulatory finding (FM-453 NEW).

### 2.2 Abordagem

`legal/sub-processors.md` é YAML frontmatter + markdown body source-of-truth (git versioned). CD pipeline em PR merge: (a) parse YAML; (b) compare com previous version; (c) if change detected → trigger 30d broadcast cron worker; (d) auto-gen `/privacy/sub-processors` Cloudflare Pages static page; (e) emit `sub_processor.published.v1` ou `.changed.v1` audit event.

Email broadcast: Cloudflare Email transactional sender (DKIM signed via tenant-scoped HKDF info=`corelink/v1/dkim-broadcast`); per-recipient delivery confirmation tracked em D1 `sub_processor_broadcast_log`. **TODOS os planos canonical (free/solo/team/business/enterprise) recebem mandatory sub-processor notifications** — corrige GPT P0-3 round-1: tier-gating viola GDPR Art. 28.2 + LGPD Art. 39 (transparência sub-processor é direito de TODOS, não premium feature). `sub_processor_notifications` purpose tem `legal_obligation` basis (privacy_model.md §5.6.1 canonical enum) — NÃO é opt-out-able via consent_revoke (apenas `marketing_email` é opt-out via consent). Objection endpoint `POST /v1/privacy/sub-processor-objection` cria ticket roteado Privacy Officer + Legal review ≤14d. Decisão accept-or-terminate per DPA (default: minor objection → accept + workaround; major → DPA escalation + customer can terminate sem penalty).

### 2.3 Valor entregue

- **GDPR Art. 28.2 + LGPD Art. 39 alignment absoluto**: lista pública + 30d broadcast + objection flow.
- **CTRL-PRIV-021 satisfação**: sub-processor list + notificação ≥ 30d antes de mudança.
- **Customer trust**: transparency + accept-or-terminate empowerment GDPR Art. 28.2.b.
- **Audit-grade**: `legal/sub-processors.md` git history + 3 CloudEvents R2 7y + per-recipient delivery confirmation D1 retention.
- **FM-453 mitigation**: broadcast miss detection via sub_processor_broadcast_log gap analysis.

### 2.4 Principais riscos & trade-offs

- **30d broadcast vs faster customer impact**: 30d é industry standard mas pode atrasar product velocity (e.g., new sub-processor needed urgently). **Mitigation**: pre-stage em legal/sub-processors.md DRAFT branch → 30d count starts em merge → urgency mitigation only via Privacy Officer waiver + ADR.
- **Objection flow accept-or-terminate vs alternative compromise**: GDPR Art. 28.2.b enables "object", interpreted strict = customer can terminate sem penalty. Aggressive empowerment vs revenue impact. **Mitigation**: default minor objection → workaround proposal (e.g., opt-out de specific sub-processor scope per tenant); major → DPA termination clause activated.
- **Email delivery reliability vs broadcast SLA**: CF Email transactional ≥ 99% delivery rate; SEV-2 alert se < 95% sustained. Per-recipient D1 log enables retry + manual intervention.
- **DKIM key rotation**: tenant-scoped DKIM key via HKDF (security_model.md §374 inheritance); rotation S-19 future. Trade-off: simplicidade vs cross-tenant signature unique key.

## 3. Customer Impact & Journey

### 3.1 Personas afetadas

- **Privacy-aware data subject**: lê `/privacy/sub-processors` page; subscribed to notifications via `team`+ plans default.
- **Tenant admin/owner**: receives 30d email broadcast; pode objection via API ou email.
- **Privacy Officer interno**: aciona 30d broadcast cron + revisa objections + Legal escalation.
- **Legal team interno**: aciona accept-or-terminate decision per DPA.
- **External auditor**: revisa git history `legal/sub-processors.md` + delivery confirmation D1 + 3 CloudEvents trail.

### 3.2 Customer journey (touchpoints)

1. Privacy Officer drafts mudança em `legal/sub-processors.md` PR (e.g., add Sentry).
2. Legal review per DPA + GDPR Art. 28.2 implications.
3. PR merge → CD pipeline (a) auto-gen `/privacy/sub-processors` page (`sub_processor.published.v1` event); (b) trigger 30d broadcast cron worker (`sub_processor.changed.v1` event); (c) D1 sub_processor_broadcast_log seed per customer.
4. 30d broadcast cron worker: enqueue Cloudflare Email transactional per subscribed customer; DKIM signed; delivery confirmation tracked.
5. Customer receives email com summary + objection link.
6. Customer clicks objection link → POST /v1/privacy/sub-processor-objection com payload (objection reason + proposed alternative).
7. Privacy Officer + Legal review ticket ≤14d → decision: accept (workaround) ou terminate (DPA clause activated).
8. Final response email a customer + audit `sub_processor.objection_filed.v1` event.

### 3.3 Jornadas (User Journeys) afetadas

- **Privacy notice (WI-S11-004)** referencia sub-processor list em "Sub-Processors" section.
- **Settings** (S-13): tenant pode subscribe/unsubscribe sub-processor notifications via consent_revoke.
- **Billing portal** (S-10): se customer terminates via accept-or-terminate, billing settles via DPA clause.

### 3.4 Métricas de customer-visible

- **Broadcast delivery rate**: ≥ 99% per `corelink_sub_processor_broadcast_delivery_total{outcome}`.
- **30d notification SLA hit**: 100% per `corelink_sub_processor_30d_notification_compliance_total`.
- **Objection ticket SLA**: ≤ 14d p95 resolution.
- **Page freshness**: `/privacy/sub-processors` reflects git latest within 1h post-merge.

### 3.5 Comunicação ao customer

Email transactional 3 locales (PT-BR/EN/ES via WI-S11-004): (a) sub-processor change notification 30d antes; (b) objection ticket confirmation; (c) accept-or-terminate decision. DKIM signed + delivery confirmation per email.

### 3.6 Mitigação de fricção

- **Default subscribed `team`+**: customer não precisa configurar nada.
- **Free/solo opt-in**: avoid spam para casual users (LGPD Art. 6 minimização).
- **Objection workaround default**: Privacy Officer propõe alternative antes de terminate (e.g., opt-out scope per tenant).
- **30d antecipação**: customer tem tempo de avaliar sem rush.

### Anti-pattern ❌

❌ Sub-processor list em DPA static (silent change risk); ❌ Broadcast < 30d (GDPR Art. 28.2 violation); ❌ Objection sem accept-or-terminate flow (GDPR Art. 28.2.b violation); ❌ Email sem DKIM (deliverability + spoofing risk); ❌ Sem delivery confirmation (FM-453 silent broadcast miss); ❌ Tier-gated (free/solo opt-in) — viola GDPR Art. 28.2 + LGPD Art. 39 (transparência sub-processor é direito universal). Mandatory all canonical plans.

## 4. Capability Mapping / Trace

| Camada | ID | Item |
|---|---|---|
| Sprint contract | R-S11-12 + R-S11-13 | Sub-processor register + 30d notification + objection flow |
| CAPs | CAP-PRIV-006 | Sub-processor register público + notification |
| Invariantes | INV-AUDIT-APPEND-ONLY (CRITICAL §3.6 L116; 3 CloudEvents Object Lock 7y) |
| Controles | CTRL-PRIV-021 (sub-processor list + notificação) |
| Padrões | PAT-DRIFT-DETECTION-001 (CI hook detects sub_processors.md change) |
| Failure modes | **FM-453 (NEW S-11)** sub-processor change broadcast miss; FM-061 (audit Object Lock vs DSR — legal hold path); FM-105 (inter-region latency email broadcast) |
| Métricas | `corelink_sub_processor_broadcast_delivery_total{outcome,locale}` + `corelink_sub_processor_30d_notification_compliance_total` + `corelink_sub_processor_objection_resolution_seconds{outcome}` + `corelink_sub_processor_page_freshness_lag_seconds` |
| Eventos | 3 CloudEvents canonical: sub_processor.published/changed/objection_filed |
| Evidence | EVT-040 (VENDOR_REVIEW per sub-processor) + EVT-044 (LEGAL_REVIEW DPA) + EVT-001 (CI logs delivery confirmation) |

## 5. Tipo e Classificação

### 5.1 Tipo primário

`feature/regulatory-baseline` — implementação primária sub-processor transparency.

### 5.2 Prioridade

**P0** — sprint contract §6 DoD bloqueante (CTRL-PRIV-021 satisfaction).

### 5.3 Blast radius

**Cross-tenant** (broadcast a all subscribed customers); bug em delivery confirmation pode causar **mass FM-453 silent broadcast miss** (regulatory finding).

### 5.4 Reversibilidade

**Reversível** via git revert sub_processors.md + republish; mas 30d clock já started pode ser pause via Privacy Officer waiver + ADR (e.g., new sub-processor não vai ser adopted após review).

### 5.5 Experiment? (A/B test, feature flag experiment)

Não — regulatory feature, sem A/B.

### 5.6 Compliance triggers

- LGPD Art. 39 (sub-processor authorization).
- GDPR Art. 28.2 (sub-processor consent + objection right).
- SOC 2 CC9.2 (vendor management).
- ISO/IEC 27701 §7.5 (sub-processor management).
- EDPB Guidelines 7/2020 (concepts of controller and processor; sub-processor section).

## 6. Escopo

### 6.1 Em escopo (exaustivo)

1. `legal/sub-processors.md` source-of-truth com YAML frontmatter (7 canonical sub-processors initial; semver; per-sub-processor full metadata).
2. CI hook script `scripts/validate_sub_processors.py`:
   - (a) YAML schema validation;
   - (b) semver bump em qualquer mudança;
   - (c) per-sub-processor required fields (id, name, role, data_categories_processed, region, certifications, dpa_url, contract_signed_at);
   - (d) Legal Review evidence path em metadata (per sub-processor).
3. CD pipeline: auto-gen `/privacy/sub-processors` Cloudflare Pages static page rendering YAML + markdown body.
4. 30d broadcast cron worker (NEW; reuses S-09 cron worker pattern) — triggered em CD pipeline merge:
   - (a) Detect changes vs previous version (diff sub_processors yaml);
   - (b) Seed D1 `sub_processor_broadcast_log` per subscribed customer;
   - (c) Cloudflare Email transactional enqueue per recipient (DKIM signed via tenant-scoped HKDF);
   - (d) Delivery confirmation tracking via Cloudflare Email webhook → D1 update.
5. Objection endpoint POST /v1/privacy/sub-processor-objection (auth via PAT; rate limit 5/dia/subject):
   - (a) Cria ticket em D1 `sub_processor_objection` table;
   - (b) Roteia notification para Privacy Officer + Legal via PagerDuty (S-09 inheritance) + email transactional;
   - (c) SLA ≤ 14d resolution;
   - (d) Final decision (accept ou terminate) emitida em audit `sub_processor.objection_filed.v1`.
6. D1 schema migration: 2 NEW tables (sub_processor_broadcast_log + sub_processor_objection) — DDL §6.1.7.
7. 3 CloudEvents canonical types em audit-`<region>` Object Lock 7y; emit fail-CLOSED.
8. **FM-453 (NEW)** declaration em failure_modes.md: "Sub-processor change broadcast miss (delivery confirmation gap)" P1 (S=4 → upgrade FF-HR-010); RB-SUB-PROCESSOR-BROADCAST-MISS mapping.
9. RB-SUB-PROCESSOR-BROADCAST-MISS runbook (NEW canonical) — SOP para detect + remediate broadcast miss.
10. WI-S11-003 integration: consent_ledger sub_processor_notifications purpose check antes de send (only subscribed customers receive).
11. Email templates 3 locales × 3 types (notification + objection confirm + decision) = 9 .mjml files reusing WI-S11-004 template infrastructure.
12. ADR-S11-008 (NEW): Default subscribed `team`+ plans rationale (LGPD Art. 6 minimização vs regulatory baseline).

#### 6.1.7 D1 schema migration (2 tables)

```sql
-- D1 migration N+4: sub_processor_broadcast_log
CREATE TABLE sub_processor_broadcast_log (
  log_id                    TEXT        PRIMARY KEY,                  -- ULID 26 chars
  broadcast_id              TEXT        NOT NULL,                     -- ULID per broadcast event (groups all recipients)
  sub_processors_version    TEXT        NOT NULL,                     -- semver of sub_processors.md
  tenant_id                 TEXT        NOT NULL,
  recipient_email_hash      TEXT        NOT NULL,                     -- sha256 (CTRL-PRIV-014)
  locale                    TEXT        NOT NULL CHECK (locale IN ('pt-BR','en-US','es-MX')),
  notification_type         TEXT        NOT NULL CHECK (notification_type IN ('30d_advance_notice','objection_confirmation','final_decision')),
  enqueued_at               TEXT        NOT NULL,                     -- ISO 8601 UTC
  delivered_at              TEXT        NULL,                         -- via Cloudflare Email webhook
  delivery_status           TEXT        NOT NULL DEFAULT 'enqueued' CHECK (delivery_status IN ('enqueued','sent','delivered','bounced','complained','failed')),
  delivery_error_class      TEXT        NULL,                         -- enum if bounced/failed
  -- idempotency UNIQUE
  UNIQUE (broadcast_id, tenant_id, recipient_email_hash, notification_type)
);

CREATE INDEX idx_sub_processor_broadcast_log_status ON sub_processor_broadcast_log(delivery_status, enqueued_at) WHERE delivery_status IN ('enqueued', 'sent');
CREATE INDEX idx_sub_processor_broadcast_log_tenant ON sub_processor_broadcast_log(tenant_id, enqueued_at DESC);

-- D1 migration N+4: sub_processor_objection
CREATE TABLE sub_processor_objection (
  objection_id              TEXT        PRIMARY KEY,                  -- ULID 26 chars
  tenant_id                 TEXT        NOT NULL,
  subject_id                TEXT        NOT NULL,
  subject_id_hash           TEXT        NOT NULL,                     -- sha256 (CTRL-PRIV-014)
  sub_processor_id          TEXT        NOT NULL,                     -- e.g., "sentry"
  sub_processors_version    TEXT        NOT NULL,                     -- semver context
  objection_reason          TEXT        NOT NULL,                     -- customer-provided rationale (encrypted at rest if PII)
  proposed_alternative      TEXT        NULL,                         -- customer-proposed workaround
  ticket_status             TEXT        NOT NULL DEFAULT 'pending' CHECK (ticket_status IN ('pending','in_review','accepted','terminated','withdrawn')),
  resolution_decision       TEXT        NULL,                         -- accept | terminate | workaround_offered
  resolution_note           TEXT        NULL,                         -- Privacy Officer + Legal rationale
  filed_at                  TEXT        NOT NULL,                     -- ISO 8601 UTC
  resolved_at               TEXT        NULL,
  expected_resolution_at    TEXT        NOT NULL,                     -- = filed_at + 14d business days
  -- idempotency UNIQUE
  UNIQUE (tenant_id, subject_id, sub_processor_id, sub_processors_version)
);

CREATE INDEX idx_sub_processor_objection_status ON sub_processor_objection(ticket_status, expected_resolution_at);
```

#### 6.1.8 FM-453 declaration (failure_modes.md)

```
| FM-453 | Sub-processor change broadcast miss (delivery confirmation gap > 5% after 30d window) | 3 | 3 | 4 | 36 | P1 (S=4 → upgrade FF-HR-010) | INV-AUDIT-APPEND-ONLY + sub_processor_broadcast_log per-recipient + RB-SUB-PROCESSOR-BROADCAST-MISS + alert SEV-2 |
```

### 6.2 Componentes C4 afetados

- **Cloudflare Pages** (`/privacy/sub-processors`).
- **D1**: 2 NEW tables (sub_processor_broadcast_log + sub_processor_objection).
- **R2 audit-`<region>`**: 3 canonical CloudEvents Object Lock 7y.
- **Cloudflare Email** (S-13 inheritance): broadcast + objection + decision emails.
- **Cron worker** (S-09 inheritance): 30d broadcast trigger.
- **CI/CD pipeline**: validate_sub_processors.py + auto-gen page.
- **PagerDuty** (S-09 inheritance): Privacy Officer + Legal alert routing for objections.

### 6.3 Arquivos do repositório

```
legal/
└─ sub-processors.md                            # YAML frontmatter + markdown body (7 canonical)

scripts/
└─ validate_sub_processors.py                   # CI hook

crates/
└─ corelink-privacy-sub-processor-emit/         # NEW small crate
   ├─ Cargo.toml
   └─ src/
      └─ lib.rs                                 # 3 CloudEvents emitter

workers/
└─ sub-processor-broadcast-cron/                # NEW cron worker
   ├─ Cargo.toml
   └─ src/
      ├─ lib.rs                                 # 30d broadcast scheduler
      ├─ delivery_tracker.rs                    # Cloudflare Email webhook → D1
      └─ objection_handler.rs                   # POST /v1/privacy/sub-processor-objection

migrations/
└─ N+4__sub_processor_tables.sql                # 2 tables DDL §6.1.7

specs/
├─ 03_architecture/adrs/
│  └─ ADR-S11-008-sub-processor-default-subscribed-tier-team-plus.md  # NEW
└─ runbooks/
   └─ RB-SUB-PROCESSOR-BROADCAST-MISS.md        # NEW canonical

# email templates (reuse WI-S11-004 mjml infra)
legal/sub-processors-templates/
├─ 30d-advance-notice.{pt-BR,en-US,es-MX}.mjml  # 3 locales
├─ objection-confirmation.{pt-BR,en-US,es-MX}.mjml
└─ final-decision.{pt-BR,en-US,es-MX}.mjml
```

### 6.4 Sistemas externos tocados

- **Cloudflare Pages** (sub-processors page).
- **R2** (audit + R2 Pages assets).
- **Cloudflare Email** (broadcast + transactional).
- **D1** (2 NEW tables).
- **PagerDuty** (objection escalation).

## 7. Anti-Scope

- ❌ DSR API endpoints — entregue em WI-S11-001.
- ❌ Erasure worker — entregue em WI-S11-002.
- ❌ Consent ledger D1 — entregue em WI-S11-003 (consume sub_processor_notifications purpose).
- ❌ Privacy notice content — entregue em WI-S11-004 (referenced section).
- ❌ Breach notification runbook — entregue em WI-S11-006.
- ❌ Residency pinning — entregue em WI-S11-007.
- ❌ DPIA + LIA + TLA+ — entregue em WI-S11-008.
- ❌ Customer DPA contract templates — anti-scope spec framework (Privacy Officer + Legal responsibility per compliance_matrix.md §10.2).
- ❌ Vendor SOC 2 / ISO 27001 audit per-sub-processor — Privacy Officer + Compliance responsibility.
- ❌ Sub-processor cost negotiation — Finance responsibility.

### Anti-pattern ❌

❌ Sub-processor list em DPA static; ❌ Broadcast < 30d; ❌ Objection sem accept-or-terminate; ❌ Email sem DKIM; ❌ Sem delivery confirmation; ❌ Tier-gated (free/solo opt-in) — viola GDPR Art. 28.2 + LGPD Art. 39 (Lote 10.11.0-bis correção P0-3).

## 8. Acceptance Criteria (Gherkin) — 8 scenarios

### AC-001: Initial sub-processor register publication v1.0.0

```gherkin
Given legal/sub-processors.md tem YAML frontmatter com 7 canonical sub-processors (Cloudflare/Neon/Grafana/Stripe/GitHub/Sigstore/PagerDuty)
And per-sub-processor todos campos required populated
When PR merge é executed
Then CI hook validate_sub_processors.py valida (a) YAML schema; (b) semver = "1.0.0"; (c) 7 sub-processors; (d) per-sub-processor Legal Review evidence path
And CD pipeline auto-gen /privacy/sub-processors page em Cloudflare Pages com tabela 7-row
And `dev.hugr.corelink.sub_processor.published.v1` emitido em audit-`<region>` com payload {version: "1.0.0", sub_processors: [...]}
And NO broadcast triggered (initial — sem previous version para diff)
```

### AC-002: Sub-processor change 30d broadcast trigger

```gherkin
Given sub-processors v1.0.0 publicado com 7 sub-processors
When PR merge para v2.0.0/ adiciona Sentry como 8º sub-processor
Then CI hook detecta change → CD pipeline triggers 30d broadcast cron worker
And `dev.hugr.corelink.sub_processor.changed.v1` emitido com payload {old_version: "1.0.0", new_version: "2.0.0", added: ["sentry"], removed: [], modified: []}
And cron worker seed D1 sub_processor_broadcast_log per customer (TODOS os 5 planos canonical — Lote 10.11.0-bis: legal_obligation basis NÃO opt-out-able)
And Cloudflare Email transactional enqueue per recipient com DKIM signed
And email content em locale do recipient (PT-BR/EN/ES via tenant.locale_default fallback)
And 30d countdown starts em ts T (broadcast trigger ts)
And /privacy/sub-processors page atualizada com Sentry row + last_updated bumped
```

### AC-003: Email delivery confirmation tracking

```gherkin
Given 1000 emails broadcast enqueued para change v2.0.0
When Cloudflare Email webhook reports delivery status per email
Then D1 sub_processor_broadcast_log row updated com delivered_at + delivery_status='delivered' (ou 'bounced','complained','failed')
And `corelink_sub_processor_broadcast_delivery_total{outcome,locale}` Prom counter incrementado per outcome
And se delivery rate < 95% após 24h, SEV-2 alert disparado (FM-453 mitigation)
And RB-SUB-PROCESSOR-BROADCAST-MISS runbook acionado para investigation + manual retry
```

### AC-004: Objection endpoint happy path

```gherkin
Given customer C em tenant T tem auth válido (PAT)
When ele POSTa /v1/privacy/sub-processor-objection com payload {sub_processor_id: "sentry", objection_reason: "GDPR data residency concern; we don't accept US-based processors", proposed_alternative: "Self-hosted Sentry"}
Then a resposta é 201 com body {objection_id: "<ULID>", expected_resolution_at: "<ts +14d>"}
And sub_processor_objection row inserida com ticket_status='pending'
And PagerDuty alert disparado para Privacy Officer + Legal (`sub_processor.objection_filed.v1` audit event)
And email confirmation enviado em locale do customer
And `corelink_sub_processor_objection_resolution_seconds{outcome='filed'}` Prom histogram start
```

### AC-005: Objection resolution decision (accept ou terminate)

```gherkin
Given um sub_processor_objection ticket pendente para sentry em tenant T
When Privacy Officer + Legal review ≤14d e decidem decision = "workaround_offered" com note "Self-hosted Sentry adopted for tenant T"
Then PATCH /v1/privacy/sub-processor-objection/{id} (admin auth) com payload {ticket_status: "accepted", resolution_decision: "workaround_offered", resolution_note: "..."}
And sub_processor_objection row updated; resolved_at = NOW()
And email final decision enviado a customer (locale)
And `dev.hugr.corelink.sub_processor.objection_filed.v1` re-emitido com payload.outcome update
And `corelink_sub_processor_objection_resolution_seconds{outcome='accepted'}` p95 ≤ 14d
But se decision = "terminated", DPA termination clause activated → S-10 billing settle workflow + cascade unsubscribe
```

### AC-006: 30d notification SLA compliance

```gherkin
Given sub-processors v2.0.0 publicado em ts T com Sentry add
When cron worker monitor 30d countdown
Then se any subscribed customer NÃO recebeu email + delivery_status != 'delivered' até T+30d, SEV-1 alert disparado
And `corelink_sub_processor_30d_notification_compliance_total{outcome='violated'}` incrementado
And RB-SUB-PROCESSOR-BROADCAST-MISS runbook acionado
And Privacy Officer + Legal escalation; consider regulatory disclosure se > 1% violation rate
```

### AC-007: Audit fail-CLOSED em emit failure

```gherkin
Given audit emit infrastructure (R2 audit-`<region>`) está indisponível
When CD pipeline tenta emit `sub_processor.published.v1` no merge
Then CD pipeline aborts (transaction-like behavior)
And /privacy/sub-processors page NÃO atualizada
And SEV-2 alert disparado
And rollback-safe: PR merge fica em "deploy_pending" status
And distinct from WI-S10-001 billing fail-OPEN (split-tier discipline; ADR-S11-002)
```

### AC-008: FM-453 detection + RB-SUB-PROCESSOR-BROADCAST-MISS

```gherkin
Given broadcast em ts T com 1000 recipients enqueued
When 24h após T, sub_processor_broadcast_log query: count(delivery_status='delivered') / count(*) < 95%
Then SEV-2 alert disparado conforme FM-453 P1 (S=4)
And RB-SUB-PROCESSOR-BROADCAST-MISS runbook acionado (SOP em specs/05_quality/runbooks/RB-SUB-PROCESSOR-BROADCAST-MISS.md)
And SOP step: (a) classify failure mode (transient infra vs DKIM key issue vs CF Email bug); (b) manual retry list filter; (c) escalate Privacy Officer se > 5% violation; (d) regulatory disclosure consideration se > 30d window
```

## 9. Design Decisions

### 9.1 Decisões locais

- **DD-001 YAML frontmatter vs separate manifest file**: YAML frontmatter colocado em sub-processors.md unifica source-of-truth (markdown body é human-readable; YAML é machine-parseable). Alternative (separate `sub-processors.yaml`) splits source.
- **DD-002 30d window business days vs corridos**: corridos (não business) — alinha com industry standard EDPB 7/2020 §125; mais conservative para customer.
- **DD-003 Mandatory subscription TODOS os 5 planos** (Lote 10.11.0-bis REVISED corrigindo GPT P0-3 round-1): ADR-S11-008 v2 — sub_processor_notifications é `legal_obligation` basis (privacy_model.md §5.6.1). Tier-gating viola GDPR Art. 28.2 + LGPD Art. 39 — transparência sub-processor é direito de TODOS, não premium feature.
- **DD-004 DKIM tenant-scoped key vs global**: tenant-scoped via HKDF (security_model.md §374 inheritance); cross-tenant signature unique, simpler.
- **DD-005 Per-recipient delivery confirmation D1 vs R2 archive**: D1 supports query per-tenant, fast investigation; R2 archive future S-19 if scale demands.

### 9.2 Decisões que justificam ADR

- **ADR-S11-008 (REVISED Lote 10.11.0-bis)**: **Mandatory subscription para TODOS os 5 planos canonical** (free/solo/team/business/enterprise) para sub-processor notifications. Rationale: GDPR Art. 28.2 + LGPD Art. 39 estabelecem transparência sobre sub-processadores como direito do titular/customer regulado, NÃO premium feature. ADR v1.0 propunha tier-gating (LGPD Art. 6 minimização) — corrigido pós GPT P0-3 round-1: regulatory transparency outranks anti-spam minimization argument. `sub_processor_notifications` purpose tem `legal_obligation` basis canonical (privacy_model.md §5.6.1) — não opt-out-able. Privacy Officer + Compliance + Legal sign-off na revisão.

### 9.3 Trade-offs explícitos

| Trade-off | Opção A | Opção B | Decisão | Rationale |
|---|---|---|---|---|
| 30d broadcast window | Business days | Calendar | Calendar | EDPB 7/2020 industry standard |
| Default subscribed | All plans | team+ only | **All 5 plans canonical** | GDPR Art. 28.2 + LGPD Art. 39 transparência universal (Lote 10.11.0-bis REVISED corrigindo P0-3 round-1; tier-gating viola direito universal sub-processor transparency) |
| DKIM key scope | Global | Tenant-scoped | Tenant-scoped | HKDF inheritance security_model |
| Delivery confirmation | D1 query | R2 archive | D1 | Per-tenant fast investigation |
| Objection rate limit | Unlimited | 5/dia/subject | 5/dia | Anti-DoS humane |
| Source-of-truth format | YAML standalone | YAML frontmatter | Frontmatter | Unified markdown source |

### Anti-pattern ❌

❌ Business days 30d (industry deviation); ❌ Tier-gated free/solo opt-in (viola GDPR Art. 28.2 + LGPD Art. 39 — Lote 10.11.0-bis correção P0-3); ❌ Global DKIM key (cross-tenant signature shared); ❌ R2 archive only delivery confirmation (no fast query); ❌ Unlimited objection rate (anti-DoS gap).

## 10. Completeness Criteria SOTA

### 10.1 Code Completeness

- [ ] **C-1.1** `legal/sub-processors.md` v1.0.0 com 7 canonical sub-processors committed.
- [ ] **C-1.2** `scripts/validate_sub_processors.py` CI hook (4 validations a-d).
- [ ] **C-1.3** D1 migration N+4 (2 tables) aplica + roll-back; UNIQUE constraints verde.
- [ ] **C-1.4** Crate `corelink-privacy-sub-processor-emit` (NEW; 3 CloudEvents) compila WASM.
- [ ] **C-1.5** Worker `sub-processor-broadcast-cron` (NEW) consumes Cloudflare Cron + emits Email transactional.
- [ ] **C-1.6** Objection endpoint POST handler em corelink-privacy-dsr-api (S-11-001 reuse) ou separate worker.
- [ ] **C-1.7** ADR-S11-008 v2 (Lote 10.11.0-bis REVISED) mandatory all-plans rationale committed.
- [ ] **C-1.8** RB-SUB-PROCESSOR-BROADCAST-MISS runbook committed.
- [ ] **C-1.9** failure_modes.md FM-453 entry + RB-SUB-PROCESSOR-BROADCAST-MISS mapping.

### 10.2 Test Completeness

- [ ] **T-2.1** Integration test full lifecycle (PR sub-processors update → CI validate → CD broadcast → delivery confirmation → objection submit → resolution) verde.
- [ ] **T-2.2** Property test idempotency broadcast 100k iter (UNIQUE invariant).
- [ ] **T-2.3** Regression test mandatory all-plans (5 tiers canonical TODOS recebem; opt-out via consent_revoke retorna 403 `legal_obligation_purpose` — NÃO é opt-out-able).
- [ ] **T-2.4** Regression test 30d countdown SLA (cron monitor; 30-day boundary).
- [ ] **T-2.5** Chaos test Cloudflare Email webhook delayed → delivery_status='enqueued' eternally → SEV-3 alert.
- [ ] **T-2.6** Regression test DKIM tenant-scoped (cross-tenant signature isolation).
- [ ] **T-2.7** Property test objection state machine (5 states; valid transitions).
- [ ] **T-2.8** Cross-locale email test: 3 locales × 3 notification types = 9 templates rendered correctly.

### 10.3 Documentation Completeness

- [ ] **D-3.1** `docs/dev/sub-processor-broadcast-architecture.md`.
- [ ] **D-3.2** ADR-S11-008 v2 mandatory all-plans rationale (Lote 10.11.0-bis REVISED).
- [ ] **D-3.3** RB-SUB-PROCESSOR-BROADCAST-MISS SOP.
- [ ] **D-3.4** OpenAPI 3.1 para POST /v1/privacy/sub-processor-objection.

### 10.4 Observability Completeness

- [ ] **O-4.1** 4 Prom metrics: broadcast_delivery_total, 30d_notification_compliance_total, objection_resolution_seconds, page_freshness_lag_seconds.
- [ ] **O-4.2** 1 dashboard `corelink-sub-processor-broadcast` em Grafana com 6 panels.
- [ ] **O-4.3** 3 CloudEvents canonical types em audit-`<region>` Object Lock 7y.

### 10.5 Security & Privacy Completeness

- [ ] **S-5.1** recipient_email_hash sha256 (CTRL-PRIV-014); raw email NUNCA em logs.
- [ ] **S-5.2** DKIM signed via tenant-scoped HKDF.
- [ ] **S-5.3** Objection endpoint rate limit 5/dia/subject (S-08 inheritance).
- [ ] **S-5.4** GDPR Art. 28.2 + LGPD Art. 39 alignment Legal Review pre-merge.
- [ ] **S-5.5** Cross-tenant DKIM isolation: 100k random pairs property test.

### 10.6 SBOM Completeness

- [ ] **B-6.1** SBOM CycloneDX 1.5+ inclui `corelink-privacy-sub-processor-emit` + `sub-processor-broadcast-cron` + Python script deps.

## 11. DoD

10.x checked + sign-off matrix §30 + chaos 30d staging + 30d broadcast simulation verde em staging tempo-acelerado.

## 12. Invariants Validated

| INV | Severity | Position canonical | Cobertura WI-S11-005 |
|---|---|---|---|
| **INV-AUDIT-APPEND-ONLY** | CRITICAL | invariant_registry.md §3.6 L116 | 3 CloudEvents canonical em audit-`<region>` Object Lock 7y; emit fail-CLOSED CD pipeline abort |

## 13. Artifacts Produced

- `legal/sub-processors.md` v1.0.0 (7 canonical sub-processors).
- `scripts/validate_sub_processors.py` CI hook.
- `crates/corelink-privacy-sub-processor-emit/` (NEW; ~600 LoC).
- `workers/sub-processor-broadcast-cron/` (NEW; ~1500 LoC).
- `migrations/N+4__sub_processor_tables.sql` (DDL §6.1.7).
- 3 CloudEvents schemas em `schemas/cloudevents/sub-processor-{published,changed,objection_filed}.v1.json`.
- 9 email templates (3 locales × 3 types) em `legal/sub-processors-templates/`.
- ADR-S11-008 v2 mandatory all-plans rationale (Lote 10.11.0-bis).
- RB-SUB-PROCESSOR-BROADCAST-MISS runbook.
- failure_modes.md FM-453 entry.
- Grafana dashboard JSON.
- 4 Prom metrics + alerts.

## 14. Quality Standards SOTA

- **14.s11.5.1** GDPR Art. 28.2 + LGPD Art. 39 Legal Review pre-merge.
- **14.s11.5.2** 30d broadcast SLA compliance ≥ 99% sustained 90d.
- **14.s11.5.3** Email delivery rate ≥ 95% sustained.
- **14.s11.5.4** Objection resolution SLA ≤ 14d p95.
- **14.s11.5.5** DKIM tenant-scoped 100k cross-tenant property test 0 leaks.
- **14.s11.5.6** Default subscribed tier policy ADR-S11-008 + Privacy Officer sign-off.
- **14.s11.5.7** FM-453 detection + remediation via RB-SUB-PROCESSOR-BROADCAST-MISS.
- **14.s11.5.8** INV §3.X positions canonical verified pre-merge.

## 15. Chaos Experiments (6)

1. Cloudflare Email webhook delayed > 24h → delivery_status='enqueued' eternally → SEV-3 alert + manual retry.
2. DKIM key rotation mid-broadcast → both old + new keys aceitos por 24h grace (kid-based).
3. Sub-processor list YAML schema bypass attempt (CI hook) → integration test detects + blocks.
4. Objection ticket > 14d SLA → SEV-2 + Privacy Officer + Legal escalation auto.
5. Cross-tenant broadcast leak (tenant T1 broadcast inclui T2 customers) → property test 100k pairs verifies isolation.
6. 30d countdown miscalculation (timezone bug) → property test 6 regions × business calendar verifies determinism.

## 16. PRR

PRR HIGH_RISK 12 sign-offs + chaos 30d + Privacy + Legal + Compliance mandatory emphatic.

## 17. Sub-tasks

| ID | Descrição | PERT |
|---|---|---|
| ST-001 | legal/sub-processors.md v1.0.0 7 canonical (Privacy Officer + Legal lead; eng support) | 1.5h |
| ST-002 | validate_sub_processors.py CI hook | 1h |
| ST-003 | D1 migration N+4 (2 tables) | 1h |
| ST-004 | corelink-privacy-sub-processor-emit crate (3 CloudEvents) | 1h |
| ST-005 | sub-processor-broadcast-cron worker (Cloudflare Cron + Email enqueue) | 2h |
| ST-006 | Delivery confirmation webhook tracking | 1h |
| ST-007 | Objection endpoint + state machine | 1.5h |
| ST-008 | ADR-S11-008 + RB-SUB-PROCESSOR-BROADCAST-MISS | 0.8h |
| ST-009 | failure_modes.md FM-453 entry | 0.3h |
| ST-010 | 9 email templates (3 locales × 3 types) | 0.7h |

**PERT total**: ~10.3h (alinha com sprint contract §12 — anteriormente estimado 10.3h).

## 18. Dependencies

- **Hard**: spec contract S-11 v1.2.0 SEALED; S-09 SEALED (cron worker + PagerDuty + audit emit + Cloudflare Email); WI-S11-003 (consent ledger consume sub_processor_notifications purpose); WI-S11-004 (template infra reuse).
- **Soft**: WI-S11-001 (DSR API objection endpoint may reuse same handler pattern).

## 19. Effort PERT: ~10.3h. ## 20. Time-boxing: 16h hard limit (lane HIGH_RISK +40% buffer).

## 21. Observability

4 Prom metrics + 1 Grafana dashboard 6 panels + 3 CloudEvents canonical.

## 22. Cost Analysis

- **Cloudflare Email transactional**: 1000 customers × 1 broadcast/quarter × 2-4 changes/year = 4-8K emails/year × $0.001 = $4-8/year.
- **D1 sub_processor tables**: low-volume (~1K rows/year); negligible.
- **R2 audit-`<region>`**: 3 events/version × 4 changes/year × 7y = ~84 events; ≈$0.01/year.
- **PagerDuty objection alerts**: ~2-5/year; included em S-09 budget.
- **Cron worker compute**: 1 invocation/broadcast; ~30s; ≈$0.001/broadcast.
- **Total estimated**: ≤ $20/year (well under §14 budget).

## 23. API Contract

OpenAPI 3.1 para `POST /v1/privacy/sub-processor-objection` em `docs/api/privacy-sub-processor-objection.md`. 3 CloudEvents schemas em `schemas/cloudevents/sub-processor-*.v1.json`. legal/sub-processors.md YAML schema em `schemas/yaml/sub-processors-frontmatter.json`.

## 24. Post-mortem Hooks

| Trigger | Severity | Owner |
|---|---|---|
| FM-453 broadcast miss > 5% delivery rate | HIGH | Privacy Officer + SRE + Compliance |
| 30d notification SLA miss > 1% violation | CRITICAL | Privacy Officer + Legal escalation; consider regulatory disclosure |
| Objection ticket > 14d SLA | HIGH | Privacy Officer + Legal review |
| DKIM key compromise | CRITICAL | SecLead + Privacy + email reputation review |
| Cross-tenant broadcast leak | CRITICAL | SecLead + Privacy + Compliance |
| INV-AUDIT-APPEND-ONLY violation | CRITICAL | Privacy Officer + SecLead |
| Sub-processor list yaml schema bypass | HIGH | SecLead + Privacy + CI hardening |
| Audit fail-OPEN regression (Lote 10.6bis) | CRITICAL | Architect + Privacy |

## 25. Rollback / Recovery

Git revert sub_processors.md + republish via CI/CD; mas 30d clock já started pode ser pause via Privacy Officer waiver + ADR (e.g., new sub-processor não vai ser adopted após review). Documented em DPA: notification clock conservatively maintained.

## 26. Security & Privacy

LINDDUN per privacy_model.md §4 + STRIDE per security_model.md §6:
- L(inkability): recipient_email_hash sha256 (CTRL-PRIV-014).
- I(dentifiability): public list intentional (transparency).
- N(on-repudiation): R2 Object Lock 7y + DKIM signed + delivery confirmation.
- D(etectability): customer subscribed status visible em consent ledger.
- D(isclosure): sub-processor list intencionalmente público.
- U(nawareness): broadcast 30d antes garante awareness.
- N(on-compliance): **GDPR Art. 28.2 + LGPD Art. 39 + SOC 2 CC9.2 + EDPB 7/2020 compliance** via lista pública + 30d broadcast + objection flow + accept-or-terminate.

## 27. Knowledge Transfer

Tech talk (45min): "S-11 Sub-Processor Register: 30d broadcast + DKIM + delivery confirmation + objection accept-or-terminate flow GDPR Art. 28.2"; doc `docs/dev/sub-processor-broadcast-architecture.md`; onboarding test 4 questões: 30d window calendar (não business; EDPB 7/2020), mandatory all-plans rationale (ADR-S11-008 v2 GDPR Art. 28.2 + LGPD Art. 39 transparência universal — Lote 10.11.0-bis REVISED), DKIM tenant-scoped HKDF, FM-453 detection + RB acionamento.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | FM-453 broadcast miss > 5% | M | M | HIGH | M | LOW | Per-recipient D1 tracking + RB-SUB-PROCESSOR-BROADCAST-MISS + SEV-2 alert; 24h gap detection |
| R-002 | 30d SLA miss (regulatory exposure) | L | M | CRITICAL | M | LOW | Cron worker SLA monitor + SEV-1 alert; Privacy Officer escalation |
| R-003 | Objection ticket > 14d SLA | M | L | HIGH | M | LOW | PagerDuty escalation + automated SLA monitor |
| R-004 | DKIM key compromise | L | L | CRITICAL | L | LOW | Tenant-scoped HKDF + key rotation S-19 |
| R-005 | Cross-tenant broadcast leak | L | L | CRITICAL | L | LOW | Tenant_id pre-check em broadcast logic; property test 100k pairs |
| R-006 | Default subscribed tier policy controversial (LGPD Art. 6 challenge) | L | L | LOW | L | LOW | ADR-S11-008 + Privacy Officer + Compliance sign-off; quarterly review |
| R-007 | Sub-processor yaml schema bypass | L | L | MEDIUM | L | LOW | CI hook strict + integration test |
| R-008 | Email locale fallback miss (titular receives en-US instead of pt-BR) | L | L | LOW | L | LOW | Fallback chain documented; tenant.locale_default override |
| R-009 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | LOW | L | LOW | INV positions §3.6 L116 verified Lote 10.11.0 |
| R-010 | Audit fail-OPEN regression (Lote 10.6bis violation) | L | L | CRITICAL | L | LOW | ADR-S11-002 cross-WI; CD pipeline abort em emit failure |
| R-011 | Cloudflare Email transactional outage | L | L | MEDIUM | L | LOW | Retry queue + manual fallback via Privacy Officer SOP |
| R-012 | 30d countdown timezone bug (regional holidays mismatched) | L | L | LOW | L | LOW | UTC anchor + property test 6 regions × calendar |

## 29. Review Checkpoints

D+0 design (Architect; broadcast atomicity); D+1 Privacy Officer (LGPD Art. 39 + EDPB 7/2020); D+2 Legal (DPA reference + accept-or-terminate); D+3 SecLead (DKIM + STRIDE); D+4 Compliance (SOC 2 CC9.2); D+5 code review; D+6 PRR.

## 30. Sign-off (HIGH_RISK 12)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ |
| 4 | Security Lead | _TBD; **mandatory** — DKIM + STRIDE_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos 30d + 100k cross-tenant property test_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 CC9.2 + GDPR Art. 28.2 + LGPD Art. 39 + EDPB 7/2020_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — CTRL-PRIV-021 + ADR-S11-008 v2 mandatory all-plans + objection flow_ |
| 11 | Architect | _TBD; **mandatory emphatic** — split-tier audit fail-CLOSED + INV §3.X verification_ |
| 12 | DPO interim | _TBD; **mandatory emphatic** — accept-or-terminate clause defensibility_ |

(Legal sign-off via DPA reference at sprint level.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.1.0 | 2026-04-28 | Gustavo (Lote 10.11.0-bis + 10.11bis) | **Canonical fixes pós baseline review aggregate 5.18/10**: (a) **GPT P0-3 sub-processor mandatory ALL plans** (free/solo/team/business/enterprise) — corrigido tier-gating v1.0 que violava GDPR Art. 28.2 + LGPD Art. 39 (transparência sub-processor é direito universal NÃO premium feature). (b) **`sub_processor_notifications` purpose tem `legal_obligation` basis canonical** (privacy_model.md §5.6.1) — NÃO é opt-out-able via consent_revoke; apenas `marketing_email` é opt-out via consent. (c) **Typo "2 CloudEvents" → "3 types canonical"** (sub_processor.{published,changed,objection_filed}.v1) — header anterior dizia 2 mas body listava 3. (d) **ADR-S11-008 REVISED**: reverte tier-gating, mandatory all customers + Privacy Officer + Compliance + Legal sign-off. (e) **HKDF info canonical** = `corelink/v1/dkim-broadcast` para email DKIM signing. |
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.11) | Criação WI-S11-005; HIGH_RISK; SOTA pós-S-10 SEALED. Sub-processor transparency regulatory baseline GDPR Art. 28.2 + LGPD Art. 39. `legal/sub-processors.md` source-of-truth YAML frontmatter + markdown body com 7 canonical sub-processors initial (Cloudflare/Neon/Grafana Cloud/Stripe/GitHub/Sigstore/PagerDuty per compliance_matrix.md §7 inheritance). Auto-gen `/privacy/sub-processors` Cloudflare Pages static page. 30d broadcast cron worker (NEW; reuses S-09 cron pattern) — triggered em CD pipeline merge → Cloudflare Email transactional per subscribed customer com DKIM signed via tenant-scoped HKDF + delivery confirmation tracked via webhook em D1 sub_processor_broadcast_log. Mandatory subscription TODOS os 5 planos canonical (free/solo/team/business/enterprise) — ADR-S11-008 v1 propunha tier-gating LGPD Art. 6 minimização; v2 (Lote 10.11.0-bis REVISED) corrige após GPT P0-3 round-1: sub_processor_notifications é legal_obligation basis (privacy_model.md §5.6.1) — NÃO é opt-out-able via consent_revoke. Transparência sub-processor é direito universal GDPR Art. 28.2 + LGPD Art. 39. Objection endpoint POST /v1/privacy/sub-processor-objection rate limit 5/dia/subject (S-08 inheritance) → cria ticket em D1 sub_processor_objection table → roteia notification para Privacy Officer + Legal via PagerDuty (S-09 inheritance) + email transactional → SLA ≤14d resolution → decision accept (workaround) ou terminate (DPA clause activated). 3 CloudEvents canonical `dev.hugr.corelink.sub_processor.{published,changed,objection_filed}.v1` per Lote 10.9bis P0-G prefix em audit-`<region>` Object Lock 7y. NEW FM-453 declaration em failure_modes.md (broadcast miss P1 S=4 → upgrade FF-HR-010). NEW RB-SUB-PROCESSOR-BROADCAST-MISS runbook canonical. 9 email templates (3 locales × 3 notification types) reusing WI-S11-004 mjml infra. 8 AC scenarios + 6 chaos + 12 risks + 8 post-mortem hooks. NEW corelink-privacy-sub-processor-emit crate + sub-processor-broadcast-cron worker. **Lote 10.10 lessons absorbed**: source-of-truth FIRST INV positions verified pre-merge; typed enum (3 notification types + 5 ticket states); sign-off cap 12; cascade discipline absoluta; split-tier audit fail-CLOSED; corelink_time canonical helper; CTRL-PRIV-014 audit minimization (recipient_email_hash sha256); pagerduty inheritance from S-09. |

## 32. Anti-patterns evitados

- ❌ Sub-processor list em DPA static (silent change risk); ❌ Broadcast < 30d (GDPR Art. 28.2 violation); ❌ Objection sem accept-or-terminate (GDPR Art. 28.2.b violation); ❌ Email sem DKIM (deliverability + spoofing); ❌ Sem delivery confirmation (FM-453 silent broadcast miss); ❌ Tier-gated (free/solo opt-in) — viola GDPR Art. 28.2 + LGPD Art. 39 transparência universal (corrigido Lote 10.11.0-bis P0-3); ❌ Business days 30d window (industry deviation); ❌ Global DKIM key (cross-tenant signature shared); ❌ Audit fail-OPEN (Lote 10.6bis split-tier violation); ❌ INV §3.X position TBD (Lote 10.8bis P1-13); ❌ recipient_email raw em logs (CTRL-PRIV-014 violation); ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3).

---
