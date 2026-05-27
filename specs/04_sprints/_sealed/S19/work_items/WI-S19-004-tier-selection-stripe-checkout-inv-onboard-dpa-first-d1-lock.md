---
id: "WI-S19-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-009"]
parent: "S-19"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "AUTH-MODEL"
  - "PRIVACY-MODEL"
  - "SECURITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s19", "onboarding", "tier-selection", "stripe-checkout", "subscription-activation", "inv-onboard-dpa-first", "d1-lock", "property-test-10k", "high-risk"]
---

# WI-S19-004 — Tier Selection + Stripe Checkout Subscription Activation (S-10 Reuse) com 5 Tiers Visible (free/starter/team/pro/enterprise — Enterprise = "Contact Us" Routes to WI-S19-005 Form) + Stripe Checkout Redirect com tenant_id Mapped to Stripe Customer + Webhook Handler (S-10 Reuse) Atualiza Local Subscription State + INV-ONBOARD-DPA-FIRST HIGH §3.12 Canonical Ratificada (Subscription Activation **Requires** DPA Signed Primeiro; Race Condition Prevented via D1 Lock `SELECT dpa_signed_ts FROM tenant WHERE tenant_id = ? FOR UPDATE` + Transactional Check; Property Test 10k Concurrent Signup Attempts → 0 Customer Billed sem DPA + 0 Race Window) + Cost Regression Gate Stripe Activation ≤ 200ms p99 + Métricas Prometheus snake_case (stripe_activation_total{outcome, tier} + dpa_first_violation_total alert > 0)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-19](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S19-004 |
| Título | Tier selection + Stripe Checkout activation + INV-ONBOARD-DPA-FIRST D1 lock + property test 10k concurrent 0 race window |
| Sprint | S-19 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-009 (billing activation = customer-facing legal contract; race condition customer billed sem DPA = breach) |

## 1. Intent

Tier selection + Stripe Checkout subscription activation é o **billing foundation** do customer onboarding. Spec contract S-19 §5.3 R-S19-7 + R-S19-8 estabelecem: (a) tier picker UI integrado Stripe Checkout (S-10 reuse) com 5 tiers; (b) **atomicity invariant**: subscription activation requires DPA signed first; race condition prevented via D1 lock + INV-ONBOARD-DPA-FIRST. Este WI implementa: 5 tiers visible (free/starter/team/pro/enterprise — enterprise = "Contact us" routes to WI-S19-005); Stripe Checkout redirect com tenant_id mapped to Stripe customer; webhook handler S-10 reuse atualiza local subscription state; **INV-ONBOARD-DPA-FIRST canonical** ratificada via D1 lock `SELECT dpa_signed_ts FROM tenant WHERE tenant_id = ? FOR UPDATE` + transactional check; property test 10k concurrent signup attempts → 0 customer billed sem DPA.

