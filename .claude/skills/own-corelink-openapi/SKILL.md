---
name: own-corelink-openapi
description: >-
  Maintain the static ownership record for corelink-openapi declarations; do not
  infer specification generation, publication, consumer reachability, or runtime.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-openapi"
  manifest: "tools/openapi/Cargo.toml"
  source-commit: "398e586ccef712477f2a4ce51e026443b67e5747"
  profile: "S"
  evidence-set: "corelink-openapi-source-static-20260921"
---

# Ownership — corelink-openapi

[Scope](#s01) · [Evidence](#s02) · [Surface](#s03) · [Axioms](#s04) · [Relations](#s05) · [Limits](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Scope and boundary

Own only this guide and `docs/ownership/crates/corelink-openapi/{REFERENCE,BLAST_RADIUS,MAINTENANCE}.md`. Inspect `tools/openapi/Cargo.toml` and `tools/openapi/src/lib.rs` as static evidence. Do not change the OpenAPI input documents, scripts, workflows, consumers, manifests, or source.

<a id="s02"></a>
## S02 — Evidence discipline

Class claims as SOURCE, DOCUMENTARY, or UNKNOWN. SOURCE is the pinned manifest and Rust text. DOCUMENTARY is the supplied S-profile checker and whitespace diff. Specification generation, publication, resolved consumers, feature/dependency resolution, compilation, test execution, parsing outcome, route reachability, network, deployment, and runtime are UNKNOWN.

<a id="s03"></a>
## S03 — Source contracts

Record separately the package declaration, `include_str!` input paths, `SPEC_VERSION`, `PACKAGE_VERSION`, `paths` constants and `ALL`, `parse_json`, and test declarations. An include macro is a declared source relation, not evidence that an artifact was generated, published, loaded, or consumed.

<a id="s04"></a>
## S04 — Five source axioms

Preserve the five falsifiable source axioms in [R05](../../../docs/ownership/crates/corelink-openapi/REFERENCE.md#r05): unsafe-code prohibition; YAML include path; JSON include path; exact `v1` major; and `paths::ALL` membership. Each needs an exact source location and a concrete edit that falsifies it.

<a id="s05"></a>
## S05 — Relation method

Write one directed static arrow per relation in [B01–B06](../../../docs/ownership/crates/corelink-openapi/BLAST_RADIUS.md#b01): manifest to crate declarations, source to each include path, constants to aggregate, parser to `serde_json`, and test text to source contracts. Do not infer resolved dependencies, external documents, consumers, execution, or reverse reachability.

<a id="s06"></a>
## S06 — Limits and route

The designated canonical OKF route is [Worker edge plane](../../../docs/knowledge/planes/worker-edge.md). It is a route only; do not copy, redefine, or revalidate it here. Stop when a task needs actual document generation, publication, a consumer, CI, parsing, route serving, compatibility, or runtime evidence; retain UNKNOWN and route to the owner with that evidence.

<a id="s07"></a>
## S07 — Completion and handoff

Success is bounded, falsifiable SOURCE documentation. Completeness requires S01–S07, [R01–R08](../../../docs/ownership/crates/corelink-openapi/REFERENCE.md#r01), [B01–B06](../../../docs/ownership/crates/corelink-openapi/BLAST_RADIUS.md#b01), and [M01–M06](../../../docs/ownership/crates/corelink-openapi/MAINTENANCE.md#m01). Run the S checker once per artifact and `git diff --check` against `398e586ccef712477f2a4ce51e026443b67e5747`; report their documentary results, the five axioms, and explicit unknowns. These checks are not approval, execution, or runtime evidence.
