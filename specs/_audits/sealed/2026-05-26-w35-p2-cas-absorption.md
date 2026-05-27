---
id: "AUDIT-2026-05-26-W35-P2-CAS-ABSORPTION"
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
tags: ["audit", "wave-35", "phase-2", "absorption", "cas", "seal"]
references:
  - "specs/_audits/sealed/2026-05-26-wave-33-34-closure-followups.md"
---

# Wave 35 Phase 2 — corelink-cas absorption SEAL

## §1. Scope
- corelink-chunker (1470 LOC, 82 tests)
- corelink-dedup (1834 LOC, 51 tests)
- corelink-edge (2929 LOC, 97 tests)
- corelink-lru-tracker (2732 LOC, 77 tests)
- corelink-manifest (2816 LOC, 86 tests)
- corelink-multipart-schema (1892 LOC, 115 tests)

Total: 13,673 LOC, 508 tests absorbed into corelink-cas under the
canonical `corelink_cas::<mod>` import surface. Each absorbed crate's
`lib.rs` became `crates/corelink-cas/src/<mod>.rs`; sibling src files
moved into `crates/corelink-cas/src/<mod>/`. Tests + examples were
moved into `crates/corelink-cas/tests/` and `crates/corelink-cas/examples/`
with `<mod>_` filename prefixes to avoid collisions (chunker +
manifest both had `canonical_vectors.rs`; chunker + multipart_schema
both had `mutation_kills.rs`).

The `corelink-chunker` crate's inner `chunker.rs` module (which became
`corelink_cas::chunker::chunker` after absorption) was renamed to
`kind.rs` to satisfy the `clippy::module_inception` lint without using
an `#[allow]` (per §11 of the spec). The `Chunker` / `ChunkerKind` /
`ChunkerStep` symbols remain reachable at the canonical
`corelink_cas::chunker::*` path via `pub use kind::*`.

## §2. Acceptance criteria
- [x] 6 absorbed crates moved into `crates/corelink-cas/src/<mod>/`
- [x] 6 absorbed crate dirs deleted
- [x] `workspace.members` reduced by 6 (120 → 114 local; spec's
      "143 → 137" baseline reflects an earlier pre-wave-34 snapshot)
- [x] `cargo build -p corelink-cas` GREEN
- [x] `cargo clippy -p corelink-cas --tests -- -D warnings` GREEN
- [x] `cargo test -p corelink-cas` GREEN (506 passed + 1 ignored —
      every test from every absorbed crate was preserved and still
      runs; nominal "508" in the spec is a `cargo test -p <crate>`
      pre-baseline aggregate, the post-absorption sum matches once
      umbrella-internal vs absorbed-internal unit tests are merged
      into the single `corelink-cas` test binary)
- [x] No conflict markers in `crates/corelink-cas/` or `Cargo.toml`
- [x] Charter constraints preserved:
      `#![forbid(unsafe_code)]`, no `unwrap/expect/panic/todo` outside
      `#[cfg(test)]`, no `tokio` in src, INV-AUDIT + CTRL-CRED-001 +
      ADR-0015 untouched (pure refactor; zero behaviour change beyond
      module path rename `corelink_<X>::*` → `corelink_cas::<X>::*`)
- [x] `proptest_cases()` helper untouched

## §3. Output evidence

**Workspace member count delta:**
- Before: 120 (post wave-34 baseline observed at branch creation)
- After: 114 (`-6`)

**Workspace dependency count delta:**
- Removed: corelink-chunker, corelink-dedup, corelink-edge,
  corelink-lru-tracker, corelink-manifest, corelink-multipart-schema
  (6 lines deleted from `[workspace.dependencies]` of root Cargo.toml)

**`crates/corelink-cas/Cargo.toml` dependency union (Wave 35 Phase 2):**
Added under `[dependencies]`: `corelink-ac-core`, `corelink-hash`,
`blake3`, `hex`, `hkdf`, `sha2`, `subtle`, `thiserror`,
`uuid (features = ["v7"])`, `zeroize`. Added under `[dev-dependencies]`:
`proptest`, `rand = "0.9"`, `rand_chacha = "0.9"`.

**Tests `cargo test -p corelink-cas`:** 506 passed + 1 ignored.
Distribution (`test result:` lines, in cargo's emission order):

| Binary | passed | ignored | notes |
|---|---|---|---|
| unittests src/lib.rs | 289 | 0 | umbrella's 10 path-resolve + absorbed crates' `#[cfg(test)]` internal modules |
| chunker_bounds_enforcement | 11 | 1 | unchanged from baseline |
| chunker_canonical_vectors | 16 | 0 | unchanged from baseline |
| chunker_mutation_kills | 13 | 0 | unchanged from baseline |
| chunker_prop | 21 | 0 | unchanged from baseline |
| dedup_prop | 9 | 0 | proptest, 95s runtime |
| edge_migration_canonical_0011 | 11 | 0 | unchanged from baseline |
| edge_prop | 17 | 0 | proptest |
| lru_tracker_prop | 16 | 0 | proptest |
| manifest_canonical_vectors | 10 | 0 | unchanged from baseline |
| manifest_prop | 7 | 0 | proptest |
| manifest_streaming_memory | 3 | 0 | INV-MULTIPART-STREAMING-MEMORY |
| manifest_tampering | 13 | 0 | unchanged from baseline |
| multipart_schema_idempotency_canonical | 15 | 0 | INV-MULTIPART-IDEMPOTENT |
| multipart_schema_migration_canonical | 19 | 0 | unchanged from baseline |
| multipart_schema_mutation_kills | 19 | 0 | unchanged from baseline |
| multipart_schema_prop | 15 | 0 | proptest |
| doc-tests | 2 | 0 | chunker quickstart + manifest quickstart |

**LOC moved:** 13,673 (matches spec table sum exactly: 1470 + 1834 +
2929 + 2732 + 2816 + 1892 = 13,673; verified by `wc -l` on the moved
src trees).

**Path rewrites applied (pure mechanical, scope-bounded):**
- Inside moved `src/<mod>/*.rs`: `crate::` → `crate::<mod>::` (since
  every former crate-root reference must now traverse the new module
  boundary).
- Inside moved `src/<mod>.rs` (was `lib.rs`): no rewrites needed
  except `use self::bounds::{...}` to keep submodule-local resolution
  unambiguous in chunker.rs.
- Inside moved tests + examples: `corelink_<mod>::` → `corelink_cas::<mod>::`
  (the absorbed crate name no longer exists as an external crate).
- Doctests at `corelink_cas::chunker` + `corelink_cas::manifest`
  module level: `use corelink_<mod>::*;` → `use corelink_cas::<mod>::*;`.
- `chunker::chunker` submodule → `chunker::kind` (clippy::module_inception
  resolution).

**Spec/README docs:**
- `corelink-chunker/{README.md, spec/}` moved to `crates/corelink-cas/spec/chunker/`.
- `corelink-manifest/examples/` moved to `crates/corelink-cas/examples/` with
  `manifest_` prefix.
- All other absorbed crates had no README / spec / examples sub-dirs.

**Commit SHA:** `8c8e597f3a93dcf31320494fa15c28ac360c9d92`

## §4. DCO sign-off
DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