```rust
// File: crates/corelink-onboarding-billing/src/stripe_activation.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;

#[async_trait]
pub trait StripeActivation: Send + Sync {
    /// POST /api/onboarding/tier-select — tier picker callback.
    /// INV-ONBOARD-DPA-FIRST enforced: D1 lock + transactional check; rejects 451 if DPA not signed.
    /// Free tier: instant activation (no Stripe Checkout).
    /// Paid tier (starter|team|pro): Stripe Checkout redirect.
    /// Enterprise tier: routes to WI-S19-005 inquiry form.
    async fn select_tier(
        &self,
        tenant_ctx: &TenantCtx,
        tier: TierKind,
    ) -> Result<TierSelectionReceipt, StripeActivationError>;

    /// POST /webhooks/stripe/checkout.session.completed — Stripe webhook S-10 reuse.
    /// Updates local subscription state atomically.
    async fn on_checkout_completed(
        &self,
        stripe_event: StripeCheckoutSessionCompletedEvent,
    ) -> Result<SubscriptionActivatedReceipt, StripeActivationError>;
}

/// 5 tiers canonical (per spec contract §5.3 R-S19-7).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TierKind {
    Free,           // Instant activation; no Stripe; rate-limited
    Starter,        // Paid; Stripe Checkout
    Team,           // Paid; Stripe Checkout
    Pro,            // Paid; Stripe Checkout
    Enterprise,     // "Contact us"; routes to inquiry form WI-S19-005
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

INV-ONBOARD-DPA-FIRST é **the most legal-load-bearing invariant** em S-19. Sem enforcement rigoroso, customer billed sem DPA signed = breach (LGPD/GDPR/CCPA enforcement risk + class action potential + breach notification mandatory + reputation damage permanent). Race condition é o único caminho realista para violação: usuário em concurrent flow (DPA acceptance em tab 1 + Stripe Checkout em tab 2; ou Stripe webhook arrives before DPA acceptance commits em D1; ou Stripe Checkout pre-completed via cached session).

Spec contract S-19 §5.3 R-S19-8 estabelece **atomicity invariant**: subscription activation requires DPA signed first; race condition prevented via D1 lock + INV-ONBOARD-DPA-FIRST. Este WI implementa via D1 transactional check com `SELECT dpa_signed_ts FROM tenant WHERE tenant_id = ? FOR UPDATE` (locks tenant row para duration of transaction; concurrent transactions wait); subscription activation rejected if `dpa_signed_ts IS NULL` com 451 + audit emit + customer notify.

**Risk justification HIGH_RISK**:

- **FF-HR-009**: subscription activation é customer-facing billing contract; race condition customer billed sem DPA = breach + legal exposure + class action risk.
- **Reversibility**: bug detected post-deploy (e.g., 1 customer billed sem DPA) = catastrophic (refund + Legal escalation + customer notification + retroactive DPA prompt + breach notification consideration).
- **Blast radius**: 100% paid signups; potentially thousands at GA scale.

**Bugs catastróficos que este WI deve catch**:

- **Race condition em concurrent flow**: tab 1 user accepts DPA at t1; tab 2 user starts Stripe Checkout at t1+1ms (DPA not yet committed em D1); checkout webhook arrives at t1+500ms (DPA committed em meanwhile); subscription activated successfully BUT race window present.
- **D1 lock not held**: SELECT FOR UPDATE syntax variation per D1 SQLite dialect; lock not actually held; concurrent transactions race; INV violation.
- **Stripe webhook idempotency bug**: same `checkout.session.completed` event delivered twice; subscription activated twice; double billing.
- **Free tier bypass**: free tier skip DPA check accidentally; user activates free tier sem DPA; INV violation (still applies to free tier because DPA is contract-wide).
- **Enterprise tier route bypass**: enterprise tier UI routes to WI-S19-005 inquiry form mas backend allows direct Stripe Checkout; race + customer billed sem DPA + enterprise pricing not enforced.
- **Stripe customer ID drift**: tenant.stripe_customer_id field not set; webhook arrives; subscription matched wrong tenant; cross-tenant billing.

**Atacante adversarial scenarios validated**:

- **Replay Stripe webhook**: pentester captures webhook + replays 1h later; verify Stripe-Signature + nonce + timestamp window 5 min reject.
- **Concurrent flow exploitation**: pentester triggers 100 concurrent tier-select + Stripe Checkout for same tenant pre-DPA; verify INV-ONBOARD-DPA-FIRST 0 violations.
- **Direct Stripe Checkout bypass**: pentester crafts Stripe Checkout URL directly + completes; webhook arrives; verify backend rejects 451 if DPA not signed.
- **Enterprise tier impersonation**: pentester selects enterprise tier mas crafts paid tier Stripe Checkout; verify route enforcement.
- **D1 lock bypass attempt**: pentester crafts SQL to bypass FOR UPDATE; verify SQL parameterized + no injection.

**Mitigation**: D1 transactional check `SELECT ... FOR UPDATE`; idempotency via Stripe `idempotency_key` + dedup in D1 audit chain; free tier ALSO requires DPA (no exception); enterprise tier route enforcement at backend (UI hint only); stripe_customer_id set atomic in single tx; property test 10k concurrent attempts; chaos test Stripe outage (WI-S19-001 reuse).

## 3. Customer Impact & Journey

**Persona 1 — Customer prospect tier selection**:
- 5 tiers visible: free (instant) | starter $19/mo | team $49/mo | pro $199/mo | enterprise "Contact us".
- Free tier: instant activation (no Stripe Checkout); rate-limited.
- Paid tier: Stripe Checkout redirect; pre-filled email + tenant metadata; tenant_id mapped to Stripe customer.
- Enterprise tier: routes to WI-S19-005 inquiry form (white-glove handoff).
- DPA-first enforced: tier select rejected with 451 if DPA not signed; user prompted to complete DPA first.

**Persona 2 — Privacy Officer cliente / Compliance auditor**:
- INV-ONBOARD-DPA-FIRST audit: D1 transactional check log; 0 customer billed sem DPA em production (alert > 0 = SEV-1).
- Stripe customer mapping audit: tenant_id ↔ stripe_customer_id 1:1 mapping enforced.

**Persona 3 — Internal Finance / Billing**:
- Stripe webhook handler S-10 reuse: subscription state synced atomic; reconciliation report daily.
- Cost regression gate: Stripe activation ≤ 200ms p99.

**SLA addendum**:
- Tier select latency p99 ≤ 200ms.
- Stripe Checkout redirect ≤ 500ms p99.
- Subscription activation atomic (zero double billing).
- INV-ONBOARD-DPA-FIRST 0 violations em property test 10k concurrent.

## 4. Capability Mapping

- **CAP-ONBOARD-003** (Tier selection + Stripe subscription) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §5.3 R-S19-7 + R-S19-8` + `S-10 Stripe webhook handler reuse` + `invariant_registry.md §3.12 INV-ONBOARD-DPA-FIRST HIGH` + `data_model.md` (tenant.stripe_customer_id field).

