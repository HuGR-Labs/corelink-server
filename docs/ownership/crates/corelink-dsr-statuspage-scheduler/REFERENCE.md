---
schema: corelink-ownership/1.1
document: reference
package: corelink-dsr-statuspage-scheduler
manifest: crates/corelink-dsr-statuspage-scheduler/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: draft
evidence_set: w012-dsr-statuspage-scheduler-static-20260920
---

# corelink-dsr-statuspage-scheduler — ownership reference

SOURCE-only reference. It records package declarations, not scheduling,
Statuspage API activity, D1 effects, audit delivery, target selection, or runtime behavior.

[Identity](#r01) · [Root](#r02) · [Targets](#r03) · [Contracts](#r04) · [Axioms](#r05) · [Fakes](#r06) · [Evidence](#r07) · [Unknowns](#r08)

<a id="r01"></a>
## R01 — Identity

The manifest names `corelink-dsr-statuspage-scheduler`, declares the listed
workspace and third-party dependencies, and has wasm32 and non-wasm32 dependency
blocks. Altering package identity, a dependency declaration, or a target block
falsifies this inventory. Evidence: `Cargo.toml`; SOURCE.

<a id="r02"></a>
## R02 — Root surface

`src/lib.rs` declares and re-exports `audit`, `cron_log`, `row_source`,
`scheduler`, and `wasm32_row_source`; the wasm-only re-export is gated. Altering
a module declaration, re-export, or gate falsifies the root surface. This does
not establish a consumer or invocation. Evidence: `src/lib.rs`; SOURCE.

<a id="r03"></a>
## R03 — Target declaration

`scheduler.rs` chooses statuspage symbols through opposing `cfg(target_arch =
"wasm32")` imports; the manifest likewise declares `worker` only for wasm32 and
`corelink-ops` only otherwise. Altering a `cfg` or target block falsifies this
record. It proves neither selected target nor build/link result. Evidence:
`Cargo.toml`, `src/{lib,scheduler,wasm32_row_source}.rs`; SOURCE.

<a id="r04"></a>
## R04 — Local contracts

`D1RowSource::fetch_window`, `CronRunLog::{record,is_recorded}`, and
`SchedulerAuditSink::emit` are declared trait boundaries. The scheduler type
holds those traits plus `StatuspageBackend`, and returns typed `RunOutcome` or
`SchedulerError`; `parse_outcome_json` and `render_outcome_json` expose local
serde conversion helpers. A signature, variant, branch, or helper-expression
change falsifies this statement. No trait or composition proves an external
call. Evidence: `src/{scheduler,row_source,cron_log,audit,wasm32_row_source}.rs`; SOURCE.

<a id="r05"></a>
## R05 — Five source axioms

| ID | Falsifiable SOURCE axiom | Static falsifier / limit |
|---|---|---|
| AX-DSRSP-01 | Native and wasm imports in `scheduler.rs` are selected by opposed `cfg` predicates. | Change either predicate; no target-selection or build claim follows. |
| AX-DSRSP-02 | `run_once` emits `Scheduled` before `is_recorded`; its explicitly logged already-recorded, row-source, empty, bridge, and publish branches emit their local `Skipped` or `Failed` event before that branch's matching return. | Reorder/remove a named `emit` call. `CronRunLog` failures from `is_recorded` or later `record` calls can return after `Scheduled`, `Skipped`, or `Succeeded` without an additional `Failed` event; no audit delivery claim follows. |
| AX-DSRSP-03 | The in-memory ledger keys a duplicate check by `(date_yyyymmdd, metric_id)`. | Change the fake predicate/key; no durable uniqueness claim follows. |
| AX-DSRSP-04 | The in-memory row source applies a `[start, start + 86_400)` filter to rows with a report. | Change filter bounds; no D1 query-result claim follows. |
| AX-DSRSP-05 | The JSON helpers call serde deserialize/serialize for `VerificationOutcome`. | Change either helper; no stored-row or data-validity claim follows. |

<a id="r06"></a>
## R06 — Fake and local-data declarations

`InMemorySchedulerAuditSink` holds a mutex-protected event vector;
`InMemoryCronRunLog` holds recorded tuples; `InMemoryD1RowSource` holds rows and
a failure flag. Removing named fields or snapshot/fetch implementations falsifies
the fake inventory. Fakes are not persistence, D1, audit, or concurrency evidence.
Evidence: `src/{audit,cron_log,row_source}.rs`; SOURCE.

<a id="r07"></a>
## R07 — Evidence boundary

SOURCE is the named manifest and local source at the pinned commit.
STATIC_RELATION names one source arrow in B01–B05. DOCUMENTARY can establish
only artifact structure. The verified [SRE operations hub](../../../knowledge/ops/sre-operations-hub.md)
is an OKF route only and is neither copied nor revalidated.

<a id="r08"></a>
## R08 — Five unknowns and closure

1. Selected target, features, compilation, and linking are UNKNOWN.
2. Consumer graph, construction, invocation, and scheduling are UNKNOWN.
3. D1 state, tenant context, credentials, API request/response, and network activity are UNKNOWN.
4. Audit storage/delivery, dedupe durability, timing, and concurrency outside fakes are UNKNOWN.
5. Configuration, deployment, monitoring, production data, and independent review are UNKNOWN.

Success: each source assertion has a falsifier. Completeness: R01–R07 and
B01–B06 reconcile. Quality: modes and unknowns remain explicit. Definition of
Done: the four ownership artifacts and structural results are handed off; no
scheduling, API, or runtime conclusion is implied.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-dsr-statuspage-scheduler/SKILL.md#s01)
