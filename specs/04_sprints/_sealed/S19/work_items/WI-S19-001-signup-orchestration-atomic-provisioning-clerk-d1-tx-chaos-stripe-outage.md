---
id: "WI-S19-001"
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
tags: ["wi", "s19", "onboarding", "signup-orchestration", "atomic-provisioning", "clerk", "d1-transaction", "chaos-stripe-outage", "inv-onboard-atomic-provisioning", "high-risk"]
---

# WI-S19-001 — Signup Orchestration Backend (Clerk Email Verify Webhook Handler + Tenant Provisioning Atomic Single D1 Transaction com Tenant + tenant_metadata + usage_counter + dpa_acceptance_pending + stripe_customer_id_placeholder + first_pat Auto-Generated 90d Expiry Shown Only-Once + Region Pinning per `corelink_locale` Cookie Lote 10.16 Canonical) + Rollback Logic on Clerk Failure ou D1 Tx Failure ou Stripe Outage + Chaos Test Stripe Outage During Signup → Tenant Rollback Consistent (No Orphan Tenant + No Inconsistent Billing) + INV-ONBOARD-ATOMIC-PROVISIONING HIGH §3.12 Ratificada via Property Test 10k Atomicity 0 Violations + PAT-CORRELATION-ID-001 Propagation Across Clerk Webhook + D1 Tx + Stripe Customer Create + First PAT Issuance + Audit Chain Integrity Per Spec Contract §5.1 R-S19-1..R-S19-3

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-19](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S19-001 |
| Título | Signup orchestration backend Clerk webhook + tenant atomic provisioning D1 single tx + first PAT 90d + region pinning cookie + rollback chaos Stripe outage + INV-ONBOARD-ATOMIC-PROVISIONING ratificada |
| Sprint | S-19 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-009 (signup atomicity = customer-facing legal contract baseline; orphan tenant = breach + customer trust loss) |

## 1. Intent

Signup orchestration é o **business logic foundation** do customer onboarding S-19. Sem atomicity rigorosa, partial state em produção = orphan tenant (provisioning iniciado mas DPA pending forever; billing inconsistent; user can't access; support ticket queue grows) ou inconsistent billing (Stripe customer criado mas tenant não existe; reconciliation manual required). Este WI implementa: Clerk email verify webhook handler triggering atomic provisioning em single D1 transaction (tenant + tenant_metadata + usage_counter + dpa_acceptance_pending=true + stripe_customer_id_placeholder=null), first PAT auto-generated com scope read-write 90d expiry shown only-once, region pinned per `corelink_locale` cookie (US default; EU detected via locale; Lote 10.16 canonical fix), rollback logic on Clerk failure ou D1 tx failure ou Stripe outage chaos, e ratifica **INV-ONBOARD-ATOMIC-PROVISIONING** (HIGH §3.12 nova) via property test 10k atomicity verifying 0 orphan tenants em adversarial scenarios.

```rust
// File: crates/corelink-onboarding/src/signup_orchestrator.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;

#[async_trait]
pub trait SignupOrchestrator: Send + Sync {
    /// POST /webhooks/clerk/email-verified — Clerk webhook handler triggered after user email verified.
    /// Atomic provisioning em single D1 transaction; rollback all on any failure.
    /// PAT-CORRELATION-ID-001 propagated.
    /// INV-ONBOARD-ATOMIC-PROVISIONING enforced via TX_BEGIN ... TX_COMMIT or TX_ROLLBACK ALL.
    async fn provision_tenant_atomic(
        &self,
        clerk_event: ClerkEmailVerifiedEvent,
        correlation_id: CorrelationId,
        rendered_locale: Bcp47Locale,                    // from corelink_locale cookie pós Lote 10.16
    ) -> Result<TenantProvisionedReceipt, SignupOrchestratorError>;

    /// Rollback hook called on chaos Stripe outage ou D1 failure.
    /// Returns RollbackReceipt; verifies no orphan tenant via integrity check.
    async fn rollback_provisioning(
        &self,
        partial_state: PartialProvisioningState,
        reason: RollbackReason,                          // stripe_outage|d1_failure|clerk_failure
    ) -> Result<RollbackReceipt, SignupOrchestratorError>;
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Customer onboarding signup é o **single most expensive customer-acquisition step** em CoreLink — bug aqui = customer trust permanently lost (impossible to re-acquire prospect after signup failure; reputation damage propagated via word-of-mouth + reviews; competitive disadvantage permanent). Spec contract S-19 §5.1 R-S19-1 + R-S19-2 + R-S19-3 estabelecem 3 hard requirements: (a) signup orchestration (Clerk email verify → tenant provisioning atomic em D1 → first PAT auto + region pin); (b) atomicity (tenant + DPA + Stripe customer ID em single tx; failure rollback all); (c) signup ≤ 3 min start-to-finish (5-dev sample weekly). Este WI entrega (a) + (b); WI-S19-002 entrega DPA click-through (referenced by atomic transaction); WI-S19-004 entrega Stripe Checkout integration (referenced by atomic transaction Stripe customer create); WI-S19-006 mede (c) via funnel duration histogram.

**Risk justification HIGH_RISK**:

- **FF-HR-009**: signup orchestration é foundation business logic do customer-facing legal contract; bug = legal exposure (customer billed sem DPA = LGPD/GDPR/CCPA breach; orphan tenant + inconsistent billing = contract challenge legitimate).
- **Reversibility**: rollback testado em chaos (Stripe outage durante signup → tenant rolls back consistently); production incident detected post-deploy = catastrophic (orphan tenants accumulate em D1; billing reconciliation manual; customer support overwhelmed; legal exposure compounds).
- **Blast radius**: signup é entrada principal do customer journey; bug afeta 100% novos signups (não bounded subset); cumulative impact por hora de outage = 10s-100s prospects lost permanently.

**Bugs catastróficos que este WI deve catch**:

- **Atomicity violation em D1**: tenant inserted mas tenant_metadata fail (FK constraint timing); transaction não rolls back; usage_counter sem tenant parent = orphan rows.
- **Clerk webhook race condition**: 2 simultaneous email_verified events para same user (retry logic); 2 tenants criados; only 1 first PAT issued; 1 tenant orphaned.
- **Stripe outage timing**: Stripe customer create succeeds + D1 commit fails; Stripe customer orphaned em Stripe (cobrado em factura mas tenant não existe em CoreLink); reconciliation manual.
- **Region pinning drift**: user com `corelink_locale=pt-BR` cookie + Accept-Language header `en-US`; signup pinned `enam` (wrong region) → INV-DATA-RESIDENCY violation downstream.
- **First PAT race**: first PAT issued + email não recebido pelo user (SES throttle); user tries re-signup; second PAT issued; first PAT orphaned em D1.
- **Correlation ID drop**: Clerk webhook event_id não propagated em D1 audit chain; debug impossible em production incident.

**Atacante adversarial scenarios validated**:

- **Replay Clerk webhook**: pentester captures webhook signature; tries replay 1h later; verify nonce + timestamp window rejects.
- **Race condition exploitation**: pentester triggers 100 concurrent email_verified events para same user; verify 1 tenant created (UNIQUE constraint user_email).
- **Stripe outage injection**: pentester throttles Stripe API to 0% availability durante signup window; verify rollback consistent + customer notified + retry queue.
- **D1 corruption injection**: pentester corrupts D1 schema mid-transaction; verify integrity check + rollback + alert SEV-2.
- **Region pinning drift**: pentester switches cookie mid-flow; verify region pin idempotent + audit emit if drift detected.

**Mitigation**: D1 transaction explicit BEGIN/COMMIT/ROLLBACK; UNIQUE constraint user_email; webhook signature verify + nonce + timestamp window 5min; saga pattern Stripe customer create reversible; PAT-CORRELATION-ID-001 propagation; property test 10k atomicity 0 violations; chaos test Stripe outage weekly em staging.

## 3. Customer Impact & Journey

**Persona 1 — Customer prospect signing up self-service**:
- Submit email → Clerk verify email → email link clicked → tenant provisioning ≤ 30s p99 → first PAT shown → CLI install command rendered → quickstart link.
- Failure path (Stripe outage): tenant rollback transparent; user sees "Service temporarily unavailable; please retry em 5 min"; email apology + retry link.
- Region pinning: cookie corelink_locale=pt-BR → tenant pinned `sam`; cookie en-US → `enam`; cookie es-419 → `enam` default ou `sam` per Geo IP (deferred S-14 region-aware routing).

**Persona 2 — Privacy Officer cliente / Compliance auditor**:
- Atomicity audit: D1 transaction logs show BEGIN/COMMIT/ROLLBACK chain; orphan tenant count alert > 0 (should never happen).
- Region pinning audit: `tenant.primary_region` field populated at signup; immutable post-signup (S-14 alignment).
- Clerk integration audit: webhook signature verify + nonce + timestamp window 5min; replay rejected.

**Persona 3 — Internal SRE on-call**:
- RB-FM-SIGNUP-FAILED stub em `specs/_runbooks/RB-FM-SIGNUP-FAILED.md` (created WI-S19-006); dry-run ≤ 5 min recovery.
- Correlation ID em logs across Clerk webhook → D1 tx → Stripe customer create → first PAT issuance.
- Atomicity rollback alert > 5/dia indicates Stripe outage ou D1 instability; SEV-2 escalation.

**SLA addendum**:
- Signup orchestration p99 ≤ 30s (Clerk webhook → tenant provisioned + first PAT issued).
- Atomicity rollback consistency 100% (zero orphan tenants em chaos drill weekly).
- Region pinning correctness 100% (cookie corelink_locale matches tenant.primary_region).

## 4. Capability Mapping

- **CAP-ONBOARD-001** (Self-service signup business logic) — IMPLEMENTA primary; Clerk webhook + atomic provisioning + first PAT + region pin.
- Trace: `_spec_contract.md §5.1 R-S19-1..R-S19-3` + `auth_model.md` (Clerk SSO + email verify) + `data_model.md §4.1` (PAT format S-03 hybrid HMAC + Argon2id) + `invariant_registry.md §3.12 INV-ONBOARD-ATOMIC-PROVISIONING` + `resilience_patterns.md PAT-CORRELATION-ID-001`.

## 5. Tipo

Feature WI; HIGH_RISK; FF-HR-009; foundation business logic.

## 6. Escopo

### 6.1 In-scope

1. **Clerk email verify webhook handler** em `crates/corelink-onboarding/src/clerk_webhook.rs`:
   - POST `/webhooks/clerk/email-verified` endpoint Cloudflare Worker.
   - Webhook signature verification via `Clerk-Signature` header (HMAC-SHA256 + nonce + timestamp window 5 min; replay rejected).
   - Event type filter: `user.email_verified` only; other events ignored (audit emit for unknown).
   - Correlation ID extracted from `Clerk-Event-Id` header; PAT-CORRELATION-ID-001 propagated downstream.
   - Idempotency: duplicate webhook (same `event_id`) returns 200 + audit emit `corelink.onboarding.webhook_duplicate`.

2. **Atomic tenant provisioning em single D1 transaction**:
   - D1 transaction explicit `BEGIN; ... COMMIT;` ou `ROLLBACK` on any failure.
   - Inserts em order:
     - `tenant` row (tenant_id UUID v7 + user_email_hash + primary_region + created_at + dpa_acceptance_pending=true).
     - `tenant_metadata` row (tenant_id FK + display_name + signup_source + correlation_id).
     - `usage_counter` row (tenant_id FK + cas_bytes_used=0 + ac_calls_used=0 + reset_at=now+1month).
     - `pat` row (tenant_id FK + pat_hash via PAT format S-03 hybrid HMAC + Argon2id + scope='read-write' + expires_at=now+90d + shown_only_once_token).
   - Verifies all FK constraints satisfied; failure any insert → ROLLBACK ALL; emit audit `corelink.onboarding.atomicity_rollback{reason}`.
   - **INV-ONBOARD-ATOMIC-PROVISIONING enforced**: zero orphan rows post-rollback verified via integrity check property test 10k.

3. **Region pinning per `corelink_locale` cookie** (Lote 10.16 canonical fix):
   - Cookie `corelink_locale` set by Next.js middleware (S-16) reflected em Clerk webhook payload (custom claims).
   - Region map: `pt-BR` → `sam`; `en-US` → `enam` (default); `es-419` → `enam` default ou `sam` per Geo IP (deferred S-14 region-aware routing).
   - `tenant.primary_region` field populated at signup; immutable post-signup (S-14 alignment INV-REGION-NO-CROSS-LEAK CRITICAL).
   - Fallback: cookie missing → `Accept-Language` header → default `enam`.

4. **First PAT auto-generation** com scope read-write + 90d expiry:
   - PAT format canonical S-03 (a) hybrid HMAC + Argon2id (NOT confused com DPA receipt JWT signing key per data_model.md §4.1).
   - Scope: `read-write` (allows CAS PUT + AC READ + DSR self-service).
   - Expiry: 90d from signup; rotation reminder email at 60d + 80d.
   - Shown only-once UI surface (S-16 R-S16-8 herdada); `shown_only_once_token` UUID v7 invalidated after first GET.
   - Stored hashed em `pat` row; raw PAT NEVER persisted plaintext.

5. **Rollback logic on failure** (Clerk webhook ou D1 tx ou Stripe outage):
   - Failure detected → `rollback_provisioning(PartialState, RollbackReason)` invoked.
   - Reverts all D1 inserts via `ROLLBACK` (transaction-level).
   - If Stripe customer was created (saga step) → invoke Stripe API to delete customer (idempotent; reuse S-10 saga pattern).
   - Emits audit `corelink.onboarding.atomicity_rollback{reason}` + customer notify email apology.
   - Retry queue: failed signups retried up to 3x exponential backoff; permanent failure → support ticket auto-created.

6. **Chaos test Stripe outage during signup**:
   - Synthetic test inject Stripe API throttle to 0% availability durante signup window (10 concurrent attempts).
   - Verify: tenant rollback consistent (zero orphan tenant em D1); zero Stripe customer orphaned (saga reverse compensation); customer notified.
   - Cadence: weekly em staging; alert > 5/dia rollback events em production (SEV-2).
   - Output: `specs/_audits/2026-XX-XX-chaos-stripe-outage-signup-s19.md`.

7. **Property test 10k atomicity** (`tests/onboarding_atomicity_proptest.rs`):
   - Generators: random Clerk webhook events (valid + malformed + replay + race condition).
   - Assertions: zero orphan tenants post-rollback; zero inconsistent billing; correlation_id propagated; FK constraints satisfied.
   - 10k iter PR + 100k nightly.
   - **INV-ONBOARD-ATOMIC-PROVISIONING ratificada** via property test green.

8. **PAT-CORRELATION-ID-001 propagation**:
   - Clerk webhook `event_id` → correlation_id em D1 audit chain → Stripe customer metadata + first PAT audit emit + first CAS PUT correlation header.
   - Debug + audit chain integrity preserved across signup flow.

### 6.2 Out-of-scope (deferred)

- DPA click-through 6-field consent UI surface (WI-S19-002 owns frontend rendering + JWT receipt issuance); **NOTE Lote 10.19 codex P0 canonical fix**: WI-S19-001 atomic D1 tx **DOES** include INSERT dpa_acceptance row (atomic com tenant + first PAT) — DPA acceptance é parte do atomic boundary; only the UI rendering + JWT receipt issuance (WI-S19-002) é out-of-scope; this clarifies INV-ONBOARD-ATOMIC-PROVISIONING canonical scope post-codex P0 fix.
- DPA versioning + re-acceptance flow (WI-S19-003).
- Tier selection + Stripe Checkout subscription activation (WI-S19-004; INV-ONBOARD-DPA-FIRST gate enforced em WI-S19-004; activation requires both DPA accepted AND stripe_customer_id linked successfully via saga compensation per Lote 10.19 codex P0 canonical).
- Enterprise inquiry form (WI-S19-005).
- Conversion funnel instrumentation + cohort dashboard (WI-S19-006).
- First-run experience UI surface (S-16 owns; this WI provides backend first PAT + correlation ID).
- Multi-DPO escalation workflow (single DPO em S-19).

## 7. Anti-Scope

- D1 transaction sem explicit BEGIN/COMMIT/ROLLBACK (atomicity violation).
- Webhook signature verify skipped (Clerk impersonation attack).
- Replay window > 5 min (replay attack window).
- First PAT shown twice (compliance gap; PAT exposure).
- Region pinning derived de Accept-Language header NÃO cookie (Lote 10.16 violation).
- Correlation ID drop em Clerk webhook → D1 tx → Stripe customer create (debug impossible).
- Synchronous Stripe customer create within D1 transaction (saga pattern required; Stripe outage = D1 lock contention).
- Property test < 10k iter (insufficient atomicity coverage).
- Skip chaos test Stripe outage weekly (operational readiness gap).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Signup orchestration backend atomic provisioning Clerk webhook D1 tx chaos Stripe outage

  Scenario: Clerk email verify webhook triggers atomic provisioning
    Given Clerk webhook signed event "user.email_verified" received
    When signature verified + nonce valid + timestamp within 5 min window
    Then D1 transaction BEGIN
    And tenant + tenant_metadata + usage_counter + first PAT inserted
    And D1 transaction COMMIT
    And first PAT response returned shown only-once token
    And correlation_id propagated em audit chain

  Scenario: Webhook signature invalid rejected
    Given Clerk webhook with malformed signature
    When handler validates
    Then 401 returned + audit emit "corelink.onboarding.webhook_invalid_signature"

  Scenario: Webhook replay attack rejected
    Given Clerk webhook event_id replayed 1h later
    When handler validates timestamp window 5 min
    Then 409 returned + audit emit "corelink.onboarding.webhook_replay_rejected"

  Scenario: Webhook duplicate idempotent
    Given same Clerk webhook event_id sent twice within 5 min window
    When handler dedup via event_id
    Then 200 returned (idempotent) + audit emit "corelink.onboarding.webhook_duplicate"

  Scenario: D1 transaction rollback on FK constraint failure
    Given tenant_metadata FK constraint violated mid-tx (synthesized)
    When handler ROLLBACK
    Then zero rows inserted (tenant + tenant_metadata + usage_counter + first PAT all reverted)
    And audit emit "corelink.onboarding.atomicity_rollback{reason='d1_failure'}"

  Scenario: Stripe outage during signup → rollback consistent (chaos test)
    Given Stripe API throttled 0% availability
    When 10 concurrent signup attempts triggered
    Then 10 tenants rolled back consistently (zero orphan tenant em D1)
    And zero Stripe customer orphaned (saga reverse compensation)
    And customers notified apology email + retry queue
    And audit emit per attempt "corelink.onboarding.atomicity_rollback{reason='stripe_outage'}"

  Scenario: Region pinning per corelink_locale cookie (Lote 10.16)
    Given Clerk webhook payload carries corelink_locale="pt-BR" custom claim
    When handler maps locale → region
    Then tenant.primary_region = "sam"
    And immutable post-signup (S-14 INV-REGION-NO-CROSS-LEAK alignment)

  Scenario: Region pinning fallback Accept-Language if cookie missing
    Given corelink_locale cookie missing + Accept-Language header "en-US"
    When handler fallback
    Then tenant.primary_region = "enam" (default)

  Scenario: First PAT auto-generated 90d expiry shown only-once
    Given tenant provisioned successfully
    When first PAT generated
    Then PAT format hybrid HMAC + Argon2id
    And scope = "read-write"
    And expires_at = now + 90d
    And shown_only_once_token UUID v7 issued
    And subsequent GET shown_only_once_token returns 410 Gone

  Scenario: Property test 10k atomicity 0 violations
    Given 10k random Clerk webhook events (valid + malformed + replay + race)
    When property test runs
    Then 0 orphan tenants
    And 0 inconsistent billing
    And 100% correlation_id propagated
    And 100% FK constraints satisfied
    And INV-ONBOARD-ATOMIC-PROVISIONING ratificada

  Scenario: Race condition 100 concurrent email_verified same user
    Given 100 concurrent webhook events for same user_email
    When handler processes
    Then 1 tenant created (UNIQUE constraint user_email)
    And 99 rejected with 409 conflict
    And audit emit per rejection

  Scenario: Correlation ID propagated across signup flow
    Given Clerk webhook event_id "evt_abc123"
    When tenant provisioned + Stripe customer created + first PAT issued
    Then D1 audit chain rows carry correlation_id="evt_abc123"
    And Stripe customer metadata.correlation_id="evt_abc123"
    And first PAT audit emit carries correlation_id="evt_abc123"
```

## 9. Design Decisions

### 9.1 Why D1 transaction explicit BEGIN/COMMIT/ROLLBACK (não autocommit)

- Atomicity hard requirement (INV-ONBOARD-ATOMIC-PROVISIONING).
- Autocommit per insert = 4 separate statements; partial failure = orphan rows.
- Explicit transaction = single rollback boundary; FK constraint violations rollback all.

### 9.2 Why saga pattern Stripe customer create (Lote 10.19 codex P0 canonical scope clarification — INV-ONBOARD-ATOMIC-PROVISIONING redefined)

- **INV-ONBOARD-ATOMIC-PROVISIONING canonical scope (Lote 10.19 codex P0 fix)**: invariant covers **tenant + DPA acceptance + first PAT** em single D1 transaction (atomic); **Stripe customer ID linkage NÃO faz parte do atomic boundary** — é eventually-consistent saga compensação pattern.
- Codex P0 finding: prior wording "tenant + DPA + Stripe customer ID em single tx with rollback-all" era contradictory porque Stripe API call ≤ 30s (network) blocking D1 transaction = lock contention + concurrent signups starved + impossible em prática.
- **Redefined canonical**: 
  - **Atomic phase (INV-ONBOARD-ATOMIC-PROVISIONING enforced)**: BEGIN IMMEDIATE → INSERT tenant + INSERT dpa_acceptance + INSERT first PAT → COMMIT (all-or-nothing).
  - **Eventually-consistent phase (saga PAT-SAGA-001 compensation)**: post-D1-commit, async background worker invokes Stripe customer.create(); on success → UPDATE tenant SET stripe_customer_id; on persistent failure (3 retries with exponential backoff) → emit `corelink.signup.stripe_link_failed` audit event + tenant degrades to `pending_billing_link` state (read-only mode, identical to DPA grace period degrade per WI-S19-003 PAT-DEGRADE-001) → user prompted via in-app banner to retry billing setup; INV-ONBOARD-DPA-FIRST still enforced (no subscription activation sem DPA + sem stripe_customer_id linked).
- **Failure window canonical**: D1 commits successfully (tenant + DPA + first PAT atomic); Stripe link fails persistently → tenant em `pending_billing_link` state (degrade read-only); NO partial billing inconsistency (subscription cannot activate sem stripe_customer_id present per INV-ONBOARD-DPA-FIRST gate em WI-S19-004).
- ADR-S19-001 documents this two-phase atomicity model + UX flow + compensation triggers.

### 9.3 Why region pinning from cookie corelink_locale (Lote 10.16 canonical)

- Accept-Language header drift: user em pt-BR mas browser Accept-Language=en-US (common em multi-locale users); cookie reflects active rendered locale após user switch via footer.
- S-16 WI-S16-001 sets cookie via Next.js middleware; S-16 WI-S16-003 consent capture uses cookie; S-19 region pinning aligns.
- **Lote 10.19 codex P1 canonical fix — Accept-Language fallback REJECTED para DPA/residency evidence**: locale must come exclusively from `corelink_locale` cookie (set by S-16 WI-S16-001 middleware). For first-visit (cookie not yet set), middleware sets cookie immediately based on Accept-Language detection BEFORE DPA flow renders; DPA capture always reads cookie. Accept-Language NÃO usado como direct fallback em DPA/region capture path (canonical Lote 10.16 alignment; breaks proof-of-informed if header diverges from rendered locale).

### 9.4 Why first PAT 90d expiry (não 30d ou 365d)

- 30d = excessive rotation friction; UX abandonment.
- 365d = long credential exposure window; rotation reminder noise low.
- 90d = balance; matches industry standard (AWS access keys 90d rotation cadence).

### 9.5 Why correlation_id propagation across Clerk webhook → D1 → Stripe → first PAT

- Debug em production incident impossible without correlation chain.
- PAT-CORRELATION-ID-001 canonical pattern.
- Audit chain integrity preserved (S-09 alignment).

### 9.6 ADR potencial?

- Não. Patterns reused (D1 transaction + Clerk webhook + saga + correlation_id standard). No novel architecture decision.

## 10. Completeness Criteria SOTA

- [ ] **10.s19.001.1** Clerk email verify webhook handler signature verify + nonce + timestamp window 5 min implemented (EVT-049).
- [ ] **10.s19.001.2** D1 transaction explicit BEGIN/COMMIT/ROLLBACK atomic provisioning (EVT-002).
- [ ] **10.s19.001.3** Tenant + tenant_metadata + usage_counter + first PAT inserted single tx (EVT-018).
- [ ] **10.s19.001.4** First PAT auto-generated 90d expiry shown only-once (EVT-018).
- [ ] **10.s19.001.5** Region pinning per `corelink_locale` cookie (Lote 10.16 canonical) (EVT-018).
- [ ] **10.s19.001.6** Rollback logic on Clerk failure ou D1 failure ou Stripe outage (EVT-023).
- [ ] **10.s19.001.7** Chaos test Stripe outage weekly em staging; rollback consistent 0 orphan tenant (EVT-023).
- [ ] **10.s19.001.8** Property test 10k atomicity 0 violations (EVT-002) — INV-ONBOARD-ATOMIC-PROVISIONING ratificada.
- [ ] **10.s19.001.9** PAT-CORRELATION-ID-001 propagated across Clerk → D1 → Stripe → first PAT (EVT-013).
- [ ] **10.s19.001.10** Webhook signature verify + nonce + timestamp window 5 min (replay rejected) (EVT-002).
- [ ] **10.s19.001.11** Idempotent webhook handling (duplicate event_id 200 returned).
- [ ] **10.s19.001.12** Métricas underscored Prometheus emitting em staging (5+ métricas) (EVT-013).

## 11. DoD

- [ ] Clerk webhook handler tested staging.
- [ ] D1 transaction atomicity verified property test 10k.
- [ ] Chaos test Stripe outage weekly executed.
- [ ] Region pinning correctness 100% (cookie + Accept-Language fallback).
- [ ] First PAT shown only-once token invalidated after first GET.
- [ ] Correlation ID propagated across signup flow.
- [ ] Tests: unit (webhook signature verify + D1 tx atomicity + region map + first PAT gen) + integration (E2E Clerk webhook → tenant provisioned → first PAT) + 4+ negative scenarios.
- [ ] INV-ONBOARD-ATOMIC-PROVISIONING ratificada.

## 12. Invariants Validated

- **INV-ONBOARD-ATOMIC-PROVISIONING** (HIGH — registry §3.12 nova): tenant + DPA + Stripe customer ID em single tx; failure rollback all. Property test 10k atomicity 0 violations + chaos test Stripe outage rollback consistent.
- **INV-DATA-RESIDENCY** (CRITICAL — registry §3.11 herdada S-14): region pinning per `corelink_locale` cookie immutable post-signup.
- **INV-AUTH-PII-ENCRYPTED** (HIGH — registry §3.X herdada S-03): user_email cipher BYTEA via pgcrypto.
- Não introduz INV nova além de INV-ONBOARD-ATOMIC-PROVISIONING (declarada em spec contract §8).

TLA+ alignment: registry §4.2 indica `onboarding_atomicity.tla` PLANNED S-19 covers `InvAtomicTx` + `InvCorrelationIdPropagation`; integration test cross-validates pending TLA spec implementation forward.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Signup orchestrator crate | `crates/corelink-onboarding/` | Rust |
| Clerk webhook handler | `crates/corelink-onboarding/src/clerk_webhook.rs` | Rust |
| Atomic provisioning logic | `crates/corelink-onboarding/src/signup_orchestrator.rs` | Rust |
| Rollback logic | `crates/corelink-onboarding/src/rollback.rs` | Rust |
| Region pin map | `crates/corelink-onboarding/src/region_map.rs` | Rust |
| D1 migration signup_atomic | `migrations/0XX_signup_atomic.sql` | SQL |
| Property test atomicity | `tests/onboarding_atomicity_proptest.rs` | Rust |
| Chaos test Stripe outage | `tests/chaos_stripe_outage_signup.rs` | Rust |
| Chaos test report | `specs/_audits/2026-XX-XX-chaos-stripe-outage-signup-s19.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s19.001.1** D1 transaction atomicity 100% (zero orphan rows post-rollback).
- **14.s19.001.2** Property test ≥ 10k iter PR + 100k iter nightly.
- **14.s19.001.3** Test coverage ≥ 90% (handler + tx + rollback + region map).
- **14.s19.001.4** SAST: cargo-audit + cargo-deny clean.
- **14.s19.001.5** Webhook signature verify constant-time HMAC compare.
- **14.s19.001.6** Chaos test cadence weekly em staging.
- **14.s19.001.7** Correlation ID propagation 100%.
- **14.s19.001.8** First PAT shown only-once enforced via shown_only_once_token UUID v7.
- **14.s19.001.9** Region pinning correctness 100% (cookie primary + Accept-Language fallback).

## 15. Chaos Experiments

1. **Stripe outage during signup**: throttle Stripe API to 0% availability; 10 concurrent signups; verify rollback consistent.
2. **D1 corruption mid-transaction**: synthesize D1 schema corruption; verify integrity check + rollback + alert SEV-2.
3. **Clerk webhook replay**: capture webhook signature; replay 1h later; verify rejected.
4. **Race condition 100 concurrent email_verified**: verify 1 tenant created (UNIQUE constraint).
5. **Region pinning drift**: switch cookie mid-flow; verify region pin idempotent.
6. **Network partition Clerk → CoreLink**: verify retry queue + customer notify.
7. **First PAT race**: 2 simultaneous webhook events for same user; verify 1 PAT issued.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (sprint S-19 §14):

- [ ] All Gherkin green.
- [ ] Property test 10k atomicity 0 violations.
- [ ] Chaos test Stripe outage weekly green.
- [ ] Region pinning correctness 100%.
- [ ] First PAT shown only-once enforced.
- [ ] Correlation ID propagation 100%.
- [ ] INV-ONBOARD-ATOMIC-PROVISIONING ratificada.
- [ ] Métricas DASH-ONBOARDING emitting.
- [ ] Cost regression gate: signup orchestration ≤ 5% overhead em signup path.
- [ ] 11 sign-offs canonical documented.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Clerk webhook handler signature verify + nonce + timestamp window | 3h |
| ST-002 | D1 schema migration signup_atomic + transaction logic | 4h |
| ST-003 | Atomic provisioning tenant + tenant_metadata + usage_counter + first PAT | 4h |
| ST-004 | Region pin map + cookie + Accept-Language fallback | 2h |
| ST-005 | Rollback logic on failure + saga pattern Stripe customer | 3h |
| ST-006 | Property test 10k atomicity | 3h |
| ST-007 | Chaos test Stripe outage weekly automation | 2h |
| ST-008 | Correlation ID propagation across signup flow | 2h |

**Total Optimistic**: ~23h. **PERT** (O=14h, M=22h, P=36h per spec contract §12): **23.0h**.

## 18. Dependencies

### Hard blockers
- S-03 SEALED (Clerk SSO + email verify foundation).
- S-09 SEALED (audit chain R2 + correlation_id propagation pattern).
- S-11 SEALED (DPA é consent type; consent ledger D1 schema reuse).
- S-13 SEALED (admin plane secret rotation Clerk webhook secret + Stripe API key).

### Soft blockers
- S-10 SEALED (Stripe customer create saga pattern reuse).
- S-14 SEALED (region pinning INV-REGION-NO-CROSS-LEAK alignment).

### Outbound
- WI-S19-002 (DPA click-through references atomic provisioning row dpa_acceptance_pending).
- WI-S19-004 (Stripe Checkout activation depends on tenant + first PAT).
- WI-S19-006 (closing PRR + property test aggregation + RB-FM-SIGNUP-FAILED stub).

## 19. Effort PERT

O: 14h, M: 22h, P: 36h → PERT **23.0h** (per spec contract §12; signup orchestration + atomicity + chaos test).

## 20. Time-boxing

**26h hard limit owner**. Chaos test Stripe outage **2h dedicated**. Se exceder: split em sub-WI (atomicity vs chaos test).

## 21. Observability

Métricas Prometheus snake_case underscored (cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado; NUNCA per-tenant labels):

- `corelink_onboarding_signup_orchestration_total{outcome, region, plan}` (outcome ∈ ok|webhook_invalid|d1_failure|stripe_outage|race_condition).
- `corelink_onboarding_signup_orchestration_duration_seconds_bucket{region, plan}` (histogram p99 ≤ 30s).
- `corelink_onboarding_atomicity_rollback_total{reason, plan}` (reason ∈ stripe_outage|d1_failure|clerk_failure; alert > 5/dia).
- `corelink_onboarding_first_pat_issued_total{region, plan}` (counter).
- `corelink_onboarding_region_pin_total{region, source, plan}` (source ∈ cookie|accept_language|default; alert if source=accept_language > 10% indicates cookie drift).
- `corelink_onboarding_webhook_signature_invalid_total{plan}` (alert > 5/dia indicates Clerk impersonation attempt).
- `corelink_onboarding_webhook_replay_rejected_total{plan}` (alert > 5/dia indicates replay attack attempt).

Dashboard DASH-ONBOARDING painel "Signup Orchestration" (4-panel: orchestration latency p99 + atomicity rollback rate + region pin distribution + webhook security events).

## 22. Cost Analysis

- D1 transaction overhead: ≤ 5ms p99 per signup (negligible).
- Clerk webhook: included em Clerk subscription (no incremental cost).
- Stripe customer create: 1 API call per signup; rate-limited; ≤ $0.001 per signup.
- Chaos test Stripe outage: ~$10/mês CI compute.
- Total: ~$10/mês incremental.

## 23. API Contract

- `POST /webhooks/clerk/email-verified` — Clerk webhook handler; signature verify + nonce + timestamp window 5 min; returns 200 (success) | 200 (idempotent) | 401 (signature invalid) | 409 (replay rejected).
- Internal: `SignupOrchestrator::provision_tenant_atomic` Rust trait.

## 24. Post-mortem Hooks

- Atomicity bug signup (orphan tenant em prod) → CRITICAL post-mortem + INV-ONBOARD-ATOMIC-PROVISIONING review.
- Chaos test Stripe outage rollback inconsistent → SEV-2 post-mortem + saga compensation review.
- Webhook signature verify bypass detected → CRITICAL post-mortem + Security incident.
- Region pinning drift > 10% sustained 7d → review cookie middleware + Accept-Language fallback logic.
- First PAT shown twice em prod → CRITICAL post-mortem + Privacy + PAT exposure review.
- Correlation ID drop > 5% sustained → review propagation logic + audit chain integrity.

## 25. Rollback / Recovery

Signup orchestration regression detected → revert via CF Workers rollback; D1 schema migration reversible (down script committed); existing tenants unaffected.

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: Clerk webhook signature verify HMAC-SHA256 + nonce + timestamp window 5 min; replay rejected.
- **Tampering**: D1 transaction atomicity prevents partial state injection; FK constraints enforce data integrity.
- **Repudiation**: correlation_id propagated across signup flow + audit chain S-09; forensic trail.
- **Information disclosure**: first PAT shown only-once token UUID v7 invalidated after first GET; PAT NEVER persisted plaintext (Argon2id hashed); user_email cipher BYTEA via pgcrypto.
- **DoS**: signup rate-limited ≤ 10/min per IP via Cloudflare; D1 lock contention bounded via saga pattern Stripe customer create.
- **Elevation of privilege**: signup creates tenant + initial admin user role; subsequent role changes require admin step-up (S-13).

**LINDDUN delta**:
- **Linkability**: tenant_id em audit é necessário (compliance); correlation_id intentional (forensic); region tier-labeled NÃO tenant-labeled.
- **Identifiability**: user_email_hash sha256 em audit (intentional CTRL-AUDIT-002); pgcrypto BYTEA em D1 row.
- **Non-repudiation**: correlation_id + audit chain + JWT receipt forensic trail.
- **Detectability**: webhook signature invalid alerted; replay rejected alerted; atomicity rollback alerted.
- **Disclosure**: PAT NEVER em logs (safeLog allowlist); webhook payload sanitized.
- **Unawareness**: customer notified per signup outcome (success email + apology email on rollback).
- **Non-compliance**: SOC 2 CC6.1 + LGPD Art. 7º + GDPR Art. 25 + WCAG 2.2 AA satisfied.

## 27. Knowledge Transfer

- Tech talk (1h): "CoreLink Signup Orchestration — Atomic D1 Transaction + Saga Stripe + Region Pin".
- Doc `docs/internal/signup-orchestration.md` — overview + Clerk webhook + atomicity invariant + region pin canonical.
- Onboarding test (3 questions): atomicity + region pin + correlation ID rationale.

## 28. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Architect (Privacy + Legal SME specialization) | _TBD_ | _pending_ |
| 4 | Privacy Officer | _TBD_ | _pending_ |
| 5 | Legal Counsel | _TBD_ | _pending_ |
| 6 | Engineer (S-19 lead) | _TBD_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ |
| 9 | SRE Lead | _TBD_ | _pending_ |
| 10 | Compliance Officer | _TBD_ | _pending_ |
| 11 | Sales lead | _TBD_ | _pending_ |

> HIGH_RISK lane (per framework §33.5.4.3 + ADR-0034 solo-tier waiver): 11 canonical sign-offs. Privacy/UX advisor folds into Privacy Officer. Crypto SME folds into Architect (precedent S-13/S-14).

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S19-001 (cycle 12.S19.0; signup orchestration backend + atomic provisioning + chaos Stripe outage + INV-ONBOARD-ATOMIC-PROVISIONING ratificada). |

## 30. Anti-patterns evitados

- D1 autocommit (atomicity violation).
- Webhook signature verify skipped (impersonation).
- Replay window > 5 min.
- First PAT shown twice.
- Region pin from Accept-Language NÃO cookie (Lote 10.16 violation).
- Correlation ID drop.
- Synchronous Stripe customer create within D1 tx (lock contention).
- Property test < 10k iter.
- Skip chaos test Stripe outage weekly.

---

**Fim WI-S19-001.**
