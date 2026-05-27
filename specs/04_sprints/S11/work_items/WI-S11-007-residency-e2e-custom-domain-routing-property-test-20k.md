---
id: "WI-S11-007"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.3.0"
created: "2026-04-26"
updated: "2026-05-27"
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
  - "DATA-MODEL"
  - "STORAGE-SEMANTICS-MATRIX"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
tags: ["wi", "s11", "residency", "data-residency", "schrems-ii", "lgpd-art-33", "gdpr-art-44", "custom-domain-routing", "20k-property-test", "fm-451", "high-risk"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-privacy-residency-enforcement` was absorbed into `corelink-privacy` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-privacy-absorption.md. Canonical consumer path is now `corelink_privacy::*`.

# WI-S11-007 — Residency Pinning E2E + Custom Domain Routing 6-Region Canonical Enum (`<tenant_id>.<region>.corelink.humangr.com` per `data_model.md §2.1` + `privacy_model.md §7.1` — wnam/enam/weur/sam/apac/afr) + Insert Checks Reject Cross-Region Writes (D1 trigger + Worker pre-flight assertion) + 20k Property Test (10k weur + 10k enam) + RB-DATA-RESIDENCY-LEAK Runbook + FM-451 Declaration + INV-DATA-RESIDENCY CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL Schrems II) §3.11 Runtime Cobertura (`crates/corelink-privacy-residency-enforcement`; tenant.primary_region canonical column em `data_model.md §4.1` L151 — NOT tenant_metadata.region_pinned legacy; Worker routing via custom domain mapping; insert checks via D1 CHECK constraint + worker pre-flight; 20k property test 10k weur + 10k enam → 0 cross-region leaks; RB-DATA-RESIDENCY-LEAK runbook NEW; FM-451 NEW declarado em failure_modes.md; TLA+ scope split (Lote 10.11.0-bis-prime cycle 3 reconciled): PARTIAL S-11 via dsr_erasure_atomicity.tla (InvResidencyPinned + temporal InvResidencyMonotonic — pinning + monotonic); FULL S-14 via region_residency.tla (cross-region routing actions) per invariant_registry.md §4.2 — S-11 entrega runtime + property tests + custom domain routing + TLA+ partial coverage; Schrems II + LGPD Art. 33 § 1º + GDPR Art. 44 alignment; CloudEvents `dev.hugr.corelink.residency.{request_routed,write_rejected_cross_region}.v1` 2 canonical types per Lote 10.9bis P0-G prefix em audit-`<region>` Object Lock 7y emit fail-CLOSED)

> **doc_status:** SEALED · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-11](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S11-007 |
| Título | Residency pinning E2E + custom domain routing 6-region canonical enum (`<tenant_id>.<region>.corelink.humangr.com`) + insert checks reject cross-region writes + 20k property test (10k weur + 10k enam) + RB-DATA-RESIDENCY-LEAK runbook NEW + FM-451 NEW declared em failure_modes.md; tenant.primary_region canonical column data_model.md §4.1 L151 (NOT tenant_metadata.region_pinned legacy correção sprint contract v1.2.0); INV-DATA-RESIDENCY CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL Schrems II) §3.11 L154 runtime cobertura; TLA+ scope split (Lote 10.11.0-bis-prime cycle 3; status updated Lote 10.11.0-bis-bis V2 2026-05-15): **PARTIAL em S-11** (`dsr_erasure_atomicity.tla` InvResidencyPinned + InvResidencyMonotonic) + **FULL LANDED EARLY (was deferred to S-14)** (`region_residency.tla` per invariant_registry.md §4.2 — ✅ GREEN per 2026-05-15-tla-coverage-audit §3; sprint owner S-14 preserved); 2 CloudEvents canonical `dev.hugr.corelink.residency.{request_routed,write_rejected_cross_region}.v1` per Lote 10.9bis P0-G prefix em audit-`<region>` Object Lock 7y emit fail-CLOSED |
| Sprint | S-11 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-003 (PII regulatory), FF-HR-005 (CTRL-PRIV-031), FF-HR-010 (1ª regulatory full impl) |

## 1. Intent

Residency pinning é o **legal exposure mitigation primary** — sem isso, **Schrems II** (CJEU C-311/18) + **LGPD Art. 33 § 1º** + **GDPR Art. 44** (international transfer restrictions) ficam unfulfilled. Customer EU dado em region US sem SCC valid = invalidação Privacy Shield + multa potencialmente catastrófica. CoreLink stack: 6 canonical regions per privacy_model.md §7.1 (wnam/enam/weur/sam/apac/afr); tenant escolhe `tenant.primary_region` em signup; CAS/AC/billing-events/audit data NUNCA cross-region.

S-11 entrega 4 layers (Lote 10.11.0-bis-prime cycle 3 reconciled; status cascade Lote 10.11.0-bis-bis V2 2026-05-15): (a) custom domain routing `<tenant_id>.<region>.corelink.humangr.com`; (b) insert checks reject cross-region writes via D1 CHECK constraint + worker pre-flight assertion; (c) 20k property test (10k weur + 10k enam) → 0 cross-region leaks; (d) **TLA+ partial coverage via `dsr_erasure_atomicity.tla`** (InvResidencyPinned: ticket pinning canonical; InvResidencyMonotonic temporal: no cross-region migration). **FULL TLA+ proof LANDED EARLY (was deferred to S-14)** via `region_residency.tla` — spec checked in via R-prep wave 2026-05-15 (✅ GREEN per 2026-05-15-tla-coverage-audit §3; sprint owner S-14 preserved for provenance); adds backend region dimension + cross-region routing actions + replication-eventually-converges. S-11 cobertura excede regulatory baseline + Schrems II defensibility (4-layer defense — runtime custom-domain + D1 CHECK + 20k property + TLA+ formal partial-S-11 + full-S-14-landed-early).

```rust
// File: crates/corelink-privacy-residency-enforcement/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;

#[async_trait]
pub trait ResidencyEnforcement: Send + Sync {
    /// Pre-flight assertion: verifica request routing matches tenant.primary_region.
    /// Falha → 451 `legal_residency_violation` (PAT-ROUTING-PINNED-001 fail-CLOSED canonical pós Lote 10.11.0-bis).
    async fn assert_request_residency(
        &self,
        tenant_ctx: &TenantCtx,
        request_region: Region,
    ) -> Result<(), ResidencyViolation>;

    /// Insert check: pre-flight verify backend write target = tenant.primary_region.
    /// Backend write rejected if mismatched (defensive for direct API call).
    async fn assert_write_residency(
        &self,
        tenant_ctx: &TenantCtx,
        backend: BackendKind,
        target_region: Region,
    ) -> Result<(), ResidencyViolation>;
}

/// 6 canonical regions per privacy_model.md §7.1 + data_model.md §2.1 L95.
/// Closed enum (cardinality discipline post Lote 10.10-quaters; novos regions ADR-required).
#[derive(strum::Display, strum::EnumIter, serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[strum(serialize_all = "snake_case")]
pub enum Region {
    /// Western North America (Canada-based) — PIPEDA, CCPA.
    Wnam,
    /// Eastern North America (US-based) — CCPA, HIPAA opt-in.
    Enam,
    /// Western Europe (NL/DE based) — GDPR.
    Weur,
    /// South America (BR-based; cliente-âncora HuGR) — LGPD.
    Sam,
    /// Asia-Pacific (SG-based) — PDPA SG, APPI JP. Phase 2.
    Apac,
    /// Africa (ZA-based) — POPIA. Phase 3.
    Afr,
}

/// CloudEvents `type` field; canonical prefix `dev.hugr.corelink.residency.<verb>.v1` per Lote 10.9bis P0-G.
#[derive(strum::Display, strum::EnumIter)]
pub enum ResidencyAuditEventType {
    #[strum(serialize = "dev.hugr.corelink.residency.request_routed.v1")]
    RequestRouted,
    #[strum(serialize = "dev.hugr.corelink.residency.write_rejected_cross_region.v1")]
    WriteRejectedCrossRegion,
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras)

### 2.1 Contexto

Schrems II (CJEU C-311/18, 2020-07-16) invalidou Privacy Shield + estabeleceu que cross-border transfers EU→US precisam de SCC + Transfer Impact Assessment (TIA) ad-hoc per transfer. LGPD Art. 33 § 1º proíbe transferência internacional sem garantias adequadas. GDPR Art. 44 estabelece "general principle for transfers" — qualquer transfer internacional sem adequacy decision OR SCC + TIA é violation.

CoreLink stack: 6 canonical regions per privacy_model.md §7.1. Tenant escolhe `tenant.primary_region` em signup; CAS/AC/billing-events/audit data fica pinned. Metadata global (rate limits, flags) pode replicar cross-region (pseudonymized only). Mudança de region = pedido formal + migration + cooldown 30d (privacy_model.md §7.2). Sub-processadores DPA + SCC ratus per compliance_matrix.md §7.

Sem runtime enforcement, customer EU pode acidentalmente ter blob em region US (e.g., DNS resolution miss, race condition em writes pre-routing setup). Bug catastrofic = legal exposure massivo.

### 2.2 Abordagem

Runtime layer 3-fold: (a) **Custom domain routing** Cloudflare Workers regex match `<tenant_id>.<region>.corelink.humangr.com`; cross-region requests rejected **451 `legal_residency_violation`** (PAT-ROUTING-PINNED-001 fail-CLOSED canonical resilience_patterns.md §3.4; NUNCA passthrough silencioso); (b) **Insert checks** D1 CHECK constraint em backend tables + Worker pre-flight assertion antes de R2 write; (c) **Property test 20k** tenants (10k weur + 10k enam) random ops → 0 cross-region leaks via simulated cross-region traffic injection.

`tenant.primary_region` é canonical column em data_model.md §4.1 L151 (NOT `tenant_metadata.region_pinned` — corrigido em sprint contract v1.2.0 Lote 10.11.0; legacy column nunca existiu, foi typo em sprint contract v1.1.0). Foundation para region routing layer reuse de S-09 multi-region observability.

TLA+ formal proof é **deferred a S-14** (`region_residency.tla` / `byok_sovereignty.tla` per invariant_registry.md §4.2 L445); S-11 entrega cobertura runtime + property tests + custom domain routing — sufficient para regulatory baseline + Schrems II defensibility. ADR-S11-010 documenta TLA+ deferral rationale.

### 2.3 Valor entregue

- **Schrems II + LGPD Art. 33 § 1º + GDPR Art. 44 alignment absoluto**: cross-region writes blocked at runtime.
- **CTRL-PRIV-031 satisfação**: residency pinning + bucket locationHint + DO stickiness.
- **INV-DATA-RESIDENCY CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL Schrems II) §3.11 runtime cobertura**: 20k property test 0 violations sustained CI.
- **Customer trust**: custom domain `<tenant>.weur.corelink.humangr.com` é visible signal of region pinning.
- **FM-451 mitigation**: residency leak detection via 2 CloudEvents R2 7y + alert.
- **Foundation S-14 BYOK**: residency_pinning + crypto-erase + customer-controlled key vault.

