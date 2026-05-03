---
id: "WI-S10-005"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.4.0"
created: "2026-04-26"
updated: "2026-05-03"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-009"]
parent: "S-10"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DATA-MODEL"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
tags: ["wi", "s10", "quota", "billing", "state-machine", "overage", "email-notification", "hard-block-429", "high-risk"]
---

# WI-S10-005 — Quota State Machine + Overage Handling + Email/In-App Notification (`crates/corelink-billing-quota/`; canonical 4-state machine `under_80 → soft_alert (80%) → ticket (95%) → hard_block (100%)` per sprint contract §5.5 R-S10-10 + §6 DoD; transitions atomic Neon Postgres `quota_state(tenant_id, current_state, last_transition_at, last_pct)`; **hard-block returns 429 com `X-RateLimit-Layer: quota` header** alignment com S-08 rate limit canonical; enterprise tier 7d grace period configurable per contract sprint contract §5.5 R-S10-11; soft alert email customer + in-app notification via Notifications service; ticket creation via support tooling integration (TBD service); transition events emitted to S-09 audit chain CloudEvents v1.0 `dev.hugr.corelink.quota.state.transitioned.v1` audit-grade trace; idempotent transitions [same percent → state, no re-fire notification]; **enterprise grace abuse detection** [3× grace events em 90d → manual review + contract amendment per sprint contract §15 R-009]; integration com WI-S10-002 counter aggregator hourly polling tenant quota status; FAIL-CLOSED transitions canonical Lote 10.6bis split-tier — quota state integrity > availability; typed `QuotaTransition` enum NOT serde_json::Value Lote 10.9-quinquies NEW-P0-2; PII customer email FIPS 140-3 encrypted at rest BYOK S-04 inheritance; LINDDUN compliance via PRIVACY-MODEL CTRL-PRIV-002; rate limit alignment com S-08 R-S08-7 X-RateLimit-Layer header standard)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-10](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S10-005 |
| Título | Quota state machine canonical 4-state (sprint contract §5.5 R-S10-10): `under_80 → soft_alert (80%) → ticket (95%) → hard_block (100%)`; transitions atomic Neon Postgres `quota_state(tenant_id, current_state, last_transition_at, last_pct, alert_email_sent, ticket_id, hard_block_started_at)`; hard-block returns **429 com `X-RateLimit-Layer: quota` header** S-08 alignment (sprint contract §5.5 R-S10-10 + S-08 R-S08-7 inheritance); soft_alert email customer via Notifications service + in-app notification em S-13 admin plane integration; ticket creation via support tooling integration; **enterprise tier 7d grace period** configurable per contract (sprint contract §5.5 R-S10-11; CHECK constraint enforces ≤14d max via Legal review per §15 R-009); transition events emitted to S-09 audit CloudEvents v1.0 `dev.hugr.corelink.quota.state.transitioned.v1` (S-09 inheritance from WI-S09-004); idempotent transitions (same pct = same state; no re-fire notification spam); **enterprise grace abuse detection** 3× grace events em 90d → manual review + contract amendment (sprint contract §15 R-009 mitigation); WI-S10-002 counter aggregator cooperation (hourly poll tenant usage_counter aggregates → compute pct vs plan limit → trigger transition); FAIL-CLOSED at quota state transition canonical Lote 10.6bis split-tier; typed QuotaTransition enum NOT serde_json::Value (Lote 10.9-quinquies NEW-P0-2); PII customer email FIPS 140-3 encrypted at rest com BYOK S-04 inheritance + CTRL-PRIV-002; corelink_time::next_month_first_utc_midnight() reset boundary (Lote 10.8bis P0-D); 12 sign-offs HIGH_RISK (Finance + Legal + Privacy emphatic) |
| Sprint | S-10 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-BILLING-001 financial integrity quota enforcement; bypass = uncontrolled customer cost), FF-HR-009 (hard-block = customer service degradation; legal exposure if wrong tier hit) |

## 1. Intent

Quota state machine é **the customer cost-protection layer + service-protection layer** entre raw counter aggregates (WI-S10-002) e CAS hot path. Sem quota enforcement: customer com runaway script (legitimate ou malicious) pode acumular cost ilimitado ($10k+ surprise bill = customer trust loss + legal dispute); CoreLink infrastructure pode ser DDOS-attacked via legitimate-looking high volume. State machine canonical **4 estados graduais**: `under_80` (silent normal operation) → `soft_alert (80%)` (email customer + in-app warning) → `ticket (95%)` (support ticket created + customer escalation) → `hard_block (100%)` (CAS write returns 429; service throttled). Enterprise tier tem 7d grace period configurable (negotiable em contract).

```rust
// File: crates/corelink-billing-quota/src/state_machine.rs

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc, Duration};

#[async_trait]
pub trait QuotaStateMachine: Send + Sync {
    /// Compute current quota state for tenant; called hourly by WI-S10-002 cooperation.
    /// FAIL-CLOSED: any error halts; SEV-2 alert.
    /// Atomic transitions via Neon Postgres single-row UPDATE com optimistic locking.
    async fn evaluate_quota_state(
        &self,
        tenant_id: TenantId,                            // S-03 TenantCtx
        billing_period: BillingPeriod,                  // YYYY-MM canonical
    ) -> Result<QuotaEvaluation, QuotaError>;

    /// Transition state atomically (under_80 → soft_alert → ticket → hard_block).
    /// Idempotent: same target state = no-op; no re-fire notification.
    async fn transition_state(
        &self,
        tenant_id: TenantId,
        from: QuotaState,
        to: QuotaState,
        trigger_pct: PercentValue,                      // 0-200 typed (DB CHECK 0..=200; evaluator satura a 100 antes de transitions — R4 NEW-P1-1)
    ) -> Result<TransitionOutcome, QuotaError>;

    /// Check if request should be hard-blocked (called from CAS hot path).
    /// FAIL-OPEN at hot path: query failure → permit (audit signal SEV-2).
    /// Distinct from state transitions (fail-CLOSED).
    async fn is_hard_blocked(&self, tenant_id: TenantId) -> Result<HardBlockDecision, QuotaError>;

    /// Reset quota state at billing period boundary (corelink_time::next_month_first_utc_midnight()).
    /// Idempotent: cron invocation safe.
    async fn reset_period_boundary(
        &self,
        billing_period: BillingPeriod,                  // current month canonical
    ) -> Result<ResetReport, QuotaError>;

    /// Apply enterprise grace period (Legal-approved per contract).
    /// Audit event mandatory; abuse detection 3× em 90d triggers manual review.
    async fn apply_grace_period(
        &self,
        tenant_id: TenantId,
        grace_days: GraceDays,                          // CHECK ≤ 14 (sprint contract §19 waiver max)
        approver: ApproverId,                           // Legal/Finance signed off
        reason: GraceReason,                            // typed enum NOT String
    ) -> Result<GraceOutcome, QuotaError>;
}

// Canonical 4-state machine per sprint contract §5.5 R-S10-10 (R4 P1-7 fix: era 5 variants; alinhado com 4-state via `grace_active: bool` flag).
// Grace é representado como flag boolean em quota_state, não como 5º estado — preserva machine canonical.
#[derive(strum::Display, strum::EnumIter, serde::Serialize, serde::Deserialize)]
pub enum QuotaState {
    #[strum(serialize = "under_80")]
    Under80,                                            // 0-79.99% silent operation
    #[strum(serialize = "soft_alert")]
    SoftAlert,                                          // 80-94.99% email + in-app
    #[strum(serialize = "ticket")]
    Ticket,                                             // 95-99.99% support escalation
    #[strum(serialize = "hard_block")]
    HardBlock,                                          // 100%+ CAS write 429 (a menos que grace_active=true em quota_state row)
}

#[derive(serde::Serialize)]
pub struct QuotaEvaluation {
    pub tenant_id: TenantId,
    pub billing_period: BillingPeriod,
    pub current_state: QuotaState,
    pub current_pct: PercentValue,                      // 0-200 typed; DB CHECK 0..=200 (R5 P1-H fix: align type vs DB; permite > 100% durante grace; evaluator satura antes de invocar transitions)
    pub plan_tier: PlanTier,                            // 5 canonical per data_model.md §1 line 68
    pub grace_active: bool,                             // enterprise tier
    pub grace_days_remaining: Option<u8>,
    pub last_transition_at: DateTime<Utc>,
    pub last_alert_email_at: Option<DateTime<Utc>>,     // dedup spam
    pub ticket_id: Option<String>,
    pub hard_block_started_at: Option<DateTime<Utc>>,
}

#[derive(serde::Serialize)]
pub enum TransitionOutcome {
    Transitioned { from: QuotaState, to: QuotaState },
    Idempotent { current: QuotaState },                 // already em target state
    Blocked { reason: TransitionBlockReason },          // e.g., grace active prevents hard_block
}

#[derive(serde::Serialize)]
pub enum HardBlockDecision {
    Allowed,                                            // under hard_block
    Blocked429 { layer: RateLimitLayer, retry_after: Duration },  // S-08 alignment
    GraceAllowed { grace_days_remaining: u8 },          // enterprise tier
}

#[derive(serde::Serialize)]
pub enum GraceReason {
    EnterpriseContractTerm,                             // standard 7d
    LegalReviewedExtension,                             // negotiated contract
    StrategicCustomer,                                  // ADR + Legal sign-off
    ProductionIncidentMitigation,                       // service-attribution; Finance reviewed
}

#[derive(serde::Serialize)]
pub enum TransitionBlockReason {
    GraceActive,                                        // enterprise grace prevents hard_block
    AlreadyAtTargetState,
    CalibrationBoundBreach,                             // statistical signal Lote 10.8bis P0-E
}

#[derive(thiserror::Error, Debug)]
pub enum QuotaError {
    #[error("Neon Postgres transaction failed (fail-CLOSED transition; SEV-2): {0}")]
    PostgresFailed(String),

    #[error("Optimistic locking failure (concurrent transition; retry after backoff): {0}")]
    OptimisticLockFailed(String),

    #[error("Notification delivery failed (email/in-app; SEV-3): {0}")]
    NotificationFailed(String),

    #[error("Ticket creation failed (support tooling; SEV-3): {0}")]
    TicketCreationFailed(String),

    #[error("Grace period violation (>14d max per sprint contract §19 waiver): requested={requested_days}")]
    GraceExceedsLimit { requested_days: u32 },

    #[error("Grace abuse detected (3× em 90d): tenant={tenant_id}; manual review required")]
    GraceAbuseDetected { tenant_id: String },

    #[error("Invalid percent value (PercentValue range 0..=200 per DB CHECK; evaluator satura a 100 antes de state transitions): {0}")]
    InvalidPercent(f64),

    #[error("Invalid state transition: from {from:?} to {to:?}")]
    InvalidTransition { from: QuotaState, to: QuotaState },

    #[error("Plan tier limit lookup failed (data_model.md plan table): {0}")]
    PlanTierLookupFailed(String),
}
```

