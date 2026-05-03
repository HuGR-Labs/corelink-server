---
id: "WI-S10-003"
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
  - "PRIVACY-MODEL"
tags: ["wi", "s10", "stripe", "billing", "idempotency-key", "webhook", "hmac-sha256", "neon-postgres", "high-risk"]
---

# WI-S10-003 — Crate `corelink-billing` Stripe Adapter + Idempotency-Key + Webhook Handler HMAC-SHA256 + `stripe_event_log` Audit Trail (`crates/corelink-billing/`; Stripe SDK v0.31+ wrapped behind canonical `StripeClient` trait; **Idempotency-Key SEMPRE present** on every Stripe POST per RFC + sprint contract §5.3 R-S10-6 [`corelink-{tenant_id}-{event_hash}` derivation 122-bit UUID v4 entropy]; **test mode pinning** via `CORELINK_STRIPE_MODE=test|live` Worker secret separation [CI sempre test; live key gated by environment promotion]; webhook handler `POST /v1/webhooks/stripe` validates `Stripe-Signature` HMAC-SHA256 com 5min replay window + `(stripe_event_id)` UNIQUE em D1 audit log; Neon Postgres schemas: `plan` (5 canonical tiers per `data_model.md §1` line 68: free/solo/team/business/enterprise; Lote 10.7bis P0-7 inheritance) + `subscription` + `invoice_line_item` + `stripe_event_log` + `customer_billing_profile`; counter aggregator (WI-S10-002) → invoice_line_item via monthly cron rollup using `corelink_time::next_month_first_utc_midnight()` canonical Lote 10.8bis P0-D; PII em customer name/email encrypted at rest FIPS 140-3 [BYOK from S-04 inheritance]; LINDDUN compliance via CTRL-PRIV-002 (data classification tags @classification=pii; privacy_model.md L209) + S-11 DSR pseudonymization procedure separada (control TBD); Stripe API retry com PAT-BACKOFF-001 max 5 retries + jitter; PAT-QUEUE-EVENTS-001 fallback em outage; FM-151 Stripe outage runbook RB-FM-151 dry-run prerequisite; **payload separation rigorosa**: Rust types não vazam Stripe primitives via API public; `cargo-deny` policy proibe stripe crate em outros workspace members)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-10](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S10-003 |
| Título | Crate `corelink-billing` Stripe adapter (sprint contract §5.3 R-S10-6 + R-S10-7); Stripe SDK v0.31+ wrapped behind StripeClient trait; **Idempotency-Key SEMPRE** every Stripe POST `corelink-{tenant_id}-{event_hash}` (RFC 7240 + Stripe API guide); test mode pinning `CORELINK_STRIPE_MODE=test|live` Worker secret env separation (CI test only; live key gated environment promotion ADR-0034 + waiver Lote 10.4bis discipline); webhook handler `POST /v1/webhooks/stripe` Stripe-Signature HMAC-SHA256 verify + (stripe_event_id) UNIQUE D1 audit log dedup + 5min replay window canonical (sprint contract §15 R-007 risk mitigation); Neon Postgres schemas plan/subscription/invoice_line_item/stripe_event_log/customer_billing_profile; counter (WI-S10-002) → invoice rollup mensal cron `corelink_time::next_month_first_utc_midnight()` (Lote 10.8bis P0-D); PII customer name/email FIPS 140-3 encrypted at rest com BYOK S-04 inheritance + CTRL-PRIV-002 (data classification tags @classification=pii; privacy_model.md L209) + S-11 DSR pseudonymization procedure separada (control TBD); Stripe API retry PAT-BACKOFF-001 max 5 + jitter (sprint contract §14.s10.2); PAT-QUEUE-EVENTS-001 fallback em outage (FM-151); RB-FM-151 dry-run runbook prerequisite (sprint contract §6 DoD); 5 SKUs canonical inheritance from WI-S10-001/002 mapped to Stripe Price IDs catalog; cardinality budget bounded; 12 sign-offs HIGH_RISK (Finance + Legal + Privacy + Architect emphatic); typed `StripeWebhookEvent` enum (NOT serde_json::Value — Lote 10.9-quinquies NEW-P0-2 lesson); explicit `serde::Serialize` impl on PII wrappers (CustomerEmail, CustomerName) calling `Redact::redact()` for log emission |
| Sprint | S-10 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-BILLING-001 financial integrity Stripe API contract; bypass = revenue leak silent), FF-HR-009 (Stripe = customer contract direct; bug = customer dispute legal exposure + chargeback storm + PCI scope risk) |

## 1. Intent

`corelink-billing` é **the Stripe contract boundary** entre CoreLink internal counter aggregates (WI-S10-002 D1 `usage_counter`) e customer-facing Stripe invoice. Stripe API integration disciplina é **non-negotiable financial-grade**: Idempotency-Key SEMPRE prevents double-charge on retry; test mode pinning prevents accidental live charges from CI; webhook signature verify HMAC-SHA256 prevents replay attack; audit trail `stripe_event_log` UNIQUE constraint prevents duplicate webhook processing. Every Stripe primitive (PaymentIntent, Customer, Subscription, Invoice) is wrapped behind canonical Rust types — `cargo-deny` enforces stripe crate proibido em outros workspace members preventing leakage.

```rust
// File: crates/corelink-billing/src/lib.rs

#![forbid(unsafe_code)]
#![deny(clippy::pedantic)]

use stripe::Client as StripeSdkClient;

#[async_trait]
pub trait StripeClient: Send + Sync {
    /// Create or retrieve Stripe customer for tenant (idempotent).
    /// Idempotency-Key: corelink-{tenant_id}-customer-{event_hash}
    async fn ensure_customer(
        &self,
        tenant_id: TenantId,                            // S-03 TenantCtx inheritance
        billing_profile: &CustomerBillingProfile,       // PII fields use redact!-wrapped types
    ) -> Result<StripeCustomerId, BillingStripeError>;

    /// Create subscription (idempotent via tenant_id + plan_id).
    /// Idempotency-Key: corelink-{tenant_id}-sub-{plan_id}-{cycle_start}
    async fn create_subscription(
        &self,
        tenant_id: TenantId,
        plan: PlanTier,                                 // 5 canonical per data_model.md §1 line 68: free/solo/team/business/enterprise (Lote 10.7bis P0-7)
        billing_cycle_anchor: DateTime<Utc>,            // corelink_time::next_month_first_utc_midnight() canonical
    ) -> Result<StripeSubscriptionId, BillingStripeError>;

    /// Generate monthly invoice from counter aggregates (WI-S10-002).
    /// Idempotency-Key: corelink-{tenant_id}-invoice-{billing_period_yyyymm}
    /// FAIL-CLOSED: any error halts; staging buffer + queue retry (PAT-QUEUE-EVENTS-001).
    async fn generate_monthly_invoice(
        &self,
        tenant_id: TenantId,
        billing_period: BillingPeriod,                  // start/end UTC month-aligned
        line_items: Vec<InvoiceLineItem>,               // 5 canonical SKUs from counter aggregator
    ) -> Result<StripeInvoiceId, BillingStripeError>;

    /// Pull invoice from Stripe API for reconciliation Layer 3 (WI-S10-004).
    async fn fetch_invoice(
        &self,
        stripe_invoice_id: StripeInvoiceId,
    ) -> Result<StripeInvoiceSnapshot, BillingStripeError>;

    /// Process refund (Stripe webhook driven).
    /// Idempotency-Key: corelink-{tenant_id}-refund-{stripe_charge_id}
    async fn process_refund(
        &self,
        stripe_charge_id: StripeChargeId,
        reason: RefundReason,                           // typed enum NOT String
    ) -> Result<StripeRefundId, BillingStripeError>;
}

#[async_trait]
pub trait StripeWebhookHandler: Send + Sync {
    /// Verify Stripe-Signature HMAC-SHA256 + check (stripe_event_id) dedup.
    /// 5min replay window canonical (sprint contract §15 R-007 mitigation).
    async fn handle_webhook(
        &self,
        request: WebhookRequest,                        // body + Stripe-Signature header
    ) -> Result<WebhookOutcome, BillingStripeError>;
}

#[derive(strum::Display, strum::EnumIter, serde::Serialize)]
pub enum PlanTier {
    // 5 canonical tiers per data_model.md §1 line 68 + slo_catalog.md §3.1; Lote 10.7bis P0-7 inheritance.
    #[strum(serialize = "free")]
    Free,
    #[strum(serialize = "solo")]
    Solo,
    #[strum(serialize = "team")]
    Team,
    #[strum(serialize = "business")]
    Business,
    #[strum(serialize = "enterprise")]
    Enterprise,
}

/// 5 canonical Stripe webhook event types we handle (Lote 10.9-quinquies NEW-P0-2 lesson absorbed).
#[derive(serde::Deserialize)]
#[serde(tag = "type")]
pub enum StripeWebhookEvent {
    #[serde(rename = "invoice.paid")]
    InvoicePaid { data: InvoicePaidData },
    #[serde(rename = "invoice.payment_failed")]
    InvoicePaymentFailed { data: InvoicePaymentFailedData },
    #[serde(rename = "charge.refunded")]
    ChargeRefunded { data: ChargeRefundedData },
    #[serde(rename = "charge.dispute.created")]
    ChargeDisputeCreated { data: ChargeDisputeCreatedData },
    #[serde(rename = "customer.subscription.deleted")]
    CustomerSubscriptionDeleted { data: SubscriptionDeletedData },
}

/// PII wrapper types with explicit serde::Serialize impl calling Redact::redact()
/// (Lote 10.9-quinquies NEW-P0-2 critical lesson absorbed from WI-S09-002).
pub struct CustomerEmail(String);
impl serde::Serialize for CustomerEmail {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        Redact::redact(&self.0).serialize(ser)          // "j***@example.com" form
    }
}

pub struct CustomerName(String);
impl serde::Serialize for CustomerName {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        Redact::redact(&self.0).serialize(ser)          // "J*** D**" form
    }
}

#[derive(thiserror::Error, Debug)]
pub enum BillingStripeError {
    #[error("Stripe API failure (status {status}; idempotency_key={key}): {message}")]
    StripeApiFailure { status: u16, key: String, message: String },

    #[error("Idempotency-Key collision detected (CRITICAL; UUID entropy review): {0}")]
    IdempotencyKeyCollision(String),

    #[error("Webhook signature verification failed (HMAC-SHA256 mismatch): {0}")]
    WebhookSignatureFailed(String),

    #[error("Webhook replay attempt detected (timestamp > 5min old): event_ts={event_ts} now={now}")]
    WebhookReplayAttempt { event_ts: String, now: String },

    #[error("Webhook duplicate processed (stripe_event_id={event_id} already em audit log)")]
    WebhookDuplicate { event_id: String },

    #[error("test mode mismatch (CORELINK_STRIPE_MODE={mode}; expected={expected})")]
    TestModeMismatch { mode: String, expected: String },

    #[error("Idempotency-Key length violation (max 255 chars Stripe; got {0})")]
    IdempotencyKeyTooLong(usize),

    #[error("PII detected em event log payload (CTRL-PRIV-001 violation): {0}")]
    PiiInEventLog(String),

    #[error("Postgres transaction failed: {0}")]
    PostgresFailed(String),

    #[error("rate limit exceeded (Stripe 429; retry-after={retry_after_seconds}s)")]
    StripeRateLimit { retry_after_seconds: u32 },
}

/// Compile-time test mode enforcement (Lote 10.7bis R5 P0-3 inheritance pattern).
pub fn assert_test_mode_in_ci() {
    if cfg!(any(test, debug_assertions)) {
        if std::env::var("CORELINK_STRIPE_MODE").as_deref() != Ok("test") {
            panic!("CORELINK_STRIPE_MODE must be 'test' em CI/test/debug builds");
        }
    }
}
```