### 2.4 Principais riscos & trade-offs

- **Custom domain DNS overhead**: tenant precisa CNAME `<tenant>.weur.corelink.humangr.com`; UX friction at signup. **Mitigation**: optional para `team`+ (managed via Cloudflare Pages); free/solo use default routing transparente.
- **D1 CHECK constraint vs trigger**: CHECK constraint compile-time + zero runtime cost; trigger runtime overhead but flexible. **Decisão (DD-001)**: CHECK constraint primary; trigger fallback for complex cases (e.g., cross-table tenant_id derivation).
- **20k vs higher property test count**: 20k is sprint contract §5.7 R-S11-19 specified; higher (100k) more confidence but slower CI. **Decisão (DD-002)**: 20k baseline + 100k nightly cron property test (S-09 inheritance pattern).
- **TLA+ deferral**: cobertura runtime + property test é industry standard mas formal proof ideal; S-14 future.
- **Cross-region observability**: residency leak detection via 2 CloudEvents needs observability backbone; S-09 inheritance.

## 3. Customer Impact & Journey

### 3.1 Personas afetadas

- **Privacy-aware data subject (EU)**: dado fica em weur; Schrems II protected.
- **Tenant admin (BR)**: signup chooses sam region; data fica em BR per LGPD Art. 33 § 1º.
- **Tenant admin (US)**: signup chooses enam; CCPA + HIPAA opt-in.
- **Privacy Officer interno**: monitora `corelink_residency_violation_total{src_region,target_region}` Prom counter.
- **External auditor (Schrems II review)**: verifies 0 cross-region traffic via 2 CloudEvents R2 7y + property test em CI.

### 3.2 Customer journey (touchpoints)

1. Signup: tenant escolhe `region` ∈ canonical 6-region enum (default = sam BR; can override).
2. Tenant em D1 inserted com `primary_region = <chosen>` (data_model.md §4.1 L151).
3. Custom domain `<tenant_id>.<region>.corelink.humangr.com` provisioned (managed via Cloudflare Pages for `team`+).
4. All requests roteadas para region pinned via custom domain DNS resolution.
5. Backend writes (CAS/AC/billing-events/audit) pre-flight asserted contra tenant.primary_region.
6. Mismatch → **451 `legal_residency_violation`** (PAT-ROUTING-PINNED-001 fail-CLOSED canonical); `dev.hugr.corelink.residency.write_rejected_cross_region.v1` audit emit.
7. Privacy Officer monitor `corelink_residency_violation_total` quarterly.

### 3.3 Jornadas (User Journeys) afetadas

- **Signup** (S-13): adiciona region picker UI + canonical 6-region enum.
- **Settings** (S-13): tenant pode request region migration via support ticket (privacy_model.md §7.2 cooldown 30d).
- **Billing portal** (S-10): billing-events region tag verifiable via residency_routed_total Prom metric.

### 3.4 Métricas de customer-visible

- **Cross-region leak rate**: 0 sustained 90d (`corelink_residency_violation_total` Prom counter).
- **Custom domain routing latency**: DNS resolution + worker routing ≤ 50ms p95 (S-09 inheritance).
- **Property test pass rate**: 20k baseline 100% sustained; 100k nightly cron 100% sustained.

