---
id: "AUDIT-2026-05-27-W36-TRIGGER-A-SEAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "wave-36", "trigger-a", "dep-graph-cycle", "traits-extraction", "seal"]
references:
  - "specs/_audits/sealed/2026-05-26-w36-stage2c-closure.md"
  - "specs/_audits/sealed/2026-05-26-wave-33-34-closure-followups.md"
---

# Wave 36 — Trigger A SEAL (materializer dep-graph cycle resolved via traits extraction)

## §1 Closes W36 Stage 2.C Trigger A

Per closure audit §5.1 + user-authorized Option 4 (2026-05-27): extracted
the four port traits + identity / outcome / error types into a new leaf
crate `corelink-billing-stripe-traits` (zero `corelink-*` deps); both
`corelink-stripe-real` and `corelink-billing-stripe-materializer`
depend on it; the prospective cycle hazard
(`billing → materializer → stripe-real → would close back to billing
if Stage 3 cargo-deny forced umbrella migration`) is resolved without
the 6-crate inversion sprint Options 1–3 would have required.

The Wave-36 Stage 3 cargo-deny lockdown (deny direct
`corelink-stripe-real` deps for non-umbrella consumers) is now
UNBLOCKED — the materializer's production sources no longer transit
through `corelink-stripe-real` for the port-trait surface.

## §2 Acceptance criteria

- [x] New crate `corelink-billing-stripe-traits` builds GREEN
      (`cargo build -p corelink-billing-stripe-traits`).
- [x] `corelink-stripe-real` builds GREEN after the in-place
      trait-definition removal + re-export consolidation
      (`cargo build -p corelink-stripe-real`). The
      `corelink_stripe_real::webhook_dispatch::*` public surface is
      preserved by re-export so back-compat with all in-tree
      consumers + the `corelink-billing::stripe::real` umbrella path
      is unbroken.
- [x] `corelink-billing-stripe-materializer` builds GREEN with its
      three production sources (`audit.rs`, `handler.rs`,
      `idempotency.rs`) consuming the trait surface from
      `corelink-billing-stripe-traits` directly. The dep-graph cycle
      hazard is resolved.
- [x] `corelink-billing` (umbrella) builds GREEN and re-exports the
      trait surface as the canonical `corelink_billing::stripe::traits::*`
      path for future binders.
- [x] `corelink-server` (container app) builds GREEN.
- [x] `cargo clippy --tests -- -D warnings` GREEN for traits crate,
      stripe-real, and materializer.
- [x] `cargo test` GREEN: 7 traits-crate tests + 65 stripe-real lib
      tests + 21 materializer lib tests + 8 materializers_e2e tests
      = 101 passing assertions across the affected surface, with
      semantic preservation verified end-to-end via
      `materializers_e2e.rs` (drives the dispatcher through the trait
      seam against the production
      `D1SubscriptionStateHandler::materialize_*` matrix).
- [x] `grep "use corelink_stripe_real::webhook_dispatch"
      crates/corelink-billing-stripe-materializer/src/` returns EMPTY
      — production files all migrated.
- [x] No merge markers (`<<<<<<<` / `>>>>>>>`) in `Cargo.toml`,
      `crates/`, `tests/`.

## §3 Symbols extracted

**Four port traits** (defined in the new crate, re-exported by
`corelink-stripe-real::webhook_dispatch` for back-compat):

- `AuditEmitter` — pluggable audit sink (fail-CLOSED).
- `IdempotencyStore` — dedup attempt outcome.
- `StateMaterializer` — per-event-type state mutation seam.
- `SliRecorder` — per-dispatch SLI observation sink.

**Nine identity / wire / outcome / error types:**

1. `CanonicalWebhookEventType` (`#[non_exhaustive]`)
2. `StripeWebhookEnvelope` (`#[non_exhaustive]`)
3. `IdempotencyToken` (BLAKE3 32-byte newtype)
4. `IdempotencyOutcome` (`#[non_exhaustive]`)
5. `MaterializerError` (`#[non_exhaustive]`)
6. `AuditRecord` (`#[non_exhaustive]` — `AuditRecord::new()`
   constructor added for cross-crate brace-init parity)
