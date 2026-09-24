---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-tier-selection
manifest: crates/corelink-tier-selection/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: S
state: author_validated
evidence_set: w011-tier-selection-source-static-20260920
---

# corelink-tier-selection — maintenance

These are evidence-aware SOURCE and DOCUMENTARY procedures for static tier
rules and local fakes. They neither run nor authorize Cargo, tests, networking,
external operation, deployment, or independent review. The verified OKF route
is a routing reference only and never substitutes for local source evidence.

[Baseline](#m01) · [Taxonomy](#m02) · [Gates](#m03) · [Fakes](#m04) · [Documents](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Fix the static baseline

**Mode:** SOURCE_STATIC. **Prerequisite:** the requested baseline is
`16d9f0303d849a1ab3df14688bd2c7cdbfee8140` and the manifest is the target.
**Predicate:** package identity and scope match [R01](REFERENCE.md#r01).
**Action:** record the baseline and inspect `Cargo.toml`, `src/lib.rs`, and the
changed local module. **Stop/recovery:** stop on a different or ambiguous tree;
recover by obtaining the intended scoped snapshot. **Evidence:** baseline and
the exact paths read; SOURCE only.

<a id="m02"></a>
## M02 — Reconcile a taxonomy change

**Mode:** SOURCE_TAXONOMY. **Prerequisite:** a `TierKind` variant, canonical
list, string, or route helper changes. **Predicate:** the two arrays and all
affected `as_str`, `requires_stripe_checkout`, and
`routes_to_inquiry_form` match arms are compared as one declared surface.
**Action:** list the changed member, order, and local helper mapping; trace
[B01](BLAST_RADIUS.md#b01) and [B02](BLAST_RADIUS.md#b02). **Stop/recovery:**
stop if catalog, entitlement, route, or consumer compatibility is needed;
record that domain as unknown and request its owner. **Evidence:** `src/tier.rs`;
SOURCE.

<a id="m03"></a>
## M03 — Reconcile a gate, identifier, or state declaration

**Mode:** SOURCE_CONTRACT. **Prerequisite:** a public identifier, context,
gate, error, receipt, row, or local state field changes. **Predicate:** every
changed public signature and field is mapped to one R03, R04, or R06 statement
and one B03–B05 relation.

**Action:** compare the declaration plus its direct
local use; retain the exact non-exhaustive marker where present. **Stop/recovery:**
stop for durable state, caller order, authorization, or effect claims; recover
by leaving the local claim bounded and recording the missing evidence.
**Evidence:** `src/{tenant,dpa,error,ledger,audit}.rs`; SOURCE.

<a id="m04"></a>
## M04 — Reconcile a local fake

**Mode:** SOURCE_FAKE. **Prerequisite:** an in-memory gate, sink, client-shaped
fixture, inspection helper, or adversarial failure branch changes. **Predicate:**
the named fake remains tied to its declared trait and its local field or branch
is described independently of external systems.

**Action:** inspect the trait, implementation, mutex-backed collection if any,
and helper or failure branch. Trace `InMemoryDpaGate`,
`InMemoryTierSelectionAuditSink`, or `InMemoryStripeClient` respectively to
[B03](BLAST_RADIUS.md#b03), [B04](BLAST_RADIUS.md#b04), or
[B05](BLAST_RADIUS.md#b05).
**Stop/recovery:** `AlwaysDenyDpaGate` and `FailingTierSelectionAuditSink` have
no matching blast relation in this bounded map; stop before assigning either to
B03 or B04, retain its source claim in [R05](REFERENCE.md#r05) or
[R07](REFERENCE.md#r07), and escalate a request for a dedicated atomic relation
if blast analysis is required.

**Boundary:** also stop if any fixture is offered as persistence,
transmission, concurrency, or execution evidence; recover by restating it as
local source.
**Evidence:** `src/{dpa,audit,stripe}.rs`; SOURCE.

<a id="m05"></a>
## M05 — Validate the four documentary artifacts

**Mode:** DOCUMENTARY. **Prerequisite:** only the assigned guide and three
package documents changed, and the supplied S-profile checker is available.
**Predicate:** S01–S07, R01–R08, B01–B06, and M01–M06 exist, links resolve,
each profile-S structural check passes, and the baseline whitespace diff has no
finding.

**Action:** run the supplied checker once each for `skill`,
`reference`, `blast_radius`, and `maintenance`; then run the whitespace diff
against the pinned baseline and inspect the changed-path list. **Stop/recovery:**
stop on a structural, link, diff, or scope failure; repair only one of the four
assigned artifacts. **Evidence:** four checker verdicts and diff result;
DOCUMENTARY only.

<a id="m06"></a>
## M06 — Handoff and explicit unknowns

**Mode:** STATIC_HANDOFF. **Prerequisite:** M01–M05 are complete.
**Predicate:** the handoff names the baseline, four paths, applicable R/B/M
identifiers, five axioms, five unknowns, four checker outcomes, and the diff
outcome.

**Action:** state SOURCE and DOCUMENTARY literally; route an
out-of-scope question through the verified OKF without copying or revalidating
it. **Stop/recovery:** stop if a static check is called an execution,
compatibility, deployment, or independent-review result; replace it with the
matching [R08](REFERENCE.md#r08) unknown. **Evidence:** this four-artifact
packet and M05 outputs.

Success is a bounded static record. Every procedure supplies a mode,
prerequisite, predicate, action, stop/recovery boundary, and evidence class.
Documentary validation is structural only; it is not a semantic approval or a
claim about any external effect.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-tier-selection/SKILL.md#s01)