**Cripto-driven invariants enforced**:

1. **INV-BILLING-NO-DUP** (HIGH; registry §3.9 line 137; verify via grep before commit per Lote 10.8bis P1-13):
   - **Idempotency-Key SEMPRE present** em every Stripe POST request (sprint contract §5.3 R-S10-6 mandatory; sprint contract §19 waiver policy NÃO-waivable).
   - Stripe API guarantees: same idempotency-key → same response (24h window).
   - Internal `(stripe_event_id)` UNIQUE em D1 `stripe_event_log` prevents duplicate webhook processing.
   - Property test 100k retries 0 duplicate charges em test mode.

2. **INV-BILLING-NO-LOSS** (HIGH; registry §3.9 line 136):
   - Stripe API outage → PAT-QUEUE-EVENTS-001 fallback queue retains pending invoices.
   - Retry com PAT-BACKOFF-001 (1s/2s/4s/8s/16s + jitter) max 5 retries.
   - RB-FM-151 dry-run runbook executed em staging (sprint contract §6 DoD prerequisite).
   - Counter aggregator (WI-S10-002) D1 `usage_counter` source-of-truth; invoice generation idempotent monthly cron.

3. **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven Lote 6.2; S-09 inheritance):
   - `stripe_event_log` table append-only via INSERT-only pattern; UPDATE rejected.
   - Each webhook event INSERTs row; (stripe_event_id) UNIQUE prevents duplicate.
   - 7y retention canonical (SOC 2 CC1.4 + GAAP ASC 606).

4. **CTRL-BILLING-001** (security_model.md): financial integrity; Stripe contract canonical.

5. **CTRL-PRIV-002** (privacy_model.md L209 — data classification tags): customer billing PII tagged @classification=pii; DSR pseudonymization via S-11 (S-11 cooperation).

6. **Lote 10.9-quinquies NEW-P0-2 lesson absorbed** (CRITICAL — Stripe webhook payload contains customer email/name PII):
   - Typed `StripeWebhookEvent` enum (NOT serde_json::Value).
   - PII wrappers `CustomerEmail` + `CustomerName` with explicit `serde::Serialize` impl calling `Redact::redact()`.
   - Webhook event log written to `stripe_event_log` table with PII redacted at serialization layer.
   - Raw Stripe webhook payload stored em separate encrypted column with BYOK access control (S-04 inheritance) for forensic replay only.

7. **Test mode pinning** (sprint contract §5.3 R-S10-6):
   - `CORELINK_STRIPE_MODE` Worker secret separated `test|live`.
   - CI/CD pipeline env: `CORELINK_STRIPE_MODE=test`.
   - Live key gated by environment promotion (ADR-0034 promotion gate canonical).
   - Compile-time assertion `assert_test_mode_in_ci()` panics em CI if live mode detected.

8. **Webhook signature verification HMAC-SHA256** (sprint contract §5.3 R-S10-6):
   - Stripe-Signature header parses `t=<unix_ts>,v1=<hex_sha256>`.
   - Verify HMAC-SHA256(`<unix_ts>.<raw_body>`, `STRIPE_WEBHOOK_SECRET`) == v1.
   - 5min replay window: `now() - unix_ts > 300s` → WebhookReplayAttempt error.
   - Constant-time comparison (subtle::ConstantTimeEq) prevents timing attack.

9. **TenantCtx propagation** (Lote 10.4bis): tenant_id from S-03 middleware; NEVER request body.

10. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): `worker::send_future` for async; `wasm-bindgen` HTTP client for Stripe API; NEVER `tokio::spawn`.

11. **5 PlanTier canonical** (per `data_model.md §1` line 68; Lote 10.7bis P0-7 inheritance):
    - free, solo, team, business, enterprise.
    - Mapped to Stripe Price IDs via Neon Postgres `plan(plan_id, stripe_price_id, ...)` table.
    - 5-tier canonical NEVER expanded (sprint contract §10 anti-scope).

12. **5 SKUs canonical** (sprint contract §10 anti-scope; inheritance WI-S10-001/002):
    - cas_storage_gb_month, cas_egress_gb, cas_put_op_count, cas_get_op_count, ac_lookup_op_count.
    - Each SKU has Stripe Price ID em catalog (preconfigured).
    - Invoice line items use canonical SKU labels (auditable).

13. **PII encryption at rest FIPS 140-3 + BYOK** (sprint contract §14.s10.4):
    - `customer_billing_profile.encrypted_email` + `encrypted_name` columns.
    - BYOK from S-04 inheritance (tenant-managed key custody for enterprise).
    - DSR erasure (S-11) → pseudonymization (NOT delete) preserves audit chain integrity.

14. **corelink_time::next_month_first_utc_midnight() canonical primitive** (Lote 10.8bis P0-D):
    - Monthly invoice cycle anchor calculation.
    - GAAP cutoff time (sprint contract §14.s10.7) — invoice em mês N reflects usage do mês N (não N-1 ou N+1).
    - NEVER ad-hoc `now() + Duration::days(30)`.

15. **`cargo-deny` policy** (workspace isolation):
    - `stripe` crate allowed ONLY em `crates/corelink-billing/`.
    - Other workspace members reference `corelink-billing::StripeClient` trait (canonical abstraction).
    - Prevents Stripe primitives leakage to hot path / observability / quota.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + Stripe API integration discipline justification)

`corelink-billing` é **the most legally exposed surface** do CoreLink — Stripe contract is direct customer-facing financial relationship; bug em invoice generation, webhook handling, ou refund processing produces customer dispute, chargeback storm, e legal exposure. Stripe API guidelines (sprint contract §17): "Idempotency-Key MUST be present on all POST requests; same key → same response within 24h"; bypass = double-charge customer = chargeback dispute = trust loss. Webhook signature verification HMAC-SHA256 is non-negotiable — without verification, attacker forges webhook → fake "invoice.paid" → CoreLink marks invoice paid → customer receives no service → support escalation.

**Why Idempotency-Key derivation `corelink-{tenant_id_short}-{operation}-{event_hash_short}`** (sprint contract §5.3 R-S10-6): canonical format de 35 chars: `corelink-` (9) + tenant_id_short (8 hex chars; primeiros 8 do UUID com hyphens stripped) + `-` (1) + operation (8 chars; e.g., `customer`/`sub`/`invoice`/`refund`) + `-` (1) + event_hash_short (8 chars; primeiros 8 hex de BLAKE3(canonical_json(payload))) = **35 chars total** (R5 P1-B fix; antes a narrativa descrevia 62-char com UUID full, contradizendo §6.1 implementação 35-char). Stripe limit de 255 chars; CoreLink interno cap em 64 chars como margem de segurança. UUID v4 entropy 122-bit no tenant_id → collision impossible by construction; CI test 1M events 0 collisions (sprint contract §15 R-003 mitigation). Length validation enforced (`IdempotencyKeyTooLong` error if > 64).

**Why test mode pinning via env separation** (sprint contract §5.3 R-S10-6): accidental live charge from CI = customer trust catastrophe. `CORELINK_STRIPE_MODE` Worker secret separated em CF environments (`test` em CI/dev, `live` em prod after promotion). Compile-time `assert_test_mode_in_ci()` panics if live detected em test build; defense-in-depth.

**Why typed `StripeWebhookEvent` enum** (Lote 10.9-quinquies NEW-P0-2 critical lesson absorbed): Stripe webhook payloads contain customer PII (email, name, billing address). Untyped `serde_json::Value` defeats compile-time PII enforcement; `#[derive(Serialize)]` on struct with `String` email field would write raw email to `stripe_event_log` table = SOC 2 CC1.4 + LGPD Art. 32 violation. Solution: typed enum + PII wrappers (`CustomerEmail`, `CustomerName`) with explicit `serde::Serialize` impl calling `Redact::redact()` — raw PII never persists to audit log.

**Why webhook 5min replay window** (sprint contract §15 R-007 mitigation): Stripe-Signature header includes `t=<unix_ts>`. Without time bound, attacker captures legitimate webhook + replays days later → state confusion. 5min window canonical (Stripe recommendation); constant-time comparison prevents timing attack.

**Why `stripe_event_log` audit trail UNIQUE constraint**: Stripe may deliver same webhook 2× under network failure. Without dedup, "invoice.paid" processed twice → state machine confusion (e.g., 2 refund attempts). `(stripe_event_id) UNIQUE` enforces idempotent webhook processing.

**Why `cargo-deny` stripe crate isolation**: Stripe SDK has 200+ types; leaking primitives to other workspace members creates coupling, makes mocking impossible, e expands cargo-audit attack surface. `corelink-billing` crate is the ONLY consumer of `stripe` crate; other crates reference `StripeClient` trait abstraction.

**Adversarial scenarios**:
- **Idempotency-Key collision attempt**: UUID v4 entropy 122-bit; collision probability ~10⁻³⁶; CI test 1M 0 collisions; `IdempotencyKeyCollision` error fail-CLOSED.
- **Webhook signature replay 6min late**: timestamp validation → `WebhookReplayAttempt`; SEV-2 alert; request rejected.
- **Webhook signature forgery**: HMAC-SHA256 verify fails → `WebhookSignatureFailed`; SEV-2 alert; request rejected; potential attack signal.
- **Stripe API 429 rate limit**: PAT-BACKOFF-001 retry; 5 max attempts; queue fallback; SEV-3 if queue depth > threshold.
- **Stripe outage 1h** (FM-151): RB-FM-151 dry-run prerequisite; queue accumulates; retry on recovery; INV-BILLING-NO-LOSS preserved.
- **PII em webhook event log leak attempt**: typed event + PII wrapper Serialize impl prevents raw write; CI test verifies output redacted.
- **Customer DSR erasure (LGPD Art. 16)**: pseudonymization via S-11 procedure; audit chain integrity preserved (CTRL-PRIV-002).
- **Refund storm (10× normal/h)**: rate limit refund handler; dispute storm detection alert SEV-2.
- **Test mode mismatch em prod build** (worst-case): compile-time assertion catches em CI; runtime check rejects with `TestModeMismatch`.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: CTRL-BILLING-001 financial integrity; bypass = revenue leak silent.
- **FF-HR-009**: Stripe = customer contract direct; bug = customer dispute legal exposure + PCI scope risk.
- 12 sign-offs (Finance + Legal + Privacy emphatic) + chaos suite + property test 100k Idempotency-Key + Stripe outage runbook RB-FM-151 dry-run.