**Cripto-driven invariants enforced**:

1. **CTRL-BILLING-001** (security_model.md): financial integrity; quota enforcement prevents uncontrolled cost.

2. **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-tenant quota; one tenant exhausting quota does NOT impact other tenants.

3. **INV-TENANT-ISOLATION** (CRITICAL, TLA+): TenantCtx-only middleware S-03 inheritance; cross-tenant injection blocked.

4. **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven Lote 6.2; S-09 inheritance):
   - Transition events CloudEvents v1.0 emitted to S-09 audit chain.
   - Event type: `dev.hugr.corelink.quota.state.transitioned.v1` (sprint contract §5.1 R-S10-1 inheritance pattern; Lote 10.9bis P0-G CloudEvents canonical).
   - Audit-grade trail for SOC 2 CC1.4 + customer dispute resolution.

5. **CTRL-AUTHZ-001 + CTRL-AUTHZ-002** (security_model.md): grace period application protected by `billing_admin` role + Legal/Finance approval; mandatory audit event.

6. **CTRL-PRIV-002** (privacy_model.md L209 — data classification tags): @classification=pii em billing customer_email column; DSR pseudonymization via S-11 procedure (cooperation).

7. **Lote 10.9-quinquies NEW-P0-2 lesson absorbed** (typed payload):
   - `QuotaTransition` event emitted to S-09 audit uses typed enum (NOT serde_json::Value).
   - PII wrappers (CustomerEmail from WI-S10-003 inheritance) explicit serde::Serialize impl Redact-wraps email for audit log.

8. **Fail-CLOSED at quota state transition** (Lote 10.6bis split-tier canonical):
   - Transition writes Neon row + emits audit event atomically.
   - Failure halts; SEV-2 alert; manual replay.

9. **Fail-OPEN at hot path `is_hard_blocked()` query** (Lote 10.6bis split-tier counter-pattern):
   - Hot path CAS PUT/GET cannot block on quota query failure (SLA p99 ≤ 3ms preserved).
   - Query failure → permit + audit signal SEV-2 (degraded quota enforcement).
   - Cooperation com WI-S10-001 hot path discipline.

10. **Idempotent transitions** (re-fire prevention):
    - Same target state from same source = no-op; no email re-spam.
    - Notification dedup via `last_alert_email_at` watermark + 24h cooldown.

11. **5 PlanTier canonical** (per `data_model.md §1` line 68; Lote 10.7bis P0-7 inheritance): plan limits queried from Neon `plan` table (WI-S10-003 inheritance); free/solo/team/business/enterprise tiers.

12. **Enterprise grace period 7d default + ≤ 14d max** (sprint contract §5.5 R-S10-11 + §19 waiver max):
    - CHECK constraint enforces ≤ 14d em Neon `quota_state.grace_days`.
    - Legal review required for grace > 7d.
    - Audit event mandatory.
    - Abuse detection: 3× grace em 90d triggers manual review + contract amendment (sprint contract §15 R-009).

13. **corelink_time::next_month_first_utc_midnight() canonical primitive** (Lote 10.8bis P0-D):
    - Quota reset at billing period boundary; cron invocation idempotent.
    - GAAP cutoff time alignment com WI-S10-003.

14. **TenantCtx propagation** (Lote 10.4bis): tenant_id from S-03 middleware; never request body.

15. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): `worker::send_future`; Neon HTTP via wasm-bindgen; NEVER `tokio::spawn`.

16. **Hard-block 429 com `X-RateLimit-Layer: quota` header** (sprint contract §5.5 R-S10-10):
    - S-08 R-S08-7 alignment canonical (rate limit headers standard).
    - `X-RateLimit-Layer` header values: `rate_limit | quota | abuse_detection` (S-08 inheritance).
    - Customer-facing standard 429 response com `Retry-After: <seconds_until_billing_period_reset>`.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + customer cost protection + service protection discipline justification)

Quota state machine é **the customer cost-protection contract** + **service-protection rate limit** que torna CoreLink billing pipeline customer-trust-grade. Without quota enforcement: customer com runaway Bazel script (e.g., `bazel build //... --remote_cache --no-cache` em CI loop) pode acumular $10k+ em horas; surprise bill destroys customer relationship. Stripe Billing Architecture Guide (sprint contract §17) e Mux exact-metering blog post: "graduated quota state machine (warn → ticket → block) is best-practice for B2B SaaS billing — gives customers chance to react before hard cutoff."

**Why 4 states canonical** (sprint contract §5.5 R-S10-10):
- `under_80` (0-79.99%): silent operation; no friction; default state.
- `soft_alert (80%)`: email customer + in-app notification; reasonable warning before reaching cap.
- `ticket (95%)`: support escalation; CoreLink Finance proactively contacts customer; opportunity for plan upgrade.
- `hard_block (100%)`: CAS write 429; service throttled; protects both customer (cost cap) e CoreLink (DDOS-via-legitimate-volume protection).

3 states would skip middle warning (customer surprise from 80% directly to 100%); 5+ states é overengineering (sprint contract §10 anti-scope discipline).

**Why hard-block 429 com `X-RateLimit-Layer: quota` header** (sprint contract §5.5 R-S10-10 + S-08 R-S08-7 alignment): customer client (Bazel) needs to distinguish quota cap (action: upgrade plan) from rate limit (action: backoff) from abuse detection (action: contact support). `X-RateLimit-Layer` header standard from S-08 canonical.

**Why enterprise tier 7d grace period configurable** (sprint contract §5.5 R-S10-11): enterprise contracts often negotiate softer caps (overage billed monthly em arrears). 7d default; configurable per contract até 14d max (sprint contract §19 waiver) com Legal review. Abuse detection: 3× grace em 90d = pattern signal; manual contract amendment review (sprint contract §15 R-009).

**Why fail-CLOSED at quota state transition + fail-OPEN at hot path is_hard_blocked()** (Lote 10.6bis split-tier canonical lesson): transitions affect customer state machine (state integrity > availability — partial transition = inconsistent state); hot path query supports millions QPS (availability > 100% accuracy — degraded quota enforcement OK; FAIL-OPEN permits + audit alert). Distinct discipline.

**Why idempotent transitions + notification dedup** (UX consideration): customer reaching 80% multiple times in same period (hourly evaluation cron) should NOT receive 24 emails per day; dedup via `last_alert_email_at` 24h cooldown.

**Why typed `QuotaTransition` enum + audit emit** (Lote 10.9-quinquies NEW-P0-2 + S-09 audit chain inheritance): transition events written to immutable 7y audit chain; without typed enum, raw customer email could leak; SOC 2 CC1.4 violation. PII wrapper inheritance from WI-S10-003 (`CustomerEmail` Serialize impl).

**Why integration com WI-S10-002 counter aggregator hourly poll**: real-time quota requires live aggregate; hourly poll trade-off (sprint contract §10 anti-scope = hourly aggregation; real-time per-second deferred). Customer can exceed 100% by 1h max before hard-block kicks in (acceptable for B2B vs strict per-second).

**Adversarial scenarios**:
- **Customer runaway script**: hourly poll → 80% → soft_alert email + in-app; 95% → ticket; 100% → hard-block; customer has 3 chances to react.
- **Enterprise grace abuse 3× em 90d**: pattern detection → manual review trigger; contract amendment per Legal.
- **Cross-tenant injection attempt**: TenantCtx middleware S-03 ensures tenant_id authenticated; quota query scoped per tenant.
- **Quota query failure mid-hot-path**: fail-OPEN permits CAS request; SEV-2 alert; degraded enforcement; eventual catch-up via reconciliation Layer 1.
- **Race condition concurrent transition**: optimistic locking via Neon `(quota_state.tenant_id, last_transition_at) UPDATE WHERE` ensures atomic transition.
- **Notification spam**: 24h cooldown via `last_alert_email_at`; dedup-aware.
- **Customer DSR erasure mid-period (LGPD Art. 16)**: pseudonymization via S-11 cooperation; `quota_state.encrypted_email` BYOK encrypted; audit chain integrity preserved (CTRL-PRIV-002).

**Risk justification HIGH_RISK**:
- **FF-HR-005**: CTRL-BILLING-001 financial integrity quota enforcement; bypass = uncontrolled cost.
- **FF-HR-009**: hard-block = customer service degradation; legal exposure if wrong tier hit.
- 12 sign-offs (Finance + Legal + Privacy emphatic) + chaos suite + property test 100k transitions + S-09 audit chain integration verified.

## 3. Customer Impact & Journey

**Persona 1 — DevOps customer at 75% of quota**: silent operation; under_80 state; no notification.

**Persona 2 — DevOps customer reaching 80%**: receives email "You've used 80% of your monthly quota. Consider upgrading to Team plan for higher limits."; in-app banner em S-13 admin plane.

**Persona 3 — Finance customer reaching 95%**: ticket auto-created via support tooling integration; CoreLink Customer Success proactively reaches out: "We see you're approaching your monthly cap — would you like to discuss plan upgrade or temporary grace period?"

**Persona 4 — Customer reaching 100%**: subsequent CAS write returns 429 com `X-RateLimit-Layer: quota` + `Retry-After: <seconds_until_period_reset>`; client (Bazel) backs off; customer escalates internally.

**Persona 5 — Enterprise customer with negotiated 7d grace**: at 100%, grace_active = true; service continues for 7 days; explicit billing for overage em next invoice; audit trail via CloudEvents.

**Persona 6 — Finance reviewing grace abuse 3× em 90d**: SEV-3 alert; manual review; Contract amendment if pattern continues; protects against grace abuse.

