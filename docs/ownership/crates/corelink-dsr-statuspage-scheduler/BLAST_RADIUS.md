---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-dsr-statuspage-scheduler
manifest: crates/corelink-dsr-statuspage-scheduler/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: draft
evidence_set: w012-dsr-statuspage-scheduler-static-20260920
---

# corelink-dsr-statuspage-scheduler — blast radius

Atomic SOURCE relations only. No arrow proves scheduling, Statuspage API activity,
D1 effects, audit delivery, deployment, or runtime behavior.

[Root](#b01) · [Targets](#b02) · [Traits](#b03) · [Local composition](#b04) · [Fakes](#b05) · [Boundary](#b06)

<a id="b01"></a>
## B01 — Crate root → declared modules

Each record is one directed source relation. A module declaration or matching
re-export change falsifies its record; none proves a consumer or invocation.

### B01a — Root → audit

**Arrow:** `src/lib.rs` → `audit` declaration and audit-item re-exports.
**Mode:** SOURCE. **Boundary:** no audit delivery follows.

### B01b — Root → cron log

**Arrow:** `src/lib.rs` → `cron_log` declaration and ledger-item re-exports.
**Mode:** SOURCE. **Boundary:** no durable ledger follows.

### B01c — Root → row source

**Arrow:** `src/lib.rs` → `row_source` declaration and row-source-item re-exports.
**Mode:** SOURCE. **Boundary:** no D1 read follows.

### B01d — Root → scheduler

**Arrow:** `src/lib.rs` → `scheduler` declaration and scheduler-item re-exports.
**Mode:** SOURCE. **Boundary:** no invocation or schedule follows.

### B01e — Root → wasm row source

**Arrow:** `src/lib.rs` → `wasm32_row_source` declaration and its gated type
re-export. **Mode:** SOURCE. **Boundary:** no target selection or wasm behavior follows.

<a id="b02"></a>
## B02 — Target predicate → import declaration

**Arrow:** manifest target blocks and `scheduler.rs` `cfg` imports → native
`corelink_ops::statuspage` or wasm `corelink_statuspage_real` names. **Mode:**
SOURCE. **Falsifier:** change a target block or predicate. **Boundary:** no
selected-target, compilation, linking, or behavior conclusion follows.

<a id="b03"></a>
## B03 — Trait boundary → local type

### B03a — D1 row source → scheduler field

**Arrow:** `D1RowSource` → `DsrStatuspagePublishScheduler::row_source`.
**Mode:** STATIC_RELATION. **Falsifier:** edit the trait or field type.
**Boundary:** no D1 implementation or effect follows.

### B03b — Cron log → scheduler field

**Arrow:** `CronRunLog` → `DsrStatuspagePublishScheduler::cron_log`.
**Mode:** STATIC_RELATION. **Falsifier:** edit the trait or field type.
**Boundary:** no durable dedupe follows.

### B03c — Audit sink → scheduler field

**Arrow:** `SchedulerAuditSink` → `DsrStatuspagePublishScheduler::audit`.
**Mode:** STATIC_RELATION. **Falsifier:** edit the trait or field type.
**Boundary:** no audit delivery follows.

### B03d — Statuspage backend → scheduler field

**Arrow:** `StatuspageBackend` → `DsrStatuspagePublishScheduler::backend`.
**Mode:** STATIC_RELATION. **Falsifier:** edit the field type.
**Boundary:** no Statuspage API request or response follows.

<a id="b04"></a>
## B04 — Local data → typed conversion

### B04a — Row source → local rows

**Arrow:** `D1RowSource::fetch_window` → local `rows` binding in `run_once`.
**Mode:** STATIC_RELATION. **Falsifier:** remove/change the call or binding.
**Boundary:** no row provenance or D1 effect follows.

### B04b — Rows → aggregate helper

**Arrow:** local `rows` → `aggregate_24h_window(&rows, window_start_unix_s)`.
**Mode:** STATIC_RELATION. **Falsifier:** change the call expression.
**Boundary:** no aggregation result is asserted.

### B04c — Aggregate → bridge helper

**Arrow:** local `stats` → `bridge_to_report(&stats)`.
**Mode:** STATIC_RELATION. **Falsifier:** change the call expression.
**Boundary:** no report validity or consumer compatibility follows.

### B04d — Report → backend call expression

**Arrow:** local `report` → `StatuspageBackend::publish_dsr_metric` call expression.
**Mode:** STATIC_RELATION. **Falsifier:** change the call expression.
**Boundary:** no Statuspage API request, response, or publication follows.

### B04e — JSON text → typed outcome

**Arrow:** `parse_outcome_json` input `&str` → serde `VerificationOutcome` value.
**Mode:** SOURCE. **Falsifier:** change the helper expression.
**Boundary:** no stored-row validity follows.

### B04f — Typed outcome → JSON text

**Arrow:** `render_outcome_json` `VerificationOutcome` input → serde `String` value.
**Mode:** SOURCE. **Falsifier:** change the helper expression.
**Boundary:** no stored-row write follows.

<a id="b05"></a>
## B05 — Fake storage → source-visible observations

### B05a — Row-source fake → filter result

**Arrow:** `InMemoryD1RowSource::rows` → `fetch_window` filtered vector.
**Mode:** SOURCE. **Falsifier:** change fake state or filter control flow.
**Boundary:** fake state is not D1 evidence.

### B05b — Ledger fake → duplicate error

**Arrow:** `InMemoryCronRunLog::rows` → `record` duplicate predicate and
`AlreadyRecorded` result. **Mode:** SOURCE. **Falsifier:** change fake state or
predicate. **Boundary:** no durable uniqueness or locking guarantee follows.

### B05c — Audit fake → snapshot

**Arrow:** `InMemorySchedulerAuditSink::events` → `emit` push and `snapshot` value.
**Mode:** SOURCE. **Falsifier:** change fake state or method control flow.
**Boundary:** no audit delivery or persistence follows.

<a id="b06"></a>
## B06 — Coverage and non-relation boundary

B01a–B05c cover source root, target declarations, individual trait fields, local
expressions, JSON helpers, and fakes. They do not enumerate consumers, establish
compatibility, select a target, execute a schedule, contact an API, persist
D1/audit data, or prove deployment/runtime. **Mode:** UNKNOWN for those domains. The verified
[SRE operations hub](../../../knowledge/ops/sre-operations-hub.md) is routed OKF
context only.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-dsr-statuspage-scheduler/SKILL.md#s01)
