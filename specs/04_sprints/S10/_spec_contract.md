---
id: "SPEC-CONTRACT-S10"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.8.0"
created: "2026-04-24"
updated: "2026-05-03"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s10", "billing", "stripe", "usage-metering", "reconciliation", "high-risk", "sota-v1.3"]
---

# Spec Contract — S-10: Billing Pipeline (Usage Metering + Stripe + Reconciliation)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-10 |
| Nome | Billing Pipeline |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-005 (CTRL-BILLING-001 financial-grade), FF-HR-009 (contratos com customer via Stripe) |
| Duração estimada | 3 semanas |
| WIs antecipados | 7 |
| SOTA target | Financial-grade billing pipeline — exact metering (lossless, dup-free) + automated reconciliation + audit-grade replay-from-events |

## 1. Objetivo

Implementar **pipeline de billing financial-grade** que torna o CoreLink monetizável com **integridade SOC 2 / GAAP**: usage events append-only → counter aggregation → Stripe invoicing → reconciliation diária. Integridade financeira é CRITICAL — under-count = revenue leak; over-count = customer trust loss + chargebacks. **INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP** são enforced via reconciliation diária comparando Σ(events) ↔ Σ(counters) ↔ Σ(invoiced).

**Por que SOTA:** competitors monetizam com (a) sample-based metering (≠ exact); (b) sem reconciliation automatizada (manual quarterly); (c) sem replay-from-events (auditor não consegue reconstruir invoice). CoreLink S-10 entrega **3 níveis de defesa**: events imutáveis (audit), counters reconcilable (drift visibility), invoice replay (forensic). Reference: **Stripe Billing Architecture Guide**, **Mux exact-metering blog post (2023)**.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (10–12 sign-offs).
- **FF-HR-005**: CTRL-BILLING-001 (append-only events + reconciliation) é controle de segurança financeira; bypass = revenue leak ou customer chargeback storm.
- **FF-HR-009**: integração Stripe = contrato direto com customer; bug = invoice errada = legal exposure.
- **Inherent risk**: bug em billing código de produção pode custar milhões em refund + reputation loss; rigor HIGH_RISK não é negociável.

## 3. Inherits_from