**Persona 7 — Customer DSR erasure request**: S-11 procedure → pseudonymize quota_state.encrypted_email; tenant_id pseudonym retained for audit chain integrity (CTRL-PRIV-002).

**Persona 8 — DevOps responding to fail-OPEN audit signal SEV-2**: quota query failure mid-hot-path; permit + signal; investigation; root-cause; reconciliation Layer 1 catches if quota was actually exceeded.

**SLA addendum**:
- Quota query at hot path: ≤ 5ms p99 (read-through cache via DO state).
- Transition latency: ≤ 1min p99 (hourly evaluation cron + atomic Neon transaction).
- Notification delivery: ≤ 5min p99 email + ≤ 30s in-app push.
- Hard-block enforcement: ≤ 1h delay (hourly evaluation; sprint contract §10 anti-scope no real-time).
- INV-AVAIL-ISOLATION: 0 cross-tenant impact em chaos test 30d.

## 4. Capability Mapping

- **CAP-BILLING-005** (Overage handling) — IMPLEMENTA primary.
- Trace: `data_model.md` (quota_state schema + plan limits) + `security_model.md CTRL-BILLING-001 + CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + CTRL-PRIV-002` + `invariant_registry.md INV-AVAIL-ISOLATION + INV-TENANT-ISOLATION + INV-AUDIT-APPEND-ONLY` + sprint contract §5.5 (R-S10-10/11) + S-08 R-S08-7 X-RateLimit-Layer canonical + Mux exact-metering blog post + Stripe Billing Architecture Guide.

## 5. Tipo

