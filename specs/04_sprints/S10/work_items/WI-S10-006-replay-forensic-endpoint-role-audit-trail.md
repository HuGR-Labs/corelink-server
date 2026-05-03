---
id: "WI-S10-006"
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
  - "PRIVACY-MODEL"
tags: ["wi", "s10", "replay", "forensic", "billing", "billing-admin-role", "audit-trail", "high-risk"]
---

# WI-S10-006 — Replay Forensic Endpoint `POST /v1/billing/replay` + Role `billing_admin` + Audit Trail (`crates/corelink-billing-replay/`; sprint contract §5.6 R-S10-12/13 + INV-BILLING-REPLAYABLE-FROM-EVENTS NEW invariant §8; reconstructs invoice byte-a-byte from R2 raw events 7y archive cooperation com WI-S10-001; `dry_run` parameter default `true` — explicit `false` requires Finance + Compliance Officer + Architect 3-of-3 sign-off audit event; role protection CTRL-AUTHZ-001 + CTRL-AUTHZ-002 `billing_admin` mandatory + S-03 RBAC inheritance; per-call audit event mandatory `dev.hugr.corelink.billing.replay.requested.v1` CloudEvents v1.0 emit to S-09 audit chain; replay output: counter aggregates re-derived + invoice line items reconstructed + Stripe invoice diff against fetched current state via WI-S10-003 fetch_invoice cooperation; comparison report 3-mode: events_match=true reconciliation green; events_match=false drift detected; output schema typed `ReplayReport` NOT serde_json::Value Lote 10.9-quinquies NEW-P0-2; rate limit replay endpoint per `billing_admin` user 10 req/h sprint contract §15 R-007 mitigation against forensic abuse + S-08 cooperation; reasonable SLA `< 30min` reconstruction per sprint contract §6 DoD + §14.s10.3 audit-grade replay; INV-BILLING-REPLAYABLE-FROM-EVENTS HIGH invariant — auditor-grade SOC 2 CC1.4 evidence trail; documented runbook RB-BILLING-001 audit replay procedure)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-10](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S10-006 |
| Título | Replay forensic endpoint `POST /v1/billing/replay?invoice_id=X&dry_run=true|false` (sprint contract §5.6 R-S10-12 + R-S10-13 + §8 INV-BILLING-REPLAYABLE-FROM-EVENTS NEW); reconstrói invoice byte-a-byte from R2 raw events 7y archive cooperation com WI-S10-001 (events source-of-truth) → re-derives counter aggregates (WI-S10-002 logic) → reconstructs invoice_line_item (WI-S10-003 logic) → diffs against current Stripe invoice (WI-S10-003 fetch_invoice cooperation); `dry_run=true` default + `dry_run=false` requires Finance + Compliance Officer + Architect 3-of-3 sign-off audit event; role protection CTRL-AUTHZ-001 + CTRL-AUTHZ-002 `billing_admin` mandatory + S-03 RBAC inheritance + MFA required; per-call audit event mandatory CloudEvents v1.0 `dev.hugr.corelink.billing.replay.requested.v1` emit to S-09 audit chain (WI-S09-004 inheritance); typed `ReplayReport` payload NOT serde_json::Value (Lote 10.9-quinquies NEW-P0-2); 3-mode comparison: `green` events_match + `drift_detected` + `tampering_signal`; Stripe API pull cooperation com PAT-BACKOFF-001 + PAT-QUEUE-EVENTS-001 fallback; rate limit replay endpoint per billing_admin user 10 req/h (sprint contract §15 R-007 mitigation against forensic abuse via S-08 R-S08-3 inheritance); SLA reconstruction < 30min p99 (sprint contract §6 DoD + §14.s10.3); RB-BILLING-001 documented runbook audit replay procedure; INV-BILLING-REPLAYABLE-FROM-EVENTS HIGH invariant (registry §3.12 line 167); 12 sign-offs HIGH_RISK |
| Sprint | S-10 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-BILLING-001 financial integrity audit-grade replay; bypass = audit failure SOC 2), FF-HR-009 (replay = customer dispute resolution legal evidence; bug = legal indefensibility) |

## 1. Intent

Replay forensic endpoint é **the audit-grade dispute resolution primitive** que enables CoreLink Finance + customer + legal mediator to reconstruct any invoice byte-a-byte from R2 raw events archive. Sem replay endpoint, customer disputing $X invoice has zero independent verification — only "trust us" stance vs Stripe ledger; legally indefensible em SOC 2 audit + GDPR Art. 22 (automated decision review). INV-BILLING-REPLAYABLE-FROM-EVENTS NEW invariant (sprint contract §8) requires every invoice reconstructable em < 30min from immutable 7y events archive.

```rust
// File: crates/corelink-billing-replay/src/replay.rs

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};

#[async_trait]
pub trait ReplayService: Send + Sync {
    /// Reconstruct invoice from R2 raw events (cooperation WI-S10-001 + WI-S10-002 + WI-S10-003).
    /// Role protection: billing_admin mandatory.
    /// Audit event mandatory: dev.hugr.corelink.billing.replay.requested.v1
    async fn replay_invoice(
        &self,
        request: ReplayRequest,
        admin_ctx: AdminCtx,                            // CTRL-AUTHZ-001 + CTRL-AUTHZ-002 billing_admin role + MFA
    ) -> Result<ReplayReport, ReplayError>;

    /// Verify hash chain integrity for replay period (cooperation WI-S10-002 hash chain).
    /// Detects tampering between event emission and replay.
    async fn verify_hash_chain_for_period(
        &self,
        billing_period: BillingPeriod,                  // YYYY-MM canonical
        region: Region,                                 // 5 canonical
    ) -> Result<HashChainVerification, ReplayError>;

    /// Compare reconstructed invoice against current Stripe invoice (cooperation WI-S10-003 fetch_invoice).
    /// Returns drift report or green status.
    async fn diff_against_stripe(
        &self,
        reconstructed: ReconstructedInvoice,
        stripe_invoice_id: StripeInvoiceId,
    ) -> Result<DiffReport, ReplayError>;
}

#[derive(serde::Deserialize)]
pub struct ReplayRequest {
    pub invoice_id: InvoiceId,                          // internal Neon invoice_line_item lookup
    pub stripe_invoice_id: Option<StripeInvoiceId>,     // override; default lookup via invoice_id
    pub dry_run: bool,                                  // default true (sprint contract §5.6 R-S10-12)
    pub admin_reason: AdminReason,                      // typed enum NOT String
    pub mfa_token: MfaToken,                            // S-03 inheritance
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum AdminReason {
    CustomerDisputeInvestigation { dispute_id: String },
    SocAuditEvidenceReconstruction { audit_period: String },
    InternalReconciliationDriftRootCause { layer: u8 },
    LegalDiscoveryRequest { case_id: String },
    TamperingInvestigation { hash_chain_violation_event_id: String },
    PostmortemBillingDriftDetection { incident_id: String },
}

#[derive(serde::Serialize)]
pub struct ReplayReport {
    pub invoice_id: InvoiceId,
    pub stripe_invoice_id: StripeInvoiceId,
    pub billing_period: BillingPeriod,
    pub tenant_id: TenantId,                            // pseudonym
    pub reconstructed_invoice: ReconstructedInvoice,
    pub stripe_current_snapshot: StripeInvoiceSnapshot,
    pub diff_report: DiffReport,
    pub hash_chain_verification: HashChainVerification,
    pub events_processed: u64,
    pub late_events_excluded: u64,                      // routed to usage_counter_late upstream
    pub overall_status: ReplayStatus,                   // green | drift_detected | tampering_signal
    pub generated_at: DateTime<Utc>,
    pub generated_by: AdminId,                          // audit
    pub generation_duration_seconds: u32,               // SLA tracking
    pub audit_event_id: CloudEventId,                   // S-09 chain reference
}

#[derive(serde::Serialize)]
pub enum ReplayStatus {
    Green {                                              // events ↔ counter ↔ invoice ↔ Stripe all match
        max_drift_pct: f64,                             // ≤ 0.001 (0.1%) effective zero
    },
    DriftDetected {
        drift_pct: f64,                                  // > 0.1%
        layer_with_drift: u8,                           // 1, 2, or 3
        per_sku_drift: BTreeMap<Sku, f64>,
    },
    TamperingSignal {
        hash_chain_violation_detected: bool,            // recomputed ≠ stored
        admin_insert_suspected: bool,
        sev1_alert_emitted: bool,
    },
}

#[derive(serde::Serialize)]
pub struct ReconstructedInvoice {
    pub line_items: Vec<ReconstructedLineItem>,
    pub total_usd_cents: u64,
    pub event_count: u64,
    pub regions_aggregated: Vec<Region>,
    pub computed_via_canonical_methods: bool,           // true = exact same code path as WI-S10-002 + WI-S10-003
}

#[derive(serde::Serialize)]
pub struct ReconstructedLineItem {
    pub sku: Sku,                                       // 5 canonical inheritance
    pub quantity: f64,                                  // canonical aggregate
    pub unit_price_usd_cents: u32,
    pub total_usd_cents: u64,
}

#[derive(serde::Serialize)]
pub struct DiffReport {
    pub line_items_match: bool,
    pub total_match: bool,
    pub max_drift_pct: f64,
    pub drift_per_sku: BTreeMap<Sku, f64>,
}

#[derive(serde::Serialize)]
pub struct HashChainVerification {
    pub period_chain_intact: bool,
    pub stored_chain_head: String,                      // BLAKE3-256 hex
    pub recomputed_chain_head: String,
    pub anomalies_detected: Vec<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum ReplayError {
    #[error("billing_admin role required (CTRL-AUTHZ-001 + CTRL-AUTHZ-002); got role={got_role}")]
    RoleProtectionViolation { got_role: String },

    #[error("MFA token required for billing_admin endpoint (S-03 RBAC); token missing")]
    MfaTokenMissing,

    #[error("3-of-3 sign-off required for dry_run=false replay (Finance+Compliance+Architect): missing={missing:?}")]
    DryRunFalseSignoffMissing { missing: Vec<String> },

    #[error("Invoice not found: invoice_id={0}")]
    InvoiceNotFound(String),

    #[error("R2 events archive read failed (cooperation WI-S10-001): {0}")]
    R2ReadFailed(String),

    #[error("Hash chain verification failed (tampering signal CRITICAL): {0}")]
    HashChainVerificationFailed(String),

    #[error("Stripe API pull failed (cooperation WI-S10-003 fetch_invoice; FM-151): {0}")]
    StripeApiFailed(String),

    #[error("Replay rate limit exceeded (10 req/h per billing_admin; sprint contract §15 R-007): retry_after={retry_after_seconds}s")]
    RateLimitExceeded { retry_after_seconds: u32 },

    #[error("Reconstruction SLA violation: duration={duration_seconds}s > 1800s (sprint contract §6 DoD)")]
    SlaViolation { duration_seconds: u32 },

    #[error("Audit event emission failed (CloudEvents to S-09 chain): {0}")]
    AuditEmissionFailed(String),
}
```