7. `AuditOutcome` (`#[non_exhaustive]`)
8. `SliObservation` (`#[non_exhaustive]` — `SliObservation::new()`
   constructor added)
9. `DispatchResponse` (`#[non_exhaustive]`)

Plus the `SLI_BILLING_STRIPE_EVENT_SECONDS` `&'static str` constant.

**Stays in `corelink-stripe-real`** (concrete adapters / test fakes;
NOT extracted):

- `InMemoryIdempotencyStore` (concrete in-memory impl)
- `RecordingStateMaterializer` (test fake)
- `RecordingAuditEmitter` (test fake)
- `RecordingSliRecorder` (test fake)
- `FixedClock` (deterministic SLI clock for webhook tests)
- `WebhookDispatcher` (HTTPS adapter / pipeline)
- Everything in `webhook.rs` (HMAC + signature primitives)
- `Clock` / `SystemClock` re-exports

## §4 Output evidence

- New crate LOC: **613** lines (`src/lib.rs`) + **33** lines
  (`Cargo.toml`).
- Materializer src files migrated: **3** (`audit.rs`, `handler.rs`,
  `idempotency.rs`) — import line replacement only, no business-logic
  changes.
- Materializer test file: `materializers_e2e.rs` — split import into
  trait surface (from traits crate) + concrete adapters (from
  stripe-real); `webhook::compute_signature` import unchanged.
- Workspace root `Cargo.toml`: 2 additions — `[workspace.members]`
  entry + `[workspace.dependencies]` path entry for the new leaf
  crate.
- Billing umbrella: 1 dep addition + 1 new `pub mod traits` block
  inside `crates/corelink-billing/src/stripe.rs` exposing the trait
  surface at the canonical `corelink_billing::stripe::traits::*`
  path.
- Build status: GREEN for all 5 affected crates (`traits`,
  `stripe-real`, `materializer`, `billing`, `server`).
- Clippy status: GREEN with `-D warnings` on `traits`, `stripe-real`,
  `materializer` (full `--tests` surface).
- Test status: GREEN — 7 traits + 65 stripe-real lib + 21
  materializer lib + 8 materializers_e2e = 101 passing test
  assertions across the affected surface; the end-to-end driver
  `materializers_e2e.rs` runs the production
  `D1SubscriptionStateHandler` against the dispatcher through the
  trait seam, pinning all 10 canonical event types.
- Zero remaining `use corelink_stripe_real::webhook_dispatch` in
  materializer `src/` (verified via grep).
- **Cycle check:** `stripe-real` now deps `traits` (leaf, zero
  `corelink-*` deps); `materializer` deps `traits` (leaf) +
  `stripe-real` (test/dev-time only — concrete fakes for the e2e
  harness); `billing` deps `materializer` + `stripe-real` +
  `traits`; **no path from `traits` back to `billing` or any
  other `corelink-*` crate** — cycle resolved by construction.
- Wave-36 Stage 3 cargo-deny lockdown: **UNBLOCKED**.

## §5 Charter compliance

- `#![forbid(unsafe_code)]` at the new traits crate's root.
- `#[non_exhaustive]` preserved on every public enum + struct moved.
- No `unwrap` / `expect` / `panic` / `indexing_slicing` / `todo` /
  `unimplemented` / `dbg_macro` / `print_*` in lib code (full clippy
  `-D warnings` GREEN).
- `subtle::ConstantTimeEq` semantics preserved: the materializer's
  `D1IdempotencyStore` retains its `subtle` dep + ct_eq invariant
  check directly (it's a property of the concrete adapter, not the
  trait surface — no migration needed).
- INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER preserved: the dispatcher's
  fail-CLOSED audit emit semantics are unchanged (the
  `WebhookDispatcher::process` pipeline is byte-for-byte identical
  modulo the brace-init → `AuditRecord::new()` / `SliObservation::new()`
  constructor calls required by the cross-crate `#[non_exhaustive]`
  contract).
- CTRL-CRED-001 preserved: no secret-handling surface moved; the
  `WebhookDispatcher`'s `webhook_secret: Vec<u8>` and its
  `<redacted>` Debug impl remain in `corelink-stripe-real`.
- No `--no-verify`. No `#[allow]` added to mask new lints.

## §6 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
