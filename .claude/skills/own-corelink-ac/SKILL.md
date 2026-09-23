---
name: own-corelink-ac
description: Own the static public-contract and change-boundary review for the corelink-ac umbrella crate.
metadata:
  evidence-set: ac-source-static-20260920
  manifest: crates/corelink-ac/Cargo.toml
  profile: S
  package: corelink-ac
  source-commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
---

# corelink-ac ownership guide

Use this guide for static, source-backed changes to the AC umbrella. It does not establish runtime behavior, deployment safety, test execution, or independent review.

[Scope](#s01) · [Surface](#s02) · [Merkle](#s03) · [Signature](#s04) · [Schema](#s05) · [Consumers](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Establish the boundary

Condition → changing `crates/corelink-ac` or documenting its ownership. Action → read its manifest and `src/lib.rs` at the selected commit. Evidence → package, dependency, test-target, and re-export declarations. Stop → SHA, manifest, or source tree differs; do not infer an absorbed crate or external implementation from names.

<a id="s02"></a>
## S02 — Preserve the public surface

Condition → an export, module, feature, or dependency changes. Action → trace `pub use ac_core::*`, `schema`, `handler`, and the `BlobMetaReader`/`OutputsValidator`/`StrictOutputsValidator` re-exports in `src/lib.rs` and `ac_core.rs`. Evidence → compile-time source declarations and affected public symbols. Stop → a caller needs a path not present in source; record it as unknown instead of asserting compatibility.

<a id="s03"></a>
## S03 — Guard Merkle and codec contracts

Condition → changing output digests, roots, bounds, JSON envelope, errors, or output aliveness. Action → compare `bounds.rs`, `merkle.rs`, `codec.rs`, `types.rs`, `outputs.rs`, and their named test targets. Evidence → `ActionResult` digest iteration, tenant-scoped batched reader contract, one-flag-per-digest predicate, and fail-closed errors in addition to constants and algorithms. Stop → any byte format, limit, reader behavior, or caller mapping lacks a source trace; escalate as a compatibility question.

<a id="s04"></a>
## S04 — Guard signature and rotation contracts

Condition → changing canonical preimages, TDK access, key IDs, signing, verification, or timing tests. Action → inspect `sig/{canonical,error,hkdf_signer,tdk}.rs` and signature tests. Evidence → preimage length/layout, reserved-ID handling, accepted-key policy interfaces, and test declarations. Stop → production key custody, rotation policy, or timing result is required; those facts are not established by this crate’s static source.

<a id="s05"></a>
## S05 — Guard schema invariants

Condition → changing regions, row shape, upsert behavior, TTL, migration text, or simulator behavior. Action → trace `schema.rs`, its `include_str!` route to `migrations/d1/0002_ac_meta.sql`, `schema/region.rs`, `schema/sim.rs`, and schema integration tests. Evidence → embedded SQL, composite key, inline checks, indexes, enum/list, validation predicates, and outcome enum. Stop → deployment order or live data compatibility is required; those remain outside static-source authority.

<a id="s06"></a>
## S06 — Assess consumers and graph impact

Condition → changing an API, dependency, wire bytes, or invariant. Action → inspect manifests and source references for CAS, worker, CF bindings, and `corelink-ac/fuzz`. Evidence → direct manifest edges for CAS and worker; CF bindings depends on worker; fuzz has an independent `hkdf`/`sha2` harness manifest. Stop → dynamic, feature-resolved, or undiscovered consumers remain unknown without a separately captured graph query.

<a id="s07"></a>
## S07 — Hand off safely

Condition → static review is complete. Action → report source commit, changed contracts, affected static relations, unknowns, and unexecuted gates. Evidence → the four ownership artifacts plus file paths and exact source locations. Stop → do not mark runtime, deployment, migration recovery, or cold review as proven; request the responsible owner and evidence when those decisions are needed.