Quota state machine Rust crate + Neon Postgres schema migration + hot path integration `is_hard_blocked()` middleware + hourly evaluation cron + Notifications service integration + support tooling integration; HIGH_RISK; FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-billing-quota/` module** — `QuotaStateMachine` trait + impls + tests.

2. **Neon Postgres schema `quota_state`** (CHECK constraints inline per Lote 10.5bis):
   ```sql
   CREATE TABLE quota_state (
       tenant_id TEXT PRIMARY KEY,
       billing_period TEXT NOT NULL,                    -- "YYYY-MM" canonical
       current_state TEXT NOT NULL,
       current_pct NUMERIC(5,2) NOT NULL,               -- 0.00-200.00 (CHECK 0..=200; precision (5,2) cobre até 999.99 sanidade)
       plan_tier TEXT NOT NULL,
       last_transition_at TIMESTAMPTZ NOT NULL,
       last_alert_email_at TIMESTAMPTZ,                 -- dedup spam (24h cooldown)
       ticket_id TEXT,                                  -- support tooling integration
       hard_block_started_at TIMESTAMPTZ,
       grace_active BOOLEAN NOT NULL DEFAULT false,
       grace_days INTEGER NOT NULL DEFAULT 0,
       grace_started_at TIMESTAMPTZ,
       grace_approved_by TEXT,                          -- Legal/Finance approver
       grace_reason TEXT,                               -- typed enum serialized
       grace_count_90d INTEGER NOT NULL DEFAULT 0,      -- abuse detection counter
       encrypted_email BYTEA,                           -- BYOK encrypted (S-04 inheritance; Notifications target)
       email_hmac TEXT,                                 -- HMAC for notification lookup (no raw)
       updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
       CHECK (current_state IN ('under_80', 'soft_alert', 'ticket', 'hard_block')),  -- canonical 4-state (R4 P1-7 fix; grace é flag separada)
       CHECK (plan_tier IN ('free', 'solo', 'team', 'business', 'enterprise')),  -- canonical per data_model.md §1 L68
       CHECK (current_pct >= 0 AND current_pct <= 200),         -- allows over 100 for reporting (capped at 200 sanity)
       CHECK (grace_days >= 0 AND grace_days <= 14),            -- sprint contract §19 waiver max
       CHECK (grace_count_90d >= 0)
   );

   CREATE INDEX idx_quota_state_billing_period ON quota_state(billing_period);
   CREATE INDEX idx_quota_state_hard_block ON quota_state(tenant_id) WHERE current_state = 'hard_block';
   CREATE INDEX idx_quota_state_alert_pending ON quota_state(tenant_id) WHERE current_state IN ('soft_alert', 'ticket') AND last_alert_email_at IS NULL;

   -- Grace audit log table (separate; append-only)
   CREATE TABLE quota_grace_audit_log (
       id BIGSERIAL PRIMARY KEY,
       tenant_id TEXT NOT NULL,
       grace_days INTEGER NOT NULL,
       grace_reason TEXT NOT NULL,
       approver TEXT NOT NULL,
       applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
       cloudevent_id TEXT NOT NULL,                     -- CloudEvents UUID; S-09 audit chain reference
       CHECK (grace_days <= 14),
       CHECK (grace_reason IN ('enterprise_contract_term', 'legal_reviewed_extension', 'strategic_customer', 'production_incident_mitigation'))
   );

   CREATE INDEX idx_quota_grace_audit_tenant_90d ON quota_grace_audit_log(tenant_id, applied_at);
   ```

3. **State machine transitions canonical** (sprint contract §5.5 R-S10-10):
   ```
   under_80 ─[≥80%]→ soft_alert ─[≥95%]→ ticket ─[≥100% AND grace=false]→ hard_block
                ↓                ↓                ↓
              [<80%]           [<95%]           [<100% OR grace=true]
                ↓                ↓                ↓
              under_80         under_80         soft_alert/ticket
   ```

   Idempotent: same pct → same state; no transition emit.

4. **Hourly evaluation cron** (cooperation WI-S10-002):
   ```rust
   #[cron("0 * * * *")]  // every hour at :00
   pub async fn quota_evaluation_cron() -> Result<(), QuotaError> {
       for tenant in active_tenants_query().await? {
           let billing_period = BillingPeriod::current_canonical();
           let evaluation = quota_machine.evaluate_quota_state(tenant.tenant_id, billing_period).await?;

           // Compute current_pct from WI-S10-002 counter aggregates
           let counters = wi_s10_002_counter_aggregator::query_for_tenant_period(
               tenant.tenant_id, billing_period
           ).await?;
           let plan_limits = plan_query(tenant.plan_tier).await?;
           let current_pct = compute_pct(counters, plan_limits);

           // Determine target state
           let target_state = match current_pct {
               p if p >= 100.0 && !evaluation.grace_active => QuotaState::HardBlock,
               p if p >= 95.0 => QuotaState::Ticket,
               p if p >= 80.0 => QuotaState::SoftAlert,
               _ => QuotaState::Under80,
           };

           // Transition if needed (idempotent)
           if target_state != evaluation.current_state {
               quota_machine.transition_state(
                   tenant.tenant_id,
                   evaluation.current_state,
                   target_state,
                   PercentValue(current_pct)
               ).await?;
           }
       }
       Ok(())
   }
   ```

5. **Hot path integration** `is_hard_blocked()` middleware:
   ```rust
   // Em CAS PUT/GET handler (cooperation S-08 rate limit middleware):
   pub async fn cas_write_handler(req: Request, ctx: TenantCtx) -> Result<Response, Error> {
       // FAIL-OPEN at hot path (Lote 10.6bis split-tier counter-pattern);
       // query failure → permit + audit signal SEV-2.
       match quota_machine.is_hard_blocked(ctx.tenant_id()).await {
           Ok(HardBlockDecision::Blocked429 { layer, retry_after }) => {
               return Ok(Response::error("quota exceeded", 429)
                   .with_header("X-RateLimit-Layer", &layer.to_string())  // S-08 alignment
                   .with_header("Retry-After", &retry_after.num_seconds().to_string()));
           }
           Ok(HardBlockDecision::GraceAllowed { grace_days_remaining }) => {
               // Add header for transparency; service continues
               response = response.with_header("X-Quota-Grace-Days-Remaining", &grace_days_remaining.to_string());
           }
           Ok(HardBlockDecision::Allowed) => { /* continue */ }
           Err(e) => {
               // Fail-OPEN; permit but audit
               worker::console_warn!("quota query failed (permit fail-OPEN): {}", e);
               emit_metric("corelink_billing_quota_query_failures_total{reason=fail_open}", 1.0);
           }
       }

       // ... CAS write logic ...
   }
   ```

6. **Soft alert email + in-app notification** (sprint contract §5.5 R-S10-10):
   - Email sent via Notifications service integration (TBD; cooperation S-13 admin plane).
   - Subject: "Your CoreLink usage is at 80% — consider upgrading"; templated.
   - In-app banner em S-13 admin plane (cooperation).
   - Dedup: `last_alert_email_at` 24h cooldown.

7. **Ticket creation 95%** (sprint contract §5.5 R-S10-10):
   - Support tooling integration (TBD; placeholder API).
   - Ticket assigned to Customer Success team.
   - `quota_state.ticket_id` stored for cross-reference.

8. **Hard-block 429 com `X-RateLimit-Layer: quota`** (sprint contract §5.5 R-S10-10 + S-08 alignment):
   - Header standard from S-08 R-S08-7 inheritance.
   - `Retry-After: <seconds_until_billing_period_reset>` calculated via `corelink_time::next_month_first_utc_midnight() - now()`.

9. **Enterprise grace period** (sprint contract §5.5 R-S10-11):
   - 7d default per enterprise contract.
   - ≤ 14d max via Legal review (sprint contract §19 waiver max).
   - CHECK constraint enforces ≤ 14d em quota_state.grace_days.
   - Mandatory audit event emission to S-09 audit chain via CloudEvents.

10. **Grace abuse detection** (sprint contract §15 R-009):
    - Trigger: 3× grace events em 90d (`SELECT COUNT(*) FROM quota_grace_audit_log WHERE tenant_id = ? AND applied_at >= NOW() - INTERVAL '90 days'`).
    - On 3rd grace: SEV-3 alert + manual review trigger; Customer Success + Legal + Finance pages.
    - Contract amendment recommended.

11. **CloudEvents v1.0 audit emission** (S-09 audit chain inheritance from WI-S09-004; CloudEvents canonical Lote 10.9bis P0-G):
    - Event type: `dev.hugr.corelink.quota.state.transitioned.v1`.
    - Subject: `tenant:<uuid>`.
    - Data: typed `QuotaTransitionData` enum (NOT serde_json::Value — Lote 10.9-quinquies NEW-P0-2):
      ```rust
      #[derive(serde::Serialize)]
      #[serde(tag = "transition_type", rename_all = "snake_case")]
      pub enum QuotaTransitionData {
          UpgradedToSoftAlert { tenant_id: TenantId, from_pct: f64, to_pct: f64, customer_email: CustomerEmail },
          UpgradedToTicket { tenant_id: TenantId, ticket_id: String, from_pct: f64, to_pct: f64 },
          UpgradedToHardBlock { tenant_id: TenantId, started_at: DateTime<Utc> },
          GraceApplied { tenant_id: TenantId, grace_days: u8, approver: ApproverId, reason: GraceReason },
          DowngradedFromHardBlock { tenant_id: TenantId, reset_at: DateTime<Utc> },
          PeriodReset { billing_period: BillingPeriod, tenants_reset: u32 },
      }
      ```
    - Sink: WI-S09-004 audit chain R2 Object Lock 7y.

12. **PII customer email FIPS 140-3 encrypted at rest + BYOK** (CTRL-PRIV-002):
    - `quota_state.encrypted_email` column (S-04 inheritance).
    - Email HMAC for notification lookup (no raw).
    - DSR erasure: pseudonymization via S-11 cooperation.

13. **TenantCtx-only enforcement** (Lote 10.4bis): tenant_id from S-03 middleware; never request body.

14. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): `worker::send_future`; Neon HTTP via wasm-bindgen; NEVER `tokio::spawn`.

15. **5 PlanTier canonical** (Lote 10.7bis P0-7): plan limits queried from Neon `plan` table (WI-S10-003 inheritance).

16. **corelink_time::next_month_first_utc_midnight() canonical primitive** (Lote 10.8bis P0-D):
    - Quota reset at billing period boundary.
    - GAAP cutoff alignment com WI-S10-003.

17. **5 SKUs canonical** (sprint contract §10 anti-scope): per-SKU pct computation from counter aggregates.

18. **Cooperation com S-08 rate limit middleware**: hard-block 429 header `X-RateLimit-Layer: quota` aligned com `rate_limit | abuse_detection` values; both layers can coexist (rate limit short-term + quota monthly).

19. **Métricas operacionais** (via WI-S09-001 emit lib; cardinality budget; underscored canonical Lote 10.9bis P0-E):
    - `corelink_billing_quota_state_transitions_total{from_state, to_state, plan_tier}` (counter; 5×5×5 = 125 séries; informational).
    - `corelink_billing_quota_hard_block_events_total{plan_tier}` (counter; 5 séries; **alert SEV-3 if > 10/h** sustained — DDoS canary).
    - `corelink_billing_quota_grace_applied_total{plan_tier, reason}` (counter; 5×4 = 20 séries; **alert SEV-3 if > 1/tenant/30d** — abuse signal).
    - `corelink_billing_quota_grace_abuse_detected_total` (counter; **alert SEV-2 if > 0** — pattern signal; sprint contract §15 R-009).
    - `corelink_billing_quota_query_latency_seconds{layer}` (histogram; layer ∈ hot_path / evaluation_cron; 2×11 + sum/count = 24).
    - `corelink_billing_quota_query_failures_total{reason}` (counter; **alert SEV-2 if > 1% sustained** — fail-OPEN audit).
    - `corelink_billing_quota_notification_delivery_failures_total{channel}` (counter; channel ∈ email/in_app/ticket; 3 séries; **alert SEV-3**).
    - `corelink_billing_quota_current_pct{tenant_tier}` (gauge; 5 tiers — anonymized aggregate; no per-tenant labels for cardinality).
    - `corelink_billing_quota_period_reset_count_total` (counter; informational; monthly cron canary).

20. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 absorbed):
    - `prop_state_transitions_canonical`: 100k random pct values; assert correct target state.
    - `prop_idempotent_transitions`: 100k same-state transitions; no spurious notifications.
    - `prop_email_dedup_24h_cooldown`: 1k transitions same tenant within 24h; assert max 1 email.
    - `prop_grace_abuse_detection_3_em_90d`: 1k grace event sequences; assert detection on 3rd.
    - `prop_hard_block_429_header`: 1k hard-block events; assert X-RateLimit-Layer: quota header present.
    - `prop_typed_payload_no_serde_json_value`: 10k transition events; assert typed QuotaTransitionData; serde_json::Value rejected at compile time.
    - `prop_pii_redacted_em_audit_event`: 10k events with customer email; assert serialized payload contains "j***@" form NOT raw.

21. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 11):
    1. **Customer hits 80% mid-period**: soft_alert email + in-app within 5min; corelink_billing_quota_state_transitions_total{from=under_80, to=soft_alert} increments.
    2. **Customer hits 95% mid-period**: ticket created; Customer Success notified; quota_state.ticket_id populated.
    3. **Customer hits 100%**: hard_block; CAS write returns 429 com X-RateLimit-Layer: quota + Retry-After; service throttled.
    4. **Enterprise grace 7d applied**: hard_block prevented; grace_active=true; service continues; audit event emitted.
    5. **Grace abuse 3× em 90d detected**: SEV-2 alert; manual review trigger; Customer Success + Legal + Finance paged.
    6. **Quota query failure mid-hot-path**: fail-OPEN permits CAS request; SEV-2 alert; audit signal.
    7. **Concurrent transition race** (cron + manual admin): optimistic locking fails first attempt; retry; idempotent.
    8. **Notification delivery failure email**: dedup-aware retry; SEV-3 alert; manual fallback via Customer Success.
    9. **Period boundary reset** (corelink_time::next_month_first_utc_midnight()): cron resets all tenants; idempotent re-run safe.
    10. **DSR erasure mid-period**: pseudonymization via S-11; quota_state.encrypted_email tombstone; audit chain integrity preserved.
    11. **Cross-tenant injection attempt**: TenantCtx middleware S-03 ensures tenant_id authenticated; quota query scoped per tenant; SEV-1 audit if mismatch detected.

### 6.2 Out-of-scope (deferred)

- Counter aggregation (delegate WI-S10-002 — input source).
- Stripe integration (delegate WI-S10-003 — plan limits source).
- Reconciliation worker (delegate WI-S10-004).
- Replay forensic endpoint (delegate WI-S10-006).
- TLA+ billing_atomicity (delegate WI-S10-007).
- Customer-facing quota dashboard UI (delegate S-13/S-16).
- Notifications service implementation (delegate dedicated service; WI-S10-005 integrates via stub).
- Support tooling integration implementation (delegate dedicated service).
- Real-time per-second quota check (anti-scope sprint contract §10 — hourly canonical).

## 7. Anti-Scope

- ❌ serde_json::Value em QuotaTransitionData (Lote 10.9-quinquies NEW-P0-2; typed canonical).
- ❌ Notification spam (24h cooldown + idempotent transitions).
- ❌ Grace > 14d (sprint contract §19 waiver max; CHECK constraint).
- ❌ Real-time per-second quota check (hourly canonical sprint contract §10).
- ❌ Fail-CLOSED at hot path is_hard_blocked() (would cascade to customer 5xx; fail-OPEN canonical Lote 10.6bis split-tier).
- ❌ Fail-OPEN at quota state transition (state integrity > availability; fail-CLOSED canonical Lote 10.6bis).
- ❌ Header `X-RateLimit-Layer` missing (S-08 alignment mandatory).
- ❌ ad-hoc `now() + Duration::days(30)` (corelink_time::next_month_first_utc_midnight canonical Lote 10.8bis P0-D).
- ❌ Per-feature quota expansion beyond 5 SKU canonical (sprint contract §10).
- ❌ Plan tier expansion beyond 5 canonical (Lote 10.7bis P0-7).
- ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ TenantCtx bypass.
- ❌ Skip audit event emission on transition (CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + INV-AUDIT-APPEND-ONLY).

## 8. Acceptance Criteria (Gherkin) — 11 scenarios

```gherkin
Feature: Quota State Machine + Overage Handling + Email/In-App Notification

  Scenario: Customer reaches 80% triggers soft_alert
    Given tenant T is at 79.5% of monthly quota
    Given quota_state.current_state = under_80
    When hourly evaluation cron runs at billing_period 2026-09; tenant T usage now 80.5%
    Then transition_state(T, under_80, soft_alert, 80.5)
    Then quota_state row UPDATED current_state=soft_alert, current_pct=80.5
    Then email sent to customer (encrypted_email decrypted via BYOK)
    Then in-app notification em S-13 admin plane
    Then last_alert_email_at = now()
    Then CloudEvents audit emit dev.hugr.corelink.quota.state.transitioned.v1
    Then corelink_billing_quota_state_transitions_total{from=under_80, to=soft_alert, plan_tier=team} increments

  Scenario: Customer hits 95% triggers ticket creation
    Given tenant T at 94.9%; current_state=soft_alert
    When usage reaches 95.5%
    Then transition to ticket state
    Then ticket created via support tooling integration
    Then quota_state.ticket_id populated
    Then Customer Success team notified via PagerDuty corelink-finance

  Scenario: Customer hits 100% triggers hard_block 429
    Given tenant T at 99.9%; current_state=ticket; grace_active=false
    When usage reaches 100.5%
    Then transition to hard_block
    Then quota_state.hard_block_started_at = now()
    Then subsequent CAS PUT request:
      Then returns HTTP 429
      Then header X-RateLimit-Layer: quota (S-08 alignment)
      Then header Retry-After: <seconds_until_period_reset>

  Scenario: Enterprise grace 7d prevents hard_block
    Given tenant T (plan=enterprise); grace_active=true; grace_days=7
    Given usage at 105%
    When CAS PUT request
    Then HardBlockDecision::GraceAllowed { grace_days_remaining: 7 }
    Then service continues
    Then header X-Quota-Grace-Days-Remaining: 7
    Then audit event emitted for transparency

  Scenario: Grace abuse 3× em 90d detected
    Given tenant T applied grace 2× em previous 90d
    When 3rd grace event applied at day 89
    Then quota_grace_audit_log row 3 inserted
    Then grace_count_90d = 3
    Then SEV-2 alert: corelink_billing_quota_grace_abuse_detected_total increments
    Then PagerDuty corelink-finance + corelink-legal paged
    Then manual review triggered; contract amendment review

  Scenario: Idempotent transition (same state)
    Given tenant T at 82%; current_state=soft_alert; last_alert_email_at=2h ago
    When hourly cron runs; usage still 82%
    Then transition_state(T, soft_alert, soft_alert, 82.0)
    Then TransitionOutcome::Idempotent
    Then NO email re-sent (24h dedup; soft_alert already)
    Then NO duplicate audit event

  Scenario: Notification dedup 24h cooldown
    Given tenant T transitioned to soft_alert at T0; email sent
    When same transition attempted at T0+12h
    Then quota_state.last_alert_email_at = T0
    Then 24h cooldown not elapsed
    Then NO duplicate email
    Then notification dedup signal logged

  Scenario: Fail-OPEN at hot path quota query failure
    Given Neon Postgres transient unavailability mid-CAS-PUT
    When is_hard_blocked() invoked from hot path
    Then QuotaError::PostgresFailed returned
    Then handler permits request (fail-OPEN)
    Then CAS write proceeds; SLA p99 ≤ 3ms preserved
    Then SEV-2 alert: corelink_billing_quota_query_failures_total{reason=fail_open}
    Then reconciliation Layer 1 (WI-S10-004) catches if quota actually exceeded

  Scenario: Fail-CLOSED at quota state transition
    Given Neon Postgres transient unavailability during transition_state()
    When transition_state(T, under_80, soft_alert, 80.5) called
    Then QuotaError::PostgresFailed returned (fail-CLOSED; no partial transition)
    Then quota_state row NOT updated
    Then SEV-2 alert
    Then manual replay after recovery

  Scenario: Period boundary reset corelink_time::next_month_first_utc_midnight()
    Given current date 2026-09-30T23:59:00Z
    Given cron fires at corelink_time::next_month_first_utc_midnight() = 2026-10-01T00:00:00Z
    When reset_period_boundary(billing_period=2026-10) invoked
    Then all quota_state rows UPDATE current_pct=0, current_state=under_80
    Then last_alert_email_at = NULL
    Then ticket_id = NULL
    Then hard_block_started_at = NULL
    Then idempotent: cron re-fire safe (no-op for already-reset rows)

  Scenario: Typed QuotaTransitionData (Lote 10.9-quinquies NEW-P0-2)
    Given developer attempts use serde_json::Value em audit event payload
    When cargo build runs
    Then compilation fails: typed QuotaTransitionData canonical
    Then merge bloqueado

  Scenario: PII redacted em quota audit event (Lote 10.9-quinquies NEW-P0-2)
    Given customer email "john@example.com" in QuotaTransitionData::UpgradedToSoftAlert
    When CloudEvent serializes em S-09 audit chain
    Then CustomerEmail Serialize impl calls Redact::redact() → "j***@example.com"
    Then audit event payload contains redacted form
    Then raw email NOT em immutable 7y archive
