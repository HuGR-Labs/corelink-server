---
id: "WI-S13-003"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.3.0"
created: "2026-04-28"
updated: "2026-05-27"
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
  - "KEY-MANAGEMENT"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
tags: ["wi", "s13", "admin-plane", "secret-rotation", "tdk", "pat-signing", "audit-chain", "byok", "roll-forward", "high-risk"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-rotation-worker` was absorbed into `corelink-ops` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-ops-absorption.md. Canonical consumer path is now `corelink_ops::*`.

# WI-S13-003 — Secret Rotation Worker (5 Asset Types: TDK 7d / PAT Signing 24h / Audit Chain 24h / Admin Signing 24h / BYOK 7d) + Per-Asset Adapter + PAT-ROLL-FORWARD-001 Auto-Rollback se Downstream Errors > 1% + Métricas Observability + INV-KEY-OVERLAP Property Test 10k Per Asset Class

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-13](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S13-003 |
| Título | Secret rotation worker zero-downtime para 5 asset types respeitando overlap canonical em `key_management.md §3.2.1` (ADR-0018; admin signing key adicionado Lote 10.13 codex P0): TDK (tenant derivation keys, S-01) overlap 7d (CTRL-KEY-005/006) / PAT signing keys (S-03) overlap 24h / Audit chain key (S-09) overlap 24h / Admin signing key (per-region HMAC para dual-approval; consumed by WI-S13-002) overlap 24h / BYOK customer CMK (S-14 customer-trigger) overlap 7d; per-asset adapter pattern com state machine (pending → active → overlap → retired → destroyed); PAT-ROLL-FORWARD-001 auto-rollback se downstream errors > 1% durante rotation; métricas `corelink_admin_rotation_*` observability; property test 10k per asset class verifies overlap respected (old + new ambos válidos para reads; writes apenas para new = INV-KEY-NO-SKIP); INV-KEY-OVERLAP enforced runtime |
| Sprint | S-13 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (secret rotation cripto-load-bearing; bypass = chaves stale = blast radius global; rotation in-flight breakage = FM-204 catastrophic) |

## 1. Intent

Implementar secret rotation worker zero-downtime cross-asset que respeita overlap canonical per `key_management.md §3.2.1` (ADR-0018) tabela autoritativa, mitigando FM-204 (secret rotation quebra serviço): (1) **TDK** (tenant derivation keys, S-01) overlap **7d** (CTRL-KEY-005 rotation + CTRL-KEY-006 overlap) — re-wrap envelope CAS background job ≥ TB-scale; 24h causa starvation; (2) **PAT signing keys** (S-03 hybrid HMAC + Argon2id cycle 9 SEAL decision (a)) overlap **24h** — curto blast radius; PAT verifica HMAC sig fast-fail ≤100µs antes Argon2id; multi-key support via signing_key_id column herdada `data_model.md §4.1`; (3) **Audit chain key** (S-09 R-S09-10, per-region) overlap **24h** — hash chain integrity precisa rotation rápida; tampering window minimal; (4) **Admin signing key** (per-region HMAC-SHA256 usada por WI-S13-002 dual-approval para assinar `op_payload || nonce || ts`) overlap **24h** — curto blast radius (igual PAT signing); admin op é dual-approver-bound; consumed by WI-S13-002 via key cache (per-region pull); ADR-0018 5ª asset class adicionada Lote 10.13 codex P0. (5) **BYOK customer CMK** (S-14, customer-trigger) overlap **7d** — CoreLink-side cache; customer notification window. Worker implements PAT-ROLL-FORWARD-001 auto-rollback se downstream errors > 1% durante rotation (rollback = revert active key promoted; old key promoted back; preserve INV-KEY-NO-SKIP). Property test 10k per asset class verifies overlap respected; INV-KEY-OVERLAP enforced runtime via state machine.

```rust
// File: crates/corelink-rotation-worker/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AssetClass {
    Tdk,                    // 7d overlap (CTRL-KEY-005 rotation + CTRL-KEY-006 overlap; canonical key_management.md §3.2.1)
    PatSigning,             // 24h overlap (CTRL-KEY-005/006)
    AuditChain,             // 24h overlap (per-region; CTRL-KEY-005/006)
    AdminSigning,           // 24h overlap (per-region HMAC para dual-approval; CTRL-KEY-005/006; ADR-0018 5ª asset class adicionada Lote 10.13)
    Byok,                   // 7d overlap (customer-trigger; CTRL-KEY-010/011/012)
}

