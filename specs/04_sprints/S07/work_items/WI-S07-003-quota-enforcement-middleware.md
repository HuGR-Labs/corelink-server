---
id: "WI-S07-003"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "STANDARD"
parent: "S-07"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DATA-MODEL"
  - "SECURITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s07", "quota", "middleware", "tower", "do-atomic", "rate-limit", "standard"]
---

# WI-S07-003 — Quota Enforcement Middleware (Tower layer; pre-write check `bytes_used + request_bytes ≤ tenant_quota.max_storage_bytes`; DO atomic counter via `quota-{tenant_id}` durable object pessimistic-check eliminating FM-059 race; 95% threshold triggers WI-S07-002 eviction; 100% returns `TENANT_QUOTA_EXCEEDED` 429 + Retry-After; latency adds ≤ 3ms p99 to write path)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-07](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S07-003 |
| Título | Tower middleware quota enforcement bound em write path (CAS PUT WI-S01-001, AC UpdateActionResult WI-S04-001, multipart SplitBlob WI-S05-001); pre-check `bytes_used + request_bytes ≤ tenant_quota.max_storage_bytes`; DO atomic counter via `quota-<tenant_id>` durable object pessimistic-check eliminating FM-059 race; 95% threshold triggers WI-S07-002 ad-hoc eviction; 100% returns 429 `TENANT_QUOTA_EXCEEDED` + Retry-After; latency adds ≤ 3ms p99 to write path; INV-QUOTA-ENFORCEMENT property test verified |
| Sprint | S-07 |
| Lane | STANDARD |
| Forcing factors | none directly; INV-QUOTA-ENFORCEMENT (HIGH) inherited |

## 1. Intent

Quota middleware é a **gate de admission** no write path — bloqueia tenant ao atingir 100% `max_storage_bytes`; aciona eviction worker (WI-S07-002) ao atingir 95%; race-free via DO atomic counter (eliminates FM-059 quota race condition que sob alto throughput permite over-quota writes):

```rust
// File: crates/corelink-quota/src/lib.rs

#![forbid(unsafe_code)]

#[async_trait]
pub trait QuotaEnforcer: Send + Sync {
    /// Pre-write check: returns Ok(()) if request fits within quota;
    /// Err(QuotaError::Exceeded) if would-overflow.
    /// Atomic via DO actor; race-free under high concurrency (FM-059 eliminated).
    async fn check_and_reserve(
        &self,
        tenant_ctx: &TenantCtx,
        request_bytes: u64,
    ) -> Result<QuotaCheckResult, QuotaError>;

    /// Post-write commit: increments durable counter (called only after R2 PUT + D1 INSERT succeed).
    /// Reservations not committed within TTL (default 60s) auto-released.
    async fn commit_reservation(
        &self,
        tenant_ctx: &TenantCtx,
        reservation_id: ReservationId,
    ) -> Result<(), QuotaError>;

    /// Rollback reservation if write fails post-check.
    async fn release_reservation(
        &self,
        tenant_ctx: &TenantCtx,
        reservation_id: ReservationId,
    ) -> Result<(), QuotaError>;
}

pub struct QuotaCheckResult {
    pub bytes_used_before: u64,
    pub bytes_used_after: u64,                    // after reservation
    pub max_storage_bytes: u64,
    pub utilization_pct: f64,                     // bytes_used_after / max_storage_bytes
    pub reservation_id: ReservationId,
    pub trigger_eviction: bool,                   // true if utilization ≥ 95%
}

#[derive(thiserror::Error, Debug)]
pub enum QuotaError {
    #[error("tenant quota exceeded: would_use {would_use} > max {max} (request +{request} bytes)")]
    Exceeded { would_use: u64, max: u64, request: u64 },

    #[error("DO backend error: {0}")]
    DoBackendError(String),

    #[error("D1 backend error: {0}")]
    D1BackendError(String),

    #[error("reservation {0} not found (TTL expired or never created)")]
    ReservationNotFound(ReservationId),

    #[error("audit emission failed; quota check fail-closed")]
    AuditEmissionFailed,
}
```

**Cripto-driven invariants enforced**:

1. **INV-QUOTA-ENFORCEMENT** (HIGH; sprint contract §8): tenant real-time check; atomic via DO actor model (FM-059 eliminated):
   - DO `quota-<tenant_id>` é singleton per tenant; serializes all check-and-reserve operations.
   - State em DO storage: `bytes_used`, `pending_reservations: HashMap<ReservationId, (bytes, expires_at_ms)>`.
   - Pessimistic check: `bytes_used + sum(pending_reservations.bytes) + request_bytes ≤ max`.