## 3. Customer Impact & Journey

**Persona 1 — Enterprise customer signing up**: signup form → tenant created → CoreLink billing creates Stripe Customer + Subscription via `ensure_customer()` + `create_subscription()`; Idempotency-Key prevents duplicate em retry; customer receives Stripe receipt.

**Persona 2 — Customer paying monthly invoice**: D+30 cron `generate_monthly_invoice()` → Stripe Invoice generated from D1 `usage_counter` aggregates → Stripe charges card → webhook `invoice.paid` → CoreLink `stripe_event_log` INSERT + `invoice_line_item.status='paid'` UPDATE.

**Persona 3 — Customer disputing $X charge**: customer files dispute via Stripe portal → webhook `charge.dispute.created` → CoreLink Dispute handler (CAP-BILLING-008 inheritance) → email Finance + freeze further charges → manual investigation via WI-S10-006 replay endpoint.

**Persona 4 — Customer DSR erasure request (LGPD/GDPR)**: S-11 DSR procedure → pseudonymize `customer_billing_profile` (encrypted_email/name fields → tombstone tenant_id pseudonym); `stripe_event_log` retains pseudonym only; audit chain integrity preserved via CTRL-PRIV-002.

**Persona 5 — Finance reviewing reconciliation drift Layer 3**: WI-S10-004 daily reconciliation detects Σ(invoice_line_item) ≠ Σ(Stripe invoiced); fetches Stripe via `fetch_invoice()` → reconciliation report drift > 0.1% → SEV-1 + Finance investigation.

**Persona 6 — Stripe outage 1h (FM-151)**: monthly cron fails → RB-FM-151 runbook executed → queue retains pending invoices → Stripe recovery → retry succeeds → 0 lost invoices; INV-BILLING-NO-LOSS preserved.

**SLA addendum**:
- Stripe API latency: ≤ 5s p99 (PAT-BACKOFF-001 retry within budget).
- Webhook processing: ≤ 100ms p99 (signature verify + D1 INSERT).
- INV-BILLING-NO-DUP: 0 double-charges em property test 100k retries.
- Test mode protection: 0 live charges from CI (CI/CD pipeline gate + compile-time assertion).
- PII redaction at log emission: 0 raw email/name em `stripe_event_log` (CI lint test).
- RB-FM-151 dry-run: SLO recovery ≤ 1h Stripe outage; 0 lost invoices em queue.

## 4. Capability Mapping

- **CAP-BILLING-003** (Stripe integration customer/subscription/invoice) — IMPLEMENTA primary.
- **CAP-BILLING-008** (Refund / dispute handler) — IMPLEMENTA secondary.
- Trace: `data_model.md` (plan/subscription/invoice_line_item/stripe_event_log/customer_billing_profile schemas) + `security_model.md CTRL-BILLING-001 + CTRL-PRIV-002` + `invariant_registry.md INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP + INV-AUDIT-APPEND-ONLY` + sprint contract §5.3 (R-S10-6/7) + Stripe Billing Architecture Guide + Stripe Webhook signature spec + RFC 7240 (idempotency).

## 5. Tipo