impl AssetClass {
    pub fn overlap_seconds(&self) -> u64 {
        match self {
            Self::Tdk => 7 * 24 * 3600,         // 7d
            Self::PatSigning => 24 * 3600,      // 24h
            Self::AuditChain => 24 * 3600,      // 24h
            Self::AdminSigning => 24 * 3600,    // 24h
            Self::Byok => 7 * 24 * 3600,        // 7d
        }
    }
    pub fn hard_upper_bound_seconds(&self) -> u64 {
        30 * 24 * 3600                          // 30d hard upper per key_management.md §3.2.1
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyState {
    Pending,                                   // generated, not yet active
    Active,                                    // current write key
    Overlap,                                   // accepted for reads, not for writes
    Retired,                                   // not accepted (post-overlap window)
    Destroyed,                                 // material zeroized
    RolledBack,                                // promotion reverted (auto-rollback path)
}

#[async_trait]
pub trait RotationAdapter: Send + Sync {
    fn asset_class(&self) -> AssetClass;

    /// Generate new key material (pending state).
    async fn generate(&self) -> Result<KeyHandle, RotationError>;

    /// Promote pending → active; previous active → overlap.
    async fn promote(&self, new: &KeyHandle) -> Result<(), RotationError>;

    /// Re-key downstream resources (e.g., re-wrap envelopes for TDK).
    /// Reports progress 0..=1.0; bounded duration per asset class.
    async fn rekey_downstream(
        &self,
        new: &KeyHandle,
        progress_callback: Box<dyn Fn(f64) + Send + Sync>,
    ) -> Result<(), RotationError>;

    /// Move overlap → retired (post overlap window).
    async fn retire(&self, old: &KeyHandle) -> Result<(), RotationError>;

    /// Destroy retired key material (zeroize + audit emit).
    async fn destroy(&self, retired: &KeyHandle) -> Result<(), RotationError>;

    /// Auto-rollback: revert active promotion (PAT-ROLL-FORWARD-001).
    async fn rollback(
        &self,
        new: &KeyHandle,
        previous_active: &KeyHandle,
    ) -> Result<(), RotationError>;

    /// Health check: downstream error rate during rotation.
    async fn downstream_error_rate(&self) -> Result<f64, RotationError>;
}

#[derive(Debug, Clone)]
pub struct KeyHandle {
    pub key_id: u64,
    pub asset_class: AssetClass,
    pub state: KeyState,
    pub created_at_ms: u64,
    pub promoted_at_ms: Option<u64>,
    pub overlap_until_ms: Option<u64>,
    pub retired_at_ms: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum RotationError {
    #[error("downstream error rate {0:.4} exceeds 1% threshold; auto-rollback triggered")]
    DownstreamErrorThreshold(f64),
    #[error("key state transition invalid: {from:?} → {to:?}")]
    InvalidTransition { from: KeyState, to: KeyState },
    #[error("overlap window {seconds}s exceeds hard upper bound 30d")]
    OverlapExceedsHardUpper { seconds: u64 },
    #[error("rotation in-flight (asset_class={0:?}; previous still in progress)")]
    RotationInFlight(AssetClass),
    #[error("KMS error: {0}")]
    Kms(String),
    #[error("D1 storage error: {0}")]
    Storage(String),
}
```

State machine canonical (matches key_management.md §3.2 InvKeyNoSkip):
```
pending --[promote]--> active --[next rotation]--> overlap --[overlap window expired]--> retired --[grace 90d]--> destroyed
                          |
                          +--[downstream errors > 1%]--> rolled_back (active demoted; previous re-promoted)
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Secret rotation é boundary cripto-load-bearing operacional crítico: cada um dos 5 asset types governa um domain inteiro: TDK governa per-tenant cripto isolation (CTRL-ISO-001 + INV-TENANT-ISOLATION); PAT signing key governa authentication path (CTRL-AUTH-001 + INV-AUTH-PAT-HMAC-SIG-VERIFIED + decision (a) hybrid HMAC + Argon2id cycle 9 SEAL); audit chain key governa forensic integrity (INV-AUDIT-APPEND-ONLY + INV-OBS-AUDIT-CHAIN-INTEGRITY); BYOK governa customer cripto sovereignty (INV-BYOK-CRYPTO-SOVEREIGNTY S-14). Bug em qualquer um desses paths = FM-204 catastrophic — rotation in-flight breakage causes either authentication failures (PAT) ou data inaccessibility (TDK) ou audit chain break (audit chain key) ou admin signing replay (admin signing key) ou customer cripto sovereignty violation (BYOK).

**Bugs catastróficos possíveis** (todos endereçados):

1. **Overlap window violation (per asset class)**: rotation completes before overlap window; in-flight reads using old key fail. Mitigação: per-asset adapter respeita `key_management.md §3.2.1` overlap canonical (7d / 24h / 24h / 7d); state machine enforce; property test 10k per asset class verifies overlap respected (old + new ambos válidos para reads).

2. **INV-KEY-NO-SKIP violation**: writes use key em invalid state ({pending, retired, destroyed}); INV registry §3.13 herdada. Mitigação: state machine atomic transitions; writes only when state=active; reads accept active OR overlap (multi-key support via signing_key_id column herdada `data_model.md §4.1`); property test verifies writes never em invalid state.

3. **Downstream errors during rotation > 1%**: rotation completes mas creates partial breakage (e.g., 5% PAT verify failures durante PAT signing key rotation). Mitigação: PAT-ROLL-FORWARD-001 auto-rollback — adapter probes `downstream_error_rate` every 60s during rotation; if > 1% sustained 5 min = auto-rollback (revert active promotion; previous re-promoted); SEV-2 alert + post-mortem.

4. **Hard upper bound 30d violation**: rotation overlap > 30d sem ADR. Mitigação: `AssetClass::hard_upper_bound_seconds()` returns 30d; adapter rejects `OverlapExceedsHardUpper` error; ADR + Security lead sign-off mandatory para override.

5. **Rotation in-flight collision**: concurrent rotation of same asset type. Mitigação: D1 UNIQUE `(asset_class, status='in_progress')` prevents concurrent; `RotationInFlight` error returned.

6. **Audit chain key rotation breaks chain integrity**: rotation completes mas hash chain has gap (event N signed with old key, event N+1 with new key, verifier can't continue). Mitigação: chain key rotation includes overlap segment with both keys; verifier accepts both during overlap; INV-OBS-AUDIT-CHAIN-INTEGRITY herdada S-09 + daily verifier alerts em break.

7. **TDK rotation re-wrap takes longer than 7d**: TB-scale background job slow; 7d window expires before re-wrap complete; in-flight reads fail. Mitigação: re-wrap progress metric `corelink_key_rewrap_progress_ratio` herdada `key_management.md §3.3`; alert SEV-3 if progress < 90% at day 6; pause rotation completion if needed; scale-out re-wrap workers.

8. **BYOK customer-trigger rotation but customer revokes CMK mid-overlap**: customer signals CMK revoke during 7d overlap; CoreLink-side DEK cache invalidated 5 min hard; old envelopes inaccessible. Mitigação: INV-BYOK-CRYPTO-SOVEREIGNTY herdada S-14 — customer revoke acceptable (intentional); rotation worker handles via `retire(old)` early.

9. **Audit emission failure during rotation**: rotation state transition succeeds mas audit_outbox INSERT fails; INV-KEY-AUDIT herdada §3.13 broken. Mitigação: D1 atomic batch [state transition + audit_outbox INSERT]; failure rolls back; chaos test fail-closed.

**Atacante adversarial scenarios**:

- **Force rotation completion bypass overlap**: attacker triggers rotation pause/resume to bypass overlap window. Mitigação: state machine atomic transitions; state stored em D1; transitions audited; bypass attempt detected via daily chain verifier.

- **Inject downstream errors to trigger auto-rollback maliciously**: attacker creates synthetic error spike to trigger rollback (denial of service via rollback). Mitigação: 5 min sustained threshold (not instant); SEV-2 alert + manual review; cost regression budget bounds rollback frequency.

- **Replay old key after retired**: attacker tries to use retired key for writes. Mitigação: state machine rejects writes em retired state; INV-KEY-NO-SKIP enforced; D1 query checks state pre-write.

- **Attack overlap window for in-flight read**: attacker captures pre-rotation request; replays during overlap; reads acceptable but writes from old key rejected. Mitigação: nonce replay protection herdada S-03; reads em overlap acceptable (intentional INV-KEY-OVERLAP property).

- **TDK key compromise pre-rotation**: attacker captures TDK; rotation 7d overlap mitigates window. Mitigação: rotation cadence + emergency rotation procedure (break-glass via `key_management.md §7`).

**Risk justification HIGH_RISK**:

- **FF-HR-005**: 5 asset types governing entire substrate (CTRL-ISO + CTRL-AUTH + CTRL-AUDIT + CTRL-ADMIN + CTRL-CRYPTO).
- **Reversibility**: PAT-ROLL-FORWARD-001 ≤ 5 min recovery; mas data corruption (TDK re-wrap mid-flight broken) requires manual recovery + customer notification.

11 sign-offs canonical incl. Architect (Crypto SME specialization MANDATORY: cripto algorithm review per asset + state machine TLA+ alignment + KMS integration + audit chain rotation + property test cross-validation) + Security Lead + AppSec + Compliance Officer (SOC 2 CC6.7 + NIST SP 800-57 Pt 1 Rev 5 §5.3 attestation).

## 3. Customer Impact & Journey

**Persona 1 — SecOps lead em prospect enterprise (RFP)**:
- RFP question: "How are encryption keys rotated? What's the overlap policy?".
- Evidence: `key_management.md §3.2.1` canonical table per asset; rotation worker S-13 implementation; property test 10k per asset class verifies overlap; metric `corelink_admin_rotation_overlap_seconds{asset_type}` baseline against canonical.
- Diferenciador: 95%+ OSS Rust SaaS opera ad-hoc rotation; CoreLink S-13 = NIST SP 800-57 Pt 1 Rev 5 §5.3 + ADR-0018 ratificado.

**Persona 2 — Auditor SOC 2 + ISO 27001 + FIPS 140-2**:
- CTRL-CRED-003 attestation (rotation enforcement); rotation drill log (EVT-001).
- Evidence pack: per-rotation audit chain (event per state transition: pending → active → overlap → retired → destroyed); 7y retention.
- Property test 10k per asset class verifies overlap respected.

**Persona 3 — Customer with BYOK (S-14 forward)**:
- Customer-trigger rotation: API `POST /v1/customer/byok/rotate` initiates per-customer; CoreLink rotation worker handles 7d overlap.
- Customer notification window matches overlap window; customer can validate new key access during overlap.

**SLA addendum**:
- Rotation overlap respected per asset class (TDK 7d / PAT 24h / audit 24h / admin signing 24h / BYOK 7d; 5 asset classes).
- Hard upper bound 30d sem ADR.
- Auto-rollback ≤ 5 min if downstream errors > 1% sustained 5 min.
- Re-wrap progress alert SEV-3 if < 90% at 80% overlap window elapsed.
- Property test cadence: 10k iter PR + 100k iter nightly per asset class.

## 4. Capability Mapping

- **CAP-ADMIN-003** (Secret rotation automation cross-asset) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.3` + `security_model.md §6.11 (CTRL-CRED-003)` + `key_management.md §3.2.1 (overlap canonical) + §3.3 (online rotation procedure)` + `resilience_patterns.md §3.7 (PAT-ROLL-FORWARD-001)` + `invariant_registry.md §3.13 (INV-KEY-OVERLAP + INV-KEY-NO-SKIP + INV-KEY-AUDIT)`.

## 5. Tipo

Background worker + KMS adapter + state machine; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Crate `corelink-rotation-worker/`** (Cloudflare Workers cron):
   - `RotationOrchestrator` struct + cron scheduler.
   - Per-asset cron schedule:
     - TDK: weekly cron (7d cadence + 7d overlap = effective 7d windows). Admin signing: daily cron (24h cadence + 24h overlap).
     - PAT signing: daily cron 02:00 UTC (24h overlap).
     - Audit chain: daily cron 02:30 UTC (24h overlap, per-region staggered).
     - BYOK: customer-triggered (no cron; trigger via admin API per-customer).
   - State machine driver — D1-backed; transitions atomic per asset.
   - Auto-rollback driver (PAT-ROLL-FORWARD-001).

2. **Crate `corelink-rotation-adapters/`** (per-asset adapter):
   - `TdkRotationAdapter` (S-01 integration; uses TDK envelope re-wrap pattern herdada).
   - `PatSigningRotationAdapter` (S-03 integration; uses signing_key_id column multi-key support).
   - `AuditChainRotationAdapter` (S-09 integration; per-region; daily verifier compatible).
   - `AdminSigningRotationAdapter` (per-region; pushes new signing key to dual-approval gate cache via DO subscribe-pub; consumed by WI-S13-002 verify path; multi-key support via key_id index).
   - `ByokRotationAdapter` (S-14 forward stub; customer-trigger interface).
   - Each adapter implements `RotationAdapter` trait.

3. **D1 schema migration** (`rotation_state` table):
   ```sql
   CREATE TABLE rotation_state (
       key_id BIGINT NOT NULL,
       asset_class TEXT NOT NULL CHECK (asset_class IN ('tdk', 'pat_signing', 'audit_chain', 'byok')),
       state TEXT NOT NULL CHECK (state IN ('pending', 'active', 'overlap', 'retired', 'destroyed', 'rolled_back')),
       region TEXT NOT NULL,
       created_at_ms BIGINT NOT NULL,
       promoted_at_ms BIGINT,
       overlap_until_ms BIGINT,
       retired_at_ms BIGINT,
       destroyed_at_ms BIGINT,
       PRIMARY KEY (asset_class, region, key_id)
   );
   CREATE UNIQUE INDEX idx_rotation_in_progress ON rotation_state(asset_class, region) WHERE state IN ('pending');
   ```
   - Index prevents concurrent rotation per (asset_class, region).

4. **Auto-rollback driver (PAT-ROLL-FORWARD-001)**:
   - Probe `downstream_error_rate` every 60s during rotation overlap window.
   - If error rate > 1% sustained 5 min → trigger rollback:
     - Revert active promotion (active → rolled_back; previous overlap → active).
     - Audit emit `admin.rotation.rolled_back` event.
     - SEV-2 alert + Slack notification.
     - Post-mortem opens (5-Why obrigatório per spec contract §18).

5. **Métricas underscored Prometheus** (per `observability_model.md §3.1 + §4.1`; label `plan` aplicável onde fizer sentido):
   - `corelink_admin_rotation_in_progress{asset_type,status}` (gauge; status ∈ pending|active|overlap|retired|rolled_back|destroyed).
   - `corelink_admin_rotation_total{asset_type,outcome}` (counter; outcome ∈ ok|rolled_back|aborted|failed).
   - `corelink_admin_rotation_overlap_seconds{asset_type}` (histogram; baseline against canonical table 3.2.1).
   - `corelink_admin_rotation_downstream_error_rate{asset_type}` (gauge; alert > 0.01).
   - `corelink_admin_rotation_rekey_progress_ratio{asset_type}` (gauge 0..=1.0).
   - `corelink_admin_rotation_duration_seconds_bucket{asset_type,phase}` (histogram; phase ∈ generate|promote|rekey|retire|destroy).

6. **Observability** — trace spans `admin.rotation.{generate,promote,rekey_downstream,retire,destroy,rollback}` com attributes:
   - `rotation.asset_class` (enum).
   - `rotation.key_id` (u64).
   - `rotation.region` (string).
   - `rotation.state_from` (enum).
   - `rotation.state_to` (enum).
   - `rotation.overlap_seconds` (u64).
   - `result` (enum).

7. **Audit emission** — CloudEvent per state transition (INV-KEY-AUDIT herdada §3.13):
   - `corelink.admin.rotation.{phase}.{outcome}` (e.g., `corelink.admin.rotation.promote.ok`, `corelink.admin.rotation.rekey.failed`).
   - Payload: `{asset_class, region, key_id, state_from, state_to, overlap_seconds, downstream_error_rate, signature}`.
   - Atomic batch with rotation_state UPDATE (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).

8. **Property tests** (10k iter PR + 100k iter nightly; per asset class):
   - `prop_rotation_overlap_respected_tdk_7d`: simulate rotation start → completion; verify durante overlap período old + new ambos válidos para reads, writes apenas para new.
   - `prop_rotation_overlap_respected_pat_signing_24h`: same pattern.
   - `prop_rotation_overlap_respected_audit_chain_24h`: same pattern.
   - `prop_rotation_overlap_respected_byok_7d`: same pattern.
   - `prop_rotation_inv_key_no_skip`: writes never em invalid state ({pending, retired, destroyed}); 100k random rotation interleavings.
   - `prop_rotation_hard_upper_bound_30d`: synthesize overlap > 30d; assert rejection `OverlapExceedsHardUpper`.
   - `prop_rotation_rollback_idempotent`: rollback applied N times = single state restore.
   - `prop_rotation_concurrent_blocked`: 1000 concurrent rotation attempts same asset+region; assert exactly 1 succeeds, 999 `RotationInFlight`.

9. **Adversarial regression tests**:
   - Force rotation completion bypass overlap → state machine rejects.
   - Inject 5% downstream error rate → auto-rollback triggers within 5 min sustained.
   - Replay old key after retired → INV-KEY-NO-SKIP rejects.
   - Audit chain key rotation break injection → daily verifier alerts SEV-2.
   - TDK re-wrap takes > 7d (artificial slow) → progress metric alert SEV-3 at 6d.
   - BYOK customer revoke mid-overlap → adapter handles via early retire; INV-BYOK-CRYPTO-SOVEREIGNTY preserved.

10. **Integration test E2E** (real KMS + D1 + audit):
    - TDK rotation: generate new TDK; promote; re-wrap 100 envelopes; verify reads succeed with old + new during overlap; verify writes only with new.
    - PAT signing rotation: generate new key; promote; old + new accepted in PAT verify path during 24h overlap.
    - Audit chain rotation: generate new chain key; promote; daily verifier validates chain integrity across rotation boundary.
    - Auto-rollback: synthesize 5% error rate; verify rollback triggers ≤ 5 min; previous re-promoted; INV-KEY-NO-SKIP preserved.

11. **Chaos test** (covered also em WI-S13-006):
    - Simulate 1 TDK rotation in staging sustained 7d overlap; verify 0 read failures from in-flight reads (DoD §6 line 4 + 10.s13.4).
    - Simulate audit chain key rotation; verify chain integrity across boundary daily verifier green.

12. **Break-glass procedure** (`specs/05_runbooks/RB-ROTATION-EMERGENCY.md`):
    - Emergency rotation trigger (suspected key compromise).
    - Bypass overlap window with explicit Security lead + Architect + Crypto SME approval (waiver) + ADR.
    - Customer notification template (BYOK customers).

### 6.2 Out-of-scope (deferred)

- **DO config-singleton + rollback**: WI-S13-001 (composed; this WI integrates with admin signing key per-region distribution).
- **Admin API + dual-approval**: WI-S13-002 (rotation start = AdminOpType `SecretRotationStart`; composed).
- **Terraform drift detection**: WI-S13-004.
- **Progressive rollout controller**: WI-S13-005.
- **Property tests + RB dry-runs + PRR doc**: WI-S13-006.
- **Customer-trigger BYOK rotation full flow**: S-14 (this WI provides adapter stub).
- **HSM-backed signing keys**: pós-GA enterprise (current = Cloudflare Workers Secrets).
- **Multi-region key replication strategy**: per-region scoped at GA; cross-region forward S-14 + S-19.
- **FIPS 140-2 cripto module attestation**: pós-GA fed market.

## 7. Anti-Scope

- Skip overlap canonical per asset (FM-204 catastrophic).
- Skip INV-KEY-NO-SKIP enforcement (writes em invalid state = data corruption).
- Skip auto-rollback PAT-ROLL-FORWARD-001 (bad rotation = sustained breakage).
- Concurrent rotation same asset+region (D1 UNIQUE prevents).
- Override hard upper bound 30d sem ADR + Security lead sign-off.
- Skip audit emission per state transition (INV-KEY-AUDIT mandatory).
- Long-lived signing keys (rotation cadence enforced).
- Skip break-glass procedure (emergency rotation mandatory).
- Auto-apply rotation sem manual ADR for overrides.
- Direct D1 `rotation_state` write (Worker binding only).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Secret rotation worker — 5 asset types + auto-rollback + INV-KEY-OVERLAP

  Background:
    Given rotation_state table operational
    Given KMS adapter per asset class wired
    Given metrics + audit chain pipeline operational

  Scenario: TDK rotation 7d overlap sustained
    Given current TDK key_id=10 active
    When rotation worker triggers TDK rotation
    Then new key_id=11 generated (state=pending)
    And key_id=11 promoted (state=active); key_id=10 → state=overlap
    And re-wrap downstream envelopes (progress metric tracked)
    And during 7d overlap, both key_id=10 + key_id=11 accepted for reads
    And during 7d overlap, only key_id=11 accepts writes (INV-KEY-NO-SKIP)
    And after 7d, key_id=10 → retired
    And after 90d retired, key_id=10 → destroyed (zeroized)
    And metric corelink_admin_rotation_overlap_seconds{asset_type="tdk"} ≈ 7*24*3600

  Scenario: PAT signing rotation 24h overlap
    Given current PAT signing key_id=20 active
    When daily cron 02:00 UTC triggers PAT rotation
    Then new key_id=21 generated, promoted; key_id=20 → overlap
    And during 24h, PAT verify accepts both keys (multi-key support)
    And after 24h, key_id=20 → retired
    And metric corelink_admin_rotation_overlap_seconds{asset_type="pat_signing"} ≈ 24*3600

  Scenario: Audit chain key rotation 24h overlap (per-region)
    Given current audit chain key_id=30 active in region us-east
    When daily cron 02:30 UTC triggers audit chain rotation
    Then new key_id=31 generated, promoted; key_id=30 → overlap
    And during 24h overlap, daily verifier accepts chain spanning boundary
    And INV-OBS-AUDIT-CHAIN-INTEGRITY preserved (no chain break)

  Scenario: BYOK customer-trigger rotation 7d overlap
    Given customer C subscribes BYOK CMK
    When customer triggers POST /v1/customer/byok/rotate
    Then rotation worker generates new DEK wrapping; promote; old → overlap
    And during 7d, reads accept old + new
    And after 7d, old retired

  Scenario: Auto-rollback when downstream errors > 1% sustained 5 min
    Given rotation in-flight (asset=pat_signing, key_id=21 active, key_id=20 overlap)
    When downstream PAT verify error rate > 1% sustained 5 min
    Then PAT-ROLL-FORWARD-001 rollback triggers
    And key_id=21 → rolled_back; key_id=20 re-promoted active
    And metric corelink_admin_rotation_total{outcome="rolled_back"} incremented
    And SEV-2 alert fires
    And audit "admin.rotation.rolled_back" emitted

  Scenario: Hard upper bound 30d violation rejected
    When rotation worker tries promote with overlap_seconds = 31d
    Then RotationError::OverlapExceedsHardUpper returned
    And operation rejected
    And ADR + Security lead sign-off required to override

  Scenario: Concurrent rotation same asset+region blocked
    Given rotation in-flight (asset=tdk, region=us-east)
    When second rotation triggered same (asset=tdk, region=us-east)
    Then RotationError::RotationInFlight(Tdk) returned
    And D1 UNIQUE constraint enforces

  Scenario: INV-KEY-NO-SKIP enforced (writes never in invalid state)
    Given rotation state machine
    When writes attempted with key_id in state {pending, retired, destroyed}
    Then writes rejected with INV-KEY-NO-SKIP violation
    And property test 100k iter green

  Scenario: TDK re-wrap progress alert at 80% overlap window
    Given TDK rotation in-flight; overlap window 7d; elapsed 5.6d (80%)
    Given re-wrap progress metric < 90%
    Then alert SEV-3 fires
    And rotation worker may pause completion if needed

  Scenario: Audit emission per state transition
    Given rotation state transition pending → active
    When promote() succeeds
    Then audit_outbox row inserted (admin.rotation.promote.ok event)
    And atomic batch with rotation_state UPDATE
    And chain hash unbroken (INV-OBS-AUDIT-CHAIN-INTEGRITY)

  Scenario: Property test 10k iter per asset class green
    Given prop_rotation_overlap_respected_{tdk,pat_signing,audit_chain,byok} 10k iter
    When tests run nightly
    Then 0 violations of INV-KEY-OVERLAP
    And 0 violations of INV-KEY-NO-SKIP
```

## 9. Design Decisions

### 9.1 Why per-asset adapter pattern (NÃO unified rotation)

- Each asset type has different operational semantics:
  - TDK: re-wrap envelopes (expensive background job).
  - PAT signing: signing_key_id column multi-key support (cheap; verify-time selection).
  - Audit chain: per-region daily verifier integration.
  - BYOK: customer-trigger (not cron).
- Unified rotation = leaky abstraction; per-asset adapter clean separation.
- Trait-based composition allows future asset types (e.g., DSR receipt JWS key — already in `key_management.md §3.2.1` table).

### 9.2 Why overlap canonical per `key_management.md §3.2.1` (NÃO single 24h global)

- ADR-0018 ratificada documenta per-asset overlap (vs single 24h global proposed initially).
- TDK 7d: re-wrap TB-scale background job ≥ 7d; 24h causa starvation.
- PAT signing 24h: short blast radius; HMAC verify fast-fail; 24h sufficient.
- Audit chain 24h: hash chain integrity; tampering window minimal; 24h fits daily cron.
- BYOK 7d: customer notification window aligned.

### 9.3 Why hard upper bound 30d sem ADR

- `key_management.md §3.2.1` canonical: nenhum asset class pode exceder 30d sem ADR + Security lead sign-off.
- Justificativa: NIST SP 800-57 Pt 1 Rev 5 §5.3 recomenda overlap < 90d; CoreLink 30d defense-in-depth.

### 9.4 Why PAT-ROLL-FORWARD-001 auto-rollback (NÃO fail-forward only)

- FM-204 catastrophic: rotation in-flight breakage = customer authentication failures sustained.
- Auto-rollback ≤ 5 min mitigates blast radius.
- Manual override available (`wrangler deploy` rollback + admin API forward-rollback opcional).

### 9.5 Why downstream error rate threshold 1% sustained 5 min

- 1% threshold balances: false-positive (transient errors) vs detect real breakage.
- 5 min sustained: filters transient spikes; detects real regression.
- Configurable via ADR if business case (e.g., SLO-tier customers requiring 0.5%).

### 9.6 Why D1 atomic batch [state transition + audit_outbox]

- INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER + INV-KEY-AUDIT mandatory.
- Failure rolls back state transition; preserves chain integrity.

### 9.7 Why state machine D1-backed (NÃO in-memory)

- Cross-Worker consistency required (rotation worker + Worker handlers both query state).
- D1 transactional state ensures atomic transitions.
- DO alternative: rotation worker is bounded (per-asset cron); D1 sufficient.

### 9.8 Why audit chain key per-region (NÃO global)

- Per-region failure isolation: audit chain break em region X não propaga.
- Compatible with daily verifier per-region.
- Rotation 24h staggered across regions to avoid simultaneous transitions.

### 9.9 Why ADR potencial?

- Sim — **ADR-XXXX**: "Secret rotation worker per-asset adapter pattern + PAT-ROLL-FORWARD-001 auto-rollback + INV-KEY-OVERLAP enforcement S-13". Decisão arquitetural cripto-load-bearing; reuse pattern em S-14 BYOK + S-19 enterprise key escrow forward.
- Whitelist em `validate_references.py` até materializar.

## 10. Completeness Criteria SOTA

- [ ] **10.s13.003.1** Property test 10k iter per asset class (4 props) `prop_rotation_overlap_respected_*` → 0 INV-KEY-OVERLAP violations (EVT-002).
- [ ] **10.s13.003.2** Property test `prop_rotation_inv_key_no_skip` 10k iter PR + 100k iter nightly → 0 violations (EVT-022).
- [ ] **10.s13.003.3** Property test `prop_rotation_hard_upper_bound_30d` rejection 100% (EVT-002).
- [ ] **10.s13.003.4** Adversarial test: force completion bypass + injected error rate + retired replay + audit chain break + slow re-wrap + BYOK customer revoke (EVT-040).
- [ ] **10.s13.003.5** E2E test: 1 TDK rotation staging sustained 7d overlap com 0 read failures from in-flight reads (DoD 10.s13.4 *(GA Evidence Gate D+45)*).
- [ ] **10.s13.003.6** SAST clean (cargo-audit + cargo-deny + clippy `-D warnings`) em rotation crates (EVT-002).
- [ ] **10.s13.003.7** Cost regression gate: rotation worker infra ≤ $30/mês (cron Worker + D1 writes + KMS calls).
- [ ] **10.s13.003.8** SLO-ADMIN-ROTATION-OVERLAP respected per asset class verified em property test verde *(GA Evidence Gate D+45)*.
- [ ] **10.s13.003.9** INV-KEY-OVERLAP + INV-KEY-NO-SKIP + INV-KEY-AUDIT enforced runtime (registry §3.13 herdada) (EVT-022).
- [ ] **10.s13.003.10** OWASP ASVS V6 (cripto) + V7 (error handling/logging) 100% checklist pass (EVT-002).
- [ ] **10.s13.003.11** NIST SP 800-57 Pt 1 Rev 5 §5.3 attestation em PRR doc (EVT-031).
- [ ] **10.s13.003.12** Break-glass procedure RB-ROTATION-EMERGENCY.md committed + dry-run executed.

## 11. DoD

- [ ] Crates `corelink-rotation-worker` + `corelink-rotation-adapters` compilam.
- [ ] All 4 adapters implement `RotationAdapter` trait.
- [ ] All 11 Gherkin scenarios green em integration test.
- [ ] Property tests 4 per-asset + INV-KEY-NO-SKIP + hard upper + concurrent blocked + rollback idempotent green.
- [ ] Adversarial regression tests 6+ scenarios green.
- [ ] E2E TDK rotation 7d overlap sustained green em staging.
- [ ] D1 schema migration applied (`rotation_state` + UNIQUE index).
- [ ] CloudEvent audit emission per state transition verified.
- [ ] Métricas emitidas (6 listadas §6.1.5).
- [ ] Trace spans em OTel pipeline.
- [ ] rustdoc + 4 examples (one per asset class).
- [ ] `docs/internal/admin-plane.md` extended (rotation section).
- [ ] ADR-XXXX (rotation worker per-asset + PAT-ROLL-FORWARD-001) escrito + ratificado.
- [ ] Break-glass runbook RB-ROTATION-EMERGENCY.md committed + dry-run.
- [ ] Code review (Architect + Crypto SME folded mandatory + Security Lead + AppSec + Compliance Officer).
- [ ] PRR Architect + Crypto SME mini-sign-off (ship gate é WI-S13-006).
- [ ] Cost regression gate green.

## 12. Invariants Validated

### Mantidas

- **INV-KEY-OVERLAP** (HIGH — registry §3.13 herdada): este WI implementa primary; property test 10k per asset class verifies overlap respected (canonical table 3.2.1).
- **INV-KEY-NO-SKIP** (HIGH — registry §3.13 herdada): writes never em invalid state; state machine + property test 100k.
- **INV-KEY-AUDIT** (HIGH — registry §3.13 herdada): every state transition emits EVT-047 + EVT-028; chain unbroken.
- **INV-OBS-AUDIT-CHAIN-INTEGRITY** (HIGH — registry §3.12 herdada S-09): audit chain key rotation preserves integrity across boundary.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL — registry §3.14 herdada S-03): D1 atomic batch fail-closed.
- **INV-AUTH-PAT-HMAC-SIG-VERIFIED** (CRITICAL — registry §3.14 herdada S-03): PAT signing rotation preserves multi-key verify path.
- **INV-BYOK-CRYPTO-SOVEREIGNTY** (CRITICAL — registry §3.12 forward S-14): BYOK adapter respects customer sovereignty.

### Novas

Nenhuma (todos INVs já em registry; este WI verifica enforcement runtime).

TLA+ alignment: registry §4.2 indica `key_lifecycle.tla` PLANNED S-13; este WI provê implementation; spec TLA é forward (S-09 ou S-12 forward implementation per registry).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| RotationOrchestrator + cron | `crates/corelink-rotation-worker/src/lib.rs` | Rust |
| RotationAdapter trait + types | `crates/corelink-rotation-adapters/src/lib.rs` | Rust |
| TdkRotationAdapter | `crates/corelink-rotation-adapters/src/tdk.rs` | Rust |
| PatSigningRotationAdapter | `crates/corelink-rotation-adapters/src/pat_signing.rs` | Rust |
| AuditChainRotationAdapter | `crates/corelink-rotation-adapters/src/audit_chain.rs` | Rust |
| ByokRotationAdapter (stub) | `crates/corelink-rotation-adapters/src/byok.rs` | Rust |
| State machine driver | `crates/corelink-rotation-worker/src/state_machine.rs` | Rust |
| Auto-rollback (PAT-ROLL-FORWARD-001) | `crates/corelink-rotation-worker/src/rollback.rs` | Rust |
| D1 migration `rotation_state` | `migrations/0XX_rotation_state.sql` | SQL |
| Property tests | `crates/corelink-rotation-worker/tests/prop_rotation.rs` | Rust |
| Adversarial regression tests | `crates/corelink-rotation-worker/tests/adversarial.rs` | Rust |
| E2E integration | `tests/e2e_rotation_tdk_7d.rs` | Rust |
| ADR-XXXX (rotation worker per-asset) | `specs/03_architecture/adrs/ADR-XXXX-rotation-worker-per-asset.md` | Markdown |
| Break-glass runbook | `specs/05_runbooks/RB-ROTATION-EMERGENCY.md` | Markdown |
| Examples | `crates/corelink-rotation-worker/examples/` (tdk_rotation.rs, pat_rotation.rs, audit_chain_rotation.rs, byok_rotation_stub.rs) | Rust |

## 14. Quality Standards SOTA

- **14.s13.003.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s13.003.2** rustdoc 100% public API + 4 examples (one per asset class).
- **14.s13.003.3** Test coverage ≥ 90% (`cargo tarpaulin`); property tests 10k+100k.
- **14.s13.003.4** Latência: state transition ≤ 100ms p99; auto-rollback ≤ 5 min.
- **14.s13.003.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` clean.
- **14.s13.003.6** Métricas RED + per-asset breakdown.
- **14.s13.003.7** Runbook RB-ROTATION-EMERGENCY committed + dry-run.
- **14.s13.003.8** Breaking changes em `RotationAdapter` trait = bump major + ADR + migration plan.
- **14.s13.003.9** Memory: bounded em re-wrap loop; Zeroizing for key material.
- **14.s13.003.10** Cost regression gate em CI.
- **14.s13.003.11** NIST SP 800-57 Pt 1 Rev 5 §5.3 attestation.
- **14.s13.003.12** Constant-time HMAC compare for audit chain key (subtle::ConstantTimeEq).

## 15. Chaos Experiments

1. **TDK rotation 7d overlap real staging**: trigger TDK rotation; sustain 7d overlap; verify 0 read failures from in-flight reads (DoD 10.s13.4); zero downstream impact.

2. **PAT signing rotation 24h**: trigger; verify multi-key support em PAT verify path during 24h overlap; post-overlap old rejected.

3. **Audit chain key rotation 24h per-region**: trigger; verify daily verifier validates chain across boundary.

4. **Auto-rollback synthesis**: inject 5% downstream error rate sustained 5 min; verify PAT-ROLL-FORWARD-001 rollback triggers ≤ 5 min; previous re-promoted; INV-KEY-NO-SKIP preserved.

5. **Force completion bypass overlap**: red team tries pause/resume to bypass overlap; state machine atomic transitions reject.

6. **Concurrent rotation same asset+region**: 1000 concurrent attempts; D1 UNIQUE rejects 999.

7. **Hard upper bound 30d violation**: synthesize overlap = 31d; assert rejected with `OverlapExceedsHardUpper`.

8. **Audit emission failure during rotation**: inject D1 audit_outbox INSERT failure; verify state transition rolled back.

9. **TDK re-wrap slow injection**: artificially slow re-wrap to >7d; alert SEV-3 fires at 80% overlap window; rotation pause completion until catch-up.

10. **BYOK customer revoke mid-overlap**: simulate customer CMK revoke during 7d overlap; verify adapter handles via early retire; INV-BYOK-CRYPTO-SOVEREIGNTY preserved.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-13 ship gate é WI-S13-006; este WI passa por mini-PRR Architect + Crypto SME mandatory + Security Lead + Compliance Officer review):

- [ ] All 11 Gherkin scenarios green.
- [ ] Property tests 8+ props × 10k iter green.
- [ ] Adversarial regression tests 6+ scenarios green.
- [ ] E2E TDK rotation 7d overlap sustained green.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-ADMIN.
- [ ] ADR-XXXX (rotation worker per-asset) published.
- [ ] Crypto SME review (cripto algorithm review per asset + state machine + KMS integration + audit chain rotation + property test cross-validation).
- [ ] Compliance Officer review (NIST SP 800-57 + SOC 2 CC6.7 + ISO 27001 A.10.1).
- [ ] Architect approval (composition with WI-S13-001 + WI-S13-002).
- [ ] OWASP ASVS V6 + V7 100% pass.
- [ ] Break-glass runbook dry-run executed.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate `corelink-rotation-worker/` scaffold | 1.5h |
| ST-002 | RotationAdapter trait + types + state machine | 2.5h |
| ST-003 | TdkRotationAdapter (S-01 integration; re-wrap) | 3h |
| ST-004 | PatSigningRotationAdapter (S-03 integration; multi-key column) | 2h |
| ST-005 | AuditChainRotationAdapter (S-09 integration; per-region) | 2.5h |
| ST-006 | ByokRotationAdapter stub (S-14 forward) | 1.5h |
| ST-007 | Auto-rollback driver PAT-ROLL-FORWARD-001 | 2.5h |
| ST-008 | D1 migration `rotation_state` + UNIQUE index | 1h |
| ST-009 | CloudEvent audit emission per transition + atomic batch | 2h |
| ST-010 | Métricas emit (6 metrics) + trace spans | 2h |
| ST-011 | Property tests 4 per-asset + 4 cross-cutting (10k iter each) | 4h |
| ST-012 | Adversarial regression tests 6+ scenarios | 3h |
| ST-013 | E2E integration test TDK 7d overlap | 3h |
| ST-014 | Chaos test (auto-rollback + concurrent + hard upper) | 2h |
| ST-015 | rustdoc + 4 examples | 1.5h |
| ST-016 | Break-glass runbook RB-ROTATION-EMERGENCY.md + dry-run | 2h |
| ST-017 | ADR-XXXX redação | 2h |
| ST-018 | Code review (Architect + Crypto SME folded + Security Lead + AppSec + Compliance) iteration | 3h |

**Total Optimistic**: ~40h. **PERT** (O=16h, M=24h, P=40h, per spec contract §12): **25.3h**. Sub-tasks soma é detail-grain; PERT spec contract é consolidated.

## 18. Dependencies

### Hard blockers

- **S-01 SEALED** (TDK envelope re-wrap pattern foundation).
- **S-03 SEALED** (PAT signing multi-key column + signing_key_id; cycle 9 SEAL decision (a) hybrid HMAC + Argon2id).
- **S-09 SEALED** (audit chain hash + atomic batch + INV-OBS-AUDIT-CHAIN-INTEGRITY + per-region daily verifier).
- **WI-S13-001 SEALED** (config-singleton consume retention_policies for rotation cadence override).
- **WI-S13-002 SEALED** (admin API SecretRotationStart op type; dual-approval gate for manual rotation).

### Soft blockers

- KMS/Cloudflare Workers Secrets API operational (platform-level; not blocker for spec).

### Outbound

- **WI-S13-002** dual-approval middleware uses admin signing key from this rotation worker (24h overlap).
- **WI-S13-006** PRR ship gate gates S-13 close.
- **S-14** BYOK rotation worker reuses adapter pattern + customer-trigger interface.
- **S-19** enterprise key escrow forward consumes rotation framework.

## 19. Effort PERT

O: 16h, M: 24h, P: 40h → PERT **25.3h** (per spec contract §12).

## 20. Time-boxing

**32h hard limit owner**. If exceeded → escalation: split em sub-WI (orchestrator + state machine vs per-asset adapters vs auto-rollback).

## 21. Observability

6 métricas listadas §6.1.5. Trace spans em §6.1.6. Logs structured JSON; nivel INFO em ok, WARN em downstream errors threshold approach, ERROR em rollback.

Dashboard widget DASH-ADMIN:
- Rotation in-progress per asset type (gauge over time).
- Rotation overlap window per asset class (vs canonical table 3.2.1).
- Downstream error rate per asset (alert > 1%).
- Re-wrap progress ratio (per active rotation).
- Auto-rollback events count (alert > 0).
- Audit chain integrity per-region (daily verifier).

## 22. Cost Analysis

- Rotation worker cron: 4 cron schedules × 1 invocation/dia × $0.001 = negligible.
- D1 `rotation_state` writes: ~10 transitions/dia × 4 assets = 40 writes; ~$1/mês.
- KMS calls (Cloudflare Workers Secrets): bounded per rotation; ~$5/mês.
- Re-wrap background workers (TDK): TB-scale background; ~$10/mês durante active rotations.
- Audit emission: shared with S-09 audit chain.
- **Total custo direto WI-S13-003**: ~$20/mês = $240/yr. Bounded vs enterprise alternative (HSM ~$1k+/mês).

## 23. API Contract

`RotationAdapter` é internal Rust trait. Admin API for manual rotation:
- `POST /v1/admin/ops` body=`{op_type: "SecretRotationStart", asset_class: "tdk|pat_signing|audit_chain|byok"}` → 202 (rotation started; async).
- `GET /v1/admin/rotation/state?asset_class=X` → 200 `Vec<KeyHandle>` (admin-visible state).

API semver stable post v1.0; breaking changes em `AssetClass` enum = bump major + ADR + migration plan.

## 24. Post-mortem Hooks

- Secret rotation falhada com downstream errors > 1% → 5-Why obrigatório (per spec contract §18).
- Auto-rollback false-positive (rotation ok mas rollback triggered) > 1× mês → review thresholds + post-mortem.
- INV-KEY-OVERLAP violation detected (property test or production) → CRITICAL post-mortem.
- INV-KEY-NO-SKIP violation (write em invalid state) → CRITICAL + Security incident.
- Audit chain break em rotation boundary → CRITICAL + compliance officer.
- Hard upper bound 30d violation em production → SEV-2 + ADR retroactive.

## 25. Rollback / Recovery

- Code rollback: revert PR + redeploy Worker.
- Rotation rollback: PAT-ROLL-FORWARD-001 auto + manual override via admin API.
- Emergency rotation: break-glass procedure RB-ROTATION-EMERGENCY.md.
- RTO: ≤ 5 min (auto-rollback); ≤ 1h (manual emergency).
- RPO: 0 (audit chain unbroken; D1 state append-only).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: KMS adapter authenticated via Cloudflare Workers Secrets; no impersonation possible.
- **Tampering**: state machine atomic transitions D1; chain hash integrity preserved across rotation boundary.
- **Repudiation**: audit emission per state transition + 7y retention.
- **Information disclosure**: key material Zeroizing wrapped; never logged; redaction macros herdada S-03.
- **DoS**: auto-rollback ≤ 5 min; cost regression bounded.
- **Elevation of privilege**: rotation start = AdminOpType (dual-approval gated WI-S13-002).

**LINDDUN delta**:
- **Linkability**: rotation events em audit é necessário (compliance accountability).
- **Identifiability**: admin actor em audit (intentional CTRL-AUDIT-002).
- **Non-repudiation**: cripto property intentional (audit chain).
- **Detectability**: rotation events publicly tracked em audit (internal team).
- **Disclosure**: key material never in logs/traces/errors; ZeroizeOnDrop enforced.
- **Unawareness**: BYOK customers notified per rotation event (S-14 forward).
- **Non-compliance**: SOC 2 CC6.7 + ISO 27001 A.10.1 (key management) + NIST SP 800-57 Pt 1 Rev 5 §5.3 + LGPD Art. 38 + GDPR Art. 32 satisfied.

## 27. Knowledge Transfer

- `crates/corelink-rotation-worker/README.md` — overview + per-asset adapter pattern.
- ADR-XXXX — rotation worker + PAT-ROLL-FORWARD-001 ratification.
- Doc `docs/internal/admin-plane.md` (rotation section) — sequence diagram per asset class.
- Workshop interno (2h) com Architect + Crypto SME + Security Lead pós-merge.
- Onboarding test (10 questions): overlap canonical per asset, hard upper bound, INV-KEY-NO-SKIP, auto-rollback threshold, break-glass procedure.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Rotation in-flight breakage (FM-204) | M | M | HIGH | M | LOW | Overlap canonical per asset + PAT-ROLL-FORWARD-001 + chaos test |
| R-002 | INV-KEY-OVERLAP violation | L | H | CRITICAL | M | LOW | Property test 10k per asset + state machine + audit |
| R-003 | INV-KEY-NO-SKIP violation (writes em invalid state) | L | H | CRITICAL | M | LOW | State machine atomic + property test 100k |
| R-004 | Auto-rollback false-positive | M | L | MEDIUM | L | LOW | 5 min sustained threshold + manual override + alert |
| R-005 | Hard upper bound 30d violation | L | M | HIGH | L | LOW | Adapter rejects + ADR + Security lead sign-off |
| R-006 | Concurrent rotation collision | L | L | LOW | L | LOW | D1 UNIQUE + RotationInFlight error |
| R-007 | Audit chain break em rotation boundary | L | H | CRITICAL | M | LOW | Per-region rotation + daily verifier + chain hash |
| R-008 | TDK re-wrap slow > 7d | M | M | HIGH | M | LOW | Progress metric alert SEV-3 + scale-out re-wrap workers + pause completion |
| R-009 | KMS API outage | L | L | HIGH | L | LOW | Cloudflare platform SLA + local cache fallback |
| R-010 | Customer BYOK revoke mid-overlap | L | L | MEDIUM | L | LOW | Adapter handles early retire; INV-BYOK-CRYPTO-SOVEREIGNTY |
| R-011 | Property test flakiness | M | L | LOW | L | LOW | Deterministic seeds + retry policy |
| R-012 | Cost regression em rotation infra | M | L | LOW | L | LOW | Cost regression gate + benchmark |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Crypto SME folded review per-asset adapter pattern + state machine + ADR-XXXX outline.
2. **Per-asset review (D+1..D+3)**: Crypto SME pair-program each adapter (TDK + PAT signing + audit chain + BYOK stub).
3. **Code (D+3)**: peer review + Crypto SME pair-program adversarial tests.
4. **Security (D+3)**: Security Lead review threat model + KMS integration + key compromise scenarios.
5. **Compliance (D+4)**: Compliance Officer review NIST SP 800-57 + SOC 2 CC6.7 attestation.
6. **AppSec (D+4)**: AppSec review break-glass procedure + emergency rotation flow.
7. **Property test (pre-merge D+5)**: 10k iter green em PR; 100k iter green em nightly.
8. **Adversarial (pre-merge D+5)**: red team session — force completion bypass + key compromise scenarios.
9. **PRR mini (D+5)**: Architect + Crypto SME + Security Lead + Compliance Officer sign-off (gates inclusion em WI-S13-006 ship gate).

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — Crypto SME specialization MANDATORY (cripto algorithm review per asset + state machine TLA+ alignment + KMS integration + audit chain rotation + property test cross-validation)_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — secret rotation boundary review + key compromise scenarios + break-glass procedure_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — per-asset rotation cadence + auto-rollback + chaos test sustained_ | _pending_ | _pending_ |
| 6 | Engineer (S-13 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — NIST SP 800-57 Pt 1 Rev 5 §5.3 + SOC 2 CC6.7 + ISO 27001 A.10.1 attestation_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; LGPD Art. 38 + GDPR Art. 32 review_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — break-glass procedure + emergency rotation + insider threat scenarios_ | _pending_ | _pending_ |

> Crypto SME (cripto algorithm review per asset + state machine + KMS integration + audit chain rotation) folds into Architect role specialization MANDATORY. Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect per framework §33.5.4.3 + ADR-0034).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S13-003 (cycle 12.S13.0). |
| 1.1.0 | 2026-05-14 | Claude Sonnet 4.6 | SEALED: crates corelink-rotation-adapters + corelink-rotation-worker; 5 adapters; PAT-ROLL-FORWARD-001 RollbackDriver (ProbeContext); RotationOrchestrator; 6 Prometheus metric descriptors; 8 property test props × 10k; 10 adversarial tests; 4 examples; D1 migration 013_rotation_state.sql; wasm32-clean; clippy clean; 30 tests green; codex 8.5/10. |

## 32. Anti-patterns evitados

- Skip overlap canonical per asset (FM-204 catastrophic).
- Skip INV-KEY-NO-SKIP enforcement.
- Skip auto-rollback PAT-ROLL-FORWARD-001.
- Concurrent rotation same asset+region.
- Override hard upper bound 30d sem ADR + Security lead.
- Skip audit emission per state transition.
- Long-lived signing keys.
- Skip break-glass procedure.
- Auto-apply rotation override sem ADR.
- Direct D1 `rotation_state` write.
- Unified rotation pattern (per-asset adapter clean separation).

---

**Fim WI-S13-003.** Próximo: WI-S13-004 (terraform drift detection daily + RB-FM-206).
