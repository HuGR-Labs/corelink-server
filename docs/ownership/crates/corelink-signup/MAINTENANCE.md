---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-signup
manifest: crates/corelink-signup/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: signup-static-source-20260920
---

# corelink-signup — maintenance

These procedures are SOURCE_STATIC/DOCUMENTARY only. They do not authorize or report Cargo, tests, identity, signup, billing, provider, network, persistence, secrets, deployment, or runtime activity. Verified architecture material is routed only through [Signup auto-provision](../../../knowledge/flows/signup-auto-provision.md).

[Baseline](#m01) · [Triage](#m02) · [Contracts](#m03) · [Fakes](#m04) · [Unknowns](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Establish the static baseline

**Mode:** SOURCE_STATIC. **Prerequisite:** target checkout and source commit are known. **Predicate:** manifest, `src/lib.rs`, affected module, and relevant test source are identified at `6be030999de1f0e0fe62d3a9abb04ec2a4fefde6`. **Action:** record paths and the changed symbol before editing. **Stop/recovery:** stop if the task requires an uninspected boundary; recover by asking its owner for bounded evidence. **Evidence:** paths and revision only.

<a id="m02"></a>
## M02 — Classify one atomic relation

**Mode:** SOURCE_STATIC. **Prerequisite:** changed source symbol is known. **Predicate:** exactly one primary B01–B05 relation and one AX-01–AX-05 axiom are named, with adjacent relations added only where a value crosses them. **Action:** map facade, representation, collaborator, store, or audit/client fixture work using [B01–B05](BLAST_RADIUS.md#b01). **Stop/recovery:** stop if the relation requires all consumers or behavior; recover with graph or execution evidence from the accountable owner. **Evidence:** source paths, relation, axiom.

<a id="m03"></a>
## M03 — Review a public contract

**Mode:** SOURCE_STATIC. **Prerequisite:** a request, outcome, trait, error, wrapper, or re-export changes. **Predicate:** the before/after Rust signature, field, variant, label, or bound is enumerated and its falsifier remains concrete. **Action:** reconcile R03, R05, B02 or B03, and affected exports. **Stop/recovery:** stop before asserting caller, format, data, or external compatibility; recover with independently collected consumer/integration evidence. **Evidence:** declaration text and explicit unknown group.

<a id="m04"></a>
## M04 — Review a fake or chaos-source change

**Mode:** SOURCE_STATIC, NOT_EXECUTED. **Prerequisite:** the selected fixture and test source are known. **Predicate:** fake behavior is described as source text and the relevant test assertion remains an assertion, not a result. **Action:** reconcile AX-05, B04/B05, and the test path. **Stop/recovery:** stop if persistence, concurrency, availability, or an external outcome is requested; recover with an authorized test or integration owner’s evidence. **Evidence:** fixture and test-source paths.

<a id="m05"></a>
## M05 — Preserve five unknown groups

**Mode:** SOURCE_STATIC. **Prerequisite:** a conclusion is being drafted. **Predicate:** graph, implementations, data, integration, and operation remain explicitly UNKNOWN unless separately evidenced. **Action:** cite [R08](REFERENCE.md#r08), and use the verified OKF page as a route only. **Stop/recovery:** stop if a source comment, fake, or concept is being used as runtime proof; recover by separating the claim and obtaining the required evidence. **Evidence:** written mode label and unknowns.

<a id="m06"></a>
## M06 — Documentary handoff and definition of done

**Mode:** DOCUMENTARY. **Prerequisite:** only these four ownership paths changed. **Predicate:** S01–S07, R01–R08, B01–B06, and M01–M06 exist; links resolve; four profile-S structural checks and `git diff --check` pass; changed paths stay scoped.

**Action:** run the supplied checker once each for `skill`, `reference`, `blast_radius`, and `maintenance`, then record its four structural verdicts and the diff verdict.

**Stop/recovery:** stop on a checker, link, diff, or scope failure; repair only an owned artifact or hand off the exact failure. **Evidence:** commands, JSON/verdicts, changed paths, and baseline.

**Success:** a reader can locate a static contract, its axiom, relation, mode, and five unknown groups.

**Completeness:** all affected declarations, fakes, and test-source assertions are mapped or marked unknown.

**Quality:** no SOURCE_STATIC or NOT_EXECUTED claim is relabeled as runtime, integration, or approval evidence.

**Definition of Done:** the scoped artifacts pass documentary checks; this is not a build, test result, semantic approval, cold review, or operational conclusion.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Skill](../../../../.claude/skills/own-corelink-signup/SKILL.md#s01)
