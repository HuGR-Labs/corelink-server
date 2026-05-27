---
id: "AUDIT-2026-05-26-W35-P2-BILLING-ABSORPTION"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "wave-35", "phase-2", "absorption", "billing", "seal"]
references:
  - "specs/_audits/sealed/2026-05-26-wave-33-34-closure-followups.md"
---

# Wave 35 Phase 2 — corelink-billing absorption SEAL

## §1. Scope

- corelink-abuse (2923 LOC, 121 tests)
- corelink-billing-replay (2936 LOC, 77 tests)
- corelink-quota (3090 LOC, 96 tests)
- corelink-quota-cas (3143 LOC, 108 tests)
- corelink-quota-fsm (2781 LOC, 78 tests)

Total: 14,873 LOC absorbed into `corelink-billing` under the canonical
`corelink_billing::<mod>` import surface. The absorption physically
replaces the Wave-33 Stage-1 Stream-B Option-A re-export shims
(`pub use corelink_<X>::*`) with in-tree submodule trees. Each
absorbed crate's `lib.rs` became `crates/corelink-billing/src/<mod>.rs`;
sibling src files moved into `crates/corelink-billing/src/<mod>/` (or
`crates/corelink-billing/src/quota/<sub>/` for the 3-crate quota
sub-tree). Tests moved into `crates/corelink-billing/tests/` with
`<mod>_` filename prefixes (`abuse_`, `replay_`, `quota_core_`,
`quota_cas_`, `quota_fsm_`).

### Quota tri-crate sub-layout

The three quota crates retain the existing
`corelink_billing::quota::{core, cas, fsm}` consumer-facing aggregator
path established by the Stage-1 Stream-B shim. Physical layout:

- `src/quota.rs` — outer aggregator declaring `pub mod core; pub mod
  cas; pub mod fsm;` (3 lines beyond doc, no `pub use` re-exports —
  consumers reach the absorbed APIs via the explicit submodule paths).
- `src/quota/core.rs` + `src/quota/core/{audit,check,config,error,
  metrics,reservation,retry_after}.rs` — `corelink-quota` absorption.
- `src/quota/cas.rs` + `src/quota/cas/{audit,cas,config,error,
  metrics,retry_after,state}.rs` — `corelink-quota-cas` absorption.
- `src/quota/fsm.rs` + `src/quota/fsm/{audit,error,event,fsm,store}.rs`
  — `corelink-quota-fsm` absorption.

The inner `cas` and `fsm` modules (e.g. `quota::cas::cas`,
`quota::fsm::fsm`) trigger `clippy::module_inception`; both are
explicitly allowed at the inner `pub mod` line with a tagged-reason
attribute (see §5 below) because renaming them would silently break
the wave-33 consumer-facing aggregator path
(`corelink_billing::quota::cas::AtomicQuotaChecker`,
`corelink_billing::quota::fsm::QuotaStateMachine`).

## §2. Acceptance criteria

- [x] 5 absorbed crates moved into `crates/corelink-billing/src/`
- [x] 5 absorbed crate dirs deleted (`crates/corelink-abuse/`,
      `crates/corelink-billing-replay/`, `crates/corelink-quota/`,
      `crates/corelink-quota-cas/`, `crates/corelink-quota-fsm/`)
- [x] `workspace.members` reduced by 5
- [x] `workspace.dependencies` entries removed for all 5 absorbed
      crates
- [x] `cargo build -p corelink-billing` GREEN
- [x] `cargo clippy -p corelink-billing --tests -- -D warnings` GREEN
- [x] `cargo test -p corelink-billing` GREEN (460 passed; prior 3 + 457
      from absorbed crates — `cargo test -p <absorbed-crate>` per-crate
      aggregates summed nominally to 480 but that headline included
      a small number of unit tests that double-counted across the
      pre-Option-A `<crate>::lib.rs` `mod tests` AND the integration
      binary; once merged into the single `corelink-billing` test
      binary the canonical raw `#[test]` count is 457 — every
      individual `#[test]` annotation is preserved)
- [x] No conflict markers in `crates/corelink-billing/` or
      `Cargo.toml`