```

## 9. Design Decisions

- 9.1: 4-state machine canonical (under_80 → soft_alert → ticket → hard_block; sprint contract §5.5 R-S10-10).
- 9.2: Grace é flag boolean em quota_state (não 5º estado; sprint contract §5.5 R-S10-11 mantém canonical 4-state); ≤ 14d max §19 waiver. R4 P1-7 alignment.
- 9.3: Hourly evaluation cron (cooperation WI-S10-002; sprint contract §10 anti-scope no real-time).
- 9.4: Hot path is_hard_blocked() FAIL-OPEN (Lote 10.6bis split-tier counter-pattern; SLA preserved).
- 9.5: State transition FAIL-CLOSED (Lote 10.6bis split-tier; state integrity > availability).
- 9.6: Idempotent transitions (no spurious re-fire; 24h cooldown email).
- 9.7: Hard-block 429 com X-RateLimit-Layer: quota header (S-08 R-S08-7 alignment canonical).
- 9.8: Optimistic locking via quota_state.last_transition_at (concurrent transition safety).
- 9.9: Notification dedup via last_alert_email_at 24h cooldown.
- 9.10: Audit event emission CloudEvents v1.0 dev.hugr.corelink.quota.state.transitioned.v1 (S-09 inheritance + Lote 10.9bis P0-G CloudEvents canonical).
- 9.11: Typed QuotaTransitionData (NOT serde_json::Value; Lote 10.9-quinquies NEW-P0-2 inheritance).
- 9.12: PII wrapper CustomerEmail (WI-S10-003 inheritance; Serialize impl Redact-wraps).
- 9.13: BYOK encryption customer_email (S-04 inheritance + CTRL-PRIV-002).
- 9.14: 5 PlanTier canonical (Lote 10.7bis P0-7); CHECK constraint.
- 9.15: 5 SKUs canonical (sprint contract §10 anti-scope).
- 9.16: corelink_time::next_month_first_utc_midnight() canonical primitive (Lote 10.8bis P0-D).
- 9.17: Grace abuse detection 3× em 90d (sprint contract §15 R-009 mitigation).
- 9.18: TenantCtx-only enforcement (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3).
- 9.19: PercentValue typed (CHECK 0..=200 sanity; allows over 100 reporting bounded).

## 10. Completeness Criteria SOTA

- [ ] **10.s10.005.1** Crate compila + integration tests green.
- [ ] **10.s10.005.2** All 11 Gherkin scenarios green.
- [ ] **10.s10.005.3** Property tests 7 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar).
- [ ] **10.s10.005.4** Chaos suite 11 scenarios green.
- [ ] **10.s10.005.5** Hard-block 429 com X-RateLimit-Layer: quota header verified em S-08 alignment test.
- [ ] **10.s10.005.6** Enterprise grace 7d default + ≤ 14d max enforced via CHECK constraint.
- [ ] **10.s10.005.7** Grace abuse detection 3× em 90d alert verified.
- [ ] **10.s10.005.8** Hourly evaluation cron + cooperation com WI-S10-002 verified.
- [ ] **10.s10.005.9** Notification dedup 24h cooldown verified.
- [ ] **10.s10.005.10** Period boundary reset corelink_time::next_month_first_utc_midnight() idempotent.
- [ ] **10.s10.005.11** CloudEvents v1.0 audit emission to S-09 chain verified (Lote 10.9bis P0-G canonical).
- [ ] **10.s10.005.12** PII redaction at audit event verified: 0 raw email em S-09 audit log payload (Lote 10.9-quinquies NEW-P0-2).
- [ ] **10.s10.005.13** Métricas (9) emitted via WI-S09-001 emit lib; cardinality budget respected (~200 séries baseline).
- [ ] **10.s10.005.14** Cargo-audit + cargo-deny + clippy clean.

## 11. DoD

- [ ] Crate compila + tests green; all Gherkin/property/chaos green; S-08 alignment verified; 12 sign-offs (HIGH_RISK; framework §33.5.4.3 cap; Finance + Legal + Privacy emphatic).

## 12. Invariants Validated

- **CTRL-BILLING-001** (security_model.md): financial integrity quota enforcement.
- **CTRL-AUTHZ-001 + CTRL-AUTHZ-002** (security_model.md): grace period role-protected `billing_admin`; mandatory audit event.
- **CTRL-PRIV-002** (privacy_model.md L209 — data classification tags): @classification=pii em customer_email coluna; DSR pseudonymization via S-11 procedure (cooperation).
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-tenant quota; cross-tenant impact zero.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): TenantCtx-only middleware S-03 inheritance.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven Lote 6.2; S-09 inheritance): transition events immutable em CloudEvents audit chain.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Quota state machine module | `crates/corelink-billing-quota/` | Rust |
| State machine impl | `crates/corelink-billing-quota/src/state_machine.rs` | Rust |
| Hot path middleware | `crates/corelink-worker/src/middleware/quota_check.rs` | Rust |
| Hourly evaluation cron | `crates/corelink-worker/src/crons/quota_evaluation.rs` | Rust |
| Period boundary reset cron | `crates/corelink-worker/src/crons/quota_period_reset.rs` | Rust |
| Neon Postgres migration | `migrations/00X_quota_state.sql` | SQL |
| Property tests | `crates/corelink-billing-quota/tests/prop_quota.rs` | Rust |
| Chaos suite | `tests/chaos_billing_quota.rs` | Rust |
| Notifications integration stub | `crates/corelink-billing-quota/src/notifications.rs` | Rust |
| Support tooling integration stub | `crates/corelink-billing-quota/src/support_tooling.rs` | Rust |

## 14. Quality Standards SOTA

- 14.s10.005.1: Zero unsafe Rust; zero unwrap em production paths.
- 14.s10.005.2: rustdoc 100% public API.
- 14.s10.005.3: Test coverage ≥ 90%.
- 14.s10.005.4: Quota query at hot path ≤ 5ms p99.
- 14.s10.005.5: Transition latency ≤ 1min p99.
- 14.s10.005.6: SAST clean; cargo-deny strict.
- 14.s10.005.7: Métricas (9 §6.1.19).
- 14.s10.005.8: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s10.005.9: TenantCtx-only (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); 5-tier canonical Plan (Lote 10.7bis P0-7); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV positions §3.9 L136/137 (NO-LOSS/NO-DUP) + §3.12 L166/167 (RECONCILE-3-LAYER/REPLAYABLE) verified Lote 10.10bis (Lote 10.8bis P1-13 lesson absorbed).
- 14.s10.005.10: corelink_time::next_month_first_utc_midnight() canonical (Lote 10.8bis P0-D); period reset boundary canonical.
- 14.s10.005.11: D1 batch ≤ 250 (Lote 10.5bis); CHECK constraints inline (Lote 10.5bis).
- 14.s10.005.12: Quota state transition fail-CLOSED + hot path fail-OPEN canonical (Lote 10.6bis split-tier).
- 14.s10.005.13: Typed QuotaTransitionData (NOT serde_json::Value; Lote 10.9-quinquies NEW-P0-2 inheritance); PII wrapper CustomerEmail explicit Serialize impl.
- 14.s10.005.14: CloudEvents v1.0 dev.hugr.corelink.quota.state.transitioned.v1 (Lote 10.9bis P0-G CloudEvents canonical inheritance).
- 14.s10.005.15: Prom metric names underscored canonical (Lote 10.9bis P0-E inheritance).
- 14.s10.005.16: Hard-block 429 X-RateLimit-Layer: quota header (S-08 R-S08-7 alignment canonical).

## 15. Chaos Experiments (11)

§6.1.21 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (framework §33.5.4.3 cap; Finance + Legal + Privacy + Architect emphatic).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + QuotaStateMachine trait + 5 QuotaState enum + typed structs | 1.5 |
| ST-002 | State machine logic (4 transitions canonical + grace) + idempotent semantics | 2 |
| ST-003 | Neon Postgres schemas (quota_state + quota_grace_audit_log) migrations + CHECK constraints | 1.5 |
| ST-004 | Hourly evaluation cron + cooperation WI-S10-002 counter aggregator | 1.5 |
| ST-005 | Hot path middleware is_hard_blocked() + fail-OPEN + 429 X-RateLimit-Layer header (S-08 alignment) | 1.5 |
| ST-006 | Soft alert email + in-app notification stubs + Notifications service integration | 1 |
| ST-007 | Ticket creation 95% + support tooling integration stub | 1 |
| ST-008 | Enterprise grace 7d + ≤14d max + Legal approval flow | 1 |
| ST-009 | Grace abuse detection 3× em 90d + alert | 1 |
| ST-010 | CloudEvents audit emission + S-09 chain integration + typed QuotaTransitionData (Lote 10.9-quinquies NEW-P0-2) | 1.5 |
| ST-011 | PII customer email BYOK encryption (S-04 inheritance) + CTRL-PRIV-002 stub | 1 |
| ST-012 | Period boundary reset cron corelink_time::next_month_first_utc_midnight() | 1 |
| ST-013 | Métricas (9) emit | 1 |
| ST-014 | Property tests (7 × 10k; 100k nightly) | 2 |
| ST-015 | Chaos suite (11) + S-08 alignment integration test | 1.5 |
| ST-016 | Privacy review + Legal grace policy sign-off + Finance walkthrough | 0.5 |

**Total**: ~19h. **PERT** O=10h M=16h P=24h: **16.3h** (matches sprint contract §12 estimate).

## 18. Dependencies

- Hard: WI-S10-002 SEALED (counter aggregates input source for hourly evaluation); WI-S10-003 SEALED (plan limits source via Neon `plan` table; CustomerEmail wrapper inheritance); WI-S09-001 SEALED (cardinality emit lib); WI-S09-004 SEALED (audit chain CloudEvents v1.0); S-03 SEALED (TenantCtx); S-04 SEALED (BYOK encryption); S-08 SEALED (X-RateLimit-Layer header standard alignment).
- Soft: WI-S10-004 (Layer 1 reconciliation may catch quota query fail-OPEN drift); WI-S10-007 (TLA+ billing_atomicity); S-11 (DSR cooperation CTRL-PRIV-002); S-13 (admin plane in-app notification consumer).
- Hard infra: Neon Postgres available; Cloudflare Worker cron-trigger; Notifications service available (TBD); support tooling available (TBD).

## 19. Effort PERT: ~16.3h. ## 20. Time-boxing: 24h hard limit.

## 21. Observability

9 metrics §6.1.19. Trace span `billing.quota.{evaluate, transition, hot_path_check, notification_send, ticket_create, grace_apply, period_reset}`.

## 22. Cost Analysis

- Neon Postgres: ~5 GB/region × 5 regions × $0.23/GB-mo × 12 = ~$70/yr (shared com WI-S10-003).
- Worker requests (hot path quota check): ~1B requests/yr × $0.30/1M = $300/yr.
- Worker cron (hourly evaluation × 5 regions × 365 = 43800/yr): negligible.
- Notifications service (email): integration cost passes through; ~1k emails/mo × $0.001 = $12/yr.
- TCO 12m: ~$400/yr quota infrastructure.
- **Cost saved by quota enforcement**: prevents customer surprise bill (avg $5k+ per incident em B2B SaaS); 4-state machine = 3 chances to react before hard block; customer trust preservation.

## 23. API Contract

- Public Rust: `QuotaStateMachine` trait + `QuotaState`, `QuotaEvaluation`, `TransitionOutcome`, `HardBlockDecision`, `GraceReason`, `QuotaError` types; `#[non_exhaustive]`.
- Wire (inbound CAS hot path): `is_hard_blocked()` middleware check.
- Wire (outbound 429): HTTP 429 com `X-RateLimit-Layer: quota` + `Retry-After: <seconds>`.
- Storage: Neon Postgres `quota_state` + `quota_grace_audit_log`; BYOK encrypted email column.
- Cron-trigger: hourly evaluation `0 * * * *`; period reset `0 0 1 * *` (1st of month UTC midnight).