**Cripto-driven invariants enforced**:

1. **INV-BILLING-REPLAYABLE-FROM-EVENTS** (HIGH; sprint contract §8 NEW; registry §3.12 line 167 — verify via grep before commit per Lote 10.8bis P1-13):
   - Any invoice reconstructable byte-a-byte from R2 raw events archive em ≤ 30min p99.
   - SOC 2 CC1.4 audit-grade evidence requirement.
   - Replay endpoint test em CI mensal (sprint contract §8 INV note "test em CI mensal").

2. **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven Lote 6.2; S-09 inheritance):
   - Replay request audit event emit to S-09 audit chain (WI-S09-004 cooperation).
   - Event type: `dev.hugr.corelink.billing.replay.requested.v1`.
   - Event chain immutable 7y; auditor reviewing replay history reconstructable.

3. **CTRL-AUTHZ-001 + CTRL-AUTHZ-002** (security_model.md): `billing_admin` role-protected; sensitive operation;MFA mandatory.

4. **CTRL-BILLING-001** (security_model.md): financial integrity replay-from-events requirement.

5. **CTRL-PRIV-002** (privacy_model.md L209 — Data classification tags): @classification=pii enforced em billing tables; replay output access role-protected (S-11 DSR pseudonymization is separate procedure).

6. **Lote 10.9-quinquies NEW-P0-2 lesson absorbed**:
   - Typed `ReplayReport` + `ReconstructedInvoice` + `DiffReport` (NOT serde_json::Value).
   - PII redaction via WI-S10-003 wrapper inheritance (CustomerEmail, CustomerName).
   - Replay output NEVER includes raw PII; pseudonym tenant_id only.

7. **Cooperation com WI-S10-001 + WI-S10-002 + WI-S10-003** (full pipeline replay):
   - WI-S10-001: R2 events archive source-of-truth (Object Lock 7y).
   - WI-S10-002: counter aggregator logic re-applied (deterministic).
   - WI-S10-003: invoice generation logic re-applied + Stripe API pull (fetch_invoice).

8. **3-of-3 sign-off for dry_run=false** (security control):
   - dry_run=true (default): no live action; comparison only; safe.
   - dry_run=false: would write reconstructed invoice em Neon overriding existing — requires Finance + Compliance Officer + Architect 3-of-3 sign-off audit event before allowing.
   - Sign-off via separate workflow; replay endpoint validates audit event chain.

9. **Rate limit replay endpoint** (sprint contract §15 R-007 mitigation):
   - 10 req/h per billing_admin user.
   - S-08 R-S08-3 multi-layer rate limit cooperation.
   - Forensic abuse detection: > 100 req/day = SEV-3 alert (audit pattern).

10. **SLA reconstruction < 30min p99** (sprint contract §6 DoD + §14.s10.3):
    - R2 events archive read + counter re-aggregation + Stripe API pull em parallel.
    - Performance test 30d staging.
    - SlaViolation error if duration > 30min (alert SEV-2).

11. **5 SKUs canonical** (sprint contract §10 anti-scope; inheritance WI-S10-001/002/003).

12. **5 PlanTier canonical** (Lote 10.7bis P0-7).

13. **5 regions canonical** (sprint contract §17 inheritance).

14. **TenantCtx propagation** (Lote 10.4bis): admin_ctx authenticated via S-03 middleware; billing_admin role + MFA mandatory.

15. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): `worker::send_future`; R2 + Neon + Stripe HTTP via wasm-bindgen; NEVER `tokio::spawn`.

16. **corelink_time::next_month_first_utc_midnight() canonical primitive** (Lote 10.8bis P0-D): billing_period boundary alignment.

17. **CloudEvents v1.0 audit emission** (Lote 10.9bis P0-G canonical inheritance):
    - `dev.hugr.corelink.billing.replay.requested.v1` emit on EVERY replay request.
    - Subject: `billing_admin:<admin_id>` (audit per-user).
    - Data typed `ReplayAuditData` enum.
    - Sink: WI-S09-004 audit chain R2 Object Lock 7y.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + audit-grade replay discipline justification)

Replay forensic endpoint é **the legal-defensibility primitive** do CoreLink billing pipeline. SOC 2 CC1.4 + GDPR Art. 22 + GAAP ASC 606 require any automated billing decision to be auditable + reconstructable + customer-disputable with independent verification. Sem replay endpoint, customer disputing $X invoice has zero verification path — only "trust CoreLink ledger" stance; legally indefensible em audit + customer dispute litigation.

**Why dry_run=true default** (sprint contract §5.6 R-S10-12): replay endpoint capabilities are powerful — could overwrite invoice generation if dry_run=false. Default safety: read-only comparison; explicit opt-in for write semantics with 3-of-3 sign-off (Finance + Compliance Officer + Architect). Defense em depth.

**Why role `billing_admin` + MFA + audit event mandatory** (CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + sprint contract §5.6 R-S10-13): replay output reveals customer billing details (pseudonymized but linkable for billing_admin); operation must be auditable per-call. MFA prevents lateral compromise; audit event creates immutable evidence of who-replayed-what-when.

**Why typed `ReplayReport` payload + PII wrapper inheritance** (Lote 10.9-quinquies NEW-P0-2 critical lesson): replay output written to logs + audit chain + Finance review files; without typed enum, customer email could leak via serde_json::Value structural permissiveness. PII wrappers (CustomerEmail, CustomerName) inherit explicit Serialize impl from WI-S09-002/WI-S10-003 calling Redact::redact() — raw PII never reaches replay output.

**Why cooperation com WI-S10-001 + WI-S10-002 + WI-S10-003** (deterministic reconstruction): replay must use **exact same code paths** as production aggregation/invoice generation — otherwise reconstruction differs from "what should have been" and detection becomes ambiguous. `computed_via_canonical_methods: bool` field validates: replay invokes WI-S10-002 counter aggregation logic + WI-S10-003 invoice line item generation logic + WI-S10-003 fetch_invoice; same code, same output (deterministic).

**Why 3-mode comparison Status Green/DriftDetected/TamperingSignal** (audit clarity):
- **Green**: events_match (counter ↔ invoice ↔ Stripe all match em pseudo-byte-a-byte tolerance ≤ 0.1%); customer dispute resolved (CoreLink data correct).
- **DriftDetected**: drift > 0.1%; root-cause analysis required; refund decision possible; reconciliation Layer 1/2/3 may have missed (rare).
- **TamperingSignal**: hash chain re-verification fails; admin INSERT bypass suspected; SEV-1 imediato; CRITICAL post-mortem; CTRL-BILLING-001 audit chain integrity review.

**Why rate limit 10 req/h per billing_admin** (sprint contract §15 R-007): forensic abuse pattern — billing_admin user (compromised or insider threat) could replay all customer invoices to extract billing details; rate limit + S-08 cooperation detects pattern; > 100/day = SEV-3 alert.

**Why SLA < 30min p99 reconstruction** (sprint contract §6 DoD + §14.s10.3): customer dispute investigation often time-sensitive (chargeback dispute deadline); auditor-grade SOC 2 walkthrough requires replay em < 30min for SOC 2 control evidence.

**Why CI mensal replay endpoint test** (sprint contract §8 INV note): INV-BILLING-REPLAYABLE-FROM-EVENTS verification — generate fake invoice → run replay → output match expected; catches regressions early.

**Adversarial scenarios**:
- **Customer dispute (chargeback)**: customer claims overcharge for billing_period 2026-09; Finance triggers replay endpoint; events ↔ counter ↔ invoice ↔ Stripe all match → dispute resolved with audit trail.
- **Tampering attempt detected**: admin INSERT bypass to usage_counter; replay hash chain verification detects mismatch → SEV-1 alert; CRITICAL post-mortem; CTRL-BILLING-001 violation audit.
- **Forensic abuse via billing_admin**: compromised billing_admin runs > 10 req/h → rate limit blocks; > 100/day → SEV-3 alert; pattern detection + investigation.
- **dry_run=false unauthorized**: developer attempts dry_run=false sin 3-of-3 sign-off audit event → DryRunFalseSignoffMissing error; SEV-2 alert; potential insider threat signal.
- **Stripe API outage during Layer 3 cooperation**: PAT-BACKOFF-001 retry; PAT-QUEUE-EVENTS-001 fallback; SLA may breach 30min → SEV-2 alert.
- **MFA bypass attempt**: MfaTokenMissing error; 401 returned; CRITICAL audit event.
- **Customer DSR erasure mid-replay**: pseudonymization preserved (CTRL-PRIV-002 + S-11 cooperation); replay continues with pseudonym tenant_id; no raw PII em output.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: CTRL-BILLING-001 financial integrity audit-grade replay; bypass = audit failure SOC 2.
- **FF-HR-009**: replay = customer dispute resolution legal evidence; bug = legal indefensibility.
- 12 sign-offs (Finance + Legal + Privacy + Architect + Compliance Officer emphatic) + chaos suite + property test 100k replay + RB-BILLING-001 documented runbook.

## 3. Customer Impact & Journey

**Persona 1 — Customer disputing chargeback $X**: Stripe webhook charge.dispute.created → CoreLink Customer Success triggers replay → events ↔ counter ↔ invoice ↔ Stripe verified → dispute response com audit trail submitted to Stripe.