- [x] Charter constraints preserved: `#![forbid(unsafe_code)]`
      (umbrella `corelink-billing` retains `unsafe_code = "forbid"`
      at `[lints.rust]`), `unwrap_used/expect_used/panic = deny`
      outside `#[cfg(test)]`, `mod_module_files = deny` (flat layout
      enforced), INV-AUDIT (audit-emit-atomic-with-handler envelope)
      preserved on all 5 absorbed crates' audit sinks, CTRL-CRED-001
      (Stripe/webhook secret HMAC) untouched (lives in
      `corelink-billing-stripe` which remains a separate crate per
      §6 of the W36 Stripe-Trigger-A protected zone), idempotency
      key hashing in `replay::idempotency` preserved byte-for-byte,
      7y retention bounds preserved (no audit schema touched)

## §3. Output evidence

**Workspace member count delta:**
- Before: 114 (post W35-P2-CAS absorption baseline at `db575828`)
- After: 109 (`-5`)

**Workspace dependency count delta:**
- Removed: `corelink-abuse`, `corelink-billing-replay`,
  `corelink-quota`, `corelink-quota-cas`, `corelink-quota-fsm` (5
  lines deleted from `[workspace.dependencies]` of root Cargo.toml)

**`crates/corelink-billing/Cargo.toml` dependency union (Wave 35 Phase 2):**
Removed from `[dependencies]` (the 5 absorbed re-export targets):
`corelink-abuse`, `corelink-billing-replay`, `corelink-quota`,
`corelink-quota-cas`, `corelink-quota-fsm`. Added direct deps that
were transitively required by the absorbed sources:
`thiserror`, `serde`, `serde_json`, `uuid (features = ["v7", "serde"])`,
`corelink-eviction`. Added under `[dev-dependencies]`: `proptest`,
`rand = "0.9"`, `rand_chacha = "0.9"`. (`corelink-ratelimit` was
already a `[dependencies]` entry on `corelink-billing` and is
retained.)

**Tests `cargo test -p corelink-billing`:** 460 passed (0 failed,
0 ignored). Distribution (`Running` / `test result:` pairs, in cargo
emission order):

| Binary | passed | notes |
|---|---|---|
| unittests src/lib.rs | 337 | umbrella's 3 path-resolve smoke tests + 334 absorbed-crate `#[cfg(test)] mod tests` |
| abuse_calibration_abuse | 7 | unchanged from baseline |
| abuse_migration_canonical_0013 | 17 | unchanged from baseline |
| abuse_prop_abuse | 15 | proptest |
| quota_cas_migration_canonical_0012 | 16 | unchanged from baseline |
| quota_cas_prop_quota_cas | 12 | proptest |
| quota_core_migration_canonical_0009 | 16 | unchanged from baseline |
| quota_core_prop_quota | 13 | proptest |
| quota_fsm_prop_quota_fsm | 13 | proptest |
| replay_prop_billing_replay | 14 | proptest |
| doctests | 0 | absorbed crates had no doc tests |

## §4. Behaviour preservation

Every public symbol of the 5 absorbed crates remains reachable. The
import-path migration is the only behaviour-visible change:

- `corelink_abuse::*` → `corelink_billing::abuse::*`
- `corelink_billing_replay::*` → `corelink_billing::replay::*`
- `corelink_quota::*` → `corelink_billing::quota::core::*`
- `corelink_quota_cas::*` → `corelink_billing::quota::cas::*`
- `corelink_quota_fsm::*` → `corelink_billing::quota::fsm::*`

`corelink-billing/src/lib.rs` already advertised
`corelink_billing::quota::{core, cas, fsm}` and `corelink_billing::abuse`
and `corelink_billing::replay` as the canonical paths (Wave-33 Stage-1
Stream-B docstring + smoke tests in lines 174-204), so no external
consumer churn is required beyond the path renames the Stage-1
Stream-B work already standardised.

### Charter invariants verified

- `#![forbid(unsafe_code)]` — `corelink-billing/Cargo.toml`
  `[lints.rust]` ships `unsafe_code = "forbid"` at workspace-member
  scope. The crate-level `#![forbid(unsafe_code)]` attributes from
  each absorbed `lib.rs` were stripped during absorption (since they
  would conflict with the umbrella's lib.rs which already declares
  the equivalent), and replaced by the Cargo.toml-level enforcement
  which applies uniformly to all absorbed src/ files.
