---
name: own-sbom-publish
description: Maintain manifest-scoped ownership records for sbom-publish without representing its declared targets, dependencies, supply-chain text, or external interactions as executed behavior.
metadata:
  schema: "corelink-ownership/1.1"
  package: "sbom-publish"
  manifest: "tools/sbom-publish/Cargo.toml"
  source-commit: "398e586ccef712477f2a4ce51e026443b67e5747"
  evidence-set: "w013-sbom-publish-source-static-398e586c"
---

# Own sbom-publish

This S-profile guide owns static declarations and source-bounded implementation
observations for the pinned package tree, together with its three companion
records. It does not establish target resolution, build, test results, command
execution, external service success, publication, release, or runtime behavior.

[Scope](#s01) · [Evidence](#s02) · [Inventory](#s03) · [Axioms](#s04) · [Relations](#s05) · [Validation](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Scope and authority

Own only this guide and `docs/ownership/crates/sbom-publish/{REFERENCE,BLAST_RADIUS,MAINTENANCE}.md`. SOURCE review is limited to `tools/sbom-publish/Cargo.toml`, `src/*.rs`, `tests/*.rs`, and `examples/*.rs` at the pinned package commit. Do not inspect or claim reverse consumers, deployment wiring, external provider state, or runtime behavior.

<a id="s02"></a>
## S02 — Evidence discipline

Class each statement as SOURCE, DOCUMENTARY, or UNKNOWN. SOURCE is a manifest declaration or a directly visible source construct/call/data-flow edge in the bounded package files. DOCUMENTARY is the controlled checker and whitespace-diff result. Cargo resolution, compilation, test/example results, actual I/O, remote interactions, release effects, deployment, and independent review are UNKNOWN. A source path or assertion is not a result.

<a id="s03"></a>
## S03 — Manifest and source inventory

Record package/workspace inheritance, lint inheritance, each named bin/lib/test/example target, normal dependency, and dev-dependency as separate declarations. Also map the library API, CLI subcommands, publisher sequence, NTIA/PURL/TSA/DT modules, metrics/errors, declared tests, and examples to their source files. The package description and source comments are claims/text; implementation details may be stated only when directly visible in code, and neither form proves successful execution.

<a id="s04"></a>
## S04 — Five falsifiable manifest axioms

Preserve [R05](../../../docs/ownership/crates/sbom-publish/REFERENCE.md#r05)'s five falsifiable axioms: package identity; workspace-inherited metadata; workspace lints; named binary/library paths; and separated normal/dev dependency plus test/example declarations. A changed manifest line is a concrete falsifier. No axiom proves any declared path is used; source observations are covered separately in R07 and B07.

<a id="s05"></a>
## S05 — Atomic relation method

Write one directed SOURCE arrow for one manifest declaration at a time in [B03](../../../docs/ownership/crates/sbom-publish/BLAST_RADIUS.md#b03). Keep target, normal dependency, dev dependency, test, and example arrows distinct. Record direct source call/data-flow paths separately in [B07](../../../docs/ownership/crates/sbom-publish/BLAST_RADIUS.md#b07), naming both endpoints and files. Stop when an inference needs a resolved graph, dynamic activation, remote response, or environment observation.

<a id="s06"></a>
## S06 — Validation and canonical route

Completeness requires S01–S07, R01–R08, B01–B07, and M01–M06 with atomic predicates and relations. The verified canonical OKF route is [ADR-S12-001](../../../docs/knowledge/adr/adr-s12-001-sbom-cyclonedx-ntia-tsa-dt.md); route to it only. Do not copy, redefine, or revalidate its policy.

Run the controlled profile-S checker once per artifact and the baseline whitespace
diff only. Its result is structural, not semantic approval or execution evidence.

<a id="s07"></a>
## S07 — Definition of done and handoff

Change only the four owned artifacts. Handoff names the baseline, paths, R/B/M
identifiers, four checker results, diff result, canonical route, and unknowns.
Do not describe an author check as Cargo, test, upload, publication, release,
provider, runtime, or independent-review evidence.
