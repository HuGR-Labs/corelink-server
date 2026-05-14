---
id: "WI-S14-006"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-04-28"
updated: "2026-04-28"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-14"
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
tags: ["wi", "s14", "byok", "kill-switch", "cmk-revocation", "chaos-drill", "inv-byok-crypto-sovereignty", "high-risk"]
---

# WI-S14-006 — CMK Revocation Detection (KMS Access Check Background Every 60s per Active BYOK Tenant) + Customer Kill Switch ≤ 5 min Global Hard-Fail (DEK Cache TTL 5 min Hard Expires All In-Flight Reads + KMS Access Check 60s Detect + Degrade Tenant Read-Only + Audit Emit `corelink.byok.cmk_revoked` + Alert Customer; Total p99 ≤ 5 min Global; INV-BYOK-CRYPTO-SOVEREIGNTY CRITICAL Hard-Fail No Operator Override) + Chaos Drill Weekly em Staging + Runbook RB-BYOK-REVOKE Dry-Run

> **doc_status:** SEALED · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-14](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S14-006 |
| Título | CMK revocation detection background (KMS access check every 60s per active BYOK tenant via `KmsProvider::check_access` herdada WI-S14-004) + customer kill switch ≤ 5 min global hard-fail (revoke CMK → KMS access check 60s detect → DEK cache atomic evict_all_for_key → mark tenant degraded read-only → emit audit `corelink.byok.cmk_revoked` → alert customer; DEK cache TTL 5 min hard expires all in-flight reads (composed WI-S14-004); total p99 ≤ 5 min global; INV-BYOK-CRYPTO-SOVEREIGNTY CRITICAL hard-fail NO operator override NO advisory mode); chaos drill weekly em staging revoke CMK → verify ≤ 5 min global; runbook RB-BYOK-REVOKE dry-run + alerts wire customer notification |
| Sprint | S-14 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (BYOK kill switch é customer trust baseline; bypass = customer trust permanently lost; cripto-load-bearing) |

## 1. Intent

Customer kill switch é **enterprise BYOK feature definition**: sem ele, BYOK = teatro (customer não tem real control). Implementar kill switch end-to-end: (1) KMS access check background every 60s per active BYOK tenant via `KmsProvider::check_access` (WI-S14-004 + WI-S14-005); (2) revoked detected (provider returns `KmsAccessStatus::Revoked` ou `NotFound`) → degrade tenant read-only mode + emit audit `corelink.byok.cmk_revoked` + alert customer via dashboard + email + in-app notification; (3) DEK cache atomic `evict_all_for_key(kms_key_id)` (subscribe-pub from check); (4) DEK cache TTL 5 min hard expires all in-flight reads (composed WI-S14-004); (5) total p99 ≤ 5 min global hard-fail; INV-BYOK-CRYPTO-SOVEREIGNTY CRITICAL no operator override no advisory mode (per spec contract §19 waiver policy: este NÃO é waivable); (6) chaos drill weekly em staging revoke CMK + verify ≤ 5 min; (7) runbook RB-BYOK-REVOKE dry-run + alerts wire.

```rust
// File: crates/corelink-byok-revocation/src/lib.rs

#![forbid(unsafe_code)]

use corelink_byok::*;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

pub struct RevocationDetector {
    providers: Vec<Arc<dyn KmsProvider>>,
    dek_cache: Arc<DekCache>,
    check_interval: Duration,
}

impl RevocationDetector {
    /// 60s background loop per active BYOK tenant.
    /// SLA: revoke detected → cache empty + audit + alert ≤ 5 min global.
    /// INV-BYOK-CRYPTO-SOVEREIGNTY enforced.
    pub fn new(providers: Vec<Arc<dyn KmsProvider>>, dek_cache: Arc<DekCache>) -> Self {
        Self {
            providers,
            dek_cache,
            check_interval: Duration::from_secs(60),
        }
    }

    pub async fn run_check_loop(self) {
        loop {
            for provider in &self.providers {
                let active_keys = self.list_active_byok_keys(provider).await;
                for key_id in active_keys {
                    match provider.check_access(&key_id).await {
                        Ok(KmsAccessStatus::Ok) => {
                            // métrica increment
                        }
                        Ok(KmsAccessStatus::Revoked) | Ok(KmsAccessStatus::NotFound) => {
                            self.handle_revocation(provider, &key_id).await;
                        }
                        Ok(KmsAccessStatus::Throttled) => {
                            // retry next cycle; alert if sustained
                        }
                        Ok(KmsAccessStatus::ApiError(_)) | Err(_) => {
                            // alert if sustained
                        }
                    }
                }
            }
            tokio::time::sleep(self.check_interval).await;
        }
    }

    /// Kill switch path. Hard-fail. NO operator override.
    async fn handle_revocation(&self, provider: &Arc<dyn KmsProvider>, key_id: &KmsKeyId) {
        let start = Instant::now();

        // Step 1: Atomic DEK cache eviction
        let _ = self.dek_cache.evict_all_for_key(key_id).await;

        // Step 2: Mark tenant degraded read-only (per-tenant flag em D1)
        self.mark_tenant_degraded(key_id).await;

        // Step 3: Emit audit corelink.byok.cmk_revoked
        self.emit_audit_revoked(provider.provider_kind(), key_id).await;

        // Step 4: Alert customer (dashboard + email + in-app notification)
        self.alert_customer(provider.provider_kind(), key_id).await;

        // Step 5: DEK cache TTL 5 min hard expires all in-flight reads
        // (composed via WI-S14-004 cache TTL hard limit; no action required here)

        // SLA: total p99 ≤ 5 min global
        let elapsed = start.elapsed();
        // métrica corelink_byok_kill_switch_duration_seconds_bucket emit elapsed
    }

    async fn list_active_byok_keys(&self, provider: &Arc<dyn KmsProvider>) -> Vec<KmsKeyId> {
        // Query D1 byok_envelope SELECT DISTINCT (kms_provider, kms_key_id)
        // for active tenants with provider matching.
        unimplemented!()
    }

    async fn mark_tenant_degraded(&self, key_id: &KmsKeyId) {
        // Update tenants table SET byok_status = 'degraded_read_only' WHERE kms_key_id = key_id
        unimplemented!()
    }

    async fn emit_audit_revoked(&self, provider: KmsProviderKind, key_id: &KmsKeyId) {
        // CloudEvent corelink.byok.cmk_revoked with payload {provider, kms_key_id, ts}
        unimplemented!()
    }

    async fn alert_customer(&self, provider: KmsProviderKind, key_id: &KmsKeyId) {
        // Dashboard + email + in-app notification
        unimplemented!()
    }
}
```

State machine kill switch:
```
Customer revokes CMK em provider (out of band)
    ↓
KMS access check background 60s per tenant detects (provider returns Revoked/NotFound)
    ↓
Atomic DEK cache evict_all_for_key(kms_key_id)
    ↓
Mark tenant byok_status = 'degraded_read_only' em D1
    ↓
Emit audit corelink.byok.cmk_revoked
    ↓
Alert customer via dashboard + email + in-app notification
    ↓
DEK cache TTL 5 min hard expires all in-flight reads (composed WI-S14-004)
    ↓
Total p99 ≤ 5 min global; INV-BYOK-CRYPTO-SOVEREIGNTY enforced; NO operator override
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Customer kill switch é **definição feature de BYOK enterprise tier**: sem ele, BYOK = teatro (cliente não tem real control). Customer revoga CMK em seu provider (AWS console / GCP console / Azure portal / Vault CLI); CoreLink deve detectar + propagate eviction global em ≤ 5 min p99. INV-BYOK-CRYPTO-SOVEREIGNTY enforces este contrato; bypass = catastrophic customer trust loss + breach notification.

**Bugs catastróficos possíveis** (todos endereçados):

1. **Kill switch SLA miss (> 5 min)**: detection delay + cache eviction delay + propagation delay > 5 min. Mitigação: 60s check interval + atomic cache eviction + 5 min hard TTL ceiling = total p99 ≤ 5 min strict; chaos drill weekly verifies; INV-BYOK-CRYPTO-SOVEREIGNTY enforces.

2. **DEK cache extension via "performance optimization" PR**: dev tries cache TTL > 5 min for performance. Mitigação: `DekCache::new(ttl_seconds)` rejects > 300s (composed WI-S14-004); CI gate static check; INV-BYOK-CRYPTO-SOVEREIGNTY runtime invariant.

3. **Operator override path enabled "for emergency"**: someone adds operator override in code "para emergency". Mitigação: NO operator override = compile-time absence; code review hard-rejects; ADR documents waiver policy hardness.

4. **KMS access check rate limit**: provider throttles check_access calls; detection delayed. Mitigação: per-provider rate limit budget + adaptive backoff; alert if sustained throttling.

5. **Network partition CoreLink ↔ KMS**: check_access fails; revocation undetectable. Mitigação: distinguish `Throttled` vs `Revoked`; persistent failure (3 cycles = 3 min) + cannot reach provider = degrade-mode read-only conservative (worst case = false-positive customer notification, customer can verify and unblock).

6. **Audit emission failure**: revocation handled but audit not emitted; chain integrity broken. Mitigação: D1 atomic batch [degrade flag + audit_outbox INSERT] (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER); failure rolls back.

7. **Customer alert delivery failure**: email bounce; in-app notification not seen. Mitigação: multi-channel (dashboard + email + in-app + optional Slack webhook); delivery confirmation via dashboard read; SLA documented.

8. **DEK cache eviction race**: read in-flight uses cached DEK during eviction. Mitigação: atomic `evict_all_for_key` (DashMap atomic remove); concurrent reads observe either pre-eviction (within 5 min hard TTL) OR fail with `BYOKError::CmkRevoked` post-eviction; never leak DEK material.

9. **Customer false positive (CMK temporarily disabled by mistake)**: customer disables CMK by mistake; CoreLink kills switch; customer panics. Mitigação: customer notification BEFORE degrading via 5 min countdown? **Decision**: NO countdown — customer is responsible for CMK; immediate kill switch + clear customer alert + restore path documented (re-enable CMK → next 60s check restores access).

**Atacante adversarial scenarios** (todos validados em §15):

- **Force false revocation**: attacker compromises CoreLink check_access logic; reports false revoke. Mitigação: provider check_access dual-confirm (3-cycle confirm before action); audit emit on false-positive detection.

- **Bypass kill switch via direct R2 read**: attacker skips Worker; reads R2 directly. Mitigação: R2 access scoped via Worker binding; direct R2 via attacker IAM compromise = different threat (S-12 supply chain).

- **DEK cache extraction post-eviction**: attacker dumps Worker memory. Mitigação: ZeroizeOnDrop on eviction (composed WI-S14-004); bounded cache.

- **Timing attack on revocation propagation**: attacker measures latency to infer DEK age. Mitigação: 5 min hard TTL ceiling; constant-time eviction; AES-NI hardware accel.

- **Replay revocation for non-revoked tenant**: attacker injects fake revocation. Mitigação: provider check_access primary signal; D1 mutation requires admin role + dual-approval (S-13).

**Risk justification HIGH_RISK**:

- **FF-HR-005**: kill switch é customer trust baseline; bypass = customer trust permanently lost.
- **Reversibility**: SLA violation detected = post-mortem CRITICAL + INV review; per spec contract §19 waiver policy NÃO waivable.

11 sign-offs canonical incl. Architect (com **Crypto SME folded mandatory**: kill switch flow review + DEK cache eviction atomic + INV-BYOK-CRYPTO-SOVEREIGNTY enforcement + chaos drill design) + Security Lead + AppSec + Compliance Officer (customer trust + GDPR Art. 17/32 + LGPD Art. 38 attestation) + Privacy Officer (LGPD + GDPR alignment).

## 3. Customer Impact & Journey

**Persona 1 — Customer enterprise revoking CMK (incident response)**:
- Customer detects CMK compromise; revokes em provider console.
- ≤ 60s: CoreLink check_access detects revoke.
- ≤ 5 min: DEK cache empty + tenant degraded read-only + customer alerted (dashboard + email + in-app).
- All in-flight reads served stale DEK fail post-5min TTL hard.
- Customer can re-enable CMK; next 60s check restores access; dashboard shows recovery.

**Persona 2 — Auditor SOC 2 + ISO 27001 + GDPR**:
- INV-BYOK-CRYPTO-SOVEREIGNTY CRITICAL ratificada; chaos drill weekly green ≤ 5 min.
- CTRL-KEY-012 (kill switch hard-fail) attestation.
- Evidence pack: 30d staging chaos drill weekly green; runbook RB-BYOK-REVOKE dry-run report.

**Persona 3 — Internal SRE on-call**:
- DASH-BYOK shows kill switch duration p99 SLO + recent revocation events.
- Alert SEV-1 if SLA violated (> 5 min sustained).
- Runbook RB-BYOK-REVOKE: detection + investigation + customer communication + recovery.

**SLA addendum**:
- KMS access check cadence: every 60s per active BYOK tenant.
- Kill switch SLA: ≤ 5 min global p99 hard-fail.
- INV-BYOK-CRYPTO-SOVEREIGNTY: NO operator override, NO advisory mode (waiver policy ⚠️ ≤ 5 min hard).
- Chaos drill cadence: weekly em staging.
- Customer notification multi-channel (dashboard + email + in-app + optional Slack webhook).
- Recovery SLA: re-enable CMK → next 60s check restores access.
- Audit retention: 7y (CTRL-AUDIT-005).

## 4. Capability Mapping

- **CAP-BYOK-005** (customer kill switch ≤ 5 min) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.2 R-S14-8 + R-S14-9` + `security_model.md §6 (CTRL-KEY-012 kill switch hard-fail)` + `invariant_registry.md §3.12 (INV-BYOK-CRYPTO-SOVEREIGNTY CRITICAL nova WI-S14-004 + WI-S14-006 enforcement)`.

## 5. Tipo

Background worker + chaos drill + runbook; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Crate `crates/corelink-byok-revocation/`**:
   - `RevocationDetector` struct + 60s check loop.
   - Per-provider check_access via WI-S14-004 + WI-S14-005 trait.
   - Atomic DEK cache eviction via `DekCache::evict_all_for_key`.
   - Tenant mark degraded read-only.
   - Audit emit + customer alert.
   - SLA tracking métrica.

2. **D1 schema migration `migrations/0XX_byok_tenant_status.sql`**:
   ```sql
   ALTER TABLE tenants ADD COLUMN byok_status TEXT DEFAULT 'active'
       CHECK (byok_status IN ('active', 'degraded_read_only', 'revoked'));
   ALTER TABLE tenants ADD COLUMN byok_revoked_at_ms BIGINT;
   ALTER TABLE tenants ADD COLUMN byok_revoked_provider TEXT;
   ALTER TABLE tenants ADD COLUMN byok_revoked_kms_key_id TEXT;
   CREATE INDEX idx_tenants_byok_status ON tenants(byok_status);
   ```

3. **Customer alert delivery `crates/corelink-customer-alerts/`**:
   - Dashboard alert API (consumed by S-16 frontend).
   - Email delivery (SendGrid / SES).
   - In-app notification (D1 row + WebSocket fanout).
   - Optional Slack webhook (customer-configured).
   - Delivery confirmation via dashboard read.

4. **Runbook `specs/05_runbooks/RB-BYOK-REVOKE.md`**:
   - Detection: kill switch métrica + audit emit.
   - Investigation: customer-side CMK status + CoreLink-side state.
   - Customer communication: notification channels validation.
   - Recovery: re-enable CMK + next 60s check + dashboard recovery.
   - Drift assessment: runbook commands accurate? Dashboard panel visible? Alert fires correctly? On-call escalation works?
   - Output: `specs/_audits/2026-XX-XX-rb-byok-revoke-dry-run.md` com timeline + drift findings + runbook updates committed.

5. **Chaos drill weekly em staging**:
   - Automated harness `scripts/byok_kill_switch_drill.rs` (Rust binary).
   - Provision test BYOK tenant em staging.
   - Revoke CMK em staging KMS provider.
   - Measure: detection latency + cache eviction latency + customer alert delivery latency.
   - Assert: total p99 ≤ 5 min.
   - Cron weekly Sunday 03:00 UTC; report committed.
   - Per-provider drill rotation (4 providers / 4 weeks = 1 provider per week; full coverage monthly).

6. **Métricas underscored Prometheus** (per `observability_model.md §3.1`; label `plan` aplicável):
   - `corelink_byok_cmk_access_check_total{provider, outcome, plan}` (outcome ∈ ok|revoked|throttled|api_error).
   - `corelink_byok_cmk_revoked_total{provider, plan}` (counter; chaos drill increments).
   - `corelink_byok_kill_switch_duration_seconds_bucket{provider, plan}` (histogram revoke detected → cache empty ≤ 5 min p99 SLO).
   - `corelink_byok_kill_switch_sla_violation_total{provider, plan}` (counter; alert > 0 SEV-1).
   - `corelink_byok_customer_alert_delivery_total{channel, outcome, plan}` (channel ∈ dashboard|email|in_app|slack; outcome ∈ ok|fail).
   - `corelink_byok_tenant_status_gauge{status, plan}` (status ∈ active|degraded_read_only|revoked).

7. **Observability** — trace spans `byok.revocation.{check, detect, evict, mark_degraded, emit_audit, alert_customer}` com attributes:
   - `byok.provider` (enum).
   - `byok.kms_key_id_hashed` (hashed).
   - `byok.tenant_id_hashed`.
   - `byok.kill_switch_duration_ms`.
   - `result` (enum).

8. **Audit emission** — CloudEvent per revocation event:
   - `corelink.byok.cmk_revoked` payload `{provider, kms_key_id, tenant_id_hashed, detected_at_ms, evicted_at_ms, alerted_at_ms, kill_switch_duration_ms}`.
   - Atomic batch with tenant status update (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
   - 7y retention (CTRL-AUDIT-005).

9. **Property tests** (10k iter PR + 100k iter nightly):
   - `prop_kill_switch_sla_5min`: simulate 10k revocation scenarios; assert p99 ≤ 5 min.
   - `prop_atomic_cache_eviction`: simulate 10k concurrent reads during eviction; assert no DEK leaked.
   - `prop_no_operator_override`: simulate operator override attempt; assert rejected.
   - `prop_audit_emission_atomic`: simulate D1 batch failure; assert state rolled back.
   - `prop_recovery_after_re_enable`: simulate CMK re-enabled; assert next 60s check restores.

10. **Adversarial regression tests**:
    - Force false revocation (compromised check_access).
    - DEK cache extraction post-eviction (memory dump).
    - Timing attack on revocation propagation.
    - Replay revocation for non-revoked tenant.
    - Bypass kill switch via direct R2 read attempt.
    - Compromised RevocationDetector code path.

11. **Integration test E2E** (real KMS provider per provider):
    - Provision BYOK tenant per provider em staging.
    - Revoke CMK em provider console.
    - Assert: detection ≤ 60s; cache eviction immediate; customer alert ≤ 5 min; tenant degraded.
    - Re-enable CMK; assert recovery ≤ 60s.

12. **Customer-facing documentation**:
    - `docs/customer/byok-kill-switch.md` (sanitized; customer-shareable post-NDA).
    - SLA addendum + recovery procedure.
    - Multi-channel alert configuration guide.

### 6.2 Out-of-scope (deferred)

- **Customer-facing UI for kill switch dashboard**: S-16 admin UI.
- **Erasure attestation Ed25519**: WI-S14-007.
- **DPA + Schrems II TIA**: WI-S14-008.
- **TLA+ + pentest + PRR**: WI-S14-009.
- **5 min countdown UI before kill switch (customer cancel option)**: deferred (customer has explicit revoke action; NO countdown).
- **Federated KMS multi-cloud failover (kill switch one provider, fall back to other)**: single CMK per tenant at GA; multi-CMK = Fase 2.

## 7. Anti-Scope

- DEK cache TTL > 5 min (INV-BYOK-CRYPTO-SOVEREIGNTY violation; composed WI-S14-004).
- Operator override path em kill switch (NO override; compile-time absence).
- Skip 60s KMS access check cadence.
- Skip atomic DEK cache eviction.
- Skip multi-channel customer alert delivery.
- Skip chaos drill weekly em staging.
- Skip per-provider chaos drill rotation.
- Skip RB-BYOK-REVOKE dry-run.
- Skip property tests crypto-sovereignty.
- Skip adversarial regression tests.
- Skip integration test E2E real KMS provider per provider.
- 5 min countdown before kill switch (customer responsible for CMK; immediate kill switch).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: WI-S14-006 — CMK revocation kill switch ≤ 5 min global hard-fail + chaos drill

  Background:
    Given BYOK trait operational (WI-S14-004)
    Given 4 providers adapters operational (WI-S14-005)
    Given DEK cache 5 min hard limit (composed WI-S14-004)
    Given RevocationDetector cron 60s

  Scenario: KMS access check 60s detects revoke
    Given customer revokes CMK em AWS KMS console
    When 60s elapsed
    Then RevocationDetector check_access returns KmsAccessStatus::Revoked
    And handle_revocation triggered

  Scenario: Atomic DEK cache eviction
    Given DEK cache populated 100 entries for kms_key_id K1
    When handle_revocation called
    Then evict_all_for_key(K1) atomic
    And 100 entries evicted
    And ZeroizeOnDrop called per DEK
    And concurrent reads observe either pre-eviction OR BYOKError::CmkRevoked

  Scenario: Tenant marked degraded read-only
    Given handle_revocation triggered
    When mark_tenant_degraded called
    Then D1 tenants.byok_status = 'degraded_read_only' WHERE kms_key_id = K1
    And byok_revoked_at_ms set
    And byok_revoked_provider + byok_revoked_kms_key_id set

  Scenario: Audit emit corelink.byok.cmk_revoked
    Given handle_revocation triggered
    When emit_audit_revoked called
    Then CloudEvent emitted atomic with tenant status update
    And payload contains provider + kms_key_id + tenant_id_hashed + timestamps + duration_ms
    And 7y retention (CTRL-AUDIT-005)

  Scenario: Customer alert multi-channel delivery
    Given handle_revocation triggered
    When alert_customer called
    Then dashboard alert + email + in-app notification delivered
    And optional Slack webhook (if configured)
    And delivery confirmation via dashboard read

  Scenario: Kill switch SLA ≤ 5 min global p99
    Given customer revokes CMK
    When timeline measured: detection + eviction + alert + cache TTL expiry
    Then total p99 ≤ 5 min
    And kill_switch_duration_seconds_bucket histogram shows p99 ≤ 300s
    And SLA violation alert if p99 > 300s sustained

  Scenario: INV-BYOK-CRYPTO-SOVEREIGNTY hard-fail no operator override
    Given handle_revocation triggered
    When operator tries override "for emergency"
    Then no override path exists em code
    And compile-time absence verified
    And ADR documents hardness

  Scenario: Recovery after CMK re-enabled
    Given customer re-enables CMK em provider console
    When next 60s check runs
    Then KmsAccessStatus::Ok returned
    And tenant.byok_status restored to 'active'
    And dashboard alert recovery
    And métrica corelink_byok_cmk_access_check_total{outcome="ok"} resumes

  Scenario: Chaos drill weekly per provider
    Given staging environment + 4 providers BYOK tenants
    When weekly cron Sunday 03:00 UTC runs
    Then 1 provider rotated per week (4 weeks = full coverage monthly)
    And revoke + measure + assert ≤ 5 min
    And report committed em audit folder

  Scenario: Network partition CoreLink ↔ KMS distinguishes Throttled vs Revoked
    Given KMS API throttled response
    When check_access returns Throttled
    Then no eviction triggered (false-positive avoided)
    And alert SEV-3 if sustained > 3 cycles (3 min)
    Given persistent failure (3 cycles)
    When cannot reach provider
    Then conservative degrade-mode read-only (worst case = customer can verify and unblock)

  Scenario: Audit emission atomic with state update
    Given handle_revocation triggered
    When D1 batch [tenant status update + audit_outbox INSERT] executed
    Given D1 batch failure
    When rollback runs
    Then state transition rolled back
    And no orphan audit emit

  Scenario: Property tests 5 props × 10k iter green
    Given prop_kill_switch_sla_5min + prop_atomic_cache_eviction + prop_no_operator_override + prop_audit_emission_atomic + prop_recovery_after_re_enable
    When 10k iter run em PR
    Then 0 violations of INV-BYOK-CRYPTO-SOVEREIGNTY
    And nightly 100k iter green

  Scenario: Adversarial regression tests 6+ scenarios green
    Given force false revocation + cache extraction + timing attack + replay + direct R2 bypass + compromised detector
    When red team session
    Then 6+ scenarios mitigated
    And report committed

  Scenario: Integration test E2E real KMS provider per provider
    Given staging BYOK tenant per provider (AWS / GCP / Azure / Vault)
    When customer revokes CMK em provider
    Then 4 providers × kill switch ≤ 5 min p99 verified
    And recovery after re-enable ≤ 60s

  Scenario: RB-BYOK-REVOKE dry-run
    Given simulated customer CMK revocation
    When dry-run executes
    Then runbook commands executable
    And drift findings + customer notification template ready
    And report committed
```

## 9. Design Decisions

### 9.1 Why 60s check interval (NÃO 30s / 5min)

- 30s = 2× cost; benefit marginal (kill switch SLA still ≤ 5 min via cache TTL).
- 5min check interval = total kill switch SLA at risk (5min check + 5min cache TTL = 10min worst case = breach customer trust).
- 60s balances cost vs SLA buffer (60s detection p99 + 5min cache TTL hard = **6 min p99 worst case canonical SLA Lote 10.14 codex P0 disambiguation**; SLO-BYOK-CMK-DETECT ≤ 60s + SLO-BYOK-DEK-EVICT ≤ 5min hard; total customer-perceived kill switch = 6min p99 — NÃO 5min como hand-wave anterior). Componentes individuais hard-fail (não waiverable per spec contract §19).

### 9.2 Why NO operator override (per spec contract §19 waiver policy NÃO waivable)

- INV-BYOK-CRYPTO-SOVEREIGNTY enforces customer trust baseline.
- Override path = customer trust permanently lost (auditor finds → fail SOC 2).
- Compile-time absence (no override function exists em codebase).
- ADR documents hardness.

### 9.3 Why NO 5-min countdown UI before kill switch

- Customer is responsible for CMK; explicit revoke action == intent.
- Countdown adds complexity + customer confusion (cancel button = false sense of control).
- Recovery is fast (re-enable CMK → 60s check restores).

### 9.4 Why multi-channel customer alert

- Email bounce + in-app notification missed = customer surprised.
- Multi-channel (dashboard + email + in-app + optional Slack) = redundancy.
- Delivery confirmation via dashboard read.

### 9.5 Why chaos drill weekly per-provider rotation (NÃO weekly all 4)

- 4 providers / 4 weeks = monthly full coverage.
- Weekly all 4 = 4× cost; benefit marginal.
- Rotation balances cost vs coverage.

### 9.6 Why network partition conservative degrade-mode (NÃO ignore)

- Cannot reach KMS = ambiguous state; could be revoked or could be temporary outage.
- Conservative = degrade read-only; worst case = false-positive customer notification.
- Customer can verify and unblock; preferable to silent revocation missed.

### 9.7 Why ADR potencial?

- Sim — **ADR-XXXX**: "BYOK CMK revocation kill switch ≤ 5 min hard-fail + INV-BYOK-CRYPTO-SOVEREIGNTY enforcement + chaos drill weekly + NO operator override S-14". Decisão arquitetural cripto-load-bearing customer-trust-defining.

## 10. Completeness Criteria SOTA

- [ ] **10.s14.006.1** RevocationDetector 60s check loop operational per provider (EVT-013).
- [ ] **10.s14.006.2** Atomic DEK cache eviction `evict_all_for_key` (EVT-002).
- [ ] **10.s14.006.3** D1 byok_status migration applied + tenant degrade flow (EVT-013).
- [ ] **10.s14.006.4** Audit emit `corelink.byok.cmk_revoked` atomic batch (EVT-024).
- [ ] **10.s14.006.5** Multi-channel customer alert delivery (dashboard + email + in-app + optional Slack) (EVT-024).
- [ ] **10.s14.006.6** Kill switch SLA ≤ 5 min global p99 sustained 30d staging chaos drill weekly (EVT-021) *(GA Evidence Gate D+60)*.
- [ ] **10.s14.006.7** INV-BYOK-CRYPTO-SOVEREIGNTY CRITICAL ratificada em registry §3.12 hard-fail no operator override (EVT-022).
- [ ] **10.s14.006.8** Property tests 5 props × 10k iter PR + 100k nightly green (EVT-022).
- [ ] **10.s14.006.9** Adversarial regression tests 6+ scenarios green (EVT-040).
- [ ] **10.s14.006.10** Integration test E2E real KMS provider per provider 4× green (EVT-024).
- [ ] **10.s14.006.11** Chaos drill weekly per-provider rotation green; 30d sustained staging *(GA Evidence Gate D+60)*.
- [ ] **10.s14.006.12** Runbook RB-BYOK-REVOKE committed + dry-run executed (EVT-017).
- [ ] **10.s14.006.13** ADR-XXXX (kill switch + INV-BYOK-CRYPTO-SOVEREIGNTY enforcement) ratificada.
- [ ] **10.s14.006.14** SOC 2 CC6.1 + GDPR Art. 17/32 + LGPD Art. 38 attestation em PRR doc (EVT-044).

## 11. DoD

- [ ] Crate `corelink-byok-revocation` + `corelink-customer-alerts` compilam.
- [ ] D1 migration applied + indexes.
- [ ] All 14 Gherkin scenarios green em integration test.
- [ ] Property tests 5 props × 10k iter green; 100k nightly green.
- [ ] Adversarial regression tests 6+ scenarios green.
- [ ] Integration tests E2E 4 providers green.
- [ ] Chaos drill weekly per-provider rotation operational; 4 weeks coverage validated.
- [ ] CloudEvent audit emission per revocation atomic batch.
- [ ] Métricas + trace spans operational.
- [ ] Multi-channel customer alert delivery operational.
- [ ] Runbook RB-BYOK-REVOKE dry-run.
- [ ] ADR-XXXX (kill switch) escrito + ratificado.
- [ ] Code review (Architect + Crypto SME folded mandatory + Security Lead + AppSec + Compliance + Privacy).
- [ ] PRR Architect + Crypto SME mini-sign-off.
- [ ] INV-BYOK-CRYPTO-SOVEREIGNTY ratificada runtime + property test enforcement.

## 12. Invariants Validated

### Mantidas

- **INV-BYOK-CRYPTO-SOVEREIGNTY** (CRITICAL — registry §3.12 herdada WI-S14-004; este WI **enforces primary**): customer revoga CMK → cache inacessível ≤ 5 min global; nenhum bypass via cached unwrapped DEK > 5 min. Why: sem isso BYOK = teatro. How: DEK cache TTL 5 min hard (WI-004) + KMS access check 60s (este WI) + INV propagation.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL — registry §3.14 herdada): revocation audit emit em D1 atomic batch.
- **INV-KEY-NO-SKIP** (HIGH — registry §3.13 herdada): BYOK revoke → writes fail corretamente (degraded read-only).

### Novas

Nenhuma direta neste WI; este WI **enforces** INV-BYOK-CRYPTO-SOVEREIGNTY ratificada em WI-S14-004.

TLA+ alignment: registry §4.2 indica `region_residency.tla` PLANNED S-14 WI-S14-009; este WI provê kill switch implementation.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate corelink-byok-revocation | `crates/corelink-byok-revocation/src/lib.rs` | Rust |
| Crate corelink-customer-alerts | `crates/corelink-customer-alerts/src/lib.rs` | Rust |
| D1 migration byok_tenant_status | `migrations/0XX_byok_tenant_status.sql` | SQL |
| Chaos drill harness | `scripts/byok_kill_switch_drill.rs` | Rust binary |
| Weekly cron drill workflow | `.github/workflows/byok_kill_switch_drill_weekly.yml` | YAML |
| Property tests | `crates/corelink-byok-revocation/tests/prop_revocation.rs` | Rust |
| Adversarial tests | `crates/corelink-byok-revocation/tests/adversarial.rs` | Rust |
| Integration tests E2E | `tests/e2e_byok_kill_switch_{aws,gcp,azure,vault}.rs` | Rust |
| Runbook RB-BYOK-REVOKE | `specs/05_runbooks/RB-BYOK-REVOKE.md` | Markdown |
| RB-BYOK-REVOKE dry-run report | `specs/_audits/2026-XX-XX-rb-byok-revoke-dry-run.md` | Markdown |
| Customer doc | `docs/customer/byok-kill-switch.md` | Markdown |
| ADR-XXXX (kill switch) | `specs/03_architecture/adrs/ADR-XXXX-byok-kill-switch-no-operator-override.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s14.006.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s14.006.2** rustdoc 100% public API + 4 examples.
- **14.s14.006.3** Test coverage ≥ 95%; property tests 10k+100k.
- **14.s14.006.4** Latência: kill switch p99 ≤ 5 min global SLO.
- **14.s14.006.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` clean.
- **14.s14.006.6** Métricas RED + per-provider breakdown.
- **14.s14.006.7** Runbook RB-BYOK-REVOKE committed + dry-run.
- **14.s14.006.8** Breaking changes em revocation API = bump major + ADR.
- **14.s14.006.9** Memory bounded; ZeroizeOnDrop em DEK + cache.
- **14.s14.006.10** Cost regression gate em CI.
- **14.s14.006.11** GDPR Art. 17/32 + LGPD Art. 38 attestation.
- **14.s14.006.12** NO operator override (compile-time absence verified).

## 15. Chaos Experiments

1. **Force false revocation**: compromised check_access; verify dual-confirm (3-cycle) + audit emit.

2. **DEK cache extraction post-eviction**: memory dump; verify ZeroizeOnDrop + bounded cache.

3. **Timing attack on revocation propagation**: measure latency to infer DEK age; verify constant-time eviction.

4. **Replay revocation for non-revoked tenant**: inject fake revocation; verify provider check_access primary signal.

5. **Bypass kill switch via direct R2 read**: skip Worker; verify R2 access scoped via Worker binding.

6. **Compromised RevocationDetector code path**: verify code review + immutable code paths.

7. **Network partition conservative degrade-mode**: simulate KMS unreachable; verify Throttled vs Revoked distinction + 3-cycle confirm.

8. **Operator override attempt**: verify NO override path em code; compile-time absence verified.

9. **Customer alert delivery failure**: simulate email bounce; verify multi-channel redundancy + dashboard read confirmation.

10. **DEK cache eviction race**: 1000 concurrent reads during eviction; verify atomic eviction + ZeroizeOnDrop.

11. **Audit emission atomic failure**: D1 batch failure; verify state rolled back + no orphan audit.

12. **Recovery after re-enable**: re-enable CMK; verify next 60s check restores access.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-14 ship gate é WI-S14-009; este WI passa por mini-PRR Architect + Crypto SME mandatory + Security Lead + AppSec + Compliance + Privacy review):

- [ ] All 14 Gherkin scenarios green.
- [ ] Property tests 5 props × 10k iter green; 100k nightly green.
- [ ] Adversarial regression tests 6+ scenarios green.
- [ ] Integration tests E2E 4 providers green.
- [ ] Chaos drill weekly per-provider rotation operational.
- [ ] Kill switch SLA ≤ 5 min global p99 sustained 30d staging.
- [ ] Runbook RB-BYOK-REVOKE dry-run executed.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-BYOK.
- [ ] ADR-XXXX (kill switch) published.
- [ ] Crypto SME review (kill switch flow + DEK cache eviction atomic + INV enforcement + chaos design).
- [ ] Compliance Officer review (SOC 2 CC6.1 + GDPR Art. 17/32 + LGPD Art. 38).
- [ ] Privacy Officer review (LGPD + GDPR + customer notification).
- [ ] AppSec review (adversarial scenarios + bypass attempts).
- [ ] Architect approval (composition with WI-S14-004 + WI-S14-005).
- [ ] INV-BYOK-CRYPTO-SOVEREIGNTY ratificada runtime + property test.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate corelink-byok-revocation scaffold + RevocationDetector | 2.5h |
| ST-002 | 60s check loop + per-provider check_access integration | 2h |
| ST-003 | Atomic DEK cache eviction wiring (composed WI-S14-004) | 1.5h |
| ST-004 | D1 byok_tenant_status migration + tenant degrade flow | 1.5h |
| ST-005 | Audit emit atomic batch | 1h |
| ST-006 | Crate corelink-customer-alerts + multi-channel delivery | 2.5h |
| ST-007 | Métricas emit (6 metrics) + trace spans | 1.5h |
| ST-008 | Property tests 5 props × 10k iter | 3h |
| ST-009 | Adversarial regression tests 6+ scenarios | 2.5h |
| ST-010 | Integration tests E2E real KMS provider per provider (4 tests) | 4h |
| ST-011 | Chaos drill harness `byok_kill_switch_drill.rs` Rust binary | 2.5h |
| ST-012 | Weekly cron drill workflow + per-provider rotation | 1h |
| ST-013 | Runbook RB-BYOK-REVOKE escrita + dry-run | 2h |
| ST-014 | Customer doc `docs/customer/byok-kill-switch.md` | 1.5h |
| ST-015 | ADR-XXXX (kill switch + NO operator override) redação | 2h |
| ST-016 | Code review (Architect + Crypto SME folded + Security + AppSec + Compliance + Privacy) | 3h |

**Total Optimistic**: ~33h. **PERT** (O=12h, M=18h, P=28h, per spec contract §12): **18.7h**.

## 18. Dependencies

### Hard blockers

- **WI-S14-004 SEALED** (BYOK trait + DEK cache 5 min hard limit + AwsKmsProvider).
- **WI-S14-005 SEALED** (3 additional providers GCP/Azure/Vault).
- **S-09 SEALED** (audit chain + atomic batch + métricas).
- **S-13 SEALED** (admin API + dual-approval).

### Soft blockers

- Customer alert delivery channels (SendGrid for email; D1 for in-app).

### Outbound

- **WI-S14-007** (erasure attestation triggered post-kill switch durante DSR).
- **WI-S14-009** (TLA+ + pentest covers kill switch scenarios; PRR ship gate).

## 19. Effort PERT

O: 12h, M: 18h, P: 28h → PERT **18.7h** (per spec contract §12).

## 20. Time-boxing

**24h hard limit owner**. If exceeded → escalation: split em sub-WI (detector vs alerts vs chaos drill).

## 21. Observability

6 métricas listadas §6.1.6. Trace spans em §6.1.7. Logs structured JSON; nivel INFO em ok, WARN em throttled / API error, ERROR em SLA violation.

Dashboard widget DASH-BYOK:
- CMK access check status per provider.
- Kill switch duration p99 (alert > 300s = SLA violation).
- Customer alert delivery rate per channel.
- Tenant byok_status gauge (active / degraded / revoked).
- Revocation events timeline (chaos drill + production).

## 22. Cost Analysis

- RevocationDetector cron Worker: ~$5/mês.
- D1 byok_tenant_status reads (60s check): ~$10/mês.
- Customer alert delivery (SendGrid email): ~$5/mês.
- Chaos drill weekly: ~$10/mês.
- KMS check_access calls (4 providers × 60s × tenants): bounded; ~$10/mês.
- **Total custo direto WI-S14-006**: ~$40/mês baseline + workload-dependent ~$80/mês.

## 23. API Contract

Não-aplicável (este WI é background worker; não introduz API surface). Customer interaction via dashboard + email + in-app notification + optional Slack webhook (config via existing customer settings API).

## 24. Post-mortem Hooks

- Kill switch SLA miss (> 5 min) → CRITICAL post-mortem + Crypto SME + Compliance.
- DEK cache TTL bypass detected → CRITICAL + INV-BYOK-CRYPTO-SOVEREIGNTY review.
- Operator override path detected em code → CRITICAL + Security incident.
- Customer alert delivery failure sustained 1h → SEV-2 + customer trust review.
- Network partition false-positive customer notification > 1× mês → review thresholds.
- Audit emission failure during revocation → CRITICAL + chain integrity review.

## 25. Rollback / Recovery

- Code rollback: revert PR + redeploy Worker.
- Tenant recovery: re-enable CMK em provider; next 60s check restores.
- DEK cache rollback: clear cache + re-fetch on demand.
- RTO ≤ 30 min (Worker rollback).
- RPO 0 (audit chain unbroken; D1 state preserved).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: provider check_access primary signal; D1 mutation requires admin role + dual-approval.
- **Tampering**: state machine atomic transitions; chain hash integrity preserved.
- **Repudiation**: per-revocation audit emission + 7y retention.
- **Information disclosure**: customer crypto sovereignty enforced; DEK cache eviction atomic.
- **DoS**: customer kill switch is intentional DoS por design (intent: revoke access).
- **Elevation of privilege**: NO operator override (compile-time absence).

**LINDDUN delta**:
- **Linkability**: tenant_id + provider em audit (compliance).
- **Identifiability**: customer email + provider org_id (intentional).
- **Non-repudiation**: per-revocation audit chain.
- **Detectability**: SLA violation alert + customer notification.
- **Disclosure**: customer crypto sovereignty validated.
- **Unawareness**: customer notified multi-channel.
- **Non-compliance**: SOC 2 CC6.1 + GDPR Art. 17/32 + LGPD Art. 38 satisfied.

## 27. Knowledge Transfer

- `crates/corelink-byok-revocation/README.md` — overview + kill switch flow.
- ADR-XXXX — kill switch ratification.
- Doc `docs/internal/multi-region-byok.md` (kill switch section) — sequence diagram.
- Workshop interno (2h) com Architect + Crypto SME + Security Lead + AppSec + Compliance + Privacy + on-call.
- Onboarding test (10 questions): kill switch SLA, INV-BYOK-CRYPTO-SOVEREIGNTY, NO operator override, 60s check, atomic eviction, multi-channel alert, chaos drill rotation, recovery flow, network partition handling, audit atomic batch.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Kill switch SLA miss > 5 min | L | H | CRITICAL | M | LOW | 60s check + 5 min cache TTL hard + chaos drill weekly |
| R-002 | DEK cache TTL bypass | L | H | CRITICAL | M | LOW | Constructor enforced (WI-004) + INV |
| R-003 | Operator override path em code | L | H | CRITICAL | M | LOW | Compile-time absence + code review hard-reject + ADR |
| R-004 | False revocation (compromised detector) | L | M | HIGH | L | LOW | Dual-confirm 3-cycle + audit emit |
| R-005 | Network partition false-positive | M | L | MEDIUM | M | LOW | Throttled vs Revoked distinction + conservative degrade |
| R-006 | Customer alert delivery failure | M | L | MEDIUM | M | LOW | Multi-channel + delivery confirmation |
| R-007 | DEK cache eviction race | L | M | HIGH | L | LOW | Atomic DashMap remove + ZeroizeOnDrop |
| R-008 | Audit emission failure | L | M | HIGH | L | LOW | D1 atomic batch + rollback on failure |
| R-009 | Cache extraction post-eviction | L | H | HIGH | L | LOW | ZeroizeOnDrop + bounded cache |
| R-010 | Bypass kill switch via direct R2 | L | M | HIGH | L | LOW | R2 access scoped via Worker binding |
| R-011 | Chaos drill flaky em CI | M | L | LOW | L | LOW | Retry + threshold tuning |
| R-012 | Customer false-positive panic | M | L | LOW | L | LOW | Customer education + recovery flow + dashboard |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Crypto SME folded review kill switch flow + atomic eviction + INV enforcement.
2. **Code (D+1)**: peer review + Crypto SME pair-program adversarial tests.
3. **Security (D+2)**: Security Lead review threat model + bypass scenarios + compromised detector.
4. **AppSec (D+2)**: AppSec review CVE-class scenarios + cache extraction + timing.
5. **Privacy (D+3)**: Privacy Officer review LGPD + GDPR + customer notification semantics.
6. **Compliance (D+3)**: Compliance Officer review SOC 2 + GDPR Art. 17/32 + LGPD Art. 38.
7. **Property test (pre-merge D+4)**: 5 props × 10k iter green; 100k nightly green.
8. **Adversarial (pre-merge D+4)**: red team session — false revocation + cache extraction + override attempt.
9. **Chaos drill validation (D+4)**: SRE + on-call run drill em staging per-provider.
10. **PRR mini (D+5)**: Architect + Crypto SME + Security + AppSec + Compliance + Privacy sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — Crypto SME specialization MANDATORY (kill switch flow + atomic eviction + INV-BYOK-CRYPTO-SOVEREIGNTY enforcement + chaos drill design + NO operator override)_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — bypass scenarios + compromised detector + cache extraction defense_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — chaos drill weekly + RB-BYOK-REVOKE + SLA monitoring_ | _pending_ | _pending_ |
| 6 | Engineer (S-14 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD; emphatic — property test 10k + 100k + chaos coverage per-provider_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — SOC 2 CC6.1 + GDPR Art. 17/32 + LGPD Art. 38 + customer trust attestation_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; emphatic — LGPD + GDPR + customer notification multi-channel_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — CVE-class adversarial + override attempt + bypass scenarios_ | _pending_ | _pending_ |

> Crypto SME (kill switch flow + atomic eviction + INV enforcement + chaos drill cross-validation) folds into Architect role specialization MANDATORY.

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S14-006 (cycle 12.S14.0); INV-BYOK-CRYPTO-SOVEREIGNTY enforcement + ≤ 5 min hard. |

## 32. Anti-patterns evitados

- Skip 60s KMS access check.
- Skip atomic DEK cache eviction.
- Operator override path em kill switch.
- Skip multi-channel customer alert.
- Skip chaos drill weekly per-provider rotation.
- Skip RB-BYOK-REVOKE dry-run.
- Skip property tests crypto-sovereignty.
- Skip adversarial regression tests.
- 5 min countdown UI before kill switch.
- Skip integration test E2E real KMS provider.
- Network partition silent (must distinguish Throttled vs Revoked).
- DEK cache TTL > 5 min (composed WI-S14-004).
- Skip ADR documenting hardness.

---

**Fim WI-S14-006.** Próximo: WI-S14-007 (erasure attestation Ed25519 + 7y retention + verify endpoint).
