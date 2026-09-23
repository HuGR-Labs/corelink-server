---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-dt-webhook
manifest: crates/corelink-dt-webhook/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: draft
evidence_set: corelink-dt-webhook-source-static-20260921
---

# corelink-dt-webhook — maintenance

Every procedure is STATIC_SOURCE, STATIC_GRAPH, or DOCUMENTARY only. None
authorizes Cargo, tests, network activity, webhook/endpoint inspection, secret
or environment access, provider interaction, deployment, or production work.
The canonical [SBOM/Dependency-Track ADR](../../../knowledge/adr/adr-s12-001-sbom-cyclonedx-ntia-tsa-dt.md)
is a route only, not procedure evidence.

[Baseline](#m01) · [Surface](#m02) · [HMAC](#m03) · [Severity/state](#m04) · [Targets](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Confirm baseline and scope

**Mode:** STATIC_SOURCE. **Prerequisite:** a requested ownership-record change.
**Predicate:** baseline is `398e586ccef712477f2a4ce51e026443b67e5747`, package
is `corelink-dt-webhook`, and only the assigned guide/three records may change.
**Procedure:** record SHA, manifest path, and changed path list. **Stop:** any
identity, baseline, or scope mismatch. **Recovery:** make no conclusion; obtain
a reconciled target. **Evidence:** revision and porcelain/path output.

<a id="m02"></a>
## M02 — Review root and domain interface

**Mode:** STATIC_SOURCE. **Prerequisite:** root export, trait, domain type, or
error change. **Predicate:** each changed public name is traced from `lib.rs`
to its defining module. **Procedure:** compare module/export names, two trait
methods, parameter/result types, and named enum/error changes. **Stop:** a
claim needs an untraced consumer, route, or selected implementor. **Recovery:**
retain that edge as unknown and request separately authorized evidence.
**Evidence:** `src/lib.rs`; `src/types.rs`.

<a id="m03"></a>
## M03 — Review HMAC source predicate

**Mode:** STATIC_SOURCE. **Prerequisite:** signature helper or HMAC dependency
change. **Predicate:** the prefix, decode branch, HMAC type, constant-time
comparison, and `HmacInvalid` mapping are source-traced. **Procedure:** follow
the supplied byte arguments through `verify_signature` and `sign`; record one
changed branch/value at a time. **Stop:** secret value, request, endpoint, or
authentication outcome is needed. **Recovery:** report it unknown; do not infer
it from helper code. **Evidence:** `src/hmac.rs`; `Cargo.toml`.

<a id="m04"></a>
## M04 — Review severity and local-state branches

**Mode:** STATIC_SOURCE. **Prerequisite:** threshold, channel arm, handler
ordering, DLQ cap, or snapshot change. **Predicate:** clamp thresholds,
channel vectors, `len() >= DLQ_CAP`, and branch order are traced. **Procedure:**
follow score → severity → vector and handler branch → local vector/DLQ/snapshot.
**Stop:** any conclusion requires actual alerting, queue persistence, metrics,
timing, or provider behavior. **Recovery:** preserve the source predicate and
mark the operation unknown. **Evidence:** `src/{severity,handler,dlq,metrics}.rs`.

<a id="m05"></a>
## M05 — Review declared targets and static graph

**Mode:** STATIC_GRAPH. **Prerequisite:** dependency, `[[test]]`, `[[example]]`,
or checked-in target-path change. **Predicate:** each manifest declaration and
existing local path is recorded without claiming selection/execution.
**Procedure:** compare direct dependency names and each declared target name/
path. Retain the known inverse source edges from `tools/dt-cli` and
`tools/dt-reconcile` (manifest dependency plus handler imports/use), and
`crates/corelink-ops` (manifest dependency plus `src/dt.rs` re-export).

**Stop:** resolved features, compile result, a test/example result, additional
reverse graph, or runtime use is required. **Recovery:** leave those unknown
and request independent evidence. **Evidence:** manifests; static
source/import/re-export text.

<a id="m06"></a>
## M06 — Documentary handoff and escalation

**Mode:** DOCUMENTARY. **Prerequisite:** only the assigned skill, reference,
blast-radius, and maintenance paths changed and the supplied S checker exists.
**Procedure:** run it once each for `skill`, `reference`, `blast_radius`, and
`maintenance` with profile `S`; run `git diff --check 398e586cc`; inspect the
changed paths.

**Predicate:** S01–S07, R01–R08, B01–B06, and M01–M06 exist; each checker
passes and the diff has no whitespace/scope error. **Stop:** a checker/diff
failure or need for operational evidence. **Recovery:** correct only an
assigned artifact or hand off the exact failed predicate. These results are not
build, test, provider, runtime, approval, or independent-review proof.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-dt-webhook/SKILL.md#s01).