### 3.5 Comunicação ao customer

Email signup confirmação em locale do tenant: "Your data is pinned to <region> per LGPD/GDPR/CCPA"; explica region migration process. Privacy notice (WI-S11-004) referencia residency policy.

### 3.6 Mitigação de fricção

- **Region picker default sam BR**: cliente-âncora HuGR; minimize friction Brazilian customers.
- **Custom domain optional**: `team`+ managed; `free`/`solo` use default routing.
- **Region migration process**: 30d cooldown + Privacy Officer review; no business interruption.

### Anti-pattern ❌

❌ Region default = enam US (Schrems II exposure default for EU customers); ❌ Custom domain mandatory at signup (UX friction); ❌ Insert checks bypass via direct R2 API (defensive missing); ❌ Property test < 20k (sprint contract §5.7 R-S11-19 violation); ❌ Sem audit event (regulatory evidence gap); ❌ Cross-region writes silent (regulatory finding); ❌ Open region string (cardinality drift; ADR-S11-006 pattern).

## 4. Capability Mapping / Trace

| Camada | ID | Item |
|---|---|---|
| Sprint contract | R-S11-16 + R-S11-17 + R-S11-18 + R-S11-19 + R-S11-19a | Tenant region opt-in + custom domain routing + insert checks + 20k property test + TLA+ deferral |
| CAPs | CAP-PRIV-007 | Residency pinning E2E |
| Invariantes | INV-DATA-RESIDENCY (CRITICAL — Lote 10.11.0-bis §3.11 L154) + INV-AUDIT-APPEND-ONLY (CRITICAL §3.6 L116) |
| Controles | CTRL-PRIV-031 (residency pinning) + CTRL-ISO-001 (tenant isolation foundation) |
| Padrões | PAT-ROUTING-PINNED-001 (custom domain routing per region) |
| Failure modes | **FM-451 (NEW S-11)** residency leak (cross-region write detected); FM-105 (inter-region latency cross-region dedup) |
| Métricas | `corelink_residency_violation_total{src_region,target_region,backend}` + `corelink_residency_routed_total{region,outcome}` + `corelink_residency_property_test_failures_total` |
| Eventos | 2 CloudEvents canonical: residency.request_routed/write_rejected_cross_region |
| Evidence | EVT-002 (CI integration tests) + EVT-024 (load test 20k property) + EVT-047 (AUDIT_EVENT) |

## 5. Tipo e Classificação

### 5.1 Tipo primário

`feature/regulatory-baseline` — implementação primária residency enforcement.

### 5.2 Prioridade

**P0** — sprint contract §6 DoD bloqueante (Schrems II legal exposure se shipped sem isso).

### 5.3 Blast radius

**Cross-tenant + cross-region** — bug em routing pode causar mass cross-region leak (catastrofic).

### 5.4 Reversibilidade

Feature flag `residency_enforcement_strict` em DO config-singleton é **write-once-true** (PAT-ROUTING-PINNED-001 canonical resilience_patterns.md §3.4 pós Lote 10.11.0-bis): NÃO há "fallback to legacy routing" — flip para `false` requer dual approval Privacy Officer + Compliance + SecLead + ANPD/EDPB pre-notification + audit chain entry justificando break-glass. Region migration NOT reversible after data committed (privacy_model.md §7.2 cooldown 30d). Production é one-way regulatory commitment; dry-run reversível apenas em staging. Documented em DPA.

### 5.5 Experiment? (A/B test, feature flag experiment)

Não — regulatory feature.

### 5.6 Compliance triggers

- LGPD Art. 33 § 1º (transferência internacional).
- GDPR Art. 44 (general principle for transfers), Art. 45 (transfers on basis of adequacy decision), Art. 46 (transfers subject to appropriate safeguards).
- Schrems II (CJEU C-311/18) — Privacy Shield invalidation.
- CCPA §1798.140 (definitions; California-residency interpretation).
- SOC 2 P5.1 (data subject access; geographic scope).
- ISO/IEC 27018 §A.11 (geographic location of customer data).

## 6. Escopo

### 6.1 Em escopo (exaustivo)

1. NEW crate `crates/corelink-privacy-residency-enforcement` (assert_request_residency + assert_write_residency).
2. Region enum canonical 6-region closed (wnam/enam/weur/sam/apac/afr per privacy_model.md §7.1).
3. Worker routing layer (S-09 multi-region inheritance):
   - (a) Custom domain `<tenant_id>.<region>.corelink.humangr.com` regex match;
   - (b) Cross-region requests rejected **451 `legal_residency_violation`** (PAT-ROUTING-PINNED-001 canonical);
   - (c) `dev.hugr.corelink.residency.request_routed.v1` audit emit per request.
4. D1 CHECK constraint em backend tables (e.g., `blob_meta`, `ac_meta`, `audit_outbox`) ensuring region tag matches tenant.primary_region.
5. Worker pre-flight assertion antes de R2 writes (CAS PUT, AC PUT, audit emit, billing-events emit) — defensive.
6. 20k property test em CI (10k tenants weur + 10k tenants enam; random ops × cross-region traffic injection):
   - File: `tests/property_residency_20k.rs`;
   - Iterações: 20k baseline em CI + 100k nightly cron;
   - Assertions: 0 cross-region leaks; per-tenant primary_region preserved.
7. RB-DATA-RESIDENCY-LEAK runbook (NEW canonical) — SOP detect + remediate residency leak.
8. **FM-451 (NEW)** declaration em failure_modes.md: "Residency leak (cross-region write detected)" P0 (S=5 → upgrade FF-HR-010); RB-DATA-RESIDENCY-LEAK mapping.
9. 2 CloudEvents canonical types em audit-`<region>` Object Lock 7y; emit fail-CLOSED.
10. Region migration request endpoint POST /v1/admin/tenant/region-migration (auth via PAT + step-up MFA per CTRL-AUTH-010; reuses WI-S11-001 step-up MFA pattern):
    - Cria ticket em D1 `region_migration_request`;
    - SLA 30d cooldown per privacy_model.md §7.2;
    - Privacy Officer + Compliance review.
11. ADR-S11-010 (NEW): TLA+ residency formal proof deferral to S-14 rationale.
12. ADR-S11-011 (NEW): Region migration cooldown 30d rationale.
13. Cardinality discipline: novos regions adicionados require ADR + Privacy Officer + Compliance review (ADR-S11-006 pattern; ADR-S11-012 NEW se sprint).
14. Privacy notice (WI-S11-004) update referencia residency section.
15. Email signup template integration: 3 locales mention region pinned + Schrems II alignment.

#### 6.1.4 D1 CHECK constraint (excerpt from blob_meta)

```sql
-- Existing blob_meta (S-01 inheritance) extended com region check
ALTER TABLE blob_meta ADD COLUMN region TEXT NOT NULL CHECK (region IN ('wnam','enam','weur','sam','apac','afr'));
ALTER TABLE blob_meta ADD CONSTRAINT chk_blob_meta_region_match_tenant
  CHECK (
    region = (SELECT primary_region FROM tenant WHERE tenant_id = blob_meta.tenant_id)
  );
-- D1 NÃO suporta subquery em CHECK — fallback via trigger:

CREATE TRIGGER trg_blob_meta_region_match BEFORE INSERT OR UPDATE ON blob_meta
FOR EACH ROW
WHEN NEW.region != (SELECT primary_region FROM tenant WHERE tenant_id = NEW.tenant_id)
BEGIN
  SELECT RAISE(ABORT, 'residency_violation: blob_meta.region must match tenant.primary_region');
END;
```