Stripe adapter Rust crate + Neon Postgres schema migrations + webhook handler Worker route + monthly invoice cron + Stripe Price IDs catalog Terraform; HIGH_RISK; FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-billing/` module** — `StripeClient` trait + impls + `StripeWebhookHandler` trait + tests.

2. **Stripe SDK v0.31+ wrapped** behind canonical `StripeClient` trait:
   - Internal `StripeSdkClient` from `stripe` crate.
   - Public API: only `StripeClient` trait + canonical types (no Stripe primitives leak).
   - `cargo-deny.toml` proibe stripe crate em workspace members != corelink-billing.

3. **Idempotency-Key derivation** (sprint contract §5.3 R-S10-6 + §15 R-003 mitigation):
   ```rust
   pub fn derive_idempotency_key(
       tenant_id: TenantId,
       operation: StripeOperation,
       event_hash: &EventHash,
   ) -> IdempotencyKey {
       // Format: corelink-{tenant_id_short}-{operation}-{event_hash_short}
       // Length: 9 + 8 + 1 + 8 + 1 + 8 = 35 chars (well under Stripe 255 limit).
       let key = format!(
           "corelink-{}-{}-{}",
           tenant_id.short_form(),                     // first 8 chars UUID
           operation.canonical_name(),                 // "customer" | "sub" | "invoice" | "refund"
           event_hash.short_form()                     // first 8 chars BLAKE3
       );
       IdempotencyKey::new(key)
   }

   impl IdempotencyKey {
       pub fn new(s: String) -> Self {
           // R4 P1-3 rationale: 64-char internal cap (vs Stripe's 255) é margin de segurança documentada:
           // (a) log readability — chaves curtas não estouram linha; (b) URL-safety quando idempotency-key vai em query string em endpoints internos; (c) D1 column sizing — VARCHAR(64) economiza storage; (d) operations canonical são curtas (`customer`/`sub`/`invoice`/`refund` ≤ 8 chars); 35-char canonical format + 29 chars margem para variants.
           // Operations > 8 chars devem ser hyphenated abbreviations (e.g., "sub-cancel" não "subscription_cancel"); enforcement via canonical_name() impl.
           assert!(s.len() <= 64, "Idempotency-Key length canonical ≤ 64 (CoreLink internal cap; Stripe limit 255 — ver ADR rationale acima)");
           Self(s)
       }
   }
   ```

4. **Test mode pinning** (sprint contract §5.3 R-S10-6):
   - Worker secret `CORELINK_STRIPE_MODE` per environment (test|live).
   - CI/CD: `CORELINK_STRIPE_MODE=test` enforced em GitHub Actions workflow.
   - `assert_test_mode_in_ci()` panics em test build if live mode detected.
   - Live key (`STRIPE_LIVE_SECRET_KEY`) gated by ADR-0034 environment promotion.

5. **Webhook handler `POST /v1/webhooks/stripe`** (sprint contract §5.3 R-S10-6):
   ```rust
   pub async fn handle_webhook(req: Request) -> Result<Response, Error> {
       let signature = req.header("Stripe-Signature").ok_or(WebhookSignatureFailed)?;
       let body = req.body_bytes().await?;

       // Step 1: parse Stripe-Signature header (t=<ts>,v1=<sig>)
       let parsed = parse_stripe_signature(&signature)?;

       // Step 2: 5min replay window check
       if Utc::now().timestamp() - parsed.timestamp > 300 {
           return Err(WebhookReplayAttempt { ... });
       }

       // Step 3: HMAC-SHA256 verify (constant-time comparison)
       let expected = hmac_sha256(format!("{}.{}", parsed.timestamp, body), webhook_secret);
       if !subtle::ConstantTimeEq::ct_eq(&expected, &parsed.signature).into() {
           return Err(WebhookSignatureFailed);
       }

       // Step 4: parse typed StripeWebhookEvent (NOT serde_json::Value)
       let event: StripeWebhookEvent = serde_json::from_slice(&body)?;

       // Step 5: dedup via stripe_event_id UNIQUE
       let stripe_event_id = event.id();
       let inserted = neon_db.execute(
           "INSERT INTO stripe_event_log (stripe_event_id, event_type, payload_redacted, received_at)
            VALUES ($1, $2, $3, NOW()) ON CONFLICT (stripe_event_id) DO NOTHING",
           &[&stripe_event_id, &event.type_name(), &serialize_redacted(&event)?]
       ).await?;
       if inserted.rows_affected() == 0 {
           return Err(WebhookDuplicate { event_id: stripe_event_id });
       }

       // Step 6: dispatch to handler
       match event {
           StripeWebhookEvent::InvoicePaid { data } => handle_invoice_paid(data).await?,
           StripeWebhookEvent::InvoicePaymentFailed { data } => handle_payment_failed(data).await?,
           StripeWebhookEvent::ChargeRefunded { data } => handle_refund(data).await?,
           StripeWebhookEvent::ChargeDisputeCreated { data } => handle_dispute(data).await?,
           StripeWebhookEvent::CustomerSubscriptionDeleted { data } => handle_sub_deleted(data).await?,
       }

       Ok(Response::ok("ack"))
   }
   ```

6. **Neon Postgres schemas** (sprint contract §5.3 R-S10-7):
   ```sql
   CREATE TABLE plan (
       plan_id TEXT PRIMARY KEY,
       tier TEXT NOT NULL,
       stripe_price_id TEXT NOT NULL,
       monthly_base_price_usd_cents INTEGER NOT NULL,
       included_storage_gb INTEGER NOT NULL,
       included_egress_gb INTEGER NOT NULL,
       overage_per_gb_storage_cents INTEGER NOT NULL,
       overage_per_gb_egress_cents INTEGER NOT NULL,
       created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
       CHECK (tier IN ('free', 'solo', 'team', 'business', 'enterprise'))  -- canonical per data_model.md §1 L68
   );

   CREATE TABLE subscription (
       subscription_id TEXT PRIMARY KEY,                -- internal UUID
       tenant_id TEXT NOT NULL,
       stripe_subscription_id TEXT NOT NULL UNIQUE,
       plan_id TEXT NOT NULL REFERENCES plan(plan_id),
       status TEXT NOT NULL,                            -- active|canceled|past_due|paused|trial
       billing_cycle_anchor TIMESTAMPTZ NOT NULL,       -- corelink_time::next_month_first_utc_midnight() canonical
       created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
       canceled_at TIMESTAMPTZ,
       CHECK (status IN ('active', 'canceled', 'past_due', 'paused', 'trial'))
   );

   CREATE TABLE invoice_line_item (
       line_item_id TEXT PRIMARY KEY,
       tenant_id TEXT NOT NULL,
       stripe_invoice_id TEXT,                          -- NULL until Stripe Invoice generated
       stripe_invoice_item_id TEXT UNIQUE,              -- Stripe-side ID
       billing_period TEXT NOT NULL,                    -- "2026-09" YYYY-MM canonical
       sku TEXT NOT NULL,                               -- 5 canonical inheritance
       quantity NUMERIC(20,4) NOT NULL,
       unit_price_usd_cents INTEGER NOT NULL,
       total_usd_cents INTEGER NOT NULL,
       status TEXT NOT NULL,                            -- pending|invoiced|paid|refunded|disputed
       created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
       CHECK (sku IN ('cas_storage_gb_month', 'cas_egress_gb', 'cas_put_op_count', 'cas_get_op_count', 'ac_lookup_op_count')),
       CHECK (status IN ('pending', 'invoiced', 'paid', 'refunded', 'disputed')),
       CHECK (quantity >= 0),
       CHECK (total_usd_cents >= 0)
   );

   CREATE TABLE stripe_event_log (
       stripe_event_id TEXT PRIMARY KEY,                -- Stripe evt_* UNIQUE
       event_type TEXT NOT NULL,                        -- 5 canonical types
       payload_redacted JSONB NOT NULL,                 -- PII redacted via Serialize impl
       payload_encrypted_blob BYTEA,                    -- raw payload BYOK encrypted (forensic only)
       received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
       processed_at TIMESTAMPTZ,
       processing_error TEXT,
       CHECK (event_type IN ('invoice.paid', 'invoice.payment_failed', 'charge.refunded', 'charge.dispute.created', 'customer.subscription.deleted'))
   );

   CREATE INDEX idx_stripe_event_log_received ON stripe_event_log(received_at);
   CREATE INDEX idx_stripe_event_log_unprocessed ON stripe_event_log(received_at) WHERE processed_at IS NULL;

   CREATE TABLE customer_billing_profile (
       tenant_id TEXT PRIMARY KEY,
       stripe_customer_id TEXT NOT NULL UNIQUE,
       encrypted_email BYTEA NOT NULL,                  -- BYOK encrypted (S-04 inheritance)
       encrypted_name BYTEA NOT NULL,                   -- BYOK encrypted
       email_hmac TEXT NOT NULL,                        -- HMAC for lookup (no raw)
       country_code TEXT NOT NULL,
       created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
       updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
       pseudonymized_at TIMESTAMPTZ,                    -- DSR erasure tombstone (S-11 cooperation)
       CHECK (length(country_code) = 2)
   );
   ```

7. **Monthly invoice cron** (canonical `corelink_time::next_month_first_utc_midnight()` Lote 10.8bis P0-D):
   ```rust
   #[cron("0 2 1 * *")]  // 02:00 UTC on 1st of month
   pub async fn monthly_invoice_cron() -> Result<(), BillingStripeError> {
       let billing_period = BillingPeriod::previous_month_canonical();
       let cycle_anchor = corelink_time::next_month_first_utc_midnight();

       for tenant in active_tenants_query().await? {
           // FAIL-CLOSED: any error halts; queue fallback PAT-QUEUE-EVENTS-001
           let counters = wi_s10_002_counter_aggregator::query_for_tenant_period(
               tenant.tenant_id, billing_period
           ).await?;

           let line_items = build_invoice_line_items(counters)?;

           let idempotency_key = derive_idempotency_key(
               tenant.tenant_id,
               StripeOperation::Invoice,
               &EventHash::from_billing_period(billing_period)
           );

           stripe_client.generate_monthly_invoice(
               tenant.tenant_id, billing_period, line_items
           ).with_idempotency_key(idempotency_key).await?;
       }
       Ok(())
   }
   ```

8. **Stripe API retry com PAT-BACKOFF-001** (sprint contract §14.s10.2):
   - Max 5 retries, exponential backoff 1s/2s/4s/8s/16s + jitter ±20%.
   - 429 Stripe rate limit → respect Retry-After header.
   - 5xx Stripe transient → retry with backoff.
   - 4xx (other than 429) → fail-CLOSED imediato; SEV-2.

9. **PAT-QUEUE-EVENTS-001 fallback queue** (FM-151 Stripe outage):
   - Cloudflare Queue `stripe-pending-${region}` per region.
   - Persistent retry buffer for pending invoices/refunds.
   - RB-FM-151 dry-run runbook (sprint contract §6 DoD prerequisite).

10. **PII encryption at rest FIPS 140-3** (sprint contract §14.s10.4):
    - `customer_billing_profile.encrypted_email/name` columns.
    - BYOK from S-04 inheritance.
    - DSR pseudonymization via S-11 cooperation (CTRL-PRIV-002).

11. **TenantCtx-only enforcement** (Lote 10.4bis): tenant_id from S-03 middleware; never request body.

12. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): `worker::send_future`; HTTP via wasm-bindgen; NEVER `tokio::spawn`.

13. **5-tier canonical Plan reference** (per `data_model.md §1` line 68; Lote 10.7bis P0-7 inheritance): PlanTier enum closed (free, solo, team, business, enterprise); 5 tiers maximum; CHECK constraint enforces.

14. **5 SKUs canonical** (sprint contract §10 anti-scope): inheritance from WI-S10-001/002; CHECK constraint em invoice_line_item.

15. **corelink_time::next_month_first_utc_midnight() canonical primitive** (Lote 10.8bis P0-D): monthly cycle anchor + invoice billing period boundary.

16. **`cargo-deny` policy**: stripe crate restricted to `corelink-billing/`; CI enforces.

17. **Métricas operacionais** (via WI-S09-001 emit lib; cardinality budget; underscored canonical Lote 10.9bis P0-E):
    - `corelink_billing_stripe_api_calls_total{operation, status}` (counter; 4 ops × 5 statuses = 20 séries).
    - `corelink_billing_stripe_api_duration_seconds{operation}` (histogram; 4 × 11 buckets + sum/count = 52).
    - `corelink_billing_stripe_idempotency_replay_total{operation}` (counter; informational; 4 séries).
    - `corelink_billing_webhook_signature_failures_total{reason}` (counter; **alert SEV-2 if > 0**; 3 reasons = 3 séries).
    - `corelink_billing_webhook_replay_attempts_total` (counter; **alert SEV-2 if > 0**).
    - `corelink_billing_webhook_duplicates_total` (counter; informational; idempotency canary).
    - `corelink_billing_invoice_value_usd_cents_total{tenant_tier}` (counter; 5 tiers = 5 séries; sprint contract R-S10-15).
    - `corelink_billing_test_mode_violations_total` (counter; **alert SEV-1 if > 0** — production safety canary).
    - `corelink_billing_pii_in_event_log_detected_total` (counter; **alert SEV-1 if > 0** — DLP regression critical).
    - `corelink_billing_stripe_queue_depth{operation}` (gauge; PAT-QUEUE-EVENTS-001 backlog; **alert SEV-2 if > 1000**).

18. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 absorbed):
    - `prop_idempotency_no_duplicate`: 100k retries same Idempotency-Key (test mode); 0 duplicate Stripe charges.
    - `prop_idempotency_key_collision_impossible`: 1M random tenant_id+event_hash combinations; 0 collisions.
    - `prop_idempotency_key_length_bounded`: 100k random inputs; key length ≤ 64 chars.
    - `prop_webhook_signature_verify`: 10k events com valid + invalid signatures; valid pass, invalid reject.
    - `prop_webhook_replay_window`: 1k events com timestamps em [-10min, +10min]; only ±5min window accepted.
    - `prop_typed_payload_no_serde_json_value`: 10k webhook payloads; assert StripeWebhookEvent typed enum; serde_json::Value rejected at compile time.
    - `prop_pii_redacted_em_event_log`: 10k webhooks com customer email/name; assert serialized payload contains "j***@" form NOT raw.

19. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 12):
    1. **Stripe API 429 rate limit**: PAT-BACKOFF-001 retry; respects Retry-After; 0 lost requests.
    2. **Stripe outage 1h** (FM-151 RB-FM-151 dry-run): queue fallback; retry on recovery; 0 lost invoices; INV-BILLING-NO-LOSS preserved.
    3. **Webhook signature forgery attempt**: HMAC-SHA256 verify fails; SEV-2 alert; request rejected.
    4. **Webhook 6min replay**: timestamp validation rejects; SEV-2 alert.
    5. **Webhook duplicate delivery (Stripe network retry)**: (stripe_event_id) UNIQUE dedup; idempotent; INV-BILLING-NO-DUP preserved.
    6. **Idempotency-Key 1M test (collision search)**: UUID v4 entropy 122-bit; 0 collisions; sprint contract §15 R-003 mitigation verified.
    7. **Test mode mismatch em prod build**: compile-time assertion catches em CI; runtime check rejects with TestModeMismatch.
    8. **PII em webhook event log leak attempt**: typed event + PII wrapper Serialize impl prevents raw write; CI test verifies output redacted.
    9. **DSR erasure mid-billing-period**: pseudonymization via S-11; audit chain integrity preserved (CTRL-PRIV-002).
    10. **Refund storm (10× normal/h)**: rate limit refund handler + dispute storm detection alert SEV-2.
    11. **Stripe webhook replay attack via captured signature**: 5min window + (stripe_event_id) UNIQUE prevents double-process; INV-BILLING-NO-DUP preserved.
    12. **Cargo-deny enforcement (workspace isolation)**: developer attempts use stripe crate em outro workspace member; cargo build fails; CI gate.

### 6.2 Out-of-scope (deferred)

- Counter aggregation (delegate WI-S10-002 — input source).
- Reconciliation worker 3-layer (delegate WI-S10-004 — Layer 3 consumer).
- Quota state machine (delegate WI-S10-005).
- Replay forensic endpoint (delegate WI-S10-006).
- TLA+ billing_atomicity (delegate WI-S10-007).
- Tax calculation (anti-scope sprint contract §10 — Stripe Tax handles).
- Multi-currency (anti-scope §10 — USD only at GA).
- Crypto / non-Stripe payment methods (anti-scope §10).
- Invoice PDF generation in-house (anti-scope §10 — Stripe handles).
- Customer-facing PCI scope (anti-scope §10 — Stripe Elements iframe).

## 7. Anti-Scope

- ❌ serde_json::Value em StripeWebhookEvent (Lote 10.9-quinquies NEW-P0-2; typed enum canonical).
- ❌ Idempotency-Key omitted (sprint contract §19 NÃO-waivable; double-charge risk).
- ❌ Webhook signature verification skip (replay attack).
- ❌ Test mode mismatch em CI/dev (compile-time assertion enforces).
- ❌ Live key em CI environment (Worker secret separated).
- ❌ Stripe primitives leaking via API public (cargo-deny enforces).
- ❌ Raw PII em stripe_event_log (Serialize impl Redact wraps).
- ❌ ad-hoc `now() + Duration::days(30)` (corelink_time::next_month_first_utc_midnight canonical).
- ❌ Per-feature SKU expansion beyond 5 canonical (sprint contract §10).
- ❌ Plan tier expansion beyond 5 canonical (Lote 10.7bis P0-7).
- ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ TenantCtx bypass.
- ❌ Reverse engineering Stripe API (sprint contract §17 references guide official).

## 8. Acceptance Criteria (Gherkin) — 12 scenarios

```gherkin
Feature: Stripe Adapter + Idempotency-Key + Webhook Handler

  Scenario: Idempotency-Key SEMPRE present em Stripe POST
    Given tenant T creates subscription Plan=team
    When StripeClient::create_subscription() invoked
    Then Stripe API request includes Idempotency-Key: corelink-{T_short}-sub-{event_hash_short}
    Then key length ≤ 64 chars
    Then Stripe responds with subscription created OR retrieved (idempotent)

  Scenario: Idempotency-Key prevents double-charge on retry
    Given Stripe API returns 5xx transient on first attempt
    When PAT-BACKOFF-001 retries within 24h window
    Then same Idempotency-Key sent
    Then Stripe returns same subscription (NOT duplicate)
    Then INV-BILLING-NO-DUP preserved
    Then property test prop_idempotency_no_duplicate green (100k iter)

  Scenario: Test mode pinning em CI
    Given GitHub Actions workflow runs CI
    Given env CORELINK_STRIPE_MODE=test
    When test build runs
    Then assert_test_mode_in_ci() passes
    Then no live Stripe API key accessible
    Then 0 live charges em CI

  Scenario: Test mode mismatch panics
    Given test build with CORELINK_STRIPE_MODE=live (mistake)
    When assert_test_mode_in_ci() invoked
    Then process panics with "CORELINK_STRIPE_MODE must be 'test' em CI/test/debug builds"
    Then merge blocked

  Scenario: Webhook signature HMAC-SHA256 verified
    Given Stripe POSTs webhook to /v1/webhooks/stripe
    Given Stripe-Signature: t=1234567890,v1=abc123def456...
    When handle_webhook() invoked
    Then HMAC-SHA256(format!("{}.{}", t, body), webhook_secret) computed
    Then constant-time comparison subtle::ConstantTimeEq
    Then if equal: proceed to event dispatch
    Then if differs: WebhookSignatureFailed; SEV-2 alert; request rejected

  Scenario: Webhook 5min replay window enforced
    Given Stripe-Signature t=<6min ago>
    When handle_webhook() invoked
    Then now() - t > 300 → WebhookReplayAttempt
    Then SEV-2 alert: corelink_billing_webhook_replay_attempts_total increments
    Then request rejected

  Scenario: Webhook duplicate dedup via stripe_event_id UNIQUE
    Given webhook evt_abc123 received at T0; stripe_event_log row inserted
    When same evt_abc123 re-delivered at T0+30s (Stripe retry)
    Then ON CONFLICT (stripe_event_id) DO NOTHING
    Then rows_affected=0 → WebhookDuplicate error
    Then NOT re-processed; INV-BILLING-NO-DUP preserved

  Scenario: Typed StripeWebhookEvent enum (Lote 10.9-quinquies NEW-P0-2)
    Given Stripe webhook payload {"type": "invoice.paid", "data": {...}}
    When serde_json::from_slice() parses
    Then StripeWebhookEvent::InvoicePaid variant matched
    Then NOT serde_json::Value (compile-time enforced)
    Then InvoicePaidData typed sub-payload

  Scenario: PII em webhook redacted at log emission
    Given webhook InvoicePaid with customer_email="john@example.com" customer_name="John Doe"
    When stripe_event_log INSERT serializes payload
    Then CustomerEmail Serialize impl calls Redact::redact() → "j***@example.com"
    Then CustomerName Serialize impl calls Redact::redact() → "J*** D**"
    Then payload_redacted JSONB column contains redacted form
    Then payload_encrypted_blob (BYOK) retains raw for forensic ONLY

  Scenario: Cargo-deny workspace isolation
    Given developer attempts add stripe crate dep em crates/corelink-cas/Cargo.toml
    When cargo build runs
    Then cargo-deny.toml policy denies
    Then CI gate fails

  Scenario: Stripe outage 1h FM-151 RB-FM-151 dry-run
    Given Stripe API returns 5xx for 1h
    When monthly_invoice_cron() runs
    Then PAT-BACKOFF-001 retries 5× then queue fallback
    Then PAT-QUEUE-EVENTS-001 stripe-pending queue retains invoices
    Then on Stripe recovery: queue drained; 0 lost invoices
    Then INV-BILLING-NO-LOSS preserved
    Then RB-FM-151 dry-run executed em staging (sprint contract §6 DoD)

  Scenario: Monthly invoice cycle anchor canonical
    Given current date 2026-09-15
    When monthly_invoice_cron fires 2026-10-01T02:00:00Z
    Then billing_period = "2026-09" (previous month canonical)
    Then cycle_anchor = corelink_time::next_month_first_utc_midnight() = 2026-11-01T00:00:00Z
    Then GAAP cutoff: invoice em mês N reflects usage do mês N (sprint contract §14.s10.7)
