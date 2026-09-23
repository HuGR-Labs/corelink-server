---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-tier-selection
manifest: crates/corelink-tier-selection/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: S
state: author_validated
evidence_set: w011-tier-selection-source-static-20260920
---

# corelink-tier-selection — blast radius

SOURCE-only atomic relation map for tier rules and local fakes. Every arrow is
a declaration, import, field, or local-call relation visible in checked-in
text. No arrow proves a caller, an external effect, durable state, execution,
deployment, or a complete consumer graph.

[Root](#b01) · [Taxonomy](#b02) · [Gate fake](#b03) · [Audit fake](#b04) · [Client fake](#b05) · [Unknowns](#b06)

<a id="b01"></a>
## B01 — Root → tier public-path relation

**Arrow:** `src/lib.rs` → `pub mod tier` and `pub use tier::{TierKind,
canonical_tiers, canonical_runner_tiers}`. **Mode:** SOURCE. **Trigger:** a
root declaration or one of the three named re-exports changes. **Evidence:**
`src/lib.rs`. **Impact:** a source import using one named root path can require
static contract review. **Boundary:** this root route is not an exhaustive
consumer list and does not show that a value was constructed or interpreted.

<a id="b02"></a>
## B02 — Tier enum → customer-tier-list relation

**Arrow:** `TierKind::{Free,Solo,Starter,Pro,Max,Enterprise}` →
`canonical_tiers()`. **Mode:** SOURCE. **Trigger:** one of these six variants,
its array membership, or its array order changes. **Evidence:** `src/tier.rs`.
**Impact:** a static consumer of `canonical_tiers` can observe a changed local
array. **Boundary:** this one array relation does not demonstrate a public
catalog, a plan assignment, a route, or compatibility with an unobserved
consumer.

<a id="b03"></a>
## B03 — Gate trait → in-memory gate relation

**Arrow:** `DpaAcceptanceGate` → `impl DpaAcceptanceGate for InMemoryDpaGate`.
**Mode:** SOURCE. **Trigger:** the trait signature, map field, lock branch, or
this implementation changes. **Evidence:** `src/dpa.rs`; root re-export in
`src/lib.rs`. **Impact:** a static caller compiled against the trait can require
source-level signature review. **Boundary:** this in-memory fake does not prove
an acceptance record, a durable lookup, a caller, or an effect outside its
local memory.

<a id="b04"></a>
## B04 — Audit-sink trait → in-memory sink relation

**Arrow:** `TierSelectionAuditSink` → `impl TierSelectionAuditSink for
InMemoryTierSelectionAuditSink`. **Mode:** SOURCE. **Trigger:** the trait
signature, vector field, snapshot helper, or this implementation changes.
**Evidence:** `src/audit.rs`; root re-export in `src/lib.rs`. **Impact:** a
static source consumer may need its fake expectations checked. **Boundary:**
the vector-backed fake establishes neither an audit destination, retention,
ordering outside one process, nor an observed event.

<a id="b05"></a>
## B05 — Client trait → in-memory client relation

**Arrow:** `StripeClient` → `impl StripeClient for InMemoryStripeClient`.
**Mode:** SOURCE. **Trigger:** the trait signature, local session map,
`arm_failure`, `sessions`, or this implementation changes. **Evidence:**
`src/stripe.rs`; root re-exports in `src/lib.rs`. **Impact:** a static source
consumer may need its client-shaped fake contract checked. **Boundary:** this
local fake does not demonstrate an external system, transmission, durable
state, timing, invocation, or an observed result.

<a id="b06"></a>
## B06 — Declaration set → unresolved-domain relation

**Arrow:** manifest plus local declarations → five unresolved domains: build
selection; callers; identifier provenance; state outside local memory; external
effects and environment operation. **Mode:** UNKNOWN. **Trigger:** a request
asks any B01–B05 relation to prove one of those domains. **Evidence:**
[R08](REFERENCE.md#r08). **Impact:** stop the local-source conclusion and
request bounded evidence from the owner of that domain. **Boundary:** no
SOURCE relation above can be expanded into a compatibility approval, execution
claim, deployed behavior, or independent-review outcome.

Known reverse Cargo consumers (each manifest is a direct edge; activation is
limited to the indicated source path being built/used):

| Relation | Consumer → source relation | Evidence | Unknown / limit |
|---|---|---|---|
| RC-001 | `corelink-stripe-real` → client imports this crate's `TierError`, `StripeClient`, and tier/session types. | `crates/corelink-stripe-real/Cargo.toml`; `src/client.rs` | Native client target selection, construction, and Stripe requests are unproven. |
| RC-002 | `corelink-billing-stripe-materializer` → tier selector imports `TierKind`. | `crates/corelink-billing-stripe-materializer/Cargo.toml`; `src/tier.rs`; `src/handler.rs` | Handler invocation, selected mapping, and durable state effects are unproven. |
| RC-003 | `corelink-billing` → `tier` module publicly re-exports this crate. | `crates/corelink-billing/Cargo.toml`; `src/tier.rs` | Downstream use of the facade and compatibility are unknown. |
| RC-004 | `corelink-container` → checkout route imports `StripeClient`, `CheckoutSessionRequest`, `TenantId`, and `TierKind`; tier-select paths refer to `select_tier`. | `crates/corelink-container/Cargo.toml`; `src/routes/tier_select_checkout.rs`; `src/routes/tier_select/part-01.rs` | Route mount/invocation, build selection, and external effects are unknown. |
| RC-005 | `e2e-signup-flow` → helper imports tier-selection contracts and casts its gate to `DpaAcceptanceGate`. | `tests/e2e-signup-flow/Cargo.toml`; `src/helpers.rs` | Test execution and production behavior are unknown. |
| RC-006 | `e2e-billing-flow` → harness imports tier-selection contracts and converts `TierError`. | `tests/e2e-billing-flow/Cargo.toml`; `src/harness.rs` | Test execution and production behavior are unknown. |

This is the known direct-consumer set from the inspected manifests, not a
complete compiled dependency graph. Features, target resolution, runtime
invocation, and any downstream effects remain outside this evidence.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-tier-selection/SKILL.md#s01)