**Persona 2 — Finance auditor reconstructing 1 invoice em < 30min** (sprint contract §6 DoD SOC 2 walkthrough): admin invokes POST /v1/billing/replay; receives ReplayReport com green status; auditor signs SOC 2 control evidence.

**Persona 3 — Legal Discovery request**: subpoena requires invoice reconstruction for Q3 2026; admin invokes replay for each invoice em scope; signed reports provided to legal team com audit trail.

**Persona 4 — Internal reconciliation drift root-cause investigation**: WI-S10-004 daily reconciliation detected drift; Engineer invokes replay endpoint to identify which layer (1, 2, or 3) introduced drift; commits fix; re-reconcile → green.

**Persona 5 — Customer Success investigating customer billing question**: customer asks "why is my invoice $X?"; Customer Success runs replay (read-only dry_run=true); shares per-SKU breakdown com customer; transparency.

**Persona 6 — Tampering investigation post-mortem**: hash chain violation detected (WI-S10-002 + WI-S10-004 cooperation); replay invoked com TamperingInvestigation reason; full forensic timeline reconstructed from R2 raw events; investigation evidence preserved.

**Persona 7 — Compliance Officer monthly CI replay test**: scheduled replay against fake invoice; output matches expected; INV-BILLING-REPLAYABLE-FROM-EVENTS verified monthly.

**SLA addendum**:
- Reconstruction latency: ≤ 30min p99 (sprint contract §6 DoD + §14.s10.3).
- Audit event emission: ≤ 100ms p99 (CloudEvents to S-09 chain).
- Rate limit per billing_admin: 10 req/h (sprint contract §15 R-007).
- INV-BILLING-REPLAYABLE-FROM-EVENTS: 100% invoices reconstructable; CI mensal test.
- Replay output retention: 7y (R2 Object Lock; cooperation WI-S09-004 audit pattern).

## 4. Capability Mapping

- **CAP-BILLING-007** (Replay-from-events forensic API) — IMPLEMENTA primary.
- Trace: `data_model.md` (replay_report schema + invoice_line_item + usage_event sources) + `security_model.md CTRL-BILLING-001 + CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + CTRL-PRIV-002` + `invariant_registry.md INV-BILLING-REPLAYABLE-FROM-EVENTS + INV-AUDIT-APPEND-ONLY` + sprint contract §5.6 (R-S10-12/13) + §8 NEW INV + Stripe Billing Architecture Guide + GAAP ASC 606 + SOC 2 CC1.4.

## 5. Tipo

