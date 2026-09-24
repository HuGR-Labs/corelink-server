---
schema: corelink-ownership/1.1
document: reference
package: corelink-dt-reconcile
manifest: tools/dt-reconcile/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: author_validated
evidence_set: w013-dt-reconcile-static-source-20260921
---

# corelink-dt-reconcile — ownership reference

SOURCE-only reference for the manifest and `tools/dt-reconcile/src/main.rs` at the fixed commit. It records declarations, branches, and calls in source text; it does not demonstrate reconciliation, data effects, provider state, runtime reachability, delivery, scheduling, execution, or deployment. The verified canonical OKF is routed only through the [Wave 013 plan](../../WAVE_013_PLAN.md); this record does not copy, redefine, or revalidate it.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Contracts](#r04) · [Axioms](#r05) · [Target](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity and evidence

| Field | Source-defined value |
|---|---|
| Package / manifest | `corelink-dt-reconcile` / `tools/dt-reconcile/Cargo.toml` |
| Source inspected | `tools/dt-reconcile/src/main.rs` |
| Target | one `[[bin]]`, `corelink-dt-reconcile`, at `src/main.rs` |
| Direct CoreLink dependency | `corelink-dt-webhook` |
| Evidence class | SOURCE at `398e586ccef712477f2a4ce51e026443b67e5747` |

Workspace version, edition, Rust version, license, publish, and lint settings are inherited manifest declarations. Direct dependency declarations identify source-level compilation intent only.

<a id="r02"></a>
## R02 — Boundary and ownership

This package owns its private `ReconcileResult`, async `run_reconciliation` source body, `main` branch over its `Result`, and the manifest’s binary declaration. It consumes imported `InMemoryDlq`, `InMemoryDtWebhookHandler`, `DtWebhookHandler`, and `sign` interfaces; their definitions and all external integration are outside this territory.

No package-local HTTP client, durable queue/store, provider client, scheduler binding, credential store, deployment descriptor, or test target is declared in the inspected manifest/source. Source comments and variable names do not establish that any such component exists or is used.

<a id="r03"></a>
## R03 — Source map

| Path | Static responsibility |
|---|---|
| `tools/dt-reconcile/Cargo.toml` | Package identity, direct dependencies, and binary name/path |
| `tools/dt-reconcile/src/main.rs` | Result fields, environment reads/default branches, local counts, imported queue/handler calls, logs, and process exit branches |

The source file contains comments and literals describing potential external arrangements. They are text in the inspected source, not evidence of an API query, delivery log, metric, alert, schedule, provider, or operational result.

<a id="r04"></a>
## R04 — Static contracts

[API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003).

<a id="api-001"></a>
### API-001 — Private reconciliation result shape

Private `ReconcileResult` has `usize` fields `findings_count`, `delivered_count`, `gap_count`, and `replayed_count`. `run_reconciliation` returns `Result<ReconcileResult, Box<dyn std::error::Error>>`. These names and types are source-local diagnostic/result shape, not evidence of measured findings, confirmed delivery, a gap, or a replay. [Index](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Main result-to-exit branches

`main` initializes a tracing subscriber, awaits `run_reconciliation`, logs a success value, then selects source exit `2` when `gap_count > 0`, `0` otherwise, and `1` on `Err`. Static branch text does not establish process execution, an exit status, alert emission, or log observation. [Index](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Imported queue and handler seam

The source constructs imported in-memory queue/handler values, signs serialized entry events, sets a signature, and awaits the imported handler trait call. It conditionally pushes an imported entry shape after a local attempt-count comparison. Imported contracts, queue persistence, handler implementation, delivery, and data effects are unknown here. [Index](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Five falsifiable source axioms

| ID | Predicate tied to inspected source | Falsifier |
|---|---|---|
| INV-001 | `gap_count` is the source expression `dt_findings_count.saturating_sub(confirmed_deliveries)`. | The expression uses a different operator or inputs. |
| INV-002 | The source creates `InMemoryDlq`, reads `len`, then calls `drain_all` before iterating returned entries. | Construction/order/call sequence changes. |
| INV-003 | A handler `Ok` branch increments local `replayed`; an `Err` branch conditionally pushes only when `attempt_count < 3`. | Either branch, guard, or increment changes. |
| INV-004 | Entry-event bytes are formed with `serde_json::to_vec`, passed with `secret` to imported `sign`, then the returned value is supplied to `set_signature`. | The source call/mapping order changes. |
| INV-005 | `main` selects local source exit branches by outer `Result` and `gap_count > 0`. | The predicate or selected exit values change. |

These are falsifiable by source inspection or focused testing if separately authorized. They do not prove arithmetic inputs correspond to data, any queue is durable, a handler delivers, a signature is accepted, or an alert exists.

<a id="r06"></a>
## R06 — Static target and configuration text

The manifest declares one binary named `corelink-dt-reconcile` with path `src/main.rs`. That source reads `DT_API_URL`, `DT_API_KEY`, and `DT_WEBHOOK_SECRET`, each with a source fallback branch. The binary declaration and reads establish only a target/source/configuration-text relationship. Values, secret availability, provider access, feature resolution, compilation, invocation, and scheduling are unknown.

<a id="r07"></a>
## R07 — Local failure branches

Within `run_reconciliation`, propagated failures can originate from imported queue methods, JSON serialization, imported signing, or the imported async handler. Its replay error branch may construct an imported entry with incremented `attempt_count`; `main` maps an outer error to a source exit branch. This is a local control-flow description, not a report of a failed reconciliation, queue mutation, retry, escalation, or externally observed exit.

<a id="r08"></a>
## R08 — Evidence, unknowns, and done gate

Five axioms govern this packet: manifest/source prove static declarations only; private/public signatures prove source-visible shapes only; imports/traits prove neither implementation nor invocation; comments/literals/local branches prove neither provider/data/runtime effect; and the canonical OKF route is reference-only. Unknowns include actual inputs/counts, reconciliation outcome, data effects, queue state/durability, signature acceptance, handler delivery, alert/metric receipt, provider state/reachability, credentials, scheduling, binary execution, compilation, reverse consumers, deployment, tests, and cold review.

Success is a bounded source account with falsifiers. Completeness means R01–R07 cover the whole local manifest/source boundary, all declared direct CoreLink-edge text, result/exit branches, and source-local failure paths. Quality means no static evidence is promoted to runtime fact. See [B06](BLAST_RADIUS.md#b06) and [M06](MAINTENANCE.md#m06) for the companion done gate.

[Back to start](#r01)