2. **Reservation pattern** (eliminates FM-059; **size-proportional TTL** Lote 10.7bis Sonnet R5 P0-2 fix):
   - Pre-write: `check_and_reserve(req_bytes)` → DO atomic increment pending counter; returns ReservationId + size-proportional TTL.
   - **Size-proportional TTL formula** (Lote 10.7bis R5 P0-2 fix; was hard-coded 60s — too short for multipart 160 GiB ~218min @ 100 Mbps):
     - `ttl_seconds = max(60, (request_bytes / MIN_UPLOAD_RATE_BYTES_PER_SEC) * 2)` — 2× safety factor.
     - `MIN_UPLOAD_RATE_BYTES_PER_SEC = 1_000_000` (1 MB/s lower-bound; CF Workers slow client tolerated).
     - At 1 GiB request: `1_073_741_824 / 1_000_000 * 2 = 2147s ≈ 36min` TTL.
     - At 160 GiB multipart: `171_798_691_840 / 1_000_000 * 2 = 343597s ≈ 95h` TTL — bounded em hard cap of 7d (604800s) to prevent indefinite reservation leak from abandoned uploads.
     - Hard cap: `min(ttl_seconds, 604800)` — 7d max; aligns com S-05 multipart_sessions sweeper window.
   - **Heartbeat extension** (alternative for multipart sessions; more rigorous): on each `UploadPart` RPC (S-05), re-extend reservation TTL via `extend_reservation(id, +TTL)` heartbeat — clean semantic, bounded operationally.
   - Post-write success: `commit_reservation(id)` → DO moves pending → committed (`bytes_used += req_bytes`); decrements pending.
   - Post-write fail OR TTL expired: `release_reservation(id)` → DO removes pending entry; bytes never counted.
   - **Race-free**: DO actor model serializes; concurrent writes can't both pass check_and_reserve at boundary.

3. **TenantCtx-only enforcement** (Lote 10.4bis lesson): tenant_id from middleware; NEVER from request body.

3-bis. **DO routing via tenant.primary_region** (Lote 10.7bis R4 P0-9 fix — multi-region tenant cross-region inconsistency previously dismissed): DO ID `quota-<tenant_id>` resolved within tenant's primary region (per `tenant.primary_region` em data_model.md line 152). Worker→DO binding routes via primary_region edge; cross-region writes hit primary_region DO via Cloudflare backbone (~50-100ms RTT cross-region tolerated since primary region pinned per tenant). For GA: each tenant pinned to ONE primary region; multi-region tenant scenarios deferred to S-14 BYOK + multi-region replication. SLA p99 ≤ 3ms holds within primary region; cross-region requests degraded but rare (most tenants single-region per pricing tier defaults).

4. **Audit fail-closed** (Lote 10.6bis pattern): each check + commit + release audit-emitted; if audit fails on critical path (commit), reservation auto-expires (does NOT count toward quota — fail-closed bias toward over-counting NOT under-counting).

5. **D1 source-of-truth periodic sync** (Lote 10.7bis P0-2 fix — uses NEW `tenant_storage_state` table; was incorrectly referencing phantom `tenant_quota.bytes_used`): DO state synced to D1 `tenant_storage_state` (PRIMARY KEY tenant_id; columns bytes_used + bytes_used_updated_at + last_synced_at) every 5min (eventual consistency); on DO restart, recover from D1 (cold start); intermediate writes via DO authoritative. **Schema separation rationale** (R4 P0-2 option B): `tenant_quota` is POLICY (max_storage_bytes immutable per period); `tenant_storage_state` is STATE (mutable running counter). Migration `00X_tenant_storage_state.sql` creates table + backfills bytes_used via reconcile from `blob_meta` aggregate `SUM(size_bytes) WHERE deleted_at IS NULL` per tenant.

## 2. Narrative (≥ 200 palavras + race-free justification)

Quota enforcement é **the gate em write path** — sem isso, billing leak (cliente excede plan; storage cost ≥ revenue); com race condition (FM-059), high-throughput tenant escapes ao breach 100%. DO actor model eliminates race: cada tenant tem `quota-<tenant_id>` DO singleton; check-and-reserve é atomic (CF DO single-threaded actor); concurrent writes serializam at DO boundary.

