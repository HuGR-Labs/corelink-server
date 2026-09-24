---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-dpa-acceptance
manifest: crates/corelink-dpa-acceptance/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-dpa-acceptance-structural-normalization-20260921
---

# corelink-dpa-acceptance — maintenance

All procedures are source/static review modes. They authorize neither Cargo execution nor operational access to keys, legal content, stores, email, routes, or traffic.

[Baseline](#m01) · [Schema](#m02) · [Locale/replay](#m03) · [Receipt](#m04) · [Effects](#m05) · [Consumers](#m06).

<a id="m01"></a>
## M01 — Establish the static baseline

Mode: STATIC_SOURCE. Prerequisite: requested package and commit are identified. Predicate: manifest and `src/lib.rs` match the recorded package boundary. Procedure: compare baseline, manifest, and listed modules. Stop: tree identity differs or a claim needs execution. Recovery: obtain the reconciled snapshot and record the mismatch. Evidence: SHA and paths read.

<a id="m02"></a>
## M02 — Review proof/schema changes

Mode: STATIC_SOURCE. Prerequisite: API-001 target is known. Predicate: field, enum, request, receipt, or error change is traceable from `schema.rs` through `service.rs`. Procedure: compare six proof fields, server-stamped timestamp, locale/jurisdiction variants, and re-exports. Stop: legal or client wire compatibility is unknown. Recovery: request the relevant consumer/legal owner’s decision. Evidence: source declarations and REL-001–004.

<a id="m03"></a>
## M03 — Review locale, hash, and idempotency changes

Mode: STATIC_SOURCE. Prerequisite: affected predicate and negative case are named. Predicate: unequal locale rejects before store; hash mismatch rejects before store; matching replay preserves identity while divergent reuse errors. Procedure: read `locale.rs`, comparison/order in `service.rs`, and `store.rs`; identify named locale/idempotency/replay targets without executing them. Stop: a conclusion requires legal text, D1 uniqueness, or live request semantics. Recovery: retain unknown and route it to the content/storage owner. Evidence: INV-001–003 and source paths.

<a id="m04"></a>
## M04 — Review receipt cryptography changes

Mode: STATIC_SOURCE. Prerequisite: algorithm, claim, key wrapper, or `kid` change is explicit. Predicate: signing and verification remain RS256 with the documented optional exact-`kid` branch. Procedure: trace `JwtReceiptClaims`, `sign_receipt`, `verify_receipt`, and service construction. Stop: key material, rotation, expiry policy acceptance, or receipt corpus compatibility is required. Recovery: escalate to key/receipt owner; do not substitute test fixtures for configured keys. Evidence: `jwt.rs`, `service.rs`, REL-007, INV-004.

<a id="m05"></a>
## M05 — Review effect ordering and failure boundary

Mode: STATIC_SOURCE. Prerequisite: sink/store behavior or order changes. Predicate: accepted audit is emitted before insert; notification remains after insert; failure taxonomy remains non-exhaustive. Procedure: trace all `?` branches in `service.rs` and trait signatures in `audit.rs`, `store.rs`, and `notify.rs`. Stop: delivery, durability, rollback, or remote ordering is asserted. Recovery: identify the adapter/operator as unknown. Evidence: INV-005/006, REL-005/006, source branch locations.

<a id="m06"></a>
## M06 — Reconcile consumers and hand off

Mode: STATIC_GRAPH. Prerequisite: public API or manifest relation changed. Predicate: privacy, container, and e2e-signup-flow static direct consumers are rechecked against the changed surface. Procedure: distinguish re-export, source reference, and execution; report unresolved consumers/features. Stop: exhaustive graph, mounted route, or traffic evidence is requested. Recovery: obtain separately recorded resolved/runtime evidence. Evidence: manifest/source references, documentary checker result, `git diff --check`, final SHA, and explicit unknowns. No author check is cold review.