Replay service Rust crate + Worker route `POST /v1/billing/replay` + Neon Postgres replay_audit_log schema + CloudEvents emit cooperation S-09 + R2 reports archive + RB-BILLING-001 documented runbook; HIGH_RISK; FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-billing-replay/` module** — `ReplayService` trait + impls + tests.

2. **Worker route `POST /v1/billing/replay`**:
   ```rust
   pub async fn replay_handler(req: Request, ctx: TenantCtx) -> Result<Response, Error> {
       // Step 1: extract admin_ctx (S-03 + CTRL-AUTHZ-001 + CTRL-AUTHZ-002 billing_admin role + MFA)
       let admin_ctx = ctx.require_role(&Role::BillingAdmin)?
                         .require_mfa()?;

       // Step 2: rate limit (S-08 cooperation + sprint contract §15 R-007)
       check_rate_limit_per_admin(&admin_ctx, RateLimitScope::ReplayEndpoint, 10, Duration::hours(1)).await?;

       // Step 3: parse + validate request
       let request: ReplayRequest = req.json().await?;

       // Step 4: dry_run=false guard
       if !request.dry_run {
           verify_3_of_3_signoff(&admin_ctx, &request).await?;  // Finance + Compliance Officer + Architect
       }

       // Step 5: emit audit event (CloudEvents v1.0 to S-09 chain)
       let audit_event_id = emit_audit_event(
           "dev.hugr.corelink.billing.replay.requested.v1",
           &admin_ctx,
           &request,
       ).await?;

       // Step 6: execute replay
       let report = replay_service.replay_invoice(request, admin_ctx).await?;

       // Step 7: return JSON response
       Ok(Response::json(&report)?)
   }
   ```

3. **Replay logic (deterministic reconstruction)**:
   ```rust
   async fn replay_invoice(&self, req: ReplayRequest, admin: AdminCtx) -> Result<ReplayReport, ReplayError> {
       let start = Utc::now();

       // Step 1: lookup invoice via Neon
       let invoice = neon_query_invoice(&req.invoice_id).await?;
       let billing_period = invoice.billing_period;
       let tenant_id = invoice.tenant_id;
       let region = invoice.region;

       // Step 2: read R2 raw events for billing_period (cooperation WI-S10-001)
       let raw_events = r2_read_events_for_period(region, billing_period).await?;

       // Step 3: re-aggregate using WI-S10-002 logic (deterministic)
       let counters_reconstructed = wi_s10_002::aggregate_for_period(raw_events).await?;

       // Step 4: re-generate invoice_line_item using WI-S10-003 logic
       let line_items_reconstructed = wi_s10_003::build_invoice_line_items(counters_reconstructed).await?;

       // Step 5: fetch current Stripe invoice (cooperation WI-S10-003)
       let stripe_snapshot = wi_s10_003::fetch_invoice(invoice.stripe_invoice_id).await?;

       // Step 6: hash chain verification (cooperation WI-S10-002)
       let hash_chain = self.verify_hash_chain_for_period(billing_period, region).await?;

       // Step 7: 3-mode comparison
       let diff = self.diff_against_stripe(reconstructed_invoice, invoice.stripe_invoice_id).await?;
       let overall_status = if !hash_chain.period_chain_intact {
           ReplayStatus::TamperingSignal { ... }
       } else if diff.max_drift_pct > 0.001 {
           ReplayStatus::DriftDetected { ... }
       } else {
           ReplayStatus::Green { max_drift_pct: diff.max_drift_pct }
       };

       // Step 8: SLA check
       let duration_seconds = (Utc::now() - start).num_seconds() as u32;
       if duration_seconds > 1800 {
           // Alert SEV-2 but still return report
           emit_metric("corelink_billing_replay_sla_violations_total", 1.0);
       }

       // Step 9: assemble report
       Ok(ReplayReport {
           invoice_id: req.invoice_id,
           stripe_invoice_id: invoice.stripe_invoice_id,
           billing_period,
           tenant_id,
           reconstructed_invoice,
           stripe_current_snapshot: stripe_snapshot,
           diff_report: diff,
           hash_chain_verification: hash_chain,
           events_processed: raw_events.len() as u64,
           late_events_excluded: ..., // from WI-S10-002 split
           overall_status,
           generated_at: Utc::now(),
           generated_by: admin.admin_id(),
           generation_duration_seconds: duration_seconds,
           audit_event_id,
       })
   }
   ```

4. **Role protection CTRL-AUTHZ-001 + CTRL-AUTHZ-002** (sprint contract §5.6 R-S10-13):
   - `billing_admin` role required.
   - MFA mandatory (S-03 inheritance).
   - JWT claim verification: `roles.contains("billing_admin") && mfa_verified == true && mfa_token_age < 5min`.

5. **Audit event emission** (CloudEvents v1.0; Lote 10.9bis P0-G canonical inheritance):
   - Event type: `dev.hugr.corelink.billing.replay.requested.v1`.
   - Subject: `billing_admin:<admin_id>`.
   - Data typed `ReplayAuditData`:
     ```rust
     #[derive(serde::Serialize)]
     pub struct ReplayAuditData {
         pub admin_id: AdminId,
         pub invoice_id: InvoiceId,
         pub stripe_invoice_id: StripeInvoiceId,
         pub billing_period: BillingPeriod,
         pub dry_run: bool,
         pub admin_reason: AdminReason,                  // typed enum
         pub source_ip_pseudonym: IpPseudonym,           // S-09 inheritance redact-wrapped
     }
     ```
   - Sink: WI-S09-004 audit chain R2 Object Lock 7y.

6. **3-of-3 sign-off for dry_run=false** (R4 NEW-P1-4 / round-1 P1-6 fix: Legal é sprint-level, NÃO per-replay row):
   - Sign-off workflow: Finance officer + Compliance Officer + Architect — each must emit audit event `dev.hugr.corelink.billing.replay.signoff.v1` within 24h window.
   - **Mecanismo de emissão**: admin CLI `corelink billing-signoff --invoice <id> --role <Finance|Compliance|Architect>` autenticado via S-03 RBAC; emits CloudEvent canonical to S-09 audit chain (WI-S09-004 cooperation).
   - **Legal sign-off é sprint-level** (DPA + Stripe contract review); must be in place ANTES de qualquer `dry_run=false`; NÃO entra na 3-of-3 per-replay matrix (Legal-at-sprint convention preserved).
   - Replay endpoint queries S-09 audit chain for 3 valid sign-off events linked to (invoice_id, billing_admin_id) tuple.
   - `DryRunFalseSignoffMissing` error if not 3.

7. **Rate limit per billing_admin** (sprint contract §15 R-007 + S-08 R-S08-3 cooperation):
   - 10 req/h per billing_admin user.
   - Limiter via S-08 token bucket inheritance.
   - SEV-3 alert if > 100 req/day per user (forensic abuse signal).

8. **Cooperation com WI-S10-001 + WI-S10-002 + WI-S10-003**:
   - WI-S10-001: R2 events archive read (cooperation function `r2_read_events_for_period`).
   - WI-S10-002: counter aggregation re-applied (cooperation function `wi_s10_002::aggregate_for_period`).
   - WI-S10-003: invoice line items rebuilt + Stripe API pull (`fetch_invoice`).
   - Re-uses production code paths (deterministic).

9. **Hash chain verification cooperation WI-S10-002** (tampering detection):
   - Re-derive chain head from R2 raw events.
   - Compare to stored hash_chain_head.
   - Mismatch = TamperingSignal status.

10. **Stripe API pull com PAT-BACKOFF-001 + PAT-QUEUE-EVENTS-001 fallback** (FM-151 cooperation):
    - Layer 3 comparison cooperation com WI-S10-004 reconciliation discipline.
    - PAT-BACKOFF-001 max 5 retries; queue fallback.

11. **Replay rate limit cooperation com S-08**:
    - X-RateLimit-Layer header inheritance for 429 response.
    - 429 com `Retry-After: <seconds_until_window_reset>`.

12. **Typed `ReplayReport` payload** (Lote 10.9-quinquies NEW-P0-2 critical):
    - All fields typed structs/enums; NEVER serde_json::Value.
    - PII wrapper inheritance from WI-S09-002 + WI-S10-003 (CustomerEmail, CustomerName).

13. **TenantCtx propagation** (Lote 10.4bis): admin_ctx authenticated upstream; tenant_id pseudonym em report.

14. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): `worker::send_future`; HTTP via wasm-bindgen.

15. **5 SKUs + 5 PlanTier + 5 regions canonical** (sprint contract §10 + Lote 10.7bis P0-7 + sprint contract §17).

16. **corelink_time::next_month_first_utc_midnight() canonical primitive** (Lote 10.8bis P0-D): billing_period boundary alignment.

17. **RB-BILLING-001 documented runbook** (sprint contract §14.s10.3):
    - Audit replay procedure documented em `runbooks/rb-billing-001-replay-procedure.md`.
    - Step-by-step Finance + Customer Success workflow.
    - 3-of-3 sign-off process documented.
    - Common scenarios + troubleshooting.

18. **CI mensal replay test** (sprint contract §8 INV verification):
    - Generate fake invoice fixture.
    - Run replay endpoint.
    - Assert output matches expected.
    - INV-BILLING-REPLAYABLE-FROM-EVENTS regression detection.

19. **Métricas operacionais** (via WI-S09-001 emit lib; cardinality budget; underscored canonical Lote 10.9bis P0-E):
    - `corelink_billing_replay_requests_total{admin_id_pseudonym, dry_run, status}` (counter; ~20 séries; admin_id pseudonym bounded).
    - `corelink_billing_replay_duration_seconds{status}` (histogram; 3 statuses × 11 buckets + sum/count = 39).
    - `corelink_billing_replay_sla_violations_total` (counter; **alert SEV-2 if > 0** sustained — > 30min reconstruction).
    - `corelink_billing_replay_drift_detected_total{layer}` (counter; 3 layers; informational; cooperation WI-S10-004).
    - `corelink_billing_replay_tampering_detected_total` (counter; **alert SEV-1 if > 0** — CRITICAL post-mortem).
    - `corelink_billing_replay_role_violations_total{reason}` (counter; **alert SEV-1 if > 0** — security breach signal).
    - `corelink_billing_replay_rate_limit_exceeded_total{admin_id_pseudonym}` (counter; **alert SEV-3 if > 5/day** — forensic abuse).
    - `corelink_billing_replay_dry_run_false_signoff_missing_total` (counter; **alert SEV-2 if > 0** — process violation).
    - `corelink_billing_replay_audit_emission_failures_total` (counter; **alert SEV-1 if > 0** — audit chain integrity).

20. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 absorbed):
    - `prop_replay_deterministic`: 100k same invoice replay; identical output.
    - `prop_replay_byte_a_byte_match`: 1k synthetic invoices; reconstruction matches original byte-a-byte (modulo Stripe metadata).
    - `prop_role_protection_enforced`: 10k random JWT claims; only billing_admin + MFA pass; rest 401.
    - `prop_dry_run_false_3_of_3_required`: 10k requests; assert 3 sign-offs validated.
    - `prop_rate_limit_10_req_h`: 10k requests; assert 11th rejected.
    - `prop_typed_report_no_serde_json_value`: 10k report serializations; typed canonical; serde_json::Value rejected at compile time.
    - `prop_pii_redacted_em_replay_output`: 10k replays with customer email/name; assert Redact::redact() applied.

21. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 11):
    1. **Customer chargeback dispute resolution**: Finance triggers replay; Green status; audit trail submitted; dispute resolved.
    2. **Drift detected (Layer 1 emit bug retroactively)**: replay shows drift > 0.1%; root-cause analysis; refund decision.
    3. **Tampering detected (admin INSERT bypass)**: hash chain verification fails; SEV-1 alert; CRITICAL post-mortem.
    4. **Rate limit 11th req em 1h blocked**: 429 com X-RateLimit-Layer header; sprint contract §15 R-007 mitigation.
    5. **dry_run=false sin 3-of-3 sign-off**: DryRunFalseSignoffMissing error; SEV-2 alert.
    6. **MFA bypass attempt**: MfaTokenMissing error; 401; SEV-1 audit.
    7. **billing_admin compromised account replays > 100/day**: SEV-3 forensic abuse alert; pattern detection.
    8. **Stripe API outage during Layer 3 comparison**: PAT-BACKOFF-001 retry + fallback; SLA may breach 30min → SEV-2.
    9. **Reconstruction SLA > 30min**: SlaViolation error; SEV-2; performance investigation.
    10. **CI mensal replay test failure**: INV-BILLING-REPLAYABLE-FROM-EVENTS regression; bloqueia merge; investigation.
    11. **Customer DSR erasure mid-replay**: pseudonymization preserved; replay continues; audit chain integrity preserved.

### 6.2 Out-of-scope (deferred)

- Counter aggregation (delegate WI-S10-002 — cooperation).
- Stripe integration (delegate WI-S10-003 — cooperation).
- Reconciliation worker (delegate WI-S10-004 — separate concern).
- Quota state machine (delegate WI-S10-005).
- TLA+ billing_atomicity (delegate WI-S10-007).
- Customer-facing replay UI (delegate S-13 admin plane).
- Sign-off workflow implementation (delegate dedicated service).
- Multi-currency replay (anti-scope sprint contract §10 — USD only at GA).

## 7. Anti-Scope

- ❌ serde_json::Value em ReplayReport (Lote 10.9-quinquies NEW-P0-2; typed canonical).
- ❌ dry_run=false default (sprint contract §5.6 R-S10-12 explicit dry_run=true default).
- ❌ Skip role protection (CTRL-AUTHZ-001 + CTRL-AUTHZ-002 billing_admin mandatory; sprint contract §19 NÃO-waivable).
- ❌ Skip MFA requirement (S-03 inheritance mandatory).
- ❌ Skip audit event emission (CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + INV-AUDIT-APPEND-ONLY).
- ❌ Reconstruction logic divergence from production (must use canonical methods deterministic).
- ❌ Rate limit > 10 req/h per billing_admin (sprint contract §15 R-007 + S-08 cooperation).
- ❌ ad-hoc `now() + Duration::days(30)` (corelink_time::next_month_first_utc_midnight canonical Lote 10.8bis P0-D).
- ❌ Per-feature SKU expansion (sprint contract §10 anti-scope).
- ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ TenantCtx bypass.
- ❌ Replay output without 3-mode status (Green/DriftDetected/TamperingSignal canonical).

## 8. Acceptance Criteria (Gherkin) — 11 scenarios

```gherkin
Feature: Replay Forensic Endpoint + Role Protection + Audit Trail

  Scenario: billing_admin replays customer invoice green status
    Given billing_admin Alice with MFA token valid
    Given invoice INV-2026-09-001 (Stripe inv_abc123)
    When POST /v1/billing/replay { invoice_id: INV-2026-09-001, dry_run: true } authenticated
    Then ReplayService::replay_invoice() invoked
    Then audit event dev.hugr.corelink.billing.replay.requested.v1 emitted to S-09 chain
    Then events ↔ counter ↔ invoice ↔ Stripe all match
    Then ReplayReport { overall_status: Green, max_drift_pct: 0.0001 } returned
    Then customer dispute resolution evidence available

  Scenario: Drift detected via replay
    Given Layer 1 emit bug retrospective; events R2 differ from invoice
    When admin invokes replay
    Then reconstructed_invoice ≠ stripe_current_snapshot
    Then DiffReport.max_drift_pct = 0.5%
    Then ReplayStatus::DriftDetected { layer_with_drift: 1 }
    Then SEV-3 alert; root-cause investigation; refund decision

  Scenario: Tampering detected via hash chain verification
    Given admin INSERT bypass to usage_counter retroactive
    When admin invokes replay for affected period
    Then verify_hash_chain_for_period() detects mismatch
    Then ReplayStatus::TamperingSignal { hash_chain_violation_detected: true }
    Then SEV-1 alert: corelink_billing_replay_tampering_detected_total increments
    Then PagerDuty corelink-security paged
    Then CRITICAL post-mortem auto-opened

  Scenario: Role protection enforced
    Given user Bob without billing_admin role
    When POST /v1/billing/replay invoked
    Then RoleProtectionViolation { got_role: "viewer" }
    Then HTTP 403 returned
    Then SEV-1 alert: corelink_billing_replay_role_violations_total{reason=missing_billing_admin} increments

  Scenario: MFA requirement enforced
    Given billing_admin Alice without recent MFA token
    When POST /v1/billing/replay invoked
    Then MfaTokenMissing error
    Then HTTP 401 returned
    Then audit event dev.hugr.corelink.billing.replay.mfa_failed.v1 emitted

  Scenario: dry_run=false requires 3-of-3 sign-off
    Given billing_admin Alice; invoice INV-2026-09-001
    When POST /v1/billing/replay { dry_run: false } sin sign-off chain
    Then DryRunFalseSignoffMissing error
    Then HTTP 403 returned
    Then SEV-2 alert: process violation; potential insider threat

  Scenario: Rate limit 10 req/h per billing_admin
    Given billing_admin Alice has executed 10 replays em previous hour
    When POST /v1/billing/replay 11th request
    Then RateLimitExceeded { retry_after_seconds: <seconds_remaining> }
    Then HTTP 429 com X-RateLimit-Layer: replay_endpoint header
    Then corelink_billing_replay_rate_limit_exceeded_total{admin_id_pseudonym=alice_pseudo} increments

  Scenario: Forensic abuse > 100/day detected
    Given billing_admin Alice (compromised account) executed 105 replays em 24h
    When 106th replay invoked
    Then RateLimitExceeded
    Then SEV-3 alert: forensic abuse pattern detected; corelink-security investigation

  Scenario: Stripe API outage during Layer 3 cooperation
    Given Stripe API returns 5xx during fetch_invoice
    When replay invoked; reaches diff_against_stripe step
    Then PAT-BACKOFF-001 retries 5×
    Then PAT-QUEUE-EVENTS-001 fallback queue retains pending Layer 3 comparison
    Then SLA may breach 30min → SEV-2 alert
    Then on Stripe recovery: queue drained; replay completes; report delivered

  Scenario: Reconstruction SLA < 30min
    Given replay request for billing_period 2026-09 com 1B events
    When ReplayService::replay_invoice() runs
    Then duration ≤ 1800s p99 (30min)
    Then if exceed: SlaViolation error; SEV-2 alert

  Scenario: PII redacted em ReplayReport (Lote 10.9-quinquies NEW-P0-2)
    Given replay reconstructs invoice com customer_email "john@example.com"
    When ReplayReport serializes
    Then CustomerEmail Serialize impl calls Redact::redact() → "j***@example.com"
    Then output em audit chain contains redacted form
    Then raw email NOT em logs ou reports

  Scenario: CI mensal replay test (INV-BILLING-REPLAYABLE-FROM-EVENTS verification)
    Given monthly cron generates fake invoice fixture
    When replay endpoint invoked against fixture
    Then output matches expected byte-a-byte (modulo Stripe metadata)
    Then INV-BILLING-REPLAYABLE-FROM-EVENTS verified
    Then test green em CI; merge unblocked
