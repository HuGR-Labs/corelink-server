---
schema: corelink-ownership/1.1
document: reference
package: corelink-terraform-drift-consumer
manifest: crates/corelink-terraform-drift-consumer/Cargo.toml
source_commit: cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6
profile: S
state: draft
evidence_set: terraform-drift-consumer-static-source-cd74a094
---

# corelink-terraform-drift-consumer — ownership reference

Static-source reference only. It does not prove a Terraform plan, provider
operation, webhook, D1 write, CloudEvent delivery, metric export, remediation,
deployment, or runtime. The verified [OKF SRE operations hub](../../../knowledge/ops/sre-operations-hub.md)
is a routed reference, not policy revalidated here.

[Identity](#r01) · [Boundary](#r02) · [Classification](#r03) · [Pipeline](#r04) · [Finding](#r05) · [Metrics](#r06) · [Errors](#r07) · [Ingress/evidence](#r08) · [Limits](#r09).

<a id="r01"></a>
## R01 — Identity and static evidence

The manifest names `corelink-terraform-drift-consumer`, declares `thiserror`,
`serde`, `serde_json`, and UUID v7/serde support, and declares `proptest` only
for development. `lib.rs` exports seven modules: `audit`, `classifier`,
`consumer`, `error`, `event`, `metrics`, and `store`. A manifest dependency or
public-module inventory change falsifies this record. Evidence: `Cargo.toml`;
`src/lib.rs`; source snapshot `cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6`.

<a id="r02"></a>
## R02 — Ownership boundary and five axioms

The package owns local drift event/finding types, region/severity
classification, audit and finding-store traits, in-memory implementations, a
local metric accumulator, error taxonomy, and generic `DriftConsumer`
orchestration. It does not own an HTTP endpoint, GitHub Actions workflow,
Terraform binary/provider, credentials, D1 binding/schema, CloudEvent broker,
Prometheus endpoint, dual-approval gate, or deployed Worker.

Five axioms: (1) manifest/source prove static declarations only; (2) traits
prove neither an external adapter nor invocation; (3) source ordering proves
neither durable audit nor distributed atomicity; (4) `RemediationDecision::Apply`
is data, not an apply call or authorization; (5) the linked verified OKF hub is
routing context only.

Falsifier: a source-visible external adapter, runtime entry point, or operation
in this package changes the boundary. Evidence:
`Cargo.toml`; `src/lib.rs`; all local modules.

<a id="r03"></a>
## R03 — Event, validation, and severity invariants

`REGIONS` contains exactly `wnam`, `enam`, `weur`, and `sam`. The Terraform
workflow matrix uses the same four codes; migration 0141 accepts these for new
D1 inserts and preserves older region literals only to copy/read history.
`DefaultDriftClassifier::classify` rejects other regions and accepts only exit
codes 0, 1, and 2. It maps `plan_diff_count` to `None` at 0, `Low` at
1–2, `Medium` at 3–10, and `High` above 10; the mapping does not otherwise
branch on the accepted exit code.

Falsifiers: `moon-base`, exit code 3, counts 2/3, or 10/11 distinguish the
predicates. `DriftPlanEvent` is only an input shape; no authenticated Worker
endpoint or HTTP ingress is implemented in this package. Evidence:
`src/event.rs`; `src/classifier.rs`; workflow matrix; migration 0141.

<a id="r04"></a>
## R04 — Consumer ordering and audit event invariant

For a successfully classified event, `process_plan_event` constructs a finding,
selects `Detected` when `plan_diff_count > 0` otherwise `CleanRun`, calls
`audit_sink.emit(...)` with `?`, then calls `store.insert(...)`, then records
local cron and finding metrics. Thus a returned audit error prevents the local
store call, and a returned store error prevents metric calls; a store error can
follow an already-successful audit call.

Falsifier: moving either call, removing `?`, or recording metrics before
`insert` changes this source-order contract. This does not prove sink
durability, rollback, or inter-system atomicity. Evidence: `src/consumer.rs`;
`src/audit.rs`; `src/metrics.rs`.

**API index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004).