## 24. Post-mortem Hooks

- INV-AVAIL-ISOLATION violation (cross-tenant impact via quota) → CRITICAL post-mortem.
- Grace abuse 3× em 90d → post-mortem (Customer Success + Legal + Finance review).
- Hard-block surge > 10/h sustained 30min → SEV-3 + post-mortem (DDoS canary).
- Notification delivery failure > 5% sustained → post-mortem (Notifications service health).
- Quota query fail-OPEN > 1% sustained → post-mortem (Neon health).
- Period boundary reset failure → SEV-1 post-mortem (billing cycle integrity).

## 25. Rollback / Recovery

- Rollback: revert hot path middleware; CAS handler skips quota check; revenue protection lost (acceptable temporarily); state machine continues evaluating quota states for reporting.
- Recovery: middleware re-mounted; state machine state intact em Neon; immediate enforcement.
- RTO ≤ 30min (Worker rollback); RPO ≤ 0min (Neon durable).

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TenantCtx middleware S-03; cross-tenant injection blocked.
- T(ampering): quota_state row UPDATE auditable via CloudEvents emit; Neon transaction logs.
- R(epudiation): audit event 7y immutable em S-09 chain.
- I(nformation disclosure): CustomerEmail BYOK encrypted; Redact-wrapped Serialize impl prevents raw em audit log.
- D(enial of Service): Hard-block 429 protects against runaway customer (intentional or accidental).
- E(scalation of Privilege): Grace application requires billing_admin role + Legal/Finance approver; mandatory audit event.

**LINDDUN** (LGPD/GDPR):
- L(inkability): quota_state per tenant_id; expected.
- I(dentifiability): customer_email BYOK-encrypted; pseudonymization via S-11 DSR.
- N(on-repudiation): audit chain immutable evidence.
- D(etectability): customer access via S-13 admin plane.
- D(isclosure): 7y retention audit chain vs erasure right (LGPD Art. 16); pseudonymization preserves chain integrity (CTRL-PRIV-002 + S-11 cooperation).
- U(nawareness): customer notified via email + in-app + ticket cycle.
- N(on-compliance): **CTRL-BILLING-001 + CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + CTRL-PRIV-002 + SOC 2 CC1.4 + LGPD Art. 32 + GDPR Art. 32** compliance.

## 27. Knowledge Transfer

Tech talk (1.5h): "S-10 Quota State Machine: 4-State Canonical + Grace + Audit Chain"; doc `docs/dev/billing-quota-architecture.md`; onboarding test 10 questions: 4 state canonical (under_80 → soft_alert → ticket → hard_block; sprint contract §5.5 R-S10-10), enterprise grace 7d default + ≤14d max + Legal review (sprint contract §5.5 R-S10-11 + §19), grace abuse 3× em 90d detection (sprint contract §15 R-009), hard-block 429 X-RateLimit-Layer: quota (S-08 alignment), fail-CLOSED transition vs fail-OPEN hot path (Lote 10.6bis split-tier), idempotent transitions + 24h email cooldown, CloudEvents v1.0 dev.hugr.corelink.quota.state.transitioned.v1 (Lote 10.9bis P0-G), typed QuotaTransitionData (Lote 10.9-quinquies NEW-P0-2), 5 PlanTier canonical (Lote 10.7bis P0-7), corelink_time::next_month_first_utc_midnight() period reset (Lote 10.8bis P0-D).

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Customer surprise bill (no warning) | M | L | HIGH | M | LOW | 4-state canonical (80%/95%/100% warn/ticket/block); customer 3 chances to react |
| R-002 | Cross-tenant injection via quota query | L | L | CRITICAL | L | LOW | TenantCtx S-03 middleware; quota query scoped per tenant_id |
| R-003 | Grace abuse pattern (legitimate-looking) | M | M | MEDIUM | M | LOW | 3× em 90d detection + manual review + contract amendment |
| R-004 | Notification spam (multiple emails per period) | M | L | MEDIUM | L | LOW | 24h cooldown + idempotent transitions |
| R-005 | Hot path quota query failure cascade | L | L | HIGH | L | LOW | Fail-OPEN at hot path (Lote 10.6bis split-tier); audit signal SEV-2 |
| R-006 | Concurrent transition race | L | L | MEDIUM | L | LOW | Optimistic locking via last_transition_at; retry idempotent |
| R-007 | Hard-block 429 missing X-RateLimit-Layer header | L | L | LOW | L | LOW | S-08 alignment test verified; Gherkin scenario |
| R-008 | Grace > 14d Legal violation | L | L | HIGH | L | LOW | CHECK constraint enforces ≤14d (sprint contract §19 waiver max) |
| R-009 | DSR erasure breaks audit chain | L | M | MEDIUM | L | LOW | Pseudonymization (NOT delete) via S-11 + CTRL-PRIV-002 |
| R-010 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | MEDIUM | L | LOW | grep verification before commit |
| R-011 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; worker::send_future |
| R-012 | PII em audit event (Lote 10.9-quinquies NEW-P0-2) | L | M | CRITICAL | L | LOW | Typed QuotaTransitionData + CustomerEmail wrapper Serialize impl |

## 29. Review Checkpoints

D+0 design (Architect; 4-state canonical + fail-CLOSED transition + fail-OPEN hot path); D+1 Finance (quota enforcement + grace policy + abuse detection); D+2 Legal (grace ≤14d max + customer-facing 429 message); D+3 AppSec (TenantCtx + CTRL-AUTHZ-001 + CTRL-AUTHZ-002 grace role); D+4 Privacy (LINDDUN + CustomerEmail wrapper inheritance + DSR cooperation); D+5 Compliance (audit chain CloudEvents emit + 7y retention); D+6 code review; D+7 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — TenantCtx + CTRL-AUTHZ-001 + CTRL-AUTHZ-002 grace role + STRIDE_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos 11 + property test 100k + S-08 alignment_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 CC1.4 + audit chain CloudEvents emit + 7y retention_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — LINDDUN + CustomerEmail wrapper inheritance + DSR cooperation S-11_ |
| 11 | Architect | _TBD; **mandatory emphatic** — 4-state canonical + fail-CLOSED transition vs fail-OPEN hot path (Lote 10.6bis) + chrono primitives (Lote 10.8bis P0-D) + INV §3.X verification (Lote 10.8bis P1-13) + Lote 10.9-quinquies NEW-P0-2 absorption + CloudEvents canonical (Lote 10.9bis P0-G)_ |
| 12 | Finance | _TBD; **mandatory emphatic** — quota enforcement customer cost protection + grace policy abuse detection + 3× em 90d trigger_ |