```

## 9. Design Decisions

- 9.1: Stripe SDK v0.31+ (canonical Rust crate; semver-locked).
- 9.2: StripeClient trait abstraction (cargo-deny enforces; mocking enabled).
- 9.3: Idempotency-Key derivation `corelink-{tenant_id_short}-{operation}-{event_hash_short}` (≤ 64 chars).
- 9.4: Test mode pinning via Worker secret `CORELINK_STRIPE_MODE` + compile-time assertion.
- 9.5: Webhook HMAC-SHA256 with subtle::ConstantTimeEq (timing attack protection).
- 9.6: 5min replay window canonical (sprint contract §15 R-007 mitigation).
- 9.7: (stripe_event_id) UNIQUE em D1 stripe_event_log dedup.
- 9.8: Typed StripeWebhookEvent enum (Lote 10.9-quinquies NEW-P0-2).
- 9.9: PII wrappers (CustomerEmail, CustomerName) with explicit Serialize impl Redact-wrapping.
- 9.10: 5 PlanTier canonical (Lote 10.7bis P0-7); CHECK constraint.
- 9.11: 5 SKUs canonical (sprint contract §10); CHECK constraint em invoice_line_item.
- 9.12: corelink_time::next_month_first_utc_midnight() canonical (Lote 10.8bis P0-D); GAAP cutoff.
- 9.13: PAT-BACKOFF-001 max 5 retries 1s/2s/4s/8s/16s + jitter (sprint contract §14.s10.2).
- 9.14: PAT-QUEUE-EVENTS-001 fallback queue (FM-151 Stripe outage).
- 9.15: RB-FM-151 dry-run prerequisite (sprint contract §6 DoD).
- 9.16: BYOK encryption customer_billing_profile (S-04 inheritance + CTRL-PRIV-002).
- 9.17: cargo-deny stripe crate workspace isolation.
- 9.18: TenantCtx-only enforcement (Lote 10.4bis).
- 9.19: CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3).
- 9.20: NEW INVs INV-BILLING-NO-DUP em invariant_registry — verify canonical position before commit (Lote 10.8bis P1-13).

## 10. Completeness Criteria SOTA

- [ ] **10.s10.003.1** Crate compila + integration tests green (Stripe test mode).
- [ ] **10.s10.003.2** All 12 Gherkin scenarios green.
- [ ] **10.s10.003.3** Property tests 7 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar).
- [ ] **10.s10.003.4** Chaos suite 12 scenarios green.
- [ ] **10.s10.003.5** **INV-BILLING-NO-DUP** property test 100k retries 0 duplicate charges.
- [ ] **10.s10.003.6** **INV-BILLING-NO-LOSS** RB-FM-151 dry-run sustained 1h Stripe outage with 0 lost invoices.
- [ ] **10.s10.003.7** Idempotency-Key 1M collision test 0 collisions (sprint contract §15 R-003).
- [ ] **10.s10.003.8** Test mode pinning verified: 0 live charges from CI em 30d.
- [ ] **10.s10.003.9** Neon Postgres schemas migrated (5 tables); CHECK constraints applied.
- [ ] **10.s10.003.10** Stripe Price IDs catalog populated (5 SKUs × 5 tiers = 25 entries).
- [ ] **10.s10.003.11** PII redaction at log emission verified: 0 raw email/name em stripe_event_log payload_redacted column (CI lint test).
- [ ] **10.s10.003.12** cargo-deny.toml policy enforced; CI gate green.
- [ ] **10.s10.003.13** Métricas (10) emitted via WI-S09-001 emit lib; cardinality budget respected (~140 séries baseline).
- [ ] **10.s10.003.14** Cargo-audit + cargo-deny + clippy clean.

## 11. DoD

- [ ] Crate compila + tests green; all Gherkin/property/chaos green; 12 sign-offs (HIGH_RISK; framework §33.5.4.3 cap; Finance + Legal + Privacy emphatic).

## 12. Invariants Validated

- **INV-BILLING-NO-DUP** (HIGH; registry §3.9 line 137; verify via grep before commit per Lote 10.8bis P1-13): Idempotency-Key SEMPRE + (stripe_event_id) UNIQUE; property test 100k retries 0 duplicates.
- **INV-BILLING-NO-LOSS** (HIGH; registry §3.9 line 136): PAT-QUEUE-EVENTS-001 fallback + PAT-BACKOFF-001 retry; RB-FM-151 dry-run 0 lost invoices.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven Lote 6.2; S-09 inheritance): stripe_event_log INSERT-only; UPDATE rejected.
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-tenant Stripe Customer; cross-tenant impossible.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): TenantCtx-only middleware S-03 inheritance.
- **CTRL-BILLING-001** (security_model.md): Stripe contract canonical financial integrity.
- **CTRL-PRIV-002** (privacy_model.md L209 — data classification tags): customer billing PII tagged @classification=pii; DSR pseudonymization via S-11 (S-11 cooperation).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Stripe adapter module | `crates/corelink-billing/` | Rust |
| StripeClient trait | `crates/corelink-billing/src/client.rs` | Rust |
| StripeWebhookHandler trait | `crates/corelink-billing/src/webhook.rs` | Rust |
| PII wrapper types | `crates/corelink-billing/src/pii.rs` | Rust |
| Neon Postgres migrations | `migrations/00X_billing_schemas.sql` | SQL |
| Webhook handler Worker route | `crates/corelink-worker/src/routes/stripe_webhook.rs` | Rust |
| Monthly invoice cron | `crates/corelink-worker/src/crons/monthly_invoice.rs` | Rust |
| Stripe Price IDs catalog | `infra/stripe/price_catalog.tf` | Terraform |
| cargo-deny policy | `cargo-deny.toml` | TOML |
| RB-FM-151 dry-run script | `runbooks/rb-fm-151-stripe-outage-dry-run.sh` | Bash |
| Property tests | `crates/corelink-billing/tests/prop_billing.rs` | Rust |
| Chaos suite | `tests/chaos_billing_stripe.rs` | Rust |

## 14. Quality Standards SOTA

- 14.s10.003.1: Zero unsafe Rust; zero unwrap em production paths.
- 14.s10.003.2: rustdoc 100% public API.
- 14.s10.003.3: Test coverage ≥ 90%.
- 14.s10.003.4: Stripe API latency ≤ 5s p99 (PAT-BACKOFF-001 retry budget).
- 14.s10.003.5: Webhook processing ≤ 100ms p99.
- 14.s10.003.6: SAST clean; cargo-deny strict.
- 14.s10.003.7: Métricas (10 §6.1.17).
- 14.s10.003.8: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s10.003.9: TenantCtx-only (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); 5-tier canonical Plan (Lote 10.7bis P0-7); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV positions §3.9 L136/137 (NO-LOSS/NO-DUP) + §3.12 L166/167 (RECONCILE-3-LAYER/REPLAYABLE) verified Lote 10.10bis (Lote 10.8bis P1-13 lesson absorbed).
- 14.s10.003.10: corelink_time::next_month_first_utc_midnight() canonical (Lote 10.8bis P0-D); GAAP cutoff time documented.
- 14.s10.003.11: D1 batch ≤ 250 (Lote 10.5bis); CHECK constraints inline (Lote 10.5bis).
- 14.s10.003.12: Stripe API discipline (Lote 10.6bis split-tier inheritance): Stripe call fail-CLOSED with queue fallback; webhook handler fail-CLOSED on signature/replay/dup (rejection canonical).
- 14.s10.003.13: BLAKE3-256 event_hash for Idempotency-Key derivation (consistency com WI-S10-001/002 hash conventions).
- 14.s10.003.14: Typed StripeWebhookEvent (NOT serde_json::Value; Lote 10.9-quinquies NEW-P0-2 inheritance); PII wrappers explicit Serialize impl.
- 14.s10.003.15: Prom metric names underscored canonical (Lote 10.9bis P0-E inheritance).
- 14.s10.003.16: cargo-deny stripe crate workspace isolation enforced (canonical S-10 discipline).

## 15. Chaos Experiments (12)

§6.1.19 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (framework §33.5.4.3 cap; Finance + Legal + Privacy + Architect emphatic).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + StripeClient trait + StripeWebhookHandler trait + 5 PlanTier + 5 StripeWebhookEvent enums | 2 |
| ST-002 | Stripe SDK v0.31+ wrap + StripeClient impl (ensure_customer, create_subscription, generate_monthly_invoice, fetch_invoice, process_refund) | 4 |
| ST-003 | Idempotency-Key derivation + length validation + collision test 1M | 2 |
| ST-004 | Test mode pinning + assert_test_mode_in_ci() + Worker secret env separation | 1.5 |
| ST-005 | Webhook handler HMAC-SHA256 verify + 5min replay window + (stripe_event_id) UNIQUE dedup | 3 |
| ST-006 | Typed StripeWebhookEvent enum + PII wrappers (CustomerEmail, CustomerName) Serialize impl | 2 |
| ST-007 | Neon Postgres schemas (plan, subscription, invoice_line_item, stripe_event_log, customer_billing_profile) migrations | 2 |
| ST-008 | Monthly invoice cron + corelink_time::next_month_first_utc_midnight() canonical | 2 |
| ST-009 | PAT-BACKOFF-001 retry + PAT-QUEUE-EVENTS-001 fallback queue + RB-FM-151 dry-run script | 3 |
| ST-010 | BYOK encryption customer_billing_profile (S-04 inheritance) + CTRL-PRIV-002 classification tag + S-11 DSR pseudonymization stub | 1.5 |
| ST-011 | cargo-deny.toml policy + CI gate enforcement | 0.5 |
| ST-012 | Métricas (10) emit | 1 |
| ST-013 | Property tests (7 × 10k; 100k nightly) | 3 |
| ST-014 | Chaos suite (12) + RB-FM-151 dry-run staging execution | 3 |
| ST-015 | Privacy review + Compliance schema versioning sign-off + Finance walkthrough Stripe API | 1 |

**Total**: ~31.5h. **PERT** O=16h M=26h P=42h: **27.0h** (matches sprint contract §12 estimate).

## 18. Dependencies

- Hard: WI-S10-001 SEALED (event source for billing); WI-S10-002 SEALED (counter aggregates input); WI-S09-001 SEALED (cardinality emit lib + métricas underscores); WI-S09-002 SEALED (PII wrapper Serialize impl pattern); WI-S09-004 SEALED (audit trail pattern); S-03 SEALED (TenantCtx); S-04 SEALED (BYOK encryption).
- Soft: WI-S10-004 (Layer 3 reconciliation consumer); WI-S10-005 (quota state machine + email integration); WI-S10-006 (replay forensic endpoint); WI-S10-007 (TLA+ billing_atomicity); S-11 (DSR cooperation CTRL-PRIV-002).
- Hard infra: Neon Postgres available; Stripe test mode account provisioned; Stripe webhook endpoint registered; Cloudflare Queue available; Worker secrets `CORELINK_STRIPE_MODE` + `STRIPE_TEST_SECRET_KEY` + `STRIPE_LIVE_SECRET_KEY` + `STRIPE_WEBHOOK_SECRET` configured per env.
- External: Stripe API uptime SLA (sprint contract §15 R-001 risk acceptance).

## 19. Effort PERT: ~27.0h. ## 20. Time-boxing: 42h hard limit.

## 21. Observability

10 metrics §6.1.17. Trace span `billing.stripe.{api_call, webhook_verify, webhook_dispatch, idempotency_derive, queue_fallback, monthly_invoice_cron}`.

## 22. Cost Analysis

- Stripe API: $0.30/transaction + 2.9% (out-of-scope for CoreLink internal cost; passed to Customer).
- Neon Postgres: ~5 GB/region × 5 regions × $0.23/GB-mo × 12 = ~$70/yr.
- Cloudflare Queue (PAT-QUEUE-EVENTS-001): $0.40/1M ops × 100M/yr = $40/yr.
- Worker requests (webhook handler): $0.30/1M × 1M/mo × 12 = $3.60/yr (negligible).
- TCO 12m: ~$120/yr billing-stripe infrastructure.
- **Cost saved by INV-BILLING-NO-DUP**: prevents double-charge customer (typical chargeback cost $15-25 + reputation; potential thousands in dispute volume); Idempotency-Key + (stripe_event_id) UNIQUE = financial protection.

## 23. API Contract

- Public Rust: `StripeClient` trait + `StripeWebhookHandler` trait + `PlanTier`, `StripeWebhookEvent`, `IdempotencyKey`, `BillingStripeError`, `CustomerEmail`, `CustomerName` types; `#[non_exhaustive]`.
- Wire (inbound): `POST /v1/webhooks/stripe` Stripe-Signature HMAC-SHA256.
- Wire (outbound): Stripe API v2024-XX (semver-locked SDK).
- Storage: Neon Postgres (5 schemas); BYOK encrypted columns.

