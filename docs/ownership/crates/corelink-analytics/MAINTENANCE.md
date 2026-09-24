---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-analytics
manifest: crates/corelink-analytics/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: S
state: draft
evidence_set: analytics-static-source-20260920
---

# corelink-analytics — maintenance

Static-analysis maintenance guide only. It neither instructs runtime operation nor records Cargo, D1, network, deployment, or review results.

[Baseline](#m01) · [Scope](#m02) · [Taxonomy](#m03) · [Admission](#m04) · [Schema](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Baseline control

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Expected source commit and assigned worktree are known | Read manifest and status before documentation or source assessment | Stop on a divergent baseline; recover by obtaining the reconciled baseline, without reset | SHA, branch, manifest path |

<a id="m02"></a>
## M02 — Scope selection

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Requested symbol maps to an analytics module | Map it to `audit`, `canonical`, `config`, `error`, `labels`, `observer`, or `validator` | Stop if it belongs to a production adapter; recover by routing to that adapter’s owner | `src/lib.rs` module/export map |

<a id="m03"></a>
## M03 — Taxonomy change

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Metric kind or label vocabulary changes | Compare canonical kinds, typed labels, forbidden labels, and static imports | Stop if cardinality bound or consumer compatibility is unknown; recover by recording the unknown and narrowing the change | `canonical.rs`, `labels.rs`, B04–B05 |

<a id="m04"></a>
## M04 — Admission or audit change

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Validator, observer, error, or audit path changes | Trace decision and mutation ordering in local source; inspect named property-test source | Stop if a handler, durable adapter, or HTTP mapping is required; recover by isolate-and-escalate, not infer behavior | `validator.rs`, `observer.rs`, `audit.rs`, `tests/prop_analytics.rs` |

<a id="m05"></a>
## M05 — Configuration or migration change

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Budget, bucket, schema version, or migration text changes | Reconcile `config.rs`, `lib.rs`, migration 0015, and its test source | Stop if applied-schema or data compatibility evidence is needed; recover with an explicit migration owner decision | `config.rs`, `migrations/d1/0015_analytics_cardinality_budgets.sql`, `tests/migration_canonical_0015.rs` |

<a id="m06"></a>
## M06 — Static handoff

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Assessment is complete | Report source paths, static consumer set, changed contract, and unknowns | Stop before claiming execution, production, deployment, or review; recover by retaining the boundary in the handoff | REFERENCE R08 and BLAST B06 |

[Reference](REFERENCE.md#r01) · [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01)
