---
name: own-corelink-go
description: Maintain source-scoped ownership records for the corelink-go Rust cgo binding without claiming a Go consumer, linked library, release, or runtime behavior.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-go"
  manifest: "tools/sdks/go/Cargo.toml"
  source-commit: "3feae2baed63061354533ffdfe2d94acfd24aa9a"
  profile: "S"
  evidence-set: "corelink-go-source-static-3feae2b"
---

# Own corelink-go

This S-profile guide owns static Rust binding text only. It does not establish
a Go module, cgo consumer, linked artifact, client request, release, network,
runtime, deployment, or independent review.

[Scope](#s01) · [Evidence](#s02) · [Surface](#s03) · [Axioms](#s04) · [Relations](#s05) · [Validation](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Scope and boundary

Own only this guide and `docs/ownership/crates/corelink-go/{REFERENCE,BLAST_RADIUS,MAINTENANCE}.md`. Source review is limited to `tools/sdks/go/Cargo.toml` and `tools/sdks/go/src/{lib,go_bridge}.rs` at the pinned commit. Do not read examples, a Go wrapper, client documentation, generated headers, release material, or runtime systems as evidence for this record.

<a id="s02"></a>
## S02 — Evidence discipline

Class every statement as SOURCE, DOCUMENTARY, or UNKNOWN. SOURCE is the named manifest and Rust binding text. DOCUMENTARY is the supplied profile-S checker and whitespace result. Artifact emission/linking, cgo ABI use, Go behavior, client behavior, telemetry delivery, test execution, network, release, runtime, deployment, and review are UNKNOWN.

<a id="s03"></a>
## S03 — Source contracts

Record manifest identity, root re-exports, the opaque-handle boundary, each exported constructor/free/accessor/digest/verify entry point, and the `corelink-client-verify` dependency as separate static contracts. A C ABI declaration is not proof of a foreign-language consumer or compatible ABI.

<a id="s04"></a>
## S04 — Falsifiable axioms

Preserve the source axioms in [R03–R07](../../../docs/ownership/crates/corelink-go/REFERENCE.md#r03): null/UTF-8 rejection before handle allocation; nonzero `client_verify` selects the default-on constructor and zero selects the disabled constructor; free handles null as a no-op; digest input checks precede slice construction; verify delegates with a null output-code pointer. Each changed axiom needs one named source location and a concrete falsifying edit.

<a id="s05"></a>
## S05 — Atomic relation method

Write one directed SOURCE arrow per relation in [B01–B06](../../../docs/ownership/crates/corelink-go/BLAST_RADIUS.md#b01). Keep manifest-to-root, root-to-bridge, bridge-to-client-verify FFI, handle lifecycle, digest, and verify seams distinct. Stop if a conclusion requires feature resolution, a reverse graph, a header, a Go call, ABI compatibility, or invocation.

<a id="s06"></a>
## S06 — Validation and canonical route

Completeness requires S01–S07, R01–R08, B01–B06, and M01–M06; predicates and arrows are atomic and falsifiable. The verified canonical OKF route is [SDK reference](../../../docs/knowledge/ops/sdk-reference.md); route to it only and neither copy nor revalidate its policy. Run the supplied profile-S checker once per artifact and the baseline whitespace diff only; their result is structural, not semantic approval or execution evidence.

Run this controlled external checker from the repository root and retain each
JSON result:

```sh
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-go/SKILL.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-go/REFERENCE.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-go/BLAST_RADIUS.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-go/MAINTENANCE.md
```

`IMPLEMENTED_CHECKS_PASS` is structural-only output: it does not certify
semantic completeness, runtime claims, cold review, or profile eligibility.

<a id="s07"></a>
## S07 — Definition of done and handoff

DoD: change only the four owned artifacts; record the baseline, source paths,
R/B/M identifiers, checker verdicts, and explicit unknowns; then make the
scoped commit. Handoff must state that Cargo, tests, Go/cgo, client use,
linking, releases, network, runtime, deployment, and independent review were
not established.
