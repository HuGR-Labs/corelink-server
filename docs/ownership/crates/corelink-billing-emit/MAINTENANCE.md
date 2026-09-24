---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-billing-emit
manifest: crates/corelink-billing-emit/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-billing-emit-structural-normalization-20260921
---

# `corelink-billing-emit` maintenance

[M01](#m01) · [M02](#m02) · [M03](#m03) · [M04](#m04) · [M05](#m05) · [M06](#m06)

All procedures are `STATIC_SOURCE` at baseline `9f372cc1f`; they authorize no
Cargo execution, test execution, network/provider access, deployment, or
publication.

<a id="m01"></a>
## M01 — Establish the static baseline

Prerequisite: a requested change names this package surface. Predicate: package
identity, source module, and direct manifest relation are known. Action: inspect
`Cargo.toml`, `src/lib.rs`, and the affected module; record baseline and source
paths. Stop/recovery: if a provider/runtime claim appears, stop and obtain
separate provider/composition evidence. Evidence: manifest/source paths and
this record; no runtime conclusion.

<a id="m02"></a>
## M02 — Change an event or key contract

Prerequisite: event fields, kind strings, units, periods, canonicalization, or
`IdemKey` change. Predicate: construction and key-input effects are explicitly
mapped. Action: compare `UsageEvent::new`, `validate_billing_period`,
`compute_canonical_bytes_for_idem`, and tracker `insert`; inspect B01–B02 and
known static consumers. Stop/recovery: stop on persisted-format or consumer
compatibility uncertainty; request a compatibility decision. Evidence: exact
source mapping, consumer manifests/imports, and explicit unknowns.

<a id="m03"></a>
## M03 — Change emitter, audit, or sink behavior

Prerequisite: a decision arm, audit record, append sequence, or sink port
changes. Predicate: concrete order is stated. Action: trace `emit` branch by
branch and `InMemoryR2UsageSink::append`; preserve or deliberately revise that
source order. Stop/recovery: stop if atomic rollback, a real R2/audit provider,
or delivery guarantee is required. Evidence: source branch/operation record.
Do not label the fake as external persistence.

<a id="m04"></a>
## M04 — Assess a static consumer/provider impact

Prerequisite: public export, trait, or `Region` relationship changes. Predicate:
known direct manifests are distinguished from import/re-export evidence. Action:
inspect the six B04 manifests, sampled imports, `corelink-billing` re-export,
and the analytics type edge; classify provider work as a handoff. Stop/recovery:
stop when a complete graph or live route is needed; obtain resolved/runtime
evidence from the appropriate owner. Evidence: paths searched and B04–B06.

<a id="m05"></a>
## M05 — Handle an unresolved risk

Prerequisite: collision, audit ordering, append-only, or delivery assertion is
broader than source enforcement. Predicate: the precise enforcement boundary
is written down. Action: state whether it is tracker state, in-memory sink
key-set behavior, or merely a trait/comment; route operational response to the
provider/composition owner. Stop/recovery: do not promise R2/Queue/Cron/D1
delivery, retention, or transactionality; recover only with separate evidence.
Evidence: R05–R08 and B03/B06.

<a id="m06"></a>
## M06 — Close a documentation-only change

Prerequisite: only the assigned four ownership paths changed. Predicate: S01–
S07, R01–R08, B01–B06, and M01–M06 exist; scope is limited and whitespace
diff is clean. Action: run the four documentary structural checks supplied for
the wave and `git diff --check`; record commands/results, paths, SHA, and
unknowns for independent review. Stop/recovery: a check pass is neither cold
review nor semantic/runtime approval; submit to the lead for that gate.

Continue with [reference](REFERENCE.md#r01) and [blast radius](BLAST_RADIUS.md#b01).
