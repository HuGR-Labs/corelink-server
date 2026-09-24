---
name: own-corelink-terraform-drift-consumer
description: >-
  Route static ownership changes to the Terraform drift consumer's canonical
  region, bounded evidence URL, event, pipeline, store, and local metric contracts; exclude Terraform and provider operation.
metadata:
  schema: "corelink-ownership/1.1"
  profile: "S"
  package: "corelink-terraform-drift-consumer"
  manifest: "crates/corelink-terraform-drift-consumer/Cargo.toml"
  source-commit: "cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6"
  evidence-set: "terraform-drift-consumer-static-source-cd74a094"
---

# Ownership — corelink-terraform-drift-consumer

Static-source routing guide. It establishes no Terraform invocation, provider
credential, GitHub webhook delivery, D1 mutation, metric export, remediation
authorization, deployment, or runtime execution.

[Trigger](#s01) · [Boundary](#s02) · [Classification](#s03) · [Pipeline](#s04) · [Remediation](#s05) · [Ingress/evidence](#s06) · [Handoff](#s07) · [Closure](#s08).

<a id="s01"></a>
## S01 — Trigger

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Manifest, workflow region, event field, or local drift contract changes | Route to these static ownership records and inspect linked workflow/migration evidence | `Cargo.toml`; `src/{lib,event,classifier,consumer,audit,store,metrics,error}.rs`; `.github/workflows/terraform-drift.yml`; migrations 0131/0141 | A Terraform plan/apply, provider, or deployed Worker result is requested |

<a id="s02"></a>
## S02 — Boundary and axioms

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Scope is uncertain | Retain only local data types, traits, in-memory seams, and generic orchestration | [R02](../../../docs/ownership/crates/corelink-terraform-drift-consumer/REFERENCE.md#r02) | Infer an external adapter, sink, caller, or operation |

Five axioms: (1) manifest and source establish declarations only; (2) a trait
does not establish its production implementation or invocation; (3) local call
ordering does not establish sink durability or cross-system atomicity; (4) an
enum carrying `Apply` is not an apply operation or approval; (5) the verified
[OKF SRE operations hub](../../../docs/knowledge/ops/sre-operations-hub.md) is
routing context only and is not revalidated here.

<a id="s03"></a>
## S03 — Event and classifier contract

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Event, region, exit-code, or severity logic changes | Trace `wnam, enam, weur, sam`, `0..=2`, severity boundaries, and D1 historical-row compatibility | [R03](../../../docs/ownership/crates/corelink-terraform-drift-consumer/REFERENCE.md#r03), [B03 REL-001](../../../docs/ownership/crates/corelink-terraform-drift-consumer/BLAST_RADIUS.md#rel-001), [B03 REL-007](../../../docs/ownership/crates/corelink-terraform-drift-consumer/BLAST_RADIUS.md#rel-007) | Claim a GitHub Action supplied, authenticated, or executed the event |

<a id="s04"></a>
## S04 — Audit/store/metric pipeline

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Consumer, sink, store, or metric code changes | Preserve classify → audit `emit` → store `insert` → local metrics order and each error exit | [R04](../../../docs/ownership/crates/corelink-terraform-drift-consumer/REFERENCE.md#r04), [B02](../../../docs/ownership/crates/corelink-terraform-drift-consumer/BLAST_RADIUS.md#b02)–[B04](../../../docs/ownership/crates/corelink-terraform-drift-consumer/BLAST_RADIUS.md#b04) | Claim durable audit, D1 atomicity, CloudEvent delivery, or Prometheus publication |

<a id="s05"></a>
## S05 — Remediation boundary

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Finding status or decision changes | Review mutable/immutable fields and the absence of authorization or dual-approval checks in `update_remediation` | [R05](../../../docs/ownership/crates/corelink-terraform-drift-consumer/REFERENCE.md#r05), [B05](../../../docs/ownership/crates/corelink-terraform-drift-consumer/BLAST_RADIUS.md#b05) | Treat `RemediationDecision::Apply` as a Terraform apply or a dual-approval proof |

<a id="s06"></a>
## S06 — Evidence URL and ingress

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Summary URL, sanitizer, workflow upload, or event delivery changes | Verify the current field, legacy serde alias, rejection markers, sanitizer output, upload input, and whether a callback actually exists | [API-004](../../../docs/ownership/crates/corelink-terraform-drift-consumer/REFERENCE.md#api-004), [R08](../../../docs/ownership/crates/corelink-terraform-drift-consumer/REFERENCE.md#r08), [B03 REL-008](../../../docs/ownership/crates/corelink-terraform-drift-consumer/BLAST_RADIUS.md#rel-008), [B03 REL-009](../../../docs/ownership/crates/corelink-terraform-drift-consumer/BLAST_RADIUS.md#rel-009), [B03 REL-010](../../../docs/ownership/crates/corelink-terraform-drift-consumer/BLAST_RADIUS.md#rel-010), [M05](../../../docs/ownership/crates/corelink-terraform-drift-consumer/MAINTENANCE.md#m05) | Do not claim URL authenticity, artifact-content validation at the consumer, webhook ingress, or runtime reachability from field shape or artifact upload |

<a id="s07"></a>
## S07 — Static handoff

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Static assessment is complete | Record source snapshot, affected contract, relations, structural results, and unresolved external edges | [M06](../../../docs/ownership/crates/corelink-terraform-drift-consumer/MAINTENANCE.md#m06) | Reclassify source inspection as runtime evidence |

<a id="s08"></a>
## S08 — Closure

| Condition | Decision | Evidence | Stop |
|---|---|---|---|
| Record is handed off | Keep all five axioms and unresolved external claims visible | [R08](../../../docs/ownership/crates/corelink-terraform-drift-consumer/REFERENCE.md#r08) | Turn a static handoff into a claim of runtime completion |

[Reference](../../../docs/ownership/crates/corelink-terraform-drift-consumer/REFERENCE.md#r01) · [Impact map](../../../docs/ownership/crates/corelink-terraform-drift-consumer/BLAST_RADIUS.md#b01) · [Start](#s01)