## 24. Post-mortem Hooks

- INV-BILLING-NO-DUP violation (double-charge customer detected) → HIGH-severity post-mortem + Idempotency-Key + (stripe_event_id) UNIQUE root-cause.
- INV-BILLING-NO-LOSS violation (Stripe outage caused lost invoice) → HIGH-severity + RB-FM-151 review.
- Idempotency-Key collision detected → CRITICAL post-mortem + UUID v4 entropy review.
- Test mode mismatch in production (live charge from CI) → CRITICAL post-mortem + CTRL-BILLING-001 violation.
- PII em stripe_event_log leak detected → CRITICAL post-mortem + CTRL-PRIV-001 violation.
- Webhook signature failure spike (potential attack signal > 10/h) → SEV-1 + AppSec investigation.
- Refund storm (Stripe webhook flood after fraud detection) > 1% MoM → post-mortem + Finance + Legal review.

## 25. Rollback / Recovery

- Rollback: revert StripeClient impl + webhook handler route; existing subscriptions continue (Stripe maintains state); monthly invoice cron disabled; queue accumulates pending; RB-FM-151 procedure for resume.
- Recovery: Stripe outage → queue retains; on recovery, drain queue with retry; reconciliation Layer 3 (WI-S10-004) catches any drift.
- RTO ≤ 30min (Worker rollback + Stripe state intact); RPO ≤ 0min (events queued; no data loss).

## 26. Security & Privacy

**STRIDE**:
- S(poofing): Webhook HMAC-SHA256 verify; Stripe-Signature canonical; constant-time comparison.
- T(ampering): stripe_event_log INSERT-only; UPDATE rejected; (stripe_event_id) UNIQUE.
- R(epudiation): Audit log immutable; webhook event_id + timestamp + signature retained.
- I(nformation disclosure): PII wrappers (CustomerEmail/Name) Serialize impl Redact-wraps; raw em encrypted_blob BYOK only; stripe_event_log payload_redacted column.
- D(enial of Service): PAT-BACKOFF-001 retry budget; PAT-QUEUE-EVENTS-001 fallback; refund storm rate-limit detection.
- E(scalation of Privilege): Stripe API key per environment (test|live); ADR-0034 promotion gate; cargo-deny workspace isolation.

**LINDDUN** (LGPD/GDPR mandatory):
- L(inkability): Stripe Customer ↔ tenant_id linkable; expected (financial-grade billing).
- I(dentifiability): Customer email/name BYOK-encrypted; pseudonymization via S-11 DSR cooperation; payload_redacted column for log access.
- N(on-repudiation): Stripe-Signature HMAC + audit log immutable evidence trail.
- D(etectability): Customer billing profile via WI-S10-006 replay endpoint role-protected.
- D(isclosure): 7y retention (SOC 2/GAAP) vs erasure right (LGPD Art. 16 vs Art. 18); pseudonymization preserves chain integrity (CTRL-PRIV-002).
- U(nawareness): Customer notified via S-13 admin plane.
- N(on-compliance): **CTRL-BILLING-001 + CTRL-PRIV-002 + SOC 2 CC1.4 + GAAP ASC 606 + LGPD Art. 32 + GDPR Art. 32 + PCI DSS (out-of-scope via Stripe Elements anti-scope §10)** compliance.

## 27. Knowledge Transfer

Tech talk (1.5h): "S-10 Stripe Adapter: Idempotency-Key + Webhook + cargo-deny Isolation"; doc `docs/dev/billing-stripe-architecture.md`; onboarding test 10 questions: Idempotency-Key derivation `corelink-{tenant_id_short}-{operation}-{event_hash_short}`, test mode pinning + compile-time assertion (sprint contract §5.3 R-S10-6), webhook HMAC-SHA256 + 5min replay window canonical, (stripe_event_id) UNIQUE dedup, typed StripeWebhookEvent (Lote 10.9-quinquies NEW-P0-2), PII wrapper Serialize impl Redact-wrapping, 5 PlanTier canonical (Lote 10.7bis P0-7), 5 SKUs canonical (sprint contract §10), corelink_time::next_month_first_utc_midnight() (Lote 10.8bis P0-D), cargo-deny stripe crate workspace isolation, PAT-BACKOFF-001 retry budget, PAT-QUEUE-EVENTS-001 fallback (FM-151).

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Idempotency-Key collision (double-charge) | L | L | CRITICAL | M | LOW | UUID v4 122-bit entropy + CI test 1M 0 collisions; sprint contract §15 R-003 |
| R-002 | Test mode mismatch em prod (accidental live charge) | L | L | CRITICAL | M | LOW | Worker secret env separation + compile-time assertion + ADR-0034 promotion gate |
| R-003 | Webhook signature forgery | L | L | HIGH | M | LOW | HMAC-SHA256 verify + constant-time comparison + 5min replay window |
| R-004 | Webhook replay attack | L | M | HIGH | M | LOW | 5min window + (stripe_event_id) UNIQUE dedup; sprint contract §15 R-007 |
| R-005 | Stripe API outage > 1h (FM-151) | L | H | MEDIUM | L | LOW | PAT-BACKOFF-001 + PAT-QUEUE-EVENTS-001 + RB-FM-151 dry-run |
| R-006 | PII em stripe_event_log leak (Lote 10.9-quinquies NEW-P0-2) | L | M | CRITICAL | L | LOW | Typed StripeWebhookEvent + PII wrapper Serialize impl + CI test |
| R-007 | Refund storm (10× normal/h) | L | M | MEDIUM | L | LOW | Rate limit refund handler + dispute storm detection alert SEV-2 |
| R-008 | Stripe SDK version mismatch breaking change | M | L | MEDIUM | L | LOW | Cargo.toml semver-lock + cargo-audit + integration tests every release |
| R-009 | Cargo-deny bypass (stripe crate leak) | L | L | MEDIUM | L | LOW | CI gate enforced; PR rejected if violation |
| R-010 | Stripe webhook IP whitelist drift | L | L | LOW | L | LOW | Stripe-Signature verify > IP whitelist (Stripe official guidance) |
| R-011 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | LOW | L | LOW | INV positions §3.9 (NO-LOSS L136 / NO-DUP L137) + §3.12 (RECONCILE-3-LAYER L166 / REPLAYABLE L167) verified Lote 10.10bis; ongoing maintenance discipline via grep CI gate |
| R-012 | DSR erasure breaks audit chain | M | M | HIGH | M | LOW | Pseudonymization (NOT delete) via S-11 + CTRL-PRIV-002; legal sign-off |