#### 6.1.5 FM-451 declaration (failure_modes.md)

```
| FM-451 | Residency leak (cross-region write detected; mismatched tenant.primary_region vs backend region) | 2 | 4 | 5 | 40 | P0 (S=5 → upgrade FF-HR-010) | INV-DATA-RESIDENCY + custom domain routing + insert checks + 20k property test + RB-DATA-RESIDENCY-LEAK + alert SEV-1 |
```

### 6.2 Componentes C4 afetados

- **Worker-CP**: routing layer + assert_request_residency middleware.
- **D1**: CHECK constraints + triggers em backend tables (blob_meta, ac_meta, audit_outbox, billing-events staging).
- **R2**: bucket locationHint per region (S-01 inheritance).
- **DO**: stickiness per tenant.primary_region (S-07 inheritance pattern).
- **Cloudflare DNS**: custom domain `<tenant>.<region>.corelink.humangr.com` records.
- **R2 audit-`<region>`**: 2 canonical CloudEvents Object Lock 7y.

### 6.3 Arquivos do repositório

```
crates/corelink-privacy-residency-enforcement/  # NEW
├─ Cargo.toml
├─ src/
│  ├─ lib.rs                                    # ResidencyEnforcement trait
│  ├─ region.rs                                 # 6-region enum canonical
│  ├─ assert_request.rs                         # Worker pre-flight assertion
│  ├─ assert_write.rs                           # Backend write defensive check
│  ├─ audit_emit.rs                             # 2 CloudEvents emitter
│  └─ error.rs                                  # ResidencyViolation enum
└─ tests/
   ├─ integration_residency_routing.rs          # signup → cross-region request → 451 (PAT-ROUTING-PINNED-001)
   ├─ property_residency_20k.rs                 # 10k weur + 10k enam × random ops
   └─ regression_d1_check_constraints.rs        # CHECK + trigger fired correctly

migrations/
└─ N+5__residency_check_constraints.sql         # ALTER TABLE + CREATE TRIGGER per backend table

specs/03_architecture/adrs/
├─ ADR-S11-010-tla-residency-deferred-s14.md    # NEW
└─ ADR-S11-011-region-migration-cooldown-30d.md # NEW

specs/05_quality/runbooks/RB-DATA-RESIDENCY-LEAK.md         # NEW canonical

# Region migration endpoint (reuse corelink-privacy-dsr-api pattern)
crates/corelink-privacy-dsr-api/src/routes/region_migration.rs  # NEW route
```

### 6.4 Sistemas externos tocados

- **Cloudflare DNS** (custom domain records).
- **Cloudflare Workers** (routing + middleware).
- **D1** (CHECK + triggers em multiple tables).
- **R2** (audit-`<region>` 2 CloudEvents).
- **DO** (stickiness per region).

## 7. Anti-Scope

- ❌ DSR API endpoints — entregue em WI-S11-001 (region migration endpoint reuses pattern).
- ❌ Erasure worker — entregue em WI-S11-002.
- ❌ Consent ledger — entregue em WI-S11-003.
- ❌ Privacy notice content — entregue em WI-S11-004.
- ❌ Sub-processor register — entregue em WI-S11-005.
- ❌ Breach notification — entregue em WI-S11-006.
- ❌ DPIA + LIA + TLA+ residency — entregue em WI-S11-008 (DPIA only); TLA+ residency formal proof S-14.
- ❌ TLA+ formal proof residency — deferred S-14 per ADR-S11-010.
- ❌ BYOK customer-controlled key vault per region — S-14.
- ❌ Cross-region migration tool (data export/import) — S-13 admin plane future.
- ❌ Schrems II Transfer Impact Assessment template per customer — pós-GA enterprise expansion.
- ❌ Region expansion APAC/AFR phase 2/3 implementation — future sprints; ADR per region addition.

### Anti-pattern ❌

❌ Region default = enam US (Schrems II exposure); ❌ Custom domain mandatory (UX friction); ❌ Insert checks bypass; ❌ Property test < 20k; ❌ Sem audit event; ❌ Cross-region writes silent; ❌ Open region string.

## 8. Acceptance Criteria (Gherkin) — 8 scenarios

### AC-001: Tenant signup region opt-in canonical 6-region

```gherkin
Given um signup form em corelink.humangr.com
When tenant submete payload {region: "weur", ...} canonical 6-region enum
Then D1 INSERT INTO tenant (tenant_id, primary_region, ...) VALUES (..., 'weur', ...) succeeds
And NÃO permite valor outside canonical 6-region (CHECK constraint reject 'us'|'eu'|'apac' string drift; força wnam/enam/weur/sam/apac/afr)
And tenant.primary_region = 'weur' permanently (region migration via separate endpoint per privacy_model.md §7.2 cooldown 30d)
And custom domain `<tenant_id>.weur.corelink.humangr.com` provisioned via Cloudflare Pages
And email confirmação enviado em locale tenant explica region pinned per LGPD/GDPR/CCPA
```

### AC-002: Cross-region request rejected 451 (PAT-ROUTING-PINNED-001 fail-CLOSED canonical)

```gherkin
Given tenant_id T tem primary_region = 'weur'
When request feita a `<T>.enam.corelink.humangr.com/v1/cas/...` (cross-region; deveria ser weur)
Then Worker routing layer detecta mismatch
And response 451 com body { "error": "legal_residency_violation", "details": "Request region 'enam' does not match tenant.primary_region 'weur'", "remediation": "Use <tenant_id>.weur.corelink.humangr.com", "framework": "Schrems II + LGPD Art. 33 §1º + GDPR Art. 44" }
And `dev.hugr.corelink.residency.request_routed.v1` audit emit com payload {tenant_id, requested_region: 'enam', expected_region: 'weur', outcome: 'rejected'}
And `corelink_residency_violation_total{src_region='enam',target_region='weur',backend='request'}` Prom counter incrementado
```

### AC-003: D1 insert check rejects cross-region write

```gherkin
Given tenant_id T em D1 com primary_region = 'weur'
And attacker tenta INSERT direto via D1 API (bypass Worker pre-flight): INSERT INTO blob_meta (tenant_id, region, ...) VALUES (T, 'enam', ...)
When trigger trg_blob_meta_region_match fires
Then RAISE(ABORT, 'residency_violation: blob_meta.region must match tenant.primary_region')
And INSERT é abortado
And NO cross-region row em blob_meta
And CloudEvent `dev.hugr.corelink.residency.write_rejected_cross_region.v1` emitido com payload {tenant_id, attempted_region: 'enam', expected_region: 'weur', backend: 'blob_meta'}
And SEV-1 alert disparado (FM-451 P0 S=5 — bypass attempt = potential exploit)
```

### AC-004: 20k property test (10k weur + 10k enam) → 0 cross-region leaks