## 5. Tipo

Feature WI; HIGH_RISK; FF-HR-009; billing-load-bearing.

## 6. Escopo

### 6.1 In-scope

1. **Tier picker backend** em `crates/corelink-onboarding-billing/`:
   - 5 tiers canonical: free | starter | team | pro | enterprise.
   - POST `/api/onboarding/tier-select` handler.
   - Free tier: instant activation (no Stripe); rate-limited (S-08 reuse).
   - Paid tier (starter|team|pro): Stripe Checkout redirect URL generated.
   - Enterprise tier: routes to WI-S19-005 inquiry form (`/api/onboarding/enterprise-inquiry` redirect).

2. **Stripe Checkout redirect**:
   - Stripe Checkout session created with `customer_email = tenant.user_email` + `metadata.tenant_id` + `success_url` + `cancel_url`.
   - tenant.stripe_customer_id set atomic em D1 single tx (idempotent if already set).
   - Reuses S-10 Stripe Checkout integration.

3. **Stripe webhook handler** (S-10 reuse):
   - `POST /webhooks/stripe/checkout.session.completed` — subscription state update.
   - Idempotency via Stripe `idempotency_key` + dedup in D1 audit chain (event_id already-processed check).
   - Updates `tenant.subscription_state = active` + `tenant.tier = <selected>` + `tenant.subscription_started_at = now()`.

