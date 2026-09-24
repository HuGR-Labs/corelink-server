---
name: own-corelink-cli
description: Maintain source-only ownership records for corelink-cli without inferring CLI execution, release, endpoint reachability, telemetry delivery, or runtime behavior.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-cli"
  manifest: "tools/cli/Cargo.toml"
  source-commit: "398e586ccef712477f2a4ce51e026443b67e5747"
  profile: "S"
  evidence-set: "corelink-cli-source-static-20260921"
---

# Ownership — corelink-cli

[Scope](#s01) · [Evidence](#s02) · [Surface](#s03) · [Axioms](#s04) · [Relations](#s05) · [Quality](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Scope and boundary

Own only this guide and `docs/ownership/crates/corelink-cli/{REFERENCE,BLAST_RADIUS,MAINTENANCE}.md`. Inspect `tools/cli/Cargo.toml` and checked-in files under `tools/cli/src/` only as needed to anchor the records. A binary target, URL literal, command name, dependency, example, or source comment does not prove an executable command, artifact, release, selected dependency, network transaction, endpoint reachability, telemetry delivery, deployment, or assigned human owner.

<a id="s02"></a>
## S02 — Evidence discipline

Class every statement as SOURCE, DOCUMENTARY, or UNKNOWN. SOURCE is the pinned manifest and checked-in Rust text. DOCUMENTARY is the supplied ownership checker and whitespace diff. Keep Cargo/build/test/check/clippy, fuzzing, generated help, binary invocation, filesystem effects, HTTP, DNS, telemetry receipt, package distribution, signing, release, runtime, deployment, and independent review UNKNOWN. The canonical [CLI reference](../../../docs/knowledge/ops/cli-reference.md) is a verified OKF route only: do not copy, redefine, or revalidate it.

<a id="s03"></a>
## S03 — Static surfaces

Record the manifest library/binary declarations, public library modules, Clap declaration tree, PAT resolver/parser call, config model and redaction helper, output enum/formatter, and telemetry guard/payload separately. A `tokio::spawn`, `reqwest` call, `println!`, `std::fs`, URL, or `std::process::exit` is a source seam, not evidence that any effect happens. Keep library re-exports distinct from the binary-only module tree.

<a id="s04"></a>
## S04 — Five axioms

Preserve the five falsifiable source predicates in [R05](../../../docs/ownership/crates/corelink-cli/REFERENCE.md#r05): unsafe-code prohibition; PAT environment-before-config resolution; PAT parsing delegated to `corelink-pat`; default-off telemetry configuration; and the telemetry false guard returning before spawn. For every altered predicate, name the exact source location and a textual falsifying edit. None establishes credentials, config contents, a process, a request, or a telemetry event.

<a id="s05"></a>
## S05 — Relation method

Describe exactly one directed SOURCE arrow per relation and retain its two endpoints, evidence path, change consequence, falsifier, and non-inference boundary. Use [B01–B06](../../../docs/ownership/crates/corelink-cli/BLAST_RADIUS.md#b01) for manifest-to-source, library facade, parser, formatter, configuration, and closure relations. Do not convert an import, call site, URL, public module, or command enum variant into a complete reverse graph, compatibility promise, selected build target, executable CLI, or runtime reachability claim.

<a id="s06"></a>
## S06 — Quality and completion criteria

Success is four bounded artifacts with package identity, pinned commit, static evidence paths, five axioms, atomic relations, procedure modes, explicit unknowns, and an OKF route that is not duplicated. Completeness requires S01–S07, R01–R08, B01–B06, and M01–M06. R predicates need a concrete falsifier; B relations need direct endpoints; M procedures need mode, prerequisite, predicate, evidence, stop, and recovery.

| Condition | Action | Evidence | Stop when |
|---|---|---|---|
| Manifest, module, or target declaration changes | Re-record the affected declaration and B01/B02 seam. | `Cargo.toml`, `src/lib.rs`, `src/main.rs`. | Resolution, compilation, or artifact evidence is needed. |
| Auth, config, output, or telemetry source changes | Re-record one local predicate and its direct relation. | Named local Rust text. | A credential, file, process, request, response, or delivery result is needed. |
| Validation requested | Use only the supplied checker and whitespace diff. | Documentary command results. | Cargo/test/network/runtime or review proof is requested. |

<a id="s07"></a>
## S07 — Definition of done and handoff

DoD: change only this guide and the three package records; run the supplied S-profile checker once per artifact and `git diff --check` against `398e586ccef712477f2a4ce51e026443b67e5747`; make the scoped commit. Handoff names the four paths, affected R/B/M identifiers, five axioms, direct arrows, documentary results, canonical OKF route, and unknowns. Structural checks are not semantic approval or independent review.
