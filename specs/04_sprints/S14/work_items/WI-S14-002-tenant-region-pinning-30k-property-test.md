---
id: "WI-S14-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-28"
updated: "2026-04-28"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-003"]
parent: "S-14"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "PRIVACY-MODEL"
  - "SECURITY-MODEL"
  - "STORAGE-SEMANTICS-MATRIX"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
tags: ["wi", "s14", "region", "pinning", "enforcement", "property-test", "schrems-ii", "lgpd", "high-risk"]
---

# WI-S14-002 — Tenant `primary_region` Enforcement no Write Path: D1 Schema Migration + Insert Checks (`tenant.region == request.region` mismatch = 403 + audit emit `corelink.region.cross_region_read_blocked`) + DO `region_enforcer` Validates Per-Request + 30k Property Test Cross-Region Scenarios → 0 Leaks + INV-REGION-NO-CROSS-LEAK CRITICAL Ratificada + Runbook RB-region-leak

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-14](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S14-002 |
| Título | Tenant region pinning enforcement runtime: D1 schema migration `tenants.primary_region NOT NULL CHECK IN ('wnam','enam','weur','sam')` + insert checks no write path validate `tenant.region == request.region` (mismatch = 403 + audit emit `corelink.region.cross_region_read_blocked`) + DO `region_enforcer` middleware validates per-request via tenant lookup; 30k property test cross-region scenarios verifies INV-REGION-NO-CROSS-LEAK 0 leaks (Schrems II + LGPD Art. 33 §1º baseline); CI gate ativo; runbook RB-region-leak (incident response cross-region leak) committed + dry-run; KV namespace per-region scope enforced em Worker binding (FM-054 prevention) |
| Sprint | S-14 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (cross-region tenant isolation 0-tolerance), FF-HR-003 (residency PII regulatory absoluto Schrems II + LGPD Art. 33) |

## 1. Intent

Enforcement runtime de tenant region pinning: cada request precisa validar `tenant.primary_region == request.region` antes de qualquer R2/D1/DO/KV access; mismatch = 403 hard-fail + audit emit `corelink.region.cross_region_read_blocked`. Sem isso, foundation infra (WI-S14-001) não previne cross-region leaks em camada aplicação — atacante pode usar tenant_id ENAM com endpoint WEUR para forçar acesso cross-region. Implementação: (1) **D1 schema migration** adiciona `tenants.primary_region NOT NULL CHECK IN ('wnam','enam','weur','sam')` column; existing tenants migrated via WI-S14-001 script (default = 'enam' for legacy); (2) **Insert checks no write path** — middleware `region_check.rs` (Tower layer) executa `tenant.primary_region == request.region` ANTES de R2/D1/DO/KV access; mismatch = `403 Forbidden` + audit emit + métrica increment; (3) **DO `region_enforcer`** — Durable Object middleware validates per-request via tenant lookup (D1 query cached em DO storage 5min TTL); (4) **KV namespace per-region scope** enforced em Worker binding (`corelink-session-{region}`); cross-region KV access = compile-time error (Worker binding only allows per-region namespace); (5) **30k property test** — `tests/region_pinning_proptest.rs` simulates 30k cross-region scenarios (random tenant_id × random request region) → asserts 0 leaks (INV-REGION-NO-CROSS-LEAK CRITICAL); (6) **CI gate ativo** — property test runs em PR + nightly 100k iter; (7) **Runbook RB-region-leak** — incident response cross-region leak detected (immediate kill + audit chain forensic + customer notification + breach assessment Schrems II).

```rust
// File: crates/corelink-region-enforcer/src/lib.rs

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Region {
    Wnam,
    Enam,
    Weur,
    Sam,
}

impl Region {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Wnam => "wnam",
            Self::Enam => "enam",
            Self::Weur => "weur",
            Self::Sam => "sam",
        }
    }
    pub fn from_str(s: &str) -> Result<Self, RegionError> {
        match s {
            "wnam" => Ok(Self::Wnam),
            "enam" => Ok(Self::Enam),
            "weur" => Ok(Self::Weur),
            "sam" => Ok(Self::Sam),
            _ => Err(RegionError::InvalidRegion(s.to_string())),
        }
    }
}

#[derive(Debug, Error)]
pub enum RegionError {
    #[error("invalid region: {0}")]
    InvalidRegion(String),
    #[error("cross-region access denied: tenant={tenant_id} primary={primary:?} requested={requested:?}")]
    CrossRegionDenied {
        tenant_id: String,
        primary: Region,
        requested: Region,
    },
    #[error("tenant not found: {0}")]
    TenantNotFound(String),
    #[error("D1 lookup error: {0}")]
    Storage(String),
}

pub struct RegionEnforcer<'a> {
    d1: &'a worker::d1::D1Database,
    do_cache: &'a worker::durable::Storage,
}

impl<'a> RegionEnforcer<'a> {
    /// Validate tenant.primary_region == request.region;
    /// returns Ok(Region) if match, Err(CrossRegionDenied) on mismatch.
    /// Cached em DO storage 5min TTL para reduce D1 RTT.
    pub async fn enforce(
        &self,
        tenant_id: &str,
        request_region: Region,
    ) -> Result<Region, RegionError> {
        let primary = self.lookup_primary_region(tenant_id).await?;
        if primary != request_region {
            return Err(RegionError::CrossRegionDenied {
                tenant_id: tenant_id.to_string(),
                primary,
                requested: request_region,
            });
        }
        Ok(primary)
    }

    async fn lookup_primary_region(&self, tenant_id: &str) -> Result<Region, RegionError> {
        // 1. Check DO storage cache (5min TTL)
        // 2. If miss, query D1 SELECT primary_region FROM tenants WHERE id = ?
        // 3. Cache result em DO storage
        unimplemented!()
    }
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Tenant region pinning enforcement é o coração do compliance Schrems II + LGPD Art. 33 baseline para CoreLink S-14: foundation infra (WI-S14-001) provisiona 4 regiões com per-region R2/D1/DO/KV; mas se aplicação não enforce `tenant.primary_region == request.region` runtime, atacante (ou bug acidental) pode usar tenant_id de WEUR com endpoint ENAM para forçar EU data on US infrastructure = catastrophic legal exposure (€20M+ GDPR fines + breach notification + customer trust permanently lost). Bug em qualquer um dos 4 paths {R2 read/write, D1 query, DO call, KV access} = INV-REGION-NO-CROSS-LEAK violation = SEV-1 incident.

**Bugs catastróficos possíveis** (todos endereçados):

1. **Insert check bypass via direct binding access**: handler chama `r2_bucket.get(key)` antes de region_check middleware. Mitigação: Tower layer ordering (region_check ANTES de handler dispatch); per-handler test asserts middleware presence; clippy custom lint detects direct binding access em handlers (advisory).

2. **DO region_enforcer cache poisoning**: stale cache returns wrong primary_region. Mitigação: 5min TTL hard limit; cache invalidation on tenant update event (S-13 admin op); D1 fallback se cache stale > 5min.

3. **Cross-region KV leak (FM-054)**: Worker binding allows global KV access. Mitigação: per-region binding name (`corelink-session-{region}`); compile-time error if Worker tries cross-region binding; runbook RB-FM-054 dry-run em WI-S14-009.

4. **Subdomain spoofing**: attacker requests `weur.api.corelink.dev` with ENAM tenant_id. Mitigação: middleware extracts request_region from custom domain; insert check rejects 403; audit emit; dashboard alert if rate > baseline.

5. **D1 row primary_region mutation post-signup**: bug em admin API allows changing `tenants.primary_region` post-signup = re-routing existing tenant data. Mitigação: D1 column `primary_region` immutable post-INSERT (CHECK constraint + UPDATE trigger reject); migration manual ticket only.

6. **Property test false-negative**: 30k iter doesn't catch edge case. Mitigação: nightly 100k iter; coverage all 4 regions × 4 ops {get, put, head, delete} = 16 coverage cells × 4 tenant types = 64 combinations × 469 iter avg = 30k.

7. **Chunk dedup cross-region leak**: tenant ENAM uploads chunk; tenant WEUR with same body computes same hash; dedup hits ENAM chunk. Mitigação: dedup is per-tenant (`(tenant_id, chunk_digest) → chunk_body` 1:1 inheritance INV-DEDUP-CONSISTENCY S-07); cross-tenant dedup forbidden by design.

8. **Backup cross-region**: backup of WEUR D1 stored em US R2 = residency violation. Mitigação: backup destination = same-region R2 audit bucket (S-09 inheritance); per-region backup cadence; integration test verifies backup region.

9. **Audit chain cross-region leak**: audit event tenant WEUR stored em ENAM audit chain. Mitigação: per-region audit chain (S-09 R-S09-10 herdada); region tag em event payload; chain integrity per-region.

**Atacante adversarial scenarios**:

- **Force WEUR tenant_id with ENAM endpoint**: middleware rejects 403; audit emit; alert if rate > baseline.

- **Tamper request_region header**: attacker sets `X-Region: weur` for ENAM endpoint. Mitigação: request_region derived from custom domain (`{region}.api.corelink.dev`) NOT from header; header ignored.

- **DO storage cache poisoning attempt**: attacker triggers tenant primary_region update via admin API. Mitigação: admin API requires admin role + dual-approval (S-13); UPDATE trigger rejects primary_region mutation post-INSERT; manual ticket required for legitimate region migration.

- **Replay request to wrong region**: attacker captures legitimate WEUR request; replays to ENAM endpoint. Mitigação: middleware rejects 403; replay protection via nonce (S-03 herdada).

**Risk justification HIGH_RISK**:

- **FF-HR-002**: cross-region tenant isolation 0-tolerance; bug = data leak.
- **FF-HR-003**: Schrems II + LGPD Art. 33 regulatory absoluto.
- **Reversibility**: cross-region leak detected = customer notification + breach assessment + Schrems II legal exposure permanent.

11 sign-offs canonical incl. Architect (middleware ordering + DO cache strategy + property test design) + Security Lead (subdomain spoofing defense + admin API mutation defense) + Privacy Officer (LGPD + GDPR alignment) + Compliance Officer (Schrems II + DPA evidence pack).

## 3. Customer Impact & Journey

**Persona 1 — Customer EU (financial services)**:
- Signup com `primary_region: weur`; D1 INSERT `tenants.primary_region = 'weur'`; immutable post-INSERT.
- Every API call routed to `weur.api.corelink.dev` (or smart routing detects WEUR via Geo-IP + tenant lookup).
- Evidence: cross-region access blocked counter `corelink_region_cross_region_read_blocked_total` zero baseline; alert if > 0 = SEV-2.

**Persona 2 — Auditor SOC 2 + Schrems II + LGPD DPO**:
- INV-REGION-NO-CROSS-LEAK CRITICAL ratificada; 30k property test 0 leaks; nightly 100k iter sustained.
- CTRL-PRIV-031 (residency) + CTRL-DATA-RESIDENCY-001 attestation.
- Evidence pack: insert checks middleware ordering + DO region_enforcer + property test reports.

**Persona 3 — Internal SRE on-call**:
- RB-region-leak committed: incident response cross-region leak detected (immediate kill + forensic + customer notification + breach assessment).
- Dashboard alert if `corelink_region_cross_region_read_blocked_total` > 0; SEV-2 escalation.
- Property test green em PR; CI gate active.

**SLA addendum**:
- Region pinning enforcement latency: middleware overhead ≤ 1ms p99 (DO cache 5min TTL).
- Property test cadence: 30k iter PR + 100k iter nightly per CI.
- Cross-region leak rate: 0 (zero tolerance; alert if > 0).
- DO cache 5min TTL hard limit.

## 4. Capability Mapping

- **CAP-REGION-002** (tenant pinning + cross-region restrict) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.1 R-S14-2` + `privacy_model.md` (CTRL-PRIV-031 residency) + `invariant_registry.md §3.12` (INV-REGION-NO-CROSS-LEAK CRITICAL nova) + `failure_modes.md` (FM-054 KV global leak prevention).

## 5. Tipo

Application middleware + D1 schema migration + property test; HIGH_RISK; FF-HR-002 + FF-HR-003.

## 6. Escopo

### 6.1 In-scope

1. **D1 schema migration `migrations/0XX_tenant_primary_region.sql`**:
   ```sql
   -- Add primary_region column (NULL initially)
   ALTER TABLE tenants ADD COLUMN primary_region TEXT
       CHECK (primary_region IS NULL OR primary_region IN ('wnam', 'enam', 'weur', 'sam'));
   
   -- Backfill existing tenants (default 'enam' for legacy single-region)
   UPDATE tenants SET primary_region = 'enam' WHERE primary_region IS NULL;
   
   -- Make NOT NULL post-backfill
   ALTER TABLE tenants ALTER COLUMN primary_region SET NOT NULL;
   
   -- UPDATE trigger reject primary_region mutation post-INSERT
   CREATE TRIGGER trg_tenants_primary_region_immutable
   BEFORE UPDATE OF primary_region ON tenants
   FOR EACH ROW
   WHEN OLD.primary_region IS NOT NULL AND OLD.primary_region != NEW.primary_region
   BEGIN
       SELECT RAISE(ABORT, 'primary_region is immutable post-INSERT');
   END;
   ```

2. **Crate `crates/corelink-region-enforcer/`**:
   - `RegionEnforcer` struct + Tower layer.
   - DO storage cache 5min TTL.
   - D1 fallback se cache stale.
   - `enforce(tenant_id, request_region)` → `Result<Region, CrossRegionDenied>`.

3. **Tower layer wiring `crates/corelink-worker/middleware/region_check.rs`**:
   - Tower layer applied AFTER auth (TenantCtx propagated) + BEFORE handler.
   - Extract request_region from custom domain (`{region}.api.corelink.dev`) — NÃO from header.
   - Call `RegionEnforcer::enforce(tenant_id, request_region)`.
   - Mismatch = 403 Forbidden + audit emit `corelink.region.cross_region_read_blocked` + métrica increment.
   - Match = continue handler dispatch.

4. **DO `region_enforcer` Durable Object**:
   - Per-region DO instance (4 instances total: WNAM/ENAM/WEUR/SAM).
   - Caches `tenant_id → primary_region` lookup 5min TTL.
   - Receives invalidation on tenant update event (S-13 admin op fan-out).
   - D1 fallback se cache stale > 5min.

5. **KV namespace per-region scope** (composed with WI-S14-001):
   - Worker binding name `corelink-session-{region}` per-region.
   - Compile-time error if Worker tries cross-region binding (TypeScript bindings types).
   - Per-region scope enforced.

6. **Property test `tests/region_pinning_proptest.rs`** (proptest crate):
   - 30k iter: random tenant_id × random request_region × random op {get, put, head, delete}.
   - Assert: tenant.primary_region == request_region OR 403 returned.
   - Coverage matrix: 4 regions × 4 ops × 4 tenant types (clean, mismatched, malformed, edge-case) = 64 combinations × ~469 iter avg.
   - Nightly 100k iter; CI gate green em PR.

7. **CI gate config** (`.github/workflows/region_pinning.yml`):
   - On PR: run 30k iter; fail PR if any leak.
   - Nightly: run 100k iter; alert SEV-2 if any leak.

8. **Runbook `specs/05_runbooks/RB-region-leak.md`**:
   - Detection: `corelink_region_cross_region_read_blocked_total` > 0 alert SEV-2.
   - Investigation: identify source tenant_id + request_region + endpoint + timeline.
   - Containment: kill request via WAF; revoke offending PAT/session.
   - Forensic: audit chain query for affected tenant; impact assessment (data leaked? metadata?).
   - Customer notification: if confirmed leak → customer alert + Schrems II breach assessment.
   - Recovery: root-cause fix + property test extension + post-mortem CRITICAL.

9. **Métricas underscored Prometheus** (per `observability_model.md §3.1`; label `plan` aplicável):
   - `corelink_region_cross_region_read_blocked_total{primary_region, request_region, plan}` (counter; alert > 0 SEV-2).
   - `corelink_region_enforcer_cache_hit_total{region, plan}` (counter).
   - `corelink_region_enforcer_cache_miss_total{region, plan}` (counter).
   - `corelink_region_enforcer_lookup_duration_seconds_bucket{region, plan}` (histogram p99 ≤ 1ms target).
   - `corelink_region_pinning_property_test_total{outcome}` (outcome ∈ pass|fail).

10. **Observability** — trace spans `region.{enforce, lookup, cache_hit, cache_miss}` com attributes:
    - `region.tenant_id` (hashed for cardinality).
    - `region.primary` (enum).
    - `region.requested` (enum).
    - `region.match` (bool).

11. **Audit emission** — CloudEvent per cross-region attempt (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER):
    - `corelink.region.cross_region_read_blocked` payload `{tenant_id_hashed, primary_region, request_region, endpoint, request_id, ts}`.
    - 7y retention (CTRL-AUDIT-005).

12. **Adversarial regression tests `tests/region_adversarial.rs`**:
    - Subdomain spoofing: tenant ENAM with `weur.api.corelink.dev` → 403.
    - Header tampering: `X-Region: weur` ignored; custom domain authoritative.
    - DO cache poisoning attempt: admin API rejects primary_region UPDATE.
    - Replay request to wrong region: nonce protection (S-03 herdada) + region enforcement.
    - D1 row direct UPDATE primary_region: trigger rejects.
    - Worker binding cross-region access: compile-time error.

13. **Integration test E2E**:
    - 4 tenant types per region (16 total) provisioned em staging.
    - Validate write/read per-region scoped (16 × 2 ops = 32 cells green).
    - Cross-region attempt = 403 (16 × 3 wrong-region ops = 48 cells reject).

### 6.2 Out-of-scope (deferred)

- **Hot blob replica + read failover**: WI-S14-003.
- **BYOK adapters**: WI-S14-004 + WI-S14-005.
- **CMK kill switch + erasure attestation**: WI-S14-006 + WI-S14-007.
- **DPA + Schrems II TIA**: WI-S14-008.
- **TLA+ region_residency + pentest + PRR**: WI-S14-009.
- **Per-tenant region migration self-service**: manual ticket only at GA.
- **Multi-DPA escalation workflow**: single DPO em S-14.

## 7. Anti-Scope

- Skip Tower layer ordering (region_check antes de handler dispatch).
- Skip DO cache 5min TTL hard limit.
- Skip property test 30k iter PR.
- Skip nightly 100k iter.
- Skip trigger D1 immutable post-INSERT.
- Skip request_region custom domain authoritative (header authoritative = spoofing).
- Allow primary_region UPDATE post-INSERT sem manual ticket + admin role + dual-approval.
- Skip audit emit on cross-region attempt.
- Skip alert SEV-2 on cross-region read blocked counter.
- Skip runbook RB-region-leak dry-run.

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: WI-S14-002 — Tenant region pinning enforcement + 30k property test + INV-REGION-NO-CROSS-LEAK

  Background:
    Given D1 migration applied (tenants.primary_region NOT NULL CHECK + immutable trigger)
    Given Tower layer region_check wired
    Given DO region_enforcer instance per-region
    Given KV namespace per-region scope enforced

  Scenario: Tenant region pinning correct match
    Given tenant T1 com primary_region = 'weur'
    When request to weur.api.corelink.dev/v1/cas/get com tenant_id=T1
    Then region_check middleware allows
    And handler dispatched
    And R2/D1/DO/KV scoped to WEUR
    And métrica corelink_region_enforcer_cache_hit_total incremented

  Scenario: Cross-region attempt blocked 403
    Given tenant T1 com primary_region = 'weur'
    When request to enam.api.corelink.dev/v1/cas/get com tenant_id=T1
    Then region_check middleware rejects 403 Forbidden
    And audit emit corelink.region.cross_region_read_blocked
    And métrica corelink_region_cross_region_read_blocked_total incremented
    And alert SEV-2 fires (if rate > baseline)

  Scenario: Subdomain spoofing rejected
    Given tenant T1 com primary_region = 'enam'
    When attacker requests weur.api.corelink.dev with tenant_id=T1
    Then 403 returned (request_region = 'weur' from custom domain != tenant.primary = 'enam')
    And audit emit + alert

  Scenario: Header tampering ignored
    Given tenant T1 primary_region = 'weur'
    When request to weur.api.corelink.dev with header X-Region: enam
    Then header ignored
    And request_region = 'weur' (from custom domain)
    And match succeeds

  Scenario: D1 immutable primary_region post-INSERT
    Given tenant T1 primary_region = 'weur' inserted
    When admin tries UPDATE tenants SET primary_region = 'enam' WHERE id = 'T1'
    Then trigger rejects with ABORT 'primary_region is immutable post-INSERT'
    And manual ticket required for legitimate migration

  Scenario: DO cache 5min TTL
    Given tenant T1 primary_region = 'weur' cached em DO storage
    When 5min elapsed
    Then cache expires
    And next request lookups D1 fresh
    And cache refilled

  Scenario: 30k property test 0 leaks
    Given proptest harness with 4 regions × 4 ops × 4 tenant types
    When 30k iter run em PR
    Then 0 cross-region leaks detected
    And INV-REGION-NO-CROSS-LEAK ratificada
    And CI gate green

  Scenario: Nightly 100k iter sustained
    Given nightly CI cron
    When 100k iter run
    Then 0 leaks detected
    And alert SEV-2 if any leak

  Scenario: KV namespace per-region cross-region access denied
    Given KV namespace corelink-session-wnam and corelink-session-weur
    When WNAM Worker tries access corelink-session-weur via direct binding
    Then compile-time error (TypeScript binding types)
    And runtime denied

  Scenario: Worker binding cross-region access compile-time error
    Given Worker binding configuration per-region
    When dev tries cross-region binding em wrangler.toml
    Then build fails
    And error message clear

  Scenario: Audit chain integrity per-region
    Given cross-region attempt audit emit
    When event written to per-region audit chain
    Then chain hash unbroken (S-09 INV-OBS-AUDIT-CHAIN-INTEGRITY)
    And per-region scope preserved

  Scenario: RB-region-leak dry-run
    Given simulated cross-region leak detection
    When dry-run executes
    Then runbook commands executable
    And forensic query produces evidence
    And customer notification template ready
    And drift findings committed
```

## 9. Design Decisions

### 9.1 Why request_region from custom domain (NÃO header)

- Header trivially spoofed by attacker.
- Custom domain authoritative (TLS-terminated by CF; cannot be tampered).
- Falls back to smart routing for `api.corelink.dev` (no region prefix); smart routing uses Geo-IP + tenant lookup.

### 9.2 Why D1 trigger immutable primary_region

- Mutation post-INSERT = re-routing existing data = catastrophic.
- Manual ticket + admin role + dual-approval (S-13) required for legitimate migration.
- Defense-in-depth (app-level + DB-level).

### 9.3 Why DO cache 5min TTL hard limit

- Reduces D1 RTT per request (1ms vs 10ms).
- 5min balances staleness vs performance.
- Cache invalidation on tenant update event (S-13 admin op fan-out).

### 9.4 Why property test 30k iter (NÃO 10k)

- INV-REGION-NO-CROSS-LEAK is CRITICAL severity (Schrems II).
- 10k iter = ~99.99% confidence; 30k = ~99.999%.
- Coverage matrix 64 combinations × 469 iter avg = 30k.
- Nightly 100k iter for sustained confidence.

### 9.5 Why KV per-region namespace prefix (composed with WI-S14-001)

- KV is globally distributed by CF; without scoping = FM-054 cross-region leak.
- Per-region binding name `corelink-session-{region}` enforces scope.
- Compile-time error via TypeScript binding types.

### 9.6 Why audit emit + métrica + alert on cross-region attempt

- Detection essential (zero tolerance).
- Audit chain forensic (impact assessment).
- Métrica triggers alert + dashboard panel.
- Alert SEV-2 on counter > 0 (zero baseline).

### 9.7 Why ADR potencial?

- Sim — **ADR-XXXX**: "Tenant region pinning enforcement + DO region_enforcer + 30k property test + INV-REGION-NO-CROSS-LEAK S-14". Decisão arquitetural foundational; reuse pattern em APAC/AFR forward.

## 10. Completeness Criteria SOTA

- [ ] **10.s14.002.1** D1 schema migration applied + immutable trigger + backfill (EVT-013).
- [ ] **10.s14.002.2** Tower layer region_check wired AFTER auth + BEFORE handler (EVT-002).
- [ ] **10.s14.002.3** DO region_enforcer per-region instance + 5min TTL cache (EVT-013).
- [ ] **10.s14.002.4** KV namespace per-region scope enforced em Worker binding (EVT-002).
- [ ] **10.s14.002.5** Property test 30k iter PR + 100k nightly → 0 leaks (EVT-002).
- [ ] **10.s14.002.6** CI gate ativo region_pinning.yml (EVT-013).
- [ ] **10.s14.002.7** Runbook RB-region-leak committed + dry-run (EVT-017).
- [ ] **10.s14.002.8** Adversarial regression tests 6+ scenarios green (EVT-040).
- [ ] **10.s14.002.9** Integration test E2E 4 regions × 4 tenant types green (EVT-024).
- [ ] **10.s14.002.10** SAST clean (cargo-audit + cargo-deny + clippy `-D warnings`) (EVT-002).
- [ ] **10.s14.002.11** INV-REGION-NO-CROSS-LEAK CRITICAL ratificada em registry §3.12 (EVT-022).
- [ ] **10.s14.002.12** SOC 2 CC6.1 + Schrems II + LGPD Art. 33 §1º + GDPR Art. 46 attestation (EVT-044).

## 11. DoD

- [ ] Crate `corelink-region-enforcer` compila + tests verde.
- [ ] D1 migration applied + immutable trigger.
- [ ] Tower layer region_check wired + ordering verified.
- [ ] DO region_enforcer per-region.
- [ ] KV namespace per-region scope enforced.
- [ ] Property test 30k iter PR green; 100k nightly green.
- [ ] CI gate ativo.
- [ ] Adversarial tests 6+ scenarios green.
- [ ] Integration tests 4 regions × 4 tenant types green.
- [ ] Métricas + audit emission operational.
- [ ] Runbook RB-region-leak dry-run.
- [ ] ADR-XXXX (region pinning) escrito + ratificado.
- [ ] Code review (Architect + Security Lead + Privacy Officer + Compliance).

## 12. Invariants Validated

### Mantidas

- **INV-DATA-RESIDENCY** (CRITICAL — registry §3.11 herdada): runtime enforcement em insert checks; this WI ratifies.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL — registry §3.14 herdada S-03): cross-region audit emit em D1 atomic batch.
- **INV-DEDUP-CONSISTENCY** (HIGH — registry §3.12 herdada S-07): per-tenant dedup prevents cross-region chunk leak.

### Novas

- **INV-REGION-NO-CROSS-LEAK** (CRITICAL — registry §3.12 NEW): este WI implementa primary; 30k property test 0 leaks; CI gate ativo. Why: Schrems II + LGPD Art. 33 baseline; gap = catastrophic legal. How: insert checks + property test + custom domain routing.

TLA+ alignment: registry §4.2 indica `region_residency.tla` PLANNED S-14 WI-S14-009; este WI provê runtime implementation.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate corelink-region-enforcer | `crates/corelink-region-enforcer/src/lib.rs` | Rust |
| Tower layer region_check | `crates/corelink-worker/middleware/region_check.rs` | Rust |
| DO region_enforcer | `crates/corelink-do-region-enforcer/src/lib.rs` | Rust |
| D1 migration | `migrations/0XX_tenant_primary_region.sql` | SQL |
| Property test | `tests/region_pinning_proptest.rs` | Rust |
| Adversarial tests | `tests/region_adversarial.rs` | Rust |
| Integration tests E2E | `tests/e2e_region_pinning.rs` | Rust |
| CI workflow | `.github/workflows/region_pinning.yml` | YAML |
| Runbook | `specs/05_runbooks/RB-region-leak.md` | Markdown |
| ADR-XXXX (region pinning) | `specs/03_architecture/adrs/ADR-XXXX-region-pinning-enforcement.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s14.002.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s14.002.2** rustdoc 100% public API + 4 examples (one per region).
- **14.s14.002.3** Test coverage ≥ 95% (`cargo tarpaulin`); property tests 30k+100k.
- **14.s14.002.4** Latência: middleware overhead ≤ 1ms p99 (DO cache); D1 fallback ≤ 10ms p99.
- **14.s14.002.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` clean.
- **14.s14.002.6** Métricas RED + cross-region attempts.
- **14.s14.002.7** Runbook RB-region-leak committed + dry-run.
- **14.s14.002.8** Breaking changes em `Region` enum = bump major + ADR + migration.
- **14.s14.002.9** Memory bounded; DO cache LRU eviction.
- **14.s14.002.10** Cost regression gate em CI.
- **14.s14.002.11** SOC 2 CC6.1 + Schrems II + LGPD Art. 33 §1º + GDPR Art. 46 attestation.
- **14.s14.002.12** Constant-time tenant_id compare (subtle::ConstantTimeEq) em DO cache.

## 15. Chaos Experiments

1. **Subdomain spoofing red team**: attacker tries 1000 cross-region attempts; verify 100% rejected 403 + audit + alert.

2. **Header tampering red team**: attacker tries 1000 header spoof attempts; verify header ignored.

3. **DO cache poisoning attempt**: admin tries UPDATE primary_region; verify trigger rejects.

4. **D1 row direct UPDATE attempt**: admin tries direct SQL UPDATE; verify trigger rejects.

5. **KV cross-region access**: WNAM Worker tries read corelink-session-weur; verify compile-time + runtime denial.

6. **Property test stress**: run 1M iter sustained 24h; verify 0 leaks.

7. **Cache stale > 5min**: simulate D1 outage; cache stale > 5min; verify D1 fallback or 503 (no stale serve).

8. **Backup region drift**: verify backup destination = same-region R2 audit bucket.

9. **Audit chain per-region integrity**: verify cross-region attempt logged em primary_region audit chain.

10. **RB-region-leak dry-run**: full incident response simulation; drift findings + customer notification template.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-14 ship gate é WI-S14-009; este WI passa por mini-PRR Architect + Security Lead + Privacy Officer + Compliance Officer review):

- [ ] All 12 Gherkin scenarios green.
- [ ] Property tests 30k + 100k iter green.
- [ ] Adversarial tests 6+ scenarios green.
- [ ] Integration tests 4 regions × 4 tenant types green.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-REGION.
- [ ] ADR-XXXX (region pinning) published.
- [ ] Privacy Officer review (LGPD + GDPR alignment).
- [ ] Compliance Officer review (Schrems II + DPA evidence pack).
- [ ] INV-REGION-NO-CROSS-LEAK ratificada em registry.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | D1 migration tenants.primary_region + immutable trigger + backfill | 1.5h |
| ST-002 | Crate corelink-region-enforcer scaffold + RegionEnforcer trait | 2h |
| ST-003 | Tower layer region_check wiring AFTER auth | 1.5h |
| ST-004 | DO region_enforcer per-region instance + 5min TTL cache | 2h |
| ST-005 | KV namespace per-region scope (composed) | 0.5h |
| ST-006 | Audit emit + métricas + trace spans | 1.5h |
| ST-007 | Property test 30k iter (4 regions × 4 ops × 4 tenant types) | 3h |
| ST-008 | Adversarial regression tests 6+ scenarios | 2h |
| ST-009 | Integration tests E2E 4 regions × 4 tenant types | 2.5h |
| ST-010 | CI workflow region_pinning.yml + nightly 100k iter | 1h |
| ST-011 | Runbook RB-region-leak escrita + dry-run | 1.5h |
| ST-012 | ADR-XXXX redação | 1.5h |
| ST-013 | Code review (Architect + Security Lead + Privacy + Compliance) | 2h |

**Total Optimistic**: ~22.5h. **PERT** (O=12h, M=18h, P=28h, per spec contract §12): **18.7h**.

## 18. Dependencies

### Hard blockers

- **WI-S14-001 SEALED** (4 regions infra; per-region DO + KV + R2 + D1).
- **S-03 SEALED** (TenantCtx propagation; auth foundation).
- **S-09 SEALED** (audit chain per-region).

### Soft blockers

- **S-13 SEALED** (admin API for legitimate region migration manual ticket flow).

### Outbound

- **WI-S14-003** (failover routing consume region_check middleware).
- **WI-S14-004..007** (BYOK + erasure attestation respect region pinning).
- **WI-S14-009** (TLA+ region_residency formalizes this implementation; pentest validates).

## 19. Effort PERT

O: 12h, M: 18h, P: 28h → PERT **18.7h** (per spec contract §12).

## 20. Time-boxing

**24h hard limit owner**. If exceeded → escalation: split em sub-WI (middleware vs property test vs runbook).

## 21. Observability

5 métricas listadas §6.1.9. Trace spans em §6.1.10. Logs structured JSON; nivel INFO em ok, WARN em cache stale, ERROR em cross-region rejected.

Dashboard widget DASH-REGION:
- Cross-region attempts blocked counter (alert > 0 SEV-2).
- DO cache hit/miss ratio per region.
- Region enforcer lookup duration p99.
- Property test pass/fail status (PR + nightly).

## 22. Cost Analysis

- D1 schema migration: one-time.
- DO region_enforcer per-region: ~$15/mês × 4 = $60/mês (composed with WI-S14-001).
- D1 query overhead: bounded by 5min TTL cache (DO storage); ~$5/mês.
- Property test CI compute: nightly 100k iter ~$5/mês.
- **Total custo direto WI-S14-002**: ~$70/mês incremental + composed with WI-S14-001 per-region DO.

## 23. API Contract

`RegionEnforcer` é internal Rust trait. Tower layer transparent ao handler. Custom domain `{region}.api.corelink.dev` é API boundary; documented em OpenAPI spec.

API semver stable post v1.0; breaking changes em `Region` enum = bump major + ADR + migration plan.

## 24. Post-mortem Hooks

- Cross-region read blocked > 0 sustained 1h → SEV-2 + post-mortem.
- Property test failure em PR → block merge + investigation.
- Property test failure nightly → CRITICAL post-mortem + INV review.
- DO cache poisoning detected → CRITICAL + Security incident.
- D1 trigger bypass detected → CRITICAL + DB integrity review.

## 25. Rollback / Recovery

- Code rollback: revert PR + redeploy Worker.
- D1 migration rollback: drop trigger + drop column (backwards-compat retain via NULL).
- RTO ≤ 30 min (Worker rollback).
- RPO 0 (D1 + audit chain preserve all data).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: subdomain spoofing rejected via custom domain authoritative; header ignored.
- **Tampering**: D1 trigger immutable post-INSERT; admin API requires manual ticket.
- **Repudiation**: cross-region attempt audit emit + 7y retention.
- **Information disclosure**: residency enforced; cross-region read blocked = 403 + alert.
- **DoS**: middleware overhead ≤ 1ms p99; cache 5min TTL.
- **Elevation of privilege**: admin API requires admin role + dual-approval (S-13).

**LINDDUN delta**:
- **Linkability**: tenant_id em audit (compliance).
- **Identifiability**: region em audit (intentional Schrems II evidence).
- **Non-repudiation**: cross-region attempt forensic evidence.
- **Detectability**: counter alert > 0 SEV-2.
- **Disclosure**: residency commitment per region em DPA (WI-S14-008).
- **Unawareness**: customer notified at signup of region selection.
- **Non-compliance**: SOC 2 CC6.1 + Schrems II + LGPD Art. 33 §1º + GDPR Art. 46 satisfied.

## 27. Knowledge Transfer

- `crates/corelink-region-enforcer/README.md` — overview + insert check pattern.
- ADR-XXXX — region pinning ratification.
- Doc `docs/internal/multi-region-byok.md` (region pinning section) — sequence diagram middleware ordering.
- Workshop interno (1h) com Architect + Security Lead + Privacy + Compliance pós-merge.
- Onboarding test (5 questions): immutable trigger, custom domain authoritative, 5min TTL, property test 30k, audit emit.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Cross-region leak via insert check bypass | L | H | CRITICAL | M | LOW | Tower ordering + property test 30k + audit |
| R-002 | DO cache poisoning | L | M | HIGH | L | LOW | 5min TTL hard + invalidation event + D1 fallback |
| R-003 | Subdomain spoofing | M | L | HIGH | M | LOW | Custom domain authoritative + audit + alert |
| R-004 | Header tampering | L | L | LOW | L | LOW | Header ignored; custom domain authoritative |
| R-005 | D1 row direct UPDATE | L | L | CRITICAL | L | LOW | Trigger rejects + admin role + dual-approval |
| R-006 | Property test false-negative | L | M | CRITICAL | L | LOW | 30k PR + 100k nightly + adversarial tests |
| R-007 | KV cross-region access | L | L | HIGH | L | LOW | Per-region binding + compile-time error |
| R-008 | Backup cross-region | L | L | HIGH | L | LOW | Same-region R2 audit bucket + integration test |
| R-009 | Audit chain cross-region leak | L | L | HIGH | L | LOW | Per-region chain (S-09) + region tag |
| R-010 | Chunk dedup cross-region | L | L | MEDIUM | L | LOW | Per-tenant dedup (INV-DEDUP-CONSISTENCY) |
| R-011 | Cache stale > 5min | M | L | LOW | L | LOW | D1 fallback + monitoring |
| R-012 | DO cache memory exhaustion | L | L | LOW | L | LOW | LRU eviction + bounded |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Security Lead review middleware ordering + DO cache strategy.
2. **D1 migration (D+1)**: DBA + Architect review trigger + backfill plan.
3. **Code (D+2)**: peer review + adversarial test scenarios.
4. **Privacy (D+3)**: Privacy Officer review LGPD + GDPR alignment.
5. **Compliance (D+3)**: Compliance Officer review Schrems II + DPA evidence pack.
6. **Property test (D+4)**: pre-merge 30k iter green; nightly 100k green.
7. **Adversarial (D+4)**: red team session — subdomain spoof + header spoof + cache poison.
8. **PRR mini (D+5)**: Architect + Security Lead + Privacy + Compliance sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — middleware ordering + DO cache + property test design + immutable trigger_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — subdomain spoofing + admin API mutation defense + audit emit_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — DO cache + RB-region-leak + alert SEV-2 wiring_ | _pending_ | _pending_ |
| 6 | Engineer (S-14 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD; emphatic — property test 30k + 100k nightly + coverage matrix_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — Schrems II + DPA evidence pack + LGPD Art. 33 attestation_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; emphatic — LGPD + GDPR Art. 46 + EDPB Recommendations alignment_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — adversarial scenarios + cache poisoning + replay protection_ | _pending_ | _pending_ |

> Crypto SME folds into Architect role specialization (cripto-touching WIs S-14-004+; this WI is application middleware foundation). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S14-002 (cycle 12.S14.0). |

## 32. Anti-patterns evitados

- Skip Tower layer ordering (region_check antes handler).
- Header authoritative (spoofing trivial).
- Skip property test 30k iter PR.
- Skip nightly 100k iter.
- Skip D1 immutable trigger.
- Allow primary_region UPDATE post-INSERT.
- Skip audit emit on cross-region.
- Skip alert SEV-2 on counter > 0.
- Skip RB-region-leak dry-run.
- DO cache TTL > 5min (staleness exposure).
- Skip per-region KV namespace scope.
- Skip adversarial regression tests.

---

**Fim WI-S14-002.** Próximo: WI-S14-003 (hot blob replica + read failover PAT-REGION-FAILOVER-001).