```gherkin
Given tests/property_residency_20k.rs com 10k tenants generated weur + 10k tenants enam
And random ops per tenant: blob PUT, blob GET, AC PUT, billing-event emit, audit emit (5 ops each tenant)
When property test executa em CI
Then total operations = 20k tenants × 5 ops = 100k operations
And per operation, region tag verified contra tenant.primary_region
And 0 cross-region leaks detected
And `corelink_residency_property_test_failures_total` Prom counter = 0
And se any leak detected, test falha + SEV-1 alert + RB-DATA-RESIDENCY-LEAK acionado
```

### AC-005: 100k nightly cron property test (S-09 inheritance pattern)

```gherkin
Given nightly cron worker (S-09 inheritance) dispara property test 100k @ 02:00 UTC
When test executa em production-like staging com 100k tenants synthetic + random ops
Then 0 cross-region leaks sustained (corelink_residency_property_test_failures_total = 0)
And se any leak detected, SEV-1 alert + Privacy Officer + Compliance escalation
And property test results aggregated em Grafana dashboard `corelink-residency-enforcement` (sprint contract §6 DoD)
```

### AC-006: Region migration cooldown 30d ADR-S11-011

```gherkin
Given tenant T tem primary_region = 'weur' set em signup ts T0
When tenant requests migration POST /v1/admin/tenant/region-migration com payload {target_region: 'sam', justification: "...", attestation: "..."}
Then Privacy Officer + Compliance review ticket criado (D1 region_migration_request)
And ticket_status = 'pending'
And cooldown 30d enforced: migration NOT executable até T0 + 30d (privacy_model.md §7.2)
And se T0 + 30d cumprido + Privacy Officer + Compliance approve, ticket_status = 'approved'
And data migration scheduled (S-13 admin plane future) com customer notification
And email confirmação a tenant em locale; explanation cooldown rationale ADR-S11-011
```

### AC-007: Audit fail-CLOSED em emit failure

```gherkin
Given audit emit infrastructure (R2 audit-`<region>`) está indisponível
When request residency violation detected
Then 2 CloudEvents emit fails
And response 503 com retry-after=60s para audit emit failure (não retorna 451; audit fail-CLOSED prioritário sobre clarity em incidente de auditoria — distinto do residency 451)
And SEV-2 alert disparado
And distinct from billing fail-OPEN (split-tier discipline; ADR-S11-002 cross-WI)
And NO violation row inserted em D1 (transaction-like behavior)
```

### AC-008: ADR-S11-010 TLA+ deferral documented

```gherkin
Given ADR-S11-010 committed em specs/03_architecture/adrs/
When validator script executes
Then ADR contém:
  - Title: "TLA+ residency formal proof deferred to S-14"
  - Decision: "S-11 entrega cobertura runtime + property tests; TLA+ deferred"
  - Rationale: 4 bullets (industry standard; S-14 BYOK overlap; sprint scope discipline; current invariant_registry.md §4.2 L445 acknowledges)
  - Alternative considered: "TLA+ in S-11" rejected (sprint scope blow)
  - Privacy Officer + Architect sign-off
And invariant_registry.md §4.2 L445 confirms "Subsumed por INV-REGION-NO-CROSS-LEAK (S-14 PLANNED `region_residency.tla` / `byok_sovereignty.tla`)"
```

## 9. Design Decisions

### 9.1 Decisões locais

- **DD-001 D1 CHECK constraint vs trigger**: trigger primary (D1 não suporta subquery em CHECK conforme schema mas suporta trigger BEFORE INSERT/UPDATE); CHECK simples para region enum closed. Compromisso: trigger overhead vs subquery flexibility.
- **DD-002 20k property test baseline + 100k nightly**: 20k em CI fast iteration; 100k nightly cron extra confidence (sprint contract R-S11-19 = 20k baseline; nightly bonus).
- **DD-003 Custom domain optional `team`+ vs mandatory all**: optional reduces UX friction free/solo; managed via Cloudflare Pages para `team`+. Trade-off: custom domain visibility (regulatory signal) vs UX.
- **DD-004 Region migration via separate endpoint vs settings UI**: separate endpoint (POST /v1/admin/tenant/region-migration) com step-up MFA + Privacy Officer review; settings UI seria too easy + bypass cooldown 30d.
- **DD-005 Default region sam BR**: cliente-âncora HuGR; minimize friction Brazilian customers; CCPA/GDPR users explicit opt to enam/weur.

### 9.2 Decisões que justificam ADR

- **ADR-S11-010 (NEW)**: TLA+ residency formal proof deferred to S-14. Rationale: (1) industry standard runtime + property tests sufficient for regulatory baseline; (2) S-14 BYOK overlaps with residency formal proof scope (`region_residency.tla` + `byok_sovereignty.tla` per invariant_registry.md §4.2 L445); (3) S-11 sprint scope discipline (8 WIs already; TLA+ adds 16+ hours); (4) current invariant_registry.md §4.2 L445 already acknowledges deferral. Privacy Officer + Architect sign-off.
- **ADR-S11-011 (NEW)**: Region migration cooldown 30d. Rationale: privacy_model.md §7.2 specifies "Mudança de região = pedido formal + migração + cooldown 30d"; 30d enables Privacy Officer + Compliance review + customer expectation alignment; data migration tooling (S-13) requires window for safe execution. Alternatives considered: 7d (too aggressive; insufficient review time), 60d (too conservative; UX friction). Privacy Officer + Compliance + Architect sign-off.

### 9.3 Trade-offs explícitos

| Trade-off | Opção A | Opção B | Decisão | Rationale |
|---|---|---|---|---|
| D1 enforcement | CHECK constraint | Trigger | B | Subquery support |
| Property test count | 20k baseline | 100k baseline | A + 100k nightly | CI speed + nightly extra confidence |
| Custom domain | Mandatory | Optional `team`+ | B | UX friction free/solo |
| Region migration | Settings UI | Endpoint + step-up MFA | B | Cooldown 30d enforce + Privacy Officer review |
| Default region | enam US | sam BR | B | Cliente-âncora HuGR |
| TLA+ formal proof | S-11 | S-14 | S-14 | Sprint scope + S-14 BYOK overlap; ADR-S11-010 |

### Anti-pattern ❌

❌ CHECK constraint sem subquery (cross-tenant region match impossible); ❌ 100k em CI baseline (CI overhead); ❌ Custom domain mandatory (UX friction free/solo); ❌ Region migration via settings UI (cooldown bypass); ❌ Default enam US (Schrems II for EU); ❌ TLA+ em S-11 (scope blow).

## 10. Completeness Criteria SOTA

### 10.1 Code Completeness

- [ ] **C-1.1** Crate `corelink-privacy-residency-enforcement` compila WASM zero warnings.
- [ ] **C-1.2** Region enum 6-region canonical closed.
- [ ] **C-1.3** Worker routing layer (custom domain regex match) integrated.
- [ ] **C-1.4** D1 migration N+5 (ALTER + CREATE TRIGGER per backend table) aplica + roll-back.
- [ ] **C-1.5** Insert checks pre-flight em CAS PUT, AC PUT, audit emit, billing-events emit (5+ paths).
- [ ] **C-1.6** Region migration endpoint POST /v1/admin/tenant/region-migration (auth + step-up MFA + Privacy Officer review).
- [ ] **C-1.7** Audit emit 2 CloudEvents canonical fail-CLOSED.
- [ ] **C-1.8** RB-DATA-RESIDENCY-LEAK runbook committed.
- [ ] **C-1.9** failure_modes.md FM-451 entry committed.
- [ ] **C-1.10** ADR-S11-010 + ADR-S11-011 committed.
- [ ] **C-1.11** Email signup template integration 3 locales (region pinned mention).