```

## 9. Design Decisions

- 9.1: dry_run=true default + 3-of-3 sign-off for dry_run=false (sprint contract §5.6 R-S10-12).
- 9.2: Role protection CTRL-AUTHZ-001 + CTRL-AUTHZ-002 billing_admin + MFA mandatory (sprint contract §5.6 R-S10-13 + §19 NÃO-waivable).
- 9.3: Audit event emission CloudEvents v1.0 dev.hugr.corelink.billing.replay.requested.v1 (S-09 inheritance + Lote 10.9bis P0-G canonical).
- 9.4: 3-mode comparison status (Green/DriftDetected/TamperingSignal — audit clarity).
- 9.5: Cooperation com WI-S10-001 + WI-S10-002 + WI-S10-003 (deterministic reconstruction).
- 9.6: Hash chain verification cooperation WI-S10-002 (tampering detection).
- 9.7: Stripe API pull cooperation WI-S10-003 fetch_invoice + PAT-BACKOFF/QUEUE fallback (FM-151).
- 9.8: Rate limit 10 req/h per billing_admin (sprint contract §15 R-007 + S-08 R-S08-3 cooperation).
- 9.9: Forensic abuse detection > 100/day (security signal).
- 9.10: SLA reconstruction < 30min p99 (sprint contract §6 DoD + §14.s10.3).
- 9.11: Typed ReplayReport (NOT serde_json::Value; Lote 10.9-quinquies NEW-P0-2 inheritance).
- 9.12: PII wrapper inheritance from WI-S09-002 + WI-S10-003 (CustomerEmail, CustomerName redact-wrapped).
- 9.13: 5 SKUs + 5 PlanTier + 5 regions canonical inheritance.
- 9.14: corelink_time::next_month_first_utc_midnight() canonical primitive (Lote 10.8bis P0-D).
- 9.15: TenantCtx propagation (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3).
- 9.16: RB-BILLING-001 documented runbook (sprint contract §14.s10.3).
- 9.17: CI mensal replay endpoint test (sprint contract §8 INV verification).
- 9.18: NEW INV-BILLING-REPLAYABLE-FROM-EVENTS em invariant_registry — verify canonical position before commit (Lote 10.8bis P1-13).

## 10. Completeness Criteria SOTA

- [ ] **10.s10.006.1** Crate compila + integration tests green.
- [ ] **10.s10.006.2** All 11 Gherkin scenarios green.
- [ ] **10.s10.006.3** Property tests 7 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar).
- [ ] **10.s10.006.4** Chaos suite 11 scenarios green.
- [ ] **10.s10.006.5** **INV-BILLING-REPLAYABLE-FROM-EVENTS** verified: 100% invoices reconstructable em < 30min p99.
- [ ] **10.s10.006.6** CI mensal replay endpoint test passing (sprint contract §8 INV).
- [ ] **10.s10.006.7** Role protection CTRL-AUTHZ-001 + CTRL-AUTHZ-002 billing_admin + MFA enforced.
- [ ] **10.s10.006.8** dry_run=false 3-of-3 sign-off enforced.
- [ ] **10.s10.006.9** Rate limit 10 req/h per billing_admin enforced (S-08 cooperation).
- [ ] **10.s10.006.10** Audit event emission to S-09 chain verified (CloudEvents canonical).
- [ ] **10.s10.006.11** PII redaction at replay output verified: 0 raw email/name em report (Lote 10.9-quinquies NEW-P0-2).
- [ ] **10.s10.006.12** RB-BILLING-001 runbook documented + reviewed by Compliance + Finance.
- [ ] **10.s10.006.13** Métricas (9) emitted via WI-S09-001 emit lib; cardinality budget respected (~80 séries baseline).
- [ ] **10.s10.006.14** Cargo-audit + cargo-deny + clippy clean.

## 11. DoD

- [ ] Crate compila + tests green; all Gherkin/property/chaos green; SOC 2 walkthrough Finance reconstrói invoice em < 30min; 12 sign-offs (HIGH_RISK; framework §33.5.4.3 cap; Finance + Legal + Privacy + Architect + Compliance Officer emphatic).

## 12. Invariants Validated

- **INV-BILLING-REPLAYABLE-FROM-EVENTS** (HIGH; sprint contract §8 NEW; registry §3.12 line 167; verify via grep before commit per Lote 10.8bis P1-13): 100% invoices reconstructable em < 30min; CI mensal test.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven Lote 6.2; S-09 inheritance): replay request audit event immutable em S-09 chain.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): admin_ctx authenticated upstream S-03; report contains pseudonym tenant_id.
- **CTRL-BILLING-001** (security_model.md): financial integrity replay-from-events.
- **CTRL-AUTHZ-001 + CTRL-AUTHZ-002** (security_model.md): role-protected billing_admin + MFA.
- **CTRL-PRIV-002** (privacy_model.md L209 — data classification tags): @classification=pii enforces classification em replay output fields; DSR pseudonymization via S-11 procedure.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Replay service module | `crates/corelink-billing-replay/` | Rust |
| Worker route | `crates/corelink-worker/src/routes/billing_replay.rs` | Rust |
| Replay logic | `crates/corelink-billing-replay/src/replay.rs` | Rust |
| Hash chain verification | `crates/corelink-billing-replay/src/hash_chain_verify.rs` | Rust |
| 3-of-3 sign-off validation | `crates/corelink-billing-replay/src/signoff.rs` | Rust |
| RB-BILLING-001 runbook | `runbooks/rb-billing-001-replay-procedure.md` | Markdown |
| Property tests | `crates/corelink-billing-replay/tests/prop_replay.rs` | Rust |
| Chaos suite | `tests/chaos_billing_replay.rs` | Rust |
| CI mensal replay test | `tests/ci_monthly_replay_test.rs` | Rust |

## 14. Quality Standards SOTA

- 14.s10.006.1: Zero unsafe Rust; zero unwrap em production paths.
- 14.s10.006.2: rustdoc 100% public API.
- 14.s10.006.3: Test coverage ≥ 90%.
- 14.s10.006.4: Reconstruction latency ≤ 30min p99 (sprint contract §6 DoD + §14.s10.3).
- 14.s10.006.5: SAST clean.
- 14.s10.006.6: Métricas (9 §6.1.19).
- 14.s10.006.7: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s10.006.8: TenantCtx propagation (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); 5-tier canonical Plan ref (Lote 10.7bis P0-7); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV positions §3.9 L136/137 (NO-LOSS/NO-DUP) + §3.12 L166/167 (RECONCILE-3-LAYER/REPLAYABLE) verified Lote 10.10bis (Lote 10.8bis P1-13 lesson absorbed).
- 14.s10.006.9: corelink_time::next_month_first_utc_midnight() canonical (Lote 10.8bis P0-D).
- 14.s10.006.10: D1 batch ≤ 250 (Lote 10.5bis); CHECK constraints inline (Lote 10.5bis).
- 14.s10.006.11: Replay endpoint role-protected + MFA + audit event mandatory (CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + INV-AUDIT-APPEND-ONLY).
- 14.s10.006.12: BLAKE3-256 hash chain consistency com WI-S09-004 + WI-S10-002 + WI-S10-004.
- 14.s10.006.13: Typed ReplayReport (NOT serde_json::Value; Lote 10.9-quinquies NEW-P0-2 inheritance); PII wrapper inheritance from WI-S09-002 + WI-S10-003.
- 14.s10.006.14: CloudEvents v1.0 dev.hugr.corelink.billing.replay.requested.v1 (Lote 10.9bis P0-G CloudEvents canonical inheritance).
- 14.s10.006.15: Prom metric names underscored canonical (Lote 10.9bis P0-E inheritance).
- 14.s10.006.16: Rate limit cooperation com S-08 R-S08-3 (10 req/h per billing_admin sprint contract §15 R-007 mitigation).

## 15. Chaos Experiments (11)

§6.1.21 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (framework §33.5.4.3 cap; Finance + Legal + Privacy + Architect + Compliance Officer emphatic).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + ReplayService trait + typed ReplayReport/Request structs | 1 |
| ST-002 | Replay logic (deterministic reconstruction) + cooperation WI-S10-001/002/003 | 2.5 |
| ST-003 | Hash chain verification cooperation WI-S10-002 | 1 |
| ST-004 | 3-mode comparison Status (Green/DriftDetected/TamperingSignal) | 1 |
| ST-005 | Worker route POST /v1/billing/replay + role + MFA | 1.5 |
| ST-006 | dry_run=false 3-of-3 sign-off validation logic | 1 |
| ST-007 | Rate limit 10 req/h per billing_admin (S-08 cooperation) | 1 |
| ST-008 | Audit event emission CloudEvents v1.0 to S-09 chain (Lote 10.9bis P0-G) | 1 |
| ST-009 | PII wrapper inheritance + redact-wrapped Serialize + report PII redaction (Lote 10.9-quinquies NEW-P0-2) | 1 |
| ST-010 | RB-BILLING-001 runbook documented + Compliance + Finance review | 1 |
| ST-011 | CI mensal replay test (INV-BILLING-REPLAYABLE-FROM-EVENTS regression) | 0.5 |
| ST-012 | Métricas (9) emit | 0.5 |
| ST-013 | Property tests (7 × 10k; 100k nightly) | 1.5 |
| ST-014 | Chaos suite (11) | 1.5 |
| ST-015 | Privacy review + Compliance audit chain validation + Finance walkthrough SOC 2 | 0.7 |

**Total**: ~15.7h. **PERT** O=8h M=14h P=22h: **14.3h** (matches sprint contract §12 estimate).

## 18. Dependencies

- Hard: WI-S10-001 SEALED (R2 events archive read source); WI-S10-002 SEALED (counter aggregator + hash chain re-verification cooperation); WI-S10-003 SEALED (invoice line items + Stripe fetch_invoice + PII wrappers CustomerEmail/CustomerName); WI-S09-001 SEALED (cardinality emit lib); WI-S09-004 SEALED (CloudEvents audit chain); S-03 SEALED (TenantCtx + RBAC role billing_admin); S-08 SEALED (rate limit cooperation R-S08-3).
- Soft: WI-S10-004 (reconciliation drift discovery may inform replay); WI-S10-007 (TLA+ billing_atomicity); S-11 (DSR cooperation CTRL-PRIV-002); S-13 (admin plane replay UI consumer).
- Hard infra: Cloudflare R2 + Neon Postgres + Stripe API access; CF Workers; CF KV (rate limit S-08 cooperation).
- External: Stripe API uptime SLA (sprint contract §15 R-001 risk acceptance).

## 19. Effort PERT: ~14.3h. ## 20. Time-boxing: 22h hard limit.

## 21. Observability

9 metrics §6.1.19. Trace span `billing.replay.{request, role_check, mfa_check, signoff_validate, r2_read, counter_reaggregate, invoice_rebuild, stripe_fetch, hash_chain_verify, diff_compute, audit_emit}`.

## 22. Cost Analysis

- R2 read events archive (replay execution): ~10 GB/replay × ~100 replays/mo × 12mo = ~12 TB; $0.36/TB read = ~$4/yr (negligible).
- Worker requests (replay endpoint): ~1k requests/mo × 12 = 12k/yr × $0.30/1M = negligible.
- Neon queries (invoice lookup + counter re-aggregation): ~50k queries/mo × 12 = 600k/yr; ~$1/yr.
- Stripe API fetch_invoice: ~1k fetches/mo × 12 = 12k/yr; rate-limit budget; minimal cost.
- TCO 12m: ~$50/yr replay infrastructure.
- **Cost saved by INV-BILLING-REPLAYABLE-FROM-EVENTS**: customer dispute resolution (avg cost $500-2k per litigation; replay = audit-grade evidence); SOC 2 audit pass enables enterprise sales (millions $); invaluable.

## 23. API Contract

- Public Rust: `ReplayService` trait + `ReplayRequest`, `ReplayReport`, `ReplayStatus`, `ReconstructedInvoice`, `DiffReport`, `HashChainVerification`, `AdminReason`, `ReplayError` types; `#[non_exhaustive]`.
- Wire (inbound): `POST /v1/billing/replay` JSON body com `invoice_id`, `dry_run`, `admin_reason`, `mfa_token`.
- Wire (outbound): JSON ReplayReport.
- Auth: JWT Bearer billing_admin role + MFA header.
- Storage: replay reports persisted em audit chain via S-09 cooperation; not separately stored.

