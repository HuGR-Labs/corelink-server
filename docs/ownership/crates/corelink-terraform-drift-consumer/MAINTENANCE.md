---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-terraform-drift-consumer
manifest: crates/corelink-terraform-drift-consumer/Cargo.toml
source_commit: cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6
profile: S
state: draft
evidence_set: terraform-drift-consumer-static-source-cd74a094
---

# corelink-terraform-drift-consumer — maintenance

Static ownership procedures only. No procedure authorizes Terraform/provider
execution, credential use, network, webhook traffic, deploy, runtime probing,
or Cargo/test execution.

[Baseline](#m01) · [Classify](#m02) · [Pipeline](#m03) · [Finding](#m04) · [Evidence/ingress](#m05) · [Relations](#m06) · [Handoff](#m07).

<a id="m01"></a>
## M01 — Confirm baseline

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| READ_ONLY | SHA and package path match this record | Read the intended revision, `Cargo.toml`, and `src/lib.rs` | Stop on a different snapshot; obtain the intended baseline without resetting unrelated work |

<a id="m02"></a>
## M02 — Classify event and classifier changes

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| READ_ONLY | Event fields, region list, exit handling, severity, or error changes | Compare the exact allowlist, accepted codes, and count boundaries against R03/B01 | Stop before inferring webhook provenance, Terraform-plan meaning, or authenticated delivery |

<a id="m03"></a>
## M03 — Maintain pipeline ordering

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| READ_ONLY | Consumer, audit sink, store, or metrics change | Trace classify → audit `emit` → store `insert` → local metrics, including each `?` return | Stop if durability, rollback, D1, CloudEvent transport, or exporter behavior is required; obtain the owning contract |

<a id="m04"></a>
## M04 — Maintain finding and remediation predicates

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| READ_ONLY | Finding fields, status, decision, or store mutation changes | Compare initial values, mutable fields, immutable-field helper coverage, and authorization inputs | Stop before calling `Apply` a Terraform action or calling a user UUID dual approval; route authorization/operation to its owner |

<a id="m05"></a>
## M05 — Preserve evidence URL and ingress boundary

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| READ_ONLY | Event URL, workflow upload, region, or ingress changes | Check `plan_summary_artifact_url`, the `plan_full_artifact_url` serde alias, exact URL markers, four-region workflow matrix, upload path/retention, and whether a callback/endpoint is actually present | Stop before claiming URL authenticity, consumer-side artifact-content validation, webhook delivery, or runtime reachability without separate evidence; retain the five axioms in [R02](REFERENCE.md#r02) |

**Procedure index:** [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004).

<a id="proc-001"></a>
### PROC-001 — Reconcile the event and region contract
Objective: review a
static event/region change. Preconditions: exact source SHA is recorded;
environment: local checkout; permissions: `READ_ONLY`; inputs: event, classifier,
workflow, migration. Steps: inspect `src/event.rs` and `src/classifier.rs`;
compare workflow matrix with the four codes; inspect migration 0141's copy,
insert, and immutable-region clauses.

Expected predicate: new region writes are limited to `wnam`, `enam`, `weur`,
`sam`; legacy rows remain copyable. Stop on mismatch or unverified migration
ordering. Recovery: request owner reconciliation without applying migrations.
Evidence: source paths and SHA. Status: `REVIEWED_NOT_EXECUTED` (static
inspection only).

**Links:** [API-001](REFERENCE.md#api-001), [INV-001](REFERENCE.md#inv-001),
[INV-004](REFERENCE.md#inv-004), [REL-001](BLAST_RADIUS.md#rel-001),
[REL-007](BLAST_RADIUS.md#rel-007).
[Procedure index](#m05)

<a id="proc-002"></a>
### PROC-002 — Trace summary evidence and ingress
Objective: determine what
the event URL means and whether the workflow sends it to this consumer.
Preconditions: exact source SHA is recorded; environment: local checkout;
permissions: `READ_ONLY`; inputs: event, consumer, workflow, sanitizer, schema.
Steps: inspect the URL guard and its call order; inspect sanitizer output keys
and workflow upload path; search workflow steps for HTTP callback/event post;
compare event field/alias with migrations 0131/0141.

Expected predicate: URL guard runs before side effects; uploaded artifact is
sanitized; ingress is reported only if an explicit adapter/call exists. Stop if
credentials, network, or runtime proof is needed. Recovery: route to
composition/deployment owner. Evidence: source paths and SHA. Status:
`REVIEWED_NOT_EXECUTED`.

**Links:** [API-002](REFERENCE.md#api-002), [API-004](REFERENCE.md#api-004),
[INV-002](REFERENCE.md#inv-002), [REL-003](BLAST_RADIUS.md#rel-003),
[REL-008](BLAST_RADIUS.md#rel-008), [REL-009](BLAST_RADIUS.md#rel-009),
[REL-010](BLAST_RADIUS.md#rel-010).
[Procedure index](#m05)

<a id="proc-003"></a>
### PROC-003 — Trace audit/store failure and remediation boundaries
Objective:
review an error, finding, status, or remediation change. Preconditions: exact
source SHA; environment: local checkout; permissions: `READ_ONLY`; inputs:
`consumer.rs`, `audit.rs`, `store.rs`, `error.rs`, API-002/003, INV-002/003,
REL-002–005. Steps: (1) trace classifier/URL failure through each return;
(2) verify audit `?` precedes local insert and store `?` precedes metric calls;
(3) compare mutable remediation fields with immutable helper fields; (4) record
the error/metric observation present in source and name external observability
as unknown.

Expected predicate: audit failure does not reach insert, and store failure
reaches neither subsequent local metric call; no authorization or external
delivery is inferred. Stop if durability, rollback, dual approval, or an
operation is needed. Recovery: preserve static unknowns and escalate to the
audit/store, authorization, or composition owner via the verified OKF route.
Evidence: call-site lines, error branch, API/INV/REL IDs. Status:
`REVIEWED_NOT_EXECUTED`.
[Procedure index](#m05)

<a id="proc-004"></a>
### PROC-004 — Validate and hand off the four artifact set
Objective: validate
document changes. Preconditions: root and four assigned paths; environment:
local isolated checkout; permissions: documentation-only. Inputs: current
artifact bytes and checker. Steps: run each S-profile checker in M07; run
`git diff --check` scoped to the four paths; inspect changed paths; record exact
outputs, source SHA and hashes; request independent cold review.

Expected predicate: all four structural checks pass and the scoped diff is
clean. Stop on checker failure, stale pin, unowned path, or missing review.
Recovery: fix assigned docs, repeat checks, then cold-review changed bytes;
keep review and execution status separate. Evidence: outputs, hashes, and
verdicts. State: `EXECUTED_LOCAL` only when actually run; review is separate.
[Procedure index](#m05)

<a id="m06"></a>
## M06 — Preserve relationship evidence

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| READ_ONLY | A material relation is reported | Name workflow/migration/source endpoints, activation, failure propagation, validation evidence, coordination owner, and unknowns from [B03–B06](BLAST_RADIUS.md#b03) | Stop if a static relation is relabeled as execution; retain source and runtime evidence separately |

<a id="m07"></a>
## M07 — Documentary handoff

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| LOCAL_ISOLATED | Only the four ownership artifacts changed | Run checked-in candidate checker four times and scoped whitespace diff check | A pass is structural only; stop before semantic approval or operational claims |

```bash
python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-terraform-drift-consumer/SKILL.md
python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-terraform-drift-consumer/REFERENCE.md
python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-terraform-drift-consumer/BLAST_RADIUS.md
python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-terraform-drift-consumer/MAINTENANCE.md
git diff --check -- .claude/skills/own-corelink-terraform-drift-consumer/SKILL.md docs/ownership/crates/corelink-terraform-drift-consumer/REFERENCE.md docs/ownership/crates/corelink-terraform-drift-consumer/BLAST_RADIUS.md docs/ownership/crates/corelink-terraform-drift-consumer/MAINTENANCE.md
```

The checked-in candidate checker validates document structure only. A pass
does not imply semantic approval. The
four checker results and `git diff --check` are structural only, not semantic
completeness, review, execution, or operational proof. [Reference](REFERENCE.md#r01)
· [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01)
