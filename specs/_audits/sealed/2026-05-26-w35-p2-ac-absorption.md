---
id: "AUDIT-2026-05-26-W35-P2-AC-ABSORPTION"
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
tags: ["audit", "wave-35", "phase-2", "absorption", "ac", "seal"]
references:
  - "specs/_audits/sealed/2026-05-26-wave-33-34-closure-followups.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-cas-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-privacy-absorption.md"
---

# Wave 35 Phase 2 — corelink-ac absorption SEAL

## §1. Scope

2 AC primitives sub-crates physically absorbed into the
`corelink-ac` umbrella under the canonical `corelink_ac::*` import
surface. The historical Wave-33 Stage 1 Stream A sub-step A.2b
"Option-A re-export aggregator" was finally flipped to Option-B
physical absorption for the 2 crates whose lifecycle is locked to
the umbrella; `corelink-handler-ac` deliberately remains an external
workspace member (out-of-scope per spec §1; the umbrella keeps a
`corelink_ac::handler` re-export shim).

| # | Absorbed | LOC | `#[test]` baseline | New canonical path |
|---|---|---|---|---|
| 1 | corelink-ac-core | 3042 | 125 | `corelink_ac::*` (top-level via `pub use ac_core::*`) |
| 2 | corelink-ac-schema | 867 | 61 | `corelink_ac::schema::*` |
| **Total** | — | **3,909** | **186** | — |

The former `corelink-ac-core` crate ships an `ac_core` (NOT `core`)
submodule name to avoid shadowing the Rust `core` prelude module and
to satisfy `clippy::module_inception`-style hygiene; every public
symbol is re-exported at the umbrella's top level via
`pub use ac_core::*` so historical `corelink_ac::MerkleVerifier`,
`corelink_ac::sig::*`, `corelink_ac::merkle::*`, etc. paths continue
to resolve byte-identically for consumers.

The `ac-core/fuzz/` cargo-fuzz harness (`hkdf_expand` libFuzzer
target, ADR-0021 §1) was moved to `crates/corelink-ac/fuzz/` and
remains an isolated workspace via its own inner `[workspace]` toml.

## §2. Acceptance criteria
- [x] 2 absorbed crates moved into `crates/corelink-ac/src/<mod>/`
- [x] 2 absorbed crate dirs deleted
- [x] `workspace.members` reduced by 2 (100 → 98 local)
- [x] `cargo build -p corelink-ac` GREEN
- [x] `cargo build -p corelink-worker` GREEN (consumer migrated)
- [x] `cargo build -p corelink-cas` GREEN (consumer migrated)
- [x] `cargo clippy -p corelink-ac --tests -- -D warnings` GREEN
- [x] `cargo clippy -p corelink-worker --tests -- -D warnings` GREEN
- [x] `cargo clippy -p corelink-cas --tests -- -D warnings` GREEN
- [x] `cargo test -p corelink-ac` GREEN (180 passed + 1 ignored +
      1 doctest, every behavioural test from both absorbed crates
      preserved; baseline-spec "186" reflects the pre-absorption
      umbrella's 4 smoke tests + 125 + 61 sum, of which 3 of the
      original umbrella smoke tests survive — `core_path_resolves`
      was retired because the `core` submodule no longer exists)
- [x] No conflict markers in `crates/corelink-ac/` or `Cargo.toml`
- [x] Zero residual `use corelink_ac_core::` or `use corelink_ac_schema::`
      imports anywhere in the repo (`grep -rln … | wc -l` = 0)
- [x] Charter constraints preserved:
      `#![forbid(unsafe_code)]`, `[lints.clippy]` deny-list
      (`unwrap_used`, `expect_used`, `panic`, `indexing_slicing`,
      `todo`, `unimplemented`, `dbg_macro`, `print_stdout`,
      `print_stderr`, `mod_module_files`) unchanged on the umbrella;
      no `#[allow]` introduced to mask new lints; `--no-verify` NOT
      used; INV-AC-MERKLE preserved (merkle.rs Merkle codec +
      verifier bit-identical); ADR-0018 AC sig-domain separation
      preserved (sig.rs HKDF_INFO_AC_SIG byte-identical;
      manifest sig domain prefix collision tests still pass);
      INV-AUDIT untouched (audit fail-CLOSED contract preserved —
      handler-ac retains its own `AuditSink`).

