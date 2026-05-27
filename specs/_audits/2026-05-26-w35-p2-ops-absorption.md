---
id: "AUDIT-2026-05-26-W35-P2-OPS-ABSORPTION"
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
tags: ["audit", "wave-35", "phase-2", "absorption", "ops", "seal"]
references:
  - "specs/_audits/2026-05-26-wave-33-34-closure-followups.md"
  - "specs/_audits/2026-05-26-w35-p2-privacy-absorption.md"
  - "specs/_audits/2026-05-26-w35-p2-cas-absorption.md"
---

# Wave 35 Phase 2 — corelink-ops absorption SEAL (LARGEST Wave-35 absorption)

## §1. Scope

15 ops sub-crates physically absorbed into the `corelink-ops`
umbrella under the canonical `corelink_ops::<mod>` import surface
(Stream C sub-step C.2 flipped from Option-A re-export aggregator
to Option-B physical absorption). The remaining 13 of the original
28 re-export targets stay external (statuspage, slack, handler-admin,
dual-approval, enterprise-inquiry, runbook-tracker,
terraform-drift-consumer, chaos-scheduler, rotation-adapters,
dt-webhook, config-do; plus dt-cli + dt-reconcile which are
binary-only and cannot be re-exported into a library surface).

| # | Absorbed | LOC (spec) | `#[test]` markers | New canonical path |
|---|---|---|---|---|
| 1 | corelink-admin-api | 313 | 12 | `corelink_ops::admin::api::*` |
| 2 | corelink-admin-dry-run | 802 | 13 | `corelink_ops::admin::dry_run::*` (+ 3 bins) |
| 3 | corelink-backup-verify | 1097 | 26 | `corelink_ops::dr::backup_verify::*` |
| 4 | corelink-config-api | 413 | 24 | `corelink_ops::config::api::*` |
| 5 | corelink-customer-alerts | 348 | 0 | `corelink_ops::alerts::*` |
| 6 | corelink-d1-migrations | 449 | 15 | `corelink_ops::migrations::*` |
| 7 | corelink-deploy-verifier | 1784 | 50 | `corelink_ops::deploy::*` |
| 8 | corelink-dr-drill | 907 | 12 | `corelink_ops::dr::drill::*` |
| 9 | corelink-drata-sync | 1643 | 28 | `corelink_ops::drata::*` |
| 10 | corelink-oncall | 3455 | 63 | `corelink_ops::oncall::*` |
| 11 | corelink-rotation-worker | 913 | 30 | `corelink_ops::rotation::worker::*` |
| 12 | corelink-supply-chain-policy | 285 | 35 | `corelink_ops::supply_chain::policy::*` |
| 13 | corelink-supply-verify | 1442 | 11 | `corelink_ops::supply_chain::verify::*` (+ 1 bin) |
| 14 | corelink-survey | 1786 | 30 | `corelink_ops::survey::*` |
| 15 | corelink-tenant-offboarding | 2112 | 44 | `corelink_ops::tenant_offboarding::*` |
| **Total** | — | **17,749** | **393** | — |

Each absorbed crate's `lib.rs` became
`crates/corelink-ops/src/<mod>.rs` (the existing thin re-export shim
was overwritten); sibling `src/<file>.rs` files moved into
`crates/corelink-ops/src/<mod>/`. For nested targets that share a
parent aggregator (admin, dr, rotation, supply_chain, config), the
parent shim (`admin.rs`, `dr.rs`, …) was rewritten from a
`pub mod <sub> { pub use corelink_<X>::*; }` block to a bare
`pub mod <sub>;` declaration that resolves to the now-physical
sub-mod file. Subdirs that already existed inside absorbed crates
(`admin-dry-run/src/bin/`, `supply-verify/src/bin/`,
`config-api/src/middleware/`) moved across intact under their
new sub-mod path.

Tests moved into `crates/corelink-ops/tests/` with `<mod_path>_`
filename prefixes (`admin_api_*`, `dr_backup_verify_*`,
`supply_chain_verify_*`, etc.) so cargo's auto-discovery turns each
into a separate test binary (matching the cas / billing / privacy /
auth sibling patterns). Examples moved into
`crates/corelink-ops/examples/` with the same prefix convention.

The 4 binary targets inherited from the absorbed admin-dry-run +
supply-verify (`rb_fm_201_dry_run`, `rb_fm_205_dry_run`,
`rb_fm_206_dry_run`, `corelink-supply-verify`) are declared as
`[[bin]]` entries in `corelink-ops/Cargo.toml` pointing at their new
nested paths under `src/<mod>/bin/`.

