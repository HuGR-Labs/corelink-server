---
name: own-corelink-handler-admin
description: >-
  Use for source-grounded ownership or contract changes to the admin handler
  traits, in-memory implementation, approval ledger, audit surface, errors, or
  SLI observer of corelink-handler-admin.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-handler-admin
  manifest: crates/corelink-handler-admin/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-handler-admin-structural-normalization-20260921
---

# Ownership — corelink-handler-admin

This guide is constrained to the crate's source-defined contracts and in-process
fakes. It does not establish an HTTP route, Cloudflare Worker, control-plane
wiring, telemetry/audit provider, durable backend, deployed behavior, identity
verification, or real-world approval authority.

[Entry](#s01) · [Boundary](#s02) · [Reads](#s03) · [Mutations](#s04) ·
[Audit and SLI](#s05) · [Stops](#s06) · [Record](#s07).

<a id="s01"></a>
## S01 — Establish the static baseline

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Ownership or contract work is proposed | Record revision and inspect `Cargo.toml`, `src/lib.rs`, `handler.rs`, `ledger.rs`, `audit.rs`, `observer.rs`, and `error.rs` | Package declaration and [R01](../../../docs/ownership/crates/corelink-handler-admin/REFERENCE.md#r01) | Baseline or target surface differs |
| A claim requires execution or a provider | Keep it outside this source-static result | [R08](../../../docs/ownership/crates/corelink-handler-admin/REFERENCE.md#r08) | Do not turn a trait or fake into runtime evidence |

<a id="s02"></a>
## S02 — Preserve the package boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing request, response, operation, trait, error, event, ledger, or observer type | Map public exports and the related static relation before editing | `src/lib.rs`; [B01](../../../docs/ownership/crates/corelink-handler-admin/BLAST_RADIUS.md#b01) | A caller, router, or provider compatibility result is not source-visible here |
| Adding HTTP, Worker, database, credential, or control-plane behavior | Hand off to the composition/runtime owner | Manifest description and [R02](../../../docs/ownership/crates/corelink-handler-admin/REFERENCE.md#r02) | This crate does not supply that implementation |

<a id="s03"></a>
## S03 — Change read behavior deliberately

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing `AdminReadRequest`, `AdminReadResponse`, or `AdminReadHandler` | Review RBAC, source-level audit ordering, body-map lookup, error, and SLI consequences | `handler.rs`; [R04](../../../docs/ownership/crates/corelink-handler-admin/REFERENCE.md#r04) | Do not infer upstream RBAC validation or an HTTP response |
| Relying on the trait's ordering requirements | Compare them with the selected implementation, not only the doc comment | [R05](../../../docs/ownership/crates/corelink-handler-admin/REFERENCE.md#r05) | The in-memory denied path emits `ReadDenied` without `ReadAttempted` |

<a id="s04"></a>
## S04 — Change a mutation as a shared control contract

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing `DualApprovalToken`, `MutateOp`, ledger result, or mutation error | Trace approval id, resource, initiator, recorded approver, consume point, audit, and applied-state order | [R06](../../../docs/ownership/crates/corelink-handler-admin/REFERENCE.md#r06); [B02](../../../docs/ownership/crates/corelink-handler-admin/BLAST_RADIUS.md#b02) | Identity verification, upstream approval creation, and production authority remain unknown |
| Changing `ApprovalLedger::verify_and_consume` | Preserve reject-without-consume and atomic lookup/verify/consume requirements | `ledger.rs`; [B03](../../../docs/ownership/crates/corelink-handler-admin/BLAST_RADIUS.md#b03) | Atomicity beyond the selected implementation needs backend-specific evidence |
| Treating `token.approver` as authorization input | Do not: the in-memory handler authorizes from the ledger result | `handler.rs`; [R06](../../../docs/ownership/crates/corelink-handler-admin/REFERENCE.md#r06) | The ledger-recorded string itself is not identity-verification evidence |

<a id="s05"></a>
## S05 — Keep audit and SLI claims source-local

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Changing audit kind or ordering | Read the selected `handler.rs` control flow and `AuditSink::emit` call sequence | [R05](../../../docs/ownership/crates/corelink-handler-admin/REFERENCE.md#r05); [B04](../../../docs/ownership/crates/corelink-handler-admin/BLAST_RADIUS.md#b04) | Do not claim durable audit persistence, provider delivery, or runtime ordering |
| Changing `SliObservation` or observer calls | Check `Sli::AvailControlPlane`, `is_error`, and all directly visible returns | `observer.rs`; [R07](../../../docs/ownership/crates/corelink-handler-admin/REFERENCE.md#r07) | A trait requirement or in-memory capture does not prove telemetry emission |

<a id="s06"></a>
## S06 — Stops and escalation

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Need authentication, authorization authority, a second human, or approval endpoint evidence | Escalate to the actual identity/route/provider scope | [R08](../../../docs/ownership/crates/corelink-handler-admin/REFERENCE.md#r08) | Never describe string comparison or comments as proof |
| Need HTTP, CF Worker, control plane, telemetry, audit provider, persistence, or deployment evidence | Identify and inspect the separately scoped adapter or entrypoint | [B05](../../../docs/ownership/crates/corelink-handler-admin/BLAST_RADIUS.md#b05) | Do not infer it from this crate |
| Baseline, public surface, or consumer set is incomplete | Stop the conclusion and request the missing scope | [B06](../../../docs/ownership/crates/corelink-handler-admin/BLAST_RADIUS.md#b06) | Do not guess compatibility |

<a id="s07"></a>
## S07 — Record the bounded result

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Work is ready to report | State baseline, files read, symbols, direct source relations, documentary checks, and unknowns | [M06](../../../docs/ownership/crates/corelink-handler-admin/MAINTENANCE.md#m06) | Do not label an author check as cold review or runtime validation |

[Back to entry](#s01)