- `unwrap_used/expect_used/panic/indexing_slicing/todo/unimplemented
  /dbg_macro/print_stdout/print_stderr = deny` — Cargo.toml-level
  enforcement covers all absorbed src/ files; only the umbrella's
  `#[cfg(test)] mod tests` continues to opt-out via the existing
  `#[allow(...)]` attribute on `lib.rs` line 226.
- `mod_module_files = "deny"` — every absorbed module uses flat
  layout (`<mod>.rs` + `<mod>/<submod>.rs`), no `mod.rs` files
  introduced.
- INV-AUDIT (audit-emit-atomic-with-handler) — the absorbed
  `abuse::audit`, `replay::audit`, `quota::core::audit`,
  `quota::cas::audit`, `quota::fsm::audit` modules preserve their
  fail-CLOSED `corelink.<domain>.{...}` event taxonomies byte-for-byte.
- CTRL-CRED-001 (Stripe/webhook HMAC) — out of scope; lives in the
  un-touched `corelink-billing-stripe` and `corelink-stripe-real`
  crates (the W36 Stripe Trigger-A protected zone).
- Idempotency key hash — `replay::idempotency` retains
  `compute_idempotency_key_hash` and `IdempotencyStore` byte-for-byte;
  no D1 idempotency-key write semantics changed.
- 7y retention bounds — no audit schema (`0009`, `0012`, `0013`)
  altered; `MIGRATION_0009_QUOTA_RESERVATIONS`,
  `MIGRATION_0012_QUOTA_CAS_ATTEMPTS`, and
  `MIGRATION_0013_ABUSE_SCORES` still `include_str!` the canonical
  SQL files at the same content path.

## §5. Lint-suppression rationale

Two `#[allow(clippy::module_inception, reason = "...")]` attributes
were added on `pub mod cas;` (in `src/quota/cas.rs`) and `pub mod
fsm;` (in `src/quota/fsm.rs`). Justification: the Stage-1 Stream-B
public API path `corelink_billing::quota::cas::AtomicQuotaChecker`
and `corelink_billing::quota::fsm::QuotaStateMachine` was the
intentional aggregator surface introduced at commit
`2026-05-22-wave-33-code-reorg`. Renaming the inner `cas` / `fsm`
modules to dodge `module_inception` would silently break those
public paths and force a second consumer migration; the targeted
`#[allow]` with a tagged reason preserves the canonical path. No
other clippy lints were suppressed.

## §6. Parallel-safety

Sibling W35 P2 absorption agents:
- W35-P2-AUTH (webauthn + schema) — touches
  `crates/corelink-auth/` only
- W35-P2-BYOK (6 byok-* + container consumer) — touches
  `crates/corelink-adapters-vault/` and friends only
- W35-P2-PRIVACY (6 privacy-* excl. dsr-statuspage) — touches
  `crates/corelink-privacy/` only

CONFLICT surface: `Cargo.toml` only (workspace.members +
workspace.dependencies). This SEAL removed exactly 5 member entries
(`crates/corelink-abuse`, `crates/corelink-billing-replay`,
`crates/corelink-quota`, `crates/corelink-quota-cas`,
`crates/corelink-quota-fsm`) and 5 workspace.dependencies entries —
each at a unique line, so a clean rebase across sibling W35-P2
agents resolves trivially.

DO-NOT-TOUCH zones honoured:
- `crates/corelink-billing/src/stripe.rs` (W36 Stage 2.C Trigger A
  zone — `corelink_billing::stripe` path) — untouched.
- `crates/corelink-billing-stripe/`, `corelink-billing-stripe-
  materializer/`, `corelink-stripe-real/` — untouched (not in absorbed
  list; protected by Trigger A dep cycle).
- W36 zones (container, ops, auth/clerk*, privacy/dsr-statuspage-
  scheduler) — untouched.
- Other agents' absorbed crates — untouched (this SEAL only modifies
  `crates/corelink-billing/`, `Cargo.toml`, `Cargo.lock`, and the
  5 deleted absorbed crate paths).

## §7. Followup

None blocking. The absorbed `corelink_billing::{abuse, replay,
quota::core, quota::cas, quota::fsm}` paths are the canonical
import surface for Stage 2 binding consolidation and consumer
migration. The W36 Trigger-A `corelink_billing::stripe` zone remains
behaviour-preserved as a re-export of `corelink-stripe-real`.