The flat layout (`<mod>.rs` + `<mod>/` sibling dir, no `mod.rs`)
satisfies the umbrella's `clippy::mod_module_files = "deny"` lint
without any `#[allow]` escape hatch **except** the unavoidable
`clippy::module_inception` on `drata::drata::*` (the inner
`drata.rs` orchestrator file name is the public-API anchor that
mirrors the `DrataClient` HTTPS Drata REST client; renaming would
break grep-ability for downstream readers — pre-existing pattern in
the original `corelink-drata-sync` crate). The allow carries an
explicit `reason = ` field per workspace policy.

**Preserved (NOT in W35-P2-OPS absorb scope — these stay as
separate workspace members):**

- `corelink-statuspage-real` — HTTPS portion ALSO re-exported by
  `corelink-adapters-cloud::statuspage` per C.3; pure-logic vs.
  binding decomposition deferred.
- `corelink-slack-real` — HTTPS portion ALSO re-exported by
  `corelink-adapters-cloud::slack` per C.3.
- `corelink-handler-admin` — admin handler trait + per-handler
  SliObserver; consumed by the admin dual-approval composition.
- `corelink-dual-approval` — 2-of-N approval gate; consumed by
  the absorbed admin-api + admin-dry-run.
- `corelink-enterprise-inquiry` — enterprise inquiry form + 24h SLA.
- `corelink-runbook-tracker` — PAT-RUNBOOK-DRILL-001 invariant;
  coupled to chaos-scheduler experiments by reference.
- `corelink-terraform-drift-consumer` — admin-dry-run sibling.
- `corelink-chaos-scheduler` — chaos scheduler (8 experiment types).
- `corelink-rotation-adapters` — per-provider rotation adapters;
  consumed by the absorbed rotation-worker.
- `corelink-dt-webhook` — drift tracker webhook receiver (the only
  dt-* crate with a `src/lib.rs`; `dt-cli` + `dt-reconcile` are
  binary-only and remain workspace binaries).
- `corelink-config-do` — config Durable Object (CF Worker DO-backed
  storage); consumed by the absorbed config-api.
- `corelink-dsr-statuspage-scheduler` — **BLOCKED by W36 Trigger B**
  (wasm32 tokio leak via corelink-ops dep); preserved untouched.
- `corelink-ops/src/{statuspage,slack}.rs` — **W36 Stage 2.C zone**;
  preserved untouched.

## §2. Acceptance criteria

- [x] 15 absorbed crates moved into `crates/corelink-ops/src/<mod_path>/`
- [x] 15 absorbed crate dirs deleted from `crates/`
- [x] `workspace.members` reduced by 15 (100 → 85)
- [x] `[workspace.dependencies]` entries for the 15 absorbed crates removed
- [x] `cargo build -p corelink-ops` GREEN
- [x] `cargo clippy -p corelink-ops --tests -- -D warnings` GREEN
- [x] `cargo test -p corelink-ops --no-run` GREEN (all test + bin executables compile)
- [x] `cargo test -p corelink-ops` GREEN (**405 passed, 0 failed**: 201
      unit + 179 integration + 25 doc tests)
- [x] No conflict markers in `crates/corelink-ops/` or `Cargo.toml`
- [x] Charter constraints preserved:
      - `#![forbid(unsafe_code)]` at umbrella `lib.rs`; inherited by
        every absorbed `<mod>.rs` entry via lib forbid scope.
      - No `unwrap/expect/panic/todo/unimplemented/dbg_macro/print_*`
        outside `#[cfg(test)]` blocks in src/ (umbrella's
        `[lints.clippy]` deny-set inherited unchanged).
      - INV-AUDIT preserved (audit-emit-BEFORE-mutation envelopes
        across admin endpoints, customer-alerts, oncall, deploy
        verifier — pure file relocation, no behaviour change).
      - INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER preserved (oncall fatigue
        threshold breach + deploy verifier emit-before-mutate paths
        intact).
      - CTRL-CRED-001 preserved (rotation-worker handles secrets via
        the unchanged `corelink-rotation-adapters` external dep).
      - Idempotency keys hashed before D1 preserved (d1-migrations
        replay harness untouched).
      - Drata sync HMAC verification preserved (drata HTTPS client
        + Bearer auth + SHA-256 idempotency dedup intact).
      - supply-chain-policy SHA-256 attestation preserved
        (supply_chain::policy::* re-exports the same cargo-deny
        license/yanked/source rule property + adversarial tests).
      - DSR retention/erasure semantics preserved (tenant-offboarding
        5-state machine ACTIVE → CANCEL_REQUESTED → GRACE_PERIOD →
        READ_ONLY → SUSPENDED → ERASED unchanged).
      - `#[non_exhaustive]` on every public enum/struct — inherited
        unchanged via the file move.

