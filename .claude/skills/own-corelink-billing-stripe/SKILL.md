---
name: own-corelink-billing-stripe
description: Ownership routing for corelink-billing-stripe; static draft only and never production authorization.
metadata:
  schema: corelink-ownership/1.1
  package: corelink-billing-stripe
  manifest: crates/corelink-billing-stripe/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-billing-stripe-structural-normalization-20260921
---

# Ownership — corelink-billing-stripe

[S01](#s01) · [S02](#s02) · [S03](#s03) · [S04](#s04) · [S05](#s05) · [S06](#s06) · [S07](#s07)

This guide is bounded to `crates/corelink-billing-stripe` at the recorded
source commit. It records source and static-manifest evidence, not a live
Stripe integration, deployed route, secret, clock, audit backend, or assigned
operational owner.

<a id="s01"></a>
## S01 — Enter the boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A change names the package, its exports, or a module under `src/` | Read the manifest, `src/lib.rs`, and the affected module before deciding scope | `Cargo.toml`; `src/lib.rs` | Package identity or baseline differs |
| A request names a real Stripe API, HTTP client, route, secret, clock, or deployed audit | Separate it from this static adapter scope and route it to the integration/runtime owner | [R08](../../../docs/ownership/crates/corelink-billing-stripe/REFERENCE.md#r08) | Do not infer operation from a trait, fake, or source comment |

<a id="s02"></a>
## S02 — Classify ownership

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A type or function is re-exported by `lib.rs` | Locate its defining module; treat the crate as owner of its local implementation and export wiring | `src/lib.rs`, defining `src/*.rs` | A requested change belongs to an upstream aggregate or downstream consumer |
| A dependency type is involved | Keep dependency ownership distinct from this crate’s use of its type | direct manifest entries; [B02](../../../docs/ownership/crates/corelink-billing-stripe/BLAST_RADIUS.md#b02) | Do not claim the adapter owns emit or aggregator implementation |

<a id="s03"></a>
## S03 — Change canonical idempotency safely

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Aggregate serialization or key derivation changes | Preserve the implemented chain: `serde_jcs::to_vec(AggregatedCounter)` then BLAKE3 over those bytes | `src/idempotency.rs`; [R04](../../../docs/ownership/crates/corelink-billing-stripe/REFERENCE.md#r04) | Canonical bytes, aggregate ownership, or reader compatibility is unknown |
| `IdempotencyKey` representation changes | Preserve the 32-byte newtype and its 64-character hex renderer unless consumers are coordinated | `src/event.rs`; [B03](../../../docs/ownership/crates/corelink-billing-stripe/BLAST_RADIUS.md#b03) | Compatibility decision or reverse consumer set is absent |

<a id="s04"></a>
## S04 — Change signature verification safely

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Header parsing, HMAC input, skew, or comparison changes | Preserve the implemented parser and verification order: parse `t=`/all `v1=`, HMAC-SHA256 over decimal timestamp + `.` + raw payload, OR `ct_eq` results, then reject absolute skew strictly greater than `300_000` ms | `src/signature.rs`; [R05](../../../docs/ownership/crates/corelink-billing-stripe/REFERENCE.md#r05) | Requested change relies on an external Stripe specification, observed delivery, or real receiver clock |
| Webhook dispatch changes | Preserve audit-before-log ordering and the typed log seam; coordinate any materializer/route behavior separately | `src/webhook.rs`; [B04](../../../docs/ownership/crates/corelink-billing-stripe/BLAST_RADIUS.md#b04) | A real route or event consumer is asserted without source wiring |

<a id="s05"></a>
## S05 — Preserve local state and error boundaries

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Ledger or webhook-log behavior changes | Keep in-memory maps keyed by `IdempotencyKey` or `stripe_event_id` and preserve their outcomes/errors | `src/{ledger,webhook_log,error}.rs`; [R06](../../../docs/ownership/crates/corelink-billing-stripe/REFERENCE.md#r06) | Treat no in-memory outcome as a real remote effect |
| Audit behavior changes | Preserve the trait seam and the source ordering that propagates an audit error before the local state mutation | `src/{adapter,webhook,audit}.rs`; [R07](../../../docs/ownership/crates/corelink-billing-stripe/REFERENCE.md#r07) | Durable audit, alerting, or external ordering evidence is required |

<a id="s06"></a>
## S06 — Validate at the right level

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static ownership artifact change | Run only the declared structural checks and `git diff --check`; inspect the allowed path list | [M04](../../../docs/ownership/crates/corelink-billing-stripe/MAINTENANCE.md#m04) | Do not report documentation checks as Cargo, test, runtime, or cold-review approval |
| Source change is separately authorized | Choose a bounded local test plan with the source owner; keep remote services and credentials out of fixtures | `tests/prop_billing_stripe.rs`; [M03](../../../docs/ownership/crates/corelink-billing-stripe/MAINTENANCE.md#m03) | Scope includes real credentials, Stripe, HTTP, deployment, or production data |

<a id="s07"></a>
## S07 — Report and escalate

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static work is complete | Report baseline, changed paths, source/static relations, check results, and explicit unknowns | [M06](../../../docs/ownership/crates/corelink-billing-stripe/MAINTENANCE.md#m06) | Do not self-certify a cold review |
| A production claim is required | Ask the appropriate integration, route, secret, clock, or audit operator for direct evidence | [R08](../../../docs/ownership/crates/corelink-billing-stripe/REFERENCE.md#r08) | Do not turn comments, dependencies, or fixtures into observation |

[Reference](../../../docs/ownership/crates/corelink-billing-stripe/REFERENCE.md#r01) ·
[Blast radius](../../../docs/ownership/crates/corelink-billing-stripe/BLAST_RADIUS.md#b01) ·
[Maintenance](../../../docs/ownership/crates/corelink-billing-stripe/MAINTENANCE.md#m01)
