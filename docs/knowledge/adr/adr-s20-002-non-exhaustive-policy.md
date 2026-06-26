---
type: "ADR"
title: "ADR-S20-002 — non_exhaustive MUST/MAY-OMIT policy"
description: "Refines charter rule L2.1 from a blanket #[non_exhaustive] mandate into a MUST/MAY-OMIT policy keyed on cross-crate API reachability and type shape, with no mass migration."
source_files:
  - "specs/03_architecture/adrs/ADR-S20-002-NON-EXHAUSTIVE-POLICY.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "crates", "api-stability", "non-exhaustive", "charter", "semver", "s20"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S20-002 — non_exhaustive MUST/MAY-OMIT policy

Charter rule L2.1 mandated `#[non_exhaustive]` on public enums/structs, but a post-W36 audit flagged 1,307 declarations and a blanket sweep would be wrong: most are not actually on the cross-crate surface, some enums are deliberately closed (spec-frozen taxonomies, RFC-closed sets), and newtypes/markers gain nothing. This ADR replaces the universal rule with a precise MUST/MAY-OMIT policy keyed on real cross-crate reachability and type shape, and explicitly forbids a retroactive mass migration. It exists so future charter audits enforce the right subset instead of generating 1,307 false positives.

# Context

The post-W36 charter-strict audit scanned 5,412 `pub enum`/`pub struct` declarations and flagged 1,307 missing the annotation, but a blanket add would be wrong on four counts — visibility carries from the `pub use` re-export point not the declaration, some enums are deliberately closed (the spec-frozen 12-arm consent taxonomy, the RFC-closed HTTP method set), single-primitive newtypes cannot grow public fields meaningfully, and marker/phantom structs have nothing to extend — and the audit itself classifies *adding* the attribute as HIGH-RISK to downstream exhaustive matching, as recorded at `specs/03_architecture/adrs/ADR-S20-002-NON-EXHAUSTIVE-POLICY.md:27-62`.

# Decision

A type MUST carry `#[non_exhaustive]` only if it is cross-crate `pub`-reachable AND falls into one of five classes — error enums, public wire/serde types crossing a process boundary or published schema, public config structs in workspace-API crates, ADR-published public surface, or documented state-machine variant enums — per `specs/03_architecture/adrs/ADR-S20-002-NON-EXHAUSTIVE-POLICY.md:65-110`. A type MAY omit it (and L2.1 does not flag it) for internal single-primitive newtypes, marker/phantom/ZSTs, private-fields builder structs, test-only types, and closed-set enums where adding a variant is itself the API change, per `specs/03_architecture/adrs/ADR-S20-002-NON-EXHAUSTIVE-POLICY.md:111-155`. The tie-breaker is whether adding a field/variant would break a downstream exhaustive match or struct-literal construction; greenfield ambiguity defaults to adding the attribute.

# Consequences

Enforcement narrows to the MUST classes with no mass migration of the 1,307 flagged lines — a MAY-OMIT type adds the attribute in the same PR as the extension that needs it (the SemVer-correct moment) — greenfield types follow the policy at authoring time, and the existing config/audit-event non-exhaustive invariants are left unchanged as instances of it, per `specs/03_architecture/adrs/ADR-S20-002-NON-EXHAUSTIVE-POLICY.md:182-199`.

# Citations

1. `specs/03_architecture/adrs/ADR-S20-002-NON-EXHAUSTIVE-POLICY.md:27-62` — the 1,307-line audit finding and the four reasons a blanket add is wrong (Context).
2. `specs/03_architecture/adrs/ADR-S20-002-NON-EXHAUSTIVE-POLICY.md:65-110` — the five MUST classes (error enums, wire/serde, config, ADR-published, state machines).
3. `specs/03_architecture/adrs/ADR-S20-002-NON-EXHAUSTIVE-POLICY.md:111-155` — the MAY-OMIT classes (newtypes, markers, builders, test-only, closed sets).
4. `specs/03_architecture/adrs/ADR-S20-002-NON-EXHAUSTIVE-POLICY.md:182-199` — consequences: refined enforcement, no mass migration, greenfield default.