4. **INV-ONBOARD-DPA-FIRST canonical enforcement (Lote 10.19 codex P0 canonical fix — SQLite/D1 NÃO suporta SELECT...FOR UPDATE; usar BEGIN IMMEDIATE)**:
   - **Canonical primitive**: SQLite/D1 supports `BEGIN IMMEDIATE TRANSACTION` (writer lock acquired immediately at BEGIN, vs default DEFERRED which delays lock acquisition until first write); concurrent BEGIN IMMEDIATE em mesma DB serializa (segundo writer espera ou retorna SQLITE_BUSY). Plus **UNIQUE constraint** em `subscription_state` (only one active subscription per tenant) prevents double-activation race window.
   - D1 transactional check before subscription activation:
     ```sql
     BEGIN IMMEDIATE TRANSACTION;
     -- BEGIN IMMEDIATE acquires reserved lock atomicamente (SQLite/D1 canonical primitive
     -- per https://sqlite.org/lang_transaction.html); concurrent writers wait até COMMIT/ROLLBACK.
     SELECT dpa_signed_ts, stripe_customer_id, subscription_state FROM tenant WHERE tenant_id = ?;
     -- if dpa_signed_ts IS NULL → ROLLBACK + return 451 "dpa_not_signed".
     -- if stripe_customer_id IS NULL → ROLLBACK + return 503 "stripe_link_pending" (saga compensation
     -- per WI-S19-001 Lote 10.19 P0 fix; tenant em pending_billing_link state).
     -- if subscription_state = 'active' → ROLLBACK + return 409 "subscription_already_active" (idempotency
     -- via UNIQUE constraint on (tenant_id, subscription_state='active') partial index).
     -- else → UPDATE tenant SET subscription_state = 'active', tier = ?, subscription_started_at = now();
     COMMIT;
     ```
   - **D1 BEGIN IMMEDIATE lock held for duration of transaction**; concurrent transactions wait or get SQLITE_BUSY (retry com exponential backoff via Worker handler); UNIQUE partial index `CREATE UNIQUE INDEX idx_tenant_active_subscription ON tenant(tenant_id) WHERE subscription_state='active'` provides defense-in-depth contra race condition (no race window).
   - Property test 10k concurrent signup attempts → 0 customer billed sem DPA (verifies lock holds + UNIQUE constraint enforces).
   - Métrica `corelink_onboarding_dpa_first_violation_total{plan}` (alert > 0 = SEV-1; should never happen).
   - **NOTE codex P0**: prior version cited `SELECT ... FOR UPDATE` syntax which é PostgreSQL/MySQL primitive; SQLite/D1 official syntax (https://sqlite.org/syntax/select-stmt.html + https://developers.cloudflare.com/d1/sql-api/sql-statements/) NÃO suporta `FOR UPDATE` clause; canonical fix usa BEGIN IMMEDIATE + UNIQUE partial index.

5. **Free tier ALSO requires DPA** (no exception):
   - Even free tier requires DPA signed first; INV-ONBOARD-DPA-FIRST applies tenant-wide.
   - Rationale: DPA é contract-wide regardless of tier; free tier customer still subject to data processing terms.

6. **Enterprise tier route enforcement**:
   - UI tier picker shows enterprise = "Contact us" with route to WI-S19-005 inquiry form.
   - Backend rejects direct Stripe Checkout for enterprise tier with 422 "use_inquiry_form".
   - Métrica `corelink_onboarding_enterprise_route_bypass_attempt_total{plan}` (alert > 0).

7. **Stripe customer ID drift prevention**:
   - tenant.stripe_customer_id set atomic in single D1 tx.
   - Reconciliation report daily: query Stripe customers + match D1 tenants; alert mismatch.

8. **Property test 10k concurrent signup**:
   - Generators: random tier selection + concurrent DPA acceptance + concurrent Stripe webhook arrivals.
   - Assertions: zero customer billed sem DPA; zero double billing; zero Stripe customer drift.
   - 10k iter PR + 100k nightly.
   - **INV-ONBOARD-DPA-FIRST ratificada** via property test green.

9. **Cost regression gate**:
   - Stripe activation latency p99 ≤ 200ms.
   - Benchmark CI on each PR; auto-fail if p99 > 200ms.

### 6.2 Out-of-scope (deferred)

- Signup orchestration backend (WI-S19-001).
- DPA click-through (WI-S19-002).
- DPA versioning + re-acceptance (WI-S19-003).
- Enterprise inquiry form (WI-S19-005).
- Conversion funnel (WI-S19-006).
- Multi-currency support (USD only at GA; multi-currency pós-GA Q1).
- Coupon / promo code support (deferred pós-GA).
- Tier downgrade / upgrade flow (deferred WI-S20-* GA hardening).
- Refund automation (manual via Stripe dashboard at GA).

## 7. Anti-Scope

- Free tier skip DPA check (INV violation).
- Enterprise tier direct Stripe Checkout (route enforcement bypass).
- D1 lock not held (race window).
- Stripe webhook idempotency skipped (double billing).
- tenant.stripe_customer_id set non-atomic (drift).
- Property test < 10k iter (insufficient race coverage).
- Skip cost regression gate.

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Tier selection + Stripe Checkout + INV-ONBOARD-DPA-FIRST D1 lock

  Scenario: Tier picker shows 5 tiers visible
    Given user navigates /onboarding/tier-select
    Then tiers free | starter | team | pro | enterprise visible
    And enterprise = "Contact us" routes to inquiry form

  Scenario: Free tier instant activation com DPA signed
    Given DPA signed (tenant.dpa_signed_ts NOT NULL)
    When user selects free tier
    Then tenant.tier = free + subscription_state = active
    And no Stripe Checkout redirect
    And rate limit applied (S-08 reuse)

  Scenario: Free tier rejected if DPA not signed (INV-ONBOARD-DPA-FIRST)
    Given DPA not signed (tenant.dpa_signed_ts IS NULL)
    When user selects free tier
    Then 451 returned com error "dpa_not_signed"
    And audit emit "corelink.onboarding.dpa_first_violation_attempt"
    And user prompted to complete DPA first

  Scenario: Paid tier Stripe Checkout redirect com DPA signed
    Given DPA signed
    When user selects starter tier
    Then Stripe Checkout session created
    And success_url + cancel_url + metadata.tenant_id populated
    And tenant.stripe_customer_id set atomic in D1 tx
    And redirect URL returned

  Scenario: Paid tier rejected if DPA not signed
    Given DPA not signed
    When user selects starter tier
    Then 451 returned + audit emit + dpa_first_violation prevented

  Scenario: Enterprise tier routes to inquiry form
    Given user selects enterprise tier
    Then route to /api/onboarding/enterprise-inquiry (WI-S19-005)
    And no Stripe Checkout

  Scenario: Enterprise tier direct Stripe Checkout bypass rejected
    Given user crafts direct Stripe Checkout URL for enterprise tier
    When backend validates
    Then 422 "use_inquiry_form" returned
    And audit emit "corelink.onboarding.enterprise_route_bypass_attempt"

  Scenario: Stripe webhook checkout.session.completed updates state
    Given Stripe webhook signed event "checkout.session.completed"
    When signature verified + idempotency check
    Then subscription_state = active + tier updated + subscription_started_at = now()
    And audit emit "corelink.onboarding.stripe_subscription_activated"

  Scenario: Stripe webhook duplicate idempotent
    Given same Stripe webhook event_id sent twice
    Then idempotency dedup; subscription activated once
    And audit emit "corelink.onboarding.stripe_webhook_duplicate"

  Scenario: D1 lock SELECT FOR UPDATE held during transaction
    Given concurrent tier-select requests for same tenant
    When first transaction holds lock
    Then second transaction waits
    And no race window between DPA check + subscription activation

  Scenario: Property test 10k concurrent → 0 customer billed sem DPA
    Given 10k random concurrent tier-select + DPA acceptance + Stripe webhook arrivals
    When property test runs
    Then 0 customer billed sem DPA
    And 0 double billing
    And 0 Stripe customer drift
    And INV-ONBOARD-DPA-FIRST ratificada

  Scenario: Stripe webhook signature invalid rejected
    Given Stripe webhook with malformed signature
    When handler validates
    Then 401 returned + audit emit "corelink.onboarding.stripe_webhook_invalid_signature"

  Scenario: Stripe webhook replay rejected
    Given Stripe webhook event_id replayed 1h later
    When handler validates timestamp window
    Then 409 returned + audit emit "corelink.onboarding.stripe_webhook_replay_rejected"

  Scenario: Stripe customer ID atomic mapping
    Given tenant_id → stripe_customer_id 1:1 mapping
    When tier-select for paid tier
    Then tenant.stripe_customer_id set atomic
    And reconciliation report daily 0 mismatch

  Scenario: Cost regression gate Stripe activation ≤ 200ms p99
    Given benchmark CI run
    When Stripe Checkout redirect measured
    Then p99 ≤ 200ms
    And auto-fail PR if p99 > 200ms
```

## 9. Design Decisions

### 9.1 Why D1 SELECT FOR UPDATE (não optimistic concurrency)

- Optimistic concurrency = retry on conflict; INV-ONBOARD-DPA-FIRST violation possible em race window between SELECT + UPDATE.
- Pessimistic lock = no race window; concurrent transactions wait deterministic.
- D1 SQLite supports FOR UPDATE syntax; verified em SQLite documentation.

### 9.2 Why free tier ALSO requires DPA (no exception)

- DPA é contract-wide regardless of tier; free tier customer still subject to data processing terms (LGPD Art. 8º + GDPR Art. 28).
- Exception = legal exposure ("free tier customers exempt from DPA" not legally defensible).
- UX impact minimal: DPA already part of signup flow; free tier just skips Stripe Checkout.

### 9.3 Why enterprise tier route enforcement at backend (não UI only)

- UI hint = bypassable via direct Stripe Checkout URL crafted by sophisticated user.
- Backend rejection = defensible against bypass + audit trail.

### 9.4 Why property test 10k concurrent (não 1k)

- Race window detection requires high concurrency; 1k may miss edge cases.
- 10k = balance between coverage and CI runtime.
- 100k nightly = catches rare race scenarios.

### 9.5 Why Stripe customer ID atomic in D1 tx (não async)

- Async = drift window; webhook may arrive before D1 update; subscription matched wrong tenant.
- Atomic = consistency baseline.

### 9.6 Why cost regression gate 200ms p99

- User-perceived latency budget: tier selection should feel instant.
- Stripe API p99 ~150ms; D1 tx ~20ms; total target 200ms.
- Auto-fail PR enforces; prevents drift.

### 9.7 ADR potencial?

- Não. Patterns reused (D1 transaction + Stripe webhook S-10 reuse + property test). No novel architecture decision.

## 10. Completeness Criteria SOTA

- [ ] **10.s19.004.1** 5 tiers visible (free|starter|team|pro|enterprise) UI + backend.
- [ ] **10.s19.004.2** Free tier instant activation com DPA signed.
- [ ] **10.s19.004.3** Paid tier Stripe Checkout redirect (S-10 reuse).
- [ ] **10.s19.004.4** Enterprise tier routes to inquiry form (WI-S19-005).
- [ ] **10.s19.004.5** INV-ONBOARD-DPA-FIRST D1 lock + transactional check ratificada.
- [ ] **10.s19.004.6** Property test 10k concurrent → 0 customer billed sem DPA (EVT-002).
- [ ] **10.s19.004.7** Stripe webhook idempotency + signature verify + replay rejected.
- [ ] **10.s19.004.8** tenant.stripe_customer_id atomic mapping; reconciliation daily.
- [ ] **10.s19.004.9** Cost regression gate Stripe activation ≤ 200ms p99.
- [ ] **10.s19.004.10** Free tier ALSO requires DPA (no exception).
- [ ] **10.s19.004.11** Enterprise tier route enforcement at backend.
- [ ] **10.s19.004.12** Métricas Prometheus snake_case (4+ métricas).

## 11. DoD

- [ ] Tier selection tested staging.
- [ ] Stripe Checkout redirect verified.
- [ ] INV-ONBOARD-DPA-FIRST ratificada via property test 10k.
- [ ] Webhook idempotency verified.
- [ ] Cost regression gate green.
- [ ] Tests: unit (tier picker + D1 lock + webhook handler) + integration (E2E tier select → Stripe → activation) + 4+ negative scenarios.

## 12. Invariants Validated

- **INV-ONBOARD-DPA-FIRST** (HIGH — registry §3.12 nova): subscription activation requires DPA signed; race condition prevented via D1 lock + transactional check + property test 10k concurrent.
- **INV-ONBOARD-ATOMIC-PROVISIONING** (HIGH — registry §3.12 herdada WI-S19-001): tenant.stripe_customer_id atomic mapping.
- Não introduz INV nova além de INV-ONBOARD-DPA-FIRST (declarada em spec contract §8).

TLA+ alignment: registry §4.2 indica `onboarding_atomicity.tla` PLANNED S-19 covers `InvDpaFirstOrdering` + `InvAtomicTx`; integration test cross-validates pending TLA spec implementation forward.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Stripe activation crate | `crates/corelink-onboarding-billing/` | Rust |
| Tier picker handler | `crates/corelink-onboarding-billing/src/tier_picker.rs` | Rust |
| Stripe activation logic | `crates/corelink-onboarding-billing/src/stripe_activation.rs` | Rust |
| Stripe webhook handler | `crates/corelink-onboarding-billing/src/stripe_webhook.rs` | Rust |
| D1 lock + tx | `crates/corelink-onboarding-billing/src/d1_lock.rs` | Rust |
| Property test DPA-first | `tests/dpa_first_proptest.rs` | Rust |
| Reconciliation daily cron | `crates/corelink-onboarding-billing/src/reconciliation.rs` | Rust |

## 14. Quality Standards SOTA

- **14.s19.004.1** D1 lock SELECT FOR UPDATE held for duration of transaction.
- **14.s19.004.2** Property test ≥ 10k iter PR + 100k nightly.
- **14.s19.004.3** Test coverage ≥ 90% (tier picker + D1 lock + webhook).
- **14.s19.004.4** SAST: cargo-audit + cargo-deny clean.
- **14.s19.004.5** Stripe webhook signature verify constant-time HMAC compare.
- **14.s19.004.6** Cost regression gate Stripe activation ≤ 200ms p99.
- **14.s19.004.7** Reconciliation daily cron 0 mismatch sustained 30d.

## 15. Chaos Experiments

1. **Concurrent flow exploitation**: 100 concurrent tier-select + DPA + webhook arrivals; verify INV preserved.
2. **D1 lock bypass attempt**: pentester crafts SQL injection; verify parameterized + safe.
3. **Stripe webhook replay**: capture + replay 1h later; verify rejected.
4. **Stripe outage during tier-select**: throttle Stripe API; verify graceful error + retry queue.
5. **Stripe customer ID drift**: synthesize webhook for unknown stripe_customer_id; verify reconciliation alert.
6. **Free tier DPA bypass attempt**: pentester selects free tier sem DPA; verify rejected.
7. **Enterprise tier direct Checkout bypass**: pentester crafts URL; verify rejected.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (sprint S-19 §14):

- [ ] All Gherkin green.
- [ ] Property test 10k concurrent 0 violations — INV-ONBOARD-DPA-FIRST ratificada.
- [ ] Cost regression gate ≤ 200ms p99.
- [ ] Reconciliation 0 mismatch sustained 30d.
- [ ] 11 sign-offs canonical documented.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Tier picker handler 5 tiers + route logic | 3h |
| ST-002 | Stripe Checkout redirect (S-10 reuse) | 2h |
| ST-003 | Stripe webhook handler S-10 reuse + idempotency | 3h |
| ST-004 | D1 lock SELECT FOR UPDATE + transactional check | 3h |
| ST-005 | INV-ONBOARD-DPA-FIRST property test 10k concurrent | 4h |
| ST-006 | tenant.stripe_customer_id atomic + reconciliation | 2h |
| ST-007 | Cost regression gate benchmark CI | 2h |

**Total Optimistic**: ~19h. **PERT** (O=12h, M=18h, P=30h per spec contract §12): **19.0h**.

## 18. Dependencies

### Hard blockers
- WI-S19-001 SEALED (signup orchestration foundation; tenant atomicity).
- WI-S19-002 SEALED (DPA click-through; tenant.dpa_signed_ts populated).
- S-10 SEALED (Stripe webhook handler + Checkout integration foundation).

### Soft blockers
- S-13 SEALED (admin plane DO config para tier pricing override).
- S-08 SEALED (rate limit free tier).

### Outbound
- WI-S19-005 (enterprise tier route to inquiry form).
- WI-S19-006 (closing PRR + property test aggregation).

## 19. Effort PERT

O: 12h, M: 18h, P: 30h → PERT **19.0h** (per spec contract §12; tier + Stripe + D1 lock + property test).

## 20. Time-boxing

**22h hard limit owner**. Property test 10k **4h dedicated**. Se exceder: split em sub-WI (tier + Stripe vs property test).

## 21. Observability

Métricas Prometheus snake_case underscored:

- `corelink_onboarding_tier_select_total{tier, outcome, plan}` (outcome ∈ ok|dpa_not_signed|enterprise_route|stripe_outage).
- `corelink_onboarding_stripe_activation_total{tier, outcome, plan}` (outcome ∈ ok|webhook_invalid|idempotency_dedup|race_condition).
- `corelink_onboarding_stripe_activation_duration_seconds_bucket{tier, plan}` (histogram p99 ≤ 200ms).
- `corelink_onboarding_dpa_first_violation_total{plan}` (counter; alert > 0 = SEV-1; should never happen).
- `corelink_onboarding_enterprise_route_bypass_attempt_total{plan}` (alert > 0).
- `corelink_onboarding_stripe_customer_id_drift_total{plan}` (counter; reconciliation daily; alert > 0).

Dashboard DASH-ONBOARDING painel "Tier + Stripe Activation" (5-panel: tier select rate per tier + stripe activation latency + DPA-first violations + enterprise route bypass + customer ID drift).

## 22. Cost Analysis

- Stripe transaction fees: 2.9% + $0.30 per transaction (passed to customer).
- D1 lock overhead: ≤ 5ms p99 per tier-select.
- Reconciliation cron: ~$5/mês CI compute.
- Total: ~$5/mês incremental.

## 23. API Contract

- `POST /api/onboarding/tier-select` — tier picker callback; returns 200 (success + Stripe Checkout URL ou activation receipt) | 451 (dpa_not_signed) | 422 (use_inquiry_form for enterprise).
- `POST /webhooks/stripe/checkout.session.completed` — Stripe webhook S-10 reuse.
- Internal: `StripeActivation::select_tier` Rust trait.

## 24. Post-mortem Hooks

- DPA-first violation detected em prod (customer billed sem DPA) → CRITICAL post-mortem + Legal + breach notification consideration + customer notify + retroactive DPA prompt.
- Stripe webhook idempotency bug detected → SEV-2 + double billing review + refund.
- D1 lock not held detected → CRITICAL + race window analysis.
- Stripe customer ID drift > 0 → SEV-2 + reconciliation manual.
- Enterprise route bypass detected → SEV-2 + Security incident.
- Cost regression > 200ms p99 sustained → SEV-3 + performance review.

## 25. Rollback / Recovery

Tier selection regression → revert via CF Workers rollback; existing subscriptions retained em Stripe + D1; tenant tier reverts to free if subscription state corrupted.

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: Stripe webhook signature verify + nonce + timestamp window 5 min.
- **Tampering**: D1 transactional check prevents partial state injection.
- **Repudiation**: Stripe customer mapping + audit chain S-09 forensic trail.
- **Information disclosure**: Stripe Checkout URL contains tenant_id (intentional metadata; no PII).
- **DoS**: tier select rate-limited; Stripe Checkout rate respeitando Stripe API limits.
- **Elevation of privilege**: tier change requires admin step-up (S-13) post-onboarding.

**LINDDUN delta**:
- **Linkability**: tenant_id ↔ stripe_customer_id 1:1 mapping (intentional billing).
- **Identifiability**: tenant.user_email em Stripe customer (intentional billing).
- **Non-repudiation**: Stripe webhook + audit chain forensic trail.
- **Detectability**: DPA-first violation alerted; reconciliation drift alerted.
- **Disclosure**: Stripe API key never em logs.
- **Unawareness**: customer notified per tier activation success.
- **Non-compliance**: SOC 2 CC6.1 + LGPD Art. 7º + GDPR Art. 28 satisfied.

## 27. Knowledge Transfer

- Tech talk (1h): "CoreLink Tier Selection + Stripe Checkout — INV-ONBOARD-DPA-FIRST D1 Lock".
- Doc `docs/internal/tier-selection-stripe-activation.md`.
- Onboarding test (3 questions): D1 lock pessimistic + free tier DPA enforcement + enterprise route.

## 28. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Architect (Privacy + Legal SME) | _TBD_ | _pending_ |
| 4 | Privacy Officer | _TBD_ | _pending_ |
| 5 | Legal Counsel | _TBD_ | _pending_ |
| 6 | Engineer (S-19 lead) | _TBD_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ |
| 9 | SRE Lead | _TBD_ | _pending_ |
| 10 | Compliance Officer | _TBD_ | _pending_ |
| 11 | Sales lead | _TBD_ | _pending_ |

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S19-004 (cycle 12.S19.0; tier selection + Stripe Checkout + INV-ONBOARD-DPA-FIRST D1 lock + property test 10k concurrent ratificada). |

## 30. Anti-patterns evitados

- Free tier skip DPA check (INV violation).
- Enterprise tier direct Stripe Checkout.
- D1 lock not held.
- Stripe webhook idempotency skipped.
- tenant.stripe_customer_id non-atomic.
- Property test < 10k iter.
- Skip cost regression gate.

---

**Fim WI-S19-004.**