## 24. Post-mortem Hooks

- INV-BILLING-REPLAYABLE-FROM-EVENTS regression detected → CRITICAL post-mortem (audit failure SOC 2).
- Replay output produces different result vs Stripe invoice → mandatory post-mortem (data integrity).
- Tampering signal (hash chain violation) → CRITICAL post-mortem (CTRL-BILLING-001 audit chain integrity).
- Role protection violation (billing_admin missing) → CRITICAL post-mortem + AppSec review.
- Forensic abuse > 100/day per billing_admin → post-mortem (potential compromise).
- Reconstruction SLA breach > 30min sustained → post-mortem (performance investigation).

## 25. Rollback / Recovery

- Rollback: revert Worker route + crate; replay endpoint unavailable; customer dispute resolution must use manual Finance audit; INV-BILLING-REPLAYABLE-FROM-EVENTS unverifiable temporariamente; events R2 retained.
- Recovery: route re-mounted; replay capability restored; CI mensal test re-runs.
- RTO ≤ 30min; RPO ≤ 0min (R2 events retained immutable).

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TenantCtx + billing_admin role + MFA via S-03 inheritance.
- T(ampering): Hash chain re-verification detects post-hoc modification.
- R(epudiation): Audit event 7y immutable; per-replay traceability.
- I(nformation disclosure): PII wrapper Serialize Redact-wraps customer email/name; pseudonym tenant_id em report.
- D(enial of Service): Rate limit 10 req/h per billing_admin; queue fallback Stripe outage.
- E(scalation of Privilege): MFA requirement; CTRL-AUTHZ-001 + CTRL-AUTHZ-002 role-protected; 3-of-3 sign-off for dry_run=false.

**LINDDUN** (LGPD/GDPR):
- L(inkability): Replay output linkable per tenant_id (pseudonym); aggregate per (region, billing_period); expected.
- I(dentifiability): Pseudonymization preserved (CTRL-PRIV-002 + S-11 cooperation); no raw PII em report.
- N(on-repudiation): Audit chain immutable; per-replay traceability.
- D(etectability): Customer access via S-13 admin plane (pseudonymized view).
- D(isclosure): 7y audit retention vs erasure right (LGPD Art. 16); pseudonymization preserves chain integrity.
- U(nawareness): Customer notified via S-13 admin plane + customer success integration.
- N(on-compliance): **CTRL-BILLING-001 + CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + CTRL-PRIV-002 + SOC 2 CC1.4 + GDPR Art. 22 (automated decision review) + GDPR Art. 32 + LGPD Art. 32** compliance.

## 27. Knowledge Transfer

Tech talk (1.5h): "S-10 Replay Forensic: Audit-Grade Reconstruction + 3-of-3 Sign-Off"; doc `docs/dev/billing-replay-architecture.md`; onboarding test 10 questions: dry_run=true default + 3-of-3 sign-off para dry_run=false (sprint contract §5.6 R-S10-12), CTRL-AUTHZ-001 + CTRL-AUTHZ-002 billing_admin role + MFA mandatory (sprint contract §5.6 R-S10-13 + §19), CloudEvents audit emit dev.hugr.corelink.billing.replay.requested.v1 (Lote 10.9bis P0-G), 3-mode comparison status (Green/DriftDetected/TamperingSignal), cooperation com WI-S10-001/002/003 deterministic reconstruction, hash chain re-verification (cooperation WI-S10-002), rate limit 10 req/h + S-08 cooperation (sprint contract §15 R-007), SLA reconstruction < 30min p99 (sprint contract §6 DoD + §14.s10.3), typed ReplayReport (Lote 10.9-quinquies NEW-P0-2), CI mensal replay test (sprint contract §8 INV verification), RB-BILLING-001 runbook procedure.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | INV-BILLING-REPLAYABLE-FROM-EVENTS regression | L | L | CRITICAL | M | LOW | CI mensal replay test; canonical methods inheritance |
| R-002 | Role protection bypass (billing_admin missing) | L | L | CRITICAL | L | LOW | CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + MFA mandatory; sprint contract §19 NÃO-waivable |
| R-003 | Forensic abuse via compromised billing_admin | L | M | HIGH | M | LOW | Rate limit 10 req/h + > 100/day SEV-3 + audit event per request |
| R-004 | Tampering admin INSERT bypass detected | L | L | CRITICAL | L | LOW | Hash chain verification cooperation WI-S10-002; SEV-1 alert |
| R-005 | dry_run=false unauthorized | L | L | HIGH | L | LOW | 3-of-3 sign-off + audit event chain validation |
| R-006 | PII em replay output (Lote 10.9-quinquies NEW-P0-2) | L | M | CRITICAL | L | LOW | Typed ReplayReport + PII wrapper Serialize impl inheritance |
| R-007 | Reconstruction SLA breach > 30min | L | M | HIGH | M | LOW | Performance test 30d staging + parallel R2/Stripe pulls + SEV-2 alert |
| R-008 | Stripe API outage during Layer 3 | L | H | MEDIUM | L | LOW | PAT-BACKOFF-001 + PAT-QUEUE-EVENTS-001 + RB-FM-151 cooperation |
| R-009 | Reconstruction logic divergence from production | L | L | CRITICAL | L | LOW | Cooperation WI-S10-001/002/003 canonical methods (computed_via_canonical_methods=true) |
| R-010 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | MEDIUM | L | LOW | grep verification before commit |
| R-011 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; worker::send_future |
| R-012 | DSR erasure breaks replay capability | M | M | MEDIUM | M | LOW | Pseudonymization (CTRL-PRIV-002 + S-11); replay continues with pseudonym |

## 29. Review Checkpoints

D+0 design (Architect; cooperation deterministic + 3-mode status); D+1 Compliance Officer (SOC 2 CC1.4 audit-grade replay + INV-BILLING-REPLAYABLE-FROM-EVENTS); D+2 Finance (replay procedure + RB-BILLING-001 walkthrough + 30min SLA); D+3 Legal sprint-level (DPA + Stripe contract terms + GDPR Art. 22 automated decision review; NÃO 3-of-3 per-replay — sign-off é Finance + Compliance Officer + Architect per §6.1.6); D+4 AppSec (CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + MFA + rate limit + tampering detection); D+5 Privacy (LINDDUN + PII wrapper inheritance + DSR cooperation S-11); D+6 SRE (rate limit cooperation S-08 + Stripe outage cooperation FM-151); D+7 code review; D+8 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + STRIDE + MFA + rate limit_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos 11 + property test 100k + CI mensal test_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 CC1.4 + INV-BILLING-REPLAYABLE-FROM-EVENTS + audit chain verification_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — LINDDUN + PII wrapper inheritance + GDPR Art. 22 automated decision review + DSR cooperation S-11_ |
| 11 | Architect | _TBD; **mandatory emphatic** — cooperation deterministic reconstruction + 3-mode status (Green/Drift/Tampering) + Lote 10.9-quinquies NEW-P0-2 absorption + Lote 10.9bis P0-G CloudEvents canonical + INV §3.X verification (Lote 10.8bis P1-13)_ |
| 12 | Finance | _TBD; **mandatory emphatic** — audit-grade replay + 3-of-3 sign-off process + RB-BILLING-001 walkthrough + 30min SLA_ |

