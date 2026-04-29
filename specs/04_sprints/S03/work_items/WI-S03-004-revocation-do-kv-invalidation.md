---
id: "WI-S03-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005", "FF-HR-009"]
parent: "S-03"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
tags: ["wi", "s03", "auth", "revocation", "durable-object", "kv-invalidation", "high-risk"]
---

# WI-S03-004 — Revocation Durable Object + KV Cache Invalidation + Cross-Region Propagation ≤ 60s p99

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-03](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S03-004 |
| Título | Revocation lifecycle: Durable Object SoT + KV cache invalidation + cross-region propagation ≤ 60s p99 + audit emission |
| Sprint | S-03 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (revocation gap = continued cross-tenant access via comprometido PAT), FF-HR-005 (SLO-FRESH-PAT-REVOKE é availability-correctness boundary), FF-HR-009 (defense-in-depth coordination) |

## 1. Intent

Implementar revocation lifecycle production-grade que garante PAT/JWT comprometido **deixa de ser aceito globalmente em ≤ 60s p99**, com audit trail completo:

```rust
// crates/corelink-worker/src/auth/revocation.rs

/// Durable Object — single source of truth para revocation state per-region.
/// Cross-region propagation via async broadcast (CF Cron + queue OR DO RPC).
pub struct RevocationDo {
    /// Persistent revocation list: pat_id → RevokedEntry
    storage: DurableObjectStorage,
    region: Region,
}

#[derive(Serialize, Deserialize)]
pub struct RevokedEntry {
    pat_id: PatId,                 // OR clerk_session_id (auth_method-agnostic)
    revoked_at: SystemTime,
    revoked_by: PrincipalId,       // who initiated revoke
    reason: RevocationReason,
    propagation_status: PropagationStatus,
}

#[derive(Serialize, Deserialize)]
pub enum RevocationReason {
    UserInitiated,        // dashboard "revoke" button
    AdminInitiated,       // admin operator
    SecurityIncident,     // compromise detected
    Expired,              // TTL passed
    ScopeChanged,         // scopes mutated (re-mint required)
    MassRevoke,           // tenant-wide rotation event
}

#[derive(Serialize, Deserialize)]
pub struct PropagationStatus {
    regions_propagated: HashSet<Region>,  // wnam, enam, weur, eeur, apac
    completed_at: Option<SystemTime>,     // None até all regions ack
}

impl RevocationDo {
    /// API: revoke PAT. Cascades (canonical: Neon `pat` é SoT per data_model.md §4.1; cycle 3 codex SEAL alignment):
    /// 1. UPDATE **Neon `pat`** SET revoked_at = now() (transactional; canonical SoT).
    /// 2. INSERT este DO storage (revocation list — DO é hot-path broadcast cache, não SoT).
    /// 3. INSERT audit_outbox `auth.token.revoked` (atomic com #1).
    /// 4. Trigger broadcast queue para outras regiões (cross-region propagation ≤ 60s SLO).
    /// 5. Local KV invalidate: WI-S03-003 SessionCache::invalidate hook (KV é hot-path session cache, não SoT).
    /// 6. Returns acknowledgment (idempotent; second call no-op).
    pub async fn revoke(&self, req: RevokeRequest) -> Result<RevokeResponse, RevocationError>;

    /// Query: is this token revoked? Used em verify path para defense-in-depth.
    /// Note: hot path uses SessionCache (WI-S03-003); este é fallback OR explicit re-check.
    pub async fn is_revoked(&self, pat_id: PatId) -> bool;

    /// Internal: broadcast handler (called via DO RPC from other regions).
    pub async fn ingest_remote_revocation(&self, entry: RevokedEntry) -> Result<()>;
}
```

**Cross-region propagation pattern** (CF Workers ecosystem):

1. Region A initiates revoke → DO_A.revoke() → D1 transaction + local KV invalidate (≤ 1s).
2. DO_A enqueues message into CF Queue `revocation_propagate` (durable; at-least-once).
3. CF Queue consumer (Workers Cron OR streamed) reads → for each peer region (B, C, D, E):
   - RPC call to DO_B.ingest_remote_revocation(entry).
   - DO_B updates local storage + KV invalidate.
   - Acknowledgment back to consumer.
4. SLO-FRESH-PAT-REVOKE: ≤ 60s p99 from initial revoke to all-regions-acknowledged.

**Stale window axes (P0 fix Lote 10.3bis — não compõem aditivamente)**: hot path (cache hit em mesma região) = 60s session TTL; cold path (cache miss) = D1 `revoked_at IS NULL` authoritative ≤ 100ms regional; cross-region propagation SLO ≤ 60s p99 — outras regiões sem cache miss → cold path D1 imediato. **Combined max = max(60s, 100ms, 60s) = 60s p99 single SLA**; documented em SLA addendum (NOT 120s aditivo).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Revocation correctness é o segundo pilar (após auth correctness) do auth posture. PAT comprometido sem revogação propagada = continued breach surface. Bugs catastróficos:

1. **Revocation broadcast loss**: queue message lost (CF Queue at-most-once antes da retry config); região B nunca recebe revoke; ataque persiste em B até cache TTL expira (60s) OU eventualidade ad-hoc. Mitigação: at-least-once com DLQ + retry policy + métrica `propagation_lag_seconds`. Consumer dedup via `(pat_id, revoked_at)` idempotency key.

2. **DO storage data loss em region failover**: Cloudflare Durable Object é region-pinned mas pode-se migrate em failure. Storage durável (pre-write log) garante persistence across migration. Confirmed: CF DO storage tem write-ahead log; data loss impossible se commit returned.

3. **KV invalidation race vs concurrent verify**: T+0 cliente A verify hit cache (returns stale TenantCtx); T+1ms revoke fires; KV.delete cache; T+5ms session_cache hit returns stale. Janela de 5ms é fundamental; mitigação: revoke também updates D1 `pat.revoked_at` BEFORE KV invalidate; verify path checks revoked_at em D1 fallback (cold path); window é ≤ session cache TTL (60s).

4. **Propagation lag > 60s SLO**: Queue backlog OR DO RPC slow OR region partition. Mitigação: tiered alert SEV-2 em > 30s p99; SEV-1 em > 60s p99; runbook RB-FM-REVOKE-LAG. Multi-region partition é CF-side; degradation expected during incidents.

5. **Mass revoke storm**: tenant emergency mass-revoke 10k PATs em 1 op; queue backpressure; em-flight verifies durante 60s window. Mitigação: rate-limit mass revoke via DO (bounded 100 revokes/sec per tenant); batch broadcast (100 entries per queue message); audit emit per entry.

6. **Audit emission gap em revocation**: revoke succeeds em D1 mas audit outbox INSERT fails; compliance gap. Mitigação: outbox INSERT é parte da mesma D1 batch que `pat` (canonical Neon SoT per data_model.md §4.1) UPDATE; reuse WI-S01-005 atomic transaction guarantee.

7. **Dashboard race vs actual revoke**: user clicks "revoke"; UI shows revoked imediato; backend processing ainda pendente; user clicks again → idempotent. Mitigação: idempotent revoke via `(pat_id, revoked_at)` unique constraint; second call no-op.

8. **`is_revoked()` query hot path**: DO RPC ~10ms p99; em verify path = +10ms latency tax. Mitigação: este WI **não** chama `is_revoked()` em hot path; hot path = SessionCache (WI-S03-003) + D1 `revoked_at` check. `is_revoked()` é admin/audit query path only.

**Atacante adversarial scenarios**:

- **Compromised PAT continued use**: atacante captura PAT; user detects + revokes. Propagation 60s. Atacante exploits 60s window. Mitigação: documented em SLA; rate limit S-08 caps 100 req/s per PAT; combined exposure bounded ≤ 6000 ops em 60s.
- **Mass revoke evasion**: tenant revokes 10k PATs após breach; atacante distributes across 10 regions concurrently. Propagation must reach all regions ≤ 60s. Chaos test em §15.
- **Replay after revoke + clock skew**: atacante uses PAT em region B com clock 30s atrás; B revokes em local clock T-30s; atacante in-flight before B sees revoke. Mitigação: clock skew ±60s tolerated em verify (WI-S03-001); revocation propagation independent of clock skew (uses authoritative DO timestamp).
- **DO RPC interception (insider)**: insider with deploy access reads/modifies DO state. Mitigação: wrangler config restrict; audit chain (S-09) records DO storage writes; chaos test admin-bypass detection.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: revocation gap = continued cross-tenant access via comprometido PAT.
- **FF-HR-005**: SLO-FRESH-PAT-REVOKE é availability-correctness boundary; ≤ 60s p99 é SLO contract.
- **FF-HR-009**: defense-in-depth coordination — revoke é cross-cutting (D1 + KV + DO + audit + cross-region).
- **Reversibility**: revoke gap detected via audit chain anomaly OR customer report; resposta = mass rotation. Mitigação proativa: chaos test propagation, monitoring lag, runbook ready.

11 sign-offs canonical incl. SRE Lead (DO operations), Architect (broadcast pattern + Crypto SME specialization), AppSec (timing windows).

## 3. Customer Impact & Journey

**Persona 1 — User revoking compromised PAT**:
- Dashboard → "Tokens" → "Revoke" → POST `/api/v1/tokens/:id` (DELETE-style).
- Server: middleware (WI-S03-003) auth → handler calls `revocation::revoke(pat_id, UserInitiated)`.
- DO: D1 UPDATE + outbox INSERT + local KV invalidate + queue broadcast.
- Customer-visible UI: immediate "revoked" state (UI optimistic; backend ack ~100ms).
- Customer-visible **propagation**: via SLA "revocation effective globally ≤ 60s p99 single SLA — eixos hot/cold/cross-region orthogonais; max 60s p99 (NOT 120s aditivo)".

**Persona 2 — Admin mass-revoking tenant tokens (security incident)**:
- Admin: POST `/api/v1/admin/tokens/revoke-all` (com `tenant_id` + reason `SecurityIncident`).
- DO orchestrates 2-phase mass revoke (cycle 4 codex SEAL alignment):
  - **Phase 1 (atomic)**: UPDATE **Neon `pat`** WHERE tenant_id=X SET revoked_at=now() (single atomic Postgres transaction; canonical SoT per data_model.md §4.1; 10k rows affected ≤ 5s).
  - **Phase 2 (chunked)**: audit_outbox INSERT em chunks of 1000 rows (10 batches × 1000 = 10k total; tolerates partial commit via queue at-least-once + consumer dedup; eventual completeness ≤ 5min).
- Queue: 100 entries per message × 100 messages = bounded backpressure.
- Customer-visible: "all tokens revoked; users must re-authenticate; propagation ≤ 60s globally".
- Audit: 10k `auth.token.revoked` events em chain.

**Persona 3 — Compliance auditor verifying revocation timeliness**:
- Audit query: `SELECT pat_id, revoked_at, propagation_completed_at FROM revocation_log WHERE tenant_id = ?`.
- Auditor sees `propagation_completed_at - revoked_at` distribution; confirma p99 ≤ 60s.
- Métrica `corelink.auth.revocation.propagation_seconds_bucket` exposed em SOC 2 evidence.

**SLA addendum**:
- `SLO-FRESH-PAT-REVOKE` ≤ 60s p99 (regions all-acknowledged).
- Combined max stale window single SLA ≤ 60s p99 (eixos orthogonais; hot/cold/cross-region não compõem aditivamente — cf. WI-S03-003 §3 SLA addendum).
- Mass revoke cap 100 revokes/sec per tenant (anti-abuse + queue backpressure).

## 4. Capability Mapping

- **CAP-AUTH-002** (PAT lifecycle: revoke) — IMPLEMENTA primary.
- **CAP-AUTH-004** (Tenant resolution + propagation) — IMPLEMENTA partial (revocation propagation cross-region).
- Trace: `auth_model.md §5 (Rotation/Revocation)` + `§8.1 (5-layer defense; revocation propagation supports Layer 1)` + `slo_catalog.md SLO-FRESH-PAT-REVOKE` + `resilience_patterns.md PAT-INVALIDATE-001`.

## 5. Tipo

Revocation lifecycle + cross-region orchestration; HIGH_RISK; FF-HR-002 + FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-worker/src/auth/revocation.rs`**:
   - `RevocationDo` struct + Cloudflare Durable Object impl.
   - `RevokeRequest`, `RevokeResponse`, `RevokedEntry`, `RevocationReason`, `PropagationStatus`.
   - Methods: `revoke`, `is_revoked`, `ingest_remote_revocation`, `query_revocation_log`.
   - DO storage: `Map<PatId, RevokedEntry>` persistent.

2. **D1 integration**:
   - UPDATE `pat SET revoked_at = ?, revocation_reason = ? WHERE pat_id = ?`.
   - INSERT `audit_outbox` (`auth.token.revoked` event).
   - Both em mesma `db.batch([...])` (atomic).
   - INSERT `revocation_log` table (denormalized; DO ↔ D1 dual-write tolerated via reconciliation; vide §6.1.7).

3. **Cross-region broadcast via CF Queue**:
   - Queue name: `revocation_propagate`.
   - Producer: DO.revoke() enqueues 1 message per RevokedEntry (batched para mass revoke).
   - Consumer: Worker Cron (every 10s) OR push consumer (preferred — lower latency).
   - Per peer region: RPC call to peer DO `ingest_remote_revocation`.
   - At-least-once delivery; consumer dedup via `(pat_id, revoked_at)` idempotency.
   - DLQ for failures > 5 retries; alert SEV-2.

4. **Local KV invalidate hook** (integration WI-S03-003):
   - Após DO storage commit, call `SessionCache::invalidate(token_hash)` em local region.
   - Token hash derivation: from PatId, lookup D1 `pat.token_hash` field.
   - Returns ack; failure não bloqueia (best-effort; KV invalidate é optimization; D1 revoked_at é SoT).

5. **Mass revoke endpoint**:
   - POST `/api/v1/admin/tokens/revoke-all` com body `{ tenant_id, reason }`.
   - Requires scope `admin-tokens` (validated em WI-S03-003 middleware).
   - Atomic D1 UPDATE batch (10k rows ≤ 5s).
   - Audit emit per row em outbox.
   - Queue 100 entries per message × N messages.
   - Returns count of revoked tokens.

6. **Métricas**:
   - `corelink.auth.revocation.requested_total{reason}` (counter).
   - `corelink.auth.revocation.propagation_seconds_bucket` (histogram p50/p95/p99; key SLO metric).
   - `corelink.auth.revocation.regions_propagated_ratio` (gauge; ratio per revoke).
   - `corelink.auth.revocation.queue_depth` (gauge).
   - `corelink.auth.revocation.dlq_size` (gauge; alert > 0 SEV-2).
   - `corelink.auth.revocation.is_revoked_query_total{path=hot|admin}` (counter).

7. **Reconciliation** (DO ↔ D1):
   - Background job (CF Cron daily) compares DO storage vs D1 `pat` (canonical Neon SoT per data_model.md §4.1) revoked_at.
   - Diff em either direction = SEV-2 alert + manual sync runbook RB-FM-REVOKE-DRIFT.
   - Bound: drift ≤ 5min em transient state aceito; > 1h sustained = real bug.

8. **Property tests** (10k iter PR; 100k nightly):
   - `prop_revocation_idempotent`: 1000 random tenants × 5 revoke retries; assert idempotent (single audit event per `(pat_id, revoked_at)`).
   - `prop_revocation_race_concurrent_verify`: 1000 iterations onde verify + revoke fire concurrent; assert verify never returns valid TenantCtx for revoked PAT post-propagation window.
   - `prop_mass_revoke_atomicity`: 100 tenants × 1000 PATs each; assert all-or-none (D1 transaction guarantee).
   - `prop_propagation_eventually`: 100 random revoke ops; assert all regions ack ≤ 60s in test harness simulator.

9. **Chaos suite**:
   - Cross-region propagation chaos (vide §15).
   - Queue outage simulation.
   - DO migration chaos.

10. **Audit events** (S-09 chain integration via WI-S01-005 outbox):
    - `auth.token.revoked` per revoke (single OR mass).
    - `auth.session.revoked` per session invalidate.
    - `auth.mass_revoke.triggered` per mass-revoke op (admin event).
    - CloudEvents 1.0 envelope; chain hash integrity (S-09 forward).

11. **rustdoc + 4 examples**:
    - `examples/revoke_basic.rs` — single PAT revoke.
    - `examples/mass_revoke.rs` — admin mass revoke.
    - `examples/propagation_monitoring.rs` — query propagation status.
    - `examples/reconciliation.rs` — drift detection.

### 6.2 Out-of-scope (deferred)

- **Distributed revocation across non-CF regions** (cliente self-hosted): pós-GA Q3+.
- **Revocation API external (RFC 7009 OAuth)**: pós-GA.
- **Per-scope revocation** (revoke `cache-w` mas keep `cache-r`; canonical hyphen-form): S-13 admin plane (granular).
- **Time-bound revocation** (auto-restore após X hours): rejected design (security anti-pattern).
- **Mass revoke beyond tenant scope** (org-wide; multi-tenant): S-14 enterprise.
- **WebAuthn credential revocation**: WI-S03-006.
- **JWT-specific revocation via jti blacklist**: Clerk-side responsibility; este WI handles PAT.

## 7. Anti-Scope

- ❌ Eventual consistency unbounded (must respect SLO ≤ 60s p99).
- ❌ Revocation via DELETE (immutability — use UPDATE revoked_at; audit trail preserved).
- ❌ Sync inter-region RPC em hot path (use queue + async).
- ❌ Mass revoke > 100/sec per tenant (rate limit anti-abuse).
- ❌ Revocation rollback (one-way; restoration requires re-mint).
- ❌ DO storage as KV cache replacement (DO is SoT; KV is hot-path optimization).
- ❌ TTL-only revocation (explicit revocation event mandatory; TTL handles expiry case).
- ❌ Custom queue impl (use CF Queue native).
- ❌ Skip audit em mass revoke (per-row audit mandatory).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Revocation lifecycle + cross-region propagation

  Background:
    Given 5 regions: wnam, enam, weur, eeur, apac
    Given each region has RevocationDo instance + SessionCache
    Given user has PAT_X com tenant_id=A em D1 + active em SessionCache em wnam

  Scenario: Single revoke happy path
    When user calls revoke(PAT_X, reason=UserInitiated) em wnam
    Then DO.revoke() executes:
      And D1 UPDATE pat (canonical Neon SoT per data_model.md §4.1) SET revoked_at=now() WHERE pat_id=PAT_X
      And D1 INSERT audit_outbox (event=auth.token.revoked)
      And DO storage INSERT RevokedEntry
      And local KV.delete(auth:session:<hash(PAT_X)>) called
      And Queue enqueue 1 message {pat_id, revoked_at, regions=[enam, weur, eeur, apac]}
    And RevokeResponse returned T+100ms p99
    And subsequent verify em wnam → SessionCache miss → D1 revoked_at check → 401

  Scenario: Cross-region propagation ≤ 60s p99
    Given revoke fired em wnam at T+0
    When Queue consumer drains
    Then DO_enam.ingest_remote_revocation called T+5s p50; ack returned
    And DO_weur.ingest_remote_revocation called T+10s p50
    And DO_eeur.ingest_remote_revocation called T+15s p50
    And DO_apac.ingest_remote_revocation called T+20s p50
    And all_regions_acknowledged at T+25s p50; T+60s p99
    And metric corelink.auth.revocation.propagation_seconds_bucket recorded
    And SLO-FRESH-PAT-REVOKE green (≤ 60s p99)

  Scenario: Idempotent revoke
    Given PAT_X already revoked at T+0
    When revoke(PAT_X) called again at T+10s
    Then DO checks: pat_id em storage → no-op
    And RevokeResponse returns existing revoked_at (não nova timestamp)
    And no duplicate audit event
    And Queue NOT enqueued (idempotent ingest dedup)

  Scenario: Mass revoke atomic + audit complete
    Given Tenant A has 10000 active PATs
    When admin POST /api/v1/admin/tokens/revoke-all com tenant_id=A reason=SecurityIncident
    Then **Neon UPDATE pat SET revoked_at=now() WHERE tenant_id=A** executes em ≤ 5s atomic Postgres transaction (Phase 1; canonical SoT per data_model.md §4.1; cycle 4 codex SEAL)
    And 10000 rows affected (UPDATE all-or-none per INV-AUTH-MASS-REVOKE-ATOMIC)
    And audit_outbox has 10000 INSERT rows chunked em 10 batches of 1000 (Phase 2; eventual completeness ≤ 5min via queue at-least-once)
    And Queue has 100 messages × 100 entries each = 10000 entries enqueued
    And response { revoked_count: 10000, mass_revoke_id: <UUID> }
    And metric corelink.auth.revocation.requested_total{reason="SecurityIncident"} += 10000

  Scenario: Race condition revoke vs verify
    Given verify_pat(PAT_X) em-flight at T+0 (cache miss; Argon2 verify started)
    Given revoke(PAT_X) fires at T+5ms
    When Argon2 completes T+250ms; middleware checks D1 revoked_at
    Then revoked_at IS NOT NULL (revoke commit completed at T+30ms)
    And response 401 com error_code COR_AUTH_TOKEN_REVOKED
    (race window mitigated via D1 revoked_at check pós-Argon2)

  Scenario: Queue outage degradation
    Given Queue producer fails (CF Queue 503)
    When revoke(PAT_X) attempts to enqueue
    Then DO logs SEV-2 + retries 3× exponential backoff
    And after 3 failures: write to DLQ via fallback (DO local pending_propagation list)
    And DO local revocation persists (D1 + DO storage); local KV invalidate
    And metric corelink.auth.revocation.queue_depth incremented (DLQ counter)
    And alert SEV-2 sustained > 5min
    Note: local revocation effective imediato; cross-region delayed até queue restored

  Scenario: DO migration during revocation
    Given DO_wnam migrating (CF region failover) durante revoke em-flight
    When CF migrates DO_wnam state
    Then DO storage write-ahead log replayed em new node
    And revoke acknowledgment delivered to caller post-migration (≤ 30s expected)
    And no data loss; idempotent ingest handles potential duplicate replay

  Scenario: Reconciliation drift detection
    Given DO storage shows PAT_X revoked at T+0
    Given Neon `pat`.revoked_at IS NULL (drift)
    When reconciliation Cron runs
    Then drift detected
    And SEV-2 alert fires
    And runbook RB-FM-REVOKE-DRIFT engaged
    And manual sync executes (admin tool)

  Scenario: Stale window single SLA bound (eixos orthogonais — P0 fix Lote 10.3bis)
    Given revoke em wnam at T+0
    Given session cache TTL 60s; cross-region propagation SLO 60s p99
    When verify em apac at T+30s (sub-window)
    Then session cache em apac may NOT have entry (cliente nunca verificou em apac antes); cold path D1 lookup
    And D1 revoked_at IS NULL filter authoritative ≤ 100ms regional Neon strong consistency
    And response 401 imediato (revoke effective em apac via D1 cold path)
    When verify em wnam (where stale cache exists) at T+30s
    Then SessionCache hit returns TenantCtx (PAT was valid at cache populate time; pre-revoke)
    And handler proceeds normally based on cached state
    And cache invalidates at T+60s (TTL expiry); subsequent verify hits cold path → 401
    And **single SLA: max stale window = 60s p99** (eixos hot/cold/cross-region orthogonais; NÃO somam)
    Note: hot path stale = TTL; cold path stale = D1 commit visibility ≤ 100ms; cross-region stale = D1 imediato (sem cache); todos bounded por 60s axis máximo (não combined 120s)
```

## 9. Design Decisions

### 9.1 Why Durable Object (não puramente D1)

DO oferece:
- **Single-writer per region** → no distributed consensus needed within region.
- **Push-based broadcast** capability (RPC outbound).
- **Persistent storage com WAL** → migration-safe.
- **Idempotent ingest natural** → DO.storage.put with conditional check.

D1 alone teria:
- Multi-writer race conditions cross-region.
- No native broadcast; must poll.
- Eventual consistency cross-region.

Combined: D1 = transactional truth (atomic com pat row); DO = real-time orchestration.

### 9.2 Why CF Queue (não pure DO RPC fan-out)

Pure DO RPC fan-out: source region → 4 RPC calls → wait all acks. Latency = max(rpcs) = 4×region-RTT ~200ms p99 best case + retries. Fragile vs partition.

CF Queue: durable; at-least-once; consumer dedup; failure-tolerant. Trade-off: +5-30s queue latency (consumer poll cadence), but bounded e robusto.

Hybrid: source DO → D1 commit + Queue enqueue (parallel local; ≤ 100ms p99). Queue consumer drains async. Combined ≤ 60s p99 SLO.

### 9.3 Why ≤ 60s p99 SLO (não 5s ou 5min)

- < 5s: requires push-based broadcast (DO RPC fan-out); fragile; queue alternative slower.
- 60s: practical balance; 4 regions × 15s avg per region = 60s p99 worst case. Aligns com session cache TTL como eixo independente (não somam — vide §3 SLA orthogonal axes).
- 5min: too long; atacante exploit window unacceptable for HIGH_RISK.

PAT-INVALIDATE-001 canonical em resilience_patterns.md.

### 9.4 Why D1 revoked_at IS NOT NULL como SoT (não DO storage)

D1 transactional commit (mesmo batch que pat). DO storage é eventual após D1 commit; pode-se ser inconsistente em DO migration window (rare but possible). Em verify path, **D1 é authoritative**; DO é optimization.

Specifically: verify path
1. SessionCache hit → return TenantCtx (hot path).
2. SessionCache miss → D1 lookup pat com revoked_at IS NULL constraint.
3. D1 returns row OR not-found; revoke handled at D1 level.
4. DO query (`is_revoked`) é admin path only; not in critical verify path.

### 9.5 Why per-tenant rate limit em mass revoke (100/sec)

Anti-abuse + queue backpressure. 10k mass revoke = 100 messages × 100 entries = 100 queue ops. Sustained mass revoke storm = queue saturated; consumer can't drain. Cap = bounded; tenant emergencies still cobertos (10k PATs in 100s = 100s SLO; aceitável vs DoS risk).

### 9.6 Why batch 100 entries per queue message (não 1)

CF Queue 1 message ≤ **128 KB** (Lote 10.3-tris P0-CONFIRMED-001 fix; was incorrect 256 KiB — Cloudflare Queue actual limit per CF docs Q4 2025); RevokedEntry ~200 bytes; 100 entries ~20 KB (well within 128 KB headroom 6.4×); 100-entry cap re-verified safe even with metadata bloat 5× = 100 KB still safe. Batching reduces queue throughput needs (10k revoke = 100 messages vs 10000); consumer processes batch atomic; cost reduction.

### 9.7 Why explicit reason enum (não free-text)

Compliance: SOC 2 + LGPD requires categorized revocation reasons. Free-text inconsistent; enum drives audit reports. UserInitiated, AdminInitiated, SecurityIncident, Expired, ScopeChanged, MassRevoke covers known cases. Future enum extension via `#[non_exhaustive]`.

### 9.8 Stale window axes orthogonais (NÃO aditivos) — single SLA 60s p99 (P0 fix Lote 10.3bis)

Stale window NÃO compõe aditivamente; eixos são independentes:

- **Hot path** (cliente em região com cache hit pré-revoke): stale = session cache TTL = 60s.
- **Cold path** (cache miss em qualquer região): D1 `revoked_at IS NULL` filter authoritative; stale = D1 commit visibility ≤ 100ms regional.
- **Cross-region propagation**: SLO ≤ 60s p99 — outras regiões sem cache → cold path D1 imediato (D1 é regional-strong + cross-region eventual ~100ms; revogation propagation atinge D1 cross-region ≤ 60s).

**Combined max** = max(60s hot, 100ms cold, 60s cross-region) = **60s p99** — single SLA customer-facing.

Trade-off:
- Session cache TTL ↓ → cost ↑ (Argon2 frequency).
- Propagation SLO ↓ → cross-region infra cost ↑ (push vs queue).
- 60s axis individual aceitável para HIGH_RISK auth (cf. AWS IAM ~minutes; GCP ~seconds-minutes).
- Documented em §3 SLA addendum.

**Anti-pattern evitado**: claim "combined 120s" era confusing (cliente assume aditivo); single SLA 60s clarifica orthogonality.

### 9.9 Why reconciliation Cron (não real-time)

DO ↔ D1 dual-write inerentemente eventually consistent across migration windows. Reconciliation Cron daily detects drift sustained > 5min. Real-time consistency would require 2PC (Cloudflare não oferece). Trade-off: 24h max detection lag para sustained drift; transient drift (< 5min) tolerated.

### 9.10 ADR potencial?

Sim — **ADR-0030**: "Revocation propagation: DO + Queue + ≤ 60s SLO single SLA". Documenta DO vs D1 SoT trade-off; queue vs RPC fan-out; stale window single SLA 60s p99 com eixos hot/cold/cross-region orthogonais (não aditivos). Whitelist em validate_references.py.

## 10. Completeness Criteria SOTA

- [ ] **10.5.1** Property tests 10k iter (PR) + 100k iter (nightly) → 0 panics, 0 idempotency violations (EVT-002).
- [ ] **10.5.2** SLO-FRESH-PAT-REVOKE ≤ 60s p99 sustained 72h em staging com realistic load (EVT-021).
- [ ] **10.5.3** Stale window single SLA ≤ 60s p99 measured (eixos hot/cold/cross-region orthogonais; max = 60s; NOT 120s aditivo) (EVT-021).
- [ ] **10.5.4** Mass revoke 10k PATs em ≤ 30s end-to-end (atomic + propagation) (EVT-021).
- [ ] **10.5.5** Idempotent revoke property: 100% retries return same revoked_at; zero duplicate audit events (EVT-002).
- [ ] **10.5.6** DO migration chaos: data loss 0; ack delivered ≤ 30s post-migration (EVT-023).
- [ ] **10.5.7** Queue outage chaos: local revocation 100% effective; cross-region degraded gracefully com SEV-2 alert (EVT-023).
- [ ] **10.5.8** Reconciliation Cron detects synthetic drift ≤ 5min (EVT-022).
- [ ] **10.5.9** Audit events emitted per revoke; chain integrity validated (S-09 forward) (EVT-031).
- [ ] **10.5.10** Cost regression gate: revoke op cost ≤ $0.0001 per (Lote 9.4 §14.10).
- [ ] **10.5.11** Adversarial test: replay PAT pós-propagation completed → 100% rejected.

## 11. DoD

- [ ] RevocationDo Durable Object impl + wrangler.toml binding.
- [ ] D1 migrations: pat revoked_at column + revocation_log table.
- [ ] Queue + consumer impl.
- [ ] Per-region SessionCache invalidate hooks integrated.
- [ ] Mass revoke endpoint + admin scope check.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em CI.
- [ ] Chaos suite green em staging.
- [ ] Métricas (6 listadas §6.1.6) emitted; dashboard.
- [ ] Reconciliation Cron deployed + drift alert configured.
- [ ] rustdoc + 4 examples.
- [ ] ADR-0030 published.
- [ ] Architect + AppSec + SRE Lead + Crypto SME reviews.
- [ ] PRR Architect sign-off.

## 12. Invariants Validated

- **INV-AUTH-REVOCATION-IDEMPOTENT** (CRITICAL): retry revoke = single audit event + same revoked_at; property test 100k.
- **INV-AUTH-REVOCATION-SLO-60S** (CRITICAL): cross-region propagation ≤ 60s p99 sustained.
- **INV-AUTH-NEON-IS-SOT** (HIGH): Neon `pat.revoked_at IS NULL` é authoritative em verify path; DO storage + KV session cache são hot-path optimization (não SoT). Cycle 4 codex SEAL canonical alignment per data_model.md §4.1.
- **INV-AUTH-MASS-REVOKE-ATOMIC** (CRITICAL): mass revoke é all-or-none via D1 batch.
- **INV-AUTH-PROPAGATION-AT-LEAST-ONCE** (HIGH): queue at-least-once + consumer dedup; eventual delivery.

TLA+ alignment: planned spec `revocation_propagation.tla` (S-09 ou S-12 forward); modela queue + DO + KV invalidate + verify race conditions.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| RevocationDo | `crates/corelink-worker/src/auth/revocation.rs` | Rust (DO) |
| RevokedEntry types | `crates/corelink-worker/src/auth/revocation_types.rs` | Rust |
| Mass revoke handler | `crates/corelink-worker/src/admin/mass_revoke.rs` | Rust |
| Queue consumer | `crates/corelink-worker/src/queue/revocation_consumer.rs` | Rust |
| Reconciliation Cron | `crates/corelink-worker/src/scheduled/revocation_reconcile.rs` | Rust |
| D1 migration | `migrations/003_revocation_log.sql` | SQL |
| wrangler.toml DO binding | `wrangler.toml` (DurableObjects + Queue) | TOML |
| Property tests | `crates/corelink-worker/tests/prop_revocation.rs` | Rust |
| Integration tests | `tests/integration_revocation.rs` | Rust |
| Chaos suite | `tests/chaos/revocation_propagation.rs` | Rust |
| Runbook RB-FM-REVOKE-LAG | `specs/05_runbooks/RB-FM-REVOKE-LAG.md` | Markdown |
| Runbook RB-FM-REVOKE-DRIFT | `specs/05_runbooks/RB-FM-REVOKE-DRIFT.md` | Markdown |
| ADR-0030 | `specs/03_architecture/adrs/ADR-0030-revocation-propagation.md` | Markdown |
| Examples (4) | `crates/corelink-worker/examples/revocation/` | Rust |

## 14. Quality Standards SOTA

- **14.5.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.5.2** rustdoc 100% public API + 4 examples.
- **14.5.3** Test coverage ≥ 95%.
- **14.5.4** Latência: revoke local ≤ 100ms p99; propagation ≤ 60s p99; mass revoke ≤ 30s p99 (10k PATs).
- **14.5.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings`.
- **14.5.6** Métricas RED + propagation lag histogram + queue depth + DLQ size.
- **14.5.7** Runbooks: RB-FM-REVOKE-LAG, RB-FM-REVOKE-DRIFT, RB-FM-160 (auth invalid storm reuse).
- **14.5.8** Breaking changes em `RevocationReason` enum = bump major + migration plan; non-breaking via `#[non_exhaustive]`.
- **14.5.9** Memory bounded: DO storage per-region ≤ 100 MiB (1M revoked entries × 100 bytes); rotation > 1 year purge policy.
- **14.5.10** Cost regression gate: revoke op ≤ $0.0001; bench em CI.

## 15. Chaos Experiments

1. **Cross-region propagation stress**: revoke 1000 PATs em 1 sec; verify all 5 regions ack ≤ 60s p99. Hypothesis: queue handles load. Procedure: load test em staging; observe p99 lag. Abort: lag > 90s sustained (alert SEV-1 antes do SLO breach 60s).

2. **Queue outage 10min**: simulate CF Queue 503 sustained; verify local revocation effective imediato (D1 commit); cross-region degraded; SEV-2 alert. Hypothesis: graceful degradation. Procedure: block queue endpoint via mock; observe DO behavior.

3. **DO migration during in-flight revoke**: trigger CF region failover during 100 revokes/sec; verify zero data loss + idempotent re-replay. Hypothesis: WAL persistence + idempotent ingest. Procedure: CF staging migration trigger; chaos test inspector.

4. **Mass revoke storm**: 10 tenants concurrent mass revoke 10k PATs each (100k total); verify rate limit enforced (100/sec/tenant) + queue backpressure handled. Hypothesis: rate limit caps; queue bounded.

5. **Race revoke vs verify (concurrent)**: 1000 iterations onde verify(PAT_X) starts at T+0 (Argon2 250ms) + revoke(PAT_X) fires at T+50ms; assert verify post-Argon2 checks D1 revoked_at + rejects. Hypothesis: D1 revoked_at SoT; race window mitigated.

6. **Stale window single SLA probe**: revoke at T+0 em wnam; query verify em apac at T+30s, T+60s; assert rejection imediato em apac (cold path D1 authoritative). Em wnam (cache hit), assert rejection ≤ 60s (TTL expiry). **Single SLA = 60s p99** (eixos não compõem; NOT 120s).

7. **Reconciliation drift injection**: synthetic D1 row with revoked_at set; DO storage missing entry; assert reconciliation Cron detects + alerts within 24h cycle. Procedure: synthetic INSERT.

8. **Queue consumer offline 1h**: stop consumer worker; verify DLQ accumulates; alert SEV-2; restart drains backlog without loss. Hypothesis: durable queue.

9. **Audit emission gap chaos**: synthetic D1 batch failure mid-revoke; assert atomic rollback (revoke não commit) + cliente retry → success. Hypothesis: D1 batch atomicity.

10. **Replay attack post-propagation**: atacante captures PAT pre-revoke; tries replay 60s + ε post-propagation completed; assert 100% rejected (D1 + DO + propagated state aligned).

## 16. PRR

PRR HIGH_RISK 11 sign-offs canonical gated em WI-S03-008 ship gate.

- [ ] All Gherkin green.
- [ ] Property + chaos suite green em staging.
- [ ] SLO-FRESH-PAT-REVOKE green sustained 72h.
- [ ] Métricas + dashboards.
- [ ] ADR-0030 published.
- [ ] Runbooks RB-FM-REVOKE-LAG + RB-FM-REVOKE-DRIFT deployed.
- [ ] OWASP ASVS V3.7 (token revocation) 100%.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | RevocationDo skeleton + DO storage layout | 3h |
| ST-002 | RevokedEntry types + RevocationReason enum | 1.5h |
| ST-003 | revoke() core logic + D1 batch atomic | 3h |
| ST-004 | ingest_remote_revocation() handler | 2h |
| ST-005 | Mass revoke endpoint + rate limit cap | 3h |
| ST-006 | Queue producer (enqueue logic) | 2h |
| ST-007 | Queue consumer + per-region RPC fan-out | 4h |
| ST-008 | DLQ handling + retry policy | 2h |
| ST-009 | SessionCache::invalidate integration hook | 1.5h |
| ST-010 | Reconciliation Cron impl | 3h |
| ST-011 | D1 migration `revocation_log` table + revoked_at column | 1.5h |
| ST-012 | Métricas (6) + trace span | 2h |
| ST-013 | Property tests 10k iter | 4h |
| ST-014 | Chaos suite (5 scenarios) | 5h |
| ST-015 | Integration test E2E | 4h |
| ST-016 | Runbooks RB-FM-REVOKE-LAG + DRIFT | 3h |
| ST-017 | rustdoc + 4 examples | 3h |
| ST-018 | ADR-0030 redação | 2h |
| ST-019 | Architect + SRE + AppSec review feedback iteration | 3h |

**Total Optimistic**: ~52.5h. **PERT** (O=47h, M=52.5h, P=78h): **~57h**.

## 18. Dependencies

### Hard blockers

- WI-S03-002 (corelink-pat) SEALED — pat_id + token_hash references.
- WI-S03-003 (middleware) SEALED — SessionCache::invalidate public hook.
- WI-S01-005 audit_outbox table available.
- WI-S03-005 (Neon schema) — `pat` (canonical Neon SoT per data_model.md §4.1) table com `revoked_at` column.

### Soft blockers

- WI-S03-007 (audit events) — não bloqueante; este WI emit events para outbox; consumer integration via S-09 forward.
- CF Queue infrastructure available em todas as regiões (operations team task).

### Outbound

- WI-S03-008 (ship gate).
- All authentication paths benefit (WI-S03-001/002/003 verify check D1 revoked_at).

## 19. Effort PERT

O: 47h, M: 52.5h, P: 78h → PERT **57h**.

## 20. Time-boxing

**64h hard limit**. Se exceder → escalation: split em sub-WI (single revoke + KV invalidate vs cross-region propagation).

## 21. Observability

6 métricas listadas §6.1.6. Trace span `revocation.revoke` com attributes:
- `revocation.reason` (enum value)
- `revocation.pat_id_hash` (sha256 prefix; PII-safe)
- `revocation.tenant_id` (UUID v7)
- `revocation.regions_target` (count)
- `revocation.duration_local_ms`
- `revocation.duration_propagation_ms`

Logs structured JSON; INFO em revoke ok; WARN em queue retry; ERROR em DLQ insert.

Dashboard widget DASH-AUTH:
- Revocation rate (req/s).
- Propagation p99 lag.
- Queue depth + DLQ size.
- Regions ack ratio per revoke.
- Stale window axes (hot TTL, cold D1 lag, cross-region) — single SLA tracking.

## 22. Cost Analysis

**Per-revoke breakdown** (single PAT):
- DO invocation: $0.20/M.
- D1 batch (UPDATE + outbox INSERT): ~$2/M.
- Queue producer (1 message): $0.40/M.
- Queue consumer: $0.20/M (4 RPC fan-out).
- Per-revoke total: ~$0.000003.

**Per-mass-revoke breakdown** (10k PATs):
- D1 batch UPDATE 10k rows: ~$5 per batch (one-time).
- Audit outbox 10k rows: included in batch.
- Queue 100 messages × 100 entries: $0.04.
- Total mass revoke 10k: ~$5; amortized $0.0005 per token.

**TCO 12m projection** (assume 100k revokes/yr + 5 mass revokes 10k each):
- Single revokes: 100k × $0.000003 = $0.30/yr (trivial).
- Mass revokes: 5 × $5 = $25/yr.
- Reconciliation Cron: 365 × $0.10 = $36.50/yr.
- **Total**: ~$62/yr (negligible).

**Cost regression gate**: per-op ≤ $0.0001; bench em CI.

## 23. API Contract

- `POST /api/v1/tokens/:id/revoke` — single revoke.
- `POST /api/v1/admin/tokens/revoke-all` — mass revoke (admin scope).
- `GET /api/v1/admin/tokens/:id/revocation` — query revocation status.
- Internal DO RPC: `ingest_remote_revocation(entry)`.

Erro mapping:
- `Token not found` → 404 `COR_AUTH_TOKEN_NOT_FOUND`.
- `Already revoked` → 200 OK (idempotent; returns existing revoked_at).
- `Scope insufficient` (não admin) → 403 `COR_AUTH_SCOPE_INSUFFICIENT`.
- `Rate limited` → 429 `COR_AUTH_RATE_LIMITED` + Retry-After.
- `Backend unavailable` → 503 `COR_AUTH_BACKEND_UNAVAILABLE`.

## 24. Post-mortem Hooks

- Propagation > 60s p99 sustained > 1h → SEV-1 + 5-Why + post-mortem CRITICAL.
- Audit emission gap em revoke → SEV-1 (compliance gap).
- DO data loss em migration → SEV-0 (rare; CF guarantees WAL).
- Mass revoke storm causes queue saturation > 1h → SEV-2.
- Reconciliation drift sustained > 24h → SEV-1 (real bug).
- Replay attack post-propagation detected → CRITICAL (security incident).

## 25. Rollback / Recovery

Hot rollback via Wrangler. DO storage purge: manual via wrangler CLI (rare; emergency only — would lose revocation state; cliente impact requires re-revoke). RTO ≤ 30 min (rollback) / ≤ 24h (state rebuild from D1 if DO purged).

Per-tenant emergency: mass-revoke endpoint OR direct D1 UPDATE (admin override).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: revoke endpoint requires admin scope (WI-S03-003 middleware enforces); insider abuse mitigated via audit chain + multi-factor admin (WI-S03-006 WebAuthn forward).
- **Tampering**: D1 transactional commit + DO WAL persistence; tamper detection via reconciliation Cron + audit chain integrity.
- **Repudiation**: append-only audit trail; `auth.token.revoked` events em chain; query API for compliance reports.
- **Information disclosure**: revocation_log denormalized but PII redacted (principal_id hashed); DO storage não exposed publicly.
- **DoS**: mass revoke rate limit 100/sec/tenant; queue backpressure bounded; revoke endpoint rate limit S-08 forward.
- **Elevation of privilege**: admin scope required; checked em middleware; insider threat via audit + waiver.

**LINDDUN delta**:
- **Linkability**: pat_id pseudonymous (UUID v7); principal_id hashed em SIEM forwarding.
- **Identifiability**: revocation_log retains PII (revoked_by); DSR pipeline S-11 covers erasure.
- **Non-repudiation**: chain integrity (S-09); revocation event imutável.
- **Detectability**: revocation events não visible para attacker (internal API); cliente vê resultado (401 post-propagation).
- **Disclosure of information**: error messages não revelam tenant size, etc.
- **Unawareness**: SLA addendum documents revocation propagation timing.
- **Non-compliance**: LGPD Art. 47 (revogação consent) + GDPR Art. 17 (erasure) supported via revocation flow.

## 27. Knowledge Transfer

- **Tech talk** (1.5h): "Revocation Propagation: DO + Queue + ≤ 60s SLO".
- **Doc** `docs/internal/revocation-architecture.md` — sequence diagrams (single revoke, mass revoke, propagation, race conditions).
- **Doc** `docs/internal/sla-addendum-revocation.md` — customer-facing SLA explanation.
- **ADR-0030** — design rationale.
- **Workshop** (2h): com SRE + Architect + AppSec — adversarial walkthrough.
- **Onboarding test** (5 questions): SLO definition, queue vs RPC trade-off, idempotency, stale window axes orthogonality (single 60s SLA), mass revoke caps.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Propagation > 60s p99 sustained | M | H | HIGH | M | LOW | Tiered alerts SEV-2/1; runbook RB-FM-REVOKE-LAG |
| R-002 | Queue outage causes cross-region revocation gap | L | H | HIGH | L | LOW | Local revocation immediate; SEV-2 alert; DLQ retry |
| R-003 | DO migration data loss em transient state | L | M | CRITICAL | L | LOW | CF WAL persistence; idempotent ingest; chaos test |
| R-004 | Mass revoke storm saturates queue | M | M | MEDIUM | M | LOW | Per-tenant rate limit 100/sec + queue cap |
| R-005 | Race verify vs revoke (Argon2 in-flight) | M | H | MEDIUM | M | LOW | D1 revoked_at SoT check pós-Argon2; window bounded |
| R-006 | Reconciliation drift sustained > 24h | L | M | HIGH | L | LOW | Daily Cron + drift alert; manual sync runbook |
| R-007 | Insider revoke abuse | L | M | CRITICAL | L | LOW | Admin scope + audit chain + WebAuthn S-03 forward |
| R-008 | Audit emission gap (D1 batch fail) | L | M | HIGH | L | LOW | Atomic batch; reuse outbox guarantee |
| R-009 | Stale window single SLA > 60s breach (any eixo) | L | M | HIGH | L | LOW | Per-eixo SLO independent (TTL hot, D1 cold, propagation cross-region); breach alert per axis |
| R-010 | DLQ accumulates causing cross-region permanent gap | L | M | HIGH | L | LOW | DLQ size alert SEV-2; retry policy 5×; runbook |
| R-011 | DO storage exhausts memory (1M+ revoked entries) | L | L | MEDIUM | L | LOW | Rotation policy: purge revoked > 1 year (audit retained em D1) |
| R-012 | Cost regression > 10% em propagation | M | L | MEDIUM | L | LOW | §14.10 gate + monthly bench |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + SRE Lead review DO + Queue pattern + SLO budgeting.
2. **Code (D+5)**: peer review + AppSec review insider threat surface.
3. **Adversarial (D+8)**: red team session — replay attack scenarios + queue poisoning.
4. **Performance (D+10)**: load test + propagation chaos.
5. **PRR (D+12)**: Architect sign-off + readiness review.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; broadcast pattern + DO sharding review_ (com Crypto SME specialization mandatory: revocation broadcast TLS posture + KV cache key derivation review) | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; revocation latency + cache poisoning review_ | _pending_ | _pending_ |
| 5 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 6 | Engineer (S-03 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; timing windows + revocation race review_ | _pending_ | _pending_ |

> Crypto SME folds into Architect role specialization (cycle 1 codex SEAL alignment per framework §33.5.4.3 + ADR-0034 solo-tier waiver). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S03-004 (Lote 10.3); SOTA full (DO + Queue propagation, combined 120s SLA, 5 INVs, 10 chaos, 12-row risk, ADR-0030 forward). |

## 32. Anti-patterns evitados

- ❌ Eventual consistency unbounded (SLO ≤ 60s p99 hard).
- ❌ Revocation via DELETE (audit immutable; UPDATE revoked_at).
- ❌ Sync RPC fan-out em hot path (queue async).
- ❌ TTL-only revocation sem explicit event.
- ❌ DO storage como hot-path cache (D1 SoT; DO orchestration).
- ❌ Skip audit em mass revoke (per-row event).
- ❌ Custom queue impl (CF Queue native).
- ❌ Mass revoke unbounded rate (100/sec/tenant cap).
- ❌ Real-time DO ↔ D1 consistency (reconciliation Cron sufficient).
- ❌ Public revocation endpoint sem admin scope.
- ❌ Reason free-text (enum + non_exhaustive).

---

**Fim WI-S03-004.** Próximo: WI-S03-005 (Neon schema account/tenant/user_account/membership/pat + pgcrypto column encryption).