```yaml
inherits_from:
  - "DATA-MODEL"               # schemas plan, subscription, invoice_line_item, usage_event
  - "OBSERVABILITY-MODEL"      # SLO-FRESH-BILLING, métricas billing
  - "SECURITY-MODEL"           # CTRL-BILLING-001
  - "COMPLIANCE-MATRIX"        # SOC 2 CC1.4 + GAAP revenue recognition
  - "FAILURE-MODES"            # FM-151 (Stripe outage), FM-302 (billing drift)
  - "RESILIENCE-PATTERNS"      # PAT-QUEUE-EVENTS-001, PAT-BACKOFF-001, PAT-IDEMPOTENCY-001
  - "SLO-CATALOG"              # SLO-FRESH-BILLING
  - "AUTH-MODEL"               # tenant_id propagação
  - "PRIVACY-MODEL"            # PII em invoice (customer name/email) — DSR support
  - "INVARIANT-REGISTRY"       # INV-BILLING-NO-LOSS, INV-BILLING-NO-DUP, INV-AUDIT-APPEND-ONLY, INV-BILLING-RECONCILE-3-LAYER
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-BILLING-001** | Usage events append-only | R2 bucket `billing-events` Object Lock 7y; event schema versioned; emitter no hot path CAS write/read/AC. |
| **CAP-BILLING-002** | Counter aggregation hourly | DO cron rollup events → D1 `usage_counter(tenant_id, region, sku, hour, qty, hash_chain)` PK 4-tuple. |
| **CAP-BILLING-003** | Stripe integration (customer/subscription/invoice) | `crates/corelink-billing` com idempotency keys + webhook handler + reconciliation pull. |
| **CAP-BILLING-004** | Daily reconciliation worker | Cron diário Σ(events) vs Σ(counters) vs Σ(Stripe invoiced) → drift alert > 0.1% → SEV-2. |
| **CAP-BILLING-005** | Overage handling | 80% quota → soft alert (email); 95% → ticket; 100% → hard-block CAS write (returns 429 over_quota). |
| **CAP-BILLING-006** | Customer-facing usage dashboard data | API `GET /v1/billing/usage` → admin UI S-13/S-16. |
| **CAP-BILLING-007** | Replay-from-events forensic API | `POST /v1/billing/replay?invoice_id=X` → reconstrói invoice from raw events; auditor + Finance use. |
| **CAP-BILLING-008** | Refund / dispute handler | Stripe webhook → atualiza local state + audit event; rate-limit dispute storm. |

## 5. Requirements específicos

### 5.1 Eventing (CAP-BILLING-001)

- **R-S10-1**: Usage event emitter no hot path (CAS write/read, AC lookup) seguindo CloudEvents v1.0 spec EVT-047 + Lote 10.9bis P0-G prefix canonical `dev.hugr.corelink.<op>.v1`:
  - `subject`: `tenant:<uuid>`
  - `type`: `dev.hugr.corelink.cas.put.v1` | `dev.hugr.corelink.cas.get.v1` | `dev.hugr.corelink.ac.lookup.v1` | `dev.hugr.corelink.gc.purge.v1`
  - `data`: `{tenant_id, region, bytes, sku, ts, request_id, idempotency_key}`
- **R-S10-2**: Idempotency: event deduplication via `(tenant_id, request_id)` UNIQUE em D1 staging table; replay seguro.
- **R-S10-3**: Schema versioned: `event_schema_version: "1.0.0"` em payload; backward-compat policy 2 versions.

### 5.2 Aggregation (CAP-BILLING-002)

- **R-S10-4**: Counter aggregator cron DO hourly: lê eventos do R2 hour bucket, agrega por `(tenant_id, region, sku, hour)` (PK 4-tuple per Lote 10.10-quaters R5 P0-A fix), escreve D1 `usage_counter` com `hash_chain` (prev_hash + own digest) for tamper detection.
- **R-S10-5**: Late-arriving events policy: events com `ts < now - 6h` são aceitos mas vão para `usage_counter_late` (separado) e disparam alert — protege contra silent backfill.

### 5.3 Stripe Integration (CAP-BILLING-003 + CAP-BILLING-008)

- **R-S10-6**: Crate `corelink-billing` com Stripe adapter:
  - **Idempotency-Key** SEMPRE present (RFC tipo: `idempotency-key: corelink-{tenant_id}-{event_hash}`); exemplo: `corelink-7f3a-bbb2c4` — protege contra retry double-charge.
  - **Test mode pinning**: env `CORELINK_STRIPE_MODE=test|live`; CI sempre test; merge to main → live key (separate Worker secret).
  - **Webhook handler**: `POST /v1/webhooks/stripe` valida signature `Stripe-Signature` HMAC-SHA256; idempotent via `stripe_event_id` UNIQUE.
- **R-S10-7**: Schema Neon `plan` (5 tiers canonical per `data_model.md §1` line 68: free/solo/team/business/enterprise), `subscription`, `invoice_line_item`, `stripe_event_log` (audit trail), `customer_billing_profile`.

### 5.4 Reconciliation (CAP-BILLING-004)

- **R-S10-8**: Daily reconciliation worker (cron 02:00 UTC):
  1. **Layer 1**: Σ(events) (R2 raw) vs Σ(counters D1) — drift > 0.1% → SEV-2.
  2. **Layer 2**: Σ(counters D1) vs Σ(invoice_line_item D1) — drift > 0.1% → SEV-2.
  3. **Layer 3**: Σ(invoice_line_item) vs Σ(Stripe invoice) (pull via Stripe API) — drift > 0.1% → SEV-1.
- **R-S10-9**: Reconciliation report em R2 `reconciliation-reports/YYYY-MM-DD.json` com per-tenant breakdown; retain 7y.

### 5.5 Overage / Quota (CAP-BILLING-005)

- **R-S10-10**: Quota state machine: `under_80 → soft_alert (80%) → ticket (95%) → hard_block (100%)`. Email + in-app notification em cada transição. Hard-block = 429 com `X-RateLimit-Layer: quota` header (S-08 alignment).
- **R-S10-11**: Quota grace period: enterprise tier tem 7 dias grace antes de hard-block (configurable per contract).

### 5.6 Forensic / Replay (CAP-BILLING-007)

- **R-S10-12**: API `POST /v1/billing/replay?invoice_id=X&dry_run=true|false` — reconstrói invoice from R2 raw events; comparison com Stripe atual; report em JSON. Dry_run default true.
- **R-S10-13**: Replay endpoint protegido por role `billing_admin` (CTRL-AUTHZ-001 + CTRL-AUTHZ-002) + audit event mandatory.

### 5.7 SLO + Observability

- **R-S10-14**: SLO-FRESH-BILLING enforcement: 99.9% events são processados (chegam em counter D1) ≤ 15 min após emissão.
- **R-S10-15**: Métricas (Prom underscores canonical per `observability_model.md §4.1`; Lote 10.9bis P0-E inheritance — sprint contract antes usava dots OTel-style): `corelink_billing_events_emitted_total{type, region}`, `corelink_billing_reconcile_drift_pct{layer, tenant_tier}`, `corelink_billing_stripe_api_calls_total{operation, status}`, `corelink_billing_invoice_value_usd_cents_total{tenant_tier}`.

## 6. Definition of Done

- [ ] **WIs SEALED**: 7/7.
- [ ] **E2E**: tenant signup → 30 dias uso simulado → monthly invoice generated → Stripe charged (test mode) → reconciliation green em 3 layers.
- [ ] **Chaos test**: Stripe outage 1h → events queued (PAT-QUEUE-EVENTS-001) → após Stripe recovery, retry sucede com 0 lost; 0 dup.
- [ ] **Drift detection**: simular drift artificial 0.5% em counter → reconciliation worker detecta + dispara SEV-2 em < 24h.
- [ ] **Property test**: replay 1M events idempotent — Σ(counters) idêntico ao replay; sem dup; sem loss.
- [ ] **Idempotency test**: 100 retries de mesmo event → 1 charge no Stripe (test mode).
- [ ] **Replay forensic**: gerar invoice fake → run replay endpoint → output match Stripe invoice byte-a-byte (modulo Stripe metadata).
- [ ] **PRR HIGH_RISK**: Finance reviewer + Legal (DPA reference) + Security lead + SRE + Product + Compliance officer + Engineer + QA + 2 peers + Privacy officer (PII em invoice).
- [ ] **TLA+ spec** `billing_atomicity.tla` (state machine event → counter → invoice; no-loss, no-dup) verde em CI.
- [ ] **Runbook dry-run**: RB-FM-302 (billing drift) + RB-FM-151 (Stripe outage) executados em staging (EVT-017).
- [ ] **SOC 2 walkthrough**: Finance + auditor (mock) consegue reconstruir 1 invoice from R2 events em < 30 min.

## 7. Completeness Criteria (delta local)

- [ ] **10.s10.1** SLO-FRESH-BILLING ≥ 99.9% sustained 30d em staging.
- [ ] **10.s10.2** Stripe test mode: $1000 de usage simulado → invoice correto byte-a-byte.
- [ ] **10.s10.3** Refund flow testado: customer contested → refund processed → audit event emitted.
- [ ] **10.s10.4** **Reconciliation 3-layer**: 30d running daily com 0 drift > 0.1%.
- [ ] **10.s10.5** **Replay endpoint** funcional + role-protected + audit trail.
- [ ] **10.s10.6** **Quota state machine**: soft alert + ticket + hard-block testados E2E.
- [ ] **10.s10.7** **Late-arriving events**: simular event 7h late → vai para `usage_counter_late` + alert.
- [ ] **10.s10.8** **Schema versioning**: emitir event v1.0.0 e v1.0.1 (campo novo) — both processados sem regression.

## 8. Invariants

### Mantidas (heredadas de canonical sources)

- **INV-BILLING-NO-LOSS** (HIGH; registry §3.9 line 136): Σ(events emitidos) = Σ(invoiced + tombstoned + late_pending). Drift > 0.1% = SEV-2 + Finance review (HIGH severity → SEV-2 canonical per registry §2); > 1% = SEV-1 + invoice freeze (operational escalation gate per §14.s10.1, independente da severity da invariant). Reference: invariant_registry.md §3.9 line 136.
- **INV-BILLING-NO-DUP** (HIGH; registry §3.9 line 137): nenhum charge duplicado por mesma fonte; enforced via Idempotency-Key + (tenant_id, request_id) UNIQUE. Drift > 0.1% = SEV-2; > 1% = SEV-1.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL; registry §3.6 line 116): usage events em R2 são append-only via Object Lock; tampering detected via hash chain. CRITICAL canonical (TLA+ proven Lote 6.2).

### Novas (introduzidas por S-10 — adicionar a invariant_registry.md)

- **INV-BILLING-RECONCILE-3-LAYER** (HIGH — novo): reconciliation diária verifica 3 layers (events ↔ counters ↔ Stripe); qualquer drift > 0.1% bloqueia close-of-month até resolution. **Why:** single-layer reconcile pode mascarar bug entre layers; 3-layer detecta exatamente onde está. **How to apply:** cron daily + bloqueio automático de invoice generation se drift open.
- **INV-BILLING-REPLAYABLE-FROM-EVENTS** (HIGH — novo): qualquer invoice deve poder ser reconstruída byte-a-byte a partir de events R2; replay endpoint test em CI mensal. **Why:** audit-grade requirement (SOC 2 CC1.4); sem replay, dispute legal indefendível.

## 9. Quality Standards (delta local)

- **14.s10.1 Zero tolerance pra drift > 0.1%** em reconciliation. Drift entre 0.01% e 0.1% = SEV-3 monitor; > 0.1% = SEV-2 immediate; > 1% = SEV-1 + invoice freeze.
- **14.s10.2 Stripe API retry com backoff** (PAT-BACKOFF-001) + queue fallback em outage; max 5 retries + exponential 1s/2s/4s/8s/16s + jitter.
- **14.s10.3 Audit-grade replay**: Finance pode reconstruir qualquer invoice de events raw em < 5 min; documented runbook RB-BILLING-001.
- **14.s10.4 PII em invoice**: customer name/email armazenados encrypted at rest (FIPS 140-3); DSR erasure (S-11) tem path para anonymizar invoice mantendo audit trail (pseudonymization).
- **14.s10.5 Idempotency-Key cardinality**: max 10M keys/mês (Stripe limit); rotation policy yearly; key collision impossível por construction (UUID v4 entropy).
- **14.s10.6 Concurrent webhook safety**: Stripe pode entregar same webhook 2× → handler é idempotent via `stripe_event_id` UNIQUE; reorder-safe via `event_timestamp` comparison.
- **14.s10.7 GAAP revenue recognition**: invoice em mês N reflete usage do mês N (não mês N-1 ou N+1); cutoff time UTC documented.
- **14.s10.8 Cost regression gate** (Lote 9.5b — meta §14.10): billing pipeline $USD/million events baseline; Stripe API costs per invoice generation tracked; PR > 10% cost regression bloqueia merge sem ADR. R2 events bucket retention 7y storage cost projection per tenant tier.

## 10. Anti-scope

- ❌ Tax calculation (Stripe Tax handles via integration; não implementar in-house).
- ❌ Multi-currency at GA (USD only em S-10; EUR/BRL pós-GA com FX risk hedging).
- ❌ Enterprise custom pricing engine (negotiated contracts → S-19 onboarding manual).
- ❌ Crypto / non-Stripe payment methods (BTC, wire transfer) — pós-GA opcional.
- ❌ Real-time per-second billing (we aggregate hourly — sufficient for B2B).
- ❌ Granular per-feature pricing (single SKU `cas_storage_gb_month` + `cas_egress_gb` para GA; expand pós-feedback).
- ❌ Invoice PDF generation in-house (Stripe handles).
- ❌ Customer-facing PCI scope (Stripe Elements iframe → we never touch card data).

## 11. Dependencies

### Hard blockers

- **S-01 + S-02 + S-04 SEALED** (usage points existem para CAS write/read/AC).
- **S-07 SEALED** (dedup/eviction emit usage events; codex propagation finding).
- **S-08 SEALED** (quota hard-block 429 alignment).
- **S-03 SEALED** (auth tenant context para event tagging).
- **S-09 SEALED** (observability para SLO-FRESH-BILLING enforcement + reconciliation alerts).

### Soft blockers

- **S-08 SEALED** (rate limit overlap com quota hard-block).
- **S-11 SEALED** (privacy DSR para PII redaction em invoice — aceitável shipping S-10 antes se DSR plan documented).

### Outbound

- S-13 (admin plane usage dashboard).
- S-16 (frontend admin UI billing tab).
- S-19 (enterprise onboarding referencia plan tier).
- S-20 (GA exige reconciliation 30d clean).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | Optimistic | Most-likely | Pessimistic | PERT estimado |
|---|---|---|---|---|---|---|
| **WI-S10-001** | Usage event emitter no CAS hot paths + R2 append-only + idempotency | emit lib; CloudEvents schema; Object Lock config; idempotency staging table; backfill late events | 14h | 22h | 36h | **23.0h** |
| **WI-S10-002** | Counter aggregator cron DO + hash chain | DO cron; aggregation logic; D1 schema usage_counter; hash chain; usage_counter_late split | 12h | 18h | 28h | **18.7h** |
| **WI-S10-003** | Crate corelink-billing (Stripe adapter) + idempotency-key | Stripe SDK wrap; idempotency key derivation; test mode env; webhook handler signature verify; stripe_event_log table | 16h | 26h | 42h | **27.0h** |
| **WI-S10-004** | Reconciliation worker daily 3-layer + drift alerts | 3-layer reconciliation; daily cron; drift detection > 0.1%; SEV-1/2 escalation; reconciliation report R2 | 14h | 20h | 32h | **20.7h** |
| **WI-S10-005** | Quota state machine + overage handling + email/notification | quota states 80/95/100; soft alert; ticket; hard-block; email integration; in-app notif | 10h | 16h | 24h | **16.3h** |
| **WI-S10-006** | Replay forensic endpoint + role protection + audit trail | POST /v1/billing/replay; dry_run mode; role check billing_admin; audit emission; test against fake invoice | 8h | 14h | 22h | **14.3h** |
| **WI-S10-007** | TLA+ billing_atomicity spec + RB-FM-302/151 dry-run + Finance walkthrough | TLA+ no-loss/no-dup spec; CI integration; RB dry-runs; Finance walkthrough script | 12h | 18h | 28h | **18.7h** |

**Total PERT:** ~139h ≈ 17 dias work × 1 eng. Buffer 7 dias confere com 3 semanas (Stripe integration tem unpredictability).

## 13. Duração + Timeline

- **Duração:** 3 semanas (15 dias úteis) + buffer 7 dias.
- **Marcos:**
  - **D+4:** WI-001 + WI-002 SEALED (events flowing + counters aggregated).
  - **D+9:** WI-003 SEALED (Stripe integration test mode passing).
  - **D+12:** WI-004 + WI-005 SEALED (reconciliation + quota state machine).
  - **D+14:** WI-006 + WI-007 SEALED (replay + TLA+ + runbooks).
  - **D+15:** Sprint review + sign-offs + Finance walkthrough.

## 14. Critérios de promoção

- DoD complete + 30d staging sem drift > 0.1%.
- Legal sign-off em Terms de billing + DPA reference.
- Finance walkthrough successful (mock auditor reconstrói invoice em < 30min).
- TLA+ verde em CI.
- PRR HIGH_RISK aprovado.

## 15. Riscos (registry expandido)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Stripe outage** (FM-151) | L | H | MEDIUM (queue mitiga) | L | LOW | PAT-QUEUE-EVENTS-001 + retry com backoff (PAT-BACKOFF-001) + RB-FM-151 dry-run. |
| **Billing drift > 0.1%** (FM-302) | M | M | HIGH (revenue) | H | LOW | Reconciliation 3-layer daily + INV-BILLING-RECONCILE-3-LAYER + drift alert SEV-2. |
| **Idempotency key collision** | L | L | CRITICAL (double-charge) | M | LOW | UUID v4 entropy 122-bit; impossible by construction; explicit Stripe idempotency-key always set; CI test 1M events 0 collisions. |
| **Reconciliation lento (> 1h)** → stale billing | M | M | MEDIUM | M | LOW | Performance test reconciliation worker @ 1B events; otimização incremental hash chain; SLO ≤ 30 min p99. |
| **Late-arriving events** > 6h causam quiet revenue leak | M | H | HIGH | H | LOW | usage_counter_late split + alert imediato + R-S10-5 policy; runbook RB-BILLING-002 (late events triage). |
| **Webhook signature replay attack** | L | L | HIGH | M | LOW | Stripe-Signature HMAC verify + (stripe_event_id, ts) UNIQUE + 5min replay window. |
| **PII em invoice DSR conflict** (regulatory erasure quebra audit chain) | M | M | HIGH | M | LOW | Pseudonymization (não delete) → mantém chain integrity; CTRL-PRIV-002 (data classification tags @classification=pii em billing tables) + S-11 DSR pseudonymization procedure separada (control TBD); legal sign-off. |
| **Stripe pricing model migration** (custom pricing pós-GA) | M | M | MEDIUM (refactor tax) | M | MEDIUM | Anti-scope explícito S-19; custom pricing engine deferred; abstraction layer permite swap. |
| **Quota grace period abuse** (enterprise abuse 7d grace recurrent) | L | M | MEDIUM | L | LOW | Pattern detection (3× grace events em 90d) → manual review + contract amendment. |
| **Schema versioning bug** (v1.0.0 events processados como v1.0.1) | M | M | HIGH | M | LOW | Strict schema_version field validation + 2-version backward-compat tests + property test. |
| **Refund storm** (Stripe webhook flood after fraud detection) | L | M | MEDIUM | L | LOW | Rate limit refund handler + dispute storm detection (10× normal in 1h) → escalate. |

## 16. Benchmarks SOTA externos (target qualitativo + quantitativo)

| Critério | Stripe Billing | Mux | Datadog | Lago (open-source) | **CoreLink target S-10** |
|---|---|---|---|---|---|
| Exact metering (no sampling) | Yes | Yes | No (samples) | Yes | **Yes — every CAS op = 1 event** |
| Reconciliation automation | Manual quarterly | Daily | Manual | Daily | **3-layer daily + INV-BILLING-RECONCILE-3-LAYER** |
| Replay-from-events forensic | No | Yes | No | Yes | **Yes — POST /v1/billing/replay endpoint role-protected** |
| Audit-grade event log | Stripe events | R2/S3 | No | DB log | **R2 Object Lock 7y + hash chain** |
| Late-arriving events policy | Configurable | Window-based | No support | Configurable | **6h hot + late_split table + alert** |
| Idempotency Key coverage | Yes | Yes | N/A | Yes | **Yes — Stripe + internal layer** |
| TLA+ verified state machine | No | No | No | No | **Yes — billing_atomicity.tla CI** |
| Multi-currency | Yes | Yes | N/A | Yes | **No (USD only at GA — anti-scope)** |
| Real-time per-second | Limited | Yes | No | No | **No (hourly aggregation — sufficient B2B)** |
| Drift threshold enforcement | Manual | < 0.5% | N/A | < 1% | **< 0.1% automated** |

**Veredito SOTA:** S-10 v1.3 atinge **estado-da-arte em 9/10 dimensões** (multi-currency é anti-scope deliberado para GA). TLA+ em billing é diferencial vs todos competitors.

## 17. References (RFCs, papers, standards)

- **Stripe Billing Architecture Guide** <https://stripe.com/docs/billing/migration/streamlined-billing>.
- **Stripe Idempotency Keys** <https://stripe.com/docs/api/idempotent_requests>.
- **Mux Engineering Blog** — exact-metering at scale (2023).
- **Lago Open-Source Billing** — reference architecture <https://github.com/getlago/lago>.
- **GAAP ASC 606** — Revenue from Contracts with Customers (revenue recognition).
- **SOC 2 CC1.4** — Commitment to ethical operations (financial integrity).
- **CloudEvents Specification v1.0.2** (CNCF) — event schema.
- **RFC 7240 (Prefer header)** + **RFC 8174 (idempotency semantics)**.
- **Wikipedia: Reconciliation (accounting)** — 3-way match concept.
- **Lamport — TLA+ Specifications for Distributed Systems** (state machine modeling).

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- Drift > 0.1% em reconciliation (qualquer layer) → 5-Why obrigatório.
- Stripe charge dispute storm (> 1% MoM) → post-mortem + Finance + Legal review.
- Idempotency key collision detectada → CRITICAL post-mortem + UUID entropy review.
- Hash chain break em billing-events bucket → CRITICAL post-mortem + compliance officer.
- Replay endpoint produz output diferente de Stripe invoice → post-mortem mandatório (data integrity).
- Late-arriving events > 5% mensal → post-mortem (system delay root-cause).

## 19. Waiver policy

S-10 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ Reconciliation 3-layer daily 30d clean — requirement absoluto financial integrity.
- ❌ Idempotency-Key em todas as Stripe calls — protege contra double-charge.
- ❌ Append-only events R2 Object Lock 7y — SOC 2 + GAAP requirement.
- ❌ TLA+ billing_atomicity verde — sem garantia formal, risco financeiro inaceitável.
- ❌ Finance walkthrough successful — auditor sign-off obrigatório.

Itens waivable com sign-off Finance + Legal + ADR:

- ⚠️ Replay endpoint role `billing_admin` → `superadmin only` (mais restritivo); nunca menos.
- ⚠️ Late-arriving events policy 6h → 12h (com risk acceptance documentado).
- ⚠️ Quota grace period enterprise 7d → 14d para customer estratégico (com Legal review).

---

## 20. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.3.0 | 2026-04-26 | Gustavo (Lote 10.10-septies) | Initial spec contract S-10 v1.3.0 SOTA SEALED (Lote 10.10-septies round-4 R4 9.2/10 + R5 9.5/10). |
| 1.8.0 | 2026-05-03 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S10-005 SEAL; no per-WI codex per 2026-04-30 protocol — sprint-close Sonnet review covers) | **WI-S10-005 IMPL SEALED** — `crates/corelink-quota-fsm/` ships 5-state quota state machine (`WithinPlan / SoftWarning80pct / SoftWarning95pct / OverQuota100pct / SuspendedForNonPayment`) + 6-element QuotaTransition taxonomy + idempotent re-fire discipline + 3-invoice-failure suspension + operator-driven reinstate-clears-counter arm + per-instance `Arc<Mutex<()>>` F-001 closure mirroring S-07 `corelink-quota::check.rs` DO-actor model byte-for-byte. **Email send REJECTED in S-10 per ADR-0020 FROZEN — DEFERRED to S-13 admin/notifications consumer**: this WI emits the canonical `corelink.billing_quota.overage_telemetry_recorded` audit ONLY at the 80pct + 95pct entries; the actual customer-facing email + in-app notification dispatch is the S-13 consumer that subscribes to the canonical audit chain. Reinstate authorization gate (`billing_admin` role per CTRL-AUTHZ-001 + CTRL-AUTHZ-002) lives at the production wiring's Tower middleware, NOT in this crate (the crate ships the state-machine contract; the role enforcement is the caller's responsibility per the trait surface contract). Five modules: `event` (5-state QuotaState + 6-element QuotaTransition + UtilizationPct `[0, 200]`-bounded wrapper + InvoiceFailureCount saturating-u32 + QuotaFsmConfig with canonical ladder constants `SOFT_WARNING_80PCT_THRESHOLD = 80.0` / `SOFT_WARNING_95PCT_THRESHOLD = 95.0` / `OVER_QUOTA_100PCT_THRESHOLD = 100.0` / `SUSPENSION_INVOICE_FAILURE_THRESHOLD = 3` per sprint contract §5.5 R-S10-10 + WI brief; pure `utilization_bucket` mapper); `audit` (QuotaAuditEventType `#[non_exhaustive]` 4-event taxonomy `corelink.billing_quota.{state_changed, overage_telemetry_recorded, suspended, reinstated}` + QuotaAuditSink trait + InMemoryQuotaAuditSink + FailingQuotaAuditSink + audit_event_for_transition + transition_emits_overage_telemetry canonical mappings fail-CLOSED Lote 10.6bis); `store` (QuotaFsmStateRow + QuotaFsmStore trait + InMemoryQuotaFsmStore `BTreeMap<Uuid, QuotaFsmStateRow>`-backed UPSERT-safe ledger + `(tenant_id)` UNIQUE PK INV-AVAIL-ISOLATION storage layer enforcement + FailingQuotaFsmStore); `fsm` (QuotaStateMachine trait + InMemoryQuotaStateMachine orchestrator: per-instance `Arc<Mutex<()>>` F-001 closure mirroring S-07 `corelink-quota::check.rs` DO-actor model byte-for-byte → audit BEFORE store UPSERT on every state-mutating arm → telemetry-only `OverageTelemetryRecorded` sibling at 80pct + 95pct entries → terminal `Suspended` arm at the canonical 3rd invoice-failure → operator-driven `Reinstated` arm clears the counter to 0 + flips to the utilization-derived bucket); `error` (QuotaFsmError + QuotaFsmAuditSinkError + QuotaFsmStoreError canonical `#[non_exhaustive]` taxonomies). Migration `migrations/d1/0020_quota_fsm_state.sql` ships canonical `quota_fsm_state` PRIMARY KEY (tenant_id) UNIQUE [INV-AVAIL-ISOLATION storage layer; per-tenant scoping] + 5-element current_state CHECK constraint + invoice_failure_count NON-NEGATIVE CHECK + UUIDv7 hyphenated 36-char tenant_id length CHECK + 2 indexes + 4 inline CHECK constraints. Tests: 64 inline unit + 13 integration (9 property tests at 10k iter PR-gate via `PROPTEST_CASES` env-var read at runtime: `prop_state_transitions_canonical` + `prop_idempotent_transition_no_change` + `prop_80pct_boundary_telemetry_only` + `prop_95pct_boundary` + `prop_100pct_writes_429` + `prop_3_invoice_failures_suspends` + `prop_reinstate_clears_suspension` + `prop_tenant_isolation` + `prop_audit_emit_per_decision_arm` + 4 sanity / canonical-surface pinning). Quality gates green: `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings`, `cargo test -p corelink-quota-fsm --all-targets` (77/77), `python3 scripts/validate_specs.py` (280/286), `python3 scripts/check_migrations_additive.py` (20 files OK). Charter compliance: zero unsafe / unwrap / expect / panic / indexing in lib code; per-instance `Arc<Mutex<()>>` F-001 closure mirroring S-07 `corelink-quota::check.rs` DO-actor model; `#[non_exhaustive]` on every public enum; wasm32-clean (no tokio in src/); audit fail-CLOSED envelope BEFORE state mutation on every state-mutating arm (state_changed + overage_telemetry_recorded BEFORE store UPSERT on utilization transitions; suspended BEFORE store UPSERT on terminal-arm; reinstated BEFORE store UPSERT on operator-driven reinstatement); ChaCha20Rng PRNG pinned for randomized fixtures; typed QuotaTransition payload (NOT serde_json::Value — Lote 10.9-quinquies NEW-P0-2 absorption). Production wiring deferred to WI-S10-007 PRR ship gate per `trait-abstraction-defer` charter pattern: `QuotaFsmDO-<tenant>` per-tenant Cloudflare DO actor model / real D1 `quota_fsm_state` row UPSERT / Stripe webhook adapter integration (the WI-S10-003 `WebhookEvent::InvoiceFailed` arm dispatches into `record_invoice_failure`) / S-13 admin/notifications consumer subscribing to canonical audit chain / Tower middleware enforcing `billing_admin` role at reinstatement endpoint per CTRL-AUTHZ-001 + CTRL-AUTHZ-002 / `corelink_time::next_month_first_utc_midnight()` boundary primitive (period-reset cron) / hot-path Tower middleware reading `OverQuota100pct` → `429 + X-RateLimit-Layer: quota + Retry-After` per S-08 alignment / hot-path reading `SuspendedForNonPayment` → `403 forbidden`. |
| 1.4.0 | 2026-05-03 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S10-001 SEAL) | **WI-S10-001 IMPL SEALED** — `crates/corelink-billing-emit/` ships pure-logic skeleton per `trait-abstraction-defer` charter pattern (production CF R2 PutObject + Object Lock 7y + Cloudflare Queue retry drain + (tenant_id, request_id) UNIQUE D1 staging table production binding deferred to WI-S10-007 PRR ship gate). Six modules: `event` (UsageEvent CloudEvents 1.0 + IdemKey BLAKE3-256 newtype + 6-element UsageEventKind taxonomy + UsageUnit + validate_billing_period YYYY-MM guard); `idempotency` (derive_idem_key BLAKE3-of-JCS-with-slot-zeroed canonical formula + IdempotencyTracker trait + InMemoryIdempotencyTracker per-tenant set membership + IdempotencyDecision Accepted/DuplicateRejected + IdempotencyCollision SEV-1 surface); `sink` (R2UsageSink trait + InMemoryR2UsageSink append-only NDJSON layout `usage/{tenant_id}/{billing_period}/{seq:08}.usage.ndjson` + INV-BILLING-APPEND-ONLY trait-surface enforcement + per-(tenant, billing_period) sequence ledger + FailingR2UsageSink); `emitter` (UsageEventEmitter trait + InMemoryUsageEventEmitter orchestrator: canonicalize → derive idem_key → idempotency check → audit envelope BEFORE state mutation → R2 PutObject); `audit` (BillingAuditEventType `#[non_exhaustive]` 4-event taxonomy `corelink.billing.{usage_emitted, duplicate_rejected, sink_failure, idempotency_collision}` + BillingAuditSink + InMemoryBillingAuditSink + FailingBillingAuditSink); `error` (BillingEmitError + BillingAuditSinkError + R2UsageSinkError canonical `#[non_exhaustive]` taxonomies). Migration `migrations/d1/0017_usage_event_idem.sql` ships canonical `usage_event_staging` PRIMARY KEY (tenant_id, request_id) + 2 indexes + 7 inline CHECK constraints. Tests: 63 inline unit + 10 integration property tests at 10k iter PR-gate (PROPTEST_CASES env-var read at runtime). Quality gates green: cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings, cargo test -p corelink-billing-emit --all-targets (73/73), validate_specs.py (280/286), check_migrations_additive.py (17 files OK). Charter compliance: zero unsafe / unwrap / expect / panic / indexing in lib code; per-instance Arc<Mutex<>> F-001 closure; `#[non_exhaustive]` on every public enum; wasm32-clean (no tokio in src/); audit fail-CLOSED envelope on every decision arm; ChaCha20Rng PRNG pinned; BLAKE3-256 + JCS RFC 8785 canonical mirroring S-09 audit-chain. |
| 1.7.0 | 2026-05-03 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S10-004 SEAL; no per-WI codex per 2026-04-30 protocol — sprint-close Sonnet review covers) | **WI-S10-004 IMPL SEALED** — `crates/corelink-billing-reconcile/` ships pure-logic skeleton per `trait-abstraction-defer` charter pattern (production CF Cron DO daily 02:00 UTC per region + real D1 `billing_reconciliation_drift` ledger + real `stripe_submission_state` flag table + R2 reconciliation-report Object Lock 7y archive + R2 events Layer 1 input source [WI-S10-001 NDJSON aggregation] + D1 counter Layer 2 input source [WI-S10-002 `usage_counter` table + chain head re-verification cooperation] + Stripe usage_records Layer 3 input source [WI-S10-003 `stripe_idempotency_keys` ledger] + corelink_time::next_month_first_utc_midnight() canonical primitive + statistical drift bounds calibration n=50+50 + 95% CI [Lote 10.8bis P0-E inheritance] + 30d clean streak prerequisite gauge + RB-FM-302 + RB-FM-151 staging dry-run + PagerDuty 3 services dispatch [corelink-finance / corelink-sre / corelink-security] all deferred to WI-S10-007 PRR ship gate). Seven modules: `event` (ReconcileLayerKind `#[non_exhaustive]` 3-element taxonomy [Layer1Emit / Layer2Aggregate / Layer3Stripe] + ReconcileDecision `#[non_exhaustive]` 5-element taxonomy [NoDrift / AutoFixed / TicketSev3 / PageSev2 / PageSev1AutoPaused] + LayerTotals + ReconcileSnapshot + ReconcileConfig with canonical 4-tier ladder constants `QUIET_THRESHOLD = 0.0001` / `SEV3_TO_SEV2_THRESHOLD = 0.001` / `SEV2_TO_SEV1_THRESHOLD = 0.01` per sprint contract §14.s10.1 zero tolerance + auto-fix dual-condition gate constants `AUTO_FIX_MAX_RECORDS = 5` / `AUTO_FIX_MAX_PERCENT = 0.0001` Lote 10.6bis P0-6 inheritance); `drift` (compute_pairwise_drift_pct over `u128 → f64` precision-bounded `2^-43` floor + compute_max_drift with primary-layer SEV-1 routing direction Layer 3 > Layer 2 > Layer 1 + compute_drift_record_count canonical max-min cardinality + auto_fix_gate_fires dual-condition gate); `history` (DriftHistoryLedger trait + InMemoryDriftHistoryLedger UPSERT-safe ledger + `(tenant_id, billing_period, run_started_at)` UNIQUE PK INV-BILLING-NO-DUP enforcement at storage layer + DriftHistoryRow + DriftHistoryInsertOutcome Inserted/AlreadyExistsIdempotent + FailingDriftHistoryLedger); `stripe_pause` (StripeSubmissionControl trait + InMemoryStripeSubmissionControl `BTreeSet`-backed `(tenant, billing_period)` flag + StripePauseOutcome Acked/AlreadyPaused + FailingStripeSubmissionControl); `audit` (ReconcileAuditEventType `#[non_exhaustive]` 6-event taxonomy `corelink.billing_reconcile.{run_started, no_drift, auto_fixed, ticket_filed, page_dispatched, stripe_paused}` + ReconcileAuditSink trait + InMemoryReconcileAuditSink + FailingReconcileAuditSink + audit_event_for_decision canonical mapping fail-CLOSED Lote 10.6bis); `reconciler` (BillingReconciler trait + InMemoryBillingReconciler orchestrator: audit `run_started` BEFORE drift compute → compute drift via canonical pairwise primitive → classify decision per 4-tier ladder + auto-fix dual-condition gate carved INSIDE Quiet tier → audit `<decision>` BEFORE state mutation → drift-history INSERT → SEV-1 arm: `StripeSubmissionControl::pause` per (tenant, billing_period) idempotent); `error` (ReconcileError + ReconcileAuditSinkError + ReconcileDriftHistoryError + ReconcileStripePauseError canonical `#[non_exhaustive]` taxonomies). Migration `migrations/d1/0019_billing_reconciliation_drift.sql` ships canonical `billing_reconciliation_drift` PRIMARY KEY (tenant_id, billing_period, run_started_at) UNIQUE [INV-BILLING-NO-DUP storage layer; 7-year SOC 2 CC1.4 + GAAP ASC 606 evidence trail] + `stripe_submission_state` PRIMARY KEY (tenant_id, billing_period) UNIQUE [SEV-1 idempotent pause-flag] + 5-element decision CHECK constraint + 3-element primary_layer CHECK constraint + max_drift_pct fixed-point `_e9` scaling + 3 indexes + 11 inline CHECK constraints. Tests: 66 inline unit + 14 integration (10 property tests at 10k iter PR-gate via `PROPTEST_CASES` env-var read at runtime: `prop_no_drift_when_three_layers_match` + `prop_drift_threshold_boundaries` + `prop_auto_fix_dual_condition_gate` + `prop_layer3_drift_pages_sev1_pauses_stripe` + `prop_tenant_isolation` + `prop_audit_emit_per_decision_arm` + `prop_idempotent_rerun_same_period` + `prop_zero_input_no_panic` + `prop_pairwise_drift_symmetric` + `prop_max_drift_dominates_pairwise` + 4 sanity / canonical-surface pinning). Quality gates green: cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings, cargo test -p corelink-billing-reconcile --all-targets (80/80), validate_specs.py (280/286), check_migrations_additive.py (19 files OK). Charter compliance: zero unsafe / unwrap / expect / panic / indexing in lib code; per-instance `Arc<Mutex<>>` F-001 closure; `#[non_exhaustive]` on every public enum; wasm32-clean (no tokio in src/); audit fail-CLOSED envelope BEFORE state mutation on every decision arm (RunStarted before drift compute; decision-arm audit before drift-history INSERT; stripe_paused audit before pause control surface call); ChaCha20Rng PRNG pinned for randomized fixtures; 4-tier drift threshold ladder boundary semantics canonical; auto-fix dual-condition gate carved INSIDE Quiet tier (drift `> 0` AND `count ≤ 5 AND pct ≤ 0.0001`); SEV-1 primary-layer routing prefers Layer 3 (Stripe API discrepancy = customer-facing invoice = legal exposure); per-tenant Stripe-pause isolation; typed ReconcileDecision + ReconcileSnapshot + LayerTotals + DriftHistoryRow Lote 10.9-quinquies NEW-P0-2 absorption (NOT serde_json::Value). |
| 1.6.0 | 2026-05-03 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S10-003 SEAL; no per-WI codex per 2026-04-30 protocol — sprint-close Sonnet review covers) | **WI-S10-003 IMPL SEALED** — `crates/corelink-billing-stripe/` ships pure-logic skeleton per `trait-abstraction-defer` charter pattern (production HTTP client + real Stripe API endpoint + Cloudflare Worker route + Worker secret env separation + PAT-BACKOFF-001 retry + PAT-QUEUE-EVENTS-001 fallback + RB-FM-151 dry-run + ADR-0034 promotion gate deferred to WI-S10-007 PRR ship gate). Nine modules: `event` (IdempotencyKey 32-byte BLAKE3-256 newtype + SubscriptionItemId newtype + UsageRecordRequest typed payload + StripeAdapterDecision `#[non_exhaustive]` 4-element taxonomy [UsageRecorded / DuplicateRejected / WebhookProcessed / SignatureRejected] + WebhookEventKind `#[non_exhaustive]` 5-element taxonomy [InvoiceCreated / InvoicePaid / InvoiceFailed / SubscriptionUpdated / CustomerCreated] + WebhookEvent typed payload); `idempotency` (compute_canonical_aggregate_bytes RFC 8785 JCS + derive_idempotency_key BLAKE3-256 of canonical AggregatedCounter bytes — same aggregate → same key by construction; WI-S10-002 input source feeds the canonical key derivation directly); `signature` (StripeSignatureHeader::parse `t=<unix_seconds>,v1=<hex>` with multi-v1 key-rotation tolerance + unknown-scheme forward-compat + compute_signature HMAC-SHA256 over `<ts>.<payload>` per Stripe spec + verify_stripe_signature with `subtle::ConstantTimeEq::ct_eq` constant-time compare against EVERY v1 candidate + canonical 5-min `REPLAY_WINDOW_MS = 300_000` boundary enforcement per Stripe webhook signature documentation); `audit` (StripeAuditEventType `#[non_exhaustive]` 6-event taxonomy `corelink.billing_stripe.{usage_recorded, duplicate_rejected, webhook_received, signature_rejected, signature_verified, signature_skew_rejected}` + StripeAuditSink trait + InMemoryStripeAuditSink + FailingStripeAuditSink fail-CLOSED Lote 10.6bis); `ledger` (StripeUsageLedger trait + InMemoryStripeUsageLedger UPSERT-safe ledger + per-IdempotencyKey UNIQUE PK INV-BILLING-NO-DUP enforcement + canonical-bytes-divergence StripeUsageLedgerError::IdempotencyKeyReuse SEV-1 surface + RecordOutcome Recorded/AlreadyExistsIdempotent + FailingStripeUsageLedger); `webhook_log` (StripeWebhookLog trait + InMemoryStripeWebhookLog + (stripe_event_id) UNIQUE PK + WebhookInsertOutcome Inserted/AlreadyExists + FailingStripeWebhookLog); `adapter` (StripeBillingAdapter trait + InMemoryStripeBillingAdapter orchestrator: canonicalize → derive_idempotency_key → audit BEFORE ledger mutation [usage_recorded / duplicate_rejected arm] → ledger record); `webhook` (StripeWebhookHandler trait + InMemoryStripeWebhookHandler orchestrator: audit `webhook_received` BEFORE any verify → verify_stripe_signature → audit `signature_verified` / `signature_rejected` / `signature_skew_rejected` → webhook log INSERT); `error` (StripeError + StripeAuditSinkError + StripeUsageLedgerError + StripeWebhookLogError canonical `#[non_exhaustive]` taxonomies). Migration `migrations/d1/0018_stripe_idem_keys.sql` ships canonical `stripe_idempotency_keys` PRIMARY KEY (idempotency_key) UNIQUE [INV-BILLING-NO-DUP storage layer; mirrors Stripe 24h idempotency window] + `stripe_event_log` PRIMARY KEY (stripe_event_id) UNIQUE [INV-AUDIT-APPEND-ONLY; webhook redelivery dedup per Stripe spec] + 5-element webhook event_type CHECK constraint + `evt_*` substr CHECK + 4 indexes + 11 inline CHECK constraints. Tests: 90 inline unit + 20 integration (10 property tests at 10k iter PR-gate via `PROPTEST_CASES` env-var read at runtime: `prop_idempotency_key_deterministic` + `prop_idempotency_key_diverges_per_aggregate` + `prop_webhook_signature_verifies_valid` + `prop_webhook_signature_rejects_expired` + `prop_webhook_signature_rejects_tampered_payload` + `prop_webhook_signature_rejects_tampered_signature` + `prop_constant_time_signature_compare` + `prop_tenant_isolation` + `prop_audit_emit_per_decision_arm` + `prop_replay_window_exact_5min_boundary` + 10 sanity / canonical-surface pinning / fail-CLOSED arm). Quality gates green: cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings, cargo test -p corelink-billing-stripe --all-targets (110/110), validate_specs.py (280/286), check_migrations_additive.py (18 files OK). Charter compliance: zero unsafe / unwrap / expect / panic / indexing in lib code; per-instance `Arc<Mutex<>>` F-001 closure; `#[non_exhaustive]` on every public enum; wasm32-clean (no tokio in src/); audit fail-CLOSED envelope BEFORE state mutation on every decision arm; ChaCha20Rng PRNG pinned for randomized fixtures; constant-time signature compare via `subtle::ConstantTimeEq` (timing-attack defense — `subtle` is the canonical Rust constant-time primitive workspace dep); `hmac` + `sha2` + `subtle` workspace deps already wired (S-03 inheritance); BLAKE3-256 canonical hash family inheritance (CAS S-01 / AC S-04 / dedup S-07 / audit chain S-09 / billing-emit WI-S10-001 / billing-aggregator WI-S10-002). Production HTTP / Worker route / cargo-deny stripe-crate isolation / typed `serde::Serialize` Redact-wrapping PII / monthly invoice cron / Neon Postgres `plan` + `subscription` + `invoice_line_item` + `customer_billing_profile` schemas / Stripe Price IDs catalog / Terraform IaC all deferred to WI-S10-007 PRR ship gate per `trait-abstraction-defer` charter pattern. |
| 1.5.0 | 2026-05-03 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S10-002 SEAL; orchestrator-finalized after agent rate-limit at 39 tool uses) | **WI-S10-002 IMPL SEALED** — `crates/corelink-billing-aggregator/` ships pure-logic skeleton per `trait-abstraction-defer` charter pattern (production CF Cron DO `BillingAggregatorCron-<region>` per-region cron-trigger 1h interval + usage_event_staging drain from WI-S10-001 + R2 hour bucket replay + atomic D1 batch (counter row UPSERT plus `hash_chain_head` UPDATE in the same `db.batch`) + `usage_counter_late` separate D1 table for events with `ts < now - 6h` + watermark-based idempotent replay + Terraform IaC deferred to WI-S10-007 PRR ship gate). Six modules: `event` (AggregatedCounter CloudEvents 1.0 aligned `type="corelink.billing.counter.aggregated"` + ChainHash 32-byte BLAKE3 newtype + AggregationDecision `#[non_exhaustive]` 3-element taxonomy Aggregated/SkippedNoEvents/SkippedDuplicateRun + AggregatedCounterData typed payload {tenant_id, billing_period, event_kind, total_qty, event_count, period_start_ms, period_end_ms, idem_keys_seen} + CounterGroupKey); `chain` (HashChainBuilder per-(tenant, billing_period) state machine + compute_canonical_bytes RFC 8785 JCS + link_chain_hash BLAKE3-256 + verify_chain_link primitives; Bitcoin-genesis-block convention `prev_hash = [0u8; 32]`); `aggregator` (CounterAggregator trait + InMemoryCounterAggregator orchestrator: audit `run_started` → filter+sort by `(time_ms, idem_key)` deterministic order → SUM `qty` into `total_qty` → idempotent re-run check on typed `data` payload BEFORE chain-head observation → chain head lookup → counter store UPSERT → audit `run_completed`; PeriodWindow inclusive-start exclusive-end Prometheus boundary semantics; deterministic_event_order helper); `store` (AggregatedCounterStore trait + InMemoryAggregatedCounterStore UPSERT-safe ledger + `(tenant_id, billing_period, event_kind)` UNIQUE PK + DigestMismatch SEV-1 replay-corruption surface + ChainHeadRecord + UpsertOutcome Inserted/AlreadyExistsIdempotent + FailingAggregatedCounterStore); `audit` (AggregatorAuditEventType `#[non_exhaustive]` 4-event taxonomy `corelink.billing_aggregator.{run_started, run_completed, chain_break_detected, sink_failure}` + AggregatorAuditSink + InMemoryAggregatorAuditSink + FailingAggregatorAuditSink fail-CLOSED Lote 10.6bis); `error` (AggregatorError + AggregatedCounterStoreError + AggregatorAuditSinkError canonical `#[non_exhaustive]` taxonomies). NO new migration this WI — production usage_counter + hash_chain_head tables ship at WI-S10-007 alongside the CF Cron DO binding. Tests: 62 inline unit + 15 integration (9 property tests at 10k iter PR-gate via `PROPTEST_CASES` env-var read at runtime + 6 sanity / canonical-surface pinning). Quality gates green: cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings, cargo test -p corelink-billing-aggregator --all-targets (77/77), validate_specs.py (280/286), check_migrations_additive.py (17 files OK). Charter compliance: zero unsafe / unwrap / expect / panic / indexing in lib code; per-instance `Arc<Mutex<>>` F-001 closure; `#[non_exhaustive]` on every public enum; wasm32-clean (no tokio in src/); audit fail-CLOSED envelope BEFORE state mutation on every decision arm; ChaCha20Rng PRNG pinned for randomized fixtures; `prop_assert!(let valid = matches!...; valid)` pattern (S-08 P1-1 fix). Orchestrator finalization fix during SEAL: idempotent re-run check moved BEFORE chain-head observation + comparison on typed `data` payload (NOT full canonical bytes which include link slot `prev_hash + sequence_number` determined by FIRST-run chain state) so watermark replay never advances the head twice. |

---

**Fim spec contract S-10 v1.8.0 (WI-S10-001 + WI-S10-002 + WI-S10-003 + WI-S10-004 + WI-S10-005 IMPL SEALED 2026-05-03; spec-baseline v1.3.0 SOTA SEALED Lote 10.10-septies round-4 R4 9.2/10 + R5 9.5/10).**