(Legal sign-off via DPA reference at sprint level + 3-of-3 sign-off process review.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.10) | Criação WI-S10-006; HIGH_RISK; SOTA pós-Lote 10.7bis + Lote 10.8bis/tris + Lote 10.9bis/tris/quaters/quinquies lessons absorbed: 5-tier canonical Plan (Lote 10.7bis P0-7); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (Lote 10.7bis P1-3); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV §3.X canonical position TBD verification before commit (Lote 10.8bis P1-13); corelink_time::next_month_first_utc_midnight() canonical primitive (Lote 10.8bis P0-D); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis). **Lote 10.9-quinquies NEW-P0-2 critical absorption**: typed ReplayReport + ReconstructedInvoice + DiffReport + HashChainVerification (NOT serde_json::Value); CustomerEmail/CustomerName wrappers inherited from WI-S10-003 explicit serde::Serialize impl Redact-wrapping. **Lote 10.9bis P0-G inheritance**: CloudEvents canonical dev.hugr.corelink.billing.replay.requested.v1 (NOT 1.0.2; NOT io.corelink). **Lote 10.9bis P0-E inheritance**: Prom metric names underscored canonical. NEW corelink-billing-replay crate + POST /v1/billing/replay endpoint + cooperation com WI-S10-001 + WI-S10-002 + WI-S10-003 deterministic reconstruction + hash chain re-verification cooperation + 3-mode status (Green/DriftDetected/TamperingSignal) + dry_run=true default + 3-of-3 sign-off Finance+Compliance+Architect for dry_run=false + role CTRL-AUTHZ-001 + CTRL-AUTHZ-002 billing_admin + MFA mandatory + CloudEvents v1.0 audit emission to S-09 chain + rate limit 10 req/h per billing_admin (S-08 cooperation; sprint contract §15 R-007 mitigation) + SLA reconstruction < 30min p99 (sprint contract §6 DoD + §14.s10.3) + RB-BILLING-001 documented runbook + CI mensal replay test (sprint contract §8 INV verification). NEW INV-BILLING-REPLAYABLE-FROM-EVENTS HIGH (sprint contract §8) registry §3.X position TBD. CTRL-BILLING-001 + CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + CTRL-PRIV-002 + SOC 2 CC1.4 + GDPR Art. 22 automated decision review + LGPD Art. 32 + GDPR Art. 32 compliance audit-grade. |
| 1.1.0 | 2026-04-26 | Gustavo (Lote 10.10bis) | P0+P1 remediation pós-R4 Opus + Sonnet R5 adversarial reviews. **8 P0s fixed**: (1) PlanTier 5-tuple inventado `free/team/enterprise/custom/trial` → canonical `free/solo/team/business/enterprise` per `data_model.md §1` line 68 — corrige CHECK constraints em WIs 003+005 + sprint contract §5.3 R-S10-7; (2) INV registry positions estampadas `§3.X TBD` → reais §3.9 line 136 (NO-LOSS HIGH), §3.9 line 137 (NO-DUP HIGH), §3.12 line 166 (RECONCILE-3-LAYER HIGH), §3.12 line 167 (REPLAYABLE HIGH); severity drift CRITICAL→HIGH cascateia em SEV-2 alert wording (vs SEV-1); Lote 10.8bis P1-13 lesson finalmente absorvida; (3) WI-007 TLA+ math 5× errado (15k claim → real 3,000; 150k → real 30,000) + caveat sobre TLC reachable state graph ≠ Cartesian product CONSTANTS; MaxConcurrentEvents bound NEW; (4) CTRL-AUTHZ-005 alucinado (security_model só define -001/-002) → CTRL-AUTHZ-001 + CTRL-AUTHZ-002 explicit; sprint contract §5.6 R-S10-13 também atualizado; (5) WI-002 UPSERT WHERE clause auto-anulante removido (`WHERE own_digest = excluded.own_digest` rejeitava updates legítimos); (6) Cloudflare R2 Terraform schema marcada com TODO pre-merge verification (era hallucinated AWS S3 pattern `cloudflare_r2_bucket_lock_configuration`); (7) **R5 P0-A**: WI-002 PRIMARY KEY `(tenant_id, sku, hour)` → `(tenant_id, region, sku, hour)` — multi-region tenants antes corrompiam dados a cada UPSERT; (8) **R5 P0-B**: WI-001 D1 CHECK enum `('cas_put',...)` rejeitava 100% dos inserts (strum serializa para long form `dev.hugr.corelink.cas.put.v1`); aligned com Lote 10.9bis P0-G CloudEvents prefix canonical. **P1s fixed**: (a) staging table region column added (R5 P1-A); (b) WI-003 narrative 62-char idempotency key contradiziando §6.1 35-char (R5 P1-B); (c) idempotency comment WI-001 (P1-C); (d) `CHECK (age_hours > 6)` off-by-one → `>= 6` (R5 P1-D); (e) cardinality 30 regions → 5 canonical (P1-G); (f) PercentValue type/DB CHECK alignment 0..=200 (R5 P1-H); (g) chrono::next_month_first_utc_midnight() (não existe em chrono crate) → corelink_time::next_month_first_utc_midnight() canonical helper internal; (h) CTRL-PRIV-002 mis-mapped to "pseudonymization" → privacy_model.md L209 "data classification tags" + S-11 cooperation para DSR pseudonymization separada; (i) sprint contract §5.7 R-S10-15 metric names dots → underscores (Prom canonical); (j) §5.1 R-S10-1 CloudEvents prefix `corelink.usage.*` → `dev.hugr.corelink.<op>.v1`; (k) WI-005 4-state vs 5-state — GraceActive removido como state, virou flag boolean preserving sprint contract §5.5 R-S10-10 canonical; (l) failure_modes.md adicionado RB-FM-151 mapping (P1-5). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |
| 1.2.0 | 2026-04-26 | Gustavo (Lote 10.10-quaters) | P0+P1 remediation pós-R4 + R5 round-2 audits (R4 6.9/10, R5 7.8/10). Same S-08/S-09 cascade-incomplete pattern: bis ~60% complete; quaters fecha residuais. **8 P0s addressed**: (1) NEW-P0-1 spec contract §8 invariants L149-151 CRITICAL→HIGH (era source-of-truth não atualizada; severity contradicting WIs); (2) NEW-P0-2 WI-002 PK 4-tuple cascade ~30%→100% — `CounterRecord` + `LateCounterRecord` Rust structs add `region: Region` field (digest input para tamper detection per-region); 15 narrative/Gherkin/design-decision references swept (tenant_id, sku, hour) → (tenant_id, region, sku, hour); (3) NEW-P0-3 TLA+ `MaxConcurrentEvents` CONSTANT NEW added to body L58-65 + bounded em EmitEvent; `Hours` time abstraction comment clarified (R4 round-1 caveat absorbed); `quota_grace_active` separate VARIABLE (R5 NEW-P0-1 fix: grace é flag, não 5º state value); `ReconcileLayer1` action NEW with drift computation (R4 round-1 P2-10 fix); (4) WI-003 PlanTier residuals §1 item 11 L247-250 + §6.1.13 L548 swept canonical 5-tuple; (5) WI-002 Gherkin L530 UPSERT WHERE clause text fixed (R4 P0-5 partial); (6) WI-001 risk register R-001/R-002 + post-mortem hooks severity CRITICAL→HIGH; SEV-1 → SEV-2 (R4 NEW-P1-2). **P1s addressed**: (a) WI-005 PercentValue cluster L74/L178/L328 align com 0..=200 DB CHECK (R4 NEW-P1-1 + R5 NEW-P1-3); (b) WI-006 sign-off `Finance + Legal + Architect` → `Finance + Compliance Officer + Architect` (Legal sprint-level only; R4 NEW-P1-4 / round-1 P1-6 finalmente fix); (c) WI-003 título L29 + WI-005 L727 + spec contract L244 CTRL-PRIV-002 mis-mapping → "data classification tags @classification=pii" + S-11 separated DSR; (d) WI-002 §6.1.11 late event reference time → cron_now (R5 NEW-P1-1); (e) WI-002 §14 corelink_time crate path canonical reference added (`crates/corelink-time/src/canonical.rs`); (f) WI-S10-003 Idempotency-Key 64-char internal cap rationale ADR documented em line 367; (g) WI-S10-007 ADR-S10-007-tla-substitutes-proptest justifying property test exemption + lightweight sanity props NEW; (h) WI-S10-006 "Mfa" → "MFA" capitalization (30 occurrences); (i) WI-004 Layer 2/3 narrative aggregation semantics clarified (collapsed across regions for Stripe scope; per-region drift preservada via Layer 1). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |
| 1.4.0 | 2026-05-03 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S10-006 SEAL; no per-WI codex per 2026-04-30 protocol — sprint-close Sonnet review covers) | **WI-S10-006 IMPL SEALED** — `crates/corelink-billing-replay/` ships pure-logic skeleton per `trait-abstraction-defer` charter pattern (production CF Worker route POST /v1/billing/replay + R2 NDJSON archive read + admin RBAC binding via Tower middleware enforcing `billing_forensics_admin` role per CTRL-AUTHZ-002 + canonical S-09 audit chain APPEND surface + 30-min p99 reconstruction SLA budget + 10 req/h rate limit cooperation S-08 + RB-BILLING-001 documented runbook + CI mensal replay endpoint test all deferred to WI-S10-007 PRR ship gate). Six modules: `event` (ReplayReason `#[non_exhaustive]` 4-element taxonomy [DriftInvestigation / CustomerDispute / ComplianceAudit / DryRun] + ReplayDecision `#[non_exhaustive]` 4-element taxonomy [Authorized {idempotent_replay} / Denied403 {presented_role} / DryRunPlan {layers_planned} / Executed {layer_diverged}] + ReplayRequest [canonical UUIDv7 request_id + requested_by + presented_role + tenant_id + billing_period + reason] + ReconstructedLayers [3-layer u128 totals snapshot] + ReplayOutcome [idempotency ledger row] + LayerDriftSummary `#[non_exhaustive]` 5-element taxonomy [AllLayersMatch / Layer1Diverged / Layer2Diverged / Layer3Diverged / MultipleLayersDiverged] + ReplayConfig [canonical defaults] + canonical `BILLING_FORENSICS_ADMIN_ROLE = "billing_forensics_admin"` per CTRL-AUTHZ-002); `audit` (ReplayAuditEventType `#[non_exhaustive]` 5-event taxonomy `corelink.billing_replay.{request_authorized, request_denied, dry_run_planned, executed, layer_diverged}` + ReplayAuditSink trait + InMemoryReplayAuditSink + FailingReplayAuditSink + audit_event_for_decision canonical mapping fail-CLOSED Lote 10.6bis); `idempotency` (ReplayIdempotencyLedger trait + InMemoryReplayIdempotencyLedger UPSERT-safe ledger + `(request_id)` UNIQUE PK canonical idempotency contract + RecordOutcome Inserted / AlreadyExistsIdempotent + DivergentPayload SEV-1 surface + FailingReplayIdempotencyLedger); `archive` (ReplayArchive trait + InMemoryReplayArchive `BTreeMap<(Uuid, String), ReconstructedLayers>`-backed deterministic re-derivation + FailingReplayArchive); `engine` (ReplayEngine trait + InMemoryReplayEngine orchestrator: per-instance `Arc<Mutex<()>>` F-001 closure → role check → audit `request_denied` BEFORE Denied403 return → idempotency lookup → audit `request_authorized` BEFORE Authorized return for idempotent re-fire → dry-run reason check → audit `dry_run_planned` BEFORE DryRunPlan return → archive read → drift classification → audit `executed` BEFORE ledger UPSERT → audit `layer_diverged` AFTER executed BEFORE ledger UPSERT for divergence anomaly → idempotency ledger UPSERT; CANONICAL_LAYER_COUNT = 3 constant + drift_summary_for_reconcile bridge cooperation with WI-S10-004); `error` (ReplayError + ReplayAuditSinkError + ReplayIdempotencyError canonical `#[non_exhaustive]` taxonomies). Migration `migrations/d1/0021_billing_replay_audit.sql` ships canonical `billing_replay_audit` PRIMARY KEY (request_id) UNIQUE [canonical idempotency contract storage layer; 7-year SOC 2 CC1.4 + GAAP ASC 606 + GDPR Art. 22 evidence trail] + 4-element decision CHECK constraint + 4-element reason CHECK constraint + 5-element layer_drift_summary CHECK constraint + 3 indexes (per-tenant per-period chronological / per-decision-arm / per-requested-by forensic-abuse pattern detection) + 9 inline CHECK constraints. Tests: 62 inline unit + 14 integration (10 property tests at 10k iter PR-gate via `PROPTEST_CASES` env-var read at runtime: `prop_authorized_role_only_executes` + `prop_idempotent_replay_same_request_id` + `prop_replay_deterministic` + `prop_dry_run_no_state_mutation` + `prop_layer_diverged_flagged` + `prop_tenant_isolation` + `prop_audit_emit_per_decision_arm` + `prop_chain_event_appended_per_replay` + `prop_drift_summary_lifted_canonical` + `prop_layer_drift_classify_consistent` + 4 sanity / canonical-surface pinning). Quality gates green: `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings`, `cargo test -p corelink-billing-replay --all-targets` (76/76), `python3 scripts/validate_specs.py` (280/286), `python3 scripts/check_migrations_additive.py` (21 files OK). Charter compliance: zero unsafe / unwrap / expect / panic / indexing in lib code; per-instance `Arc<Mutex<()>>` F-001 closure mirroring S-07 `corelink-quota::check.rs` DO-actor model; `#[non_exhaustive]` on every public enum; wasm32-clean (no tokio in src/); audit fail-CLOSED envelope BEFORE state mutation on every decision arm (request_denied BEFORE Denied403 return; request_authorized BEFORE Authorized idempotent-replay return; dry_run_planned BEFORE DryRunPlan return; executed BEFORE idempotency ledger UPSERT; layer_diverged AFTER executed BEFORE ledger UPSERT for supplemental forensic evidence); ChaCha20Rng PRNG pinned for randomized fixtures; `prop_assert!(let arm = matches!...; arm)` pattern (S-08 P1-1 fix); typed ReplayDecision + ReplayRequest + ReconstructedLayers + ReplayOutcome Lote 10.9-quinquies NEW-P0-2 absorption (NOT serde_json::Value); idempotency-on-`request_id` canonical forensic determinism rationale enforced at orchestrator + ledger boundary (DivergentPayload tampering signal surface); CTRL-AUTHZ-002 separate `billing_forensics_admin` role from regular admin (least-privilege-bounded audit-grade replay capability per WI-S10-006 §1 invariant 3); canonical S-09 audit chain extension semantics (every replay invocation lands at least one canonical audit row per INV-OBS-AUDIT-CHAIN-INTEGRITY). |
| 1.3.0 | 2026-04-26 | Gustavo (Lote 10.10-sextus) | P0+P1 remediation pós-R4 + R5 round-3 audits (R4 8.0/10, R5 9.1/10 CONDITIONAL SEAL). 2 P0s + 8 P1s residuais addressed. **2 P0s**: (1) R4 NEW-P0-1 severity cascade incompleto — quaters só varreu WI-001; sextus completou WI-002 (L745-746 post-mortem + L786 R-001) + WI-003 (L869-870) + WI-007 (L743 + L785 R-005) — todos `INV-BILLING-NO-LOSS/NO-DUP` violation references → HIGH-severity post-mortem + SEV-2 alert (não SEV-1). (2) R4 NEW-P0-2 WI-S10-004 Layer 1 ainda 3-tuple `(tenant, sku, hour)` → 4-tuple `(tenant, region, sku, hour)` em título L41 + trait doc L67 + narrative L217 + Gherkin L521 + sprint contract §5.2 R-S10-4 line 86 + CAP-BILLING-002 line 65 — mascarava multi-region drift via cross-region offsets (financial-grade integrity restored). **8 P1s**: (a) R5 P1-QUIN-1 WI-002 L538 + step 5 Gherkin/narrative `hour_window.start_ts` → `cron_now`; (b) R5 P1-QUIN-2 WI-001 L210 §1 item 9 idempotency key long-form → 35-char canonical; (c) R4 P1-1 WI-003 título L41 + ST-010 sub-task CTRL-PRIV-002 pseudonymization → "data classification tags @classification=pii" + S-11 separated DSR; (d) R4 P1-2 WI-006 §29 D+3 review checkpoint Legal sign-off → sprint-level only (NÃO 3-of-3 per-replay; matched §6.1.6 fix); (e) R4 P1-3 spec contract metadata bumped v1.1.0/sota-v1.1/2026-04-24 → v1.3.0/sota-v1.3/2026-04-26; (f) R4 P1-4 + R5 P1-QUIN-3 TLA+ block: `BillingPeriods` + `RequestIds` agora declared CONSTANTS; helper operators (`Abs`, `SumCounters`, `SumLineItems`, `ComputeStripeInvoiceId`, `StripeIdParts`, `EventAccountedInInvoice`) documented as separate INSTANCE module; (g) R5 P1-QUIN-4 INV_BILLING_NO_LOSS retry_queue Sequence membership bug fixed via `RetrySet == {retry_queue[i] : i \in DOMAIN retry_queue}` Range conversion (semantic correctness); (h) R4 P1-5 `compute_counter_digest()` doc explicitly confirms region IS in canonical_json input (cross-region tamper detection guaranteed); (i) R4 P1-6 §14.s10.x.8 + R-010/R-011 risk register stale "INV §3.X TBD pre-merge" → concrete §3.9 L136/137 + §3.12 L166/167 verified Lote 10.10bis (residual após mitigação LOW). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |

