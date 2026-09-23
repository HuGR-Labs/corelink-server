---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-enterprise-inquiry
manifest: crates/corelink-enterprise-inquiry/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: author_validated
evidence_set: w012-enterprise-inquiry-source-static-20260920
---

# corelink-enterprise-inquiry — maintenance

These procedures are SOURCE-static or DOCUMENTARY. They do not authorize Cargo, test execution, network activity, credentials, deployment, or external operations.

[Baseline](#m01) · [Surface](#m02) · [Ledger](#m03) · [Ports](#m04) · [Checks](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Fix the baseline

**Mode:** SOURCE_STATIC. **Prerequisite:** pinned revision `3feae2baed63061354533ffdfe2d94acfd24aa9a` is available. **Predicate:** the pinned manifest path and R01 identity match. **Action:** inspect the manifest and affected Rust text at the pinned revision, for example `git show 3feae2baed63061354533ffdfe2d94acfd24aa9a:crates/corelink-enterprise-inquiry/src/lib.rs`. **Stop/recovery:** stop when the pinned object or path is absent; seek a scoped refresh. **Evidence:** pinned revision and source text.

<a id="m02"></a>
## M02 — Reconcile a public representation

**Mode:** SOURCE_CONTRACT. **Prerequisite:** a module/export, form field, enum, error, constant, or helper changed. **Predicate:** R02/R04 names the exact source shape and textual falsifier. **Action:** update the relevant source record and B01/B02 relation together; reconcile the known `corelink-ops` re-export and `corelink-slack-real` trait implementation when an exposed port changes. **Stop/recovery:** stop when compatibility beyond these checked-in edges or wire behavior is needed; mark it UNKNOWN under R08. **Evidence:** package `src/` and the named consumer manifests/source.

<a id="m03"></a>
## M03 — Reconcile a ledger rule

**Mode:** SOURCE_FLOW. **Prerequisite:** validation, idempotency, audit ordering, status transition, escalation, or SLA predicate changed. **Predicate:** one AX-EI-01 through AX-EI-03 remains atomic and falsifiable. **Action:** trace only the local branch and revise R05/B02. **Stop/recovery:** stop before asserting persistence, timing, concurrency, or delivery; retain that domain in R08. **Evidence:** `src/{ledger,form,outbox,error}.rs`.

<a id="m04"></a>
## M04 — Reconcile a port or adapter declaration

**Mode:** SOURCE_PORT. **Prerequisite:** an audit, Slack, CRM, mail, encryption, or HubSpot type/implementation changed. **Predicate:** trait signature, implementation relation, and boundary are separately recorded in R03/R07 and B03/B04. **Action:** inspect the local trait and implementation; preserve the explicit source limit. **Stop/recovery:** stop for credentials, provider behavior, transmission, remote acceptance, or secret handling; route those to R08. **Evidence:** named source modules only.

<a id="m05"></a>
## M05 — Validate documentary artifacts

**Mode:** DOCUMENTARY. **Prerequisite:** only the assigned skill and three package documents changed. **Predicate:** S01–S07, R01–R08, B01–B06, and M01–M06 exist; links resolve; four profile-S checks and `git diff --check 3feae2baed63061354533ffdfe2d94acfd24aa9a` pass. **Action:** run `docs/ownership/tools/check_docs.py` once each for `skill`, `reference`, `blast_radius`, and `maintenance`, then the whitespace diff. **Stop/recovery:** repair only these four paths on failure. **Evidence:** checker verdicts and diff output.

<a id="m06"></a>
## M06 — Handoff bounded evidence

**Mode:** STATIC_HANDOFF. **Prerequisite:** M01–M05 are complete. **Predicate:** handoff gives baseline, four paths, affected R/B/M IDs, five axioms, five unknown groups, checker verdicts, and diff outcome. **Action:** state SOURCE or DOCUMENTARY evidence literally and route policy questions only to the [OKF profile](../../../internal/okf-wiki/01-okf-corelink-profile.contract.md). **Stop/recovery:** replace an unsupported operational assertion with the corresponding R08 unknown. **Evidence:** these four artifacts and M05 outputs.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-enterprise-inquiry/SKILL.md#s01)