## 29. Review Checkpoints

D+0 design (Architect; Idempotency-Key + cargo-deny isolation); D+1 Finance (Stripe API discipline + financial integrity); D+2 Legal (DPA reference + Stripe contract terms); D+3 AppSec (HMAC-SHA256 + replay window + test mode pinning); D+4 Privacy (LINDDUN + PII wrapper inheritance from WI-S09-002); D+5 Compliance (schema versioning + audit trail + GAAP cutoff); D+6 SRE (RB-FM-151 dry-run + PAT-BACKOFF/QUEUE); D+7 code review; D+8 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — HMAC-SHA256 + replay window + test mode pinning + STRIDE_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos 12 + property test 100k + RB-FM-151 dry-run_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 CC1.4 + GAAP ASC 606 + schema versioning + audit chain stripe_event_log_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — LINDDUN + PII wrapper Serialize impl inheritance + DSR cooperation S-11_ |
| 11 | Architect | _TBD; **mandatory emphatic** — Idempotency-Key derivation canonical + cargo-deny workspace isolation + chrono primitives (Lote 10.8bis P0-D) + INV §3.X verification (Lote 10.8bis P1-13) + Lote 10.9-quinquies NEW-P0-2 absorption_ |
| 12 | Finance | _TBD; **mandatory emphatic** — Stripe API contract integrity + Idempotency-Key SEMPRE non-waivable + RB-FM-151 dry-run_ |