<a id="api-001"></a>
### API-001 — Classifier
**Symbols:** `DriftClassifier::classify`, `DefaultDriftClassifier::classify`.
**Input / precondition:** borrowed `DriftPlanEvent`; region must be in `REGIONS` and exit code in `{0,1,2}`.
**Output / errors:** `DriftSeverity` or `InvalidRegion` / `UnrecognisedExitCode`.
**Effects / compatibility:** pure classification; count boundaries are in R03 and REL-001. Evidence: `src/classifier.rs`, `src/event.rs`.
[Index](#r04)

<a id="api-002"></a>
### API-002 — Consumer pipeline
**Symbols:** `DriftConsumer::process_plan_event`, `DriftAuditSink::emit`, `DriftFindingStore::insert`.
**Input / precondition:** event, mutable store, audit sink, classifier; no authenticated ingress is implemented here.
**Output / errors:** `Result<DriftFinding, DriftConsumerError>`; unsafe URL, classifier, audit, or store errors propagate; audit error stops before insert and store error stops before metrics.
**Effects / compatibility:** audit call precedes local insert and metrics; no cross-system atomicity. Evidence: `src/consumer.rs`, `src/audit.rs`.
[Index](#r04)

<a id="api-003"></a>
### API-003 — Finding-store seam
**Symbols:** `DriftFindingStore::{insert,update_remediation,all_findings,open_findings}`.
**Input / precondition:** finding ID exists for updates; unique for insertion.
**Output / errors:** borrowed finding lists or typed `NotFound` / `StoreFailed` results.
**Effects / compatibility:** in-memory implementation mutates remediation fields only and has no delete method; it does not enforce authorization. Evidence: `src/store.rs`.
[Index](#r04)

<a id="api-004"></a>
### API-004 — Evidence URL boundary and event compatibility
**Symbols:** `is_safe_summary_artifact_url`, `DriftPlanEvent::plan_summary_artifact_url`.
**Input / precondition:** optional URL; deserialization also accepts legacy key `plan_full_artifact_url` through `serde(alias)`.
**Output / errors:** validator returns bool; `process_plan_event` returns `UnsafeEvidenceUrl` before classification, audit, store, or metrics when a supplied URL fails.
**Effects / compatibility:** URL guard requires lowercase `https://` prefix and rejects case-insensitive occurrences of `.tfplan`, `.log`, `plan.json`, `full-plan`, and `terraform-plan-`; it is not a parsed-URL host allowlist, authentication, or artifact-content inspection. Evidence: `src/event.rs`, `src/consumer.rs`, `src/error.rs`.
[Index](#r04)

**Invariant index:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004).

<a id="inv-001"></a>
### INV-001 — Accepted classifier domain
**Predicate:** only listed regions and exit codes 0, 1, 2 classify. **Enforcement:** `DefaultDriftClassifier::classify`. **Violation:** any other value returns typed error. **Verification:** boundary cases in R03; test execution unknown. **State:** SOURCE.
[Index](#r03)

<a id="inv-002"></a>
### INV-002 — Audit precedes local insert
**Predicate:** audit `emit` succeeds before store `insert` is called. **Enforcement:** `?` ordering in `process_plan_event`. **Violation:** reordered calls or swallowed error permits insert after audit failure. **Verification:** source-order inspection and existing unit-test source; execution unknown. **State:** SOURCE.
[Index](#r04)

<a id="inv-003"></a>
### INV-003 — No automatic apply or deletion surface
**Predicate:** this package has no Terraform invocation and `DriftFindingStore` declares no delete method. **Enforcement:** current public module/source inventory. **Violation:** adding either call or method falsifies the predicate. **Verification:** package-source search; no runtime claim. **State:** SOURCE.
[Index](#r02)

<a id="inv-004"></a>
### INV-004 — Canonical regions and bounded evidence pointer
**Predicate:** classifier accepts only `wnam|enam|weur|sam`; a present summary URL must pass `is_safe_summary_artifact_url` before pipeline side effects. **Enforcement:** `REGIONS`, `DefaultDriftClassifier::classify`, and first guard in `DriftConsumer::process_plan_event`; migration 0141 separately guards new D1 inserts. **Violation:** legacy/unknown region, unsupported code, or rejected URL returns before audit/store/metrics. **Verification:** source predicates and region/security test sources; execution unknown. **State:** SOURCE.
[Index](#r08)

<a id="r05"></a>
## R05 — Finding and remediation invariant

`DriftFinding::from_event` creates a UUIDv7 finding in `Open` status with no
remediation fields and fixed `runbook_ref` `RB-FM-206`. The store trait exposes
insert, remediation update, and read methods, but no delete method. The current
in-memory `update_remediation` changes status and the three remediation fields.
It accepts `remediated_by_user_id`, but contains no authorization or
dual-approval predicate/check.
`check_immutable_fields_unchanged` compares only finding ID, region, detection
time, diff count, and severity.

Falsifier: a new delete surface, mutation of one of those five fields, or an
authorization/dual-approval check changes this record. Neither the trait nor comments
establish append-only durable storage or an authorized remediation. Evidence:
`src/event.rs`; `src/store.rs`.

<a id="r06"></a>
## R06 — Local metric and non-apply surface

`DriftMetrics` locally retains vectors for findings, cron outcomes, and
remediation-duration observations. `DriftMetricOutcome` exposes four labels;
the source comment lists four canonical names, but the struct has no age-gauge
field or exporter. `auto_apply_forbidden()` is a `const fn` returning `true`,
and `process_plan_event` has no apply call.

Falsifier: adding an exporter, age-gauge storage, or an apply invocation
changes these claims. This does not prove metric publication or that a broader
program cannot invoke Terraform. Evidence: `src/metrics.rs`; `src/consumer.rs`;
`src/lib.rs`.

<a id="r07"></a>
## R07 — Error and serialization boundary

`DriftConsumerError` distinguishes invalid region, unrecognised exit code,
audit/store failure, missing finding, and forbidden operation. `DriftStoreError`
adds append-only-violation and internal variants. The event, finding, severity,
status, remediation-decision, and audit-record types declare serde derives.
Falsifier: changing a variant, branch, or derive changes the visible local
contract. Derives do not establish received or emitted JSON. Evidence:
`src/error.rs`; `src/event.rs`; `src/audit.rs`.

<a id="r08"></a>
## R08 — Workflow ingress and bounded evidence

The workflow schedules or dispatches four regional plan jobs, creates a
sanitized summary artifact, uploads it for seven days, and posts Slack alerts.
The sanitizer writes fixed metadata and action counts; the workflow cleanup
trap removes runner-local plan JSON, saved plan, and log.

The event includes `plan_summary_artifact_url` and a legacy serde alias, but
the inspected workflow contains no HTTP callback/event-post step to this crate.
The source comment describing a callback is not proof of ingress. The local URL
predicate rejects several raw-payload-looking names; it does not authenticate a
caller, prove the URL is a GitHub artifact, or inspect artifact bytes.

Falsifier: a source-visible callback/endpoint adapter or changed upload path
changes this boundary. Evidence: `.github/workflows/terraform-drift.yml`;
`scripts/sanitize_terraform_drift.py`; `src/event.rs`; `src/consumer.rs`;
`migrations/d1/0131_terraform_drift_summary_artifact.sql`.

[Relation map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)

<a id="r09"></a>
## R09 — Unknowns and closure

Unknown: callers beyond the static `corelink-ops` re-export; feature selection;
webhook authentication and delivery/ingress (the inspected workflow has no
callback step); Terraform CLI/plan
generation/provider behavior; OIDC credentials; D1 schema/durability;
CloudEvent transport/durability; Prometheus/Analytics export; real metric age
gauge; remediation authorization/dual approval; real apply/revert operation;
runtime, deployment, traffic, and unrun test results.

Success: every implementation statement R01–R08 has a falsifier. Completeness:
R01–R08 reconcile with B01–B10. Quality: the five axioms and unknowns stay
explicit. Definition of Done: the four static ownership artifacts and their
structural results are handed off; no operational result or approval follows.
[Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