## 32. Anti-patterns evitados

- ❌ serde_json::Value em ReplayReport (Lote 10.9-quinquies NEW-P0-2; typed canonical); ❌ dry_run=false default (sprint contract §5.6 R-S10-12 explicit dry_run=true default); ❌ Skip role protection (CTRL-AUTHZ-001 + CTRL-AUTHZ-002 billing_admin mandatory; sprint contract §19 NÃO-waivable); ❌ Skip MFA requirement (S-03 inheritance mandatory); ❌ Skip audit event emission (CTRL-AUTHZ-001 + CTRL-AUTHZ-002 + INV-AUDIT-APPEND-ONLY); ❌ Reconstruction logic divergence from production (must use canonical methods deterministic via cooperation com WI-S10-001/002/003); ❌ Rate limit > 10 req/h per billing_admin (sprint contract §15 R-007 + S-08 cooperation); ❌ ad-hoc `now() + Duration::days(30)` (corelink_time::next_month_first_utc_midnight canonical Lote 10.8bis P0-D); ❌ Per-feature SKU expansion (sprint contract §10 anti-scope); ❌ Plan tier expansion beyond 5 canonical (Lote 10.7bis P0-7); ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3); ❌ TenantCtx bypass; ❌ Replay output without 3-mode status (Green/DriftDetected/TamperingSignal canonical); ❌ Raw PII em report ou logs (PII wrapper Serialize impl Redact-wraps); ❌ Skip CI mensal replay test (sprint contract §8 INV verification mandatory); ❌ Skip RB-BILLING-001 runbook (sprint contract §14.s10.3).

---

**Fim WI-S10-006.** Próximo: WI-S10-007 (TLA+ billing_atomicity spec + RB-FM-302/151 dry-run + Finance walkthrough).
