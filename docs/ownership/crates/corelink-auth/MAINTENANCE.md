---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-auth
manifest: crates/corelink-auth/Cargo.toml
source_commit: b64025a5ef6295c1f23614339b6bb5a1e909f793
profile: H
state: draft
evidence_set: auth-source-static-20260920
---

# corelink-auth — maintenance

Source/static maintenance procedures for the hybrid authentication facade. They do not authorize Cargo execution, target builds, network access, deployment, database mutation, runtime probing, or independent-review claims.

[Baseline](#m01) · [Ownership](#m02) · [Features](#m03) · [Schema](#m04) · [WebAuthn](#m05) · [Consumers](#m06)

<a id="m01"></a>
## M01 — Fix the review baseline

**Mode:** STATIC_SOURCE. **Prerequisite:** requested revision and changed paths are known. **Predicate:** the manifest and tree identity match the review record. **Action:** record the SHA, `Cargo.toml`, `src/lib.rs`, and affected files. **Evidence:** revision output and path inventory. **Stop/recovery:** stop on an ambiguous/different baseline; recover by obtaining the intended source snapshot, not by substituting a newer checkout.

<a id="m02"></a>
## M02 — Classify facade versus inline changes

**Mode:** STATIC_SOURCE. **Prerequisite:** a public module or dependency changes. **Predicate:** every changed public path is classified as external re-export or local implementation. **Action:** inspect the four façade files and the `schema`/`webauthn` roots; name the external implementation owner for each façade. **Evidence:** `src/{clerk,clerk_cf,pat,tenant_path,schema,webauthn}.rs` and manifest dependencies. **Stop/recovery:** stop if a re-export is treated as a local implementation change; recover by tracing the defining crate separately and recording compatibility as unknown until then.

<a id="m03"></a>
## M03 — Review feature-gated surface changes

**Mode:** STATIC_SOURCE. **Prerequisite:** feature, cfg, middleware, or worker-session code changes. **Predicate:** forwarded feature and public cfg routes agree in source. **Action:** compare `clerk-jwt-adapter`, `tower-middleware`, and both gated modules with their targets. **Evidence:** `Cargo.toml:55-67`; `src/lib.rs:143-162`. **Stop/recovery:** stop where a conclusion needs resolved features or target output; recover with separately recorded build/graph evidence and do not infer it from a wasm-related comment.

<a id="m04"></a>
## M04 — Review inline schema changes

**Mode:** STATIC_SOURCE. **Prerequisite:** SQL inclusion, email hash, pseudonym, RLS guidance, or simulator changes. **Predicate:** the affected source relation has a cited before/after invariant. **Action:** trace `schema.rs` through the migration include and all affected helpers; compare hash normalization, pseudonym domain/width, and simulator errors/constraints. **Evidence:** `src/schema.rs`, `src/schema/{email_hash,pseudonymize,rls,sim}.rs`, and `migrations/002_auth_tables.sql`. **Stop/recovery:** stop if a claim needs live database/RLS/migration behavior; recover by escalating the exact missing database predicate.

<a id="m05"></a>
## M05 — Review inline WebAuthn changes

**Mode:** STATIC_SOURCE. **Prerequisite:** an AAGUID, challenge, credential, engine, origin, recovery, sign-count, step-up, or store surface changes. **Predicate:** inputs, output/error, and owning module are all identified. **Action:** trace the changed symbol through `webauthn.rs` exports and its local module; record changed TTL/type/trait predicates. **Evidence:** `src/webauthn.rs` and the affected `src/webauthn/*.rs`. **Stop/recovery:** stop if browser, authenticator, production-engine, store, or delivery behavior is required; recover only with authorized scoped evidence from its owner.

<a id="m06"></a>
## M06 — Reconcile consumers and hand off

**Mode:** STATIC_GRAPH. **Prerequisite:** a public contract, manifest edge, or feature changed. **Predicate:** direct consumers and unproven consumer categories are distinguished. **Action:** inspect manifests/imports for REAPI first; assess CLI, OpenAPI, adapters, worker, statuspage, and fuzz only with a concrete static edge. Record source paths, B01–B06 relations, and unknowns for handoff. **Evidence:** consumer manifests/source plus [Reference](REFERENCE.md#r07) and [Blast radius](BLAST_RADIUS.md#b06). **Stop/recovery:** stop when a complete/dynamic/reverse feature graph is needed; recover by obtaining a

separately recorded graph query or owner review. Do not certify runtime reachability or cold review.
