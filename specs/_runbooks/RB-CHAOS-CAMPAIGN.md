---
id: "RB-CHAOS-CAMPAIGN"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "chaos", "wave-22", "fail-closed", "r-prep"]
---

# RB-CHAOS-CAMPAIGN — Wave-22 chaos campaign harness runbook

> Companion audit: [`specs/_audits/2026-05-16-chaos-campaign-harness.md`](../_audits/2026-05-16-chaos-campaign-harness.md)
> Adjacent: `RB-ACTIVE-FAILOVER`, `RB-AUDIT-EXPORT-INTEGRITY`,
> `RB-COLD-RESTORE-FROM-ZERO`, `RB-CHAOS-CATALOG`.

## What this runbook is

`tests/chaos` ships **eight fail-CLOSED chaos scenarios**, each verifying
the observable triplet (fail-CLOSED, audit event, alert) for one
canonical incident class. This runbook tells you:

- how to run the suite
- which scenarios fire which alerts in production
- expected recovery time per scenario
- how to extend the campaign

## How to run

The campaign is **opt-in** behind a Cargo feature flag — default
`cargo test --workspace` does not run it.

```bash
# Run the full campaign (≈ seconds on a laptop):
cargo test -p chaos-campaign --features chaos --no-fail-fast

# Run a single scenario:
cargo test -p chaos-campaign --features chaos \
  --test campaign_rls_guc_dropout_rejects_insert

# Verify default-off behaviour (zero tests, exit 0):
cargo test -p chaos-campaign
```

The harness is pure in-process — no testcontainers, no network IO, no
external infra. CI executes the opt-in suite on demand via the
`workflow_dispatch` matrix; it is not on the default PR gate so a
regression in the campaign cannot block routine merges, but a release
candidate **must** pass it before tagging.

## Scenario → alert → recovery matrix

| # | Scenario                          | Audit event                                  | Alert (severity)                       | Expected detection time | Expected recovery time | First responder                  |
| - | --------------------------------- | -------------------------------------------- | -------------------------------------- | ----------------------- | ---------------------- | -------------------------------- |
| 1 | Network partition between regions | `corelink.failover.region.degraded`          | `region_isolated` (SEV-1)              | ≤ 30 s probe interval   | ≤ 5 min auto-failover  | on-call SRE → RB-ACTIVE-FAILOVER |
| 2 | D1 pool exhausted                 | `corelink.d1.pool.exhausted`                 | `d1_pool_saturated` (SEV-2)            | ≤ 1 min Retry-After     | ≤ 10 min (autoscale)   | on-call SRE                      |
| 3 | Neon shadow sink silent failure   | `corelink.audit.shadow_sink.silent_failure` | `audit_shadow_sink_drift` (SEV-2)      | ≤ 5 min reconcile pass  | ≤ 30 min (replay)      | on-call SRE → RB-AUDIT-EXPORT-INTEGRITY  |
| 4 | RLS GUC dropped mid-tx            | `corelink.rls.guc.dropped`                   | `rls_policy_violation_attempt` (SEV-1) | ≤ 30 s WAF/alert        | ≤ 5 min (rollback)     | on-call security                 |
| 5 | BYOK provider 503                 | `corelink.byok.provider.unavailable`         | `byok_provider_unavailable` (SEV-1)    | ≤ 30 s circuit-breaker  | ≤ 15 min (provider)    | on-call security + provider esc. |
| 6 | Stripe webhook clock skew         | `corelink.billing.webhook.replay_window`     | `stripe_webhook_clock_skew` (SEV-2)    | ≤ 1 min                 | ≤ 10 min (NTP)         | on-call billing                  |
| 7 | Clerk JWKS rotation               | `corelink.clerk.jwks.rotated`                | `clerk_jwks_rotation` (INFO)           | immediate (per-request) | n/a — self-heal        | none — informational             |
| 8 | CAS multipart abort               | `corelink.cas.multipart.aborted`             | `cas_multipart_aborted` (INFO)         | immediate (per-request) | n/a — GC pass          | none — expected on client cancel |

## When an alert fires in production

1. **SEV-1** — page primary on-call. The matching scenario test
   documents the expected fail-CLOSED behaviour: confirm the operation
   is failing CLOSED (not silently succeeding) before any mitigation.
2. **SEV-2** — slack on-call channel. Inspect the audit-event count
   over the last 5 min; if it exceeds the documented baseline the
   incident is escalating.
3. **INFO** — no human action; the system has handled the chaos as
   designed (JWKS rotation, multipart abort GC). These are surfaced for
   audit-trail completeness.

## Extending the campaign

Add a ninth scenario:

1. Add a mini-model in `tests/chaos/src/lib.rs` exposing
   `inject_<failure>()` plus a `<surface>_outcome()` accessor and audit
   / alert vectors.
2. Add `tests/chaos/tests/campaign_<scenario>.rs` gated by
   `#[cfg(feature = "chaos")]`, asserting the observable triplet.
3. Register the test binary in `tests/chaos/Cargo.toml` `[[test]]`.
4. Append a row to the matrix in
   `specs/_audits/2026-05-16-chaos-campaign-harness.md` and to the
   matrix above.
5. Run `cargo test -p chaos-campaign --features chaos` and
   `cargo clippy -p chaos-campaign --all-targets --features chaos -- -D warnings`.

## Charter constraints honoured

- `#![forbid(unsafe_code)]`
- no `unwrap` / `expect` / `panic` in library code
- default-off feature flag — workspace-default test surface unchanged
- pure in-process synchronous state machines; no `tokio`, no
  testcontainers, no network IO
- DCO sign-off + Co-Authored-By on every commit