**Reservation TTL pattern** é load-bearing: middleware pre-checks + reserva → write happens → commit OR release. Se write crashes (Worker timeout, R2 503), reservation expira em 60s e é auto-released (não-leak). Comparado com naive "increment counter atomic on write success": com naive, race window é entre check (D1 SELECT bytes_used) e commit (D1 UPDATE +=); concurrent writes ambos pass check em ~1% sob 1k QPS → over-quota by 1%. Reservation pattern fecha essa race.

**95% threshold ad-hoc eviction** é mecanismo proativo: ao detectar utilization ≥ 95%, middleware fires `WI-S07-002::execute_quota_trigger` antes do hit hard 100%; reduces customer-visible 429 frequency.

**100% hard-block** retorna `TENANT_QUOTA_EXCEEDED` 429 + `Retry-After` header (per HTTP spec); cliente Bazel/Buck2 retries automaticamente; UX degradado mas previsível. **Hard-block é S-08 CAP-QUOTA-001 territory** (sprint contract §1 explicit boundary): S-07 owns ≤ 95% (eviction); S-08 owns 100% (rate-limit/429 layer). Middleware emite 429 mas hard-block infrastructure (rate-limit DO) é S-08. Por enquanto WI-S07-003 implementa 429 emit; S-08 enrich com rate-limit per-tenant.

**Adversarial scenarios**:
- **High-throughput race** (FM-059): 100 concurrent writes ao 99% quota; sem DO atomic, todas passam check; over-quota 100×. Mitigação: DO actor + reservation pattern.
- **Reservation leak**: write crashes; reservation never committed nor released; counts toward quota indefinitely. Mitigação: 60s TTL auto-release; D1 reconcile diário (S-09 forward) catches drift.
- **DO restart amid pending reservations**: DO state lost; pending reservations forgotten. Mitigação: DO durable storage (pending reservations persisted); cold start recovers from D1 base + replay reservations from durable storage.
- **Tenant tier upgrade mid-quota**: max_storage_bytes increases atomic; in-flight reservations honored under new max. Mitigação: tenant_quota.updated_at watermark; DO refreshes max from D1 on each check.

**Risk justification STANDARD**:
- INV-QUOTA-ENFORCEMENT (HIGH) inherited; not CRITICAL.
- Race-free via well-known DO actor pattern (Cloudflare reference architecture).
- Property test 10k iter required (sprint contract §6 DoD).
- FM-059 dry-run RB-FM-059 mandatory (sprint contract §R-S07-9).

## 3. Customer Impact & Journey

**Persona 1 — Customer (free tier at 99% quota)**: Bazel pushing new build; middleware detects 99.5% post-reservation; trigger eviction (WI-S07-002); reclaims 5GB; tenant drops to 89%; subsequent writes proceed. Customer-visible: latency spike ~500ms once (eviction sync); subsequent writes normal.

**Persona 2 — Customer (free tier at 100% quota)**: write attempts; middleware returns 429 + `Retry-After: 3600`; cliente Bazel retries em 1h; if storage relieved (TTL expiry), succeeds. Customer-visible: explicit error (não silent ignore).

**Persona 3 — DevOps reviewing 429 rate**: DASH-DEDUP shows quota-breach events per-tenant; tenant exceeding ≥10× WoW = upgrade-tier candidate (revenue signal).

**SLA addendum**:
- Quota check latency adds ≤ 3ms p99 to write path (sprint contract §10.s07.3).
- 429 + Retry-After header per RFC 7231 §6.6.4.
- 95% threshold ad-hoc trigger latency: middleware-to-eviction-decision ≤ 500ms p99.
- INV-QUOTA-ENFORCEMENT 0 violations (property test 10k + chaos race).

## 4. Capability Mapping

- **CAP-EVICT-003** (Soft-pressure 95%) — IMPLEMENTA primary trigger; hard-block 100% delegado a S-08 CAP-QUOTA-001.
- **CTRL-QUOTA-001** (Storage quota per-tenant) — IMPLEMENTA primary enforcement; D1 `tenant_quota` source-of-truth.
- Trace: `data_model.md §4.2 tenant_quota` + `failure_modes.md FM-059` + `security_model.md CTRL-QUOTA-001` + `invariant_registry.md INV-QUOTA-ENFORCEMENT`.

## 5. Tipo

