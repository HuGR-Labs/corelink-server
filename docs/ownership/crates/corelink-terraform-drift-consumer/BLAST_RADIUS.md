---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-terraform-drift-consumer
manifest: crates/corelink-terraform-drift-consumer/Cargo.toml
source_commit: cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6
profile: S
state: draft
evidence_set: terraform-drift-consumer-static-source-cd74a094
---

# corelink-terraform-drift-consumer — blast radius

Atomic SOURCE relationships at the pinned snapshot. These records do not prove Terraform/provider execution, workflow delivery, webhook ingress, D1 writes, audit delivery, metric export, deployment, or runtime behavior.

[B01 Scope](#b01) · [B02 Method](#b02) · [B03 Direct relationships](#b03) · [B04 Transitive propagation](#b04) · [B05 Change validation](#b05) · [B06 Coverage](#b06).

<a id="b01"></a>
## B01 — Scope and owners

This package owns its drift-event input shape, classifier, `DriftConsumer`, audit/store traits, in-memory store, local metric accumulator, and error taxonomy. `corelink-ops` is a known static façade/re-export consumer. Workflow/sanitizer and migration/schema owners own their respective surfaces. No endpoint adapter or callback into this consumer is present in the inspected workflow/package source. The linked SRE/OKF material is a routing reference only.

<a id="b02"></a>
## B02 — Method and populations

**Inventory:** inspected the package manifest, module/target declarations, event, classifier, consumer, audit, store, metrics, and error source. Search included workflow, sanitizer, D1 migrations, and `corelink-ops` manifest/re-export.

**Resolved graph:** no artifact-specific target or feature resolution was run. **Semantic graph:** traced input validation, classification, audit→store→metrics order, remediation mutations, migration triggers, sanitizer output, workflow upload/Slack steps, and the static façade. **Reverse population:** `corelink-ops` is a found direct first-party consumer; exhaustive inverse consumers and external writers remain unknown. Search scope is the named snapshot/files; no tests/runtime were executed.

**Relationship index:** [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010).

<a id="b03"></a>
## B03 — Direct relationships

<a id="rel-001"></a>
### REL-001 — Event to classifier
**Identity:** `repo:1232040291:boundary:terraform-drift-event-classifier`. **Type/endpoints:** data/control; event region/exit code/diff count → classifier. **Activation:** consumer receives a source-level event. **Contract/effect:** four region strings and exit codes 0/1/2 classify; count determines severity.

**Failure/validation:** invalid region/code returns typed error before audit/store/metrics. Inspect event/classifier and boundary test source; coordinate event/schema owner. **Evidence:** pinned source; input origin/authentication unknown. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Classification to audit record
**Identity:** `repo:1232040291:boundary:terraform-drift-classification-audit-record`. **Type/endpoints:** data; classified event/finding → audit record. **Activation:** consumer handles accepted event. **Contract/effect:** zero diff selects `CleanRun`; nonzero selects `Detected`; local typed record is built.

**Failure/validation:** source/field changes alter record content. Compare constructors, types, literals, and test source; coordinate audit envelope owner. **Evidence:** consumer/event/audit source; no transport/durability. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Audit to store ordering
**Identity:** `repo:1232040291:boundary:terraform-drift-audit-store-order`. **Type/endpoints:** runtime-call (source); audit emit → local store insert. **Activation:** accepted event reaches pipeline. **Contract/effect:** `?` returns on audit error before insert; store error can follow successful emit.

**Failure/validation:** reordering/error swallowing changes local order; no distributed transaction. Inspect call order/test source; coordinate audit/store adapter owners. **Evidence:** consumer/trait/store source; no sink durability or rollback. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Store result to local metrics
**Identity:** `repo:1232040291:boundary:terraform-drift-store-local-metrics`. **Type/endpoints:** runtime-call (source); successful insert → local cron/finding metrics. **Activation:** store returns `Ok`. **Contract/effect:** insert error exits before metric calls.

**Failure/validation:** ordering changes can misalign local vectors with successful inserts. Trace consumer branch and accumulator; coordinate adapter owner before adding export. **Evidence:** consumer/metrics source; no published metric. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Finding to remediation mutation
**Identity:** `repo:1232040291:boundary:terraform-drift-finding-remediation`. **Type/endpoints:** data; finding ID → remediation update → status/decision/completion/user fields. **Activation:** caller invokes store trait. **Contract/effect:** in-memory store mutates four fields; immutable helper compares five fields but is not called by this method.

**Failure/validation:** mutation changes local lifecycle state. Compare trait, implementation, helper, and authorization inputs; coordinate authorization owner. **Evidence:** store/event source; no approval or Terraform apply/revert. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Static `corelink-ops` façade
**Identity:** `repo:1232040291:boundary:terraform-drift-ops-facade`. **Type/endpoints:** dependency/re-export; package → `corelink-ops` manifest and public facade. **Activation:** source import uses facade. **Contract/effect:** import path only; implementation ownership stays here.

**Failure/validation:** renamed/removed exports can break facade and downstream imports. Compare manifests, facade source, and inverse imports; coordinate ops owner. **Evidence:** pinned manifest/source; callers/runtime unknown. [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Workflow region matrix to classifier/schema
**Identity:** `repo:1232040291:boundary:terraform-drift-region-contract`. **Type/endpoints:** config/data; workflow matrix → event/classifier → migration 0141 triggers. **Activation:** regional job values and new finding inserts. **Contract/effect:** canonical four regions; migration retains legacy values for copy/read.

**Failure/validation:** mismatch rejects or misclassifies new events/rows. Compare workflow, event/classifier, migration clauses, and contract-test source; coordinate workflow/schema owners. **Evidence:** source only; not executed. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Sanitizer and workflow summary upload
**Identity:** `repo:1232040291:boundary:terraform-drift-summary-artifact`. **Type/endpoints:** build/data; sanitizer output → workflow summary upload. **Activation:** matrix job sanitizes and uploads. **Contract/effect:** fixed metadata/action counts; raw plan/log files removed from runner paths; summary retention is seven days.

**Failure/validation:** sanitizer/upload drift can remove evidence or expose raw inputs. Inspect cleanup, upload path, allowlisted output, and test source; coordinate workflow/security owner. **Evidence:** source only; no action execution/artifact inspection. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Event URL to D1 schema compatibility
**Identity:** `repo:1232040291:boundary:terraform-drift-summary-url-schema`. **Type/endpoints:** data/schema; event summary URL and serde alias ↔ D1 column/migrations. **Activation:** only if an external writer is connected. **Contract/effect:** legacy input key is accepted; old rows retain old column; new field names the summary artifact.

**Failure/validation:** field/column drift may lose or misroute evidence. Compare field, alias, migrations, and any writer mapping; coordinate schema/event owners. **Evidence:** source/migration; writer mapping unknown. [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Workflow-to-consumer ingress gap
**Identity:** `repo:1232040291:boundary:terraform-drift-workflow-ingress-gap`. **Type/endpoints:** external-contract absence; workflow plan/upload/Slack → no inspected ingress → `process_plan_event`. **Activation:** a claim that scheduled/dispatch workflow reaches this consumer. **Contract/effect:** no source-visible callback in inspected files.

**Failure/validation:** assuming ingress can leave processing unreachable or unauthenticated. Search workflow and package ingress source; require composition/deployment owner evidence. **Evidence:** workflow/lib/event/consumer source; bounded absence only. [Relation index](#b03)

<a id="b04"></a>
## B04 — Transitive propagation

Workflow region configuration can affect classifier acceptance and the D1 insert trigger through REL-007. Sanitizer/upload changes affect the summary URL only if a writer connects workflow to the event (REL-008→REL-009); the inspected snapshot has no such edge (REL-010).

The audit/store/local-metric chain is source control flow REL-002→REL-003→REL-004; audit may succeed before store failure, with no cross-system transaction. The `corelink-ops` façade may propagate API changes (REL-006), but downstream consumers are unresolved. No path proves apply, D1 write, audit export, or metric publication.

<a id="b05"></a>
## B05 — Change → impact → validation

| Change | Impact propagation | Validation / coordination |
|---|---|---|
| Event fields, region or exit/severity rule | REL-001/007/009 and migration compatibility | Compare API-001, INV-001/004, workflow matrix, serde alias, migrations 0131/0141; coordinate event/workflow/schema owners |
| Audit/store call order or errors | REL-002/003/004 | Trace each `?`/return branch against API-002 and INV-002; inspect failure-test source; coordinate sink/store owners |
| Finding/remediation fields or decision | REL-005, API-003, INV-003 | Compare mutable/immutable field sets and authorization inputs; coordinate authorization owner; do not infer an operation |
| Sanitizer/upload/URL guard | REL-008/009/010, API-004, INV-004 | Inspect sanitizer allowlist, workflow upload/cleanup and URL guard order; retain ingress/auth unknown; coordinate workflow and security owners |
| Facade export | REL-006 | Compare ops manifest, re-export and discovered inverse source imports; coordinate composition owner |
| Local metrics or error mapping | REL-004 and R06/R07 | Trace call sites, error variant, `DriftMetricOutcome` labels and local storage; require separate adapter evidence for export |

<a id="b06"></a>
## B06 — Coverage and unknowns

Covered: manifest/modules, pipeline/remediation/store/metrics/errors, one known ops façade, workflow/sanitizer, and named D1 migrations. Failure/observability is local: typed consumer/store errors, audit/store returns/order, in-memory metric vectors. This does not prove external delivery, durable audit, D1 operation, metric export, alerts, or authorization.

Unknown: exhaustive inverse/external consumers, resolved targets/features, callback/auth ingress, workflow execution, Terraform CLI/provider/OIDC, D1 durability, CloudEvent transport, age-gauge/exporter, dual approval, apply/revert, deploy, and runtime. New consumers require renewed discovery; known-file hashes alone are insufficient.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