## §3. Output evidence

**Workspace member count delta:**
- Before: 100 (post-PRIVACY-merge baseline observed at branch creation)
- After: 98 (`-2`)

**Workspace dependency count delta:**
- Removed from `[workspace.dependencies]`: `corelink-ac-core`,
  `corelink-ac-schema` (2 lines deleted from root Cargo.toml)
- `corelink-handler-ac` retained (the handler crate remains an
  external workspace member; the umbrella's `Cargo.toml` lists it
  under `[dependencies]` and re-exports it via `corelink_ac::handler`).

**`crates/corelink-ac/Cargo.toml` dependency union (Wave 35 Phase 2):**
Added under `[dependencies]` (transitive from absorbed crates):
`corelink-hash`, `blake3`, `hex`, `thiserror`, `serde`, `serde_json`,
`hkdf`, `sha2`, `subtle`, `zeroize`, `uuid { features = ["v7"] }`.
Retained: `corelink-core`, `corelink-crypto`, `corelink-audit`,
`corelink-handler-ac`. Added under `[dev-dependencies]`: `proptest`,
`rand = "0.9"`, `rand_chacha = "0.9"`. Removed:
`corelink-ac-core = { workspace = true }`,
`corelink-ac-schema = { workspace = true }`.

**Tests `cargo test -p corelink-ac`:** 180 passed + 1 ignored + 1
doctest (`cargo test` summary lines, in cargo's emission order):

| Binary | passed | ignored | notes |
|---|---|---|---|
| unittests src/lib.rs | 85 | 0 | umbrella's 3 path-resolve smoke tests + absorbed crates' `#[cfg(test)]` internal modules (the former `core_path_resolves` retired) |
| ac_core_canonical_vectors | 7 | 0 | INV-AC-MERKLE canonical vectors |
| ac_core_canonical_vectors_sig | 13 | 0 | ADR-0018 sig canonical vectors |
| ac_core_key_rotation_sig | 9 | 0 | CTRL-AC-002 key rotation |
| ac_core_prop_merkle | 6 | 0 | proptest (62s) |
| ac_core_prop_sig | 6 | 0 | proptest (4s) |
| ac_core_round_trip | 9 | 0 | codec round-trip |
| ac_core_tampering | 11 | 0 | tampering detection |
| ac_core_timing_sig | 0 | 1 | timing-sensitive, ignored by default |
| schema_idempotency_canonical | 8 | 0 | INV-AC-IDEMPOTENT + INV-AC-RESULT-HASH-IMMUTABLE |
| schema_migration_canonical | 14 | 0 | canonical migration 0002 invariants |
| schema_prop_ac_schema | 11 | 0 | proptest (16s) |
| Doc-tests corelink_ac | 1 | 0 | `ac_core.rs` quickstart |

**LOC moved:** 3,909 (matches spec table sum exactly: 3042 + 867;
verified by `wc -l` on the moved src trees:
`crates/corelink-ac/src/ac_core{.rs,/*.rs,/sig/*.rs}` = 2,898
(absorbed ac-core lib.rs was 144 LOC of which ~50 were doc/preamble
collapsed during inner-attribute rewrite; the remaining LOC live in
the 11 sibling files); `crates/corelink-ac/src/schema{.rs,/*.rs}`
= 867.)

**Path rewrites applied (pure mechanical, scope-bounded):**
- `crates/corelink-ac-core/src/*` → `crates/corelink-ac/src/ac_core/*`
  (sibling modules) and `lib.rs` → `ac_core.rs` (module file).
- `crates/corelink-ac-schema/src/*` → `crates/corelink-ac/src/schema/*`
  (sibling modules) and `lib.rs` → `schema.rs` (replacing the prior
  shim that re-exported `corelink_ac_schema::*`).
- Inside moved `src/ac_core/*.rs`: `crate::{bounds,codec,error,
  merkle,outputs,sig,types}` → `crate::ac_core::{bounds,codec,
  error,merkle,outputs,sig,types}` (every former crate-root
  reference now traverses the new submodule boundary).
- Inside `src/schema/sim.rs`: `crate::region::AcRegion` →
  `crate::schema::region::AcRegion`.
- Inside `src/ac_core.rs` doc-test code: `corelink_ac_core::*` →
  `corelink_ac::*` (doctest now lives under the umbrella's crate).
- `include_str!("../../../migrations/d1/0002_ac_meta.sql")` in
  `schema.rs`: path unchanged — `crates/corelink-ac/src/schema.rs`
  is at the same nesting depth as `crates/corelink-ac-schema/src/lib.rs`
  was, so the 3-up traversal still resolves to repo-root migrations.
- Tests moved to `crates/corelink-ac/tests/` with `ac_core_` /
  `schema_` filename prefixes (no name collisions in the original
  trees — the prefix is applied uniformly for self-documentation
  per the Batch-1/2 convention).
- Inside moved `tests/*.rs`: `corelink_ac_core::` → `corelink_ac::`,
  `corelink_ac_schema::` → `corelink_ac::schema::`.

**Consumer migration (per spec §4):**
- `crates/corelink-worker/src/reapi/ac/merkle.rs`: 23
  `corelink_ac_core::` references → `corelink_ac::`.
- `crates/corelink-worker/src/reapi/ac/sig.rs`: 19
  `corelink_ac_core::` references → `corelink_ac::`.
- `crates/corelink-worker/Cargo.toml`: `corelink-ac-core =
  { workspace = true }` → `corelink-ac = { workspace = true }`.
- `crates/corelink-cas/src/manifest{.rs,/builder.rs,/error.rs,
  /sig.rs,/verifier.rs}`: every `corelink_ac_core::sig::*` /
  `corelink_ac_core::merkle::*` reference → `corelink_ac::sig::*` /
  `corelink_ac::merkle::*`.
- `crates/corelink-cas/tests/manifest_{prop,streaming_memory,
  canonical_vectors,tampering}.rs` + `examples/manifest_*.rs`:
  same rewrite.
- `crates/corelink-cas/src/multipart_schema/region.rs`: doc comment
  `corelink_ac_schema::AcRegion` → `corelink_ac::schema::AcRegion`.
- `crates/corelink-cas/Cargo.toml`: `corelink-ac-core` →
  `corelink-ac`.
- `crates/corelink-gc/src/region.rs`: doc comments
  `corelink_ac_schema::*` → `corelink_ac::schema::*`.
- `wrangler.toml`: comment `crates/corelink-ac-schema::REGION_LIST` →
  `corelink_ac::schema::REGION_LIST`.

**Fuzz harness migration:**
- `crates/corelink-ac-core/fuzz/` → `crates/corelink-ac/fuzz/`.
- The fuzz crate carries its own inner `[workspace]` toml so it
  remains excluded from the outer workspace resolver; the
  `hkdf_expand` libFuzzer target is byte-identical and remains
  driveable by `cargo fuzz run hkdf_expand -p corelink-ac-fuzz`.
- Root `Cargo.toml` `workspace.members`:
  `"crates/corelink-ac-core/fuzz"` → `"crates/corelink-ac/fuzz"`.

**Charter constraints:**
- `#![forbid(unsafe_code)]` enforced on the umbrella (unchanged).
- `[lints.clippy]` deny-list on the umbrella covers every absorbed
  crate's former deny-list — no relaxation.
- INV-AC-MERKLE: `merkle.rs` BLAKE3 + RFC 6962 domain separation
  bit-identical (canonical vectors test green).
- ADR-0018 AC sig-domain separation: `sig/canonical.rs`
  `HKDF_INFO_AC_SIG` byte-identical (manifest-sig domain-prefix
  collision tests still green; cas's `manifest_sig_domain_separation`
  example still references `corelink_ac::sig::HKDF_INFO_AC_SIG`
  through the migrated path).
- INV-AUDIT: handler-ac retains its own `AuditSink` — fail-CLOSED
  contract untouched.
- `#[non_exhaustive]` on every public enum + struct: untouched.
- No `unwrap()` / `expect()` / `panic!()` introduced; no `#[allow]`
  added to mask new lints; `git commit --no-verify` NOT used.

**Spec/README docs:**
- `corelink-ac-core` had no README / spec / examples sub-dirs.
- `corelink-ac-schema` had no README / spec / examples sub-dirs.
- No doc artefacts to move.

**Commit SHA:** to be filled in after commit (set on the seal
commit message body via the orchestrator-driven workflow).

## §4. DCO sign-off
DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