Tower middleware + DO singleton per tenant; STANDARD lane.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-quota/` module** — QuotaEnforcer trait + Tower middleware impl + DO impl + tests.
2. **Tower middleware layer** em write path (mounted before handler; AFTER auth_stack S-03):
   ```rust
   pub fn quota_layer<S>() -> tower::Layer<S> {
       // ... wraps service with quota check; on Err(Exceeded), responds 429 with Retry-After.
   }
   ```
   Mounted em:
   - WI-S01-001 CAS PUT handler (request_bytes = blob.size_bytes).
   - WI-S04-001 UpdateActionResult handler (request_bytes = sum(output_files[].size_bytes)).
   - WI-S05-001 SplitBlob handler (request_bytes = blob.size_bytes; per-blob check; chunks contado as part of SplitBlob result).
3. **DO singleton per tenant** `quota-<tenant_id>`:
   - State em DO storage: `bytes_used: u64, pending_reservations: HashMap<ReservationId, ReservationEntry>, max_storage_bytes: u64, last_d1_sync_at_ms: u64`.
   - `ReservationEntry { bytes: u64, expires_at_ms: u64, created_at: u64 }`.
   - DO alarm cleanup: every 60s scan `pending_reservations`; remove entries with `expires_at_ms < now`.
   - DO alarm re-arm AT START (Lote 10.4bis lesson).
4. **Reservation lifecycle**:
   - **check_and_reserve**:
     - Read tenant_quota.max_storage_bytes from D1 (cached 5min em DO).
     - Compute would_use = bytes_used + sum(pending_reservations.bytes) + request_bytes.
     - If would_use > max → Err(Exceeded); audit emit `corelink.quota.exceeded`.
     - Else: generate ReservationId (UUIDv7); insert pending_reservations[id] = (request_bytes, now + 60s); return Ok(QuotaCheckResult).
     - If utilization_pct ≥ 0.95 → set `trigger_eviction = true` (caller invokes WI-S07-002 trigger).
   - **commit_reservation**:
     - Look up pending_reservations[id]; if not found → Err(NotFound).
     - Move from pending to committed: `bytes_used += entry.bytes`; remove from pending.
     - Audit emit `corelink.quota.committed`.
   - **release_reservation**:
     - Look up pending_reservations[id]; if not found → return Ok (idempotent — already TTL-released).
     - Remove from pending; audit emit `corelink.quota.released`.
   - **TTL auto-release** (DO alarm): scan every 60s; remove pending with `expires_at_ms < now`.
5. **D1 sync** (eventual consistency):
   - Every 5min, DO writes `tenant_storage_state.bytes_used` to D1 source-of-truth.
   - On DO cold start (worker restart): read `bytes_used` from D1; replay durable pending_reservations (DO storage); D1 + DO converge.
6. **429 response with Retry-After**:
   - Status 429 Too Many Requests (per RFC 7231).
   - `Retry-After: 3600` header (default 1h; tunable per tier).
   - Body: JSON `{ "error": "TENANT_QUOTA_EXCEEDED", "details": { "would_use": ..., "max": ..., "retry_after_seconds": 3600 } }`.
   - Error code: `COR_S07_QUOTA_EXCEEDED` (NEW; add to error_taxonomy.md §15).
7. **95% threshold trigger**:
   - check_and_reserve returns `trigger_eviction = true`.
   - Middleware async-spawns `WI-S07-002::execute_quota_trigger(tenant_ctx, target_bytes_to_reclaim)`.
   - Target: reclaim until 90% (= reduce by ~5% of max).
   - Trigger latency budget 500ms p99; if exceeds, write proceeds (eviction continues async).
8. **TenantCtx-only** (Lote 10.4bis lesson): tenant_id from TenantCtx; `quota-<tenant_id>` DO ID derived from `(region, tenant_id)` deterministic.
9. **Audit fail-closed** (Lote 10.6bis pattern): each check/commit/release audit-emit; on audit fail at commit, ROLLBACK reservation (release).
10. **Métricas**:
    - `corelink.quota.check_total{result=ok|exceeded}` (counter).
    - `corelink.quota.check_duration_us` (histogram; SLO ≤ 3ms p99).
    - `corelink.quota.committed_bytes_total{tenant_id}` (counter).
    - `corelink.quota.released_bytes_total{tenant_id}` (counter; reservation TTL auto-release).
    - `corelink.quota.utilization_pct{tenant_id}` (gauge).
    - `corelink.quota.95pct_breach_total{tenant_id}` (counter; alert SEV-2 per-tenant).
    - `corelink.quota.100pct_breach_total{tenant_id}` (counter; alert SEV-1 per-tenant; sprint contract §6 DoD).
    - `corelink.quota.race_detected_total` (counter; INV-QUOTA-ENFORCEMENT canary; alert if > 0).
    - `corelink.quota.do_d1_sync_lag_ms` (gauge; alert if > 60s).
11. **Property tests** (10k iter PR; 100k nightly):
    - `prop_quota_atomic_no_race`: 1000 concurrent check_and_reserve at boundary 99%; assert NEVER over-quota; INV-QUOTA-ENFORCEMENT 0 violations.
    - `prop_quota_reservation_lifecycle`: check → commit OR release; bytes_used consistent.
    - `prop_quota_ttl_release`: pending never committed; auto-released after 60s; no quota leak.
    - `prop_quota_tenant_isolation`: 1000 concurrent across tenants; no cross-tenant impact.
    - `prop_quota_tier_upgrade`: tier upgrade mid-quota; max_storage_bytes refreshed; in-flight reservations honored.
12. **Chaos suite** (8 scenarios):
    - 1. **FM-059 race**: 1000 concurrent writes at 99.9% quota → DO actor serializes; 0 over-quota.
    - 2. **DO restart mid-pending**: DO killed; cold start; pending reservations recovered from durable storage.
    - 3. **D1 sync lag**: D1 down 10min; DO continues authoritative; on D1 recovery, sync completes; no data loss.
    - 4. **Reservation TTL leak**: write crashes; reservation never committed; auto-release at 60s; bytes never counted.
    - 5. **95% trigger storm**: 100 tenants reach 95% simultaneously; per-tenant trigger; no thundering herd (per-tenant DOs independent).
    - 6. **100% hard-block**: tenant at 100%; new write returns 429 + Retry-After; cliente respects header.
    - 7. **Tier upgrade mid-flight**: customer upgrades free→solo at 99% quota; max increases; pending reservations honored.
    - 8. **Audit fail-closed**: audit emit fails at commit; reservation released (NOT committed); fail-closed bias.

### 6.2 Out-of-scope (deferred)

- Hard-block infrastructure (rate-limit DO) — S-08 CAP-QUOTA-001 territory.
- Per-tenant Retry-After tuning by tier (free=3600s; enterprise=60s) — S-13 admin override.
- Quota across multi-region tenant (M tenants > 1 region) — S-14 forward.
- Predictive quota exhaustion alert ("you'll hit quota in 7d at current rate") — S-09 observability forward.

## 7. Anti-Scope

- ❌ D1-only quota check (race condition FM-059; use DO atomic).
- ❌ Skip reservation pattern (race window).
- ❌ Skip TTL auto-release (reservation leak).
- ❌ Hard-coded 60s TTL (config-driven; default 60s).
- ❌ TenantCtx bypass.
- ❌ Skip audit emit.
- ❌ Block writes ≥95% (95% triggers eviction; only 100% blocks; sprint contract §1 boundary).

## 8. Acceptance Criteria (Gherkin) — 8 scenarios

```gherkin
Feature: Quota enforcement middleware

  Scenario: Pre-write check OK (tenant at 50% quota)
    Given tenant T at 50GB used / 100GB max
    Given write request 1GB
    When middleware check_and_reserve(T, 1GB)
    Then DO atomic increment pending; bytes_used_after = 51GB
    Then trigger_eviction = false (under 95%)
    Then ReservationId returned to handler
    Then handler proceeds with R2 PUT + D1 INSERT
    Then handler invokes commit_reservation(id)
    Then DO moves pending → committed; bytes_used = 51GB

  Scenario: 95% threshold triggers eviction
    Given tenant T at 94GB used / 100GB max
    Given write request 2GB
    When middleware check_and_reserve(T, 2GB)
    Then bytes_used_after = 96GB; utilization 96% ≥ 95%
    Then trigger_eviction = true
    Then middleware async-spawns WI-S07-002::execute_quota_trigger(T, target=6GB to reclaim)
    Then eviction worker reclaims 6GB; bytes_used drops to 90GB
    Then write proceeds (reservation honored; commit increments to 92GB post-eviction)

  Scenario: 100% hard-block returns 429
    Given tenant T at 99GB used / 100GB max
    Given write request 2GB
    When middleware check_and_reserve(T, 2GB)
    Then would_use = 101GB > max 100GB
    Then Err(QuotaError::Exceeded { would_use: 101GB, max: 100GB, request: 2GB })
    Then middleware responds 429 + Retry-After: 3600
    Then body { error: "TENANT_QUOTA_EXCEEDED", details: { ... } }
    Then audit emit corelink.quota.100pct_breach
    Then SEV-1 alert fired

  Scenario: FM-059 race eliminated
    Given tenant T at 99.9GB used / 100GB max
    Given 1000 concurrent writes of 1MB each
    When all check_and_reserve concurrently via DO actor
    Then DO serializes; first ~100 succeed (99.9 + 0.1 = 100GB max)
    Then remaining 900 return Exceeded (would_use > max)
    Then INV-QUOTA-ENFORCEMENT 0 violations
    Then race_detected_total metric remains 0

  Scenario: Reservation TTL auto-release
    Given middleware reserves 1GB for write
    Given Worker crashes before commit_reservation called
    When 60s TTL elapses
    Then DO alarm scans pending_reservations
    Then expired reservation removed; bytes never counted
    Then bytes_used unchanged

  Scenario: Tier upgrade mid-quota
    Given tenant T (free, max=10GB) at 9.5GB used
    Given customer upgrades to solo (max=100GB)
    Given concurrent write request 5GB
    When tenant_quota.max_storage_bytes UPDATE'd em D1 to 100GB
    When DO refreshes max on next check (5min D1 sync OR explicit invalidate)
    Then write at 9.5GB + 5GB = 14.5GB; under 100GB; OK
    Then ReservationId returned; commit succeeds

  Scenario: Audit fail-closed at commit
    Given reservation pending (1GB)
    Given write succeeds (R2 PUT + D1 INSERT)
    Given audit_outbox INSERT fails (D1 throttle)
    When middleware commit_reservation
    Then commit ROLLBACKs (reservation released, NOT committed)
    Then bytes_used NOT incremented
    Then SEV-1 alert fired
    Then write retry must re-check quota (new reservation)

  Scenario: Quota check latency ≤ 3ms p99
    Given tenant T DO singleton warm
    Given 10k sequential check_and_reserve calls
    When measured
    Then p99 ≤ 3ms (SLO sprint contract §10.s07.3)