(Legal sign-off via DPA reference at sprint level + Stripe contract addendum; not per-WI.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.10) | Criação WI-S10-003; HIGH_RISK; SOTA pós-Lote 10.7bis + Lote 10.8bis/tris + Lote 10.9bis/tris/quaters/quinquies lessons absorbed: 5-tier canonical Plan (Lote 10.7bis P0-7); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (Lote 10.7bis P1-3); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV §3.X canonical position TBD verification before commit (Lote 10.8bis P1-13); corelink_time::next_month_first_utc_midnight() canonical primitive (Lote 10.8bis P0-D); calibration n=50+50 + 95% CI bounded (Lote 10.8bis P0-E); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis). **Lote 10.9-quinquies NEW-P0-2 critical absorption**: typed StripeWebhookEvent enum (NOT serde_json::Value); PII wrappers (CustomerEmail, CustomerName) explicit serde::Serialize impl calling Redact::redact() — prevents raw PII writing to stripe_event_log audit trail. **Lote 10.6bis split-tier inheritance**: Stripe API call fail-CLOSED with PAT-QUEUE-EVENTS-001 fallback + webhook handler fail-CLOSED on signature/replay/dup. **Lote 10.9bis P0-E inheritance**: Prom metric names underscored canonical. NEW corelink-billing crate + Stripe SDK v0.31+ wrap + StripeClient/StripeWebhookHandler traits + Idempotency-Key derivation `corelink-{tenant_id_short}-{operation}-{event_hash_short}` + test mode pinning + webhook HMAC-SHA256 verify + 5min replay window + (stripe_event_id) UNIQUE dedup + 5 Neon Postgres schemas (plan/subscription/invoice_line_item/stripe_event_log/customer_billing_profile) + monthly invoice cron canonical + PAT-BACKOFF-001 retry + PAT-QUEUE-EVENTS-001 fallback + RB-FM-151 dry-run prerequisite + BYOK encryption (S-04 inheritance) + CTRL-PRIV-002 classification + S-11 DSR pseudonymization (cooperation) + cargo-deny stripe crate workspace isolation. INV-BILLING-NO-DUP + INV-BILLING-NO-LOSS CRITICAL invariants Stripe-layer enforcement (registry position TBD). 5 PlanTier canonical (Lote 10.7bis P0-7). 5 SKUs canonical inheritance from WI-S10-001/002 (sprint contract §10 anti-scope). 5 webhook event types canonical. CTRL-BILLING-001 + SOC 2 CC1.4 + GAAP ASC 606 + LGPD Art. 32 + GDPR Art. 32 compliance Stripe-layer. |
| 1.1.0 | 2026-04-26 | Gustavo (Lote 10.10bis) | P0+P1 remediation pós-R4 Opus + Sonnet R5 adversarial reviews. **8 P0s fixed**: (1) PlanTier 5-tuple inventado `free/team/enterprise/custom/trial` → canonical `free/solo/team/business/enterprise` per `data_model.md §1` line 68 — corrige CHECK constraints em WIs 003+005 + sprint contract §5.3 R-S10-7; (2) INV registry positions estampadas `§3.X TBD` → reais §3.9 line 136 (NO-LOSS HIGH), §3.9 line 137 (NO-DUP HIGH), §3.12 line 166 (RECONCILE-3-LAYER HIGH), §3.12 line 167 (REPLAYABLE HIGH); severity drift CRITICAL→HIGH cascateia em SEV-2 alert wording (vs SEV-1); Lote 10.8bis P1-13 lesson finalmente absorvida; (3) WI-007 TLA+ math 5× errado (15k claim → real 3,000; 150k → real 30,000) + caveat sobre TLC reachable state graph ≠ Cartesian product CONSTANTS; MaxConcurrentEvents bound NEW; (4) CTRL-AUTHZ-005 alucinado (security_model só define -001/-002) → CTRL-AUTHZ-001 + CTRL-AUTHZ-002 explicit; sprint contract §5.6 R-S10-13 também atualizado; (5) WI-002 UPSERT WHERE clause auto-anulante removido (`WHERE own_digest = excluded.own_digest` rejeitava updates legítimos); (6) Cloudflare R2 Terraform schema marcada com TODO pre-merge verification (era hallucinated AWS S3 pattern `cloudflare_r2_bucket_lock_configuration`); (7) **R5 P0-A**: WI-002 PRIMARY KEY `(tenant_id, sku, hour)` → `(tenant_id, region, sku, hour)` — multi-region tenants antes corrompiam dados a cada UPSERT; (8) **R5 P0-B**: WI-001 D1 CHECK enum `('cas_put',...)` rejeitava 100% dos inserts (strum serializa para long form `dev.hugr.corelink.cas.put.v1`); aligned com Lote 10.9bis P0-G CloudEvents prefix canonical. **P1s fixed**: (a) staging table region column added (R5 P1-A); (b) WI-003 narrative 62-char idempotency key contradiziando §6.1 35-char (R5 P1-B); (c) idempotency comment WI-001 (P1-C); (d) `CHECK (age_hours > 6)` off-by-one → `>= 6` (R5 P1-D); (e) cardinality 30 regions → 5 canonical (P1-G); (f) PercentValue type/DB CHECK alignment 0..=200 (R5 P1-H); (g) chrono::next_month_first_utc_midnight() (não existe em chrono crate) → corelink_time::next_month_first_utc_midnight() canonical helper internal; (h) CTRL-PRIV-002 mis-mapped to "pseudonymization" → privacy_model.md L209 "data classification tags" + S-11 cooperation para DSR pseudonymization separada; (i) sprint contract §5.7 R-S10-15 metric names dots → underscores (Prom canonical); (j) §5.1 R-S10-1 CloudEvents prefix `corelink.usage.*` → `dev.hugr.corelink.<op>.v1`; (k) WI-005 4-state vs 5-state — GraceActive removido como state, virou flag boolean preserving sprint contract §5.5 R-S10-10 canonical; (l) failure_modes.md adicionado RB-FM-151 mapping (P1-5). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |
| 1.2.0 | 2026-04-26 | Gustavo (Lote 10.10-quaters) | P0+P1 remediation pós-R4 + R5 round-2 audits (R4 6.9/10, R5 7.8/10). Same S-08/S-09 cascade-incomplete pattern: bis ~60% complete; quaters fecha residuais. **8 P0s addressed**: (1) NEW-P0-1 spec contract §8 invariants L149-151 CRITICAL→HIGH (era source-of-truth não atualizada; severity contradicting WIs); (2) NEW-P0-2 WI-002 PK 4-tuple cascade ~30%→100% — `CounterRecord` + `LateCounterRecord` Rust structs add `region: Region` field (digest input para tamper detection per-region); 15 narrative/Gherkin/design-decision references swept (tenant_id, sku, hour) → (tenant_id, region, sku, hour); (3) NEW-P0-3 TLA+ `MaxConcurrentEvents` CONSTANT NEW added to body L58-65 + bounded em EmitEvent; `Hours` time abstraction comment clarified (R4 round-1 caveat absorbed); `quota_grace_active` separate VARIABLE (R5 NEW-P0-1 fix: grace é flag, não 5º state value); `ReconcileLayer1` action NEW with drift computation (R4 round-1 P2-10 fix); (4) WI-003 PlanTier residuals §1 item 11 L247-250 + §6.1.13 L548 swept canonical 5-tuple; (5) WI-002 Gherkin L530 UPSERT WHERE clause text fixed (R4 P0-5 partial); (6) WI-001 risk register R-001/R-002 + post-mortem hooks severity CRITICAL→HIGH; SEV-1 → SEV-2 (R4 NEW-P1-2). **P1s addressed**: (a) WI-005 PercentValue cluster L74/L178/L328 align com 0..=200 DB CHECK (R4 NEW-P1-1 + R5 NEW-P1-3); (b) WI-006 sign-off `Finance + Legal + Architect` → `Finance + Compliance Officer + Architect` (Legal sprint-level only; R4 NEW-P1-4 / round-1 P1-6 finalmente fix); (c) WI-003 título L29 + WI-005 L727 + spec contract L244 CTRL-PRIV-002 mis-mapping → "data classification tags @classification=pii" + S-11 separated DSR; (d) WI-002 §6.1.11 late event reference time → cron_now (R5 NEW-P1-1); (e) WI-002 §14 corelink_time crate path canonical reference added (`crates/corelink-time/src/canonical.rs`); (f) WI-S10-003 Idempotency-Key 64-char internal cap rationale ADR documented em line 367; (g) WI-S10-007 ADR-S10-007-tla-substitutes-proptest justifying property test exemption + lightweight sanity props NEW; (h) WI-S10-006 "Mfa" → "MFA" capitalization (30 occurrences); (i) WI-004 Layer 2/3 narrative aggregation semantics clarified (collapsed across regions for Stripe scope; per-region drift preservada via Layer 1). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |
| 1.4.0 | 2026-05-03 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S10-003 SEAL; no per-WI codex per 2026-04-30 protocol — sprint-close Sonnet review covers) | **WI-S10-003 IMPL SEALED** — `crates/corelink-billing-stripe/` ships pure-logic skeleton per `trait-abstraction-defer` charter pattern (production HTTP client + real Stripe API endpoint POST `/v1/subscription_items/{id}/usage_records` + Cloudflare Worker route `POST /v1/webhooks/stripe` + Worker secret `CORELINK_STRIPE_MODE=test\|live` + `STRIPE_TEST_SECRET_KEY` / `STRIPE_LIVE_SECRET_KEY` / `STRIPE_WEBHOOK_SECRET` env separation + PAT-BACKOFF-001 retry budget + PAT-QUEUE-EVENTS-001 fallback + RB-FM-151 dry-run + ADR-0034 promotion gate deferred to WI-S10-007 PRR ship gate). Nine modules: `event` (IdempotencyKey 32-byte BLAKE3-256 newtype + SubscriptionItemId newtype + UsageRecordRequest typed payload + StripeAdapterDecision `#[non_exhaustive]` 4-element taxonomy [UsageRecorded / DuplicateRejected / WebhookProcessed / SignatureRejected] + WebhookEventKind `#[non_exhaustive]` 5-element taxonomy [InvoiceCreated / InvoicePaid / InvoiceFailed / SubscriptionUpdated / CustomerCreated; canonical Stripe event-type strings] + WebhookEvent typed payload); `idempotency` (compute_canonical_aggregate_bytes RFC 8785 JCS + derive_idempotency_key BLAKE3-256 of canonical AggregatedCounter bytes — same aggregate → same key by construction; WI-S10-002 input source); `signature` (StripeSignatureHeader::parse `t=<unix_seconds>,v1=<hex>` with multi-v1 key-rotation tolerance + unknown-scheme forward-compat + compute_signature HMAC-SHA256 over `<ts>.<payload>` per Stripe spec + verify_stripe_signature with `subtle::ConstantTimeEq::ct_eq` constant-time compare against EVERY v1 candidate + canonical 5-min `REPLAY_WINDOW_MS = 300_000` boundary enforcement per Stripe webhook signature documentation); `audit` (StripeAuditEventType `#[non_exhaustive]` 6-event taxonomy `corelink.billing_stripe.{usage_recorded, duplicate_rejected, webhook_received, signature_rejected, signature_verified, signature_skew_rejected}` + StripeAuditSink trait + InMemoryStripeAuditSink + FailingStripeAuditSink fail-CLOSED Lote 10.6bis); `ledger` (StripeUsageLedger trait + InMemoryStripeUsageLedger UPSERT-safe ledger + per-IdempotencyKey UNIQUE PK INV-BILLING-NO-DUP enforcement + canonical-bytes-divergence StripeUsageLedgerError::IdempotencyKeyReuse SEV-1 surface + RecordOutcome Recorded/AlreadyExistsIdempotent + FailingStripeUsageLedger); `webhook_log` (StripeWebhookLog trait + InMemoryStripeWebhookLog + (stripe_event_id) UNIQUE PK + WebhookInsertOutcome Inserted/AlreadyExists + FailingStripeWebhookLog); `adapter` (StripeBillingAdapter trait + InMemoryStripeBillingAdapter orchestrator: canonicalize → derive_idempotency_key → audit BEFORE ledger mutation [usage_recorded / duplicate_rejected arm] → ledger record); `webhook` (StripeWebhookHandler trait + InMemoryStripeWebhookHandler orchestrator: audit `webhook_received` BEFORE any verify → verify_stripe_signature → audit `signature_verified` / `signature_rejected` / `signature_skew_rejected` → webhook log INSERT); `error` (StripeError + StripeAuditSinkError + StripeUsageLedgerError + StripeWebhookLogError canonical `#[non_exhaustive]` taxonomies). Migration `migrations/d1/0018_stripe_idem_keys.sql` ships canonical `stripe_idempotency_keys` PRIMARY KEY (idempotency_key) UNIQUE [INV-BILLING-NO-DUP storage layer; mirrors Stripe 24h idempotency window] + `stripe_event_log` PRIMARY KEY (stripe_event_id) UNIQUE [INV-AUDIT-APPEND-ONLY; webhook redelivery dedup per Stripe spec] + 5-element webhook event_type CHECK constraint + `evt_*` substr CHECK + 4 indexes + 11 inline CHECK constraints. Tests: 90 inline unit + 20 integration (10 property tests at 10k iter PR-gate via `PROPTEST_CASES` env-var read at runtime: `prop_idempotency_key_deterministic` + `prop_idempotency_key_diverges_per_aggregate` + `prop_webhook_signature_verifies_valid` + `prop_webhook_signature_rejects_expired` + `prop_webhook_signature_rejects_tampered_payload` + `prop_webhook_signature_rejects_tampered_signature` + `prop_constant_time_signature_compare` + `prop_tenant_isolation` + `prop_audit_emit_per_decision_arm` + `prop_replay_window_exact_5min_boundary` + 10 sanity / canonical-surface pinning / fail-CLOSED arm). Quality gates green: cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings, cargo test -p corelink-billing-stripe --all-targets (110/110), validate_specs.py (280/286), check_migrations_additive.py (18 files OK). Charter compliance: zero unsafe / unwrap / expect / panic / indexing in lib code; per-instance `Arc<Mutex<>>` F-001 closure; `#[non_exhaustive]` on every public enum; wasm32-clean (no tokio in src/); audit fail-CLOSED envelope BEFORE state mutation on every decision arm; ChaCha20Rng PRNG pinned for randomized fixtures; `prop_assert!(let valid = matches!...)` pattern (S-08 P1-1 fix); constant-time signature compare via `subtle::ConstantTimeEq` (timing-attack defense); `hmac` + `sha2` + `subtle` workspace deps already wired (S-03 inheritance); BLAKE3-256 canonical hash family inheritance (CAS S-01 / AC S-04 / dedup S-07 / audit chain S-09 / billing-emit WI-S10-001 / billing-aggregator WI-S10-002). Production HTTP / Worker route / cargo-deny stripe-crate isolation / typed `serde::Serialize` Redact-wrapping PII / monthly invoice cron / Neon Postgres `plan` + `subscription` + `invoice_line_item` + `customer_billing_profile` schemas / Stripe Price IDs catalog / Terraform IaC all deferred to WI-S10-007 PRR ship gate per `trait-abstraction-defer` charter pattern. |
| 1.3.0 | 2026-04-26 | Gustavo (Lote 10.10-sextus) | P0+P1 remediation pós-R4 + R5 round-3 audits (R4 8.0/10, R5 9.1/10 CONDITIONAL SEAL). 2 P0s + 8 P1s residuais addressed. **2 P0s**: (1) R4 NEW-P0-1 severity cascade incompleto — quaters só varreu WI-001; sextus completou WI-002 (L745-746 post-mortem + L786 R-001) + WI-003 (L869-870) + WI-007 (L743 + L785 R-005) — todos `INV-BILLING-NO-LOSS/NO-DUP` violation references → HIGH-severity post-mortem + SEV-2 alert (não SEV-1). (2) R4 NEW-P0-2 WI-S10-004 Layer 1 ainda 3-tuple `(tenant, sku, hour)` → 4-tuple `(tenant, region, sku, hour)` em título L41 + trait doc L67 + narrative L217 + Gherkin L521 + sprint contract §5.2 R-S10-4 line 86 + CAP-BILLING-002 line 65 — mascarava multi-region drift via cross-region offsets (financial-grade integrity restored). **8 P1s**: (a) R5 P1-QUIN-1 WI-002 L538 + step 5 Gherkin/narrative `hour_window.start_ts` → `cron_now`; (b) R5 P1-QUIN-2 WI-001 L210 §1 item 9 idempotency key long-form → 35-char canonical; (c) R4 P1-1 WI-003 título L41 + ST-010 sub-task CTRL-PRIV-002 pseudonymization → "data classification tags @classification=pii" + S-11 separated DSR; (d) R4 P1-2 WI-006 §29 D+3 review checkpoint Legal sign-off → sprint-level only (NÃO 3-of-3 per-replay; matched §6.1.6 fix); (e) R4 P1-3 spec contract metadata bumped v1.1.0/sota-v1.1/2026-04-24 → v1.3.0/sota-v1.3/2026-04-26; (f) R4 P1-4 + R5 P1-QUIN-3 TLA+ block: `BillingPeriods` + `RequestIds` agora declared CONSTANTS; helper operators (`Abs`, `SumCounters`, `SumLineItems`, `ComputeStripeInvoiceId`, `StripeIdParts`, `EventAccountedInInvoice`) documented as separate INSTANCE module; (g) R5 P1-QUIN-4 INV_BILLING_NO_LOSS retry_queue Sequence membership bug fixed via `RetrySet == {retry_queue[i] : i \in DOMAIN retry_queue}` Range conversion (semantic correctness); (h) R4 P1-5 `compute_counter_digest()` doc explicitly confirms region IS in canonical_json input (cross-region tamper detection guaranteed); (i) R4 P1-6 §14.s10.x.8 + R-010/R-011 risk register stale "INV §3.X TBD pre-merge" → concrete §3.9 L136/137 + §3.12 L166/167 verified Lote 10.10bis (residual após mitigação LOW). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |

## 32. Anti-patterns evitados

- ❌ serde_json::Value em StripeWebhookEvent (Lote 10.9-quinquies NEW-P0-2; typed canonical); ❌ Idempotency-Key omitted (sprint contract §19 NÃO-waivable); ❌ Webhook signature verification skip (replay attack); ❌ Test mode mismatch em CI/dev (compile-time assertion); ❌ Live key em CI environment (Worker secret separation); ❌ Stripe primitives leaking via API public (cargo-deny enforces); ❌ Raw PII em stripe_event_log (PII wrapper Serialize impl Redact-wraps); ❌ ad-hoc `now() + Duration::days(30)` (corelink_time::next_month_first_utc_midnight canonical Lote 10.8bis P0-D); ❌ Per-feature SKU expansion beyond 5 canonical (sprint contract §10); ❌ Plan tier expansion beyond 5 canonical (Lote 10.7bis P0-7); ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3); ❌ TenantCtx bypass; ❌ Reverse engineering Stripe API; ❌ DELETE row em customer_billing_profile DSR (pseudonymization preserves audit chain); ❌ Skip RB-FM-151 dry-run (sprint contract §6 DoD prerequisite).

---

**Fim WI-S10-003.** Próximo: WI-S10-004 (Reconciliation worker daily 3-layer + drift alerts).