(Legal sign-off via DPA reference at sprint level + grace policy review.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.10) | Criação WI-S10-005; HIGH_RISK; SOTA pós-Lote 10.7bis + Lote 10.8bis/tris + Lote 10.9bis/tris/quaters/quinquies lessons absorbed: 5-tier canonical Plan (Lote 10.7bis P0-7); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (Lote 10.7bis P1-3); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV §3.X canonical position TBD verification before commit (Lote 10.8bis P1-13); corelink_time::next_month_first_utc_midnight() canonical primitive (Lote 10.8bis P0-D); calibration n=50+50 + 95% CI bounded (Lote 10.8bis P0-E); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis). **Lote 10.9-quinquies NEW-P0-2 critical absorption**: typed QuotaTransitionData enum (NOT serde_json::Value); CustomerEmail wrapper inherited from WI-S10-003 explicit serde::Serialize impl calling Redact::redact() — prevents raw email writing to S-09 audit chain immutable 7y archive. **Lote 10.6bis split-tier canonical inheritance**: fail-CLOSED at quota state transition (state integrity > availability) + fail-OPEN at hot path is_hard_blocked() (SLA preserved). **Lote 10.9bis P0-G inheritance**: CloudEvents canonical dev.hugr.corelink.quota.state.transitioned.v1 (NOT 1.0.2; NOT io.corelink). **Lote 10.9bis P0-E inheritance**: Prom metric names underscored canonical. NEW corelink-billing-quota crate + 4-state canonical machine (under_80/soft_alert/ticket/hard_block) + 5th state grace_active + hourly evaluation cron + hot path is_hard_blocked() middleware + 429 X-RateLimit-Layer: quota header (S-08 R-S08-7 alignment) + soft_alert email + ticket 95% + enterprise grace 7d (≤14d max sprint contract §19 waiver) + grace abuse detection 3× em 90d (sprint contract §15 R-009) + CloudEvents v1.0 audit emission to S-09 chain + Neon Postgres quota_state + quota_grace_audit_log schemas + BYOK encrypted customer_email + period boundary reset cron corelink_time::next_month_first_utc_midnight(). CTRL-BILLING-001 + CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + CTRL-PRIV-002 + SOC 2 CC1.4 + LGPD Art. 32 + GDPR Art. 32 compliance. |
| 1.1.0 | 2026-04-26 | Gustavo (Lote 10.10bis) | P0+P1 remediation pós-R4 Opus + Sonnet R5 adversarial reviews. **8 P0s fixed**: (1) PlanTier 5-tuple inventado `free/team/enterprise/custom/trial` → canonical `free/solo/team/business/enterprise` per `data_model.md §1` line 68 — corrige CHECK constraints em WIs 003+005 + sprint contract §5.3 R-S10-7; (2) INV registry positions estampadas `§3.X TBD` → reais §3.9 line 136 (NO-LOSS HIGH), §3.9 line 137 (NO-DUP HIGH), §3.12 line 166 (RECONCILE-3-LAYER HIGH), §3.12 line 167 (REPLAYABLE HIGH); severity drift CRITICAL→HIGH cascateia em SEV-2 alert wording (vs SEV-1); Lote 10.8bis P1-13 lesson finalmente absorvida; (3) WI-007 TLA+ math 5× errado (15k claim → real 3,000; 150k → real 30,000) + caveat sobre TLC reachable state graph ≠ Cartesian product CONSTANTS; MaxConcurrentEvents bound NEW; (4) CTRL-AUTHZ-005 alucinado (security_model só define -001/-002) → CTRL-AUTHZ-001 + CTRL-AUTHZ-002 explicit; sprint contract §5.6 R-S10-13 também atualizado; (5) WI-002 UPSERT WHERE clause auto-anulante removido (`WHERE own_digest = excluded.own_digest` rejeitava updates legítimos); (6) Cloudflare R2 Terraform schema marcada com TODO pre-merge verification (era hallucinated AWS S3 pattern `cloudflare_r2_bucket_lock_configuration`); (7) **R5 P0-A**: WI-002 PRIMARY KEY `(tenant_id, sku, hour)` → `(tenant_id, region, sku, hour)` — multi-region tenants antes corrompiam dados a cada UPSERT; (8) **R5 P0-B**: WI-001 D1 CHECK enum `('cas_put',...)` rejeitava 100% dos inserts (strum serializa para long form `dev.hugr.corelink.cas.put.v1`); aligned com Lote 10.9bis P0-G CloudEvents prefix canonical. **P1s fixed**: (a) staging table region column added (R5 P1-A); (b) WI-003 narrative 62-char idempotency key contradiziando §6.1 35-char (R5 P1-B); (c) idempotency comment WI-001 (P1-C); (d) `CHECK (age_hours > 6)` off-by-one → `>= 6` (R5 P1-D); (e) cardinality 30 regions → 5 canonical (P1-G); (f) PercentValue type/DB CHECK alignment 0..=200 (R5 P1-H); (g) chrono::next_month_first_utc_midnight() (não existe em chrono crate) → corelink_time::next_month_first_utc_midnight() canonical helper internal; (h) CTRL-PRIV-002 mis-mapped to "pseudonymization" → privacy_model.md L209 "data classification tags" + S-11 cooperation para DSR pseudonymization separada; (i) sprint contract §5.7 R-S10-15 metric names dots → underscores (Prom canonical); (j) §5.1 R-S10-1 CloudEvents prefix `corelink.usage.*` → `dev.hugr.corelink.<op>.v1`; (k) WI-005 4-state vs 5-state — GraceActive removido como state, virou flag boolean preserving sprint contract §5.5 R-S10-10 canonical; (l) failure_modes.md adicionado RB-FM-151 mapping (P1-5). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |
| 1.2.0 | 2026-04-26 | Gustavo (Lote 10.10-quaters) | P0+P1 remediation pós-R4 + R5 round-2 audits (R4 6.9/10, R5 7.8/10). Same S-08/S-09 cascade-incomplete pattern: bis ~60% complete; quaters fecha residuais. **8 P0s addressed**: (1) NEW-P0-1 spec contract §8 invariants L149-151 CRITICAL→HIGH (era source-of-truth não atualizada; severity contradicting WIs); (2) NEW-P0-2 WI-002 PK 4-tuple cascade ~30%→100% — `CounterRecord` + `LateCounterRecord` Rust structs add `region: Region` field (digest input para tamper detection per-region); 15 narrative/Gherkin/design-decision references swept (tenant_id, sku, hour) → (tenant_id, region, sku, hour); (3) NEW-P0-3 TLA+ `MaxConcurrentEvents` CONSTANT NEW added to body L58-65 + bounded em EmitEvent; `Hours` time abstraction comment clarified (R4 round-1 caveat absorbed); `quota_grace_active` separate VARIABLE (R5 NEW-P0-1 fix: grace é flag, não 5º state value); `ReconcileLayer1` action NEW with drift computation (R4 round-1 P2-10 fix); (4) WI-003 PlanTier residuals §1 item 11 L247-250 + §6.1.13 L548 swept canonical 5-tuple; (5) WI-002 Gherkin L530 UPSERT WHERE clause text fixed (R4 P0-5 partial); (6) WI-001 risk register R-001/R-002 + post-mortem hooks severity CRITICAL→HIGH; SEV-1 → SEV-2 (R4 NEW-P1-2). **P1s addressed**: (a) WI-005 PercentValue cluster L74/L178/L328 align com 0..=200 DB CHECK (R4 NEW-P1-1 + R5 NEW-P1-3); (b) WI-006 sign-off `Finance + Legal + Architect` → `Finance + Compliance Officer + Architect` (Legal sprint-level only; R4 NEW-P1-4 / round-1 P1-6 finalmente fix); (c) WI-003 título L29 + WI-005 L727 + spec contract L244 CTRL-PRIV-002 mis-mapping → "data classification tags @classification=pii" + S-11 separated DSR; (d) WI-002 §6.1.11 late event reference time → cron_now (R5 NEW-P1-1); (e) WI-002 §14 corelink_time crate path canonical reference added (`crates/corelink-time/src/canonical.rs`); (f) WI-S10-003 Idempotency-Key 64-char internal cap rationale ADR documented em line 367; (g) WI-S10-007 ADR-S10-007-tla-substitutes-proptest justifying property test exemption + lightweight sanity props NEW; (h) WI-S10-006 "Mfa" → "MFA" capitalization (30 occurrences); (i) WI-004 Layer 2/3 narrative aggregation semantics clarified (collapsed across regions for Stripe scope; per-region drift preservada via Layer 1). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |
| 1.4.0 | 2026-05-03 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S10-005 SEAL; no per-WI codex per 2026-04-30 protocol — sprint-close Sonnet review covers) | **WI-S10-005 IMPL SEALED** — `crates/corelink-quota-fsm/` ships the canonical 5-state quota state-machine pure-logic skeleton per `trait-abstraction-defer` charter pattern. State taxonomy `WithinPlan / SoftWarning80pct / SoftWarning95pct / OverQuota100pct / SuspendedForNonPayment` (`#[non_exhaustive]` 5-element enum). Transition taxonomy `NoChange / TransitionedTo80pct / TransitionedTo95pct / TransitionedTo100pct / Suspended / Reinstated` (`#[non_exhaustive]` 6-element enum). Audit taxonomy `corelink.billing_quota.{state_changed, overage_telemetry_recorded, suspended, reinstated}` (4-event `#[non_exhaustive]`). **Email send DEFERRED to S-13** per ADR-0020 FROZEN — this WI emits the `corelink.billing_quota.overage_telemetry_recorded` audit ONLY at the 80pct + 95pct entries (the canonical telemetry contract that S-13 admin/notifications consumer subscribes to + dispatches the customer-facing email + in-app notification). Reinstate authorization gate (`billing_admin` role per CTRL-AUTHZ-001 + CTRL-AUTHZ-002) lives at the production wiring's Tower middleware, NOT here — this crate ships the state-machine contract; the role enforcement is the caller's responsibility per the trait surface contract. **3-invoice-failure suspension threshold canonical** (default per WI brief + sprint contract §15 R-009 abuse detection inheritance) — bumping the per-tenant counter on every Stripe `invoice.payment_failed` webhook delivery; threshold breach lands the terminal `SuspendedForNonPayment` arm. **Idempotent re-fire** at every arm (per WI brief: "re-firing same state transition is no-op"); same utilization snapshot → `NoChange` arm → no audit row, no store UPSERT. **Suspension dominates utilization**: a tenant in the terminal arm is a no-op for `evaluate_utilization()` until operator-driven `reinstate()` clears the per-tenant counter to 0 + flips to the utilization-derived bucket. Five modules: `event` (5-state QuotaState + 6-element QuotaTransition + UtilizationPct `[0, 200]`-bounded wrapper + InvoiceFailureCount saturating-u32 counter + QuotaFsmConfig with canonical ladder constants `SOFT_WARNING_80PCT_THRESHOLD = 80.0` / `SOFT_WARNING_95PCT_THRESHOLD = 95.0` / `OVER_QUOTA_100PCT_THRESHOLD = 100.0` / `SUSPENSION_INVOICE_FAILURE_THRESHOLD = 3` per sprint contract §5.5 R-S10-10 + WI brief; pure `utilization_bucket` mapper); `audit` (QuotaAuditEventType `#[non_exhaustive]` 4-event taxonomy + QuotaAuditSink trait + InMemoryQuotaAuditSink + FailingQuotaAuditSink + audit_event_for_transition + transition_emits_overage_telemetry canonical mappings fail-CLOSED Lote 10.6bis); `store` (QuotaFsmStateRow + QuotaFsmStore trait + InMemoryQuotaFsmStore `BTreeMap<Uuid, QuotaFsmStateRow>`-backed UPSERT-safe ledger + `(tenant_id)` UNIQUE PK INV-AVAIL-ISOLATION storage layer enforcement + FailingQuotaFsmStore); `fsm` (QuotaStateMachine trait + InMemoryQuotaStateMachine orchestrator: per-instance `Arc<Mutex<()>>` F-001 closure mirroring S-07 `corelink-quota::check.rs` DO-actor model byte-for-byte → audit BEFORE store UPSERT on every state-mutating arm → telemetry-only `OverageTelemetryRecorded` sibling at 80pct + 95pct entries → terminal `Suspended` arm at the canonical 3rd invoice-failure → operator-driven `Reinstated` arm clears the counter to 0); `error` (QuotaFsmError + QuotaFsmAuditSinkError + QuotaFsmStoreError canonical `#[non_exhaustive]` taxonomies). Migration `migrations/d1/0020_quota_fsm_state.sql` ships canonical `quota_fsm_state` PRIMARY KEY (tenant_id) UNIQUE [INV-AVAIL-ISOLATION storage layer; per-tenant scoping] + 5-element current_state CHECK constraint + invoice_failure_count NON-NEGATIVE CHECK + UUIDv7 hyphenated 36-char tenant_id length CHECK + 2 indexes + 4 inline CHECK constraints. Tests: 64 inline unit + 13 integration (9 property tests at 10k iter PR-gate via `PROPTEST_CASES` env-var read at runtime: `prop_state_transitions_canonical` + `prop_idempotent_transition_no_change` + `prop_80pct_boundary_telemetry_only` + `prop_95pct_boundary` + `prop_100pct_writes_429` + `prop_3_invoice_failures_suspends` + `prop_reinstate_clears_suspension` + `prop_tenant_isolation` + `prop_audit_emit_per_decision_arm` + 4 sanity / canonical-surface pinning). Quality gates green: `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings`, `cargo test -p corelink-quota-fsm --all-targets` (77/77), `python3 scripts/validate_specs.py` (280/286), `python3 scripts/check_migrations_additive.py` (20 files OK). Charter compliance: zero unsafe / unwrap / expect / panic / indexing in lib code; per-instance `Arc<Mutex<()>>` F-001 closure mirroring S-07 DO-actor model; `#[non_exhaustive]` on every public enum (QuotaState 5-element / QuotaTransition 6-element / QuotaAuditEventType 4-event / QuotaFsmError / QuotaFsmAuditSinkError / QuotaFsmStoreError / DriftHistoryInsertOutcome equivalent storage outcomes); wasm32-clean (no tokio in src/); audit fail-CLOSED envelope BEFORE state mutation on every state-mutating arm (`state_changed` + `overage_telemetry_recorded` BEFORE store UPSERT on utilization transitions; `suspended` BEFORE store UPSERT on terminal-arm; `reinstated` BEFORE store UPSERT on operator-driven reinstatement); ChaCha20Rng PRNG pinned for randomized fixtures; typed `QuotaTransition` payload (NOT `serde_json::Value` — Lote 10.9-quinquies NEW-P0-2 absorption). Production wiring deferred to WI-S10-007 PRR ship gate per `trait-abstraction-defer` charter pattern: `QuotaFsmDO-<tenant>` per-tenant Cloudflare DO actor model / real D1 `quota_fsm_state` row UPSERT / Stripe webhook adapter integration (the WI-S10-003 `WebhookEvent::InvoiceFailed` arm dispatches into `record_invoice_failure`) / S-13 admin/notifications consumer subscribing to the canonical audit chain / Tower middleware enforcing `billing_admin` role at the reinstatement endpoint per CTRL-AUTHZ-001 + CTRL-AUTHZ-002 / `corelink_time::next_month_first_utc_midnight()` boundary primitive (period-reset cron) / hot-path Tower middleware reading `OverQuota100pct` → `429 + X-RateLimit-Layer: quota + Retry-After` per S-08 alignment / hot-path reading `SuspendedForNonPayment` → `403 forbidden`. |
| 1.3.0 | 2026-04-26 | Gustavo (Lote 10.10-sextus) | P0+P1 remediation pós-R4 + R5 round-3 audits (R4 8.0/10, R5 9.1/10 CONDITIONAL SEAL). 2 P0s + 8 P1s residuais addressed. **2 P0s**: (1) R4 NEW-P0-1 severity cascade incompleto — quaters só varreu WI-001; sextus completou WI-002 (L745-746 post-mortem + L786 R-001) + WI-003 (L869-870) + WI-007 (L743 + L785 R-005) — todos `INV-BILLING-NO-LOSS/NO-DUP` violation references → HIGH-severity post-mortem + SEV-2 alert (não SEV-1). (2) R4 NEW-P0-2 WI-S10-004 Layer 1 ainda 3-tuple `(tenant, sku, hour)` → 4-tuple `(tenant, region, sku, hour)` em título L41 + trait doc L67 + narrative L217 + Gherkin L521 + sprint contract §5.2 R-S10-4 line 86 + CAP-BILLING-002 line 65 — mascarava multi-region drift via cross-region offsets (financial-grade integrity restored). **8 P1s**: (a) R5 P1-QUIN-1 WI-002 L538 + step 5 Gherkin/narrative `hour_window.start_ts` → `cron_now`; (b) R5 P1-QUIN-2 WI-001 L210 §1 item 9 idempotency key long-form → 35-char canonical; (c) R4 P1-1 WI-003 título L41 + ST-010 sub-task CTRL-PRIV-002 pseudonymization → "data classification tags @classification=pii" + S-11 separated DSR; (d) R4 P1-2 WI-006 §29 D+3 review checkpoint Legal sign-off → sprint-level only (NÃO 3-of-3 per-replay; matched §6.1.6 fix); (e) R4 P1-3 spec contract metadata bumped v1.1.0/sota-v1.1/2026-04-24 → v1.3.0/sota-v1.3/2026-04-26; (f) R4 P1-4 + R5 P1-QUIN-3 TLA+ block: `BillingPeriods` + `RequestIds` agora declared CONSTANTS; helper operators (`Abs`, `SumCounters`, `SumLineItems`, `ComputeStripeInvoiceId`, `StripeIdParts`, `EventAccountedInInvoice`) documented as separate INSTANCE module; (g) R5 P1-QUIN-4 INV_BILLING_NO_LOSS retry_queue Sequence membership bug fixed via `RetrySet == {retry_queue[i] : i \in DOMAIN retry_queue}` Range conversion (semantic correctness); (h) R4 P1-5 `compute_counter_digest()` doc explicitly confirms region IS in canonical_json input (cross-region tamper detection guaranteed); (i) R4 P1-6 §14.s10.x.8 + R-010/R-011 risk register stale "INV §3.X TBD pre-merge" → concrete §3.9 L136/137 + §3.12 L166/167 verified Lote 10.10bis (residual após mitigação LOW). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |

## 32. Anti-patterns evitados

- ❌ serde_json::Value em QuotaTransitionData (Lote 10.9-quinquies NEW-P0-2; typed canonical); ❌ Notification spam (24h cooldown + idempotent transitions); ❌ Grace > 14d (sprint contract §19 waiver max; CHECK constraint); ❌ Real-time per-second quota check (hourly canonical sprint contract §10); ❌ Fail-CLOSED at hot path is_hard_blocked() (cascade to customer 5xx; fail-OPEN canonical Lote 10.6bis split-tier); ❌ Fail-OPEN at quota state transition (state integrity > availability; fail-CLOSED canonical); ❌ Header X-RateLimit-Layer missing (S-08 alignment mandatory); ❌ ad-hoc `now() + Duration::days(30)` (corelink_time::next_month_first_utc_midnight canonical Lote 10.8bis P0-D); ❌ Per-feature quota expansion beyond 5 SKU canonical (sprint contract §10); ❌ Plan tier expansion beyond 5 canonical (Lote 10.7bis P0-7); ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3); ❌ TenantCtx bypass; ❌ Skip audit event emission on transition (CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + INV-AUDIT-APPEND-ONLY); ❌ Raw email em audit chain (PII wrapper Serialize impl Redact-wraps); ❌ Notification ack timing leak (constant-time comparison não aplica; informational metric only).

---

**Fim WI-S10-005.** Próximo: WI-S10-006 (Replay forensic endpoint + role protection + audit trail).
