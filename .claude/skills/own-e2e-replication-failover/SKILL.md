---
name: own-e2e-replication-failover
description: Route source-static ownership and maintenance for the e2e-replication-failover Cargo package without treating its harness as runtime or provider evidence.
metadata:
  schema: "corelink-ownership/1.1"
  package: "e2e-replication-failover"
  manifest: "tests/e2e-replication-failover/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "e2e-replication-failover-static-20260921"
---

# e2e-replication-failover ownership

[S01](#s01) · [S02](#s02) · [S03](#s03) · [S04](#s04) · [S05](#s05) · [S06](#s06) · [S07](#s07).

<a id="s01"></a>
## S01 — Activation

| Use this skill when | Do not use it when |
|---|---|
| Editing `tests/e2e-replication-failover/`, its fixture, six scenario assertions, or these ownership records. | Changing coordinator or replica-worker implementation contracts; use their package ownership records. |
| Tracing how this harness composes in-memory coordinator collaborators. | Claiming CI selection, test execution, a staging drill, provider behavior, or production operation. |

<a id="s02"></a>
## S02 — Territory and authority

This package owns `E2EFixture` and its test source only. `corelink-replication-coordinator` owns coordinator, heartbeat, and audit contracts and implementation; `corelink-replica-worker` owns `Region`, which the coordinator re-exports. `E2EFixture::new` is the local in-memory composition root. The production composition root and runtime operator are unknown. Independent cold review belongs to the lead/reviewer, never the author; the reviewer identity is not supplied.

<a id="s03"></a>
## S03 — Reading and routing

Start with the manifest, then `src/lib.rs`, then `tests/scenarios.rs`; use the
reference for contracts/state and the blast record for dependency, inverse and
CI boundaries. Route coordinator decisions, heartbeat/audit traits and errors
to `corelink-replication-coordinator`; route the `Region` definition to
`corelink-replica-worker`. Route CI selection, staging drills and production
composition to their owning workflow/operator records. The verified [OKF
profile](../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md)
is a navigation reference, not a second policy.

<a id="s04"></a>
## S04 — Decisions and invariants

Use the decision form `condition → action → required evidence → stop when`.
Apply these evidence limits to every claim and route each invariant to its
falsifier in the reference:

1. Manifest and source prove declarations and visible code, not resolved selection or execution.
2. Re-exports and signatures identify static contracts, not behavior beyond their implementation owner.
3. Imports and trait composition show a source edge, not invocation or a complete consumer graph.
4. In-memory fixtures and assertions prove neither that tests ran nor provider behavior.
5. The wave plan and verified [OKF profile](../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) are routing context only; do not copy, redefine, or revalidate OKF policy.

<a id="s05"></a>
## S05 — Source map and change routing

Read `src/lib.rs` for `E2EFixture::{new,init_topology,heartbeat_all_healthy}` and `tests/scenarios.rs` for the six test functions and exact predicates. The package fixes `T0` and supplies timestamps; it has no clock abstraction. The [reference](../../../docs/ownership/crates/e2e-replication-failover/REFERENCE.md#r03) records targets and contracts; [blast radius](../../../docs/ownership/crates/e2e-replication-failover/BLAST_RADIUS.md#b03) records the source edges.

| Condition | Action and evidence | Stop when |
|---|---|---|
| A fixture or assertion changes | Trace its exact source predicate and update the matching API, invariant, and REL entries. | The conclusion requires an executed test or resolved target graph. |
| Coordinator or `Region` contract changes | Route to the coordinator or replica-worker implementation owner; update only the observed boundary. | A person/team escalation route or peer relation key is needed but unavailable. |
| A claim concerns audit delivery, a drill, or runtime | Keep it `UNKNOWN`; route canonical concepts through OKF without restating policy. | No authorized runtime operator or evidence is identified. |

<a id="s06"></a>
## S06 — Decisions and validation limits

Use the supplied profile-S structural checker and `git diff --check` for these documents. They do not approve meaning. This source-static task does not run Cargo, Rust, builds, tests, fuzzing, HTTP/network calls, providers, GitHub, deployment, production, databases, or storage. Follow [maintenance](../../../docs/ownership/crates/e2e-replication-failover/MAINTENANCE.md#m01).

<a id="s07"></a>
## S07 — Stop, output, and handoff

Keep the set in draft until independent cold review, peer-side REL reconciliation, and missing escalation routes are resolved. For every change, output the pinned source SHA, exact changed paths, affected API/INV/REL IDs, structural checker results, and remaining unknowns.

A historical audit statement or structural checker result is not current execution evidence or approval. Refuse requests to execute a drill, touch providers, alter production state, or represent a source-static assertion as a runtime result; hand those requests to the named operational owner when one is verified, otherwise record the route as unknown.

[Back to start](#s01)
