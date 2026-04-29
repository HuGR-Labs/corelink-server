---
id: "WI-S13-001"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-28"
updated: "2026-04-28"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-13"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
tags: ["wi", "s13", "admin-plane", "config-singleton", "feature-flags", "rollback", "cas", "high-risk"]
---

# WI-S13-001 — DO `config-singleton` per-region + Schema Versioned (Feature Flags + Tunables + Retention) + CAS Atomic Update + Propagation Pub-Sub ≤ 5s Edge Global + Rollback API ≤ 5 min + D1 `config_change_log` 90d Retention

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-13](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S13-001 |
| Título | DO `config-singleton` per-region implementando schema versioned (feature flags tipados + rate-limit tunables + retention policies) com CAS atomic `(version, payload)` update; propagação edge global ≤ 5s via Worker subscribe DO change events; D1 `config_change_log` 90d retention; rollback endpoint `POST /v1/admin/config/rollback?to_version=X` ≤ 5 min recovery; foundation que WI-S13-002 (admin API) consume |
| Sprint | S-13 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (config governing rate-limits + retention policies = blast radius global se incorrect; CAS bypass = lost write race condition catastrophic) |

## 1. Intent

Implementar DO `config-singleton` per-region como source-of-truth para CoreLink runtime config: (1) **feature flags tipados** `Map<feature_id, {enabled: bool, rollout_pct: 0-100, allowlist_tenants: [uuid]}>`; (2) **rate-limit tunables** `Map<{layer, tier}, {refill_rate, burst}>`; (3) **retention policies** `Map<resource, ttl_days>`; updated via CAS `(version, payload)` atomic — reject se `version != current` (concurrent updates lost-write impossible); (4) **propagação** edge global ≤ 5s via Worker subscribe DO change events (Cloudflare Queue fan-out); (5) **D1 `config_change_log`** 90d history retention para audit + rollback; (6) **rollback API** `POST /v1/admin/config/rollback?to_version=X` recovery ≤ 5 min p99 (drill cadence monthly).

```rust
// File: crates/corelink-config-do/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPayload {
    pub schema_version: u32,                // bumped via ADR
    pub feature_flags: std::collections::BTreeMap<String, FeatureFlag>,
    pub rate_limits: std::collections::BTreeMap<RateLimitKey, RateLimitTunable>,
    pub retention_policies: std::collections::BTreeMap<String, RetentionPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlag {
    pub enabled: bool,
    pub rollout_pct: u8,                    // 0..=100
    pub allowlist_tenants: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RateLimitKey {
    pub layer: String,                      // e.g. "cas_put", "ac_get"
    pub tier: String,                       // "Solo", "Team", "Business", "Enterprise"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitTunable {
    pub refill_rate_per_sec: u32,
    pub burst: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub ttl_days: u32,
}

#[async_trait]
pub trait ConfigSingletonStore: Send + Sync {
    /// CAS update: succeeds only if current_version == expected_version. Returns new version on success.
    async fn update(
        &self,
        expected_version: u64,
        new_payload: ConfigPayload,
        actor: &AdminActor,
    ) -> Result<u64, ConfigError>;

    async fn current(&self) -> Result<(u64, ConfigPayload), ConfigError>;

    async fn rollback_to(
        &self,
        to_version: u64,
        actor: &AdminActor,
    ) -> Result<u64, ConfigError>;

    async fn history(&self, limit: u32) -> Result<Vec<ConfigVersionEntry>, ConfigError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigVersionEntry {
    pub version: u64,
    pub payload_hash: [u8; 32],             // SHA-256 do canonical JSON
    pub actor: AdminActor,
    pub mfa_ts_ms: u64,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminActor {
    pub user_id: Uuid,
    pub email_hash: [u8; 32],               // SHA-256(email) — pseudonymized for audit
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("version conflict (expected {expected}, current {current})")]
    VersionConflict { expected: u64, current: u64 },
    #[error("schema validation failed: {0}")]
    SchemaInvalid(String),
    #[error("rollback target version {0} not in 90d retention window")]
    VersionExpired(u64),
    #[error("propagation timeout (>{0}s)")]
    PropagationTimeout(u32),
    #[error("storage backend error: {0}")]
    Backend(String),
}
```

Propagation: DO emits change event via Cloudflare Queue `cfg-change-region-<region>`; per-Worker consumer updates in-memory snapshot; SLO-ADMIN-CONFIG-PROPAGATION ≤ 5s p99 sustained 30d staging.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

DO config-singleton é boundary cripto-load-bearing operacional: rate-limit tunables governam rate-limit middleware (CTRL-RATE-001); retention policies governam DSR + GC behavior (CTRL-AUDIT-005 + INV-GC-GRACE-RESPECTED); feature flags governam dark launches + canary deploys (PAT-PROGRESSIVE-ROLLOUT-001 base). Bug catastrófico em qualquer um desses paths = blast radius global multi-tenant — flag `enabled=false` perdido em race = customer feature inadvertently turned off; rate-limit `refill_rate=0` propagated = denial-of-service self-inflicted; retention `ttl_days=0` propagated = data deleted irreversibly.

**Bugs catastróficos possíveis** (todos endereçados):

1. **Lost-write race condition** (concurrent updates): 2 admins editam config simultaneamente; sem CAS, last-writer-wins silenciosamente perde primeira mudança. Mitigação: CAS atomic `(expected_version, new_payload)` reject se `version != current`; property test 10k concurrent updates 0 lost writes.

2. **Stale propagation** (Worker subscribe falha): DO updates mas Worker em região remote não recebe change event; legacy config persiste; rate-limit divergence per-region. Mitigação: SLO-ADMIN-CONFIG-PROPAGATION ≤ 5s p99 sustained; Worker periodic poll (60s) como safety net (idempotent reconciliation); chaos test simulating Queue outage → fallback poll triggers.

3. **Schema drift** (DO field added sem migration): old Worker reads new payload com unknown field; deserialization fails; rate-limit/retention default unsafe behavior. Mitigação: `schema_version` field bumped via ADR mandatory; serde `#[serde(deny_unknown_fields)]` enforced em DO write path; old Worker rejects schema_version > supported (alert SEV-3).

4. **Rollback target corrupts state** (rollback to T-7d but runtime state assumes new schema): rollback succeeds em config-singleton mas runtime invariants violated (e.g., rate-limit `tier="EnterprisePlus"` em rollback target não-supported em runtime). Mitigação: pre-rollback simulation step (validate rollback target against runtime invariants); manual override available; monthly drill validates path.

5. **CAS retry storm** (UI/CLI auto-retry on conflict): N admins editing → cascade retries → DO overload. Mitigação: exponential backoff client-side; metric `corelink_admin_config_cas_conflict_total` alerts > 10/min; CLI rate-limit retry ≤ 3 attempts.

6. **Rollback target version expired** (>90d): admin tries rollback to T-100d; D1 retention purged target. Mitigação: rollback API returns `VersionExpired` error explicit; UI/CLI warns user; D1 90d retention enforced via lifecycle rule.

7. **Audit chain break em config write path**: DO update succeeds mas audit_outbox INSERT fails; later config rollback can't be reconstructed. Mitigação: D1 atomic batch [DO trigger + audit_outbox INSERT + config_change_log INSERT]; failure rolls back DO write (INV-AUDIT-APPEND-ONLY herdada); chaos test audit fail-closed.

8. **Privilege escalation via config rollback** (admin rolls back to permissive state): rollback to old version sem dual-approval = bypass gate. Mitigação: rollback endpoint enforces dual-approval (WI-S13-002 dependency) + audit emission rich.

**Atacante adversarial scenarios**:

- **Rollback to malicious past version**: attacker compromises admin credentials; rolls back to past version where vulnerability was patched. Mitigação: rollback requires dual-approval (WI-S13-002); 90d retention bound limits attack window; audit chain detects.

- **CAS bypass via direct DO write**: attacker bypasses API and writes DO directly. Mitigação: DO accessible apenas via Worker binding (Cloudflare platform isolation); admin role + WebAuthn step-up enforce em API path; no direct DO ingress.

- **Propagation delay attack**: attacker triggers change durante peak load; waits for propagation lag; exploits stale config window. Mitigação: SLO 5s p99 sustained; Worker periodic poll 60s safety net; rate-limit changes apply only forward (no retroactive reduction).

**Risk justification HIGH_RISK**:

- **FF-HR-005**: config governing rate-limit + retention + feature flags = blast radius global se incorrect.
- **Reversibility**: rollback ≤ 5 min recovery; mas downstream side-effects (e.g., `ttl_days=0` propagated 5s = data already GC'd) podem ser irreversibles. Mitigação: dual-approval em destructive config (retention reduction, feature disable).

11 sign-offs canonical incl. Architect (cripto-touching review: CAS semantics + audit chain integration; folded Crypto SME specialization) + Security Lead (admin role + dual-approval gate) + SRE Lead (propagation SLO + monthly drill).

## 3. Customer Impact & Journey

**Persona 1 — SRE on-call em prospect enterprise**:
- Customer queries `GET /v1/admin/config/current` → returns `(version: 42, payload: {...})`.
- Audit query: `GET /v1/admin/config/history?limit=20` → 20 last versions com actor + mfa_ts.
- Rollback drill: monthly `POST /v1/admin/config/rollback?to_version=37`; recovery ≤ 5 min p99.

**Persona 2 — Auditor SOC 2 Type II**:
- D1 `config_change_log` 90d retention = audit-grade history per change (actor + payload_hash + mfa_ts).
- CTRL-AUDIT-002 (write events ricos) satisfied.

**Persona 3 — Engineer onboarding em CoreLink**:
- `crates/corelink-config-api/README.md` documenta CRUD flow + CAS retry pattern + propagation timing model.
- ADR-XXXX (forward) documenta `schema_version` bump policy + DO single-instance per-region rationale.

**SLA addendum**:
- Config propagation latency: ≤ 5s p99 edge global sustained 30d staging.
- Rollback recovery: ≤ 5 min p99 (drill monthly).
- D1 history retention: 90d hard.
- CAS retry budget: ≤ 3 attempts client-side.

## 4. Capability Mapping

- **CAP-ADMIN-001** (DO config-singleton + flag CRUD + propagation) — IMPLEMENTA primary.
- **CAP-ADMIN-007** (config rollback API ≤ 5 min) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.1` + `security_model.md §6.8 (CTRL-AUDIT-002)` + `observability_model.md §3.1 + §4.1` (cardinality budget + naming convention).

## 5. Tipo

DO + Worker + D1 admin plane foundation; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Crate `corelink-config-do/`** (Cloudflare Durable Object):
   - `ConfigSingletonStore` trait + DO impl `ConfigSingletonDO`.
   - Schema `ConfigPayload` versioned com `schema_version: u32` field.
   - CAS atomic update via DO transactional storage (`storage.transaction`).
   - Storage layout: `current_version` (u64) + `current_payload` (bytes) + `history[version] -> payload` (90d retention; older versions purged via lifecycle).
   - Methods: `update`, `current`, `rollback_to`, `history`.
   - Single-instance per-region (DO `idFromName("corelink-config-{region}")`).

2. **Crate `corelink-config-api/`** (Worker handlers):
   - `GET /v1/admin/config/current` → `(version, payload)`.
   - `PUT /v1/admin/config` body=`{expected_version, new_payload}` → `(new_version)` ou 409 `VersionConflict`.
   - `POST /v1/admin/config/rollback?to_version=X` (dual-approval gated; integrates WI-S13-002) → `(new_version)`.
   - `GET /v1/admin/config/history?limit=N` → `Vec<ConfigVersionEntry>`.
   - Middleware: admin role check (CTRL-AUTHZ-001) + MFA freshness ≤ 30 min (CTRL-AUTH-010 + INV-ADMIN-MFA-FRESHNESS).
   - Schema validation pre-write: serde `#[serde(deny_unknown_fields)]` + custom `validate_payload` (e.g., `rollout_pct ≤ 100`, `refill_rate ≥ 1`, `ttl_days ≤ 365 OR ADR override`).

3. **Propagation pub-sub**:
   - DO `update()` emits change event via Cloudflare Queue `cfg-change-region-{region}` (payload: `{version, payload_hash}`).
   - Per-Worker consumer (`cfg-consumer-worker`) maintains in-memory snapshot via `OnceCell<Arc<ConfigPayload>>`; refresh on Queue event; safety-net 60s periodic poll to DO.
   - SLO-ADMIN-CONFIG-PROPAGATION ≤ 5s p99 edge global.

4. **D1 `config_change_log` table** (90d retention):
   ```sql
   CREATE TABLE config_change_log (
     version BIGINT PRIMARY KEY,
     payload_hash BLOB(32) NOT NULL,
     payload BLOB NOT NULL,                      -- canonical JSON serde_jcs RFC 8785
     actor_user_id BLOB(16) NOT NULL,
     actor_email_hash BLOB(32) NOT NULL,
     mfa_ts_ms BIGINT NOT NULL,
     created_at_ms BIGINT NOT NULL,
     change_type TEXT NOT NULL CHECK (change_type IN ('update', 'rollback')),
     previous_version BIGINT
   );
   CREATE INDEX idx_config_change_log_created_at ON config_change_log(created_at_ms);
   ```
   - Cron job daily 02:00 UTC purges rows where `created_at_ms < now - 90d`.
   - Atomic batch INSERT em mesma D1 batch que DO trigger + audit_outbox INSERT (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER pattern herdada S-03).

5. **Métricas underscored Prometheus** (per `observability_model.md §3.1 + §4.1`; label `plan` aplicável):
   - `corelink_admin_config_propagation_seconds_bucket` (histogram p50/p95/p99).
   - `corelink_admin_config_cas_conflict_total{layer,plan}` (counter; layer ∈ feature_flags|rate_limits|retention).
   - `corelink_admin_config_rollback_total{outcome,plan}` (outcome ∈ ok|version_unknown|version_expired|state_corrupt).
   - `corelink_admin_config_update_total{outcome,plan}` (outcome ∈ ok|version_conflict|schema_invalid|propagation_timeout).
   - `corelink_admin_config_history_size_gauge` (D1 row count).

6. **Observability** — trace span `admin.config.update` + `admin.config.rollback` + `admin.config.propagation` com attributes:
   - `config.version` (u64).
   - `config.expected_version` (u64).
   - `config.payload_hash` (string; hex 64 chars).
   - `actor.user_id` (UUID).
   - `result` (enum).

7. **Property tests** (10k iter PR + 100k iter nightly):
   - `prop_config_cas_concurrent_no_lost_writes`: 10k concurrent update attempts variando `expected_version` random; assert exactly 1 succeeds per round, 0 lost writes.
   - `prop_config_schema_drift_rejected`: 10k random payloads; old Worker schema_version=N reads new schema_version=N+1; assert rejection (no silent default).
   - `prop_config_rollback_target_validation`: 10k random rollback targets; assert `VersionExpired` for >90d, `VersionUnknown` for non-existent, `ok` for valid.
   - `prop_config_payload_validation`: 10k random payloads; assert `rollout_pct > 100` rejected, `refill_rate == 0` rejected, `ttl_days > 365` requires ADR flag.

8. **Adversarial regression tests**:
   - Direct DO write bypass attempt → rejected (DO accessible only via Worker binding).
   - Schema field injection (unknown field) → rejected via `deny_unknown_fields`.
   - CAS retry storm 1000 concurrent → metric alert fires; circuit breaker engages.
   - Propagation Queue outage → Worker periodic poll (60s) recovers; metric reflects.
   - D1 90d retention bypass attempt → cron purges; rollback to >90d returns `VersionExpired`.

9. **Integration test E2E**:
   - PUT /v1/admin/config with `expected_version=N` → 200 `(new_version=N+1)`.
   - PUT /v1/admin/config concurrent (2 callers same expected_version) → 1×200, 1×409.
   - Synthetic propagation test: PUT update + 5 Workers em 5 regiões verify in-memory snapshot updated ≤ 5s p99.
   - Rollback drill: state at v40; PUT v41/v42/v43; rollback to v41 → 200 `(new_version=v44 com payload of v41)`.

10. **`docs/internal/admin-plane.md` documentation** (this WI scope):
    - Architecture: DO single-instance per-region + Worker subscribe via Queue + Worker safety-net poll.
    - Schema migration policy (schema_version bump via ADR).
    - CAS retry pattern (client-side exponential backoff).
    - Rollback drill playbook.

### 6.2 Out-of-scope (deferred)

- **Admin API endpoint full** (incl. dual-approval middleware): WI-S13-002.
- **Secret rotation** (4 asset types): WI-S13-003.
- **Terraform drift detection**: WI-S13-004.
- **Progressive rollout controller**: WI-S13-005.
- **Property tests 10k full + RB dry-runs + PRR**: WI-S13-006.
- **Admin UI panel** (web): S-16.
- **Customer-facing feature flags**: out of scope (S-13 internal only).
- **Multi-region config federation**: Cloudflare-only at GA.
- **Schema migration tooling auto-apply**: pós-GA enterprise.
- **Encrypted config payload at rest**: not needed (config não contém secrets; secrets em separate KMS path).

## 7. Anti-Scope

- Skip CAS atomic (lost-write race condition catastrophic).
- Skip schema_version field (silent drift = catastrophic).
- Skip propagation SLO (stale config = blast radius global).
- Skip D1 90d retention (audit-grade history mandatory).
- Skip rollback API (recovery vector mandatory per CAP-ADMIN-007).
- Direct DO write path (Worker binding only).
- Encrypt config payload (config não contém secrets).
- Bypass MFA freshness em config update (CTRL-AUTH-010 enforced).
- Auto-apply schema migration (manual ADR + bump only).
- Long-running write transaction (DO storage atomic; CAS native).
- Skip audit chain integration (INV-AUDIT-APPEND-ONLY mandatory).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: DO config-singleton + CAS update + propagation + rollback

  Background:
    Given DO config-singleton initialized per-region with schema_version=1
    And admin role + MFA fresh ≤ 30 min for caller

  Scenario: Successful CAS update
    Given current_version=10
    When admin PUT /v1/admin/config body={expected_version: 10, new_payload: {...}}
    Then response 200 with new_version=11
    And metric corelink_admin_config_update_total{outcome="ok"} incremented
    And D1 config_change_log row inserted (version=11)
    And audit_outbox row inserted (admin.config.update event)

  Scenario: Concurrent CAS conflict
    Given current_version=10
    When admin A PUT with expected_version=10 succeeds (new_version=11)
    And admin B PUT concurrent with expected_version=10
    Then admin B response 409 VersionConflict
    And metric corelink_admin_config_cas_conflict_total{layer="any"} incremented

  Scenario: Schema validation rejects malformed payload
    When admin PUT body has rollout_pct=150
    Then response 400 SchemaInvalid("rollout_pct out of range")
    And metric corelink_admin_config_update_total{outcome="schema_invalid"} incremented

  Scenario: Schema drift via unknown field rejected
    When admin PUT body has unknown field "magic_flag"
    Then response 400 SchemaInvalid("unknown field")
    And no DO write occurred

  Scenario: Propagation edge global ≤ 5s p99
    When admin PUT succeeds (new_version=20)
    Then 5 Workers em 5 regiões observam new payload em ≤ 5s p99
    And metric corelink_admin_config_propagation_seconds_bucket reflects latency

  Scenario: Propagation Queue outage falls back to periodic poll
    Given Cloudflare Queue cfg-change-region-X outage
    When admin PUT succeeds
    Then Worker periodic poll (60s) eventually picks up new payload
    And metric corelink_admin_config_propagation_seconds_bucket shows ≤ 60s

  Scenario: Rollback to T-7d succeeds ≤ 5 min
    Given history contains version 30 from 7d ago
    When admin POST /v1/admin/config/rollback?to_version=30
    Then response 200 new_version=current+1 with payload of v30
    And recovery time ≤ 5 min p99
    And metric corelink_admin_config_rollback_total{outcome="ok"} incremented
    And audit emission "admin.config.rollback" with prev_state_hash

  Scenario: Rollback to expired version (>90d) rejected
    When admin POST /v1/admin/config/rollback?to_version=1 (created 100d ago)
    Then response 410 VersionExpired
    And metric corelink_admin_config_rollback_total{outcome="version_expired"} incremented

  Scenario: Rollback requires dual-approval (FORWARD WI-S13-002)
    When admin POST /v1/admin/config/rollback without X-Dual-Approver header
    Then response 403 DualApprovalMissing
    And no rollback occurred

  Scenario: MFA freshness enforced (>30 min stale)
    Given admin MFA timestamp 31 min ago
    When admin PUT /v1/admin/config
    Then response 401 MfaStale (force re-MFA)
    And metric corelink_admin_mfa_freshness_total{outcome="stale"} incremented

  Scenario: Direct DO write bypass attempt
    When attacker tries to write DO storage directly (bypass Worker)
    Then DO is accessible only via Worker binding (Cloudflare platform isolation)
    And no bypass possible

  Scenario: D1 90d retention enforced
    Given config_change_log row with created_at_ms = now - 91d
    When daily purge cron runs
    Then row deleted from D1
    And metric corelink_admin_config_history_size_gauge decremented

  Scenario: Property test concurrent updates 10k green
    Given prop_config_cas_concurrent_no_lost_writes 10k iter
    When test runs nightly
    Then 0 lost writes detected
    And exactly 1 winner per round
```

## 9. Design Decisions

### 9.1 Why Durable Object single-instance per-region

- Strong consistency required (CAS atomic) — DO storage transactional; KV eventual = unsafe.
- Per-region isolation = blast radius local; 1 region misconfig não propaga cross-region.
- Cloudflare platform-native; no external SoT (no Postgres for hot config path).
- Latency: DO read ≤ 10ms p99 from same-region Worker.

Alternativa rejeitada: D1 single-row config (Postgres-equivalent semantics) — D1 SQLite-based, throughput limit + cross-region writes not native; DO is purpose-built for this access pattern.

### 9.2 Why CAS atomic `(version, payload)` update

- Lost-write race condition prevention é mandatory (concurrent admin edits frequent in prod).
- DO storage transaction native; no locking required; backoff retry client-side.
- Property test 10k concurrent verify 0 lost writes.

Alternativa rejeitada: optimistic locking via D1 `UPDATE WHERE version = ?` — D1 row locking weaker; DO transactional storage stronger.

### 9.3 Why propagation via Queue + safety-net poll

- Queue fan-out fast path (≤ 5s p99 edge global).
- Periodic poll 60s safety net (Queue outage tolerance).
- Idempotent reconciliation — Worker accepts version >= local; rejects version < local (no flapping).

Alternativa rejeitada: Cloudflare Pub/Sub (newer; less mature in 2026); WebSocket persistent connections (per-Worker resource overhead).

### 9.4 Why D1 `config_change_log` 90d retention

- Audit-grade history for SOC 2 CC8.1 (system change management).
- 90d window covers: monthly drill + quarterly compliance review + audit lookbacks.
- > 90d rollback rare; if needed, restore from R2 long-term audit archive (CTRL-AUDIT-005 7y retention).

Alternativa rejeitada: 365d D1 retention — D1 storage cost regression; R2 archive já cobre long-term.

### 9.5 Why schema_version mandatory + ADR bump policy

- Schema drift catastrophic em distributed system (old Worker reads new payload silent default).
- ADR forces design review + migration plan.
- `serde(deny_unknown_fields)` enforces compile-time + runtime.

### 9.6 Why rollback API forward dependency dual-approval (WI-S13-002)

- Rollback é destructive op (config state replaced; downstream side-effects).
- Dual-approval gate prevents single-engineer rogue rollback.
- WI-S13-001 implements rollback API endpoint; WI-S13-002 adds dual-approval middleware (composition).

### 9.7 Why MFA freshness 30 min hard-check (CTRL-AUTH-010)

- Stale MFA = persistent session hijack vector.
- 30 min window balances UX (admin frequent operations) vs security (CTRL-AUTH-010 baseline).
- Hard-check (no grace) = INV-ADMIN-MFA-FRESHNESS enforcement.

### 9.8 Why no encrypted config payload at rest

- Config payload not secrets (feature flags, rate limits, retention policies = pseudo-public operational metadata).
- Secrets path separated (KMS + WI-S13-003 rotation worker).
- D1 + DO storage already encrypted at rest by Cloudflare platform.

### 9.9 Why ADR potencial?

- Sim — **ADR-XXXX**: "DO config-singleton single-instance per-region with CAS atomic update". Decisão arquitetural foundational; reuse pattern em S-14 (BYOK config) + S-16 (admin UI cache invalidation).
- Whitelist em `validate_references.py` até materializar.

## 10. Completeness Criteria SOTA

- [ ] **10.s13.001.1** Property test 10k iter (PR) + 100k iter (nightly) `prop_config_cas_concurrent_no_lost_writes` → 0 lost writes (EVT-002).
- [ ] **10.s13.001.2** Property test `prop_config_schema_drift_rejected` 10k iter → 100% rejection (EVT-002).
- [ ] **10.s13.001.3** Adversarial test: direct DO write bypass + schema field injection + CAS retry storm + propagation Queue outage + D1 retention bypass (EVT-040).
- [ ] **10.s13.001.4** E2E test: PUT update + propagation 5 regions ≤ 5s p99 + rollback drill ≤ 5 min p99 (EVT-018).
- [ ] **10.s13.001.5** SAST clean (cargo-audit + cargo-deny + clippy `-D warnings`) em `corelink-config-do` + `corelink-config-api` crates (EVT-002).
- [ ] **10.s13.001.6** Cost regression gate: DO storage + D1 history ≤ $20/mês.
- [ ] **10.s13.001.7** SLO-ADMIN-CONFIG-PROPAGATION ≤ 5s p99 sustained 30d staging *(GA Evidence Gate D+45)*.
- [ ] **10.s13.001.8** Monthly rollback drill executed; report committed.
- [ ] **10.s13.001.9** INV-ADMIN-MFA-FRESHNESS enforced em update + rollback paths (EVT-022).
- [ ] **10.s13.001.10** OWASP ASVS V14 (configuration) 100% checklist pass (EVT-002).

## 11. DoD

- [ ] Crates `corelink-config-do` + `corelink-config-api` compilam (workspace).
- [ ] DO `ConfigSingletonDO` implements `ConfigSingletonStore`; CAS atomic via DO transaction.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em PR + 100k green em nightly.
- [ ] E2E propagation test 5 regions ≤ 5s p99 green.
- [ ] Rollback drill ≤ 5 min p99 green (synthetic + monthly real drill).
- [ ] D1 `config_change_log` schema migration applied; daily purge cron operational.
- [ ] Métricas emitidas (5 listadas §6.1.5).
- [ ] Trace spans `admin.config.update|rollback|propagation` em OTel pipeline.
- [ ] rustdoc + 3 examples (CRUD basic, CAS retry pattern, rollback drill).
- [ ] `docs/internal/admin-plane.md` published (this WI scope).
- [ ] ADR-XXXX (DO config-singleton) escrito + ratificado.
- [ ] Code review (Architect + Security Lead).
- [ ] PRR Architect mini-sign-off (ship gate é WI-S13-006).
- [ ] Cost regression gate green.

## 12. Invariants Validated

### Mantidas

- **INV-ADMIN-MFA-FRESHNESS** (HIGH — registry §3.12 herdada): middleware enforce em update + rollback paths; expiry test green.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL — registry §3.6 herdada S-09): admin.config.update + admin.config.rollback events append-only; chain hash unbroken.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL — registry §3.14 herdada S-03): D1 batch atomic [DO trigger + audit_outbox INSERT + config_change_log INSERT]; failure rolls back.

### Novas

Nenhuma (rotation overlap + dual-approval invariants são WI-S13-002 + WI-S13-003).

TLA+ alignment: não-aplicável runtime CAS (DO transactional native); audit emission coberto por `audit_immutability.tla` (herdada).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| ConfigSingletonStore trait + DO impl | `crates/corelink-config-do/src/lib.rs` | Rust |
| ConfigPayload + types | `crates/corelink-config-do/src/types.rs` | Rust |
| Worker handlers (CRUD + rollback) | `crates/corelink-config-api/src/handlers.rs` | Rust |
| MFA freshness middleware | `crates/corelink-config-api/src/middleware/mfa_freshness.rs` | Rust |
| Propagation consumer Worker | `crates/corelink-config-consumer/` | Rust |
| D1 migration `config_change_log` | `migrations/0XX_config_change_log.sql` | SQL |
| Daily purge cron worker | `crates/corelink-config-purge-cron/` | Rust |
| Property tests | `crates/corelink-config-do/tests/prop_cas.rs` | Rust |
| Adversarial tests | `crates/corelink-config-api/tests/adversarial.rs` | Rust |
| E2E integration | `tests/e2e_config_propagation.rs` | Rust |
| ADR-XXXX (DO config-singleton) | `specs/03_architecture/adrs/ADR-XXXX-do-config-singleton.md` | Markdown |
| Admin plane doc | `docs/internal/admin-plane.md` | Markdown |
| Examples | `crates/corelink-config-api/examples/` (crud_basic.rs, cas_retry.rs, rollback_drill.rs) | Rust |

## 14. Quality Standards SOTA

- **14.s13.001.1** Zero `unsafe`; zero `unwrap` em src/ (allow em tests).
- **14.s13.001.2** rustdoc 100% public API + 3 examples.
- **14.s13.001.3** Test coverage ≥ 90% (`cargo tarpaulin`).
- **14.s13.001.4** Latência: DO read ≤ 10ms p99; PUT update ≤ 50ms p99 (incl. D1 batch); propagation ≤ 5s p99 edge global.
- **14.s13.001.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` clean.
- **14.s13.001.6** Métricas RED + propagation latency + CAS conflict rate.
- **14.s13.001.7** Runbook RB-FM-201 (config rate-limit drop) referenciado WI-S13-006.
- **14.s13.001.8** Breaking changes em `ConfigPayload` schema = bump major + ADR + migration plan.
- **14.s13.001.9** Memory: DO storage ≤ 1 MiB per-region (config payload + 90d history hashes; full payloads em D1).
- **14.s13.001.10** Cost regression gate em CI (DO + D1 + Queue ≤ $20/mês).
- **14.s13.001.11** Schema versioning enforced via ADR + serde `deny_unknown_fields`.

## 15. Chaos Experiments

1. **CAS retry storm**: 1000 concurrent admin updates same expected_version; observe metric alert + circuit breaker engages; verify 0 lost writes via property test.

2. **Propagation Queue outage**: simulate Queue down sustained 5 min; verify Worker periodic poll (60s) picks up new payload; SLO breached gracefully (alert SEV-3); recovery em ≤ 60s pós-Queue restore.

3. **Direct DO write bypass attempt**: red team tries to write DO storage directly; verify Cloudflare platform binding isolation rejects.

4. **Schema field injection**: inject unknown field via crafted PUT body; verify `deny_unknown_fields` rejects with 400.

5. **D1 90d retention bypass**: attempt rollback to version >90d; verify `VersionExpired` returned + audit log entry.

6. **Audit chain break em config update**: simulate D1 batch INSERT audit_outbox failure mid-update; verify DO update rolled back (atomic batch); INV-AUDIT-APPEND-ONLY preserved.

7. **Rollback during high-traffic window**: rollback to T-7d during peak load; verify recovery ≤ 5 min p99; downstream invariants preserved.

8. **Concurrent rollback + update**: admin A rollback, admin B update simultaneously; verify CAS resolves correctly (one wins, one 409).

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-13 ship gate é WI-S13-006; este WI passa por mini-PRR Architect + Security Lead review):

- [ ] All Gherkin scenarios green.
- [ ] Property tests + adversarial tests green.
- [ ] E2E propagation 5 regions ≤ 5s p99.
- [ ] Rollback drill ≤ 5 min p99.
- [ ] D1 daily purge cron operational.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-ADMIN.
- [ ] ADR-XXXX (DO config-singleton) published.
- [ ] Architect approval (CAS semantics + audit chain integration).
- [ ] Security Lead review (admin role + MFA freshness gate + audit emission).
- [ ] OWASP ASVS V14 100% pass.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate `corelink-config-do/` scaffold + ConfigPayload types | 1.5h |
| ST-002 | DO impl `ConfigSingletonDO` + CAS atomic via storage transaction | 3h |
| ST-003 | Schema validation (`deny_unknown_fields` + custom `validate_payload`) | 1.5h |
| ST-004 | Crate `corelink-config-api/` Worker handlers (CRUD + rollback) | 2.5h |
| ST-005 | MFA freshness middleware (CTRL-AUTH-010) | 1.5h |
| ST-006 | Propagation Queue producer + consumer Worker | 2.5h |
| ST-007 | D1 migration `config_change_log` + daily purge cron | 1.5h |
| ST-008 | Métricas emit (5 metrics) + trace spans | 1.5h |
| ST-009 | Property tests 10k iter (4 props) | 2.5h |
| ST-010 | Adversarial tests (5 scenarios) | 2h |
| ST-011 | E2E integration test propagation 5 regions | 2h |
| ST-012 | Chaos test propagation outage + rollback drill | 1.5h |
| ST-013 | rustdoc + 3 examples | 1h |
| ST-014 | `docs/internal/admin-plane.md` (this WI scope) | 1h |
| ST-015 | ADR-XXXX redação | 1h |
| ST-016 | Code review (Architect + Security Lead) iteration | 1.5h |

**Total Optimistic**: ~28h. **PERT** (O=14h, M=22h, P=36h, per spec contract §12): **23.0h**. Sub-tasks soma é detail-grain; PERT spec contract é consolidated.

## 18. Dependencies

### Hard blockers

- **S-03 SEALED** (admin role + WebAuthn step-up + MFA freshness Tower middleware foundation).
- **S-09 SEALED** (audit chain hash + audit_outbox D1 batch atomic pattern + métricas underscored Prometheus pipeline).

### Soft blockers

- Cloudflare Queue available em region (Cloudflare platform task; not blocker para spec).

### Outbound

- **WI-S13-002** (admin API + dual-approval) — consumes config-singleton para feature flag toggles + retention policy changes; rollback endpoint requires dual-approval middleware.
- **WI-S13-003** (secret rotation) — rotation worker reads `retention_policies` from config-singleton.
- **WI-S13-005** (progressive rollout) — controller reads feature flags from config-singleton para stage gating.
- **WI-S13-006** (PRR ship gate) — gates S-13 close.

## 19. Effort PERT

O: 14h, M: 22h, P: 36h → PERT **23.0h** (per spec contract §12).

## 20. Time-boxing

**28h hard limit**. If exceeded → escalation: split em sub-WI (DO + CAS vs propagation + rollback).

## 21. Observability

5 métricas listadas §6.1.5. Trace spans `admin.config.update|rollback|propagation` com attributes:
- `config.version` (u64)
- `config.expected_version` (u64)
- `config.payload_hash` (string hex 64)
- `actor.user_id` (UUID)
- `result` (enum)

Logs structured JSON; nivel INFO em success, WARN em CAS conflict, ERROR em propagation timeout.

Dashboard widget DASH-ADMIN:
- Config propagation latency p99 (per region) — gauge over time.
- CAS conflict rate.
- Rollback success ratio + recovery time.
- D1 history size gauge.

## 22. Cost Analysis

- DO storage: ~10 KiB per-region × 5 regions = 50 KiB; ~$0.10/mês.
- D1 `config_change_log`: ~1 KiB per change × 100 changes/mês × 90d retention = 9 MB; ~$1/mês.
- Cloudflare Queue: ~100 events/dia × 30 = 3000/mês × $0.40/M = negligível.
- Worker invocations (handlers): ~1000/mês × $0.15/M = negligível.
- **Total custo direto WI-S13-001**: ~$5/mês = $60/yr. Negligível vs alternativas (Postgres single-row + Redis pub-sub ~$50/mês).

## 23. API Contract

`ConfigSingletonStore` é internal Rust trait; HTTP API público:
- `GET /v1/admin/config/current` → 200 `(version, payload)`.
- `PUT /v1/admin/config` → 200 `(new_version)` ou 409 `VersionConflict` ou 400 `SchemaInvalid`.
- `POST /v1/admin/config/rollback?to_version=X` → 200 `(new_version)` ou 410 `VersionExpired` ou 404 `VersionUnknown` ou 403 `DualApprovalMissing`.
- `GET /v1/admin/config/history?limit=N` → 200 `Vec<ConfigVersionEntry>`.

API semver stable post v1.0; breaking changes em payload schema = bump `schema_version` + ADR + migration plan.

## 24. Post-mortem Hooks

- CAS lost-write detected (post-property-test or production) → CRITICAL post-mortem.
- Propagation SLO violated > 1h sustained → SEV-2 + post-mortem.
- Rollback drill failure (>5 min p99) → SEV-2 + post-mortem.
- D1 retention bypass (row >90d found) → SEV-3 + post-mortem (cron failure).
- Audit chain break em config write path → CRITICAL + 5-Why obrigatório.

## 25. Rollback / Recovery

- Code rollback: revert PR + redeploy Worker.
- Data rollback: rollback to previous `version` via API; recovery ≤ 5 min p99.
- DO state recovery: from D1 `config_change_log` history (90d window).
- RTO: ≤ 5 min (rollback API).
- RPO: 0 (audit chain unbroken; D1 history append-only).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: admin role check + WebAuthn step-up + MFA freshness ≤ 30 min hard-check.
- **Tampering**: CAS atomic version-aware; concurrent updates lost-write impossible.
- **Repudiation**: D1 `config_change_log` + audit chain hash unbroken.
- **Information disclosure**: config payload pseudo-public; secrets em separate path (rotation worker).
- **DoS**: CAS retry storm metric alert + circuit breaker.
- **Elevation of privilege**: rollback requires dual-approval (WI-S13-002 forward).

**LINDDUN delta**:
- **Linkability**: admin actor em audit é necessário (compliance accountability).
- **Identifiability**: admin user_id + email_hash em D1 + audit (intentional CTRL-AUDIT-002).
- **Non-repudiation**: WebAuthn attestation embedded em audit (CTRL-AUDIT-003).
- **Detectability**: config changes publicly tracked em audit (internal team).
- **Disclosure**: payload pseudo-public; no secrets.
- **Unawareness**: admin onboarding training + runbook documented.
- **Non-compliance**: SOC 2 CC6.7/CC8.1 + ISO 27001 A.5.15/A.5.16 + LGPD Art. 38 satisfied.

## 27. Knowledge Transfer

- `crates/corelink-config-api/README.md` — overview + integration pattern.
- ADR-XXXX — DO config-singleton single-instance per-region rationale.
- Doc `docs/internal/admin-plane.md` — sequence diagram update + propagation timing model.
- Workshop interno (1h) com Architect + Security Lead pós-merge.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | CAS lost-write race condition | L | H | CRITICAL | M | LOW | DO transactional + property test 10k concurrent |
| R-002 | Propagation Queue outage > 5s | M | M | MEDIUM | M | LOW | Periodic poll 60s safety net + chaos test |
| R-003 | Schema drift catastrophe | L | H | CRITICAL | M | LOW | `deny_unknown_fields` + ADR bump policy |
| R-004 | Rollback corrupts runtime state | L | M | HIGH | L | LOW | Pre-rollback simulation + monthly drill + manual override |
| R-005 | Direct DO write bypass | L | H | CRITICAL | L | LOW | Cloudflare platform binding isolation |
| R-006 | D1 retention purge bypass | L | M | MEDIUM | L | LOW | Daily cron + monitoring |
| R-007 | Audit chain break em config path | L | H | HIGH | M | LOW | D1 atomic batch + chaos test fail-closed |
| R-008 | CAS retry storm DoS | M | L | LOW | L | LOW | Client-side exponential backoff + circuit breaker |
| R-009 | Propagation latency regression | L | M | MEDIUM | L | LOW | Cost regression gate + benchmark |
| R-010 | MFA freshness bypass | L | H | CRITICAL | L | LOW | Hard-check 30 min + signed timestamp + clock-skew tolerance |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect review DO single-instance per-region + CAS semantics; ADR-XXXX outline.
2. **Code (D+1)**: peer review (folded into Engineer + Architect per ADR-0034).
3. **Security (D+1)**: Security Lead review admin role + MFA freshness gate + audit emission.
4. **Property test (pre-merge D+2)**: 10k iter green em PR; 100k iter green em nightly.
5. **Chaos (pre-merge D+2)**: propagation outage + rollback drill.
6. **PRR mini (D+3)**: Architect + Security Lead sign-off (gates inclusion em WI-S13-006 ship gate).

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — DO single-instance per-region + CAS semantics + audit chain integration_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — admin role + MFA freshness + audit emission boundary review_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — propagation SLO + monthly rollback drill_ | _pending_ | _pending_ |
| 6 | Engineer (S-13 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; SOC 2 CC6.7/CC8.1 evidence pack_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; LGPD Art. 38 audit log review_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; admin API surface threat model_ | _pending_ | _pending_ |

> Crypto SME folds into Architect role specialization (per framework §33.5.4.3 + ADR-0034). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S13-001 (cycle 12.S13.0). |

## 32. Anti-patterns evitados

- Skip CAS atomic (lost-write race condition catastrophic).
- Skip schema_version bumping policy (silent drift catastrophic).
- Skip propagation SLO (stale config = blast radius global).
- Skip D1 90d retention (audit-grade history mandatory).
- Skip rollback API (recovery vector mandatory).
- Direct DO write path (Worker binding only).
- Encrypt config payload (config not secrets).
- Bypass MFA freshness em update (CTRL-AUTH-010).
- Auto-apply schema migration sem ADR.
- Long-running write transaction (DO native CAS).
- Skip audit chain integration.

---

**Fim WI-S13-001.** Próximo: WI-S13-002 (admin API + dual-approval enforcement + collusion-rotation defense).