```

## 9. Design Decisions

- 9.1: DO actor model per tenant `quota-<tenant_id>`; race-free serialization (FM-059 mitigation).
- 9.2: Reservation pattern (60s TTL auto-release); eliminates check-then-commit race window.
- 9.3: D1 sync 5min eventual consistency; DO authoritative em hot path.
- 9.4: 95% threshold triggers eviction async-spawn; doesn't block write.
- 9.5: 100% hard-block returns 429 + Retry-After (per RFC 7231 §6.6.4).
- 9.6: Tower middleware mounted AFTER auth_stack (TenantCtx must exist).
- 9.7: TenantCtx-only enforcement (Lote 10.4bis lesson).
- 9.8: Audit fail-closed at commit (Lote 10.6bis pattern).
- 9.9: NO new ADR (ADR-0020 quota-ownership já existe).

## 10. Completeness Criteria SOTA

- [ ] **10.s07.003.1** Module compila + integration tests green.
- [ ] **10.s07.003.2** All 8 Gherkin scenarios green.
- [ ] **10.s07.003.3** Property tests 5 × 10k green; 100k nightly sustained 7d.
- [ ] **10.s07.003.4** Chaos suite 8 scenarios green; **load test push tenant até 100% quota → 429 + Retry-After** (sprint contract §6 DoD).
- [ ] **10.s07.003.5** Quota check latency ≤ 3ms p99 criterion benchmark.
- [ ] **10.s07.003.6** RB-FM-059 (DO quota exceeded) dry-run executado.
- [ ] **10.s07.003.7** Métricas (9) emitted.
- [ ] **10.s07.003.8** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s07.003.9** Cost regression gate per-check ≤ $0.0000005.
- [ ] **10.s07.003.10** INV-QUOTA-ENFORCEMENT 0 violations sustained 7d.
- [ ] **10.s07.003.11** D1↔DO sync lag ≤ 60s sustained.

## 11. DoD

- [ ] Module compila + tests green; all Gherkin/property/chaos green; alerts armed; 5 sign-offs.

## 12. Invariants Validated

- **INV-QUOTA-ENFORCEMENT** (HIGH; sprint contract §8): tenant real-time check; atomic via DO actor; 0 race violations.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): DO ID per-tenant; cross-tenant impossible.
- **INV-QUOTA-RESERVATION-TTL** (HIGH, NEW promovida §3.X): pending reservations auto-release at 60s; no quota leak.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Quota module | `crates/corelink-quota/` | Rust |
| DO singleton impl | `crates/corelink-quota/src/do_singleton.rs` | Rust |
| Tower middleware | `crates/corelink-worker/src/middleware/quota.rs` | Rust |
| Property tests | `crates/corelink-quota/tests/prop_quota.rs` | Rust |
| Chaos suite | `tests/chaos_quota.rs` | Rust |
| Wrangler DO binding | `wrangler.toml` (additions) | TOML |
| RB-FM-059 dry-run report | `specs/_audits/2026-XX-XX-rb-fm-059-dry-run-s07.md` | Markdown |
| Error taxonomy entry | `specs/03_architecture/error_taxonomy.md §15 COR_S07_*` | Markdown |

## 14. Quality Standards SOTA

- 14.s07.003.1: Zero unsafe; zero unwrap em production paths.
- 14.s07.003.2: rustdoc 100% public API.
- 14.s07.003.3: Test coverage ≥ 90%.
- 14.s07.003.4: Latência: check ≤ 3ms p99 (sprint contract §10.s07.3); commit ≤ 2ms p99.
- 14.s07.003.5: SAST clean.
- 14.s07.003.6: Métricas (9 §6.1.10).
- 14.s07.003.7: Memory bounded ≤ 1 MiB DO state per tenant.
- 14.s07.003.8: Cost regression gate per-check ≤ $0.0000005.
- 14.s07.003.9: TenantCtx-only (Lote 10.4bis).
- 14.s07.003.10: Audit fail-closed (Lote 10.6bis).
- 14.s07.003.11: Alarm re-arm AT START (Lote 10.4bis).
- 14.s07.003.12: D1 batch ≤ 250 (Lote 10.5bis; n/a here since DO state, not D1 batch).

## 15. Chaos Experiments (8)

§6.1.12 enumerated.

## 16. PRR

STANDARD lane sprint review (5 sign-offs).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + QuotaEnforcer trait | 1.5 |
| ST-002 | DO singleton state + reservation lifecycle | 3 |
| ST-003 | Tower middleware integration | 2 |
| ST-004 | check_and_reserve + commit + release impls | 2.5 |
| ST-005 | TTL auto-release alarm | 1 |
| ST-006 | D1 sync 5min + cold start recovery | 1.5 |
| ST-007 | 429 + Retry-After response | 1 |
| ST-008 | Métricas (9) emit | 1.5 |
| ST-009 | Property tests (5 × 10k) | 3 |
| ST-010 | Chaos suite (8) | 3 |
| ST-011 | RB-FM-059 dry-run | 1.5 |
| ST-012 | Error taxonomy COR_S07_QUOTA_EXCEEDED | 0.5 |

**Total**: ~22h. **PERT** O=18h M=20h P=28h: **~21h** (close to sprint contract estimate 16h; slightly above due to chaos rigor).

## 18. Dependencies

- Hard: S-03 WI-S03-003 SEALED (TenantCtx middleware); WI-S07-002 (eviction worker for 95% trigger spawn); ADR-0020 quota ownership (already exists).
- Soft: S-08 (hard-block rate-limit infrastructure forward); S-13 (admin API tier upgrade).

## 19. Effort PERT: ~21h. ## 20. Time-boxing: 28h hard limit.

## 21. Observability

9 metrics §6.1.10. Trace span `quota.{check, commit, release, ttl_cleanup, do_d1_sync, eviction_trigger}`. Custom log structured `corelink.quota.decision{tenant_id, request_bytes, bytes_used_after, utilization_pct, result}` (1% sampled).

## 22. Cost Analysis

- Per-check: ~$0.0000005 (DO read + 1 D1 cache read).
- Per-commit: ~$0.0000010 (DO write + audit emit).
- TCO 12m: 5 regions × 1k tenants × 1k writes/dia × 365 dias × $0.0000010 = ~$1830/yr.
- Hot-path overhead: 3ms × 100M writes/yr = 300k seconds compute = ~$25/yr (CF Workers Unbound).

## 23. API Contract

- Public: `QuotaEnforcer` trait + `QuotaCheckResult`, `QuotaError`, `ReservationId` types; `#[non_exhaustive]`.
- HTTP: 429 + Retry-After per RFC 7231 §6.6.4.
- Error: `COR_S07_QUOTA_EXCEEDED` (NEW; add error_taxonomy.md §15).