### 10.2 Test Completeness

- [ ] **T-2.1** Integration test signup → cross-region request rejected → audit emit verde.
- [ ] **T-2.2** Property test 20k em CI (10k weur + 10k enam × 5 ops cada) → 0 leaks.
- [ ] **T-2.3** Property test 100k nightly cron staging → 0 leaks sustained 90d.
- [ ] **T-2.4** Regression test D1 CHECK + trigger constraint enforcement.
- [ ] **T-2.5** Chaos test residency leak via direct D1 API → trigger fires + SEV-1 alert + RB acionado.
- [ ] **T-2.6** Regression test region migration cooldown 30d (privacy_model.md §7.2 + ADR-S11-011).
- [ ] **T-2.7** Cross-tenant attack: 100k random pairs (T1, T2) → 0 cross-tenant region leaks.
- [ ] **T-2.8** Audit fail-CLOSED chaos test em residency violation detection.

### 10.3 Documentation Completeness

- [ ] **D-3.1** `docs/dev/residency-enforcement-architecture.md`.
- [ ] **D-3.2** ADR-S11-010 (TLA+ deferral) + ADR-S11-011 (cooldown 30d).
- [ ] **D-3.3** RB-DATA-RESIDENCY-LEAK runbook canonical SOP.
- [ ] **D-3.4** Privacy notice (WI-S11-004) update referencia residency section.

### 10.4 Observability Completeness

- [ ] **O-4.1** 3 Prom metrics: violation_total, routed_total, property_test_failures_total.
- [ ] **O-4.2** 1 dashboard `corelink-residency-enforcement` em Grafana com 5 panels.
- [ ] **O-4.3** 2 CloudEvents canonical types em audit-`<region>` Object Lock 7y.

### 10.5 Security & Privacy Completeness

- [ ] **S-5.1** Region enum closed (cardinality discipline; novos regions ADR-required).
- [ ] **S-5.2** D1 CHECK + trigger constraint per backend table (defensive layered).
- [ ] **S-5.3** Worker pre-flight + insert checks layered (defense-in-depth).
- [ ] **S-5.4** Schrems II + LGPD Art. 33 § 1º + GDPR Art. 44 alignment Legal Review pre-merge.
- [ ] **S-5.5** Cross-tenant attack mitigation 100k property test 0 leaks.
- [ ] **S-5.6** Audit fail-CLOSED em residency violation detection (regulatory evidence priority).

### 10.6 SBOM Completeness

- [ ] **B-6.1** SBOM CycloneDX 1.5+ inclui `corelink-privacy-residency-enforcement`.

## 11. DoD

10.x checked + sign-off matrix §30 + chaos 30d staging + 100k nightly property test verde sustained 30d + Schrems II Legal Review pre-merge.

## 12. Invariants Validated

| INV | Severity | Position canonical | Cobertura WI-S11-007 |
|---|---|---|---|
| **INV-DATA-RESIDENCY** | CRITICAL (Lote 10.11.0-bis: Schrems II) | invariant_registry.md §3.11 L154 | Custom domain routing + insert checks D1 + 20k property test runtime cobertura. **TLA+ scope split (Lote 10.11.0-bis-prime cycle 3 reconciled; status cascade Lote 10.11.0-bis-bis V2 2026-05-15)**: PARTIAL S-11 via `dsr_erasure_atomicity.tla` (InvResidencyPinned + InvResidencyMonotonic — pinning + monotonic) **✅ GREEN**; FULL formal proof (cross-region routing actions) **LANDED EARLY** via `region_residency.tla` (sprint owner S-14 preserved per ADR-S11-010 + invariant_registry.md §4.2; ✅ GREEN per 2026-05-15-tla-coverage-audit §3). |
| **INV-AUDIT-APPEND-ONLY** | CRITICAL | invariant_registry.md §3.6 L116 | 2 CloudEvents canonical em audit-`<region>` Object Lock 7y; emit fail-CLOSED |

## 13. Artifacts Produced

- `crates/corelink-privacy-residency-enforcement/` (NEW; ~2000 LoC).
- `migrations/N+5__residency_check_constraints.sql` (ALTER + triggers per backend table).
- `docs/dev/residency-enforcement-architecture.md`.
- ADR-S11-010 + ADR-S11-011 committed.
- RB-DATA-RESIDENCY-LEAK runbook canonical.
- failure_modes.md FM-451 entry.
- 2 CloudEvents schemas em `schemas/cloudevents/residency-{request_routed,write_rejected_cross_region}.v1.json`.
- Grafana dashboard JSON.
- 3 Prom metrics + alerts.
- D1 region_migration_request table DDL.

## 14. Quality Standards SOTA

- **14.s11.7.1** Schrems II + LGPD Art. 33 § 1º + GDPR Art. 44 Legal Review pre-merge.
- **14.s11.7.2** 20k property test em CI 0 leaks 100% sustained.
- **14.s11.7.3** 100k nightly cron property test 0 leaks sustained 90d.
- **14.s11.7.4** D1 CHECK + trigger constraint defense-in-depth.
- **14.s11.7.5** Region migration cooldown 30d (privacy_model.md §7.2 + ADR-S11-011).
- **14.s11.7.6** TLA+ deferral documented + invariant_registry.md §4.2 L445 alignment.
- **14.s11.7.7** Cross-tenant attack 100k property test 0 leaks.
- **14.s11.7.8** INV §3.X positions canonical verified pre-merge (Lote 10.8bis P1-13).

## 15. Chaos Experiments (6)

1. Direct D1 API bypass attempt (insert direto sem worker pre-flight) → trigger fires + SEV-1.
2. Custom domain DNS resolution failure → fallback default routing rejected (CTRL-PRIV-031 strict).
3. Region migration cooldown bypass attempt → endpoint enforces 30d strict + ADR-S11-011.
4. Cross-tenant region leak (T1 weur tenta ler T2 sam blob via cross-tenant request) → TenantCtx + region check both fire.
5. Property test 20k seed bias attack (all tenants em 1 region) → property generator forces balanced 50/50 weur/enam.
6. Audit emit failure mid-violation detection → 503 (fail-CLOSED) + SEV-2 alert + retry queue.

## 16. PRR

PRR HIGH_RISK 12 sign-offs + chaos 30d staging + 100k property test sustained + Schrems II Legal Review.

## 17. Sub-tasks

