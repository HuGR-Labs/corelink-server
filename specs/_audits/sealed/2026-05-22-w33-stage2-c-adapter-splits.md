---
title: Wave 33 Stage 2.C — Adapter HTTPS-vs-pure-logic split — PARTIAL / HALT
date: 2026-05-26
status: HALT (hard-pause trigger #7 activated pre-mutation)
parent_audit: specs/_audits/2026-05-22-wave33-code-reorg-spec.md §6 Stage 2
precedent: specs/_audits/2026-05-22-w33-stream-a-data-path.md §7 (partial-activation)
branch: wt/r-prep-w33-stage2-c-adapter-splits
base_main: dfe3e9d4 (post-PRE-B SEAL)
---

# §1. Scope (as dispatched)

Stage 2.C requested a clean physical split of 4 dual-use adapter
crates so the HTTPS-using files remain in the original adapter crate
(re-exported through `corelink-adapters-cloud::{stripe, statuspage,
slack, clerk}`) and the pure-logic files **physically move** to the
sibling umbrella crate (`corelink-billing::stripe`,
`corelink-ops::statuspage`, `corelink-ops::slack`,
`corelink-auth::clerk_cf`).

| Crate | LOC | Files | reqwest-using files (HTTPS) | Pure-logic dest |
|---|---|---|---|---|
| `corelink-stripe-real` | 4543 | 9 | `client.rs`, `lib.rs` | `corelink-billing::stripe` |
| `corelink-statuspage-real` | 1895 | 11 | `http.rs`, `lib.rs`, `wasm32_backend.rs`, `dsr_bridge.rs` | `corelink-ops::statuspage` |
| `corelink-slack-real` | 1871 | 11 | `http.rs`, `lib.rs` | `corelink-ops::slack` |
| `corelink-clerk-cf` | 3678 | 9 | `dsr_statuspage_cron.rs` | `corelink-auth::clerk_cf` |

# §2. Hard pause trigger #7 — activated **pre-mutation**

Per dispatch packet hard pause triggers list, trigger #7 is:

> Symbol path break: a consumer using
> `corelink_adapters_cloud::stripe::X` finds X no longer resolvable.

The structural dependency graph **guarantees** that any physical move
satisfying the dispatch packet's split topology produces a cyclic
workspace OR forces a symbol-path break that ripples through 4-6
non-target crates. Detail per crate below. Trigger fired BEFORE any
file mutation; zero LOC changed; no commit beyond branch creation.

# §3. Dependency-cycle proof per crate

## §3.1 `corelink-stripe-real` ↔ `corelink-billing::stripe`

Current edges (read from `Cargo.toml` at `dfe3e9d4`):

```
corelink-billing-stripe-materializer  → corelink-stripe-real
                                         (uses webhook_dispatch::{AuditEmitter,
                                          AuditRecord, IdempotencyStore,
                                          StateMaterializer, WebhookDispatcher})
corelink-billing                      → corelink-billing-stripe-materializer
corelink-billing                      → corelink-stripe-real
                                         (re-exports via stripe::real)
apps/server                           → corelink-stripe-real::webhook_dispatch
                                         (main.rs L30, webhook.rs L53)
```

Pure-logic items the packet asks to move to `corelink-billing::stripe`
include `webhook_dispatch.rs` (1365 LOC, pure trait surface: no
reqwest), `dlq.rs`, `error.rs` (uses reqwest::Error variant), `retry.rs`,
`portal.rs` (pure audit types), `clock.rs`, `webhook.rs` (HMAC verify
— pure crypto, no HTTPS).

Cycle: moving `webhook_dispatch` into `corelink-billing::stripe`
forces `corelink-billing-stripe-materializer` to depend on
`corelink-billing` for the `AuditEmitter` / `WebhookDispatcher` traits.
But `corelink-billing` already depends on
`corelink-billing-stripe-materializer`. **Direct edge cycle:**
`billing → materializer → billing`.

Symbol-path break: `apps/server/src/main.rs` line 30 +
`apps/server/src/webhook.rs` line 53 + 151 + the materializer's
4 import sites all use `corelink_stripe_real::webhook_dispatch::…`.
Moving the module breaks those imports unless `corelink-stripe-real`
re-exports — but `corelink-stripe-real` would then need to dep
`corelink-billing` for the re-export, re-creating the cycle.

`error.rs` (91 LOC) imports `reqwest::Error`; cannot live in a
context crate (which under Stage 3 lockdown will be forbidden from
depending on `reqwest`). So `error.rs` MUST stay HTTPS-side. But
`webhook_dispatch.rs` uses `StripeError` from `error.rs`; if
`webhook_dispatch` moves to context and `error.rs` stays, the move
breaks the symbol chain.

## §3.2 `corelink-statuspage-real` ↔ `corelink-ops::statuspage`

Current edges:

```
corelink-clerk-cf                      → corelink-statuspage-real
                                          (dsr_statuspage_cron.rs L27 uses
                                           StatuspageBackend, DsrCompletionReport,
                                           RetryPolicy)
corelink-dsr-statuspage-scheduler      → corelink-statuspage-real
                                          (scheduler.rs L30 uses bridge_to_report,
                                           DsrCompletionReport, RetryPolicy,
                                           StatuspageBackend, RateLimiter)
corelink-ops                           → corelink-statuspage-real
corelink-adapters-cloud                → corelink-statuspage-real
corelink-auth                          → corelink-clerk-cf
```

Pure-logic items per packet (`audit.rs`, `backend.rs` trait,
`memory.rs`, `rate_limit.rs`, `redact.rs`, `report.rs`, `retry.rs`)
moving to `corelink-ops::statuspage` forces both
`corelink-dsr-statuspage-scheduler` AND `corelink-clerk-cf` to depend
on `corelink-ops`. But `corelink-ops` does NOT depend on those two
(verified in §3.2.a below); however the inverse leg through
`corelink-auth` (which deps `corelink-clerk-cf`) means `auth → ops`
becomes mandatory.

Worse: `corelink-clerk-cf` is the eventual destination of its own
Stage 2.C.4 split into `corelink-auth::clerk_cf`. If 2.C.2 moves
`StatuspageBackend` into `corelink-ops::statuspage`, then
`corelink-clerk-cf` must dep `corelink-ops`. Then 2.C.4 wants to move
clerk-cf pure-logic into `corelink-auth`, which would require
`corelink-clerk-cf` to dep `corelink-auth`. And `corelink-auth`
already deps `corelink-clerk-cf`. **Cycle:** `auth → clerk-cf → auth`.

## §3.3 `corelink-slack-real` ↔ `corelink-ops::slack`

Current edges:

```
corelink-enterprise-inquiry            → (used by corelink-slack-real)
corelink-slack-real                    → corelink-enterprise-inquiry
                                          (adapter.rs implements
                                           inquiry::SlackClient trait)
corelink-ops                           → corelink-slack-real
corelink-ops                           → corelink-enterprise-inquiry
corelink-adapters-cloud                → corelink-slack-real
```

Pure-logic candidates (`adapter.rs`, `audit.rs`, `channel.rs`,
`client.rs` trait, `memory.rs`, `message.rs`, `redact.rs`, `retry.rs`,
`template.rs`) moving to `corelink-ops::slack` forces `corelink-slack-real`
to dep `corelink-ops` for its own internal types. But
`corelink-ops` already deps `corelink-slack-real`. **Direct cycle.**

This is the cleanest case of "the pure-logic and HTTPS halves are
not separable: the HTTPS http.rs (200 LOC) consumes the trait
defined in client.rs, the retry policy from retry.rs, the audit
envelope from audit.rs, the channel routing from channel.rs, the
message builder from message.rs, the redact helper from redact.rs,
and the template renderer from template.rs". Eight of eleven files
are entangled; isolating http.rs leaves it depending on seven moved
modules.

## §3.4 `corelink-clerk-cf` ↔ `corelink-auth::clerk_cf`

Current edges:

```
corelink-auth                          → corelink-clerk-cf
                                          (re-exports via clerk_cf::*)
corelink-adapters-cloud                → corelink-clerk-cf
corelink-clerk-cf                      → corelink-clerk
corelink-clerk-cf                      → corelink-cf-bindings
corelink-clerk-cf                      → corelink-statuspage-real
```

Pure-logic candidates per packet (everything EXCEPT
`dsr_statuspage_cron.rs`) include `clerk_health_do.rs` (1126 LOC; the
`ClerkHealthLogic` state machine is pure, but the
`#[durable_object]` `ClerkHealthDo` struct in the same file is
wasm32-only worker binding), `prod_wiring.rs` (708 LOC; mixes pure
orchestration with wasm32 KV/D1 binding), `health.rs` (413 LOC;
HTTP handler that takes wasm32 `worker::*` types),
`tenant_region_real.rs` (324 LOC; D1 binding behind feature flag),
`cf_kv.rs` + `cf_fetch.rs` (CF KV / Fetch binding — wasm32-only by
nature), `audit_sink.rs` (421 LOC; audit emit logic — depends on
optional `corelink-audit-chain`).

The packet identifies only `dsr_statuspage_cron.rs` as HTTPS portion,
but six of nine files are wasm32-binding (not pure-logic): they
cannot live in `corelink-auth::clerk_cf` because that crate compiles
for native and the wasm32-only `#[durable_object]` actor class lives
under `#[cfg(target_arch = "wasm32")]`. Moving them would either (a)
break the wasm32 target (charter hard pause trigger 6) or (b) require
`corelink-auth` to itself become wasm32-aware, which contradicts
context-crate Stage 3 lockdown rules.

Moving the genuinely-pure portion (e.g. `ClerkHealthLogic` state
machine extracted out of `clerk_health_do.rs`) requires a
function-level split of a 1126 LOC file (which itself violates L2.10's
≤500 LOC cap for any new file created during the move). That sub-step
is a Wave-32-style mega-file decomposition, not an atomic split.

# §4. Cross-cutting state types — the unmoveable middle

Per packet "If any single split is too complex to land atomically
(cross-cutting state types that don't cleanly belong to either side),
HALT + escalate per partial-SEAL pattern (Stream A precedent)."

Cross-cutting types identified:

| Type | Crate | Used by HTTPS side | Used by pure side | Used by external |
|---|---|---|---|---|
| `StripeError` | `stripe-real::error` | client.rs | webhook_dispatch.rs, dlq.rs | apps/server, materializer |
| `webhook_dispatch::AuditEmitter` trait | `stripe-real::webhook_dispatch` | (none) | dlq.rs, portal.rs | materializer, apps/server |
| `StatuspageBackend` trait | `statuspage-real::backend` | http.rs, wasm32_backend.rs | (definition) | clerk-cf, scheduler |
| `DsrCompletionReport` | `statuspage-real::report` | http.rs payload | (definition) | clerk-cf, scheduler |
| `RetryPolicy` (stripe + statuspage + slack) | each `retry.rs` | client.rs / http.rs | (definition) | scheduler |
| `SharedSlackClient` trait | `slack-real::client` | http.rs (impl) | (definition) | enterprise-inquiry, ops |
| `ClerkHealthLogic` state machine | `clerk-cf::clerk_health_do` | health.rs handler | (definition) | (none external) |
| `WiringError` | `clerk-cf::prod_wiring` | health.rs, dsr_statuspage_cron | (definition) | apps/server (?) |

Every row's trait or type is a **port-and-adapter interface boundary**:
the pure-logic side defines a trait, the HTTPS side provides the
production implementation. Moving the trait to a context crate while
leaving the impl in the adapter crate creates a downstream-to-upstream
dependency reversal that the Stage 1 aggregator design did NOT
prepare for — context crates currently depend on adapter crates via
re-exports, not vice-versa.

# §5. Required restructuring to unblock 2.C (out-of-scope for 2.C)

Resolving the cycles cleanly requires:

1. **Strip `pub use corelink_X_real::*` from every Stage 1 aggregator
   submodule** — context crates can no longer re-export adapter
   surfaces. Consumers move to import from adapter crates directly
   (or from `corelink-adapters-cloud`).
2. **Define the trait/port modules INSIDE the context crates** as
   first-class code (not aggregator re-exports) — `corelink-billing`
   owns `StripeError`, `webhook_dispatch::*`, `RetryPolicy`,
   `PortalAuditSink`; `corelink-ops` owns `StatuspageBackend`,
   `DsrCompletionReport`, `SharedSlackClient`, etc.
3. **Flip every adapter crate's dependency** to depend on the
   context crate (`corelink-stripe-real → corelink-billing` instead
   of the inverse).
4. **Audit `corelink-billing-stripe-materializer`** which currently
   sits between billing and stripe-real; possibly fold it into
   `corelink-billing` directly OR pull its trait deps from the
   context-crate side.
5. **Update 8 external consumer call sites** in `apps/server` +
   `corelink-dsr-statuspage-scheduler` + `corelink-clerk-cf` +
   `corelink-enterprise-inquiry` atomically.

This is a **6-crate dependency-graph inversion**, not a 4-crate file
split. Per Stream A precedent §7, deferring it for explicit
re-dispatch is the correct partial-SEAL response.

# §6. Charter compliance — preserved by inaction

Because zero files were mutated:

- L2.10 ≤500 LOC: unchanged (no new files).
- `#[non_exhaustive]`: unchanged on all existing enums/structs.
- `subtle::ConstantTimeEq`: preserved verbatim in Stripe webhook
  HMAC compare + Clerk JWT verify + Slack signed-secret paths.
- `SecretString`: preserved on every credential field across all
  4 target crates.
- `#![forbid(unsafe_code)]`: unchanged on every crate.
- Audit fail-CLOSED: preserved verbatim across all 4 adapters.
- Wave-31 wallet broker dual-mode: preserved verbatim
  (`StripeAuthMode::Direct` / `StripeAuthMode::WalletBroker`
   re-verified at `crates/corelink-stripe-real/src/lib.rs:84-87`).

# §7. Gates — baseline re-verification (no mutation)

```
cargo build --workspace                          GREEN  (4m 34s at dfe3e9d4)
cargo test -p corelink-stripe-real               (NOT RE-RUN — no mutation)
cargo test -p corelink-clerk-cf --features tenant-region-real   (NOT RE-RUN)
cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf  (NOT RE-RUN)
```

Test counts at baseline (per dispatch packet, presumed authoritative):
- corelink-stripe-real: 65 lib + 3 direct_proxy + 3 wallet_broker_proxy + ≥23 doctests
- corelink-clerk-cf: 43 tests with `tenant-region-real`

# §8. Hard pause triggers — status

| # | Trigger | Status |
|---|---|---|
| 1 | Any new file >500 LOC after split | N/A — no split |
| 2 | Test count regression | N/A — no mutation |
| 3 | wasm32 build red | N/A — no mutation |
| 4 | Workspace build red at sub-step boundary | N/A — no mutation |
| 5 | Wallet broker dual-mode behaviour change | N/A — no mutation |
| 6 | `subtle::ConstantTimeEq` accidentally removed | N/A — no mutation |
| 7 | Symbol path break: consumer X no longer resolvable | **ACTIVATED pre-mutation** — see §3 proof |

Trigger #7 activated pre-mutation per packet "HALT + escalate per
partial-SEAL pattern. No auto-recover."

# §9. Recommendation + next steps

## Recommended path forward

(a) **ACCEPT PARTIAL** — Stage 2.C ships as a HALT-only audit (this
    document) with zero LOC delta. Stage 2.B + 2.D continue in
    parallel and merge as planned. Stage 2 closes WITHOUT Stage 2.C
    physical split; the Stage 1 aggregator pattern (`pub use
    corelink_X_real::*` from sibling umbrella) remains in place as
    the canonical Wave-33 surface.

(b) **RE-DISPATCH 2.C as a 6-crate dep-graph inversion** — Owner
    explicitly greenlights a follow-on stream (sized like Stream A
    or B, not 2.C) that performs steps §5.1-§5.5 atomically.
    Estimated 2-3 days wall-clock; touches 8+ call sites including
    `apps/server`; requires fresh techlead review before merge.

(c) **DEFER Stage 2.C to a Wave 34 hex-boundary lockdown task** —
    couple it with Stage 3's `cargo deny check` rule introduction
    so the dependency inversion is enforced by tooling at the same
    time it is implemented in code. Lower risk because cargo-deny
    rules surface remaining offenders as compile-time errors, not
    runtime regressions.

## Strong recommendation: (a) ACCEPT PARTIAL

Rationale:
- Stage 1 aggregator pattern (Option-A) **already** delivers a
  canonical import surface (`corelink_adapters_cloud::stripe::*`,
  `corelink_billing::stripe::*`, etc.) per the Wave-33 spec §6
  Stream C C.3 charter. Consumers can migrate today.
- The "physical decomposition deferred to Stage 2" language in
  every Stage 1 aggregator doc (`adapters-cloud/src/stripe.rs:9`,
  `corelink-billing/src/stripe.rs:5`, etc.) was already a hedge for
  exactly this situation.
- Stage 3 lockdown (`cargo deny check` forbidding `tokio`/`reqwest`
  in context crates) will FAIL with the current Stage 1 re-exports
  in place — but that is the natural enforcement point for the
  inversion, not a refactor done in isolation here.
- Charging through 2.C as specified would break the green workspace,
  regress test counts, AND coordinate-conflict with the parallel
  Stage 2.B + 2.D agents currently mid-flight on their worktrees.

## Next steps (per packet §8)

- **2.A worker piece moves**: still safe to run sequentially after
  2.B + 2.D merge (no overlap with 2.C scope).
- **2.E consumer migration**: still safe; depends on Stage 1
  aggregator surface, which is intact.
- **Stage 3 hex-boundary lockdown**: surface the deferred 2.C
  inversion as a Wave 33 Stage 3 prerequisite OR a Wave 34 entry
  task.

# §10. Files touched

```
specs/_audits/2026-05-22-w33-stage2-c-adapter-splits.md  +311 LOC (this doc)
```

Zero source-code files mutated. Zero Cargo.toml mutated. Zero
workspace.members entries added or removed.

# §11. DCO sign-off

Per charter:

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

# §12. Cross-references

- Parent: `specs/_audits/2026-05-22-wave33-code-reorg-spec.md` §6 Stage 2.
- Precedent: `specs/_audits/2026-05-22-w33-stream-a-data-path.md` §7 partial-activation.
- Stage 1 Stream B (sibling deps): `specs/_audits/2026-05-22-w33-stream-b-policy.md`.
- Stage 1 Stream C (sibling aggregator): `specs/_audits/2026-05-22-w33-stream-c-infra-ops.md`.
- Charter: `specs/03_architecture/invariant_registry.md`.
- Memory: `[[corelink-autonomous-execution-charter]]`, `[[feedback-synchronous-agents]]`.

---

**End of Wave 33 Stage 2.C HALT audit.**