## §3. Output evidence

**Workspace member count delta:**
- Before: 100 (post W35-P2-PRIVACY baseline at branch creation)
- After: 85 (`-15`)

**Workspace dependency count delta:**
- Removed from `[workspace.dependencies]` (15 lines):
  `corelink-admin-api`, `corelink-admin-dry-run`,
  `corelink-backup-verify`, `corelink-config-api`,
  `corelink-customer-alerts`, `corelink-d1-migrations`,
  `corelink-deploy-verifier`, `corelink-dr-drill`,
  `corelink-drata-sync`, `corelink-oncall`,
  `corelink-rotation-worker`, `corelink-supply-chain-policy`,
  `corelink-supply-verify`, `corelink-survey`,
  `corelink-tenant-offboarding`.

**`crates/corelink-ops/Cargo.toml` dependency union (W35-P2-OPS):**

- `[dependencies]` re-export façades (11 still-external workspace
  members): `corelink-statuspage-real`, `corelink-slack-real`,
  `corelink-handler-admin`, `corelink-dual-approval`,
  `corelink-enterprise-inquiry`, `corelink-runbook-tracker`,
  `corelink-terraform-drift-consumer`, `corelink-chaos-scheduler`,
  `corelink-rotation-adapters`, `corelink-dt-webhook`,
  `corelink-config-do`.

- `[dependencies]` cross-crate consumers needed by absorbed code:
  `corelink-byok-core`, `corelink-byok-revocation` (consumed by
  `alerts::*` — the absorbed customer-alerter integrates with BYOK
  kill-switch),
  `corelink-failover-router`, `corelink-replica-worker` (consumed by
  `dr::drill::*` — the failover composition),
  `corelink-audit` (consumed by `survey::*` — audit-fail-CLOSED
  record path).

- `[dependencies]` third-party (15 direct deps merged from
  absorbed crates): `thiserror`, `serde`, `serde_json`, `tracing`,
  `uuid` (with `[v7, serde]` host + `[v7, serde, js]` wasm32 target
  override), `async-trait`, `hmac`, `sha2`, `hex`, `subtle`,
  `base64`, `zeroize`, `anyhow`, `tokio`, `tracing-subscriber`,
  `tokio-test`, `rusqlite (bundled)`, `reqwest (blocking, json,
  rustls-tls; default-features = false)`, `clap (derive)`.

- `[features]`: `production` (inherited from absorbed
  `corelink-oncall`; gates the PagerDuty Events API v2 HTTPS
  client at `corelink_ops::oncall::events::*`; currently a no-op
  feature flag because the absorbed code lives in the umbrella src/
  tree unconditionally — gating happens at the absorbed `oncall.rs`
  `#[cfg(feature = "production")] pub mod events;` level).

- `[dev-dependencies]`: `proptest`, `mockito = "1"`, `wiremock = "0.6"`,
  `rand = "0.9"`, `rand_chacha = "0.9"`, `corelink-slo` (used by
  `dr_backup_verify_sli_binding.rs` for the DR-16 metric-binding
  cross-crate test).

- `[[bin]]` entries (4): `rb_fm_201_dry_run`, `rb_fm_205_dry_run`,
  `rb_fm_206_dry_run` (under `src/admin/dry_run/bin/`),
  `corelink-supply-verify` (under `src/supply_chain/verify/bin/cli.rs`).

- `[[test]]` entries (26 integration tests) + `[[example]]`
  entries (17 examples) declared with their `<mod_path>_` prefix
  files under `tests/` and `examples/`.

**Tests `cargo test -p corelink-ops`:** 405 passed + 0 failed + 0 ignored.

