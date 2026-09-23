---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-ac
manifest: crates/corelink-ac/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: S
state: draft
evidence_set: ac-source-static-20260920
---

# corelink-ac — maintenance

Procedures are source/static-graph review modes. They are not instructions to deploy, touch data, use secrets, or claim test/cold-review evidence.

[Baseline](#m01) · [Surface](#m02) · [Merkle](#m03) · [Signature](#m04) · [Schema](#m05) · [Consumers](#m06).

<a id="m01"></a>
## M01 — Confirm the review baseline

Mode: STATIC_SOURCE. Predicate: a requested change names `corelink-ac` and its commit is known. Procedure: compare the requested SHA, `crates/corelink-ac/Cargo.toml`, and `src/lib.rs`; record exact paths. Stop: SHA or tree identity is unavailable or differs. Recovery: acquire a reconciled source snapshot; do not substitute a newer checkout. Evidence: SHA plus inspected path list.

<a id="m02"></a>
## M02 — Classify public-surface changes

Mode: STATIC_SOURCE. Predicate: exports, modules, dependencies, or error types change. Procedure: trace root re-exports into `ac_core`, `schema`, and the handler shim; include public `BlobMetaReader`, `OutputsValidator`, and `StrictOutputsValidator`; list added, removed, or moved paths. Stop: a compatibility claim depends on an untraced consumer. Recovery: mark compatibility unknown and request the consumer owner. Evidence: source declarations and consumer references.

<a id="m03"></a>
## M03 — Review Merkle, codec, or output-aliveness changes

Mode: STATIC_SOURCE. Predicate: digest ordering, roots, bounds, envelopes, Merkle errors, or output aliveness changes. Procedure: compare `bounds.rs`, `merkle.rs`, `codec.rs`, `types.rs`, and `outputs.rs`; trace `ActionResult` digests through the tenant-scoped batch reader and strict validator.

Stop: any reader error, false flag, or flag-count mismatch can be ignored, or a byte/bound change has no consumer decision. Recovery: preserve fail-closed `BackendError`/`BlobMissing` behavior, identify the caller, and hold at contract review. Evidence: output-reader/validator declarations, error paths, constants, and named test targets.

<a id="m04"></a>
## M04 — Review signature or rotation changes

Mode: STATIC_SOURCE. Predicate: preimage, TDK seam, signature size, key ID, accepted keys, or verifier behavior changes. Procedure: compare all files under `src/ac_core/sig` and signature integration-test declarations. Stop: the decision requires production keys, active rotation window, or measured timing. Recovery: route those facts to the key/operations owner without representing static source as proof. Evidence: layout/constants/error paths and cited unknown owner.

<a id="m05"></a>
## M05 — Review schema-simulation changes

Mode: STATIC_SOURCE. Predicate: region list, upsert result, constraints, TTL, migration SQL, or simulator changes. Procedure: read `schema.rs`, confirm its `include_str!` route to `migrations/d1/0002_ac_meta.sql`, then review the composite `(tenant_id, action_digest)` key, inline checks, three declared indexes, region/simulator source, and schema tests.

Stop: durable D1 compatibility, deploy order, or recovery is required but evidence is absent. Recovery: escalate with the exact SQL invariant and affected rows unknown; do not treat embedded text as execution proof. Evidence: include route, SQL clauses, enum/validation/outcome declarations, and test names.

<a id="m06"></a>
## M06 — Reconcile static consumers and hand off

Mode: STATIC_GRAPH. Predicate: a public contract or manifest dependency changed. Procedure: inspect direct CAS/worker manifests and source use, CF bindings’ worker edge, and the independent fuzz manifest; state direct versus transitive versus independent. Stop: feature-resolved, dynamic, or additional consumers are needed. Recovery: obtain a separately recorded graph query/reviewer result; do not call the static list exhaustive. Evidence: manifests, source references, unknown-consumer statement, and review handoff.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Start](#m01).