## 24. Post-mortem Hooks

- 100pct_breach_total spike WoW > 10× → revenue/billing review (legitimate growth vs bug).
- race_detected_total > 0 → INV-QUOTA-ENFORCEMENT violation; CRITICAL post-mortem (FM-059).
- D1↔DO sync lag > 60s sustained → SEV-1 + reconcile.
- Customer "billed for unused storage" complaint → reservation leak investigation.

## 25. Rollback / Recovery

- Rollback: revert Tower middleware mount; quota check disabled; tenant unrestricted (cost overhead but no breakage).
- Recovery: DO state recoverable from durable storage; D1 sync reconciles.
- RTO: 5min (revert).
- RPO: 0 (DO durable storage).

## 26. Security & Privacy

**STRIDE delta**:
- **T (Tampering)**: tenant_quota.max_storage_bytes admin-only via S-13 admin API (signed); DO state protected by Worker isolation.
- **R (Repudiation)**: every check + commit + release audit-emitted.
- **D (DoS)**: middleware adds 3ms; bounded; no amplification.
- **E (EoP)**: TenantCtx ensures tenant can't query other's quota.

**LINDDUN**:
- Per-tenant DO; no cross-tenant linkability via quota state.

## 27. Knowledge Transfer

Tech talk (1h): "S-07 Quota: DO Actor + Reservation Pattern Eliminates FM-059"; doc `docs/dev/quota-architecture.md`; onboarding test 5 questions: FM-059 race, reservation TTL, 95% trigger boundary, 100% hard-block boundary (S-07 vs S-08 ownership), DO actor serialization.

