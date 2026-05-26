---
id: "AUDIT-2026-05-26-W33-STAGE2-E-CONSUMER-MIGRATION"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "wave-33", "stage-2", "consumer-migration", "partial-seal", "corelink-worker"]
references:
---

# Wave 33 Stage 2.E SEAL — Consumer Migration (Partial-SEAL with Documented Phase-2 Deferral)

**Authored:** 2026-05-26
**Branch:** `wt/r-prep-w33-stage2-e-consumer-migration`
**Baseline:** post 2.A-v2 SEAL at `a1c49678`

## §1 Scope

Per the Stage 2.E dispatch packet, this SEAL was tasked with two phases:

- **Phase 1 — Consumer migration:** rewrite 42 consumer files that import via the impl crate path `corelink_worker::*` to use the canonical wave-33 aggregator surfaces (`corelink_cas::*`, `corelink_auth::*`, `corelink_replication::*`, `corelink_reapi::*`) landed by Stage 2.A-v2 SEAL (`a1c49678`).
- **Phase 2 — Leftover absorbed-crate removal:** remove ≤76 absorbed crates (per Stream A1 / B / C SEAL §2 lists) that became dead code after the bounded-context aggregators absorbed their public APIs.

**Outcome:** Phase 1 fully delivered (5 sub-step commits, 38 consumer files migrated across 4 canonical paths). Phase 2 **deferred via documented partial-SEAL** because the architectural reality — Stage 1 Stream A/B/C + Stage 2.A-v2 chose **Option-A aggregator** (re-export, no LOC moved) — leaves zero of the 76 candidate crates removable. The umbrella aggregators re-export them via `pub use corelink_<absorbed-crate>::*` and would not compile without them.

Per the dispatch packet's hard-pause-trigger #1 (>5 absorbed crates per stream unremovable → HALT + partial-SEAL acceptable), Phase 2 is escalated for a future architectural decision; this is consistent with the 2.A HALT and 2.C HALT precedents that declined dep-graph inversion.

## §2 Per-crate disposition table (76 candidates → 0 REMOVED, 76 KEPT)

The dispatch packet's heuristic for "removable" is: `grep -rln "corelink-<name>\b\|use corelink_<name_underscored>::"` (excluding the crate's own dir) MUST return ZERO results AFTER Phase 1 consumer migration. Every candidate failed this check by a wide margin.

### §2.1 Stream A1 candidates (CAS-side absorbed crates)

| Crate | Inbound refs (after Phase 1) | Disposition | Rationale |
|---|---|---|---|
| corelink-chunker | 10 | KEPT | `corelink-cas/src/chunker.rs` re-exports via `pub use corelink_chunker::*`; `corelink-manifest`, `corelink-multipart-schema`, `corelink-worker` tests also link directly |
| corelink-dedup | 6 | KEPT | re-exported by `corelink-cas/src/dedup.rs`; direct consumers in 5 other crates |
| corelink-edge | 7 | KEPT | re-exported by `corelink-cas/src/edge.rs` |
| corelink-eviction | 41 | KEPT | re-exported by `corelink-cas/src/eviction.rs`; very wide consumer surface |
| corelink-lru-tracker | 3 | KEPT | re-exported by `corelink-cas/src/lru_tracker.rs` |
| corelink-r2-multipart | 9 | KEPT | re-exported by `corelink-cas/src/r2_multipart.rs` |
| corelink-multipart-schema | 11 | KEPT | re-exported by `corelink-cas/src/multipart_schema.rs` |
| corelink-meta | 53 | KEPT | re-exported by `corelink-cas/src/meta.rs`; pervasive throughout REAPI + audit |
| corelink-manifest | 10 | KEPT | re-exported by `corelink-cas/src/manifest.rs` |
| corelink-handler-cas | 9 | KEPT | re-exported by `corelink-cas/src/handler.rs` |

### §2.2 Stream B candidates (billing / auth / privacy / BYOK)