| ID | Descrição | PERT |
|---|---|---|
| ST-001 | Crate scaffold + ResidencyEnforcement trait + Region enum | 1.5h |
| ST-002 | Worker routing layer custom domain regex (S-09 multi-region inheritance) | 2h |
| ST-003 | D1 migration N+5 ALTER + triggers per backend table | 2h |
| ST-004 | Insert checks pre-flight em 5+ paths (CAS PUT, AC PUT, audit, billing, manifest) | 2.5h |
| ST-005 | 2 CloudEvents emitter fail-CLOSED | 1h |
| ST-006 | Property test 20k em CI (10k weur + 10k enam × 5 ops) | 2h |
| ST-007 | Property test 100k nightly cron worker | 1.5h |
| ST-008 | Region migration endpoint POST /v1/admin/tenant/region-migration | 2h |
| ST-009 | RB-DATA-RESIDENCY-LEAK runbook | 1h |
| ST-010 | failure_modes.md FM-451 entry | 0.3h |
| ST-011 | ADR-S11-010 + ADR-S11-011 | 1h |
| ST-012 | 3 Prom metrics + Grafana dashboard | 1h |
| ST-013 | Email signup template 3 locales region mention (WI-S11-004 mjml) | 0.5h |
| ST-014 | Documentation residency-enforcement-architecture.md | 0.4h |

**PERT total**: ~18.7h (alinha com sprint contract §12).

## 18. Dependencies

- **Hard**: spec contract S-11 v1.2.0 SEALED; S-09 SEALED (multi-region observability + cron workers + audit emit); S-01 SEALED (CAS foundation com region tag); WI-S11-001 (region migration endpoint reuses pattern + step-up MFA).
- **Soft**: WI-S11-004 (email signup template 3 locales reuse); WI-S11-008 (TLA+ residency é deferred to S-14 mas DPIA WI-S11-008 referencia residency).

## 19. Effort PERT: ~18.7h. ## 20. Time-boxing: 28h hard limit (lane HIGH_RISK +40% buffer).

## 21. Observability

3 Prom metrics + 1 Grafana dashboard 5 panels + 2 CloudEvents canonical.

## 22. Cost Analysis

- **Cloudflare DNS**: custom domain records ~6 regions × N tenants; included em CF tier.
- **D1 triggers**: runtime overhead minimal (~10µs per insert).
- **R2 audit-`<region>`**: 2 events/violation × ~100 violations/year × 7y; ≈$0.001/year.
- **Property test compute (20k CI + 100k nightly)**: ~5min CI + 30min nightly; ≈$0.01/run × 365 = $3.65/year nightly + per-PR.
- **Region migration tickets**: low-volume (~10/year initial); negligible.
- **Total estimated**: ≤ $20/year (well under §14 budget).

## 23. API Contract

2 CloudEvents schemas em `schemas/cloudevents/residency-*.v1.json`. Region enum schema em `schemas/json/region-canonical.json` (closed enum 6 values). Region migration endpoint OpenAPI 3.1 em `docs/api/admin-region-migration.md`.

## 24. Post-mortem Hooks

| Trigger | Severity | Owner |
|---|---|---|
| FM-451 residency leak detected | CRITICAL | Privacy Officer + SecLead + Compliance + Legal escalation; consider Schrems II disclosure se EU customer |
| INV-DATA-RESIDENCY violation (cross-region write succeeded) | CRITICAL (Lote 10.11.0-bis-prime: HIGH→CRITICAL Schrems II + LGPD Art. 33 §1º) | Privacy Officer + Architect; **SEV-1 alert** + ANPD/EDPB pre-notification consideration |
| 100k nightly property test failure | HIGH | Architect + Privacy Officer; emergency triage |
| Region migration cooldown bypass exploit | CRITICAL | SecLead + Privacy + Compliance |
| D1 trigger bypass (e.g., direct R2 API write sem trigger) | CRITICAL | SecLead + Architect; defense-in-depth review |
| Custom domain DNS resolution failure cascading | HIGH | SRE + Cloudflare support escalation |
| TLA+ residency deferral controversial (S-14 not yet ready) | MEDIUM | Privacy Officer + Architect; ADR-S11-010 review |
| Audit fail-OPEN regression (Lote 10.6bis) | CRITICAL | Architect + Privacy |

## 25. Rollback / Recovery

**No transparent-passthrough fallback** (corrige GPT P1-7 round-1: rollback flag flip permitia fail-open privacy leak). Feature flag `residency_enforcement_strict` em DO config-singleton é **write-once-true** (PAT-ROUTING-PINNED-001 canonical): uma vez ativada, NÃO pode ser flipada para `false` sem (a) dual approval Privacy Officer + Compliance + SecLead, (b) ANPD/EDPB pre-notification, (c) audit chain entry justificando break-glass. Rollback de runtime defect: redirect 307 fail-CLOSED para custom domain regional (PAT-ROUTING-PINNED-001 §3.4); se redirect down, retorna 451 `legal_residency_violation` (NUNCA passthrough). NÃO rollback de tenant.primary_region (privacy_model.md §7.2 monotonic — region migration via separate endpoint cooldown 30d). Dry-run reversível apenas em staging (production é one-way regulatory commitment).

## 26. Security & Privacy

LINDDUN per privacy_model.md §4 + STRIDE per security_model.md §6:
- L(inkability): tenant_id em audit pseudonymized (CTRL-PRIV-014).
- I(dentifiability): region tag intentional (regulatory transparency).
- N(on-repudiation): R2 Object Lock 7y + 2 CloudEvents permanent.
- D(etectability): Privacy Officer + Compliance monitor `corelink_residency_violation_total` quarterly.
- D(isclosure): residency enforcement is the disclosure mechanism (transparent regulatory posture).
- U(nawareness): privacy notice (WI-S11-004) + email signup explica region pinning.
- N(on-compliance): **Schrems II + LGPD Art. 33 § 1º + GDPR Art. 44 + CCPA + SOC 2 P5.1 + ISO/IEC 27018 §A.11 compliance** via runtime enforcement + 20k property + custom domain + audit.

## 27. Knowledge Transfer

Tech talk (1h): "S-11 Residency Pinning E2E: 6-region canonical + custom domain routing + D1 CHECK constraint + 20k property test + ADR-S11-010 TLA+ deferral S-14"; doc `docs/dev/residency-enforcement-architecture.md`; onboarding test 6 questões: 6-region canonical enum (privacy_model.md §7.1), custom domain pattern `<tenant_id>.<region>.corelink.humangr.com`, D1 trigger vs CHECK constraint trade-off (DD-001), region migration cooldown 30d rationale (ADR-S11-011), TLA+ deferral S-14 rationale (ADR-S11-010), Schrems II + LGPD Art. 33 § 1º + GDPR Art. 44 alignment.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | FM-451 residency leak via direct D1 API bypass | L | M | CRITICAL | M | LOW | D1 trigger defense-in-depth + chaos test + SEV-1 alert |
| R-002 | 20k property test seed bias (e.g., all tenants em 1 region) | L | L | LOW | L | LOW | Balanced 50/50 generator + property invariant verified |
| R-003 | Custom domain DNS resolution failure cascading | L | L | HIGH | L | LOW | Cloudflare DNS SLA + fallback default routing rejected (strict CTRL-PRIV-031) |
| R-004 | Region migration cooldown bypass exploit | L | L | CRITICAL | L | LOW | Endpoint enforces 30d strict + step-up MFA + Privacy Officer review |
| R-005 | TLA+ deferral S-14 controversial | M | L | MEDIUM | L | LOW | ADR-S11-010 + invariant_registry.md §4.2 L445 alignment + quarterly review |
| R-006 | Cross-tenant region leak (T1 reads T2 cross-tenant + cross-region) | L | L | CRITICAL | L | LOW | TenantCtx + region check both layers; 100k property test |
| R-007 | D1 trigger overhead > 50µs per insert | L | L | LOW | L | LOW | Profile em CI + DELETE batch perf benchmark |
| R-008 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | LOW | L | LOW | INV positions §3.6 L116 + §3.11 L154 verified Lote 10.11.0 |
| R-009 | Audit fail-OPEN regression (Lote 10.6bis violation) | L | L | CRITICAL | L | LOW | ADR-S11-002 cross-WI; 503 + retry-after em emit failure |
| R-010 | Custom domain provisioning latency > 5min at signup | L | L | LOW | L | LOW | Cloudflare API SLA + manual provisioning fallback |
| R-011 | Region enum drift (open string slip in code) | L | L | LOW | L | LOW | Closed enum + ADR per addition + cardinality discipline |
| R-012 | Property test 100k nightly false-positive | L | L | LOW | L | LOW | Deterministic seeding + reproducibility CI artifact |