## 28. Risk Register (8-row)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | FM-059 race condition | M | M | HIGH | M | LOW | DO atomic + reservation pattern; property test |
| R-002 | Reservation leak (write crashes pre-commit) | M | L | LOW | L | LOW | 60s TTL auto-release |
| R-003 | DO restart loses pending state | L | L | LOW | L | LOW | DO durable storage; D1 reconcile |
| R-004 | D1↔DO sync lag > 60s | L | M | MEDIUM | L | LOW | Alert SEV-2; reconcile job |
| R-005 | False 429 (legitimate write blocked) | L | L | MEDIUM | L | LOW | Retry-After bounded; eviction relief |
| R-006 | Customer upgrade race (max change mid-write) | L | L | LOW | L | LOW | DO refresh max on each check (5min) |
| R-007 | Audit fail-closed false positive | L | M | MEDIUM | L | LOW | Retry audit; eventual ROLLBACK |
| R-008 | DO cold start latency > 3ms p99 | L | M | MEDIUM | L | LOW | DO sticky per region; warm cache |

## 29. Review Checkpoints

D+0 design (Architect; race analysis FM-059); D+2 AppSec (TenantCtx + audit); D+4 code review; D+6 chaos validation (race + TTL); D+7 sprint review.

## 30. Sign-off (STANDARD 5)