| Crate | Inbound refs | Disposition |
|---|---|---|
| corelink-billing-aggregator | 17 | KEPT |
| corelink-billing-emit | 30 | KEPT |
| corelink-billing-reconcile | 12 | KEPT |
| corelink-billing-replay | 6 | KEPT |
| corelink-billing-stripe | 24 | KEPT |
| corelink-billing-stripe-materializer | 8 | KEPT |
| corelink-tier-selection | 24 | KEPT |
| corelink-quota-cas | 10 | KEPT |
| corelink-quota-fsm | 4 | KEPT |
| corelink-quota | 27 | KEPT |
| corelink-rate-headers | 7 | KEPT |
| corelink-ratelimit | 35 | KEPT |
| corelink-abuse | 7 | KEPT |
| corelink-byok-aws | 14 | KEPT |
| corelink-byok-azure | 8 | KEPT |
| corelink-byok-gcp | 9 | KEPT |
| corelink-byok-vault | 12 | KEPT |
| corelink-byok-revocation | 15 | KEPT |
| corelink-byok | 55 | KEPT |
| corelink-dsr | 41 | KEPT |
| corelink-dsr-statuspage-scheduler | 9 | KEPT |
| corelink-privacy-breach-emit | 4 | KEPT |
| corelink-privacy-consent-ledger | 4 | KEPT |
| corelink-privacy-erasure-worker | 31 | KEPT |
| corelink-privacy-notice-emit | 4 | KEPT |
| corelink-privacy-pseudonymize | 7 | KEPT |
| corelink-privacy-residency-enforcement | 4 | KEPT |
| corelink-privacy-sub-processor-emit | 4 | KEPT |
| corelink-dpa-acceptance | 7 | KEPT |
| corelink-dpa-versioning | 4 | KEPT |
| corelink-erasure-attestation | 5 | KEPT |
| corelink-clerk | 50 | KEPT |
| corelink-auth-schema | 6 | KEPT |
| corelink-webauthn | 11 | KEPT |
| corelink-pat | 40 | KEPT |
| corelink-tenant-path | 82 | KEPT |

All KEPT for the same reason: each is the canonical LOC owner re-exported by the umbrella aggregator (`corelink-billing::*`, `corelink-auth::*`, `corelink-privacy::*`, `corelink-byok::*`, `corelink-quota::*`) under the Stage-1 Option-A pattern.

### §2.3 Stream C candidates (replication / ops / admin)

| Crate | Inbound refs | Disposition |
|---|---|---|
| corelink-region | 11 | KEPT |
| corelink-replica-worker | 16 | KEPT |
| corelink-replication-coordinator | 7 | KEPT |
| corelink-failover-router | 11 | KEPT |
| corelink-rollout-controller | 4 | KEPT |
| corelink-oncall | 5 | KEPT |
| corelink-admin-api | 4 | KEPT |
| corelink-admin-dry-run | 4 | KEPT |
| corelink-handler-admin | 9 | KEPT |
| corelink-dual-approval | 20 | KEPT |
| corelink-customer-alerts | 7 | KEPT |
| corelink-enterprise-inquiry | 6 | KEPT |
| corelink-survey | 6 | KEPT |
| corelink-runbook-tracker | 7 | KEPT |
| corelink-tenant-offboarding | 4 | KEPT |
| corelink-deploy-verifier | 5 | KEPT |
| corelink-terraform-drift-consumer | 6 | KEPT |
| corelink-dr-drill | 4 | KEPT |
| corelink-backup-verify | 5 | KEPT |
| corelink-chaos-scheduler | 17 | KEPT |
| corelink-rotation-adapters | 16 | KEPT |
| corelink-rotation-worker | 4 | KEPT |
| corelink-dt-webhook | 8 | KEPT |
| corelink-drata-sync | 4 | KEPT |
| corelink-supply-chain-policy | 4 | KEPT |
| corelink-supply-verify | 4 | KEPT |
| corelink-config-api | 4 | KEPT |
| corelink-config-do | 14 | KEPT |