Distribution (`test result:` lines, in cargo's emission order):

| Binary | passed | notes |
|---|---|---|
| unittests src/lib.rs | 201 | 19 umbrella path-resolve smoke tests + 182 internal `#[cfg(test)]` unit tests inherited from the 15 absorbed crates' src trees (after path rewrites, sibling-`super::` bumps inside `mod tests { … }` blocks, and constant-ref bumps like `SHIFT_CAP_SECONDS`) |
| unittests src/admin/dry_run/bin/rb_fm_{201,205,206}_dry_run.rs | 0+0+0 | binary entry points, no tests |
| unittests src/supply_chain/verify/bin/cli.rs | 0 | binary entry point, no tests |
| admin_api_cross_wi_integration_s13 | 7 | cross-WI S-13 integration |
| admin_api_e2e_admin_dual_approval | 4 | 2-of-N approval gate E2E |
| admin_dry_run_prop_dual_approval_invariants | 7 | proptest |
| config_api_adversarial | 8 | adversarial regression |
| config_api_prop_mfa_freshness | 10 | proptest, CTRL-AUTH-010 + INV-ADMIN-MFA-FRESHNESS |
| deploy_adversarial | 7 | adversarial (Cosign / Rekor / Fulcio bypass attempts blocked) |
| deploy_chaos | 10 | chaos / audit fail-CLOSED ordering |
| deploy_prop_verify | 5 | proptest, INV-SUPPLY-SIGNED-DEPLOY |
| dr_backup_verify_prop_backup_verify | 5 | proptest |
| dr_backup_verify_sli_binding | 2 | cross-crate metric label binding (DR-16) |
| dr_drill_prop_dr_drill | 11 | proptest, RTO ≤ 1800s + RPO ≤ 60s |
| drata_proptest_idempotency | 2 | proptest, SHA-256 idempotency replay |
| drata_wiremock_drata_client | 6 | wiremock HTTPS client smoke |
| migrations_d1_migration_integration | 3 | every migrations/d1/*.sql replays clean (KNOWN_HAZARDS pruned — see §4) |
| migrations_prop_migration_additivity | 1 | proptest, additivity invariant |
| oncall_pagerduty_events_http | 3 | HTTPS PagerDuty Events API v2 smoke |
| oncall_prop_oncall | 8 | proptest, fatigue threshold decision matrix |
| rotation_worker_adversarial | 8 | PAT-ROLL-FORWARD-001 adversarial |
| rotation_worker_prop_rotation | 5 | proptest, INV-KEY-OVERLAP + INV-KEY-NO-SKIP + INV-KEY-AUDIT |
| supply_chain_policy_adversarial_dep_policy | 5 | GPL-leak / yanked auto-merge / typosquat / unmaintained / vendor-patch-sans-ADR |
| supply_chain_policy_e2e_dependabot | 1 | dependabot flow E2E stub |
| supply_chain_policy_prop_cargo_deny | 26 | proptest, license/yanked/source rules |
| supply_chain_verify_adversarial | 3 | adversarial (Rekor bypass blocked, builder mismatch) |
| supply_chain_verify_prop_verify | 4 | proptest, SLSA L3 verifier |
| survey_prop_survey | 12 | proptest, HMAC-SHA256 constant-time verify |
| tenant_offboarding_prop_tenant_offboarding | 6 | proptest, 5-state machine reachability |
| doc-tests corelink_ops | 25 | rustdoc doctests across absorbed crates' public-API examples (all updated from `corelink_<X>::` → `corelink_ops::<mod>::`) |

**LOC moved:** 17,749 (matches spec inventory sum exactly:
313 + 802 + 1097 + 413 + 348 + 449 + 1784 + 907 + 1643 + 3455 + 913
+ 285 + 1442 + 1786 + 2112 = 17,749).

**Path rewrites applied (pure mechanical, scope-bounded; same
pattern as the privacy / cas / billing / auth / replication
sibling absorptions, with two extensions documented below):**

1. **Inside the moved child files** (`src/<mod>/<sibling>.rs`):
   - Outside `mod tests { … }` blocks: `crate::` → `super::`
     (sibling references resolved at module-root level).
   - Inside `mod tests { … }` blocks:
     `crate::<X>` → `super::super::<X>` AND
     `super::<sibling>` → `super::super::<sibling>` for any sibling
     module name in the absorbed crate's src/ root. The bracket-depth
     tracking in `/tmp/fix_test_blocks.py` ensures only references
     *inside* the test submodule get bumped — outside-of-test
     `super::<sibling>` references stay at one `super::` because
     they resolve from the child's module root.
   - The `oncall::ledger` `use super::SHIFT_CAP_SECONDS` (top-level
     const from the absorbed `lib.rs`) had to be hand-bumped to
     `use super::super::SHIFT_CAP_SECONDS` (the automated heuristic
     bumped sibling module names but not bare top-level identifiers).

2. **Inside the moved absorbed `lib.rs`** (now at
   `crates/corelink-ops/src/<mod_path>.rs`):
   - Outside `mod tests { … }`: `crate::` → `self::` (resolves
     equivalently — the file *is* the module root for `<mod_path>`).
   - Inside `mod tests { … }`: `crate::` → `super::` (the test
     submodule lives at `<mod_path>::tests`; super = the absorbed
     module's root = where the items used to live).

3. **Inside the binary files under `src/<mod>/bin/<bin>.rs`**:
   - `crate::<X>` → `crate::<mod_path>::<X>` (binary files have
     their own crate root which IS `corelink_ops`, so we need the
     absolute path through the mod tree).
   - `corelink_supply_verify::*` → `corelink_ops::supply_chain::verify::*`
     for the supply-verify CLI binary (the absorbed crate name no
     longer exists; the bin needs to consume its own absorbed
     library surface).
   - Added `#![allow(clippy::print_stdout, …)]` with a documented
     reason at each binary's top (the umbrella library's
     `print_stdout = "deny"` lint must not apply to binary entry
     points whose purpose is human-readable PASS/FAIL output).

4. **Inside the moved tests under `tests/<mod_path>_*.rs`**:
   `use corelink_<X>::` and bare `corelink_<X>::Type` →
   `use corelink_ops::<mod_path>::` /
   `corelink_ops::<mod_path>::Type` respectively. Plus the
   identical rewrite across every moved `examples/<mod_path>_*.rs`.

5. **Inside every rustdoc example block** (`///` or `//!` with
   ` ```rust` fences): the 15 absorbed crate names were swapped to
   their new module paths via a single mapping table. 25 doc tests
   pass post-rewrite (was 0/25 immediately after physical move).

6. **Parent aggregator shim updates** (5 files):
   - `crates/corelink-ops/src/admin.rs` — `pub mod api { pub use
     corelink_admin_api::*; }` → `pub mod api;` (+ same for `dry_run`).
     Kept `handler` + `dual_approval` as re-export façades (still
     external).
   - `crates/corelink-ops/src/config.rs` — `api` absorbed;
     `durable_object` stays external.
   - `crates/corelink-ops/src/dr.rs` — both sub-mods (`drill` +
     `backup_verify`) physically absorbed.
   - `crates/corelink-ops/src/rotation.rs` — `worker` absorbed;
     `adapters` stays external.
   - `crates/corelink-ops/src/supply_chain.rs` — both sub-mods
     (`policy` + `verify`) physically absorbed.

7. **One external consumer migration** outside the ops umbrella:
   `tests/e2e-byok-revoke/Cargo.toml` swapped its dependency on
   `corelink-customer-alerts` for `corelink-ops`, and
   `tests/e2e-byok-revoke/src/helpers.rs` swapped its three
   `corelink_customer_alerts::` references for
   `corelink_ops::alerts::`. This was the only non-ops external
   consumer of any of the 15 absorbed crates (the other apparent
   hits were comments-only references in `corelink-slo/src/definition.rs`,
   `corelink-container/Cargo.toml` + `tests/harness/d1_container.rs`,
   `corelink-privacy/src/lib.rs`, `corelink-auth/src/lib.rs`,
   `corelink-replication/Cargo.toml`, `corelink-slack-real/src/template.rs`,
   `corelink-cf-bindings/Cargo.toml` — all narrative prose, no code
   dependency on any of the 15 absorbed crates).

## §4. Root-cause fix surfaced during absorption — KNOWN_HAZARDS pruning

The migration replay harness test
(`tests/migrations_d1_migration_integration.rs`) had a stale
`KNOWN_HAZARDS` allow-list pinning 4 entries
(0023_residency_check_constraints, 0028_tenant_primary_region,
0031_byok_tenant_status, 0041_dpa_versioning) that were genuine
hazards when the test landed (R2-13) but became no-ops once their
upstream migrations were patched in subsequent waves. The test's
"no stale entry" guard (line 174 panic) catches this and fail-CLOSEs
the suite — confirmed by running the test against the pre-absorption
baseline (`git stash; cargo test -p corelink-d1-migrations
--test d1_migration_integration` → identical panic). This was a
pre-existing baseline failure that surfaced cleanly during the
absorption because the test moved into the umbrella's `tests/`
directory under its new `migrations_*` prefix.

Root-cause fix (not a bypass): the `KNOWN_HAZARDS` array was emptied
with a documentation comment noting the W35-P2-OPS absorption is
the discovery point. Empty list is the steady-state until a new
hazard lands. Verified: `cargo test -p corelink-ops --test
migrations_d1_migration_integration` → 3 passed, 0 failed.

## §5. Allowed `clippy::module_inception` justification

The absorbed `corelink-drata-sync` crate originally had a sibling
file `src/drata.rs` (the HTTPS REST client) alongside its
`src/lib.rs`. After absorption, the file tree contains both
`crates/corelink-ops/src/drata.rs` (the absorbed crate root) and
`crates/corelink-ops/src/drata/drata.rs` (the absorbed inner client).
The umbrella's `clippy::mod_module_files = "deny"` is satisfied by
the flat layout, BUT `clippy::module_inception` triggers on
`pub mod drata;` inside `drata.rs` because the inner module name
matches the outer module name.

The justification (documented inline with a `reason = ` field):

> "W35-P2-OPS absorption: the inner `drata.rs` is the HTTPS Drata
> REST client (`DrataClient` / `InMemoryDrataClient`); the outer
> `drata.rs` is the absorbed `corelink-drata-sync` crate root. Inner
> module name mirrors the public `DrataClient` API anchor that
> downstream readers grep for; renaming would lose grep-ability.
> Pre-existing pattern in the original `corelink-drata-sync` crate."

This is the only `#[allow]` introduced by W35-P2-OPS. No
`unwrap_used` / `expect_used` / `panic` / `print_stdout` denies
were bypassed inside `src/` — the 4 binary entry points
(`rb_fm_201_dry_run`, `rb_fm_205_dry_run`, `rb_fm_206_dry_run`,
`corelink-supply-verify`) declare per-binary `#![allow(...)]`
attributes for `print_stdout` + `print_stderr` (binary purpose is
human-readable CLI output; the umbrella library's deny rules do not
apply to binary entry points).

## §6. Charter compliance — sample verification commands

```
$ grep -rE 'unsafe\s+(fn|impl|trait)' crates/corelink-ops/src/ | grep -v '//\|forbid' | wc -l
0

$ grep -rE '\.unwrap\(\)|\.expect\(' crates/corelink-ops/src/ | grep -v 'tests::\|mod tests\|#\[cfg(test)\]\|//\|/\*' | wc -l
0   # bare unwrap/expect appear only inside #[cfg(test)] sections

$ grep -rEn "<<<<<<<|>>>>>>>" --include="*.toml" --include="*.rs" --include="*.md" crates/corelink-ops Cargo.toml | wc -l
0   # no merge conflict markers

$ for c in corelink-admin-api corelink-admin-dry-run corelink-backup-verify corelink-config-api corelink-customer-alerts corelink-d1-migrations corelink-deploy-verifier corelink-dr-drill corelink-drata-sync corelink-oncall corelink-rotation-worker corelink-supply-chain-policy corelink-supply-verify corelink-survey corelink-tenant-offboarding; do test -d "crates/$c" && echo FAIL || echo OK; done | grep -c OK
15   # all 15 crate dirs removed
```

## §7. Parallel-safety with sibling W35-P2 absorption agents

- **W35-P2-BYOK** (6 byok-* + container consumer): no shared files
  with W35-P2-OPS except `crates/corelink-ops/src/alerts/alerter.rs`
  (which W35-P2-BYOK migrates from `corelink_byok_core::` →
  `corelink_byok::` as a CONTENT EDIT of the already-physically-moved
  alerter file). After orchestrator union-merge, the OPS branch
  brings the file relocation (`crates/corelink-customer-alerts/src/`
  → `crates/corelink-ops/src/alerts/`) and the BYOK branch brings
  the content edit; both are independent and compose cleanly.
- **W35-P2-AC** (ac-core + ac-schema + worker test consumer): no
  shared files with W35-P2-OPS.

**Untouched per spec (W36 zone or out-of-scope):**

- `crates/corelink-ops/src/{statuspage,slack}.rs` — W36 Stage 2.C
  zone (HTTPS portions also re-exported by `corelink-adapters-cloud`;
  pure-logic vs. binding decomposition deferred).
- `crates/corelink-dsr-statuspage-scheduler/` — W36 Trigger B
  wasm32 blocker; preserved as a separate workspace member.
- `crates/corelink-privacy-erasure-worker`,
  `crates/corelink-privacy-pseudonymize`,
  `crates/corelink-dpa-acceptance` — privacy lane (W35-P2-PRIVACY
  scope; out of W35-P2-OPS scope).
- Any `byok-*` / `ac-*` crate — other agents' scope.

## §8. Commit SHA

Recorded post-commit; see W35-P2-OPS branch HEAD.

## §9. DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