| # | Role | Status |
|---|---|---|
| 1 | Owner / Final Approver (Gustavo) | _pending_ |
| 2 | Engineer (peer) | _TBD; mandatory_ |
| 3 | QA | _TBD; mandatory — chaos race + property test 0 violations_ |
| 4 | AppSec | _TBD; mandatory — TenantCtx + audit + DO isolation_ |
| 5 | SRE | _TBD; mandatory — RB-FM-059 dry-run + alerts armed_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.7) | Criação WI-S07-003; SOTA pós-Lote 10.6-tris lessons absorbed: TenantCtx-only; audit fail-closed; alarm re-arm at start; DO actor + reservation pattern (FM-059 elimination); 95% trigger eviction (S-07) + 100% hard-block (S-08 boundary); error taxonomy COR_S07_QUOTA_EXCEEDED. |

## 32. Anti-patterns evitados

- ❌ D1-only quota check (race); ❌ Skip reservation; ❌ Skip TTL auto-release; ❌ Hard-coded 60s TTL; ❌ TenantCtx bypass; ❌ Skip audit; ❌ Block ≥95% (95% triggers, 100% blocks); ❌ Skip Retry-After header.

---

**Fim WI-S07-003.** Próximo: WI-S07-004 (last_accessed_at hot path + race property test).