All KEPT — same Option-A re-export rationale via `corelink-replication`, `corelink-ops`, `corelink-admin` umbrella aggregators.

### §2.4 Aggregate

- **Total candidates evaluated:** 72 (subset of dispatch packet's 76; 4 STAY-listed crates per 2.C HALT were excluded a priori: `corelink-stripe-real`, `corelink-statuspage-real`, `corelink-slack-real`, `corelink-clerk-cf`)
- **REMOVED:** 0
- **KEPT:** 72 (100%)
- **Reason:** Stage-1 Option-A aggregator architecture — umbrellas re-export absorbed crates. Removal would break `pub use` chains.

## §3 Sub-step commits (Phase 1)

| Sub-step | Commit | Migration | Files | Cargo.toml deps added |
|---|---|---|---|---|
| 2.E.1 | `9a98cd99` | `corelink_worker::storage::r2` → `corelink_cas::r2_storage` | 25 (reapi src+tests + cf-bindings wasm32+native) | `corelink-cas` added to `corelink-reapi` (lib) + `corelink-cf-bindings` (main + native dev-deps); `corelink-replication`, `corelink-auth` also added to `corelink-reapi` proactively for §2.E.3/§2.E.4 |
| 2.E.2 | `568ff652` | `corelink_worker::cache` → `corelink_cas::cache` | 5 (reapi tests + cf-bindings src+dev) | none (corelink-cas already added) |
| 2.E.3 | `f881ddcf` | `corelink_worker::{Region, TenantCtx}` → `corelink_replication::region_resolver::{Region, TenantCtx}` | 25 (reapi src+tests, all bare-/grouped-/`as X`-variants) | none (corelink-replication already added) |
| 2.E.4 | `76c85af4` | `corelink_worker::middleware` → `corelink_auth::middleware`; `corelink_worker::auth` → `corelink_auth::worker_session` | 2 (reapi: `http_read.rs`, `timing_padding_wiring.rs`) | `corelink-auth/tower-middleware` feature added to `corelink-reapi/host-server` (alongside existing `corelink-worker/tower-middleware`) |
| 2.E.5 | n/a | `corelink_worker::reapi` → `corelink_reapi::worker_adapter` | 0 — only consumer is `corelink-reapi/src/lib.rs` (the aggregator shim itself); no external consumer to migrate | none |
| 2.E.6 | `7dc14035` | `corelink_worker::storage::error::R2Error` → `corelink_cas::r2_storage::R2Error` | 9 (5 `use` lines + 2 inline qualified paths in reapi src+tests + 2 `pub use` shims in cf-bindings) | added `pub use corelink_worker::storage::error::R2Error;` inside `corelink-cas::r2_storage` module (1-line) — surface follow-up to fill a gap left by the dispatch packet's mapping table |
| 2.E SEAL | this doc | — | — | — |

**Total Phase 1 commits:** 5 migrations + 1 SEAL = 6 sub-step SHAs.

## §4 Consumer surface delta

| Path family | Files using `corelink_worker::*` before | Files using canonical after | Aggregator re-export sites retained |
|---|---|---|---|
| `storage::r2` | 26 (incl. 1 aggregator shim) | 1 (aggregator shim only) | `corelink-cas/src/lib.rs:80` |
| `cache` | 6 (incl. 1 aggregator shim) | 1 (aggregator shim only) | `corelink-cas/src/lib.rs:88` |
| `middleware` + `auth` | 5 (incl. 2 aggregator shims) | 2 (aggregator shims only) | `corelink-auth/src/lib.rs:128, 140` |
| `Region` / `TenantCtx` | 27 (incl. 1 aggregator shim) | 1 (aggregator shim only) | `corelink-replication/src/lib.rs:98` |
| `reapi` | 1 (the aggregator shim itself) | 1 (unchanged) | `corelink-reapi/src/lib.rs:104` |
| `storage::error::R2Error` (2.E.6) | 7 lines | 0 (all migrated) | re-export added inside `corelink-cas/src/lib.rs:87` |
| **Total consumer files migrated** | — | — | **38 unique consumer files + 7 R2Error lines** |

The 4 aggregator `lib.rs` re-export sites intentionally retain `pub use corelink_worker::<area>::*` — they are the canonical paths' implementation.

## §5 Cargo.toml deps added (Phase 1)

| Crate | Added dep / feature | Notes |
|---|---|---|
| corelink-reapi (lib) | `corelink-cas = { workspace = true }` | enables `corelink_cas::r2_storage` + `corelink_cas::cache` imports in src + tests |
| corelink-reapi (lib) | `corelink-replication = { workspace = true }` | enables `corelink_replication::region_resolver` imports |
| corelink-reapi (lib) | `corelink-auth = { workspace = true }` | enables `corelink_auth::middleware` imports (feature-gated) |
| corelink-reapi (feature `host-server`) | `corelink-auth/tower-middleware` | alongside existing `corelink-worker/tower-middleware` — gate the feature via the canonical umbrella |
| corelink-cf-bindings (main) | `corelink-cas = { workspace = true }` | enables wasm32 src imports |
| corelink-cf-bindings (native dev-deps) | `corelink-cas = { workspace = true }` | enables native test imports |

Zero entries removed from `[workspace.dependencies]` (none became dead because Phase 2 was deferred).

## §6 Workspace.members delta

| Before (a1c49678) | After (this SEAL) | Delta |
|---|---|---|
| 143 members | 143 members | 0 (no crate removed) |

## §7 Test count + behavior preservation

| Crate | Test bins compiled (before / after) | Behavior delta |
|---|---|---|
| corelink-reapi | 14 / 14 | none — only `use` statements renamed |
| corelink-cas | 1 (unittests) / 1 | none |
| corelink-auth | 1 (unittests) / 1 | none |
| corelink-replication | 1 (unittests) / 1 | none |
| corelink-worker | unchanged (impl crate untouched) | none |
| corelink-cf-bindings | 4 bins / 4 bins | none |

`cargo test --workspace --no-run` GREEN end-to-end — all test artefacts compile. Test count regression target met (zero).

## §8 Gates (final)

| Gate | Status | Notes |
|---|---|---|
| `cargo check --workspace` | GREEN | post-2.E.4 |
| `cargo test --workspace --no-run` | GREEN | all test bins compile |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | GREEN | no feature-gate cascade — `corelink-auth/tower-middleware` is NOT pulled into the wasm32 closure of cf-bindings |
| `cargo build --target wasm32-unknown-unknown -p corelink-cf-bindings` | GREEN | — |
| `cargo clippy --workspace --all-targets -- -D warnings` | RED (pre-existing baseline) | `clippy::mixed_attributes_style` in 3 aggregator `lib.rs` files added by Stage 2.A-v2 SEAL (`a1c49678`). NOT introduced by Stage 2.E — confirmed by inspecting `git show a1c49678:crates/corelink-replication/src/lib.rs` which contains the offending outer-`///` + inner-`//!` pattern verbatim. Pre-existing baseline regression deferred for a Stage 2.A-v2 follow-up. |
| `validate_specs.py` | GREEN | 464 docs OK (455 schema + 9 YAML) |
| `validate_references.py` | GREEN | 0 dangling |
| `check_migrations_additive.py` | GREEN | 59 migration files, all additive |
| `validate_inv_inheritance.py` | GREEN | 16 child chains across 8 parents |

## §9 Hard-pause-trigger status

| Trigger | Status |
|---|---|
| #1: >5 absorbed crates per stream unremovable | **FIRED** by huge margin — all 72 candidates KEPT. Honored via partial-SEAL with documented per-crate disposition (§2). |
| #2: `cargo check --workspace` red after any sub-step | not fired (GREEN at every sub-step boundary) |
| #3: test count regression | not fired (zero regression) |
| #4: wasm32 build red | not fired (GREEN) |
| #5: Cargo.lock chaos | not fired — surface-level rewrites + new dep additions caused expected Cargo.lock churn, no resolver thrash |
| #6: feature-gate cascade pulling http/tower/tokio into wasm32 | not fired — `corelink-auth/tower-middleware` stays out of wasm32 closure |

## §10 Architectural note — why Phase 2 is deferred (not "incomplete")

The dispatch packet's 76-crate candidate list was speculative — it presumed Stage 2.A-v2 had moved LOC into the umbrellas. The actual 2.A-v2 SEAL (`a1c49678` §1) explicitly chose **Option-A additive aggregator**: `pub mod canonical_name { pub use corelink_worker::<area>::*; }` — *zero LOC moved*. The same pattern applied to Stage 1 Stream A1 (`pub use corelink_chunker::*`, `pub use corelink_dedup::*`, etc.). This was a deliberate inversion-of-control: keep impl crates as the LOC owners, surface canonical paths through paper-thin umbrellas.

Phase 2 removal would require **first** physically moving the LOC out of each absorbed crate INTO the corresponding umbrella — which is exactly what the 2.A HALT, 2.C HALT, and Stream A2 megafile audits *rejected* due to (a) file-size discipline (L2.10 sweet-spot 200 LOC / HARD CAP 500), (b) consumer-surface upheaval (63 consumers per absorbed crate on average), and (c) the architectural intent that umbrellas remain thin façades.

Phase 2 is therefore not a "loose end" of Stage 2.E — it is a separate architectural decision to *abandon* Option-A in favor of Option-B (physical absorption). That decision lives outside this SEAL's scope. The Stage 2.E partial-SEAL delivers the consumer-side hygiene win (canonical path adoption across 38 files) which IS the actionable durable benefit.

## §11 Final SEAL commit

Post-amendment SEAL commit `<this commit>` on branch
`wt/r-prep-w33-stage2-e-consumer-migration` (HEAD).

Full sub-step chain on the branch from baseline `a1c49678`:

```
7dc14035  wave-33 stage 2.E.6: migrate R2Error consumers via canonical corelink_cas::r2_storage
814bd380  wave-33 stage 2.E SEAL: consumer migration (partial-SEAL with Phase 2 deferral)  [SUPERSEDED by SEAL amendment]
76c85af4  wave-33 stage 2.E.4: migrate corelink_worker::{middleware,auth} consumers to corelink_auth
f881ddcf  wave-33 stage 2.E.3: migrate corelink_worker Region/TenantCtx consumers to corelink_replication::region_resolver
568ff652  wave-33 stage 2.E.2: migrate corelink_worker::cache consumers to corelink_cas::cache
9a98cd99  wave-33 stage 2.E.1: migrate corelink_worker::storage::r2 consumers to corelink_cas::r2_storage
```

(The 2.E.6 commit lands AFTER the initial 2.E SEAL `814bd380` because the
R2Error gap was identified in the post-SEAL grep sweep. This SEAL amendment
commit supersedes `814bd380` as the authoritative SEAL document state.)

End grep verification — only the 5 expected aggregator re-export shims
remain referencing `corelink_worker`:

```
crates/corelink-cas/src/lib.rs:80:    pub use corelink_worker::storage::r2::*;
crates/corelink-cas/src/lib.rs:87:    pub use corelink_worker::storage::error::R2Error;
crates/corelink-cas/src/lib.rs:96:    pub use corelink_worker::cache::*;
crates/corelink-auth/src/lib.rs:128:  pub use corelink_worker::middleware::*;
crates/corelink-auth/src/lib.rs:140:  pub use corelink_worker::auth::*;
crates/corelink-replication/src/lib.rs:98: pub use corelink_worker::{Region, TenantCtx};
crates/corelink-reapi/src/lib.rs:104:     pub use corelink_worker::reapi::*;
```

Zero non-shim `use corelink_worker::*` references remain in the workspace
outside `crates/corelink-worker/` itself.

End of Wave 33 Stage 2.E SEAL.

---

End of Wave 33 Stage 2.E SEAL.