## 29. Review Checkpoints

D+0 design (Architect; D1 CHECK vs trigger trade-off + ADR-S11-010 deferral); D+1 Privacy Officer (Schrems II + LGPD Art. 33 § 1º); D+2 Legal (DPA reference + customer notification); D+3 SecLead (defense-in-depth + STRIDE); D+4 Compliance (SOC 2 P5.1 + ISO 27018 §A.11); D+5 code review; D+6 PRR.

## 30. Sign-off (HIGH_RISK 12)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ |
| 4 | Security Lead | _TBD; **mandatory** — defense-in-depth + STRIDE_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos 30d + 20k + 100k nightly property test_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — Schrems II + LGPD Art. 33 § 1º + GDPR Art. 44 + ISO 27018 §A.11_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — CTRL-PRIV-031 + ADR-S11-010 deferral + ADR-S11-011 cooldown_ |
| 11 | Architect | _TBD; **mandatory emphatic** — D1 CHECK vs trigger + INV §3.X verification + custom domain routing_ |
| 12 | DPO interim | _TBD; **mandatory emphatic** — Schrems II defensibility + 20k property test_ |

(Legal sign-off via DPA reference at sprint level.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.1.0 | 2026-04-28 | Gustavo (Lote 10.11.0-bis + 10.11bis) | **Canonical fixes pós baseline review aggregate 5.18/10**: (a) **GPT P1-7 residency_enforcement_strict write-once-true** (PAT-ROUTING-PINNED-001 canonical resilience_patterns.md §3.4) — anterior permitia flip → "transparent passthrough" fail-OPEN privacy leak. Agora: dual approval Privacy Officer + Compliance + SecLead + ANPD/EDPB pre-notification + audit chain entry para break-glass. (b) **Fail-CLOSED 451 `legal_residency_violation`** em redirect indisponível — NUNCA passthrough silencioso. (c) **INV-DATA-RESIDENCY HIGH→CRITICAL** (Schrems II + 20k property test + custom domain routing TLA+ via PAT-FORMAL-VERIFICATION-001). (d) **FM-451 residency violation** declared canonical em failure_modes.md §3.10. (e) **tenant.primary_region CHECK constraint** PG-side enforced (data_model.md §4.1). |
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.11) | Criação WI-S11-007; HIGH_RISK; SOTA pós-S-10 SEALED. Residency pinning E2E legal exposure mitigation primary. Schrems II + LGPD Art. 33 § 1º + GDPR Art. 44 alignment via runtime layer 3-fold: (a) custom domain routing `<tenant_id>.<region>.corelink.humangr.com` 6-region canonical enum (wnam/enam/weur/sam/apac/afr per privacy_model.md §7.1); (b) D1 CHECK + trigger constraint defense-in-depth em backend tables (blob_meta + ac_meta + audit_outbox + billing-events staging); (c) Worker pre-flight assertion antes de R2 writes. tenant.primary_region canonical column data_model.md §4.1 L151 (NOT tenant_metadata.region_pinned legacy correção sprint contract v1.2.0 Lote 10.11.0). 20k property test em CI (10k weur + 10k enam × 5 random ops cada) → 0 cross-region leaks. 100k nightly cron property test sustained 90d (S-09 inheritance pattern). NEW RB-DATA-RESIDENCY-LEAK runbook + NEW FM-451 declaration em failure_modes.md (residency leak P0 S=5 → upgrade FF-HR-010). 2 CloudEvents canonical `dev.hugr.corelink.residency.{request_routed,write_rejected_cross_region}.v1` per Lote 10.9bis P0-G prefix em audit-`<region>` Object Lock 7y emit fail-CLOSED. Region migration endpoint POST /v1/admin/tenant/region-migration com step-up MFA + Privacy Officer + Compliance review + cooldown 30d (privacy_model.md §7.2 + ADR-S11-011). NEW ADR-S11-010 (TLA+ residency formal proof deferred to S-14 — `region_residency.tla` / `byok_sovereignty.tla` per invariant_registry.md §4.2 L445; rationale: industry standard runtime + property tests sufficient regulatory baseline; S-14 BYOK overlap; sprint scope discipline) + ADR-S11-011 (cooldown 30d rationale). INV-DATA-RESIDENCY CRITICAL (Lote 10.11.0-bis: HIGH→CRITICAL Schrems II) §3.11 L154 runtime cobertura + INV-AUDIT-APPEND-ONLY CRITICAL §3.6 L116 cobertura. CTRL-PRIV-031 (residency pinning) satisfação. 8 AC scenarios + 6 chaos + 12 risks + 8 post-mortem hooks. Email signup template integration 3 locales (region pinned mention via WI-S11-004 mjml infra). **Lote 10.10 lessons absorbed**: source-of-truth FIRST INV positions verified pre-merge (Lote 10.8bis P1-13); typed enum (6 regions canonical closed; cardinality discipline ADR-S11-006 pattern); sign-off cap 12; cascade discipline absoluta; split-tier audit fail-CLOSED; corelink_time canonical helper; severity cascade lesson (Lote 10.10-sextus): HIGH severity invariant violation → SEV-1 alert override em CRITICAL data exposure scenarios; CTRL-PRIV-014 audit minimization (tenant_id pseudonymized). |

## 32. Anti-patterns evitados

- ❌ Region default = enam US (Schrems II exposure); ❌ Custom domain mandatory all (UX friction free/solo); ❌ Insert checks bypass via direct API (defense-in-depth gap); ❌ Property test < 20k (sprint contract §5.7 R-S11-19 violation); ❌ Sem audit event (regulatory evidence gap); ❌ Cross-region writes silent (regulatory finding); ❌ Open region string (cardinality drift); ❌ TLA+ em S-11 (sprint scope blow); ❌ Region migration via settings UI (cooldown bypass); ❌ INV §3.X position TBD (Lote 10.8bis P1-13); ❌ Audit fail-OPEN (Lote 10.6bis split-tier violation); ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3).

---
